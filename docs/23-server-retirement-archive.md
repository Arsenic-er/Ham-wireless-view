# Server retirement and recovery archive

- Archive date: 2026-08-22
- Repository: https://github.com/Arsenic-er/Ham-wireless-view
- Public release: `v0.1.0-alpha.3`
- Purpose: preserve the reproducible project state before the temporary build server expires

## 1. What is archived on GitHub

The public repository contains the complete tracked source tree, project documentation, dependency locks, test tooling, data-download adapters, validation scripts, and Windows cross-build scripts. The `main` branch and the `v0.1.0-alpha.3` tag are the canonical source archive.

The Alpha 3 GitHub Release preserves:

- `HamHeatmap.exe`: standalone unsigned Windows x64 executable;
- `HamHeatmap_0.1.0_x64-setup.exe`: unsigned per-user NSIS installer with the offline WebView2 component;
- `SHA256SUMS.txt`: release asset checksums;
- `runtime-computation-data.sha256`: the reproducibility manifest for the server's DEM/WBM set.

The binaries were built from product-code commit `5482f05`. Later commits before the archive tag only record artifacts, documentation, and repository housekeeping.

## 2. Runtime data inventory

| Item | Count / size | Archive decision |
| --- | ---: | --- |
| Copernicus GLO-90 2021_1 DEM GeoTIFF | 101 files / 442,592,438 bytes | Source/version and every SHA-256 archived; re-download through project scripts |
| Copernicus GLO-90 2021_1 WBM GeoTIFF | 101 files / 3,343,361 bytes | Source/version and every SHA-256 archived; re-download through project scripts |
| Historical four-province PMTiles | 33,044,072 bytes | Not published; historical source/size/hash only |
| Runtime SQLite cache/index | 262,144 bytes at audit time | Not published; reconstruct from downloaded assets |
| Runtime state, logs and temporary operation files | variable | Not published |

The exact 202-file DEM/WBM manifest is [`docs/archive/runtime-computation-data.sha256`](archive/runtime-computation-data.sha256). Run this from the restored runtime data root:

```bash
sha256sum -c /path/to/Ham-wireless-view/docs/archive/runtime-computation-data.sha256
```

Expected result: 202 files report `OK`.

## 3. Intentionally excluded material

The following server content is reproducible and is not project data suitable for Git:

- approximately 16 GB of project-local compiler/tool downloads under `.tools/`;
- approximately 22 GB of Rust/Tauri build outputs under the workspace `target/` directories;
- approximately 190 MB of `node_modules/`;
- old copies of release artifacts that already exist in GitHub Releases.

The following content is deliberately private:

- authentication/device-login logs and credentials;
- validation PIDs, server logs, temporary requests and operation state;
- cached calculations containing exact user-selected coordinates or results;
- the SQLite runtime cache database;
- the historical PMTiles binary, whose embedded upstream boundary/land-cover attribution chain was not cleared for public redistribution.

The raw chat transcript is excluded because it contains operational paths, server access details, test coordinates and authentication history. Its public-safe decisions are summarized in [`docs/22-project-history-and-handoff.zh-Hans.md`](22-project-history-and-handoff.zh-Hans.md).

## 4. Recorded binary checksums

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `HamHeatmap.exe` | 16,296,960 | `0cc828063378caa5ce2a588377b506abfca3f3c741fccd33cec997e61c3af685` |
| `HamHeatmap_0.1.0_x64-setup.exe` | 217,333,194 | `9b2dea3938bf1330fa3822fad4fbedf22e61ae11056d35480634f9e2b1261107` |

Both binaries are unsigned Alpha artifacts. Their presence in a Release is an archival and testing convenience, not a production-readiness claim.

## 5. Restore source and toolchains

```bash
git clone https://github.com/Arsenic-er/Ham-wireless-view.git
cd Ham-wireless-view
git checkout v0.1.0-alpha.3

scripts/install-rust-project.sh
scripts/install-node-project.sh
scripts/node-project.sh install --prefix app
```

The scripts install project-scoped tools under `.tools/`; they do not require the retired server's global tool state.

Run the source gates:

```bash
scripts/cargo-project.sh test --workspace --all-targets --locked
scripts/cargo-project.sh clippy --workspace --all-targets --locked -- -D warnings
scripts/node-project.sh --prefix app run check
scripts/node-project.sh --prefix app test
scripts/node-project.sh --prefix app run build
```

## 6. Restore calculation data

For the small reference sample:

```bash
scripts/fetch-glo90-sample.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- inspect-dem
```

For the real 200 km validation region:

```bash
scripts/fetch-glo90-chengdu-region.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- validate --threads 4
```

Arbitrary regions can be planned and prepared through the cache CLI. A download starts only after explicit `--yes`:

```bash
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache plan --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5 --yes
```

All persistent DEM/WBM, partial files, indexes and calculation caches remain subject to the decimal 2,500,000,000-byte cap.

## 7. Rebuild Windows artifacts

```bash
scripts/install-windows-cross-tools.sh
scripts/tauri-windows-cross.sh
scripts/verify-windows-artifacts.sh
```

The Windows cross-toolchain and build targets require substantial temporary disk space. Do not commit them; keep only verified final artifacts in GitHub Releases.

## 8. Release/recovery acceptance

Before deleting an expiring server, verify:

1. `main` and `v0.1.0-alpha.3` resolve to the intended archive commit.
2. The four Alpha 3 Release assets exist with the recorded sizes and hashes.
3. The repository's CI run for the archive commit succeeds.
4. A fresh clone can install tools and pass the documented source gates.
5. Required DEM/WBM regions can be re-downloaded and match the archived manifest where the same 202-file set is restored.

No server files should be deleted merely because this document exists; deletion is a separate owner-authorized operation.
