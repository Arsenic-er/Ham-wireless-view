use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::str::FromStr;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use hamheatmap_cache::{
    CacheState, CacheStore, GeoPoint as CacheGeoPoint, Glo90DownloadService, TOTAL_CACHE_CAP_BYTES,
    execute_download_plan, glo90_assets, plan_glo90_region,
};
use hamheatmap_coverage::{
    CoverageConfig, CoverageDelta, CoverageGrid, FlatTerrain, GeoPoint, ITM_WARNING_BIT_COUNT,
    MODEL_DEFAULTS_VERSION, UniformWater, compute_coverage,
};
use hamheatmap_terrain::{DemTileSet, WaterTileSet};

const ITM_WARNING_NAMES: [&str; ITM_WARNING_BIT_COUNT] = [
    "tx_terminal_height",
    "rx_terminal_height",
    "frequency",
    "path_distance_near_upper_limit",
    "path_distance_large",
    "path_distance_near_lower_limit",
    "path_distance_small",
    "tx_horizon_angle",
    "rx_horizon_angle",
    "tx_horizon_distance_short",
    "rx_horizon_distance_short",
    "tx_horizon_distance_long",
    "rx_horizon_distance_long",
    "extreme_variabilities",
    "surface_refractivity",
];

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("validate") => validate(&args[2..]),
        Some("cache") => cache(&args[2..]),
        Some("help" | "--help" | "-h") | None => {
            print_help();
            Ok(())
        }
        Some(other) => Err(format!("unknown command {other:?}; run with --help")),
    }
}

fn print_help() {
    println!(
        "HamHeatmap minimum-viability CLI\n\
         \n\
         Commands:\n\
           validate [--dem-dir DIR --water-dir DIR] [--output-dir DIR] [--lat N] [--lon E]\n\
                    [--threads N] [--cache-root DIR]\n\
           cache plan [--cache-root DIR] [--lat N] [--lon E]\n\
           cache prepare [--cache-root DIR] [--lat N] [--lon E] [--yes]\n\
           cache status [--cache-root DIR]\n\
           cache cleanup [--cache-root DIR]\n\
           cache adopt-dem --source DIR [--cache-root DIR] [--lat N] [--lon E] [--yes]\n\
           cache delete-region --region-id ID [--cache-root DIR]\n\
         \n\
         The validation always computes the fixed 401x401, 1 km, 200 km\n\
         coverage grid. It emits transparent diagnostic heatmaps without a\n\
         basemap or administrative boundaries."
    );
}

fn cache(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("plan") => cache_plan(&args[1..]),
        Some("prepare") => cache_prepare(&args[1..]),
        Some("status") => cache_status(&args[1..]),
        Some("cleanup") => cache_cleanup(&args[1..]),
        Some("adopt-dem") => cache_adopt_dem(&args[1..]),
        Some("delete-region") => cache_delete_region(&args[1..]),
        Some(other) => Err(format!("unknown cache command {other:?}; run with --help")),
        None => Err("cache requires a command; run with --help".into()),
    }
}

fn cache_plan(args: &[String]) -> Result<(), String> {
    let cache_root = cache_root(args)?;
    let center = cache_center(args)?;
    let plan = plan_glo90_region(center).map_err(|error| error.to_string())?;
    let mut store = CacheStore::open(&cache_root).map_err(|error| error.to_string())?;
    store
        .upsert_region(&plan)
        .map_err(|error| error.to_string())?;
    let ready_keys: std::collections::HashSet<_> = store
        .list_assets()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|asset| asset.state == CacheState::Ready)
        .map(|asset| asset.asset_key)
        .collect();
    let ready_descriptors: Vec<_> = plan
        .tiles
        .iter()
        .filter_map(|tile| glo90_assets(*tile).ok())
        .flatten()
        .filter(|asset| ready_keys.contains(&asset.asset_key))
        .collect();
    let ready_dem_count = ready_descriptors
        .iter()
        .filter(|asset| asset.kind == hamheatmap_cache::CacheKind::Dem)
        .count();
    let ready_water_count = ready_descriptors
        .iter()
        .filter(|asset| asset.kind == hamheatmap_cache::CacheKind::Water)
        .count();
    println!("region_id={}", plan.region_id);
    println!(
        "bounds=south:{:.6},west:{:.6},north:{:.6},east:{:.6}",
        plan.bounds.south, plan.bounds.west, plan.bounds.north, plan.bounds.east
    );
    println!(
        "tiles={} dem_ready={} dem_missing={} water_ready={} water_missing={}",
        plan.tiles.len(),
        ready_dem_count,
        plan.tiles.len() - ready_dem_count,
        ready_water_count,
        plan.tiles.len() - ready_water_count
    );
    println!(
        "tile_ids={}",
        plan.tiles
            .iter()
            .map(|tile| tile.filename().trim_end_matches(".tif").to_owned())
            .collect::<Vec<_>>()
            .join(",")
    );
    Ok(())
}

fn cache_prepare(args: &[String]) -> Result<(), String> {
    let cache_root = cache_root(args)?;
    let center = cache_center(args)?;
    let plan = plan_glo90_region(center).map_err(|error| error.to_string())?;
    let mut store = CacheStore::open(&cache_root).map_err(|error| error.to_string())?;
    let service = Glo90DownloadService::new();
    progress("probing pinned GLO-90 HTTPS assets")?;
    let download_plan = service
        .probe_region(&mut store, plan)
        .map_err(|error| error.to_string())?;
    println!("region_id={}", download_plan.region.region_id);
    println!(
        "ready={} missing={} generated_ocean_assets={} additional_download_bytes={} cap_bytes={}",
        download_plan.ready_asset_count,
        download_plan.assets.len(),
        download_plan.generated_asset_count,
        download_plan.additional_download_bytes,
        TOTAL_CACHE_CAP_BYTES
    );
    if download_plan.assets.is_empty() {
        println!("region is already fully cached and verified");
        return Ok(());
    }
    if !flag(args, "--yes") {
        println!("download not started; review the size and rerun with --yes to confirm");
        return Ok(());
    }

    let cancelled = AtomicBool::new(false);
    let mut last_percent = None;
    execute_download_plan(
        &service,
        &mut store,
        &download_plan,
        &cancelled,
        |progress| {
            let percent = progress
                .total_downloaded_bytes
                .saturating_mul(100)
                .checked_div(progress.total_expected_bytes)
                .unwrap_or(100);
            if last_percent != Some(percent) && (percent % 5 == 0 || percent == 100) {
                println!(
                    "download {}% asset {}/{} {}",
                    percent,
                    progress.asset_index + 1,
                    progress.asset_count,
                    progress.asset_key
                );
                last_percent = Some(percent);
            }
        },
    )
    .map_err(|error| error.to_string())?;
    let ready_dem_paths = store
        .ready_paths_for_region(&download_plan.region)
        .map_err(|error| error.to_string())?;
    let ready_water_paths = store
        .ready_water_paths_for_region(&download_plan.region)
        .map_err(|error| error.to_string())?;
    println!(
        "ready: region_id={} verified_dem_tiles={} verified_water_tiles={}",
        download_plan.region.region_id,
        ready_dem_paths.len(),
        ready_water_paths.len()
    );
    Ok(())
}

fn cache_status(args: &[String]) -> Result<(), String> {
    let cache_root = cache_root(args)?;
    let store = CacheStore::open(&cache_root).map_err(|error| error.to_string())?;
    let usage = store.usage().map_err(|error| error.to_string())?;
    let assets = store.list_assets().map_err(|error| error.to_string())?;
    println!("cache_root={}", cache_root.display());
    println!(
        "total_bytes={} remaining_bytes={} cap_bytes={}",
        usage.total_bytes,
        usage.remaining_bytes(),
        usage.cap_bytes
    );
    println!(
        "basemap={} dem={} water={} calculation={} partial={} metadata_unindexed={}",
        usage.basemap_bytes,
        usage.dem_bytes,
        usage.water_bytes,
        usage.calculation_bytes,
        usage.partial_bytes,
        usage.metadata_and_unindexed_bytes
    );
    let ready = assets
        .iter()
        .filter(|asset| asset.state == CacheState::Ready)
        .count();
    let downloading = assets
        .iter()
        .filter(|asset| asset.state == CacheState::Downloading)
        .count();
    let corrupt = assets
        .iter()
        .filter(|asset| asset.state == CacheState::Corrupt)
        .count();
    println!(
        "assets={} ready={} downloading={} corrupt={}",
        assets.len(),
        ready,
        downloading,
        corrupt
    );
    Ok(())
}

fn cache_cleanup(args: &[String]) -> Result<(), String> {
    let cache_root = cache_root(args)?;
    let mut store = CacheStore::open(&cache_root).map_err(|error| error.to_string())?;
    store.reconcile().map_err(|error| error.to_string())?;
    let usage = store.usage().map_err(|error| error.to_string())?;
    println!(
        "cache reconciled: total_bytes={} cap_bytes={}",
        usage.total_bytes, usage.cap_bytes
    );
    Ok(())
}

fn cache_adopt_dem(args: &[String]) -> Result<(), String> {
    let cache_root = cache_root(args)?;
    let source = PathBuf::from(required_option::<String>(args, "--source")?);
    let center = cache_center(args)?;
    let plan = plan_glo90_region(center).map_err(|error| error.to_string())?;
    println!(
        "adoption plan: region_id={} tiles={} source={} cache_root={}",
        plan.region_id,
        plan.tiles.len(),
        source.display(),
        cache_root.display()
    );
    if !flag(args, "--yes") {
        println!("files were not moved; rerun with --yes after reviewing the paths");
        return Ok(());
    }
    let mut store = CacheStore::open(&cache_root).map_err(|error| error.to_string())?;
    let ready_paths = store
        .adopt_glo90_region(&plan, &source)
        .map_err(|error| error.to_string())?;
    let verified_paths = store
        .ready_paths_for_region(&plan)
        .map_err(|error| error.to_string())?;
    if ready_paths.len() != verified_paths.len() {
        return Err("adopted and verified DEM tile counts differ".into());
    }
    println!(
        "adopted and verified {} DEM tiles for {}",
        verified_paths.len(),
        plan.region_id
    );
    Ok(())
}

fn cache_delete_region(args: &[String]) -> Result<(), String> {
    let cache_root = cache_root(args)?;
    let region_id = required_option::<String>(args, "--region-id")?;
    let mut store = CacheStore::open(&cache_root).map_err(|error| error.to_string())?;
    let result = store
        .delete_region(&region_id)
        .map_err(|error| error.to_string())?;
    println!(
        "deleted region_id={} assets={} freed_bytes={}",
        region_id, result.deleted_asset_count, result.freed_bytes
    );
    Ok(())
}

fn cache_root(args: &[String]) -> Result<PathBuf, String> {
    Ok(PathBuf::from(option(
        args,
        "--cache-root",
        "data".to_string(),
    )?))
}

fn cache_center(args: &[String]) -> Result<CacheGeoPoint, String> {
    Ok(CacheGeoPoint {
        lat: option(args, "--lat", 30.5_f64)?,
        lon: option(args, "--lon", 103.5_f64)?,
    })
}

fn validate(args: &[String]) -> Result<(), String> {
    let dem_dir = optional_option::<String>(args, "--dem-dir")?.map(PathBuf::from);
    let water_dir = optional_option::<String>(args, "--water-dir")?.map(PathBuf::from);
    let explicit_cache_root = optional_option::<String>(args, "--cache-root")?.map(PathBuf::from);
    if dem_dir.is_some() != water_dir.is_some() {
        return Err("--dem-dir and --water-dir must be provided together".into());
    }
    if (dem_dir.is_some() || water_dir.is_some()) && explicit_cache_root.is_some() {
        return Err("--dem-dir/--water-dir and --cache-root are mutually exclusive".into());
    }
    let cache_root = if dem_dir.is_none() {
        Some(explicit_cache_root.unwrap_or_else(|| PathBuf::from("data")))
    } else {
        None
    };
    let output_dir = PathBuf::from(option(args, "--output-dir", "reports/mvp".to_string())?);
    let center = GeoPoint {
        lat: option(args, "--lat", 30.5_f64)?,
        lon: option(args, "--lon", 103.5_f64)?,
    };
    let threads = option(
        args,
        "--threads",
        std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
    )?;

    let mut cache_guard = None;
    let load_started = Instant::now();
    let (dem, water) = if let Some(cache_root) = cache_root {
        progress(&format!(
            "loading planned DEM and water-mask tiles from cache {}",
            cache_root.display()
        ))?;
        let plan = plan_glo90_region(CacheGeoPoint {
            lat: center.lat,
            lon: center.lon,
        })
        .map_err(|error| error.to_string())?;
        let mut store = CacheStore::open(cache_root).map_err(|error| error.to_string())?;
        store
            .upsert_region(&plan)
            .map_err(|error| error.to_string())?;
        let dem_paths = store.ready_paths_for_region(&plan).map_err(|error| {
            format!(
                "{error}; run `cache prepare --lat {} --lon {}` while online",
                center.lat, center.lon
            )
        })?;
        let water_paths = store.ready_water_paths_for_region(&plan).map_err(|error| {
            format!(
                "{error}; run `cache prepare --lat {} --lon {}` while online",
                center.lat, center.lon
            )
        })?;
        store
            .set_active_region(Some(&plan.region_id))
            .map_err(|error| error.to_string())?;
        let dem = DemTileSet::open_paths(dem_paths).map_err(|error| error.to_string())?;
        let water = WaterTileSet::open_paths(water_paths).map_err(|error| error.to_string())?;
        cache_guard = Some(store);
        (dem, water)
    } else {
        let dem_dir = dem_dir.ok_or_else(|| "internal DEM source selection error".to_string())?;
        let water_dir =
            water_dir.ok_or_else(|| "internal water source selection error".to_string())?;
        progress(&format!(
            "loading DEM tiles from {} and water masks from {}",
            dem_dir.display(),
            water_dir.display()
        ))?;
        (
            DemTileSet::open_directory(&dem_dir).map_err(|error| error.to_string())?,
            WaterTileSet::open_directory(&water_dir).map_err(|error| error.to_string())?,
        )
    };
    let data_load_seconds = load_started.elapsed().as_secs_f64();
    let center_elevation_m = dem
        .sample_bilinear(center.lon, center.lat)
        .map_err(|error| error.to_string())?;
    println!(
        "loaded {} DEM and {} water tiles in {data_load_seconds:.3}s; center elevation {center_elevation_m:.2} m; water pixels {}",
        dem.tile_count(),
        water.tile_count(),
        water.statistics().water_sample_count,
    );

    let base_145_config = CoverageConfig::base_to_handheld(center, 145.0, threads);
    let base_145 = run_scenario(
        "real terrain 145 MHz, 20 m TX",
        &dem,
        &water,
        base_145_config,
        &output_dir.join("coverage-real-145mhz-20m.png"),
    )?;

    let flat = FlatTerrain {
        elevation_m: center_elevation_m,
    };
    let flat_145 = run_scenario(
        "flat terrain 145 MHz control",
        &flat,
        &water,
        base_145_config,
        &output_dir.join("coverage-flat-145mhz-20m.png"),
    )?;

    let all_land = UniformWater { is_water: false };
    let land_only_145 = run_scenario(
        "real terrain 145 MHz, all-land control",
        &dem,
        &all_land,
        base_145_config,
        &output_dir.join("coverage-real-145mhz-all-land-control.png"),
    )?;

    let mut config_435 = base_145_config;
    config_435.frequency_mhz = 435.0;
    let real_435 = run_scenario(
        "real terrain 435 MHz, 20 m TX",
        &dem,
        &water,
        config_435,
        &output_dir.join("coverage-real-435mhz-20m.png"),
    )?;

    let mut high_145_config = base_145_config;
    high_145_config.tx_height_m = 80.0;
    let high_145 = run_scenario(
        "real terrain 145 MHz, 80 m TX",
        &dem,
        &water,
        high_145_config,
        &output_dir.join("coverage-real-145mhz-80m.png"),
    )?;

    let terrain_delta = base_145
        .delta_from(&flat_145)
        .map_err(|error| error.to_string())?;
    let water_delta = base_145
        .delta_from(&land_only_145)
        .map_err(|error| error.to_string())?;
    let frequency_delta = real_435
        .delta_from(&base_145)
        .map_err(|error| error.to_string())?;
    let height_delta = high_145
        .delta_from(&base_145)
        .map_err(|error| error.to_string())?;
    let report_path = output_dir.join("validation.json");
    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    fs::write(
        &report_path,
        validation_json(
            center,
            dem.tile_count(),
            water.tile_count(),
            center_elevation_m,
            data_load_seconds,
            &base_145,
            &flat_145,
            &land_only_145,
            &real_435,
            &high_145,
            terrain_delta,
            water_delta,
            frequency_delta,
            height_delta,
        ),
    )
    .map_err(|error| error.to_string())?;

    println!(
        "terrain mean signed delta: {:.3} dB",
        terrain_delta.mean_signed_difference_db
    );
    println!(
        "435-145 mean signed delta: {:.3} dB",
        frequency_delta.mean_signed_difference_db
    );
    println!(
        "water-minus-all-land mean signed delta: {:.3} dB; affected paths {}",
        water_delta.mean_signed_difference_db, base_145.statistics.water_affected_pixel_count
    );
    println!(
        "80m-20m improved pixels: {} of {}",
        height_delta.improved_pixel_count, height_delta.compared_pixel_count
    );
    println!("validation report: {}", report_path.display());
    if let Some(store) = cache_guard.as_mut() {
        store
            .set_active_region(None)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn run_scenario(
    label: &str,
    elevation_source: &impl hamheatmap_coverage::ElevationSource,
    water_source: &impl hamheatmap_coverage::WaterSource,
    config: CoverageConfig,
    output_path: &Path,
) -> Result<CoverageGrid, String> {
    progress(&format!("computing {label}"))?;
    let started = Instant::now();
    let grid = compute_coverage(elevation_source, water_source, config)
        .map_err(|error| error.to_string())?;
    grid.write_png(output_path)
        .map_err(|error| error.to_string())?;
    println!(
        "completed {label} in {:.3}s (propagation {:.3}s), PNG {}",
        started.elapsed().as_secs_f64(),
        grid.propagation_time.as_secs_f64(),
        output_path.display()
    );
    Ok(grid)
}

fn progress(message: &str) -> Result<(), String> {
    println!("{message}...");
    io::stdout().flush().map_err(|error| error.to_string())
}

#[allow(clippy::too_many_arguments)]
fn validation_json(
    center: GeoPoint,
    dem_tile_count: usize,
    water_tile_count: usize,
    center_elevation_m: f32,
    data_load_seconds: f64,
    base_145: &CoverageGrid,
    flat_145: &CoverageGrid,
    land_only_145: &CoverageGrid,
    real_435: &CoverageGrid,
    high_145: &CoverageGrid,
    terrain_delta: CoverageDelta,
    water_delta: CoverageDelta,
    frequency_delta: CoverageDelta,
    height_delta: CoverageDelta,
) -> String {
    format!(
        concat!(
            "{{\n",
            "  \"schema_version\": 2,\n",
            "  \"model_defaults_version\": \"{}\",\n",
            "  \"center\": {{\"lat\": {:.8}, \"lon\": {:.8}, \"dem_elevation_m\": {:.3}}},\n",
            "  \"data\": {{\"dem_tile_count\": {}, \"water_tile_count\": {}, \"load_seconds\": {:.6}}},\n",
            "  \"base_145\": {},\n",
            "  \"flat_145\": {},\n",
            "  \"land_only_145\": {},\n",
            "  \"real_435\": {},\n",
            "  \"high_145\": {},\n",
            "  \"terrain_minus_flat\": {},\n",
            "  \"water_minus_all_land\": {},\n",
            "  \"frequency_435_minus_145\": {},\n",
            "  \"height_80m_minus_20m\": {},\n",
            "  \"power_tenfold_expected_delta_db\": 10.0\n",
            "}}\n"
        ),
        MODEL_DEFAULTS_VERSION,
        center.lat,
        center.lon,
        center_elevation_m,
        dem_tile_count,
        water_tile_count,
        data_load_seconds,
        statistics_json(base_145),
        statistics_json(flat_145),
        statistics_json(land_only_145),
        statistics_json(real_435),
        statistics_json(high_145),
        delta_json(terrain_delta),
        delta_json(water_delta),
        delta_json(frequency_delta),
        delta_json(height_delta),
    )
}

fn statistics_json(grid: &CoverageGrid) -> String {
    let statistics = &grid.statistics;
    format!(
        concat!(
            "{{\"valid_pixels\": {}, \"masked_pixels\": {}, ",
            "\"below_threshold_pixels\": {}, \"warning_pixels\": {}, ",
            "\"warning_mask_or\": {}, \"warning_counts\": {}, ",
            "\"minimum_dbm\": {:.6}, \"maximum_dbm\": {:.6}, ",
            "\"mean_dbm\": {:.6}, \"water_affected_pixels\": {}, ",
            "\"mean_path_water_fraction\": {:.9}, \"maximum_path_water_fraction\": {:.9}, ",
            "\"receiver_generation_seconds\": {:.6}, ",
            "\"propagation_seconds\": {:.6}, ",
            "\"modes\": {{\"line_of_sight\": {}, \"diffraction\": {}, ",
            "\"troposcatter\": {}, \"unknown\": {}}}}}"
        ),
        statistics.valid_pixel_count,
        statistics.masked_pixel_count,
        statistics.below_threshold_pixel_count,
        statistics.warning_pixel_count,
        statistics.warning_mask_or,
        warning_counts_json(&statistics.warning_bit_counts),
        statistics.minimum_dbm,
        statistics.maximum_dbm,
        statistics.mean_dbm,
        statistics.water_affected_pixel_count,
        statistics.mean_path_water_fraction,
        statistics.maximum_path_water_fraction,
        grid.receiver_generation_time.as_secs_f64(),
        grid.propagation_time.as_secs_f64(),
        statistics.mode_counts.line_of_sight,
        statistics.mode_counts.diffraction,
        statistics.mode_counts.troposcatter,
        statistics.mode_counts.unknown,
    )
}

fn warning_counts_json(counts: &[usize; ITM_WARNING_BIT_COUNT]) -> String {
    let entries: Vec<_> = ITM_WARNING_NAMES
        .iter()
        .zip(counts)
        .map(|(name, count)| format!("\"{name}\": {count}"))
        .collect();
    format!("{{{}}}", entries.join(", "))
}

fn delta_json(delta: CoverageDelta) -> String {
    format!(
        concat!(
            "{{\"compared_pixels\": {}, \"improved_pixels\": {}, ",
            "\"worsened_pixels\": {}, \"unchanged_pixels\": {}, ",
            "\"mean_signed_difference_db\": {:.6}, ",
            "\"maximum_gain_db\": {:.6}, \"maximum_loss_db\": {:.6}}}"
        ),
        delta.compared_pixel_count,
        delta.improved_pixel_count,
        delta.worsened_pixel_count,
        delta.unchanged_pixel_count,
        delta.mean_signed_difference_db,
        delta.maximum_gain_db,
        delta.maximum_loss_db,
    )
}

fn option<T>(args: &[String], name: &str, default: T) -> Result<T, String>
where
    T: FromStr,
    T::Err: ToString,
{
    let Some(index) = args.iter().position(|argument| argument == name) else {
        return Ok(default);
    };
    let raw = args
        .get(index + 1)
        .ok_or_else(|| format!("{name} requires a value"))?;
    raw.parse::<T>()
        .map_err(|error| format!("invalid {name} value {raw:?}: {}", error.to_string()))
}

fn optional_option<T>(args: &[String], name: &str) -> Result<Option<T>, String>
where
    T: FromStr,
    T::Err: ToString,
{
    let Some(index) = args.iter().position(|argument| argument == name) else {
        return Ok(None);
    };
    let raw = args
        .get(index + 1)
        .ok_or_else(|| format!("{name} requires a value"))?;
    raw.parse::<T>()
        .map(Some)
        .map_err(|error| format!("invalid {name} value {raw:?}: {}", error.to_string()))
}

fn required_option<T>(args: &[String], name: &str) -> Result<T, String>
where
    T: FromStr,
    T::Err: ToString,
{
    optional_option(args, name)?.ok_or_else(|| format!("{name} is required"))
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|argument| argument == name)
}
