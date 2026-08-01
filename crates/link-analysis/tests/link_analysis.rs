// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use geographiclib_rs::{DirectGeodesic, Geodesic, InverseGeodesic};
use hamheatmap_link_analysis::{
    EARTH_RADIUS_M, EFFECTIVE_EARTH_K_FACTOR, EFFECTIVE_EARTH_RADIUS_M, FlatTerrain, GeoPoint,
    GeometryClassification, LinkAnalysisConfig, LinkAnalysisError, LinkProfileSample,
    MAX_PROFILE_SAMPLE_SPACING_M, TerrainSource, UniformWater, WaterSource, analyze_link,
    classify_geometry, effective_earth_bulge_m, first_fresnel_radius_m, wavelength_m,
};
use hamheatmap_propagation::{
    ModelDefaults, Polarization, PredictionInputs, TerrainProfile, predict_p2p,
};

const TEST_DISTANCE_M: f64 = 10_000.0;

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected:.12}, got {actual:.12}, tolerance {tolerance}"
    );
}

fn points_at_distance(distance_m: f64) -> (GeoPoint, GeoPoint) {
    let transmitter = GeoPoint {
        lat: 30.0,
        lon: 110.0,
    };
    let geodesic = Geodesic::wgs84();
    let (lat, lon, _): (f64, f64, f64) =
        geodesic.direct(transmitter.lat, transmitter.lon, 90.0, distance_m);
    (transmitter, GeoPoint { lat, lon })
}

fn base_config(distance_m: f64) -> LinkAnalysisConfig {
    let (transmitter, receiver) = points_at_distance(distance_m);
    LinkAnalysisConfig {
        transmitter,
        receiver,
        frequency_mhz: 145.0,
        polarization: Polarization::Vertical,
        tx_height_m: 50.0,
        rx_height_m: 50.0,
        tx_power_dbm: 43.979_400_086_720_375,
        tx_gain_dbi: 6.0,
        rx_gain_dbi: -3.0,
    }
}

#[derive(Clone, Copy)]
struct RidgeTerrain {
    origin: GeoPoint,
    ridge_distance_m: f64,
    ridge_elevation_m: f32,
}

impl TerrainSource for RidgeTerrain {
    fn elevation_m(&self, lon: f64, lat: f64) -> Result<f32, String> {
        let geodesic = Geodesic::wgs84();
        let (distance_m, _, _, _): (f64, f64, f64, f64) =
            geodesic.inverse(self.origin.lat, self.origin.lon, lat, lon);
        if (distance_m - self.ridge_distance_m).abs() < 1.0 {
            Ok(self.ridge_elevation_m)
        } else {
            Ok(0.0)
        }
    }
}

struct MissingTerrain;

impl TerrainSource for MissingTerrain {
    fn elevation_m(&self, _lon: f64, _lat: f64) -> Result<f32, String> {
        Err("missing DEM tile".into())
    }
}

struct MissingWater;

impl WaterSource for MissingWater {
    fn is_water(&self, _lon: f64, _lat: f64) -> Result<bool, String> {
        Err("missing WBM tile".into())
    }
}

fn ridge_for_normalized_clearance(
    config: LinkAnalysisConfig,
    normalized_clearance: f64,
) -> RidgeTerrain {
    let geodesic = Geodesic::wgs84();
    let (distance_m, _, _, _): (f64, f64, f64, f64) = geodesic.inverse(
        config.transmitter.lat,
        config.transmitter.lon,
        config.receiver.lat,
        config.receiver.lon,
    );
    let midpoint_m = distance_m / 2.0;
    let wavelength_m = wavelength_m(config.frequency_mhz).unwrap();
    let radius_m = first_fresnel_radius_m(wavelength_m, midpoint_m, midpoint_m);
    let bulge_m = effective_earth_bulge_m(midpoint_m, distance_m, EFFECTIVE_EARTH_RADIUS_M);
    let line_m = config.tx_height_m;
    let terrain_m = line_m - normalized_clearance * radius_m - bulge_m;
    RidgeTerrain {
        origin: config.transmitter,
        ridge_distance_m: midpoint_m,
        ridge_elevation_m: terrain_m as f32,
    }
}

#[test]
fn flat_link_uses_wgs84_uniform_samples_and_raw_itm_profile() {
    let config = base_config(TEST_DISTANCE_M);
    let result = analyze_link(
        &FlatTerrain { elevation_m: 0.0 },
        &UniformWater { is_water: false },
        config,
    )
    .unwrap();
    let expected_intervals = (result.distance_m / MAX_PROFILE_SAMPLE_SPACING_M).ceil() as usize;
    assert_eq!(result.profile.len(), expected_intervals + 1);
    assert!(result.sample_spacing_m <= MAX_PROFILE_SAMPLE_SPACING_M);
    assert_close(
        result.sample_spacing_m * expected_intervals as f64,
        result.distance_m,
        1e-8,
    );
    assert_eq!(result.profile.first().unwrap().point, config.transmitter);
    assert_eq!(result.profile.last().unwrap().point, config.receiver);
    assert_close(
        result.profile.last().unwrap().distance_m,
        result.distance_m,
        1e-8,
    );
    assert_eq!(
        result.geometry.classification,
        GeometryClassification::DirectLineOfSight
    );

    let raw_profile =
        TerrainProfile::new(result.sample_spacing_m, vec![0.0; result.profile.len()]).unwrap();
    let mut inputs = PredictionInputs::land_water_v1(145.0, Polarization::Vertical);
    inputs.tx_height_m = config.tx_height_m;
    inputs.rx_height_m = config.rx_height_m;
    inputs.ground = ModelDefaults::LAND_WATER_V1.land;
    let expected_prediction = predict_p2p(&raw_profile, inputs).unwrap();
    assert_close(
        result.prediction.basic_transmission_loss_db,
        expected_prediction.basic_transmission_loss_db,
        1e-10,
    );
    assert!(
        result
            .profile
            .iter()
            .all(|sample| sample.terrain_elevation_m == 0.0)
    );
    assert!(
        result
            .profile
            .iter()
            .skip(1)
            .take(result.profile.len() - 2)
            .any(|sample| sample.adjusted_terrain_elevation_m > 0.0)
    );
}

#[test]
fn fresnel_radius_and_curvature_match_analytic_midpoint_values() {
    let result = analyze_link(
        &FlatTerrain { elevation_m: 0.0 },
        &UniformWater { is_water: false },
        base_config(TEST_DISTANCE_M),
    )
    .unwrap();
    let midpoint = &result.profile[result.profile.len() / 2];
    let d1_m = midpoint.distance_m;
    let d2_m = result.distance_m - d1_m;
    let expected_radius_m = (result.wavelength_m * d1_m * d2_m / result.distance_m).sqrt();
    assert_close(midpoint.fresnel_radius_m, expected_radius_m, 1e-10);
    let expected_bulge_m = EFFECTIVE_EARTH_RADIUS_M
        * (1.0 - (result.distance_m / (2.0 * EFFECTIVE_EARTH_RADIUS_M)).cos());
    assert_close(midpoint.earth_bulge_m, expected_bulge_m, 1e-4);
    assert_close(
        EFFECTIVE_EARTH_RADIUS_M,
        EARTH_RADIUS_M * EFFECTIVE_EARTH_K_FACTOR,
        0.0,
    );
    assert_eq!(result.profile.first().unwrap().earth_bulge_m, 0.0);
    assert_close(result.profile.last().unwrap().earth_bulge_m, 0.0, 1e-12);
}

#[test]
fn earth_bulge_short_path_matches_parabolic_approximation() {
    let total_m = 50_000.0;
    let along_m = 17_000.0;
    let exact = effective_earth_bulge_m(along_m, total_m, EFFECTIVE_EARTH_RADIUS_M);
    let approximate = along_m * (total_m - along_m) / (2.0 * EFFECTIVE_EARTH_RADIUS_M);
    assert_close(exact, approximate, 1e-4);
}

#[test]
fn geometry_classification_covers_clear_intruded_and_severe_ridges() {
    let config = base_config(TEST_DISTANCE_M);
    let clear = analyze_link(
        &FlatTerrain { elevation_m: 0.0 },
        &UniformWater { is_water: false },
        config,
    )
    .unwrap();
    assert_eq!(
        clear.geometry.classification,
        GeometryClassification::DirectLineOfSight
    );
    assert!(clear.geometry.geometric_line_of_sight);
    assert!(clear.geometry.sixty_percent_fresnel_clear);

    let intruded = analyze_link(
        &ridge_for_normalized_clearance(config, 0.59),
        &UniformWater { is_water: false },
        config,
    )
    .unwrap();
    assert_eq!(
        intruded.geometry.classification,
        GeometryClassification::FresnelAffected
    );
    assert!(intruded.geometry.geometric_line_of_sight);
    assert!(!intruded.geometry.sixty_percent_fresnel_clear);

    let blocked_within_fresnel = analyze_link(
        &ridge_for_normalized_clearance(config, -0.5),
        &UniformWater { is_water: false },
        config,
    )
    .unwrap();
    assert_eq!(
        blocked_within_fresnel.geometry.classification,
        GeometryClassification::FresnelAffected
    );
    assert!(!blocked_within_fresnel.geometry.geometric_line_of_sight);

    let severe = analyze_link(
        &ridge_for_normalized_clearance(config, -1.05),
        &UniformWater { is_water: false },
        config,
    )
    .unwrap();
    assert_eq!(
        severe.geometry.classification,
        GeometryClassification::SeverelyObstructed
    );
}

fn synthetic_profile(
    normalized_clearance: f64,
    endpoint_clearance_m: f64,
) -> Vec<LinkProfileSample> {
    [0.0, 5_000.0, 10_000.0]
        .into_iter()
        .enumerate()
        .map(|(index, distance_m)| {
            let interior = index == 1;
            LinkProfileSample {
                point: GeoPoint { lat: 0.0, lon: 0.0 },
                distance_m,
                terrain_elevation_m: 0.0,
                earth_bulge_m: 0.0,
                adjusted_terrain_elevation_m: 0.0,
                line_of_sight_elevation_m: if interior {
                    normalized_clearance * 10.0
                } else {
                    endpoint_clearance_m
                },
                fresnel_radius_m: if interior { 10.0 } else { 0.0 },
                line_of_sight_clearance_m: if interior {
                    normalized_clearance * 10.0
                } else {
                    endpoint_clearance_m
                },
                normalized_fresnel_clearance: interior.then_some(normalized_clearance),
            }
        })
        .collect()
}

#[test]
fn classification_thresholds_are_inclusive_and_endpoints_do_not_limit_clearance() {
    let direct = classify_geometry(&synthetic_profile(0.6, 0.5)).unwrap();
    assert_eq!(
        direct.classification,
        GeometryClassification::DirectLineOfSight
    );
    assert_close(direct.minimum_line_of_sight_clearance_m, 6.0, 0.0);
    let affected = classify_geometry(&synthetic_profile(-1.0, 0.5)).unwrap();
    assert_eq!(
        affected.classification,
        GeometryClassification::FresnelAffected
    );
    let severe = classify_geometry(&synthetic_profile(-1.000_001, 0.5)).unwrap();
    assert_eq!(
        severe.classification,
        GeometryClassification::SeverelyObstructed
    );
}

#[test]
fn higher_frequency_shrinks_the_first_fresnel_zone() {
    let mut vhf = base_config(TEST_DISTANCE_M);
    vhf.frequency_mhz = 145.0;
    let mut uhf = vhf;
    uhf.frequency_mhz = 435.0;
    let terrain = FlatTerrain { elevation_m: 0.0 };
    let water = UniformWater { is_water: false };
    let vhf_result = analyze_link(&terrain, &water, vhf).unwrap();
    let uhf_result = analyze_link(&terrain, &water, uhf).unwrap();
    let middle = vhf_result.profile.len() / 2;
    assert!(
        vhf_result.profile[middle].fresnel_radius_m > uhf_result.profile[middle].fresnel_radius_m
    );
}

#[test]
fn antenna_gains_change_received_power_but_not_geometry() {
    let config = base_config(TEST_DISTANCE_M);
    let terrain = FlatTerrain { elevation_m: 0.0 };
    let water = UniformWater { is_water: false };
    let baseline = analyze_link(&terrain, &water, config).unwrap();
    let mut higher_gain = config;
    higher_gain.tx_gain_dbi += 4.0;
    higher_gain.rx_gain_dbi += 6.0;
    let changed = analyze_link(&terrain, &water, higher_gain).unwrap();
    assert_eq!(baseline.geometry, changed.geometry);
    assert_eq!(baseline.profile, changed.profile);
    assert_eq!(baseline.prediction, changed.prediction);
    assert_close(
        changed.received_power_dbm - baseline.received_power_dbm,
        10.0,
        1e-12,
    );
}

#[test]
fn water_fraction_uses_every_raw_profile_sample() {
    let config = base_config(TEST_DISTANCE_M);
    let result = analyze_link(
        &FlatTerrain { elevation_m: 0.0 },
        &UniformWater { is_water: true },
        config,
    )
    .unwrap();
    assert_eq!(result.path_water_fraction, 1.0);
}

#[test]
fn missing_or_non_finite_data_blocks_analysis() {
    let config = base_config(TEST_DISTANCE_M);
    let terrain_error =
        analyze_link(&MissingTerrain, &UniformWater { is_water: false }, config).unwrap_err();
    assert!(matches!(terrain_error, LinkAnalysisError::Terrain { .. }));
    let water_error =
        analyze_link(&FlatTerrain { elevation_m: 0.0 }, &MissingWater, config).unwrap_err();
    assert!(matches!(water_error, LinkAnalysisError::WaterMask { .. }));
    let non_finite = analyze_link(
        &FlatTerrain {
            elevation_m: f32::NAN,
        },
        &UniformWater { is_water: false },
        config,
    )
    .unwrap_err();
    assert!(matches!(non_finite, LinkAnalysisError::Terrain { .. }));
}

#[test]
fn itm_distance_bounds_are_enforced_before_sampling() {
    let water = UniformWater { is_water: false };
    for distance_m in [999.0, 200_001.0] {
        let error = analyze_link(
            &FlatTerrain { elevation_m: 0.0 },
            &water,
            base_config(distance_m),
        )
        .unwrap_err();
        assert!(matches!(error, LinkAnalysisError::InvalidInput(_)));
    }
    let mut identical = base_config(TEST_DISTANCE_M);
    identical.receiver = identical.transmitter;
    let error = analyze_link(&FlatTerrain { elevation_m: 0.0 }, &water, identical).unwrap_err();
    assert!(matches!(error, LinkAnalysisError::InvalidInput(_)));
}

#[test]
fn endpoint_distance_is_wgs84_geodesic() {
    let config = base_config(TEST_DISTANCE_M);
    let result = analyze_link(
        &FlatTerrain { elevation_m: 0.0 },
        &UniformWater { is_water: false },
        config,
    )
    .unwrap();
    assert_close(result.distance_m, TEST_DISTANCE_M, 1e-6);
}
