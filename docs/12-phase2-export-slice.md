# Phase 2 内部诊断导出切片

- 日期：2026-07-16
- 状态：核心、Windows 目标和 production 重建通过；Windows GUI 冒烟待完成

## 完成内容

前端新增导出弹窗和固定报告渲染器。只有存在未过期的真实计算结果时按钮可用，导出开始时冻结结果、参数和时间，并用同步门闩阻止重复操作。

报告固定为 1600×1100 浅色画布，包含：

- 完整固定 200 km 圆和中心发射点；
- 100 km 比例尺；
- 与 Rust `color_for_dbm` 相同的 -60/-75/-90/-105/-120/-140 dBm 连续色标；
- 场景、频率、功率、天线增益、高度和极化；
- 有效像素、最大/平均/最小 dBm、水体影响和耗时；
- HamHeatmap、NTIA ITM v1.4 固定提交、`land-water-v1` 和 Copernicus DEM GLO-90 DEM/WBM 标识；
- 带时区的生成时间、模型限制和不可移除的“内部测试，不得公开发布”提示。

当前报告不含行政边界、审图号或未授权底图，因此只允许内部 Alpha 使用，不代表正式地图导出合规已经完成。

## Rust 导出核心

新增 workspace crate `hamheatmap-export`：

- 只接受 `data:image/png;base64,`；
- Data URL 上限 12,000,000 字符，解码 PNG 上限 8,000,000 字节；
- 强制 PNG IHDR 和完整解码均为 1600×1100；
- PNG 原样保存；PDF 使用 `printpdf 0.11.1` 生成 A4 横向单页；
- 建议文件名只接受 ASCII 字母、数字、点、连字符和下划线，扩展名必须匹配；
- 文件先写同目录唯一临时文件并 `sync_all`，再原子替换目标；失败清理临时文件。

Tauri 只增加 Windows 目标的 `tauri-plugin-dialog 2.7.1`。保存对话框和最终路径写入都在 Rust 命令内完成，没有给 WebView 增加 `fs` capability。

## 已通过验证

- `hamheatmap-export` 6/6：固定 PNG 往返、可解析单页 PDF、非法 MIME/Base64/尺寸/超限拒绝、文件名约束、原子替换成功和失败均无残片。
- 前端 11/11：原有 8 项加报告模型、文件名和色标锚点。
- TypeScript 类型检查通过。
- `x86_64-pc-windows-msvc` cargo-xwin `check --locked` 通过，包含 Tauri dialog、PDF 和 Windows 原子替换代码。
- Production EXE/NSIS 重建通过；EXE 为 16,039,936 bytes、SHA-256 `e984f112cb0ba2dc3918beca3a04719fd4e398629604d72d802e48074a55dc8a`。
- EXE 导入表复查未出现 `VCRUNTIME`、`MSVCP`、`api-ms-win-crt` 或 `UCRTBASE` 动态运行库。

## 仍需验证

- 用真实计算结果在 Windows WebView2 生成 PNG，验证 1600×1100、中文、圆、比例尺和色标布局。
- PNG/PDF 保存、取消、覆盖、只读目录、中文/空格/长路径和失败无残片。
- 在另一台 Windows 10/11 电脑打开 PDF/PNG。
- 在 Windows 上首次启动最新 EXE，完成 WebView2、主题、单实例和原生保存对话框冒烟。
- 合规底图供应者、审图号、署名和导出授权就绪后的正式地图离屏渲染。
