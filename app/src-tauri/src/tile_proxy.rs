// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;
use std::{path::Path, sync::RwLock, time::Duration};
use tauri::http::{Method, Request, Response, StatusCode, header};
use zeroize::Zeroizing;

pub(crate) const TILE_PROTOCOL_SCHEME: &str = "tianditu";
const TILE_HOST: &str = "t0.tianditu.gov.cn";
const TOKEN_FILE_NAME: &str = "tianditu-token.dpapi";
const MIN_ZOOM: u8 = 1;
const MAX_ZOOM: u8 = 18;
const MAX_TILE_BYTES: usize = 2 * 1024 * 1024;
const BASEMAP_PROBE_SCHEMA_VERSION: u8 = 1;
const PROBE_LAYER: &str = "vec";
const PROBE_ZOOM: u8 = 8;
const PROBE_COLUMN: u32 = 215;
const PROBE_ROW: u32 = 106;

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BasemapInfo {
    configured: bool,
    provider: &'static str,
    protocol_scheme: &'static str,
    vector_template: &'static str,
    vector_label_template: &'static str,
    imagery_template: &'static str,
    imagery_label_template: &'static str,
    attribution: &'static str,
    min_zoom: u8,
    max_zoom: u8,
}
impl BasemapInfo {
    fn new(configured: bool) -> Self {
        Self {
            configured,
            provider: "Tianditu",
            protocol_scheme: TILE_PROTOCOL_SCHEME,
            vector_template: "tianditu://localhost/vec/{z}/{x}/{y}",
            vector_label_template: "tianditu://localhost/cva/{z}/{x}/{y}",
            imagery_template: "tianditu://localhost/img/{z}/{x}/{y}",
            imagery_label_template: "tianditu://localhost/cia/{z}/{x}/{y}",
            attribution: "天地图",
            min_zoom: MIN_ZOOM,
            max_zoom: MAX_ZOOM,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum BasemapProbeStatus {
    Reachable,
    NotConfigured,
    Network,
    Timeout,
    UpstreamOrCredential,
    InvalidContent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BasemapProbeResult {
    schema_version: u8,
    status: BasemapProbeStatus,
}

impl BasemapProbeResult {
    fn new(status: BasemapProbeStatus) -> Self {
        Self {
            schema_version: BASEMAP_PROBE_SCHEMA_VERSION,
            status,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TileRequest<'a> {
    layer: &'a str,
    z: u8,
    x: u32,
    y: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TileFetchFailure {
    Network,
    Timeout,
    UpstreamOrCredential,
    InvalidContent,
    PayloadTooLarge,
}

pub(crate) struct TileProxy {
    token: RwLock<Option<Zeroizing<String>>>,
    vault: TokenVault,
    http: ureq::Agent,
}
impl TileProxy {
    pub(crate) fn new(data_root: &Path) -> Self {
        let vault = TokenVault::new(data_root);
        let token = vault.load().unwrap_or(None);
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(12)))
            .timeout_connect(Some(Duration::from_secs(4)))
            .user_agent("HamHeatmap/0.1 TiandituTileProxy")
            .build();
        Self {
            token: RwLock::new(token),
            vault,
            http: config.into(),
        }
    }
    pub(crate) fn info(&self) -> Result<BasemapInfo, String> {
        Ok(BasemapInfo::new(
            self.token
                .read()
                .map_err(|_| "online basemap credential state is unavailable".to_string())?
                .is_some(),
        ))
    }
    pub(crate) fn configure(&self, token: String) -> Result<BasemapInfo, String> {
        let token = Zeroizing::new(token);
        validate_token(&token)?;
        self.vault.save(token.as_bytes())?;
        *self
            .token
            .write()
            .map_err(|_| "online basemap credential state is unavailable".to_string())? =
            Some(token);
        Ok(BasemapInfo::new(true))
    }
    pub(crate) fn clear(&self) -> Result<BasemapInfo, String> {
        self.vault.clear()?;
        *self
            .token
            .write()
            .map_err(|_| "online basemap credential state is unavailable".to_string())? = None;
        Ok(BasemapInfo::new(false))
    }
    pub(crate) fn probe(&self) -> Result<BasemapProbeResult, String> {
        let token = self
            .token
            .read()
            .map_err(|_| "online basemap credential state is unavailable".to_string())?
            .as_ref()
            .cloned();
        let Some(token) = token else {
            return Ok(BasemapProbeResult::new(
                BasemapProbeStatus::NotConfigured,
            ));
        };
        let tile = TileRequest {
            layer: PROBE_LAYER,
            z: PROBE_ZOOM,
            x: PROBE_COLUMN,
            y: PROBE_ROW,
        };
        let status = match self.fetch_tile_checked(tile, &token) {
            Ok(_) => BasemapProbeStatus::Reachable,
            Err(error) => error.probe_status(),
        };
        Ok(BasemapProbeResult::new(status))
    }

    pub(crate) fn handle(&self, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
        if request.method() != Method::GET || !request.body().is_empty() {
            return reply(
                StatusCode::METHOD_NOT_ALLOWED,
                "text/plain; charset=utf-8",
                b"only GET is allowed".to_vec(),
            );
        }
        let tile = match parse_tile_request(request.uri()) {
            Ok(v) => v,
            Err(v) => {
                return reply(
                    StatusCode::BAD_REQUEST,
                    "text/plain; charset=utf-8",
                    v.as_bytes().to_vec(),
                );
            }
        };
        let token = self.token.read().ok().and_then(|v| v.as_ref().cloned());
        let Some(token) = token else {
            return reply(
                StatusCode::UNAUTHORIZED,
                "text/plain; charset=utf-8",
                b"online basemap token is not configured".to_vec(),
            );
        };
        match self.fetch_tile(tile, &token) {
            Ok((mime, body)) => reply(StatusCode::OK, mime, body),
            Err(status) => reply(
                status,
                "text/plain; charset=utf-8",
                b"online tile request failed".to_vec(),
            ),
        }
    }
    fn fetch_tile(
        &self,
        tile: TileRequest<'_>,
        token: &str,
    ) -> Result<(&'static str, Vec<u8>), StatusCode> {
        self.fetch_tile_checked(tile, token)
            .map_err(TileFetchFailure::protocol_status)
    }

    fn fetch_tile_checked(
        &self,
        tile: TileRequest<'_>,
        token: &str,
    ) -> Result<(&'static str, Vec<u8>), TileFetchFailure> {
        let url = tile_url(tile, token);
        let mut response = self
            .http
            .get(url.as_str())
            .header("Accept", "image/png,image/jpeg")
            .header("Accept-Encoding", "identity")
            .call()
            .map_err(|error| classify_request_error(&error))?;
        if response.status() != StatusCode::OK {
            return Err(TileFetchFailure::UpstreamOrCredential);
        }
        if let Some(value) = response.headers().get(header::CONTENT_LENGTH) {
            let size = value
                .to_str()
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .ok_or(TileFetchFailure::InvalidContent)?;
            if size > MAX_TILE_BYTES {
                return Err(TileFetchFailure::PayloadTooLarge);
            }
        }
        let mime = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(normalize_mime)
            .ok_or(TileFetchFailure::InvalidContent)?;
        let body = response
            .body_mut()
            .with_config()
            .limit((MAX_TILE_BYTES + 1) as u64)
            .read_to_vec()
            .map_err(|error| classify_request_error(&error))?;
        if body.len() > MAX_TILE_BYTES {
            return Err(TileFetchFailure::PayloadTooLarge);
        }
        if !valid_signature(mime, &body) {
            return Err(TileFetchFailure::InvalidContent);
        }
        Ok((mime, body))
    }
}

impl TileFetchFailure {
    fn probe_status(self) -> BasemapProbeStatus {
        match self {
            Self::Network => BasemapProbeStatus::Network,
            Self::Timeout => BasemapProbeStatus::Timeout,
            Self::UpstreamOrCredential => BasemapProbeStatus::UpstreamOrCredential,
            Self::InvalidContent | Self::PayloadTooLarge => BasemapProbeStatus::InvalidContent,
        }
    }

    fn protocol_status(self) -> StatusCode {
        match self {
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Network
            | Self::Timeout
            | Self::UpstreamOrCredential
            | Self::InvalidContent => StatusCode::BAD_GATEWAY,
        }
    }
}

fn classify_request_error(error: &ureq::Error) -> TileFetchFailure {
    match error {
        ureq::Error::Timeout(_) => TileFetchFailure::Timeout,
        ureq::Error::StatusCode(_)
        | ureq::Error::RedirectFailed
        | ureq::Error::TooManyRedirects => TileFetchFailure::UpstreamOrCredential,
        ureq::Error::Protocol(_)
        | ureq::Error::BodyExceedsLimit(_)
        | ureq::Error::LargeResponseHeader(_, _)
        | ureq::Error::BodyStalled => TileFetchFailure::InvalidContent,
        _ => TileFetchFailure::Network,
    }
}
fn validate_token(token: &str) -> Result<(), String> {
    if !(16..=128).contains(&token.len()) || !token.bytes().all(|v| v.is_ascii_alphanumeric()) {
        return Err("天地图 tk 必须为 16 到 128 位 ASCII 字母或数字".to_string());
    }
    Ok(())
}

fn ensure_credential_headroom(
    current_bytes: u64,
    cap_bytes: u64,
    encrypted_bytes: usize,
) -> Result<(), String> {
    let requested = u64::try_from(encrypted_bytes)
        .map_err(|_| "online basemap credential is too large".to_string())?;
    if current_bytes > cap_bytes || requested > cap_bytes - current_bytes {
        return Err("持久数据空间不足，请先删除缓存后再配置在线地图".into());
    }
    Ok(())
}
fn tile_url(tile: TileRequest<'_>, token: &str) -> Zeroizing<String> {
    Zeroizing::new(format!(
        "https://{TILE_HOST}/{}_w/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER={}&STYLE=default&TILEMATRIXSET=w&FORMAT=tiles&TILECOL={}&TILEROW={}&TILEMATRIX={}&tk={}",
        tile.layer, tile.layer, tile.x, tile.y, tile.z, token
    ))
}

fn parse_tile_request(uri: &tauri::http::Uri) -> Result<TileRequest<'_>, &'static str> {
    if uri.query().is_some() {
        return Err("query parameters are not allowed");
    }
    let scheme = uri.scheme_str().unwrap_or_default();
    let host = uri.host().unwrap_or_default();
    if !((scheme == TILE_PROTOCOL_SCHEME && host == "localhost")
        || ((scheme == "http" || scheme == "https") && host == "tianditu.localhost"))
    {
        return Err("invalid tile origin");
    }
    let s: Vec<_> = uri
        .path()
        .strip_prefix('/')
        .unwrap_or_default()
        .split('/')
        .collect();
    if s.len() != 4 {
        return Err("invalid tile path");
    }
    if !matches!(s[0], "vec" | "cva" | "img" | "cia") {
        return Err("unsupported tile layer");
    }
    let z = canonical_u32(s[1]).ok_or("invalid tile zoom")?;
    let x = canonical_u32(s[2]).ok_or("invalid tile column")?;
    let y = canonical_u32(s[3]).ok_or("invalid tile row")?;
    if !(u32::from(MIN_ZOOM)..=u32::from(MAX_ZOOM)).contains(&z) {
        return Err("tile zoom is out of range");
    }
    let dimension = 1_u32 << z;
    if x >= dimension || y >= dimension {
        return Err("tile coordinate is out of range");
    }
    Ok(TileRequest {
        layer: s[0],
        z: z as u8,
        x,
        y,
    })
}
fn canonical_u32(value: &str) -> Option<u32> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return None;
    }
    let parsed = value.parse::<u32>().ok()?;
    (parsed.to_string() == value).then_some(parsed)
}
fn normalize_mime(value: &str) -> Option<&'static str> {
    match value
        .split(';')
        .next()?
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/png" => Some("image/png"),
        "image/jpeg" => Some("image/jpeg"),
        _ => None,
    }
}
fn valid_signature(mime: &str, bytes: &[u8]) -> bool {
    match mime {
        "image/png" => bytes.starts_with(&[137, 80, 78, 71, 13, 10, 26, 10]),
        "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        _ => false,
    }
}
fn reply(status: StatusCode, mime: &'static str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "no-store, max-age=0")
        .header(header::PRAGMA, "no-cache")
        .header("Access-Control-Allow-Origin", "*")
        .header("Cross-Origin-Resource-Policy", "cross-origin")
        .header("X-Content-Type-Options", "nosniff")
        .header(header::CONTENT_LENGTH, body.len().to_string())
        .body(body)
        .expect("static protocol response headers must be valid")
}

struct TokenVault {
    #[cfg(windows)]
    root: std::path::PathBuf,
    #[cfg(windows)]
    path: std::path::PathBuf,
}
impl TokenVault {
    fn new(root: &Path) -> Self {
        #[cfg(windows)]
        {
            Self {
                root: root.to_path_buf(),
                path: root.join("settings").join(TOKEN_FILE_NAME),
            }
        }
        #[cfg(not(windows))]
        {
            let _ = root;
            Self {}
        }
    }
    fn load(&self) -> Result<Option<Zeroizing<String>>, String> {
        #[cfg(windows)]
        {
            if !self.path.exists() {
                return Ok(None);
            }
            let meta = std::fs::metadata(&self.path)
                .map_err(|_| "cannot read online basemap credential metadata".to_string())?;
            if meta.len() == 0 || meta.len() > 4096 {
                return Err("invalid basemap credential file".into());
            }
            let encrypted = std::fs::read(&self.path)
                .map_err(|_| "cannot read online basemap credential".to_string())?;
            let clear = dpapi::unprotect(&encrypted)?;
            let token = std::str::from_utf8(&clear)
                .map_err(|_| "invalid basemap credential".to_string())?;
            let token = Zeroizing::new(token.to_owned());
            validate_token(&token)?;
            Ok(Some(token))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }
    fn save(&self, token: &[u8]) -> Result<(), String> {
        #[cfg(windows)]
        {
            use std::io::Write;
            let encrypted = dpapi::protect(token)?;
            let parent = self
                .path
                .parent()
                .ok_or("invalid basemap credential path")?;
            std::fs::create_dir_all(parent)
                .map_err(|_| "cannot create basemap settings directory".to_string())?;
            let cache_store = hamheatmap_cache::CacheStore::open(&self.root)
                .map_err(|_| "persistent data store is unavailable".to_string())?;
            let tmp = self.path.with_extension("tmp");
            match std::fs::remove_file(&tmp) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("cannot clear stale basemap credential".into()),
            }
            let usage = cache_store
                .usage()
                .map_err(|_| "cannot measure persistent data usage".to_string())?;
            ensure_credential_headroom(usage.total_bytes, usage.cap_bytes, encrypted.len())?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&tmp)
                .map_err(|_| "cannot create basemap credential".to_string())?;
            let write_result = file.write_all(&encrypted).and_then(|_| file.sync_all());
            drop(file);
            if write_result.is_err() {
                let _ = std::fs::remove_file(&tmp);
                return Err("cannot write basemap credential".into());
            }
            if dpapi::replace_file(&tmp, &self.path).is_err() {
                let _ = std::fs::remove_file(&tmp);
                return Err("cannot commit basemap credential".into());
            }
            Ok(())
        }
        #[cfg(not(windows))]
        {
            let _ = token;
            Ok(())
        }
    }
    fn clear(&self) -> Result<(), String> {
        #[cfg(windows)]
        {
            let tmp = self.path.with_extension("tmp");
            let mut failed = false;
            for path in [&self.path, &tmp] {
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => failed = true,
                }
            }
            if failed {
                Err("cannot clear basemap credential".into())
            } else {
                Ok(())
            }
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }
}

#[cfg(windows)]
mod dpapi {
    use std::{
        ffi::c_void,
        os::windows::ffi::OsStrExt,
        path::Path,
        ptr::{null, null_mut},
    };
    use zeroize::Zeroizing;
    const UI_FORBIDDEN: u32 = 1;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    const ENTROPY: &[u8] = b"HamHeatmap/Tianditu/v1";
    #[repr(C)]
    struct Blob {
        size: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            i: *const Blob,
            d: *const u16,
            e: *const Blob,
            r: *const c_void,
            p: *const c_void,
            f: u32,
            o: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            i: *const Blob,
            d: *mut *mut u16,
            e: *const Blob,
            r: *const c_void,
            p: *const c_void,
            f: u32,
            o: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }
    pub(super) fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
        let mut source_wide: Vec<u16> = source.as_os_str().encode_wide().collect();
        let mut destination_wide: Vec<u16> = destination.as_os_str().encode_wide().collect();
        source_wide.push(0);
        destination_wide.push(0);
        // SAFETY: both paths are valid, NUL-terminated UTF-16 buffers for the call.
        let result = unsafe {
            MoveFileExW(
                source_wide.as_ptr(),
                destination_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result == 0 {
            Err("Windows could not commit basemap credential".into())
        } else {
            Ok(())
        }
    }
    pub(super) fn protect(v: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        transform(v, true)
    }
    pub(super) fn unprotect(v: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        transform(v, false)
    }
    unsafe fn zero_and_free(blob: &mut Blob) {
        if blob.data.is_null() {
            return;
        }
        for offset in 0..blob.size as usize {
            // SAFETY: the caller guarantees DPAPI owns a writable buffer of blob.size bytes.
            unsafe {
                std::ptr::write_volatile(blob.data.add(offset), 0);
            }
        }
        // SAFETY: DPAPI allocated the buffer with LocalAlloc.
        let _ = unsafe { LocalFree(blob.data.cast()) };
        blob.data = null_mut();
        blob.size = 0;
    }
    fn transform(value: &[u8], protect: bool) -> Result<Zeroizing<Vec<u8>>, String> {
        let input = Blob {
            size: u32::try_from(value.len()).map_err(|_| "credential too large")?,
            data: value.as_ptr().cast_mut(),
        };
        let entropy = Blob {
            size: ENTROPY.len() as u32,
            data: ENTROPY.as_ptr().cast_mut(),
        };
        let mut output = Blob {
            size: 0,
            data: null_mut(),
        };
        // SAFETY: buffers are valid and DPAPI initializes output on success.
        let ok = unsafe {
            if protect {
                CryptProtectData(
                    &input,
                    null(),
                    &entropy,
                    null(),
                    null(),
                    UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &input,
                    null_mut(),
                    &entropy,
                    null(),
                    null(),
                    UI_FORBIDDEN,
                    &mut output,
                )
            }
        };
        if ok == 0 {
            // SAFETY: a failing DPAPI call may still have initialized its output blob.
            unsafe {
                zero_and_free(&mut output);
            }
            return Err("Windows could not process basemap credential".into());
        }
        if output.data.is_null() || output.size == 0 || output.size > 4096 {
            // SAFETY: a successful DPAPI call owns any non-null output buffer.
            unsafe {
                zero_and_free(&mut output);
            }
            return Err("Windows returned an invalid basemap credential".into());
        }
        // SAFETY: DPAPI output remains valid until LocalFree.
        let copied = unsafe {
            let value = Zeroizing::new(
                std::slice::from_raw_parts(output.data, output.size as usize).to_vec(),
            );
            zero_and_free(&mut output);
            value
        };
        Ok(copied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn token_is_strict() {
        assert!(validate_token("1234567890abcdef").is_ok());
        for bad in ["short", "1234567890abcde-", " 1234567890abcdef "] {
            assert!(validate_token(bad).is_err());
        }
        assert!(validate_token(&"A".repeat(128)).is_ok());
        assert!(validate_token(&"A".repeat(129)).is_err());
    }
    #[test]
    fn credential_headroom_checks_peak_bytes() {
        assert!(ensure_credential_headroom(90, 100, 10).is_ok());
        assert!(ensure_credential_headroom(91, 100, 10).is_err());
        assert!(ensure_credential_headroom(u64::MAX, u64::MAX, 0).is_ok());
        assert!(ensure_credential_headroom(u64::MAX, u64::MAX, 1).is_err());
        assert!(ensure_credential_headroom(u64::MAX, u64::MAX - 1, 0).is_err());
        assert!(ensure_credential_headroom(u64::MAX - 1, u64::MAX, 1).is_ok());
        assert!(ensure_credential_headroom(1, u64::MAX, usize::MAX).is_err());
    }

    #[test]
    fn paths_are_fixed_and_canonical() {
        for mapped in [
            "http://tianditu.localhost/img/8/145/93",
            "https://tianditu.localhost/img/8/145/93",
        ] {
            let uri = mapped.parse().unwrap();
            let tile = parse_tile_request(&uri).unwrap();
            assert_eq!(tile.layer, "img");
            assert_eq!((tile.z, tile.x, tile.y), (8, 145, 93));
        }

        let uri = "tianditu://localhost/img/8/145/93".parse().unwrap();
        assert_eq!(
            parse_tile_request(&uri),
            Ok(TileRequest {
                layer: "img",
                z: 8,
                x: 145,
                y: 93
            })
        );
        for bad in [
            "tianditu://evil.invalid/img/8/145/93",
            "tianditu://localhost/terrain/8/145/93",
            "tianditu://localhost/img/0/0/0",
            "tianditu://localhost/img/19/0/0",
            "tianditu://localhost/img/8/256/0",
            "tianditu://localhost/img/8/01/0",
            "tianditu://localhost/img/8/1/0?tk=x",
        ] {
            assert!(parse_tile_request(&bad.parse().unwrap()).is_err(), "{bad}");
        }
    }
    #[test]
    fn mime_magic_and_headers_are_strict() {
        let png = [137, 80, 78, 71, 13, 10, 26, 10];
        let jpg = [0xff, 0xd8, 0xff];
        assert!(valid_signature("image/png", &png));
        assert!(valid_signature("image/jpeg", &jpg));
        assert!(!valid_signature("image/png", &jpg));
        assert_eq!(normalize_mime("text/html"), None);
        let response = reply(StatusCode::BAD_REQUEST, "text/plain", vec![]);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "no-store, max-age=0"
        );
        assert_eq!(response.headers()["Access-Control-Allow-Origin"], "*");
    }
    #[test]
    fn handler_rejects_mutation_and_missing_credentials_without_network() {
        let root = std::env::temp_dir().join(format!("hamheatmap-handler-{}", std::process::id()));
        let proxy = TileProxy::new(&root);
        let post = Request::builder()
            .method(Method::POST)
            .uri("tianditu://localhost/vec/8/145/93")
            .body(Vec::new())
            .unwrap();
        assert_eq!(proxy.handle(post).status(), StatusCode::METHOD_NOT_ALLOWED);

        let body = Request::builder()
            .method(Method::GET)
            .uri("tianditu://localhost/vec/8/145/93")
            .body(vec![1])
            .unwrap();
        assert_eq!(proxy.handle(body).status(), StatusCode::METHOD_NOT_ALLOWED);

        let get = Request::builder()
            .uri("tianditu://localhost/vec/8/145/93")
            .body(Vec::new())
            .unwrap();
        assert_eq!(proxy.handle(get).status(), StatusCode::UNAUTHORIZED);
    }
    #[cfg(not(windows))]
    #[test]
    fn fallback_is_memory_only() {
        let root = std::env::temp_dir().join(format!("hamheatmap-token-{}", std::process::id()));
        let proxy = TileProxy::new(&root);
        assert!(
            proxy
                .configure("1234567890abcdef".into())
                .unwrap()
                .configured
        );
        assert!(!root.join("settings").join(TOKEN_FILE_NAME).exists());
        assert!(!proxy.clear().unwrap().configured);
    }
    #[test]
    fn tile_urls_are_fixed_web_mercator_requests() {
        for layer in ["vec", "cva", "img", "cia"] {
            let tile = TileRequest {
                layer,
                z: 8,
                x: 145,
                y: 93,
            };
            let actual = tile_url(tile, "1234567890abcdef");
            let expected = format!(
                "https://t0.tianditu.gov.cn/{layer}_w/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER={layer}&STYLE=default&TILEMATRIXSET=w&FORMAT=tiles&TILECOL=145&TILEROW=93&TILEMATRIX=8&tk=1234567890abcdef"
            );
            assert_eq!(actual.as_str(), expected);
        }
    }

    #[test]
    fn protocol_errors_never_expose_upstream_url_or_token() {
        let token = "1234567890abcdef";
        let tile = TileRequest {
            layer: "img",
            z: 8,
            x: 145,
            y: 93,
        };
        let upstream_url = tile_url(tile, token);
        let response = reply(
            StatusCode::BAD_GATEWAY,
            "text/plain; charset=utf-8",
            b"online tile request failed".to_vec(),
        );
        let body = String::from_utf8(response.into_body()).unwrap();
        assert!(!body.contains(upstream_url.as_str()));
        assert!(!body.contains(token));
    }

    #[test]
    fn probe_result_schema_and_statuses_are_fixed_and_redacted() {
        let cases = [
            (BasemapProbeStatus::Reachable, "reachable"),
            (BasemapProbeStatus::NotConfigured, "not-configured"),
            (BasemapProbeStatus::Network, "network"),
            (BasemapProbeStatus::Timeout, "timeout"),
            (
                BasemapProbeStatus::UpstreamOrCredential,
                "upstream-or-credential",
            ),
            (BasemapProbeStatus::InvalidContent, "invalid-content"),
        ];
        for (status, expected) in cases {
            let value = serde_json::to_value(BasemapProbeResult::new(status)).unwrap();
            assert_eq!(value["schemaVersion"], serde_json::json!(1));
            assert_eq!(value["status"], serde_json::json!(expected));
            assert_eq!(value.as_object().unwrap().len(), 2);
            let encoded = value.to_string();
            for forbidden in ["tk", "url", "body", "path", TILE_HOST] {
                assert!(!encoded.contains(forbidden));
            }
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn probe_without_credentials_returns_without_network() {
        let root = std::env::temp_dir().join(format!(
            "hamheatmap-probe-unconfigured-{}",
            std::process::id()
        ));
        let proxy = TileProxy::new(&root);
        assert_eq!(
            proxy.probe().unwrap(),
            BasemapProbeResult::new(BasemapProbeStatus::NotConfigured)
        );
        assert!(!root.join("settings").join(TOKEN_FILE_NAME).exists());
    }

    #[test]
    fn probe_tile_is_a_fixed_representative_request() {
        let tile = TileRequest {
            layer: PROBE_LAYER,
            z: PROBE_ZOOM,
            x: PROBE_COLUMN,
            y: PROBE_ROW,
        };
        assert_eq!(
            tile,
            TileRequest {
                layer: "vec",
                z: 8,
                x: 215,
                y: 106,
            }
        );
        let actual = tile_url(tile, "1234567890abcdef");
        assert_eq!(
            actual.as_str(),
            "https://t0.tianditu.gov.cn/vec_w/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER=vec&STYLE=default&TILEMATRIXSET=w&FORMAT=tiles&TILECOL=215&TILEROW=106&TILEMATRIX=8&tk=1234567890abcdef"
        );
    }

    #[test]
    fn probe_failure_categories_are_stable() {
        assert_eq!(
            classify_request_error(&ureq::Error::Timeout(ureq::Timeout::Global)),
            TileFetchFailure::Timeout
        );
        assert_eq!(
            classify_request_error(&ureq::Error::HostNotFound),
            TileFetchFailure::Network
        );
        assert_eq!(
            classify_request_error(&ureq::Error::StatusCode(401)),
            TileFetchFailure::UpstreamOrCredential
        );
        assert_eq!(
            classify_request_error(&ureq::Error::BodyExceedsLimit(1)),
            TileFetchFailure::InvalidContent
        );
        assert_eq!(
            TileFetchFailure::PayloadTooLarge.probe_status(),
            BasemapProbeStatus::InvalidContent
        );
        assert_eq!(
            TileFetchFailure::PayloadTooLarge.protocol_status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }

    #[test]
    #[cfg(windows)]
    fn dpapi_round_trip() {
        let input = b"1234567890abcdef";
        let encrypted = dpapi::protect(input).unwrap();
        assert_ne!(encrypted.as_slice(), input);
        assert_eq!(dpapi::unprotect(&encrypted).unwrap().as_slice(), input);
    }
}
