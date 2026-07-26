# HamHeatmap 私有服务器验证平台

- 初始日期：2026-07-24
- 恢复切片更新：2026-07-27
- 主机：`gpu-273312`（`ubuntu@150.65.181.202`）
- 工作区：`/home/ubuntu/hamheatmap`
- 对应决策：`decisions/0012-private-server-validation-platform.md`
- 状态：私有平台、真实成都计算、浏览器视觉、加固版 stop/build/start/readiness 和真实 HTTP 取消恢复均已验证；仍不替代整机重启、Windows/Tauri 与地图合规验收

## 1. 目标与边界

该平台让项目所有者在 Windows 浏览器中验证服务器上的真实 HamHeatmap 共享核心，同时不把源码、Node/Rust 工具链、DEM/WBM 或构建缓存复制到本机。

它只用于内部 Alpha：

- 不是公开网站，不开放服务器公网端口；
- 不替代 Windows Tauri/WebView2、安装包和文件系统验收；
- 不提供 PNG/PDF 文件导出；
- 不证明合规底图、审图号或公开地图导出已经完成；
- 不改变正式桌面版坐标与结果本地处理的产品目标。

## 2. 架构

```text
Windows browser http://127.0.0.1:1421
            │
            │ SSH local forwarding（公钥认证与加密）
            ▼
gpu-273312 127.0.0.1:1421
  hamheatmap-validation-server
       ├─ app/dist validation build
       ├─ same-origin JSON API
       └─ hamheatmap-app-service
            ├─ CacheStore / 2.5 GB hard cap
            ├─ GLO-90 DEM + WBM
            ├─ Coverage / NTIA ITM
            └─ map/report PNG results in memory
```

HTTP 层只做协议适配。频段、单位换算、坐标校验、固定数据源、缓存完整性、配额、DEM/WBM 读取、ITM 和覆盖层仍由共享 Rust 服务执行。

## 3. 三态前端

| 模式 | 选择条件 | 数据准备/缓存 | 传播计算 | PNG/PDF 文件导出 |
|---|---|---:|---:|---:|
| `tauri` | `window.__TAURI_INTERNALS__` 存在 | 是 | 是 | 是 |
| `validation-server` | 非 Tauri 且 `VITE_VALIDATION_SERVER=1` | 是 | 是 | 否 |
| `preview` | 其他普通浏览器构建 | 否 | 否 | 否 |

Tauri 始终优先于 Vite 标志。validation 模式显示单独横幅，说明坐标、无线电参数和计算请求会发送到本服务器；计算和数据准备按钮可按真实状态启用，导出按钮始终禁用。普通 preview 继续只显示确认流程和界面状态，不执行写入或返回模拟传播结果。

## 4. HTTP 契约

| 方法 | 路径 | 作用 |
|---|---|---|
| GET | `/healthz` | 进程健康与 schema 版本 |
| GET | `/api/bootstrap` | 模型、网格和缓存配额 |
| GET | `/api/cache-overview` | 实际缓存用量与区域列表 |
| POST | `/api/inspect-point` | 区域计划、ready 状态和中心高程 |
| POST | `/api/estimate-download` | 固定来源下载量与配额预检 |
| POST | `/api/download-region` | DEM/WBM 下载、生成、校验与 ready |
| POST | `/api/delete-cache-region` | 引用安全的区域删除 |
| POST | `/api/calculate` | 真实 DEM/WBM、ITM 与双 PNG 结果 |
| POST | `/api/cancel-download` | 取消当前下载类任务 |
| POST | `/api/cancel-calculation` | 取消当前计算类任务 |

没有导出端点。POST JSON 使用与 Tauri 调用相同的包装字段，并拒绝未知字段。服务一次只允许一个共享操作；冲突返回 HTTP 409。当前 validation 适配器没有 Tauri 事件流，因而不记录逐阶段实时进度；取消通过另一条 HTTP 请求设置共享取消令牌。

`/healthz` 是不打开缓存的轻量进程存活检查，只返回 HTTP 状态和协议 schema。需要确认 `CacheStore` 能获得锁、完成重启整理并满足配额时，调用方必须成功执行 `/api/bootstrap`；因此管理脚本的 `health` 结果不能替代数据就绪判断。

## 5. 网络、隐私与安全

规范进程固定监听 `127.0.0.1:1421`。服务器公网 IP 上不开放该端口，访问控制由已有 SSH 公钥承担。

请求边界：

- CLI 只接受 loopback SocketAddr，非回环绑定在启动前即被拒绝；
- 每个 HTTP 请求必须具有唯一且与实际监听地址/端口匹配的 `Host`，同时允许同端口 `localhost`，以阻断 DNS rebinding；
- 请求体默认不超过 1,048,576 bytes；
- 请求头不超过 16,384 bytes；
- 单个静态文件不超过 32 MiB；
- 不支持 `Transfer-Encoding`；
- JSON API 要求 `application/json`；取消端点同样强制该媒体类型，使跨站简单 POST 不能触发任务取消；
- 静态路径拒绝百分号编码、反斜杠、父目录、异常字符和未知扩展；
- canonical path 必须仍位于 `app/dist`；
- `app/dist` 与运行数据目录不能重叠；
- 响应包含同源 CSP、`X-Content-Type-Options: nosniff` 和 `Referrer-Policy: no-referrer`。

validation 模式是一项明确的内部隐私例外：浏览器中的测试坐标、参数和计算请求会离开 Windows 本机并进入用户控制的服务器。不要使用敏感真实位置。服务没有遥测、账号、第三方计算 API或服务器文件导出；数据下载仍只访问既有固定 Copernicus HTTPS 来源。

## 6. 构建、启动与访问

服务器：

```bash
cd /home/ubuntu/hamheatmap
scripts/validation-platform.sh build
scripts/validation-platform.sh start
scripts/validation-platform.sh status
scripts/validation-platform.sh health
scripts/validation-platform.sh self-test
```

Windows PowerShell：

```powershell
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 1421:127.0.0.1:1421 ubuntu@150.65.181.202
```

保持 PowerShell 窗口运行，在浏览器打开：

```text
http://127.0.0.1:1421
```

停止：

```bash
scripts/validation-platform.sh stop
```

`start` 使用 `nohup + setsid`，SSH 断开后进程继续运行；服务器重启后不会自动恢复。不要用通配进程名或手工 PID 执行 `kill`。停止入口除核对用户、项目 release binary 和完整 argv 外，还核对进程 start time；后台 runner 以 PID、start time、boot ID 的 claim 证明生命周期所有权，避免只凭可复用 PID 发送信号。

`self-test` 在独立的项目内临时状态目录验证陈旧控制锁、runner claim、符号链接路径逃逸和精确 argv，不启动或停止持久服务。加固版真实 `stop → build → start → status/health → bootstrap/cache-overview` 已另行通过，证据见 9.4 节。

## 7. 项目内运行资源

```text
.runtime/validation-platform/
├─ build.txt
├─ server-help.txt
├─ data/
├─ logs/
│  ├─ launcher.log
│  ├─ server.log
│  └─ server.log.1..3
└─ state/
   ├─ control.lock/
   │  └─ owner
   ├─ runner.claim/
   │  └─ owner
   ├─ runner.pid
   └─ server.pid
```

所有目录权限由脚本收紧。`server.log` 达到约 10 MB 时轮转并保留 3 份，`launcher.log` 达到约 1 MB 时轮转。该目录被 Git 忽略；平台不使用 Docker、systemd 或系统级数据目录。

## 8. 已确认的代码测试

### 8.1 Rust validation server

`hamheatmap-validation-server` 的 11 项测试全部通过，覆盖：

1. CLI 默认值、参数覆盖、IPv4/IPv6 回环监听，以及非回环地址拒绝；
2. 静态路径与 MIME fail-closed；
3. 请求体上限、唯一 `Host` 与 JSON 元数据；
4. 单任务门闩、按任务类型取消，以及取消/成功结果交付的线性化；
5. 静态目录与数据目录重叠拒绝；
6. 静态/API 路由、未知文件和导出端点拒绝；
7. camelCase 包装契约、未知字段和错误媒体类型拒绝；
8. HTTP `Host` 必须匹配实际回环监听地址/端口，拒绝缺失、重复、错误端口和外部主机名；
9. 取消端点强制 `application/json`，拒绝无媒体类型或表单媒体类型的简单跨站 POST；
10. 安全响应头、HEAD 无响应体语义，以及 Tauri CSP 允许 `data:`/`blob:` 覆盖层但不放宽外部网络来源。
11. 已接受取消时丢弃随后到达的成功值，并在 lease 释放后允许下一项操作。

### 8.2 前端

前端 26 项测试通过。其中 validation 专项测试确认：

- preview、validation-server、Tauri 三态能力矩阵；
- Tauri 对 validation 构建标志的优先级；
- 同源 GET/POST、Tauri 形状请求体、无响应体取消 POST 的 JSON 媒体类型和错误传播；
- validation 横幅明确披露远程处理；
- validation 模式允许真实计算但禁止文件导出；
- preview 仍禁止确认下载和真实计算；
- 清空只移除热力图，保留发射点和数据就绪状态，并允许同一点立即重新计算；
- MapLibre 覆盖层将后端 PNG data URL 同步转换为 Blob URL，校验 PNG 签名，并在替换、清空和组件卸载时复用或释放对象 URL。
- 取消重算会清除旧 heatmap、禁用导出，并在被取消的 promise 结束后恢复干净重试。

### 8.3 管理脚本

2026-07-27 的加固版先通过 `bash -n` 与 `self-test`，随后以 revision `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d` 完成真实 stop/build/start/readiness。旧 PID `214692` 被严格停止，新 PID `1114524` 通过身份、重复 start 和 runner 排他检查；完整运行证据见 9.4 节。

## 9. 真实成都验证

中心坐标：`30.5°N, 103.5°E`。

### 9.1 数据准备

根 Agent 已确认真实 validation-server 数据准备完成：

| 指标 | 结果 |
|---|---:|
| DEM | 25 ready |
| WBM | 25 ready |
| 本次下载 | 132,997,688 bytes |
| 首次数据根总量 | 133,063,224 bytes |
| 中心高程 | 526.3443 m |

这些结果证明首次服务器验证数据已准备并能由共享缓存服务读取；随后同一数据已完成传播计算和浏览器视觉验收。浏览器缓存对话框以十进制显示为 `133.1 MB / 2.50 GB`。恢复切片因索引/运行元数据变化测得 `133,071,416 bytes`，详见 9.4 节。

### 9.2 真实传播计算

`band` JSON 契约已修复并增加回归测试，随后通过 `/api/calculate` 完成一次真实计算。请求参数如下：

| 参数 | 值 |
|---|---:|
| 中心 | `(30.5, 103.5)` |
| 频段 / 具体频率 | 144 MHz 频段 / `145.00 MHz` |
| 发射功率 | `25 W` |
| 发射天线 | `6 dBi / 20 m` |
| 接收天线 | `-3 dBi / 1.5 m` |
| 极化 | 垂直 |
| 传播路径 | 共享 Coverage / NTIA ITM |

接口同时返回两张 `401×401` PNG：原始本地方位投影 heatmap，以及供 MapLibre 显示的 EPSG:3857 map overlay。

| 统计字段 | 结果 |
|---|---:|
| valid | 125,628 |
| below | 77,496 |
| warning | 99,214 |
| min | -250.14908 dBm |
| max | -41.75736 dBm |
| mean | -146.670031115 dBm |
| water | 109,817 |
| meanWater | 0.0128492517 |
| propagation | 2.696916 s |
| total | 8.311013 s |
| HTTP response | 407,060 bytes |

完整 `/api/calculate` HTTP 响应体 SHA-256 为 `4d219a120ef38ad9eb3c2cf5bd0b939ffe247bee123ca1768b124b10c67468f6`。随后对相同输入重复计算；除运行耗时字段外，输出保持确定：

- 原始 heatmap 字段哈希两次均为 `1e64b5c0c95ba12c5ed52589304df66343b9d2c5f3d48288d3b7250c92f610a7`；
- Web Mercator overlay 字段哈希两次均为 `e41b715614045b09a863956e46a0111e4e3761ade29a9eeff069a266ccc5b542`；
- 非耗时统计字段哈希两次均为 `c7d45d6e69db14f72e80017991bf0a37c6cfc2f59ab9694d178e550bd98a0ea3`。

### 9.3 浏览器视觉结果

通过 SSH 本地转发在 Windows 浏览器打开 `http://127.0.0.1:1421`，服务端仍只监听回环地址。文本验收记录如下：

- 验证窗口为 `1080×700`，浅色和深色主题均通过；
- 成都真实 heatmap 可见，并随地图缩放和平移保持正确联动；
- 缓存对话框显示 `133.1 MB / 2.50 GB`；
- validation 模式导出保持禁用；
- 清空流程已验证：只清除 heatmap，保留发射点和就绪数据，同一点可以立即重新计算；
- 修复前首先出现 CSP 对 PNG data URL 的拒绝；CSP 收紧范围内加入 `data:`/`blob:` 后，MapLibre ImageSource 仍出现 data URL AJAXError，最终改为带 PNG 签名校验和生命周期管理的 Blob URL，并由 3 项前端测试覆盖复用、替换、清空与释放；
- 刷新页面后重新执行真实计算，界面报告 `8.3 s / 125628 px`，浏览器控制台 error 和 warning 均为空；
- 本轮按要求只保留文字证据，没有生成或提交截图。

### 9.4 加固版应用进程恢复与真实取消

本次部署以提交和 build metadata revision `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d` 为基线。

| 检查 | 结果 |
|---|---|
| stop/build/start | 旧 PID `214692` 退出；新 PID `1114524` |
| 管理与 liveness | `status`、`health` 通过 |
| readiness | `/api/bootstrap`、`/api/cache-overview` 通过 |
| 缓存不变性 | 重启前后均为 `133,071,416 bytes`、`partial=0` |
| 区域完整性 | 两个区域各 `50/50 ready` |
| 重复 start | 仍为 PID `1114524` |
| runner 排他 | 直接 `__run` 返回 `another validation runner is active` |

`scripts/validation-recovery-smoke.sh` 加固后再次连续对真实受管回环服务运行通过。运行前，无效 `/api/inspect-point` 返回 HTTP 422，确认 gate 为空；后台 calculate curl 必须同时是本 shell 的 job、以本 shell 为 PPID，并匹配记录的 start time 和 curl executable，脚本才把 gate 内操作视为自己所有。该所有权判断只适用于受控单客户端，不能替代多客户端 operation ID。

每次取消端点都返回 `cancelled=true`；被取消的 calculate 返回 HTTP 422 且没有两个 PNG 字段。随后相同请求返回 HTTP 200，`schemaVersion=2`，`imageWidth/imageHeight` 与 `mapOverlayWidth/mapOverlayHeight` 均为 `401×401`。两个 data URL 字段各只出现一次、payload 非空、Base64 可解码，解码结果前 24 bytes 均验证为 PNG signature、IHDR 与 `401×401`。

结束探针返回业务校验 HTTP 422 而非冲突 409，最终 `/healthz` 为 200。脚本再次输出 `validation recovery smoke passed: cancel=true cancelled_http=422 recovery_http=200`，operation gate 与健康状态恢复，且没有 `validation-recovery-smoke.*` 临时目录残留。

这是应用进程 stop/start 证据，不是 GPU 主机整机重启测试。SSH 隧道访问此前已独立验证；本烟雾脚本直接命中服务器回环地址，以减少浏览器和隧道时序噪声。

## 10. 尚未关闭

- 多标签页/多客户端并发取消仍没有 operation ID 绑定；当前保障范围是单服务门闩与官方单窗口 UI 正常路径；
- HTTP 模式的渐进进度显示；
- Windows 10/11 WebView2、原生保存、安装/卸载和真实文件系统；
- 十进制 2.5 GB 实体边界压力、磁盘不足、弱网中断和进程强制崩溃注入；
- GPU 主机整机重启后的手动恢复流程；
- 合规中国大陆底图、审图号、署名、离线/导出授权；
- 传播结果的外场测量校准。

本次私有浏览器验收只关闭服务器回环验证路径中的真实计算与显示问题，不能据此关闭 Windows/Tauri 实机或中国大陆地图合规门槛。
