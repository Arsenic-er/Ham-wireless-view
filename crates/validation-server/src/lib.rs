//! Minimal same-origin HTTP bridge for internal browser validation.

use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hamheatmap_app_service::{AppService, CalculationRequest, MapPoint};
use serde::{Deserialize, Serialize};

pub const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:1421";
pub const DEFAULT_REQUEST_BODY_LIMIT: usize = 1_048_576;
const MAX_HEADER_BYTES: usize = 16_384;
const MAX_STATIC_FILE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    pub bind_address: String,
    pub dist_dir: PathBuf,
    pub data_root: PathBuf,
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
            request_body_limit: DEFAULT_REQUEST_BODY_LIMIT,
        };

        let mut args = args.into_iter();
        while let Some(argument) = args.next() {
            let value = match argument.as_str() {
                "--bind" | "--dist-dir" | "--data-root" | "--request-body-limit" => args
                    .next()
                    .ok_or_else(|| format!("{argument} requires a value"))?,
                "--help" | "-h" => return Err("--help must be handled by main".into()),
                _ => return Err(format!("unknown argument: {argument}")),
            };
            match argument.as_str() {
                "--bind" => config.bind_address = value,
                "--dist-dir" => config.dist_dir = PathBuf::from(value),
                "--data-root" => config.data_root = PathBuf::from(value),
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
Options:\n  --bind <ADDRESS>              Loopback listen address (default 127.0.0.1:1421)\n  --dist-dir <PATH>             Built frontend directory (default app/dist)\n  --data-root <PATH>            Runtime data directory (default .runtime/validation-server/data)\n  --request-body-limit <BYTES>  Maximum JSON request body (default 1048576)\n  -h, --help                    Show this help\n\n\
Environment:\n  HAMHEATMAP_VALIDATION_BIND\n  HAMHEATMAP_VALIDATION_DIST_DIR\n  HAMHEATMAP_VALIDATION_DATA_ROOT\n"
}

#[derive(Clone)]
pub struct ValidationServer {
    state: Arc<ServerState>,
}

struct ServerState {
    service: AppService,
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
        Ok(Self {
            state: Arc::new(ServerState {
                service: AppService::new(data_root),
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
        let path = request.target.split('?').next().unwrap_or(&request.target);
        let api_response = match (request.method.as_str(), path) {
            ("GET", "/healthz") => Some(json_response(
                200,
                &HealthResponse {
                    status: "ok",
                    schema_version: 1,
                },
            )),
            ("GET", "/api/bootstrap") => {
                Some(self.with_operation(OperationKind::Other, |_| self.state.service.bootstrap()))
            }
            ("GET", "/api/cache-overview") => {
                Some(self.with_operation(OperationKind::Other, |_| {
                    self.state.service.cache_overview()
                }))
            }
            ("POST", "/api/inspect-point") => Some(self.json_operation(
                &request,
                OperationKind::Other,
                |body: PointBody, _| self.state.service.inspect_point(body.point),
            )),
            ("POST", "/api/estimate-download") => Some(self.json_operation(
                &request,
                OperationKind::Download,
                |body: PointBody, cancelled| {
                    self.state
                        .service
                        .estimate_download_with_cancel(body.point, cancelled)
                },
            )),
            ("POST", "/api/download-region") => Some(self.json_operation(
                &request,
                OperationKind::Download,
                |body: PointBody, cancelled| {
                    self.state
                        .service
                        .download_region(body.point, cancelled, |_| {})
                },
            )),
            ("POST", "/api/delete-cache-region") => Some(self.json_operation(
                &request,
                OperationKind::Other,
                |body: DeleteRegionBody, _| self.state.service.delete_cache_region(&body.region_id),
            )),
            ("POST", "/api/calculate") => Some(self.json_operation(
                &request,
                OperationKind::Calculation,
                |body: CalculationBody, cancelled| {
                    self.state
                        .service
                        .calculate(&body.request, cancelled, |_| {})
                },
            )),
            ("POST", "/api/cancel-calculation") => {
                Some(self.cancel_operation(&request, OperationKind::Calculation))
            }
            ("POST", "/api/cancel-download") => {
                Some(self.cancel_operation(&request, OperationKind::Download))
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

    fn with_operation<T: Serialize>(
        &self,
        kind: OperationKind,
        operation: impl FnOnce(&AtomicBool) -> Result<T, String>,
    ) -> Response {
        let lease = match self.state.operations.begin(kind) {
            Ok(lease) => lease,
            Err(error) => return error.into_response(),
        };
        match operation(&lease.cancelled) {
            Ok(value) => json_response(200, &value),
            Err(message) => ApiError::service(message).into_response(),
        }
    }

    fn json_operation<B: for<'de> Deserialize<'de>, T: Serialize>(
        &self,
        request: &Request,
        kind: OperationKind,
        operation: impl FnOnce(B, &AtomicBool) -> Result<T, String>,
    ) -> Response {
        if let Err(error) = require_json_content_type(request) {
            return error.into_response();
        }
        let body = match serde_json::from_slice::<B>(&request.body) {
            Ok(body) => body,
            Err(error) => {
                return ApiError::bad_request(format!("invalid JSON request: {error}"))
                    .into_response();
            }
        };
        self.with_operation(kind, |cancelled| operation(body, cancelled))
    }

    fn cancel_operation(&self, request: &Request, kind: OperationKind) -> Response {
        if let Err(error) = require_json_content_type(request) {
            return error.into_response();
        }
        if !request.body.is_empty() {
            return ApiError::bad_request("cancellation requests must not have a body")
                .into_response();
        }
        json_response(
            200,
            &CancelResponse {
                cancelled: self.state.operations.cancel(kind),
            },
        )
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationKind {
    Other,
    Download,
    Calculation,
}

#[derive(Default)]
struct OperationGate {
    active: Mutex<Option<ActiveOperation>>,
}

struct ActiveOperation {
    kind: OperationKind,
    cancelled: Arc<AtomicBool>,
}

struct OperationLease {
    gate: Arc<OperationGate>,
    cancelled: Arc<AtomicBool>,
}

impl OperationGate {
    fn begin(self: &Arc<Self>, kind: OperationKind) -> Result<OperationLease, ApiError> {
        let mut active = self
            .active
            .lock()
            .map_err(|_| ApiError::internal("operation gate is poisoned"))?;
        if active.is_some() {
            return Err(ApiError::busy());
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        *active = Some(ActiveOperation {
            kind,
            cancelled: cancelled.clone(),
        });
        Ok(OperationLease {
            gate: self.clone(),
            cancelled,
        })
    }

    fn cancel(&self, kind: OperationKind) -> bool {
        let Ok(active) = self.active.lock() else {
            return false;
        };
        let Some(operation) = active.as_ref() else {
            return false;
        };
        if operation.kind != kind {
            return false;
        }
        operation.cancelled.store(true, Ordering::Release);
        true
    }
}

impl Drop for OperationLease {
    fn drop(&mut self) {
        if let Ok(mut active) = self.gate.active.lock() {
            *active = None;
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PointBody {
    point: MapPoint,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeleteRegionBody {
    region_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CalculationBody {
    request: CalculationRequest,
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

fn write_response(stream: &mut impl Write, response: Response) -> io::Result<()> {
    let reason = match response.status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        _ => "Internal Server Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: {}\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nContent-Security-Policy: default-src 'self'; img-src 'self' data: blob:; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src 'self' data: blob:; worker-src 'self' blob:; child-src 'self' blob:\r\n\r\n",
        response.status,
        reason,
        response.content_type,
        response.body.len(),
        response.cache_control,
    )?;
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
            "--request-body-limit".into(),
            "2048".into(),
        ])
        .unwrap();
        assert_eq!(config.bind_address, "127.0.0.1:0");
        assert_eq!(config.dist_dir, PathBuf::from("web"));
        assert_eq!(config.data_root, PathBuf::from("runtime"));
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

    #[test]
    fn operation_gate_is_exclusive_and_cancellable_by_kind() {
        let gate = Arc::new(OperationGate::default());
        let lease = gate.begin(OperationKind::Calculation).unwrap();
        let busy = match gate.begin(OperationKind::Other) {
            Ok(_) => panic!("operation gate unexpectedly allowed concurrent work"),
            Err(error) => error,
        };
        assert_eq!(busy.status, 409);
        assert!(!gate.cancel(OperationKind::Download));
        assert!(gate.cancel(OperationKind::Calculation));
        assert!(lease.cancelled.load(Ordering::Acquire));
        drop(lease);
        assert!(gate.begin(OperationKind::Other).is_ok());
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
            request_body_limit: 1024,
        };
        let error = match ValidationServer::new(&config) {
            Ok(_) => panic!("overlapping frontend and runtime directories were accepted"),
            Err(error) => error,
        };
        assert!(error.contains("must not overlap"));
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
            b"",
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
