# ADR-0016：有界渐进覆盖预览与双传输通道

- 日期：2026-07-27
- 状态：已采纳；Linux 自动化与受管回环 HTTP 已验证，Windows Tauri Channel 实机待验
- 补充：ADR-0007 的薄 Tauri 壳、ADR-0011 的 EPSG:3857 覆盖层和 ADR-0013 的 exact operation capability

## 背景

一次 200 km、1 km 像素的真实传播计算包含约 12.6 万个 ITM 接收点。最终结果此前只在全部像素完成并编码双 PNG 后出现，用户虽然能看到百分比，却不能确认地形差异已经真实进入地图。直接把 worker 的浮点栅格、每批 PNG 或最终结果放入全局状态，会增加并发编码、内存、IPC 和生命周期竞态。

桌面 Tauri 与私有 validation 浏览器具有不同传输边界：桌面 invoke 可以使用调用级 IPC Channel；validation 平台只能经回环 HTTP 和 exact operation capability 工作。两者必须共享同一预览契约，同时保持最终 `CalculationResult` 是唯一权威、可导出的结果。

## 决策

### 1. 传播核心只提供可选像素批次

coverage 层提供 `compute_coverage_with_pixel_batches`。worker 可并发调用批次回调，每批最多 64 个 `CoveragePixel`，只包含栅格索引和已计算 dBm。批次切片只在回调期间有效；调用方必须立即复制或合并。

最终 `CoverageGrid` 仍由原有规范结果路径汇总。批次全部合并后的栅格必须与无批次入口完全一致；未选择预览的既有 API 不建立累计栅格，也不编码中间 PNG。批次回调可能先于相应进度计数，因此预览完成数和 status progress 只要求各自单调，不要求瞬时相等。

### 2. 有界单编码管线

应用服务为一次计算建立全 NaN 的 `401×401` 累计栅格。worker 只在短临界区合并已完成像素，不执行 PNG 编码。

- 每跨约 5% 有效像素尝试发送一次信号；
- 信号使用容量 1 的同步通道和非阻塞发送，积压只保留“需要重新取最新快照”这一事实；
- 只有一个编码线程读取最新栅格；
- 两次编码开始之间至少间隔 800 ms；
- 未完成的 NaN 像素通过既有 EPSG:3857 覆盖层编码保持透明；
- 100% 预览不发送，由最终 schema 3 结果表示；
- 编码或预览传输失败只丢弃该帧，不使传播计算失败。

因此预览是 best-effort。足够快的计算、早期取消或 transport 关闭可以产生零帧。

### 3. 临时 schema 与权威结果分离

`CalculationPreview` 使用独立 schema 1，只含：

- 严格递增的 preview `sequence`；
- 完成/总像素数；
- EPSG:3857、`401×401` 和四角；
- 一张 MapLibre 覆盖层 PNG Data URL。

它不含原始本地方位投影报告 PNG、统计摘要、冻结导出快照或缓存身份，不能导出、持久化、恢复或进入 terminal。最终 `CalculationResult` 仍是唯一权威结果。

### 4. 两种有界传输

Tauri 的每次 `calculate` invoke 接收独立 `tauri::ipc::Channel<CalculationPreview>`。进度继续走 `calculation-progress` 事件；预览不使用全局事件。invoke settle 后前端关闭 handler，并以 schema、投影和 sequence 过滤迟到/无效消息。

validation 平台提供严格 POST JSON：

```text
POST /api/operation-preview
{"operationId":"…","afterSequence":N}
```

只有 exact ID 的活动 calculation 且最新 preview sequence 大于 `afterSequence` 时返回 200；未知但格式有效的 ID、reserved/非 calculation、没有更新、取消中或终态返回 204。无效 JSON、未知字段、错误媒体类型和无效 ID 格式按 API 错误返回。服务端只保存活动任务的最新一帧；取消、成功、失败或 lease Drop 都清除它。status 和 terminal 继续不含 PNG。

preview sequence 与 operation-status sequence 相互独立。validation 前端每轮先请求 status、再请求 preview，轮次不重叠，并同时校验 exact ID、本地 generation、当前 handle 和 preview sequence。

### 5. UI 生命周期与资源释放

React 分开保存临时 `preview` 和权威 `result`。地图优先显示最终结果，否则显示当前预览；导出只读取最终结果。

开始新计算、选择新点、修改参数、取消、失败和清空都会清除或抑制旧预览。成功响应以最终结果替换预览。MapLibre 对未变化的 data URL 复用 Blob URL，并在替换、清空和组件卸载时撤销旧 URL。

## 结果

优点：

- 真实模型计算尚未结束时即可看到地形相关的部分覆盖；
- worker 不执行压缩，编码并发恒为 1，信号积压恒为 1；
- 两种运行平台共享一个严格 schema，而 transport 生命周期各自受控；
- 临时图层不能冒充可导出的最终结果；
- status/terminal 仍保持无结果、无 PNG 的小型白名单协议。

代价与限制：

- 并发 worker 的完成顺序会形成块状或不均匀填充，不保证径向推进；
- PNG 与 Base64 增加 CPU 和瞬时内存；现有 Linux 进程高水位不是预览增量开销基准；
- 成都当前首帧约在总耗时的 77% 后出现，功能成立但早期反馈仍可优化；
- 极快计算可能直接显示最终结果；
- validation capability 持有者可在活动期读取预览，因此端点必须继续受同源回环、POST body 和 SSH 隧道边界约束；
- Windows WebView2 与 Rust Channel 的实际消息传输、连续图层刷新和取消迟到消息仍未实机验证。

## 未采用方案

- 每像素或每 64 像素批次编码 PNG：编码频率和竞争不可控。
- 把预览塞进 operation-status 或 terminal：放大高频状态协议并使结果跨越终态生命周期。
- 为服务器建立可恢复预览队列：与 latest-only 临时 UX 目标不符，并增加配额和清理问题。
- 使用全局 Tauri 预览事件：旧任务消息更容易污染后来 invoke。
- 把 float dBm 栅格暴露给 React：扩大 IPC、像素检查和非权威数据误用面。
- 允许导出预览或把最后预览当最终结果：破坏完整性、冻结参数和统计语义。

## 验收证据与未关闭项

自动化覆盖批次/最终一致性、单调与节流、严格 schema、Tauri Channel 契约、validation exact/latest-only 端点、UI 清理和 Blob URL 生命周期。成都真实缓存的两次受管烟雾均观察到两张 sequence 与 PNG 内容不同的部分覆盖层；完整记录见 `../18-progressive-coverage-preview-validation.md`。

仍需：

- [ ] Windows 10/11 WebView2 与 Rust Channel 实机传输；
- [ ] Windows 上连续 MapLibre 图层替换、取消迟到消息和进程内存；
- [ ] 通过 SSH 隧道进行用户可见浏览器交互与控制台验收；
- [ ] 中国大陆合规底图、审图号、离线/导出授权；
- [ ] 在更多 CPU 档位量化首帧时间与预览增量资源开销。
