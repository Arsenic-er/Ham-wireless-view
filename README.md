# HamHeatmap

HamHeatmap（业余无线电传播热力图）是一个面向中国大陆业余无线电爱好者的开源 Windows 桌面工具。用户在地图上选择一个发射点，软件根据频率、功率、天线增益、高度、极化、地形和陆地/水面参数，生成固定半径 200 km、1 km 像素的预测接收功率热力图。

## 当前状态

需求基线已经确认，Phase 0、计算核心最小可行性验证、Phase 1 DEM/WBM 缓存闭环和陆地/统一水体传播建模已经通过。Phase 2 已完成 React/MapLibre 主界面、地图点选、参数与主题、Tauri→Rust 真实计算、区域准备与安全缓存删除、服务器 Windows 构建，以及带强制内部水印的离线 PNG/PDF 诊断报告。合规底图、正式地图导出、热力图最终重投影、代码签名和 Windows 10/11 完整实机验收仍未完成。

首轮结果见 `docs/05-phase0-validation-report.md`，真实地形最小可行性结果见 `docs/06-minimum-viability-validation.md`，缓存闭环见 `docs/07-phase1-cache-validation.md`，陆地/水体结果见 `docs/08-land-water-validation.md`，桌面首切片见 `docs/09-phase2-desktop-slice.md`，下载与缓存交互见 `docs/10-phase2-download-cache-slice.md`。通过桌面服务契约运行成都真实缓存时，125,628 个像素约 9.75 秒完成；下载状态烟雾测试确认 50 个 DEM/WBM 资产无需重复下载并正确进入 ready。上述数字仍不是 Windows 整机验收。

> [!WARNING]
> 当前版本是未签名的内部 Alpha。传播结果是模型估算，尚未经过外场测量校准，不得作为生命安全、应急指挥或法规合规决策的唯一依据。仓库公开的是源代码；这不代表当前占位地图或导出报告已经满足中国大陆公开地图发行要求。

## MVP

- 144 MHz 与 430 MHz，具体频率可输入两位小数。
- Longley–Rice / NTIA ITM 点对点地形传播。
- 基地台→手台、手台→基地台预设。
- 水平/垂直极化。
- 200 km 圆形范围、1 km 输出、dBm 固定色标。
- 地形只用于隐藏计算，不在底图显示。
- 浅色/深色 UI。
- 区域数据缓存和离线计算，所有持久数据硬上限 2.5 GB。
- 带强制水印的内部诊断 PNG/PDF；正式合规地图导出待底图授权与审图号。
- Windows 10/11 64 位。

## 文档

- `docs/01-product-requirements.md`：产品需求和验收标准。
- `docs/02-technical-design.md`：架构、计算、缓存与实施阶段。
- `docs/03-data-and-map-compliance.md`：数据来源、许可和中国大陆地图合规门槛。
- `docs/04-test-plan.md`：模型、UI、数据、性能和发布测试。
- `docs/05-phase0-validation-report.md`：ITM、GLO-90、真实路径与性能验证结果。
- `docs/06-minimum-viability-validation.md`：真实 200 km 全圆计算、确定性、性能和模型敏感性验收。
- `docs/07-phase1-cache-validation.md`：任意点瓦片规划、配额、下载、续传、迁移和缓存计算闭环。
- `docs/08-land-water-validation.md`：WBM、纯海洋瓦片、陆水参数混合及成都/青岛真实验证。
- `docs/09-phase2-desktop-slice.md`：桌面首切片、Rust IPC 契约、视觉与最小窗口验证。
- `docs/10-phase2-download-cache-slice.md`：下载确认、进度/取消、区域列表和引用安全删除验证。
- `docs/11-windows-cross-build.md`：服务器 Windows 交叉构建、静态 CRT、产物和实机门槛。
- `docs/12-phase2-export-slice.md`：内部诊断 PNG/PDF、原生保存与正式地图导出边界。
- `docs/decisions/`：带证据的工程决策记录。

## 开发与构建

克隆公开仓库：

```bash
git clone https://github.com/Arsenic-er/Ham-wireless-view.git
cd Ham-wireless-view
```

推荐把 Rust、Node、Windows SDK 与下载缓存放在项目自己的 `.tools` 下。Rust 核心检查：

```bash
scripts/cargo-project.sh test --workspace --all-targets --locked
scripts/cargo-project.sh clippy --workspace --all-targets --locked -- -D warnings
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- single-path --terrain ridge --frequency 145
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- benchmark --threads 4 --terrain flat --frequency 145
```

真实 DEM 样本不提交到 Git。可重复下载并验证后运行：

```bash
scripts/fetch-glo90-sample.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- inspect-dem
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- dem-path --frequency 145
```

真实 200 km 验证会准备成都周边的 GLO-90 DEM 与 WBM，并生成五张诊断热力图：

```bash
scripts/fetch-glo90-chengdu-region.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- validate --threads 4
```

诊断输出位于 `reports/mvp/`，不含底图、边界或审图信息，不用于公开地图发布。

通用缓存命令会先显示预计下载量；只有显式增加 `--yes` 才开始下载：

```bash
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache plan --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5 --yes
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache status
```

桌面前端使用项目内固定的 Node.js 24.18.0。完成一次项目内工具安装后，可运行：

```bash
scripts/install-node-project.sh
scripts/node-project.sh install --prefix app
scripts/node-project.sh --prefix app run check
scripts/node-project.sh --prefix app test
scripts/node-project.sh --prefix app run build
scripts/node-project.sh --prefix app run dev
```

Tauri 壳层位于 `app/src-tauri/`。JAIST Linux 负责前端、共享 Rust 服务、浏览器视觉回归和内部 Windows 交叉构建；正式发布仍必须在 Windows 10/11 验证 WebView2、安装包和文件系统行为。

项目内交叉工具准备完成后，服务器可生成 Windows 单文件 EXE 与内嵌 WebView2 的离线安装包：

```bash
scripts/tauri-windows-cross.sh
```

该交叉构建只用于内部 Alpha；它不替代 Windows 10/11 实机、代码签名和地图合规验收。详细记录见 `docs/11-windows-cross-build.md`。

## 重要限制

该软件是规划与教学工具，不保证实际通联。MVP 不考虑建筑、植被、城市杂波、外部干扰、实时天气、异常传播、水面反射或馈线损耗。

面向中国大陆公开发行前，必须完成合规底图授权、审核和审图号检查。开发底图或国际开源边界不能直接进入正式发行版。

## 技术栈

桌面端使用 Tauri 2.11.5、React 19.2.7、TypeScript 7.0.2、Vite 8.1.4 和 MapLibre GL JS 5.24.0。后端使用 Rust、内嵌 SQLite、NTIA 官方 ITM C++ v1.4、纯 Rust `tiff` 和 rustls HTTPS。

## 许可证

项目源代码采用 Apache License 2.0。地图、DEM、水体和第三方依赖分别遵守其自身许可与署名要求。
