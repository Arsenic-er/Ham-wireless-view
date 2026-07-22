//! Application-facing contract shared by the Tauri shell and core tests.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use geographiclib_rs::{DirectGeodesic, Geodesic};
use hamheatmap_cache::{
    CacheError, CacheRegion, CacheState, CacheStore, CacheUsage, DownloadProgress,
    GeoPoint as CacheGeoPoint, Glo90DownloadService, glo90_assets, plan_glo90_region,
};
use hamheatmap_coverage::{
    CoverageConfig, CoverageProgress, GRID_SIZE, GeoPoint, MODEL_DEFAULTS_VERSION,
    compute_coverage_with_control,
};
use hamheatmap_propagation::{Polarization, dbd_to_dbi, dbm_to_watts};
use hamheatmap_terrain::{DemTileId, DemTileSet, WaterTileSet};
use serde::{Deserialize, Serialize};

pub const APP_SERVICE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Band {
    Vhf144,
    Uhf430,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PowerUnit {
    Watt,
    Dbm,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GainUnit {
    Dbi,
    Dbd,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PolarizationChoice {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculationRequest {
    pub center: MapPoint,
    pub band: Band,
    pub frequency_mhz: f64,
    pub power_value: f64,
    pub power_unit: PowerUnit,
    pub tx_gain_value: f64,
    pub tx_gain_unit: GainUnit,
    pub tx_height_m: f64,
    pub rx_gain_value: f64,
    pub rx_gain_unit: GainUnit,
    pub rx_height_m: f64,
    pub polarization: PolarizationChoice,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CalculationPhase {
    LoadingData,
    Computing,
    Encoding,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculationProgress {
    pub phase: CalculationPhase,
    pub percent: f64,
    pub completed_pixel_count: usize,
    pub total_pixel_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheUsageView {
    pub total_bytes: u64,
    pub dem_bytes: u64,
    pub water_bytes: u64,
    pub partial_bytes: u64,
    pub metadata_bytes: u64,
    pub remaining_bytes: u64,
    pub cap_bytes: u64,
}

impl From<CacheUsage> for CacheUsageView {
    fn from(value: CacheUsage) -> Self {
        Self {
            total_bytes: value.total_bytes,
            dem_bytes: value.dem_bytes,
            water_bytes: value.water_bytes,
            partial_bytes: value.partial_bytes,
            metadata_bytes: value.metadata_and_unindexed_bytes,
            remaining_bytes: value.remaining_bytes(),
            cap_bytes: value.cap_bytes,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapInfo {
    pub schema_version: u32,
    pub model_name: &'static str,
    pub model_version: &'static str,
    pub coverage_radius_km: u32,
    pub grid_size: usize,
    pub cache_usage: CacheUsageView,
    pub internal_build_warning: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PointInspection {
    pub point: MapPoint,
    pub region_id: String,
    pub tile_count: usize,
    pub ready_dem_count: usize,
    pub ready_water_count: usize,
    pub missing_asset_count: usize,
    pub data_ready: bool,
    pub elevation_m: Option<f32>,
    pub cache_usage: CacheUsageView,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEstimate {
    pub point: MapPoint,
    pub region_id: String,
    pub tile_count: usize,
    pub ready_asset_count: usize,
    pub required_asset_count: usize,
    pub generated_asset_count: usize,
    pub additional_download_bytes: u64,
    pub resumable_bytes: u64,
    pub projected_total_bytes: u64,
    pub projected_remaining_bytes: u64,
    pub cache_usage: CacheUsageView,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgressView {
    pub asset_index: usize,
    pub asset_count: usize,
    pub asset_key: String,
    pub asset_downloaded_bytes: u64,
    pub asset_expected_bytes: u64,
    pub total_downloaded_bytes: u64,
    pub total_expected_bytes: u64,
    pub percent: f64,
}

impl From<&DownloadProgress> for DownloadProgressView {
    fn from(value: &DownloadProgress) -> Self {
        let percent = if value.total_expected_bytes == 0 {
            100.0
        } else {
            100.0 * value.total_downloaded_bytes as f64 / value.total_expected_bytes as f64
        };
        Self {
            asset_index: value.asset_index + 1,
            asset_count: value.asset_count,
            asset_key: value.asset_key.clone(),
            asset_downloaded_bytes: value.asset_downloaded_bytes,
            asset_expected_bytes: value.asset_expected_bytes,
            total_downloaded_bytes: value.total_downloaded_bytes,
            total_expected_bytes: value.total_expected_bytes,
            percent: percent.clamp(0.0, 100.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub inspection: PointInspection,
    pub prepared_asset_count: usize,
    pub downloaded_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheRegionView {
    pub region_id: String,
    pub center: MapPoint,
    pub asset_count: usize,
    pub ready_asset_count: usize,
    pub partial_asset_count: usize,
    pub referenced_bytes: u64,
    pub reclaimable_bytes: u64,
    pub created_unix: i64,
}

impl From<CacheRegion> for CacheRegionView {
    fn from(value: CacheRegion) -> Self {
        Self {
            region_id: value.region_id,
            center: MapPoint {
                lat: value.center_lat,
                lon: value.center_lon,
            },
            asset_count: value.asset_count,
            ready_asset_count: value.ready_asset_count,
            partial_asset_count: value.partial_asset_count,
            referenced_bytes: value.referenced_bytes,
            reclaimable_bytes: value.reclaimable_bytes,
            created_unix: value.created_unix,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheOverview {
    pub usage: CacheUsageView,
    pub regions: Vec<CacheRegionView>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheDeleteResult {
    pub deleted_asset_count: usize,
    pub freed_bytes: u64,
    pub overview: CacheOverview,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculationStatisticsView {
    pub valid_pixel_count: usize,
    pub below_threshold_pixel_count: usize,
    pub warning_pixel_count: usize,
    pub minimum_dbm: f32,
    pub maximum_dbm: f32,
    pub mean_dbm: f64,
    pub water_affected_pixel_count: usize,
    pub mean_path_water_fraction: f64,
    pub propagation_seconds: f64,
    pub total_seconds: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalculationResult {
    pub schema_version: u32,
    pub model_name: &'static str,
    pub model_version: &'static str,
    pub center: MapPoint,
    pub image_width: usize,
    pub image_height: usize,
    pub image_corners: [[f64; 2]; 4],
    pub heatmap_png_data_url: String,
    pub statistics: CalculationStatisticsView,
}

#[derive(Clone, Debug)]
pub struct AppService {
    cache_root: PathBuf,
}

impl AppService {
    pub fn new(cache_root: impl Into<PathBuf>) -> Self {
        Self {
            cache_root: cache_root.into(),
        }
    }

    pub fn cache_root(&self) -> &Path {
        &self.cache_root
    }

    pub fn bootstrap(&self) -> Result<BootstrapInfo, String> {
        let store = CacheStore::open(&self.cache_root).map_err(|error| error.to_string())?;
        let usage = store.usage().map_err(|error| error.to_string())?;
        Ok(BootstrapInfo {
            schema_version: APP_SERVICE_SCHEMA_VERSION,
            model_name: "NTIA ITM Point-to-Point",
            model_version: MODEL_DEFAULTS_VERSION,
            coverage_radius_km: 200,
            grid_size: GRID_SIZE,
            cache_usage: usage.into(),
            internal_build_warning: "内部测试底图，不得公开发布",
        })
    }

    pub fn inspect_point(&self, point: MapPoint) -> Result<PointInspection, String> {
        validate_point(point)?;
        let plan = plan_glo90_region(CacheGeoPoint {
            lat: point.lat,
            lon: point.lon,
        })
        .map_err(|error| error.to_string())?;
        let mut store = CacheStore::open(&self.cache_root).map_err(|error| error.to_string())?;

        let assets: HashMap<_, _> = store
            .list_assets()
            .map_err(|error| error.to_string())?
            .into_iter()
            .map(|asset| (asset.asset_key.clone(), asset))
            .collect();
        let mut region_keys = HashSet::with_capacity(plan.tiles.len() * 2);
        let mut ready_dem_count = 0;
        let mut ready_water_count = 0;
        for tile in &plan.tiles {
            for descriptor in glo90_assets(*tile).map_err(|error| error.to_string())? {
                region_keys.insert(descriptor.asset_key.clone());
                if assets
                    .get(&descriptor.asset_key)
                    .is_some_and(|asset| asset.state == CacheState::Ready)
                {
                    match descriptor.kind {
                        hamheatmap_cache::CacheKind::Dem => ready_dem_count += 1,
                        hamheatmap_cache::CacheKind::Water => ready_water_count += 1,
                        _ => {}
                    }
                }
            }
        }
        let data_ready =
            ready_dem_count == plan.tiles.len() && ready_water_count == plan.tiles.len();
        let elevation_m = if data_ready {
            let dem_paths = store
                .ready_paths_for_region(&plan)
                .map_err(|error| error.to_string())?;
            store
                .ready_water_paths_for_region(&plan)
                .map_err(|error| error.to_string())?;
            let center_tile = DemTileId {
                south_lat_deg: point.lat.floor() as i32,
                west_lon_deg: point.lon.floor() as i32,
            };
            let center_filename = center_tile.filename();
            let center_path = dem_paths
                .into_iter()
                .find(|path| {
                    path.file_name()
                        .is_some_and(|filename| filename == center_filename.as_str())
                })
                .ok_or_else(|| format!("中心点高程瓦片 {center_filename} 不在区域计划中"))?;
            let dem = DemTileSet::open_paths([center_path]).map_err(|error| error.to_string())?;
            Some(
                dem.sample_bilinear(point.lon, point.lat)
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        let usage = store.usage().map_err(|error| error.to_string())?;
        let ready_asset_count = ready_dem_count + ready_water_count;
        Ok(PointInspection {
            point,
            region_id: plan.region_id,
            tile_count: plan.tiles.len(),
            ready_dem_count,
            ready_water_count,
            missing_asset_count: region_keys.len().saturating_sub(ready_asset_count),
            data_ready,
            elevation_m,
            cache_usage: usage.into(),
        })
    }

    pub fn estimate_download(&self, point: MapPoint) -> Result<DownloadEstimate, String> {
        let cancelled = AtomicBool::new(false);
        self.estimate_download_with_cancel(point, &cancelled)
    }

    pub fn estimate_download_with_cancel(
        &self,
        point: MapPoint,
        cancelled: &AtomicBool,
    ) -> Result<DownloadEstimate, String> {
        validate_point(point)?;
        let region = plan_glo90_region(CacheGeoPoint {
            lat: point.lat,
            lon: point.lon,
        })
        .map_err(cache_error_message)?;
        let mut store = CacheStore::open(&self.cache_root).map_err(cache_error_message)?;
        let plan = Glo90DownloadService::new()
            .probe_region_with_cancel(&mut store, region, cancelled)
            .map_err(cache_error_message)?;
        let usage = store.usage().map_err(cache_error_message)?;
        let resumable_bytes = plan.assets.iter().try_fold(0_u64, |total, asset| {
            total
                .checked_add(asset.resumable_bytes)
                .ok_or_else(|| "可续传字节数溢出".to_string())
        })?;
        let projected_total_bytes = usage
            .total_bytes
            .checked_add(plan.additional_download_bytes)
            .ok_or_else(|| "预计缓存占用字节数溢出".to_string())?;
        Ok(DownloadEstimate {
            point,
            region_id: plan.region.region_id.clone(),
            tile_count: plan.region.tiles.len(),
            ready_asset_count: plan.ready_asset_count,
            required_asset_count: plan.assets.len(),
            generated_asset_count: plan.generated_asset_count,
            additional_download_bytes: plan.additional_download_bytes,
            resumable_bytes,
            projected_total_bytes,
            projected_remaining_bytes: usage.cap_bytes.saturating_sub(projected_total_bytes),
            cache_usage: usage.into(),
        })
    }

    pub fn download_region(
        &self,
        point: MapPoint,
        cancelled: &AtomicBool,
        progress: impl Fn(DownloadProgressView),
    ) -> Result<DownloadResult, String> {
        validate_point(point)?;
        let region = plan_glo90_region(CacheGeoPoint {
            lat: point.lat,
            lon: point.lon,
        })
        .map_err(cache_error_message)?;
        let mut store = CacheStore::open(&self.cache_root).map_err(cache_error_message)?;
        let service = Glo90DownloadService::new();
        let plan = service
            .probe_region_with_cancel(&mut store, region, cancelled)
            .map_err(cache_error_message)?;
        let prepared_asset_count = plan.assets.len();
        let downloaded_bytes = plan.additional_download_bytes;
        let mut last_emitted_percent = -1.0_f64;
        let mut last_emitted_at = Instant::now();
        service
            .execute(&mut store, &plan, cancelled, |value| {
                let view = DownloadProgressView::from(value);
                let finished = view.total_downloaded_bytes >= view.total_expected_bytes;
                if last_emitted_percent < 0.0
                    || view.percent - last_emitted_percent >= 0.5
                    || last_emitted_at.elapsed().as_millis() >= 250
                    || finished
                {
                    last_emitted_percent = view.percent;
                    last_emitted_at = Instant::now();
                    progress(view);
                }
            })
            .map_err(cache_error_message)?;
        drop(store);
        let inspection = self.inspect_point(point)?;
        if !inspection.data_ready {
            return Err("下载结束后区域数据仍不完整，请检查缓存完整性".into());
        }
        Ok(DownloadResult {
            inspection,
            prepared_asset_count,
            downloaded_bytes,
        })
    }

    pub fn cache_overview(&self) -> Result<CacheOverview, String> {
        let store = CacheStore::open(&self.cache_root).map_err(cache_error_message)?;
        cache_overview(&store)
    }

    pub fn delete_cache_region(&self, region_id: &str) -> Result<CacheDeleteResult, String> {
        if region_id.trim().is_empty() {
            return Err("缓存区域标识不能为空".into());
        }
        let mut store = CacheStore::open(&self.cache_root).map_err(cache_error_message)?;
        let result = store
            .delete_region(region_id)
            .map_err(cache_error_message)?;
        let overview = cache_overview(&store)?;
        Ok(CacheDeleteResult {
            deleted_asset_count: result.deleted_asset_count,
            freed_bytes: result.freed_bytes,
            overview,
        })
    }

    pub fn calculate(
        &self,
        request: &CalculationRequest,
        cancelled: &AtomicBool,
        progress: impl Fn(CalculationProgress) + Sync,
    ) -> Result<CalculationResult, String> {
        let config = request_to_config(request)?;
        let started = Instant::now();
        progress(CalculationProgress {
            phase: CalculationPhase::LoadingData,
            percent: 0.0,
            completed_pixel_count: 0,
            total_pixel_count: 0,
        });

        let plan = plan_glo90_region(CacheGeoPoint {
            lat: request.center.lat,
            lon: request.center.lon,
        })
        .map_err(|error| error.to_string())?;
        let mut store = CacheStore::open(&self.cache_root).map_err(|error| error.to_string())?;
        store
            .upsert_region(&plan)
            .map_err(|error| error.to_string())?;
        let dem_paths = store
            .ready_paths_for_region(&plan)
            .map_err(|error| format!("{error}; 请先联网缓存当前发射点周围的数据"))?;
        let water_paths = store
            .ready_water_paths_for_region(&plan)
            .map_err(|error| format!("{error}; 请先联网缓存当前发射点周围的数据"))?;
        store
            .set_active_region(Some(&plan.region_id))
            .map_err(|error| error.to_string())?;

        let result = (|| {
            let dem = DemTileSet::open_paths(dem_paths).map_err(|error| error.to_string())?;
            let water = WaterTileSet::open_paths(water_paths).map_err(|error| error.to_string())?;
            progress(CalculationProgress {
                phase: CalculationPhase::Computing,
                percent: 5.0,
                completed_pixel_count: 0,
                total_pixel_count: 125_628,
            });
            let grid = compute_coverage_with_control(
                &dem,
                &water,
                config,
                cancelled,
                |value: CoverageProgress| {
                    progress(CalculationProgress {
                        phase: CalculationPhase::Computing,
                        percent: 5.0
                            + 90.0 * value.completed_pixel_count as f64
                                / value.total_pixel_count as f64,
                        completed_pixel_count: value.completed_pixel_count,
                        total_pixel_count: value.total_pixel_count,
                    });
                },
            )
            .map_err(|error| error.to_string())?;
            progress(CalculationProgress {
                phase: CalculationPhase::Encoding,
                percent: 96.0,
                completed_pixel_count: grid.statistics.valid_pixel_count,
                total_pixel_count: grid.statistics.valid_pixel_count,
            });
            let png = grid.encode_png().map_err(|error| error.to_string())?;
            let statistics = CalculationStatisticsView {
                valid_pixel_count: grid.statistics.valid_pixel_count,
                below_threshold_pixel_count: grid.statistics.below_threshold_pixel_count,
                warning_pixel_count: grid.statistics.warning_pixel_count,
                minimum_dbm: grid.statistics.minimum_dbm,
                maximum_dbm: grid.statistics.maximum_dbm,
                mean_dbm: grid.statistics.mean_dbm,
                water_affected_pixel_count: grid.statistics.water_affected_pixel_count,
                mean_path_water_fraction: grid.statistics.mean_path_water_fraction,
                propagation_seconds: grid.propagation_time.as_secs_f64(),
                total_seconds: started.elapsed().as_secs_f64(),
            };
            Ok(CalculationResult {
                schema_version: APP_SERVICE_SCHEMA_VERSION,
                model_name: "NTIA ITM Point-to-Point",
                model_version: MODEL_DEFAULTS_VERSION,
                center: request.center,
                image_width: GRID_SIZE,
                image_height: GRID_SIZE,
                image_corners: heatmap_image_corners(request.center),
                heatmap_png_data_url: format!(
                    "data:image/png;base64,{}",
                    BASE64_STANDARD.encode(png)
                ),
                statistics,
            })
        })();
        let clear_result = store
            .set_active_region(None)
            .map_err(|error| error.to_string());
        match (result, clear_result) {
            (Ok(value), Ok(())) => {
                progress(CalculationProgress {
                    phase: CalculationPhase::Complete,
                    percent: 100.0,
                    completed_pixel_count: value.statistics.valid_pixel_count,
                    total_pixel_count: value.statistics.valid_pixel_count,
                });
                Ok(value)
            }
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }
}

fn cache_overview(store: &CacheStore) -> Result<CacheOverview, String> {
    let usage = store.usage().map_err(cache_error_message)?;
    let regions = store
        .list_regions()
        .map_err(cache_error_message)?
        .into_iter()
        .map(CacheRegionView::from)
        .collect();
    Ok(CacheOverview {
        usage: usage.into(),
        regions,
    })
}

fn cache_error_message(error: CacheError) -> String {
    match error {
        CacheError::QuotaExceeded {
            current_bytes,
            requested_additional_bytes,
            cap_bytes,
        } => format!(
            "缓存空间不足：当前 {current_bytes} 字节，还需 {requested_additional_bytes} 字节，上限为 {cap_bytes} 字节。请先在缓存管理中删除区域。"
        ),
        CacheError::DiskSpaceInsufficient {
            available_bytes,
            requested_additional_bytes,
        } => format!(
            "磁盘可用空间不足：当前可用 {available_bytes} 字节，本次至少需要 {requested_additional_bytes} 字节。"
        ),
        CacheError::Network(message) => {
            format!("无法连接固定的 Copernicus 数据源：{message}")
        }
        CacheError::Integrity { asset_key, message } => {
            format!("缓存资产 {asset_key} 完整性检查失败：{message}")
        }
        CacheError::MissingAssets(asset_keys) => {
            format!("当前区域仍缺少 {} 个缓存资产", asset_keys.len())
        }
        CacheError::ActiveRegion(_) => "该区域正在使用中，请先取消当前任务".into(),
        CacheError::Cancelled => "操作已取消；已完成的资产和可续传临时文件会保留".into(),
        other => other.to_string(),
    }
}

fn request_to_config(request: &CalculationRequest) -> Result<CoverageConfig, String> {
    validate_point(request.center)?;
    let frequency_range = match request.band {
        Band::Vhf144 => 144.0..=148.0,
        Band::Uhf430 => 430.0..=440.0,
    };
    if !request.frequency_mhz.is_finite()
        || !frequency_range.contains(&request.frequency_mhz)
        || ((request.frequency_mhz * 100.0).round() - request.frequency_mhz * 100.0).abs() > 1e-8
    {
        return Err("频率必须位于所选频段内，并精确到最多两位小数".into());
    }
    let tx_power_w = match request.power_unit {
        PowerUnit::Watt => request.power_value,
        PowerUnit::Dbm => dbm_to_watts(request.power_value).map_err(|error| error.to_string())?,
    };
    validate_range("发射功率", tx_power_w, 0.1, 1000.0)?;
    let tx_gain_dbi = gain_to_dbi(request.tx_gain_value, request.tx_gain_unit)?;
    let rx_gain_dbi = gain_to_dbi(request.rx_gain_value, request.rx_gain_unit)?;
    validate_range("发射天线增益", tx_gain_dbi, -20.0, 30.0)?;
    validate_range("接收天线增益", rx_gain_dbi, -20.0, 30.0)?;
    validate_range("发射天线高度", request.tx_height_m, 0.5, 500.0)?;
    validate_range("接收天线高度", request.rx_height_m, 0.5, 500.0)?;
    let threads = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    Ok(CoverageConfig {
        center: GeoPoint {
            lat: request.center.lat,
            lon: request.center.lon,
        },
        frequency_mhz: request.frequency_mhz,
        polarization: match request.polarization {
            PolarizationChoice::Horizontal => Polarization::Horizontal,
            PolarizationChoice::Vertical => Polarization::Vertical,
        },
        tx_power_w,
        tx_gain_dbi,
        rx_gain_dbi,
        tx_height_m: request.tx_height_m,
        rx_height_m: request.rx_height_m,
        threads,
        profile_sample_spacing_m: hamheatmap_coverage::PROFILE_SAMPLE_SPACING_M,
    })
}

fn gain_to_dbi(value: f64, unit: GainUnit) -> Result<f64, String> {
    match unit {
        GainUnit::Dbi => Ok(value),
        GainUnit::Dbd => dbd_to_dbi(value).map_err(|error| error.to_string()),
    }
}

fn validate_point(point: MapPoint) -> Result<(), String> {
    if !point.lat.is_finite()
        || !point.lon.is_finite()
        || !(-90.0..=90.0).contains(&point.lat)
        || !(-180.0..=180.0).contains(&point.lon)
    {
        return Err("发射点经纬度无效".into());
    }
    Ok(())
}

fn validate_range(name: &str, value: f64, minimum: f64, maximum: f64) -> Result<(), String> {
    if !value.is_finite() || !(minimum..=maximum).contains(&value) {
        return Err(format!("{name}必须位于 {minimum}–{maximum} 范围内"));
    }
    Ok(())
}

fn heatmap_image_corners(center: MapPoint) -> [[f64; 2]; 4] {
    let geodesic = Geodesic::wgs84();
    let corner_distance_m = 200_000.0_f64 * 2.0_f64.sqrt();
    [-45.0, 45.0, 135.0, -135.0].map(|azimuth| {
        let (lat, lon, _): (f64, f64, f64) =
            geodesic.direct(center.lat, center.lon, azimuth, corner_distance_m);
        [lon, lat]
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use geographiclib_rs::InverseGeodesic;

    use super::*;

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let sequence = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "hamheatmap-app-service-{name}-{}-{sequence}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn request() -> CalculationRequest {
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
            rx_gain_value: -3.0,
            rx_gain_unit: GainUnit::Dbi,
            rx_height_m: 1.5,
            polarization: PolarizationChoice::Vertical,
        }
    }

    #[test]
    fn base_to_handheld_request_maps_to_coverage_config() {
        let config = request_to_config(&request()).unwrap();
        assert_eq!(config.frequency_mhz, 145.0);
        assert_eq!(config.tx_power_w, 25.0);
        assert_eq!(config.tx_gain_dbi, 6.0);
        assert_eq!(config.rx_gain_dbi, -3.0);
        assert_eq!(config.polarization, Polarization::Vertical);
    }

    #[test]
    fn dbm_and_dbd_units_are_normalized_once_in_rust() {
        let mut value = request();
        value.power_value = 43.979_400_086_720_375;
        value.power_unit = PowerUnit::Dbm;
        value.tx_gain_value = 3.85;
        value.tx_gain_unit = GainUnit::Dbd;
        let config = request_to_config(&value).unwrap();
        assert!((config.tx_power_w - 25.0).abs() < 1e-9);
        assert!((config.tx_gain_dbi - 6.0).abs() < 1e-12);
    }

    #[test]
    fn frequency_must_match_band_and_two_decimal_contract() {
        let mut value = request();
        value.frequency_mhz = 435.0;
        assert!(request_to_config(&value).is_err());
        value.band = Band::Uhf430;
        assert!(request_to_config(&value).is_ok());
        value.frequency_mhz = 435.001;
        assert!(request_to_config(&value).is_err());
    }

    #[test]
    fn image_corners_are_wgs84_diagonals_of_fixed_grid() {
        let center = request().center;
        let geodesic = Geodesic::wgs84();
        for corner in heatmap_image_corners(center) {
            let distance_m: f64 = geodesic.inverse(center.lat, center.lon, corner[1], corner[0]);
            assert!((distance_m - 200_000.0 * 2.0_f64.sqrt()).abs() < 0.01);
        }
    }

    #[test]
    fn cache_cap_exposed_to_ui_is_exact_decimal_limit() {
        assert_eq!(hamheatmap_cache::TOTAL_CACHE_CAP_BYTES, 2_500_000_000);
    }

    #[test]
    fn download_estimate_honors_a_preexisting_cancellation_without_network_access() {
        let directory = TestDirectory::new("cancelled-estimate");
        let cancelled = AtomicBool::new(true);
        let error = AppService::new(&directory.0)
            .estimate_download_with_cancel(
                MapPoint {
                    lat: 30.5,
                    lon: 103.5,
                },
                &cancelled,
            )
            .unwrap_err();
        assert!(error.contains("取消"), "{error}");
    }

    #[test]
    fn cache_overview_and_delete_expose_region_lifecycle() {
        let directory = TestDirectory::new("region-lifecycle");
        let plan = plan_glo90_region(CacheGeoPoint {
            lat: 30.5,
            lon: 103.5,
        })
        .unwrap();
        let region_id = plan.region_id.clone();
        {
            let mut store = CacheStore::open(&directory.0).unwrap();
            store.upsert_region(&plan).unwrap();
        }
        let service = AppService::new(&directory.0);
        let overview = service.cache_overview().unwrap();
        assert_eq!(overview.regions.len(), 1);
        assert_eq!(overview.regions[0].region_id, region_id);
        assert_eq!(overview.regions[0].asset_count, plan.tiles.len() * 2);
        assert_eq!(overview.regions[0].ready_asset_count, 0);

        let deleted = service.delete_cache_region(&region_id).unwrap();
        assert_eq!(deleted.deleted_asset_count, plan.tiles.len() * 2);
        assert_eq!(deleted.freed_bytes, 0);
        assert!(deleted.overview.regions.is_empty());
    }

    #[test]
    fn download_progress_is_one_based_and_bounded() {
        let value = DownloadProgress {
            asset_index: 1,
            asset_count: 4,
            asset_key: "dem:test".into(),
            asset_downloaded_bytes: 60,
            asset_expected_bytes: 100,
            total_downloaded_bytes: 60,
            total_expected_bytes: 100,
        };
        let view = DownloadProgressView::from(&value);
        assert_eq!(view.asset_index, 2);
        assert_eq!(view.percent, 60.0);
    }
}
