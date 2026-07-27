# 发射点地面海拔与有界下载传输验证记录

- 日期：2026-07-27
- 主机：`gpu-273312`（`ubuntu@150.65.181.202`）
- 工作区：`/home/ubuntu/hamheatmap`
- 对应决策：ADR 0014、ADR 0015
- 状态：自动化、真实 HTTPS、受管构建/readiness、成都 DEM 自动/手动计算和 operation 恢复烟雾已通过；SSH 隧道浏览器视觉、Windows、受控弱网、实体配额/磁盘压力和地图合规待验

## 1. 验证范围

本轮验证两个独立但同时交付的切片：

1. 发射点地面海拔请求、PFL 首样点、schema 3 冻结结果与前端/导出状态；
2. HTTPS-only、零重定向、有限超时以及取消/读取错误/early EOF 的 partial 检查点。

验证不改变 ITM 版本、`land-water-v1`、固定 `401×401` / 200 km / 1 km 网格或中国大陆地图合规边界。私有平台仍只监听服务器回环并通过 SSH 隧道供内部访问；本报告中的 HTTP 烟雾不等于浏览器视觉或 Windows WebView2 验收。

## 2. 代码与网络门禁

当前切片完成：

| 检查 | 结果 |
|---|---:|
| Rust workspace | 95 passed |
| 前端 | 46 passed |
| 真实 GLO-90 HTTPS | 3/3 |
| Rustfmt / Clippy `-D warnings` | 通过 |
| TypeScript check / Vite build | 通过 |
| Windows xwin 目标检查 | 通过 |

定向测试覆盖：

- `txGroundElevationOverrideM` 缺失、`null`、手动值、边界与非有限拒绝；
- 手动模式仍校验中心 DEM，只替换 PFL 样点 0；
- schema 3 的 `txGroundElevationM` / `txGroundElevationSource`，bootstrap schema 2 保持不变；
- DEM 未返回时禁用手动来源并在 handler 层防御拒绝；
- 预设保留、新点重置、清空热力图保留及冻结导出；
- Agent HTTPS-only、零重定向、分阶段/总超时和 HEAD 200-only；
- 读取错误、early EOF、取消后的 partial 文件/SQLite/续传一致性。

真实 HTTPS 3/3 继续覆盖首次 DEM/WBM 下载、取消后强 ETag/Range 续传和 DEM/WBM 成对 404 纯海洋生成。它们证明固定真实来源仍可工作，不构成弱网超时注入或 Windows 网络栈证据。

## 3. 受管构建与运行身份

`scripts/validation-platform.sh build` 生成的 metadata：

| 字段 | 值 |
|---|---|
| revision | `2e4411de809d1f78b6dd1407d51a2351d58b02ed` |
| built_at | `2026-07-27T04:37:14Z` |
| frontend_mode | `validation-server` |
| listen | `127.0.0.1:1421` |
| server SHA-256 | `e8151b46aad3318abddbade68a465c8c04c9851a24166888f57b9cadebae78fa` |

受管进程 PID 为 `1301627`，身份检查确认只绑定 `127.0.0.1:1421`。管理脚本 `status` 与 `health` 通过；`/healthz` 返回 `{"status":"ok","schemaVersion":1}`。共享服务 bootstrap 成功且保持 schema 2，证明缓存锁、重启整理和配额 readiness 可用，而不只是 HTTP liveness。

本轮没有绑定公网、开放防火墙端口、增加反向代理或服务器端导出。

## 4. 成都 DEM 自动/手动真实计算

`scripts/validation-manual-elevation-smoke.sh` 对同一真实缓存和相同无线电输入执行两次完整 calculation：

| 参数 | 值 |
|---|---|
| 发射点 | `30.5°N, 103.5°E` |
| 频段 / 频率 | 144 MHz / `145.00 MHz` |
| 发射功率 | `25 W` |
| 发射天线 | `6 dBi`、`20 m AGL` |
| 接收天线 | `-3 dBi`、`1.5 m AGL` |
| 极化 | 垂直 |
| 自动请求 | `txGroundElevationOverrideM: null` |
| 手动请求 | `txGroundElevationOverrideM: 1500.0` |

结果：

| 检查 | DEM 自动 | 手动覆盖 |
|---|---:|---:|
| HTTP | 200 | 200 |
| calculation schema | 3 | 3 |
| `txGroundElevationSource` | `dem` | `manual` |
| `txGroundElevationM` | `526.3442993164062` | `1500.0` |
| 原始 PNG | 唯一、可解码、`401×401` | 唯一、可解码、`401×401` |
| EPSG:3857 overlay | 唯一、可解码、`401×401` | 唯一、可解码、`401×401` |

脚本对两个 PNG payload 分别计算 SHA-256，并断言自动/手动的原始 heatmap 哈希不同、overlay 哈希也不同；两项均通过。该结果证明手动值确实进入传播与地图显示产品，而不是只改变表单或报告元数据。脚本还要求每个 operation 至少发布 calculation progress、进入 succeeded terminal、terminal 不含 PNG、ack 后状态消失。

本轮只记录“哈希已发生变化”的断言，不补造未保留在摘要中的具体哈希字符串。

## 5. Operation 恢复与 schema 3 回归

更新后的 `scripts/validation-recovery-smoke.sh` 在同一受管构建上通过：

```text
ticket_a_cancelled=true ticket_b_http=200 progress_a=2 progress_b=2
```

脚本继续验证 exact-ID/family 取消、busy 不消费 reserved ticket、旧 ID 不影响后来 ID、terminal/ack 隔离、被取消请求无双 PNG、同票恢复成功和两个可解码 `401×401` PNG。本轮还显式断言恢复结果为 calculation schema 3、`txGroundElevationSource="dem"`，并具有有限且符合中心 DEM 的有效地面海拔；因此旧 schema 2 运行记录没有被错误复用于当前协议。

## 6. 缓存与清理不变量

本轮受管 build/smoke 前后：

- 缓存实际总量均为 `133,071,416 bytes`；
- partial 均为 `0`；
- 两个登记区域均为 `50/50 ready`；
- 没有残留 `validation-manual-elevation-smoke.*` 或 recovery smoke 临时目录；
- 最终服务仍为 PID `1301627`、回环监听且 healthy。

因此本轮计算、operation 取消/恢复与临时烟雾文件没有改变真实缓存内容。该数字不是十进制 2.5 GB 实体压力测试。

## 7. 仍未关闭

- [ ] 通过 SSH 隧道在浏览器确认 DEM/手动控件、真实两次热力图、进度、清空/新点行为、浅/深色布局和控制台无错误；
- [ ] 在可控弱网真实触发 DNS、连接、响应头/体与全局超时，量化取消延迟和用户重试续传；
- [ ] Windows 10/11 WebView2 的表单、下载/续传、计算、原生 PNG/PDF、文件路径和安装行为；
- [ ] 十进制 2.5 GB 实体缓存、真实磁盘不足、强制崩溃、GPU 整机重启与日志轮转压力；
- [ ] 合规中国大陆底图、审图号、署名、离线/导出授权和公开发布验收；
- [ ] 传播结果与手动站址海拔的外场测量校准。

Linux 自动化、真实 HTTPS、受管回环 HTTP 和交叉目标检查都不能外推为上述项目已通过。

## 8. 后续 partial 写失败加固（2026-07-27）

本报告原始 revision 的运行数字保持不变。后续代码审查补充关闭了两项残余：

1. 发射点海拔来源继续在 DEM 未返回时禁用并由 handler 拒绝；删除不可达的 `elevationM ?? 0`，新增合法 `0 m` DEM 精确进入手动覆盖的前端测试。
2. Range 下载在 `write_all` 部分成功后报错时，以受界文件游标尝试持久化实际 partial 长度。检查点成功才允许同进程续传；游标读取失败、越出当前块范围或检查点自身失败时仍返回原始写错误，并尝试把 partial 标记 corrupt、关闭后删除并重置 missing。即使标记与删除同时失败，重启也只保留 SQLite checkpoint，截掉文件额外尾部；文件更短或缺失则废弃。

故障注入从 4-byte 已可信偏移开始，先写 2 bytes 后返回固定 I/O 错误。成功检查点用例确认文件/SQLite 均为 6 bytes 且强 ETag/Range probe 从 6 续传；同步失败、游标读取失败和游标越界用例均确认原始错误不被掩盖、文件被删除、状态为 missing，重开 CacheStore 后仍从 0 开始。store 层另构造 DB checkpoint 小于、等于和大于文件长度的重启状态，确认只保留等长或截回的可信前缀。

后续门禁通过 Rust workspace `102 passed / 3 ignored`、cache `28 passed / 3 ignored`、真实 GLO-90 HTTPS `3/3`、前端 `7 files / 52 tests`、rustfmt、Clippy `--all-targets -D warnings`、TypeScript、Vite validation build 与 Windows x64 cargo-xwin workspace/all-targets check。

这仍不是实际 ENOSPC/EIO、Windows 文件系统或弱网压力证据；相应项目继续保留在第 7 节和测试计划第 24 节。

后续提交 `4042d0c0bd808b898de1556b9b047c9831922c0c` 已在受管平台完成 stop/build/start。构建时间为 `2026-07-27T07:02:51Z`，server SHA-256 为 `647547e576308d81e807e7b1b72aedb2e8d8778f235c1dbd3f521a77d8295ea5`；PID `1457203` 经管理身份检查和 `ss` 确认仅监听 `127.0.0.1:1421`，health schema 1、bootstrap schema 2 与 self-test 均通过。

同一构建上的 recovery smoke 继续得到 `ticket_a_cancelled=true ticket_b_http=200 progress_a=2 progress_b=2`。成都 progressive smoke 得到 2 张不同预览 PNG，最后完成计数 `123260 / 125628`、首帧 `5707 ms`、总耗时 `7176 ms`；终态仍为完整 calculation 结果。缓存保持 `133,071,416 bytes`、partial `0`，两个登记区域均为 `50/50 ready`。

这些运行结果只回归受管回环平台，没有把确定性 writer 故障注入外推为真实磁盘耗尽或 Windows 文件系统证据。

关闭“双清理失败后重启复活未知尾部”风险的提交 `93b96abd3a0c1c099870509bbe3711ef4bb6db95` 随后再次完成受管 stop/build/start，取代 `4042d0c` 成为当前运行代码。最终构建时间 `2026-07-27T07:15:45Z`，server SHA-256 为 `32bb5b05ddc18ca49d34f7b5d04fd48fe6f0f04099d7444e4b0ff7f8649efbbe`；PID `1468926` 经 `ss` 确认仅监听 `127.0.0.1:1421`。

最终构建的 health schema 1、bootstrap schema 2、self-test、recovery smoke 和 progressive smoke 均通过。渐进烟雾得到 2 张不同预览，最后完成 `120400 / 125628`，首帧 `5452 ms`、总耗时 `7041 ms`；缓存仍为 `133,071,416 bytes`、partial `0`，两个区域各 `50/50 ready`。

后续纯文档提交不会重建二进制，验证平台 metadata 继续精确记录实际运行的 `93b96ab` 代码 revision。
