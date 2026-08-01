// Ham Wireless View
// Project creator and lead developer: Arsenic-er
// SPDX-FileCopyrightText: 2026 Arsenic-er
// SPDX-License-Identifier: Apache-2.0

//! Computation-only readers for aligned Copernicus GLO-90 DEM and water-mask tiles.

use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};

use tiff::decoder::{Decoder, DecodingResult};
use tiff::encoder::{Compression, DeflateLevel, TiffEncoder, colortype};
use tiff::tags::Tag;

pub const GLO90_SAMPLES_PER_DEGREE: u32 = 1200;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeoReference {
    pub sample_origin_lon: f64,
    pub sample_origin_lat: f64,
    pub pixel_width_deg: f64,
    pub pixel_height_deg: f64,
}

impl GeoReference {
    fn from_tags(pixel_scale: &[f64], tiepoint: &[f64]) -> Result<Self, DemError> {
        if pixel_scale.len() < 2 || tiepoint.len() < 6 {
            return Err(DemError::Format(
                "GeoTIFF needs ModelPixelScale and ModelTiepoint tags".into(),
            ));
        }
        let pixel_width_deg = pixel_scale[0];
        let pixel_height_deg = pixel_scale[1];
        if !pixel_width_deg.is_finite()
            || !pixel_height_deg.is_finite()
            || pixel_width_deg <= 0.0
            || pixel_height_deg <= 0.0
        {
            return Err(DemError::Format("invalid GeoTIFF pixel scale".into()));
        }
        let sample_origin_lon = tiepoint[3] - tiepoint[0] * pixel_width_deg;
        let sample_origin_lat = tiepoint[4] + tiepoint[1] * pixel_height_deg;
        if !sample_origin_lon.is_finite() || !sample_origin_lat.is_finite() {
            return Err(DemError::Format("invalid GeoTIFF tiepoint".into()));
        }
        Ok(Self {
            sample_origin_lon,
            sample_origin_lat,
            pixel_width_deg,
            pixel_height_deg,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElevationStatistics {
    pub minimum_m: f32,
    pub maximum_m: f32,
    pub mean_m: f64,
    pub valid_sample_count: usize,
    pub nodata_sample_count: usize,
}

#[derive(Clone, Debug)]
pub struct DemRaster {
    width: u32,
    height: u32,
    elevations_m: Vec<f32>,
    georef: GeoReference,
    nodata: Option<f32>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DemTileId {
    pub south_lat_deg: i32,
    pub west_lon_deg: i32,
}

impl DemTileId {
    pub fn from_path(path: &Path) -> Result<Self, DemError> {
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or_else(|| DemError::Format(format!("invalid DEM filename: {}", path.display())))?;
        Self::from_stem(stem)
    }

    pub fn from_stem(stem: &str) -> Result<Self, DemError> {
        let bytes = stem.as_bytes();
        if bytes.len() != 7 || !matches!(bytes[0], b'N' | b'S') || !matches!(bytes[3], b'E' | b'W')
        {
            return Err(DemError::Format(format!(
                "DEM tile name must look like N30E103, got {stem:?}"
            )));
        }
        let latitude = stem[1..3]
            .parse::<i32>()
            .map_err(|_| DemError::Format(format!("invalid DEM latitude in {stem:?}")))?;
        let longitude = stem[4..7]
            .parse::<i32>()
            .map_err(|_| DemError::Format(format!("invalid DEM longitude in {stem:?}")))?;
        Ok(Self {
            south_lat_deg: if bytes[0] == b'S' {
                -latitude
            } else {
                latitude
            },
            west_lon_deg: if bytes[3] == b'W' {
                -longitude
            } else {
                longitude
            },
        })
    }

    pub fn filename(self) -> String {
        let latitude_prefix = if self.south_lat_deg < 0 { 'S' } else { 'N' };
        let longitude_prefix = if self.west_lon_deg < 0 { 'W' } else { 'E' };
        format!(
            "{latitude_prefix}{:02}{longitude_prefix}{:03}.tif",
            self.south_lat_deg.abs(),
            self.west_lon_deg.abs()
        )
    }
}

#[derive(Debug)]
pub struct DemTileSet {
    tiles: HashMap<DemTileId, DemRaster>,
    samples_per_degree: i64,
}

impl DemTileSet {
    pub fn open_directory(path: impl AsRef<Path>) -> Result<Self, DemError> {
        let path = path.as_ref();
        let mut tile_paths: Vec<PathBuf> = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<_, _>>()?;
        tile_paths.retain(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("tif"))
        });
        tile_paths.sort();
        if tile_paths.is_empty() {
            return Err(DemError::Format(format!(
                "no .tif DEM tiles found in {}",
                path.display()
            )));
        }

        Self::open_paths(tile_paths)
    }

    /// Opens only the explicitly selected tiles. This prevents a growing
    /// offline cache from forcing every calculation to decode unrelated DEMs.
    pub fn open_paths<I, P>(paths: I) -> Result<Self, DemError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut tile_paths: Vec<PathBuf> = paths
            .into_iter()
            .map(|path| path.as_ref().to_path_buf())
            .collect();
        if tile_paths.is_empty() {
            return Err(DemError::Format("no DEM tile paths were selected".into()));
        }
        tile_paths.sort();
        tile_paths.dedup();

        let mut tiles = HashMap::with_capacity(tile_paths.len());
        let mut samples_per_degree = None;
        for tile_path in tile_paths {
            let tile_id = DemTileId::from_path(&tile_path)?;
            let raster = DemRaster::open(&tile_path)?;
            let tile_samples_per_degree = i64::from(raster.width());
            if raster.height() != raster.width() || tile_samples_per_degree <= 0 {
                return Err(DemError::Format(format!(
                    "{} is not a square one-degree DEM tile",
                    tile_path.display()
                )));
            }
            let expected_step = 1.0 / tile_samples_per_degree as f64;
            let expected_origin_lon = f64::from(tile_id.west_lon_deg);
            let expected_origin_lat = f64::from(tile_id.south_lat_deg + 1);
            let georef = raster.georeference();
            let tolerance = 1e-10;
            if (georef.pixel_width_deg - expected_step).abs() > tolerance
                || (georef.pixel_height_deg - expected_step).abs() > tolerance
                || (georef.sample_origin_lon - expected_origin_lon).abs() > tolerance
                || (georef.sample_origin_lat - expected_origin_lat).abs() > tolerance
            {
                return Err(DemError::Format(format!(
                    "{} georeference does not match its one-degree tile name",
                    tile_path.display()
                )));
            }
            if samples_per_degree.is_some_and(|value| value != tile_samples_per_degree) {
                return Err(DemError::Format(
                    "DEM tiles use inconsistent resolutions".into(),
                ));
            }
            samples_per_degree = Some(tile_samples_per_degree);
            if tiles.insert(tile_id, raster).is_some() {
                return Err(DemError::Format(format!(
                    "duplicate DEM tile {}",
                    tile_id.filename()
                )));
            }
        }

        Ok(Self {
            tiles,
            samples_per_degree: samples_per_degree.unwrap(),
        })
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn samples_per_degree(&self) -> i64 {
        self.samples_per_degree
    }

    pub fn contains_tile(&self, tile_id: DemTileId) -> bool {
        self.tiles.contains_key(&tile_id)
    }

    /// Bilinearly samples the global GLO grid, including across one-degree tile
    /// boundaries. A missing or NoData corner blocks the sample.
    pub fn sample_bilinear(&self, lon: f64, lat: f64) -> Result<f32, DemError> {
        if !lon.is_finite()
            || !lat.is_finite()
            || !(-180.0..=180.0).contains(&lon)
            || !(-90.0..=90.0).contains(&lat)
        {
            return Err(DemError::OutOfBounds { lon, lat });
        }
        let scale = self.samples_per_degree as f64;
        let global_x = lon * scale;
        let global_y = lat * scale;
        let x0 = global_x.floor() as i64;
        let y0 = global_y.floor() as i64;
        let tx = (global_x - x0 as f64) as f32;
        let ty = (global_y - y0 as f64) as f32;
        let values = [
            self.grid_value(x0, y0, lon, lat)?,
            self.grid_value(x0 + 1, y0, lon, lat)?,
            self.grid_value(x0, y0 + 1, lon, lat)?,
            self.grid_value(x0 + 1, y0 + 1, lon, lat)?,
        ];
        let south = values[0] * (1.0 - tx) + values[1] * tx;
        let north = values[2] * (1.0 - tx) + values[3] * tx;
        Ok(south * (1.0 - ty) + north * ty)
    }

    fn grid_value(
        &self,
        longitude_index: i64,
        latitude_index: i64,
        requested_lon: f64,
        requested_lat: f64,
    ) -> Result<f32, DemError> {
        let (tile_id, column, row) =
            grid_address(longitude_index, latitude_index, self.samples_per_degree);
        let tile = self.tiles.get(&tile_id).ok_or(DemError::MissingTile {
            south_lat_deg: tile_id.south_lat_deg,
            west_lon_deg: tile_id.west_lon_deg,
        })?;
        let value = tile.value(column, row);
        if tile.is_nodata(value) {
            return Err(DemError::NoData {
                lon: requested_lon,
                lat: requested_lat,
            });
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaterMaskStatistics {
    pub land_sample_count: usize,
    pub water_sample_count: usize,
}

#[derive(Clone, Debug)]
struct WaterRaster {
    width: u32,
    height: u32,
    water: Vec<u8>,
    georef: GeoReference,
}

#[derive(Debug)]
pub struct WaterTileSet {
    tiles: HashMap<DemTileId, WaterRaster>,
    samples_per_degree: i64,
}

impl WaterTileSet {
    pub fn open_directory(path: impl AsRef<Path>) -> Result<Self, WaterMaskError> {
        let path = path.as_ref();
        let mut tile_paths: Vec<PathBuf> = fs::read_dir(path)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<_, _>>()?;
        tile_paths.retain(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("tif"))
        });
        tile_paths.sort();
        if tile_paths.is_empty() {
            return Err(WaterMaskError::Format(format!(
                "no .tif water-mask tiles found in {}",
                path.display()
            )));
        }
        Self::open_paths(tile_paths)
    }

    pub fn open_paths<I, P>(paths: I) -> Result<Self, WaterMaskError>
    where
        I: IntoIterator<Item = P>,
        P: AsRef<Path>,
    {
        let mut tile_paths: Vec<PathBuf> = paths
            .into_iter()
            .map(|path| path.as_ref().to_path_buf())
            .collect();
        if tile_paths.is_empty() {
            return Err(WaterMaskError::Format(
                "no water-mask tile paths were selected".into(),
            ));
        }
        tile_paths.sort();
        tile_paths.dedup();

        let mut tiles = HashMap::with_capacity(tile_paths.len());
        let mut samples_per_degree = None;
        for tile_path in tile_paths {
            let tile_id = DemTileId::from_path(&tile_path)
                .map_err(|error| WaterMaskError::Format(error.to_string()))?;
            let raster = WaterRaster::open(&tile_path)?;
            let tile_samples_per_degree = i64::from(raster.width);
            if raster.height != raster.width || tile_samples_per_degree <= 0 {
                return Err(WaterMaskError::Format(format!(
                    "{} is not a square one-degree water-mask tile",
                    tile_path.display()
                )));
            }
            let expected_step = 1.0 / tile_samples_per_degree as f64;
            let expected_origin_lon = f64::from(tile_id.west_lon_deg);
            let expected_origin_lat = f64::from(tile_id.south_lat_deg + 1);
            let tolerance = 1e-10;
            if (raster.georef.pixel_width_deg - expected_step).abs() > tolerance
                || (raster.georef.pixel_height_deg - expected_step).abs() > tolerance
                || (raster.georef.sample_origin_lon - expected_origin_lon).abs() > tolerance
                || (raster.georef.sample_origin_lat - expected_origin_lat).abs() > tolerance
            {
                return Err(WaterMaskError::Format(format!(
                    "{} georeference does not match its one-degree tile name",
                    tile_path.display()
                )));
            }
            if samples_per_degree.is_some_and(|value| value != tile_samples_per_degree) {
                return Err(WaterMaskError::Format(
                    "water-mask tiles use inconsistent resolutions".into(),
                ));
            }
            samples_per_degree = Some(tile_samples_per_degree);
            if tiles.insert(tile_id, raster).is_some() {
                return Err(WaterMaskError::Format(format!(
                    "duplicate water-mask tile {}",
                    tile_id.filename()
                )));
            }
        }

        Ok(Self {
            tiles,
            samples_per_degree: samples_per_degree.unwrap(),
        })
    }

    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    pub fn samples_per_degree(&self) -> i64 {
        self.samples_per_degree
    }

    pub fn contains_tile(&self, tile_id: DemTileId) -> bool {
        self.tiles.contains_key(&tile_id)
    }

    pub fn statistics(&self) -> WaterMaskStatistics {
        let water_sample_count = self
            .tiles
            .values()
            .map(|tile| tile.water.iter().filter(|value| **value != 0).count())
            .sum();
        let sample_count: usize = self.tiles.values().map(|tile| tile.water.len()).sum();
        WaterMaskStatistics {
            land_sample_count: sample_count - water_sample_count,
            water_sample_count,
        }
    }

    /// Samples the categorical WBM pixel containing the requested WGS-84 point.
    /// Source values 1 (ocean), 2 (lake), and 3 (river) are already collapsed
    /// to the same internal water class while 0 remains land.
    pub fn sample_is_water(&self, lon: f64, lat: f64) -> Result<bool, WaterMaskError> {
        if !lon.is_finite()
            || !lat.is_finite()
            || !(-180.0..=180.0).contains(&lon)
            || !(-90.0..=90.0).contains(&lat)
        {
            return Err(WaterMaskError::OutOfBounds { lon, lat });
        }
        let west_lon_deg = if lon == 180.0 {
            179
        } else {
            lon.floor() as i32
        };
        let south_lat_deg = if lat == 90.0 { 89 } else { lat.floor() as i32 };
        let tile_id = DemTileId {
            south_lat_deg,
            west_lon_deg,
        };
        let tile = self
            .tiles
            .get(&tile_id)
            .ok_or(WaterMaskError::MissingTile {
                south_lat_deg,
                west_lon_deg,
            })?;
        let scale = self.samples_per_degree as f64;
        let column = ((lon - f64::from(west_lon_deg)) * scale)
            .floor()
            .clamp(0.0, scale - 1.0) as u32;
        let row = ((f64::from(south_lat_deg + 1) - lat) * scale)
            .floor()
            .clamp(0.0, scale - 1.0) as u32;
        Ok(tile.value(column, row) != 0)
    }
}

impl WaterRaster {
    fn open(path: &Path) -> Result<Self, WaterMaskError> {
        let file = File::open(path)?;
        let mut decoder = Decoder::new(BufReader::new(file))?;
        let (width, height) = decoder.dimensions()?;
        let pixel_scale = decoder.get_tag_f64_vec(Tag::ModelPixelScaleTag)?;
        let tiepoint = decoder.get_tag_f64_vec(Tag::ModelTiepointTag)?;
        let georef = GeoReference::from_tags(&pixel_scale, &tiepoint)
            .map_err(|error| WaterMaskError::Format(error.to_string()))?;
        let source_values = match decoder.read_image()? {
            DecodingResult::U8(values) => values,
            other => {
                return Err(WaterMaskError::Format(format!(
                    "expected 8-bit unsigned water-mask samples, got {other:?}"
                )));
            }
        };
        let expected_len = width as usize * height as usize;
        if source_values.len() != expected_len {
            return Err(WaterMaskError::Format(format!(
                "decoded {} samples for {width}x{height} water mask",
                source_values.len()
            )));
        }
        let water = source_values
            .into_iter()
            .map(collapse_water_category)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            width,
            height,
            water,
            georef,
        })
    }

    fn value(&self, column: u32, row: u32) -> u8 {
        self.water[row as usize * self.width as usize + column as usize]
    }
}

fn collapse_water_category(value: u8) -> Result<u8, WaterMaskError> {
    match value {
        0 => Ok(0),
        1..=3 => Ok(1),
        other => Err(WaterMaskError::Format(format!(
            "unsupported Copernicus WBM pixel value {other}; expected 0..=3"
        ))),
    }
}

/// Encodes the deterministic zero-elevation replacement used only when the
/// pinned Copernicus collection confirms that a geocell is absent because it
/// contains no global landmass. The matching WBM tile must also be generated.
pub fn encode_uniform_ocean_dem_tile(tile_id: DemTileId) -> Result<Vec<u8>, DemError> {
    let samples = vec![0.0_f32; (GLO90_SAMPLES_PER_DEGREE as usize).pow(2)];
    encode_geotiff::<colortype::Gray32Float>(tile_id, &samples).map_err(DemError::Tiff)
}

/// Encodes the matching all-ocean WBM tile. Source category 1 is immediately
/// collapsed to the product's single internal water class when it is read.
pub fn encode_uniform_ocean_water_tile(tile_id: DemTileId) -> Result<Vec<u8>, WaterMaskError> {
    let samples = vec![1_u8; (GLO90_SAMPLES_PER_DEGREE as usize).pow(2)];
    encode_geotiff::<colortype::Gray8>(tile_id, &samples).map_err(WaterMaskError::Tiff)
}

fn encode_geotiff<C>(tile_id: DemTileId, samples: &[C::Inner]) -> Result<Vec<u8>, tiff::TiffError>
where
    C: tiff::encoder::colortype::ColorType,
    [C::Inner]: tiff::encoder::TiffValue,
{
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut encoder = TiffEncoder::new(&mut cursor)?
            .with_compression(Compression::Deflate(DeflateLevel::Best));
        let mut image =
            encoder.new_image::<C>(GLO90_SAMPLES_PER_DEGREE, GLO90_SAMPLES_PER_DEGREE)?;
        let pixel_step = 1.0 / f64::from(GLO90_SAMPLES_PER_DEGREE);
        image
            .encoder()
            .write_tag(Tag::ModelPixelScaleTag, &[pixel_step, pixel_step, 0.0][..])?;
        image.encoder().write_tag(
            Tag::ModelTiepointTag,
            &[
                0.0,
                0.0,
                0.0,
                f64::from(tile_id.west_lon_deg),
                f64::from(tile_id.south_lat_deg + 1),
                0.0,
            ][..],
        )?;
        image.encoder().write_tag(
            Tag::GeoKeyDirectoryTag,
            &[
                1_u16, 1, 0, 4, 1024, 0, 1, 2, 1025, 0, 1, 2, 2048, 0, 1, 4326, 2054, 0, 1, 9102,
            ][..],
        )?;
        image.write_data(samples)?;
    }
    Ok(cursor.into_inner())
}

fn grid_address(
    longitude_index: i64,
    latitude_index: i64,
    samples_per_degree: i64,
) -> (DemTileId, u32, u32) {
    let west_lon_deg = longitude_index.div_euclid(samples_per_degree) as i32;
    let column = longitude_index.rem_euclid(samples_per_degree) as u32;
    let latitude_degree = latitude_index.div_euclid(samples_per_degree) as i32;
    let latitude_remainder = latitude_index.rem_euclid(samples_per_degree);
    let (south_lat_deg, row) = if latitude_remainder == 0 {
        (latitude_degree - 1, 0)
    } else {
        (
            latitude_degree,
            (samples_per_degree - latitude_remainder) as u32,
        )
    };
    (
        DemTileId {
            south_lat_deg,
            west_lon_deg,
        },
        column,
        row,
    )
}

impl DemRaster {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DemError> {
        let file = File::open(path)?;
        let mut decoder = Decoder::new(BufReader::new(file))?;
        let (width, height) = decoder.dimensions()?;
        let pixel_scale = decoder.get_tag_f64_vec(Tag::ModelPixelScaleTag)?;
        let tiepoint = decoder.get_tag_f64_vec(Tag::ModelTiepointTag)?;
        let nodata = decoder
            .get_tag_ascii_string(Tag::GdalNodata)
            .ok()
            .and_then(|value| value.trim_matches(char::from(0)).trim().parse::<f32>().ok());
        let georef = GeoReference::from_tags(&pixel_scale, &tiepoint)?;
        let elevations_m = match decoder.read_image()? {
            DecodingResult::F32(values) => values,
            other => {
                return Err(DemError::Format(format!(
                    "expected 32-bit floating DEM samples, got {other:?}"
                )));
            }
        };
        let expected_len = width as usize * height as usize;
        if elevations_m.len() != expected_len {
            return Err(DemError::Format(format!(
                "decoded {} samples for {width}x{height} raster",
                elevations_m.len()
            )));
        }
        Ok(Self {
            width,
            height,
            elevations_m,
            georef,
            nodata,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn georeference(&self) -> GeoReference {
        self.georef
    }

    pub fn nodata(&self) -> Option<f32> {
        self.nodata
    }

    pub fn sample_bounds(&self) -> (f64, f64, f64, f64) {
        let west = self.georef.sample_origin_lon;
        let north = self.georef.sample_origin_lat;
        let east = west + (self.width - 1) as f64 * self.georef.pixel_width_deg;
        let south = north - (self.height - 1) as f64 * self.georef.pixel_height_deg;
        (west, south, east, north)
    }

    pub fn statistics(&self) -> Result<ElevationStatistics, DemError> {
        let mut minimum_m = f32::INFINITY;
        let mut maximum_m = f32::NEG_INFINITY;
        let mut sum_m = 0.0_f64;
        let mut valid_sample_count = 0_usize;
        let mut nodata_sample_count = 0_usize;

        for &value in &self.elevations_m {
            if self.is_nodata(value) {
                nodata_sample_count += 1;
                continue;
            }
            minimum_m = minimum_m.min(value);
            maximum_m = maximum_m.max(value);
            sum_m += f64::from(value);
            valid_sample_count += 1;
        }
        if valid_sample_count == 0 {
            return Err(DemError::Format("DEM contains no valid samples".into()));
        }
        Ok(ElevationStatistics {
            minimum_m,
            maximum_m,
            mean_m: sum_m / valid_sample_count as f64,
            valid_sample_count,
            nodata_sample_count,
        })
    }

    pub fn sample_bilinear(&self, lon: f64, lat: f64) -> Result<f32, DemError> {
        if !lon.is_finite() || !lat.is_finite() {
            return Err(DemError::OutOfBounds { lon, lat });
        }
        let column = (lon - self.georef.sample_origin_lon) / self.georef.pixel_width_deg;
        let row = (self.georef.sample_origin_lat - lat) / self.georef.pixel_height_deg;
        if column < 0.0
            || row < 0.0
            || column > (self.width - 1) as f64
            || row > (self.height - 1) as f64
        {
            return Err(DemError::OutOfBounds { lon, lat });
        }

        let x0 = column.floor() as u32;
        let y0 = row.floor() as u32;
        let x1 = (x0 + 1).min(self.width - 1);
        let y1 = (y0 + 1).min(self.height - 1);
        let tx = (column - f64::from(x0)) as f32;
        let ty = (row - f64::from(y0)) as f32;
        let values = [
            self.value(x0, y0),
            self.value(x1, y0),
            self.value(x0, y1),
            self.value(x1, y1),
        ];
        if values.iter().any(|&value| self.is_nodata(value)) {
            return Err(DemError::NoData { lon, lat });
        }
        let north = values[0] * (1.0 - tx) + values[1] * tx;
        let south = values[2] * (1.0 - tx) + values[3] * tx;
        Ok(north * (1.0 - ty) + south * ty)
    }

    fn value(&self, column: u32, row: u32) -> f32 {
        self.elevations_m[row as usize * self.width as usize + column as usize]
    }

    fn is_nodata(&self, value: f32) -> bool {
        !value.is_finite() || self.nodata.is_some_and(|nodata| value == nodata)
    }
}

#[derive(Debug)]
pub enum DemError {
    Io(std::io::Error),
    Tiff(tiff::TiffError),
    Format(String),
    OutOfBounds {
        lon: f64,
        lat: f64,
    },
    NoData {
        lon: f64,
        lat: f64,
    },
    MissingTile {
        south_lat_deg: i32,
        west_lon_deg: i32,
    },
}

impl fmt::Display for DemError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "DEM I/O error: {error}"),
            Self::Tiff(error) => write!(formatter, "GeoTIFF decode error: {error}"),
            Self::Format(message) => write!(formatter, "invalid DEM: {message}"),
            Self::OutOfBounds { lon, lat } => {
                write!(formatter, "coordinate ({lon}, {lat}) is outside DEM")
            }
            Self::NoData { lon, lat } => {
                write!(formatter, "DEM has NoData around ({lon}, {lat})")
            }
            Self::MissingTile {
                south_lat_deg,
                west_lon_deg,
            } => write!(
                formatter,
                "missing DEM tile with south latitude {south_lat_deg} and west longitude {west_lon_deg}"
            ),
        }
    }
}

impl Error for DemError {}

impl From<std::io::Error> for DemError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<tiff::TiffError> for DemError {
    fn from(error: tiff::TiffError) -> Self {
        Self::Tiff(error)
    }
}

#[derive(Debug)]
pub enum WaterMaskError {
    Io(std::io::Error),
    Tiff(tiff::TiffError),
    Format(String),
    OutOfBounds {
        lon: f64,
        lat: f64,
    },
    MissingTile {
        south_lat_deg: i32,
        west_lon_deg: i32,
    },
}

impl fmt::Display for WaterMaskError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "water-mask I/O error: {error}"),
            Self::Tiff(error) => write!(formatter, "water-mask GeoTIFF decode error: {error}"),
            Self::Format(message) => write!(formatter, "invalid water mask: {message}"),
            Self::OutOfBounds { lon, lat } => {
                write!(formatter, "coordinate ({lon}, {lat}) is outside water mask")
            }
            Self::MissingTile {
                south_lat_deg,
                west_lon_deg,
            } => write!(
                formatter,
                "missing water-mask tile with south latitude {south_lat_deg} and west longitude {west_lon_deg}"
            ),
        }
    }
}

impl Error for WaterMaskError {}

impl From<std::io::Error> for WaterMaskError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<tiff::TiffError> for WaterMaskError {
    fn from(error: tiff::TiffError) -> Self {
        Self::Tiff(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn georeference_applies_tiepoint_offsets() {
        let georef =
            GeoReference::from_tags(&[0.25, 0.5, 0.0], &[2.0, 4.0, 0.0, 103.5, 31.0, 0.0]).unwrap();
        assert!((georef.sample_origin_lon - 103.0).abs() < 1e-12);
        assert!((georef.sample_origin_lat - 33.0).abs() < 1e-12);
    }

    #[test]
    fn tile_names_round_trip_in_both_hemispheres() {
        for stem in ["N30E103", "S05W007"] {
            let tile = DemTileId::from_stem(stem).unwrap();
            assert_eq!(tile.filename(), format!("{stem}.tif"));
        }
    }

    #[test]
    fn global_grid_addresses_cross_longitude_and_latitude_edges() {
        let scale = 1200;
        let (tile, column, row) = grid_address(104 * scale, 30 * scale + 600, scale);
        assert_eq!(
            tile,
            DemTileId {
                south_lat_deg: 30,
                west_lon_deg: 104,
            }
        );
        assert_eq!((column, row), (0, 600));

        let (tile, column, row) = grid_address(103 * scale + 1199, 31 * scale, scale);
        assert_eq!(
            tile,
            DemTileId {
                south_lat_deg: 30,
                west_lon_deg: 103,
            }
        );
        assert_eq!((column, row), (1199, 0));
    }

    #[test]
    fn water_source_categories_collapse_to_one_internal_class() {
        assert_eq!(collapse_water_category(0).unwrap(), 0);
        for source_value in 1..=3 {
            assert_eq!(collapse_water_category(source_value).unwrap(), 1);
        }
        assert!(collapse_water_category(255).is_err());
    }

    #[test]
    fn generated_uniform_ocean_tiles_round_trip_through_production_readers() {
        let tile_id = DemTileId {
            south_lat_deg: 34,
            west_lon_deg: 121,
        };
        let directory =
            std::env::temp_dir().join(format!("hamheatmap-ocean-tile-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let dem_path = directory.join(tile_id.filename());
        let water_directory = directory.join("water");
        fs::create_dir_all(&water_directory).unwrap();
        let water_path = water_directory.join(tile_id.filename());
        fs::write(&dem_path, encode_uniform_ocean_dem_tile(tile_id).unwrap()).unwrap();
        fs::write(
            &water_path,
            encode_uniform_ocean_water_tile(tile_id).unwrap(),
        )
        .unwrap();

        let dem = DemTileSet::open_paths([&dem_path]).unwrap();
        let water = WaterTileSet::open_paths([&water_path]).unwrap();
        assert_eq!(dem.sample_bilinear(121.5, 34.5).unwrap(), 0.0);
        assert!(water.sample_is_water(121.5, 34.5).unwrap());
        assert_eq!(water.statistics().land_sample_count, 0);
        assert_eq!(
            water.statistics().water_sample_count,
            (GLO90_SAMPLES_PER_DEGREE as usize).pow(2)
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
