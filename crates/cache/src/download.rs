use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use hamheatmap_terrain::{
    DemTileId, encode_uniform_ocean_dem_tile, encode_uniform_ocean_water_tile,
};
use sha2::{Digest, Sha256};

use crate::planner::{AssetDescriptor, DemRegionPlan, glo90_assets};
use crate::{CacheError, CacheKind, CacheStore};

const USER_AGENT: &str = "HamHeatmap/0.1 (+https://github.com/hamheatmap)";
const ALLOWED_GLO90_URL_PREFIX: &str = "https://copernicus-dem-90m.s3.amazonaws.com/";
const COPY_BUFFER_BYTES: usize = 128 * 1024;
const GENERATED_OCEAN_ETAG: &str = "generated:uniform-ocean-v1";
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetTransfer {
    Https,
    GeneratedOcean { tile: DemTileId },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProbedAsset {
    pub descriptor: AssetDescriptor,
    pub expected_size_bytes: u64,
    pub expected_sha256: Option<String>,
    pub source_etag: Option<String>,
    pub accepts_ranges: bool,
    pub resumable_bytes: u64,
    pub transfer: AssetTransfer,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadPlan {
    pub region: DemRegionPlan,
    pub ready_asset_count: usize,
    pub assets: Vec<ProbedAsset>,
    pub additional_download_bytes: u64,
    pub generated_asset_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct HeadMetadata {
    expected_size_bytes: u64,
    source_etag: Option<String>,
    accepts_ranges: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum HeadProbe {
    Present(HeadMetadata),
    NotFound,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DownloadProgress {
    pub asset_index: usize,
    pub asset_count: usize,
    pub asset_key: String,
    pub asset_downloaded_bytes: u64,
    pub asset_expected_bytes: u64,
    pub total_downloaded_bytes: u64,
    pub total_expected_bytes: u64,
}

#[derive(Debug)]
pub struct Glo90DownloadService {
    agent: ureq::Agent,
}

impl Default for Glo90DownloadService {
    fn default() -> Self {
        Self::new()
    }
}

impl Glo90DownloadService {
    pub fn new() -> Self {
        Self {
            agent: ureq::Agent::new_with_defaults(),
        }
    }

    pub fn probe_region(
        &self,
        store: &mut CacheStore,
        region: DemRegionPlan,
    ) -> Result<DownloadPlan, CacheError> {
        let cancelled = AtomicBool::new(false);
        self.probe_region_with_cancel(store, region, &cancelled)
    }

    pub fn probe_region_with_cancel(
        &self,
        store: &mut CacheStore,
        region: DemRegionPlan,
        cancelled: &AtomicBool,
    ) -> Result<DownloadPlan, CacheError> {
        let mut ready_asset_count = 0;
        let mut assets = Vec::new();
        let mut additional_download_bytes = 0_u64;
        let mut generated_asset_count = 0_usize;
        for tile in &region.tiles {
            if cancelled.load(Ordering::Acquire) {
                return Err(CacheError::Cancelled);
            }
            let descriptors = glo90_assets(*tile)?;
            let ready = [
                store.asset_is_ready(&descriptors[0])?,
                store.asset_is_ready(&descriptors[1])?,
            ];
            if ready.iter().all(|value| *value) {
                ready_asset_count += 2;
                continue;
            }
            let dem_head = self.probe_head(&descriptors[0])?;
            if cancelled.load(Ordering::Acquire) {
                return Err(CacheError::Cancelled);
            }
            let water_head = self.probe_head(&descriptors[1])?;
            let head = [dem_head, water_head];
            let uniform_ocean = match (&head[0], &head[1]) {
                (HeadProbe::NotFound, HeadProbe::NotFound) => true,
                (HeadProbe::Present(_), HeadProbe::Present(_)) => false,
                _ => {
                    return Err(CacheError::Integrity {
                        asset_key: tile.filename(),
                        message: "Copernicus DEM and WBM availability disagree for one geocell"
                            .into(),
                    });
                }
            };

            for index in 0..descriptors.len() {
                if cancelled.load(Ordering::Acquire) {
                    return Err(CacheError::Cancelled);
                }
                let descriptor = descriptors[index].clone();
                if ready[index] {
                    ready_asset_count += 1;
                    continue;
                }

                let (expected_size_bytes, expected_sha256, source_etag, accepts_ranges, transfer) =
                    if uniform_ocean {
                        let bytes = generated_ocean_bytes(*tile, descriptor.kind)?;
                        generated_asset_count += 1;
                        (
                            bytes.len() as u64,
                            Some(sha256_bytes(&bytes)),
                            Some(GENERATED_OCEAN_ETAG.into()),
                            false,
                            AssetTransfer::GeneratedOcean { tile: *tile },
                        )
                    } else {
                        let HeadProbe::Present(metadata) = &head[index] else {
                            unreachable!("paired availability was checked above");
                        };
                        (
                            metadata.expected_size_bytes,
                            None,
                            metadata.source_etag.clone(),
                            metadata.accepts_ranges,
                            AssetTransfer::Https,
                        )
                    };
                if store.register_existing_managed_file(
                    &descriptor,
                    expected_size_bytes,
                    source_etag.as_deref(),
                )? {
                    ready_asset_count += 1;
                    continue;
                }
                let resumable_bytes = store.resumable_bytes_for_probe(
                    &descriptor,
                    expected_size_bytes,
                    source_etag.as_deref(),
                    accepts_ranges,
                )?;
                additional_download_bytes = additional_download_bytes
                    .checked_add(expected_size_bytes - resumable_bytes)
                    .ok_or_else(|| CacheError::InvalidData("download size overflowed".into()))?;
                assets.push(ProbedAsset {
                    descriptor,
                    expected_size_bytes,
                    expected_sha256,
                    source_etag,
                    accepts_ranges,
                    resumable_bytes,
                    transfer,
                });
            }
        }
        store.preflight_additional_capacity(additional_download_bytes)?;
        Ok(DownloadPlan {
            region,
            ready_asset_count,
            assets,
            additional_download_bytes,
            generated_asset_count,
        })
    }

    fn probe_head(&self, descriptor: &AssetDescriptor) -> Result<HeadProbe, CacheError> {
        validate_download_url(&descriptor.url)?;
        let response = match self
            .agent
            .head(&descriptor.url)
            .config()
            .timeout_global(Some(HEAD_TIMEOUT))
            .build()
            .header("User-Agent", USER_AGENT)
            .header("Accept-Encoding", "identity")
            .call()
        {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(404)) => return Ok(HeadProbe::NotFound),
            Err(error) => {
                return Err(CacheError::Network(format!(
                    "HEAD {} failed: {error}",
                    descriptor.url
                )));
            }
        };
        let expected_size_bytes =
            header_u64(response.headers(), "content-length")?.ok_or_else(|| {
                CacheError::Network(format!("HEAD {} omitted Content-Length", descriptor.url))
            })?;
        if expected_size_bytes == 0 {
            return Err(CacheError::Network(format!(
                "HEAD {} returned a zero Content-Length",
                descriptor.url
            )));
        }
        Ok(HeadProbe::Present(HeadMetadata {
            expected_size_bytes,
            source_etag: header_string(response.headers(), "etag"),
            accepts_ranges: header_string(response.headers(), "accept-ranges")
                .is_some_and(|value| value.eq_ignore_ascii_case("bytes")),
        }))
    }

    pub fn execute<F>(
        &self,
        store: &mut CacheStore,
        plan: &DownloadPlan,
        cancelled: &AtomicBool,
        mut on_progress: F,
    ) -> Result<Vec<std::path::PathBuf>, CacheError>
    where
        F: FnMut(&DownloadProgress),
    {
        store.upsert_region(&plan.region)?;
        let total_expected_bytes = plan.additional_download_bytes;
        let mut total_downloaded_bytes = 0_u64;
        let mut ready_paths = Vec::with_capacity(plan.assets.len());
        for (asset_index, asset) in plan.assets.iter().enumerate() {
            if cancelled.load(Ordering::Acquire) {
                return Err(CacheError::Cancelled);
            }
            let (partial_path, offset) = store.prepare_download(
                &asset.descriptor,
                asset.expected_size_bytes,
                asset.expected_sha256.as_deref(),
                asset.source_etag.as_deref(),
                asset.resumable_bytes,
            )?;
            if offset == asset.expected_size_bytes {
                ready_paths.push(store.finalize_download(
                    &asset.descriptor,
                    asset.expected_size_bytes,
                    asset.expected_sha256.as_deref(),
                )?);
                continue;
            }

            if let AssetTransfer::GeneratedOcean { tile } = asset.transfer {
                let bytes = generated_ocean_bytes(tile, asset.descriptor.kind)?;
                if bytes.len() as u64 != asset.expected_size_bytes {
                    return Err(CacheError::Integrity {
                        asset_key: asset.descriptor.asset_key.clone(),
                        message: "generated ocean asset changed size after planning".into(),
                    });
                }
                let mut output = File::create(&partial_path)?;
                output.write_all(&bytes)?;
                output.sync_all()?;
                let written = bytes.len() as u64;
                store.update_partial_size(&asset.descriptor.asset_key, written)?;
                total_downloaded_bytes = total_downloaded_bytes.saturating_add(written);
                on_progress(&DownloadProgress {
                    asset_index,
                    asset_count: plan.assets.len(),
                    asset_key: asset.descriptor.asset_key.clone(),
                    asset_downloaded_bytes: written,
                    asset_expected_bytes: asset.expected_size_bytes,
                    total_downloaded_bytes,
                    total_expected_bytes,
                });
                ready_paths.push(store.finalize_download(
                    &asset.descriptor,
                    asset.expected_size_bytes,
                    asset.expected_sha256.as_deref(),
                )?);
                continue;
            }

            let mut request = self
                .agent
                .get(&asset.descriptor.url)
                .header("User-Agent", USER_AGENT)
                .header("Accept-Encoding", "identity");
            if offset > 0 {
                request = request.header("Range", format!("bytes={offset}-"));
                let etag = asset
                    .source_etag
                    .as_deref()
                    .ok_or_else(|| CacheError::Integrity {
                        asset_key: asset.descriptor.asset_key.clone(),
                        message: "resumed download has no strong ETag".into(),
                    })?;
                request = request.header("If-Range", etag);
            }
            let mut response = match request.call() {
                Ok(response) => response,
                Err(ureq::Error::StatusCode(status))
                    if offset > 0 && matches!(status, 412 | 416) =>
                {
                    store.discard_partial(&asset.descriptor)?;
                    return Err(CacheError::Network(format!(
                        "resumed GET {} was rejected with HTTP {status}; retry the download",
                        asset.descriptor.url
                    )));
                }
                Err(error) => return Err(network_error(error)),
            };
            let status = response.status().as_u16();
            if (offset == 0 && status != 200) || (offset > 0 && status != 206) {
                if offset > 0 {
                    store.discard_partial(&asset.descriptor)?;
                }
                return Err(CacheError::Network(format!(
                    "GET {} returned HTTP {status} for offset {offset}",
                    asset.descriptor.url
                )));
            }
            if offset > 0 {
                let content_range =
                    header_string(response.headers(), "content-range").ok_or_else(|| {
                        CacheError::Network(format!(
                            "ranged GET {} omitted Content-Range",
                            asset.descriptor.url
                        ))
                    })?;
                if !content_range_matches(&content_range, offset, asset.expected_size_bytes) {
                    store.discard_partial(&asset.descriptor)?;
                    return Err(CacheError::Network(format!(
                        "ranged GET {} returned unexpected Content-Range {content_range:?}",
                        asset.descriptor.url
                    )));
                }
            } else if asset.source_etag.as_deref().is_some_and(|etag| {
                header_string(response.headers(), "etag").as_deref() != Some(etag)
            }) {
                store.discard_partial(&asset.descriptor)?;
                return Err(CacheError::Network(format!(
                    "GET {} returned a different ETag than HEAD",
                    asset.descriptor.url
                )));
            }

            let mut output = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&partial_path)?;
            output.seek(SeekFrom::Start(offset))?;
            output.set_len(offset)?;
            let mut reader = response.body_mut().as_reader();
            let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
            let mut written = offset;
            loop {
                if cancelled.load(Ordering::Acquire) {
                    output.sync_all()?;
                    store.update_partial_size(&asset.descriptor.asset_key, written)?;
                    return Err(CacheError::Cancelled);
                }
                let count = reader.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                written =
                    written
                        .checked_add(count as u64)
                        .ok_or_else(|| CacheError::Integrity {
                            asset_key: asset.descriptor.asset_key.clone(),
                            message: "download byte counter overflowed".into(),
                        })?;
                if written > asset.expected_size_bytes {
                    output.sync_all()?;
                    drop(output);
                    std::fs::remove_file(&partial_path)?;
                    store.mark_corrupt(&asset.descriptor.asset_key)?;
                    return Err(CacheError::Integrity {
                        asset_key: asset.descriptor.asset_key.clone(),
                        message: format!(
                            "server sent more bytes than declared: {written} > {}",
                            asset.expected_size_bytes
                        ),
                    });
                }
                output.write_all(&buffer[..count])?;
                total_downloaded_bytes += count as u64;
                on_progress(&DownloadProgress {
                    asset_index,
                    asset_count: plan.assets.len(),
                    asset_key: asset.descriptor.asset_key.clone(),
                    asset_downloaded_bytes: written,
                    asset_expected_bytes: asset.expected_size_bytes,
                    total_downloaded_bytes,
                    total_expected_bytes,
                });
            }
            output.sync_all()?;
            store.update_partial_size(&asset.descriptor.asset_key, written)?;
            ready_paths.push(store.finalize_download(
                &asset.descriptor,
                asset.expected_size_bytes,
                asset.expected_sha256.as_deref(),
            )?);
        }
        Ok(ready_paths)
    }
}

pub fn execute_download_plan<F>(
    service: &Glo90DownloadService,
    store: &mut CacheStore,
    plan: &DownloadPlan,
    cancelled: &AtomicBool,
    on_progress: F,
) -> Result<Vec<std::path::PathBuf>, CacheError>
where
    F: FnMut(&DownloadProgress),
{
    service.execute(store, plan, cancelled, on_progress)
}

fn content_range_matches(value: &str, offset: u64, expected_size_bytes: u64) -> bool {
    let Some(value) = value.strip_prefix("bytes ") else {
        return false;
    };
    let Some((range, total)) = value.split_once('/') else {
        return false;
    };
    let Some((start, end)) = range.split_once('-') else {
        return false;
    };
    start.parse::<u64>().ok() == Some(offset)
        && end.parse::<u64>().ok().and_then(|end| end.checked_add(1)) == Some(expected_size_bytes)
        && total.parse::<u64>().ok() == Some(expected_size_bytes)
}

fn validate_download_url(url: &str) -> Result<(), CacheError> {
    if !url.starts_with(ALLOWED_GLO90_URL_PREFIX)
        || url.contains('@')
        || url.contains('?')
        || url.contains('#')
    {
        return Err(CacheError::InvalidInput(format!(
            "download URL is outside the pinned HTTPS allowlist: {url:?}"
        )));
    }
    Ok(())
}

fn header_string(headers: &ureq::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .map(str::to_owned)
}

fn header_u64(headers: &ureq::http::HeaderMap, name: &str) -> Result<Option<u64>, CacheError> {
    header_string(headers, name)
        .map(|value| {
            value.parse::<u64>().map_err(|_| {
                CacheError::Network(format!("invalid {name} response header {value:?}"))
            })
        })
        .transpose()
}

fn network_error(error: ureq::Error) -> CacheError {
    CacheError::Network(error.to_string())
}

fn generated_ocean_bytes(tile: DemTileId, kind: CacheKind) -> Result<Vec<u8>, CacheError> {
    match kind {
        CacheKind::Dem => encode_uniform_ocean_dem_tile(tile)
            .map_err(|error| CacheError::InvalidData(error.to_string())),
        CacheKind::Water => encode_uniform_ocean_water_tile(tile)
            .map_err(|error| CacheError::InvalidData(error.to_string())),
        _ => Err(CacheError::InvalidInput(format!(
            "cannot generate a uniform ocean asset for cache kind {}",
            kind.as_str()
        ))),
    }
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut encoded = String::with_capacity(64);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_allowlist_rejects_host_confusion_and_queries() {
        assert!(
            validate_download_url("https://copernicus-dem-90m.s3.amazonaws.com/a/b.tif").is_ok()
        );
        assert!(
            validate_download_url("https://copernicus-dem-90m.s3.amazonaws.com.evil.example/a.tif")
                .is_err()
        );
        assert!(
            validate_download_url("https://user@copernicus-dem-90m.s3.amazonaws.com/a.tif")
                .is_err()
        );
        assert!(
            validate_download_url(
                "https://copernicus-dem-90m.s3.amazonaws.com/a.tif?redirect=evil"
            )
            .is_err()
        );
    }

    #[test]
    fn content_range_must_cover_the_planned_remainder() {
        assert!(content_range_matches("bytes 40-99/100", 40, 100));
        assert!(!content_range_matches("bytes 39-99/100", 40, 100));
        assert!(!content_range_matches("bytes 40-98/100", 40, 100));
        assert!(!content_range_matches("bytes 40-99/101", 40, 100));
    }
}
