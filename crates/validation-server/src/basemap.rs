// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use ureq::Agent;

const TIANDITU_PROVIDER_ID: &str = "tianditu";
const TILE_PATH_TEMPLATE: &str = "/api/basemap/tianditu/{layer}/{z}/{x}/{y}";
const MAX_ZOOM: u8 = 18;
pub(crate) const SATELLITE_TILE_PATH_PREFIX: &str = "/api/basemap/satellite/";
const SATELLITE_TILE_PATH_TEMPLATE: &str = "/api/basemap/satellite/{z}/{x}/{y}";
const SATELLITE_PROVIDER_ID: &str = "eoxcloudless";
const SATELLITE_MAX_ZOOM: u8 = 14;
const SATELLITE_ATTRIBUTION: &str = "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)";
const SATELLITE_UPSTREAM_PREFIX: &str =
    "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2025_3857/default/g";
const MAX_TILE_BYTES: usize = 2 * 1024 * 1024;
const TOKEN_FILE_LIMIT: u64 = 512;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(5);
const TIANDITU_LAYERS: [BasemapLayerMetadata; 2] = [
    BasemapLayerMetadata {
        id: "vec",
        display_name: "Vector",
    },
    BasemapLayerMetadata {
        id: "cva",
        display_name: "Labels",
    },
];

#[derive(Clone)]
pub(crate) struct Basemap {
    tianditu: BasemapProxy,
}

#[derive(Clone)]
struct BasemapProxy {
    token: Option<String>,
    agent: Agent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Layer {
    Vector,
    Annotation,
}

impl Layer {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "vec" => Some(Self::Vector),
            "cva" => Some(Self::Annotation),
            _ => None,
        }
    }

    fn upstream_name(self) -> &'static str {
        match self {
            Self::Vector => "vec",
            Self::Annotation => "cva",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TileRequest {
    layer: Layer,
    zoom: u8,
    x: u32,
    y: u32,
}

impl TileRequest {
    fn parse(path: &str) -> Result<Self, BasemapError> {
        let mut parts = path.split('/');
        if parts.next() != Some("")
            || parts.next() != Some("api")
            || parts.next() != Some("basemap")
            || parts.next() != Some(TIANDITU_PROVIDER_ID)
        {
            return Err(BasemapError::InvalidPath);
        }
        let layer = parts
            .next()
            .and_then(Layer::parse)
            .ok_or(BasemapError::InvalidPath)?;
        let zoom = parse_decimal(parts.next()).ok_or(BasemapError::InvalidPath)?;
        let x = parse_decimal(parts.next()).ok_or(BasemapError::InvalidPath)?;
        let y = parse_decimal(parts.next()).ok_or(BasemapError::InvalidPath)?;
        if parts.next().is_some() || zoom > u32::from(MAX_ZOOM) {
            return Err(BasemapError::InvalidPath);
        }
        let matrix_size = 1_u32.checked_shl(zoom).ok_or(BasemapError::InvalidPath)?;
        if x >= matrix_size || y >= matrix_size {
            return Err(BasemapError::InvalidPath);
        }
        Ok(Self {
            layer,
            zoom: zoom as u8,
            x,
            y,
        })
    }

    fn upstream_url(self, token: &str) -> String {
        let layer = self.layer.upstream_name();
        format!(
            "https://t0.tianditu.gov.cn/{layer}_w/wmts?SERVICE=WMTS&REQUEST=GetTile&VERSION=1.0.0&LAYER={layer}&STYLE=default&TILEMATRIXSET=w&TILEMATRIX={}&TILEROW={}&TILECOL={}&FORMAT=tiles&tk={token}",
            self.zoom, self.y, self.x
        )
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SatelliteTileRequest {
    zoom: u8,
    x: u32,
    y: u32,
}

impl SatelliteTileRequest {
    fn parse(path: &str) -> Result<Self, BasemapError> {
        let mut parts = path.split('/');
        if parts.next() != Some("")
            || parts.next() != Some("api")
            || parts.next() != Some("basemap")
            || parts.next() != Some("satellite")
        {
            return Err(BasemapError::InvalidPath);
        }
        let zoom = parse_decimal(parts.next()).ok_or(BasemapError::InvalidPath)?;
        let x = parse_decimal(parts.next()).ok_or(BasemapError::InvalidPath)?;
        let y = parse_decimal(parts.next()).ok_or(BasemapError::InvalidPath)?;
        if parts.next().is_some() || zoom > u32::from(SATELLITE_MAX_ZOOM) {
            return Err(BasemapError::InvalidPath);
        }
        let matrix_size = 1_u32.checked_shl(zoom).ok_or(BasemapError::InvalidPath)?;
        if x >= matrix_size || y >= matrix_size {
            return Err(BasemapError::InvalidPath);
        }
        Ok(Self {
            zoom: zoom as u8,
            x,
            y,
        })
    }

    fn upstream_url(self) -> String {
        format!(
            "{SATELLITE_UPSTREAM_PREFIX}/{}/{}/{}.jpg",
            self.zoom, self.y, self.x
        )
    }
}

fn parse_decimal(value: Option<&str>) -> Option<u32> {
    let value = value?;
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    value.parse().ok()
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum BasemapError {
    InvalidPath,
    Disabled,
    UpstreamUnavailable,
    InvalidUpstreamResponse,
}

pub(crate) struct BasemapTile {
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BasemapMetadata {
    enabled: bool,
    provider_id: &'static str,
    display_name: &'static str,
    attribution: &'static str,
    mode: &'static str,
    max_zoom: u8,
    layers: &'static [BasemapLayerMetadata],
    #[serde(skip_serializing_if = "Option::is_none")]
    tile_path_template: Option<&'static str>,
    satellite: SatelliteBasemapMetadata,
}
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct SatelliteBasemapMetadata {
    enabled: bool,
    provider_id: &'static str,
    display_name: &'static str,
    attribution: &'static str,
    mode: &'static str,
    max_zoom: u8,
    tile_path_template: &'static str,
}

const SATELLITE_METADATA: SatelliteBasemapMetadata = SatelliteBasemapMetadata {
    enabled: true,
    provider_id: SATELLITE_PROVIDER_ID,
    display_name: "Sentinel-2 Cloudless 2025",
    attribution: SATELLITE_ATTRIBUTION,
    mode: "same-origin-proxy",
    max_zoom: SATELLITE_MAX_ZOOM,
    tile_path_template: SATELLITE_TILE_PATH_TEMPLATE,
};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct BasemapLayerMetadata {
    id: &'static str,
    display_name: &'static str,
}

impl Basemap {
    pub(crate) fn load(token_file: &Path) -> Result<Self, String> {
        Ok(Self {
            tianditu: BasemapProxy::load(token_file)?,
        })
    }

    pub(crate) fn metadata(&self) -> BasemapMetadata {
        self.tianditu.metadata()
    }

    pub(crate) fn fetch_tianditu(&self, path: &str) -> Result<BasemapTile, BasemapError> {
        self.tianditu.fetch(path)
    }
    pub(crate) fn fetch_satellite(&self, path: &str) -> Result<BasemapTile, BasemapError> {
        self.tianditu.fetch_satellite(path)
    }
}

impl BasemapProxy {
    fn load(token_file: &Path) -> Result<Self, String> {
        let token = load_token(token_file)?;
        let agent = Agent::config_builder()
            .https_only(true)
            .max_redirects(0)
            .timeout_global(Some(REQUEST_TIMEOUT))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_body(Some(RECEIVE_TIMEOUT))
            .build()
            .into();
        Ok(Self { token, agent })
    }

    fn metadata(&self) -> BasemapMetadata {
        BasemapMetadata {
            enabled: self.token.is_some(),
            provider_id: TIANDITU_PROVIDER_ID,
            display_name: "天地图",
            attribution: "天地图",
            mode: "same-origin-proxy",
            max_zoom: MAX_ZOOM,
            layers: &TIANDITU_LAYERS,
            tile_path_template: Some(TILE_PATH_TEMPLATE),
            satellite: SATELLITE_METADATA,
        }
    }

    fn fetch(&self, path: &str) -> Result<BasemapTile, BasemapError> {
        let request = TileRequest::parse(path)?;
        let token = self.token.as_deref().ok_or(BasemapError::Disabled)?;
        let url = request.upstream_url(token);
        self.fetch_url(&url)
    }

    fn fetch_satellite(&self, path: &str) -> Result<BasemapTile, BasemapError> {
        let request = SatelliteTileRequest::parse(path)?;
        self.fetch_url(&request.upstream_url())
    }

    fn fetch_url(&self, url: &str) -> Result<BasemapTile, BasemapError> {
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|_| BasemapError::UpstreamUnavailable)?;
        if response.status().as_u16() != 200 {
            return Err(BasemapError::UpstreamUnavailable);
        }
        if response
            .headers()
            .get("content-length")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .is_some_and(|length| length > MAX_TILE_BYTES)
        {
            return Err(BasemapError::InvalidUpstreamResponse);
        }
        let declared_content_type = response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .ok_or(BasemapError::InvalidUpstreamResponse)?
            .to_owned();
        let body = response
            .body_mut()
            .with_config()
            .limit((MAX_TILE_BYTES + 1) as u64)
            .read_to_vec()
            .map_err(|_| BasemapError::UpstreamUnavailable)?;
        if body.len() > MAX_TILE_BYTES {
            return Err(BasemapError::InvalidUpstreamResponse);
        }
        let content_type = validate_image(&declared_content_type, &body)?;
        Ok(BasemapTile { content_type, body })
    }
}

fn load_token(path: &Path) -> Result<Option<String>, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(format!(
                "cannot inspect basemap token file {}: {error}",
                path.display()
            ));
        }
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "basemap token path must be a regular non-symlink file: {}",
            path.display()
        ));
    }
    if metadata.len() == 0 || metadata.len() > TOKEN_FILE_LIMIT {
        return Err(format!(
            "basemap token file has an invalid size: {}",
            path.display()
        ));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "basemap token file permissions must not grant group or other access: {}",
            path.display()
        ));
    }
    let raw = fs::read_to_string(path)
        .map_err(|error| format!("cannot read basemap token file {}: {error}", path.display()))?;
    let token = raw.trim();
    if !(16..=128).contains(&token.len()) || !token.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(format!(
            "basemap token file does not contain a valid token: {}",
            path.display()
        ));
    }
    Ok(Some(token.to_owned()))
}

fn validate_image(declared_content_type: &str, body: &[u8]) -> Result<&'static str, BasemapError> {
    let declared = declared_content_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let detected = if body.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if body.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if body.len() >= 12 && body.starts_with(b"RIFF") && &body[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return Err(BasemapError::InvalidUpstreamResponse);
    };
    let declared_matches =
        declared == detected || (declared == "image/jpg" && detected == "image/jpeg");
    if !declared_matches {
        return Err(BasemapError::InvalidUpstreamResponse);
    }
    Ok(detected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_ID: AtomicU64 = AtomicU64::new(0);

    fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "hamheatmap-basemap-{}-{nanos}-{}-{name}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn tile_paths_are_strict_and_bounded() {
        assert_eq!(
            TileRequest::parse("/api/basemap/tianditu/vec/18/262143/262143").unwrap(),
            TileRequest {
                layer: Layer::Vector,
                zoom: 18,
                x: 262_143,
                y: 262_143,
            }
        );
        assert_eq!(
            TileRequest::parse("/api/basemap/tianditu/cva/0/0/0")
                .unwrap()
                .layer,
            Layer::Annotation
        );
        for invalid in [
            "/api/basemap/tianditu/vec_w/1/0/0",
            "/api/basemap/other/vec/1/0/0",
            "/api/basemap/tianditu/vec/19/0/0",
            "/api/basemap/tianditu/vec/2/4/0",
            "/api/basemap/tianditu/vec/2/0/4",
            "/api/basemap/tianditu/vec/02/0/0",
            "/api/basemap/tianditu/vec/2/-1/0",
            "/api/basemap/tianditu/vec/2/0/0/extra",
            "/api/basemap/tianditu/vec/2/0",
            "/api/basemap/tianditu/vec/2/0/0?tk=attacker",
        ] {
            assert_eq!(TileRequest::parse(invalid), Err(BasemapError::InvalidPath));
        }
    }

    #[test]
    fn satellite_tile_paths_are_strict_and_reorder_wmts_coordinates() {
        let request = SatelliteTileRequest::parse("/api/basemap/satellite/14/16383/0").unwrap();
        assert_eq!(
            request,
            SatelliteTileRequest {
                zoom: 14,
                x: 16_383,
                y: 0,
            }
        );
        assert_eq!(
            request.upstream_url(),
            "https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2025_3857/default/g/14/0/16383.jpg"
        );
        for invalid in [
            "/api/basemap/satellite/15/0/0",
            "/api/basemap/satellite/2/4/0",
            "/api/basemap/satellite/02/0/0",
            "/api/basemap/satellite/2/-1/0",
            "/api/basemap/satellite/2/0/0/extra",
            "/api/basemap/satellite/2/0/0?source=evil",
        ] {
            assert_eq!(
                SatelliteTileRequest::parse(invalid),
                Err(BasemapError::InvalidPath)
            );
        }
    }

    #[test]
    fn upstream_url_has_only_fixed_origin_and_wmts_parameters() {
        let request = TileRequest::parse("/api/basemap/tianditu/cva/8/201/99").unwrap();
        let url = request.upstream_url("0123456789abcdef");
        assert!(url.starts_with("https://t0.tianditu.gov.cn/cva_w/wmts?"));
        assert!(url.contains("LAYER=cva"));
        assert!(url.contains("TILEMATRIX=8"));
        assert!(url.contains("TILEROW=99"));
        assert!(url.contains("TILECOL=201"));
        assert!(url.ends_with("tk=0123456789abcdef"));
    }

    #[test]
    fn token_file_is_optional_but_present_files_are_fail_closed() {
        let missing = temp_path("missing");
        assert_eq!(load_token(&missing).unwrap(), None);

        let valid = temp_path("valid");
        fs::write(&valid, b"0123456789abcdef\n").unwrap();
        #[cfg(unix)]
        fs::set_permissions(&valid, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            load_token(&valid).unwrap().as_deref(),
            Some("0123456789abcdef")
        );
        let proxy = BasemapProxy::load(&valid).unwrap();
        let metadata = serde_json::to_value(proxy.metadata()).unwrap();
        assert_eq!(metadata["enabled"], true);
        assert_eq!(metadata["providerId"], "tianditu");
        let encoded = serde_json::to_string(&metadata).unwrap();
        assert!(!encoded.contains("0123456789abcdef"));
        assert!(!encoded.contains("t0.tianditu.gov.cn"));
        assert!(!encoded.contains("token"));

        let invalid = temp_path("invalid");
        fs::write(&invalid, b"https://attacker.invalid").unwrap();
        #[cfg(unix)]
        fs::set_permissions(&invalid, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(load_token(&invalid).is_err());

        let _ = fs::remove_file(valid);
        let _ = fs::remove_file(invalid);
    }

    #[test]
    fn image_validation_requires_matching_type_and_signature() {
        let png = b"\x89PNG\r\n\x1a\nrest";
        assert_eq!(validate_image("image/png", png).unwrap(), "image/png");
        assert_eq!(
            validate_image("IMAGE/PNG; charset=binary", png).unwrap(),
            "image/png"
        );
        assert!(validate_image("image/jpeg", png).is_err());
        assert!(validate_image("text/html", b"<html>error</html>").is_err());
        assert!(validate_image("image/png", b"not a png").is_err());
    }
}
