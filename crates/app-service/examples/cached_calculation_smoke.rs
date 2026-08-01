use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use std::env;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use hamheatmap_app_service::{
    AppService, Band, CalculationRequest, GainUnit, MapPoint, PolarizationChoice, PowerUnit,
};

fn main() -> Result<(), String> {
    let cache_root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"));
    let service = AppService::new(cache_root);
    let point = MapPoint {
        lat: 30.5,
        lon: 103.5,
    };
    let inspection = service.inspect_point(point)?;
    if !inspection.data_ready {
        return Err(format!(
            "cached smoke point is missing {} assets",
            inspection.missing_asset_count
        ));
    }
    let cancelled = AtomicBool::new(false);
    let result = service.calculate(
        &CalculationRequest {
            center: point,
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
        },
        &cancelled,
        |_| {},
    )?;
    if !result
        .heatmap_png_data_url
        .starts_with("data:image/png;base64,iVBOR")
    {
        return Err("calculation heatmap is not a PNG data URL".into());
    }
    if !result
        .map_overlay_png_data_url
        .starts_with("data:image/png;base64,iVBOR")
    {
        return Err("calculation map overlay is not a PNG data URL".into());
    }
    if result.map_overlay_filter_encoding != "u8-dbm-floor-v1" {
        return Err("calculation map overlay filter encoding is unsupported".into());
    }
    let filter_bins = BASE64_STANDARD
        .decode(&result.map_overlay_filter_base64)
        .map_err(|error| format!("calculation map overlay filter is invalid base64: {error}"))?;
    let expected_bins = result.map_overlay_width * result.map_overlay_height;
    if filter_bins.len() != expected_bins {
        return Err(format!(
            "calculation map overlay filter has {} bins, expected {expected_bins}",
            filter_bins.len()
        ));
    }
    if let Some((index, value)) = filter_bins
        .iter()
        .copied()
        .enumerate()
        .find(|(_, value)| *value > 81)
    {
        return Err(format!(
            "calculation map overlay filter bin {index} is {value}, expected 0..81"
        ));
    }
    println!(
        "desktop service smoke passed: pixels={} mean_dbm={:.3} total_seconds={:.3} heatmap_url_bytes={} overlay_url_bytes={} filter_bytes={}",
        result.statistics.valid_pixel_count,
        result.statistics.mean_dbm,
        result.statistics.total_seconds,
        result.heatmap_png_data_url.len(),
        result.map_overlay_png_data_url.len(),
        filter_bins.len()
    );
    Ok(())
}
