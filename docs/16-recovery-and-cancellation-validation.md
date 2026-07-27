# HamHeatmap 恢复与取消验证记录

- 日期：2026-07-27
- 主机：`gpu-273312`（`ubuntu@150.65.181.202`）
- 工作区：`/home/ubuntu/hamheatmap`
- 范围：缓存重启整理、2.5 GB 边界不变量、取消结果交付、私有平台管理状态、operation capability 与 HTTP 轮询进度
- 状态：旧协议证据保留；新 operation 协议的代码回归、full build、受管应用进程重建/readiness 和两次真实 HTTP identity/progress 烟雾已验证；SSH 隧道浏览器可见进度、真实 2.5 GB 压力、整机重启、Windows 实机与地图合规待验证

## 1. 本切片要关闭的故障窗口

此前的正常路径已经能下载 DEM/WBM、续传 partial、计算双 PNG，并通过 SSH 隧道显示真实热力图。本切片处理的是正常流程边缘的状态竞争：

1. partial 已增长，但 SQLite 的 `size_bytes` 还没更新；
2. partial 已原子改名为最终文件，但 ready 状态还没提交；
3. ready、missing 或 corrupt 记录旁留下不应续传的 partial；
4. 文件总量恰好到达硬上限，或 metadata 写入自身把根目录推过上限；
5. 用户取消与 worker 成功返回几乎同时发生；
6. SSH 中断后控制锁遗留，或 PID 被复用后管理脚本误判所有者；
7. `/healthz` 成功被错误理解为缓存与数据也已就绪。

本切片不改变传播模型、401×401 网格、地图合规边界或 Windows 产品形态。

## 2. 缓存恢复不变量

`CacheStore::open` 获得缓存根独占锁并初始化 SQLite 后，按“整理，再检查现有硬上限”的顺序执行：

| 索引状态 / 文件状态 | 打开时动作 |
|---|---|
| ready + 最终文件有效 + 陈旧 partial | 保留最终文件，删除 partial |
| ready + 最终文件无效 | 标记 corrupt，删除 partial |
| downloading + 最终文件已完整改名 | 校验并推进 ready，删除 partial |
| downloading + partial 长度等于 SQLite checkpoint | 保留为可评估续传 |
| downloading + partial 长于 SQLite checkpoint | 截断并同步到 checkpoint，绝不上调 SQLite |
| downloading + partial 短于 checkpoint，或 checkpoint 非零但文件缺失 | 删除并标记 corrupt |
| downloading + partial 超出期望大小 | 删除 partial，标记 corrupt |
| missing/corrupt + partial | 删除 partial |
| 未登记 partial | 删除 |
| 整理后根目录恰好等于 cap | 允许打开 |
| 整理后根目录超过 cap | 阻断打开，不删除可信 downloading partial |

SQLite `size_bytes` 只在文件先 `sync_all` 后更新，因此是唯一可信前缀上限。续传仍要求 downloading 状态、期望总大小、SQLite partial 长度、整理后的磁盘实际长度、强 ETag 和 Range 能力一致。弱 ETag、不支持 Range、状态变化或大小变化都不能沿用旧 partial；截断或同步失败会阻断 `CacheStore::open`，不能降级为继续使用未知尾部。

区域及其全部资产描述符在同一 SQLite 事务内写入。写入前检查 metadata headroom，提交前再扫描根目录硬上限；失败时不能留下半个 region、孤立 asset 或不完整 region-assets 引用。

自动化使用缩小的 cap 制造“已检查点数据恰好到 cap”“已检查点后多 1 byte”和 metadata 临界点，并构造 SQLite checkpoint 与文件长短不一致的重启状态。未检查点的额外尾部会先截断，只有文件与 SQLite 都确认超限才阻断打开。这证明规则，不证明已经在 2.5 GB 实体数据和真实磁盘耗尽条件下完成压力测试。

## 3. 取消与结果交付

### 3.1 后端线性化点

validation server 的 `OperationLease::finish` 和 Tauri 的 `DesktopOperationLease::finish` 在持有操作状态锁时完成以下顺序：

1. 确认 lease 身份仍对应 active 操作；
2. 读取取消标志；
3. 清除 active；
4. 若取消已经被接受，丢弃 worker 的成功值并返回取消；否则交付原 outcome。

Tauri 取消仍绑定桌面 active lease；validation 新协议同时要求 exact operation ID 与匹配 family。两者都只设置对应 active 的取消标志，不会提前释放门闩。因此旧 worker 真正结束前，新操作仍应被拒绝或由官方 UI 保持不可重试状态。`AppService` 还在传播完成、两张 PNG 编码、Base64 转换和最终结构交付前设置检查点。

### 3.2 前端结果卫生

官方单窗口 UI 的回归测试从已有热力图开始发起重算并取消，确认：

- 旧 heatmap 被清除；
- 导出立即保持禁用；
- 已取消 promise 结束前不把旧成功结果恢复；
- 操作收尾后可重新计算并显示新结果。

上述前端结果卫生测试仍成立。旧 HTTP 协议没有 operation ID 的限制由 ADR 0013 的 capability 设计取代；但新 revision 的自动化、受管服务与浏览器证据需按 7.3 节补齐，不能沿用旧烟雾数字推断。

### 3.3 Operation capability、状态与进度

validation 长操作执行如下状态流：

```text
ticket(kind) → reserved → running → cancellation-requested → cancelled
                            └───────────────────────────────→ succeeded
                            └───────────────────────────────→ failed
```

- `POST /api/operation-ticket` 由服务端 CSPRNG 生成 UUIDv4 capability；reserved 最多 32 项、TTL 60 秒。
- estimate/download/calculate 带 `operationId`；匹配 ticket 只在 gate 空闲时原子消费，busy 不消费。
- `operation-status` 按 exact ID 返回 state、单调 sequence 和白名单 progress，不保存结果、PNG、URL、路径或详细错误；terminal 最多 32 项、TTL 5 分钟。
- cancel 同时匹配 exact ID 与 calculation/download family。未知、错 family 或终态返回 200/false，绝不回退到按 kind 操作当前 lease。
- progress、cancel、finish 和 Drop 在同一 mutex 下核对 ID/generation。取消先到时成功值被丢弃；finish 先到时迟到 cancel 不影响后来 ID；Drop 进入 failed 并释放 gate。
- `operation-ack` 按 exact ID 删除 reserved/terminal，重复或未知确认幂等 false。
- validation 前端用约 250 ms 非重叠 POST 轮询复用既有进度监听器，并以本地 generation 隔离旧响应。同步长请求仍是唯一结果来源；settle 后停止轮询并 best-effort ack。

## 4. 私有平台管理恢复

管理脚本使用两种不同所有权对象：

- `control.lock/owner`：覆盖 build/start/stop 这类短管理命令；
- `runner.claim/owner`：覆盖后台 runner 从启动到退出的完整生命周期。

owner 记录 PID、Linux 进程 start time 和 boot ID。陈旧目录只有在所有者不再匹配且目录至少经过 5 秒初始化保护期后才能原子移走；存活所有者保持排他。PID 文件只用于定位，不再单独授权信号操作。

发送信号前还必须校验用户、可执行文件和完整 argv。server 的 argv 必须精确包含固定 `--bind 127.0.0.1:1421`、dist 和 data 路径；runner 与日志 monitor 也必须匹配精确内部子命令和进程 start time。所有托管路径逐分量拒绝符号链接，并在解析后保持位于项目根内。

`scripts/validation-platform.sh self-test` 使用独立临时状态目录检查陈旧锁/claim 恢复、存活 claim 排他、符号链接逃逸拒绝、精确 argv 与当前托管 PID 身份；它不停止、重启或重建持久平台。

## 5. Liveness 与 readiness

- `GET /healthz`：只证明当前回环 HTTP 进程能响应并返回协议 schema；不打开缓存。
- `GET /api/bootstrap`：通过共享 `AppService` 打开 `CacheStore`，会取得锁、执行重启整理、检查硬上限并读取真实 usage。

因此部署或恢复验收必须把 `health` 和 `bootstrap` 分开记录。前者成功、后者失败时，平台进程仍是 live，但数据服务不是 ready。

## 6. 自动化证据

| 检查 | 结果 | 边界 |
|---|---:|---|
| Rust workspace 离线 | 77 passed / 3 ignored | 3 项 ignored 为显式真实网络测试 |
| app-service | 12 | 含编码阶段取消检查点 |
| cache | 21 | 含重启整理、exact-cap 和 metadata 事务 |
| coverage / export / propagation | 15 / 6 / 6 | 传播与输出回归 |
| official reference / terrain | 1 / 5 | 模型参考与数据读取 |
| validation server | 11 | 含完成/取消线性化 |
| 真实 GLO-90 HTTPS | 3/3 | 另行联网运行 |
| 前端 | 26 | 含取消旧结果清理、延迟取消屏障和重试 |
| Tauri 纯状态控制器 | 4/4 | 不需要 Windows UI |
| Windows xwin 目标检查 | 通过 | 不是 EXE/NSIS 最终重建 |
| 管理脚本 `bash -n` / `self-test` | 通过 | 不等于真实 stop/start |
| 加固版真实恢复运行 | 通过 | stop/build/start/readiness、缓存不变、真实取消与重算 |

离线 workspace 的 77 项由 app-service 12、cache 21、coverage 15、export 6、propagation 6、official reference 1、terrain 5、validation server 11 构成。3 项真实网络测试单列为 ignored，不重复计入 77。
以上数量和“通过”仅对应 revision `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d` 的旧 HTTP 协议，不与新切片混算。

revision `867c25aeb2091055b56d1259f6ad7293d21f7495` 的新增证据如下：

| 检查 | 结果 | 边界 |
|---|---:|---|
| Rust workspace 离线 | 83 passed / 3 ignored | ignored 仍为显式真实网络测试 |
| 真实 GLO-90 HTTPS | 3/3 | 另行联网运行 |
| validation server | 17/17 | operation ticket/status/cancel/ack 与线性化 |
| 前端 | 6 files / 41 tests | backend 专项 20；含非重叠轮询、generation 与有界 cleanup/cancel |
| Tauri 纯状态控制器 | 4/4 | 不等于 Windows WebView2 实机 |
| 质量门禁 | 全部通过 | fmt、clippy `-D warnings`、TypeScript check、Vite build、xwin、`bash -n`、`self-test`、`git diff --check` |
| full build | 通过 | `built_at=2026-07-26T19:02:43Z`；server SHA-256 `e80c8890ebcd2059341cd495e78546d51287a916776f0a1991e8d99f062afa0c` |


## 7. 真实运行验收：旧协议与新受管 HTTP 已完成，浏览器可见进度待补

第 7.1—7.2 节的应用基线和 build metadata revision 均为 `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d`；第 7.3 节单列 revision `867c25aeb2091055b56d1259f6ad7293d21f7495` 的新协议证据。

### 7.1 进程与缓存恢复

- [x] 严格停止旧 PID `214692`，完成 `build → start`，新受管 PID 为 `1114524`。
- [x] `status`、`health`、`/api/bootstrap` 和 `/api/cache-overview` 全部通过。
- [x] 缓存总量在应用进程重启前后均为 `133,071,416 bytes`，partial 为 0；两个区域各 `50/50 ready`。
- [x] 重复 `start` 保持 PID `1114524`，没有启动第二个 server。
- [x] 直接调用内部 `__run` 被拒绝并返回 `another validation runner is active`。

### 7.2 真实 HTTP 取消与重算

`scripts/validation-recovery-smoke.sh` 加固后再次连续对受管回环服务真实运行通过：

- [x] 运行前以无效 point 调用 `/api/inspect-point` 返回 HTTP 422，确认 gate 初始为空。
- [x] 后台 calculate curl 同时匹配本 shell job、PPID、进程 start time 和 curl executable 后，才把 gate 中的操作视为本脚本所有。
- [x] 活动计算进入 gate 后，取消端点返回 HTTP 200 和 `cancelled=true`。
- [x] 被取消的 calculate 返回 HTTP 422，响应不含 `heatmapPngDataUrl` 或 `mapOverlayPngDataUrl`。
- [x] 相同请求随后返回 HTTP 200；`schemaVersion=2`，原始图和 map overlay 的宽高字段均为 `401×401`。
- [x] 两个 PNG data URL 字段各恰好出现一次且 payload 非空；Base64 均成功解码，前 24 bytes 均验证为 PNG signature、IHDR 和 `401×401` 尺寸。
- [x] 最终 gate probe 返回业务校验 HTTP 422 而非冲突 409，`/healthz` 仍返回 200。
- [x] 最终输出 `validation recovery smoke passed: cancel=true cancelled_http=422 recovery_http=200`，运行后没有 `validation-recovery-smoke.*` 临时目录残留。

烟雾脚本直接访问服务器 `127.0.0.1:1421`；SSH 隧道路径已由此前浏览器验收独立覆盖。该身份校验只证明受控单 shell/单客户端测试拥有其后台 curl，不能替代多客户端 operation ID。本次是应用进程 stop/start，不是 GPU 主机整机重启。

### 7.3 Operation identity 与轮询进度（受管 HTTP 已实测）

旧的 curl PID/PPID 归属推断不再是协议安全边界。新烟雾直接使用服务器签发的 capability，并完成以下证据：

- [x] revision、server/frontend 测试、格式/lint、脚本检查和 full managed build 均通过；旧 PID `1114524` 更新为 `1185566`，重复 start 后 PID 不变；
- [x] 服务器签发两个不同 ID；ID-A 活动时 ID-B calculate 返回 409，ID-B 仍为 `reserved`、`sequence=0`、`progress=null`，随后复用同一 ID-B；
- [x] canonical 未知 ID 和错 family cancel 均为 false；ID-A 至少观察到一个真实 calculation progress snapshot，观测时 `sequence=2`；
- [x] 正确取消 ID-A 返回 true，calculate 为 HTTP 422且无双 PNG，terminal 为 cancelled；ack 首次 true、再次 false，随后 status 为 404；
- [x] 同一 ID-B 恢复计算为 HTTP 200，两张各自唯一的 PNG 均通过 Base64/signature/IHDR `401×401` 验证；至少观察到一个真实 calculation progress snapshot，观测时 `sequence=2`，terminal succeeded 不含 PNG，ack 后 status 为 404；
- [x] 已 ack 的旧 ID-A cancel 为 false且不影响活动 ID-B；最终 gate、health、bootstrap、cache overview 正常，缓存/区域状态不变且没有临时烟雾目录；
- [ ] 浏览器通过 SSH 隧道显示真实逐阶段进度，轮询不重叠，取消/重试无旧 generation 污染且控制台无新错误。

`validation-recovery-smoke.sh` 连续两次输出 `ticket_a_cancelled=true ticket_b_http=200 progress_a=2 progress_b=2`；其中 `progress_a/progress_b` 记录的是所观察 progress snapshot 的 sequence 值，不是快照数量。因此 operation identity、白名单 progress 快照和恢复语义只在受管服务器 HTTP 范围内关闭；浏览器可见轮询仍待实测，Windows WebView2 和公开产品安全边界仍需独立验收。

## 8. 仍未关闭

- [ ] 使用真实十进制 2.5 GB 数据执行 exact-cap、over-cap、磁盘不足和强制崩溃注入；
- [ ] GPU 主机整机重启后的手动恢复与缓存 overview 复核；
- [ ] 弱网下载中断/恢复和日志轮转压力；
- [ ] 第 7.3 节的 SSH 隧道浏览器可见进度、取消/重试 UI 和控制台证据；新构建与受管 HTTP 烟雾已关闭，“缺少 operation ID / 没有 HTTP 进度”的实现不得回退为按 kind 取消；
- [ ] Windows 10/11 WebView2、安装、原生导出和真实文件系统；
- [ ] 合规中国大陆底图、审图号、署名、离线/导出授权和公开发布验收。
