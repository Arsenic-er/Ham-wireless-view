# HamHeatmap Phase 2 桌面首切片验证报告

- 日期：2026-07-16
- 状态：React 界面、共享 Rust 桌面契约和真实缓存计算通过；尚非 Windows 可发布构建
- 主机：JAIST `gpu-753856`，Ubuntu 22.04.3 LTS，x86_64

## 1. 本轮目标

在不引入不合规边界或伪造传播结果的前提下，把已经验证的命令行计算核心推进为第一版桌面交互：

```text
空白 WGS84 地图点选
  → 200 km 圆与坐标/Maidenhead
  → 新手预设、频率、功率、增益、高度、极化
  → Rust 缓存检查
  → 可取消 ITM 计算与进度事件
  → 内存 PNG 热力图
```

本轮不实现区域下载确认/删除、正式合规底图、导出或安装包。

## 2. 实现结构

### 2.1 前端

`app/` 已包含：

- React 19.2.7、TypeScript 7.0.2、Vite 8.1.4、MapLibre GL JS 5.24.0。
- 固定 Node.js 24.18.0 LTS、SHA-256 校验安装脚本和精确 `package-lock.json`。
- 无网络资源、无行政边界、无高程表达的 WGS-84 开发画布。
- 单击发射点、固定 200 km WGS-84 圆、十进制度坐标和六字符 Maidenhead。
- 基地台→手台、手台→基地台预设；144/430 频段和两位小数频率。
- W/dBm、dBi/dBd、发射/接收高度和水平/垂直极化输入。
- `system/light/dark` 主题；固定红橙黄绿青蓝 dBm 色标不随主题改变。
- 缓存配额概览、计算/取消/清空状态区和固定操作栏。
- 明确的“内部测试底图，不得公开发布”横幅。

浏览器模式只用于界面检查：点选后显示桌面后端未连接，计算按钮保持不可用，不生成模拟热力图。

### 2.2 共享 Rust 服务

新建 `hamheatmap-app-service`：

- serde IPC schema version 1。
- 后端再次验证频段、两位小数、功率、增益、高度、极化和坐标。
- W/dBm、dBi/dBd 只在 Rust 归一化一次。
- 点选检查规划 DEM/WBM 数量、ready 状态、完整性、中心高程和全局配额。
- 只解码中心 DEM 瓦片显示海拔，不在每次点选时解码全部区域；计算时才加载完整区域。
- 点选检查不写入新的 region 记录；只有缓存准备或实际计算才注册区域。
- 完整计算返回 401×401 PNG data URL、四角坐标、模型版本和统计摘要，不持久保存会话结果。

### 2.3 取消与进度

覆盖引擎新增兼容旧 API 的 `compute_coverage_with_control`：

- 预先取消时在生成/采样前退出。
- 每个接收点开始前检查取消。
- 长剖面每 64 个约 90 m 样本检查取消。
- 每约 1% 有效像素发送一次原子进度。
- worker 返回 `CoverageError::Cancelled`，不会编码半成品。

Tauri 壳层只持有数据根、单任务互斥状态与 `AtomicBool`；阻塞任务结束、失败或 join 错误后恢复可计算状态。

## 3. 地图与合规控制

- 当前画布只有背景、经纬网、发射点、200 km 圆和计算热力图，不包含国界、省界、海岸线、水系或地名。
- 没有 hillshade、contour、terrain、slope、DEM raster 或 3D terrain。
- 热力图 raster 没有 hover/click 监听，不存在像素 dBm 查询接口。
- WBM 和 DEM 文件只由 Rust 读取，前端无法访问。
- 正式 `CompliantBasemapProvider`、中国大陆有效区、审图号、离线/导出授权仍是发布 P0 阻断项。

当前热力图通过 MapLibre image 四角放置。四角由 WGS-84 固定网格的 `±200 km` 对角计算，但 MapLibre 内部图像插值尚未完成全幅误差量化；发布前必须验证，必要时在 Rust 侧重投影。

## 4. 自动化结果

### 4.1 前端

```text
TypeScript check: passed
Vitest: 2 files, 8 tests passed
Vite production build: passed
```

测试覆盖：

- Vincenty/WGS-84 200 km 闭合圆与四个图像角。
- 基本方位方向。
- Maidenhead。
- 两个场景预设。
- 144/430 默认频率。
- W↔dBm、dBi↔dBd 往返。
- 频段与输入范围拒绝。

生产构建结果：`index.html 0.53 kB`、CSS `83.53 kB`、JS `1,242.91 kB`，gzip 后约 `0.35 / 13.46 / 340.99 kB`。JS 主要包含离线 MapLibre 运行时，首版桌面包可接受；后续仍可延迟加载地图模块。构建与测试脚本显式使用 `vite.config.ts`，避免 TypeScript 历史产物遮蔽正式配置。

### 4.2 Rust

- 全工作区默认测试：33 passed、0 failed、3 个显式网络测试 ignored。
- 新 `app-service`：5 个请求/单位/几何/配额测试。
- 覆盖引擎：新增预取消与运行中回调取消测试。
- 官方 NTIA v1.4 回归保持通过。
- `cargo fmt --check` 与 Clippy `-D warnings` 纳入最终质量门。

## 5. 真实缓存桌面服务 smoke

命令：

```bash
scripts/cargo-project.sh run --release --locked \
  -p hamheatmap-app-service --example cached_calculation_smoke -- data
```

成都 `(30.5°N, 103.5°E)` 已缓存区域结果：

| 指标 | 结果 |
|---|---:|
| 有效接收像素 | 125,628 |
| 模型 | NTIA ITM / `land-water-v1` |
| 平均接收功率 | -146.670 dBm |
| 数据校验、载入、计算和编码 | 9.752 s |
| PNG data URL 长度 | 224,378 bytes |

数据 URL 以 PNG 标准签名的 Base64 前缀开始。该 smoke 直接调用 Tauri 使用的共享服务，不经过旧 `validate` CLI 的诊断场景包装。

## 6. 浏览器视觉验收

使用真实 Vite 构建在默认窗口和 Tauri 最小窗口 `1080×700` 检查：

- 深色与浅色主题均无文本反色、不可见控件或色标变化。
- 地图中央点选后移除空状态，显示发射点、Maidenhead 与虚线 200 km 圆。
- 两个预设正确交换功率、增益和天线高度。
- 430 MHz 频段把默认具体频率改为 435 MHz。
- 缓存弹窗显示十进制 2.50 GB 上限及 DEM/WBM/partial/metadata 分类。
- 右侧参数可滚动，底部状态与操作按钮保持固定。
- 1080 px 宽度下色标说明最初发生溢出，扩大响应式断点至 1180 px 后复查通过。
- 浏览器控制台：0 error、0 warning。

## 7. 尚未关闭

- JAIST 没有 Windows MSVC `lib.exe`；Linux 交叉检查在 `ring` 构建脚本处按预期停止，不能作为 Tauri 源码已在 Windows 编译的证据。
- 必须在 Windows 10/11 安装 MSVC C++ Build Tools、WebView2 和 Rust MSVC target 后运行 `cargo check`、开发启动和安装包测试。
- 区域数据缺失时，目前只显示缺失状态；尚未接入预计大小、用户确认、下载进度/取消和删除。
- 合规有效区、合规底图、审图号和离线/导出授权未接入。
- 热力图四角放置的全幅几何误差、渐进显示、PNG/PDF 导出未验收。
- 发射点手动海拔覆盖、缓存区域列表和设置页尚未实现。

## 8. 结论

桌面 UI 已不再是静态草图：地图、参数、主题、缓存状态和任务控制具有可运行代码，并通过共享 Rust 契约完成了一次真实缓存传播计算。下一切片应优先接入“缺数据→预计量→用户确认→下载/取消→ready”的完整状态机，然后进行 Windows 实机构建；合规底图仍保持独立 P0 门槛。

后续状态：上述下载与缓存管理切片已完成，验证记录见 `10-phase2-download-cache-slice.md`。本报告保留为首切片当时的历史验收记录。
