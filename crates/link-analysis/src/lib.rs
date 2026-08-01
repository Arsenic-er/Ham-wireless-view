// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

//! Pure domain analysis for one terrain-aware radio link.
//!
//! Raw terrain elevations are passed to NTIA ITM. Earth-curvature adjustment
//! exists only in the returned display geometry and clearance classification,
//! so curvature is never counted twice.

use std::error::Error;
use std::fmt;

use geographiclib_rs::{DirectGeodesic, Geodesic, InverseGeodesic};
use hamheatmap_propagation::{
    ModelDefaults, Polarization, PredictionInputs, PredictionOutput, PropagationError,
    TerrainProfile, predict_p2p, received_power_dbm,
};
use hamheatmap_terrain::{DemTileSet, WaterTileSet};

pub const MIN_LINK_DISTANCE_M: f64 = 1_000.0;
pub const MAX_LINK_DISTANCE_M: f64 = 200_000.0;
pub const MAX_PROFILE_SAMPLE_SPACING_M: f64 = 90.0;
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;
pub const EFFECTIVE_EARTH_K_FACTOR: f64 = 4.0 / 3.0;
pub const EFFECTIVE_EARTH_RADIUS_M: f64 = EARTH_RADIUS_M * EFFECTIVE_EARTH_K_FACTOR;
pub const SPEED_OF_LIGHT_M_PER_S: f64 = 299_792_458.0;
pub const REQUIRED_FRESNEL_CLEARANCE_FRACTION: f64 = 0.6;
pub const LINK_GEOMETRY_MODEL_VERSION: &str = "fixed-k-4-3-fresnel-v1";
pub const PROPAGATION_MODEL_DEFAULTS_VERSION: &str = ModelDefaults::LAND_WATER_V1.version;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkAnalysisConfig {
    pub transmitter: GeoPoint,
    pub receiver: GeoPoint,
    pub frequency_mhz: f64,
    pub polarization: Polarization,
    pub tx_height_m: f64,
    pub rx_height_m: f64,
    pub tx_power_dbm: f64,
    pub tx_gain_dbi: f64,
    pub rx_gain_dbi: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryClassification {
    DirectLineOfSight,
    FresnelAffected,
    SeverelyObstructed,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkGeometryDiagnostics {
    pub classification: GeometryClassification,
    pub geometric_line_of_sight: bool,
    pub sixty_percent_fresnel_clear: bool,
    pub minimum_line_of_sight_clearance_m: f64,
    pub minimum_normalized_fresnel_clearance: f64,
    pub critical_sample_index: usize,
    pub critical_distance_m: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinkProfileSample {
    pub point: GeoPoint,
    pub distance_m: f64,
    pub terrain_elevation_m: f64,
    pub earth_bulge_m: f64,
    pub adjusted_terrain_elevation_m: f64,
    pub line_of_sight_elevation_m: f64,
    pub fresnel_radius_m: f64,
    pub line_of_sight_clearance_m: f64,
    pub normalized_fresnel_clearance: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LinkAnalysisResult {
    pub config: LinkAnalysisConfig,
    pub geometry_model_version: &'static str,
    pub propagation_model_defaults_version: &'static str,
    pub distance_m: f64,
    pub initial_bearing_deg: f64,
    pub final_bearing_deg: f64,
    pub sample_spacing_m: f64,
    pub wavelength_m: f64,
    pub effective_earth_radius_m: f64,
    pub tx_ground_elevation_m: f64,
    pub rx_ground_elevation_m: f64,
    pub path_water_fraction: f64,
    pub prediction: PredictionOutput,
    pub received_power_dbm: f64,
    pub geometry: LinkGeometryDiagnostics,
    pub profile: Vec<LinkProfileSample>,
}

pub trait TerrainSource: Sync {
    fn elevation_m(&self, lon: f64, lat: f64) -> Result<f32, String>;
}

impl TerrainSource for DemTileSet {
    fn elevation_m(&self, lon: f64, lat: f64) -> Result<f32, String> {
        self.sample_bilinear(lon, lat)
            .map_err(|error| error.to_string())
    }
}

pub trait WaterSource: Sync {
    fn is_water(&self, lon: f64, lat: f64) -> Result<bool, String>;
}

impl WaterSource for WaterTileSet {
    fn is_water(&self, lon: f64, lat: f64) -> Result<bool, String> {
        self.sample_is_water(lon, lat)
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FlatTerrain {
    pub elevation_m: f32,
}

impl TerrainSource for FlatTerrain {
    fn elevation_m(&self, _lon: f64, _lat: f64) -> Result<f32, String> {
        Ok(self.elevation_m)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct UniformWater {
    pub is_water: bool,
}

impl WaterSource for UniformWater {
    fn is_water(&self, _lon: f64, _lat: f64) -> Result<bool, String> {
        Ok(self.is_water)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkAnalysisError {
    InvalidInput(String),
    Terrain {
        sample_index: usize,
        point: GeoPoint,
        message: String,
    },
    WaterMask {
        sample_index: usize,
        point: GeoPoint,
        message: String,
    },
    Propagation(PropagationError),
}

impl fmt::Display for LinkAnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid link input: {message}"),
            Self::Terrain {
                sample_index,
                point,
                message,
            } => write!(
                formatter,
                "terrain sampling failed at sample {sample_index} ({:.8}, {:.8}): {message}",
                point.lon, point.lat
            ),
            Self::WaterMask {
                sample_index,
                point,
                message,
            } => write!(
                formatter,
                "water-mask sampling failed at sample {sample_index} ({:.8}, {:.8}): {message}",
                point.lon, point.lat
            ),
            Self::Propagation(error) => write!(formatter, "link propagation failed: {error}"),
        }
    }
}

impl Error for LinkAnalysisError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Propagation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PropagationError> for LinkAnalysisError {
    fn from(error: PropagationError) -> Self {
        Self::Propagation(error)
    }
}

pub fn analyze_link(
    terrain: &impl TerrainSource,
    water: &impl WaterSource,
    config: LinkAnalysisConfig,
) -> Result<LinkAnalysisResult, LinkAnalysisError> {
    validate_config(config)?;
    let geodesic = Geodesic::wgs84();
    let (distance_m, initial_bearing_deg, final_bearing_deg, _arc_degrees): (f64, f64, f64, f64) =
        geodesic.inverse(
            config.transmitter.lat,
            config.transmitter.lon,
            config.receiver.lat,
            config.receiver.lon,
        );
    if !(MIN_LINK_DISTANCE_M..=MAX_LINK_DISTANCE_M).contains(&distance_m) {
        return Err(LinkAnalysisError::InvalidInput(format!(
            "link distance must be in {MIN_LINK_DISTANCE_M}..={MAX_LINK_DISTANCE_M} metres"
        )));
    }

    let interval_count = (distance_m / MAX_PROFILE_SAMPLE_SPACING_M).ceil() as usize;
    let sample_spacing_m = distance_m / interval_count as f64;
    let mut points = Vec::with_capacity(interval_count + 1);
    let mut raw_elevations_m = Vec::with_capacity(interval_count + 1);
    let mut water_sample_count = 0_usize;

    for sample_index in 0..=interval_count {
        let distance_along_m = sample_spacing_m * sample_index as f64;
        let point = if sample_index == 0 {
            config.transmitter
        } else if sample_index == interval_count {
            config.receiver
        } else {
            let (lat, lon, _final_bearing): (f64, f64, f64) = geodesic.direct(
                config.transmitter.lat,
                config.transmitter.lon,
                initial_bearing_deg,
                distance_along_m,
            );
            GeoPoint { lat, lon }
        };
        let elevation_m = terrain
            .elevation_m(point.lon, point.lat)
            .map_err(|message| LinkAnalysisError::Terrain {
                sample_index,
                point,
                message,
            })?;
        if !elevation_m.is_finite() {
            return Err(LinkAnalysisError::Terrain {
                sample_index,
                point,
                message: "terrain elevation is not finite".into(),
            });
        }
        let is_water = water.is_water(point.lon, point.lat).map_err(|message| {
            LinkAnalysisError::WaterMask {
                sample_index,
                point,
                message,
            }
        })?;
        water_sample_count += usize::from(is_water);
        points.push(point);
        raw_elevations_m.push(f64::from(elevation_m));
    }

    let path_water_fraction = water_sample_count as f64 / raw_elevations_m.len() as f64;
    let profile = TerrainProfile::new(sample_spacing_m, raw_elevations_m.clone())?;
    let mut prediction_inputs =
        PredictionInputs::land_water_v1(config.frequency_mhz, config.polarization);
    prediction_inputs.tx_height_m = config.tx_height_m;
    prediction_inputs.rx_height_m = config.rx_height_m;
    prediction_inputs.ground =
        ModelDefaults::LAND_WATER_V1.ground_for_water_fraction(path_water_fraction)?;
    let prediction = predict_p2p(&profile, prediction_inputs)?;
    let received_power_dbm = received_power_dbm(
        config.tx_power_dbm,
        config.tx_gain_dbi,
        config.rx_gain_dbi,
        prediction.basic_transmission_loss_db,
    )?;
    let wavelength_m = wavelength_m(config.frequency_mhz)?;
    let tx_ground_elevation_m = raw_elevations_m[0];
    let rx_ground_elevation_m = raw_elevations_m[interval_count];
    let tx_antenna_elevation_m = tx_ground_elevation_m + config.tx_height_m;
    let rx_antenna_elevation_m = rx_ground_elevation_m + config.rx_height_m;

    let mut display_profile = Vec::with_capacity(points.len());
    for (sample_index, (&point, &terrain_elevation_m)) in
        points.iter().zip(&raw_elevations_m).enumerate()
    {
        let distance_along_m = sample_spacing_m * sample_index as f64;
        let earth_bulge_m =
            effective_earth_bulge_m(distance_along_m, distance_m, EFFECTIVE_EARTH_RADIUS_M);
        let adjusted_terrain_elevation_m = terrain_elevation_m + earth_bulge_m;
        let fraction = distance_along_m / distance_m;
        let line_of_sight_elevation_m =
            tx_antenna_elevation_m + (rx_antenna_elevation_m - tx_antenna_elevation_m) * fraction;
        let fresnel_radius_m = first_fresnel_radius_m(
            wavelength_m,
            distance_along_m,
            distance_m - distance_along_m,
        );
        let line_of_sight_clearance_m = line_of_sight_elevation_m - adjusted_terrain_elevation_m;
        let normalized_fresnel_clearance = if fresnel_radius_m > 0.0 {
            Some(line_of_sight_clearance_m / fresnel_radius_m)
        } else {
            None
        };
        display_profile.push(LinkProfileSample {
            point,
            distance_m: distance_along_m,
            terrain_elevation_m,
            earth_bulge_m,
            adjusted_terrain_elevation_m,
            line_of_sight_elevation_m,
            fresnel_radius_m,
            line_of_sight_clearance_m,
            normalized_fresnel_clearance,
        });
    }
    let geometry = classify_geometry(&display_profile)?;

    Ok(LinkAnalysisResult {
        config,
        geometry_model_version: LINK_GEOMETRY_MODEL_VERSION,
        propagation_model_defaults_version: PROPAGATION_MODEL_DEFAULTS_VERSION,
        distance_m,
        initial_bearing_deg,
        final_bearing_deg,
        sample_spacing_m,
        wavelength_m,
        effective_earth_radius_m: EFFECTIVE_EARTH_RADIUS_M,
        tx_ground_elevation_m,
        rx_ground_elevation_m,
        path_water_fraction,
        prediction,
        received_power_dbm,
        geometry,
        profile: display_profile,
    })
}

pub fn wavelength_m(frequency_mhz: f64) -> Result<f64, LinkAnalysisError> {
    if !frequency_mhz.is_finite() || !(20.0..=20_000.0).contains(&frequency_mhz) {
        return Err(LinkAnalysisError::InvalidInput(
            "frequency must be finite and in 20..=20000 MHz".into(),
        ));
    }
    Ok(SPEED_OF_LIGHT_M_PER_S / (frequency_mhz * 1_000_000.0))
}

pub fn first_fresnel_radius_m(wavelength_m: f64, d1_m: f64, d2_m: f64) -> f64 {
    if !wavelength_m.is_finite()
        || wavelength_m <= 0.0
        || !d1_m.is_finite()
        || !d2_m.is_finite()
        || d1_m < 0.0
        || d2_m < 0.0
        || d1_m + d2_m <= 0.0
    {
        return f64::NAN;
    }
    (wavelength_m * d1_m * d2_m / (d1_m + d2_m)).sqrt()
}

pub fn effective_earth_bulge_m(
    distance_along_m: f64,
    total_distance_m: f64,
    effective_earth_radius_m: f64,
) -> f64 {
    if !distance_along_m.is_finite()
        || !total_distance_m.is_finite()
        || !effective_earth_radius_m.is_finite()
        || total_distance_m <= 0.0
        || effective_earth_radius_m <= 0.0
        || !(0.0..=total_distance_m).contains(&distance_along_m)
    {
        return f64::NAN;
    }
    2.0 * effective_earth_radius_m
        * (distance_along_m / (2.0 * effective_earth_radius_m)).sin()
        * ((total_distance_m - distance_along_m) / (2.0 * effective_earth_radius_m)).sin()
}

pub fn classify_geometry(
    profile: &[LinkProfileSample],
) -> Result<LinkGeometryDiagnostics, LinkAnalysisError> {
    if profile.len() < 3 {
        return Err(LinkAnalysisError::InvalidInput(
            "geometry classification needs at least three profile samples".into(),
        ));
    }
    let interior = &profile[1..profile.len() - 1];
    let minimum_line_of_sight_clearance_m = interior
        .iter()
        .map(|sample| sample.line_of_sight_clearance_m)
        .reduce(f64::min)
        .ok_or_else(|| LinkAnalysisError::InvalidInput("profile is empty".into()))?;
    if !minimum_line_of_sight_clearance_m.is_finite() {
        return Err(LinkAnalysisError::InvalidInput(
            "profile contains non-finite clearance".into(),
        ));
    }
    let (critical_interior_index, minimum_normalized_fresnel_clearance) = interior
        .iter()
        .enumerate()
        .filter_map(|(index, sample)| {
            sample
                .normalized_fresnel_clearance
                .filter(|value| value.is_finite())
                .map(|value| (index, value))
        })
        .min_by(|left, right| left.1.total_cmp(&right.1))
        .ok_or_else(|| {
            LinkAnalysisError::InvalidInput(
                "profile has no finite interior Fresnel clearance".into(),
            )
        })?;
    let critical_sample_index = critical_interior_index + 1;
    let classification =
        if minimum_normalized_fresnel_clearance >= REQUIRED_FRESNEL_CLEARANCE_FRACTION {
            GeometryClassification::DirectLineOfSight
        } else if minimum_normalized_fresnel_clearance >= -1.0 {
            GeometryClassification::FresnelAffected
        } else {
            GeometryClassification::SeverelyObstructed
        };
    Ok(LinkGeometryDiagnostics {
        classification,
        geometric_line_of_sight: minimum_line_of_sight_clearance_m >= 0.0,
        sixty_percent_fresnel_clear: minimum_normalized_fresnel_clearance
            >= REQUIRED_FRESNEL_CLEARANCE_FRACTION,
        minimum_line_of_sight_clearance_m,
        minimum_normalized_fresnel_clearance,
        critical_sample_index,
        critical_distance_m: profile[critical_sample_index].distance_m,
    })
}

fn validate_config(config: LinkAnalysisConfig) -> Result<(), LinkAnalysisError> {
    validate_point("transmitter", config.transmitter)?;
    validate_point("receiver", config.receiver)?;
    wavelength_m(config.frequency_mhz)?;
    for (name, value) in [
        ("transmitter height", config.tx_height_m),
        ("receiver height", config.rx_height_m),
    ] {
        if !value.is_finite() || !(0.5..=3000.0).contains(&value) {
            return Err(LinkAnalysisError::InvalidInput(format!(
                "{name} must be finite and in 0.5..=3000 metres"
            )));
        }
    }
    for (name, value) in [
        ("transmitter power", config.tx_power_dbm),
        ("transmitter gain", config.tx_gain_dbi),
        ("receiver gain", config.rx_gain_dbi),
    ] {
        if !value.is_finite() {
            return Err(LinkAnalysisError::InvalidInput(format!(
                "{name} must be finite"
            )));
        }
    }
    Ok(())
}

fn validate_point(name: &str, point: GeoPoint) -> Result<(), LinkAnalysisError> {
    if !point.lat.is_finite()
        || !point.lon.is_finite()
        || !(-90.0..=90.0).contains(&point.lat)
        || !(-180.0..=180.0).contains(&point.lon)
    {
        return Err(LinkAnalysisError::InvalidInput(format!(
            "{name} WGS84 coordinates are invalid"
        )));
    }
    Ok(())
}
