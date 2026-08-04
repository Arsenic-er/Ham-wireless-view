# HamHeatmap

[English](README.md) | [简体中文](README.zh-Hans.md) | **繁體中文** | [日本語](README.ja.md)

<!-- locale: zh-Hant -->
<!-- canonical: README.md -->

HamHeatmap（業餘無線電傳播熱力圖）是一套面向中國大陸業餘無線電愛好者的開源 Windows 桌面軟體。使用者在地圖上選擇一個發射點後，軟體會根據頻率、功率、天線增益、高度、極化、地形及陸地／水面參數，以固定 200 km 半徑和 1 km 網格預測接收功率熱力圖。

英文 [`README.md`](README.md) 是規範介紹頁；本頁提供完整的繁體中文介紹。

<!-- section:current-status -->
<!-- synchronized-tests: frontend-files=17 frontend=155 rust=133 ignored=5 -->
## 目前狀態

### 線上驗證版

- 私有 validation 服務在伺服器上執行，僅監聽 `127.0.0.1:1421`，必須透過 SSH 通道存取。
- 目前原始碼在 validation 存在合法 token 時使用同源天地圖普通地圖；沒有 token 時自動使用 CARTO Voyager base + labels。衛星模式使用 EOxCloudless，並保留目前的線上地名圖層（無 token 時為 CARTO labels）。普通底圖、地名及衛星圖的失敗彼此隔離，未受影響的圖層可繼續使用；只有兩個可用視覺底圖都失效時才退回 WGS84 座標網格。受管回環服務已從乾淨提交 d7bd31a 重建：health schema 1、CARTO base/labels、EOx 衛星、舊天地圖拒絕、查詢注入拒絕與 no-store 檢查均通過；瀏覽器視覺仍待驗。
- 已實作 -140..-60 dBm、步進 1 dB 的全域顯示門檻滑桿；它會動態隱藏較弱像素，不會重新計算傳播結果，也不改變統計或本次匯出內容。目前閘門通過前端 17 files / 155 tests、Rust workspace 133 passed / 5 ignored，以及三組真實快取 HTTP 煙霧測試。8 圖層伺服器 CPU 微基準的 P95 為 5.982 ms。由於 Codex Windows ACL 故障，受管瀏覽器拖曳仍未驗證，Windows WebView2 實機測試也尚待完成。
- 過去的四省 PMTiles 路線已不再是產品目標，只保留為歷史工程證據。EOxCloudless 是 validation 與目前 Windows 原始碼使用的線上衛星圖層，其影像不會打包進 Windows 發行資產。

- 目前原始碼已實作獨立的線上雙點 TX/RX 鏈路分析模式，適用於 1–200 km 路徑。計算完成後會自動開啟非模態浮動地形／菲涅爾剖面視窗；點擊窗外會降至 42% 透明度且地圖仍可互動，點回視窗即可恢復，關閉按鈕或 Escape 可收起，並可由已完成結果重新開啟。受管程序現已包含此原始碼，但真實鏈路分析、剖面視窗和瀏覽器互動仍待驗；沒有部署公開服務。 覆蓋模式 TX 與鏈路模式 TX/RX 的初始選點提示現已縮為地圖頂部緊湊狀態列，且不會攔截地圖點擊。

### Windows Alpha

- 目前 Windows/Tauri 原始碼預設使用不需天地圖 `tk` 的 CARTO Voyager 線上地圖、地名與 EOxCloudless Sentinel-2 衛星影像。個人天地圖 `tk` 是可選覆寫，設定後由目前使用者 DPAPI 加密保存；清除後會繼續使用公共底圖。線上圖磚不會進入 2.5 GB 快取或診斷匯出。Alpha 2 引入的天地圖明確連線檢查仍保留。
- v0.1.0-alpha.2 已從提交 9b0fb79 交叉建置並上傳至 GitHub Releases：獨立 EXE 為 16,174,080 bytes，內含離線 WebView2 元件的 NSIS 安裝程式為 217,265,419 bytes。
- Release 同時提供 `SHA256SUMS.txt`。兩個 Windows 產物都未簽章；Windows 10/11 實機、SmartScreen、安裝／解除安裝，以及中國大陸真實網路仍待驗證。
- Windows 產品只使用線上視覺底圖，不規劃或發行離線地圖套件；任何線上圖磚都不會持久保存。DEM/WBM、partial 檔案、索引和計算快取仍受不可變更的十進位 2.5 GB 上限約束；已快取區域可在無網路時繼續計算，並在 WGS84 座標網格上顯示結果。

- 此鏈路分析原始碼晚於 v0.1.0-alpha.2，尚未封裝為新的 Windows 發行版。

<!-- section:windows-download -->
## Windows 下載

請開啟 [v0.1.0-alpha.2 發行頁面](https://github.com/Arsenic-er/Ham-wireless-view/releases/tag/v0.1.0-alpha.2) 下載：

- `HamHeatmap.exe`：獨立程式；目標電腦必須已安裝 WebView2 Runtime。
- `HamHeatmap_*_x64-setup.exe`：目前使用者安裝程式，內含離線 WebView2 元件，因此檔案較大。
- `SHA256SUMS.txt`：用於驗證下載檔案完整性的校驗值。

SHA-256：`HamHeatmap.exe` 為 a1968a48bca419d58680adca31759284f7971d36c590503451212114c3808247；安裝程式為 4df826b0eb96cd5a69f3c6a3a6d2b9d248c067fe60be34bb9bcd2e7bbe0fbc0e。

> [!WARNING]
> 目前版本是未簽章的內部 Alpha。傳播結果是模型估算，尚未經過外場量測校準，不得作為生命安全、緊急指揮或法規遵循決策的唯一依據。公開原始碼倉庫並不代表目前的線上底圖整合或匯出報告已符合中國大陸公開地圖發行要求。

<!-- section:mvp -->
## MVP

- 獨立的線上雙點 TX/RX 鏈路分析，支援 1–200 km 路徑，使用真實 DEM/WBM，並按 WGS84 以 ≤ 90 m 間距取樣。
- 採用 k = 4/3 有效地球曲率，繪製完整第一菲涅爾區（F1）及 60% F1 淨空邊界，並結合 NTIA ITM 損耗。
- TX/RX 的天線高度、增益及水平／垂直極化均可獨立編輯；RX 規劃閾值可編輯，預設為 -120 dBm。正交極化採用明確且版本化的 20 dB 規劃假設。
- 響應式 SVG 地形剖面包含動態距離刻度、通視直線、完整 F1 包絡及淨空邊界。
- 穩定結果代碼 `direct-los`、`obstructed-usable`、`predicted-unavailable` 只是依目前輸入、DEM、標準大氣及閾值作出的規劃預測，不保證現場通聯。清除鏈路分析不會清除覆蓋熱力圖。
- 支援 144 MHz 與 430 MHz 頻段，具體頻率可輸入至小數點後兩位。
- Longley–Rice / NTIA ITM 點對點地形傳播。
- 基地臺→手持電臺、手持電臺→基地臺預設。
- 水平／垂直極化。
- 發射點地面海拔預設由 DEM 讀取，可在 `-500..9000 m AMSL` 內手動覆寫；發射天線高度仍使用 AGL。
- 固定 200 km 圓形半徑、1 km 輸出網格及固定 dBm 色階。
- 地形只用於隱藏計算，不會顯示在底圖上。
- 淺色／深色 UI。
- Windows 桌面預設使用不需 `tk` 的 CARTO Voyager 地圖／地名與 EOxCloudless 衛星影像；有效的個人天地圖 `tk` 可選覆寫預設底圖並由 DPAPI 加密。圖磚不會持久保存，也不會納入匯出。
- 區域資料快取與離線計算；所有持久資料的硬性上限為 2.5 GB。
- 具有強制浮水印的內部診斷 PNG/PDF；正式合規地圖匯出仍需取得底圖授權與審圖號。
- Windows 10/11 64-bit。

<!-- section:documentation -->
## 文件

工程文件目前主要以簡體中文撰寫；其中記錄的指令、路徑和驗證證據仍是工程事實來源。

- `docs/01-product-requirements.md`：產品需求與驗收標準。
- `docs/02-technical-design.md`：架構、計算、快取及實作階段。
- `docs/03-data-and-map-compliance.md`：資料來源、授權及中國大陸地圖合規閘門。
- `docs/04-test-plan.md`：模型、UI、資料、效能與發行測試。
- `docs/05-phase0-validation-report.md`：ITM、GLO-90、真實路徑及效能驗證。
- `docs/06-minimum-viability-validation.md`：真實 200 km 全圓計算、確定性、效能與模型敏感度驗收。
- `docs/07-phase1-cache-validation.md`：任意點圖磚規劃、配額、下載、續傳、遷移及快取計算閉環。
- `docs/08-land-water-validation.md`：WBM、純海洋圖磚、陸水參數混合及成都／青島驗證。
- `docs/09-phase2-desktop-slice.md`：桌面首個切片、Rust IPC 契約、視覺與最小視窗驗證。
- `docs/10-phase2-download-cache-slice.md`：下載確認、進度／取消、區域清單和參照安全刪除。
- `docs/11-windows-cross-build.md`：伺服器端 Windows 交叉建置、靜態 CRT、產物及實機閘門。
- `docs/12-phase2-export-slice.md`：內部診斷 PNG/PDF、原生儲存及正式地圖匯出邊界。
- `docs/13-web-mercator-overlay-validation.md`：MapLibre 四角映射誤差與 Web Mercator 覆蓋圖層驗收。
- `docs/14-windows-cross-build-gpu273312.md`：目前伺服器的固定工具鏈及 PE／安裝程式稽核。
- `docs/15-private-validation-platform.md`：SSH 通道私有驗證平台、三種前端模式、執行邊界與驗證記錄。
- `docs/16-recovery-and-cancellation-validation.md`：快取復原、配額邊界、取消線性化及管理指令碼強化。
- `docs/17-manual-elevation-and-download-transport-validation.md`：手動發射點海拔、有界下載、schema 3 及受管真實計算證據。
- `docs/18-progressive-coverage-preview-validation.md`：漸進式預覽、雙傳輸契約、真實成都執行及待驗 Windows 閘門。
- `docs/19-parameter-sensitivity-validation.md`：真實成都逐像素參數矩陣、雙 PNG 確定性及不可變快取快照。
- `docs/20-tianditu-basemap-proxy.md`：天地圖同源代理、token 邊界、動態比例尺、清除／重播行為與待驗閘門。
- [`docs/decisions/0024-point-to-point-link-analysis.md`](docs/decisions/0024-point-to-point-link-analysis.md)：鎖定的點對點鏈路分析契約、公式、分類及驗證邊界。
- `docs/decisions/`：具有證據的工程決策。
- [`docs/21-protomaps-four-province-basemap.md`](docs/21-protomaps-four-province-basemap.md)：已退出目前產品目標的四省 PMTiles 歷史驗證證據。

<!-- section:development -->
## 開發與建置

唯一規範的開發工作區是 `gpu-273312`（`ubuntu@150.65.181.202`）上的 `/home/ubuntu/hamheatmap`。原始碼、相依套件、快取、建置及驗證產物都保留在該伺服器，Windows 電腦只接收最終執行檔。

複製公開倉庫：

```bash
git clone https://github.com/Arsenic-er/Ham-wireless-view.git
cd Ham-wireless-view
```

建議將 Rust、Node、Windows SDK 與下載快取放在專案本機的 `.tools` 目錄。Rust 核心檢查：

```bash
scripts/cargo-project.sh test --workspace --all-targets --locked
scripts/cargo-project.sh clippy --workspace --all-targets --locked -- -D warnings
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- single-path --terrain ridge --frequency 145
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- benchmark --threads 4 --terrain flat --frequency 145
```

十進位 2.5 GB 硬性上限及當機復原壓力測試不會包含在一般測試中。它需要伺服器至少有 4 GB 可用空間，會在 `.runtime/cache-stress/` 依序寫入真實非稀疏資料、強制子行程結束，並在完成後刪除專用執行目錄：

```bash
scripts/cache-durability-stress.sh
```

指令碼會拒絕使用非空的壓力測試目錄，也不會讀取、修改或清理 `.runtime/validation-platform/data/` 的真實驗證快取。

真實參數敏感度矩陣必須明確選擇執行。它會在短暫獨占真實快取時複製一致的快照，之後只對該快照執行，逐像素驗證功率、增益、頻率、高度及極化：

```bash
scripts/parameter-sensitivity-smoke.sh
```

真實 DEM 樣本不提交至 Git。下載並驗證可重現樣本後執行：

```bash
scripts/fetch-glo90-sample.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- inspect-dem
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- dem-path --frequency 145
```

真實 200 km 驗證會準備成都周邊的 GLO-90 DEM 與 WBM，並產生五張診斷熱力圖：

```bash
scripts/fetch-glo90-chengdu-region.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- validate --threads 4
```

診斷輸出位於 `reports/mvp/`，不含底圖、邊界或審圖資訊，不用於公開地圖發行。

通用快取指令會先顯示預估下載量；只有明確加入 `--yes` 才會開始下載：

```bash
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache plan --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5 --yes
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache status
```

桌面前端使用專案固定的 Node.js 24.18.0。完成一次專案內工具安裝後，可執行：

```bash
scripts/install-node-project.sh
scripts/node-project.sh install --prefix app
scripts/node-project.sh --prefix app run check
scripts/node-project.sh --prefix app test
scripts/node-project.sh --prefix app run build
scripts/node-project.sh --prefix app run dev
```

<!-- section:validation-platform -->
## 私有伺服器驗證平台

上述原始碼層級的鏈路分析變更尚未重新建置至受管 validation 程序，也未在該程序上重新啟動。本節指令描述的是現有私有平台，不代表已公開部署。

驗證平台只供內部開發使用。它從同一來源提供 validation React 前端與重複使用 `hamheatmap-app-service` 的 Linux HTTP 橋接器，以檢查真實資料準備、快取管理和傳播計算。行程固定監聽 `127.0.0.1:1421`；不得改為 `0.0.0.0`、占用 Cockpit 的 `9090`，或在雲端防火牆開啟新連接埠。

在伺服器建置並啟動：

```bash
cd /home/ubuntu/hamheatmap
scripts/validation-platform.sh build
scripts/validation-platform.sh start
scripts/validation-platform.sh status
scripts/validation-platform.sh health
scripts/validation-platform.sh self-test
```

在 Windows PowerShell 建立 SSH 通道，並保持該終端機開啟：

```powershell
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 1421:127.0.0.1:1421 ubuntu@150.65.181.202
```

在本機瀏覽器開啟 `http://127.0.0.1:1421`。這是通道的本機端點，不是伺服器公開連接埠。驗證完成後在伺服器停止：

```bash
scripts/validation-platform.sh stop
```

在計算、下載估算或下載開始前，validation 瀏覽器會取得不可猜測的短期 operation ID。長請求帶上該 ID，輪詢使用同一 ID，取消也只能影響該次計算或下載。狀態輪詢使用 JSON `POST`，不提供「目前作業」或作業清單，也不回傳熱力圖、URL、伺服器路徑或詳細錯誤。

三種前端模式具有不同權限：

| 模式 | 準備／快取／計算 | 檔案匯出 | 用途 |
|---|---:|---:|---|
| Windows Tauri | 是 | 是 | 最終桌面行為 |
| 私有 validation server | 是 | 是 | 真實核心驗證；瀏覽器本機診斷下載 |
| 一般瀏覽器 preview | 否 | 否 | 僅供 UI 與視覺檢查 |

validation 會把座標、無線電參數及請求從 Windows 瀏覽器送到使用者控制的 JAIST 伺服器。這是桌面版「座標與結果只留在本機」原則的明確內部測試例外，請只使用測試座標。診斷檔案由瀏覽器產生並下載，不使用匯出端點，也不在伺服器寫入檔案。執行資料、PID、日誌及建置中繼資料都留在 `.runtime/validation-platform/`；平台不使用 Docker、系統服務或系統層級執行目錄。協定與驗證證據請參閱 `docs/15-private-validation-platform.md`、`docs/16-recovery-and-cancellation-validation.md` 及 `docs/18-progressive-coverage-preview-validation.md`。

Tauri 殼層位於 `app/src-tauri/`。JAIST Linux 負責前端、共用 Rust 服務、瀏覽器視覺回歸及內部 Windows 交叉建置。正式發行仍須在 Windows 10/11 驗證 WebView2、安裝程式和檔案系統行為。

還原固定的專案內 Windows 交叉工具鏈，並建置單檔 EXE 與內含離線 WebView2 元件的安裝程式：

```bash
scripts/install-windows-cross-tools.sh
scripts/tauri-windows-cross.sh
```

還原指令碼只會寫入伺服器專案的 `.tools/`。LLVM、歸檔與 xwin SDK 合計約占 14 GB；Windows release target 和離線 WebView2 安裝程式還會使用額外空間。此交叉建置仍只是內部 Alpha 閘門，不能取代 Windows 實機、程式碼簽章或地圖合規驗收。

<!-- section:limitations -->
## 重要限制

三類鏈路結果是在所選參數、真實 DEM/WBM、標準大氣 k = 4/3 假設及可編輯閾值下的規劃預測，不保證現場通聯；目前模型也未加入建築、植被、局部雜波、干擾或即時大氣條件。

HamHeatmap 是規劃及教學工具，不保證實際通聯。MVP 不考慮建築、植被、都市雜波、外部干擾、即時天氣、異常傳播、水面反射或饋線損耗。

面向中國大陸公開發行前，必須完成底圖授權、審核及審圖號檢查。開發底圖或國際開源邊界資料不能直接納入正式發行版。

<!-- section:technology -->
## 技術架構

桌面程式使用 Tauri 2.11.5、React 19.2.7、TypeScript 7.0.2、Vite 8.1.4 和 MapLibre GL JS 5.24.0。後端使用 Rust、內嵌 SQLite、NTIA 官方 ITM C++ v1.4、純 Rust `tiff` 及 rustls HTTPS。

目前所有視覺底圖都在線上。PMTiles JavaScript 4.4.1 與 fflate 0.8.3 已從目前原始碼和下一次建置目標移除，四省歸檔也不會發行。現有公開 Alpha 2 仍包含這兩個歷史 JavaScript 相依套件，但不含離線地圖歸檔。

<!-- section:author -->
## 開發者

專案建立者與首席開發者：[Arsenic-er](https://github.com/Arsenic-er)。

<!-- section:license -->
## 授權條款

專案原始碼採用 [Apache License 2.0](LICENSE)。地圖、DEM、水體資料及第三方相依套件各自遵循其授權與署名要求；詳見 [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md)。
專案作者身分與發行署名記錄於 [AUTHORS.md](AUTHORS.md) 與 [NOTICE](NOTICE)。
