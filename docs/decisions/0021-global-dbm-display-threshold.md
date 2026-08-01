# ADR 0021：已完成覆盖层使用 u8 场强 bins 进行全局显示筛选

- 状态：已接受
- 日期：2026-08-01
- 范围：React/MapLibre 已完成覆盖层与 CalculationResult 契约
- 不改变：ITM 计算、渐进预览、统计、计算缓存、诊断 PNG/PDF 和单像素不可检查原则

## 背景

用户希望在底部固定色标上拖动一个“游标卡尺”式控件。例如阈值设为 `-120 dBm` 时，地图只保留预测接收功率 `>= -120 dBm` 的区域，更弱像素在拖动过程中动态透明。当前会话可叠加最多 8 个 `401×401` 已完成结果，因此实现既要保持固定 dBm 语义，也要避免每个 input 事件重新运行传播、重新编码 PNG 或重建 MapLibre 图层。

既有最终结果只把 EPSG:3857 dBm 栅格渲染成 RGBA PNG。直接从 RGB 反推 dBm 会把显示调色板和筛选契约绑在一起，并受到颜色量化影响；把完整 float32 栅格发送到前端则使有效载荷增至 4 倍，并扩大未来像素查询误用的表面。

## 决策

### 1. 产品语义

- 提供一个全局整数阈值，范围 `-140..-60 dBm`、步长 1 dB、默认 `-140 dBm`。
- 对所有已完成会话覆盖层，像素仅在 `received_power_dbm >= threshold` 时保留原 PNG 的 RGBA；其他像素 alpha 置 0。
- 该值是短暂的地图显示状态，不持久化。选择新点、参数变化、新计算和底图切换保留它；“清空热力图”删除覆盖层但保留它；应用新启动恢复 `-140 dBm`。
- 渐进预览不跟随筛选。统计、模型输出、缓存键、权威结果和 PNG/PDF 导出保持完整未筛选语义。

### 2. `u8-dbm-floor-v1` 契约

`CalculationResult` 升级为 schema 4，新增：

```text
mapOverlayFilterEncoding = "u8-dbm-floor-v1"
mapOverlayFilterBase64   = Base64(width × height bytes)
```

每个 byte 与 `mapOverlayPngDataUrl` 的像素顺序和尺寸完全一致：

- 0：非有限、圆外或 `< -140 dBm`，即原 PNG 已透明。
- 1..80：`floor(received_power_dbm) + 141`，对应 `-140..-61 dBm`。
- 81：`>= -60 dBm`。

对整数阈值 `t`，cutoff 为 `t + 141`，当且仅当 `bin >= cutoff` 时保留原 alpha。使用 floor 而非 round，保证支持范围内每个整数阈值的 `>=` 判断在边界两侧准确。前端必须验证固定 encoding、Base64、解码长度和 PNG 尺寸；任何不一致都 fail closed。

这个编码只表示筛选顺序，不能恢复小数 dBm，也不得用于 hover/click 查询或公开原始传播栅格。bootstrap 保持 schema 2，`CalculationPreview` 保持 schema 1。

### 3. CanvasSource 与帧调度

每个已完成结果首次出现时，前端只解码一次 PNG，并保留：

- 原始 RGBA；
- 解码后的 `Uint8Array` bins；
- 一个与结果尺寸一致的 canvas；
- 一个 `animate:false` 的 MapLibre CanvasSource。

阈值变化只线性扫描 RGBA alpha：原透明像素保持透明，bin 低于 cutoff 的像素置透明，其余恢复原 alpha。RGB 永远不改。更新后只请求一次 CanvasSource 纹理上传，不移除/新增 source 或 layer，也不重设 camera。

连续 `input` 使用 `requestAnimationFrame` 合并，并要求两次实际 alpha 更新至少相隔 33 ms，从而最多 30 fps。相同整数值不重绘；若多个输入落在同一等待窗口，仅绘制最新值，旧回调不能覆盖最新阈值。清空、淘汰第 9 层、同点替换和组件卸载都释放对应画布/图像引用；style 暂不可用时沿用 desired-state 重放。

单张 bins 是 `401 × 401 = 160,801 bytes`；8 层原始 bins 合计约 1.29 MB。8 层原始 RGBA 合计约 5.15 MB，未计 canvas/WebGL 和 PNG 对象开销。该规模允许在不发送 float32 栅格的前提下进行桌面端线性 alpha 更新，但“无明显卡顿”必须由服务器微基准、受管真实浏览器和 Windows 实机分别验证，不能从复杂度推断。

## 未采用方案

### 每次拖动重新计算或请求后端图片

会引入约数秒级传播计算或持续 IPC/HTTP/PNG 编码，无法满足直接操控，也会给取消和结果身份增加无意义状态，因此拒绝。

### 从 PNG RGB 反解 dBm

现有连续调色板大致可逆，但量化、alpha、未来颜色调整和图像解码会让筛选语义不再是模型值的稳定契约，因此拒绝。

### 传输 float32 Web Mercator 栅格

能保留小数值，但负载是 u8 bins 的 4 倍，并为本项目明确禁止的单像素数值检查提供了不必要的数据面。当前阈值只有整数步长，u8 已足够，因此拒绝。

### MapLibre 自定义 shader 图层

可以把比较移到 GPU，但会引入自定义 WebGL 生命周期、上下文恢复、主题/样式重放和导出风险。401×401、最多 8 层的 alpha 扫描更简单且可测试；只有 Windows 实机证明 CanvasSource 无法达到交互目标时才重新评估。

## 后果

优点：

- 阈值与真实重采样 dBm 值一致，不依赖调色板反推。
- 拖动不触发传播、网络、PNG 编码或图层重建。
- 额外结果数据固定且远小于 float32。
- 不改变预览、统计和导出的既有权威边界。

代价与门槛：

- CalculationResult 从 schema 3 升级到 4，所有 Rust/TypeScript/HTTP/Tauri 契约测试和真实烟雾必须更新。
- 每个最终层保留 RGBA、bins、canvas 和 GPU 纹理；8 层内存与 30 fps 最坏路径必须实测。
- 渐进预览在计算中不跟随阈值，界面须避免暗示它已经筛选。
- 本 ADR 不实现或授权离线地图包。首个 Windows Alpha Release 仍不含离线地图；后续只能引入具有正式授权、不可变 manifest/校验和、可删除且完整计入十进制 2.5 GB 的资产，当前四省内部 PMTiles 必须排除。
