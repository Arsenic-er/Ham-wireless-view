# ADR-0013：服务端 operation capability 与轮询进度

- 日期：2026-07-27
- 状态：已采纳；代码回归与受管 HTTP 运行已验证，SSH 隧道浏览器可见进度待补
- 替代：ADR-0012 决策 9 中“按任务类型取消当前任务”和“HTTP 模式无渐进进度”的部分
- 保留：ADR-0012 的回环监听、SSH 隧道、三态前端、无导出和单共享操作边界

## 背景

私有 validation 平台最初用单任务门闩串行化共享 Rust 服务，并允许取消同类型的当前任务。恢复烟雾可以用受控 shell 的 curl PID、PPID 和 start time 证明测试进程所有权，但 HTTP 协议本身不能证明取消请求来自启动该 worker 的标签页。两个标签页同时使用时，旧标签页可能取消另一个标签页后来开始的同类型任务。

validation 浏览器也没有 Tauri 事件通道。长请求可以返回最终结果，取消可以通过并发 HTTP 请求设置令牌，但浏览器在等待期间没有真实 calculation/download 进度。把结果改成服务器异步队列会增加 PNG 保留、恢复、容量和隐私边界；给所有操作增加账号/会话又超出 SSH 隧道内的私有 Alpha 范围。

需要一个最小协议：把取消和状态绑定到不可猜测的具体操作，向现有进度 UI 提供真实更新，同时不把热力图变成服务器端可枚举结果。

## 决策

### 1. 服务端签发 capability

新增：

```text
POST /api/operation-ticket
{"kind":"estimate-download"|"download"|"calculation"}
```

服务器使用密码学安全随机源生成 UUIDv4 `operationId`。客户端不能提供或选择 ID。operation ID 是短期 bearer capability，不是用户账号、跨租户认证或公网访问授权；回环监听与 SSH 隧道仍是外层访问边界。

reserved ticket：

- 最多 32 项；
- TTL 60 秒；
- 记录 ID、精确 kind、创建时间和 reserved 状态；generation 仅在匹配 ticket 被原子消费、进入 active 时分配；
- 不保存请求、坐标、URL 或结果。

### 2. 原子消费与单任务 gate

长请求包装为：

```text
POST /api/estimate-download {"operationId":"…","point":{…}}
POST /api/download-region   {"operationId":"…","point":{…}}
POST /api/calculate         {"operationId":"…","request":{…}}
```

只有匹配 ID 与 kind 的未过期 reserved ticket 可以启动。ticket 检查、共享 gate 检查和 reserved→running 必须在同一个 operation-state mutex 内完成。

- gate 空闲：原子消费 ticket 并建立 active lease；
- gate 忙：返回冲突，但 ticket 保持 reserved；
- ID 未知、过期、重复消费或 kind 不匹配：拒绝，不启动 worker；
- 任何失败路径都不得退化为“使用当前同类操作”。

点检查、缓存概览、删除等短操作可继续使用共享 gate，但不获得可轮询 ticket；它们也不能被 operation cancel 端点命中。

### 3. 状态与进度快照

新增：

```text
POST /api/operation-status
{"operationId":"…"}
```

状态集合固定为：

```text
reserved
running
cancellation-requested
succeeded
failed
cancelled
```

响应只包含 schema version、精确 ID、kind、state、单调 `sequence` 和三类 tagged progress：`estimate-download` 只有 `{type, stage:"estimating"}`，不含 URL/资产/结果；`download` 只有字节、资产序号/数量与 percent，不含内部 asset key 或 URL；`calculation` 只有 phase、percent 与完成/总像素数。

状态响应不得包含：

- 计算或下载结果；
- PNG、Base64 或 data URL；
- 远端下载 URL；
- 服务器文件路径；
- 详细错误、堆栈或内部基础设施信息。

终态快照最多 32 项，TTL 5 分钟。相关协议操作在持锁时清理过期 reserved/terminal 项，容量淘汰不能删除 active lease。服务不提供 current operation 或 operation list；status 使用 POST JSON，使 capability 不进入查询字符串、浏览历史和常规访问日志。

### 4. 精确取消与确认

取消请求改为：

```text
POST /api/cancel-calculation {"operationId":"…"}
POST /api/cancel-download    {"operationId":"…"}
```

服务器同时匹配 exact ID 和取消 family。未知 ID、错 family、已终态或已过期操作返回 HTTP 200 与 `cancelled=false`，不改变 active。接受取消时，在同一 mutex 内进入 `cancellation-requested`、递增 sequence 并设置对应 worker 的取消令牌；gate 仍由 worker lease 持有。

新增：

```text
POST /api/operation-ack {"operationId":"…"}
```

ack 按 exact ID 删除 reserved 或 terminal 项。未知、重复或过期 ack 幂等返回 false。ack 不释放 active，不返回结果，也不改变同步长请求的结果语义。

### 5. 线性化与异常释放

progress、cancel、finish 和 lease Drop 必须使用同一个 operation-state mutex，并同时验证 operation ID 与 generation：

- cancel 先被接受：worker 即使随后返回成功，也发布 `cancelled` 并丢弃成功值；
- finish 先完成：迟到 cancel 看到 terminal 并返回 false，不能命中后来 operation；
- 旧 progress callback：ID/generation 不匹配时丢弃，不能递增后来 operation 的 sequence；
- lease 未显式 finish 而 Drop：释放 active 并发布最小 `failed` terminal，不泄露 panic/error 细节。

### 6. validation 浏览器轮询

validation 前端保留公开 backend 函数与现有 calculation/download progress 监听器。每项长请求维护 handle：ticket promise、解析后的 ID、本地 generation、轮询定时器、in-flight poll 和 settle 标志。

流程：

1. 先领取 ticket；
2. 发送带 operation ID 的同步长请求；
3. 以约 250 ms 的递归 `setTimeout` 发起非重叠 `operation-status` POST；
4. 仅在 handle、ID、generation 匹配且 sequence 更新时分发 progress；
5. 用户在 ticket 返回前取消时，等待本 handle 的 ticket promise，只向该 ID 发送 cancel；
6. 长请求 settle 后停止定时器、abort/等待 in-flight poll，并以 1.5 秒上限完成一次 final status；
7. final status 结束后按 handle identity 先释放当前 handle，再用独立 1.5 秒上限 best-effort ack。

先释放 handle、后 bounded ack 是身份安全边界：旧 cleanup 只能清除仍匹配的 generation，迟到 final status/ack 不能覆盖或清除后来 operation，ack 卡顿也不能继续占用客户端槽。final status 或 ack 失败不替代同步长请求结果，服务器 TTL/容量负责最终回收。

取消共享 3 秒总 deadline。exact cancel 返回 false 后必须查询同一 operation ID 的 exact status；`reserved` 或 `running` 时每 100 ms 继续用同一 ID 重试，`cancellation-requested`、`cancelled/succeeded/failed`、404 或原 handle settle 时停止。deadline 用尽时向 UI 抛出明确取消超时错误，不能静默成功、改用新 handle 或回退为按 kind 取消。

普通轮询的临时 status 失败不替代长请求错误；旧 poll 迟到时不得更新新任务。Tauri 继续使用原生事件，preview 继续禁止真实操作。

同步 estimate/download/calculate 响应仍是业务结果的唯一权威来源。terminal status 不保存 PNG，也不支持在长响应丢失后恢复结果；需要结果时重新发起新的 ticket 和长请求。

## 结果

优点：

- 多标签页的旧取消请求不能仅凭 kind 命中后来任务；
- validation 浏览器能显示共享 Rust 服务的真实阶段进度；
- 不建立服务器端 PNG 结果队列、current/list 枚举或长期会话；
- exact ID、TTL、容量和 ack 使状态生命周期明确且有界；
- 同一互斥锁给取消、进度和结果交付提供可测试的线性化点。

代价与限制：

- 每次长操作增加 ticket、轮询和 ack 请求；
- 浏览器关闭或网络中断时 terminal 会保留到 TTL/容量回收；
- capability 在持有期内必须按 bearer secret 对待，但它不替代 SSH 访问控制；
- 丢失同步成功响应不能从 status 恢复 PNG；
- 该设计只关闭 validation 平台的多客户端错误取消和 HTTP 渐进进度问题，不证明 Windows WebView2、公开 Web 服务或地图合规。

## 被拒绝的方案

- 继续按 kind 取消当前任务：无法把取消绑定到发起标签页。
- 使用客户端生成 ID：服务器无法区分未授权自选 ID 与已签发 capability。
- 提供 current/list API：会扩大操作枚举和跨标签页干扰面。
- 把 operation ID 放在 GET URL：容易进入浏览历史和访问日志。
- 在 terminal 中保存双 PNG：增加大对象保留、恢复语义和内存 DoS 风险。
- 为内部平台增加账号、cookie session 或公网消息通道：超出 SSH 隧道 Alpha 的最小范围。
- 使用固定 `setInterval`：慢响应时会产生重叠轮询和乱序更新。

## 验收与记录

实现回归已覆盖 ticket kind/UUIDv4、busy 不消费、TTL/容量、exact-ID + family 取消、三类 tagged 状态白名单、sequence、ack、取消/finish/Drop 线性化，以及前端非重叠轮询、generation 隔离、有界 final/ack 和 3 秒取消 deadline。revision `867c25aeb2091055b56d1259f6ad7293d21f7495` 的实际结果为 Rust workspace offline `83 passed / 3 ignored`、真实 HTTPS `3/3`、validation server `17/17`、前端 `6 files / 41 tests`（backend 专项 20）和 Tauri 纯状态 `4/4`；fmt、clippy `-D warnings`、TypeScript check、Vite build、xwin、`bash -n`、`self-test` 与 `git diff --check` 均通过。

同一 revision 已完成 full managed build，`built_at=2026-07-26T19:02:43Z`，server SHA-256 为 `e80c8890ebcd2059341cd495e78546d51287a916776f0a1991e8d99f062afa0c`；stop/start 将 PID `1114524` 更新为 `1185566`，重复 start 后 PID 不变，health/bootstrap/cache readiness 通过，缓存前后均为 `133,071,416 bytes`、`partial=0`，两个区域各 `50/50 ready`。

更新后的 `validation-recovery-smoke.sh` 连续两次通过，均输出 `ticket_a_cancelled=true ticket_b_http=200 progress_a=2 progress_b=2`；`progress_a/progress_b` 是所观察 progress snapshot 的 sequence 值，不是快照数量。内部断言覆盖未知 ID/错 family 为 false、ID-B 忙时 409 且保持 reserved `sequence=0/progress=null`、ID-A correct cancel true 后 HTTP 422无 PNG、cancelled terminal 与 ack true/false/404、同一 ID-B 复用为 HTTP 200、旧 ID-A 不影响 ID-B、唯一可解码 `401×401` 双 PNG、succeeded terminal 无 PNG、ack/404，以及最终 gate/health、缓存不变和无临时目录残留。

这些证据只关闭受管回环 HTTP 的 operation identity 与白名单 progress 快照。通过 SSH 隧道确认浏览器中的可见逐阶段进度、取消屏障、重试和无控制台错误仍待执行；Windows 10/11 WebView2、十进制 2.5 GB 实体压力、GPU 整机重启和中国大陆地图合规也仍是独立门槛。
