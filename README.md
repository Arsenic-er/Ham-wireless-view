# HamHeatmap

**English** | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | [日本語](README.ja.md)

<!-- locale: en -->
<!-- canonical: README.md -->

HamHeatmap is an open-source Windows desktop application for amateur-radio operators in mainland China. Select one transmitter location on the map and the application uses frequency, power, antenna gain, height, polarization, terrain, and land/water parameters to predict received power on a fixed 200 km radius, 1 km grid.

<!-- section:current-status -->
<!-- synchronized-tests: frontend-files=17 frontend=152 rust=133 ignored=5 -->
## Current status

### Online validation build

- The private validation service runs on the server, listens only on `127.0.0.1:1421`, and is accessed through an SSH tunnel.
- The current source uses the same-origin Tianditu vector map when validation has a valid token and automatically selects CARTO Voyager base + labels when no token exists. Satellite mode uses EOxCloudless with the active online labels, including CARTO labels in no-token mode. Base, label, and satellite failures are isolated so an unaffected layer can remain usable; only loss of both usable visual bases falls back to the WGS84 coordinate grid. The managed loopback service has been rebuilt from clean commit d7bd31a: health schema 1, CARTO base/labels, EOx satellite, old-Tianditu rejection, query rejection, and no-store checks pass; browser visuals remain pending.
- A global -140..-60 dBm display-threshold slider with 1 dB steps dynamically hides weaker pixels without recalculating propagation or changing statistics and the current export. Current gates pass 17 frontend test files / 152 tests and Rust workspace 133 passed / 5 ignored, plus three real-cache HTTP smoke suites. The 8-layer server CPU microbenchmark has a P95 of 5.982 ms. Managed-browser dragging is still unverified because of a Codex Windows ACL failure, and Windows WebView2 hardware testing is also pending.
- The former four-province PMTiles path is no longer a product target and remains only as historical engineering evidence. EOxCloudless is an online satellite layer for validation and is not included in the public Windows release assets.

- The current source implements a separate online point-to-point TX/RX link-analysis mode for 1–200 km paths. Completion automatically opens a non-modal floating terrain/Fresnel profile: clicking outside dims it to 42% while the map stays interactive, clicking it restores full opacity, close/Escape dismisses it, and the completed result can reopen it. The managed process now contains this source, but live link-analysis/profile-dialog and browser acceptance remain pending; no public service is deployed. Initial site guidance for coverage TX and link TX/RX is now a compact status strip at the top of the map that does not intercept map clicks.

### Windows Alpha

- Windows/Tauri supports online Tianditu vector and satellite maps. The user supplies a personal `tk`, which Windows encrypts with current-user DPAPI. Online tiles do not enter the 2.5 GB cache or diagnostic exports. Alpha 2 adds an explicit connection check that distinguishes “configuration saved” from “online map reachable”, and the probe never writes a tile cache.
- v0.1.0-alpha.2 was cross-built from commit 9b0fb79 and uploaded to GitHub Releases: the standalone EXE is 16,174,080 bytes and the NSIS installer with the offline WebView2 component is 217,265,419 bytes.
- The Release also includes `SHA256SUMS.txt`. Both Windows artifacts are unsigned; Windows 10/11 hardware testing, SmartScreen, installation/uninstallation, and real mainland-China network testing remain pending.
- The Windows product uses online visual basemaps only and does not plan or distribute an offline map package. Online tiles are never persisted. DEM/WBM, partial files, indexes, and calculation caches remain subject to the immutable decimal 2.5 GB cap; cached regions can still be calculated without a network connection and displayed on the WGS84 coordinate grid.

- This link-analysis source is newer than v0.1.0-alpha.2 and has not been packaged into a new Windows release.

<!-- section:windows-download -->
## Windows download

Open the [v0.1.0-alpha.2 release page](https://github.com/Arsenic-er/Ham-wireless-view/releases/tag/v0.1.0-alpha.2) and download:

- `HamHeatmap.exe`: standalone application; the target computer must already have the WebView2 Runtime.
- `HamHeatmap_*_x64-setup.exe`: per-user installer with the offline WebView2 component; this file is larger.
- `SHA256SUMS.txt`: checksums for verifying the downloaded files.

SHA-256: `HamHeatmap.exe` is a1968a48bca419d58680adca31759284f7971d36c590503451212114c3808247; the installer is 4df826b0eb96cd5a69f3c6a3a6d2b9d248c067fe60be34bb9bcd2e7bbe0fbc0e.

> [!WARNING]
> This is an unsigned internal Alpha. Propagation results are model estimates and have not been calibrated against field measurements. Do not use them as the sole basis for life-safety, emergency-command, or regulatory-compliance decisions. Publishing the source repository does not establish that the current online basemap integration or exported reports meet mainland-China public map distribution requirements.

<!-- section:mvp -->
## MVP

- Separate online point-to-point TX/RX link analysis for 1–200 km paths, using real DEM/WBM and WGS84 samples spaced ≤ 90 m apart.
- A k = 4/3 effective-Earth-curvature profile, the full first Fresnel zone (F1), and a 60% F1 clearance boundary, combined with NTIA ITM loss.
- Independently editable TX/RX antenna height, gain, and horizontal/vertical polarization; the editable RX planning threshold defaults to -120 dBm. Orthogonal polarization applies a visible, versioned 20 dB planning assumption.
- A responsive SVG terrain profile with dynamic distance ticks, the direct path, full F1 envelope, and clearance boundary.
- Stable result codes `direct-los`, `obstructed-usable`, and `predicted-unavailable` are planning predictions for the current inputs, DEM, standard atmosphere, and threshold—not field-contact guarantees. Clearing link analysis does not clear coverage heatmaps.
- 144 MHz and 430 MHz bands, with exact frequencies entered to two decimal places.
- Longley–Rice / NTIA ITM point-to-point terrain propagation.
- Base-station-to-handheld and handheld-to-base-station presets.
- Horizontal and vertical polarization.
- Transmitter ground elevation is read from the DEM by default and may be manually overridden within `-500..9000 m AMSL`; transmitter antenna height remains AGL.
- Fixed 200 km circular radius, 1 km output grid, and fixed dBm color scale.
- Terrain is used only in hidden calculations and is not rendered on the basemap.
- Light and dark UI themes.
- Online Tianditu vector/satellite maps on Windows desktop; a personal `tk` is DPAPI-encrypted, and tiles are neither persisted nor included in exports.
- Regional data caching and offline calculation, with a hard 2.5 GB limit on all persistent data.
- Internal diagnostic PNG/PDF exports with a mandatory watermark; formally compliant map export still requires basemap authorization and a map review number.
- Windows 10/11 64-bit.

<!-- section:documentation -->
## Documentation

Most engineering documents are currently written in Simplified Chinese. Their commands, paths, and recorded evidence remain authoritative while translations are added gradually.

- `docs/01-product-requirements.md`: product requirements and acceptance criteria.
- `docs/02-technical-design.md`: architecture, calculations, cache, and implementation phases.
- `docs/03-data-and-map-compliance.md`: data sources, licensing, and mainland-China map-compliance gates.
- `docs/04-test-plan.md`: model, UI, data, performance, and release tests.
- `docs/05-phase0-validation-report.md`: ITM, GLO-90, real-path, and performance validation.
- `docs/06-minimum-viability-validation.md`: real 200 km full-circle calculation, determinism, performance, and model-sensitivity acceptance.
- `docs/07-phase1-cache-validation.md`: arbitrary-point tile planning, quota, download, resume, migration, and cached calculation.
- `docs/08-land-water-validation.md`: WBM, ocean-only tiles, land/water parameter mixing, and Chengdu/Qingdao validation.
- `docs/09-phase2-desktop-slice.md`: first desktop slice, Rust IPC contract, visuals, and minimum-window validation.
- `docs/10-phase2-download-cache-slice.md`: download confirmation, progress/cancellation, region list, and reference-safe deletion.
- `docs/11-windows-cross-build.md`: server-side Windows cross-build, static CRT, artifacts, and hardware gates.
- `docs/12-phase2-export-slice.md`: internal diagnostic PNG/PDF, native save, and formal-map-export boundary.
- `docs/13-web-mercator-overlay-validation.md`: MapLibre corner-mapping error and Web Mercator overlay acceptance.
- `docs/14-windows-cross-build-gpu273312.md`: pinned toolchain on the current server and PE/installer audit.
- `docs/15-private-validation-platform.md`: SSH-tunnel validation platform, three frontend modes, operating boundary, and evidence.
- `docs/16-recovery-and-cancellation-validation.md`: cache recovery, quota boundary, cancellation linearization, and management-script hardening.
- `docs/17-manual-elevation-and-download-transport-validation.md`: manual transmitter elevation, bounded download, schema 3, and managed real-calculation evidence.
- `docs/18-progressive-coverage-preview-validation.md`: progressive preview, dual transport, real Chengdu run, and pending Windows gates.
- `docs/19-parameter-sensitivity-validation.md`: real Chengdu pixel-level parameter matrix, dual-PNG determinism, and immutable cache snapshots.
- `docs/20-tianditu-basemap-proxy.md`: Tianditu same-origin proxy, token boundary, dynamic scale, clear/replay behavior, and pending gates.
- [`docs/decisions/0024-point-to-point-link-analysis.md`](docs/decisions/0024-point-to-point-link-analysis.md): locked point-to-point link-analysis contract, equations, classifications, and verification boundary.
- `docs/decisions/`: evidence-backed engineering decisions.
- [`docs/21-protomaps-four-province-basemap.md`](docs/21-protomaps-four-province-basemap.md): historical four-province PMTiles validation evidence, no longer a product target.

<!-- section:development -->
## Development and build

The only canonical development workspace is `/home/ubuntu/hamheatmap` on `gpu-273312` (`ubuntu@150.65.181.202`). Source, dependencies, caches, builds, and validation artifacts stay on that server; the Windows computer receives only final executables.

Clone the public repository:

```bash
git clone https://github.com/Arsenic-er/Ham-wireless-view.git
cd Ham-wireless-view
```

Keep Rust, Node, Windows SDK, and download caches under the project-local `.tools` directory. Run the core Rust gates with:

```bash
scripts/cargo-project.sh test --workspace --all-targets --locked
scripts/cargo-project.sh clippy --workspace --all-targets --locked -- -D warnings
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- single-path --terrain ridge --frequency 145
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- benchmark --threads 4 --terrain flat --frequency 145
```

The decimal 2.5 GB hard-cap and crash-recovery stress test is intentionally excluded from normal tests. It requires at least 4 GB free space, writes real non-sparse data sequentially under `.runtime/cache-stress/`, forcibly exits a child process, and removes its dedicated runtime directory when finished:

```bash
scripts/cache-durability-stress.sh
```

The script rejects a non-empty stress directory and does not read, modify, or clean the real validation cache at `.runtime/validation-platform/data/`.

The real parameter-sensitivity matrix is explicit opt-in. It takes a consistent snapshot while briefly holding exclusive access to the real cache, then runs only against that snapshot and checks power, gain, frequency, height, and polarization pixel by pixel:

```bash
scripts/parameter-sensitivity-smoke.sh
```

Real DEM samples are not committed to Git. Download and verify a reproducible sample, then run:

```bash
scripts/fetch-glo90-sample.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- inspect-dem
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- dem-path --frequency 145
```

The real 200 km validation prepares GLO-90 DEM and WBM data around Chengdu and produces five diagnostic heatmaps:

```bash
scripts/fetch-glo90-chengdu-region.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- validate --threads 4
```

Diagnostic output is stored in `reports/mvp/`. It contains no basemap, boundaries, or map review information and is not for public map distribution.

Generic cache commands first display the estimated download size. A download begins only when `--yes` is explicitly added:

```bash
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache plan --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5 --yes
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache status
```

The desktop frontend uses project-pinned Node.js 24.18.0. After one project-local tool installation, run:

```bash
scripts/install-node-project.sh
scripts/node-project.sh install --prefix app
scripts/node-project.sh --prefix app run check
scripts/node-project.sh --prefix app test
scripts/node-project.sh --prefix app run build
scripts/node-project.sh --prefix app run dev
```

<!-- section:validation-platform -->
## Private server validation platform

The current source-level link-analysis changes described above have not yet been rebuilt into or restarted on the managed validation process. The commands in this section document the existing private platform; they do not indicate a public deployment.

The validation platform is for internal development only. It serves the validation React build and a Linux HTTP bridge reusing `hamheatmap-app-service` from one origin, so real data preparation, cache management, and propagation calculations can be checked. The process is fixed to `127.0.0.1:1421`; never change it to `0.0.0.0`, reuse Cockpit port `9090`, or open a new cloud-firewall port.

Build and start it on the server:

```bash
cd /home/ubuntu/hamheatmap
scripts/validation-platform.sh build
scripts/validation-platform.sh start
scripts/validation-platform.sh status
scripts/validation-platform.sh health
scripts/validation-platform.sh self-test
```

Create an SSH tunnel in Windows PowerShell and keep that terminal open:

```powershell
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 1421:127.0.0.1:1421 ubuntu@150.65.181.202
```

Open `http://127.0.0.1:1421` in a local browser. This is the tunnel's local endpoint, not a public server port. Stop the platform on the server after validation:

```bash
scripts/validation-platform.sh stop
```

Before a calculation, download estimate, or download, the validation browser requests an unguessable short-lived operation ID. The long request carries that ID, polling uses the same ID, and cancellation can affect only that exact calculation or download. Status polling uses JSON `POST`, exposes no “current operation” or operation list, and returns no heatmap, URL, server path, or detailed error.

The three frontend modes have separate privileges:

| Mode | Prepare/cache/calculate | File export | Purpose |
|---|---:|---:|---|
| Windows Tauri | Yes | Yes | Final desktop behavior |
| Private validation server | Yes | Yes | Real core validation; browser-local diagnostic download |
| Ordinary browser preview | No | No | UI and visual checks only |

Validation sends coordinates, radio parameters, and requests from the Windows browser to the user-controlled JAIST server. This is an explicit internal-test exception to the desktop rule that coordinates and results remain local. Use only test coordinates. Diagnostic files are generated and downloaded in the browser; no export endpoint or server-side file write is used. Runtime data, PIDs, logs, and build metadata stay under `.runtime/validation-platform/`; the platform uses no Docker, system service, or system-level runtime directory. See `docs/15-private-validation-platform.md`, `docs/16-recovery-and-cancellation-validation.md`, and `docs/18-progressive-coverage-preview-validation.md` for protocol and validation evidence.

The Tauri shell is under `app/src-tauri/`. JAIST Linux handles frontend work, shared Rust services, browser visual regression, and internal Windows cross-builds. A formal release still requires Windows 10/11 testing of WebView2, installers, and filesystem behavior.

Restore the pinned project-local Windows cross-toolchain and build the single-file EXE plus the installer with the offline WebView2 component:

```bash
scripts/install-windows-cross-tools.sh
scripts/tauri-windows-cross.sh
```

The restore script writes only under the server project's `.tools/`. LLVM, its archives, and the xwin SDK use about 14 GB; the Windows release target and offline WebView2 installer require additional space. This cross-build remains an internal Alpha gate and does not replace Windows hardware, code-signing, or map-compliance acceptance.

<!-- section:limitations -->
## Important limitations

The three link result classes are planning predictions under the selected inputs, real DEM/WBM, the standard-atmosphere k = 4/3 assumption, and the editable threshold. They do not guarantee a field contact, and the current model does not add buildings, vegetation, local clutter, interference, or live atmospheric conditions.

HamHeatmap is a planning and educational tool and does not guarantee an actual radio contact. The MVP does not model buildings, vegetation, urban clutter, external interference, real-time weather, anomalous propagation, water-surface reflection, or feedline loss.

Before public distribution in mainland China, the release must complete basemap authorization, review, and map review-number checks. Development basemaps or international open-source boundary data cannot be included directly in a formal release.

<!-- section:technology -->
## Technology

The desktop application uses Tauri 2.11.5, React 19.2.7, TypeScript 7.0.2, Vite 8.1.4, and MapLibre GL JS 5.24.0. The backend uses Rust, embedded SQLite, official NTIA ITM C++ v1.4, pure-Rust `tiff`, and rustls HTTPS.

All current visual basemaps are online. PMTiles JavaScript 4.4.1 and fflate 0.8.3 have been removed from the current source and next build target, and the four-province archive will not be distributed. The public Alpha 2 still contains those two historical JavaScript dependencies but no offline map archive.

<!-- section:author -->
## Author

Project creator and lead developer: [Arsenic-er](https://github.com/Arsenic-er).

<!-- section:license -->
## License

Project source code is licensed under the [Apache License 2.0](LICENSE). Maps, DEM, water data, and third-party dependencies remain subject to their own licenses and attribution requirements; see [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md).
Project authorship and release attribution are recorded in [AUTHORS.md](AUTHORS.md) and [NOTICE](NOTICE).
