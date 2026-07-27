# 渐进式传播覆盖预览验证记录

- 日期：2026-07-27
- 功能提交：`a1219c5ca3254a2a40a50829526cd9bd062d8ea9`（`Add-progressive-coverage-previews`）
- 烟雾脚本竞态修复：`88204765182de7e842859e672050614c091f1986`
- 平台：`gpu-273312`，`/home/ubuntu/hamheatmap`
- 状态：Linux 自动化、受管回环 HTTP 和 Windows x64 交叉构建通过；Windows WebView2/Tauri Channel 实机待验

## 1. 验证范围

本切片验证以下完整路径：

```text
Coverage worker 像素批次
  → AppService NaN 累计栅格与单线程编码
  → CalculationPreview schema 1
  ├─ Tauri invoke Channel 契约
  └─ validation exact-ID/latest-only HTTP
       → React 临时状态
       → MapLibre Blob URL 覆盖层
       → 最终 CalculationResult 替换
```

预览只是一张活动任务期间的 EPSG:3857 地图覆盖层。最终 schema 3 结果仍单独生成原始报告 PNG 和地图 overlay PNG，且是唯一可导出的权威结果。

本记录不验证合规中国大陆底图，也不把服务器交叉构建等同于 Windows 10/11 实机运行。

## 2. 自动化结果

### 2.1 Rust

| 范围 | 结果 |
|---|---:|
| workspace 离线测试 | 100 passed / 3 ignored |
| 真实 GLO-90 HTTPS ignored tests | 3/3 passed |
| coverage 专项 | 20 passed |
| app-service 专项 | 17 passed |
| validation-server 专项 | 19 passed |
| rustfmt | passed |
| Clippy workspace `--all-targets -D warnings` | passed |

coverage、app-service 和 validation-server 数量包含在 workspace 总数中，不应再次相加。专项覆盖：

- 批次最多 64 个像素、栅格索引合法、进度严格递增；
- 有批次/无批次的最终 `CoverageGrid` 完全一致；
- NaN 未完成区、约 5% 信号阈值、容量 1 合并、单编码线程和 800 ms 最短间隔；
- preview sequence/完成数递增、100% 不发送、最终结果不受预览影响；
- exact-ID `/api/operation-preview` 的 200/204、latest-only、生命周期清理和 status/terminal 隔离；
- 取消、成功、失败与 lease Drop 后预览不再可取。

### 2.2 前端与 Tauri 契约

| 范围 | 结果 |
|---|---:|
| Vitest | 7 files / 51 tests passed |
| TypeScript | passed |
| Vite validation build | passed |
| Tauri Rust target / full xwin build | passed |

前端测试覆盖 status→preview 串行非重叠轮询、`afterSequence`、200/204、exact ID/generation 隔离、旧响应抑制、最终替换、不可导出以及 Blob URL 复用/释放。

Tauri 测试通过真实 JavaScript `Channel` 对象的 mockIPC 契约，确认参数名 `previewChannel` 与调用期 handler；Rust 命令接收 `preview_channel: Channel<CalculationPreview>`。这些是契约和构建证据，不是 Windows WebView2 的实际跨进程消息证据。

## 3. Windows x64 服务器产物

产物全部在服务器生成，没有复制到本机：

| 产物 | 字节 | SHA-256 |
|---|---:|---|
| `app/src-tauri/target/x86_64-pc-windows-msvc/release/HamHeatmap.exe` | 16,038,912 | `88503ab822968bfd9ac42604a4d778839ebbe8dc5b3cf4126d12e69f5905ab2a` |
| `app/src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis/HamHeatmap_0.1.0_x64-setup.exe` | 211,379,116 | `933d10f03ad0adb94a73a7eab4e4f14e94c3fcc2cf47139bff1eb72243e20444` |

链接阶段只有缺少第三方 PDB 的 LNK4099 警告和实验性交叉宿主提示，构建成功。尚未在 Windows 10/11 启动这些文件，因而没有验证安装/卸载、WebView2↔Rust Channel、原生保存或真实地图连续刷新。

## 4. 受管部署

功能提交完成 full build 后，以 validation 管理脚本 stop/build/start：

| 字段 | 值 |
|---|---|
| build revision | `a1219c5ca3254a2a40a50829526cd9bd062d8ea9` |
| built_at | `2026-07-27T05:48:52Z` |
| server SHA-256 | `03bb62e9bc4facdba01c1693fbf2a63ab70d961606a09cfed6fe9b128c845bd2` |
| 当前记录 PID | `1403529` |
| bind | `127.0.0.1:1421` |
| frontend mode | `validation-server` |

`status`、`health`、`self-test`、bootstrap 与 cache overview 均通过。端口没有公网绑定。

后续 `8820476` 只修复烟雾脚本观察 curl 完成状态的竞态，没有修改或重建服务端二进制。因此“被测应用 revision”与“最终测试工具 revision”必须分开记录。

## 5. 成都真实渐进预览

使用已有成都缓存和 145.00 MHz、25 W、6 dBi、20 m 发射端、-3 dBi、1.5 m 接收端、垂直极化的真实请求。两次 `scripts/validation-progressive-preview-smoke.sh` 均通过：

| 指标 | 运行 1 | 运行 2 |
|---|---:|---:|
| 预览帧数 | 2 | 2 |
| 唯一 PNG 数 | 2 | 2 |
| 最后一帧完成像素 | 123,410 | 121,808 |
| 总有效像素 | 125,628 | 125,628 |
| 总耗时 | 7,246 ms | 7,301 ms |
| 首帧时间 | 5,610 ms | 5,660 ms |
| 两帧间隔 | 1,086 ms | 1,244 ms |
| 最大 preview JSON | 181,698 bytes | 181,326 bytes |

每帧都满足：

- `schemaVersion=1`；
- `0 < completedPixelCount < totalPixelCount`；
- sequence 与完成像素数严格递增；
- `EPSG:3857`、`401×401`、四角有限；
- Data URL 唯一，Base64 可解码；
- PNG signature 和 IHDR `401×401` 正确；
- 相邻帧 PNG SHA-256 不同。

权威响应为 HTTP 200、schema 3，并包含各自唯一且有效的原始 heatmap PNG 与 EPSG:3857 overlay PNG；响应不包含 preview 字段。完成后 `operation-preview` 返回 204，terminal status 不含 PNG/Data URL，ack 后 status 返回 404。

预览首帧约在总耗时的 77% 后出现。它证明真实部分覆盖已经成功传输和渲染所需的契约成立，但也表明“尽早反馈”仍有优化空间，不应表述为固定每 800 ms 更新。

## 6. 资源、恢复与测试工具

运行前后缓存均为：

- 总量 `133,071,416 bytes`；
- `partial=0`；
- 两个区域各 `50/50 ready`。

干净重启后的服务进程基线为 `VmHWM=2,920 KiB`、`VmRSS=2,920 KiB`；第二次真实计算后为 `VmHWM=195,484 KiB`、`VmRSS=20,200 KiB`。这是整个 Linux validation server 的进程高水位，不是预览功能的增量内存，也不能外推到 Windows。

适配 schema 3 的 `validation-recovery-smoke.sh` 随后连续两次通过，每次输出：

```text
validation recovery smoke passed: ticket_a_cancelled=true ticket_b_http=200 progress_a=2 progress_b=2
```

第一次 progressive smoke 曾因 curl 子进程恰好结束、shell job/进程归属检查先于 HTTP 状态文件观察而失败。应用服务保持健康、缓存不变且无临时目录残留；这不是产品计算失败。`8820476` 修复为：观察到子进程停止后仍 `wait` 原子进程，并硬性校验 curl 退出码、状态文件非空和 HTTP 200。修复后两次真实运行通过。

## 7. 仍未关闭

- [ ] Windows 10/11 WebView2↔Rust Channel 实机消息传输；
- [ ] Windows 上连续 MapLibre 更新、取消后迟到 Channel 抑制与内存；
- [ ] SSH 隧道浏览器中的用户可见渐进过程、取消/重试和控制台；
- [ ] 更多 CPU 档位的首帧时间、PNG 编码占比和预览增量 RSS；
- [ ] 十进制 2.5 GB 实体缓存、磁盘不足、弱网和强制崩溃；
- [ ] 中国大陆合规底图、审图号、桌面离线/导出授权；
- [ ] 外场实测校准。

结论：渐进预览的计算、应用服务、validation exact-ID 传输、前端生命周期和 Tauri Channel 契约已完成自动化验证，真实成都受管回环计算稳定观察到两张不同的部分覆盖层。桌面端实际运行和地图合规仍是独立发布门槛。
