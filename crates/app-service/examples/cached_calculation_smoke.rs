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
        return Err("calculation result is not a PNG data URL".into());
    }
    println!(
        "desktop service smoke passed: pixels={} mean_dbm={:.3} total_seconds={:.3} png_data_url_bytes={}",
        result.statistics.valid_pixel_count,
        result.statistics.mean_dbm,
        result.statistics.total_seconds,
        result.heatmap_png_data_url.len()
    );
    Ok(())
}
