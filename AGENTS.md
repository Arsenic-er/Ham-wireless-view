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
- All persistent data, including partial downloads, has an immutable decimal 2.5 GB cap: 2,500,000,000 bytes.
- No project save/history, multi-transmitter comparison, telemetry, or cloud sync in MVP.

## Map compliance

- Map compliance is a P0 release gate, not a polish task.
- Never ship Natural Earth, OSM, or another international boundary dataset in a mainland-China public build without formal approval.
- Production basemap must have a valid source, review number, offline/export authorization, and required attribution.
- Do not redraw or reinterpret national/provincial boundaries.
- Development builds using placeholder maps must say they are internal and not for public distribution.

## Engineering rules

- Keep ITM and data-source versions pinned and recorded in outputs.
- Avoid magic propagation constants; use a versioned `ModelDefaults` structure and decision records.
- Use official reference cases and synthetic terrain fixtures before real-data UI work.
- Benchmark the 401×401 coverage engine before promising the 60-second target.
- Treat missing/corrupt DEM as a blocking error; never silently substitute zero on land.
- Keep frontend map code unable to access raw DEM tiles.
- Preserve user privacy: coordinates and calculation results stay local.
- Add or update tests for every change to propagation math, cache quota, coordinate transforms, and color thresholds.
- Restore the pinned project-local Windows cross toolchain with `scripts/install-windows-cross-tools.sh`; do not install it system-wide or download it to the Windows desktop.

## Documentation

- Product changes update `docs/01-product-requirements.md`.
- Architecture changes update `docs/02-technical-design.md` and add a decision record when non-trivial.
- Data or basemap changes update `docs/03-data-and-map-compliance.md`.
- Acceptance changes update `docs/04-test-plan.md`.

