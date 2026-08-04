# Windows 服务器交叉构建记录

- 日期：2026-08-01
- 主机：JAIST `/home/ubuntu/hamheatmap`
- 目标：`x86_64-pc-windows-msvc`
- 源提交：`59ae5b188f48db52618846246de27eb0cfe6bbba`
- 状态：Tauri 交叉构建第二次执行退出 0；Windows 实机与真实中国大陆网络仍需单独记录
- 发布边界：正式 NSIS 内嵌离线 WebView2、按当前用户安装，产物未签名

## 目标与边界

所有源码、依赖、工具链和构建缓存均留在服务器。Windows 电脑只接收最终 standalone EXE 或 NSIS 安装包，不复制源码、`node_modules`、Rust target 或 SDK。

该构建用于内部 Alpha 验证。它尚未完成代码签名、Windows 10/11 双系统安装测试、默认 CARTO/EOX 公共底图、可选个人天地图 `tk` 和中国大陆真实 ISP 可达性实测，因此不能据此宣称桌面或中国大陆网络已经验收。

## 一条命令构建

项目内工具准备完成后运行：

```bash
cd /home/ubuntu/hamheatmap
scripts/tauri-windows-cross.sh
```

脚本只使用 `.tools/` 中的 Node、Rust、cargo-xwin、xwin SDK、LLVM、NSIS 和 proot，并执行 Tauri production build。2026-08-01 基于上述源提交的第二次完整执行退出 0；它不会写入 Windows 开发机，也不要求服务器安装系统级 NSIS。

## 产物

原始应用：

```text
app/src-tauri/target/x86_64-pc-windows-msvc/release/HamHeatmap.exe
```

内嵌 WebView2 的离线 NSIS 安装包：

```text
app/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/HamHeatmap_0.1.0_x64-setup.exe
```

2026-08-01 已验证产物：

| 产物 | 大小 | SHA-256 |
|---|---:|---|
| `HamHeatmap.exe` | 16,104,960 bytes | `1146de0f7bbd0e409c676c3f75d5c7f6741700252418ebfcf15212c343bda7ed` |
| `HamHeatmap_0.1.0_x64-setup.exe` | 217,258,090 bytes | `46434fc5179ae8d5dd65acdb1c251907292aa689a10755aa6ac08a932d2c2000` |

## Windows 在线地图能力

当前源码构建出的 Windows/Tauri 应用默认使用 CARTO Voyager `base+labels` 与 EOxCloudless Sentinel-2 卫星影像，不要求天地图 `tk`。前端只接受固定 `http://basemap.localhost/...` 映射模板；Tauri/Wry 在 WebView2 中把它拦截到 Windows Rust 后端，后端代理固定 HTTPS 上游，并对路径、坐标、大小、MIME 与图片签名执行严格检查。

个人天地图 `tk` 是可选覆盖：设置后由当前用户 DPAPI 加密保存，固定 `http://tianditu.localhost/...` 普通/卫星映射模板优先；清除后继续使用 CARTO/EOX。所有在线瓦片响应均为 `no-store`，不会持久缓存、不计入 2.5 GB DEM/WBM 配额，也不进入诊断 PNG/PDF。

Windows WebView2 不会可靠拦截 MapLibre 子资源中的 `scheme://localhost/...` 地址，因此发行元数据必须使用 Tauri 的 `http://<scheme>.localhost/...` 映射形式。在线地图错误会保留安全诊断（固定图源名、错误分类、可选状态码、时间和重试次数）；点击重试不会立即清除，只有相同失败图源报告真实 `tile.state=loaded` 后才关闭。原始错误正文、请求 URL 和令牌从不进入界面或复制文本。

本节上方与 Alpha 2 小节记录的 2026-08-01 哈希是历史产物。2026-08-04 已基于提交 `ccf3155d5b55ce755e76db4a6ca23c241223f6e8` 重新执行完整交叉构建，将默认公共底图行为纳入新的 EXE/NSIS。

| 2026-08-04 公开底图产物 | 大小 | SHA-256 |
|---|---:|---|
| `HamHeatmap.exe` | 16,296,960 bytes | `7535b5cf45501105f3d441e3a8a4bddaf6350bfd78ee7be1e56f4ae66b2e0dd7` |
| `HamHeatmap_0.1.0_x64-setup.exe` | 217,335,060 bytes | `c2567a95a945e260425646ad88dfa5e4ae44fca90e86eca418121a2dd4b54d93` |

验证结果：前端 `17 files / 155 tests`、Rust workspace `133 passed / 5 ignored`、TypeScript、production build、rustfmt、严格 Clippy、Windows xwin check/Clippy/test `--no-run` 与 `verify-windows-artifacts.sh` 全部通过；CARTO base/labels 与 EOX 卫星三个固定上游样本均返回匹配 MIME 和图片签名。Windows 实机和中国大陆真实 ISP 仍待验收。

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
scripts/cargo-xwin-static.sh check --manifest-path app/src-tauri/Cargo.toml --all-targets --locked --target x86_64-pc-windows-msvc
scripts/cargo-xwin-static.sh clippy --manifest-path app/src-tauri/Cargo.toml --all-targets --locked --target x86_64-pc-windows-msvc -- -D warnings
scripts/cargo-xwin-static.sh test --manifest-path app/src-tauri/Cargo.toml --lib --no-run --locked --target x86_64-pc-windows-msvc
bash -n scripts/cargo-xwin-static.sh scripts/makensis-project.sh scripts/tauri-windows-cross.sh
```

## 早期 Alpha 自动化与交叉构建基线

先前提交 `59ae5b188f48db52618846246de27eb0cfe6bbba` 的构建证据为：前端 `11 files / 79 tests`；Rust workspace `113 passed / 5 ignored`；TypeScript、production build、rustfmt、workspace Clippy 和 validation 管理 self-test 通过；Windows xwin all-target check、严格 Clippy 与测试程序 `--no-run` 通过。交叉编译成功证明目标代码可编译和打包，不等于测试程序已在 Windows 上执行。

正式 NSIS 包含离线 WebView2，安装范围为当前用户；standalone EXE 和 NSIS 均未签名。

## Alpha 2 连接自检重建

2026-08-01 基于提交 `9b0fb795b4b24feb1f79ce93609b8b0f58de8d41` 重新执行完整交叉构建并发布 `v0.1.0-alpha.2`。该版本增加用户显式触发的天地图连接自检：保存配置与可达性状态分离，结果只返回固定脱敏状态，探测不写瓦片缓存。

回归证据：

- TypeScript、13 个前端测试文件/111 项测试和 production build 通过。
- Rust workspace `114 passed / 5 ignored`、rustfmt 与严格 Clippy 通过。
- Windows xwin all-target check、严格 Clippy 和测试程序 `--no-run` 通过；只有既有缺失 MSVC PDB 的 LNK4099 非阻断警告。
- `scripts/tauri-windows-cross.sh -- --locked` 完整执行退出 0，耗时 108.1 秒；`scripts/verify-windows-artifacts.sh` 通过。

| Alpha 2 产物 | 大小 | SHA-256 |
|---|---:|---|
| `HamHeatmap.exe` | 16,174,080 bytes | `a1968a48bca419d58680adca31759284f7971d36c590503451212114c3808247` |
| `HamHeatmap_0.1.0_x64-setup.exe` | 217,265,419 bytes | `4df826b0eb96cd5a69f3c6a3a6d2b9d248c067fe60be34bb9bcd2e7bbe0fbc0e` |

安装包内容审计仅见插件、离线 WebView2 安装器、HamHeatmap.exe 与第三方许可证；不含 PMTiles、DEM/WBM、密钥、源码或缓存。两个 EXE 仍未签名。有效个人 `tk`、中国大陆真实 ISP、Windows 10/11 DPAPI/WebView2 与 SmartScreen 仍需实机验收。

## Windows 实机门槛

- [ ] 在 Windows 10 和 Windows 11 分别启动 standalone EXE，验证主题、参数、缓存、计算、导出和第二实例聚焦。
- [ ] 在 Windows 10 和 Windows 11 分别验证当前用户 NSIS 的离线 WebView2 安装、启动和卸载，以及 SmartScreen/未签名提示。
- [ ] 不配置 `tk`，验证 CARTO Voyager 地图/地名、EOxCloudless 卫星切换、动态比例尺、断网回退与重启。
- [ ] 使用用户自己的有效天地图 `tk` 验证可选覆盖、`vec+cva`、`img+cia`、中文注记、替换/清除和重启后 DPAPI 恢复。
- [ ] 验证断网、弱网、无效 `tk`、配额错误、取消、Range 续传、缓存删除失败回滚、接近 2.5 GB 上限和离线传播计算。
- [ ] 从至少一个中国大陆家庭或移动网络验证瓦片可达；不得从服务器或其他地区的请求结果外推。
- [ ] 在实机检查 DevTools、日志、崩溃信息、bootstrap 和导出文件不含 `tk`，并确认诊断 PNG/PDF 不包含在线底图。
- [ ] 公开发布前完成代码签名以及所需地图服务、应用分发和导出授权审查。
