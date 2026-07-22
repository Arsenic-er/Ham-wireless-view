# ADR-0003: Use a pure Rust GLO-90 reader

- Status: accepted for Phase 1
- Date: 2026-07-16

## Context

The technical design preferred avoiding GDAL if a small pure Rust reader could
decode the selected Copernicus GLO-90 product. Phase 0 downloaded the AWS Open
Data N30/E103 COG (5,169,591 bytes), verified its SHA-256, and decoded it with
the MIT-licensed `tiff 0.11.3` crate using only its Deflate feature.

The 1200x1200 32-bit floating raster decoded in approximately 0.089 seconds on
the JAIST validation host. Required `ModelPixelScale`, `ModelTiepoint`, and
optional `GDAL_NODATA` tags were read without a native GIS runtime. A 960-point
real-terrain path was then sampled bilinearly and passed to ITM successfully.

## Decision

Use `tiff 0.11.3` for local GLO-90 tile decoding in Phase 1. Keep HTTPS
download, Range resume, size/hash validation, quota accounting, and atomic
cache promotion outside the decoder. Do not add GDAL to the MVP unless the
multi-tile benchmark exposes a requirement the pure Rust path cannot satisfy.

## Consequences

- Windows packaging avoids a large native GIS dependency.
- The cache owns complete validated tiles; the decoder does not make network
  requests.
- Phase 1 must validate GeoKey/CRS metadata against the pinned dataset manifest,
  add tile-boundary interpolation tests, and benchmark a complete 200 km region.
- NoData remains blocking on land; a missing value is never converted to zero.

