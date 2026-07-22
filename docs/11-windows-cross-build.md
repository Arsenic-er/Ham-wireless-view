# Windows 服务器交叉构建记录

- 日期：2026-07-16
- 主机：JAIST `/home/ubuntu/hamheatmap`
- 目标：`x86_64-pc-windows-msvc`
- 状态：服务器构建通过；Windows 实机冒烟仍需单独记录

## 目标与边界

所有源码、依赖、工具链、缓存和安装包均留在服务器。Windows 开发机只接收原始 `HamHeatmap.exe`，不复制源码、`node_modules`、Rust target、SDK 或 NSIS 安装包。

该构建用于内部 Alpha 验证。它尚未完成代码签名、Windows 10/11 双系统安装测试、2.5 GB 配额压力测试和中国大陆公开发行地图合规，因此不能公开分发。

## 一条命令构建

项目内工具准备完成后运行：

```bash
cd /home/ubuntu/hamheatmap
scripts/tauri-windows-cross.sh
```

脚本只使用 `.tools/` 中的 Node、Rust、cargo-xwin、xwin SDK、LLVM、NSIS 和 proot，并执行 Tauri production build。它不会写入 Windows 开发机，也不要求服务器安装系统级 NSIS。

## 产物

原始应用：

```text
app/src-tauri/target/x86_64-pc-windows-msvc/release/HamHeatmap.exe
```

内嵌 WebView2 的离线 NSIS 安装包：

```text
app/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/HamHeatmap_0.1.0_x64-setup.exe
```

2026-07-16 已验证产物：

| 产物 | 大小 | SHA-256 |
|---|---:|---|
| `HamHeatmap.exe` | 16,039,936 bytes | `e984f112cb0ba2dc3918beca3a04719fd4e398629604d72d802e48074a55dc8a` |
| `HamHeatmap_0.1.0_x64-setup.exe` | 211,209,699 bytes | `adad47b3020d4ac86e34f865b5f3a3993ed75aac7e6b6621d39530e2fc7ba58d` |

## 运行库策略

`cargo-xwin` 提供 MSVC/Windows SDK 搜索路径。项目 runner 额外启用 Rust `+crt-static`，并显式传入 xwin 的 `libucrt.lib`，解决 Tauri 同时构建 `cdylib` 时的 UCRT 默认库顺序冲突。

最终 EXE 导入表没有 `VCRUNTIME140.dll`、`MSVCP*.dll` 或 `api-ms-win-crt*.dll`。这减少了单文件启动对 Visual C++ Redistributable 的依赖，但 Tauri 界面仍需要 Windows WebView2 Runtime。离线安装包已内嵌 WebView2；开发机若已有 WebView2，可直接运行原始 EXE。

## 验证命令

```bash
sha256sum \
  app/src-tauri/target/x86_64-pc-windows-msvc/release/HamHeatmap.exe \
  app/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/HamHeatmap_0.1.0_x64-setup.exe

.tools/llvm-20.1.8/bin/llvm-objdump -p \
  app/src-tauri/target/x86_64-pc-windows-msvc/release/HamHeatmap.exe
```

代码回归必须另外运行：

```bash
scripts/cargo-project.sh test --workspace --all-targets --locked
scripts/cargo-project.sh clippy --workspace --all-targets --locked -- -D warnings
scripts/node-project.sh --prefix app run check
scripts/node-project.sh --prefix app test -- --run
scripts/node-project.sh --prefix app run build
bash -n scripts/cargo-xwin-static.sh scripts/makensis-project.sh scripts/tauri-windows-cross.sh
```

## Windows 实机门槛

- 直接启动原始 EXE，确认主题、空白合规占位地图、参数面板和缓存状态正常。
- 第二次启动只聚焦已有窗口，不产生第二个主实例。
- 在 Windows 10 与 11 分别验证离线安装、卸载和 WebView2 安装路径。
- 验证取消、Range 续传、缓存删除失败回滚、接近 2.5 GB 上限和离线计算。
- 公开发布前完成代码签名及中国大陆底图授权、审图号和导出授权。
