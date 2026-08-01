// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use geographiclib_rs::{DirectGeodesic, Geodesic};
use hamheatmap_link_analysis::{
    FlatTerrain, GeoPoint as CoreGeoPoint, LinkAnalysisConfig as CoreLinkAnalysisConfig,
    UniformWater, analyze_link as analyze_link_core,
};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use super::*;

fn point_pair() -> (MapPoint, MapPoint) {
    let tx = MapPoint {
        lat: 30.0,
        lon: 110.0,
    };
    let geodesic = Geodesic::wgs84();
    let (lat, lon, _): (f64, f64, f64) = geodesic.direct(tx.lat, tx.lon, 90.0, 10_000.0);
    (tx, MapPoint { lat, lon })
}

fn request() -> LinkAnalysisRequest {
    let (tx, rx) = point_pair();
    LinkAnalysisRequest {
        tx: LinkEndpointRequest {
            point: tx,
            antenna_height_m: 50.0,
            antenna_gain_value: 6.0,
            antenna_gain_unit: GainUnit::Dbi,
            polarization: PolarizationChoice::Vertical,
        },
        rx: LinkEndpointRequest {
            point: rx,
            antenna_height_m: 50.0,
            antenna_gain_value: -3.0,
            antenna_gain_unit: GainUnit::Dbi,
            polarization: PolarizationChoice::Vertical,
        },
        band: Band::Vhf144,
        frequency_mhz: 145.0,
        tx_power_value: 25.0,
        tx_power_unit: PowerUnit::Watt,
        receiver_threshold_dbm: -120.0,
    }
}

fn flat_core(request: &LinkAnalysisRequest) -> CoreLinkAnalysisResult {
    let config = request_to_core_config(request).unwrap();
    analyze_link_core(
        &FlatTerrain { elevation_m: 0.0 },
        &UniformWater { is_water: false },
        config,
    )
    .unwrap()
}

#[test]
fn request_json_matches_frontend_contract_exactly_and_rejects_unknown_fields() {
    let value = json!({
        "tx": {
            "point": {"lat": 30.0, "lon": 110.0},
            "antennaHeightM": 20.0,
            "antennaGainValue": 6.0,
            "antennaGainUnit": "dbi",
            "polarization": "vertical"
        },
        "rx": {
            "point": {"lat": 30.1, "lon": 110.1},
            "antennaHeightM": 1.5,
            "antennaGainValue": -3.0,
            "antennaGainUnit": "dbd",
            "polarization": "horizontal"
        },
        "band": "vhf-144",
        "frequencyMhz": 145.0,
        "txPowerValue": 25.0,
        "txPowerUnit": "watt",
        "receiverThresholdDbm": -120.0
    });
    let decoded: LinkAnalysisRequest = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), value);

    let mut unknown_top = value.clone();
    unknown_top
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), json!(true));
    assert!(serde_json::from_value::<LinkAnalysisRequest>(unknown_top).is_err());

    let mut unknown_endpoint = value;
    unknown_endpoint["tx"]
        .as_object_mut()
        .unwrap()
        .insert("unexpected".into(), json!(true));
    assert!(serde_json::from_value::<LinkAnalysisRequest>(unknown_endpoint).is_err());
}

#[test]
fn request_normalizes_power_gain_and_transmitter_polarization() {
    let mut request = request();
    request.tx_power_value = 43.979_400_086_720_375;
    request.tx_power_unit = PowerUnit::Dbm;
    request.tx.antenna_gain_value = 0.0;
    request.tx.antenna_gain_unit = GainUnit::Dbd;
    request.tx.polarization = PolarizationChoice::Horizontal;
    request.rx.polarization = PolarizationChoice::Vertical;
    let config = request_to_core_config(&request).unwrap();
    assert!((config.tx_power_dbm - request.tx_power_value).abs() < 1e-10);
    assert!((config.tx_gain_dbi - 2.15).abs() < 1e-12);
    assert_eq!(config.polarization, Polarization::Horizontal);
}

#[test]
fn request_validation_enforces_band_threshold_gain_and_height() {
    let mut invalid = request();
    invalid.frequency_mhz = 435.0;
    assert!(request_to_core_config(&invalid).is_err());
    invalid = request();
    invalid.receiver_threshold_dbm = -160.1;
    assert!(request_to_core_config(&invalid).is_err());
    invalid = request();
    invalid.tx.antenna_gain_value = 30.1;
    assert!(request_to_core_config(&invalid).is_err());
    invalid = request();
    invalid.rx.antenna_height_m = 0.49;
    assert!(request_to_core_config(&invalid).is_err());
}

#[test]
fn cross_polarization_applies_public_twenty_db_planning_loss() {
    let matched_request = request();
    let matched = result_from_core(&matched_request, flat_core(&matched_request)).unwrap();
    assert_eq!(matched.polarization_mismatch_loss_db, 0.0);
    assert_eq!(
        matched.co_polarized_reference_power_dbm,
        matched.predicted_rx_power_dbm
    );

    let mut crossed_request = matched_request;
    crossed_request.rx.polarization = PolarizationChoice::Horizontal;
    let crossed = result_from_core(&crossed_request, flat_core(&crossed_request)).unwrap();
    assert_eq!(
        crossed.polarization_mismatch_loss_db,
        CROSS_POLARIZATION_MISMATCH_LOSS_DB
    );
    assert!(
        (crossed.co_polarized_reference_power_dbm
            - crossed.predicted_rx_power_dbm
            - CROSS_POLARIZATION_MISMATCH_LOSS_DB)
            .abs()
            < 1e-12
    );

    crossed_request.receiver_threshold_dbm = crossed.co_polarized_reference_power_dbm;
    let unavailable_after_mismatch =
        result_from_core(&crossed_request, flat_core(&crossed_request)).unwrap();
    assert_eq!(
        unavailable_after_mismatch.classification,
        LinkClassification::PredictedUnavailable
    );
    assert_eq!(
        unavailable_after_mismatch.classification_reason,
        "polarization-mismatch"
    );

    crossed_request.receiver_threshold_dbm = crossed.co_polarized_reference_power_dbm + 1.0;
    let already_unavailable =
        result_from_core(&crossed_request, flat_core(&crossed_request)).unwrap();
    assert_eq!(
        already_unavailable.classification,
        LinkClassification::PredictedUnavailable
    );
    assert_eq!(
        already_unavailable.classification_reason,
        "negative-link-margin"
    );
}

#[test]
fn final_classification_combines_margin_geometry_and_itm_mode() {
    assert_eq!(
        classify_link(
            1.0,
            GeometryClassification::DirectLineOfSight,
            PropagationMode::LineOfSight
        )
        .unwrap(),
        LinkClassification::DirectLos
    );
    assert_eq!(
        classify_link(
            1.0,
            GeometryClassification::FresnelAffected,
            PropagationMode::Diffraction
        )
        .unwrap(),
        LinkClassification::ObstructedUsable
    );
    assert_eq!(
        classify_link(
            -0.001,
            GeometryClassification::DirectLineOfSight,
            PropagationMode::LineOfSight
        )
        .unwrap(),
        LinkClassification::PredictedUnavailable
    );
    assert!(
        classify_link(
            10.0,
            GeometryClassification::DirectLineOfSight,
            PropagationMode::Unknown(99)
        )
        .is_err()
    );
}

#[test]
fn usable_obstruction_reason_codes_preserve_geometry_evidence() {
    assert_eq!(
        classification_reason(
            LinkClassification::ObstructedUsable,
            GeometryClassification::SeverelyObstructed,
            PropagationMode::Diffraction,
            false,
            false,
            10.0,
            10.0,
        ),
        "positive-margin-severe-obstruction-modeled-usable"
    );
    assert_eq!(
        classification_reason(
            LinkClassification::ObstructedUsable,
            GeometryClassification::FresnelAffected,
            PropagationMode::LineOfSight,
            true,
            false,
            10.0,
            10.0,
        ),
        "positive-margin-fresnel-intrusion-geometric-los"
    );
    assert_eq!(
        classification_reason(
            LinkClassification::ObstructedUsable,
            GeometryClassification::FresnelAffected,
            PropagationMode::Diffraction,
            false,
            false,
            10.0,
            10.0,
        ),
        "positive-margin-diffraction"
    );
}

#[test]
fn result_serialization_has_exact_frontend_fields_and_finite_profile() {
    let request = request();
    let result = result_from_core(&request, flat_core(&request)).unwrap();
    let value = serde_json::to_value(&result).unwrap();
    let object = value.as_object().unwrap();
    let keys = [
        "schemaVersion",
        "classification",
        "classificationReason",
        "distanceM",
        "initialBearingDeg",
        "finalBearingDeg",
        "frequencyMhz",
        "wavelengthM",
        "sampleSpacingM",
        "sampleCount",
        "effectiveEarthRadiusM",
        "kFactor",
        "txGroundElevationM",
        "rxGroundElevationM",
        "txAntennaElevationM",
        "rxAntennaElevationM",
        "geometricLos",
        "fresnelClearance60",
        "minimumLosClearanceM",
        "minimumFresnelClearanceRatio",
        "criticalSampleIndex",
        "itmMode",
        "itmBasicTransmissionLossDb",
        "itmWarnings",
        "waterFraction",
        "coPolarizedReferencePowerDbm",
        "polarizationMismatchLossDb",
        "predictedRxPowerDbm",
        "receiverThresholdDbm",
        "linkMarginDb",
        "critical",
        "profile",
    ];
    for key in keys {
        assert!(object.contains_key(key), "missing {key}");
    }
    assert_eq!(object.len(), keys.len());
    assert_eq!(result.sample_count, result.profile.len());
    assert!(result.critical_sample_index < result.profile.len());
    assert_eq!(result.profile.first().unwrap().distance_m, 0.0);
    assert!((result.profile.last().unwrap().distance_m - result.distance_m).abs() < 1e-6);
    assert!((0.0..360.0).contains(&result.initial_bearing_deg));
    assert!((0.0..360.0).contains(&result.final_bearing_deg));
    assert!(result.profile.iter().all(|sample| {
        [
            sample.distance_m,
            sample.lat,
            sample.lon,
            sample.terrain_elevation_m,
            sample.earth_bulge_m,
            sample.adjusted_terrain_m,
            sample.los_height_m,
            sample.fresnel_radius_m,
        ]
        .into_iter()
        .all(f64::is_finite)
    }));
    assert!(matches!(value["classification"], Value::String(_)));
}

#[test]
fn near_threshold_margin_marks_result_critical() {
    let mut request = request();
    let core = flat_core(&request);
    request.receiver_threshold_dbm = core.received_power_dbm - 2.5;
    let result = result_from_core(&request, core).unwrap();
    assert!((result.link_margin_db - 2.5).abs() < 1e-10);
    assert!(result.critical);
}

#[test]
fn core_config_keeps_endpoint_coordinates_exact() {
    let request = request();
    let config: CoreLinkAnalysisConfig = request_to_core_config(&request).unwrap();
    assert_eq!(
        config.transmitter,
        CoreGeoPoint {
            lat: request.tx.point.lat,
            lon: request.tx.point.lon
        }
    );
    assert_eq!(
        config.receiver,
        CoreGeoPoint {
            lat: request.rx.point.lat,
            lon: request.rx.point.lon
        }
    );
}

#[test]
fn preexisting_cancellation_stops_before_cache_access() {
    let cache_root = PathBuf::from(format!(
        "/tmp/hamheatmap-link-cancelled-{}",
        std::process::id()
    ));
    assert!(!cache_root.exists());
    let service = AppService::new(&cache_root);
    let cancelled = AtomicBool::new(true);
    let error = service.analyze_link(&request(), &cancelled).unwrap_err();
    assert!(error.contains("cancelled"));
    assert!(!cache_root.exists());
}
