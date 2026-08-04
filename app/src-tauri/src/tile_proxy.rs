// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use serde::Serialize;
use std::{path::Path, sync::RwLock, time::Duration};
use tauri::http::{Method, Request, Response, StatusCode, header};
use zeroize::Zeroizing;

pub(crate) const TILE_PROTOCOL_SCHEME: &str = "tianditu";
pub(crate) const PUBLIC_TILE_PROTOCOL_SCHEME: &str = "basemap";
const TILE_HOST: &str = "t0.tianditu.gov.cn";
const CARTO_ORIGIN: &str = "https://a.basemaps.cartocdn.com";
const EOX_ORIGIN: &str = "https://tiles.maps.eox.at";
const EOX_TILE_PREFIX: &str = "/wmts/1.0.0/s2cloudless-2025_3857/default/g";
const TOKEN_FILE_NAME: &str = "tianditu-token.dpapi";
const MIN_ZOOM: u8 = 1;
const MAX_ZOOM: u8 = 18;
const PUBLIC_MIN_ZOOM: u8 = 0;
const SATELLITE_MAX_ZOOM: u8 = 14;
const MAX_TILE_BYTES: usize = 2 * 1024 * 1024;
const BASEMAP_PROBE_SCHEMA_VERSION: u8 = 1;
const PROBE_LAYER: &str = "vec";
const PROBE_ZOOM: u8 = 8;
const PROBE_COLUMN: u32 = 215;
const PROBE_ROW: u32 = 106;
const CARTO_ATTRIBUTION: &str = "© OpenStreetMap contributors © CARTO";
const SATELLITE_ATTRIBUTION: &str = "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)";

const PUBLIC_BASEMAP_LAYERS: [PublicBasemapLayer; 2] = [
    PublicBasemapLayer {
        id: "base",
        display_name: "Base map",
    },
    PublicBasemapLayer {
        id: "labels",
        display_name: "Place labels",
    },
];

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

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicBasemapInfo {
    enabled: bool,
    provider_id: &'static str,
    display_name: &'static str,
    attribution: &'static str,
    mode: &'static str,
    max_zoom: u8,
    layers: &'static [PublicBasemapLayer],
    tile_path_template: &'static str,
    satellite: PublicSatelliteInfo,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicBasemapLayer {
    id: &'static str,
    display_name: &'static str,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicSatelliteInfo {
    enabled: bool,
    provider_id: &'static str,
    display_name: &'static str,
    attribution: &'static str,
    mode: &'static str,
    max_zoom: u8,
    tile_path_template: &'static str,
}

impl PublicBasemapInfo {
    pub(crate) const fn new() -> Self {
        Self {
            enabled: true,
            provider_id: "carto-voyager",
            display_name: "CARTO Voyager / OpenStreetMap",
            attribution: CARTO_ATTRIBUTION,
            mode: "desktop-protocol-proxy",
            max_zoom: MAX_ZOOM,
            layers: &PUBLIC_BASEMAP_LAYERS,
            tile_path_template: "basemap://localhost/carto/{layer}/{z}/{x}/{y}",
            satellite: PublicSatelliteInfo {
                enabled: true,
                provider_id: "eoxcloudless",
                display_name: "Sentinel-2 Cloudless 2025",
                attribution: SATELLITE_ATTRIBUTION,
                mode: "desktop-protocol-proxy",
                max_zoom: SATELLITE_MAX_ZOOM,
                tile_path_template: "basemap://localhost/eox/satellite/{z}/{x}/{y}",
            },
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
enum PublicTileLayer {
    CartoBase,
    CartoLabels,
    EoxSatellite,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PublicTileRequest {
    layer: PublicTileLayer,
    z: u8,
    x: u32,
    y: u32,
}

impl PublicTileRequest {
    fn upstream_url(self) -> String {
        match self.layer {
            PublicTileLayer::CartoBase | PublicTileLayer::CartoLabels => {
                let path = match self.layer {
                    PublicTileLayer::CartoBase => "rastertiles/voyager_nolabels",
                    PublicTileLayer::CartoLabels => "rastertiles/voyager_only_labels",
                    PublicTileLayer::EoxSatellite => unreachable!(),
                };
                format!("{CARTO_ORIGIN}/{path}/{}/{}/{}.png", self.z, self.x, self.y)
            }
            PublicTileLayer::EoxSatellite => format!(
                "{EOX_ORIGIN}{EOX_TILE_PREFIX}/{}/{}/{}.jpg",
                self.z, self.y, self.x
            ),
        }
    }

    fn accept_header(self) -> &'static str {
        match self.layer {
            PublicTileLayer::CartoBase | PublicTileLayer::CartoLabels => "image/png",
            PublicTileLayer::EoxSatellite => "image/jpeg",
        }
    }

    fn expected_mime(self) -> &'static str {
        match self.layer {
            PublicTileLayer::CartoBase | PublicTileLayer::CartoLabels => "image/png",
            PublicTileLayer::EoxSatellite => "image/jpeg",
        }
    }
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
            .user_agent("HamHeatmap/0.1 DesktopTileProxy")
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
            return Ok(BasemapProbeResult::new(BasemapProbeStatus::NotConfigured));
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
    pub(crate) fn handle_public(&self, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
        if request.method() != Method::GET || !request.body().is_empty() {
            return reply(
                StatusCode::METHOD_NOT_ALLOWED,
                "text/plain; charset=utf-8",
                b"only GET is allowed".to_vec(),
            );
        }
        let tile = match parse_public_tile_request(request.uri()) {
            Ok(tile) => tile,
            Err(message) => {
                return reply(
                    StatusCode::BAD_REQUEST,
                    "text/plain; charset=utf-8",
                    message.as_bytes().to_vec(),
                );
            }
        };
        match self.fetch_public_tile(tile) {
            Ok((mime, body)) => reply(StatusCode::OK, mime, body),
            Err(status) => reply(
                status,
                "text/plain; charset=utf-8",
                b"public map tile request failed".to_vec(),
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

    fn fetch_public_tile(
        &self,
        tile: PublicTileRequest,
    ) -> Result<(&'static str, Vec<u8>), StatusCode> {
        self.fetch_public_tile_checked(tile)
            .map_err(TileFetchFailure::protocol_status)
    }

    fn fetch_public_tile_checked(
        &self,
        tile: PublicTileRequest,
    ) -> Result<(&'static str, Vec<u8>), TileFetchFailure> {
        let url = tile.upstream_url();
        let mut response = self
            .http
            .get(url.as_str())
            .header("Accept", tile.accept_header())
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
                .and_then(|value| value.parse::<usize>().ok())
                .ok_or(TileFetchFailure::InvalidContent)?;
            if size > MAX_TILE_BYTES {
                return Err(TileFetchFailure::PayloadTooLarge);
            }
        }
        let mime = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(normalize_mime)
            .filter(|mime| *mime == tile.expected_mime())
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
            Self::Network | Self::Timeout | Self::UpstreamOrCredential | Self::InvalidContent => {
                StatusCode::BAD_GATEWAY
            }
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

fn parse_public_tile_request(uri: &tauri::http::Uri) -> Result<PublicTileRequest, &'static str> {
    if uri.query().is_some() {
        return Err("query parameters are not allowed");
    }
    let scheme = uri.scheme_str().unwrap_or_default();
    let host = uri.host().unwrap_or_default();
    if !((scheme == PUBLIC_TILE_PROTOCOL_SCHEME && host == "localhost")
        || ((scheme == "http" || scheme == "https") && host == "basemap.localhost"))
    {
        return Err("invalid public tile origin");
    }
    let segments: Vec<_> = uri
        .path()
        .strip_prefix('/')
        .unwrap_or_default()
        .split('/')
        .collect();
    let (layer, z, x, y) = match segments.as_slice() {
        ["carto", layer, z, x, y] => {
            let layer = match *layer {
                "base" => PublicTileLayer::CartoBase,
                "labels" => PublicTileLayer::CartoLabels,
                _ => return Err("unsupported public tile layer"),
            };
            (layer, *z, *x, *y)
        }
        ["eox", "satellite", z, x, y] => (PublicTileLayer::EoxSatellite, *z, *x, *y),
        _ => return Err("invalid public tile path"),
    };
    let z = canonical_u32(z).ok_or("invalid public tile zoom")?;
    let x = canonical_u32(x).ok_or("invalid public tile column")?;
    let y = canonical_u32(y).ok_or("invalid public tile row")?;
    let max_zoom = match layer {
        PublicTileLayer::CartoBase | PublicTileLayer::CartoLabels => MAX_ZOOM,
        PublicTileLayer::EoxSatellite => SATELLITE_MAX_ZOOM,
    };
    if !(u32::from(PUBLIC_MIN_ZOOM)..=u32::from(max_zoom)).contains(&z) {
        return Err("public tile zoom is out of range");
    }
    let dimension = 1_u32 << z;
    if x >= dimension || y >= dimension {
        return Err("public tile coordinate is out of range");
    }
    Ok(PublicTileRequest {
        layer,
        z: z as u8,
        x,
        y,
    })
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
    fn public_metadata_is_fixed_and_contains_no_upstream_origins() {
        let value = serde_json::to_value(PublicBasemapInfo::new()).unwrap();
        assert_eq!(value["enabled"], true);
        assert_eq!(value["providerId"], "carto-voyager");
        assert_eq!(value["mode"], "desktop-protocol-proxy");
        assert_eq!(value["maxZoom"], 18);
        assert_eq!(value["layers"][0]["id"], "base");
        assert_eq!(value["layers"][1]["id"], "labels");
        assert_eq!(
            value["tilePathTemplate"],
            "basemap://localhost/carto/{layer}/{z}/{x}/{y}"
        );
        assert_eq!(value["satellite"]["providerId"], "eoxcloudless");
        assert_eq!(value["satellite"]["maxZoom"], 14);
        assert_eq!(
            value["satellite"]["tilePathTemplate"],
            "basemap://localhost/eox/satellite/{z}/{x}/{y}"
        );
        let encoded = value.to_string();
        assert!(!encoded.contains(CARTO_ORIGIN));
        assert!(!encoded.contains(EOX_ORIGIN));
        assert!(!encoded.contains("tianditu"));
    }

    #[test]
    fn public_paths_are_fixed_canonical_and_provider_scoped() {
        for mapped in [
            "basemap://localhost/carto/base/18/262143/262143",
            "http://basemap.localhost/carto/labels/0/0/0",
            "https://basemap.localhost/eox/satellite/14/16383/0",
        ] {
            assert!(
                parse_public_tile_request(&mapped.parse().unwrap()).is_ok(),
                "{mapped}"
            );
        }
        assert_eq!(
            parse_public_tile_request(&"basemap://localhost/carto/base/8/201/99".parse().unwrap())
                .unwrap(),
            PublicTileRequest {
                layer: PublicTileLayer::CartoBase,
                z: 8,
                x: 201,
                y: 99,
            }
        );
        for bad in [
            "basemap://evil.invalid/carto/base/1/0/0",
            "basemap://localhost/tianditu/vec/1/0/0",
            "basemap://localhost/carto/vec/1/0/0",
            "basemap://localhost/carto/base/19/0/0",
            "basemap://localhost/eox/satellite/15/0/0",
            "basemap://localhost/eox/other/1/0/0",
            "basemap://localhost/carto/base/2/4/0",
            "basemap://localhost/carto/base/2/0/4",
            "basemap://localhost/carto/base/02/0/0",
            "basemap://localhost/carto/base/2/-1/0",
            "basemap://localhost/carto/base/2/0/0/extra",
            "basemap://localhost/carto/base/2/0/0?source=evil",
            "http://tianditu.localhost/carto/base/1/0/0",
        ] {
            assert!(
                parse_public_tile_request(&bad.parse().unwrap()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn public_upstream_urls_and_content_contracts_are_fixed() {
        let base = PublicTileRequest {
            layer: PublicTileLayer::CartoBase,
            z: 8,
            x: 201,
            y: 99,
        };
        assert_eq!(
            base.upstream_url(),
            "https://a.basemaps.cartocdn.com/rastertiles/voyager_nolabels/8/201/99.png"
        );
        assert_eq!(base.accept_header(), "image/png");
        assert_eq!(base.expected_mime(), "image/png");

        let labels = PublicTileRequest {
            layer: PublicTileLayer::CartoLabels,
            ..base
        };
        assert_eq!(
            labels.upstream_url(),
            "https://a.basemaps.cartocdn.com/rastertiles/voyager_only_labels/8/201/99.png"
        );

        let satellite = PublicTileRequest {
            layer: PublicTileLayer::EoxSatellite,
            z: 14,
            x: 16_383,
            y: 0,
        };
        assert_eq!(
            satellite.upstream_url(),
            "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2025_3857/default/g/14/0/16383.jpg"
        );
        assert_eq!(satellite.accept_header(), "image/jpeg");
        assert_eq!(satellite.expected_mime(), "image/jpeg");
    }

    #[test]
    fn public_handler_rejects_mutation_and_invalid_paths_without_network() {
        let root =
            std::env::temp_dir().join(format!("hamheatmap-public-handler-{}", std::process::id()));
        let proxy = TileProxy::new(&root);
        let post = Request::builder()
            .method(Method::POST)
            .uri("basemap://localhost/carto/base/8/145/93")
            .body(Vec::new())
            .unwrap();
        assert_eq!(
            proxy.handle_public(post).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );

        let body = Request::builder()
            .method(Method::GET)
            .uri("basemap://localhost/carto/base/8/145/93")
            .body(vec![1])
            .unwrap();
        assert_eq!(
            proxy.handle_public(body).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );

        let invalid = Request::builder()
            .uri("basemap://localhost/carto/evil/8/145/93")
            .body(Vec::new())
            .unwrap();
        assert_eq!(
            proxy.handle_public(invalid).status(),
            StatusCode::BAD_REQUEST
        );
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
