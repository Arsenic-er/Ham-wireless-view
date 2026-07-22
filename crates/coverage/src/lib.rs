//! Fixed-grid real-terrain coverage engine used by the minimum-viability proof.

use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use geographiclib_rs::{DirectGeodesic, Geodesic};
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
    pub statistics: CoverageStatistics,
    pub receiver_generation_time: Duration,
    pub propagation_time: Duration,
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

    fn write_png_to(&self, writer: impl Write) -> Result<(), CoverageError> {
        let mut rgba = Vec::with_capacity(GRID_PIXEL_COUNT * 4);
        for &value in &self.values_dbm {
            rgba.extend_from_slice(&color_for_dbm(value));
        }
        let mut encoder = png::Encoder::new(writer, GRID_SIZE as u32, GRID_SIZE as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgba)?;
        Ok(())
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
    let receiver_started = Instant::now();
    let receivers = generate_receivers(config.center);
    let receiver_generation_time = receiver_started.elapsed();
    let tx_power_dbm = watts_to_dbm(config.tx_power_w)
        .map_err(|error| CoverageError::InvalidInput(error.to_string()))?;
    let mut prediction_inputs =
        PredictionInputs::land_water_v1(config.frequency_mhz, config.polarization);
    prediction_inputs.tx_height_m = config.tx_height_m;
    prediction_inputs.rx_height_m = config.rx_height_m;

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
                        prediction_inputs,
                        tx_power_dbm,
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
    prediction_inputs: PredictionInputs,
    tx_power_dbm: f64,
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
            let elevation = elevation_source
                .elevation_m(point.lon, point.lat)
                .map_err(|message| CoverageError::Terrain { point, message })?;
            if water_source
                .is_water(point.lon, point.lat)
                .map_err(|message| CoverageError::WaterMask { point, message })?
            {
                water_sample_count += 1;
            }
            pfl.push(f64::from(elevation));
            path.advance();
        }
        let water_fraction = water_sample_count as f64 / (interval_count + 1) as f64;
        let mut path_inputs = prediction_inputs;
        path_inputs.ground = ModelDefaults::LAND_WATER_V1
            .ground_for_water_fraction(water_fraction)
            .map_err(|error| CoverageError::Propagation(error.to_string()))?;
        let prediction = predict_p2p_pfl(&pfl, path_inputs)
            .map_err(|error| CoverageError::Propagation(error.to_string()))?;
        let received_power = received_power_dbm(
            tx_power_dbm,
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

fn generate_receivers(center: GeoPoint) -> Vec<Receiver> {
    let geodesic = Geodesic::wgs84();
    let mut receivers = Vec::with_capacity(126_000);
    for y_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
        for x_km in -GRID_RADIUS_KM..=GRID_RADIUS_KM {
            if x_km == 0 && y_km == 0 {
                continue;
            }
            let distance_km = f64::from(x_km * x_km + y_km * y_km).sqrt();
            if distance_km > f64::from(GRID_RADIUS_KM) {
                continue;
            }
            let azimuth_deg = f64::from(x_km).atan2(f64::from(y_km)).to_degrees();
            let distance_m = distance_km * 1000.0;
            let (lat, lon, _): (f64, f64, f64) =
                geodesic.direct(center.lat, center.lon, azimuth_deg, distance_m);
            let row = (GRID_RADIUS_KM - y_km) as usize;
            let column = (x_km + GRID_RADIUS_KM) as usize;
            receivers.push(Receiver {
                raster_index: row * GRID_SIZE + column,
                point: GeoPoint { lat, lon },
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
                inputs,
                tx_power_dbm,
                &[receiver],
                None,
            )
            .unwrap();
            let water = compute_chunk(
                &terrain,
                &UniformWater { is_water: true },
                config,
                inputs,
                tx_power_dbm,
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
