# ADR 0009：服务器持有 Windows 交叉构建环境

- 日期：2026-07-16
- 状态：已采纳

## 背景

项目源代码、依赖和构建缓存必须只保存在 JAIST 服务器 `/home/ubuntu/hamheatmap`。开发者 Windows 桌面只接收最终可直接启动的 `HamHeatmap.exe`，不能承担 Rust、Node、MSVC SDK 或 NSIS 构建环境。

Tauri 官方把 Windows 原生构建列为首选路径，同时提供基于 `cargo-xwin` 和 NSIS 的 Linux 交叉构建方案。项目需要在不安装系统级软件包、不使用开发机磁盘的条件下产生可复测的 Windows 内部版本。

## 决策

- 在项目忽略目录 `.tools/` 内固定 Node、Rust、cargo-xwin、xwin SDK、LLVM、NSIS 与 proot。
- 使用 `scripts/tauri-windows-cross.sh` 作为服务器唯一的完整 Windows 构建入口。
- 使用 `scripts/cargo-xwin-static.sh` 启用 `+crt-static`，并显式链接 xwin SDK 的 `libucrt.lib`，避免 Tauri `cdylib` 的 UCRT 默认库冲突。
- 使用 `scripts/makensis-project.sh` 将项目内 NSIS 资源映射到 NSIS 的固定查找路径，不修改服务器系统目录。
- NSIS 安装包内嵌 WebView2 离线安装组件；原始 EXE 供已安装 WebView2 的开发机做快速冒烟测试。
- 构建产物保持未签名，只允许内部 Alpha 验证；公开发布前必须完成 Windows 原生回归、代码签名和地图合规门槛。

## 结果

优点：

- Windows 开发机只保留一个最终 EXE。
- 服务器可一条命令重建 PE32+ 应用和离线 NSIS 安装包。
- 工具不污染服务器系统环境，也不会被打包进仓库。
- 最终应用不依赖 `VCRUNTIME140.dll`、`MSVCP*.dll` 或 `api-ms-win-crt*.dll`。

代价与限制：

- 交叉编译仍是实验性发行路径，不能替代 Windows 10/11 原生安装、WebView2 和文件系统验证。
- 项目内交叉工具链体积较大，但仅存在服务器。
- 未签名内部构建可能触发 Windows SmartScreen 提示。

## 验证证据

2026-07-16 服务器构建成功：

- 含诊断导出功能的 `HamHeatmap.exe`：16,039,936 bytes；SHA-256 `e984f112cb0ba2dc3918beca3a04719fd4e398629604d72d802e48074a55dc8a`。
- 对应 `HamHeatmap_0.1.0_x64-setup.exe`：211,209,699 bytes；SHA-256 `adad47b3020d4ac86e34f865b5f3a3993ed75aac7e6b6621d39530e2fc7ba58d`。
- EXE 导入表只包含 Windows 系统 DLL，未出现动态 MSVC/UCRT 运行库。

完整命令、产物路径和边界见 `docs/11-windows-cross-build.md`。
