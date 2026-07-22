use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use hamheatmap_app_service::{AppService, MapPoint};

fn main() -> Result<(), String> {
    let cache_root = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("data"));
    let service = AppService::new(cache_root);
    let point = MapPoint {
        lat: 30.5,
        lon: 103.5,
    };

    let estimate = service.estimate_download(point)?;
    if estimate.required_asset_count != 0 || estimate.additional_download_bytes != 0 {
        return Err(format!(
            "expected the validated Chengdu cache to be ready, got {} missing assets and {} bytes",
            estimate.required_asset_count, estimate.additional_download_bytes
        ));
    }
    if estimate.ready_asset_count != estimate.tile_count * 2 {
        return Err("ready asset count does not match paired DEM/WBM tiles".into());
    }

    let cancelled = AtomicBool::new(false);
    let result = service.download_region(point, &cancelled, |_| {})?;
    if !result.inspection.data_ready
        || result.prepared_asset_count != 0
        || result.downloaded_bytes != 0
    {
        return Err("zero-download ready transition did not produce a ready inspection".into());
    }
    let overview = service.cache_overview()?;
    if overview.regions.is_empty() {
        return Err("cache overview omitted the prepared Chengdu region".into());
    }

    println!(
        "download workflow smoke passed: regions={} ready_assets={} total_bytes={}",
        overview.regions.len(),
        estimate.ready_asset_count,
        overview.usage.total_bytes
    );
    Ok(())
}
