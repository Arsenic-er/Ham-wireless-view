# Third-party components

This engineering inventory supplements, but does not replace, the complete release
bill of materials and notice file required before public distribution.

| Component | Version/pin | License | Use |
|---|---|---|---|
| NTIA ITS Irregular Terrain Model | v1.4 / `668e4ab0b31a7ea1e949e4824272955d63e7c731` | U.S. Government work with worldwide permission and disclaimer | Propagation core |
| `tiff` | 0.11.3 | MIT | GLO-90 GeoTIFF decoding |
| `cc` | Cargo.lock | MIT OR Apache-2.0 | C++ build helper |
| `geographiclib-rs` | 0.2.7 | MIT | WGS84 receiver endpoint generation |
| `png` | 0.18.1 | MIT OR Apache-2.0 | Diagnostic RGBA heatmap encoding |
| `rusqlite` | 0.40.1 | MIT | Persistent cache index |
| `libsqlite3-sys` / SQLite | 0.38.1 / bundled | MIT wrapper / SQLite public domain | Bundled cross-platform database engine |
| `ureq` | 3.3.0 | MIT OR Apache-2.0 | Blocking HTTPS download client |
| `sha2` | 0.11.0 | MIT OR Apache-2.0 | Streaming SHA-256 integrity checks |
| `printpdf` | 0.11.1 | MIT | Offline single-page PDF report encoding |
| `zeroize` | 1.8.2 | MIT OR Apache-2.0 | Clears desktop online-map credential buffers |
| Tianditu online WMTS | User-supplied account key / current service | Provider service terms apply; no tiles or key bundled | Transient Windows map and imagery display only |
| EOxCloudless Sentinel-2 Cloudless 2025 WMTS | Current online service | CC BY-NC-SA 4.0 for the recorded non-commercial 2025 service; commercial use requires applicable EOX authorization | Transient private-validation satellite display only; no tiles bundled or persisted |
| `tauri-plugin-dialog` | 2.7.1 | Apache-2.0 OR MIT | Native Windows save dialog |
| `fs4` | 1.1.0 | MIT OR Apache-2.0 | Cross-platform disk-space inspection |
| PMTiles JavaScript | 4.4.1 | BSD-3-Clause | Historical validation client removed from current source/dependencies; still present in the published Alpha 2 JavaScript bundle, without an offline map archive |
| fflate | 0.8.3 (transitive) | MIT | Historical PMTiles dependency removed from current source/dependencies; still present in the published Alpha 2 JavaScript bundle |
| Protomaps four-province validation archive | source build 20260731 | ODbL 1.0 Produced Work; upstream notices apply | Historical private-validation asset only; about 33 MB remains in server runtime pending managed deletion and is never shipped |

The complete NTIA terms are preserved at `third_party/ntia-itm/LICENSE.md`.
Rust transitive versions are fixed in `Cargo.lock`; their license texts must be
collected automatically and reviewed before packaging a release.

The historical validation archive is 33,044,072 bytes with SHA-256
5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0.
Its UI attribution is © OpenStreetMap contributors. The raw archive still contains
Natural Earth/OSM boundaries, and the upstream landcover attribution chain remains
to be confirmed. ADR-0022 removes PMTiles from the current product target; the
archive is not included in the formal EXE, but the server runtime copy has not yet
been deleted. PMTiles/fflate source and dependency cleanup is complete; the published Alpha 2 still contains the historical JavaScript modules until it is superseded by a new build.
