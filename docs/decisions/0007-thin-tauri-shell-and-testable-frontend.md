# ADR-0007：薄 Tauri 壳层与可独立验收的桌面前端

- 状态：Accepted
- 日期：2026-07-16

## 背景

产品需要 Windows 10/11 桌面体验、离线缓存、长时间本地计算、取消和进度，同时 JAIST 主开发机是没有 WebKitGTK 开发库和 MSVC 的 Linux 服务器。如果把输入换算、缓存判断或传播逻辑写入 React，浏览器预览与正式桌面运行会出现两套模型；如果所有界面只能在 Tauri WebView 中启动，又无法在当前环境持续做布局、主题和交互回归。

## 决策

1. `app/` 使用 React、TypeScript、Vite 和 MapLibre，能够作为纯前端开发服务器启动。浏览器模式只执行界面状态与视觉检查，显示“内部测试底图，不得公开发布”，并明确拒绝传播计算；不得生成模拟热力图冒充模型输出。
2. 新建共享 `hamheatmap-app-service` Rust crate，集中定义 serde IPC 类型、输入校验、W/dBm 与 dBi/dBd 归一化、缓存检查、DEM/WBM 加载、覆盖计算、PNG 编码和结果统计。
3. `app/src-tauri/` 保持薄壳，只负责：解析应用数据目录、管理单任务/取消状态、把阻塞计算放入 worker、发送进度事件和注册 IPC 命令。Tauri 不复制模型常量或链路预算。
4. Tauri 应用数据根直接作为 `CacheStore` 根目录，使 SQLite、锁、partial、DEM、WBM 及以后加入的底图/计算缓存共同受十进制 2,500,000,000-byte 上限约束。
5. 计算期间只允许一个任务。`AtomicBool` 取消令牌由 Tauri 状态持有，覆盖 worker 在每个接收点开始前和长剖面每 64 个样本检查；失败、取消和 worker join 完成后都释放任务状态和活动缓存区域。
6. 进度使用 `calculation-progress` 事件，阶段为 `loading-data`、`computing`、`encoding`、`complete`。计算阶段约每 1% 有效像素通知一次，避免逐像素 IPC。
7. 热力图 PNG 在内存编码并以 data URL 返回，不把会话结果写入持久缓存。当前 MapLibre `image` source 以 WGS-84 固定网格四个角定位，属于内部切片；发布前必须量化整幅图误差，超过 1 km 容差时改为 Rust 侧 Web Mercator 重投影。
8. 开发地图使用无网络资源、无行政边界、无山体表达的 WGS-84 坐标网格。正式合规底图和中国大陆有效区由未来 `CompliantBasemapProvider` 接入，不能从 WBM、DEM 或国际边界数据推导。
9. Node.js 固定为项目内 24.18.0 LTS；前端依赖使用精确版本和 `package-lock.json`。Tauri Rust 依赖在独立 `app/src-tauri/Cargo.lock` 中固定，避免 Linux 工作区测试被平台 GUI 依赖阻断。

## 结果

- 同一 Rust 服务可由 Tauri、命令行 smoke 和单元测试调用，传播结果不会因前端框架变化而分叉。
- React 界面可以在 JAIST 上完成类型、单元、生产构建和浏览器视觉回归；正式桌面仍必须在 Windows MSVC/WebView2 环境编译和验收。
- 取消与进度成为覆盖引擎的可测试能力，而不是仅改变按钮文字。
- 浏览器预览不能测试应用数据目录、Tauri IPC、WebView2、Windows 文件锁、安装包或真实计算按钮状态，因此不能代替 Windows E2E。
- 内存 PNG 约数百 KiB，首版 IPC 可接受；若未来引入渐进热力图，应改用 Tauri channel/binary 数据，不把每个像素序列化为 JSON 数组。

## 官方依据

- Tauri 2 prerequisites：https://v2.tauri.app/start/prerequisites/
- Tauri command/IPC documentation：https://v2.tauri.app/develop/calling-rust/
- Node.js downloads：https://nodejs.org/en/download
- MapLibre GL JS documentation：https://maplibre.org/maplibre-gl-js/docs/
