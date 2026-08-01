// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

use geographiclib_rs::{DirectGeodesic, Geodesic};
use hamheatmap_terrain::DemTileId;

use crate::{CacheError, CacheKind};

pub const COVERAGE_RADIUS_M: f64 = 200_000.0;
pub const GLO90_DATASET_ID: &str = "cop-dem-glo-90";
pub const GLO90_DATASET_VERSION: &str = "2021_1-aws-cog";
pub const GLO90_WBM_DATASET_ID: &str = "cop-dem-glo-90-wbm";
pub const GLO90_WBM_DATASET_VERSION: &str = GLO90_DATASET_VERSION;
pub const GLO90_TILE_SAMPLE_MARGIN_DEG: f64 = 1.0 / 1200.0;
const GLO90_BASE_URL: &str = "https://copernicus-dem-90m.s3.amazonaws.com";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoPoint {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoBounds {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DemRegionPlan {
    pub region_id: String,
    pub center: GeoPoint,
    pub bounds: GeoBounds,
    pub tiles: Vec<DemTileId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetDescriptor {
    pub asset_key: String,
    pub kind: CacheKind,
    pub dataset_id: String,
    pub dataset_version: String,
    pub relative_path: String,
    pub url: String,
}

pub fn plan_glo90_region(center: GeoPoint) -> Result<DemRegionPlan, CacheError> {
    validate_center(center)?;
    let geodesic = Geodesic::wgs84();
    let mut bounds = GeoBounds {
        south: center.lat,
        west: center.lon,
        north: center.lat,
        east: center.lon,
    };
    for half_degree in 0..720 {
        let azimuth_deg = half_degree as f64 * 0.5;
        let (lat, lon): (f64, f64) =
            geodesic.direct(center.lat, center.lon, azimuth_deg, COVERAGE_RADIUS_M);
        bounds.south = bounds.south.min(lat);
        bounds.north = bounds.north.max(lat);
        bounds.west = bounds.west.min(lon);
        bounds.east = bounds.east.max(lon);
    }
    if bounds.east - bounds.west > 180.0 {
        return Err(CacheError::InvalidInput(
            "coverage region crosses the antimeridian, which is outside the mainland-China service area"
                .into(),
        ));
    }
    bounds.south -= GLO90_TILE_SAMPLE_MARGIN_DEG;
    bounds.west -= GLO90_TILE_SAMPLE_MARGIN_DEG;
    bounds.north += GLO90_TILE_SAMPLE_MARGIN_DEG;
    bounds.east += GLO90_TILE_SAMPLE_MARGIN_DEG;

    let south_tile = bounds.south.floor() as i32;
    let north_tile = bounds.north.floor() as i32;
    let west_tile = bounds.west.floor() as i32;
    let east_tile = bounds.east.floor() as i32;
    let mut tiles =
        Vec::with_capacity(((north_tile - south_tile + 1) * (east_tile - west_tile + 1)) as usize);
    for latitude in south_tile..=north_tile {
        for longitude in west_tile..=east_tile {
            tiles.push(DemTileId {
                south_lat_deg: latitude,
                west_lon_deg: longitude,
            });
        }
    }

    Ok(DemRegionPlan {
        region_id: region_id(center),
        center,
        bounds,
        tiles,
    })
}

pub fn glo90_asset(tile: DemTileId) -> Result<AssetDescriptor, CacheError> {
    let (stem, product) = glo90_product(tile)?;
    Ok(AssetDescriptor {
        asset_key: format!("dem:{GLO90_DATASET_ID}:{GLO90_DATASET_VERSION}:{stem}"),
        kind: CacheKind::Dem,
        dataset_id: GLO90_DATASET_ID.into(),
        dataset_version: GLO90_DATASET_VERSION.into(),
        relative_path: format!("dem/{GLO90_DATASET_VERSION}/{stem}.tif"),
        url: format!("{GLO90_BASE_URL}/{product}/{product}.tif"),
    })
}

pub fn glo90_wbm_asset(tile: DemTileId) -> Result<AssetDescriptor, CacheError> {
    let (stem, product) = glo90_product(tile)?;
    let water_filename = product.replace("_DEM", "_WBM");
    Ok(AssetDescriptor {
        asset_key: format!("water:{GLO90_WBM_DATASET_ID}:{GLO90_WBM_DATASET_VERSION}:{stem}"),
        kind: CacheKind::Water,
        dataset_id: GLO90_WBM_DATASET_ID.into(),
        dataset_version: GLO90_WBM_DATASET_VERSION.into(),
        relative_path: format!("water/{GLO90_WBM_DATASET_VERSION}/{stem}.tif"),
        url: format!("{GLO90_BASE_URL}/{product}/AUXFILES/{water_filename}.tif"),
    })
}

pub fn glo90_assets(tile: DemTileId) -> Result<[AssetDescriptor; 2], CacheError> {
    Ok([glo90_asset(tile)?, glo90_wbm_asset(tile)?])
}

fn glo90_product(tile: DemTileId) -> Result<(String, String), CacheError> {
    if !(-90..=89).contains(&tile.south_lat_deg) || !(-180..=179).contains(&tile.west_lon_deg) {
        return Err(CacheError::InvalidInput(format!(
            "invalid one-degree tile coordinates: lat={}, lon={}",
            tile.south_lat_deg, tile.west_lon_deg
        )));
    }
    let stem = tile.filename().trim_end_matches(".tif").to_owned();
    let latitude = signed_component(tile.south_lat_deg, 'N', 'S', 2);
    let longitude = signed_component(tile.west_lon_deg, 'E', 'W', 3);
    let product = format!("Copernicus_DSM_COG_30_{latitude}_00_{longitude}_00_DEM");
    Ok((stem, product))
}

fn signed_component(value: i32, positive: char, negative: char, width: usize) -> String {
    let prefix = if value < 0 { negative } else { positive };
    format!("{prefix}{:0width$}", value.abs())
}

fn validate_center(center: GeoPoint) -> Result<(), CacheError> {
    if !center.lat.is_finite()
        || !center.lon.is_finite()
        || !(-90.0..=90.0).contains(&center.lat)
        || !(-180.0..=180.0).contains(&center.lon)
    {
        return Err(CacheError::InvalidInput(format!(
            "invalid WGS84 center ({}, {})",
            center.lat, center.lon
        )));
    }
    if center.lat.abs() > 87.0 {
        return Err(CacheError::InvalidInput(
            "coverage center is too close to a pole".into(),
        ));
    }
    Ok(())
}

fn region_id(center: GeoPoint) -> String {
    let latitude_microdegrees = (center.lat * 1_000_000.0).round() as i64;
    let longitude_microdegrees = (center.lon * 1_000_000.0).round() as i64;
    format!("glo90-2021_1-r200-lat{latitude_microdegrees:+09}-lon{longitude_microdegrees:+010}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chengdu_plan_matches_validated_five_by_five_region() {
        let plan = plan_glo90_region(GeoPoint {
            lat: 30.5,
            lon: 103.5,
        })
        .unwrap();
        assert_eq!(plan.tiles.len(), 25);
        assert_eq!(plan.tiles.first().unwrap().filename(), "N28E101.tif");
        assert_eq!(plan.tiles.last().unwrap().filename(), "N32E105.tif");
        assert!(plan.bounds.south < 28.7);
        assert!(plan.bounds.north > 32.3);
    }

    #[test]
    fn asset_paths_and_urls_handle_both_hemispheres() {
        let descriptor = glo90_asset(DemTileId {
            south_lat_deg: -7,
            west_lon_deg: -12,
        })
        .unwrap();
        assert!(descriptor.relative_path.ends_with("S07W012.tif"));
        assert!(descriptor.url.contains("_S07_00_W012_00_DEM/"));
        assert!(descriptor.url.starts_with(GLO90_BASE_URL));

        let water = glo90_wbm_asset(DemTileId {
            south_lat_deg: -7,
            west_lon_deg: -12,
        })
        .unwrap();
        assert_eq!(water.kind, CacheKind::Water);
        assert!(water.relative_path.ends_with("S07W012.tif"));
        assert!(water.url.contains("/AUXFILES/"));
        assert!(water.url.ends_with("_S07_00_W012_00_WBM.tif"));
    }

    #[test]
    fn region_id_is_stable_at_microdegree_precision() {
        let first = plan_glo90_region(GeoPoint {
            lat: 30.5000001,
            lon: 103.4999999,
        })
        .unwrap();
        let second = plan_glo90_region(GeoPoint {
            lat: 30.5000002,
            lon: 103.4999998,
        })
        .unwrap();
        assert_eq!(first.region_id, second.region_id);
    }
}
