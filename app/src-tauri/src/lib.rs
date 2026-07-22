use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use hamheatmap_app_service::{
    AppService, BootstrapInfo, CacheDeleteResult, CacheOverview, CalculationRequest,
    CalculationResult, DownloadEstimate, DownloadProgressView, DownloadResult, MapPoint,
    PointInspection,
};
use hamheatmap_export::{
    ReportFormat, encode_report, path_with_format_extension, validate_suggested_file_name,
    write_report_atomic,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
#[cfg(windows)]
use tauri_plugin_dialog::DialogExt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DesktopOperation {
    Bootstrapping,
    InspectingPoint,
    EstimatingDownload,
    Downloading,
    ReadingCache,
    Calculating,
    DeletingCache,
    Exporting,
}

impl DesktopOperation {
    fn label(self) -> &'static str {
        match self {
            Self::Bootstrapping => "应用初始化",
            Self::InspectingPoint => "区域数据检查",
            Self::EstimatingDownload => "数据下载量检查",
            Self::Downloading => "区域数据下载",
            Self::ReadingCache => "缓存状态读取",
            Self::Calculating => "传播计算",
            Self::DeletingCache => "缓存删除",
            Self::Exporting => "结果导出",
        }
    }
}

struct DesktopState {
    data_root: PathBuf,
    cancelled: Arc<AtomicBool>,
    running_operation: Arc<Mutex<Option<DesktopOperation>>>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExportFormat {
    Png,
    Pdf,
}

impl From<ExportFormat> for ReportFormat {
    fn from(value: ExportFormat) -> Self {
        match value {
            ExportFormat::Png => Self::Png,
            ExportFormat::Pdf => Self::Pdf,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportRequest {
    format: ExportFormat,
    suggested_file_name: String,
    report_png_data_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportResultView {
    cancelled: bool,
    path: Option<String>,
    bytes_written: u64,
}

fn begin_operation(state: &DesktopState, operation: DesktopOperation) -> Result<(), String> {
    let mut running = state
        .running_operation
        .lock()
        .map_err(|_| "operation state lock is poisoned".to_string())?;
    if let Some(current) = *running {
        return Err(format!("{}正在进行，请稍候或先取消", current.label()));
    }
    *running = Some(operation);
    state.cancelled.store(false, Ordering::Release);
    Ok(())
}

fn finish_operation(running: &Mutex<Option<DesktopOperation>>) {
    if let Ok(mut value) = running.lock() {
        *value = None;
    }
}

#[tauri::command]
async fn bootstrap(state: State<'_, DesktopState>) -> Result<BootstrapInfo, String> {
    begin_operation(&state, DesktopOperation::Bootstrapping)?;
    let data_root = state.data_root.clone();
    let running = Arc::clone(&state.running_operation);
    let join_result =
        tauri::async_runtime::spawn_blocking(move || AppService::new(data_root).bootstrap()).await;
    finish_operation(&running);
    join_result.map_err(|error| format!("bootstrap worker failed: {error}"))?
}

#[tauri::command]
async fn inspect_point(
    point: MapPoint,
    state: State<'_, DesktopState>,
) -> Result<PointInspection, String> {
    begin_operation(&state, DesktopOperation::InspectingPoint)?;
    let data_root = state.data_root.clone();
    let running = Arc::clone(&state.running_operation);
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).inspect_point(point)
    })
    .await;
    finish_operation(&running);
    join_result.map_err(|error| format!("point inspection worker failed: {error}"))?
}

#[tauri::command]
async fn estimate_download(
    point: MapPoint,
    state: State<'_, DesktopState>,
) -> Result<DownloadEstimate, String> {
    begin_operation(&state, DesktopOperation::EstimatingDownload)?;
    let data_root = state.data_root.clone();
    let cancelled = Arc::clone(&state.cancelled);
    let running = Arc::clone(&state.running_operation);
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).estimate_download_with_cancel(point, &cancelled)
    })
    .await;
    finish_operation(&running);
    join_result.map_err(|error| format!("download estimate worker failed: {error}"))?
}

#[tauri::command]
async fn download_region(
    point: MapPoint,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DownloadResult, String> {
    begin_operation(&state, DesktopOperation::Downloading)?;
    let data_root = state.data_root.clone();
    let cancelled = Arc::clone(&state.cancelled);
    let running = Arc::clone(&state.running_operation);
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).download_region(point, &cancelled, |progress| {
            let _ = app.emit::<DownloadProgressView>("download-progress", progress);
        })
    })
    .await;
    finish_operation(&running);
    join_result.map_err(|error| format!("download worker failed: {error}"))?
}

#[tauri::command]
async fn cache_overview(state: State<'_, DesktopState>) -> Result<CacheOverview, String> {
    begin_operation(&state, DesktopOperation::ReadingCache)?;
    let data_root = state.data_root.clone();
    let running = Arc::clone(&state.running_operation);
    let join_result =
        tauri::async_runtime::spawn_blocking(move || AppService::new(data_root).cache_overview())
            .await;
    finish_operation(&running);
    join_result.map_err(|error| format!("cache overview worker failed: {error}"))?
}

#[tauri::command]
async fn delete_cache_region(
    region_id: String,
    state: State<'_, DesktopState>,
) -> Result<CacheDeleteResult, String> {
    begin_operation(&state, DesktopOperation::DeletingCache)?;
    let data_root = state.data_root.clone();
    let running = Arc::clone(&state.running_operation);
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).delete_cache_region(&region_id)
    })
    .await;
    finish_operation(&running);
    join_result.map_err(|error| format!("cache delete worker failed: {error}"))?
}

#[tauri::command]
async fn calculate(
    request: CalculationRequest,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<CalculationResult, String> {
    begin_operation(&state, DesktopOperation::Calculating)?;
    let data_root = state.data_root.clone();
    let cancelled = Arc::clone(&state.cancelled);
    let running = Arc::clone(&state.running_operation);
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).calculate(&request, &cancelled, |progress| {
            let _ = app.emit("calculation-progress", progress);
        })
    })
    .await;
    finish_operation(&running);
    join_result.map_err(|error| format!("calculation worker failed: {error}"))?
}

#[tauri::command]
async fn export_result(
    request: ExportRequest,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ExportResultView, String> {
    begin_operation(&state, DesktopOperation::Exporting)?;
    let running = Arc::clone(&state.running_operation);
    let join_result =
        tauri::async_runtime::spawn_blocking(move || export_result_blocking(&app, request)).await;
    finish_operation(&running);
    join_result.map_err(|error| format!("export worker failed: {error}"))?
}

#[cfg(windows)]
fn export_result_blocking(
    app: &AppHandle,
    request: ExportRequest,
) -> Result<ExportResultView, String> {
    let format = ReportFormat::from(request.format);
    validate_suggested_file_name(&request.suggested_file_name, format)
        .map_err(|error| error.to_string())?;
    let filter_name = match format {
        ReportFormat::Png => "PNG 图像",
        ReportFormat::Pdf => "PDF 报告",
    };
    let selected = app
        .dialog()
        .file()
        .set_title("导出 HamHeatmap 传播预测报告")
        .set_file_name(&request.suggested_file_name)
        .add_filter(filter_name, &[format.extension()])
        .blocking_save_file();
    let Some(selected) = selected else {
        return Ok(ExportResultView {
            cancelled: true,
            path: None,
            bytes_written: 0,
        });
    };
    let selected_path = selected
        .into_path()
        .map_err(|error| format!("cannot resolve selected export path: {error}"))?;
    let destination = path_with_format_extension(&selected_path, format);
    let bytes = encode_report(&request.report_png_data_url, format)
        .map_err(|error| format!("cannot encode export report: {error}"))?;
    let bytes_written = write_report_atomic(&destination, &bytes)
        .map_err(|error| format!("cannot write export report: {error}"))?;
    Ok(ExportResultView {
        cancelled: false,
        path: Some(destination.to_string_lossy().into_owned()),
        bytes_written,
    })
}

#[cfg(not(windows))]
fn export_result_blocking(
    _app: &AppHandle,
    _request: ExportRequest,
) -> Result<ExportResultView, String> {
    Err("file export is supported by the Windows desktop build only".into())
}

#[tauri::command]
fn cancel_calculation(state: State<'_, DesktopState>) {
    state.cancelled.store(true, Ordering::Release);
}

#[tauri::command]
fn cancel_download(state: State<'_, DesktopState>) {
    state.cancelled.store(true, Ordering::Release);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }));
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_dialog::init());
    builder
        .setup(|app| {
            let data_root = app.path().app_local_data_dir().map_err(|error| {
                std::io::Error::other(format!("cannot resolve local app data directory: {error}"))
            })?;
            app.manage(DesktopState {
                data_root,
                cancelled: Arc::new(AtomicBool::new(false)),
                running_operation: Arc::new(Mutex::new(None)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            inspect_point,
            estimate_download,
            download_region,
            cache_overview,
            delete_cache_region,
            calculate,
            export_result,
            cancel_calculation,
            cancel_download
        ])
        .run(tauri::generate_context!())
        .expect("failed to run HamHeatmap desktop application");
}
