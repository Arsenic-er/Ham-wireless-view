# HamHeatmap 技术设计草案

- 文档版本：0.1-draft
- 日期：2026-07-16
- 对应需求：`01-product-requirements.md`

## 1. 技术目标

构建一个 Windows 10/11 64 位桌面应用：视觉底图仅在线，传播数据计算离线优先。前端负责地图、参数表单、进度和导出预览；Rust 后端负责 DEM/WBM 与计算缓存、坐标计算、ITM 调用、并行调度、结果栅格化和文件导出。

优先保证：计算可复现、地形确实影响结果、UI 不冻结、离线数据行为明确、数据和模型版本可追踪。

## 2. 技术栈

- 桌面框架：Tauri 2.11.5。
- 前端：React 19.2.7 + TypeScript 7.0.2 + Vite 8.1.4。
- 地图：MapLibre GL JS 5.24.0；Windows/Tauri 通过原生 `tianditu:` 协议显示天地图 `vec/cva` 或 `img/cia`；私有 validation 普通地图使用同源天地图代理，卫星图使用同源 EOxCloudless 代理。桌面与 validation 的密钥和网络路径彼此隔离。
- WGS84 坐标网格是所有在线底图不可用时的唯一视觉降级，不属于离线底图资产，并继续承载发射点、范围、比例尺和热力图。
- PMTiles JavaScript、fflate 与四省归档只保留为历史 validation 证据，不再属于当前产品架构。
- 前端工具链：项目内固定 Node.js 24.18.0，不依赖 JAIST 主机全局 Node。
- 后端：Rust stable。
- 传播核心：NTIA ITM C++ 源码，以本地静态库或 DLL 方式绑定。
- 并行：Rust Rayon 或等价线程池。
- 本地数据库：SQLite。
- 栅格读取：纯 Rust `tiff 0.11.3`（只启用 Deflate）读取本地 GLO-90 DEM/WBM GeoTIFF，不引入 GDAL。计算核心已经完成统一全局栅格寻址、跨边界双线性高程插值、分类水体采样和 200 km 全圆计算；正式缓存索引、原子下载、断点续传和用户发起删除已经通过 Phase 1 验证。
- 导出：前端固定离屏 Canvas 生成 1600×1100 内部诊断报告；Tauri 下由 `hamheatmap-export` 校验 PNG、用 `printpdf 0.11.1` 封装 A4 横向 PDF 并原子保存，validation 下由浏览器直接下载 PNG，或把报告画布转为高质量 JPEG 后嵌入单页 A4 横向 PDF。validation 不增加服务器导出端点、文件正文上传或目标路径参数。正式合规地图导出仍待底图授权。
- 安装：Tauri NSIS 64 位安装包；额外打包便携 ZIP。
- CI/服务器：Linux 执行格式、单元测试、核心基准，并用项目内 cargo-xwin/LLVM/NSIS 生成内部 Windows 交叉构建。
- 正式发布：Windows 10/11 执行原生安装、WebView2、文件系统、性能和签名验证。

Tauri 在 Windows 使用 WebView2；离线安装包采用内嵌 WebView2 离线安装组件，以免目标电脑安装时必须联网。
服务器交叉构建由 `scripts/tauri-windows-cross.sh` 统一编排，静态 CRT 和项目内 NSIS 的理由与限制见 ADR 0009。

## 3. 目录结构建议

```text
hamheatmap/
├─ AGENTS.md
├─ README.md
├─ LICENSE
├─ docs/
│  ├─ 01-product-requirements.md
│  ├─ 02-technical-design.md
│  ├─ 03-data-licenses.md
│  ├─ 04-test-plan.md
│  └─ decisions/
├─ app/
│  ├─ src/
│  ├─ public/
│  └─ src-tauri/
├─ crates/
│  ├─ propagation/
│  ├─ terrain/
│  ├─ cache/
│  ├─ app-service/
│  └─ export/
├─ third_party/
│  └─ ntia-itm/
├─ fixtures/
│  ├─ synthetic-terrain/
│  └─ itm-reference/
├─ scripts/
└─ .github/workflows/
```

## 4. 逻辑架构

### 4.1 前端层

- `MapView`：合规底图、中文地名、地图/卫星切换、当前发射点、历史站点标记、200 km 圆、最多 8 个不可检查的会话覆盖层和图例；不渲染高程视觉层，也不把卫星影像解释为传播输入。
- `ParameterPanel`：场景、频率、功率、增益、AGL 天线高度、发射点地面海拔来源和值、极化。
- `CalculationPanel`：开始、取消、进度、状态、模型名称。
- `CacheManager`：已缓存区域、分类大小、删除和 2.5 GB 硬限制。
- `ExportDialog`：PNG/PDF 预览和保存。
- `Settings`：单位偏好、日志目录、语言和版本信息。

### 4.2 Rust 服务层

- `AppService`：用例编排和状态机。
- `DownloadService`：数据清单、断点下载、校验和、重试。
- `CacheService`：缓存索引、配额、引用和删除。
- `TerrainService`：DEM/WBM 瓦片定位、读取、插值和剖面采样。
- `PropagationService`：输入归一化、ITM 调用和链路预算。
- `CoverageService`：圆形网格生成、并行任务、进度和取消。
- `ExportService`：前端冻结报告快照；独立 Rust crate 负责输入校验、PDF 编码和原子文件保存；Tauri 只负责原生保存对话框。

Phase 2 把可序列化的桌面契约放在独立 `hamheatmap-app-service` crate 中，Tauri 只负责操作系统数据目录、异步阻塞任务调度、事件发送和单任务状态；输入范围、W/dBm、dBi/dBd、频段、缓存验证、下载计划、DEM/WBM 加载和 ITM 调用仍由共享 Rust 服务执行。浏览器模式只用于界面与视觉检查，明确禁止返回模拟传播结果或执行真实下载。

当前 IPC 命令：

```text
bootstrap          → 模型、固定网格和缓存配额
inspect_point      → 区域计划、缓存就绪度和中心高程
estimate_download  → 固定来源 HEAD、续传量、整批配额/磁盘预检
download_region    → 后端重建计划、原子下载/生成、校验和与 ready 检查
cancel_download    → 下载取消令牌，保留完整资产与合法 partial
cache_overview     → 实际目录用量和已登记区域/共享引用摘要
delete_cache_region → 用户确认后只回收无其他区域引用的资产
calculate          → 真实 DEM/WBM + ITM + 内存 PNG
export_result       → 原生保存对话框 + 校验后的 PNG/A4 PDF 原子写入
cancel_calculation → 原子取消令牌
```

`calculate` 在阻塞线程运行，并通过 `calculation-progress` 事件报告数据加载、约 1% 像素批次、PNG 编码和完成阶段。覆盖 worker 每个接收点开始前及长剖面内每 64 个样本检查取消；取消结果不生成可导出热力图。

`export_result` 只接受格式、ASCII 安全建议文件名和固定报告 PNG Data URL。Rust 强制校验 MIME、Base64、1600×1100 IHDR、完整解码及 12,000,000 字符 IPC 上限；前端不能直接提交任意保存路径。内部报告始终保留版本、来源、限制和不可移除水印。

下载估算只接受 `MapPoint`，URL、资产键和版本全部由 Rust 从固定瓦片计划生成。估算阶段不创建区域引用；用户确认后 `download_region` 在后端重新探测并执行，以避免信任前端回传的 URL 或大小。下载进度由 `download-progress` 事件报告，并按至少 0.5% 变化、250 ms 或资产完成节流。Tauri 用同一个 operation mutex 串行化初始化、点检查、估算、下载、概览、计算、删除和导出命令，避免缓存锁及报告快照竞态。

### 4.2.1 发射点地面海拔计算契约

`CalculationRequest.txGroundElevationOverrideM` 是可空字段；字段缺失或 `null` 均表示 DEM 自动，有限数值表示手动覆盖。Rust 在应用服务与覆盖引擎两层都验证手动值位于 `-500..=9000 m AMSL`，从而兼容旧请求但不信任前端验证。

覆盖引擎在建立 worker 前始终读取并校验中心 DEM。手动模式不能绕过缺瓦片、NoData、损坏或非有限值检查；校验成功后才选择“手动值或中心 DEM”作为本次计算的有效发射点地面海拔。该有效值只替换每条 ITM PFL 的第一个发射端地形样点；`txHeightM` 继续表示独立 AGL，后续剖面/接收点高程继续读取 DEM，全部 WBM 样点和陆水比例语义不变。

前端默认发送 `null`，DEM/手动模式切换只改变该字段。场景预设保留它；选择不同地图点重置为 `null`，清空热力图则保留当前点和字段。有效天线 AMSL 只用于界面说明，不成为新的传播输入。

`CalculationResult` 的 schema 3 首次冻结以下字段；当前 schema 4 继续保持相同语义：

- `txGroundElevationM`：本次计算实际使用的有限 AMSL 数值；
- `txGroundElevationSource`：严格为 `dem` 或 `manual`。

`bootstrap` 和其他 AppService 契约仍保持 schema 2。内部报告只读取当前 CalculationResult 的冻结结果值与来源，不能用计算后的表单或再次读取的 DEM 重建该字段。schema 4 仅新增显示筛选元数据，决策依据见 ADR 0014 与 ADR 0021。

### 4.2.2 私有 validation server 操作协议

validation HTTP 桥接器保留同步长请求作为结果的唯一权威来源，但为计算和下载类长操作增加服务端签发的 capability：

```text
POST /api/operation-ticket {"kind":"estimate-download"|"download"|"calculation"}
  → server-generated CSPRNG UUIDv4 operationId（reserved）

POST /api/estimate-download {"operationId":"…","point":{…}}
POST /api/download-region   {"operationId":"…","point":{…}}
POST /api/calculate         {"operationId":"…","request":{…}}

POST /api/operation-status {"operationId":"…"}
POST /api/cancel-download {"operationId":"…"}
POST /api/cancel-calculation {"operationId":"…"}
POST /api/operation-ack {"operationId":"…"}
```

操作规则：

- `operationId` 由服务器使用密码学安全随机源生成并编码为 UUIDv4，等同短期 bearer capability；客户端不能指定 ID。
- reserved ticket 必须在同一个 operation-state mutex 内，由匹配 `kind` 的长请求原子消费。共享 gate 忙时返回冲突，但不能消费 ticket；错 kind 或失效 ticket 不能进入 worker。
- reserved ticket 最多保留 32 个、TTL 为 60 秒；终态快照最多保留 32 个、TTL 为 5 分钟。每次相关操作先清理过期项，避免无人确认的状态无限增长。
- 状态只有 `reserved`、`running`、`cancellation-requested`、`succeeded`、`failed`、`cancelled`。响应包含 schema version、精确 operation ID、kind、单调 `sequence` 和三类 tagged 白名单 progress：`estimate-download` 只有 `{type, stage:"estimating"}`，不含 URL、资产或结果；`download` 只有字节、资产序号/数量和 percent，不含内部 asset key/URL；`calculation` 只有 phase、percent 和完成/总像素数。所有状态均不含结果、PNG、数据 URL、下载 URL、服务器路径或详细错误。
- 不提供 current/list 端点，也不允许在 ID 缺失、未知或过期时退化为“按 kind 取消当前任务”。状态使用 POST JSON，使 capability 不进入查询字符串、浏览历史或常规访问日志。
- `cancel-calculation` 与 `cancel-download` 同时校验 exact ID 和取消 family；未知 ID、错 family 或已经进入终态均返回 HTTP 200 与 `cancelled=false`，不能影响后来任务。
- `operation-ack` 按 exact ID 删除 reserved 或 terminal 记录；重复、未知或已过期确认幂等返回 false。ack 不提前释放 active worker，也不保存或返回长请求结果。
- progress、cancel、finish 和 lease Drop 通过同一 mutex 串行化，并同时核对 ID 与 generation。取消先被接受时，随后 worker 成功必须转成 `cancelled` 且丢弃结果；finish 先完成时，迟到取消不能命中下一项任务；未正常 finish 的 Drop 发布 `failed` 终态并释放 gate。

validation 浏览器在启动长请求前领取 ticket，保留 ticket promise、ID 和客户端 generation。它以约 250 ms 的递归 `setTimeout` 发起非重叠同源状态 POST，把新 sequence 转发给既有 `calculation-progress` / `download-progress` 监听器；轮询临时失败不改变长请求结果。旧 generation 或旧 ID 的迟到响应不得更新 UI。

长请求 settle 后先停止轮询；final status 与 best-effort ack 各自使用 1.5 秒超时。final status 结束后，前端必须按 handle identity 先释放当前 handle，再执行有界 ack，因此迟到 cleanup 不能清空后来 generation，ack 卡顿也不能继续占用客户端 operation 槽。

取消捕获本次 handle，即使 ticket 尚未返回也只等待该 ID，并受 3 秒总 deadline 约束。若 exact cancel 返回 false，立即查询同一 ID 的 exact status；`reserved` 或 `running` 时每 100 ms 重试相同 ID，`cancellation-requested`、任何 terminal、404 或原 handle settle 时停止。deadline 超时必须向 UI 返回明确取消超时错误，不能静默报告已取消或退化为按 kind 操作。

该协议只用于回环 validation 平台。Tauri 继续使用原生事件和桌面 operation lease，普通 preview 继续禁止真实操作。设计依据见 ADR 0013。

2026-07-27 的 full build revision `867c25aeb2091055b56d1259f6ad7293d21f7495` 已完成 server/frontend 回归和两次真实回环烟雾。每次烟雾都为 ID-A 与 ID-B 各至少观察到一个真实 calculation progress snapshot，观测时 `sequence=2`；同时通过 exact-ID/family 取消、reserved ticket 复用、terminal/ack 隔离与双 PNG 恢复。该证据只关闭受管 HTTP 协议与进度快照；通过 SSH 隧道看到浏览器中的逐阶段进度、取消屏障和无控制台错误仍待实测。

### 4.2.3 渐进式覆盖预览

渐进式预览复用最终传播计算，不建立第二套模型。coverage 层在原有结果与进度之外提供只读像素批次回调；worker 可以并发调用，借用切片只在回调期间有效，每批最多 64 个 `CoveragePixel { raster_index, received_power_dbm }`。批次回调可能先于对应 progress 计数，因此两者只分别保证单调，不要求瞬时相等。所有批次合并后必须与原有最终 `CoverageGrid` 完全一致；未使用批次回调的既有 API 行为不变，也不承担预览编码开销。

`AppService::calculate_with_preview` 在一次计算内维护一个初始全为 NaN 的 `401×401` 栅格。worker 只合并批次；编码只由一个专用线程执行，避免多个 worker 同时压缩 PNG。每跨过约 5% 有效像素阈值只向容量 1 的有界信号通道执行非阻塞通知，编码线程再以至少 800 ms 的间隔合并通知并读取最新快照。已经完成的像素使用与最终覆盖层相同的 EPSG:3857 重投影和固定色标，尚未完成的 NaN 像素透明。编码失败只跳过该帧，不改变最终计算结果。

预览契约独立使用 schema 1：

```text
CalculationPreview
  schemaVersion = 1
  sequence
  completedPixelCount / totalPixelCount
  mapOverlayProjection = "EPSG:3857"
  mapOverlayWidth / mapOverlayHeight = 401
  mapOverlayCorners
  mapOverlayPngDataUrl
```

`sequence` 与完成像素数严格递增；实现不发送 100% 预览，100% 只由最终 `CalculationResult` 表示。预览没有统计摘要、原始报告 PNG 或导出身份，且不写入计算缓存。计算足够快、取消发生较早或 transport 已关闭时允许零帧预览，最终结果语义不受影响。

桌面端 `calculate` 命令为每次调用接收一个 Tauri IPC `Channel<CalculationPreview>`，而 `calculation-progress` 继续使用事件。Channel 生命周期限定在对应 invoke；前端只接收 schema、投影和严格递增 sequence 均有效的消息，invoke settle 后关闭接收，迟到消息不能进入下一次计算。

私有 validation 平台增加：

```text
POST /api/operation-preview
{"operationId":"…","afterSequence":0}
```

只有相同 exact ID 的活动 calculation 且存在 `sequence > afterSequence` 的最新帧时返回 HTTP 200；未知但格式有效的 ID、尚无新帧、非计算操作、取消中或终态返回 204，无效 JSON、未知字段、错误媒体类型和无效 ID 格式按 API 错误处理。服务器每个活动任务只保存最新一帧，不把 PNG 放入 status/terminal；取消、完成、失败或 lease Drop 都清除它。浏览器在每次 status 轮询之后串行请求 preview，保持请求不重叠，并同时校验 ID、generation 和 sequence；preview sequence 与 status sequence 相互独立。

React 分开保存当前临时 `preview`、当前可导出的权威 `result`，以及最多 8 项的 `sessionResults`。开始新计算、取消、错误、选择新点、参数变化和清空都会抑制或清除旧预览；选择新点或取消重算只撤销当前导出身份，不删除 `sessionResults` 中已完成的其他覆盖层。成功响应冻结本次 `RadioParameters`，按精确中心坐标替换同点旧项或追加新项，并把最新项放在最上层；超过 8 项时移除最早项。已完成结果使用独立 MapLibre CanvasSource/layer；预览仍使用既有 image source/Blob URL。清空与卸载时逐项释放画布、图像和 Blob URL；历史站点使用独立 GeoJSON source。导出始终只读取当前最新的 `result` 及其冻结参数，不做多层合成，也不读取显示阈值。预览决策见 ADR 0016，会话层决策见 ADR 0019，显示筛选决策见 ADR 0021。

### 4.2.4 validation 在线天地图主路径与地图状态

validation server 可从项目内 `.runtime/validation-platform/secrets/tianditu.token` 读取可选天地图 token。token 只保留在服务器进程；bootstrap 返回不含凭据和上游主机的 `BasemapInfo`，浏览器只请求固定同源模板 `/api/basemap/tianditu/{layer}/{z}/{x}/{y}`。代理只允许 `vec/cva`、规范十进制 `z/x/y` 和 `z<=18`，固定访问 `https://t0.tianditu.gov.cn`，禁止重定向，并对超时、2 MiB 上限、MIME 和图片签名 fail closed。响应为 `no-store`，不进入 2.5 GB DEM/WBM 与计算缓存。

前端只有在 provider、模式、模板、缩放和 `vec/cva` 元数据全部匹配固定契约时才增加 raster source。普通地图位于经纬网和分析层下方，在线中文注记位于热力图上方，发射点标记保持最上层。token 缺失或在线请求失败时 `enabled=false`，MapView 继续显示 WGS84 内部测试画布。

MapLibre 原生 `ScaleControl` 位于右下，使用 metric 单位和 120 px 最大宽度，随 move 事件自动更新。MapView 以 refs 保存最新 basemap、point、`sessionResults`、active ID、preview 和 stale 状态；若 style 暂不可操作，同步标记为 pending，并在 load 后或 `styledata/idle` 恢复时重放。这样清空 props 不会因一次 `isStyleLoaded=false` 而丢失，恢复时会删除全部不再需要的 heatmap layer/source 并逐项释放 Blob URL。验证记录见 `20-tianditu-basemap-proxy.md`。

该代理用于回环 validation 在线验证。Windows 使用 ADR-0020 的原生天地图协议；两者都不授权离线缓存、再分发或把在线瓦片嵌入诊断导出。

### 4.2.5 历史：私有区域 PMTiles 主验证底图

本节记录已发生的 PMTiles 工程验证，自 2026-08-02 起不再定义当前产品路径；现行架构见 ADR-0022。

四省验证归档固定为 source build 20260731、bbox 107.5,18,125.5,33.5、z0-9、33,044,072 bytes；SHA-256 为 5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0。归档包含 939 个 region tiles、837 个 archive entries，占 2.5 GB 上限的 1.32%，payload 为 gzip 压缩 MVT。

validation server 只通过同源 HTTP Range 暴露该固定归档。端点保持回环绑定；合法单段 Range 必须返回精确的 206、Content-Range、Accept-Ranges 和长度，无效、越界或不支持的 Range 按固定契约失败，不得默默退化为整包响应。bootstrap 只返回浏览器完成协议注册所需的相对 URL 与非秘密元数据。

MapLibre 注册 PMTiles protocol 后构建六类可信可见 source layer：earth、landcover、landuse、water、roads、places。boundaries 与 pois 不进入可见样式。`places` 分成省级、主要城市、县区和乡镇 symbol layer；文字表达式按 `name:zh-Hans`、本地 `name`、`name:en` 回退，按 `min_zoom`、`kind`、`kind_detail` 与碰撞优先级控制密度。固定 z0-9 归档只承诺省、市、区县和乡镇级可用，不保证村、自然村或街道名称完整。

样式不配置 glyph URL。MapLibre GL JS 5.24.0 在无 glyph URL 时由 TinySDF 从本机字体生成字形；地图构造显式使用 `Microsoft YaHei, Noto Sans CJK SC, PingFang SC, sans-serif` 中文系统字体栈。本轮不下载或打包 WOFF/TTF、glyph PBF 或第二份地名数据，Tauri 与 validation CSP 也不增加外部字体源。

渲染顺序固定为：基础地表/道路/水体 < 经纬网 < 传播热力图 < 200 km 范围与地名注记 < 发射点。用于插入热力图的“首个地名层”必须同时识别 PMTiles 和天地图注记，使预览、最终结果、主题切换和 desired-state 重放都不能把热力图压到文字之上。地图始终显示 © OpenStreetMap contributors，ScaleControl 和既有状态重放逻辑继续复用。

原始归档仍含 boundaries 以及 Natural Earth/OSM 内容，显示过滤不等于删除；当前只用于私有验证、不纳入正式 EXE，且不作公开发行结论。源数据按 ODbL Produced Work 处理，landcover 上游署名要求仍待确认。

自动化、固定 SHA-256、同源 Range、受管运行及 PMTiles JavaScript getHeader/getZxy 实际读取均已通过；真实浏览器视觉因 Codex 桌面 ACL 故障仍待人工确认。证据统一记录在 docs/21-protomaps-four-province-basemap.md。

### 4.2.6 地图/卫星切换与 EOxCloudless 代理

地图模式使用在线天地图普通地图；validation 卫星模式使用 EOxCloudless Sentinel-2 2025 的 `s2cloudless-2025_3857` WMTS，矩阵集为 EPSG:3857，支持 z0-14。前端只接受固定 bootstrap 契约并请求同源模板 `/api/basemap/satellite/{z}/{x}/{y}`；服务器把它映射到固定 `https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless-2025_3857/default/g/{z}/{y}/{x}.jpg`。2026-08-01 的 live 四省请求已确认响应为 JPEG，代理返回 `Cache-Control: no-store`。

代理沿用最小网络面：只允许规范十进制 z/x/y 和 z0-14，固定 HTTPS 主机与路径，不接受查询字符串、用户 URL、凭据或重定向；设置有界连接/读取/总超时、响应体上限、JPEG MIME 与签名校验，并对客户端返回 `Cache-Control: no-store`。浏览器 CSP 仍只需 `connect-src 'self'`。卫星请求不写入 Rust 缓存、SQLite、浏览器持久存储或 Service Worker，不出现在缓存管理中，因此十进制 2.5 GB 配额和现有预算保持不变。

卫星 raster 位于在线注记和传播热力图之下；切换时保留 camera、发射点、200 km 圆、预览/最终热力图和参数状态。卫星请求失败时先切回在线普通地图，普通地图也不可用时显示 WGS84 网格并给出非阻塞提示；失败瓦片不写入缓存。EOx 署名在卫星模式持续可见：`EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)`。

EOxCloudless 只是视觉背景，不进入 DEM/WBM 采样、ITM、路径陆水比例或结果统计。Google Maps / Google Satellite 不采用，因为其 API key、计费、调用策略以及离线缓存/再分发限制会引入凭据和不可控持久化边界。详细取舍见 ADR 0018。

### 4.2.7 Windows/Tauri 在线天地图

桌面版不复用 validation 的 HTTP 代理，而是注册固定自定义协议 `tianditu://localhost/{layer}/{z}/{x}/{y}`。普通地图组合 `vec+cva`，卫星图组合 `img+cia`；四个模板、provider、协议、缩放范围和署名由 Rust `get_online_basemap` 返回，前端严格匹配后才启用。MapLibre 从始至终看不到供应方 Key 或可变上游 URL。

`configure_online_basemap` 校验用户输入后，在 Windows 用当前用户作用域 DPAPI 保存密文；`clear_online_basemap` 删除密文并立即回到未配置态。前端 Key 字段只保存本次编辑的临时值，不进入 localStorage、sessionStorage、查询字符串、bootstrap、错误文本或日志。非 Windows 编译只支持测试所需的内存态，不得用明文文件持久化。

`probe_online_basemap` 只由设置界面的显式操作调用，并在桌面单操作门闩内通过固定中国区域代表瓦片复用同一 HTTPS 代理校验链。schema 1 响应只允许 `reachable/not-configured/network/timeout/upstream-or-credential/invalid-content` 六种状态，不返回 Key、上游 URL、响应正文、路径或供应方错误细节；界面文案由本地固定映射生成。保存成功不自动等价于连接成功，`upstream-or-credential` 也不能被解释为已精确判定 Key 无效。

自定义协议后端只接受 `vec/cva/img/cia`、规范十进制坐标和 z1-18；它固定构造 `https://t0.tianditu.gov.cn` WMTS 请求，拒绝重定向，限制连接/读取/总超时与 2 MiB 响应体，同时验证成功状态、图片 MIME 和 PNG/JPEG 签名。响应统一 `Cache-Control: no-store`，不写入 SQLite、缓存目录、浏览器存储或 Service Worker。

桌面 CSP 的 `img-src` 和 `connect-src` 都只放行 `tianditu:` 自定义协议及其 Windows 映射域 `http://tianditu.localhost`、`https://tianditu.localhost`；不开放任意公网 HTTPS。设置入口只在 Tauri 模式可见；普通 preview 和 validation-server 不获得桌面命令能力。地图/卫星切换不改变发射点、传播输入、热力图或相机。未配置 Key、断网、配额或上游错误都 fail closed，并保留可行动的设置提示。

在线瓦片只作实时视觉背景，不进入 DEM/WBM/ITM、2.5 GB 持久配额或诊断 PNG/PDF。高德/腾讯不作为裸瓦片替代，因为其 GCJ-02 地图语义会与 WGS84 传播覆盖产生偏移。完整决策见 ADR 0020。

### 4.2.8 已完成覆盖层的全局场强显示阈值

显示阈值只作用于已完成结果。范围固定为 `-140..-60 dBm`、步长 1 dB、默认 `-140 dBm`；最多 8 个会话层共用一个值。拖动时只改变像素 alpha，保留可见像素原本的固定色标，不调用 calculate、不改变 ITM、统计、缓存、渐进预览或诊断导出。清空覆盖层不重置阈值，应用新启动时回到默认值。

`CalculationResult` 升级到 schema 4，并在既有地图 PNG 之外增加：

```text
mapOverlayFilterEncoding = "u8-dbm-floor-v1"
mapOverlayFilterBase64   = Base64(width × height bytes)
```

`u8-dbm-floor-v1` 与同一张 EPSG:3857 地图覆盖 PNG 逐像素对齐。值 0 表示非有限、圆外或低于固定 `-140 dBm` 可视下限；值 1..81 表示 `floor(dBm) + 141`，其中所有 `>= -60 dBm` 饱和为 81。整数阈值 `t` 的 cutoff 为 `t + 141`，仅当 bin `>= cutoff` 时保留 PNG 原 alpha。该编码只携带筛选顺序，不是可查询的 float 栅格，前端仍不得提供像素检查。

前端对每个最终 PNG 只解码一次，保留原始 RGBA 与校验过长度的 `Uint8Array` bins，并创建 `animate:false` 的 MapLibre `CanvasSource`。游标 `input` 事件通过 `requestAnimationFrame` 合并，并以不短于 33 ms 的间隔限制为最多 30 帧/秒；每帧只线性扫描最多 `8 × 401 × 401` 个像素并更新 alpha，然后请求一次纹理上传。相同整数值不重复绘制，旧帧不能覆盖最新值。该路径避免在拖动中重新编码 PNG、重建 source/layer 或发出 IPC/HTTP 请求。

渐进预览契约保持 schema 1 和 image source，不新增 bins；显示阈值不筛预览。统计继续来自完整 float32 权威栅格，PNG/PDF 继续使用未筛选的原始报告 PNG。设计取舍见 ADR 0021。

### 4.3 原生传播层

- 使用 NTIA 官方 ITM C++ 代码作为固定版本的第三方依赖。
- Rust 只通过窄 FFI 接口调用：输入结构、地形剖面、输出损耗、警告和传播模式。
- 任何 ITM warning/error 都映射为内部枚举，并计入日志与计算摘要。
- 版本升级必须通过参考输入/输出回归测试。

## 5. 坐标与覆盖网格

- 数据和内部持久坐标：WGS-84 / EPSG:4326。
- 发射点以经纬度保存。
- 每次计算建立以发射点为中心的局部等距投影，用于生成固定 1 km 网格。
- 候选网格为 `401 × 401`，中心索引为 `(200, 200)`。
- 只保留中心距离 `≤ 200 km` 的像素，约 12.6 万个接收点。
- 每个接收像素的经纬度由局部投影反算，用该位置 DEM 作为接收点海拔。
- 200 km 边界按测地距离判断，避免直接用经纬度差。
- 发射点有效区使用单独的中国大陆多边形；结果数据允许延伸到周边国家和近海以保持完整圆形。

### 5.1 地图覆盖层重投影

传播引擎的规范结果保持为局部等距 `401×401` dBm 栅格；该原始栅格和原始 PNG 用于统计、回归与内部诊断报告。MapLibre 不直接显示原始 PNG，因为它会把四个 WGS-84 角点转换为 Web Mercator 后用两个三角形做仿射插值，无法表达局部等距到 Web Mercator 的非线性变换。

Rust 服务从同一 dBm 栅格额外生成 `401×401`、轴对齐 `EPSG:3857` 地图覆盖层。覆盖层矩形包围原始样本域，图像四边再按输出像素间距各扩展半个像素，使首末像素中心而非图像外缘对应计算域边界。每个目标像素中心先从 Web Mercator 反算到 WGS-84，再经发射点测地反算换成局部等距坐标，并对原始 dBm 栅格做 NaN 感知双线性采样。圆外、原始域外、无有限贡献和低于透明阈值的结果保持透明。

桌面服务同时返回原始报告 PNG 与地图覆盖层 PNG、投影、宽高和四角。`MapView` 只读取地图覆盖层，报告渲染器只读取原始 PNG。设计理由和误差门槛见 ADR 0011，验证记录见 `13-web-mercator-overlay-validation.md`。

2026-07-24 自动化验收覆盖纬度 18°、30.5°、40°、54°，最大样本中心定位误差分别为 711.655 m、716.127 m、725.742 m、739.625 m；总体最大值 739.625 m 小于 1 km。绝对仿射 dBm/MapLibre UV、轴对齐四角、半像素边界、199 km 内侧 alpha、NaN 感知重采样、确定性 PNG、真实 14 字段序列化和前端字段分离同时通过回归。精确 200 km 连续边界点均在图像域内；部分边界的最近像素中心透明，但 `3×3` 邻域最近可见中心最差 `1012.102 m`，小于该处一个 WGS-84 实算输出像素对角线 `1431.578 m`。因此自动化栅格几何风险在当前 1 km 输出语义内关闭。

## 6. 地形数据设计

### 6.1 DEM

首选 Copernicus DEM GLO-90：

- 全球约 90 m 分辨率。
- WGS-84 水平坐标。
- 原始数据为 1°×1° GeoTIFF/DTED 产品。
- 正式数据版本写入缓存清单和 PDF。
- 默认匿名下载适配器使用 AWS Open Data 的 GLO-90 COG 镜像，避免要求新手创建数据账号。
- Phase 0 已验证 N30/E103 样本：1200×1200、32-bit float、Deflate，纯 Rust 完整解码约 0.089 秒；样本数据不提交 Git。

### 6.2 水体

选用与 DEM 同版本、同网格的 Copernicus DEM GLO-90 Water Body Mask（WBM），通过同一 AWS Open Data 匿名 COG 镜像获取。WBM 是 8-bit GeoTIFF，源值 `0/1/2/3` 分别表示非水体、海洋、湖泊和河流。应用读取后立即把 `1/2/3` 折叠为 `water`，只保留布尔陆水分类；未知值作为数据错误阻断计算。

```text
WaterMaskProvider
├─ CopernicusGlo90WbmProvider
├─ GeneratedUniformOceanProvider
└─ FutureProvider
```

GLO-90 只发布覆盖陆地的对象，纯海洋 1° 地理单元可能同时没有 DEM 和 WBM 对象。下载器只有在同一固定版本、同一地理单元的 DEM 与 WBM 均明确返回 `404` 时，才生成确定性的本地全零 DEM 和全水体 WBM；只缺少其中一个对象、网络错误或其他状态都阻断准备流程。生成资产使用 SHA-256、同目录原子写入并计入 2.5 GB 硬配额，不能把一般性的“缺数据”解释为海洋。

WBM 只参与传播计算，不作为可见底图。正式在线底图仍通过受限 provider 接入，要求具有有效审图信息和明确的桌面在线服务、叠加与署名授权；不持久缓存在线瓦片。

### 6.3 地形采样

- DEM 剖面采样间距初始使用原生约 90 m。
- 瓦片边界使用一致的双线性插值规则。
- WBM 采用包含采样点的原始分类像素，不对分类值做双线性插值。
- 经 DEM/WBM 成对 `404` 确认的纯海洋单元高程为 0 m、水体为真；其他水面仍读取正式 DEM/WBM，不能把一般缺数据误当水面。
- NoData、瓦片损坏或版本混用均作为计算阻断错误。
- 发射点地面海拔可由用户覆盖，但中心 DEM 在两种模式下都必须先读取并验证。
- 手动值只替换 PFL 首样点；发射天线 AGL、其余 DEM 与整条路径 WBM 采样不变。
- 接收点海拔不可全局覆盖。

## 7. 传播计算

### 7.1 ITM 模式

- 使用 Point-to-Point Prediction Mode。
- 对每个接收点构建从 TX 到 RX 的地形剖面。
- ITM 输出基本传输损耗 `A__db`、warning 和传播模式。
- ITM 自身可能返回视距、绕射或对流层散射模式；“不考虑异常传播”指不接入实时天气、逆温和导波预测，不人为移除 ITM 的标准对流层散射分支。

### 7.2 隐藏默认参数

`ModelDefaults` 的 `land-water-v1` 固定值：

- 时间、位置、情景统计值：50/50/50 中值条件。
- 有效地球半径：标准 `k = 4/3` 对应的默认折射条件。
- 气候：`climate = 5`（Continental Temperate）。
- 地表折射率：`N_0 = 301 N-units`。
- 模型可变模式：`mdvar = 12`，不向普通用户展示。

这些数值不得散落为魔法常量，统一放入带版本号的 `ModelDefaults`。

### 7.3 极化

- UI：水平/垂直。
- ITM：水平映射为 `pol=0`，垂直映射为 `pol=1`。
- 默认垂直。

### 7.4 陆地/水面等效参数

ITM 每次调用接收一组相对介电常数 `epsilon` 与电导率 `sigma`。`land-water-v1` 采用以下固定参数：

- 陆地：`epsilon = 15`、`sigma = 0.005 S/m`。
- 统一水体：`epsilon = 81`、`sigma = 0.010 S/m`。为满足单一水体类别要求，海洋、湖泊和河流都使用这组偏保守的淡水型默认值，不使用海水 `5.0 S/m`。
- 沿每条约 90 m 剖面统计水体样本比例 `f`，然后线性混合：`epsilon = 15 + (81 - 15) × f`，`sigma = 0.005 + (0.010 - 0.005) × f`。
- `f = 0` 与 `f = 1` 精确回到全陆地和全水体端点；参数、公式和版本由 ADR-0006 固定。
- 不模拟镜面反射、多径、潮汐和海上导波。

### 7.5 近距离处理

中心像素不计算接收功率，由发射点图标覆盖。固定 1 km 网格中最近的有效接收像素恰好位于 1.0 km，不存在 0–1 km 的有效结果像素。Phase 0 实测确认固定的 NTIA ITM v1.4 在 1.0 km 返回成功、无 warning，并与后续网格点连续；因此所有非中心覆盖像素统一使用 ITM，不在热力图内拼接自由空间模型。自由空间损耗只保留为诊断函数，不进入 MVP 覆盖栅格。该决策避免在 1.0/1.001 km 人工切换时产生约 13.5 dB 的不连续。

### 7.6 链路预算

```text
tx_dbm = 10 × log10(tx_watt × 1000)
tx_gain_dbi = tx_gain_dbd + 2.15  // 仅当用户选择 dBd
rx_gain_dbi = rx_gain_dbd + 2.15
rx_dbm = tx_dbm + tx_gain_dbi + rx_gain_dbi - itm_basic_loss_db
```

不加入馈线损耗、接头损耗、人体吸收或极化失配附加损耗。用户选择的极化作为 ITM 地表波参数输入，而不是额外减去固定交叉极化损耗。

## 8. 并行与性能

### 8.1 首版算法

1. 生成约 12.6 万接收像素。
2. 按方位角和距离分块，减少 DEM 瓦片抖动。
3. 复用已读取的 DEM 瓦片和相邻射线路径数据。
4. 使用固定大小线程池并行提取剖面和调用 ITM。
5. 每完成约 1% 或每 250 ms 向 UI 发送一次合并进度。
6. 结果分块写入内存栅格；UI 每 500–1000 ms 更新一次图层，避免过度重绘。

### 8.2 性能风险

逐像素、约 90 m 剖面的点对点 ITM 计算量较大。开发第一阶段必须先做命令行基准，不直接承诺 60 秒：

- 平坦合成 DEM。
- 山区真实 DEM。
- 144 与 430 MHz。
- 1、2、4、8、16 线程。
- 记录剖面提取时间、ITM 时间、内存和总时间。

若四核 60 秒目标未达到，按顺序优化：DEM 块缓存、射线前缀复用、批量 FFI、剖面采样策略、空间分块。不得通过把输出像素改粗于 1 km 来隐藏性能问题。

2026-07-16 最小可行性实现采用 GeographicLib 生成精确 WGS84 接收端点，以球面大圆递推生成约 90 m 的内部剖面样本，并把首尾点强制替换为精确端点。代表性方位测试中，球面剖面相对 WGS84 测地线的最大偏离小于一个 90 m DEM 像素。成都 25 瓦片实测中，四线程单个真实地形场景传播计算为 8.3–8.6 秒，四场景总墙钟 29.25 秒，峰值 RSS 155,328 KiB；完整记录见 `06-minimum-viability-validation.md`。因此计算核心已达到四核 60 秒的阶段性目标，但 Windows 整机目标仍需桌面阶段复测。

加入 WBM 读取、逐剖面陆水统计和全陆地控制场景后，青岛沿海五场景四线程完整墙钟约 52.01 秒、峰值 RSS 159,932 KiB；单个真实地形场景约 11 秒。成都对应完整墙钟约 51.41 秒、峰值 RSS 195,324 KiB。陆水建模仍满足 Linux 计算核心 60 秒阶段目标，详见 `08-land-water-validation.md`。

Phase 2 桌面服务使用相同成都缓存和用户输入契约运行单场景时，125,628 个像素连同数据校验、载入和内存 PNG 编码约 9.75 秒完成，PNG 数据 URL 为 224,378 bytes。该结果证明 IPC 上层不会改变传播输出路径；Windows WebView2 渲染和整机内存仍需实机测量。

### 8.3 取消

- 每个任务有取消令牌。
- 下载、剖面块和 ITM 批次之间检查取消状态。
- 取消后释放临时栅格，UI 回到可编辑状态。
- 已完整下载并校验的数据仍保留缓存。
- validation HTTP 取消只接受服务端签发的 exact operation ID 与匹配 family；错 ID、错 family、终态或过期 capability 不改变 active。
- validation 的 progress、cancel、finish 与 Drop 使用同一状态锁决定线性化顺序；每次回调还核对 generation，旧 worker 不能发布到后来 ID。
- 同步 HTTP 请求只在 operation terminal 化后交付成功或取消；terminal 仅用于轮询可见性，不承担结果恢复。

## 9. 热力图数据格式

- 内部栅格：`401 × 401`，`float32 dBm`。
- 圆外和 `< -140 dBm` 使用 NoData/透明掩膜，但保留原始低值用于诊断需另行决定。
- 渲染纹理使用固定颜色查找表，禁止按每次结果自动拉伸。
- 热力图图层设置为无交互；不向前端暴露按像素查询命令。
- 结果元数据包含输入哈希、模型版本、数据版本、计算时间和 warning 统计。
- 当前会话最多保留 8 个不同发射点的完整结果；同点重算替换，最新结果置顶，第 9 个不同点淘汰最早项，重启不恢复。
- 计算结果 schema 4 冻结有效发射点地面海拔与 `dem/manual` 来源，并携带 `u8-dbm-floor-v1` 地图筛选 bins；bootstrap schema 仍为 2，preview schema 仍为 1。
- 同一结果包含两个固定 `401×401` 渲染产品：局部等距原始 PNG 用于内部报告；反向重采样、轴对齐 EPSG:3857 PNG 与逐像素 u8 bins 用于 MapLibre 已完成层。
- 地图覆盖层元数据显式记录 `EPSG:3857`、宽高和 WGS-84 四角；四角对应扩展半个像素后的图像外边缘。
- 重采样只消费内存中的 dBm 栅格，不重新运行 ITM，也不把 Web Mercator 像素回写为计算结果。

## 10. 缓存设计

### 10.1 配额

- 全部持久数据硬上限：2,500,000,000 字节（十进制 2.5 GB），不可配置。
- 分类：DEM、水体、下载临时文件、SQLite/索引和计算缓存；视觉底图不持久化、不计入配额。
- SQLite 保存瓦片 ID、范围、版本、大小、校验和、最后使用时间和状态。
- 写入前执行配额预检；没有足够空间时不开始下载。
- 用户删除正在使用区域时必须先取消相关计算。
- Phase 1 实现以整个应用数据根目录的实际文件长度为准，而不是只相信 SQLite 记账；索引、锁文件、未登记文件和 `.partial` 都计入 2.5 GB。下载另保留最多 16 MB 的索引/事务安全余量，用户不可配置。
- 配额不足与文件系统可用空间不足分别返回错误；不自动淘汰旧区域。

- 不为视觉底图预留配额；天地图与 EOxCloudless 瓦片统一 `no-store`，不得产生浏览器或 Rust 持久副本。
- 已缓存完整 DEM/WBM 的区域可在无网状态下继续计算；地图视觉降级为 WGS84 网格，不用在线或卫星像素替代计算资产。
- 服务器当前约 33 MB 的四省 PMTiles 是尚未删除的历史 runtime 资产，不是现行缓存类别；删除必须作为独立受管清理任务执行并记录。

### 10.2 下载完整性

- 临时文件使用 `.partial` 后缀。
- 支持 Range 断点续传时恢复，否则重新下载单个瓦片。
- 下载完成后验证 Content-Length 与已知校验和。
- 原子改名后才把数据库状态标为 ready。
- 应用异常退出后，下次启动清理失效的 partial 文件。
- 下载 URL 由瓦片 ID 内部生成，只允许固定的 AWS GLO-90 HTTPS 主机；拒绝用户信息、查询参数、片段和相似域名。
- HTTP Agent 在 URL 校验之外强制 `https_only`、`max_redirects = 0`，并为 DNS 解析、连接、发送、响应头、响应体以及整次请求设置有限超时。重定向不会被跟随，因此白名单不能被 3xx 跳转绕过。
- HEAD 元数据只有 HTTP 200 可作为存在对象；HTTP 404 只进入既有的 DEM/WBM 成对海洋判定，其他 2xx/3xx/4xx/5xx 都是网络/完整性错误。
- AWS 公开对象提供 Content-Length 和 Range，但当前没有逐瓦片、经认证的 SHA-256 清单。首次下载以 HTTPS、域名白名单和长度验证为基础，完成后记录本地 SHA-256，之后每次计算前复核。公开发行前仍应生成并签名应用自己的固定版本清单。
- 每个地理单元同时规划 DEM 与 WBM。只有同单元的两个固定 URL 都返回 `404` 才生成纯海洋资产；单边 `404`、超时或服务器错误均不可降级。生成资产也经过 SHA-256、原子提交和配额核算。
- 取消、响应体读取错误和早于 Content-Length 的 EOF 都先对 partial 执行 `sync_all` 并把完整写入长度写回 SQLite，再返回取消或网络错误。
- `write_all` 部分写入后失败时，根据受界的实际文件游标 best-effort 同步 partial/SQLite，同时始终返回原始写错误；游标读取失败、游标越出本块受界范围或二次检查点失败时，先标记 corrupt，再关闭并删除不可信 partial。即使标记与删除同时失败，重启 reconcile 也只信任 SQLite 已完成检查点的长度：更长尾部被截断并同步，更短或缺失文件被废弃，绝不以文件长度反向上调索引。系统不自动重试；用户再次发起准备时，只有强 ETag、Range 能力、期望大小和磁盘/索引长度全部一致才继续该 partial。

传输加固决策见 ADR 0015。

### 10.3 缓存键

计算缓存键至少包含：

```text
model_version
model_defaults_version
dem_dataset_version
water_dataset_version
tx_lat_lon
frequency
polarization
tx_ground_elevation_override
tx/rx heights
tx/rx gains
tx power
```

任何一项变化都不能复用旧结果。

## 11. 状态机与错误处理

顶层状态：

```text
Idle → PointSelected → DataChecking
DataChecking → DownloadRequired → Downloading → Ready
DataChecking → Ready
Ready → Calculating → Completed
Downloading/Calculating → Cancelling → Ready 或 PointSelected
任意状态 → RecoverableError
```

错误必须区分：无网络、配额不足、磁盘不足、数据缺失、校验失败、DEM NoData、ITM 输入错误、ITM warning、导出失败和内部错误。界面消息说明用户下一步能做什么，技术细节写入本地日志。

## 12. 安全与隐私

- 不上传用户坐标、参数或结果。
- 下载请求只包含数据瓦片标识和应用 User-Agent。
- 不在代码中硬编码第三方账号、密码或长期令牌。
- Tauri 命令采用最小权限白名单，前端不能执行任意系统命令。
- 下载 URL 必须在允许域名列表内并使用 HTTPS；Agent 同时拒绝 HTTP 与重定向并使用有限分阶段/总超时。
- 对远端清单和下载内容进行校验，防止缓存投毒。
- 私有 validation 模式是坐标离开 Windows 本机的显式例外；仍只允许同源回环/SSH 隧道访问，不新增 CORS 或公网入口。
- operation ID 作为短期 capability，只能由服务器生成并放在 JSON body 中；禁止放入 URL、提供 current/list 枚举或在状态响应泄露结果与基础设施细节。
- ticket/terminal 集合同时实施 TTL 和最大 32 项限制；ack、过期清理及容量回收都必须按 exact ID 运行。

## 13. 测试策略

### 13.1 单元测试

- W↔dBm、dBd↔dBi。
- 经纬度与局部网格往返。
- 200 km 圆形掩膜。
- 色标边界和透明阈值。
- 缓存配额与删除。
- 输入校验。
- 水体比例统计。
- 海洋、湖泊和河流归并为统一水体类别。
- 1 km 内自由空间近似与 ITM 边界连续性。
- Web Mercator 正反算、半像素边界和轴对齐四角。
- 合成梯度的 NaN 感知双线性重采样、圆外透明和确定性 PNG。
- 纬度 18°、30.5°、40°、54° 的地图像素定位误差小于 1 km。
- 精确 200 km 连续边界位于半像素扩展域内；最近可见栅格中心满足该处一个 WGS-84 实算输出像素对角线容差。
- 绝对仿射 dBm 与 MapLibre UV 像素中心、真实 14 字段 camelCase 序列化和地图/报告字段分离。
- operation ticket 只接受三种 kind，ID 为服务端 CSPRNG UUIDv4；客户端指定 ID、错 kind、过期 ticket 和重复消费失败。
- busy gate 不消费 reserved ticket；容量 32、reserved 60 秒、terminal 5 分钟和 ack 回收均有确定性时钟/边界测试。
- status 状态转换与 sequence 单调；progress 仅含白名单字段，序列化中不存在 PNG、data URL、下载 URL、路径或错误详情。
- exact-ID + family 取消覆盖未知 ID、错 family、终态、取消先于完成、完成先于迟到取消、Drop failed 及旧 generation 回调。
- 前端轮询非重叠，临时失败可恢复；旧 generation/ID 不分发进度，取消在 ticket 返回前仍绑定原 handle，settle 后停止并 best-effort ack。
- 发射点地面海拔字段缺失、`null` 和有限手动值的反序列化；`-500/9000` 边界与非有限/越界拒绝。
- 手动模式仍读取中心 DEM，且只替换 PFL 首样点；DEM 自动基线、AGL、后续 DEM 与 WBM 语义保持不变。
- schema 4 的 `txGroundElevationM`、`txGroundElevationSource`、`u8-dbm-floor-v1` 编码和 bins 序列化，以及 bootstrap schema 2、preview schema 1 不变。
- u8 bins 的 0/1/81 边界、整数 cutoff 等价性、Base64 解码长度与 PNG 尺寸一致性。
- 场景预设保留覆盖、新点重置 DEM 自动但保留其他已完成覆盖层、清空全部会话层并保留当前点/覆盖与显示阈值、冻结导出读取结果快照而非表单。
- 阈值游标默认/范围/步长、30 fps 合帧、相同值不重绘、最多 8 个 CanvasSource 同步 alpha，以及拖动不调用计算/统计/预览/导出路径。
- 下载 Agent 的 HTTPS-only、零重定向与有限超时配置；HEAD 只接受 200 元数据。
- 取消、读取错误、early EOF 和部分写入错误都覆盖 partial/SQLite 一致性；写错误后的游标读取失败、游标越界或检查点失败均不掩盖原始错误，且不可信 partial 不能在同进程或重启后续传。

### 13.2 ITM 回归

- 使用 NTIA 仓库提供的 point-to-point 示例输入/输出。
- 固定 FFI ABI 测试。
- 对 error/warning 映射做覆盖测试。

### 13.3 合成地形

- 全平面。
- 单山脊。
- 双山脊。
- 全水面。
- 半陆地半水面。
- 海拔突变和 NoData。

### 13.4 端到端

- 首次选择点并下载。
- 无网络时在 WGS84 网格上打开已缓存 DEM/WBM 区域并完成计算。
- 离线选择未缓存区域。
- 计算中取消。
- 达到和试图超过 2,500,000,000 字节硬上限。
- 不同发射点连续计算保留独立覆盖层，同点重算替换，超过 8 项淘汰最早项。
- 清空全部会话覆盖层但不删除缓存，并保留全局显示阈值。
- 拖动 `-140..-60 dBm` 游标时同步筛选最多 8 个已完成层，无网络/IPC/重算，渐进预览和导出保持未筛选语义。
- Tauri 原生保存与 validation 浏览器本地 PNG/PDF 导出。
- 在线普通/卫星底图失败时 WGS84 网格降级，且已缓存 DEM/WBM 计算不受视觉底图状态影响。

## 14. 数据与法律风险

- Copernicus GLO-90 可免费使用，但公开展示和改编数据必须保留规定的来源声明。
- Natural Earth 数据属于公共领域，只作为内部开发诊断底图；其国界表示不自动满足面向中国大陆公开发行的要求。
- OSM 标准栅格瓦片服务禁止离线预取，不能作为本项目的缓存来源。
- 中国大陆公开地图的展示、登载和互联网地图服务存在地图审核、合法来源、审图号和数据服务器等要求。发布前必须进行针对桌面离线开源软件形态的专业合规确认。
- 开发期使用的国际边界数据不得进入面向中国大陆公众的正式发行版。
- 公开发行版必须显示底图来源与审图号，并完成对动态热力图叠加、缩放、裁切和 PNG/PDF 导出的合规确认。

## 15. 分阶段实施

### Phase 0：技术验证

- 初始化仓库、许可证和 CI。
- 编译 NTIA ITM 并通过官方参考测试。
- 下载单个 GLO-90 瓦片。
- 生成合成 DEM 的单条路径结果。
- 完成 12.6 万点命令行性能基准。

### Phase 1：计算 MVP

- Rust 覆盖引擎、缓存索引、区域下载、固定色标输出。
- 不做完整 UI，只输出诊断 PNG/JSON。
- 状态：覆盖引擎、DEM/WBM 多瓦片读取、任意点区域规划、SQLite 缓存索引、配额、原子下载、断点续传、纯海洋缺瓦片生成、区域引用删除、沿剖面陆水参数混合和诊断 PNG 已完成验证；取消接口已进入后端但尚未连接 UI。

### Phase 2：桌面 MVP

- Tauri/React/MapLibre 主界面。
- 点选、参数、下载、进度、取消、清空和热力图。
- 状态：应用骨架、空白合规占位地图、单点与 200 km 圆、参数预设、浅/深/系统主题、固定色标、Rust 输入归一化、真实缓存计算、区域下载、缓存管理、内部 PNG/PDF 诊断报告、Windows 交叉编译和离线 NSIS 产物已完成；ADR 0011 的局部等距原始 PNG 与轴对齐 Web Mercator 地图覆盖层双栅格实现和自动化验收已完成，代表性纬度最大定位误差 739.625 m；合规有效区、正式地图导出、签名和 Windows 10/11 完整实机验收尚未完成。

### Phase 3：发布准备

- 正式合规地图 PNG/PDF、便携版和安装版。
- 合规底图替换或审核。
- Windows 实机 QA、许可证清单和公开仓库。

## 16. 官方技术依据

- NTIA ITM：https://github.com/NTIA/itm
- Tauri Windows 安装：https://v2.tauri.app/distribute/windows-installer/
- MapLibre GL JS：https://maplibre.org/maplibre-gl-js/docs/
- Copernicus DEM：https://dataspace.copernicus.eu/explore-data/data-collections/copernicus-contributing-missions/collections-description/COP-DEM
- Copernicus DEM AWS Open Data：https://registry.opendata.aws/copernicus-dem/
- Natural Earth 使用条款：https://www.naturalearthdata.com/about/terms-of-use/
- OSM 标准瓦片策略：https://operations.osmfoundation.org/policies/tiles/
- 《地图管理条例》：https://xzfg.moj.gov.cn/front/law/detail?LawID=421&Query=

## 17. 未关闭的工程决策

- 是否在获得足够实测数据后发布新的陆地/统一水体参数版本；`land-water-v1` 在此之前保持可复现，不静默修改。
- 经复核并签名的固定 DEM/WBM 大小和 SHA-256 发布清单。
- 全中国区域的缓存预算、区域管理体验和 2.5 GB 边界压力恢复；后端不做静默淘汰。
- Windows 桌面整机是否继续达到四核 60 秒目标；Linux 计算核心与 Windows 交叉 production build 已通过。
- release 优化构建下的覆盖层反向重采样和第二张 PNG 编码耗时，以及 Windows 10/11 WebView2 中 MapLibre 的实机几何回归。
- Windows WebView2 中下载确认、事件进度、取消、Range 续传和区域删除的完整端到端回归；共享 Rust 核心与浏览器界面已分别通过。
- 合规底图就绪后的正式地图离屏渲染和生产导出硬门槛。
- 面向中国大陆公开发行时的合规底图和审核路线。

## 18. UI 主题与地图显示约束

- 提供 `system`、`light`、`dark` 三种主题设置；`system` 为初始值。
- 热力图颜色表在两种主题中完全相同，不根据背景自动反转。
- 合规底图提供浅色和深色样式；若授权数据只提供一种样式，则另一主题只改变应用外壳，地图本身保持授权样式。
- 地图禁止添加 hillshade、terrain、contour、slope、elevation raster 或 3D terrain 图层。
- DEM 只能由 Rust 计算服务读取，前端地图接口不能直接请求或显示 DEM 瓦片。
- 发射点信息可以显示该点自动读取的海拔；这不构成地图高程可视化。
