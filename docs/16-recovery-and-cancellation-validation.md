# HamHeatmap 恢复与取消验证记录

- 日期：2026-07-27
- 主机：`gpu-273312`（`ubuntu@150.65.181.202`）
- 工作区：`/home/ubuntu/hamheatmap`
- 范围：缓存重启整理、2.5 GB 边界不变量、取消结果交付、私有平台管理状态、operation capability 与 HTTP 轮询进度
- 状态：旧协议代码、加固版应用进程重建/重启、readiness 和真实 HTTP 取消恢复已验证；新 operation 协议的构建/烟雾/浏览器证据，以及真实 2.5 GB 压力、整机重启、Windows 实机与地图合规待验证

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
| downloading + 合法 partial | 用实际长度更新 SQLite，保留为可评估续传 |
| downloading + partial 超出期望大小 | 删除 partial，标记 corrupt |
| missing/corrupt + partial | 删除 partial |
| 未登记 partial | 删除 |
| 整理后根目录恰好等于 cap | 允许打开 |
| 整理后根目录超过 cap | 阻断打开，不删除可信 downloading partial |

续传仍要求 downloading 状态、期望总大小、SQLite partial 长度、磁盘实际长度、强 ETag 和 Range 能力一致。弱 ETag、不支持 Range、状态变化或大小变化都不能沿用旧 partial。

区域及其全部资产描述符在同一 SQLite 事务内写入。写入前检查 metadata headroom，提交前再扫描根目录硬上限；失败时不能留下半个 region、孤立 asset 或不完整 region-assets 引用。

自动化使用缩小的 cap 制造“恰好到 cap”“多 1 byte”和 metadata 临界点，以便快速、确定地覆盖边界。这证明规则，不证明已经在 2.5 GB 实体数据和真实磁盘耗尽条件下完成压力测试。

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
以上数量和“通过”仅对应 revision `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d` 的旧 HTTP 协议。operation capability 切片不得复用这些数量；实际 server/frontend 测试、脚本检查和运行证据完成后再新增一行。


## 7. 真实运行验收：旧协议已完成，新协议待补

本次应用基线提交和 validation build metadata revision 均为 `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d`。

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

### 7.3 Operation identity 与轮询进度（待实测）

旧的 curl PID/PPID 归属推断不再是协议安全边界。新烟雾应直接使用服务器签发的 capability，并记录以下证据：

- [ ] 新 revision 的 server/frontend 测试、格式、lint、脚本语法和 managed build 实际结果；
- [ ] 同一客户端领取的两个 operation ID 不同且均为服务端 UUIDv4；busy 不消费 reserved ticket；
- [ ] 活动 ID-A 运行时，未知 ID 与错 family 取消均为 false，ID-A 状态与 progress 继续推进；
- [ ] 正确取消 ID-A 后，calculate 为取消且无双 PNG，status 为 cancelled；ack 后 ID-A 不可再查询；
- [ ] ID-B 恢复计算为 HTTP 200，两张 `401×401` PNG 完成 Base64/signature/IHDR 验证，终态 succeeded 后 ack；
- [ ] 旧 ID-A 的 cancel/status 不影响 ID-B 或下一任务，最终 gate、health、bootstrap、cache overview 正常；
- [ ] 浏览器通过 SSH 隧道显示真实逐阶段进度，轮询不重叠，取消/重试无旧 generation 污染且控制台无新错误。

证据完成前只记录“协议已实现、待实测”，不写新的 revision、PID、测试数量、进度 sequence 或运行通过结论。完成后，HTTP 渐进进度与多标签页错误取消风险可在 validation 平台范围内关闭；Windows WebView2 和公开产品安全边界仍需独立验收。

## 8. 仍未关闭

- [ ] 使用真实十进制 2.5 GB 数据执行 exact-cap、over-cap、磁盘不足和强制崩溃注入；
- [ ] GPU 主机整机重启后的手动恢复与缓存 overview 复核；
- [ ] 弱网下载中断/恢复和日志轮转压力；
- [ ] 第 7.3 节的新构建、HTTP 烟雾和浏览器证据；“缺少 operation ID / 没有 HTTP 进度”的实现待办已由 ADR 0013 切片关闭，不得回退为按 kind 取消；
- [ ] Windows 10/11 WebView2、安装、原生导出和真实文件系统；
- [ ] 合规中国大陆底图、审图号、署名、离线/导出授权和公开发布验收。
