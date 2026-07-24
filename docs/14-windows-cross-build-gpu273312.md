# GPU-273312 Windows 交叉构建验证记录

- 日期：2026-07-24
- 主机：`gpu-273312`，工作区 `/home/ubuntu/hamheatmap`
- 源码提交：`7c1648a Reproject heatmap overlay for MapLibre`
- 目标：`x86_64-pc-windows-msvc`
- 状态：Linux 交叉构建和静态检查通过；Windows 10/11 实机、代码签名与公开地图合规仍未完成

## 构建与基线

完整 production 构建使用锁文件：

```bash
cd /home/ubuntu/hamheatmap
scripts/tauri-windows-cross.sh -- --locked
```

构建前基线为：

- Rust workspace：`57 passed`、`0 failed`、`3 ignored`；忽略项是显式联网的 GLO-90 测试。
- 前端：4 个测试文件、13 项测试全部通过；TypeScript 和 Vite production build 通过。
- release 优化构建连续生成两张地图覆盖层共约 `0.67 s`，约 `0.335 s/张`。
- MSVC 链接期间出现 `LNK4099` 缺少第三方 PDB 的警告；它不影响链接结果、PE 导入表或运行时代码，因此本次作为非阻断警告记录。

## 项目内构建环境

本次只使用服务器项目目录下的工具和缓存：Rust 1.97.0、Node.js 24.18.0、cargo-xwin 0.23.0、LLVM 20.1.8、NSIS 3.08-2、proot 5.1.0、xwin VS 17、SDK 10.0.26100.0、CRT 14.44.35220。构建环境不依赖 Windows 开发机磁盘。

## 产物

| 产物 | 大小 | SHA-256 |
|---|---:|---|
| 独立 `HamHeatmap.exe` | 16,061,952 bytes | `61d75429a474a7b31b224a057769b1d930855d2c1d461245f387771f4e570f8d` |
| NSIS 离线安装包 | 211,439,966 bytes | `5fdb9724bad3d7712173093b1ff3a61a93d3e9f82a89f2c593745450c11ea066` |
| WebView2 离线安装器 | 203,862,736 bytes | `4617c48d275bd99ead4b941ce250f751af17415f83a902fae313244053262975` |
| `nsis_tauri_utils.dll` | 34,304 bytes | `5ba143b5db4a87d32d6e7802e033330aae56cbceabe0d1e3ba41948385ad4709` |
| NSIS 内嵌 `HamHeatmap.exe` | 16,061,952 bytes | `0393f6833342f8cbae4c86011f8e7934a6f24f1a53ac98d952d67d1c4ba2e57c` |

Tauri 在复制应用进 NSIS 时写入 bundle-type marker，所以独立 EXE 与内嵌 EXE 的哈希不同。本次二者大小相同，只有偏移 `11405251..11405253` 的连续三个 marker 字节不同。

## PE 与导入表

独立应用为 `COFF-x86-64`、`IMAGE_FILE_MACHINE_AMD64` 和 Windows GUI subsystem，并启用 `DYNAMIC_BASE`、`HIGH_ENTROPY_VA`、`NX_COMPAT`。NSIS 3 安装器使用预期的 32 位启动 stub，实际应用载荷仍为 x64；stub 启用 `DYNAMIC_BASE` 和 `NX_COMPAT`。

应用导入 15 个 Windows 系统 DLL：

```text
user32.dll
comctl32.dll
kernel32.dll
shell32.dll
ntdll.dll
api-ms-win-core-synch-l1-2-0.dll
ws2_32.dll
bcryptprimitives.dll
ole32.dll
gdi32.dll
oleaut32.dll
dwmapi.dll
shlwapi.dll
advapi32.dll
bcrypt.dll
```

导入表没有 `VCRUNTIME`、`MSVCP`、`UCRTBASE` 或 `api-ms-win-crt`，静态 CRT 门槛通过。应用与 NSIS 的 PE certificate table 均为空，符合当前内部 Alpha 未签名策略。WebView2 安装器的 `CertificateTableSize=0x2ED0`，证明包含 Authenticode blob；Linux 检查没有宣称已验证证书链信任。

## 安装包内容

项目内 p7zip 解包后精确得到 8 个文件：5 个预期 NSIS 插件/界面资源、WebView2 离线安装器、`HamHeatmap.exe` 和 `THIRD_PARTY_LICENSES.md`。没有源码、`.tools`、`node_modules`、DEM、地图数据或构建缓存。内嵌 WebView2 与 Tauri 缓存原文件 SHA-256 完全相同。

验证入口：

```bash
scripts/verify-windows-artifacts.sh
```

脚本使用 `llvm-readobj` 和项目内 p7zip，严格限制临时目录在 `app/src-tauri/target/verify/`，退出时安全清理。它不替代 Windows Authenticode 信任链、安装和运行测试。

## 尚未关闭的门槛

- 在 Windows 10 和 Windows 11 上验证独立 EXE；独立 EXE 要求系统已有 WebView2 Runtime。
- 完全断网安装、卸载和 WebView2 首次部署。
- 单实例、MapLibre/WebGL、真实覆盖层位置、编码阶段取消与 IPC 内存实测。
- 下载/续传、2.5 GB 压力、中文和长路径 PNG/PDF 导出。
- HamHeatmap EXE/NSIS 代码签名与 SmartScreen 验证。
- 中国大陆公开发行底图授权、审图号、署名和导出授权。
