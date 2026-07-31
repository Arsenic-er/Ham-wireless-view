use std::fs;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use sha2::{Digest, Sha256};
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
pub(crate) const PMTILES_PATH: &str = "/api/basemap/pmtiles/four-provinces.pmtiles";
const PMTILES_ARCHIVE_BYTES: u64 = 33_044_072;
const MAX_PMTILES_ARCHIVE_BYTES: u64 = 500_000_000;
const MAX_PMTILES_RANGE_BYTES: u64 = 8 * 1024 * 1024;
const PMTILES_MAGIC: &[u8; 8] = b"PMTiles\x03";
const PMTILES_BOUNDS: [f64; 4] = [107.5, 18.0, 125.5, 33.5];
const PMTILES_SHA256: [u8; 32] = [
    0x5b, 0xda, 0x49, 0xbf, 0x90, 0x9a, 0x5b, 0x9f, 0xae, 0x93, 0x13, 0x53, 0xed, 0xf5, 0xae, 0xa8,
    0x2b, 0xa3, 0x5b, 0xe9, 0xf8, 0x18, 0x71, 0x28, 0x64, 0x3b, 0x97, 0x2e, 0xed, 0x4c, 0x87, 0xd0,
];
const PMTILES_LAYERS: [BasemapLayerMetadata; 6] = [
    BasemapLayerMetadata {
        id: "earth",
        display_name: "Land",
    },
    BasemapLayerMetadata {
        id: "landcover",
        display_name: "Land cover",
    },
    BasemapLayerMetadata {
        id: "landuse",
        display_name: "Land use",
    },
    BasemapLayerMetadata {
        id: "water",
        display_name: "Water",
    },
    BasemapLayerMetadata {
        id: "roads",
        display_name: "Roads",
    },
    BasemapLayerMetadata {
        id: "places",
        display_name: "Places",
    },
];
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
    pmtiles: Option<PmtilesArchive>,
    tianditu: BasemapProxy,
}

#[derive(Clone)]
struct PmtilesArchive {
    file: Arc<Mutex<File>>,
    length: u64,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    resource_path: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bounds: Option<[f64; 4]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_bytes: Option<u64>,
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

pub(crate) struct PmtilesResponse {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
    pub(crate) content_length: u64,
    pub(crate) content_range: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum PmtilesError {
    Disabled,
    InvalidRange,
    ReadFailed,
}

impl Basemap {
    pub(crate) fn load(pmtiles_file: &Path, token_file: &Path) -> Result<Self, String> {
        Ok(Self {
            pmtiles: PmtilesArchive::load(pmtiles_file)?,
            tianditu: BasemapProxy::load(token_file)?,
        })
    }

    #[cfg(test)]
    pub(crate) fn load_unchecked_for_test(
        pmtiles_file: &Path,
        token_file: &Path,
    ) -> Result<Self, String> {
        let file = File::open(pmtiles_file)
            .map_err(|error| format!("cannot open test PMTiles archive: {error}"))?;
        let length = file
            .metadata()
            .map_err(|error| format!("cannot inspect test PMTiles archive: {error}"))?
            .len();
        Ok(Self {
            pmtiles: Some(PmtilesArchive {
                file: Arc::new(Mutex::new(file)),
                length,
            }),
            tianditu: BasemapProxy::load(token_file)?,
        })
    }

    pub(crate) fn metadata(&self) -> BasemapMetadata {
        if self.pmtiles.is_some() {
            BasemapMetadata {
                enabled: true,
                provider_id: "protomaps",
                display_name: "Protomaps (internal validation)",
                attribution: "\u{00a9} OpenStreetMap contributors",
                mode: "same-origin-pmtiles",
                max_zoom: 9,
                layers: &PMTILES_LAYERS,
                tile_path_template: None,
                resource_path: Some(PMTILES_PATH),
                bounds: Some(PMTILES_BOUNDS),
                archive_bytes: Some(PMTILES_ARCHIVE_BYTES),
                satellite: SATELLITE_METADATA,
            }
        } else {
            self.tianditu.metadata()
        }
    }

    pub(crate) fn fetch_tianditu(&self, path: &str) -> Result<BasemapTile, BasemapError> {
        self.tianditu.fetch(path)
    }
    pub(crate) fn fetch_satellite(&self, path: &str) -> Result<BasemapTile, BasemapError> {
        self.tianditu.fetch_satellite(path)
    }

    pub(crate) fn read_pmtiles(
        &self,
        range: Option<&str>,
        head_only: bool,
    ) -> Result<PmtilesResponse, PmtilesError> {
        let archive = self.pmtiles.as_ref().ok_or(PmtilesError::Disabled)?;
        if head_only && range.is_none() {
            return Ok(PmtilesResponse {
                status: 200,
                body: Vec::new(),
                content_length: archive.length,
                content_range: None,
            });
        }
        let (start, end) = parse_range(range.ok_or(PmtilesError::InvalidRange)?, archive.length)?;
        let length = end - start + 1;
        let body = if head_only {
            Vec::new()
        } else {
            archive.read_exact_range(start, length)?
        };
        Ok(PmtilesResponse {
            status: 206,
            body,
            content_length: length,
            content_range: Some(format!("bytes {start}-{end}/{}", archive.length)),
        })
    }

    pub(crate) fn pmtiles_length(&self) -> Option<u64> {
        self.pmtiles.as_ref().map(|archive| archive.length)
    }
}

impl PmtilesArchive {
    fn load(path: &Path) -> Result<Option<Self>, String> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(format!(
                    "cannot inspect PMTiles archive {}: {error}",
                    path.display()
                ));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(format!(
                "PMTiles path must be a regular non-symlink file: {}",
                path.display()
            ));
        }
        if metadata.len() != PMTILES_ARCHIVE_BYTES || metadata.len() > MAX_PMTILES_ARCHIVE_BYTES {
            return Err(format!(
                "PMTiles archive has an unexpected size: {}",
                path.display()
            ));
        }
        let mut file = File::open(path)
            .map_err(|error| format!("cannot open PMTiles archive {}: {error}", path.display()))?;
        let opened_metadata = file.metadata().map_err(|error| {
            format!(
                "cannot inspect opened PMTiles archive {}: {error}",
                path.display()
            )
        })?;
        if !opened_metadata.is_file() || opened_metadata.len() != metadata.len() {
            return Err(format!(
                "PMTiles archive changed during validation: {}",
                path.display()
            ));
        }
        #[cfg(unix)]
        if opened_metadata.dev() != metadata.dev() || opened_metadata.ino() != metadata.ino() {
            return Err(format!(
                "PMTiles archive changed between inspection and open: {}",
                path.display()
            ));
        }
        let mut magic = [0_u8; 8];
        file.read_exact(&mut magic).map_err(|error| {
            format!(
                "cannot read PMTiles archive header {}: {error}",
                path.display()
            )
        })?;
        if &magic != PMTILES_MAGIC {
            return Err(format!(
                "PMTiles archive has invalid magic or version: {}",
                path.display()
            ));
        }
        Self::validate_sha256(&mut file, path)?;
        Ok(Some(Self {
            file: Arc::new(Mutex::new(file)),
            length: metadata.len(),
        }))
    }

    fn validate_sha256(file: &mut File, path: &Path) -> Result<(), String> {
        file.seek(SeekFrom::Start(0)).map_err(|error| {
            format!(
                "cannot seek PMTiles archive before hashing {}: {error}",
                path.display()
            )
        })?;
        let mut hasher = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|error| {
                format!("cannot hash PMTiles archive {}: {error}", path.display())
            })?;
            if count == 0 {
                break;
            }
            hasher.update(&buffer[..count]);
        }
        let digest: [u8; 32] = hasher.finalize().into();
        if digest != PMTILES_SHA256 {
            return Err(format!(
                "PMTiles archive SHA-256 mismatch: {}",
                path.display()
            ));
        }
        file.seek(SeekFrom::Start(0)).map_err(|error| {
            format!(
                "cannot rewind PMTiles archive after hashing {}: {error}",
                path.display()
            )
        })?;
        Ok(())
    }

    fn read_exact_range(&self, start: u64, length: u64) -> Result<Vec<u8>, PmtilesError> {
        let capacity = usize::try_from(length).map_err(|_| PmtilesError::InvalidRange)?;
        let mut body = vec![0_u8; capacity];
        let mut file = self.file.lock().map_err(|_| PmtilesError::ReadFailed)?;
        file.seek(SeekFrom::Start(start))
            .and_then(|_| file.read_exact(&mut body))
            .map_err(|_| PmtilesError::ReadFailed)?;
        Ok(body)
    }
}

fn parse_range(value: &str, archive_length: u64) -> Result<(u64, u64), PmtilesError> {
    let raw = value
        .strip_prefix("bytes=")
        .ok_or(PmtilesError::InvalidRange)?;
    if raw.is_empty() || raw.contains(',') || raw.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return Err(PmtilesError::InvalidRange);
    }
    let (start, end) = raw.split_once('-').ok_or(PmtilesError::InvalidRange)?;
    if start.is_empty()
        || end.is_empty()
        || !start.bytes().all(|byte| byte.is_ascii_digit())
        || !end.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(PmtilesError::InvalidRange);
    }
    let start = start
        .parse::<u64>()
        .map_err(|_| PmtilesError::InvalidRange)?;
    let end = end.parse::<u64>().map_err(|_| PmtilesError::InvalidRange)?;
    let length = end
        .checked_sub(start)
        .and_then(|difference| difference.checked_add(1))
        .ok_or(PmtilesError::InvalidRange)?;
    if end >= archive_length || length > MAX_PMTILES_RANGE_BYTES {
        return Err(PmtilesError::InvalidRange);
    }
    Ok((start, end))
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
            resource_path: None,
            bounds: None,
            archive_bytes: None,
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
    use std::io::Write;
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
    fn write_sparse_pmtiles(path: &Path) {
        let mut file = File::create(path).unwrap();
        file.write_all(PMTILES_MAGIC).unwrap();
        file.set_len(PMTILES_ARCHIVE_BYTES).unwrap();
    }

    #[test]
    fn pmtiles_archive_is_optional_but_present_files_fail_closed() {
        let missing = temp_path("missing.pmtiles");
        assert!(PmtilesArchive::load(&missing).unwrap().is_none());

        let wrong_hash = temp_path("wrong-hash.pmtiles");
        write_sparse_pmtiles(&wrong_hash);
        let error = match PmtilesArchive::load(&wrong_hash) {
            Err(error) => error,
            Ok(_) => panic!("wrong-hash PMTiles archive was accepted"),
        };
        assert!(error.contains("SHA-256 mismatch"), "{error}");

        let wrong_magic = temp_path("wrong-magic.pmtiles");
        let file = File::create(&wrong_magic).unwrap();
        file.set_len(PMTILES_ARCHIVE_BYTES).unwrap();
        assert!(PmtilesArchive::load(&wrong_magic).is_err());

        let wrong_size = temp_path("wrong-size.pmtiles");
        fs::write(&wrong_size, PMTILES_MAGIC).unwrap();
        assert!(PmtilesArchive::load(&wrong_size).is_err());

        #[cfg(unix)]
        {
            let link = temp_path("link.pmtiles");
            std::os::unix::fs::symlink(&wrong_hash, &link).unwrap();
            assert!(PmtilesArchive::load(&link).is_err());
            let _ = fs::remove_file(link);
        }

        let _ = fs::remove_file(wrong_hash);
        let _ = fs::remove_file(wrong_magic);
        let _ = fs::remove_file(wrong_size);
    }

    #[test]
    fn pmtiles_range_parser_accepts_only_one_bounded_closed_range() {
        assert_eq!(parse_range("bytes=0-7", 100).unwrap(), (0, 7));
        assert_eq!(parse_range("bytes=92-99", 100).unwrap(), (92, 99));
        for invalid in [
            "",
            "Bytes=0-7",
            "bytes=0-",
            "bytes=-8",
            "bytes=8-7",
            "bytes=0-100",
            "bytes=0-1,4-5",
            "bytes= 0-7",
            "bytes=0 -7",
        ] {
            assert_eq!(parse_range(invalid, 100), Err(PmtilesError::InvalidRange));
        }
        assert_eq!(
            parse_range(&format!("bytes=0-{}", MAX_PMTILES_RANGE_BYTES), u64::MAX),
            Err(PmtilesError::InvalidRange)
        );
    }

    #[test]
    fn pmtiles_metadata_and_reads_use_fixed_internal_contract() {
        let archive_path = temp_path("contract.pmtiles");
        write_sparse_pmtiles(&archive_path);
        let missing_token = temp_path("missing.token");
        let basemap = Basemap {
            pmtiles: Some(PmtilesArchive {
                file: Arc::new(Mutex::new(File::open(&archive_path).unwrap())),
                length: PMTILES_ARCHIVE_BYTES,
            }),
            tianditu: BasemapProxy::load(&missing_token).unwrap(),
        };
        let metadata = serde_json::to_value(basemap.metadata()).unwrap();
        assert_eq!(metadata["providerId"], "protomaps");
        assert_eq!(metadata["mode"], "same-origin-pmtiles");
        assert_eq!(metadata["resourcePath"], PMTILES_PATH);
        assert_eq!(metadata["bounds"], serde_json::json!(PMTILES_BOUNDS));
        assert_eq!(metadata["archiveBytes"], PMTILES_ARCHIVE_BYTES);
        assert_eq!(metadata["maxZoom"], 9);
        assert_eq!(
            metadata["attribution"],
            "\u{00a9} OpenStreetMap contributors"
        );
        assert_eq!(metadata["layers"].as_array().unwrap().len(), 6);
        assert_eq!(metadata["layers"][5]["id"], "places");
        assert_eq!(metadata["satellite"]["enabled"], true);
        assert_eq!(metadata["satellite"]["providerId"], "eoxcloudless");
        assert_eq!(metadata["satellite"]["mode"], "same-origin-proxy");
        assert_eq!(metadata["satellite"]["maxZoom"], 14);
        assert_eq!(
            metadata["satellite"]["tilePathTemplate"],
            SATELLITE_TILE_PATH_TEMPLATE
        );
        assert!(
            metadata["displayName"]
                .as_str()
                .unwrap()
                .contains("internal validation")
        );
        assert!(!serde_json::to_string(&metadata).unwrap().contains('?'));

        let header = basemap.read_pmtiles(Some("bytes=0-7"), false).unwrap();
        assert_eq!(header.status, 206);
        assert_eq!(header.body, PMTILES_MAGIC);
        assert_eq!(header.content_length, 8);
        assert_eq!(header.content_range.as_deref(), Some("bytes 0-7/33044072"));

        let head = basemap.read_pmtiles(None, true).unwrap();
        assert_eq!(head.status, 200);
        assert!(head.body.is_empty());
        assert_eq!(head.content_length, PMTILES_ARCHIVE_BYTES);
        assert!(matches!(
            basemap.read_pmtiles(None, false),
            Err(PmtilesError::InvalidRange)
        ));
        let _ = fs::remove_file(archive_path);
    }
}
