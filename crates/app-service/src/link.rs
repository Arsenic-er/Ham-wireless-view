// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use hamheatmap_cache::{CacheStore, GeoPoint as CacheGeoPoint, plan_glo90_region};
use hamheatmap_link_analysis::{
    GeometryClassification, LinkAnalysisConfig as CoreLinkAnalysisConfig,
    LinkAnalysisResult as CoreLinkAnalysisResult, analyze_link as analyze_link_core,
};
use hamheatmap_propagation::{
    Polarization, PropagationMode, dbd_to_dbi, dbm_to_watts, watts_to_dbm,
};
use hamheatmap_terrain::{DemTileSet, WaterTileSet};
use serde::{Deserialize, Serialize};
use std::sync::atomic::AtomicBool;

use crate::{
    AppService, Band, GainUnit, MapPoint, PolarizationChoice, PowerUnit, cache_error_message,
    calculation_cancellation_checkpoint, validate_point, validate_range,
};

pub const LINK_ANALYSIS_RESULT_SCHEMA_VERSION: u32 = 1;
pub const CROSS_POLARIZATION_MISMATCH_LOSS_DB: f64 = 20.0;
pub const CRITICAL_LINK_MARGIN_DB: f64 = 3.0;
pub const CRITICAL_FRESNEL_RATIO_DELTA: f64 = 0.05;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkEndpointRequest {
    pub point: MapPoint,
    pub antenna_height_m: f64,
    pub antenna_gain_value: f64,
    pub antenna_gain_unit: GainUnit,
    pub polarization: PolarizationChoice,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LinkAnalysisRequest {
    pub tx: LinkEndpointRequest,
    pub rx: LinkEndpointRequest,
    pub band: Band,
    pub frequency_mhz: f64,
    pub tx_power_value: f64,
    pub tx_power_unit: PowerUnit,
    pub receiver_threshold_dbm: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LinkClassification {
    DirectLos,
    ObstructedUsable,
    PredictedUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkProfileSampleView {
    pub distance_m: f64,
    pub lat: f64,
    pub lon: f64,
    pub terrain_elevation_m: f64,
    pub earth_bulge_m: f64,
    pub adjusted_terrain_m: f64,
    pub los_height_m: f64,
    pub fresnel_radius_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkAnalysisResult {
    pub schema_version: u32,
    pub classification: LinkClassification,
    pub classification_reason: &'static str,
    pub distance_m: f64,
    pub initial_bearing_deg: f64,
    pub final_bearing_deg: f64,
    pub frequency_mhz: f64,
    pub wavelength_m: f64,
    pub sample_spacing_m: f64,
    pub sample_count: usize,
    pub effective_earth_radius_m: f64,
    pub k_factor: f64,
    pub tx_ground_elevation_m: f64,
    pub rx_ground_elevation_m: f64,
    pub tx_antenna_elevation_m: f64,
    pub rx_antenna_elevation_m: f64,
    pub geometric_los: bool,
    pub fresnel_clearance_60: bool,
    pub minimum_los_clearance_m: f64,
    pub minimum_fresnel_clearance_ratio: f64,
    pub critical_sample_index: usize,
    pub itm_mode: &'static str,
    pub itm_basic_transmission_loss_db: f64,
    pub itm_warnings: u64,
    pub water_fraction: f64,
    pub co_polarized_reference_power_dbm: f64,
    pub polarization_mismatch_loss_db: f64,
    pub predicted_rx_power_dbm: f64,
    pub receiver_threshold_dbm: f64,
    pub link_margin_db: f64,
    pub critical: bool,
    pub profile: Vec<LinkProfileSampleView>,
}

impl AppService {
    pub fn analyze_link(
        &self,
        request: &LinkAnalysisRequest,
        cancelled: &AtomicBool,
    ) -> Result<LinkAnalysisResult, String> {
        calculation_cancellation_checkpoint(cancelled)?;
        let core_config = request_to_core_config(request)?;
        let plan = plan_glo90_region(CacheGeoPoint {
            lat: request.tx.point.lat,
            lon: request.tx.point.lon,
        })
        .map_err(cache_error_message)?;
        let mut store = CacheStore::open(&self.cache_root).map_err(cache_error_message)?;
        store.upsert_region(&plan).map_err(cache_error_message)?;
        let dem_paths = store.ready_paths_for_region(&plan).map_err(|error| {
            format!("{error}; cache the transmitter region before link analysis")
        })?;
        let water_paths = store.ready_water_paths_for_region(&plan).map_err(|error| {
            format!("{error}; cache the transmitter region before link analysis")
        })?;
        store
            .set_active_region(Some(&plan.region_id))
            .map_err(cache_error_message)?;

        let result = (|| {
            calculation_cancellation_checkpoint(cancelled)?;
            let dem = DemTileSet::open_paths(dem_paths).map_err(|error| error.to_string())?;
            let water = WaterTileSet::open_paths(water_paths).map_err(|error| error.to_string())?;
            calculation_cancellation_checkpoint(cancelled)?;
            let core =
                analyze_link_core(&dem, &water, core_config).map_err(|error| error.to_string())?;
            calculation_cancellation_checkpoint(cancelled)?;
            result_from_core(request, core)
        })();
        let clear_result = store.set_active_region(None).map_err(cache_error_message);
        match (result, clear_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }
}

fn request_to_core_config(request: &LinkAnalysisRequest) -> Result<CoreLinkAnalysisConfig, String> {
    validate_point(request.tx.point)?;
    validate_point(request.rx.point)?;
    let frequency_range = match request.band {
        Band::Vhf144 => 144.0..=148.0,
        Band::Uhf430 => 430.0..=440.0,
    };
    if !request.frequency_mhz.is_finite()
        || !frequency_range.contains(&request.frequency_mhz)
        || ((request.frequency_mhz * 100.0).round() - request.frequency_mhz * 100.0).abs() > 1e-8
    {
        return Err(
            "frequency must be within the selected band and use at most two decimals".into(),
        );
    }
    let tx_power_w = match request.tx_power_unit {
        PowerUnit::Watt => request.tx_power_value,
        PowerUnit::Dbm => {
            dbm_to_watts(request.tx_power_value).map_err(|error| error.to_string())?
        }
    };
    validate_range("transmitter power", tx_power_w, 0.1, 1000.0)?;
    validate_range(
        "receiver threshold",
        request.receiver_threshold_dbm,
        -160.0,
        -40.0,
    )?;
    let tx_gain_dbi = endpoint_gain_dbi(request.tx)?;
    let rx_gain_dbi = endpoint_gain_dbi(request.rx)?;
    validate_range(
        "transmitter antenna height",
        request.tx.antenna_height_m,
        0.5,
        500.0,
    )?;
    validate_range(
        "receiver antenna height",
        request.rx.antenna_height_m,
        0.5,
        500.0,
    )?;
    Ok(CoreLinkAnalysisConfig {
        transmitter: hamheatmap_link_analysis::GeoPoint {
            lat: request.tx.point.lat,
            lon: request.tx.point.lon,
        },
        receiver: hamheatmap_link_analysis::GeoPoint {
            lat: request.rx.point.lat,
            lon: request.rx.point.lon,
        },
        frequency_mhz: request.frequency_mhz,
        polarization: polarization(request.tx.polarization),
        tx_height_m: request.tx.antenna_height_m,
        rx_height_m: request.rx.antenna_height_m,
        tx_power_dbm: watts_to_dbm(tx_power_w).map_err(|error| error.to_string())?,
        tx_gain_dbi,
        rx_gain_dbi,
    })
}

fn endpoint_gain_dbi(endpoint: LinkEndpointRequest) -> Result<f64, String> {
    let gain_dbi = match endpoint.antenna_gain_unit {
        GainUnit::Dbi => endpoint.antenna_gain_value,
        GainUnit::Dbd => {
            dbd_to_dbi(endpoint.antenna_gain_value).map_err(|error| error.to_string())?
        }
    };
    validate_range("antenna gain", gain_dbi, -20.0, 30.0)?;
    Ok(gain_dbi)
}

fn polarization(value: PolarizationChoice) -> Polarization {
    match value {
        PolarizationChoice::Horizontal => Polarization::Horizontal,
        PolarizationChoice::Vertical => Polarization::Vertical,
    }
}

fn result_from_core(
    request: &LinkAnalysisRequest,
    core: CoreLinkAnalysisResult,
) -> Result<LinkAnalysisResult, String> {
    let itm_mode = itm_mode_name(core.prediction.mode)?;
    let polarization_mismatch_loss_db = if request.tx.polarization == request.rx.polarization {
        0.0
    } else {
        CROSS_POLARIZATION_MISMATCH_LOSS_DB
    };
    let co_polarized_reference_power_dbm = core.received_power_dbm;
    let co_polarized_reference_margin_db =
        co_polarized_reference_power_dbm - request.receiver_threshold_dbm;
    let predicted_rx_power_dbm = co_polarized_reference_power_dbm - polarization_mismatch_loss_db;
    let link_margin_db = predicted_rx_power_dbm - request.receiver_threshold_dbm;
    let classification = classify_link(
        link_margin_db,
        core.geometry.classification,
        core.prediction.mode,
    )?;
    let classification_reason = classification_reason(
        classification,
        core.geometry.classification,
        core.prediction.mode,
        core.geometry.geometric_line_of_sight,
        polarization_mismatch_loss_db > 0.0,
        co_polarized_reference_margin_db,
        link_margin_db,
    );
    let critical = link_margin_db.abs() <= CRITICAL_LINK_MARGIN_DB
        || (core.geometry.minimum_normalized_fresnel_clearance
            - hamheatmap_link_analysis::REQUIRED_FRESNEL_CLEARANCE_FRACTION)
            .abs()
            <= CRITICAL_FRESNEL_RATIO_DELTA;
    let profile = core
        .profile
        .iter()
        .map(|sample| LinkProfileSampleView {
            distance_m: sample.distance_m,
            lat: sample.point.lat,
            lon: sample.point.lon,
            terrain_elevation_m: sample.terrain_elevation_m,
            earth_bulge_m: sample.earth_bulge_m,
            adjusted_terrain_m: sample.adjusted_terrain_elevation_m,
            los_height_m: sample.line_of_sight_elevation_m,
            fresnel_radius_m: sample.fresnel_radius_m,
        })
        .collect();
    Ok(LinkAnalysisResult {
        schema_version: LINK_ANALYSIS_RESULT_SCHEMA_VERSION,
        classification,
        classification_reason,
        distance_m: core.distance_m,
        initial_bearing_deg: core.initial_bearing_deg.rem_euclid(360.0),
        final_bearing_deg: core.final_bearing_deg.rem_euclid(360.0),
        frequency_mhz: request.frequency_mhz,
        wavelength_m: core.wavelength_m,
        sample_spacing_m: core.sample_spacing_m,
        sample_count: core.profile.len(),
        effective_earth_radius_m: core.effective_earth_radius_m,
        k_factor: hamheatmap_link_analysis::EFFECTIVE_EARTH_K_FACTOR,
        tx_ground_elevation_m: core.tx_ground_elevation_m,
        rx_ground_elevation_m: core.rx_ground_elevation_m,
        tx_antenna_elevation_m: core.tx_ground_elevation_m + request.tx.antenna_height_m,
        rx_antenna_elevation_m: core.rx_ground_elevation_m + request.rx.antenna_height_m,
        geometric_los: core.geometry.geometric_line_of_sight,
        fresnel_clearance_60: core.geometry.sixty_percent_fresnel_clear,
        minimum_los_clearance_m: core.geometry.minimum_line_of_sight_clearance_m,
        minimum_fresnel_clearance_ratio: core.geometry.minimum_normalized_fresnel_clearance,
        critical_sample_index: core.geometry.critical_sample_index,
        itm_mode,
        itm_basic_transmission_loss_db: core.prediction.basic_transmission_loss_db,
        itm_warnings: core.prediction.warnings,
        water_fraction: core.path_water_fraction,
        co_polarized_reference_power_dbm,
        polarization_mismatch_loss_db,
        predicted_rx_power_dbm,
        receiver_threshold_dbm: request.receiver_threshold_dbm,
        link_margin_db,
        critical,
        profile,
    })
}

fn itm_mode_name(mode: PropagationMode) -> Result<&'static str, String> {
    match mode {
        PropagationMode::LineOfSight => Ok("line-of-sight"),
        PropagationMode::Diffraction => Ok("diffraction"),
        PropagationMode::Troposcatter => Ok("troposcatter"),
        PropagationMode::Unknown(value) => Err(format!("ITM returned unknown mode {value}")),
    }
}

fn classify_link(
    link_margin_db: f64,
    geometry: GeometryClassification,
    mode: PropagationMode,
) -> Result<LinkClassification, String> {
    itm_mode_name(mode)?;
    if link_margin_db < 0.0 {
        return Ok(LinkClassification::PredictedUnavailable);
    }
    if geometry == GeometryClassification::DirectLineOfSight && mode == PropagationMode::LineOfSight
    {
        Ok(LinkClassification::DirectLos)
    } else {
        Ok(LinkClassification::ObstructedUsable)
    }
}

fn classification_reason(
    classification: LinkClassification,
    geometry: GeometryClassification,
    mode: PropagationMode,
    geometric_line_of_sight: bool,
    polarization_mismatch: bool,
    co_polarized_reference_margin_db: f64,
    link_margin_db: f64,
) -> &'static str {
    match classification {
        LinkClassification::PredictedUnavailable
            if polarization_mismatch
                && co_polarized_reference_margin_db >= 0.0
                && link_margin_db < 0.0 =>
        {
            "polarization-mismatch"
        }
        LinkClassification::PredictedUnavailable => "negative-link-margin",
        LinkClassification::DirectLos => "positive-margin-direct-los",
        LinkClassification::ObstructedUsable => match (geometry, mode) {
            (GeometryClassification::SeverelyObstructed, _) => {
                "positive-margin-severe-obstruction-modeled-usable"
            }
            (_, PropagationMode::Troposcatter) => "positive-margin-troposcatter",
            (GeometryClassification::FresnelAffected, _) if geometric_line_of_sight => {
                "positive-margin-fresnel-intrusion-geometric-los"
            }
            (_, PropagationMode::Diffraction) => "positive-margin-diffraction",
            _ => "positive-margin-fresnel-obstructed",
        },
    }
}

#[cfg(test)]
mod tests;
