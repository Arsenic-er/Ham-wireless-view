//! Fixed-grid real-terrain coverage engine used by the minimum-viability proof.

use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use geographiclib_rs::{DirectGeodesic, Geodesic, InverseGeodesic};
use hamheatmap_propagation::{
    ModelDefaults, Polarization, PredictionInputs, PropagationMode, predict_p2p_pfl,
    received_power_dbm, watts_to_dbm,
};
use hamheatmap_terrain::{DemTileSet, WaterTileSet};

pub const GRID_RADIUS_KM: i32 = 200;
pub const GRID_SIZE: usize = (GRID_RADIUS_KM as usize) * 2 + 1;
pub const GRID_PIXEL_COUNT: usize = GRID_SIZE * GRID_SIZE;
pub const PROFILE_SAMPLE_SPACING_M: f64 = 90.0;
pub const ITM_WARNING_BIT_COUNT: usize = 15;
pub const MODEL_DEFAULTS_VERSION: &str = ModelDefaults::LAND_WATER_V1.version;
pub const MAP_OVERLAY_PROJECTION: &str = "EPSG:3857";

const WEB_MERCATOR_RADIUS_M: f64 = 6_378_137.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverageConfig {
    pub center: GeoPoint,
    pub frequency_mhz: f64,
    pub polarization: Polarization,
    pub tx_power_w: f64,
    pub tx_gain_dbi: f64,
    pub rx_gain_dbi: f64,
    pub tx_height_m: f64,
    pub tx_ground_elevation_override_m: Option<f64>,
    pub rx_height_m: f64,
    pub threads: usize,
    pub profile_sample_spacing_m: f64,
}

impl CoverageConfig {
    pub fn base_to_handheld(center: GeoPoint, frequency_mhz: f64, threads: usize) -> Self {
        Self {
            center,
            frequency_mhz,
            polarization: Polarization::Vertical,
            tx_power_w: 25.0,
            tx_gain_dbi: 6.0,
            rx_gain_dbi: -3.0,
            tx_height_m: 20.0,
            tx_ground_elevation_override_m: None,
            rx_height_m: 1.5,
            threads,
            profile_sample_spacing_m: PROFILE_SAMPLE_SPACING_M,
        }
    }

    fn validate(self) -> Result<(), CoverageError> {
        if !self.center.lat.is_finite()
            || !self.center.lon.is_finite()
            || !(-90.0..=90.0).contains(&self.center.lat)
            || !(-180.0..=180.0).contains(&self.center.lon)
        {
            return Err(CoverageError::InvalidInput(
                "center latitude/longitude is invalid".into(),
            ));
        }
        if self.threads == 0 {
            return Err(CoverageError::InvalidInput(
                "thread count must be positive".into(),
            ));
        }
        if !self.profile_sample_spacing_m.is_finite()
            || self.profile_sample_spacing_m <= 0.0
            || self.profile_sample_spacing_m > 1000.0
        {
            return Err(CoverageError::InvalidInput(
                "profile sample spacing must be in (0, 1000] m".into(),
            ));
        }
        if self
            .tx_ground_elevation_override_m
            .is_some_and(|value| !value.is_finite() || !(-500.0..=9000.0).contains(&value))
        {
            return Err(CoverageError::InvalidInput(
                "transmitter ground elevation override must be in [-500, 9000] m".into(),
            ));
        }
        watts_to_dbm(self.tx_power_w)
            .map_err(|error| CoverageError::InvalidInput(error.to_string()))?;
        Ok(())
    }
}

pub trait ElevationSource: Sync {
    fn elevation_m(&self, lon: f64, lat: f64) -> Result<f32, String>;
}

impl ElevationSource for DemTileSet {
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

impl ElevationSource for FlatTerrain {
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModeCounts {
    pub line_of_sight: usize,
    pub diffraction: usize,
    pub troposcatter: usize,
    pub unknown: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CoverageStatistics {
    pub valid_pixel_count: usize,
    pub masked_pixel_count: usize,
    pub below_threshold_pixel_count: usize,
    pub transparent_pixel_count: usize,
    pub warning_pixel_count: usize,
    pub warning_mask_or: u64,
    pub warning_bit_counts: [usize; ITM_WARNING_BIT_COUNT],
    pub mode_counts: ModeCounts,
    pub minimum_dbm: f32,
    pub maximum_dbm: f32,
    pub mean_dbm: f64,
    pub water_affected_pixel_count: usize,
    pub mean_path_water_fraction: f64,
    pub maximum_path_water_fraction: f64,
}

#[derive(Clone, Debug)]
pub struct CoverageGrid {
    values_dbm: Vec<f32>,
    pub config: CoverageConfig,
    pub tx_ground_elevation_m: f64,
    pub statistics: CoverageStatistics,
    pub receiver_generation_time: Duration,
    pub propagation_time: Duration,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MapOverlay {
    pub projection: &'static str,
    pub width: usize,
    pub height: usize,
    pub corners: [[f64; 2]; 4],
    pub png: Vec<u8>,
}

#[derive(Clone, Copy, Debug)]
struct MapOverlayLayout {
    center: GeoPoint,
    sample_min_x_m: f64,
    sample_max_y_m: f64,
    sample_step_x_m: f64,
    sample_step_y_m: f64,
    corners: [[f64; 2]; 4],
}

impl MapOverlayLayout {
    fn new(center: GeoPoint) -> Self {
        let mut min_x_m = f64::INFINITY;
        let mut max_x_m = f64::NEG_INFINITY;
        let mut min_y_m = f64::INFINITY;
        let mut max_y_m = f64::NEG_INFINITY;
        let geodesic = Geodesic::wgs84();
        for y_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
            for x_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
                let Some((point, _)) = local_grid_point(&geodesic, center, x_km, y_km) else {
                    continue;
                };
                let (x_m, y_m) = web_mercator_project(center, point);
                min_x_m = min_x_m.min(x_m);
                max_x_m = max_x_m.max(x_m);
                min_y_m = min_y_m.min(y_m);
                max_y_m = max_y_m.max(y_m);
            }
        }
        let (center_x_m, center_y_m) = web_mercator_project(center, center);
        let half_span_x_m = (center_x_m - min_x_m).max(max_x_m - center_x_m);
        let half_span_y_m = (center_y_m - min_y_m).max(max_y_m - center_y_m);
        let sample_step_x_m = 2.0 * half_span_x_m / (GRID_SIZE - 1) as f64;
        let sample_step_y_m = 2.0 * half_span_y_m / (GRID_SIZE - 1) as f64;
        let sample_min_x_m = center_x_m - half_span_x_m;
        let sample_max_y_m = center_y_m + half_span_y_m;
        let edge_min_x_m = sample_min_x_m - sample_step_x_m / 2.0;
        let edge_max_x_m = center_x_m + half_span_x_m + sample_step_x_m / 2.0;
        let edge_min_y_m = center_y_m - half_span_y_m - sample_step_y_m / 2.0;
        let edge_max_y_m = sample_max_y_m + sample_step_y_m / 2.0;
        let corners = [
            web_mercator_unproject(center, edge_min_x_m, edge_max_y_m),
            web_mercator_unproject(center, edge_max_x_m, edge_max_y_m),
            web_mercator_unproject(center, edge_max_x_m, edge_min_y_m),
            web_mercator_unproject(center, edge_min_x_m, edge_min_y_m),
        ]
        .map(|point| [point.lon, point.lat]);
        Self {
            center,
            sample_min_x_m,
            sample_max_y_m,
            sample_step_x_m,
            sample_step_y_m,
            corners,
        }
    }

    fn sample_point(self, row: usize, column: usize) -> GeoPoint {
        web_mercator_unproject(
            self.center,
            self.sample_min_x_m + column as f64 * self.sample_step_x_m,
            self.sample_max_y_m - row as f64 * self.sample_step_y_m,
        )
    }

    #[cfg(test)]
    fn nearest_sample(self, point: GeoPoint) -> GeoPoint {
        let (x_m, y_m) = web_mercator_project(self.center, point);
        let column = ((x_m - self.sample_min_x_m) / self.sample_step_x_m)
            .round()
            .clamp(0.0, (GRID_SIZE - 1) as f64) as usize;
        let row = ((self.sample_max_y_m - y_m) / self.sample_step_y_m)
            .round()
            .clamp(0.0, (GRID_SIZE - 1) as f64) as usize;
        self.sample_point(row, column)
    }
}

impl CoverageGrid {
    pub fn values_dbm(&self) -> &[f32] {
        &self.values_dbm
    }

    pub fn write_png(&self, path: impl AsRef<Path>) -> Result<(), CoverageError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(path)?;
        self.write_png_to(BufWriter::new(file))
    }

    pub fn encode_png(&self) -> Result<Vec<u8>, CoverageError> {
        let mut bytes = Vec::new();
        self.write_png_to(&mut bytes)?;
        Ok(bytes)
    }

    pub fn encode_map_overlay(&self) -> Result<MapOverlay, CoverageError> {
        encode_map_overlay_values(&self.values_dbm, self.config.center)
    }

    fn write_png_to(&self, writer: impl Write) -> Result<(), CoverageError> {
        let mut rgba = Vec::with_capacity(GRID_PIXEL_COUNT * 4);
        for &value in &self.values_dbm {
            rgba.extend_from_slice(&color_for_dbm(value));
        }
        write_rgba_png(writer, GRID_SIZE, GRID_SIZE, &rgba)
    }

    pub fn comparison(&self, other: &Self) -> Result<CoverageComparison, CoverageError> {
        if self.values_dbm.len() != other.values_dbm.len() {
            return Err(CoverageError::InvalidInput(
                "coverage grids have different dimensions".into(),
            ));
        }
        let mut compared_pixel_count = 0;
        let mut changed_pixel_count = 0;
        let mut absolute_difference_sum_db = 0.0;
        let mut maximum_absolute_difference_db = 0.0_f32;
        for (&left, &right) in self.values_dbm.iter().zip(&other.values_dbm) {
            if !left.is_finite() || !right.is_finite() {
                continue;
            }
            let difference = (left - right).abs();
            compared_pixel_count += 1;
            absolute_difference_sum_db += f64::from(difference);
            maximum_absolute_difference_db = maximum_absolute_difference_db.max(difference);
            if difference > 0.1 {
                changed_pixel_count += 1;
            }
        }
        if compared_pixel_count == 0 {
            return Err(CoverageError::InvalidInput(
                "coverage grids have no comparable pixels".into(),
            ));
        }
        Ok(CoverageComparison {
            compared_pixel_count,
            changed_pixel_count,
            mean_absolute_difference_db: absolute_difference_sum_db / compared_pixel_count as f64,
            maximum_absolute_difference_db,
        })
    }

    /// Returns signed per-pixel change (`self - baseline`) for engineering
    /// validation of height, frequency, or terrain effects.
    pub fn delta_from(&self, baseline: &Self) -> Result<CoverageDelta, CoverageError> {
        if self.values_dbm.len() != baseline.values_dbm.len() {
            return Err(CoverageError::InvalidInput(
                "coverage grids have different dimensions".into(),
            ));
        }
        let mut compared_pixel_count = 0;
        let mut improved_pixel_count = 0;
        let mut worsened_pixel_count = 0;
        let mut signed_sum_db = 0.0;
        let mut maximum_gain_db = f32::NEG_INFINITY;
        let mut maximum_loss_db = f32::INFINITY;
        for (&value, &baseline_value) in self.values_dbm.iter().zip(&baseline.values_dbm) {
            if !value.is_finite() || !baseline_value.is_finite() {
                continue;
            }
            let delta = value - baseline_value;
            compared_pixel_count += 1;
            signed_sum_db += f64::from(delta);
            maximum_gain_db = maximum_gain_db.max(delta);
            maximum_loss_db = maximum_loss_db.min(delta);
            if delta > 0.1 {
                improved_pixel_count += 1;
            } else if delta < -0.1 {
                worsened_pixel_count += 1;
            }
        }
        if compared_pixel_count == 0 {
            return Err(CoverageError::InvalidInput(
                "coverage grids have no comparable pixels".into(),
            ));
        }
        Ok(CoverageDelta {
            compared_pixel_count,
            improved_pixel_count,
            worsened_pixel_count,
            unchanged_pixel_count: compared_pixel_count
                - improved_pixel_count
                - worsened_pixel_count,
            mean_signed_difference_db: signed_sum_db / compared_pixel_count as f64,
            maximum_gain_db,
            maximum_loss_db,
        })
    }
}

fn encode_map_overlay_values(
    values_dbm: &[f32],
    center: GeoPoint,
) -> Result<MapOverlay, CoverageError> {
    debug_assert_eq!(values_dbm.len(), GRID_PIXEL_COUNT);
    let layout = MapOverlayLayout::new(center);
    let rgba = render_map_overlay(values_dbm, layout);
    let mut png = Vec::new();
    write_rgba_png(&mut png, GRID_SIZE, GRID_SIZE, &rgba)?;
    Ok(MapOverlay {
        projection: MAP_OVERLAY_PROJECTION,
        width: GRID_SIZE,
        height: GRID_SIZE,
        corners: layout.corners,
        png,
    })
}

fn render_map_overlay(values_dbm: &[f32], layout: MapOverlayLayout) -> Vec<u8> {
    let geodesic = Geodesic::wgs84();
    let mut rgba = Vec::with_capacity(GRID_PIXEL_COUNT * 4);
    for row in 0..GRID_SIZE {
        for column in 0..GRID_SIZE {
            rgba.extend_from_slice(&color_for_dbm(map_overlay_sample_dbm(
                values_dbm, layout, &geodesic, row, column,
            )));
        }
    }
    rgba
}

fn map_overlay_sample_dbm(
    values_dbm: &[f32],
    layout: MapOverlayLayout,
    geodesic: &Geodesic,
    row: usize,
    column: usize,
) -> f32 {
    let point = layout.sample_point(row, column);
    let (distance_m, azimuth_deg, _, _): (f64, f64, f64, f64) =
        geodesic.inverse(layout.center.lat, layout.center.lon, point.lat, point.lon);
    if distance_m > f64::from(GRID_RADIUS_KM) * 1000.0 + 1e-6 {
        return f32::NAN;
    }
    let azimuth_rad = azimuth_deg.to_radians();
    let source_column = (f64::from(GRID_RADIUS_KM) + distance_m * azimuth_rad.sin() / 1000.0)
        .clamp(0.0, (GRID_SIZE - 1) as f64);
    let source_row = (f64::from(GRID_RADIUS_KM) - distance_m * azimuth_rad.cos() / 1000.0)
        .clamp(0.0, (GRID_SIZE - 1) as f64);
    sample_dbm_nan_aware(values_dbm, source_row, source_column)
}

fn sample_dbm_nan_aware(values_dbm: &[f32], row: f64, column: f64) -> f32 {
    let row0 = row.floor() as usize;
    let column0 = column.floor() as usize;
    let row1 = (row0 + 1).min(GRID_SIZE - 1);
    let column1 = (column0 + 1).min(GRID_SIZE - 1);
    let row_fraction = row - row0 as f64;
    let column_fraction = column - column0 as f64;
    let samples = [
        (
            row0,
            column0,
            (1.0 - row_fraction) * (1.0 - column_fraction),
        ),
        (row0, column1, (1.0 - row_fraction) * column_fraction),
        (row1, column0, row_fraction * (1.0 - column_fraction)),
        (row1, column1, row_fraction * column_fraction),
    ];
    let mut weighted_sum = 0.0;
    let mut finite_weight = 0.0;
    for (sample_row, sample_column, weight) in samples {
        let value = values_dbm[sample_row * GRID_SIZE + sample_column];
        if value.is_finite() && weight > 0.0 {
            weighted_sum += f64::from(value) * weight;
            finite_weight += weight;
        }
    }
    if finite_weight == 0.0 {
        f32::NAN
    } else {
        (weighted_sum / finite_weight) as f32
    }
}

fn write_rgba_png(
    writer: impl Write,
    width: usize,
    height: usize,
    rgba: &[u8],
) -> Result<(), CoverageError> {
    debug_assert_eq!(rgba.len(), width * height * 4);
    let mut encoder = png::Encoder::new(writer, width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(rgba)?;
    Ok(())
}

fn normalize_longitude(longitude: f64) -> f64 {
    (longitude + 540.0).rem_euclid(360.0) - 180.0
}

fn web_mercator_project(center: GeoPoint, point: GeoPoint) -> (f64, f64) {
    let longitude_delta = normalize_longitude(point.lon - center.lon);
    let x_m = (center.lon + longitude_delta).to_radians() * WEB_MERCATOR_RADIUS_M;
    let y_m = point.lat.to_radians().tan().asinh() * WEB_MERCATOR_RADIUS_M;
    (x_m, y_m)
}

fn web_mercator_unproject(_center: GeoPoint, x_m: f64, y_m: f64) -> GeoPoint {
    GeoPoint {
        lat: (y_m / WEB_MERCATOR_RADIUS_M).sinh().atan().to_degrees(),
        lon: normalize_longitude(x_m.to_degrees() / WEB_MERCATOR_RADIUS_M),
    }
}

fn local_grid_point(
    geodesic: &Geodesic,
    center: GeoPoint,
    x_km: i32,
    y_km: i32,
) -> Option<(GeoPoint, f64)> {
    let distance_km = f64::from(x_km * x_km + y_km * y_km).sqrt();
    if distance_km > f64::from(GRID_RADIUS_KM) {
        return None;
    }
    let distance_m = distance_km * 1000.0;
    if distance_m == 0.0 {
        return Some((center, 0.0));
    }
    let azimuth_deg = f64::from(x_km).atan2(f64::from(y_km)).to_degrees();
    let (lat, lon, _): (f64, f64, f64) =
        geodesic.direct(center.lat, center.lon, azimuth_deg, distance_m);
    Some((GeoPoint { lat, lon }, distance_m))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverageComparison {
    pub compared_pixel_count: usize,
    pub changed_pixel_count: usize,
    pub mean_absolute_difference_db: f64,
    pub maximum_absolute_difference_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverageDelta {
    pub compared_pixel_count: usize,
    pub improved_pixel_count: usize,
    pub worsened_pixel_count: usize,
    pub unchanged_pixel_count: usize,
    pub mean_signed_difference_db: f64,
    pub maximum_gain_db: f32,
    pub maximum_loss_db: f32,
}

#[derive(Clone, Copy, Debug)]
struct Receiver {
    raster_index: usize,
    point: GeoPoint,
    distance_m: f64,
}

#[derive(Clone, Copy, Debug)]
struct PixelResult {
    raster_index: usize,
    received_power_dbm: f32,
    warnings: u64,
    mode: PropagationMode,
    water_fraction: f64,
}

#[derive(Debug)]
struct ChunkResult {
    pixels: Vec<PixelResult>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoverageProgress {
    pub completed_pixel_count: usize,
    pub total_pixel_count: usize,
}

#[derive(Clone, Copy)]
struct CoverageControl<'a> {
    cancelled: &'a AtomicBool,
    completed: &'a AtomicUsize,
    progress: &'a (dyn Fn(CoverageProgress) + Sync),
    total: usize,
    notification_interval: usize,
}

#[derive(Clone, Copy)]
struct ChunkParameters {
    prediction_inputs: PredictionInputs,
    tx_power_dbm: f64,
    tx_ground_elevation_m: f64,
}

pub fn compute_coverage(
    elevation_source: &impl ElevationSource,
    water_source: &impl WaterSource,
    config: CoverageConfig,
) -> Result<CoverageGrid, CoverageError> {
    let cancelled = AtomicBool::new(false);
    compute_coverage_with_control(elevation_source, water_source, config, &cancelled, |_| {})
}

pub fn compute_coverage_with_control(
    elevation_source: &impl ElevationSource,
    water_source: &impl WaterSource,
    config: CoverageConfig,
    cancelled: &AtomicBool,
    progress: impl Fn(CoverageProgress) + Sync,
) -> Result<CoverageGrid, CoverageError> {
    config.validate()?;
    if cancelled.load(Ordering::Acquire) {
        return Err(CoverageError::Cancelled);
    }
    let tx_ground_elevation_m = resolve_tx_ground_elevation(elevation_source, config)?;
    let receiver_started = Instant::now();
    let receivers = generate_receivers(config.center);
    let receiver_generation_time = receiver_started.elapsed();
    let tx_power_dbm = watts_to_dbm(config.tx_power_w)
        .map_err(|error| CoverageError::InvalidInput(error.to_string()))?;
    let mut prediction_inputs =
        PredictionInputs::land_water_v1(config.frequency_mhz, config.polarization);
    prediction_inputs.tx_height_m = config.tx_height_m;
    prediction_inputs.rx_height_m = config.rx_height_m;
    let chunk_parameters = ChunkParameters {
        prediction_inputs,
        tx_power_dbm,
        tx_ground_elevation_m,
    };

    let propagation_started = Instant::now();
    let worker_count = config.threads.min(receivers.len());
    let chunk_size = receivers.len().div_ceil(worker_count);
    let completed = AtomicUsize::new(0);
    let control = CoverageControl {
        cancelled,
        completed: &completed,
        progress: &progress,
        total: receivers.len(),
        notification_interval: receivers.len().div_ceil(100).max(1),
    };
    progress(CoverageProgress {
        completed_pixel_count: 0,
        total_pixel_count: receivers.len(),
    });
    let chunks = std::thread::scope(|scope| {
        let handles: Vec<_> = receivers
            .chunks(chunk_size)
            .map(|chunk| {
                scope.spawn(move || {
                    compute_chunk(
                        elevation_source,
                        water_source,
                        config,
                        chunk_parameters,
                        chunk,
                        Some(&control),
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| CoverageError::Worker("coverage worker panicked".into()))?
            })
            .collect::<Result<Vec<_>, CoverageError>>()
    })?;
    let propagation_time = propagation_started.elapsed();

    let mut values_dbm = vec![f32::NAN; GRID_PIXEL_COUNT];
    let mut warning_pixel_count = 0;
    let mut warning_mask_or = 0_u64;
    let mut warning_bit_counts = [0_usize; ITM_WARNING_BIT_COUNT];
    let mut mode_counts = ModeCounts::default();
    let mut minimum_dbm = f32::INFINITY;
    let mut maximum_dbm = f32::NEG_INFINITY;
    let mut sum_dbm = 0.0_f64;
    let mut water_affected_pixel_count = 0_usize;
    let mut sum_path_water_fraction = 0.0_f64;
    let mut maximum_path_water_fraction = 0.0_f64;
    for chunk in chunks {
        for pixel in chunk.pixels {
            values_dbm[pixel.raster_index] = pixel.received_power_dbm;
            minimum_dbm = minimum_dbm.min(pixel.received_power_dbm);
            maximum_dbm = maximum_dbm.max(pixel.received_power_dbm);
            sum_dbm += f64::from(pixel.received_power_dbm);
            if pixel.water_fraction > 0.0 {
                water_affected_pixel_count += 1;
            }
            sum_path_water_fraction += pixel.water_fraction;
            maximum_path_water_fraction = maximum_path_water_fraction.max(pixel.water_fraction);
            if pixel.warnings != 0 {
                warning_pixel_count += 1;
                warning_mask_or |= pixel.warnings;
                for (bit, count) in warning_bit_counts.iter_mut().enumerate() {
                    if pixel.warnings & (1_u64 << bit) != 0 {
                        *count += 1;
                    }
                }
            }
            match pixel.mode {
                PropagationMode::LineOfSight => mode_counts.line_of_sight += 1,
                PropagationMode::Diffraction => mode_counts.diffraction += 1,
                PropagationMode::Troposcatter => mode_counts.troposcatter += 1,
                PropagationMode::Unknown(_) => mode_counts.unknown += 1,
            }
        }
    }
    let valid_pixel_count = receivers.len();
    let masked_pixel_count = GRID_PIXEL_COUNT - valid_pixel_count;
    let below_threshold_pixel_count = values_dbm
        .iter()
        .filter(|value| value.is_finite() && **value < -140.0)
        .count();
    let transparent_pixel_count = masked_pixel_count + below_threshold_pixel_count;
    Ok(CoverageGrid {
        values_dbm,
        config,
        tx_ground_elevation_m,
        statistics: CoverageStatistics {
            valid_pixel_count,
            masked_pixel_count,
            below_threshold_pixel_count,
            transparent_pixel_count,
            warning_pixel_count,
            warning_mask_or,
            warning_bit_counts,
            mode_counts,
            minimum_dbm,
            maximum_dbm,
            mean_dbm: sum_dbm / valid_pixel_count as f64,
            water_affected_pixel_count,
            mean_path_water_fraction: sum_path_water_fraction / valid_pixel_count as f64,
            maximum_path_water_fraction,
        },
        receiver_generation_time,
        propagation_time,
    })
}

fn compute_chunk(
    elevation_source: &impl ElevationSource,
    water_source: &impl WaterSource,
    config: CoverageConfig,
    parameters: ChunkParameters,
    receivers: &[Receiver],
    control: Option<&CoverageControl<'_>>,
) -> Result<ChunkResult, CoverageError> {
    let maximum_intervals =
        (f64::from(GRID_RADIUS_KM) * 1000.0 / config.profile_sample_spacing_m).ceil() as usize;
    let mut pfl = Vec::with_capacity(maximum_intervals + 3);
    let mut pixels = Vec::with_capacity(receivers.len());
    for receiver in receivers {
        if control.is_some_and(|value| value.cancelled.load(Ordering::Acquire)) {
            return Err(CoverageError::Cancelled);
        }
        let interval_count =
            (receiver.distance_m / config.profile_sample_spacing_m).ceil() as usize;
        let sample_spacing_m = receiver.distance_m / interval_count as f64;
        pfl.clear();
        pfl.push(interval_count as f64);
        pfl.push(sample_spacing_m);
        let mut water_sample_count = 0_usize;
        let mut path = SphericalPath::new(config.center, receiver.point, interval_count);
        for sample_index in 0..=interval_count {
            if sample_index % 64 == 0
                && control.is_some_and(|value| value.cancelled.load(Ordering::Relaxed))
            {
                return Err(CoverageError::Cancelled);
            }
            let point = if sample_index == interval_count {
                receiver.point
            } else {
                path.current()
            };
            let elevation = profile_terrain_elevation(
                elevation_source,
                point,
                sample_index,
                parameters.tx_ground_elevation_m,
            )?;
            if water_source
                .is_water(point.lon, point.lat)
                .map_err(|message| CoverageError::WaterMask { point, message })?
            {
                water_sample_count += 1;
            }
            pfl.push(elevation);
            path.advance();
        }
        let water_fraction = water_sample_count as f64 / (interval_count + 1) as f64;
        let mut path_inputs = parameters.prediction_inputs;
        path_inputs.ground = ModelDefaults::LAND_WATER_V1
            .ground_for_water_fraction(water_fraction)
            .map_err(|error| CoverageError::Propagation(error.to_string()))?;
        let prediction = predict_p2p_pfl(&pfl, path_inputs)
            .map_err(|error| CoverageError::Propagation(error.to_string()))?;
        let received_power = received_power_dbm(
            parameters.tx_power_dbm,
            config.tx_gain_dbi,
            config.rx_gain_dbi,
            prediction.basic_transmission_loss_db,
        )
        .map_err(|error| CoverageError::Propagation(error.to_string()))?;
        pixels.push(PixelResult {
            raster_index: receiver.raster_index,
            received_power_dbm: received_power as f32,
            warnings: prediction.warnings,
            mode: prediction.mode,
            water_fraction,
        });
        if let Some(control) = control {
            let completed = control.completed.fetch_add(1, Ordering::AcqRel) + 1;
            if completed == control.total || completed % control.notification_interval == 0 {
                (control.progress)(CoverageProgress {
                    completed_pixel_count: completed,
                    total_pixel_count: control.total,
                });
            }
        }
    }
    Ok(ChunkResult { pixels })
}

fn resolve_tx_ground_elevation(
    elevation_source: &impl ElevationSource,
    config: CoverageConfig,
) -> Result<f64, CoverageError> {
    let dem_tx_ground_elevation_m = sample_terrain(elevation_source, config.center)?;
    Ok(config
        .tx_ground_elevation_override_m
        .unwrap_or(dem_tx_ground_elevation_m))
}

fn profile_terrain_elevation(
    elevation_source: &impl ElevationSource,
    point: GeoPoint,
    sample_index: usize,
    tx_ground_elevation_m: f64,
) -> Result<f64, CoverageError> {
    if sample_index == 0 {
        Ok(tx_ground_elevation_m)
    } else {
        sample_terrain(elevation_source, point)
    }
}

fn sample_terrain(
    elevation_source: &impl ElevationSource,
    point: GeoPoint,
) -> Result<f64, CoverageError> {
    let elevation = elevation_source
        .elevation_m(point.lon, point.lat)
        .map_err(|message| CoverageError::Terrain { point, message })?;
    if !elevation.is_finite() {
        return Err(CoverageError::Terrain {
            point,
            message: "terrain elevation is not finite".into(),
        });
    }
    Ok(f64::from(elevation))
}

fn generate_receivers(center: GeoPoint) -> Vec<Receiver> {
    let geodesic = Geodesic::wgs84();
    let mut receivers = Vec::with_capacity(126_000);
    for y_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
        for x_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
            if x_km == 0 && y_km == 0 {
                continue;
            }
            let Some((point, distance_m)) = local_grid_point(&geodesic, center, x_km, y_km) else {
                continue;
            };
            let row = (GRID_RADIUS_KM - y_km) as usize;
            let column = (x_km + GRID_RADIUS_KM) as usize;
            receivers.push(Receiver {
                raster_index: row * GRID_SIZE + column,
                point,
                distance_m,
            });
        }
    }
    receivers
}

#[derive(Clone, Copy, Debug)]
struct Vector3 {
    x: f64,
    y: f64,
    z: f64,
}

impl Vector3 {
    fn from_point(point: GeoPoint) -> Self {
        let lat = point.lat.to_radians();
        let lon = point.lon.to_radians();
        Self {
            x: lat.cos() * lon.cos(),
            y: lat.cos() * lon.sin(),
            z: lat.sin(),
        }
    }

    fn dot(self, other: Self) -> f64 {
        self.x * other.x + self.y * other.y + self.z * other.z
    }
}

#[derive(Clone, Copy, Debug)]
struct SphericalPath {
    start: Vector3,
    tangent: Vector3,
    cosine: f64,
    sine: f64,
    cosine_step: f64,
    sine_step: f64,
}

impl SphericalPath {
    fn new(start: GeoPoint, end: GeoPoint, interval_count: usize) -> Self {
        let start = Vector3::from_point(start);
        let end = Vector3::from_point(end);
        let cosine_omega = start.dot(end).clamp(-1.0, 1.0);
        let omega = cosine_omega.acos();
        let sine_omega = omega.sin();
        let tangent = Vector3 {
            x: (end.x - start.x * cosine_omega) / sine_omega,
            y: (end.y - start.y * cosine_omega) / sine_omega,
            z: (end.z - start.z * cosine_omega) / sine_omega,
        };
        let step = omega / interval_count as f64;
        Self {
            start,
            tangent,
            cosine: 1.0,
            sine: 0.0,
            cosine_step: step.cos(),
            sine_step: step.sin(),
        }
    }

    fn current(self) -> GeoPoint {
        let x = self.start.x * self.cosine + self.tangent.x * self.sine;
        let y = self.start.y * self.cosine + self.tangent.y * self.sine;
        let z = self.start.z * self.cosine + self.tangent.z * self.sine;
        GeoPoint {
            lat: z.clamp(-1.0, 1.0).asin().to_degrees(),
            lon: y.atan2(x).to_degrees(),
        }
    }

    fn advance(&mut self) {
        let next_cosine = self.cosine * self.cosine_step - self.sine * self.sine_step;
        let next_sine = self.sine * self.cosine_step + self.cosine * self.sine_step;
        self.cosine = next_cosine;
        self.sine = next_sine;
    }
}

pub fn color_for_dbm(value: f32) -> [u8; 4] {
    const ANCHORS: [(f32, [u8; 3]); 6] = [
        (-60.0, [255, 0, 0]),
        (-75.0, [255, 165, 0]),
        (-90.0, [255, 255, 0]),
        (-105.0, [0, 180, 0]),
        (-120.0, [0, 255, 255]),
        (-140.0, [0, 0, 255]),
    ];
    if !value.is_finite() || value < -140.0 {
        return [0, 0, 0, 0];
    }
    if value >= ANCHORS[0].0 {
        return [255, 0, 0, 255];
    }
    for pair in ANCHORS.windows(2) {
        let (high_value, high_color) = pair[0];
        let (low_value, low_color) = pair[1];
        if value >= low_value {
            let fraction = (high_value - value) / (high_value - low_value);
            return [
                lerp_channel(high_color[0], low_color[0], fraction),
                lerp_channel(high_color[1], low_color[1], fraction),
                lerp_channel(high_color[2], low_color[2], fraction),
                255,
            ];
        }
    }
    [0, 0, 255, 255]
}

fn lerp_channel(start: u8, end: u8, fraction: f32) -> u8 {
    (f32::from(start) + (f32::from(end) - f32::from(start)) * fraction)
        .round()
        .clamp(0.0, 255.0) as u8
}

#[derive(Debug)]
pub enum CoverageError {
    InvalidInput(String),
    Terrain { point: GeoPoint, message: String },
    WaterMask { point: GeoPoint, message: String },
    Propagation(String),
    Worker(String),
    Cancelled,
    Io(std::io::Error),
    Png(png::EncodingError),
}

impl fmt::Display for CoverageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(message) => write!(formatter, "invalid coverage input: {message}"),
            Self::Terrain { point, message } => write!(
                formatter,
                "terrain sampling failed at ({:.8}, {:.8}): {message}",
                point.lon, point.lat
            ),
            Self::WaterMask { point, message } => write!(
                formatter,
                "water-mask sampling failed at ({:.8}, {:.8}): {message}",
                point.lon, point.lat
            ),
            Self::Propagation(message) => write!(formatter, "propagation failed: {message}"),
            Self::Worker(message) => write!(formatter, "coverage worker failed: {message}"),
            Self::Cancelled => write!(formatter, "coverage calculation cancelled"),
            Self::Io(error) => write!(formatter, "coverage output I/O failed: {error}"),
            Self::Png(error) => write!(formatter, "PNG encoding failed: {error}"),
        }
    }
}

impl Error for CoverageError {}

impl From<std::io::Error> for CoverageError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<png::EncodingError> for CoverageError {
    fn from(error: png::EncodingError) -> Self {
        Self::Png(error)
    }
}

#[cfg(test)]
mod tests {
    use geographiclib_rs::InverseGeodesic;

    use super::*;

    fn constant_circle_values(value: f32) -> Vec<f32> {
        let mut values = vec![f32::NAN; GRID_PIXEL_COUNT];
        for y_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
            for x_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
                if x_km * x_km + y_km * y_km > GRID_RADIUS_KM * GRID_RADIUS_KM {
                    continue;
                }
                let row = (GRID_RADIUS_KM - y_km) as usize;
                let column = (x_km + GRID_RADIUS_KM) as usize;
                values[row * GRID_SIZE + column] = value;
            }
        }
        values
    }

    fn nearest_overlay_index(layout: MapOverlayLayout, point: GeoPoint) -> usize {
        let (x_m, y_m) = web_mercator_project(layout.center, point);
        let column = ((x_m - layout.sample_min_x_m) / layout.sample_step_x_m)
            .round()
            .clamp(0.0, (GRID_SIZE - 1) as f64) as usize;
        let row = ((layout.sample_max_y_m - y_m) / layout.sample_step_y_m)
            .round()
            .clamp(0.0, (GRID_SIZE - 1) as f64) as usize;
        (row * GRID_SIZE + column) * 4
    }

    #[test]
    fn fixed_circle_contains_expected_number_of_receivers() {
        let receivers = generate_receivers(GeoPoint {
            lat: 30.5,
            lon: 103.5,
        });
        assert_eq!(receivers.len(), 125_628);
        assert!(
            receivers
                .iter()
                .all(|receiver| receiver.distance_m <= 200_000.0)
        );
        assert!(
            receivers
                .iter()
                .all(|receiver| receiver.distance_m >= 1_000.0)
        );
    }

    #[test]
    fn fixed_color_thresholds_and_transparency_are_stable() {
        assert_eq!(color_for_dbm(-60.0), [255, 0, 0, 255]);
        assert_eq!(color_for_dbm(-75.0), [255, 165, 0, 255]);
        assert_eq!(color_for_dbm(-90.0), [255, 255, 0, 255]);
        assert_eq!(color_for_dbm(-105.0), [0, 180, 0, 255]);
        assert_eq!(color_for_dbm(-120.0), [0, 255, 255, 255]);
        assert_eq!(color_for_dbm(-140.0), [0, 0, 255, 255]);
        assert_eq!(color_for_dbm(-140.001), [0, 0, 0, 0]);
        assert_eq!(color_for_dbm(f32::NAN), [0, 0, 0, 0]);
    }

    #[test]
    fn spherical_profile_stays_within_one_dem_pixel_of_wgs84_geodesic() {
        let geodesic = Geodesic::wgs84();
        let center = GeoPoint {
            lat: 30.5,
            lon: 103.5,
        };
        for azimuth in [0.0, 45.0, 90.0, 135.0, 180.0, -90.0] {
            let (end_lat, end_lon, _): (f64, f64, f64) =
                geodesic.direct(center.lat, center.lon, azimuth, 200_000.0);
            let mut path = SphericalPath::new(
                center,
                GeoPoint {
                    lat: end_lat,
                    lon: end_lon,
                },
                2,
            );
            path.advance();
            let midpoint = path.current();
            let (expected_lat, expected_lon, _): (f64, f64, f64) =
                geodesic.direct(center.lat, center.lon, azimuth, 100_000.0);
            let error_m: f64 =
                geodesic.inverse(midpoint.lat, midpoint.lon, expected_lat, expected_lon);
            assert!(error_m < 90.0, "azimuth {azimuth}: {error_m} m");
        }
    }

    #[test]
    fn constants_match_the_product_grid() {
        assert_eq!(GRID_SIZE, 401);
        assert_eq!(GRID_PIXEL_COUNT, 160_801);
        assert!((PROFILE_SAMPLE_SPACING_M - 90.0).abs() < f64::EPSILON);
    }

    #[test]
    fn automatic_tx_ground_elevation_uses_the_validated_center_dem_sample() {
        struct CountingElevation {
            calls: AtomicUsize,
            elevation_m: f32,
        }

        impl ElevationSource for CountingElevation {
            fn elevation_m(&self, _lon: f64, _lat: f64) -> Result<f32, String> {
                self.calls.fetch_add(1, Ordering::Relaxed);
                Ok(self.elevation_m)
            }
        }

        let source = CountingElevation {
            calls: AtomicUsize::new(0),
            elevation_m: 412.25,
        };
        let config = CoverageConfig::base_to_handheld(
            GeoPoint {
                lat: 30.5,
                lon: 103.5,
            },
            145.0,
            1,
        );
        assert_eq!(
            resolve_tx_ground_elevation(&source, config).unwrap(),
            412.25
        );
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn manual_tx_ground_elevation_still_validates_dem_and_only_replaces_profile_origin() {
        struct CountingElevation {
            calls: AtomicUsize,
        }

        impl ElevationSource for CountingElevation {
            fn elevation_m(&self, _lon: f64, _lat: f64) -> Result<f32, String> {
                self.calls.fetch_add(1, Ordering::Relaxed);
                Ok(412.25)
            }
        }

        let source = CountingElevation {
            calls: AtomicUsize::new(0),
        };
        let mut config = CoverageConfig::base_to_handheld(
            GeoPoint {
                lat: 30.5,
                lon: 103.5,
            },
            145.0,
            1,
        );
        config.tx_ground_elevation_override_m = Some(800.0);
        let effective = resolve_tx_ground_elevation(&source, config).unwrap();
        assert_eq!(effective, 800.0);
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);

        assert_eq!(
            profile_terrain_elevation(&source, config.center, 0, effective).unwrap(),
            800.0
        );
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
        assert_eq!(
            profile_terrain_elevation(&source, config.center, 1, effective).unwrap(),
            412.25
        );
        assert_eq!(source.calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn manual_tx_ground_elevation_cannot_bypass_an_invalid_center_dem_sample() {
        let mut config = CoverageConfig::base_to_handheld(
            GeoPoint {
                lat: 30.5,
                lon: 103.5,
            },
            145.0,
            1,
        );
        config.tx_ground_elevation_override_m = Some(800.0);
        let error = resolve_tx_ground_elevation(
            &FlatTerrain {
                elevation_m: f32::NAN,
            },
            config,
        )
        .unwrap_err();
        assert!(matches!(error, CoverageError::Terrain { .. }));
    }

    #[test]
    fn tx_ground_elevation_override_bounds_and_non_finite_values_are_rejected() {
        let mut config = CoverageConfig::base_to_handheld(
            GeoPoint {
                lat: 30.5,
                lon: 103.5,
            },
            145.0,
            1,
        );
        for accepted in [-500.0, 9000.0] {
            config.tx_ground_elevation_override_m = Some(accepted);
            assert!(config.validate().is_ok());
        }
        for rejected in [
            -500.001,
            9000.001,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            config.tx_ground_elevation_override_m = Some(rejected);
            assert!(matches!(
                config.validate(),
                Err(CoverageError::InvalidInput(_))
            ));
        }
    }

    #[test]
    fn map_overlay_geometry_stays_within_one_kilometre_across_china_latitudes() {
        let geodesic = Geodesic::wgs84();
        for latitude in [18.0, 30.5, 40.0, 54.0] {
            let center = GeoPoint {
                lat: latitude,
                lon: 104.0,
            };
            let layout = MapOverlayLayout::new(center);
            let mut maximum_error_m = 0.0_f64;
            for y_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
                for x_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
                    let Some((point, _)) = local_grid_point(&geodesic, center, x_km, y_km) else {
                        continue;
                    };
                    let nearest = layout.nearest_sample(point);
                    let error_m: f64 =
                        geodesic.inverse(point.lat, point.lon, nearest.lat, nearest.lon);
                    maximum_error_m = maximum_error_m.max(error_m);
                }
            }
            println!(
                "map overlay maximum sample-centre error at {latitude:.1} degrees: {maximum_error_m:.3} m"
            );
            assert!(
                maximum_error_m < 1_000.0,
                "latitude {latitude}: {maximum_error_m} m"
            );
        }
    }

    #[test]
    fn map_overlay_corners_are_an_axis_aligned_mercator_rectangle() {
        for latitude in [18.0, 30.5, 40.0, 54.0] {
            let center = GeoPoint {
                lat: latitude,
                lon: 104.0,
            };
            let layout = MapOverlayLayout::new(center);
            let projected = layout.corners.map(|corner| {
                web_mercator_project(
                    center,
                    GeoPoint {
                        lon: corner[0],
                        lat: corner[1],
                    },
                )
            });
            assert!((projected[0].1 - projected[1].1).abs() < 1e-6);
            assert!((projected[2].1 - projected[3].1).abs() < 1e-6);
            assert!((projected[0].0 - projected[3].0).abs() < 1e-6);
            assert!((projected[1].0 - projected[2].0).abs() < 1e-6);
            let center_sample =
                layout.sample_point(GRID_RADIUS_KM as usize, GRID_RADIUS_KM as usize);
            let center_error_m: f64 = Geodesic::wgs84().inverse(
                center.lat,
                center.lon,
                center_sample.lat,
                center_sample.lon,
            );
            assert!(center_error_m < 1e-6, "{center_error_m} m");
        }
    }

    #[test]
    fn map_overlay_keeps_circle_transparency_and_cardinal_interior_visible() {
        let center = GeoPoint {
            lat: 40.0,
            lon: 104.0,
        };
        let geodesic = Geodesic::wgs84();
        let layout = MapOverlayLayout::new(center);
        let rgba = render_map_overlay(&constant_circle_values(-90.0), layout);
        for index in [
            0,
            (GRID_SIZE - 1) * 4,
            (GRID_SIZE - 1) * GRID_SIZE * 4,
            (GRID_PIXEL_COUNT - 1) * 4,
        ] {
            assert_eq!(rgba[index + 3], 0, "overlay corner must be transparent");
        }
        assert_eq!(
            rgba[(GRID_RADIUS_KM as usize * GRID_SIZE + GRID_RADIUS_KM as usize) * 4 + 3],
            255
        );
        for azimuth in [0.0, 90.0, 180.0, -90.0] {
            let (lat, lon, _): (f64, f64, f64) =
                geodesic.direct(center.lat, center.lon, azimuth, 199_000.0);
            let index = nearest_overlay_index(layout, GeoPoint { lat, lon });
            assert_eq!(rgba[index + 3], 255, "azimuth {azimuth}");
        }
    }

    #[test]
    fn map_overlay_pixel_sampling_matches_absolute_affine_field_and_image_uv() {
        let center = GeoPoint {
            lat: 35.0,
            lon: 104.0,
        };
        let geodesic = Geodesic::wgs84();
        let layout = MapOverlayLayout::new(center);
        let mut values = vec![f32::NAN; GRID_PIXEL_COUNT];
        for y_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
            for x_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
                if x_km * x_km + y_km * y_km > GRID_RADIUS_KM * GRID_RADIUS_KM {
                    continue;
                }
                let row = (GRID_RADIUS_KM - y_km) as usize;
                let column = (x_km + GRID_RADIUS_KM) as usize;
                values[row * GRID_SIZE + column] =
                    (-100.0 + 0.031 * f64::from(x_km) - 0.047 * f64::from(y_km)) as f32;
            }
        }
        let edge_mercator = layout.corners.map(|corner| {
            web_mercator_project(
                center,
                GeoPoint {
                    lon: corner[0],
                    lat: corner[1],
                },
            )
        });
        for (row, column) in [
            (200, 200),
            (165, 235),
            (238, 158),
            (110, 230),
            (260, 310),
            (285, 95),
        ] {
            let point = layout.sample_point(row, column);
            let (distance_m, azimuth_deg, _, _): (f64, f64, f64, f64) =
                geodesic.inverse(center.lat, center.lon, point.lat, point.lon);
            let azimuth_rad = azimuth_deg.to_radians();
            let local_x_km = distance_m * azimuth_rad.sin() / 1000.0;
            let local_y_km = distance_m * azimuth_rad.cos() / 1000.0;
            let expected_dbm = -100.0 + 0.031 * local_x_km - 0.047 * local_y_km;
            let actual_dbm = f64::from(map_overlay_sample_dbm(
                &values, layout, &geodesic, row, column,
            ));
            assert!(
                (actual_dbm - expected_dbm).abs() < 2e-5,
                "row {row}, column {column}: actual {actual_dbm}, expected {expected_dbm}"
            );

            let u = (column as f64 + 0.5) / GRID_SIZE as f64;
            let v = (row as f64 + 0.5) / GRID_SIZE as f64;
            let image_x_m = edge_mercator[0].0 + u * (edge_mercator[1].0 - edge_mercator[0].0);
            let image_y_m = edge_mercator[0].1 + v * (edge_mercator[3].1 - edge_mercator[0].1);
            let (sample_x_m, sample_y_m) = web_mercator_project(center, point);
            assert!((image_x_m - sample_x_m).abs() < 1e-6);
            assert!((image_y_m - sample_y_m).abs() < 1e-6);
        }
    }

    #[test]
    fn map_overlay_render_preserves_synthetic_east_and_north_gradient_directions() {
        let center = GeoPoint {
            lat: 40.0,
            lon: 104.0,
        };
        let mut values = vec![f32::NAN; GRID_PIXEL_COUNT];
        for y_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
            for x_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
                if x_km * x_km + y_km * y_km > GRID_RADIUS_KM * GRID_RADIUS_KM {
                    continue;
                }
                let row = (GRID_RADIUS_KM - y_km) as usize;
                let column = (x_km + GRID_RADIUS_KM) as usize;
                values[row * GRID_SIZE + column] = -112.0 + 0.05 * x_km as f32 + 0.1 * y_km as f32;
            }
        }
        let rgba = render_map_overlay(&values, MapOverlayLayout::new(center));
        let pixel = |row: usize, column: usize| {
            let index = (row * GRID_SIZE + column) * 4;
            <[u8; 4]>::try_from(&rgba[index..index + 4]).unwrap()
        };
        let east = pixel(200, 210);
        let west = pixel(200, 190);
        let north = pixel(190, 200);
        let south = pixel(210, 200);
        for sample in [east, west, north, south] {
            assert_eq!(sample[0], 0, "samples must stay in the green-to-cyan band");
            assert_eq!(sample[3], 255);
        }
        assert!(
            east[2] + 5 < west[2],
            "eastward increasing dBm gradient was reversed: east={east:?}, west={west:?}"
        );
        assert!(
            north[2] + 5 < south[2],
            "northward increasing dBm gradient was reversed: north={north:?}, south={south:?}"
        );
    }

    #[test]
    fn exact_cardinal_200_km_points_fit_bounds_with_one_ground_pixel_raster_tolerance() {
        let geodesic = Geodesic::wgs84();
        for latitude in [18.0, 30.5, 40.0, 54.0] {
            let center = GeoPoint {
                lat: latitude,
                lon: 104.0,
            };
            let layout = MapOverlayLayout::new(center);
            let rgba = render_map_overlay(&constant_circle_values(-90.0), layout);
            let edge_min_x_m = layout.sample_min_x_m - layout.sample_step_x_m / 2.0;
            let edge_max_y_m = layout.sample_max_y_m + layout.sample_step_y_m / 2.0;
            for azimuth in [0.0, 90.0, 180.0, -90.0] {
                let (lat, lon, _): (f64, f64, f64) =
                    geodesic.direct(center.lat, center.lon, azimuth, 200_000.0);
                let (x_m, y_m) = web_mercator_project(center, GeoPoint { lat, lon });
                let image_column = (x_m - edge_min_x_m) / layout.sample_step_x_m - 0.5;
                let image_row = (edge_max_y_m - y_m) / layout.sample_step_y_m - 0.5;
                assert!(
                    (-1e-8..=(GRID_SIZE - 1) as f64 + 1e-8).contains(&image_column),
                    "latitude {latitude}, azimuth {azimuth}: column {image_column}"
                );
                assert!(
                    (-1e-8..=(GRID_SIZE - 1) as f64 + 1e-8).contains(&image_row),
                    "latitude {latitude}, azimuth {azimuth}: row {image_row}"
                );
                let nearest_index = nearest_overlay_index(layout, GeoPoint { lat, lon });
                let nearest_is_transparent = rgba[nearest_index + 3] == 0;
                let nearest_row = image_row.round().clamp(0.0, (GRID_SIZE - 1) as f64) as usize;
                let nearest_column =
                    image_column.round().clamp(0.0, (GRID_SIZE - 1) as f64) as usize;
                let mut nearest_visible_error_m = f64::INFINITY;
                for row in nearest_row.saturating_sub(1)..=(nearest_row + 1).min(GRID_SIZE - 1) {
                    for column in
                        nearest_column.saturating_sub(1)..=(nearest_column + 1).min(GRID_SIZE - 1)
                    {
                        let index = (row * GRID_SIZE + column) * 4;
                        if rgba[index + 3] == 0 {
                            continue;
                        }
                        let sample = layout.sample_point(row, column);
                        let error_m: f64 = geodesic.inverse(lat, lon, sample.lat, sample.lon);
                        nearest_visible_error_m = nearest_visible_error_m.min(error_m);
                    }
                }
                let nearest_sample = layout.sample_point(nearest_row, nearest_column);
                let mut ground_pixel_diagonal_m = 0.0_f64;
                for row_offset in [-1_isize, 1] {
                    for column_offset in [-1_isize, 1] {
                        let Some(diagonal_row) = nearest_row.checked_add_signed(row_offset) else {
                            continue;
                        };
                        let Some(diagonal_column) =
                            nearest_column.checked_add_signed(column_offset)
                        else {
                            continue;
                        };
                        if diagonal_row >= GRID_SIZE || diagonal_column >= GRID_SIZE {
                            continue;
                        }
                        let diagonal_sample = layout.sample_point(diagonal_row, diagonal_column);
                        let diagonal_m: f64 = geodesic.inverse(
                            nearest_sample.lat,
                            nearest_sample.lon,
                            diagonal_sample.lat,
                            diagonal_sample.lon,
                        );
                        ground_pixel_diagonal_m = ground_pixel_diagonal_m.max(diagonal_m);
                    }
                }
                assert!(ground_pixel_diagonal_m > 0.0);
                assert!(
                    nearest_visible_error_m <= ground_pixel_diagonal_m + 1e-6,
                    "latitude {latitude}, azimuth {azimuth}: no visible nearby pixel within one geodesic output-pixel diagonal rasterization tolerance ({ground_pixel_diagonal_m:.3} m)"
                );
                if nearest_is_transparent {
                    println!(
                        "exact 200 km boundary has a transparent nearest centre at latitude {latitude}, azimuth {azimuth}; nearest visible nearby centre is {nearest_visible_error_m:.3} m away, within the {ground_pixel_diagonal_m:.3} m geodesic output-pixel diagonal rasterization tolerance"
                    );
                }
            }
        }
    }

    #[test]
    fn map_overlay_bilinear_sampling_renormalizes_around_nan() {
        let mut values = vec![f32::NAN; GRID_PIXEL_COUNT];
        values[200 * GRID_SIZE + 201] = -80.0;
        values[201 * GRID_SIZE + 200] = -100.0;
        values[201 * GRID_SIZE + 201] = -120.0;
        let sampled = sample_dbm_nan_aware(&values, 200.5, 200.5);
        assert!((sampled - -100.0).abs() < 1e-6, "{sampled}");
        assert!(sample_dbm_nan_aware(&values, 100.0, 100.0).is_nan());
    }

    #[test]
    fn map_overlay_png_is_deterministic_and_fixed_size() {
        let center = GeoPoint {
            lat: 30.5,
            lon: 103.5,
        };
        let values = constant_circle_values(-105.0);
        let first = encode_map_overlay_values(&values, center).unwrap();
        let second = encode_map_overlay_values(&values, center).unwrap();
        assert_eq!(first.projection, MAP_OVERLAY_PROJECTION);
        assert_eq!((first.width, first.height), (GRID_SIZE, GRID_SIZE));
        assert_eq!(first.png, second.png);
        assert_eq!(&first.png[16..20], &(GRID_SIZE as u32).to_be_bytes());
        assert_eq!(&first.png[20..24], &(GRID_SIZE as u32).to_be_bytes());
    }

    #[test]
    fn all_water_path_uses_different_ground_parameters_than_all_land() {
        let center = GeoPoint {
            lat: 36.0671,
            lon: 120.3826,
        };
        let geodesic = Geodesic::wgs84();
        let config = CoverageConfig::base_to_handheld(center, 145.0, 1);
        let mut inputs = PredictionInputs::land_water_v1(145.0, Polarization::Vertical);
        inputs.tx_height_m = config.tx_height_m;
        inputs.rx_height_m = config.rx_height_m;
        let tx_power_dbm = watts_to_dbm(config.tx_power_w).unwrap();
        let terrain = FlatTerrain { elevation_m: 0.0 };
        let mut maximum_difference_db = 0.0_f32;
        for distance_m in [1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0] {
            let (lat, lon, _): (f64, f64, f64) =
                geodesic.direct(center.lat, center.lon, 90.0, distance_m);
            let receiver = Receiver {
                raster_index: 0,
                point: GeoPoint { lat, lon },
                distance_m,
            };
            let land = compute_chunk(
                &terrain,
                &UniformWater { is_water: false },
                config,
                ChunkParameters {
                    prediction_inputs: inputs,
                    tx_power_dbm,
                    tx_ground_elevation_m: 0.0,
                },
                &[receiver],
                None,
            )
            .unwrap();
            let water = compute_chunk(
                &terrain,
                &UniformWater { is_water: true },
                config,
                ChunkParameters {
                    prediction_inputs: inputs,
                    tx_power_dbm,
                    tx_ground_elevation_m: 0.0,
                },
                &[receiver],
                None,
            )
            .unwrap();
            assert_eq!(land.pixels[0].water_fraction, 0.0);
            assert_eq!(water.pixels[0].water_fraction, 1.0);
            maximum_difference_db = maximum_difference_db.max(
                (water.pixels[0].received_power_dbm - land.pixels[0].received_power_dbm).abs(),
            );
        }
        assert!(maximum_difference_db > 0.001);
    }

    #[test]
    fn pre_cancelled_calculation_stops_before_sampling() {
        let cancelled = AtomicBool::new(true);
        let error = compute_coverage_with_control(
            &FlatTerrain { elevation_m: 0.0 },
            &UniformWater { is_water: false },
            CoverageConfig::base_to_handheld(
                GeoPoint {
                    lat: 30.5,
                    lon: 103.5,
                },
                145.0,
                1,
            ),
            &cancelled,
            |_| panic!("a cancelled calculation must not report progress"),
        )
        .unwrap_err();
        assert!(matches!(error, CoverageError::Cancelled));
    }

    #[test]
    fn running_calculation_can_be_cancelled_from_progress_callback() {
        let cancelled = AtomicBool::new(false);
        let error = compute_coverage_with_control(
            &FlatTerrain { elevation_m: 0.0 },
            &UniformWater { is_water: false },
            CoverageConfig::base_to_handheld(
                GeoPoint {
                    lat: 30.5,
                    lon: 103.5,
                },
                145.0,
                2,
            ),
            &cancelled,
            |progress| {
                if progress.completed_pixel_count > 0 {
                    cancelled.store(true, Ordering::Release);
                }
            },
        )
        .unwrap_err();
        assert!(matches!(error, CoverageError::Cancelled));
    }
}
