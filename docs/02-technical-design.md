# HamHeatmap 技术设计草案

- 文档版本：0.1-draft
- 日期：2026-07-16
- 对应需求：`01-product-requirements.md`

## 1. 技术目标

构建一个 Windows 10/11 64 位离线优先桌面应用。前端负责地图、参数表单、进度和导出预览；Rust 后端负责数据缓存、DEM 读取、坐标计算、ITM 调用、并行调度、结果栅格化和文件导出。

优先保证：计算可复现、地形确实影响结果、UI 不冻结、离线数据行为明确、数据和模型版本可追踪。

## 2. 技术栈

- 桌面框架：Tauri 2.11.5。
- 前端：React 19.2.7 + TypeScript 7.0.2 + Vite 8.1.4。
- 地图：MapLibre GL JS 5.24.0；开发构建使用无网络资源、无行政边界的 WGS84 空白坐标画布。
- 前端工具链：项目内固定 Node.js 24.18.0，不依赖 JAIST 主机全局 Node。
- 后端：Rust stable。
- 传播核心：NTIA ITM C++ 源码，以本地静态库或 DLL 方式绑定。
- 并行：Rust Rayon 或等价线程池。
- 本地数据库：SQLite。
- 栅格读取：纯 Rust `tiff 0.11.3`（只启用 Deflate）读取本地 GLO-90 DEM/WBM GeoTIFF，不引入 GDAL。计算核心已经完成统一全局栅格寻址、跨边界双线性高程插值、分类水体采样和 200 km 全圆计算；正式缓存索引、原子下载、断点续传和用户发起删除已经通过 Phase 1 验证。
- 导出：前端固定离屏 Canvas 生成 1600×1100 内部诊断报告；`hamheatmap-export` 校验 PNG、用 `printpdf 0.11.1` 封装 A4 横向 PDF 并原子保存。正式合规地图导出仍待底图授权。
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

- `MapView`：合规底图、发射点、200 km 圆、不可检查的热力图和图例；不渲染高程视觉层。
- `ParameterPanel`：场景、频率、功率、增益、高度、极化。
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

### 4.2.1 私有 validation server 操作协议

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

该协议只用于回环 validation 平台。Tauri 继续使用原生事件和桌面 operation lease，普通 preview 继续禁止真实操作。设计依据见 ADR 0013；新构建、真实回环烟雾和浏览器可见进度需要另行记录后才能宣称通过。

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

WBM 只参与传播计算，不作为可见底图。正式底图仍通过 `CompliantBasemapProvider` 接入，要求具有有效审图信息和明确的桌面离线授权；不使用 OSM 标准瓦片做离线下载。

### 6.3 地形采样

- DEM 剖面采样间距初始使用原生约 90 m。
- 瓦片边界使用一致的双线性插值规则。
- WBM 采用包含采样点的原始分类像素，不对分类值做双线性插值。
- 经 DEM/WBM 成对 `404` 确认的纯海洋单元高程为 0 m、水体为真；其他水面仍读取正式 DEM/WBM，不能把一般缺数据误当水面。
- NoData、瓦片损坏或版本混用均作为计算阻断错误。
- 发射点海拔可由用户覆盖；接收点海拔不可全局覆盖。

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
- 当前会话只保留一个完整结果。
- 同一结果包含两个固定 `401×401` 渲染产品：局部等距原始 PNG 用于内部报告；反向重采样、轴对齐 EPSG:3857 PNG 用于 MapLibre。
- 地图覆盖层元数据显式记录 `EPSG:3857`、宽高和 WGS-84 四角；四角对应扩展半个像素后的图像外边缘。
- 重采样只消费内存中的 dBm 栅格，不重新运行 ITM，也不把 Web Mercator 像素回写为计算结果。

## 10. 缓存设计

### 10.1 配额

- 全部持久数据硬上限：2,500,000,000 字节（十进制 2.5 GB），不可配置。
- 分类：基础地图、DEM、水体、下载临时文件、计算缓存。
- SQLite 保存瓦片 ID、范围、版本、大小、校验和、最后使用时间和状态。
- 写入前执行配额预检；没有足够空间时不开始下载。
- 用户删除正在使用区域时必须先取消相关计算。
- Phase 1 实现以整个应用数据根目录的实际文件长度为准，而不是只相信 SQLite 记账；索引、锁文件、未登记文件和 `.partial` 都计入 2.5 GB。下载另保留最多 16 MB 的索引/事务安全余量，用户不可配置。
- 配额不足与文件系统可用空间不足分别返回错误；不自动淘汰旧区域。

### 10.2 下载完整性

- 临时文件使用 `.partial` 后缀。
- 支持 Range 断点续传时恢复，否则重新下载单个瓦片。
- 下载完成后验证 Content-Length 与已知校验和。
- 原子改名后才把数据库状态标为 ready。
- 应用异常退出后，下次启动清理失效的 partial 文件。
- 下载 URL 由瓦片 ID 内部生成，只允许固定的 AWS GLO-90 HTTPS 主机；拒绝用户信息、查询参数、片段和相似域名。
- AWS 公开对象提供 Content-Length 和 Range，但当前没有逐瓦片、经认证的 SHA-256 清单。首次下载以 HTTPS、域名白名单和长度验证为基础，完成后记录本地 SHA-256，之后每次计算前复核。公开发行前仍应生成并签名应用自己的固定版本清单。
- 每个地理单元同时规划 DEM 与 WBM。只有同单元的两个固定 URL 都返回 `404` 才生成纯海洋资产；单边 `404`、超时或服务器错误均不可降级。生成资产也经过 SHA-256、原子提交和配额核算。

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
- 下载 URL 必须在允许域名列表内并使用 HTTPS。
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
- 离线打开已缓存区域。
- 离线选择未缓存区域。
- 计算中取消。
- 达到和试图超过 2,500,000,000 字节硬上限。
- 清空地图不删除缓存。
- PNG/PDF 导出。

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
