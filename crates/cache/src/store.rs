use std::collections::HashSet;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, ErrorKind, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use hamheatmap_terrain::DemTileSet;
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::planner::{AssetDescriptor, DemRegionPlan, glo90_asset, glo90_assets, glo90_wbm_asset};
use crate::{CacheError, CacheKind};

pub const TOTAL_CACHE_CAP_BYTES: u64 = 2_500_000_000;
const CACHE_SCHEMA_VERSION: i32 = 1;
const INDEX_FILE_NAME: &str = "cache.sqlite3";
const LOCK_FILE_NAME: &str = ".cache.lock";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheState {
    Missing,
    Downloading,
    Ready,
    Corrupt,
}

impl CacheState {
    fn from_str(value: &str) -> Result<Self, CacheError> {
        match value {
            "missing" => Ok(Self::Missing),
            "downloading" => Ok(Self::Downloading),
            "ready" => Ok(Self::Ready),
            "corrupt" => Ok(Self::Corrupt),
            other => Err(CacheError::InvalidData(format!(
                "unknown cache state {other:?}"
            ))),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheAsset {
    pub asset_key: String,
    pub kind: CacheKind,
    pub dataset_id: String,
    pub dataset_version: String,
    pub relative_path: String,
    pub expected_size_bytes: u64,
    pub size_bytes: u64,
    pub expected_sha256: Option<String>,
    pub sha256: Option<String>,
    pub source_etag: Option<String>,
    pub state: CacheState,
    pub last_used_unix: i64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheUsage {
    pub total_bytes: u64,
    pub basemap_bytes: u64,
    pub dem_bytes: u64,
    pub water_bytes: u64,
    pub calculation_bytes: u64,
    pub partial_bytes: u64,
    pub metadata_and_unindexed_bytes: u64,
    pub cap_bytes: u64,
}

impl CacheUsage {
    pub fn remaining_bytes(self) -> u64 {
        self.cap_bytes.saturating_sub(self.total_bytes)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeleteRegionResult {
    pub deleted_asset_count: usize,
    pub freed_bytes: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CacheRegion {
    pub region_id: String,
    pub center_lat: f64,
    pub center_lon: f64,
    pub asset_count: usize,
    pub ready_asset_count: usize,
    pub partial_asset_count: usize,
    pub referenced_bytes: u64,
    pub reclaimable_bytes: u64,
    pub created_unix: i64,
}

#[derive(Debug)]
pub struct CacheStore {
    root: PathBuf,
    connection: Connection,
    _lock_file: File,
    active_region: Option<String>,
    cap_bytes: u64,
}

impl CacheStore {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, CacheError> {
        Self::open_with_cap(root.as_ref(), TOTAL_CACHE_CAP_BYTES)
    }

    fn open_with_cap(root: &Path, cap_bytes: u64) -> Result<Self, CacheError> {
        if cap_bytes == 0 {
            return Err(CacheError::InvalidInput(
                "cache capacity must be positive".into(),
            ));
        }
        fs::create_dir_all(root)?;
        let root = fs::canonicalize(root)?;
        let lock_path = root.join(LOCK_FILE_NAME);
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock_file.try_lock().map_err(|error| {
            CacheError::InvalidData(format!(
                "cache is already open by another process or cannot be locked: {error}"
            ))
        })?;
        let connection = Connection::open(root.join(INDEX_FILE_NAME))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = DELETE;
             PRAGMA synchronous = FULL;",
        )?;
        initialize_schema(&connection)?;
        let mut store = Self {
            root,
            connection,
            _lock_file: lock_file,
            active_region: None,
            cap_bytes,
        };
        store.reconcile()?;
        store.enforce_existing_cap()?;
        Ok(store)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn upsert_region(&mut self, plan: &DemRegionPlan) -> Result<(), CacheError> {
        let mut descriptors = Vec::with_capacity(plan.tiles.len().saturating_mul(2));
        for tile in &plan.tiles {
            descriptors.extend(glo90_assets(*tile)?);
        }
        for descriptor in &descriptors {
            self.absolute_path(&descriptor.relative_path)?;
        }
        self.preflight_additional_bytes(0)?;

        let root = self.root.clone();
        let cap_bytes = self.cap_bytes;
        let transaction = self.connection.transaction()?;
        transaction.execute(
            "INSERT INTO regions (
                 region_id, center_lat, center_lon, radius_m, south, west, north, east, created_unix
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(region_id) DO UPDATE SET
                 center_lat=excluded.center_lat,
                 center_lon=excluded.center_lon,
                 south=excluded.south,
                 west=excluded.west,
                 north=excluded.north,
                 east=excluded.east",
            params![
                plan.region_id,
                plan.center.lat,
                plan.center.lon,
                crate::planner::COVERAGE_RADIUS_M,
                plan.bounds.south,
                plan.bounds.west,
                plan.bounds.north,
                plan.bounds.east,
                now_unix(),
            ],
        )?;
        for descriptor in &descriptors {
            write_asset_descriptor(&transaction, descriptor)?;
            transaction.execute(
                "INSERT OR IGNORE INTO region_assets (region_id, asset_key) VALUES (?1, ?2)",
                params![plan.region_id, descriptor.asset_key],
            )?;
        }
        enforce_root_cap(&root, cap_bytes)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_assets(&self) -> Result<Vec<CacheAsset>, CacheError> {
        let mut statement = self.connection.prepare(
            "SELECT asset_key, kind, dataset_id, dataset_version, relative_path,
                    expected_size_bytes, size_bytes, expected_sha256, sha256, source_etag,
                    state, last_used_unix
             FROM assets ORDER BY kind, dataset_id, dataset_version, asset_key",
        )?;
        let rows = statement.query_map([], row_to_asset)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CacheError::from)
    }

    pub fn usage(&self) -> Result<CacheUsage, CacheError> {
        let total_bytes = directory_size_bytes(&self.root)?;
        let mut usage = CacheUsage {
            total_bytes,
            cap_bytes: self.cap_bytes,
            ..CacheUsage::default()
        };
        let assets = self.list_assets()?;
        let mut classified_bytes = 0_u64;
        for asset in assets {
            let bytes = match asset.state {
                CacheState::Ready => asset.size_bytes,
                CacheState::Downloading => self
                    .partial_path_for_relative(&asset.relative_path)?
                    .metadata()
                    .map(|metadata| metadata.len())
                    .unwrap_or(0),
                CacheState::Missing | CacheState::Corrupt => 0,
            };
            if asset.state == CacheState::Downloading {
                usage.partial_bytes = usage.partial_bytes.saturating_add(bytes);
            } else {
                match asset.kind {
                    CacheKind::Basemap => usage.basemap_bytes += bytes,
                    CacheKind::Dem => usage.dem_bytes += bytes,
                    CacheKind::Water => usage.water_bytes += bytes,
                    CacheKind::Calculation => usage.calculation_bytes += bytes,
                    CacheKind::DownloadTemporary => usage.partial_bytes += bytes,
                }
            }
            classified_bytes = classified_bytes.saturating_add(bytes);
        }
        usage.metadata_and_unindexed_bytes = total_bytes.saturating_sub(classified_bytes);
        Ok(usage)
    }

    pub fn list_regions(&self) -> Result<Vec<CacheRegion>, CacheError> {
        let rows = {
            let mut statement = self.connection.prepare(
                "SELECT region_id, center_lat, center_lon, created_unix
                 FROM regions ORDER BY created_unix DESC, region_id",
            )?;
            let values = statement.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, f64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?;
            values.collect::<Result<Vec<_>, _>>()?
        };

        let mut regions = Vec::with_capacity(rows.len());
        for (region_id, center_lat, center_lon, created_unix) in rows {
            let asset_keys = self.region_asset_keys(&region_id)?;
            let mut ready_asset_count = 0_usize;
            let mut partial_asset_count = 0_usize;
            let mut referenced_bytes = 0_u64;
            let mut reclaimable_bytes = 0_u64;
            for asset_key in &asset_keys {
                let Some(asset) = self.asset(asset_key)? else {
                    continue;
                };
                if asset.state == CacheState::Ready {
                    ready_asset_count += 1;
                }
                let final_path = self.absolute_path(&asset.relative_path)?;
                let partial_path = self.partial_path_for_relative(&asset.relative_path)?;
                let final_bytes = final_path
                    .metadata()
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                let partial_bytes = partial_path
                    .metadata()
                    .map(|metadata| metadata.len())
                    .unwrap_or(0);
                if partial_bytes > 0 {
                    partial_asset_count += 1;
                }
                let asset_bytes = final_bytes.saturating_add(partial_bytes);
                referenced_bytes = referenced_bytes.saturating_add(asset_bytes);
                let reference_count: i64 = self.connection.query_row(
                    "SELECT COUNT(*) FROM region_assets WHERE asset_key=?1",
                    [asset_key],
                    |row| row.get(0),
                )?;
                if reference_count == 1 {
                    reclaimable_bytes = reclaimable_bytes.saturating_add(asset_bytes);
                }
            }
            regions.push(CacheRegion {
                region_id,
                center_lat,
                center_lon,
                asset_count: asset_keys.len(),
                ready_asset_count,
                partial_asset_count,
                referenced_bytes,
                reclaimable_bytes,
                created_unix,
            });
        }
        Ok(regions)
    }

    pub fn ready_paths_for_region(
        &mut self,
        plan: &DemRegionPlan,
    ) -> Result<Vec<PathBuf>, CacheError> {
        self.ready_paths_for_region_kind(plan, CacheKind::Dem)
    }

    pub fn ready_water_paths_for_region(
        &mut self,
        plan: &DemRegionPlan,
    ) -> Result<Vec<PathBuf>, CacheError> {
        self.ready_paths_for_region_kind(plan, CacheKind::Water)
    }

    fn ready_paths_for_region_kind(
        &mut self,
        plan: &DemRegionPlan,
        kind: CacheKind,
    ) -> Result<Vec<PathBuf>, CacheError> {
        let mut paths = Vec::with_capacity(plan.tiles.len());
        let mut missing = Vec::new();
        for tile in &plan.tiles {
            let descriptor = match kind {
                CacheKind::Dem => glo90_asset(*tile)?,
                CacheKind::Water => glo90_wbm_asset(*tile)?,
                _ => {
                    return Err(CacheError::InvalidInput(format!(
                        "region paths are not available for cache kind {}",
                        kind.as_str()
                    )));
                }
            };
            match self.asset(&descriptor.asset_key)? {
                Some(asset) if asset.state == CacheState::Ready => {
                    let path = self.absolute_path(&asset.relative_path)?;
                    if verify_file(&path, asset.expected_size_bytes, asset.sha256.as_deref())? {
                        paths.push(path);
                        self.connection.execute(
                            "UPDATE assets SET last_used_unix=?2 WHERE asset_key=?1",
                            params![asset.asset_key, now_unix()],
                        )?;
                    } else {
                        self.mark_corrupt(&asset.asset_key)?;
                        missing.push(asset.asset_key);
                    }
                }
                _ => missing.push(descriptor.asset_key),
            }
        }
        if missing.is_empty() {
            Ok(paths)
        } else {
            Err(CacheError::MissingAssets(missing))
        }
    }

    pub fn set_active_region(&mut self, region_id: Option<&str>) -> Result<(), CacheError> {
        if let Some(region_id) = region_id {
            let exists: bool = self.connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM regions WHERE region_id=?1)",
                [region_id],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(CacheError::InvalidInput(format!(
                    "unknown cache region {region_id:?}"
                )));
            }
        }
        self.active_region = region_id.map(str::to_owned);
        Ok(())
    }

    pub fn delete_region(&mut self, region_id: &str) -> Result<DeleteRegionResult, CacheError> {
        if self.active_region.as_deref() == Some(region_id) {
            return Err(CacheError::ActiveRegion(region_id.into()));
        }
        let asset_keys = self.region_asset_keys(region_id)?;
        if asset_keys.is_empty() {
            return Err(CacheError::InvalidInput(format!(
                "unknown or empty cache region {region_id:?}"
            )));
        }
        let mut candidates = Vec::with_capacity(asset_keys.len());
        for asset_key in &asset_keys {
            let asset = self.asset(asset_key)?.ok_or_else(|| {
                CacheError::InvalidData(format!(
                    "region {region_id:?} references missing cache asset {asset_key:?}"
                ))
            })?;
            candidates.push((
                self.absolute_path(&asset.relative_path)?,
                self.partial_path_for_relative(&asset.relative_path)?,
                asset,
            ));
        }

        let transaction = self.connection.transaction()?;
        let deleted_region_count =
            transaction.execute("DELETE FROM regions WHERE region_id=?1", [region_id])?;
        if deleted_region_count != 1 {
            return Err(CacheError::InvalidData(format!(
                "cache region {region_id:?} disappeared during deletion"
            )));
        }
        let mut result = DeleteRegionResult::default();
        for (final_path, partial_path, asset) in candidates {
            let reference_count: i64 = transaction.query_row(
                "SELECT COUNT(*) FROM region_assets WHERE asset_key=?1",
                [&asset.asset_key],
                |row| row.get(0),
            )?;
            if reference_count != 0 {
                continue;
            }
            let freed = remove_regular_file_if_present(&final_path)?
                .saturating_add(remove_regular_file_if_present(&partial_path)?);
            result.deleted_asset_count +=
                transaction.execute("DELETE FROM assets WHERE asset_key=?1", [&asset.asset_key])?;
            result.freed_bytes = result.freed_bytes.saturating_add(freed);
        }
        transaction.commit()?;
        let _ = remove_empty_directories(&self.root);
        Ok(result)
    }

    /// Moves a previously downloaded, complete GLO-90 region from another
    /// directory inside this cache root into the managed dataset layout.
    /// Each tile move is atomic and the operation is restartable.
    pub fn adopt_glo90_region(
        &mut self,
        plan: &DemRegionPlan,
        source_directory: impl AsRef<Path>,
    ) -> Result<Vec<PathBuf>, CacheError> {
        let source_directory = fs::canonicalize(source_directory.as_ref())?;
        if !source_directory.starts_with(&self.root) {
            return Err(CacheError::InvalidInput(format!(
                "adoption source must be inside cache root {}: {}",
                self.root.display(),
                source_directory.display()
            )));
        }
        let mut candidates = Vec::with_capacity(plan.tiles.len());
        let mut missing = Vec::new();
        for tile in &plan.tiles {
            let descriptor = glo90_asset(*tile)?;
            let source_path = source_directory.join(tile.filename());
            let final_path = self.absolute_path(&descriptor.relative_path)?;
            let selected = if source_path.is_file() {
                source_path
            } else if final_path.is_file() {
                final_path
            } else {
                missing.push(descriptor.asset_key);
                continue;
            };
            if fs::symlink_metadata(&selected)?.file_type().is_symlink() {
                return Err(CacheError::InvalidData(format!(
                    "symbolic links cannot be adopted: {}",
                    selected.display()
                )));
            }
            candidates.push(selected);
        }
        if !missing.is_empty() {
            return Err(CacheError::MissingAssets(missing));
        }
        // Decode all candidates before moving anything. This verifies tile
        // names, GeoTIFF structure, resolution, and cross-tile consistency.
        DemTileSet::open_paths(&candidates)
            .map_err(|error| CacheError::InvalidData(error.to_string()))?;

        self.upsert_region(plan)?;
        let mut ready_paths = Vec::with_capacity(plan.tiles.len());
        for tile in &plan.tiles {
            let descriptor = glo90_asset(*tile)?;
            self.register_asset_descriptor(&descriptor)?;
            let source_path = source_directory.join(tile.filename());
            let final_path = self.absolute_path(&descriptor.relative_path)?;
            if source_path.is_file() && source_path != final_path {
                if final_path.exists() {
                    return Err(CacheError::InvalidData(format!(
                        "both legacy and managed copies exist for {}",
                        descriptor.asset_key
                    )));
                }
                if let Some(parent) = final_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&source_path, &final_path)?;
            }
            let size_bytes = final_path.metadata()?.len();
            let digest = sha256_file(&final_path)?;
            self.connection.execute(
                "UPDATE assets SET
                     expected_size_bytes=?2,
                     size_bytes=?2,
                     expected_sha256=NULL,
                     sha256=?3,
                     state='ready',
                     last_used_unix=?4
                 WHERE asset_key=?1",
                params![
                    descriptor.asset_key,
                    to_i64(size_bytes)?,
                    digest,
                    now_unix(),
                ],
            )?;
            ready_paths.push(final_path);
        }
        remove_empty_directories(&self.root)?;
        self.enforce_existing_cap()?;
        Ok(ready_paths)
    }

    pub fn reconcile(&mut self) -> Result<(), CacheError> {
        let assets = self.list_assets()?;
        let mut registered_partials = HashSet::new();
        for asset in assets {
            let final_path = self.absolute_path(&asset.relative_path)?;
            let partial_path = self.partial_path_for_relative(&asset.relative_path)?;
            registered_partials.insert(partial_path.clone());
            match asset.state {
                CacheState::Ready => {
                    if verify_file(
                        &final_path,
                        asset.expected_size_bytes,
                        asset.sha256.as_deref(),
                    )? {
                        remove_regular_file_if_present(&partial_path)?;
                    } else {
                        self.mark_corrupt(&asset.asset_key)?;
                        remove_regular_file_if_present(&partial_path)?;
                    }
                }
                CacheState::Downloading => {
                    if final_path.is_file()
                        && verify_file(
                            &final_path,
                            asset.expected_size_bytes,
                            asset.expected_sha256.as_deref(),
                        )?
                    {
                        let digest = sha256_file(&final_path)?;
                        self.set_ready(&asset.asset_key, asset.expected_size_bytes, &digest)?;
                        remove_regular_file_if_present(&partial_path)?;
                    } else {
                        match regular_file_size(&partial_path)? {
                            Some(partial_bytes)
                                if asset.expected_size_bytes == 0
                                    || asset.size_bytes > asset.expected_size_bytes
                                    || partial_bytes > asset.expected_size_bytes =>
                            {
                                fs::remove_file(&partial_path)?;
                                self.mark_corrupt(&asset.asset_key)?;
                            }
                            Some(partial_bytes) if partial_bytes < asset.size_bytes => {
                                fs::remove_file(&partial_path)?;
                                self.mark_corrupt(&asset.asset_key)?;
                            }
                            Some(partial_bytes) if partial_bytes > asset.size_bytes => {
                                let output = OpenOptions::new().write(true).open(&partial_path)?;
                                output.set_len(asset.size_bytes)?;
                                output.sync_all()?;
                            }
                            Some(_) => {}
                            None if asset.size_bytes > 0 => self.mark_corrupt(&asset.asset_key)?,
                            None => self.update_partial_size(&asset.asset_key, 0)?,
                        }
                    }
                }
                CacheState::Missing | CacheState::Corrupt => {
                    remove_regular_file_if_present(&partial_path)?;
                }
            }
        }
        for partial_path in find_partial_files(&self.root)? {
            if !registered_partials.contains(&partial_path) {
                fs::remove_file(partial_path)?;
            }
        }
        Ok(())
    }

    pub(crate) fn asset(&self, asset_key: &str) -> Result<Option<CacheAsset>, CacheError> {
        self.connection
            .query_row(
                "SELECT asset_key, kind, dataset_id, dataset_version, relative_path,
                        expected_size_bytes, size_bytes, expected_sha256, sha256, source_etag,
                        state, last_used_unix
                 FROM assets WHERE asset_key=?1",
                [asset_key],
                row_to_asset,
            )
            .optional()
            .map_err(CacheError::from)
    }

    pub(crate) fn asset_is_ready(
        &mut self,
        descriptor: &AssetDescriptor,
    ) -> Result<bool, CacheError> {
        let Some(asset) = self.asset(&descriptor.asset_key)? else {
            return Ok(false);
        };
        if asset.state != CacheState::Ready {
            return Ok(false);
        }
        let path = self.absolute_path(&asset.relative_path)?;
        if verify_file(&path, asset.expected_size_bytes, asset.sha256.as_deref())? {
            return Ok(true);
        }
        self.mark_corrupt(&asset.asset_key)?;
        Ok(false)
    }

    pub(crate) fn resumable_bytes_for_probe(
        &mut self,
        descriptor: &AssetDescriptor,
        expected_size_bytes: u64,
        source_etag: Option<&str>,
        accepts_ranges: bool,
    ) -> Result<u64, CacheError> {
        self.register_asset_descriptor(descriptor)?;
        let partial_path = self.partial_path_for_relative(&descriptor.relative_path)?;
        let Some(partial_bytes) = regular_file_size(&partial_path)? else {
            return Ok(0);
        };
        if partial_bytes == 0 {
            return Ok(0);
        }
        let asset = self.asset(&descriptor.asset_key)?.ok_or_else(|| {
            CacheError::InvalidData(format!(
                "cache asset {} disappeared during resume validation",
                descriptor.asset_key
            ))
        })?;
        let safe_to_resume = accepts_ranges
            && partial_bytes <= expected_size_bytes
            && asset.state == CacheState::Downloading
            && asset.expected_size_bytes == expected_size_bytes
            && asset.size_bytes == partial_bytes
            && strong_etags_match(asset.source_etag.as_deref(), source_etag);
        if safe_to_resume {
            return Ok(partial_bytes);
        }
        self.discard_partial(descriptor)?;
        Ok(0)
    }

    pub(crate) fn discard_partial(&self, descriptor: &AssetDescriptor) -> Result<(), CacheError> {
        let partial_path = self.partial_path_for_relative(&descriptor.relative_path)?;
        if regular_file_size(&partial_path)?.is_some() {
            fs::remove_file(&partial_path)?;
        }
        self.connection.execute(
            "UPDATE assets SET
                 expected_size_bytes=0,
                 size_bytes=0,
                 expected_sha256=NULL,
                 sha256=NULL,
                 source_etag=NULL,
                 state='missing',
                 last_used_unix=?2
             WHERE asset_key=?1",
            params![descriptor.asset_key, now_unix()],
        )?;
        Ok(())
    }

    pub(crate) fn register_existing_managed_file(
        &mut self,
        descriptor: &AssetDescriptor,
        expected_size_bytes: u64,
        source_etag: Option<&str>,
    ) -> Result<bool, CacheError> {
        self.register_asset_descriptor(descriptor)?;
        let final_path = self.absolute_path(&descriptor.relative_path)?;
        let Ok(metadata) = final_path.metadata() else {
            return Ok(false);
        };
        if !metadata.is_file() || metadata.len() != expected_size_bytes {
            self.mark_corrupt(&descriptor.asset_key)?;
            return Ok(false);
        }
        let digest = sha256_file(&final_path)?;
        self.connection.execute(
            "UPDATE assets SET
                 expected_size_bytes=?2,
                 size_bytes=?2,
                 expected_sha256=NULL,
                 sha256=?3,
                 source_etag=?4,
                 state='ready',
                 last_used_unix=?5
             WHERE asset_key=?1",
            params![
                descriptor.asset_key,
                to_i64(expected_size_bytes)?,
                digest,
                source_etag,
                now_unix(),
            ],
        )?;
        Ok(true)
    }

    pub(crate) fn prepare_download(
        &mut self,
        descriptor: &AssetDescriptor,
        expected_size_bytes: u64,
        expected_sha256: Option<&str>,
        source_etag: Option<&str>,
        planned_resumable_bytes: u64,
    ) -> Result<(PathBuf, u64), CacheError> {
        if expected_size_bytes == 0 {
            return Err(CacheError::InvalidInput(format!(
                "asset {} has zero expected size",
                descriptor.asset_key
            )));
        }
        if planned_resumable_bytes > expected_size_bytes {
            return Err(CacheError::InvalidInput(format!(
                "planned resume offset exceeds expected size for {}",
                descriptor.asset_key
            )));
        }
        validate_sha256(expected_sha256)?;
        self.register_asset_descriptor(descriptor)?;
        let partial_path = self.partial_path_for_relative(&descriptor.relative_path)?;
        if let Some(parent) = partial_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let current_partial_bytes = regular_file_size(&partial_path)?.unwrap_or(0);
        if current_partial_bytes != planned_resumable_bytes {
            return Err(CacheError::Integrity {
                asset_key: descriptor.asset_key.clone(),
                message: format!(
                    "partial file changed after planning: actual={current_partial_bytes}, planned={planned_resumable_bytes}"
                ),
            });
        }
        if planned_resumable_bytes > 0 {
            let asset = self.asset(&descriptor.asset_key)?.ok_or_else(|| {
                CacheError::InvalidData(format!(
                    "cache asset {} disappeared before resume",
                    descriptor.asset_key
                ))
            })?;
            if asset.state != CacheState::Downloading
                || asset.expected_size_bytes != expected_size_bytes
                || asset.size_bytes != planned_resumable_bytes
                || !strong_etags_match(asset.source_etag.as_deref(), source_etag)
            {
                return Err(CacheError::Integrity {
                    asset_key: descriptor.asset_key.clone(),
                    message: "resume metadata changed after planning".into(),
                });
            }
        }
        let additional_bytes = expected_size_bytes - planned_resumable_bytes;
        self.preflight_additional_bytes(additional_bytes)?;
        self.connection.execute(
            "UPDATE assets SET
                 expected_size_bytes=?2,
                 expected_sha256=?3,
                 source_etag=?4,
                 size_bytes=?5,
                 state='downloading',
                 last_used_unix=?6
             WHERE asset_key=?1",
            params![
                descriptor.asset_key,
                to_i64(expected_size_bytes)?,
                expected_sha256,
                source_etag,
                to_i64(planned_resumable_bytes)?,
                now_unix(),
            ],
        )?;
        Ok((partial_path, planned_resumable_bytes))
    }

    pub(crate) fn preflight_additional_capacity(
        &self,
        additional_bytes: u64,
    ) -> Result<(), CacheError> {
        self.preflight_additional_bytes(additional_bytes)
    }

    pub(crate) fn finalize_download(
        &mut self,
        descriptor: &AssetDescriptor,
        expected_size_bytes: u64,
        expected_sha256: Option<&str>,
    ) -> Result<PathBuf, CacheError> {
        let partial_path = self.partial_path_for_relative(&descriptor.relative_path)?;
        let final_path = self.absolute_path(&descriptor.relative_path)?;
        let actual_size = partial_path.metadata()?.len();
        if actual_size != expected_size_bytes {
            return Err(CacheError::Integrity {
                asset_key: descriptor.asset_key.clone(),
                message: format!(
                    "download size mismatch: actual={actual_size}, expected={expected_size_bytes}"
                ),
            });
        }
        let digest = sha256_file(&partial_path)?;
        if expected_sha256.is_some_and(|expected| !expected.eq_ignore_ascii_case(&digest)) {
            fs::remove_file(&partial_path)?;
            self.mark_corrupt(&descriptor.asset_key)?;
            return Err(CacheError::Integrity {
                asset_key: descriptor.asset_key.clone(),
                message: format!("SHA-256 mismatch: actual={digest}"),
            });
        }
        if final_path.exists() {
            fs::remove_file(&final_path)?;
        }
        fs::rename(&partial_path, &final_path)?;
        self.set_ready(&descriptor.asset_key, actual_size, &digest)?;
        self.enforce_existing_cap()?;
        Ok(final_path)
    }

    pub(crate) fn update_partial_size(
        &self,
        asset_key: &str,
        size_bytes: u64,
    ) -> Result<(), CacheError> {
        self.connection.execute(
            "UPDATE assets SET size_bytes=?2, last_used_unix=?3 WHERE asset_key=?1",
            params![asset_key, to_i64(size_bytes)?, now_unix()],
        )?;
        Ok(())
    }

    pub(crate) fn mark_corrupt(&self, asset_key: &str) -> Result<(), CacheError> {
        self.connection.execute(
            "UPDATE assets SET state='corrupt', size_bytes=0, sha256=NULL, last_used_unix=?2
             WHERE asset_key=?1",
            params![asset_key, now_unix()],
        )?;
        Ok(())
    }

    fn register_asset_descriptor(
        &mut self,
        descriptor: &AssetDescriptor,
    ) -> Result<(), CacheError> {
        self.absolute_path(&descriptor.relative_path)?;
        self.preflight_additional_bytes(0)?;

        let root = self.root.clone();
        let cap_bytes = self.cap_bytes;
        let transaction = self.connection.transaction()?;
        write_asset_descriptor(&transaction, descriptor)?;
        enforce_root_cap(&root, cap_bytes)?;
        transaction.commit()?;
        Ok(())
    }

    fn set_ready(&self, asset_key: &str, size_bytes: u64, digest: &str) -> Result<(), CacheError> {
        self.connection.execute(
            "UPDATE assets SET size_bytes=?2, sha256=?3, state='ready', last_used_unix=?4
             WHERE asset_key=?1",
            params![asset_key, to_i64(size_bytes)?, digest, now_unix()],
        )?;
        Ok(())
    }

    fn region_asset_keys(&self, region_id: &str) -> Result<Vec<String>, CacheError> {
        let mut statement = self
            .connection
            .prepare("SELECT asset_key FROM region_assets WHERE region_id=?1 ORDER BY asset_key")?;
        let rows = statement.query_map([region_id], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CacheError::from)
    }

    fn preflight_additional_bytes(&self, additional_bytes: u64) -> Result<(), CacheError> {
        let usage = self.usage()?;
        let headroom = (self.cap_bytes / 100).min(16_000_000);
        if usage
            .total_bytes
            .saturating_add(additional_bytes)
            .saturating_add(headroom)
            > self.cap_bytes
        {
            return Err(CacheError::QuotaExceeded {
                current_bytes: usage.total_bytes,
                requested_additional_bytes: additional_bytes,
                cap_bytes: self.cap_bytes,
            });
        }
        let available_bytes = fs4::available_space(&self.root)?;
        if available_bytes < additional_bytes.saturating_add(headroom) {
            return Err(CacheError::DiskSpaceInsufficient {
                available_bytes,
                requested_additional_bytes: additional_bytes,
            });
        }
        Ok(())
    }

    fn enforce_existing_cap(&self) -> Result<(), CacheError> {
        let usage = self.usage()?;
        if usage.total_bytes > self.cap_bytes {
            return Err(CacheError::QuotaExceeded {
                current_bytes: usage.total_bytes,
                requested_additional_bytes: 0,
                cap_bytes: self.cap_bytes,
            });
        }
        Ok(())
    }

    fn absolute_path(&self, relative_path: &str) -> Result<PathBuf, CacheError> {
        let path = Path::new(relative_path);
        if path.as_os_str().is_empty() || path.is_absolute() {
            return Err(CacheError::InvalidInput(format!(
                "cache path must be a non-empty relative path: {relative_path:?}"
            )));
        }
        for component in path.components() {
            if !matches!(component, Component::Normal(_)) {
                return Err(CacheError::InvalidInput(format!(
                    "cache path contains an unsafe component: {relative_path:?}"
                )));
            }
        }
        Ok(self.root.join(path))
    }

    fn partial_path_for_relative(&self, relative_path: &str) -> Result<PathBuf, CacheError> {
        let final_path = self.absolute_path(relative_path)?;
        let mut partial_name = OsString::from(final_path.as_os_str());
        partial_name.push(".partial");
        Ok(PathBuf::from(partial_name))
    }
}

fn write_asset_descriptor(
    connection: &Connection,
    descriptor: &AssetDescriptor,
) -> Result<(), CacheError> {
    connection.execute(
        "INSERT INTO assets (
             asset_key, kind, dataset_id, dataset_version, relative_path,
             expected_size_bytes, size_bytes, state, last_used_unix
         ) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 'missing', ?6)
         ON CONFLICT(asset_key) DO UPDATE SET
             kind=excluded.kind,
             dataset_id=excluded.dataset_id,
             dataset_version=excluded.dataset_version,
             relative_path=excluded.relative_path",
        params![
            descriptor.asset_key,
            descriptor.kind.as_str(),
            descriptor.dataset_id,
            descriptor.dataset_version,
            descriptor.relative_path,
            now_unix(),
        ],
    )?;
    Ok(())
}

fn enforce_root_cap(root: &Path, cap_bytes: u64) -> Result<(), CacheError> {
    let current_bytes = directory_size_bytes(root)?;
    if current_bytes > cap_bytes {
        return Err(CacheError::QuotaExceeded {
            current_bytes,
            requested_additional_bytes: 0,
            cap_bytes,
        });
    }
    Ok(())
}

fn initialize_schema(connection: &Connection) -> Result<(), CacheError> {
    let version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > CACHE_SCHEMA_VERSION {
        return Err(CacheError::InvalidData(format!(
            "cache schema {version} is newer than supported schema {CACHE_SCHEMA_VERSION}"
        )));
    }
    if version == 0 {
        connection.execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE assets (
                 asset_key TEXT PRIMARY KEY,
                 kind TEXT NOT NULL,
                 dataset_id TEXT NOT NULL,
                 dataset_version TEXT NOT NULL,
                 relative_path TEXT NOT NULL UNIQUE,
                 expected_size_bytes INTEGER NOT NULL,
                 size_bytes INTEGER NOT NULL,
                 expected_sha256 TEXT,
                 sha256 TEXT,
                 source_etag TEXT,
                 state TEXT NOT NULL,
                 last_used_unix INTEGER NOT NULL
             );
             CREATE TABLE regions (
                 region_id TEXT PRIMARY KEY,
                 center_lat REAL NOT NULL,
                 center_lon REAL NOT NULL,
                 radius_m REAL NOT NULL,
                 south REAL NOT NULL,
                 west REAL NOT NULL,
                 north REAL NOT NULL,
                 east REAL NOT NULL,
                 created_unix INTEGER NOT NULL
             );
             CREATE TABLE region_assets (
                 region_id TEXT NOT NULL REFERENCES regions(region_id) ON DELETE CASCADE,
                 asset_key TEXT NOT NULL REFERENCES assets(asset_key) ON DELETE CASCADE,
                 PRIMARY KEY(region_id, asset_key)
             );
             PRAGMA user_version = 1;
             COMMIT;",
        )?;
    }
    Ok(())
}

fn row_to_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<CacheAsset> {
    let kind: String = row.get(1)?;
    let state: String = row.get(10)?;
    let expected_size: i64 = row.get(5)?;
    let size: i64 = row.get(6)?;
    let kind = CacheKind::from_str(&kind).map_err(to_sql_conversion_error)?;
    let state = CacheState::from_str(&state).map_err(to_sql_conversion_error)?;
    if expected_size < 0 || size < 0 {
        return Err(to_sql_conversion_error(CacheError::InvalidData(
            "cache index contains a negative file size".into(),
        )));
    }
    Ok(CacheAsset {
        asset_key: row.get(0)?,
        kind,
        dataset_id: row.get(2)?,
        dataset_version: row.get(3)?,
        relative_path: row.get(4)?,
        expected_size_bytes: expected_size as u64,
        size_bytes: size as u64,
        expected_sha256: row.get(7)?,
        sha256: row.get(8)?,
        source_etag: row.get(9)?,
        state,
        last_used_unix: row.get(11)?,
    })
}

fn to_sql_conversion_error(error: CacheError) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(error))
}

fn to_i64(value: u64) -> Result<i64, CacheError> {
    i64::try_from(value)
        .map_err(|_| CacheError::InvalidInput(format!("byte count is too large: {value}")))
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn validate_sha256(value: Option<&str>) -> Result<(), CacheError> {
    if value.is_some_and(|value| {
        value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
    }) {
        return Err(CacheError::InvalidInput(
            "expected SHA-256 must contain exactly 64 hexadecimal characters".into(),
        ));
    }
    Ok(())
}

fn strong_etags_match(previous: Option<&str>, current: Option<&str>) -> bool {
    match (previous.map(str::trim), current.map(str::trim)) {
        (Some(previous), Some(current)) => {
            !previous.is_empty()
                && !previous
                    .get(..2)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("w/"))
                && !current.is_empty()
                && !current
                    .get(..2)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("w/"))
                && previous == current
        }
        _ => false,
    }
}

fn regular_file_size(path: &Path) -> Result<Option<u64>, CacheError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(metadata.len())),
        Ok(_) => Err(CacheError::InvalidData(format!(
            "cache path is not a regular file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CacheError::Io(error)),
    }
}

fn remove_regular_file_if_present(path: &Path) -> Result<u64, CacheError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            let size = metadata.len();
            fs::remove_file(path)?;
            Ok(size)
        }
        Ok(_) => Err(CacheError::InvalidData(format!(
            "cache path is not a regular file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(0),
        Err(error) => Err(CacheError::Io(error)),
    }
}

fn sha256_file(path: &Path) -> Result<String, CacheError> {
    let mut reader = BufReader::new(File::open(path)?);
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 128 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let bytes = digest.finalize();
    let mut hexadecimal = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut hexadecimal, "{byte:02x}")
            .map_err(|error| CacheError::InvalidData(error.to_string()))?;
    }
    Ok(hexadecimal)
}

fn verify_file(
    path: &Path,
    expected_size: u64,
    expected_sha: Option<&str>,
) -> Result<bool, CacheError> {
    let Ok(metadata) = path.metadata() else {
        return Ok(false);
    };
    if !metadata.is_file() || metadata.len() != expected_size {
        return Ok(false);
    }
    if let Some(expected_sha) = expected_sha {
        return Ok(expected_sha.eq_ignore_ascii_case(&sha256_file(path)?));
    }
    Ok(true)
}

fn directory_size_bytes(root: &Path) -> Result<u64, CacheError> {
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                return Err(CacheError::InvalidData(format!(
                    "symbolic links are not allowed inside the cache: {}",
                    entry.path().display()
                )));
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.checked_add(metadata.len()).ok_or_else(|| {
                    CacheError::InvalidData("cache size counter overflowed".into())
                })?;
            }
        }
    }
    Ok(total)
}

fn find_partial_files(root: &Path) -> Result<Vec<PathBuf>, CacheError> {
    let mut partials = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file()
                && entry.file_name().to_string_lossy().ends_with(".partial")
            {
                partials.push(entry.path());
            }
        }
    }
    Ok(partials)
}

fn remove_empty_directories(root: &Path) -> Result<(), CacheError> {
    let mut directories = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
                directories.push(entry.path());
            }
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        if fs::read_dir(&directory)?.next().is_none() {
            fs::remove_dir(directory)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::planner::{GeoPoint, plan_glo90_region};

    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(name: &str) -> Self {
            let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "hamheatmap-cache-{name}-{}-{unique}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn one_tile_plan(region_id: &str) -> DemRegionPlan {
        let mut plan = plan_glo90_region(GeoPoint {
            lat: 30.5,
            lon: 103.5,
        })
        .unwrap();
        plan.region_id = region_id.into();
        plan.tiles.truncate(1);
        plan
    }

    #[test]
    fn quota_preflight_counts_partial_growth_and_metadata_headroom() {
        let directory = TestDirectory::new("quota");
        let mut store = CacheStore::open_with_cap(&directory.0, 100_000).unwrap();
        let plan = one_tile_plan("quota-region");
        store.upsert_region(&plan).unwrap();
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let usage = store.usage().unwrap();
        let too_large = 100_000 - usage.total_bytes + 1;
        let error = store
            .prepare_download(&descriptor, too_large, None, None, 0)
            .unwrap_err();
        assert!(matches!(error, CacheError::QuotaExceeded { .. }));
    }

    #[test]
    fn finalization_is_atomic_and_records_sha256() {
        let directory = TestDirectory::new("finalize");
        let mut store = CacheStore::open(&directory.0).unwrap();
        let plan = one_tile_plan("finalize-region");
        store.upsert_region(&plan).unwrap();
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let (partial_path, offset) = store
            .prepare_download(&descriptor, 5, None, Some("test-etag"), 0)
            .unwrap();
        assert_eq!(offset, 0);
        File::create(&partial_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();
        let final_path = store.finalize_download(&descriptor, 5, None).unwrap();
        assert!(final_path.is_file());
        assert!(!partial_path.exists());
        let asset = store.asset(&descriptor.asset_key).unwrap().unwrap();
        assert_eq!(asset.state, CacheState::Ready);
        assert_eq!(
            asset.sha256.as_deref(),
            Some("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
    }

    #[test]
    fn checksum_mismatch_never_becomes_ready() {
        let directory = TestDirectory::new("checksum");
        let mut store = CacheStore::open(&directory.0).unwrap();
        let plan = one_tile_plan("checksum-region");
        store.upsert_region(&plan).unwrap();
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let wrong_hash = "0".repeat(64);
        let (partial_path, _) = store
            .prepare_download(&descriptor, 5, Some(&wrong_hash), None, 0)
            .unwrap();
        File::create(&partial_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();
        let error = store
            .finalize_download(&descriptor, 5, Some(&wrong_hash))
            .unwrap_err();
        assert!(matches!(error, CacheError::Integrity { .. }));
        assert!(!partial_path.exists());
        assert_eq!(
            store.asset(&descriptor.asset_key).unwrap().unwrap().state,
            CacheState::Corrupt
        );
    }

    #[test]
    fn shared_tiles_survive_until_last_region_is_deleted() {
        let directory = TestDirectory::new("sharing");
        let mut store = CacheStore::open(&directory.0).unwrap();
        let first = one_tile_plan("first");
        let mut second = first.clone();
        second.region_id = "second".into();
        store.upsert_region(&first).unwrap();
        store.upsert_region(&second).unwrap();
        let descriptor = glo90_asset(first.tiles[0]).unwrap();
        let (partial_path, _) = store
            .prepare_download(&descriptor, 5, None, None, 0)
            .unwrap();
        File::create(&partial_path)
            .unwrap()
            .write_all(b"hello")
            .unwrap();
        let final_path = store.finalize_download(&descriptor, 5, None).unwrap();

        let regions = store.list_regions().unwrap();
        assert_eq!(regions.len(), 2);
        assert!(regions.iter().all(|region| region.referenced_bytes == 5));
        assert!(regions.iter().all(|region| region.reclaimable_bytes == 0));

        let first_result = store.delete_region("first").unwrap();
        assert_eq!(first_result.deleted_asset_count, 0);
        assert!(final_path.exists());
        let remaining = store.list_regions().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].reclaimable_bytes, 5);
        let second_result = store.delete_region("second").unwrap();
        assert_eq!(second_result.deleted_asset_count, 2);
        assert_eq!(second_result.freed_bytes, 5);
        assert!(!final_path.exists());
    }

    #[test]
    fn failed_file_removal_keeps_region_indexed_and_deletion_retryable() {
        let directory = TestDirectory::new("delete-retry");
        let mut store = CacheStore::open(&directory.0).unwrap();
        let plan = one_tile_plan("delete-retry");
        store.upsert_region(&plan).unwrap();
        let descriptors = glo90_assets(plan.tiles[0]).unwrap();
        for descriptor in &descriptors {
            let (partial_path, _) = store
                .prepare_download(descriptor, 5, None, None, 0)
                .unwrap();
            File::create(&partial_path)
                .unwrap()
                .write_all(b"hello")
                .unwrap();
            store.finalize_download(descriptor, 5, None).unwrap();
        }

        let asset_keys = store.region_asset_keys("delete-retry").unwrap();
        let first = store.asset(&asset_keys[0]).unwrap().unwrap();
        let second = store.asset(&asset_keys[1]).unwrap().unwrap();
        let first_path = store.absolute_path(&first.relative_path).unwrap();
        let second_path = store.absolute_path(&second.relative_path).unwrap();
        fs::remove_file(&second_path).unwrap();
        fs::create_dir(&second_path).unwrap();

        assert!(store.delete_region("delete-retry").is_err());
        assert!(!first_path.exists());
        assert_eq!(store.list_regions().unwrap().len(), 1);
        assert_eq!(store.region_asset_keys("delete-retry").unwrap().len(), 2);
        assert!(store.asset(&asset_keys[0]).unwrap().is_some());
        assert!(store.asset(&asset_keys[1]).unwrap().is_some());

        fs::remove_dir(&second_path).unwrap();
        let result = store.delete_region("delete-retry").unwrap();
        assert_eq!(result.deleted_asset_count, 2);
        assert!(store.list_regions().unwrap().is_empty());
        assert!(store.asset(&asset_keys[0]).unwrap().is_none());
        assert!(store.asset(&asset_keys[1]).unwrap().is_none());
    }

    #[test]
    fn resume_requires_matching_strong_etag_size_state_and_range_support() {
        let directory = TestDirectory::new("resume-safe");
        let mut store = CacheStore::open(&directory.0).unwrap();
        let plan = one_tile_plan("resume-safe");
        store.upsert_region(&plan).unwrap();
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let (partial_path, _) = store
            .prepare_download(&descriptor, 100, None, Some("\"v1\""), 0)
            .unwrap();
        File::create(&partial_path)
            .unwrap()
            .write_all(&[7_u8; 40])
            .unwrap();
        store
            .update_partial_size(&descriptor.asset_key, 40)
            .unwrap();

        assert_eq!(
            store
                .resumable_bytes_for_probe(&descriptor, 100, Some("\"v1\""), true)
                .unwrap(),
            40
        );
        assert!(partial_path.is_file());
    }

    #[test]
    fn unsafe_partial_is_discarded_before_capacity_is_estimated() {
        let directory = TestDirectory::new("resume-discard");
        let mut store = CacheStore::open(&directory.0).unwrap();
        let plan = one_tile_plan("resume-discard");
        store.upsert_region(&plan).unwrap();
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let (partial_path, _) = store
            .prepare_download(&descriptor, 100, None, Some("\"old\""), 0)
            .unwrap();
        File::create(&partial_path)
            .unwrap()
            .write_all(&[9_u8; 40])
            .unwrap();
        store
            .update_partial_size(&descriptor.asset_key, 40)
            .unwrap();

        assert_eq!(
            store
                .resumable_bytes_for_probe(&descriptor, 100, Some("\"new\""), true)
                .unwrap(),
            0
        );
        assert!(!partial_path.exists());
        let asset = store.asset(&descriptor.asset_key).unwrap().unwrap();
        assert_eq!(asset.state, CacheState::Missing);
        assert_eq!(asset.size_bytes, 0);
    }

    #[test]
    fn weak_etag_or_missing_range_support_never_resumes() {
        assert!(!strong_etags_match(Some("W/\"v1\""), Some("W/\"v1\"")));
        assert!(!strong_etags_match(Some("\"v1\""), Some("W/\"v1\"")));
        assert!(strong_etags_match(Some("\"v1\""), Some("\"v1\"")));

        let directory = TestDirectory::new("resume-no-range");
        let mut store = CacheStore::open(&directory.0).unwrap();
        let plan = one_tile_plan("resume-no-range");
        store.upsert_region(&plan).unwrap();
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let (partial_path, _) = store
            .prepare_download(&descriptor, 100, None, Some("\"v1\""), 0)
            .unwrap();
        File::create(&partial_path)
            .unwrap()
            .write_all(&[1_u8; 10])
            .unwrap();
        store
            .update_partial_size(&descriptor.asset_key, 10)
            .unwrap();
        assert_eq!(
            store
                .resumable_bytes_for_probe(&descriptor, 100, Some("\"v1\""), false)
                .unwrap(),
            0
        );
        assert!(!partial_path.exists());
    }

    #[test]
    fn active_region_cannot_be_deleted() {
        let directory = TestDirectory::new("active");
        let mut store = CacheStore::open(&directory.0).unwrap();
        let plan = one_tile_plan("active");
        store.upsert_region(&plan).unwrap();
        store.set_active_region(Some("active")).unwrap();
        assert!(matches!(
            store.delete_region("active"),
            Err(CacheError::ActiveRegion(_))
        ));
    }

    #[test]
    fn restart_only_trusts_checkpointed_partial_length() {
        let directory = TestDirectory::new("restart-partial-growth");
        let plan = one_tile_plan("restart-partial-growth");
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let partial_path = {
            let mut store = CacheStore::open(&directory.0).unwrap();
            store.upsert_region(&plan).unwrap();
            let (partial_path, _) = store
                .prepare_download(&descriptor, 100, None, Some("\"v1\""), 0)
                .unwrap();
            File::create(&partial_path)
                .unwrap()
                .write_all(&[3_u8; 40])
                .unwrap();
            store
                .update_partial_size(&descriptor.asset_key, 20)
                .unwrap();
            partial_path
        };

        let mut reopened = CacheStore::open(&directory.0).unwrap();
        let asset = reopened.asset(&descriptor.asset_key).unwrap().unwrap();
        assert_eq!(asset.state, CacheState::Downloading);
        assert_eq!(asset.size_bytes, 20);
        assert_eq!(partial_path.metadata().unwrap().len(), 20);
        assert_eq!(
            reopened
                .resumable_bytes_for_probe(&descriptor, 100, Some("\"v1\""), true)
                .unwrap(),
            20
        );

        let short_directory = TestDirectory::new("restart-partial-short");
        let short_plan = one_tile_plan("restart-partial-short");
        let short_descriptor = glo90_asset(short_plan.tiles[0]).unwrap();
        let short_partial_path = {
            let mut store = CacheStore::open(&short_directory.0).unwrap();
            store.upsert_region(&short_plan).unwrap();
            let (partial_path, _) = store
                .prepare_download(&short_descriptor, 100, None, Some("\"v1\""), 0)
                .unwrap();
            File::create(&partial_path)
                .unwrap()
                .write_all(&[3_u8; 20])
                .unwrap();
            store
                .update_partial_size(&short_descriptor.asset_key, 40)
                .unwrap();
            partial_path
        };

        let reopened = CacheStore::open(&short_directory.0).unwrap();
        let asset = reopened
            .asset(&short_descriptor.asset_key)
            .unwrap()
            .unwrap();
        assert_eq!(asset.state, CacheState::Corrupt);
        assert_eq!(asset.size_bytes, 0);
        assert!(!short_partial_path.exists());

        let missing_directory = TestDirectory::new("restart-partial-missing");
        let missing_plan = one_tile_plan("restart-partial-missing");
        let missing_descriptor = glo90_asset(missing_plan.tiles[0]).unwrap();
        {
            let mut store = CacheStore::open(&missing_directory.0).unwrap();
            store.upsert_region(&missing_plan).unwrap();
            store
                .prepare_download(&missing_descriptor, 100, None, Some("\"v1\""), 0)
                .unwrap();
            store
                .update_partial_size(&missing_descriptor.asset_key, 40)
                .unwrap();
        }

        let mut reopened = CacheStore::open(&missing_directory.0).unwrap();
        let asset = reopened
            .asset(&missing_descriptor.asset_key)
            .unwrap()
            .unwrap();
        assert_eq!(asset.state, CacheState::Corrupt);
        assert_eq!(asset.size_bytes, 0);
        assert_eq!(
            reopened
                .resumable_bytes_for_probe(&missing_descriptor, 100, Some("\"v1\""), true)
                .unwrap(),
            0
        );
    }

    #[test]
    fn restart_removes_stale_partial_for_ready_asset() {
        let directory = TestDirectory::new("restart-ready-partial");
        let plan = one_tile_plan("restart-ready-partial");
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let partial_path = {
            let mut store = CacheStore::open(&directory.0).unwrap();
            store.upsert_region(&plan).unwrap();
            let (partial_path, _) = store
                .prepare_download(&descriptor, 5, None, Some("\"v1\""), 0)
                .unwrap();
            File::create(&partial_path)
                .unwrap()
                .write_all(b"hello")
                .unwrap();
            store.finalize_download(&descriptor, 5, None).unwrap();
            File::create(&partial_path)
                .unwrap()
                .write_all(b"stale")
                .unwrap();
            partial_path
        };

        let reopened = CacheStore::open(&directory.0).unwrap();
        assert!(!partial_path.exists());
        assert_eq!(
            reopened
                .asset(&descriptor.asset_key)
                .unwrap()
                .unwrap()
                .state,
            CacheState::Ready
        );
        assert_eq!(reopened.usage().unwrap().partial_bytes, 0);
    }

    #[test]
    fn restart_cleans_stale_partial_before_enforcing_hard_cap() {
        const TEST_CAP_BYTES: u64 = 100_000;
        let directory = TestDirectory::new("restart-hard-cap");
        let plan = one_tile_plan("restart-hard-cap");
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let partial_path = {
            let mut store = CacheStore::open_with_cap(&directory.0, TEST_CAP_BYTES).unwrap();
            store.upsert_region(&plan).unwrap();
            let partial_path = store
                .partial_path_for_relative(&descriptor.relative_path)
                .unwrap();
            fs::create_dir_all(partial_path.parent().unwrap()).unwrap();
            let usage = store.usage().unwrap();
            File::create(&partial_path)
                .unwrap()
                .set_len(TEST_CAP_BYTES - usage.total_bytes + 1)
                .unwrap();
            assert!(store.usage().unwrap().total_bytes > TEST_CAP_BYTES);
            partial_path
        };

        let reopened = CacheStore::open_with_cap(&directory.0, TEST_CAP_BYTES).unwrap();
        assert!(!partial_path.exists());
        assert!(reopened.usage().unwrap().total_bytes <= TEST_CAP_BYTES);
    }

    #[test]
    fn restart_finishes_atomic_rename_left_before_index_update() {
        let directory = TestDirectory::new("restart-after-rename");
        let plan = one_tile_plan("restart-after-rename");
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let final_path = {
            let mut store = CacheStore::open(&directory.0).unwrap();
            store.upsert_region(&plan).unwrap();
            let (partial_path, _) = store
                .prepare_download(&descriptor, 5, None, Some("\"v1\""), 0)
                .unwrap();
            File::create(&partial_path)
                .unwrap()
                .write_all(b"hello")
                .unwrap();
            let final_path = store.absolute_path(&descriptor.relative_path).unwrap();
            fs::rename(&partial_path, &final_path).unwrap();
            final_path
        };

        let reopened = CacheStore::open(&directory.0).unwrap();
        let asset = reopened.asset(&descriptor.asset_key).unwrap().unwrap();
        assert_eq!(asset.state, CacheState::Ready);
        assert_eq!(asset.size_bytes, 5);
        assert!(final_path.is_file());
    }

    #[test]
    fn trusted_partial_at_cap_reopens_but_one_byte_more_is_rejected() {
        const TEST_CAP_BYTES: u64 = 100_000;
        let directory = TestDirectory::new("trusted-partial-hard-cap");
        let plan = one_tile_plan("trusted-partial-hard-cap");
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let partial_path = {
            let mut store = CacheStore::open_with_cap(&directory.0, TEST_CAP_BYTES).unwrap();
            store.upsert_region(&plan).unwrap();
            store
                .connection
                .execute(
                    "UPDATE assets SET
                         expected_size_bytes=?2,
                         size_bytes=0,
                         source_etag='\"v1\"',
                         state='downloading'
                     WHERE asset_key=?1",
                    params![descriptor.asset_key, to_i64(TEST_CAP_BYTES * 2).unwrap()],
                )
                .unwrap();
            let partial_path = store
                .partial_path_for_relative(&descriptor.relative_path)
                .unwrap();
            fs::create_dir_all(partial_path.parent().unwrap()).unwrap();
            let baseline = store.usage().unwrap().total_bytes;
            let partial_bytes = TEST_CAP_BYTES - baseline;
            File::create(&partial_path)
                .unwrap()
                .set_len(partial_bytes)
                .unwrap();
            store
                .update_partial_size(&descriptor.asset_key, partial_bytes)
                .unwrap();
            assert_eq!(store.usage().unwrap().total_bytes, TEST_CAP_BYTES);
            partial_path
        };

        let reopened = CacheStore::open_with_cap(&directory.0, TEST_CAP_BYTES).unwrap();
        assert_eq!(reopened.usage().unwrap().total_bytes, TEST_CAP_BYTES);
        assert_eq!(
            reopened
                .asset(&descriptor.asset_key)
                .unwrap()
                .unwrap()
                .state,
            CacheState::Downloading
        );

        let current_partial_bytes = partial_path.metadata().unwrap().len();
        OpenOptions::new()
            .write(true)
            .open(&partial_path)
            .unwrap()
            .set_len(current_partial_bytes + 1)
            .unwrap();
        drop(reopened);
        let reopened = CacheStore::open_with_cap(&directory.0, TEST_CAP_BYTES).unwrap();
        assert_eq!(
            partial_path.metadata().unwrap().len(),
            current_partial_bytes
        );
        assert_eq!(reopened.usage().unwrap().total_bytes, TEST_CAP_BYTES);

        OpenOptions::new()
            .write(true)
            .open(&partial_path)
            .unwrap()
            .set_len(current_partial_bytes + 1)
            .unwrap();
        reopened
            .update_partial_size(&descriptor.asset_key, current_partial_bytes + 1)
            .unwrap();
        drop(reopened);
        let error = CacheStore::open_with_cap(&directory.0, TEST_CAP_BYTES).unwrap_err();
        assert!(matches!(
            error,
            CacheError::QuotaExceeded {
                current_bytes,
                requested_additional_bytes: 0,
                cap_bytes: TEST_CAP_BYTES,
            } if current_bytes == TEST_CAP_BYTES + 1
        ));
        assert!(partial_path.is_file());
    }

    #[test]
    fn metadata_headroom_guard_rejects_without_index_changes() {
        const TEST_CAP_BYTES: u64 = 100_000;
        let directory = TestDirectory::new("metadata-headroom-guard");
        let mut store = CacheStore::open_with_cap(&directory.0, TEST_CAP_BYTES).unwrap();
        let plan = one_tile_plan("metadata-headroom-guard");
        let descriptor = glo90_asset(plan.tiles[0]).unwrap();
        let headroom = (TEST_CAP_BYTES / 100).min(16_000_000);
        let baseline = store.usage().unwrap().total_bytes;
        let padding_path = directory.0.join("near-cap-padding.bin");
        File::create(&padding_path)
            .unwrap()
            .set_len(TEST_CAP_BYTES - baseline - headroom + 1)
            .unwrap();
        let usage_before = store.usage().unwrap();
        assert!(usage_before.total_bytes <= TEST_CAP_BYTES);
        assert!(usage_before.total_bytes + headroom > TEST_CAP_BYTES);

        assert!(matches!(
            store.upsert_region(&plan),
            Err(CacheError::QuotaExceeded { .. })
        ));
        assert_eq!(store.usage().unwrap(), usage_before);
        assert!(store.list_regions().unwrap().is_empty());
        assert!(store.list_assets().unwrap().is_empty());

        assert!(matches!(
            store.resumable_bytes_for_probe(&descriptor, 100, Some("\"v1\""), true),
            Err(CacheError::QuotaExceeded { .. })
        ));
        assert_eq!(store.usage().unwrap(), usage_before);
        assert!(store.list_regions().unwrap().is_empty());
        assert!(store.list_assets().unwrap().is_empty());
    }

    #[test]
    fn metadata_batch_over_cap_rolls_back_region_and_assets() {
        const TEST_CAP_BYTES: u64 = 100_000;
        let directory = TestDirectory::new("metadata-transaction-rollback");
        let mut store = CacheStore::open_with_cap(&directory.0, TEST_CAP_BYTES).unwrap();
        let mut plan = plan_glo90_region(GeoPoint {
            lat: 30.5,
            lon: 103.5,
        })
        .unwrap();
        plan.region_id = "metadata-transaction-rollback".into();
        let headroom = (TEST_CAP_BYTES / 100).min(16_000_000);
        let baseline = store.usage().unwrap().total_bytes;
        let padding_path = directory.0.join("transaction-padding.bin");
        File::create(&padding_path)
            .unwrap()
            .set_len(TEST_CAP_BYTES - baseline - headroom)
            .unwrap();
        let usage_before = store.usage().unwrap();
        assert_eq!(usage_before.total_bytes + headroom, TEST_CAP_BYTES);

        assert!(matches!(
            store.upsert_region(&plan),
            Err(CacheError::QuotaExceeded { .. })
        ));
        assert_eq!(store.usage().unwrap(), usage_before);
        assert!(store.list_regions().unwrap().is_empty());
        assert!(store.list_assets().unwrap().is_empty());
    }
}
