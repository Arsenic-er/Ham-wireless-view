// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::fmt::Write;
use std::sync::atomic::AtomicBool;

use hamheatmap_cache::{CacheStore, GeoPoint as CacheGeoPoint, plan_glo90_region};
use hamheatmap_coverage::CoverageGrid;
use hamheatmap_terrain::{DemTileSet, WaterTileSet};
use serde::Serialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use super::*;

const LINEAR_DELTA_TOLERANCE_DB: f64 = 0.000_05;
const MATERIAL_CHANGE_DB: f64 = 0.01;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeltaSummary {
    compared_pixel_count: usize,
    changed_pixel_count: usize,
    improved_pixel_count: usize,
    worsened_pixel_count: usize,
    minimum_delta_db: f64,
    maximum_delta_db: f64,
    mean_delta_db: f64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderedHashes {
    heatmap_png_sha256: String,
    map_overlay_png_sha256: String,
}

fn baseline_request() -> CalculationRequest {
    CalculationRequest {
        center: MapPoint {
            lat: 30.5,
            lon: 103.5,
        },
        band: Band::Vhf144,
        frequency_mhz: 145.0,
        power_value: 25.0,
        power_unit: PowerUnit::Watt,
        tx_gain_value: 6.0,
        tx_gain_unit: GainUnit::Dbi,
        tx_height_m: 20.0,
        tx_ground_elevation_override_m: None,
        rx_gain_value: -3.0,
        rx_gain_unit: GainUnit::Dbi,
        rx_height_m: 1.5,
        polarization: PolarizationChoice::Vertical,
    }
}

fn calculate(dem: &DemTileSet, water: &WaterTileSet, request: &CalculationRequest) -> CoverageGrid {
    let config = request_to_config(request).expect("real sensitivity request must be valid");
    compute_coverage_with_control(dem, water, config, &AtomicBool::new(false), |_| {})
        .expect("real sensitivity calculation must succeed")
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn grid_sha256(grid: &CoverageGrid) -> String {
    let mut digest = Sha256::new();
    for value in grid.values_dbm() {
        digest.update(value.to_bits().to_le_bytes());
    }
    let digest = digest.finalize();
    hex_digest(digest.as_ref())
}

fn png_sha256(grid: &CoverageGrid) -> String {
    let png = grid
        .encode_png()
        .expect("real sensitivity heatmap PNG must encode");
    let digest = Sha256::digest(png);
    hex_digest(digest.as_ref())
}

fn rendered_hashes(grid: &CoverageGrid) -> RenderedHashes {
    let overlay = grid
        .encode_map_overlay()
        .expect("real sensitivity map overlay PNG must encode");
    let overlay_digest = Sha256::digest(overlay.png);
    RenderedHashes {
        heatmap_png_sha256: png_sha256(grid),
        map_overlay_png_sha256: hex_digest(overlay_digest.as_ref()),
    }
}

fn assert_rendered_change(
    label: &str,
    candidate: &CoverageGrid,
    baseline: &RenderedHashes,
) -> RenderedHashes {
    let candidate = rendered_hashes(candidate);
    assert_ne!(
        candidate.heatmap_png_sha256, baseline.heatmap_png_sha256,
        "{label} did not change the report heatmap PNG"
    );
    assert_ne!(
        candidate.map_overlay_png_sha256, baseline.map_overlay_png_sha256,
        "{label} did not change the EPSG:3857 map overlay PNG"
    );
    candidate
}

fn delta_summary(candidate: &CoverageGrid, baseline: &CoverageGrid) -> DeltaSummary {
    assert_eq!(candidate.values_dbm().len(), baseline.values_dbm().len());
    let mut summary = DeltaSummary {
        compared_pixel_count: 0,
        changed_pixel_count: 0,
        improved_pixel_count: 0,
        worsened_pixel_count: 0,
        minimum_delta_db: f64::INFINITY,
        maximum_delta_db: f64::NEG_INFINITY,
        mean_delta_db: 0.0,
    };
    let mut sum_delta_db = 0.0_f64;
    for (candidate_value, baseline_value) in
        candidate.values_dbm().iter().zip(baseline.values_dbm())
    {
        assert_eq!(
            candidate_value.is_finite(),
            baseline_value.is_finite(),
            "parameter change altered the fixed 200 km valid-pixel mask"
        );
        if !candidate_value.is_finite() {
            continue;
        }
        let delta = f64::from(*candidate_value) - f64::from(*baseline_value);
        summary.compared_pixel_count += 1;
        sum_delta_db += delta;
        summary.minimum_delta_db = summary.minimum_delta_db.min(delta);
        summary.maximum_delta_db = summary.maximum_delta_db.max(delta);
        if delta.abs() > MATERIAL_CHANGE_DB {
            summary.changed_pixel_count += 1;
        }
        if delta > MATERIAL_CHANGE_DB {
            summary.improved_pixel_count += 1;
        } else if delta < -MATERIAL_CHANGE_DB {
            summary.worsened_pixel_count += 1;
        }
    }
    assert!(
        summary.compared_pixel_count > 0,
        "matrix compared no valid pixels"
    );
    summary.mean_delta_db = sum_delta_db / summary.compared_pixel_count as f64;
    summary
}

fn assert_uniform_delta(
    label: &str,
    candidate: &CoverageGrid,
    baseline: &CoverageGrid,
    expected_delta_db: f64,
) -> DeltaSummary {
    let summary = delta_summary(candidate, baseline);
    assert!(
        (summary.minimum_delta_db - expected_delta_db).abs() <= LINEAR_DELTA_TOLERANCE_DB
            && (summary.maximum_delta_db - expected_delta_db).abs() <= LINEAR_DELTA_TOLERANCE_DB,
        "{label} was not a uniform {expected_delta_db} dB shift: {summary:?}"
    );
    assert_eq!(
        summary.changed_pixel_count, summary.compared_pixel_count,
        "{label} did not change every valid pixel"
    );
    summary
}

fn assert_spatial_change(label: &str, summary: DeltaSummary) {
    assert!(
        summary.changed_pixel_count > summary.compared_pixel_count / 100,
        "{label} changed too few pixels to establish a spatial effect: {summary:?}"
    );
    assert!(
        summary.maximum_delta_db - summary.minimum_delta_db > 0.1,
        "{label} only produced an effectively uniform offset: {summary:?}"
    );
}

#[test]
#[ignore = "requires the real Chengdu DEM/WBM cache; run scripts/parameter-sensitivity-smoke.sh"]
fn real_chengdu_parameter_sensitivity_matrix() {
    let cache_root = env::var_os("HAMHEATMAP_REAL_CACHE_ROOT")
        .expect("HAMHEATMAP_REAL_CACHE_ROOT must point to the real validation cache");
    let baseline_request = baseline_request();
    let plan = plan_glo90_region(CacheGeoPoint {
        lat: baseline_request.center.lat,
        lon: baseline_request.center.lon,
    })
    .expect("Chengdu region plan must be valid");
    let (dem_paths, water_paths) = {
        let mut store = CacheStore::open(cache_root).expect("real cache must open exclusively");
        let dem_paths = store
            .ready_paths_for_region(&plan)
            .expect("all planned Chengdu DEM assets must be ready and valid");
        let water_paths = store
            .ready_water_paths_for_region(&plan)
            .expect("all planned Chengdu WBM assets must be ready and valid");
        (dem_paths, water_paths)
    };
    let dem = DemTileSet::open_paths(dem_paths).expect("real DEM tiles must decode");
    let water = WaterTileSet::open_paths(water_paths).expect("real WBM tiles must decode");

    let baseline = calculate(&dem, &water, &baseline_request);
    let repeated = calculate(&dem, &water, &baseline_request);
    assert_eq!(
        baseline
            .values_dbm()
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        repeated
            .values_dbm()
            .iter()
            .map(|value| value.to_bits())
            .collect::<Vec<_>>(),
        "identical real inputs must produce bit-identical dBm grids"
    );
    let baseline_rendered = rendered_hashes(&baseline);
    let repeated_rendered = rendered_hashes(&repeated);
    assert_eq!(
        baseline_rendered, repeated_rendered,
        "identical real inputs must produce byte-identical heatmap and map-overlay PNGs"
    );

    let mut request = baseline_request.clone();
    request.power_value *= 10.0;
    let power = calculate(&dem, &water, &request);
    let power_rendered = assert_rendered_change("10x transmit power", &power, &baseline_rendered);
    let power_delta = assert_uniform_delta("10x transmit power", &power, &baseline, 10.0);

    let mut request = baseline_request.clone();
    request.tx_gain_value += 6.0;
    let tx_gain = calculate(&dem, &water, &request);
    let tx_gain_rendered =
        assert_rendered_change("+6 dB transmitter gain", &tx_gain, &baseline_rendered);
    let tx_gain_delta = assert_uniform_delta("+6 dB transmitter gain", &tx_gain, &baseline, 6.0);

    let mut request = baseline_request.clone();
    request.rx_gain_value += 6.0;
    let rx_gain = calculate(&dem, &water, &request);
    let rx_gain_rendered =
        assert_rendered_change("+6 dB receiver gain", &rx_gain, &baseline_rendered);
    let rx_gain_delta = assert_uniform_delta("+6 dB receiver gain", &rx_gain, &baseline, 6.0);

    let mut request = baseline_request.clone();
    request.band = Band::Uhf430;
    request.frequency_mhz = 435.0;
    let frequency = calculate(&dem, &water, &request);
    let frequency_rendered =
        assert_rendered_change("145 to 435 MHz", &frequency, &baseline_rendered);
    let frequency_delta = delta_summary(&frequency, &baseline);
    assert_spatial_change("145 to 435 MHz", frequency_delta);
    assert!(
        frequency_delta.mean_delta_db < -1.0,
        "435 MHz should be materially weaker on average: {frequency_delta:?}"
    );

    let mut request = baseline_request.clone();
    request.tx_height_m = 80.0;
    let tx_height = calculate(&dem, &water, &request);
    let tx_height_rendered =
        assert_rendered_change("20 to 80 m transmitter AGL", &tx_height, &baseline_rendered);
    let tx_height_delta = delta_summary(&tx_height, &baseline);
    assert_spatial_change("20 to 80 m transmitter AGL", tx_height_delta);
    assert!(
        tx_height_delta.improved_pixel_count > 0,
        "higher transmitter must improve at least part of the region"
    );

    let mut request = baseline_request.clone();
    request.rx_height_m = 10.0;
    let rx_height = calculate(&dem, &water, &request);
    let rx_height_rendered =
        assert_rendered_change("1.5 to 10 m receiver AGL", &rx_height, &baseline_rendered);
    let rx_height_delta = delta_summary(&rx_height, &baseline);
    assert_spatial_change("1.5 to 10 m receiver AGL", rx_height_delta);
    assert!(
        rx_height_delta.improved_pixel_count > 0,
        "higher receiver must improve at least part of the region"
    );

    let mut request = baseline_request.clone();
    request.polarization = PolarizationChoice::Horizontal;
    let polarization = calculate(&dem, &water, &request);
    let polarization_rendered = assert_rendered_change(
        "vertical to horizontal polarization",
        &polarization,
        &baseline_rendered,
    );
    let polarization_delta = delta_summary(&polarization, &baseline);
    assert_spatial_change("vertical to horizontal polarization", polarization_delta);

    let report = json!({
        "schemaVersion": 1,
        "modelVersion": MODEL_DEFAULTS_VERSION,
        "center": baseline_request.center,
        "validPixelCount": baseline.statistics.valid_pixel_count,
        "baselineGridSha256": grid_sha256(&baseline),
        "baselineRenderedSha256": baseline_rendered,
        "repeatGridSha256": grid_sha256(&repeated),
        "repeatRenderedSha256": repeated_rendered,
        "power250WMinus25W": power_delta,
        "txGain12Minus6Dbi": tx_gain_delta,
        "rxGain3MinusNegative3Dbi": rx_gain_delta,
        "frequency435Minus145Mhz": frequency_delta,
        "txHeight80Minus20M": tx_height_delta,
        "rxHeight10Minus1_5M": rx_height_delta,
        "horizontalMinusVertical": polarization_delta,
        "scenarioGridSha256": {
            "power250W": grid_sha256(&power),
            "txGain12Dbi": grid_sha256(&tx_gain),
            "rxGain3Dbi": grid_sha256(&rx_gain),
            "frequency435Mhz": grid_sha256(&frequency),
            "txHeight80M": grid_sha256(&tx_height),
            "rxHeight10M": grid_sha256(&rx_height),
            "horizontal": grid_sha256(&polarization),
        },
        "scenarioRenderedSha256": {
            "power250W": power_rendered,
            "txGain12Dbi": tx_gain_rendered,
            "rxGain3Dbi": rx_gain_rendered,
            "frequency435Mhz": frequency_rendered,
            "txHeight80M": tx_height_rendered,
            "rxHeight10M": rx_height_rendered,
            "horizontal": polarization_rendered,
        }
    });
    println!(
        "PARAMETER_SENSITIVITY_JSON={}",
        serde_json::to_string(&report).expect("sensitivity report must serialize")
    );
}
