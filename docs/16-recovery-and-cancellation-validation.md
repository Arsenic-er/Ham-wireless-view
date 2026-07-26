# HamHeatmap 恢复与取消验证记录

- 日期：2026-07-27
- 主机：`gpu-273312`（`ubuntu@150.65.181.202`）
- 工作区：`/home/ubuntu/hamheatmap`
- 范围：缓存重启整理、2.5 GB 边界不变量、计算取消结果交付、私有平台管理状态
- 状态：代码与自动化测试通过；真实 HTTP 取消、加固版平台重建/重启和真实 2.5 GB 压力待验证

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

取消只设置同类型 active 操作的标志，不会提前释放门闩。因此旧 worker 真正结束前，新操作仍应被拒绝或由官方 UI 保持不可重试状态。`AppService` 还在传播完成、两张 PNG 编码、Base64 转换和最终结构交付前设置检查点。

### 3.2 前端结果卫生

官方单窗口 UI 的回归测试从已有热力图开始发起重算并取消，确认：

- 旧 heatmap 被清除；
- 导出立即保持禁用；
- 已取消 promise 结束前不把旧成功结果恢复；
- 操作收尾后可重新计算并显示新结果。

当前 HTTP 取消请求没有 operation ID，也没有把取消请求绑定到发起该计算的浏览器实例。因此不能把上述结果外推为多标签页/多客户端安全；该场景需要未来在协议中加入不可猜测的 operation ID 或等价所有权令牌。

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

离线 workspace 的 77 项由 app-service 12、cache 21、coverage 15、export 6、propagation 6、official reference 1、terrain 5、validation server 11 构成。3 项真实网络测试单列为 ignored，不重复计入 77。

## 7. 待完成的运行验收

以下项目在本文件当前版本中没有通过声明：

- [ ] 通过 SSH 隧道发起真实 `/api/calculate`，计算进行中 POST `/api/cancel-calculation`；
- [ ] 确认取消响应被接受、计算请求不返回双 PNG，界面不显示或导出半成品；
- [ ] 取消完成后相同输入可重新计算，并与既有确定性结果一致；
- [ ] 对加固版执行 `stop → build → start → status → health → bootstrap`；
- [ ] 验证重复 start 拒绝/幂等和严格 stop，不向无关 PID 发送信号；
- [ ] 模拟服务器重启后的手动恢复并核对缓存 overview；
- [ ] 使用真实十进制 2.5 GB 数据执行 exact-cap、over-cap、磁盘不足和故障注入；
- [ ] 增加 HTTP 渐进进度；
- [ ] 完成 Windows 10/11 WebView2、安装、导出和地图合规验收。

只有在命令、响应、日志和输出哈希被补充到本节后，相关运行项才能从待办改为通过。
