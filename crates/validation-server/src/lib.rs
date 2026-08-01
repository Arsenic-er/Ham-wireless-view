//! Minimal same-origin HTTP bridge for internal browser validation.

mod basemap;

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use basemap::{Basemap, BasemapError, SATELLITE_TILE_PATH_PREFIX};
use hamheatmap_app_service::{
    AppService, CalculationPreview, CalculationProgress, CalculationRequest, DownloadProgressView,
    MapPoint,
};
use serde::{Deserialize, Serialize};

pub const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:1421";
pub const DEFAULT_REQUEST_BODY_LIMIT: usize = 1_048_576;
const MAX_HEADER_BYTES: usize = 16_384;
const MAX_STATIC_FILE_BYTES: u64 = 32 * 1024 * 1024;
const TICKET_TTL: Duration = Duration::from_secs(60);
const TERMINAL_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_TICKETS: usize = 32;
const MAX_TERMINALS: usize = 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    pub bind_address: String,
    pub dist_dir: PathBuf,
    pub data_root: PathBuf,
    pub basemap_token_file: PathBuf,
    pub request_body_limit: usize,
}

impl ServerConfig {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let current_dir = std::env::current_dir()
            .map_err(|error| format!("cannot read current directory: {error}"))?;
        let mut config = Self {
            bind_address: std::env::var("HAMHEATMAP_VALIDATION_BIND")
                .unwrap_or_else(|_| DEFAULT_BIND_ADDRESS.into()),
            dist_dir: std::env::var_os("HAMHEATMAP_VALIDATION_DIST_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| current_dir.join("app/dist")),
            data_root: std::env::var_os("HAMHEATMAP_VALIDATION_DATA_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| current_dir.join(".runtime/validation-server/data")),
            basemap_token_file: std::env::var_os("HAMHEATMAP_VALIDATION_BASEMAP_TOKEN_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    current_dir.join(".runtime/validation-platform/secrets/tianditu.token")
                }),
            request_body_limit: DEFAULT_REQUEST_BODY_LIMIT,
        };

        let mut args = args.into_iter();
        while let Some(argument) = args.next() {
            let value = match argument.as_str() {
                "--bind"
                | "--dist-dir"
                | "--data-root"
                | "--basemap-token-file"
                | "--request-body-limit" => args
                    .next()
                    .ok_or_else(|| format!("{argument} requires a value"))?,
                "--help" | "-h" => return Err("--help must be handled by main".into()),
                _ => return Err(format!("unknown argument: {argument}")),
            };
            match argument.as_str() {
                "--bind" => config.bind_address = value,
                "--dist-dir" => config.dist_dir = PathBuf::from(value),
                "--data-root" => config.data_root = PathBuf::from(value),
                "--basemap-token-file" => {
                    config.basemap_token_file = PathBuf::from(value);
                }
                "--request-body-limit" => {
                    config.request_body_limit = value
                        .parse::<usize>()
                        .ok()
                        .filter(|limit| *limit > 0)
                        .ok_or_else(|| {
                            "--request-body-limit must be a positive integer".to_string()
                        })?;
                }
                _ => unreachable!(),
            }
        }
        config.listen_address()?;
        Ok(config)
    }

    fn listen_address(&self) -> Result<SocketAddr, String> {
        let address = self
            .bind_address
            .parse::<SocketAddr>()
            .map_err(|error| format!("invalid listen address: {error}"))?;
        if !address.ip().is_loopback() {
            return Err(format!(
                "listen address must be loopback, got {}",
                self.bind_address
            ));
        }
        Ok(address)
    }
}

pub fn help_text() -> &'static str {
    "HamHeatmap internal validation server\n\n\
Usage:\n  hamheatmap-validation-server [OPTIONS]\n\n\
Options:\n  --bind <ADDRESS>              Loopback listen address (default 127.0.0.1:1421)\n  --dist-dir <PATH>             Built frontend directory (default app/dist)\n  --data-root <PATH>            Runtime data directory (default .runtime/validation-server/data)\n  --basemap-token-file <PATH>   Optional TianDiTu token file\n  --request-body-limit <BYTES>  Maximum JSON request body (default 1048576)\n  -h, --help                    Show this help\n\n\
Environment:\n  HAMHEATMAP_VALIDATION_BIND\n  HAMHEATMAP_VALIDATION_DIST_DIR\n  HAMHEATMAP_VALIDATION_DATA_ROOT\n  HAMHEATMAP_VALIDATION_BASEMAP_TOKEN_FILE\n"
}

#[derive(Clone)]
pub struct ValidationServer {
    state: Arc<ServerState>,
}

struct ServerState {
    service: AppService,
    basemap: Basemap,
    dist_dir: PathBuf,
    listen_address: SocketAddr,
    request_body_limit: usize,
    operations: Arc<OperationGate>,
}

impl ValidationServer {
    pub fn new(config: &ServerConfig) -> Result<Self, String> {
        let listen_address = config.listen_address()?;
        Self::new_with_listen_address(config, listen_address)
    }

    fn new_with_listen_address(
        config: &ServerConfig,
        listen_address: SocketAddr,
    ) -> Result<Self, String> {
        fs::create_dir_all(&config.data_root)
            .map_err(|error| format!("cannot create runtime data directory: {error}"))?;
        let data_root = fs::canonicalize(&config.data_root).map_err(|error| {
            format!(
                "cannot open runtime data directory {}: {error}",
                config.data_root.display()
            )
        })?;
        let dist_dir = fs::canonicalize(&config.dist_dir).map_err(|error| {
            format!(
                "cannot open frontend directory {}: {error}",
                config.dist_dir.display()
            )
        })?;
        if !dist_dir.join("index.html").is_file() {
            return Err(format!(
                "frontend directory is missing index.html: {}",
                dist_dir.display()
            ));
        }
        if data_root.starts_with(&dist_dir) || dist_dir.starts_with(&data_root) {
            return Err("frontend and runtime data directories must not overlap".into());
        }
        let basemap = Basemap::load(&config.basemap_token_file)?;
        Ok(Self {
            state: Arc::new(ServerState {
                service: AppService::new(data_root),
                basemap,
                dist_dir,
                listen_address,
                request_body_limit: config.request_body_limit,
                operations: Arc::new(OperationGate::default()),
            }),
        })
    }

    pub fn bind(config: &ServerConfig) -> Result<(Self, TcpListener), String> {
        let requested_address = config.listen_address()?;
        let listener = TcpListener::bind(requested_address)
            .map_err(|error| format!("cannot listen on {}: {error}", config.bind_address))?;
        let listen_address = listener
            .local_addr()
            .map_err(|error| format!("cannot inspect listener address: {error}"))?;
        let server = Self::new_with_listen_address(config, listen_address)?;
        Ok((server, listener))
    }

    pub fn serve(self, listener: TcpListener) -> io::Result<()> {
        for connection in listener.incoming() {
            match connection {
                Ok(stream) => {
                    let server = self.clone();
                    thread::spawn(move || {
                        if let Err(error) = server.serve_stream(stream) {
                            eprintln!("validation connection error: {error}");
                        }
                    });
                }
                Err(error) => eprintln!("validation accept error: {error}"),
            }
        }
        Ok(())
    }

    /// Serves a bounded number of connections for integration tests.
    pub fn serve_n(&self, listener: &TcpListener, count: usize) -> io::Result<()> {
        let mut workers = Vec::with_capacity(count);
        for _ in 0..count {
            let (stream, _) = listener.accept()?;
            let server = self.clone();
            workers.push(thread::spawn(move || server.serve_stream(stream)));
        }
        for worker in workers {
            worker
                .join()
                .map_err(|_| io::Error::other("validation worker panicked"))??;
        }
        Ok(())
    }

    fn serve_stream(&self, mut stream: TcpStream) -> io::Result<()> {
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(60)))?;
        let request = match read_request(&mut stream, self.state.request_body_limit) {
            Ok(request) => request,
            Err(error) => {
                write_response(&mut stream, error.into_response())?;
                return Ok(());
            }
        };
        let response = self.route(request);
        write_response(&mut stream, response)
    }

    fn route(&self, request: Request) -> Response {
        if !host_matches_listen_address(&request.host, self.state.listen_address) {
            return ApiError::bad_request("unexpected Host header for validation listener")
                .into_response();
        }
        let (path, has_query) = match request.target.split_once('?') {
            Some((path, _)) => (path, true),
            None => (request.target.as_str(), false),
        };
        if has_query && (path.starts_with("/api/") || path == "/healthz") {
            return ApiError::bad_request(
                "query strings are not accepted by validation capability APIs",
            )
            .into_response();
        }
        let api_response = match (request.method.as_str(), path) {
            ("GET", "/healthz") => Some(json_response(
                200,
                &HealthResponse {
                    status: "ok",
                    schema_version: 1,
                },
            )),
            ("GET", "/api/bootstrap") => Some(self.bootstrap_response()),
            ("GET", path) if path.starts_with(SATELLITE_TILE_PATH_PREFIX) => {
                Some(self.satellite_basemap_response(path))
            }
            ("GET", path) if path.starts_with("/api/basemap/") => {
                Some(self.tianditu_basemap_response(path))
            }
            ("GET", "/api/cache-overview") => {
                Some(self.with_operation(|_| self.state.service.cache_overview()))
            }
            ("POST", "/api/inspect-point") => {
                Some(self.json_operation(&request, |body: PointBody, _| {
                    self.state.service.inspect_point(body.point)
                }))
            }
            ("POST", "/api/operation-ticket") => Some(self.operation_ticket(&request)),
            ("POST", "/api/operation-status") => Some(self.operation_status(&request)),
            ("POST", "/api/operation-preview") => Some(self.operation_preview(&request)),
            ("POST", "/api/operation-ack") => Some(self.operation_ack(&request)),
            ("POST", "/api/estimate-download") => Some(self.json_ticketed_operation(
                &request,
                TicketKind::EstimateDownload,
                |body: TicketPointBody, cancelled, _| {
                    self.state
                        .service
                        .estimate_download_with_cancel(body.point, cancelled)
                },
            )),
            ("POST", "/api/download-region") => Some(self.json_ticketed_operation(
                &request,
                TicketKind::Download,
                |body: TicketPointBody, cancelled, reporter| {
                    self.state
                        .service
                        .download_region(body.point, cancelled, move |progress| {
                            reporter.report_download(progress)
                        })
                },
            )),
            ("POST", "/api/delete-cache-region") => {
                Some(self.json_operation(&request, |body: DeleteRegionBody, _| {
                    self.state.service.delete_cache_region(&body.region_id)
                }))
            }
            ("POST", "/api/calculate") => Some(self.json_ticketed_operation(
                &request,
                TicketKind::Calculation,
                |body: TicketCalculationBody, cancelled, reporter| {
                    let progress_reporter = reporter.clone();
                    self.state.service.calculate_with_preview(
                        &body.request,
                        cancelled,
                        move |progress| progress_reporter.report_calculation(progress),
                        move |preview| {
                            reporter.report_preview(preview);
                        },
                    )
                },
            )),
            ("POST", "/api/cancel-calculation") => {
                Some(self.cancel_operation(&request, CancelFamily::Calculation))
            }
            ("POST", "/api/cancel-download") => {
                Some(self.cancel_operation(&request, CancelFamily::Download))
            }
            (_, path) if path.starts_with("/api/") || path == "/healthz" => {
                Some(ApiError::method_not_allowed().into_response())
            }
            _ => None,
        };
        if let Some(response) = api_response {
            return response;
        }
        if request.method != "GET" && request.method != "HEAD" {
            return ApiError::method_not_allowed().into_response();
        }
        self.static_response(path, request.method == "HEAD")
            .unwrap_or_else(ApiError::into_response)
    }

    fn bootstrap_response(&self) -> Response {
        self.with_operation(|_| {
            let bootstrap = self.state.service.bootstrap()?;
            let mut payload = serde_json::to_value(bootstrap)
                .map_err(|error| format!("cannot serialize bootstrap response: {error}"))?;
            let object = payload
                .as_object_mut()
                .ok_or_else(|| "bootstrap response is not a JSON object".to_string())?;
            let basemap = serde_json::to_value(self.state.basemap.metadata())
                .map_err(|error| format!("cannot serialize basemap metadata: {error}"))?;
            object.insert("basemap".into(), basemap);
            Ok(payload)
        })
    }

    fn tianditu_basemap_response(&self, path: &str) -> Response {
        match self.state.basemap.fetch_tianditu(path) {
            Ok(tile) => Response {
                status: 200,
                content_type: tile.content_type,
                body: tile.body,
                head_only: false,
                cache_control: "no-store",
            },
            Err(BasemapError::InvalidPath) => ApiError::not_found().into_response(),
            Err(BasemapError::Disabled) => {
                ApiError::unavailable("basemap is disabled").into_response()
            }
            Err(BasemapError::UpstreamUnavailable) => {
                ApiError::bad_gateway("basemap upstream is unavailable").into_response()
            }
            Err(BasemapError::InvalidUpstreamResponse) => {
                ApiError::bad_gateway("basemap upstream returned an invalid tile").into_response()
            }
        }
    }
    fn satellite_basemap_response(&self, path: &str) -> Response {
        match self.state.basemap.fetch_satellite(path) {
            Ok(tile) => Response {
                status: 200,
                content_type: tile.content_type,
                body: tile.body,
                head_only: false,
                cache_control: "no-store",
            },
            Err(BasemapError::InvalidPath) => ApiError::not_found().into_response(),
            Err(BasemapError::Disabled) => {
                ApiError::unavailable("satellite basemap is disabled").into_response()
            }
            Err(BasemapError::UpstreamUnavailable) => {
                ApiError::bad_gateway("satellite upstream is unavailable").into_response()
            }
            Err(BasemapError::InvalidUpstreamResponse) => {
                ApiError::bad_gateway("satellite upstream returned an invalid tile").into_response()
            }
        }
    }
    fn with_operation<T: Serialize>(
        &self,
        operation: impl FnOnce(&AtomicBool) -> Result<T, String>,
    ) -> Response {
        let lease = match self.state.operations.begin_other() {
            Ok(lease) => lease,
            Err(error) => return error.into_response(),
        };
        let outcome = operation(&lease.cancelled);
        match lease.finish(outcome) {
            Ok(Ok(value)) => json_response(200, &value),
            Ok(Err(message)) => ApiError::service(message).into_response(),
            Err(error) => error.into_response(),
        }
    }

    fn json_operation<B: for<'de> Deserialize<'de>, T: Serialize>(
        &self,
        request: &Request,
        operation: impl FnOnce(B, &AtomicBool) -> Result<T, String>,
    ) -> Response {
        let body = match parse_json::<B>(request) {
            Ok(body) => body,
            Err(error) => return error.into_response(),
        };
        self.with_operation(|cancelled| operation(body, cancelled))
    }

    fn json_ticketed_operation<B: TicketedBody, T: Serialize>(
        &self,
        request: &Request,
        kind: TicketKind,
        operation: impl FnOnce(B, &AtomicBool, OperationReporter) -> Result<T, String>,
    ) -> Response {
        let body = match parse_json::<B>(request) {
            Ok(body) => body,
            Err(error) => return error.into_response(),
        };
        let lease = match self
            .state
            .operations
            .begin_ticketed(body.operation_id(), kind)
        {
            Ok(lease) => lease,
            Err(error) => return error.into_response(),
        };
        let reporter = lease.reporter().expect("ticketed lease has a reporter");
        let outcome = operation(body, &lease.cancelled, reporter);
        match lease.finish(outcome) {
            Ok(Ok(value)) => json_response(200, &value),
            Ok(Err(message)) => ApiError::service(message).into_response(),
            Err(error) => error.into_response(),
        }
    }

    fn operation_ticket(&self, request: &Request) -> Response {
        let body = match parse_json::<TicketRequest>(request) {
            Ok(body) => body,
            Err(error) => return error.into_response(),
        };
        match self.state.operations.reserve(body.kind) {
            Ok(ticket) => json_response(200, &ticket),
            Err(error) => error.into_response(),
        }
    }

    fn operation_status(&self, request: &Request) -> Response {
        let body = match parse_json::<OperationIdBody>(request) {
            Ok(body) => body,
            Err(error) => return error.into_response(),
        };
        match self.state.operations.status(&body.operation_id) {
            Ok(status) => json_response(200, &status),
            Err(error) => error.into_response(),
        }
    }

    fn operation_preview(&self, request: &Request) -> Response {
        let body = match parse_json::<OperationPreviewBody>(request) {
            Ok(body) => body,
            Err(error) => return error.into_response(),
        };
        match self
            .state
            .operations
            .preview(&body.operation_id, body.after_sequence)
        {
            Ok(Some(preview)) => json_response(200, &preview),
            Ok(None) => no_content_response(),
            Err(error) => error.into_response(),
        }
    }

    fn operation_ack(&self, request: &Request) -> Response {
        let body = match parse_json::<OperationIdBody>(request) {
            Ok(body) => body,
            Err(error) => return error.into_response(),
        };
        match self.state.operations.ack(&body.operation_id) {
            Ok(acknowledged) => json_response(200, &AckResponse { acknowledged }),
            Err(error) => error.into_response(),
        }
    }

    fn cancel_operation(&self, request: &Request, family: CancelFamily) -> Response {
        let body = match parse_json::<OperationIdBody>(request) {
            Ok(body) => body,
            Err(error) => return error.into_response(),
        };
        match self.state.operations.cancel(&body.operation_id, family) {
            Ok(cancelled) => json_response(200, &CancelResponse { cancelled }),
            Err(error) => error.into_response(),
        }
    }

    fn static_response(&self, url_path: &str, head_only: bool) -> Result<Response, ApiError> {
        let relative = static_relative_path(url_path)?;
        let mime = mime_for_path(&relative).ok_or_else(ApiError::not_found)?;
        let candidate = self.state.dist_dir.join(&relative);
        let canonical = fs::canonicalize(&candidate).map_err(|_| ApiError::not_found())?;
        if !canonical.starts_with(&self.state.dist_dir) || !canonical.is_file() {
            return Err(ApiError::not_found());
        }
        let metadata = canonical.metadata().map_err(|_| ApiError::not_found())?;
        if metadata.len() > MAX_STATIC_FILE_BYTES {
            return Err(ApiError::payload_too_large());
        }
        let body = fs::read(canonical).map_err(|_| ApiError::not_found())?;
        Ok(Response {
            status: 200,
            content_type: mime,
            body,
            head_only,
            cache_control: "no-cache",
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum TicketKind {
    EstimateDownload,
    Download,
    Calculation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CancelFamily {
    Download,
    Calculation,
}

impl TicketKind {
    fn matches_cancel_family(self, family: CancelFamily) -> bool {
        match family {
            CancelFamily::Download => {
                matches!(self, Self::EstimateDownload | Self::Download)
            }
            CancelFamily::Calculation => self == Self::Calculation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum OperationStateLabel {
    Reserved,
    Running,
    CancellationRequested,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type")]
enum OperationProgress {
    #[serde(rename = "estimate-download")]
    EstimateDownload { stage: &'static str },
    #[serde(rename = "download")]
    Download {
        #[serde(rename = "assetIndex")]
        asset_index: usize,
        #[serde(rename = "assetCount")]
        asset_count: usize,
        #[serde(rename = "assetDownloadedBytes")]
        asset_downloaded_bytes: u64,
        #[serde(rename = "assetExpectedBytes")]
        asset_expected_bytes: u64,
        #[serde(rename = "totalDownloadedBytes")]
        total_downloaded_bytes: u64,
        #[serde(rename = "totalExpectedBytes")]
        total_expected_bytes: u64,
        percent: f64,
    },
    #[serde(rename = "calculation")]
    Calculation {
        phase: hamheatmap_app_service::CalculationPhase,
        percent: f64,
        #[serde(rename = "completedPixelCount")]
        completed_pixel_count: usize,
        #[serde(rename = "totalPixelCount")]
        total_pixel_count: usize,
    },
}

impl OperationProgress {
    fn download(value: DownloadProgressView) -> Self {
        Self::Download {
            asset_index: value.asset_index,
            asset_count: value.asset_count,
            asset_downloaded_bytes: value.asset_downloaded_bytes,
            asset_expected_bytes: value.asset_expected_bytes,
            total_downloaded_bytes: value.total_downloaded_bytes,
            total_expected_bytes: value.total_expected_bytes,
            percent: value.percent,
        }
    }

    fn calculation(value: CalculationProgress) -> Self {
        Self::Calculation {
            phase: value.phase,
            percent: value.percent,
            completed_pixel_count: value.completed_pixel_count,
            total_pixel_count: value.total_pixel_count,
        }
    }
}

#[derive(Default)]
struct OperationGate {
    inner: Mutex<OperationStore>,
}

#[derive(Default)]
struct OperationStore {
    tickets: HashMap<String, TicketRecord>,
    active: Option<ActiveOperation>,
    terminals: HashMap<String, TerminalRecord>,
    terminal_order: VecDeque<String>,
    next_generation: u64,
}

struct TicketRecord {
    kind: TicketKind,
    created_at: Instant,
}

struct ActiveOperation {
    operation_id: Option<String>,
    kind: Option<TicketKind>,
    generation: u64,
    cancelled: Arc<AtomicBool>,
    sequence: u64,
    progress: Option<OperationProgress>,
    preview: Option<CalculationPreview>,
}

#[derive(Clone)]
struct TerminalRecord {
    kind: TicketKind,
    state: OperationStateLabel,
    sequence: u64,
    progress: Option<OperationProgress>,
    completed_at: Instant,
}

struct OperationLease {
    gate: Arc<OperationGate>,
    operation_id: Option<String>,
    generation: u64,
    cancelled: Arc<AtomicBool>,
    finished: bool,
}

impl std::fmt::Debug for OperationLease {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationLease")
            .field("operation_id", &self.operation_id)
            .field("generation", &self.generation)
            .field("finished", &self.finished)
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct OperationReporter {
    gate: Arc<OperationGate>,
    operation_id: String,
    generation: u64,
}

impl OperationReporter {
    fn report(&self, progress: OperationProgress) -> bool {
        let Ok(mut inner) = self.gate.inner.lock() else {
            return false;
        };
        let Some(active) = inner.active.as_mut() else {
            return false;
        };
        if active.operation_id.as_deref() != Some(&self.operation_id)
            || active.generation != self.generation
        {
            return false;
        }
        active.sequence = active.sequence.saturating_add(1);
        active.progress = Some(progress);
        true
    }

    fn report_download(&self, progress: DownloadProgressView) {
        self.report(OperationProgress::download(progress));
    }

    fn report_calculation(&self, progress: CalculationProgress) {
        self.report(OperationProgress::calculation(progress));
    }

    fn report_preview(&self, preview: CalculationPreview) -> bool {
        let Ok(mut inner) = self.gate.inner.lock() else {
            return false;
        };
        let Some(active) = inner.active.as_mut() else {
            return false;
        };
        if active.operation_id.as_deref() != Some(&self.operation_id)
            || active.generation != self.generation
            || active.kind != Some(TicketKind::Calculation)
            || active.cancelled.load(Ordering::Acquire)
            || preview.completed_pixel_count > preview.total_pixel_count
            || active
                .preview
                .as_ref()
                .is_some_and(|current| current.sequence >= preview.sequence)
        {
            return false;
        }
        active.preview = Some(preview);
        true
    }
}

impl OperationStore {
    fn prune(&mut self, now: Instant) {
        self.tickets
            .retain(|_, ticket| !has_expired(now, ticket.created_at, TICKET_TTL));
        self.terminals
            .retain(|_, terminal| !has_expired(now, terminal.completed_at, TERMINAL_TTL));
        self.terminal_order
            .retain(|operation_id| self.terminals.contains_key(operation_id));
    }

    fn next_generation(&mut self) -> u64 {
        self.next_generation = self.next_generation.wrapping_add(1).max(1);
        self.next_generation
    }

    fn insert_terminal(&mut self, operation_id: String, terminal: TerminalRecord) {
        while self.terminals.len() >= MAX_TERMINALS {
            let Some(oldest) = self.terminal_order.pop_front() else {
                self.terminals.clear();
                break;
            };
            self.terminals.remove(&oldest);
        }
        self.terminal_order.push_back(operation_id.clone());
        self.terminals.insert(operation_id, terminal);
    }
}

impl OperationGate {
    fn reserve(&self, kind: TicketKind) -> Result<TicketResponse, ApiError> {
        for _ in 0..8 {
            let operation_id = generate_operation_id()?;
            match self.reserve_with_id_at(kind, operation_id, Instant::now()) {
                Ok(ticket) => return Ok(ticket),
                Err(error) if error.status == 409 && error.message == "operation id collision" => {}
                Err(error) => return Err(error),
            }
        }
        Err(ApiError::internal(
            "cannot allocate a unique operation identifier",
        ))
    }

    fn reserve_with_id_at(
        &self,
        kind: TicketKind,
        operation_id: String,
        now: Instant,
    ) -> Result<TicketResponse, ApiError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        inner.prune(now);
        let collides = inner.tickets.contains_key(&operation_id)
            || inner.terminals.contains_key(&operation_id)
            || inner
                .active
                .as_ref()
                .and_then(|active| active.operation_id.as_deref())
                == Some(operation_id.as_str());
        if collides {
            return Err(ApiError::operation_id_collision());
        }
        if inner.tickets.len() >= MAX_TICKETS {
            return Err(ApiError::too_many_tickets());
        }
        inner.tickets.insert(
            operation_id.clone(),
            TicketRecord {
                kind,
                created_at: now,
            },
        );
        Ok(TicketResponse {
            schema_version: 1,
            operation_id,
            kind,
            state: "reserved",
        })
    }

    fn begin_other(self: &Arc<Self>) -> Result<OperationLease, ApiError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        inner.prune(Instant::now());
        if inner.active.is_some() {
            return Err(ApiError::busy());
        }
        let generation = inner.next_generation();
        let cancelled = Arc::new(AtomicBool::new(false));
        inner.active = Some(ActiveOperation {
            operation_id: None,
            kind: None,
            generation,
            cancelled: cancelled.clone(),
            sequence: 0,
            progress: None,
            preview: None,
        });
        Ok(OperationLease {
            gate: self.clone(),
            operation_id: None,
            generation,
            cancelled,
            finished: false,
        })
    }

    fn begin_ticketed(
        self: &Arc<Self>,
        operation_id: &str,
        kind: TicketKind,
    ) -> Result<OperationLease, ApiError> {
        validate_operation_id(operation_id)?;
        self.begin_ticketed_at(operation_id, kind, Instant::now())
    }

    fn begin_ticketed_at(
        self: &Arc<Self>,
        operation_id: &str,
        kind: TicketKind,
        now: Instant,
    ) -> Result<OperationLease, ApiError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        inner.prune(now);
        if inner.active.is_some() {
            return Err(ApiError::busy());
        }
        let ticket = inner
            .tickets
            .get(operation_id)
            .ok_or_else(ApiError::unknown_operation)?;
        if ticket.kind != kind {
            return Err(ApiError::ticket_kind_mismatch());
        }
        inner.tickets.remove(operation_id);
        let generation = inner.next_generation();
        let cancelled = Arc::new(AtomicBool::new(false));
        let progress =
            (kind == TicketKind::EstimateDownload).then_some(OperationProgress::EstimateDownload {
                stage: "estimating",
            });
        inner.active = Some(ActiveOperation {
            operation_id: Some(operation_id.to_owned()),
            kind: Some(kind),
            generation,
            cancelled: cancelled.clone(),
            sequence: 1,
            progress,
            preview: None,
        });
        Ok(OperationLease {
            gate: self.clone(),
            operation_id: Some(operation_id.to_owned()),
            generation,
            cancelled,
            finished: false,
        })
    }

    fn status(&self, operation_id: &str) -> Result<OperationStatusResponse, ApiError> {
        validate_operation_id(operation_id)?;
        self.status_at(operation_id, Instant::now())
    }

    fn status_at(
        &self,
        operation_id: &str,
        now: Instant,
    ) -> Result<OperationStatusResponse, ApiError> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        inner.prune(now);
        if let Some(ticket) = inner.tickets.get(operation_id) {
            return Ok(OperationStatusResponse {
                schema_version: 1,
                operation_id: operation_id.to_owned(),
                kind: ticket.kind,
                state: OperationStateLabel::Reserved,
                sequence: 0,
                progress: None,
            });
        }

        if let Some(active) = inner
            .active
            .as_ref()
            .filter(|active| active.operation_id.as_deref() == Some(operation_id))
        {
            let kind = active
                .kind
                .expect("an identified operation always has a ticket kind");
            let state = if active.cancelled.load(Ordering::Acquire) {
                OperationStateLabel::CancellationRequested
            } else {
                OperationStateLabel::Running
            };
            return Ok(OperationStatusResponse {
                schema_version: 1,
                operation_id: operation_id.to_owned(),
                kind,
                state,
                sequence: active.sequence,
                progress: active.progress.clone(),
            });
        }
        let terminal = inner
            .terminals
            .get(operation_id)
            .ok_or_else(ApiError::unknown_operation)?;
        Ok(OperationStatusResponse {
            schema_version: 1,
            operation_id: operation_id.to_owned(),
            kind: terminal.kind,
            state: terminal.state,
            sequence: terminal.sequence,
            progress: terminal.progress.clone(),
        })
    }

    fn preview(
        &self,
        operation_id: &str,
        after_sequence: u64,
    ) -> Result<Option<CalculationPreview>, ApiError> {
        validate_operation_id(operation_id)?;
        let inner = self
            .inner
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        Ok(inner
            .active
            .as_ref()
            .filter(|active| {
                active.operation_id.as_deref() == Some(operation_id)
                    && active.kind == Some(TicketKind::Calculation)
                    && !active.cancelled.load(Ordering::Acquire)
            })
            .and_then(|active| active.preview.as_ref())
            .filter(|preview| preview.sequence > after_sequence)
            .cloned())
    }

    fn cancel(&self, operation_id: &str, family: CancelFamily) -> Result<bool, ApiError> {
        validate_operation_id(operation_id)?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        inner.prune(Instant::now());
        let Some(active) = inner.active.as_mut() else {
            return Ok(false);
        };
        if active.operation_id.as_deref() != Some(operation_id)
            || !active
                .kind
                .is_some_and(|kind| kind.matches_cancel_family(family))
        {
            return Ok(false);
        }
        if !active.cancelled.swap(true, Ordering::AcqRel) {
            active.sequence = active.sequence.saturating_add(1);
        }
        active.preview = None;
        Ok(true)
    }

    fn ack(&self, operation_id: &str) -> Result<bool, ApiError> {
        validate_operation_id(operation_id)?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        inner.prune(Instant::now());
        if inner
            .active
            .as_ref()
            .is_some_and(|active| active.operation_id.as_deref() == Some(operation_id))
        {
            return Ok(false);
        }
        let acknowledged = inner.tickets.remove(operation_id).is_some()
            | inner.terminals.remove(operation_id).is_some();
        if acknowledged {
            inner
                .terminal_order
                .retain(|candidate| candidate != operation_id);
        }
        Ok(acknowledged)
    }
}

impl OperationLease {
    fn reporter(&self) -> Option<OperationReporter> {
        self.operation_id
            .as_ref()
            .map(|operation_id| OperationReporter {
                gate: self.gate.clone(),
                operation_id: operation_id.clone(),
                generation: self.generation,
            })
    }

    fn finish<T>(self, outcome: Result<T, String>) -> Result<Result<T, String>, ApiError> {
        self.finish_at(outcome, Instant::now())
    }

    fn finish_at<T>(
        mut self,
        outcome: Result<T, String>,
        now: Instant,
    ) -> Result<Result<T, String>, ApiError> {
        let mut inner = self
            .gate
            .inner
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        let Some(active) = inner.active.take() else {
            return Err(ApiError::internal(
                "operation lease lost its active identity",
            ));
        };
        let is_current = active.generation == self.generation
            && active.operation_id == self.operation_id
            && Arc::ptr_eq(&active.cancelled, &self.cancelled);
        if !is_current {
            inner.active = Some(active);
            return Err(ApiError::internal(
                "operation lease lost its active identity",
            ));
        }
        let was_cancelled = active.cancelled.load(Ordering::Acquire);
        if let (Some(operation_id), Some(kind)) = (active.operation_id, active.kind) {
            let state = if was_cancelled {
                OperationStateLabel::Cancelled
            } else if outcome.is_ok() {
                OperationStateLabel::Succeeded
            } else {
                OperationStateLabel::Failed
            };
            inner.prune(now);
            inner.insert_terminal(
                operation_id,
                TerminalRecord {
                    kind,
                    state,
                    sequence: active.sequence.saturating_add(1),
                    progress: active.progress,
                    completed_at: now,
                },
            );
        }
        self.finished = true;
        drop(inner);
        if was_cancelled {
            Ok(Err("operation cancelled".into()))
        } else {
            Ok(outcome)
        }
    }
}

impl Drop for OperationLease {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        let Ok(mut inner) = self.gate.inner.lock() else {
            return;
        };
        let is_current = inner.active.as_ref().is_some_and(|active| {
            active.generation == self.generation
                && active.operation_id == self.operation_id
                && Arc::ptr_eq(&active.cancelled, &self.cancelled)
        });
        if !is_current {
            return;
        }
        let active = inner.active.take().expect("checked active operation");
        if let (Some(operation_id), Some(kind)) = (active.operation_id, active.kind) {
            let now = Instant::now();
            inner.prune(now);
            inner.insert_terminal(
                operation_id,
                TerminalRecord {
                    kind,
                    state: OperationStateLabel::Failed,
                    sequence: active.sequence.saturating_add(1),
                    progress: active.progress,
                    completed_at: now,
                },
            );
        }
    }
}

fn has_expired(now: Instant, created_at: Instant, ttl: Duration) -> bool {
    now.checked_duration_since(created_at)
        .is_some_and(|elapsed| elapsed >= ttl)
}

fn generate_operation_id() -> Result<String, ApiError> {
    let mut bytes = [0_u8; 16];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| ApiError::internal(format!("cannot generate operation id: {error}")))?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Ok(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    ))
}

fn validate_operation_id(operation_id: &str) -> Result<(), ApiError> {
    let bytes = operation_id.as_bytes();
    let valid_shape = bytes.len() == 36
        && bytes[8] == b'-'
        && bytes[13] == b'-'
        && bytes[18] == b'-'
        && bytes[23] == b'-'
        && bytes[14] == b'4'
        && matches!(bytes[19], b'8' | b'9' | b'a' | b'b')
        && bytes.iter().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                *byte == b'-'
            } else {
                byte.is_ascii_digit() || matches!(*byte, b'a'..=b'f')
            }
        });
    if valid_shape {
        Ok(())
    } else {
        Err(ApiError::bad_request(
            "operationId must be a canonical lowercase UUIDv4",
        ))
    }
}

fn parse_json<B: for<'de> Deserialize<'de>>(request: &Request) -> Result<B, ApiError> {
    require_json_content_type(request)?;
    serde_json::from_slice::<B>(&request.body)
        .map_err(|error| ApiError::bad_request(format!("invalid JSON request: {error}")))
}

trait TicketedBody: for<'de> Deserialize<'de> {
    fn operation_id(&self) -> &str;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PointBody {
    point: MapPoint,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TicketPointBody {
    operation_id: String,
    point: MapPoint,
}

impl TicketedBody for TicketPointBody {
    fn operation_id(&self) -> &str {
        &self.operation_id
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteRegionBody {
    region_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TicketCalculationBody {
    operation_id: String,
    request: CalculationRequest,
}

impl TicketedBody for TicketCalculationBody {
    fn operation_id(&self) -> &str {
        &self.operation_id
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TicketRequest {
    kind: TicketKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationIdBody {
    operation_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OperationPreviewBody {
    operation_id: String,
    after_sequence: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TicketResponse {
    schema_version: u32,
    operation_id: String,
    kind: TicketKind,
    state: &'static str,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct OperationStatusResponse {
    schema_version: u32,
    operation_id: String,
    kind: TicketKind,
    state: OperationStateLabel,
    sequence: u64,
    progress: Option<OperationProgress>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HealthResponse {
    status: &'static str,
    schema_version: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CancelResponse {
    cancelled: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AckResponse {
    acknowledged: bool,
}

#[derive(Debug)]
struct Request {
    method: String,
    target: String,
    host: String,
    content_type: Option<String>,
    body: Vec<u8>,
}

fn read_request(stream: &mut impl Read, body_limit: usize) -> Result<Request, ApiError> {
    let mut bytes = Vec::with_capacity(2048);
    let header_end = loop {
        if let Some(position) = find_bytes(&bytes, b"\r\n\r\n") {
            break position + 4;
        }
        if bytes.len() >= MAX_HEADER_BYTES {
            return Err(ApiError::bad_request("HTTP headers are too large"));
        }
        let mut chunk = [0_u8; 2048];
        let read = stream
            .read(&mut chunk)
            .map_err(|error| ApiError::bad_request(format!("cannot read HTTP request: {error}")))?;
        if read == 0 {
            return Err(ApiError::bad_request("incomplete HTTP request"));
        }
        bytes.extend_from_slice(&chunk[..read]);
    };
    if header_end > MAX_HEADER_BYTES {
        return Err(ApiError::bad_request("HTTP headers are too large"));
    }
    let header = std::str::from_utf8(&bytes[..header_end - 4])
        .map_err(|_| ApiError::bad_request("HTTP headers are not valid UTF-8"))?
        .to_owned();
    let mut lines = header.split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| ApiError::bad_request("missing HTTP request line"))?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| ApiError::bad_request("missing HTTP method"))?;
    let target = parts
        .next()
        .ok_or_else(|| ApiError::bad_request("missing HTTP target"))?;
    let version = parts
        .next()
        .ok_or_else(|| ApiError::bad_request("missing HTTP version"))?;
    if parts.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(ApiError::bad_request("invalid HTTP request line"));
    }
    if !target.starts_with('/') {
        return Err(ApiError::bad_request(
            "only origin-form HTTP targets are accepted",
        ));
    }

    let mut content_length = None;
    let mut content_type = None;
    let mut host = None;
    for line in lines {
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| ApiError::bad_request("malformed HTTP header"))?;
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        match name.as_str() {
            "host" => {
                if value.is_empty() {
                    return Err(ApiError::bad_request("Host header must not be empty"));
                }
                if host.replace(value.to_string()).is_some() {
                    return Err(ApiError::bad_request("duplicate Host header"));
                }
            }
            "content-length" => {
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| ApiError::bad_request("invalid Content-Length"))?;
                if content_length.replace(parsed).is_some() {
                    return Err(ApiError::bad_request("duplicate Content-Length"));
                }
            }
            "content-type" => content_type = Some(value.to_string()),
            "transfer-encoding" => {
                return Err(ApiError::bad_request("Transfer-Encoding is not supported"));
            }
            _ => {}
        }
    }
    let host = host.ok_or_else(|| ApiError::bad_request("missing Host header"))?;
    let content_length = content_length.unwrap_or(0);
    if content_length > body_limit {
        return Err(ApiError::payload_too_large());
    }
    let required = header_end
        .checked_add(content_length)
        .ok_or_else(ApiError::payload_too_large)?;
    while bytes.len() < required {
        let mut chunk = [0_u8; 8192];
        let read = stream.read(&mut chunk).map_err(|error| {
            ApiError::bad_request(format!("cannot read HTTP request body: {error}"))
        })?;
        if read == 0 {
            return Err(ApiError::bad_request("incomplete HTTP request body"));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > required.saturating_add(8192) {
            return Err(ApiError::bad_request(
                "HTTP request body exceeds declared length",
            ));
        }
    }
    Ok(Request {
        method: method.to_string(),
        target: target.to_string(),
        host,
        content_type,
        body: bytes[header_end..required].to_vec(),
    })
}

fn host_matches_listen_address(host: &str, listen_address: SocketAddr) -> bool {
    if let Ok(address) = host.parse::<SocketAddr>() {
        return address == listen_address;
    }
    let Some((name, port)) = host.rsplit_once(':') else {
        return false;
    };
    name.eq_ignore_ascii_case("localhost")
        && port.parse::<u16>().ok() == Some(listen_address.port())
}

fn require_json_content_type(request: &Request) -> Result<(), ApiError> {
    let Some(content_type) = request.content_type.as_deref() else {
        return Err(ApiError::unsupported_media_type());
    };
    if !content_type
        .split(';')
        .next()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    {
        return Err(ApiError::unsupported_media_type());
    }
    Ok(())
}

fn static_relative_path(url_path: &str) -> Result<PathBuf, ApiError> {
    if url_path.contains('%') || url_path.contains('\\') || url_path.contains('\0') {
        return Err(ApiError::not_found());
    }
    if url_path == "/" {
        return Ok(PathBuf::from("index.html"));
    }
    let relative = Path::new(url_path.trim_start_matches('/'));
    if relative.as_os_str().is_empty() {
        return Err(ApiError::not_found());
    }
    for component in relative.components() {
        let Component::Normal(value) = component else {
            return Err(ApiError::not_found());
        };
        let value = value.to_str().ok_or_else(ApiError::not_found)?;
        if value.is_empty()
            || !value
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "._-".contains(character))
        {
            return Err(ApiError::not_found());
        }
    }
    Ok(relative.to_path_buf())
}

fn mime_for_path(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "html" => Some("text/html; charset=utf-8"),
        "css" => Some("text/css; charset=utf-8"),
        "js" | "mjs" => Some("text/javascript; charset=utf-8"),
        "json" | "map" => Some("application/json; charset=utf-8"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "svg" => Some("image/svg+xml"),
        "ico" => Some("image/x-icon"),
        "webp" => Some("image/webp"),
        "woff" => Some("font/woff"),
        "woff2" => Some("font/woff2"),
        "webmanifest" => Some("application/manifest+json"),
        _ => None,
    }
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

struct Response {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    head_only: bool,
    cache_control: &'static str,
}

fn json_response(value: u16, payload: &impl Serialize) -> Response {
    match serde_json::to_vec(payload) {
        Ok(body) => Response {
            status: value,
            content_type: "application/json; charset=utf-8",
            body,
            head_only: false,
            cache_control: "no-store",
        },
        Err(error) => {
            ApiError::internal(format!("cannot serialize JSON response: {error}")).into_response()
        }
    }
}

fn no_content_response() -> Response {
    Response {
        status: 204,
        content_type: "application/json; charset=utf-8",
        body: Vec::new(),
        head_only: true,
        cache_control: "no-store",
    }
}

fn write_response(stream: &mut impl Write, response: Response) -> io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Internal Server Error",
    };
    let content_length = response.body.len();
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: {}\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' data: blob:; worker-src 'self' blob:; child-src 'self' blob:\r\n",
        response.status, reason, response.content_type, content_length, response.cache_control,
    )?;
    write!(stream, "\r\n")?;
    if !response.head_only {
        stream.write_all(&response.body)?;
    }
    stream.flush()
}

#[derive(Serialize)]
struct ErrorPayload<'a> {
    message: &'a str,
}

#[derive(Debug)]
struct ApiError {
    status: u16,
    message: String,
}

impl ApiError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: 400,
            message: message.into(),
        }
    }

    fn not_found() -> Self {
        Self {
            status: 404,
            message: "resource not found".into(),
        }
    }

    fn unknown_operation() -> Self {
        Self {
            status: 404,
            message: "operation not found".into(),
        }
    }

    fn ticket_kind_mismatch() -> Self {
        Self {
            status: 400,
            message: "operation ticket kind does not match this endpoint".into(),
        }
    }

    fn method_not_allowed() -> Self {
        Self {
            status: 405,
            message: "HTTP method not allowed".into(),
        }
    }

    fn busy() -> Self {
        Self {
            status: 409,
            message: "another validation operation is already running".into(),
        }
    }

    fn too_many_tickets() -> Self {
        Self {
            status: 409,
            message: "too many reserved operation tickets".into(),
        }
    }

    fn operation_id_collision() -> Self {
        Self {
            status: 409,
            message: "operation id collision".into(),
        }
    }

    fn payload_too_large() -> Self {
        Self {
            status: 413,
            message: "request or static asset is too large".into(),
        }
    }

    fn unsupported_media_type() -> Self {
        Self {
            status: 415,
            message: "JSON API requests require application/json".into(),
        }
    }

    fn bad_gateway(message: impl Into<String>) -> Self {
        Self {
            status: 502,
            message: message.into(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: 503,
            message: message.into(),
        }
    }

    fn service(message: impl Into<String>) -> Self {
        Self {
            status: 422,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: 500,
            message: message.into(),
        }
    }

    fn into_response(self) -> Response {
        json_response(
            self.status,
            &ErrorPayload {
                message: &self.message,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_override_defaults_and_validate_bind_address() {
        let config = ServerConfig::from_args([
            "--bind".into(),
            "127.0.0.1:0".into(),
            "--dist-dir".into(),
            "web".into(),
            "--data-root".into(),
            "runtime".into(),
            "--basemap-token-file".into(),
            "runtime/tianditu.token".into(),
            "--request-body-limit".into(),
            "2048".into(),
        ])
        .unwrap();
        assert_eq!(config.bind_address, "127.0.0.1:0");
        assert_eq!(config.dist_dir, PathBuf::from("web"));
        assert_eq!(config.data_root, PathBuf::from("runtime"));
        assert_eq!(
            config.basemap_token_file,
            PathBuf::from("runtime/tianditu.token")
        );
        assert_eq!(config.request_body_limit, 2048);
        assert!(ServerConfig::from_args(["--bind".into(), "bad".into()]).is_err());
        assert!(ServerConfig::from_args(["--bind".into(), "[::1]:0".into()]).is_ok());
        for address in ["0.0.0.0:1421", "[::]:1421", "192.0.2.1:1421"] {
            let error = ServerConfig::from_args(["--bind".into(), address.into()]).unwrap_err();
            assert!(error.contains("must be loopback"), "{address}: {error}");
        }
    }

    #[test]
    fn static_paths_and_mime_types_are_fail_closed() {
        assert_eq!(
            static_relative_path("/").unwrap(),
            PathBuf::from("index.html")
        );
        assert_eq!(
            static_relative_path("/assets/index-a1.js").unwrap(),
            PathBuf::from("assets/index-a1.js")
        );
        for unsafe_path in ["/../secret", "/%2e%2e/secret", "/a\\b.js", "/bad name.js"] {
            assert!(static_relative_path(unsafe_path).is_err(), "{unsafe_path}");
        }
        assert_eq!(
            mime_for_path(Path::new("x.js")),
            Some("text/javascript; charset=utf-8")
        );
        assert_eq!(mime_for_path(Path::new("x.exe")), None);
    }

    #[test]
    fn request_parser_enforces_body_limit_host_and_json_metadata() {
        let mut valid = b"POST /api/inspect-point HTTP/1.1\r\nHost: 127.0.0.1:1421\r\nContent-Type: application/json\r\nContent-Length: 30\r\n\r\n{\"point\":{\"lat\":30,\"lon\":103}}".as_slice();
        let request = read_request(&mut valid, 1024).unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.host, "127.0.0.1:1421");
        assert!(require_json_content_type(&request).is_ok());

        let mut too_large =
            b"POST / HTTP/1.1\r\nHost: localhost:1421\r\nContent-Length: 20\r\n\r\n".as_slice();
        assert_eq!(read_request(&mut too_large, 10).unwrap_err().status, 413);

        let mut missing_host = b"GET /healthz HTTP/1.1\r\n\r\n".as_slice();
        let missing_error = read_request(&mut missing_host, 1024).unwrap_err();
        assert_eq!(missing_error.status, 400);
        assert!(missing_error.message.contains("missing Host"));

        let mut duplicate_host =
            b"GET /healthz HTTP/1.1\r\nHost: localhost:1421\r\nHost: 127.0.0.1:1421\r\n\r\n"
                .as_slice();
        let duplicate_error = read_request(&mut duplicate_host, 1024).unwrap_err();
        assert_eq!(duplicate_error.status, 400);
        assert!(duplicate_error.message.contains("duplicate Host"));
    }

    #[test]
    fn host_matching_is_exact_for_ipv4_localhost_and_ipv6() {
        let ipv4 = "127.0.0.1:1421".parse().unwrap();
        assert!(host_matches_listen_address("127.0.0.1:1421", ipv4));
        assert!(host_matches_listen_address("LOCALHOST:1421", ipv4));
        assert!(!host_matches_listen_address("localhost:1422", ipv4));
        assert!(!host_matches_listen_address("attacker.example:1421", ipv4));
        assert!(!host_matches_listen_address("127.0.0.2:1421", ipv4));

        let ipv6 = "[::1]:1421".parse().unwrap();
        assert!(host_matches_listen_address("[::1]:1421", ipv6));
        assert!(host_matches_listen_address("localhost:1421", ipv6));
        assert!(!host_matches_listen_address("[::1]:1422", ipv6));
        assert!(!host_matches_listen_address("127.0.0.1:1421", ipv6));
    }

    fn test_operation_id(value: u64) -> String {
        format!("00000000-0000-4000-8000-{value:012x}")
    }

    fn test_preview(sequence: u64) -> CalculationPreview {
        CalculationPreview {
            schema_version: 1,
            sequence,
            completed_pixel_count: sequence * 100,
            total_pixel_count: 125_628,
            map_overlay_projection: "EPSG:3857",
            map_overlay_width: 401,
            map_overlay_height: 401,
            map_overlay_corners: [[102.0, 31.0], [104.0, 31.0], [104.0, 29.0], [102.0, 29.0]],
            map_overlay_png_data_url: format!("data:image/png;base64,preview-{sequence}"),
        }
    }

    #[test]
    fn generated_operation_ids_are_lowercase_uuid_v4() {
        for _ in 0..32 {
            let operation_id = generate_operation_id().unwrap();
            assert_eq!(operation_id.len(), 36);
            assert_eq!(&operation_id[8..9], "-");
            assert_eq!(&operation_id[13..14], "-");
            assert_eq!(&operation_id[18..19], "-");
            assert_eq!(&operation_id[23..24], "-");
            assert_eq!(&operation_id[14..15], "4");
            assert!("89ab".contains(&operation_id[19..20]));
            assert!(
                operation_id
                    .chars()
                    .all(|character| character == '-' || character.is_ascii_hexdigit())
            );
            assert_eq!(operation_id, operation_id.to_ascii_lowercase());
        }
    }

    #[test]
    fn inbound_operation_ids_require_canonical_lowercase_uuid_v4() {
        assert!(validate_operation_id("00000000-0000-4000-8000-000000000001").is_ok());
        for invalid in [
            "",
            "00000000-0000-4000-8000-00000000001",
            "00000000-0000-4000-8000-000000000001x",
            "000000000000-4000-8000-000000000001",
            "00000000-0000-5000-8000-000000000001",
            "00000000-0000-4000-7000-000000000001",
            "00000000-0000-4000-C000-000000000001",
            "00000000-0000-4000-8000-00000000000g",
        ] {
            let error = validate_operation_id(invalid).unwrap_err();
            assert_eq!(error.status, 400, "{invalid}");
        }
    }

    #[test]
    fn tickets_are_single_use_kind_bound_and_not_consumed_while_busy() {
        let gate = Arc::new(OperationGate::default());
        let calculation_id = test_operation_id(1);
        let download_id = test_operation_id(2);
        gate.reserve_with_id_at(
            TicketKind::Calculation,
            calculation_id.clone(),
            Instant::now(),
        )
        .unwrap();
        let collision = gate
            .reserve_with_id_at(TicketKind::Download, calculation_id.clone(), Instant::now())
            .unwrap_err();
        assert_eq!(collision.status, 409);
        gate.reserve_with_id_at(TicketKind::Download, download_id.clone(), Instant::now())
            .unwrap();

        let reserved = gate.status(&calculation_id).unwrap();
        assert_eq!(reserved.state, OperationStateLabel::Reserved);
        assert_eq!(reserved.sequence, 0);
        assert_eq!(reserved.progress, None);
        let wrong_kind = gate
            .begin_ticketed(&calculation_id, TicketKind::Download)
            .unwrap_err();
        assert_eq!(wrong_kind.status, 400);
        let calculation = gate
            .begin_ticketed(&calculation_id, TicketKind::Calculation)
            .unwrap();
        let busy = gate
            .begin_ticketed(&download_id, TicketKind::Download)
            .unwrap_err();
        assert_eq!(busy.status, 409);
        assert_eq!(calculation.finish(Ok(())).unwrap(), Ok(()));
        assert_eq!(
            gate.begin_ticketed(&calculation_id, TicketKind::Calculation)
                .unwrap_err()
                .status,
            404
        );
        let download = gate
            .begin_ticketed(&download_id, TicketKind::Download)
            .unwrap();
        assert_eq!(download.finish(Ok(())).unwrap(), Ok(()));
    }

    #[test]
    fn cancellation_is_exact_id_family_scoped_and_linearized_with_finish() {
        let gate = Arc::new(OperationGate::default());
        let first_id = test_operation_id(10);
        gate.reserve_with_id_at(TicketKind::Calculation, first_id.clone(), Instant::now())
            .unwrap();
        let cancelled_lease = gate
            .begin_ticketed(&first_id, TicketKind::Calculation)
            .unwrap();
        assert!(!gate.cancel(&first_id, CancelFamily::Download).unwrap());
        assert!(
            !gate
                .cancel(&test_operation_id(999), CancelFamily::Calculation)
                .unwrap()
        );
        assert!(gate.cancel(&first_id, CancelFamily::Calculation).unwrap());
        assert!(gate.cancel(&first_id, CancelFamily::Calculation).unwrap());
        let cancelling = gate.status(&first_id).unwrap();
        assert_eq!(cancelling.state, OperationStateLabel::CancellationRequested);
        assert_eq!(cancelling.sequence, 2);
        let cancelled_outcome = cancelled_lease.finish(Ok(42)).unwrap();
        assert_eq!(cancelled_outcome.unwrap_err(), "operation cancelled");
        assert_eq!(
            gate.status(&first_id).unwrap().state,
            OperationStateLabel::Cancelled
        );

        let second_id = test_operation_id(11);
        gate.reserve_with_id_at(TicketKind::Calculation, second_id.clone(), Instant::now())
            .unwrap();
        let completed_lease = gate
            .begin_ticketed(&second_id, TicketKind::Calculation)
            .unwrap();
        assert!(!gate.cancel(&first_id, CancelFamily::Calculation).unwrap());
        assert!(!completed_lease.cancelled.load(Ordering::Acquire));
        assert_eq!(completed_lease.finish(Ok(42)).unwrap(), Ok(42));
        assert!(!gate.cancel(&second_id, CancelFamily::Calculation).unwrap());
        assert_eq!(
            gate.status(&second_id).unwrap().state,
            OperationStateLabel::Succeeded
        );
    }

    #[test]
    fn progress_is_whitelisted_sequenced_terminal_and_acknowledgeable() {
        let gate = Arc::new(OperationGate::default());
        let operation_id = test_operation_id(20);
        gate.reserve_with_id_at(TicketKind::Download, operation_id.clone(), Instant::now())
            .unwrap();
        let lease = gate
            .begin_ticketed(&operation_id, TicketKind::Download)
            .unwrap();
        let reporter = lease.reporter().unwrap();
        reporter.report_download(DownloadProgressView {
            asset_index: 2,
            asset_count: 5,
            asset_key: "must-not-leak".into(),
            asset_downloaded_bytes: 10,
            asset_expected_bytes: 20,
            total_downloaded_bytes: 30,
            total_expected_bytes: 100,
            percent: 30.0,
        });
        let running = gate.status(&operation_id).unwrap();
        assert_eq!(running.state, OperationStateLabel::Running);
        assert_eq!(running.sequence, 2);
        let running_json = serde_json::to_value(&running).unwrap();
        assert_eq!(running_json["progress"]["type"], "download");
        assert!(running_json.to_string().find("assetKey").is_none());
        assert!(running_json.to_string().find("must-not-leak").is_none());

        assert_eq!(
            lease.finish::<()>(Err("private detail".into())).unwrap(),
            Err("private detail".into())
        );
        let terminal = gate.status(&operation_id).unwrap();
        assert_eq!(terminal.state, OperationStateLabel::Failed);
        assert_eq!(terminal.sequence, 3);
        let terminal_json = serde_json::to_string(&terminal).unwrap();
        assert!(!terminal_json.contains("private detail"));
        assert!(gate.ack(&operation_id).unwrap());
        assert!(!gate.ack(&operation_id).unwrap());
        assert_eq!(gate.status(&operation_id).unwrap_err().status, 404);
    }

    #[test]
    fn preview_is_latest_only_exact_and_cleared_on_cancel_and_finish() {
        let gate = Arc::new(OperationGate::default());
        let operation_id = test_operation_id(40);
        let unknown_id = test_operation_id(41);
        assert_eq!(gate.preview(&unknown_id, 0).unwrap(), None);

        gate.reserve_with_id_at(
            TicketKind::Calculation,
            operation_id.clone(),
            Instant::now(),
        )
        .unwrap();
        assert_eq!(gate.preview(&operation_id, 0).unwrap(), None);
        let lease = gate
            .begin_ticketed(&operation_id, TicketKind::Calculation)
            .unwrap();
        let reporter = lease.reporter().unwrap();
        assert!(reporter.report_preview(test_preview(1)));
        assert_eq!(gate.preview(&operation_id, 1).unwrap(), None);
        assert_eq!(gate.preview(&operation_id, 0).unwrap().unwrap().sequence, 1);
        assert!(reporter.report_preview(test_preview(3)));
        assert!(!reporter.report_preview(test_preview(2)));
        let latest = gate.preview(&operation_id, 1).unwrap().unwrap();
        assert_eq!(latest.sequence, 3);
        assert!(latest.map_overlay_png_data_url.ends_with("preview-3"));

        let status_json = serde_json::to_string(&gate.status(&operation_id).unwrap()).unwrap();
        assert!(!status_json.contains("mapOverlay"));
        assert!(!status_json.contains("preview-3"));

        assert!(
            gate.cancel(&operation_id, CancelFamily::Calculation)
                .unwrap()
        );
        assert_eq!(gate.preview(&operation_id, 0).unwrap(), None);
        assert!(!reporter.report_preview(test_preview(4)));
        assert!(lease.finish::<()>(Ok(())).unwrap().is_err());
        assert_eq!(gate.preview(&operation_id, 0).unwrap(), None);

        let finished_id = test_operation_id(42);
        gate.reserve_with_id_at(TicketKind::Calculation, finished_id.clone(), Instant::now())
            .unwrap();
        let finished = gate
            .begin_ticketed(&finished_id, TicketKind::Calculation)
            .unwrap();
        assert!(finished.reporter().unwrap().report_preview(test_preview(1)));
        finished.finish(Ok(())).unwrap().unwrap();
        assert_eq!(gate.preview(&finished_id, 0).unwrap(), None);

        let failed_id = test_operation_id(43);
        gate.reserve_with_id_at(TicketKind::Calculation, failed_id.clone(), Instant::now())
            .unwrap();
        let failed = gate
            .begin_ticketed(&failed_id, TicketKind::Calculation)
            .unwrap();
        assert!(failed.reporter().unwrap().report_preview(test_preview(1)));
        assert!(failed.finish::<()>(Err("failure".into())).unwrap().is_err());
        assert_eq!(gate.preview(&failed_id, 0).unwrap(), None);

        let dropped_id = test_operation_id(44);
        gate.reserve_with_id_at(TicketKind::Calculation, dropped_id.clone(), Instant::now())
            .unwrap();
        let dropped = gate
            .begin_ticketed(&dropped_id, TicketKind::Calculation)
            .unwrap();
        assert!(dropped.reporter().unwrap().report_preview(test_preview(1)));
        drop(dropped);
        assert_eq!(gate.preview(&dropped_id, 0).unwrap(), None);
    }

    #[test]
    fn stale_reporter_cannot_mutate_the_next_operation() {
        let gate = Arc::new(OperationGate::default());
        let first_id = test_operation_id(21);
        gate.reserve_with_id_at(TicketKind::Download, first_id.clone(), Instant::now())
            .unwrap();
        let first_lease = gate
            .begin_ticketed(&first_id, TicketKind::Download)
            .unwrap();
        let stale_reporter = first_lease.reporter().unwrap();
        first_lease.finish(Ok(())).unwrap().unwrap();

        let second_id = test_operation_id(22);
        gate.reserve_with_id_at(TicketKind::Calculation, second_id.clone(), Instant::now())
            .unwrap();
        let second_lease = gate
            .begin_ticketed(&second_id, TicketKind::Calculation)
            .unwrap();
        let before = gate.status(&second_id).unwrap();
        assert_eq!(before.sequence, 1);
        assert_eq!(before.progress, None);

        assert!(!stale_reporter.report(OperationProgress::EstimateDownload {
            stage: "estimating",
        }));
        let after = gate.status(&second_id).unwrap();
        assert_eq!(after.sequence, before.sequence);
        assert_eq!(after.progress, before.progress);
        assert!(!second_lease.cancelled.load(Ordering::Acquire));
        second_lease.finish(Ok(())).unwrap().unwrap();
    }

    #[test]
    fn ticket_and_terminal_ttl_limits_and_drop_failure_are_enforced() {
        let gate = Arc::new(OperationGate::default());
        let now = Instant::now();
        let expired_ticket = test_operation_id(30);
        gate.reserve_with_id_at(TicketKind::Download, expired_ticket.clone(), now)
            .unwrap();
        assert_eq!(
            gate.begin_ticketed_at(&expired_ticket, TicketKind::Download, now + TICKET_TTL)
                .unwrap_err()
                .status,
            404
        );

        for value in 100..100 + MAX_TICKETS as u64 {
            gate.reserve_with_id_at(TicketKind::Download, test_operation_id(value), now)
                .unwrap();
        }
        assert_eq!(
            gate.reserve_with_id_at(TicketKind::Download, test_operation_id(999), now)
                .unwrap_err()
                .status,
            409
        );
        for value in 100..100 + MAX_TICKETS as u64 {
            assert!(gate.ack(&test_operation_id(value)).unwrap());
        }

        for value in 200..=200 + MAX_TERMINALS as u64 {
            let operation_id = test_operation_id(value);
            gate.reserve_with_id_at(TicketKind::Calculation, operation_id.clone(), now)
                .unwrap();
            let lease = gate
                .begin_ticketed_at(&operation_id, TicketKind::Calculation, now)
                .unwrap();
            lease.finish_at(Ok(()), now).unwrap().unwrap();
        }
        assert_eq!(
            gate.status(&test_operation_id(200)).unwrap_err().status,
            404
        );
        let newest = test_operation_id(200 + MAX_TERMINALS as u64);
        assert_eq!(
            gate.status_at(&newest, now + TERMINAL_TTL)
                .unwrap_err()
                .status,
            404
        );

        let dropped_id = test_operation_id(500);
        gate.reserve_with_id_at(TicketKind::Download, dropped_id.clone(), Instant::now())
            .unwrap();
        let dropped = gate
            .begin_ticketed(&dropped_id, TicketKind::Download)
            .unwrap();
        drop(dropped);
        assert_eq!(
            gate.status(&dropped_id).unwrap().state,
            OperationStateLabel::Failed
        );
        assert!(gate.begin_other().is_ok());
    }

    static NEXT_TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    struct ServerFixture {
        root: PathBuf,
        server: ValidationServer,
    }

    impl ServerFixture {
        fn new() -> Self {
            let unique = NEXT_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir().join(format!(
                "hamheatmap-validation-server-test-{}-{nanos}-{unique}",
                std::process::id()
            ));
            let dist_dir = root.join("dist");
            fs::create_dir_all(dist_dir.join("assets")).unwrap();
            fs::write(
                dist_dir.join("index.html"),
                b"<!doctype html><title>test</title>",
            )
            .unwrap();
            fs::write(dist_dir.join("assets/app.js"), b"console.log('test');").unwrap();
            fs::write(dist_dir.join("assets/tool.exe"), b"not executable").unwrap();
            let config = ServerConfig {
                bind_address: "127.0.0.1:0".into(),
                dist_dir,
                data_root: root.join("data"),
                basemap_token_file: root.join("missing-tianditu.token"),
                request_body_limit: 1024,
            };
            let server = ValidationServer::new(&config).unwrap();
            Self { root, server }
        }

        fn request(
            &self,
            method: &str,
            target: &str,
            content_type: Option<&str>,
            body: &[u8],
        ) -> Response {
            self.request_with_host("localhost:0", method, target, content_type, body)
        }

        fn request_with_host(
            &self,
            host: &str,
            method: &str,
            target: &str,
            content_type: Option<&str>,
            body: &[u8],
        ) -> Response {
            self.server.route(Request {
                method: method.into(),
                target: target.into(),
                host: host.into(),
                content_type: content_type.map(str::to_owned),
                body: body.to_vec(),
            })
        }
    }

    impl Drop for ServerFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn response_json(response: &Response) -> serde_json::Value {
        assert_eq!(response.content_type, "application/json; charset=utf-8");
        serde_json::from_slice(&response.body).unwrap()
    }

    #[test]
    fn configuration_rejects_frontend_and_runtime_directory_overlap() {
        let fixture = ServerFixture::new();
        let dist_dir = fixture.root.join("overlap-dist");
        fs::create_dir_all(&dist_dir).unwrap();
        fs::write(dist_dir.join("index.html"), b"test").unwrap();
        let config = ServerConfig {
            bind_address: "127.0.0.1:0".into(),
            data_root: dist_dir.join("runtime"),
            dist_dir,
            basemap_token_file: fixture.root.join("missing-tianditu.token"),
            request_body_limit: 1024,
        };
        let error = match ValidationServer::new(&config) {
            Ok(_) => panic!("overlapping frontend and runtime directories were accepted"),
            Err(error) => error,
        };
        assert!(error.contains("must not overlap"));
    }

    #[test]
    fn preview_http_endpoint_is_exact_latest_only_and_fail_closed() {
        let fixture = ServerFixture::new();
        let operation_id = test_operation_id(600);
        let unknown_id = test_operation_id(601);
        fixture
            .server
            .state
            .operations
            .reserve_with_id_at(
                TicketKind::Calculation,
                operation_id.clone(),
                Instant::now(),
            )
            .unwrap();
        let lease = fixture
            .server
            .state
            .operations
            .begin_ticketed(&operation_id, TicketKind::Calculation)
            .unwrap();
        let request_body = serde_json::to_vec(&serde_json::json!({
            "operationId": operation_id,
            "afterSequence": 0
        }))
        .unwrap();
        let empty = fixture.request(
            "POST",
            "/api/operation-preview",
            Some("application/json"),
            &request_body,
        );
        assert_eq!(empty.status, 204);
        assert!(empty.body.is_empty());

        assert!(lease.reporter().unwrap().report_preview(test_preview(7)));
        let available = fixture.request(
            "POST",
            "/api/operation-preview",
            Some("application/json"),
            &request_body,
        );
        assert_eq!(available.status, 200);
        let preview_json = response_json(&available);
        assert_eq!(preview_json["sequence"], 7);
        assert_eq!(preview_json["completedPixelCount"], 700);
        assert_eq!(preview_json["mapOverlayProjection"], "EPSG:3857");
        assert_eq!(
            preview_json["mapOverlayPngDataUrl"],
            "data:image/png;base64,preview-7"
        );

        let caught_up_body = serde_json::to_vec(&serde_json::json!({
            "operationId": operation_id,
            "afterSequence": 7
        }))
        .unwrap();
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/operation-preview",
                    Some("application/json"),
                    &caught_up_body,
                )
                .status,
            204
        );
        let unknown_body = serde_json::to_vec(&serde_json::json!({
            "operationId": unknown_id,
            "afterSequence": 0
        }))
        .unwrap();
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/operation-preview",
                    Some("application/json"),
                    &unknown_body,
                )
                .status,
            204
        );
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/operation-preview",
                    Some("text/plain"),
                    &request_body
                )
                .status,
            415
        );
        let unknown_field = br#"{"operationId":"00000000-0000-4000-8000-000000000600","afterSequence":0,"extra":true}"#;
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/operation-preview",
                    Some("application/json"),
                    unknown_field,
                )
                .status,
            400
        );
        lease.finish(Ok(())).unwrap().unwrap();
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/operation-preview",
                    Some("application/json"),
                    &request_body,
                )
                .status,
            204
        );
    }

    #[test]
    fn static_and_api_routes_are_fail_closed() {
        let fixture = ServerFixture::new();

        let health = fixture.request("GET", "/healthz", None, b"");
        assert_eq!(health.status, 200);
        assert_eq!(response_json(&health)["status"], "ok");

        let index = fixture.request("GET", "/", None, b"");
        assert_eq!(index.status, 200);
        assert_eq!(index.content_type, "text/html; charset=utf-8");
        assert_eq!(index.cache_control, "no-cache");
        assert!(index.body.starts_with(b"<!doctype html>"));

        let script = fixture.request("GET", "/assets/app.js", None, b"");
        assert_eq!(script.status, 200);
        assert_eq!(script.content_type, "text/javascript; charset=utf-8");

        for target in [
            "/../secret",
            "/%2e%2e/secret",
            "/assets/tool.exe",
            "/missing.js",
        ] {
            assert_eq!(fixture.request("GET", target, None, b"").status, 404);
        }
        assert_eq!(
            fixture
                .request("GET", "/api/basemap/tianditu/vec/5/25/12", None, b"")
                .status,
            503
        );
        assert_eq!(
            fixture
                .request("GET", "/api/basemap/tianditu/evil/5/25/12", None, b"")
                .status,
            404
        );
        assert_eq!(
            fixture
                .request("GET", "/api/basemap/tianditu/vec/5/32/12", None, b"")
                .status,
            404
        );
        assert_eq!(
            fixture
                .request(
                    "GET",
                    "/api/basemap/tianditu/vec/5/25/12?tk=evil",
                    None,
                    b""
                )
                .status,
            400
        );
        assert_eq!(
            fixture
                .request("POST", "/api/basemap/tianditu/vec/5/25/12", None, b"")
                .status,
            405
        );
        assert_eq!(
            fixture
                .request("GET", "/api/basemap/satellite/15/0/0", None, b"")
                .status,
            404
        );
        assert_eq!(
            fixture
                .request("GET", "/api/basemap/satellite/2/0/0?source=evil", None, b"")
                .status,
            400
        );
        assert_eq!(
            fixture
                .request("POST", "/api/basemap/satellite/2/0/0", None, b"")
                .status,
            405
        );
        assert_eq!(fixture.request("POST", "/", None, b"").status, 405);
        assert_eq!(
            fixture
                .request("POST", "/api/export-result", None, b"")
                .status,
            405
        );
    }

    #[test]
    fn api_contract_uses_wrapped_camel_case_json_and_denies_bad_media() {
        let fixture = ServerFixture::new();

        let bootstrap = fixture.request("GET", "/api/bootstrap", None, b"");
        assert_eq!(bootstrap.status, 200);
        let bootstrap_json = response_json(&bootstrap);
        assert_eq!(bootstrap_json["coverageRadiusKm"], 200);
        assert_eq!(bootstrap_json["gridSize"], 401);
        assert_eq!(bootstrap_json["basemap"]["enabled"], false);
        assert_eq!(bootstrap_json["basemap"]["providerId"], "tianditu");
        assert_eq!(bootstrap_json["basemap"]["displayName"], "天地图");
        assert_eq!(bootstrap_json["basemap"]["attribution"], "天地图");
        assert_eq!(bootstrap_json["basemap"]["mode"], "same-origin-proxy");
        assert_eq!(bootstrap_json["basemap"]["maxZoom"], 18);
        assert_eq!(
            bootstrap_json["basemap"]["tilePathTemplate"],
            "/api/basemap/tianditu/{layer}/{z}/{x}/{y}"
        );
        assert_eq!(bootstrap_json["basemap"]["layers"][0]["id"], "vec");
        assert_eq!(bootstrap_json["basemap"]["layers"][1]["id"], "cva");
        assert_eq!(
            bootstrap_json["basemap"]["satellite"]["providerId"],
            "eoxcloudless"
        );
        assert_eq!(
            bootstrap_json["basemap"]["satellite"]["mode"],
            "same-origin-proxy"
        );
        assert_eq!(bootstrap_json["basemap"]["satellite"]["maxZoom"], 14);
        assert_eq!(
            bootstrap_json["basemap"]["satellite"]["tilePathTemplate"],
            "/api/basemap/satellite/{z}/{x}/{y}"
        );
        let encoded_bootstrap = String::from_utf8(bootstrap.body.clone()).unwrap();
        assert!(!encoded_bootstrap.contains("token"));
        assert!(!encoded_bootstrap.contains("t0.tianditu.gov.cn"));
        assert!(!encoded_bootstrap.contains("tiles.maps.eox.at"));

        let point_body = br#"{"point":{"lat":30.5,"lon":103.5}}"#;
        let inspection = fixture.request(
            "POST",
            "/api/inspect-point",
            Some("application/json; charset=utf-8"),
            point_body,
        );
        assert_eq!(inspection.status, 200);
        let inspection_json = response_json(&inspection);
        assert_eq!(inspection_json["point"]["lat"], 30.5);
        assert_eq!(inspection_json["dataReady"], false);
        assert!(inspection_json["missingAssetCount"].as_u64().unwrap() > 0);

        assert_eq!(
            fixture
                .request("POST", "/api/inspect-point", Some("text/plain"), point_body)
                .status,
            415
        );
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/inspect-point",
                    Some("application/json"),
                    br#"{"point":{"lat":30.5,"lon":103.5},"unexpected":true}"#,
                )
                .status,
            400
        );

        let cancel = fixture.request(
            "POST",
            "/api/cancel-calculation",
            Some("application/json"),
            br#"{"operationId":"00000000-0000-4000-8000-000000000001"}"#,
        );
        assert_eq!(cancel.status, 200);
        assert_eq!(response_json(&cancel)["cancelled"], false);
        assert_eq!(
            fixture
                .request("POST", "/api/cancel-calculation", None, b"")
                .status,
            415
        );
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/cancel-download",
                    Some("application/x-www-form-urlencoded"),
                    b"",
                )
                .status,
            415
        );
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/cancel-download",
                    Some("application/json"),
                    b"not-empty",
                )
                .status,
            400
        );
    }

    #[test]
    fn operation_http_contract_is_strict_identity_bound_and_acknowledgeable() {
        fn issue_ticket(fixture: &ServerFixture, kind: &str) -> String {
            let body = format!(r#"{{"kind":"{kind}"}}"#);
            let response = fixture.request(
                "POST",
                "/api/operation-ticket",
                Some("application/json"),
                body.as_bytes(),
            );
            assert_eq!(response.status, 200);
            let payload = response_json(&response);
            assert_eq!(payload["schemaVersion"], 1);
            assert_eq!(payload["kind"], kind);
            assert_eq!(payload["state"], "reserved");
            let operation_id = payload["operationId"].as_str().unwrap().to_owned();
            validate_operation_id(&operation_id).unwrap();
            operation_id
        }

        fn id_body(operation_id: &str) -> Vec<u8> {
            format!(r#"{{"operationId":"{operation_id}"}}"#).into_bytes()
        }

        let fixture = ServerFixture::new();
        let reserved_id = issue_ticket(&fixture, "download");
        let reserved_status = fixture.request(
            "POST",
            "/api/operation-status",
            Some("application/json"),
            &id_body(&reserved_id),
        );
        assert_eq!(reserved_status.status, 200);
        let reserved_json = response_json(&reserved_status);
        assert_eq!(reserved_json["operationId"], reserved_id);
        assert_eq!(reserved_json["kind"], "download");
        assert_eq!(reserved_json["state"], "reserved");
        assert_eq!(reserved_json["sequence"], 0);
        assert!(reserved_json["progress"].is_null());

        let query_status = fixture.request(
            "POST",
            "/api/operation-status?operationId=attacker-controlled",
            Some("application/json"),
            &id_body(&reserved_id),
        );
        assert_eq!(query_status.status, 400);
        let query_ack = fixture.request(
            "POST",
            "/api/operation-ack?operationId=attacker-controlled",
            Some("application/json"),
            &id_body(&reserved_id),
        );
        assert_eq!(query_ack.status, 400);
        assert_eq!(
            fixture.request("GET", "/healthz?probe=1", None, b"").status,
            400
        );
        let still_reserved = fixture.request(
            "POST",
            "/api/operation-status",
            Some("application/json"),
            &id_body(&reserved_id),
        );
        assert_eq!(still_reserved.status, 200);
        assert_eq!(response_json(&still_reserved)["state"], "reserved");

        let ack = fixture.request(
            "POST",
            "/api/operation-ack",
            Some("application/json"),
            &id_body(&reserved_id),
        );
        assert_eq!(ack.status, 200);
        assert_eq!(response_json(&ack)["acknowledged"], true);
        let second_ack = fixture.request(
            "POST",
            "/api/operation-ack",
            Some("application/json"),
            &id_body(&reserved_id),
        );
        assert_eq!(response_json(&second_ack)["acknowledged"], false);
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/operation-status",
                    Some("application/json"),
                    &id_body(&reserved_id),
                )
                .status,
            404
        );

        let wrong_kind_id = issue_ticket(&fixture, "calculation");
        let wrong_kind_body =
            format!(r#"{{"operationId":"{wrong_kind_id}","point":{{"lat":30.5,"lon":103.5}}}}"#);
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/estimate-download",
                    Some("application/json"),
                    wrong_kind_body.as_bytes(),
                )
                .status,
            400
        );
        let preserved = fixture.request(
            "POST",
            "/api/operation-status",
            Some("application/json"),
            &id_body(&wrong_kind_id),
        );
        assert_eq!(response_json(&preserved)["state"], "reserved");

        let consumed_id = issue_ticket(&fixture, "estimate-download");
        let invalid_point_body =
            format!(r#"{{"operationId":"{consumed_id}","point":{{"lat":999.0,"lon":103.5}}}}"#);
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/estimate-download",
                    Some("application/json"),
                    invalid_point_body.as_bytes(),
                )
                .status,
            422
        );
        let failed = fixture.request(
            "POST",
            "/api/operation-status",
            Some("application/json"),
            &id_body(&consumed_id),
        );
        assert_eq!(response_json(&failed)["state"], "failed");
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/estimate-download",
                    Some("application/json"),
                    invalid_point_body.as_bytes(),
                )
                .status,
            404
        );

        let active_id = test_operation_id(700);
        fixture
            .server
            .state
            .operations
            .reserve_with_id_at(
                TicketKind::EstimateDownload,
                active_id.clone(),
                Instant::now(),
            )
            .unwrap();
        let lease = fixture
            .server
            .state
            .operations
            .begin_ticketed(&active_id, TicketKind::EstimateDownload)
            .unwrap();
        let cancel = fixture.request(
            "POST",
            "/api/cancel-download",
            Some("application/json"),
            &id_body(&active_id),
        );
        assert_eq!(response_json(&cancel)["cancelled"], true);
        let repeated_cancel = fixture.request(
            "POST",
            "/api/cancel-download",
            Some("application/json"),
            &id_body(&active_id),
        );
        assert_eq!(response_json(&repeated_cancel)["cancelled"], true);
        let wrong_cancel = fixture.request(
            "POST",
            "/api/cancel-calculation",
            Some("application/json"),
            &id_body(&active_id),
        );
        assert_eq!(response_json(&wrong_cancel)["cancelled"], false);
        let active_ack = fixture.request(
            "POST",
            "/api/operation-ack",
            Some("application/json"),
            &id_body(&active_id),
        );
        assert_eq!(response_json(&active_ack)["acknowledged"], false);
        let cancelling = fixture.request(
            "POST",
            "/api/operation-status",
            Some("application/json"),
            &id_body(&active_id),
        );
        assert_eq!(
            response_json(&cancelling)["state"],
            "cancellation-requested"
        );
        assert_eq!(
            lease.finish(Ok(())).unwrap(),
            Err("operation cancelled".into())
        );

        for endpoint in [
            "/api/operation-ticket",
            "/api/operation-status",
            "/api/cancel-download",
            "/api/operation-ack",
        ] {
            assert_eq!(
                fixture
                    .request("POST", endpoint, Some("text/plain"), b"{}")
                    .status,
                415,
                "{endpoint}"
            );
        }
        for (endpoint, body) in [
            (
                "/api/operation-ticket",
                br#"{"kind":"download","extra":true}"#.as_slice(),
            ),
            (
                "/api/operation-status",
                br#"{"operationId":"00000000-0000-4000-8000-000000000001","extra":true}"#
                    .as_slice(),
            ),
            (
                "/api/cancel-download",
                br#"{"operationId":"00000000-0000-4000-8000-000000000001","extra":true}"#
                    .as_slice(),
            ),
            (
                "/api/operation-ack",
                br#"{"operationId":"00000000-0000-4000-8000-000000000001","extra":true}"#
                    .as_slice(),
            ),
        ] {
            assert_eq!(
                fixture
                    .request("POST", endpoint, Some("application/json"), body)
                    .status,
                400,
                "{endpoint}"
            );
        }

        let invalid_id_body = br#"{"operationId":"00000000-0000-5000-8000-000000000001"}"#;
        for endpoint in [
            "/api/operation-status",
            "/api/cancel-calculation",
            "/api/cancel-download",
            "/api/operation-ack",
        ] {
            assert_eq!(
                fixture
                    .request("POST", endpoint, Some("application/json"), invalid_id_body,)
                    .status,
                400,
                "{endpoint}"
            );
        }
        assert_eq!(
            fixture
                .request(
                    "POST",
                    "/api/estimate-download",
                    Some("application/json"),
                    br#"{"operationId":"BAD","point":{"lat":30.5,"lon":103.5}}"#,
                )
                .status,
            400
        );
    }

    #[test]
    fn loopback_http_enforces_host_and_has_security_headers_and_head_semantics() {
        fn send(address: SocketAddr, request: &str) -> String {
            let mut stream = TcpStream::connect(address).unwrap();
            stream.write_all(request.as_bytes()).unwrap();
            stream.shutdown(std::net::Shutdown::Write).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response
        }

        let fixture = ServerFixture::new();
        let config = ServerConfig {
            bind_address: "127.0.0.1:0".into(),
            dist_dir: fixture.root.join("dist"),
            data_root: fixture.root.join("http-data"),
            basemap_token_file: fixture.root.join("missing-tianditu.token"),
            request_body_limit: 1024,
        };
        let (server, listener) = ValidationServer::bind(&config).unwrap();
        let address = listener.local_addr().unwrap();
        let worker = thread::spawn(move || server.serve_n(&listener, 6));

        let response = send(
            address,
            &format!("HEAD / HTTP/1.1\r\nHost: {address}\r\n\r\n"),
        );
        let localhost = send(
            address,
            &format!(
                "GET /healthz HTTP/1.1\r\nHost: localhost:{}\r\n\r\n",
                address.port()
            ),
        );
        let missing = send(address, "GET /healthz HTTP/1.1\r\n\r\n");
        let duplicate = send(
            address,
            &format!(
                "GET /healthz HTTP/1.1\r\nHost: localhost:{}\r\nHost: {address}\r\n\r\n",
                address.port()
            ),
        );
        let unexpected = send(
            address,
            &format!(
                "GET /healthz HTTP/1.1\r\nHost: attacker.example:{}\r\n\r\n",
                address.port()
            ),
        );
        let wrong_port = send(
            address,
            &format!(
                "GET /healthz HTTP/1.1\r\nHost: localhost:{}\r\n\r\n",
                address.port().wrapping_add(1)
            ),
        );
        worker.join().unwrap().unwrap();

        assert!(response.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(response.contains("X-Content-Type-Options: nosniff\r\n"));
        assert!(response.contains("Referrer-Policy: no-referrer\r\n"));
        assert!(response.contains("Content-Security-Policy: default-src 'self';"));
        assert!(response.contains("connect-src 'self' data: blob:;"));
        assert!(!response.contains("connect-src *"));
        assert!(!response.contains("connect-src http:"));
        assert!(!response.contains("connect-src https:"));
        assert!(response.ends_with("\r\n\r\n"));
        assert!(!response.contains("<!doctype html>"));
        assert!(localhost.starts_with("HTTP/1.1 200 OK\r\n"));
        for rejected in [missing, duplicate, unexpected, wrong_port] {
            assert!(rejected.starts_with("HTTP/1.1 400 Bad Request\r\n"));
        }
    }

    #[test]
    fn tauri_csp_allows_data_heatmaps_without_broad_network_sources() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../../../app/src-tauri/tauri.conf.json")).unwrap();
        let security = &config["app"]["security"];
        for key in ["csp", "devCsp"] {
            let csp = security[key].as_str().unwrap();
            let connect_sources = csp
                .split(';')
                .map(str::trim)
                .find_map(|directive| directive.strip_prefix("connect-src "))
                .unwrap_or_else(|| panic!("missing connect-src in {key}"))
                .split_whitespace()
                .collect::<Vec<_>>();
            assert!(connect_sources.contains(&"data:"), "missing data: in {key}");
            assert!(connect_sources.contains(&"blob:"), "missing blob: in {key}");
            assert!(!connect_sources.contains(&"*"), "wildcard source in {key}");
            assert!(
                !connect_sources.contains(&"http:"),
                "broad http: source in {key}"
            );
            assert!(
                !connect_sources.contains(&"https:"),
                "broad https: source in {key}"
            );
        }
    }
}
