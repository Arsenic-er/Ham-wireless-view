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
| `tauri-plugin-dialog` | 2.7.1 | Apache-2.0 OR MIT | Native Windows save dialog |
| `fs4` | 1.1.0 | MIT OR Apache-2.0 | Cross-platform disk-space inspection |

The complete NTIA terms are preserved at `third_party/ntia-itm/LICENSE.md`.
Rust transitive versions are fixed in `Cargo.lock`; their license texts must be
collected automatically and reviewed before packaging a release.
