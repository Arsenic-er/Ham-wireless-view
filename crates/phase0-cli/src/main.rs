use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;
use std::time::Instant;

use hamheatmap_propagation::{
    Polarization, PredictionInputs, PropagationMode, TerrainProfile, predict_p2p,
    received_power_dbm, watts_to_dbm,
};
use hamheatmap_terrain::DemRaster;

const GRID_RADIUS_KM: i32 = 200;
const PROFILE_TARGET_SPACING_M: f64 = 90.0;

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
        Some("single-path") => single_path(&args[2..]),
        Some("inspect-dem") => inspect_dem(&args[2..]),
        Some("dem-path") => dem_path(&args[2..]),
        Some("benchmark") => benchmark(&args[2..]),
        Some("help" | "--help" | "-h") | None => {
            print_help();
            Ok(())
        }
        Some(other) => Err(format!("unknown command {other:?}; run with --help")),
    }
}

fn print_help() {
    println!(
        "HamHeatmap Phase 0 engineering CLI\n\
         \n\
         Commands:\n\
           single-path [--terrain flat|ridge] [--frequency 145|435] [--distance KM]\n\
           inspect-dem [--path FILE]\n\
           dem-path [--path FILE] [--frequency 145|435]\n\
           benchmark [--threads N] [--terrain flat|ridge] [--frequency 145|435]\n\
                     [--limit N]\n\
         \n\
         The production grid is always 401x401 at 1 km spacing with a 200 km\n\
         circular mask. --limit exists only for fast engineering smoke tests."
    );
}

fn default_dem_path() -> String {
    "data/dem/2021_1-aws-cog/N30E103.tif".into()
}

fn inspect_dem(args: &[String]) -> Result<(), String> {
    let path = PathBuf::from(option(args, "--path", default_dem_path())?);
    let started = Instant::now();
    let dem = DemRaster::open(&path).map_err(|error| error.to_string())?;
    let stats = dem.statistics().map_err(|error| error.to_string())?;
    let (west, south, east, north) = dem.sample_bounds();
    let georef = dem.georeference();
    let center_lon = (west + east) / 2.0;
    let center_lat = (south + north) / 2.0;
    let center_elevation_m = dem
        .sample_bilinear(center_lon, center_lat)
        .map_err(|error| error.to_string())?;

    println!("command=inspect-dem");
    println!("path={}", path.display());
    println!("width={}", dem.width());
    println!("height={}", dem.height());
    println!("pixel_width_deg={:.12}", georef.pixel_width_deg);
    println!("pixel_height_deg={:.12}", georef.pixel_height_deg);
    println!("west_sample_lon={west:.12}");
    println!("south_sample_lat={south:.12}");
    println!("east_sample_lon={east:.12}");
    println!("north_sample_lat={north:.12}");
    println!("nodata={:?}", dem.nodata());
    println!("valid_sample_count={}", stats.valid_sample_count);
    println!("nodata_sample_count={}", stats.nodata_sample_count);
    println!("minimum_elevation_m={:.3}", stats.minimum_m);
    println!("maximum_elevation_m={:.3}", stats.maximum_m);
    println!("mean_elevation_m={:.3}", stats.mean_m);
    println!("center_elevation_m={center_elevation_m:.3}");
    println!("decode_seconds={:.6}", started.elapsed().as_secs_f64());
    Ok(())
}

fn dem_path(args: &[String]) -> Result<(), String> {
    let path = PathBuf::from(option(args, "--path", default_dem_path())?);
    let frequency_mhz = option(args, "--frequency", 145.0_f64)?;
    let dem = DemRaster::open(&path).map_err(|error| error.to_string())?;
    let start = (103.05_f64, 30.50_f64);
    let end = (103.95_f64, 30.50_f64);
    let distance_m = haversine_distance_m(start, end);
    let interval_count = (distance_m / PROFILE_TARGET_SPACING_M).ceil() as usize;
    let mut elevations_m = Vec::with_capacity(interval_count + 1);
    for index in 0..=interval_count {
        let fraction = index as f64 / interval_count as f64;
        let lon = start.0 + (end.0 - start.0) * fraction;
        let lat = start.1 + (end.1 - start.1) * fraction;
        elevations_m.push(f64::from(
            dem.sample_bilinear(lon, lat)
                .map_err(|error| error.to_string())?,
        ));
    }
    let minimum_elevation_m = elevations_m.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum_elevation_m = elevations_m
        .iter()
        .copied()
        .fold(f64::NEG_INFINITY, f64::max);
    let profile = TerrainProfile::new(distance_m / interval_count as f64, elevations_m)
        .map_err(|error| error.to_string())?;
    let output = predict_p2p(
        &profile,
        PredictionInputs::phase0(frequency_mhz, Polarization::Vertical),
    )
    .map_err(|error| error.to_string())?;

    println!("command=dem-path");
    println!("path={}", path.display());
    println!("frequency_mhz={frequency_mhz:.2}");
    println!("start_lon={:.6}", start.0);
    println!("start_lat={:.6}", start.1);
    println!("end_lon={:.6}", end.0);
    println!("end_lat={:.6}", end.1);
    println!("distance_km={:.6}", distance_m / 1000.0);
    println!("profile_samples={}", profile.elevation_count());
    println!("minimum_elevation_m={minimum_elevation_m:.3}");
    println!("maximum_elevation_m={maximum_elevation_m:.3}");
    println!("mode={:?}", output.mode);
    println!(
        "basic_transmission_loss_db={:.6}",
        output.basic_transmission_loss_db
    );
    println!("warnings=0x{:x}", output.warnings);
    Ok(())
}

fn haversine_distance_m(start: (f64, f64), end: (f64, f64)) -> f64 {
    const EARTH_MEAN_RADIUS_M: f64 = 6_371_008.8;
    let lat1 = start.1.to_radians();
    let lat2 = end.1.to_radians();
    let delta_lat = (end.1 - start.1).to_radians();
    let delta_lon = (end.0 - start.0).to_radians();
    let a =
        (delta_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (delta_lon / 2.0).sin().powi(2);
    2.0 * EARTH_MEAN_RADIUS_M * a.sqrt().asin()
}

#[derive(Clone, Copy, Debug)]
enum TerrainKind {
    Flat,
    Ridge,
}

impl FromStr for TerrainKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "flat" => Ok(Self::Flat),
            "ridge" => Ok(Self::Ridge),
            _ => Err(format!("terrain must be flat or ridge, got {value:?}")),
        }
    }
}

fn single_path(args: &[String]) -> Result<(), String> {
    let terrain = option(args, "--terrain", TerrainKind::Ridge)?;
    let frequency_mhz = option(args, "--frequency", 145.0_f64)?;
    let distance_km = option(args, "--distance", 100.0_f64)?;
    let profile = synthetic_profile(distance_km, terrain)?;
    let inputs = PredictionInputs::phase0(frequency_mhz, Polarization::Vertical);
    let output = predict_p2p(&profile, inputs).map_err(|error| error.to_string())?;
    let tx_power_dbm = watts_to_dbm(25.0).map_err(|error| error.to_string())?;
    let rx_power_dbm =
        received_power_dbm(tx_power_dbm, 6.0, -3.0, output.basic_transmission_loss_db)
            .map_err(|error| error.to_string())?;

    println!("command=single-path");
    println!("terrain={terrain:?}");
    println!("frequency_mhz={frequency_mhz:.2}");
    println!("distance_km={:.3}", output.distance_km);
    println!("profile_samples={}", profile.elevation_count());
    println!("mode={:?}", output.mode);
    println!(
        "basic_transmission_loss_db={:.6}",
        output.basic_transmission_loss_db
    );
    println!("free_space_loss_db={:.6}", output.free_space_loss_db);
    println!(
        "reference_attenuation_db={:.6}",
        output.reference_attenuation_db
    );
    println!(
        "terrain_irregularity_m={:.6}",
        output.terrain_irregularity_m
    );
    println!("warnings=0x{:x}", output.warnings);
    println!("rx_power_dbm={rx_power_dbm:.6}");
    Ok(())
}

fn benchmark(args: &[String]) -> Result<(), String> {
    let default_threads = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let threads = option(args, "--threads", default_threads)?;
    if threads == 0 {
        return Err("--threads must be positive".into());
    }
    let terrain = option(args, "--terrain", TerrainKind::Flat)?;
    let frequency_mhz = option(args, "--frequency", 145.0_f64)?;
    let limit = optional::<usize>(args, "--limit")?;
    let mut distances = coverage_distances();
    let full_point_count = distances.len();
    if let Some(limit) = limit {
        distances.truncate(limit);
    }
    if distances.is_empty() {
        return Err("benchmark has no receiver points".into());
    }

    let inputs = PredictionInputs::phase0(frequency_mhz, Polarization::Vertical);
    let started = Instant::now();
    let worker_count = threads.min(distances.len());
    let chunk_size = distances.len().div_ceil(worker_count);
    let partials = std::thread::scope(|scope| {
        let handles: Vec<_> = distances
            .chunks(chunk_size)
            .map(|chunk| scope.spawn(move || benchmark_chunk(chunk, terrain, inputs)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("benchmark worker panicked"))
            .collect::<Vec<_>>()
    });
    let mut stats = BenchStats::new();
    for partial in partials {
        stats.merge(partial);
    }
    let elapsed = started.elapsed();

    println!("command=benchmark");
    println!("grid=401x401");
    println!("grid_spacing_km=1");
    println!("radius_km=200");
    println!("full_circle_point_count={full_point_count}");
    println!("evaluated_point_count={}", distances.len());
    println!("threads={worker_count}");
    println!("terrain={terrain:?}");
    println!("frequency_mhz={frequency_mhz:.2}");
    println!("profile_target_spacing_m={PROFILE_TARGET_SPACING_M:.1}");
    println!("elapsed_seconds={:.6}", elapsed.as_secs_f64());
    println!(
        "points_per_second={:.3}",
        distances.len() as f64 / elapsed.as_secs_f64()
    );
    println!("itm_point_count={}", stats.itm_count);
    println!("free_space_point_count={}", stats.free_space_count);
    println!("error_count={}", stats.error_count);
    println!("warning_point_count={}", stats.warning_count);
    println!("line_of_sight_count={}", stats.line_of_sight_count);
    println!("diffraction_count={}", stats.diffraction_count);
    println!("troposcatter_count={}", stats.troposcatter_count);
    println!("unknown_mode_count={}", stats.unknown_mode_count);
    println!("minimum_loss_db={:.6}", stats.minimum_loss_db);
    println!("maximum_loss_db={:.6}", stats.maximum_loss_db);
    println!(
        "mean_loss_db={:.6}",
        stats.loss_sum_db / stats.loss_count as f64
    );

    if stats.error_count > 0 {
        return Err(format!(
            "{} ITM calls failed; benchmark is invalid",
            stats.error_count
        ));
    }
    Ok(())
}

fn coverage_distances() -> Vec<f64> {
    let mut distances = Vec::with_capacity(126_000);
    for y in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
        for x in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
            if x == 0 && y == 0 {
                continue;
            }
            let distance_km = ((x * x + y * y) as f64).sqrt();
            if distance_km <= GRID_RADIUS_KM as f64 {
                distances.push(distance_km);
            }
        }
    }
    distances
}

fn synthetic_profile(distance_km: f64, terrain: TerrainKind) -> Result<TerrainProfile, String> {
    let distance_m = distance_km * 1000.0;
    let interval_count = (distance_m / PROFILE_TARGET_SPACING_M).ceil().max(1.0) as usize;
    let sample_spacing_m = distance_m / interval_count as f64;
    let mut elevations_m = vec![100.0; interval_count + 1];

    if matches!(terrain, TerrainKind::Ridge) {
        for (index, elevation) in elevations_m.iter_mut().enumerate() {
            let position = index as f64 / interval_count as f64;
            let offset = (position - 0.5).abs();
            if offset < 0.04 {
                *elevation += 800.0 * (1.0 - offset / 0.04);
            }
        }
    }
    TerrainProfile::new(sample_spacing_m, elevations_m).map_err(|error| error.to_string())
}

#[derive(Debug)]
struct BenchStats {
    itm_count: usize,
    free_space_count: usize,
    error_count: usize,
    warning_count: usize,
    line_of_sight_count: usize,
    diffraction_count: usize,
    troposcatter_count: usize,
    unknown_mode_count: usize,
    loss_count: usize,
    loss_sum_db: f64,
    minimum_loss_db: f64,
    maximum_loss_db: f64,
}

impl BenchStats {
    fn new() -> Self {
        Self {
            itm_count: 0,
            free_space_count: 0,
            error_count: 0,
            warning_count: 0,
            line_of_sight_count: 0,
            diffraction_count: 0,
            troposcatter_count: 0,
            unknown_mode_count: 0,
            loss_count: 0,
            loss_sum_db: 0.0,
            minimum_loss_db: f64::INFINITY,
            maximum_loss_db: f64::NEG_INFINITY,
        }
    }

    fn record_loss(&mut self, loss_db: f64) {
        self.loss_count += 1;
        self.loss_sum_db += loss_db;
        self.minimum_loss_db = self.minimum_loss_db.min(loss_db);
        self.maximum_loss_db = self.maximum_loss_db.max(loss_db);
    }

    fn merge(&mut self, other: Self) {
        self.itm_count += other.itm_count;
        self.free_space_count += other.free_space_count;
        self.error_count += other.error_count;
        self.warning_count += other.warning_count;
        self.line_of_sight_count += other.line_of_sight_count;
        self.diffraction_count += other.diffraction_count;
        self.troposcatter_count += other.troposcatter_count;
        self.unknown_mode_count += other.unknown_mode_count;
        self.loss_count += other.loss_count;
        self.loss_sum_db += other.loss_sum_db;
        self.minimum_loss_db = self.minimum_loss_db.min(other.minimum_loss_db);
        self.maximum_loss_db = self.maximum_loss_db.max(other.maximum_loss_db);
    }
}

fn benchmark_chunk(
    distances_km: &[f64],
    terrain: TerrainKind,
    inputs: PredictionInputs,
) -> BenchStats {
    let mut stats = BenchStats::new();
    for &distance_km in distances_km {
        let profile = match synthetic_profile(distance_km, terrain) {
            Ok(profile) => profile,
            Err(_) => {
                stats.error_count += 1;
                continue;
            }
        };
        match predict_p2p(&profile, inputs) {
            Ok(output) => {
                stats.itm_count += 1;
                stats.record_loss(output.basic_transmission_loss_db);
                if output.warnings != 0 {
                    stats.warning_count += 1;
                }
                match output.mode {
                    PropagationMode::LineOfSight => stats.line_of_sight_count += 1,
                    PropagationMode::Diffraction => stats.diffraction_count += 1,
                    PropagationMode::Troposcatter => stats.troposcatter_count += 1,
                    PropagationMode::Unknown(_) => stats.unknown_mode_count += 1,
                }
            }
            Err(_) => stats.error_count += 1,
        }
    }
    stats
}

fn option<T>(args: &[String], name: &str, default: T) -> Result<T, String>
where
    T: FromStr,
    T::Err: ToString,
{
    optional(args, name).map(|value| value.unwrap_or(default))
}

fn optional<T>(args: &[String], name: &str) -> Result<Option<T>, String>
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
