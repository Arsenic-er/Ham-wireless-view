# HamHeatmap Agent Instructions

## Workspace

- Canonical workspace: `/home/ubuntu/hamheatmap` on `gpu-273312` (`ubuntu@150.65.181.202`).
- Keep project documents, source, fixtures, scripts, build tools, dependency caches, validation artifacts, and build outputs on this server inside the canonical workspace. The Windows desktop may receive only the final EXE requested by the user.
- Read all files in `docs/` before changing product scope or architecture.

## Durable product decisions

- Windows 10/11 64-bit desktop application for mainland China.
- 144 MHz and 430 MHz bands; exact frequency has two decimal places.
- Single transmitter, fixed 200 km radius, fixed 1 km output grid.
- Output is dBm only with fixed red/orange/yellow/green/cyan/blue scale; below -140 dBm is transparent.
- Use NTIA Longley–Rice/ITM point-to-point mode with terrain profiles.
- Support horizontal and vertical polarization; default vertical.
- Do not model feedline loss.
- Land and water use different ground parameters, but all water is one class.
- DEM is computation-only. Never add hillshade, contours, terrain colors, slope layers, or 3D terrain to the map.
- Heatmap pixels are not inspectable. Do not add hover/click dBm probes.
- Provide light and dark UI themes; default follows Windows system theme.
- Support application UI locales `en`, `zh-CN`, `zh-TW`, and `ja-JP`; English is the source locale and fallback.
- Persist explicit locale choice, otherwise follow the Windows/browser locale, and never lose map/session state when changing language.
- Keep `README.md` as the canonical English project page with complete Simplified Chinese, Traditional Chinese, and Japanese linked translations.
- All persistent data, including partial downloads, has an immutable decimal 2.5 GB cap: 2,500,000,000 bytes.
- Every propagation run still has one transmitter. The current app session may retain up to eight completed distinct-site coverage layers; same-site recalculation replaces that site. Do not persist them or interpret overlap as joint/multi-transmitter field strength. No project files, cross-start history, telemetry, or cloud sync in MVP.

## Map compliance

- Map compliance is a P0 release gate, not a polish task.
- Never ship Natural Earth, OSM, or another international boundary dataset in a mainland-China public build without formal approval.
- Production basemap must have a valid source, review number, offline/export authorization, and required attribution.
- Do not redraw or reinterpret national/provincial boundaries.
- Development builds using placeholder maps must say they are internal and not for public distribution.
- The Windows online basemap uses a user-owned Tianditu key through the fixed Tauri `tianditu://localhost/{layer}/{z}/{x}/{y}` protocol; never embed a shared key or expose it to the WebView, URL, log, bootstrap payload, or Git.
- Permit only `vec/cva/img/cia`, a fixed HTTPS upstream, canonical tile coordinates, no redirects, bounded responses, image validation, and `no-store`.
- Protect the key with Windows current-user DPAPI. Non-Windows builds must not persist it in plaintext.
- Online basemap tiles are transient display data: never add them to the 2.5 GB cache, bulk-download them, or include them in diagnostic PNG/PDF exports.

## Engineering rules

- Keep ITM and data-source versions pinned and recorded in outputs.
- Avoid magic propagation constants; use a versioned `ModelDefaults` structure and decision records.
- Use official reference cases and synthetic terrain fixtures before real-data UI work.
- Benchmark the 401×401 coverage engine before promising the 60-second target.
- Treat missing/corrupt DEM as a blocking error; never silently substitute zero on land.
- Keep frontend map code unable to access raw DEM tiles.
- Preserve user privacy: coordinates and calculation results stay local.
- Visible basemaps are online-only. Do not add PMTiles, MBTiles, offline visual map packages, persistent basemap tile caches, or a future offline-basemap download path.
- Offline operation refers only to already cached DEM, WBM, indexes, and calculation data. Without an online basemap, keep the WGS84 grid, transmitter markers, range overlays, heatmaps, and cached-area calculations usable.
- Add or update tests for every change to propagation math, cache quota, coordinate transforms, and color thresholds.
- Restore the pinned project-local Windows cross toolchain with `scripts/install-windows-cross-tools.sh`; do not install it system-wide or download it to the Windows desktop.

## Private validation platform

- The validation platform is internal test infrastructure, not a public web product and not a replacement for the Windows/Tauri application.
- Build it only with `scripts/validation-platform.sh build`. That command must use `VITE_VALIDATION_SERVER=1`, the project-local Node toolchain, and the release `hamheatmap-validation-server` binary.
- The managed platform must bind only to `127.0.0.1:1421`. Access it through SSH local forwarding. Never bind it to `0.0.0.0`, open a cloud firewall port, reuse Cockpit port `9090`, or add a public reverse proxy.
- Do not use Docker, systemd units, Caddy, Nginx, or system-level storage for this platform. Keep data, PID files, logs, and metadata under `.runtime/validation-platform/`.
- Use `scripts/validation-platform.sh start|status|health|stop`; never stop processes by a generic name or unverified PID. The script must continue to verify the executable and fixed bind/dist/data arguments before signalling a process.
- Preserve the three frontend modes: Tauri has download/cache/calculate/native-save export; validation-server has download/cache/calculate plus browser-local diagnostic downloads; ordinary preview is interface-only and must not perform real mutation or calculation.
- Never add server-side export or arbitrary filesystem paths to the validation API. Validation PNG/PDF export must stay client-side through a browser Blob download, without sending report bytes or a destination path to the server.
- Validation mode is an explicit privacy exception: test coordinates and requests leave the Windows browser for the user-controlled server through SSH. Keep the disclosure banner and do not use sensitive real coordinates.
- Every validation-server calculation or download-family operation must begin with a server-generated CSPRNG UUIDv4 capability from `POST /api/operation-ticket`; a long request may atomically consume only a matching reserved ticket, and a busy response must not consume it.
- Status, cancellation, progress, finish, drop cleanup, and acknowledgement must bind to the exact operation ID. Never fall back to "the current operation" or cancel by kind alone, and never add a current/list endpoint.
- Keep reserved tickets bounded to 32 entries with a 60-second TTL and terminal snapshots bounded to 32 entries with a 5-minute TTL. Terminal/status payloads may contain only whitelisted state and progress metadata; never results, PNG data, URLs, filesystem paths, or detailed errors.
- Serialize progress, cancellation, and finish through the same operation-state mutex. Cancellation accepted before finish wins; a dropped lease publishes a failed terminal snapshot; an old poll or cancel must never affect a later operation.
- Validation-browser progress uses non-overlapping same-origin POST polling and reuses the existing calculation/download progress listeners. Keep the synchronous long request as the authoritative result, isolate stale polls by operation ID and client generation, and acknowledge completed tickets best-effort.
- Do not start or stop the persistent validation process unless the root task explicitly requests it. Do not describe code tests, a real Chengdu calculation, browser visuals, Windows behavior, or map compliance as passed until the corresponding evidence has actually been recorded.

## Documentation

- Product changes update `docs/01-product-requirements.md`.
- Architecture changes update `docs/02-technical-design.md` and add a decision record when non-trivial.
- Data or basemap changes update `docs/03-data-and-map-compliance.md`.
- Acceptance changes update `docs/04-test-plan.md`.

## Authorship and licensing

- Ham Wireless View was created and is led by Arsenic-er. Keep AUTHORS.md, NOTICE, .github/CODEOWNERS, package metadata, and the four localized README license sections consistent with that attribution.
- Every first-party source, test, script, workflow, Cargo manifest, HTML, CSS, and original SVG file must keep the syntax-appropriate header containing Ham Wireless View, Project creator and lead developer: Arsenic-er, SPDX-FileCopyrightText: 2026 Arsenic-er, and SPDX-License-Identifier: Apache-2.0.
- Preserve shebangs, HTML doctypes, and XML declarations before attribution blocks where required by the file format.
- Never stamp or rewrite third_party/**, lock files, app/src-tauri/gen/**, LICENSE, or third-party attribution content as if Arsenic-er owned it.
- Run python3 scripts/check-source-attribution.py before handoff. Any missing first-party header, protected-file misattribution, or unclassified tracked/untracked file is a blocking failure.
- New non-commentable first-party formats must be explicitly classified by the checker and represented in central metadata; do not weaken the gate with a catch-all exclusion.
