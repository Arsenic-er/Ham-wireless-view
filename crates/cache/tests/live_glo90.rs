use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use hamheatmap_cache::{
    AssetTransfer, CacheStore, GeoPoint, Glo90DownloadService, execute_download_plan,
    plan_glo90_region,
};
use hamheatmap_terrain::{DemTileId, DemTileSet, WaterTileSet};

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "hamheatmap-live-glo90-{}-{name}",
            std::process::id(),
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

#[test]
#[ignore = "requires access to the pinned AWS GLO-90 HTTPS bucket"]
fn one_real_tile_download_is_atomic_indexed_and_decodable() {
    let directory = TestDirectory::new("atomic");
    let mut plan = plan_glo90_region(GeoPoint {
        lat: 30.5,
        lon: 103.5,
    })
    .unwrap();
    plan.region_id.push_str("-live-one-tile");
    let target = DemTileId {
        south_lat_deg: 30,
        west_lon_deg: 103,
    };
    plan.tiles.retain(|tile| *tile == target);
    assert_eq!(plan.tiles, vec![target]);

    let mut store = CacheStore::open(&directory.0).unwrap();
    let service = Glo90DownloadService::new();
    let download_plan = service.probe_region(&mut store, plan.clone()).unwrap();
    assert_eq!(download_plan.assets.len(), 2);
    let paths = execute_download_plan(
        &service,
        &mut store,
        &download_plan,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(paths.len(), 2);
    let dem_paths = store.ready_paths_for_region(&plan).unwrap();
    let water_paths = store.ready_water_paths_for_region(&plan).unwrap();
    assert_eq!(dem_paths.len(), 1);
    assert_eq!(water_paths.len(), 1);
    let dem = DemTileSet::open_paths(&dem_paths).unwrap();
    let water = WaterTileSet::open_paths(&water_paths).unwrap();
    assert_eq!(dem.tile_count(), 1);
    assert_eq!(water.tile_count(), 1);
    assert!(dem.sample_bilinear(103.5, 30.5).unwrap().is_finite());
    assert!(water.statistics().land_sample_count > 0);
}

#[test]
#[ignore = "requires access to the pinned AWS GLO-90 HTTPS bucket"]
fn interrupted_real_tile_download_resumes_from_partial_file() {
    let directory = TestDirectory::new("resume");
    let mut plan = plan_glo90_region(GeoPoint {
        lat: 30.5,
        lon: 103.5,
    })
    .unwrap();
    plan.region_id.push_str("-live-resume");
    let target = DemTileId {
        south_lat_deg: 30,
        west_lon_deg: 103,
    };
    plan.tiles.retain(|tile| *tile == target);

    let mut store = CacheStore::open(&directory.0).unwrap();
    let service = Glo90DownloadService::new();
    let download_plan = service.probe_region(&mut store, plan.clone()).unwrap();
    let cancelled = AtomicBool::new(false);
    let first_result =
        execute_download_plan(&service, &mut store, &download_plan, &cancelled, |_| {
            cancelled.store(true, Ordering::Relaxed)
        });
    assert!(matches!(
        first_result,
        Err(hamheatmap_cache::CacheError::Cancelled)
    ));
    let interrupted_usage = store.usage().unwrap();
    assert!(interrupted_usage.partial_bytes > 0);
    assert!(interrupted_usage.partial_bytes < download_plan.assets[0].expected_size_bytes);
    let interrupted_partial_bytes = interrupted_usage.partial_bytes;

    let resume_plan = service.probe_region(&mut store, plan.clone()).unwrap();
    let resume_asset = resume_plan
        .assets
        .iter()
        .find(|asset| asset.resumable_bytes > 0)
        .expect("the interrupted asset should have a safe resume offset");
    assert_eq!(resume_asset.resumable_bytes, interrupted_partial_bytes);
    assert_eq!(
        resume_plan.additional_download_bytes,
        download_plan
            .additional_download_bytes
            .checked_sub(interrupted_partial_bytes)
            .unwrap()
    );

    cancelled.store(false, Ordering::Relaxed);
    execute_download_plan(&service, &mut store, &resume_plan, &cancelled, |_| {}).unwrap();
    let verified_paths = store.ready_paths_for_region(&plan).unwrap();
    assert_eq!(verified_paths.len(), 1);
    assert_eq!(store.ready_water_paths_for_region(&plan).unwrap().len(), 1);
    assert_eq!(store.usage().unwrap().partial_bytes, 0);
}

#[test]
#[ignore = "requires access to the pinned AWS GLO-90 HTTPS bucket"]
fn paired_404_for_official_ocean_geocell_generates_verified_local_tiles() {
    let directory = TestDirectory::new("uniform-ocean");
    let mut plan = plan_glo90_region(GeoPoint {
        lat: 36.0671,
        lon: 120.3826,
    })
    .unwrap();
    plan.region_id.push_str("-live-uniform-ocean");
    let target = DemTileId {
        south_lat_deg: 34,
        west_lon_deg: 121,
    };
    plan.tiles.retain(|tile| *tile == target);

    let mut store = CacheStore::open(&directory.0).unwrap();
    let service = Glo90DownloadService::new();
    let download_plan = service.probe_region(&mut store, plan.clone()).unwrap();
    assert_eq!(download_plan.assets.len(), 2);
    assert_eq!(download_plan.generated_asset_count, 2);
    assert!(download_plan.assets.iter().all(|asset| matches!(
        asset.transfer,
        AssetTransfer::GeneratedOcean { tile } if tile == target
    )));
    execute_download_plan(
        &service,
        &mut store,
        &download_plan,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();

    let dem = DemTileSet::open_paths(store.ready_paths_for_region(&plan).unwrap()).unwrap();
    let water =
        WaterTileSet::open_paths(store.ready_water_paths_for_region(&plan).unwrap()).unwrap();
    assert_eq!(dem.sample_bilinear(121.5, 34.5).unwrap(), 0.0);
    assert!(water.sample_is_water(121.5, 34.5).unwrap());
}
