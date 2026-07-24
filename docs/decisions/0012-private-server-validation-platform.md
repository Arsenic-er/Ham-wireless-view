# ADR-0012：通过 SSH 隧道提供私有服务器验证平台

- 日期：2026-07-24
- 状态：已采纳，仅限内部开发验证

## 背景

HamHeatmap 的正式产品仍是 Windows 10/11 离线优先桌面应用，坐标、参数和结果默认只在用户电脑处理。开发阶段的普通 Vite 浏览器预览故意不连接 Rust 后端，因此只能检查界面，不能验证真实 DEM/WBM 缓存、ITM 计算和地图覆盖层。用户又要求源码、依赖、数据和验证资源全部留在 `gpu-273312` 的 `/home/ubuntu/hamheatmap`，暂不依赖本地 EXE。

直接公开一个 Web 服务会扩大攻击面、改变隐私语义，并可能把无合规底图的内部画布误认为可公开平台。服务器现有 `9090` 还属于 Cockpit，不能复用。项目需要一条能运行真实共享核心、但不转变产品形态的最小验证路径。

## 决策

1. 新增独立 workspace crate `hamheatmap-validation-server`。它只负责静态提供 validation 前端、把同源 JSON API 适配到既有 `hamheatmap-app-service`，不复制传播、缓存或配额逻辑。
2. 前端固定为三态：
   - 检测到 Tauri 时使用 Tauri IPC，允许数据准备、缓存、计算和 Windows 原生导出；
   - `VITE_VALIDATION_SERVER=1` 时使用同源 HTTP，允许数据准备、缓存和计算，但禁止导出；
   - 其他浏览器构建保持 interface-only preview，禁止真实写入和计算。
   Tauri 检测优先于构建标志，避免桌面包意外进入 HTTP 模式。
3. validation API 只暴露 `bootstrap`、点检查、下载估算与执行、缓存概览与删除、传播计算以及下载/计算取消。请求体沿用 Tauri 形状的 camelCase 包装；不增加任意 URL、任意文件路径或 `/api/export-result`。
4. 验证平台由 `scripts/validation-platform.sh` 统一 build/start/status/health/stop。构建必须使用项目内 Node、`VITE_VALIDATION_SERVER=1` 和 release Rust binary；PID、日志、数据和构建元数据全部保存在 `.runtime/validation-platform/`。
5. 管理脚本把监听地址固定为 `127.0.0.1:1421`。Windows 浏览器只能通过 SSH 本地端口转发访问。不得绑定 `0.0.0.0`、开放云安全组端口、占用 `9090` 或增加公网反向代理。
6. 不为该平台使用 Docker、systemd、Caddy、Nginx 或系统级数据目录。`nohup + setsid` 只保证 SSH 断开后继续运行，不保证服务器重启后自动启动。
7. 管理脚本停止进程前必须同时验证 PID 文件所有者、可执行文件绝对路径以及固定 bind/dist/data 参数；PID 不明确或已被复用时拒绝发送信号。日志当前文件以 10 MB 为轮转阈值并保留 3 份，启动日志约 1 MB 轮转。
8. HTTP 桥接保持 fail-closed：默认请求体上限 1 MiB、请求头上限 16 KiB、静态文件上限 32 MiB；拒绝 Transfer-Encoding、路径穿越、未知静态扩展、非 JSON API 媒体类型和未知 JSON 字段。响应带同源 CSP、`nosniff`、`no-referrer` 和显式缓存策略。
9. 所有共享服务操作由单任务门闩串行化；并行操作返回冲突。下载与计算取消只作用于相同类型的当前任务。当前 HTTP 模式没有 Tauri 进度事件流，长操作以请求完成为结果，取消使用独立同源请求。
10. validation 页面必须持续显示“内部服务器验证”提示，明确坐标、无线电参数和计算请求会发送到用户控制的服务器。只使用测试坐标；这是一项内部测试隐私例外，不改变正式 Windows 产品的本地处理承诺。
11. 服务器模式不提供 PNG/PDF 文件导出。导出仍依赖 Tauri 的 Windows 原生保存对话框、路径校验和原子写入，不能通过浏览器服务模拟。

## 结果

优点：

- 浏览器能够使用与桌面端相同的 Rust `AppService` 验证真实缓存和传播结果，不再用模拟热力图。
- SSH 公钥同时承担身份控制和传输加密，不需要为内部 Alpha 建立公网 TLS、账号系统或新的防火墙规则。
- 验证资源和运行状态留在规范项目目录，Windows 本机不接收源码、数据或工具链。
- 三态能力表把 interface preview、服务器真实核心验证和 Windows 桌面行为明确分开。

限制：

- 请求和结果在用户浏览器与 JAIST 服务器之间传输，不能声称“只留 Windows 本机”。
- 没有服务器端导出、Tauri 事件进度、WebView2、Windows 文件系统、安装包或原生保存对话框覆盖。
- `nohup` 不是重启管理器，服务器重启后需要重新启动。
- 私有平台仍使用内部无行政边界画布，不关闭合规底图、审图号或公开地图发布门槛。

## 被拒绝的方案

- 把 Vite preview 绑定公网：没有真实 Rust 后端，也缺少认证和隐私边界。
- 把 validation server 绑定公网或复用 Cockpit `9090`：扩大攻击面并干扰主机管理服务。
- 用 Docker 或系统服务托管：运行资源会离开规范项目目录，并引入当前验证不需要的系统状态。
- 在普通浏览器 preview 返回模拟结果：会让界面截图冒充真实传播建模。
- 把验证平台发展成公开 Web 产品：超出 Windows 离线 MVP 范围，并引入账号、云端隐私和地图互联网服务合规问题。

## 验收与记录

代码、进程、真实数据和浏览器检查点记录在 `../15-private-validation-platform.md`。只有该文档明确记录的项目可以宣称通过；私有验证结果不能外推为 Windows 10/11 实机或公开地图合规验收。
