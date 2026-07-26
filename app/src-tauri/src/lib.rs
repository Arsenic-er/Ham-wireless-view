use std::path::PathBuf;

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

mod operation_state;
use operation_state::{CancellationTarget, DesktopOperation, DesktopOperationController};
#[cfg(windows)]
use tauri_plugin_dialog::DialogExt;

struct DesktopState {
    data_root: PathBuf,
    operations: DesktopOperationController,
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

#[tauri::command]
async fn bootstrap(state: State<'_, DesktopState>) -> Result<BootstrapInfo, String> {
    let lease = state.operations.begin(DesktopOperation::Bootstrapping)?;
    let data_root = state.data_root.clone();
    let join_result =
        tauri::async_runtime::spawn_blocking(move || AppService::new(data_root).bootstrap()).await;
    let outcome = match join_result {
        Ok(outcome) => outcome,
        Err(error) => Err(format!("bootstrap worker failed: {error}")),
    };
    lease.finish(outcome)
}

#[tauri::command]
async fn inspect_point(
    point: MapPoint,
    state: State<'_, DesktopState>,
) -> Result<PointInspection, String> {
    let lease = state.operations.begin(DesktopOperation::InspectingPoint)?;
    let data_root = state.data_root.clone();
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).inspect_point(point)
    })
    .await;
    let outcome = match join_result {
        Ok(outcome) => outcome,
        Err(error) => Err(format!("point inspection worker failed: {error}")),
    };
    lease.finish(outcome)
}

#[tauri::command]
async fn estimate_download(
    point: MapPoint,
    state: State<'_, DesktopState>,
) -> Result<DownloadEstimate, String> {
    let lease = state
        .operations
        .begin(DesktopOperation::EstimatingDownload)?;
    let data_root = state.data_root.clone();
    let cancelled = lease.cancellation_flag();
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).estimate_download_with_cancel(point, &cancelled)
    })
    .await;
    let outcome = match join_result {
        Ok(outcome) => outcome,
        Err(error) => Err(format!("download estimate worker failed: {error}")),
    };
    lease.finish(outcome)
}

#[tauri::command]
async fn download_region(
    point: MapPoint,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<DownloadResult, String> {
    let lease = state.operations.begin(DesktopOperation::Downloading)?;
    let data_root = state.data_root.clone();
    let cancelled = lease.cancellation_flag();
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).download_region(point, &cancelled, |progress| {
            let _ = app.emit::<DownloadProgressView>("download-progress", progress);
        })
    })
    .await;
    let outcome = match join_result {
        Ok(outcome) => outcome,
        Err(error) => Err(format!("download worker failed: {error}")),
    };
    lease.finish(outcome)
}

#[tauri::command]
async fn cache_overview(state: State<'_, DesktopState>) -> Result<CacheOverview, String> {
    let lease = state.operations.begin(DesktopOperation::ReadingCache)?;
    let data_root = state.data_root.clone();
    let join_result =
        tauri::async_runtime::spawn_blocking(move || AppService::new(data_root).cache_overview())
            .await;
    let outcome = match join_result {
        Ok(outcome) => outcome,
        Err(error) => Err(format!("cache overview worker failed: {error}")),
    };
    lease.finish(outcome)
}

#[tauri::command]
async fn delete_cache_region(
    region_id: String,
    state: State<'_, DesktopState>,
) -> Result<CacheDeleteResult, String> {
    let lease = state.operations.begin(DesktopOperation::DeletingCache)?;
    let data_root = state.data_root.clone();
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).delete_cache_region(&region_id)
    })
    .await;
    let outcome = match join_result {
        Ok(outcome) => outcome,
        Err(error) => Err(format!("cache delete worker failed: {error}")),
    };
    lease.finish(outcome)
}

#[tauri::command]
async fn calculate(
    request: CalculationRequest,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<CalculationResult, String> {
    let lease = state.operations.begin(DesktopOperation::Calculating)?;
    let data_root = state.data_root.clone();
    let cancelled = lease.cancellation_flag();
    let join_result = tauri::async_runtime::spawn_blocking(move || {
        AppService::new(data_root).calculate(&request, &cancelled, |progress| {
            let _ = app.emit("calculation-progress", progress);
        })
    })
    .await;
    let outcome = match join_result {
        Ok(outcome) => outcome,
        Err(error) => Err(format!("calculation worker failed: {error}")),
    };
    lease.finish(outcome)
}

#[tauri::command]
async fn export_result(
    request: ExportRequest,
    app: AppHandle,
    state: State<'_, DesktopState>,
) -> Result<ExportResultView, String> {
    let lease = state.operations.begin(DesktopOperation::Exporting)?;
    let join_result =
        tauri::async_runtime::spawn_blocking(move || export_result_blocking(&app, request)).await;
    let outcome = match join_result {
        Ok(outcome) => outcome,
        Err(error) => Err(format!("export worker failed: {error}")),
    };
    lease.finish(outcome)
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
    let _ = state.operations.cancel(CancellationTarget::Calculation);
}

#[tauri::command]
fn cancel_download(state: State<'_, DesktopState>) {
    let _ = state.operations.cancel(CancellationTarget::Download);
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
                operations: DesktopOperationController::default(),
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
