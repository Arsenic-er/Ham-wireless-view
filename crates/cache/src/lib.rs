//! Persistent data planning, quota enforcement, integrity, and atomic download.

mod download;
mod planner;
mod store;

use std::error::Error;
use std::fmt;
use std::io;

pub use download::{
    AssetTransfer, DownloadPlan, DownloadProgress, Glo90DownloadService, ProbedAsset,
    execute_download_plan,
};
pub use planner::{
    AssetDescriptor, COVERAGE_RADIUS_M, DemRegionPlan, GLO90_DATASET_ID, GLO90_DATASET_VERSION,
    GLO90_WBM_DATASET_ID, GLO90_WBM_DATASET_VERSION, GeoBounds, GeoPoint, glo90_asset,
    glo90_assets, glo90_wbm_asset, plan_glo90_region,
};
pub use store::{
    CacheAsset, CacheRegion, CacheState, CacheStore, CacheUsage, DeleteRegionResult,
    TOTAL_CACHE_CAP_BYTES,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CacheKind {
    Basemap,
    Dem,
    Water,
    DownloadTemporary,
    Calculation,
}

impl CacheKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Basemap => "basemap",
            Self::Dem => "dem",
            Self::Water => "water",
            Self::DownloadTemporary => "download-temporary",
            Self::Calculation => "calculation",
        }
    }

    fn from_str(value: &str) -> Result<Self, CacheError> {
        match value {
            "basemap" => Ok(Self::Basemap),
            "dem" => Ok(Self::Dem),
            "water" => Ok(Self::Water),
            "download-temporary" => Ok(Self::DownloadTemporary),
            "calculation" => Ok(Self::Calculation),
            other => Err(CacheError::InvalidData(format!(
                "unknown cache kind {other:?}"
            ))),
        }
    }
}

#[derive(Debug)]
pub enum CacheError {
    Io(io::Error),
    Sql(rusqlite::Error),
    Network(String),
    InvalidInput(String),
    InvalidData(String),
    QuotaExceeded {
        current_bytes: u64,
        requested_additional_bytes: u64,
        cap_bytes: u64,
    },
    DiskSpaceInsufficient {
        available_bytes: u64,
        requested_additional_bytes: u64,
    },
    Integrity {
        asset_key: String,
        message: String,
    },
    MissingAssets(Vec<String>),
    ActiveRegion(String),
    Cancelled,
}

impl fmt::Display for CacheError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "I/O error: {error}"),
            Self::Sql(error) => write!(formatter, "cache index error: {error}"),
            Self::Network(message) => write!(formatter, "network error: {message}"),
            Self::InvalidInput(message) => write!(formatter, "invalid input: {message}"),
            Self::InvalidData(message) => write!(formatter, "invalid cache data: {message}"),
            Self::QuotaExceeded {
                current_bytes,
                requested_additional_bytes,
                cap_bytes,
            } => write!(
                formatter,
                "cache quota exceeded: current={current_bytes}, additional={requested_additional_bytes}, cap={cap_bytes}"
            ),
            Self::DiskSpaceInsufficient {
                available_bytes,
                requested_additional_bytes,
            } => write!(
                formatter,
                "disk space insufficient: available={available_bytes}, additional={requested_additional_bytes}"
            ),
            Self::Integrity { asset_key, message } => {
                write!(
                    formatter,
                    "integrity check failed for {asset_key}: {message}"
                )
            }
            Self::MissingAssets(asset_keys) => {
                write!(
                    formatter,
                    "{} required cache assets are missing",
                    asset_keys.len()
                )
            }
            Self::ActiveRegion(region_id) => {
                write!(formatter, "cache region {region_id:?} is currently in use")
            }
            Self::Cancelled => write!(formatter, "operation cancelled"),
        }
    }
}

impl Error for CacheError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Sql(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for CacheError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for CacheError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Sql(error)
    }
}
