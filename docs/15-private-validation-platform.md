# HamHeatmap 私有服务器验证平台

- 初始日期：2026-07-24
- 恢复切片更新：2026-07-27
- operation 协议切片更新：2026-07-27
- 渐进覆盖预览更新：2026-07-27
- 在线底图与地图控件更新：2026-07-31；无 token CARTO Voyager/OSM 回退更新：2026-08-02
- 四省 PMTiles 历史底图切片：2026-07-31（既有自动化、Range、SSH 与受管运行证据保留；2026-08-02 起退出当前目标）
- 会话覆盖层与浏览器诊断导出更新：2026-08-01（自动化、推送、受管部署与隧道健康已通过；浏览器交互待用户验收）
- 主机：`gpu-273312`（`ubuntu@150.65.181.202`）
- 工作区：`/home/ubuntu/hamheatmap`
- 对应决策：`decisions/0012-private-server-validation-platform.md`、`decisions/0013-operation-identity-and-polled-progress.md`、`decisions/0016-progressive-coverage-preview-transport.md`、`decisions/0019-session-coverage-layers-and-browser-export.md`
- 状态：历史构建保留为分节证据；现行源码在有 token 时使用同源天地图普通地图，无 token 时使用同源 CARTO Voyager/OSM 普通地图，卫星图为 EOxCloudless，失败回退 WGS84 网格。当前受管服务尚未重建，CARTO live 瓦片与浏览器视觉尚未记录。

四省 PMTiles 已退出当前产品目标，但约 33 MB runtime 资产尚未删除；PMTiles 相关内容只作历史证据。目标改变不代表代码切换、受管部署或磁盘清理已经完成。

## 1. 目标与边界

该平台让项目所有者在 Windows 浏览器中验证服务器上的真实 HamHeatmap 共享核心，同时不把源码、Node/Rust 工具链、DEM/WBM 或构建缓存复制到本机。

它只用于内部 Alpha：

- 不是公开网站，不开放服务器公网端口；
- 不替代 Windows Tauri/WebView2、安装包和文件系统验收；
- 只提供浏览器本地诊断 PNG/PDF 下载，不提供服务器导出路由或服务器文件写入；
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
            ├─ active latest-only preview in memory
            └─ authoritative map/report PNG results in synchronous response
```

HTTP 层只做协议适配。频段、单位换算、坐标校验、固定数据源、缓存完整性、配额、DEM/WBM 读取、ITM 和覆盖层仍由共享 Rust 服务执行。

现行普通地图走互斥的只读在线路径：合法天地图 token 存在时，浏览器请求同源 `/api/basemap/tianditu/{vec|cva}/{z}/{x}/{y}`；token 文件不存在时，bootstrap 明确返回 `carto-voyager`，浏览器请求 `/api/basemap/carto/{base|labels}/{z}/{x}/{y}`。服务器只访问各自固定 HTTPS 上游，token、上游 URL 和文件路径不进入 bootstrap 或浏览器。两条路径均不写底图缓存，也不进入诊断报告。

EOxCloudless 继续作为同源在线卫星路径；普通/卫星都失败时 MapView 使用 WGS84 坐标网格。PMTiles Range 路径、固定归档校验和本地 places 属于待移除实现，完成前不得描述为已经删除。

## 3. 三态前端

| 模式 | 选择条件 | 数据准备/缓存 | 传播计算 | PNG/PDF 文件导出 |
|---|---|---:|---:|---:|
| `tauri` | `window.__TAURI_INTERNALS__` 存在 | 是 | 是 | 是 |
| `validation-server` | 非 Tauri 且 `VITE_VALIDATION_SERVER=1` | 是 | 是 | 浏览器本地诊断下载 |
| `preview` | 其他普通浏览器构建 | 否 | 否 | 否 |

Tauri 始终优先于 Vite 标志。validation 模式显示单独横幅，说明坐标、无线电参数和计算请求会发送到本服务器；计算和数据准备按钮可按真实状态启用。当前最新结果完成且参数未过期时，导出按钮允许浏览器本地生成并下载诊断 PNG/PDF；报告正文和目标路径不发送到服务器。普通 preview 继续只显示确认流程和界面状态，不执行写入、返回模拟传播结果或导出。

## 4. HTTP 契约

| 方法 | 路径 | 作用 |
|---|---|---|
| GET | `/healthz` | 进程健康与 schema 版本 |
| GET | `/api/bootstrap` | 模型、网格、缓存配额和无凭据底图元数据 |
| GET | `/api/basemap/tianditu/{vec|cva}/{z}/{x}/{y}` | 可选在线瓦片同源代理 |
| GET | `/api/basemap/carto/{base|labels}/{z}/{x}/{y}` | 未配置天地图 token 时启用的 CARTO Voyager/OSM 同源代理 |
| GET | /api/basemap/pmtiles/four-provinces.pmtiles | 固定归档的单段 HTTP Range 响应 |
| HEAD | /api/basemap/pmtiles/four-provinces.pmtiles | 固定长度、类型与 Range 能力 |
| GET | `/api/cache-overview` | 实际缓存用量与区域列表 |
| POST | `/api/inspect-point` | 区域计划、ready 状态和中心高程 |
| POST | `/api/operation-ticket` | 为 estimate/download/calculation 签发短期 capability |
| POST | `/api/operation-status` | exact-ID 状态、sequence 与白名单进度快照 |
| POST | `/api/operation-preview` | 活动 exact-ID calculation 的 latest-only 临时地图覆盖层 |
| POST | `/api/operation-ack` | exact-ID 回收 reserved/terminal 快照 |
| POST | `/api/estimate-download` | 带 operationId 的固定来源下载量与配额预检 |
| POST | `/api/download-region` | 带 operationId 的 DEM/WBM 下载、生成、校验与 ready |
| POST | `/api/delete-cache-region` | 引用安全的区域删除 |
| POST | `/api/calculate` | 带 operationId 的真实 DEM/WBM、ITM 与同步双 PNG 结果 |
| POST | `/api/cancel-download` | exact ID + download family 取消 |
| POST | `/api/cancel-calculation` | exact ID + calculation family 取消 |

没有导出端点、current operation 端点或 operation list。浏览器导出只消费前端内存中的已完成结果，使用 Blob 下载。POST JSON 继续拒绝未知字段；服务一次只允许一个共享操作，冲突返回 HTTP 409。

`POST /api/operation-ticket {"kind":…}` 只接受 `estimate-download`、`download`、`calculation`。服务器用密码学安全随机源生成 UUIDv4 `operationId`；客户端不能自选 ID。reserved ticket 最多 32 项、TTL 60 秒。匹配长请求在同一个状态 mutex 内原子消费 ticket；gate 忙时不消费，错 kind、过期或重复 ticket 不能进入 worker。

estimate/download 的长请求包装为 `{"operationId":"…","point":{…}}`，calculate 为 `{"operationId":"…","request":{…}}`。`operation-status`、两个 cancel 和 `operation-ack` 均为带 exact ID 的 POST JSON，capability 不进入 URL。取消还必须匹配 operation family；未知 ID、错 family 或终态操作返回 HTTP 200 与 `cancelled=false`，不允许退化为按 kind 取消 active。

status 只返回 schema version、operation ID、kind、`reserved/running/cancellation-requested/succeeded/failed/cancelled` 状态、单调 sequence 和 estimate-download/download/calculation 三类 tagged 白名单进度；不返回结果、PNG、data URL、下载 URL、服务器路径或详细错误。terminal 最多 32 项、TTL 5 分钟；ack 按 exact ID 删除 reserved/terminal，重复或未知 ack 幂等返回 false。

progress、cancel、finish 与 lease Drop 使用同一 mutex，并同时核对 ID/generation。取消先被接受时丢弃后来成功，finish 先完成时迟到取消不能命中下一任务，未正常 finish 的 Drop 发布 failed 终态。同步长请求仍是结果的唯一权威来源，状态端点不承担 PNG 结果恢复。

`operation-preview` 严格接收 `{"operationId":"…","afterSequence":N}`。只有相同 exact ID 的活动 calculation 且存在更大的 preview sequence 时返回 HTTP 200 和 schema 1 覆盖层；未知但格式有效的 ID、reserved/非 calculation、无更新、取消中或终态返回 204 空体。preview sequence 与 status sequence 相互独立。服务器只保留活动任务最新帧，取消、完成、失败或 Drop 都清除它，terminal 与 status 继续不含 PNG。

该端点显式携带临时结果，不应被“status 不含结果”的安全声明掩盖。持有活动 operation capability 的同源调用方可以读取预览，因此 ID 仍只位于 POST JSON body，并依赖回环监听、Host 校验、无 CORS 与 SSH 隧道边界；预览不进入 URL、日志、终态或持久缓存。

validation 浏览器在长请求前领取 ticket，以约 250 ms 的递归定时器执行非重叠轮次：先请求 status 并分发新的进度 sequence，再为 calculation 串行请求 `operation-preview`，初始 `afterSequence=0`。每个 handle 保存 ticket promise、ID、客户端 generation 和最后 preview sequence；旧 poll 和迟到响应不能更新新任务。取消共享 3 秒总 deadline；长请求 settle 后停止轮询，final status 与 best-effort ack 各有 1.5 秒上限，并按 handle identity 先释放当前 handle 再 ack。

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
- operation ID 作为 bearer capability，只放在同源 POST JSON body；不写 URL，不提供 current/list，不增加 CORS，也不把 ID 视为跨用户认证机制；
- CSPRNG UUIDv4、60 秒 reserved TTL、5 分钟 terminal TTL、双 32 项上限和 exact-ID ack 共同限制 capability 暴露窗口与内存占用；
- status 输出按字段白名单构造，不序列化工作结果、PNG、URL、服务器路径或详细错误；HTTP 日志也不应记录请求 body 中的 capability。
- 历史 PMTiles 端点曾只允许 GET/HEAD 与单段 bytes Range；该安全证据保留，但端点现为待移除能力，不属于 ADR-0022 当前目标。

validation 模式是一项明确的内部隐私例外：浏览器中的测试坐标、参数和计算请求会离开 Windows 本机并进入用户控制的服务器。不要使用敏感真实位置。服务没有遥测、账号、第三方计算 API 或服务器文件导出；浏览器下载的报告不回传服务器。DEM/WBM 下载仍只访问既有固定 Copernicus HTTPS 来源。配置 token 后，在线底图代理还会从服务器访问固定天地图 HTTPS 主机，但不把浏览器坐标或无线电参数作为上游参数。

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

可选底图 token 只能由交互式管理命令静默录入；命令不会显示 token：

```bash
scripts/validation-platform.sh basemap-token status
scripts/validation-platform.sh basemap-token set
scripts/validation-platform.sh basemap-token clear
```

`set/clear` 后按提示通过受管 `stop`、`start` 应用。未配置 token 时普通地图选择 CARTO Voyager/OSM；配置合法 token 后选择天地图。源码或自动化通过不能冒充受管进程已经重建或真实瓦片已经可见。

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
├─ secrets/
│  └─ tianditu.token
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

### 8.4 Operation capability 与轮询进度

新切片实际通过 Rust workspace offline `83 passed / 3 ignored`、真实 GLO-90 HTTPS `3/3`、validation server `17/17`、前端 `6 files / 41 tests`（其中 backend 专项 20）和 Tauri 纯状态 `4/4`。fmt、clippy `-D warnings`、TypeScript check、Vite build、xwin、`bash -n`、`self-test` 与 `git diff --check` 也均通过。旧的 11 项 server / 26 项 frontend 数量仍只属于上一构建的历史证据，不与新数量混算。

full build revision 为 `867c25aeb2091055b56d1259f6ad7293d21f7495`，`built_at=2026-07-26T19:02:43Z`，server SHA-256 为 `e80c8890ebcd2059341cd495e78546d51287a916776f0a1991e8d99f062afa0c`。受管部署与两次真实回环烟雾见 9.5 节；浏览器可见进度和控制台结果仍需单列，不得从 HTTP 测试推断。

### 8.5 渐进覆盖预览

当前切片实际通过 Rust workspace offline `100 passed / 3 ignored`、真实 GLO-90 HTTPS `3/3`、coverage `20`、app-service `17`、validation-server `19` 和前端 `7 files / 51 tests`。专项 crate 数量包含在 workspace 总数中。rustfmt、Clippy workspace `--all-targets -D warnings`、TypeScript、Vite validation build、Windows x64 full xwin、`bash -n`、管理 self-test 与 diff check 通过。

功能 full build revision 为 `a1219c5ca3254a2a40a50829526cd9bd062d8ea9`，`built_at=2026-07-27T05:48:52Z`，server SHA-256 为 `03bb62e9bc4facdba01c1693fbf2a63ab70d961606a09cfed6fe9b128c845bd2`。测试脚本竞态修复 revision `88204765182de7e842859e672050614c091f1986` 未重建服务二进制。

受管部署与两次真实渐进预览烟雾见 9.7 节和 `18-progressive-coverage-preview-validation.md`。这些证据不证明 Windows WebView2↔Rust Channel 或 SSH 隧道中的用户可见过程。

### 8.6 在线底图与地图控件

2026-07-31 新增天地图 `vec/cva` 同源代理、私密 token 文件、右下动态公制比例尺和 MapLibre desired-state 重放。专项证据为前端 2 个文件/4 项测试、Rust `basemap::tests` 4 项测试通过；前端全量为 9 个文件/56 项。Rust workspace all-targets、Clippy、TypeScript、validation Vite build、Windows x64 xwin、`bash -n`、管理 self-test 与 diff check 均通过。清空回归覆盖 style 暂不可用时保留 pending，并在 idle 后删除 layer/source、撤销 Blob URL。

功能 revision `6e9714c6cdcdeb54ff47e229d8d43b18bf32b3c6` 已完成受管 `stop → build → start → status/health`，`built_at=2026-07-31T12:19:55Z`，server SHA-256 为 `d5f57bd71de4f64c62359591edbbee9b23461461d63265b68dd2a5f9dac640f9`。新 PID `2306446` 只监听 `127.0.0.1:1421`；bootstrap 明确 `enabled=false`，合法瓦片路径返回 HTTP 503 和 `Cache-Control: no-store`。

当前 token 未配置，因此没有真实瓦片、上游 HTTP 200、浏览器截图或控制台证据。完整方法和发布边界见 `20-tianditu-basemap-proxy.md`。

### 8.7 历史：四省 PMTiles 接入（已退出当前目标）

固定资产为 source build 20260731、bbox 107.5,18,125.5,33.5、z0-9、33,044,072 bytes、SHA-256 5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0、gzip MVT、939 个 region tiles / 837 个 archive entries，占 2.5 GB 的 1.32%。

实现目标是同源 Range-only 读取，MapLibre 仅显示 earth、landcover、landuse、water、roads，并持续显示 © OpenStreetMap contributors。boundaries、places、pois 不进入可见样式。原始归档仍含 boundaries 与 Natural Earth/OSM 内容；当前只作私有验证、不纳入正式 EXE，且不作公开发行结论。

前端 9 文件/62 测试、Rust workspace 112 passed/5 ignored、validation-server 27/27、固定 SHA-256、Range/HEAD/bootstrap、SSH 隧道及 PMTiles JavaScript getHeader/getZxy MVT 读取均通过。功能提交 db052e6 已完成 clean stop/build/start；真实浏览器视觉与控制台因 Codex 桌面 ACL 故障仍待人工确认，详细证据见 docs/21-protomaps-four-province-basemap.md。

### 8.8 会话覆盖层与浏览器诊断导出

2026-08-01 当前切片把已完成覆盖结果扩展为最多 8 个会话层：不同坐标累积、同点替换、最新置顶、第 9 项淘汰最早项；选择新点或取消重算不删除旧会话层，“清空”删除全部层。结果不持久化，重叠不计算联合场强。

validation 能力矩阵现在允许浏览器本地诊断导出。PNG 使用固定 1600×1100 报告画布 Blob 下载；PDF 在浏览器内构造单页 A4 横向文件。服务器路由、请求体上限和文件系统权限均未扩展，未知 `/api/export-result` 仍被拒绝。第 8.2、9.3 节中“导出禁用”“取消重算清除唯一旧图”的文字是当时版本的历史证据，已由本节和 ADR 0019 取代。

专项自动化已完成 5 文件/46 项测试；前端全量为 11 文件/73 项，Rust workspace 为 `113 passed / 5 ignored`，rustfmt、Clippy `--all-targets -D warnings`、validation 管理脚本 `bash -n` 与 self-test 均通过。功能提交 `6261f8dc22bdeeefcdd19e923582d72f5918fbb0` 已推送并完成干净 stop/build/start；新 PID 2496862 只监听 `127.0.0.1:1421`，服务器 health、bootstrap、未知导出路由拒绝以及 Windows SSH 隧道 health 均通过。真实浏览器多点交互和文件打开仍待用户验收。

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
ADR 0013 的 exact operation ID 已在实现层面取代上述单客户端归属推断；这一段的运行数字仍来自旧协议，新 revision 的受管 HTTP 证据单列于 9.5 节。

### 9.5 Operation identity 受管 HTTP 验收

full build revision `867c25aeb2091055b56d1259f6ad7293d21f7495` 的构建时间为 `2026-07-26T19:02:43Z`，server SHA-256 为 `e80c8890ebcd2059341cd495e78546d51287a916776f0a1991e8d99f062afa0c`。

| 检查 | 结果 |
|---|---|
| stop/build/start | 旧 PID `1114524` 退出；新 PID `1185566` |
| 重复 start | PID 保持 `1185566`，没有第二个 server |
| liveness/readiness | `status`、`health`、`/api/bootstrap`、`/api/cache-overview` 全部通过 |
| 缓存不变性 | 前后均为 `133,071,416 bytes`、`partial=0`；两个区域各 `50/50 ready` |
| 真实烟雾 | `validation-recovery-smoke.sh` 连续两次通过，均输出 `ticket_a_cancelled=true ticket_b_http=200 progress_a=2 progress_b=2`；`progress_a/progress_b` 是所观察快照的 sequence 值 |

烟雾内部实际断言：canonical 未知 ID cancel 为 false，错 family cancel 为 false；ID-A 活动时 ID-B calculate 返回 HTTP 409，ID-B 保持 `reserved`、`sequence=0`、`progress=null`。正确取消 ID-A 返回 true，ID-A calculate 为 HTTP 422 且不含 PNG，terminal 为 cancelled；ack 首次 true、再次 false，随后 status 为 404。

脚本复用同一 ID-B 后计算返回 HTTP 200；已 ack 的旧 ID-A cancel 为 false且 ID-B 仍保持活动。结果包含两张各自唯一且 Base64 可解码的 PNG，均通过 signature/IHDR `401×401` 检查；ID-B terminal 为 succeeded 且不含 PNG，ack 后 status 为 404。

每次运行最终 gate 为空、`/healthz` 为 200，缓存/readiness 不变且无 `validation-recovery-smoke.*` 临时目录残留。本节只证明服务器回环上的受管 HTTP identity 与 progress；本轮没有通过 SSH 隧道在浏览器验证可见逐阶段进度、取消/重试 UI 或控制台。


### 9.6 Schema 3 手动地面海拔受管验收

revision `2e4411de809d1f78b6dd1407d51a2351d58b02ed` 已完成受管 stop/build/start。PID `1301627` 只监听 `127.0.0.1:1421`，server SHA-256 为 `e8151b46aad3318abddbade68a465c8c04c9851a24166888f57b9cadebae78fa`；health、bootstrap schema 2 与缓存 readiness 通过。

`scripts/validation-manual-elevation-smoke.sh` 在成都真实缓存上确认自动 `526.3442993164062 m` 与手动 `1500.0 m` 的 schema 3 来源正确，原始热力图和 EPSG:3857 覆盖层哈希均变化。更新后的 recovery smoke 同时通过 exact-ID 取消、进度、ack、schema 3 和 DEM 来源断言。

运行前后缓存均为 `133,071,416 bytes`、`partial=0`，两个区域各 `50/50 ready`，无 smoke 临时目录残留。完整证据见 `17-manual-elevation-and-download-transport-validation.md`。

本节仍不关闭 SSH 隧道浏览器视觉、Windows/Tauri、弱网/磁盘压力或地图合规。

### 9.7 渐进式覆盖预览受管验收

功能 revision `a1219c5ca3254a2a40a50829526cd9bd062d8ea9` 完成 full build 和受管 stop/build/start：

| 字段 | 结果 |
|---|---|
| built_at | `2026-07-27T05:48:52Z` |
| PID | `1403529` |
| bind | `127.0.0.1:1421` |
| server SHA-256 | `03bb62e9bc4facdba01c1693fbf2a63ab70d961606a09cfed6fe9b128c845bd2` |
| liveness/readiness | status、health、self-test、bootstrap、cache overview 通过 |

`scripts/validation-progressive-preview-smoke.sh` 在成都真实缓存上两次通过。每次都观察到 2 张 schema 1、sequence/完成数/PNG 内容不同的部分覆盖层；总像素 `125,628`，最后预览分别完成 `123,410` 和 `121,808`，首帧分别为 `5,610 ms` 和 `5,660 ms`，总耗时分别为 `7,246 ms` 和 `7,301 ms`。

烟雾同时验证：

- 每帧为 EPSG:3857、`401×401`，PNG signature/IHDR 有效且 `completed < total`；
- 最终同步响应为 schema 3，包含有效原始热力图和地图 overlay，且不含 preview 字段；
- terminal 后 preview 返回 204，status/terminal 不含 PNG，ack 后 status 返回 404；
- 缓存前后均为 `133,071,416 bytes`、`partial=0`，两个区域各 `50/50 ready`；
- schema 3 recovery smoke 随后连续两次通过。

干净重启后的进程基线 `VmHWM/VmRSS=2,920 KiB`；第二次计算后 `VmHWM=195,484 KiB`、`VmRSS=20,200 KiB`。这是整个 Linux 进程的高水位，不是预览增量内存，也不能外推到 Windows。

第一次烟雾因测试脚本观察 curl 子进程退出与 HTTP 状态文件之间的竞态而失败；应用始终健康、无缓存变化和临时目录残留。提交 `88204765182de7e842859e672050614c091f1986` 只修复脚本并在退出后继续硬性校验 curl 状态、状态文件与 HTTP 200，没有重建 `a1219c5` 服务二进制。

完整自动化数量、Windows 交叉产物哈希和逐次指标见 `18-progressive-coverage-preview-validation.md`。本节只关闭受管回环平台；SSH 隧道浏览器可见过程和 Windows WebView2↔Rust Channel 仍待验。

## 10. 尚未关闭

- 四省 PMTiles 的历史自动化、Range、SSH 隧道、固定资产校验和受管运行证据已回填；当前代码、依赖与路由移除已完成，约 33 MB runtime 资产受管清理尚未完成；
- operation capability 与渐进覆盖预览的代码回归、新构建和受管 HTTP 烟雾已通过；SSH 隧道浏览器可见预览、取消/重试 UI 和控制台仍待验证；
- 在线天地图当前未配置 token；既有禁用态已受管部署并 fail closed，但纯在线主路径尚未完成受管重启，真实 `vec/cva`、缩放、比例尺、署名、热力图层级与清空浏览器烟雾待验证；
- EOxCloudless 卫星失败→在线普通地图→WGS84 网格的分级回退尚未按 ADR-0022 验证；
- Windows 10/11 WebView2、原生保存、安装/卸载和真实文件系统；
- 十进制 2.5 GB 实体边界压力、磁盘不足、弱网中断和进程强制崩溃注入；
- GPU 主机整机重启后的手动恢复流程；
- 合规中国大陆在线底图、有效审图号、在线服务/叠加/应用分发/导出授权；
- 传播结果的外场测量校准。

此前私有浏览器验收只关闭旧协议下服务器回环验证路径中的真实计算与热力图显示问题，不包含本轮 operation progress UI。任何服务器 HTTP 或历史浏览器证据都不能据此关闭 Windows/Tauri 实机或中国大陆地图合规门槛。
