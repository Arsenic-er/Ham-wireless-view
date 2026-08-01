# HamHeatmap MVP 测试计划

- 文档版本：0.2-draft
- 日期：2026-08-02

## 1. 测试层级

1. Rust/C++ 单元与回归测试。
2. 合成地形集成测试。
3. 真实 DEM 区域测试。
4. Tauri 桌面端到端测试。
5. Windows 10/11 64 位实机测试。
6. 数据许可、署名和地图合规发布检查。

## 2. 模型测试

- NTIA ITM 官方 point-to-point 样例全部通过。
- 链路原始 DEM PFL 只调用一次 ITM；显示曲率不写回 PFL，P.526/F1 诊断不追加第二份绕射损耗。
- 合成 200 km 路径在 `k=4/3`、`Re=6,371,008.8 m` 时中点隆起为 `588.6 ± 0.5 m`。
- 200 km 中点 F1 半径在 145.00/435.00 MHz 时分别为 `321.5 ± 0.2 m` 与 `185.6 ± 0.2 m`；端点 F1 为 0 且不参与归一化净空最小值。
- 几何射线、60% F1 和 margin 边界覆盖等号两侧；`margin=0` 预计可用，略低于 0 为 `predicted-unavailable`。
- 功率 W/dBm、增益 dBi/dBd 换算误差小于 `1e-9`。
- 发射功率增加 10 倍时结果整体增加约 10 dB。
- 水平/垂直极化正确映射到 ITM `pol=0/1`。
- 平地距离增加时接收功率总体下降。
- 单山脊后出现附加衰减。
- 提高天线高度能够改善至少部分遮挡像素。
- 144 MHz 与 430 MHz 输出存在合理差异。
- 全陆地与全水体采用不同参数并产生差异。
- 海洋、湖泊、河流输入均归并为同一 `water` 类别。
- 中心像素无接收结果；最近的 1.0 km 网格像素由 ITM 成功计算且无 warning，不拼接自由空间模型。

## 3. 网格与地理测试

- 网格固定 `401 × 401`、1 km 间距。
- 链路 WGS84 inverse 距离和 direct 样本与权威 GeographicLib 结果一致；仅接受 `1,000..=200,000 m`。
- 链路样本数为 `ceil(D/90 m)+1`、间隔不超过 90 m且首尾点精确等于 TX/RX；200 km 路径约 2,224 个样本。
- 链路地图连接线按测地线分段，Web Mercator 屏幕距离不进入剖面计算。
- 只保留距中心不超过 200 km 的像素。
- 圆外结果透明且不能导出为有效像素。
- 发射点必须位于合规中国大陆有效区。
- 海岸和边境站点仍生成完整 200 km 圆。
- 接收点海拔逐像素从 DEM 取得。
- 发射点手动地面海拔范围为 `-500..=9000 m AMSL`；字段缺失/`null` 与 DEM 自动兼容。
- 手动模式仍读取并验证中心 DEM，只替换每条 PFL 首样点；AGL、其余 DEM 和 WBM 采样不变。
- 当前计算结果 schema 4 冻结有效地面海拔与 `dem/manual` 来源并携带显示筛选 bins；bootstrap schema 保持 2，preview schema 保持 1。
- 原始报告栅格保持 `401×401` 局部等距样本；地图另生成 `401×401`、轴对齐 EPSG:3857 覆盖层。
- 地图图像边界相对首末输出像素中心各扩展半个像素；精确 200 km 连续边界点必须位于图像域内，199 km 内侧 alpha 可见。若最近像素中心透明，`3×3` 邻域内最近可见中心误差不得超过该处 WGS-84 实算一个输出像素对角线。
- 纬度 18°、30.5°、40°、54° 的代表性覆盖层定位误差均小于 1 km。
- 反向重采样对有限邻域重新归一化；圆外、原始 NaN、原始域外和无有限贡献像素透明。

## 4. UI 测试

- 首次启动默认跟随系统主题。
- 覆盖/链路顶层模式切换不重建地图，不把覆盖发射点当链路 TX，也不清除另一模式结果。
- 链路第一次地图单击设置 TX、第二次设置 RX；标记和连接线可区分，越界保留 TX 并允许重选 RX。
- “清空链路”只清 TX/RX、连接线、SVG 与链路结果；“清空热力图”只清覆盖层/预览，两者互不影响。
- SVG 显示曲率抬高后的地形、端点直线射线、完整 +/-1.0 F1、0.6 F1 边界、TX/RX 和最小净空点；游标显示原始 DEM 与地球隆起，Y 轴为 m AMSL 并提示纵向比例放大。
- 动态距离轴始终包含 0 和 D，按可视宽度选择 5–9 的目标密度并使用 1/2/5 倍数步长；小于 10 km 的路径显示 m，其余显示 km。
- SVG 游标显示完整权威样本的距离、地形、射线、F1 和净空；降采样只影响绘制，不改变游标、最严重点或分类。
- 浅色/深色切换即时生效并持久保存。
- 两种主题下色标数值和颜色顺序一致。
- 底图不存在 hillshade、等高线、坡度、高程着色或 3D terrain。
- 热力图像素不响应 hover/click，也不存在像素查询命令。
- MapView 只使用独立 Web Mercator 覆盖层字段；内部报告继续使用原始局部等距 PNG，二者不能误接。
- 发射点信息显示坐标、海拔、Maidenhead 与缓存状态。
- 参数变化后旧结果变淡、显示过期状态且禁止导出。
- 计算时参数锁定；取消后恢复。
- Rust worker 在接收点之间和长剖面采样期间响应取消；取消后不编码或保留半成品。
- 场景预设保留手动地面海拔；选择新点重置 DEM 自动、清除当前预览/导出身份，但保留其他已完成会话覆盖层。
- 不同发射点完成后累积独立覆盖层；完全同点重算替换旧层；最多 8 项，第 9 项淘汰最早项。
- 清空删除全部会话覆盖层，保留当前发射点、参数、地面海拔模式/值、全局显示阈值和缓存，同一点可立即重算。
- 全局阈值游标范围 `-140..-60 dBm`、1 dB 步长、默认 `-140 dBm`；最多 8 个已完成层同步筛选，低于阈值的像素透明。
- 每个已完成结果使用独立 MapLibre CanvasSource/layer；最新层置顶，历史站点可见，重叠不解释为联合场强。渐进预览仍保持 image source/Blob lease。
- 样式暂时未就绪时的清空不得丢失；style 恢复后必须删除全部旧 heatmap layer/source 并逐项释放 Blob URL。
- 地图右下显示随缩放和平移变化的公制比例尺，左下发射点坐标不被遮挡。
- DEM 自动/手动界面显示 DEM 参考值与有效天线 AMSL，且不把 AMSL 误当新的传播输入。
- Windows 在线地图设置分别显示“配置已保存”和代表瓦片连接自检结果；保存失败、自检失败与自检通过不能共用误导性成功文案。
- 自检只能由用户显式触发；重新测试、清除配置、busy 阻断和关闭重开均保持确定状态，普通 preview 与 validation-server 不暴露该入口。
- 自检响应严格为 schema 1 固定状态集合，非法 schema/状态 fail closed；界面不显示 Key、上游 URL、响应正文、路径或供应方内部错误。
- validation-server 模式中，计算与下载进度由约 250 ms 的非重叠状态轮询驱动，并复用 Tauri 已有的进度监听接口。
- 多标签页同时存在时，取消只影响本页 exact operation ID；旧 ID、旧 generation 或迟到 poll 不能覆盖新任务进度/结果。

## 5. 数据与缓存测试

- 缺数据且在线时先显示预计大小，再经用户确认下载。
- 链路数据计划只包含路径和插值所需 DEM/WBM 单元；仍使用正式缓存引用、原子提交与十进制 2.5 GB 配额。
- 链路路径任一样本缺 DEM/WBM、NoData、损坏、版本混用或单边 404 都 fail closed，不生成部分 SVG 或三类结果。
- 缺数据且离线时禁止计算。
- 下载 Agent 强制 HTTPS-only、零重定向和有限 DNS/连接/发送/响应/总超时；HEAD 只有 200 可作为存在对象元数据。
- 下载中断后可恢复或安全重下单个瓦片，不自动重试。
- 取消、响应体读取错误和 early EOF 都在返回前同步 partial 并更新 SQLite 实际长度。
- 只有强 ETag、Range、期望大小与磁盘/索引长度全部一致才续传。
- 同一地理单元的 DEM/WBM 成对 `404` 时生成可校验纯海洋资产；单边 `404` 或其他错误必须阻断。
- 校验失败的数据不能进入 ready 状态。
- 临时文件计入配额。
- 持久数据总量永不超过 2,500,000,000 字节。
- 空间不足和配额不足分别提示。
- 删除区域后对应离线计算失效，其他区域不受影响。
- 缓存管理不展示 DEM 图像或高程预览。

## 6. 导出测试

- 内部诊断 PNG 固定为 `1600×1100`，包含完整 200 km 圆、发射点、100 km 比例尺、精确 dBm 色标、输入、冻结的有效地面海拔/`dem|manual` 来源、统计、版本、时区、限制和不可移除水印。
- validation 模式的 PNG/PDF 必须由浏览器本地生成并通过 Blob 下载；不得请求服务器导出路由、上传报告正文或提交目标文件路径。
- 浏览器 PDF 必须具有 `%PDF-1.4` 头、单页 Pages tree、JPEG XObject、有效 xref/startxref 与 EOF；非法 JPEG 或尺寸必须拒绝。
- 多层会话只导出当前最新且未过期的单个已完成结果及其冻结参数，不导出视觉叠层合成图。
- 地面海拔及来源必须来自计算结果 schema 4，不得在导出时用当前表单或重新读取 DEM 替换。
- 调整地图显示阈值不得改变诊断 PNG/PDF 字节、统计摘要或可导出结果身份；导出继续使用未筛选权威结果。
- 内部诊断 PDF 为 A4 横向单页，嵌入同一报告 PNG，解析后页数和页面尺寸正确。
- 非 PNG MIME、非法 Base64、非 `1600×1100` 图像和超限负载均被 Rust 拒绝。
- 保存取消不创建文件；写入失败不覆盖已有目标且不留下临时文件。
- 浅色/深色主题均可导出；热力图颜色不变。
- 参数过期、计算取消或数据错误时禁止导出。
- 导出图没有像素查询信息或交互残留。
- 正式地图 PNG/PDF 只有在在线底图供应者清单同时具备审图号、署名、热力图叠加授权和所需导出授权时才能启用。

## 7. 性能测试

基准硬件为普通四核 Windows 10/11 64 位电脑：

- 已缓存区域目标 60 秒内完成。
- 已缓存数据的 200 km 单链路目标 2 秒内完成，分别记录 WGS84、DEM/WBM、ITM、曲率/F1、序列化和 SVG 首绘耗时。
- SVG 游标、窗口 resize、主题/语言切换和动态刻度不得触发 analyze-link IPC/HTTP、ITM 或 DEM/WBM 重读。
- UI 事件响应延迟目标小于 100 ms。
- 进度至少每秒更新一次。
- 计算峰值内存目标小于 2 GB，最终以基准结果确定。
- 1、2、4、8、16 线程记录剖面提取、ITM、渲染和总耗时。
- 阈值连续拖动采用 30 fps 上限；记录 1 层与 8 层 `401×401` alpha 更新耗时、掉帧和主线程长任务。每帧目标不超过 33 ms，且拖动不得产生 calculate IPC/HTTP、PNG 重编码或 source/layer 重建。

性能目标未达成时优先优化缓存、分块、剖面复用和 FFI，不改变 1 km 输出要求。

## 8. 隐私与安全测试

- 抓包确认坐标、功率和结果不上传。
- 下载只访问白名单 HTTPS 域名；合法初始 URL 的 3xx 也必须拒绝，不能跟随到其他主机或协议。
- Tauri 前端不能执行任意命令或读取任意文件。
- 损坏清单、路径穿越和超大文件被拒绝。
- 日志不记录不必要的精确坐标；诊断导出需用户主动操作。
- operation ID 必须由服务端 CSPRNG 生成，保持 UUIDv4 形状；客户端提交自选 ID、错 kind、过期或重复消费均被拒绝。
- status/cancel/ack 只接受 POST JSON exact ID，不提供 current/list 或 URL 查询 capability；状态响应不含 PNG、URL、路径和详细错误。
- reserved/terminal 状态分别验证 60 秒/5 分钟 TTL、32 项上限和 ack 回收；未知、错 family 或终态取消返回 false 且不改变 active。

## 9. 地图合规测试

- 生产构建必须具有非空审图号、来源和授权标志。
- 生产构建不包含 Natural Earth/OSM 开发边界包。
- 中国边界、重要岛屿和省级边界来自同一合规底图源。
- 浅色、深色样式分别列入发行清单。
- 地图界面、PNG、PDF 均显著显示必要署名和审图号。
- 未通过合规门槛的构建只能标记“内部测试版”，不能生成公开发行包。

## 10. MVP 退出标准

- 所有 P0/P1 测试通过。
- Web Mercator 覆盖层在代表性中国纬度满足 `< 1 km` 样本中心门槛；精确 200 km 连续边界位于半像素扩展域内，并按一个输出像素对角线容差验收栅格化 alpha；原始报告 PNG 保持不变。
- 无未处理 ITM error；warning 有统计和解释。
- 三个真实区域案例通过人工审查：平原、山区、沿海。
- Windows 10 和 Windows 11 至少各完成一次安装、离线计算和导出。
- 2.5 GB 配额压力测试通过。
- 双点链路的解析几何、ITM/预算、三分类、跨极化、状态隔离、四语言和 SVG 门禁全部通过。
- 数据许可证清单完成。
- 地图授权、审核和审图号完成；否则只能发布源代码和无底图内部测试构建。

## 11. 最小可行性检查点（2026-07-16）

成都内陆 200 km 案例已经通过计算核心检查：25 个真实 GLO-90 瓦片、125,628 个有效像素、无 ITM error 或未知传播模式、四线程单场景小于 9 秒、峰值内存约 152 MiB。4 与 16 线程生成的四张 PNG 哈希完全一致；真实地形/平地、145/435 MHz、20/80 m 发射高度对照均产生可解释变化。完整数据见 `06-minimum-viability-validation.md`。

这只关闭“算法能否以真实地形生成稳定热力图”的风险，不替代本节前述完整 MVP 退出标准。Windows、UI、缓存配额压力、导出和地图合规仍未验收。

## 12. Phase 1 缓存检查点（2026-07-16）

- 成都中心精确规划为 25 个瓦片，在线 HEAD 预计量为 132,164,681 bytes。
- 历史瓦片经整体 GeoTIFF 解码、原子迁移和逐文件 SHA-256 后，索引显示 25 ready、0 partial、0 corrupt。
- 实际数据根总量 132,205,641 bytes，其中 DEM 132,164,681 bytes、索引和其他元数据 40,960 bytes，未突破 2,500,000,000 bytes。
- 单瓦片真实 HTTPS 测试覆盖首次下载、原子改名、SQLite 入库、SHA-256 和 DEM 解码；第二次测试在写入 partial 后取消，再以 Range 续传完成。
- 缓存入口完整四场景运行 31.91 秒、峰值 RSS 157,920 KiB，四张 PNG 哈希与迁移前一致。
- 配额拒绝、校验失败、共享瓦片引用、活动区域删除保护、相似域名拒绝均有自动化测试。

结果见 `07-phase1-cache-validation.md`。Windows 文件锁/原子改名、UI 取消、全量 2.5 GB 压力和签名数据清单仍未关闭。

## 13. 陆地/水体检查点（2026-07-16）

- WBM 生产读取器通过真实 GLO-90 8-bit GeoTIFF、分类值折叠、地理配准和未知值拒绝测试。
- 全陆地、全水体及混合比例精确映射到 `land-water-v1` 参数端点和线性混合公式；1/2/5/10/20/50 km 多距离 ITM 对照证明两类参数能产生差异。
- 三项显式网络测试覆盖真实 DEM/WBM 成对下载、Range 续传，以及纯海洋单元成对 `404` 后生成并用生产读取器解码。
- 成都内陆案例的平均路径水体比例为 0.01285；相对全陆地对照的平均变化为 +0.02255 dB，证明少量河湖不会造成不合理的整体跃变。
- 青岛沿海案例的平均路径水体比例为 0.52999，125,044 条路径受水体影响；相对全陆地对照平均 +0.49804 dB，单像素最大增益 +15.96344 dB、最大损失 -1.38491 dB。
- 青岛五场景四线程完整运行约 52.01 秒、峰值 RSS 159,932 KiB；仍满足 Linux 计算核心 60 秒阶段目标。
- 成都与青岛缓存合计 90 个 ready 资产，总根目录 186,287,679 bytes、0 partial、0 corrupt，未突破十进制 2.5 GB 硬上限。

完整结果和输出 SHA-256 见 `08-land-water-validation.md`。这关闭了首版水体来源、纯海洋缺瓦片处理、统一水体参数与沿海混合路径的工程风险；真实传播精度仍需未来外场测量校准。

## 14. Phase 2 桌面首切片检查点（2026-07-16）

- React/TypeScript 生产构建通过，8 个前端测试覆盖 WGS-84 200 km 圆、图像四角、Maidenhead、两种预设、频段默认值、单位往返和输入范围。
- 浏览器视觉回归覆盖默认主题、浅色主题、地图点选、固定覆盖圆、基地台/手台预设、144/430 频段切换和缓存概览；控制台 0 error/warning。
- 在 Tauri 最小窗口 `1080×700` 下检查页头、地图、参数滚动、固定操作区和色标；色标早期溢出已通过 1180 px 响应式断点修复。
- Rust 工作区共 33 个默认测试通过；新增测试覆盖桌面请求映射、dBm/dBd 单次归一化、频段契约、图像四角、精确 2.5 GB 上限、预取消和运行中取消。
- 通过 `hamheatmap-app-service` 对成都真实缓存完成 125,628 像素计算，约 9.75 秒生成 224,378-byte PNG 数据 URL。
- Tauri Windows production build 已通过项目内 cargo-xwin/LLVM；生成 64 位 PE 应用和内嵌 WebView2 的 NSIS 离线安装包，最终 EXE 未导入动态 MSVC/UCRT 运行库。
- 交叉构建不替代 Windows 原生 QA。Windows 10/11 的 WebView2、文件目录、原子操作、安装/卸载、代码签名和整机性能仍是下一检查点。
- 完整构建证据、哈希与边界见 `11-windows-cross-build.md`。

完整记录见 `09-phase2-desktop-slice.md`。该检查点当时尚未验收区域下载/删除、热力图最终地图重投影和 PNG/PDF 导出；区域下载/删除已由下一节与 `10-phase2-download-cache-slice.md` 补充，内部 PNG/PDF 已由第 16 节补充，地图重投影已由第 17 节补充。合规有效区仍未验收。

## 15. Phase 2 下载与缓存管理检查点（2026-07-16）

- Rust 工作区 35 个默认测试通过；新增区域概览、共享资产可回收字节和下载进度契约测试。
- 三项真实 HTTPS 测试 3/3 通过，覆盖首次下载、取消后 Range 续传和纯海洋成对 `404` 生成。
- 成都 50 个 ready DEM/WBM 资产通过桌面服务估算和零下载 ready 状态转移，缓存总量保持 `186,287,679 bytes`。
- 下载开始前执行整批 2.5 GB/磁盘预检；前端不提交 URL、大小、ETag 或哈希。
- 浏览器视觉回归覆盖下载确认和缓存管理弹窗，`1080×700` 无溢出，控制台 0 error/warning；浏览器不执行真实下载。
- 区域删除展示引用字节和实际可释放字节，只回收引用数归零的资产，不自动淘汰。

完整记录见 `10-phase2-download-cache-slice.md`。Windows WebView2 中的真实确认、事件、取消、续传、删除和接近 2.5 GB 压力仍未验收。

## 16. Phase 2 内部诊断导出检查点（2026-07-16）

- 前端固定报告渲染器与导出弹窗完成；报告不是当前窗口截图，参数、结果和生成时间在开始时冻结。
- `hamheatmap-export` 独立 Rust crate 完成严格 PNG 验证、A4 PDF 编码、安全文件名和同目录原子替换。
- Tauri 只在 Rust 命令内打开 Windows 原生保存对话框，WebView 未获得宽泛文件系统权限。
- 内部报告不含行政边界、审图号或未经授权底图，强制显示“内部测试，不得公开发布”。
- 前端 11 个测试、导出核心 6 个测试、Rust 工作区 47 个默认测试、TypeScript 检查和 Windows MSVC 目标检查均通过。
- Windows production EXE/NSIS 重建和静态 CRT 导入表检查已通过；WebView2 原生保存流程和另一台 Windows 电脑打开产物仍待验证。

完整记录见 `12-phase2-export-slice.md`。正式地图导出继续受在线底图、审图号、署名、热力图叠加授权和所需导出授权硬门槛约束。

## 17. Web Mercator 地图覆盖层检查点（2026-07-24）

- 旧 MapLibre 四角两三角映射已按其 Web Mercator 仿射插值过程复核；纬度 18°–54° 的最大误差为 2.035–8.587 km，确认不能满足 1 km 门槛。
- ADR 0011 固定双栅格方案：原始 `401×401` PNG 保留给内部报告，地图使用单独的 `401×401` 轴对齐 EPSG:3857 反向重采样覆盖层。
- 自动化验收必须覆盖半像素边界、四角轴对齐、代表性纬度 `< 1 km`、199 km 内侧 alpha、精确 200 km 连续边界的单像素栅格容差、NaN/圆外透明、确定性编码和前端字段分离。
- 新覆盖层在纬度 18°、30.5°、40°、54° 的最大定位误差分别为 711.655 m、716.127 m、725.742 m、739.625 m；总体最大值 `739.625 m < 1 km`，四个代表性纬度全部通过。
- `map_overlay_pixel_sampling_matches_absolute_affine_field_and_image_uv`、`calculation_result_serializes_all_overlay_fields_in_camel_case` 和前端 `buildMapOverlayImageSpec` 测试分别关闭绝对仿射 dBm/MapLibre UV、真实 14 字段 camelCase 序列化和地图/报告字段分离风险。
- 精确 200 km 连续边界点全部位于半像素扩展图像域内；部分边界的最近像素中心透明，`3×3` 邻域最近可见中心最差为纬度 18°向南 `1012.102 m`，小于该处 WGS-84 实算一个输出像素对角线 `1431.578 m`。
- Rust 工作区 `57 passed`、`0 failed`、`3 ignored`（显式联网 GLO-90）；rustfmt、`scripts/cargo-project.sh clippy --workspace --all-targets --locked --offline -- -D warnings`、TypeScript 检查、前端 4 文件/13 测试、Vite production build 和 `git diff --check` 全部通过。本检查点已关闭。
- release 优化构建下的覆盖层重采样/第二张 PNG 编码耗时，以及 Windows 10/11 WebView2 中 MapLibre 实机几何仍未验收，不能由本检查点推断为已关闭。

验证方法、旧方案数值和结果表见 `13-web-mercator-overlay-validation.md`。

## 18. GPU-273312 Windows 交叉构建检查点（2026-07-24）

- 使用固定项目内工具链和 `scripts/tauri-windows-cross.sh -- --locked` 完成 x64 MSVC production 构建。
- 独立 EXE 为 16,061,952 bytes，NSIS 离线安装包为 211,439,966 bytes；SHA-256 已记录在 `14-windows-cross-build-gpu273312.md`。
- 应用通过 AMD64、Windows GUI、ASLR、高熵 ASLR 和 NX 检查；15 个导入 DLL 均为 Windows 系统组件，不含动态 MSVC/UCRT 运行库。
- NSIS 只包含预期插件、x64 应用、第三方许可和 WebView2 离线安装器；不含源码、构建工具、DEM 或地图缓存。
- `scripts/verify-windows-artifacts.sh` 自动检查 PE、证书表、导入表、包内容和 Tauri 三字节 bundle marker，并安全清理解包目录。
- Rust `57 passed`、前端 13 项测试通过；release 覆盖层重采样与 PNG 编码约 `0.335 s/张`。
- Windows 10/11 实机、断网安装、代码签名、编码阶段取消、真实 MapLibre 几何和地图合规仍未完成，不能公开发布。

完整证据见 `14-windows-cross-build-gpu273312.md`。

## 19. 私有服务器验证平台检查点（2026-07-24）

该检查点只验证 `127.0.0.1:1421` 回环服务经 SSH 隧道复用真实 Rust 核心的内部开发路径，不替代 Windows/Tauri、公开 Web 服务或地图合规验收。

### 19.1 已确认的代码回归

- `hamheatmap-validation-server`：11/11 通过，覆盖 CLI 非回环拒绝、路径/MIME、请求上限、唯一且匹配监听地址的 `Host`、单任务门闩与取消线性化、目录隔离、路由 fail-closed、JSON 契约、取消 POST 的 JSON 媒体类型、安全响应头、HEAD 语义，以及 CSP 允许 `data:`/`blob:` 热力图而不放宽外部网络源。
- 前端：26/26 通过，包含 preview/validation-server/Tauri 三态能力、Tauri 优先级、同源适配器、无体取消 POST 的 JSON 媒体类型、远程处理横幅、服务器计算启用、导出禁用、取消期间阻断新操作、清空后保留发射点并可重算，以及 MapLibre Blob URL 的 PNG 校验、复用和生命周期释放。
- `scripts/validation-platform.sh`：`bash -n` 和 `git diff --check` 通过；运行资源固定在 `.runtime/validation-platform/`。
- validation server 没有导出路由；未知 `/api/export-result` 由路由测试确认为拒绝，浏览器能力测试确认 `canExport=false`。

### 19.2 运行与安全检查

- [x] `scripts/validation-platform.sh build` 生成带 `VITE_VALIDATION_SERVER=1` 的 `app/dist` 与 release server。
- [x] 基于 revision `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d` 完成 `stop → build → start → status/health → bootstrap/cache-overview`；旧 PID `214692` 已退出，新 PID 为 `1114524`。
- [x] 服务只监听 `127.0.0.1:1421`，公网地址直接连接失败；CLI 也会拒绝任何非回环绑定。
- [x] Windows 端通过 SSH 本地转发打开 `http://127.0.0.1:1421`，浏览器访问地址为本机 `127.0.0.1`。
- [x] 重复 `start` 保持 PID `1114524`；直接 `__run` 被 runner claim 拒绝，严格 stop 只终止已验证的旧 PID `214692`。
- [x] PID、数据、日志和 build metadata 均收敛在 `.runtime/validation-platform/`，本次未创建 Docker、systemd、Caddy/Nginx 或系统级项目存储。
- [ ] 日志轮转阈值与请求取消日志脱敏尚未进行压力验证。
- [x] validation 远程处理横幅在 1080×700 浏览器验收中可见，并有自动化内容测试。
- [x] validation 模式浏览器导出保持禁用，服务端不存在文件路径或导出端点。
- [x] 缺失/重复/错误端口/外部 `Host` 被拒绝；取消端点要求 `application/json`，阻断普通跨站表单的简单 POST。

### 19.3 成都真实数据与计算

中心为 `(30.5, 103.5)`。首次真实数据准备已确认：25 个 DEM、25 个 WBM ready，本次下载 `132,997,688 bytes`，当时数据根总量 `133,063,224 bytes`，中心高程 `526.3443 m`；浏览器缓存对话框显示 `133.1 MB / 2.50 GB`。
2026-07-27 的应用进程重启恢复检查中，缓存总量在重启前后均为 `133,071,416 bytes`、partial 为 0，两个缓存区域各 `50/50 ready`。

- [x] 真实 DEM/WBM 数据准备和中心高程读取。
- [x] 修复 `band` JSON 契约后完成真实 `/api/calculate`。
- [x] 相同输入重复计算的原始 heatmap、地图覆盖层和非耗时统计保持确定。

确定性哈希：heatmap 两次均为 `1e64b5c0c95ba12c5ed52589304df66343b9d2c5f3d48288d3b7250c92f610a7`，overlay 两次均为 `e41b715614045b09a863956e46a0111e4e3761ade29a9eeff069a266ccc5b542`，非耗时统计两次均为 `c7d45d6e69db14f72e80017991bf0a37c6cfc2f59ab9694d178e550bd98a0ea3`。
- [x] 真实 HTTP 取消返回 `cancelled=true`；被取消计算为 HTTP 422 且不含双 PNG。随后相同请求 HTTP 200，`schemaVersion=2`，两组尺寸均为 `401×401`，两个唯一且非空的 data URL 均通过 Base64 与 24-byte PNG/IHDR `401×401` 验证。

真实计算请求：中心 `(30.5,103.5)`，`145.00 MHz`，`25 W`，发射天线 `6 dBi / 20 m`，接收天线 `-3 dBi / 1.5 m`，垂直极化。共享 Coverage / NTIA ITM 路径返回 `401×401` 原始 heatmap 和 `401×401` EPSG:3857 map overlay。

| 统计字段 | 结果 |
|---|---:|
| valid | 125,628 |
| below | 77,496 |
| warning | 99,214 |
| min | -250.14908 dBm |
| max | -41.75736 dBm |
| mean | -146.670031115 dBm |
| water | 109,817 |
| meanWater | 0.0128492517 |
| propagation | 2.696916 s |
| total | 8.311013 s |
| HTTP response | 407,060 bytes |

完整响应体 SHA-256：`4d219a120ef38ad9eb3c2cf5bd0b939ffe247bee123ca1768b124b10c67468f6`。

### 19.4 浏览器视觉

- [x] 在本机 SSH 隧道地址 `127.0.0.1:1421` 完成浏览器验收。
- [x] `1080×700` 窗口下浅色和深色主题均通过。
- [x] 真实 heatmap 可见，并随地图缩放和平移保持联动。
- [x] 缓存对话框显示 `133.1 MB / 2.50 GB`。
- [x] 服务器模式下导出禁用。
- [x] 清空只移除 heatmap，保留发射点与就绪数据，同一点可以立即重新计算。
- [x] 页面刷新后重新完成真实计算，界面报告 `8.3 s / 125628 px`；浏览器控制台 error 和 warning 均为空。
- [ ] 完整 200 km 圆边界、圆外透明和无像素检查交互尚未在本轮文字记录中逐项独立确认。
- [x] 内部验证横幅在浏览器截图与 DOM 记录中均可见；自动化测试覆盖其内容。

问题关闭链路：最初 PNG data URL 被 CSP 拒绝；CSP 在不放宽外部来源的前提下加入 `data:`/`blob:` 后，MapLibre ImageSource 仍出现 data URL AJAXError；最终改为带 PNG 签名校验和生命周期测试的 Blob URL。刷新后真实覆盖层正常显示并跟随地图交互。本轮按要求只保留文字证据，没有生成截图。

完整架构、安全边界和命令见 `15-private-validation-platform.md`。即使本节已完成真实计算与浏览器显示，Windows 10/11 WebView2、原生导出、安装包、地图授权/审图号和传播外场精度仍是独立门槛。

## 20. 恢复与取消检查点（2026-07-27）

本检查点针对三类失败窗口：缓存进程在 partial 写入、原子改名或索引更新之间退出；取消与计算成功几乎同时发生；私有平台管理命令因 SSH 中断、进程号复用或陈旧锁而失去所有权证据。详细记录见 `16-recovery-and-cancellation-validation.md`。

### 20.1 缓存恢复与配额边界

- `CacheStore::open` 在执行硬上限判定前先整理索引和文件系统：partial 只能以 SQLite 已持久化的 checkpoint 长度为可信上限，更长尾部截断并同步，更短或缺失时废弃；同时删除 ready/missing/corrupt 资产的陈旧 partial，并把“最终文件已改名、索引仍为 downloading”的资产校验后推进为 ready。
- 只有与 downloading 状态、期望大小、强 ETag 和 Range 能力一致的 partial 才允许续传；不可信或超出期望大小的 partial 被删除或标记损坏。
- 精确到自定义测试上限的可信 partial 可以重开；已检查点后再增长 1 byte 必须拒绝，未检查点尾部则先截回可信长度。区域和资产元数据写入执行预留空间检查，批量区域写入使用事务，失败后不能留下半个区域或孤立资产记录。
- 这些测试使用缩小的测试配额快速覆盖边界；十进制 `2,500,000,000 bytes` 的真实实体数据压力、磁盘耗尽和进程强制崩溃注入仍未完成。

### 20.2 取消结果线性化

- validation server 和 Tauri 各自用带身份的操作 lease 把“接受取消”和“发布成功结果”线性化；若取消标志在 lease 结束前已经被接受，即使 worker 返回成功，也只能向调用方返回取消。
- `AppService` 在传播完成后、两张 PNG 编码之间、Base64 转换之间和最终结果交付前继续检查取消，缩小编码阶段不可响应窗口。
- 前端取消重算时立即丢弃旧 heatmap 并禁用导出；被取消的请求结束后允许干净重试，旧结果不能重新出现。
- `scripts/validation-recovery-smoke.sh` 加固后再次连续通过：运行前无效 inspect 返回 422 确认 gate 为空；只有后台 curl 同时匹配本 shell job、PPID、start time 和 executable，才把活动 gate 认作本测试所有。该证明仍限受控单客户端，不关闭 operation ID 多客户端风险。

### 20.3 管理脚本恢复

- build/start/stop 控制锁记录 PID、`/proc/<pid>/stat` start time 和 boot ID；只有超过初始化保护期且所有者不再存活的锁才可恢复。
- 后台 runner 另持有覆盖其完整生命周期的 claim；PID 文件不再单独作为进程所有权证明。server、runner 和日志 monitor 均要求精确 argv、可执行文件、用户和 start time 匹配后才可发送信号。
- 所有管理路径必须位于项目工作区且路径分量不能是符号链接；自检覆盖陈旧锁/claim 恢复、存活 claim 排他、符号链接逃逸拒绝、精确 argv 和当前托管进程身份。
- `/healthz` 只表示回环 HTTP 进程能响应及协议 schema；它不打开缓存。需要确认缓存锁、重启整理和 2.5 GB 配额可用时，必须调用 `/api/bootstrap`。管理脚本 `health` 不能被描述为数据就绪检查。

### 20.4 证据与未关闭项

- Rust workspace 离线清单合计 `77 passed / 3 ignored`：app-service 12、cache 21、coverage 15、export 6、propagation 6、official reference 1、terrain 5、validation server 11；3 项 ignored 是显式真实网络测试。
- 3 项真实 GLO-90 HTTPS 测试另行 `3/3` 通过；它们不属于离线 `77 passed`。
- 前端 26 项测试通过；其中取消重算与延迟取消测试确认旧结果清除、取消期间阻断新操作、导出禁用和重试恢复。
- Tauri 纯状态控制器 4/4 通过，并完成 Windows xwin 目标编译检查；这不等于 Windows EXE/NSIS 最终重建或 Windows 实机验收。
- `scripts/validation-platform.sh` 已通过 `bash -n` 和 `self-test`。
- [x] 真实 HTTP 长计算取消、无半结果和同请求重算通过。
- [x] 加固版 `stop → build → start → status/health → bootstrap/cache-overview` 全链路通过。
- [ ] 十进制 2.5 GB 实体缓存压力、磁盘不足与进程崩溃注入。
- [ ] GPU 主机整机重启、Windows 10/11 WebView2 和地图合规。
- [ ] operation ID 与 HTTP 渐进进度由第 21 节的新协议切片接管；在新构建证据补齐前不得标记通过。

### 20.5 加固版真实恢复运行

应用基线提交与 validation build metadata revision 均为 `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d`。

| 检查 | 结果 |
|---|---|
| 受管进程切换 | 旧 PID `214692` 经严格 `stop` 退出；`build → start` 后新 PID `1114524` |
| 进程与数据就绪 | `status`、`health`、`/api/bootstrap`、`/api/cache-overview` 全部通过 |
| 缓存恢复 | 重启前后均为 `133,071,416 bytes`、`partial=0`；两个区域各 `50/50 ready` |
| 重复 start | 返回已运行并保持 PID `1114524` |
| runner 排他 | 直接调用 `__run` 被拒绝：`another validation runner is active` |
| 真实取消恢复 | `scripts/validation-recovery-smoke.sh` 连续运行通过 |

真实取消中，取消端点返回 `cancelled=true`；被取消的 calculate 返回 HTTP 422，响应不含 `heatmapPngDataUrl` 或 `mapOverlayPngDataUrl`。随后相同请求返回 HTTP 200，并验证 `schemaVersion=2`、原始图与 overlay 尺寸字段均为 `401×401`、两个 data URL 各唯一且非空、Base64 可解码、解码结果前 24 bytes 为 PNG signature/IHDR `401×401`。

最终 gate probe 不再返回 409，`/healthz` 仍为 200；脚本再次输出 `validation recovery smoke passed: cancel=true cancelled_http=422 recovery_http=200`，且没有 `validation-recovery-smoke.*` 临时目录残留。

本记录验证的是受管应用进程 stop/start，不是 GPU 主机整机重启。整机重启后的手动恢复仍保留为待办。

## 21. Operation identity 与 HTTP 渐进进度检查点（受管 HTTP 已实测）

ADR 0013 用服务端签发的短期 UUIDv4 capability 取代“按 kind 取消当前任务”，并让 validation 浏览器通过状态轮询复用现有进度 UI。第 20 节仍是 revision `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d` 的旧协议历史证据；本节记录 revision `867c25aeb2091055b56d1259f6ad7293d21f7495` 的新证据，二者不混算。

新基线完成 full build，`built_at=2026-07-26T19:02:43Z`，server SHA-256 为 `e80c8890ebcd2059341cd495e78546d51287a916776f0a1991e8d99f062afa0c`。代码门禁结果为 Rust workspace offline `83 passed / 3 ignored`、真实 HTTPS `3/3`、validation server `17/17`、前端 `6 files / 41 tests`（其中 backend 专项 20），并通过 fmt、clippy `-D warnings`、TypeScript check、Vite build、xwin、Tauri 纯状态 `4/4`、`bash -n`、`self-test` 与 `git diff --check`。

### 21.1 Rust 协议回归

- [x] ticket 仅接受 `estimate-download`、`download`、`calculation`，ID 来自服务端密码学安全随机源并具有 UUIDv4 形状。
- [x] 匹配长请求在同一状态锁中原子消费 reserved ticket；busy 不消费，错 kind、过期和重复消费失败。
- [x] reserved 最多 32 项/TTL 60 秒，terminal 最多 32 项/TTL 5 分钟；确定性边界测试覆盖过期和容量回收。
- [x] status 覆盖 `reserved/running/cancellation-requested/succeeded/failed/cancelled`、单调 sequence，以及 estimate-download/download/calculation 三类 tagged 白名单 progress。
- [x] status/terminal 序列化检查不存在结果、PNG/data URL、下载 URL、文件路径或详细错误，也不存在 current/list 路由。
- [x] cancel 同时匹配 exact ID 与 family；未知、错 family、终态和迟到取消返回 200/false，不能影响后来操作。
- [x] ack 按 exact ID 回收 reserved/terminal；重复、未知和过期 ack 幂等返回 false。
- [x] progress、cancel、finish 与 Drop 在同一 mutex 下线性化；取消先到丢弃成功，finish 先到隔离迟到取消，Drop 发布 failed 并释放 gate。

### 21.2 前端回归

- [x] validation 长请求先领取 ticket，再发送带 `operationId` 的包装 body；导出、preview 和 Tauri 能力边界不变。
- [x] 状态轮询使用约 250 ms 递归定时器且不重叠，只分发相同 ID/generation 的新 sequence。
- [x] calculation/download progress 复用现有监听器；旧 poll、临时 poll 错误和旧终态不能污染新任务。
- [x] ticket 未返回前立即取消仍等待并绑定原 handle；取消受 3 秒总 deadline 约束；settle 后停止轮询，final status 与 ack 各有 1.5 秒上限并按 handle identity 清理。
- [x] 同步长响应仍是唯一结果来源；status 成功不能恢复丢失的 PNG 或绕过正式 calculate/download response。

### 21.3 受管服务与浏览器验收

- [x] 完成 `stop → full build → start → status/health → bootstrap/cache-overview`；旧 PID `1114524` 更新为 `1185566`，重复 `start` 后 PID 不变。
- [x] `validation-recovery-smoke.sh` 连续两次通过；两个 ID 均由服务器签发且不同，未知 ID/错 family 返回 false，正确 ID-A 取消返回 true。
- [x] ID-A 的 calculate 返回 HTTP 422 且无双 PNG；terminal 为 cancelled，ack 首次 true、再次 false，随后 status 为 404。
- [x] ID-A 活动时 ID-B calculate 返回 409，ID-B 保持 reserved、`sequence=0`、`progress=null`；随后复用同一 ID-B 成功返回 HTTP 200。
- [x] 旧且已 ack 的 ID-A cancel 返回 false且不影响活动 ID-B；两张唯一、可解码 PNG 均通过 signature/IHDR `401×401` 检查，ID-B terminal 为 succeeded 且不含 PNG，ack 后 status 为 404。
- [x] 两次烟雾均为 ID-A/ID-B 各至少观察到一个真实 calculation progress snapshot，观测时 `sequence=2`；前端自动化另行覆盖轮询非重叠和 generation 隔离。未把该证据写成真实下载字节进度。
- [ ] 通过 SSH 隧道在浏览器确认可见进度、取消屏障、重试和无控制台错误；不得把这项证据外推为 Windows WebView2。
- [x] 运行后 gate 为空、`/healthz` 仍为 200；缓存前后均为 `133,071,416 bytes`、`partial=0`，两个区域各 `50/50 ready`，且没有烟雾临时目录残留。

本节只关闭代码级 operation identity/轮询协议与受管 HTTP 烟雾。浏览器通过 SSH 隧道的可见进度和控制台仍待实测；Windows 10/11 实机、十进制 2.5 GB 实体压力、GPU 整机重启和中国大陆地图合规也仍是独立门槛。

## 22. 发射点地面海拔与有界下载切片（2026-07-27）

本节记录当前实现的自动化与受管运行证据。第 19—21 节中的 `schemaVersion=2` 是相应历史构建的真实结果；当前 `CalculationResult` 已按 ADR 0014 升级为 schema 3，旧数字不被改写或冒充当前协议证据。

### 22.1 已完成的定向测试

- [x] 请求省略 `txGroundElevationOverrideM`、显式 `null` 与有限手动值均按契约反序列化。
- [x] `-500`、`9000 m` 边界接受；越界、NaN 与正负无穷拒绝。
- [x] DEM 自动使用已验证中心样本；手动模式仍执行中心 DEM 读取/有限值验证。
- [x] 手动有效值只替换 PFL 样点 0；后续地形继续由 DEM 提供，WBM 水体采样路径不变。
- [x] 结果 schema 3 包含有限 `txGroundElevationM` 与严格 `dem/manual`；bootstrap schema 2 不变。
- [x] 前端默认 null、模式切换、场景预设保留、新点重置、清空热力图保留和表单边界验证。
- [x] 导出报告读取冻结结果的有效海拔/来源，并在新增参数行后保持固定画布布局。
- [x] HTTP Agent 配置为 HTTPS-only、零重定向和有限分阶段/总超时；HEAD 只接受 200 元数据。
- [x] 脚本化读取错误、early EOF 与取消均同步 partial、更新 SQLite，并保留符合强 ETag/Range 条件的可续传字节。

### 22.2 已完成的全量与真实网络门禁

- [x] Rust workspace 全量：`95 passed`。
- [x] 前端全量：`46 passed`。
- [x] 真实 GLO-90 HTTPS：`3/3`，覆盖首次 DEM/WBM、取消后 Range 续传和成对 404 海洋生成。
- [x] Rustfmt、Clippy `-D warnings`、TypeScript check、Vite production build 与 Windows xwin 目标检查通过。
- [x] `git diff --check` 由本轮文档完成后单独执行并记录。

这些自动化/联网结果证明代码与既有真实来源回归；受管部署和真实计算另由下一节记录，仍不能外推为用户可见浏览器或 Windows 行为。

### 22.3 受管运行证据与尚未完成验收

- [x] 成都真实缓存 DEM 自动值为 `526.3442993164062 m`、来源 `dem`；手动值为 `1500.0 m`、来源 `manual`。两次均为 schema 3，原始热力图与 EPSG:3857 覆盖层各自哈希不同。
- [x] revision `2e4411de809d1f78b6dd1407d51a2351d58b02ed` 完成受管 stop/build/start；PID `1301627` 只监听 `127.0.0.1:1421`，health、bootstrap schema 2、schema 3 recovery smoke 与缓存不变性通过。
- [ ] 通过 SSH 隧道浏览器确认 DEM/手动控件、真实 auto/manual 热力图变化、进度、清空/新点规则、浅/深色布局和控制台状态。
- [ ] 在可控弱网中实际触发 DNS/连接/响应体超时，量化取消延迟并确认用户重试续传。
- [ ] Windows 10/11 WebView2 的表单、下载/续传、计算、原生 PNG/PDF 导出和长路径/文件系统。
- [ ] 十进制 2.5 GB 实体缓存、磁盘不足、强制崩溃、整机重启、日志压力与地图合规。

只有对应证据完成后才能逐项勾选；Linux 自动化、真实 HTTPS 或 xwin check 不能外推为浏览器视觉、Windows 实机或中国大陆地图合规通过。

## 23. 渐进式传播覆盖预览检查点（2026-07-27）

ADR 0016 把预览定义为 best-effort、latest-only、不可导出的临时覆盖层。本节记录功能提交 `a1219c5ca3254a2a40a50829526cd9bd062d8ea9` 与测试工具提交 `88204765182de7e842859e672050614c091f1986` 的证据；旧章节的历史数量不回填。

### 23.1 引擎与应用服务

- [x] 可选像素批次入口不改变无预览 API；批次最多 64 个像素，索引合法、完成像素不重复。
- [x] 并发批次合并后的完整栅格与非批次最终 `CoverageGrid` 完全一致。
- [x] 累计栅格初始 NaN，未完成区域编码透明；最终覆盖层继续由规范完整栅格生成。
- [x] 约 5% 信号阈值、容量 1 非阻塞合并、单编码线程和至少 800 ms 编码间隔由定向测试覆盖。
- [x] preview sequence 与完成数严格递增，100% 预览不发送；编码失败/transport 关闭不使权威计算失败。
- [x] schema 1 只含完成计数和 EPSG:3857 地图 PNG，不含原始报告 PNG、最终统计或导出身份。

### 23.2 validation、Tauri 与 React

- [x] `/api/operation-preview` 严格解析 `{operationId, afterSequence}`；活动 exact-ID calculation 有新帧时 200，未知/无更新/非 calculation/终态时 204。
- [x] 服务端仅保存活动任务最新帧；取消、成功、失败和 Drop 清理；status/terminal/ack 持续不含 PNG 或 Data URL。
- [x] validation 前端每轮 status→preview 串行且不重叠，初始 `afterSequence=0`，旧 ID/generation/handle/sequence 和迟到响应不能污染新任务。
- [x] Tauri JS 使用真实 `Channel` 对象的 mockIPC 契约测试；Rust 命令参数、schema 过滤和 invoke settle 后迟到消息抑制通过。
- [x] React 分离 `preview` 与 `result`；预览不可导出，成功最终结果替换，取消/失败/新点/参数/清空均清除或抑制预览。
- [x] MapLibre Blob URL 在相同帧复用，并在替换、清空和卸载时释放。

### 23.3 全量门禁与受管运行

- [x] Rust workspace offline：`100 passed / 3 ignored`；真实 GLO-90 HTTPS 另行 `3/3`。
- [x] coverage `20`、app-service `17`、validation-server `19` 项专项测试通过；这些已包含在 workspace 总数中。
- [x] 前端 `7 files / 51 tests`，TypeScript 与 Vite validation build 通过。
- [x] rustfmt、Clippy workspace `--all-targets -D warnings`、Windows x64 full xwin 构建、`bash -n`、管理 self-test 与 diff check 通过。
- [x] 成都真实缓存两次烟雾均得到 2 张 sequence/完成数/PNG 内容不同的预览；首帧分别 5,610/5,660 ms，总耗时 7,246/7,301 ms。
- [x] 每帧 schema 1、`0 < completed < total`、EPSG:3857、`401×401`、PNG/IHDR 有效；最终 schema 3 双 PNG 有效，terminal preview 为 204 且 status 无 PNG。
- [x] 缓存前后均 `133,071,416 bytes`、`partial=0`、两个区域各 `50/50 ready`；recovery smoke 连续两次通过。
- [ ] 经 SSH 隧道在浏览器观察完整渐进过程、取消/重试和控制台。
- [ ] Windows 10/11 实机验证 WebView2↔Rust Channel、连续 MapLibre 更新、取消迟到消息、安装/卸载和内存。
- [ ] 中国大陆合规底图、审图号、离线与导出授权。

完整运行指标、构建产物哈希、进程高水位与烟雾脚本竞态说明见 `18-progressive-coverage-preview-validation.md`。Windows 交叉构建成功只证明可编译和打包，不能勾选 Windows 实机或地图合规门槛。

## 24. partial 写失败与零米 DEM 加固（2026-07-27）

本检查点关闭两项静态/容错残余，不改变产品输入或自动重试策略。

### 24.1 已完成行为

- [x] DEM 未返回时继续由 fieldset disabled 与 handler guard 双重阻止切换手动覆盖；删除不可达的 `null → 0 m` 后备表达式。
- [x] 新增真实 DEM `0 m` 用例，确认海平面零值可精确进入手动覆盖，且不与缺失 `null` 混淆。
- [x] 从非零续传偏移故障注入：当前块部分写入后返回原始 I/O 错误，失败块不增加 total、不发布进度。
- [x] 二次检查点成功时，partial 实际长度与 SQLite 同步，同进程强 ETag/Range probe 可从该长度续传。
- [x] 二次 `sync_all`、文件游标读取失败或游标越出当前块范围时均不掩盖原始写错误；不可信 partial 被删除并重置 missing，重启后仍从 0 下载。
- [x] 任何失败路径都不进入 finalize/ready，也不增加隐式自动重试。
- [x] 即使 corrupt 标记与即时删除都失败并留下 `Downloading(DB=4, file=6)` 等价状态，重启也截回 DB checkpoint；文件短于 DB 或缺失则废弃，未知尾部不能被复活。

### 24.2 当前门禁

- [x] Rust workspace offline：`102 passed / 3 ignored`。
- [x] cache crate：`28 passed / 3 ignored`；真实 GLO-90 HTTPS：`3/3`。
- [x] 前端：`7 files / 52 tests`；TypeScript 与 Vite validation build 通过。
- [x] rustfmt、Clippy workspace `--all-targets -D warnings` 与 Windows x64 cargo-xwin workspace/all-targets check 通过。
- [x] `git diff --check` 与文档结构检查通过。
- [ ] 在真实磁盘不足/配额边缘环境触发部分写入失败，确认不同 Windows/Linux 文件系统错误与用户提示。
- [ ] Windows 10/11 实机验证 Range 恢复、休眠/断网、杀进程和原子 ready。

本切片的故障 writer 包装真实临时文件，能确定性地产生“先写 2 bytes、随后失败”、检查点同步失败、游标读取失败和游标越界；store 层另构造清理失败可能遗留的 DB/file 长度差异，并验证只保留已检查点前缀。它证明错误优先级和恢复不变量，不等同于真实磁盘耗尽压力测试。

### 24.3 受管验证平台回归

- [x] 提交 `4042d0c0bd808b898de1556b9b047c9831922c0c` 完成 stop/build/start，构建时间 `2026-07-27T07:02:51Z`，server SHA-256 为 `647547e576308d81e807e7b1b72aedb2e8d8778f235c1dbd3f521a77d8295ea5`。
- [x] 新进程 PID `1457203` 经进程身份与 `ss` 确认只监听 `127.0.0.1:1421`；health schema 1、bootstrap schema 2 和管理 self-test 通过。
- [x] recovery smoke 通过：被取消任务保持 cancelled，后继同票 calculation HTTP 200，两个任务各有 2 次进度。
- [x] 成都渐进预览 smoke 通过：2 帧不同 PNG，最后一帧完成 `123260 / 125628`，首帧 `5707 ms`，总耗时 `7176 ms`，终态仍由完整 schema 3 结果决定。
- [x] 运行前后缓存保持 `133,071,416 bytes`、partial `0`，两个登记区域均为 `50/50 ready`。

本节仍是 Linux 回环内部平台证据，不替代 Windows 实机、浏览器人工视觉、真实 ENOSPC/EIO 或中国大陆地图合规验收。

重启可信检查点栅栏提交 `93b96abd3a0c1c099870509bbe3711ef4bb6db95` 随后再次完成受管 stop/build/start；这是当前运行构建，上一组 `4042d0c` 数字保留为中间历史证据。最终构建时间 `2026-07-27T07:15:45Z`，server SHA-256 `32bb5b05ddc18ca49d34f7b5d04fd48fe6f0f04099d7444e4b0ff7f8649efbbe`，PID `1468926` 仅监听 `127.0.0.1:1421`。

最终构建的 health schema 1、bootstrap schema 2、管理 self-test 与 recovery smoke 均通过；recovery 仍为 `ticket_a_cancelled=true ticket_b_http=200 progress_a=2 progress_b=2`。成都 progressive smoke 得到 2 张不同预览，最后完成 `120400 / 125628`，首帧 `5452 ms`、总耗时 `7041 ms`。缓存保持 `133,071,416 bytes`、partial `0`，两个区域各 `50/50 ready`。

纯文档证据提交不重建已验证的 `93b96ab` 二进制；运行 metadata 继续精确指向该代码 revision。

## 25. 生产缓存硬上限与崩溃恢复压力门禁（2026-07-28）

本门禁只验证缓存存储，不启动、停止或替换受管验证平台。压力目录固定在项目的 `.runtime/cache-stress/`，与 `.runtime/validation-platform/data/` 隔离。

### 25.1 生产不变量

- [x] partial 文件每次完成 `sync_all` 后，SQLite checkpoint 之前重新扫描整个缓存目录；只要实体数据超过十进制 `2,500,000,000` 字节，就在任何 SQLite 写入前返回 `QuotaExceeded`。
- [x] 超限一字节时 `connection.total_changes()`、asset 行和 region→asset 关系均不变；重开只截掉未被索引信任的尾字节，缓存恢复到精确上限并保持 `Downloading`。
- [x] 磁盘空间查询可在私有预检辅助函数中注入；`Ok(0)` 稳定得到 `DiskSpaceInsufficient { available_bytes: 0, requested_additional_bytes: 1 }`，且索引和用量不变。
- [x] 普通 cache 单元测试现为 `30 passed / 1 ignored`；真实压力测试保持显式 opt-in，不进入日常测试。

### 25.2 实体压力与强制退出证据

- [x] `scripts/cache-durability-stress.sh` 先要求至少 4 GB 可用空间，并拒绝非空压力目录。
- [x] 测试顺序写入非零 8 MiB 块，不使用 `set_len`；Linux `st_blocks × 512` 必须不少于文件长度，缓存目录总字节精确等于 `2,500,000,000`。
- [x] 在精确上限追加并同步一字节后，checkpoint 立即被拒绝，SQLite 不变；重开截回可信长度并重新达到精确上限。
- [x] 三个独立子进程以退出码 97 模拟：partial 已同步但索引未更新、partial 已同步且索引已更新、原子 rename 已完成但 ready 索引未更新。重开分别截回旧前缀、保留新前缀、恢复 `Ready`。
- [x] 最终加固版服务器实跑 `1 passed`，父测试耗时 `5.264 s`、脚本总耗时 `6 s`；结束后 `.runtime/cache-stress/` 不存在。
- [x] 真实验证缓存前后均为 `133,079,734` 文件系统字节，内容清单哈希前后同为 `ac69d2ecf509bf6faf80d08570a480bbee34950c26f1f702d79eb325c0dd76f8`。
- [x] 全工作区 `104 passed / 4 ignored`；rustfmt、Clippy `--all-targets -D warnings`、Windows x64 cargo-xwin workspace/all-targets、`bash -n` 与 `git diff --check` 均通过。

### 25.3 仍未外推

- [ ] 注入的零可用空间证明错误映射与无索引副作用，不等同于真实文件系统 ENOSPC、EIO、断电或存储设备掉线。
- [ ] Linux 子进程强制退出不替代 Windows 10/11 上 NTFS、休眠、杀进程、安装/卸载与用户提示实机验收。
- [ ] 本门禁不改变或证明中国大陆合规在线底图、审图号、热力图叠加授权和导出授权。

## 26. 真实传播参数敏感性门禁（2026-07-28）

本门禁复用桌面请求的 Rust 归一化路径和真实成都 DEM/WBM，但在受锁保护的一致缓存快照上运行，不调用或替换受管 validation 二进制。完整方法和数值见 `19-parameter-sensitivity-validation.md`。

### 26.1 逐像素与渲染结果

- [x] 相同输入的 125,628 个有效 dBm 像素逐 bit 一致；原始栅格 SHA-256 为 `3415f575c8f7c8a51f3ca8fa4f4387be3055382aea527ec064206ca8ffb0429a`。
- [x] 相同输入的报告 PNG 与 EPSG:3857 地图覆盖 PNG 分别字节一致；哈希为 `78e5adb8debe7aa213c55d38765a608d1bf5ac9fb1890d9e57c582939cf7b125` 与 `58e8dc314c607a38270f3d6dd923574ced2e2587e128c10b12d3bd7cea39786b`。
- [x] 25→250 W 的所有有效像素为 `+9.999992..+10.000008 dB`；平均 `+10.000000 dB`。
- [x] TX 6→12 dBi 与 RX -3→3 dBi 各自在所有有效像素产生 `+5.999992..+6.000008 dB`，证明两端增益独立进入链路预算。
- [x] 145→435 MHz 的全部有效像素变化，平均 `-18.068198 dB`，差值范围 `-45.764847..-0.557541 dB`。
- [x] TX 20→80 m AGL 有 124,615 个像素显著变化、38,769 个改善；RX 1.5→10 m AGL 有 125,074 个显著变化、53,808 个改善。
- [x] 水平−垂直极化有 102,984 个像素显著变化，范围 `-5.568375..+1.492321 dB`；没有把极化简化为表单状态。
- [x] 上述七个参数场景的报告 PNG 与地图覆盖 PNG 都不同于基线，避免“数值变了但用户看到的热力图未变”的误通过。

### 26.2 缓存与运行边界

- [x] 脚本逐路径分量拒绝符号链接，安全清理只允许项目内 `.runtime/parameter-sensitivity-smoke/run.*`。
- [x] 真实 cache lock 从复制一致快照保持到矩阵结束；计算只打开快照，真实 SQLite 的 reconcile/last-used 不被触发。
- [x] 真实缓存前后均为 `133,079,734 bytes`，包含 SQLite/lock/DEM/WBM 的完整清单 SHA-256 均为 `a850ee81b363e91c88a638f836f7199882b27c97f358da8be975fa6cc1b919bf`，无 partial 或运行目录残留。
- [x] 最终精确测试选择器运行 `1 passed`，耗时 `25.92 s`；快照、矩阵和清理端到端总计 `28.7 s`。
- [ ] 这证明参数接线、确定性和模型内部响应，不替代 Windows WebView2、外场校准、中国大陆地图合规或应急可靠性验证。

## 27. 在线底图、动态比例尺与地图状态重放（2026-07-31）

### 27.1 已完成代码级检查

- [x] 前端信任检查只接受固定 `tianditu + same-origin-proxy + vec/cva` 元数据和同源路径模板；不可信模板 fail closed。
- [x] 前端专项 `src/lib/basemap.test.ts` 与 `src/components/MapView.test.tsx` 共 4 项通过，覆盖底图添加/移除、右下 metric 比例尺和延迟清空重放。
- [x] 清空时序验证已有覆盖层在 `isStyleLoaded=false` 时不误操作，并在后续 `idle` 恢复后删除 layer/source、只撤销一次 Blob URL。
- [x] Rust `basemap::tests` 4 项通过，覆盖严格瓦片路径/矩阵边界、固定上游 URL、token 文件 fail closed 和 MIME/图片签名一致性。
- [x] bootstrap 元数据与路由测试代码断言不返回 token、上游主机或 token 文件路径；缺 token 时底图保持 disabled。
- [x] 前端全量 9 个文件/56 项、Rust workspace all-targets、Clippy `-D warnings`、TypeScript、validation Vite build、Windows x64 xwin、`bash -n`、管理 self-test 与 diff check 通过。
- [x] revision `6e9714c6cdcdeb54ff47e229d8d43b18bf32b3c6` 已完成受管 `stop → build → start`；PID `2306446` 健康且只监听 `127.0.0.1:1421`，server SHA-256 为 `d5f57bd71de4f64c62359591edbbee9b23461461d63265b68dd2a5f9dac640f9`。
- [x] 无 token 的实跑 bootstrap 返回固定天地图元数据和 `enabled=false`；合法瓦片路径返回 503 与 `no-store`，证明禁用态 fail closed。

### 27.2 仍未关闭

- [ ] 当前未配置天地图 token；尚未取得真实 `vec/cva` HTTP 200、瓦片解码、浏览器视觉、控制台、缩放和清空烟雾证据。
- [ ] 尚未验证 token 服务条款、调用额度、必要署名和弱网/上游错误用户体验。
- [ ] 尚未取得有效审图号及 Windows 桌面离线缓存、再分发、应用分发和 PNG/PDF 导出授权。
- [ ] 在线天地图内部验证不能替代正式 `CompliantBasemapProvider`、中国大陆有效区、签名瓦片清单或 Windows 10/11 实机。

完整边界见 `20-tianditu-basemap-proxy.md`。

## 28. 历史：四省 Protomaps PMTiles 内部底图（2026-07-31，部分已验证；已退出当前目标）

本检查点固定的资产事实为 source build 20260731、bbox 107.5,18,125.5,33.5、z0-9、33,044,072 bytes、SHA-256 5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0、gzip MVT、939 个 region tiles 与 837 个 archive entries；该归档占 2.5 GB 的 1.32%。已执行项按真实日志回填，浏览器视觉没有证据的项目继续保持未勾选。

### 28.1 资产、Range 与安全

- [x] HEAD 与 bootstrap 报告固定文件大小 33,044,072 bytes。
- [ ] 本轮日志未记录 SHA-256 重新计算，不能用已知基线值替代执行证据。
- [x] live header 为 PMTiles v3；bootstrap 的 bbox、maxZoom 与 archiveBytes 匹配固定值。
- [ ] archive directory 尚未独立检查。
- [x] live GET 单段 Range 返回 206、精确 Content-Range 与长度；HEAD 另行确认 Accept-Ranges: bytes。
- [x] Rust 自动化覆盖缺 Range、越界、多段、开放/后缀区间和超过 8 MiB 的 Range，并按契约拒绝。
- [x] bootstrap 不暴露服务器文件路径，Range 端点只由回环 validation server 提供。
- [ ] gzip MVT 可由 PMTiles/MapLibre 解码，首屏、缩放和平移不会触发整包下载。

### 28.2 图层、署名与交互

- [x] 历史提交 db052e6 的前端自动化只构建 earth、landcover、landuse、water、roads 五个 style layer，并拒绝 boundaries、places、pois；这是 2026-07-31 基线，不代表 2026-08-01 的地名新需求已实现。
- [ ] 新目标必须把 places 加为可信第六 source layer，同时继续拒绝 boundaries 与 pois；实现证据见第 29 节，完成前不得改为通过。
- [ ] 道路、水体与土地覆盖在热力图下方正确对齐；发射点、比例尺和必要控件层级正常。
- [x] 前端测试确认 © OpenStreetMap contributors 文本，第三方清单记录 PMTiles/fflate/ODbL 与 landcover caveat；真实浏览器可读性仍待验。
- [x] PMTiles 主路径与天地图 fallback 的固定契约均通过前端信任检查。
- [x] 地图 desired-state、区域 fit 与延迟清空重放测试通过。

### 28.3 已完成自动化与受管运行

- [x] frontend check PASS，9 files / 59 tests PASS。
- [x] Rust workspace all-targets locked offline 为 112 passed / 5 ignored；validation server 定向测试 27 / 27 PASS。
- [x] workspace Clippy -D warnings、validation-platform bash -n 与 self-test PASS。
- [x] dirty workspace 受管 build/start 成功；PID 2342699 只监听 127.0.0.1:1421，health 正常。
- [x] bootstrap 返回 enabled=true、providerId=protomaps、固定 resourcePath、bounds、maxZoom=9 与 archiveBytes=33044072；cache total=293,517,252 bytes。
- [x] live GET bytes=0-7 返回 206，Content-Range 为 bytes 0-7/33044072，body hex 为 50 4d 54 69 6c 65 73 03。
- [x] live HEAD 返回 200，Content-Length 为 33044072，Accept-Ranges 为 bytes。
- [x] 运行数据根约 293.5 MB；约 108 MB 的试验目录已删除，本轮底图试验资产只保留约 33 MB 归档。

构建 metadata 的 revision 仍为 130043e，只表示 dirty build 时检出的旧 HEAD；不能把它写成包含本轮未提交源码的提交号。本轮没有 commit 或 push。

### 28.4 仍未关闭

- [ ] 自动浏览器视觉因 Codex 桌面 node_repl 的 Windows sandbox ACL 故障未完成；没有截图、控制台、WebGL、图层对齐或真实 MVT 解码证据。
- [ ] 本轮日志未记录 SHA-256 重新计算、archive directory 独立检查或重启后哈希对比。

本机 127.0.0.1:1421 已连通，用户可直接刷新页面人工验证；可访问性本身不算浏览器视觉通过。

原始归档仍含 boundaries 与 Natural Earth/OSM 内容；当前只作私有验证、不纳入正式 EXE，且本检查点不作公开发行结论。

完整资产与逐项证据见 docs/21-protomaps-four-province-basemap.md。

## 29. 历史切片：PMTiles 中文地名与 EOx 地图/卫星切换（2026-08-01，自动化与 live HTTP 已通过）

本节区分已经执行的自动化/live HTTP 与仍需用户完成的真实浏览器视觉。不得把单元测试外推为字体、碰撞、WebGL 或实际瓦片视觉通过。

### 29.1 离线中文地名契约

- [x] bootstrap 与前端严格信任契约把 `places` 列为 earth、landcover、landuse、water、roads 之后的第六 source layer；缺失、多余、重复或错误顺序均 fail closed。
- [x] 可见样式只从 `places` 渲染省级、主要城市、县区和乡镇，继续禁止 boundaries 与 pois；不得把 z0-9 资产描述成村级或街道级完整数据。
- [x] 名称表达式严格按 `name:zh-Hans`、本地 `name`、`name:en` 回退；三者都缺失时不产生空注记。
- [x] 省/主要城市/县区/乡镇分别有确定的 zoom、filter、文字大小、碰撞优先级与浅色/深色 paint；重复同步、主题切换和 style replay 不产生重复 layer/source。
- [x] MapLibre style 没有 glyph URL；地图显式使用 `Microsoft YaHei, Noto Sans CJK SC, PingFang SC, sans-serif`，构建产物不新增 WOFF/TTF/PBF 或第二份地名数据；真实 Network 仍在 29.2 验证。
- [x] 前端自动化覆盖 PMTiles label layer 的添加、移除、中文回退、主题、首个 label anchor 和不可信 metadata 拒绝。

### 29.2 图层顺序与真实浏览器

- [x] 自动化确认图层顺序为基础地表/道路/水体 < 经纬网 < 传播热力图 < 200 km 范围与地名 < 发射点，且延迟清空/状态重放不残留热力图。
- [ ] 福州、杭州、南昌、广州在代表性 z4/z8/z10 视图中可读；省、主要城市、县区、乡镇按缩放渐进出现，碰撞不会形成不可用的文字墙。
- [ ] 浅色、深色主题与卫星背景下文字/halo 可读，热力图不遮住地名，地名不遮住发射点，比例尺与署名不被控件遮挡。
- [ ] 完全断网时 PMTiles 地名仍可显示；控制台无 glyph、字体、MVT、WebGL 或缺 source-layer 错误。
- [x] 自动化确认清空热力图与地图/卫星切换保留正确 desired state；缩放、平移和真实 WebGL 视觉仍需人工确认。

### 29.3 EOxCloudless 同源代理

- [x] 前端只接受固定 `EOxCloudless + same-origin-proxy + z0-14` 能力信息和相对模板 `/api/basemap/satellite/{z}/{x}/{y}`；bootstrap 不暴露上游 URL、凭据或可变主机。
- [x] 路由只接受规范十进制 z/x/y、z0-14 和合法矩阵边界，拒绝查询字符串、前导零、负数、溢出、额外路径段、任意 URL 和非 GET 方法。
- [x] 上游固定为 HTTPS EOX host 与 Sentinel-2 2025 EPSG:3857 WMTS path，z/y/x 映射正确；重定向、错误 MIME/签名、超限响应、超时和非 200 均 fail closed。
- [x] bootstrap 不泄露上游 URL；live 成功瓦片为 JPEG 并带 `Cache-Control: no-store`，代理代码不写入 Rust 数据根、SQLite 或缓存管理。
- [x] 浏览器断网或卫星 source error 会自动回退 PMTiles 地图并显示非阻塞提示；camera、发射点、200 km 圆和已有热力图组件不重建。
- [ ] 地图/卫星切换前后，相同输入的 DEM/WBM 资产、ITM 请求、计算统计和热力图字节完全一致；卫星像素不进入任何传播分析。
- [x] 前端自动化确认卫星模式持续显示 `EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)`，地图模式继续显示 © OpenStreetMap contributors。
- [ ] 全量前端、Rust、Clippy、构建、管理脚本 self-test、受管 stop/build/start、live HTTP、浏览器视觉与控制台门禁通过。

### 29.4 尚未关闭

- [x] 2026-08-01 已完成 9 files/65 frontend tests、Rust workspace 113 passed/5 ignored、validation server 28/28、Clippy、构建、管理 self-test、受管 stop/build/start 和 live EOX JPEG/no-store。
- [ ] 必须复核 EOX 官方 capabilities、公共 endpoint 的长期可用性、速率/服务条件，以及 2025 非商业 CC BY-NC-SA 4.0 与项目实际发行方式的兼容性；商业用途需另行授权。
- [x] EOxCloudless 在线层不是离线卫星方案；断网只保证回退 PMTiles，不保证卫星影像离线可见。
- [x] Google Maps / Google Satellite 因 key、计费和缓存/再分发限制未进入本实现，不为其建立兼容或回退测试。

## 30. 会话覆盖层与 validation 浏览器导出（2026-08-01，自动化已通过）

本节的规则取代第 16、20、21 节中“validation 导出禁用”“新点/取消重算清除唯一旧结果”的历史切片行为；旧章节保留当时证据，不代表当前产品状态。

### 30.1 会话覆盖层

- [x] 纯状态测试覆盖不同坐标累积、完全同点替换并置顶、最多 8 项及第 9 项淘汰最早项。
- [x] App 测试覆盖第一点计算完成、选择第二点时第一层继续存在、第二点完成后共两层，以及“清空”后归零但保留当前点。
- [x] 取消重算只撤销当前导出身份，之前完成的会话覆盖层继续显示；被取消半成品不进入会话结果。
- [x] MapView 测试确认两个结果拥有独立 source/layer，新增第二层不删除第一层，组件卸载逐项释放两个 Blob URL。
- [x] desired-state 延迟清空继续覆盖 style 未就绪后在 idle 重放删除，不泄漏 layer/source/Blob。
- [ ] 受管浏览器连续计算两个真实不同站点，视觉确认旧层、历史站点标记、最新层层级、地图/卫星切换和清空全部。

### 30.2 浏览器诊断导出

- [x] validation 能力矩阵改为 `canExport=true`；普通 preview 仍为 false，Tauri 仍有优先级。
- [x] PNG 校验签名后使用浏览器 Blob 下载，返回字节数且不调用服务器导出路由。
- [x] PDF builder 生成单页 A4 横向 PDF，测试覆盖 PDF 头、Pages tree、DCTDecode image、xref、startxref/EOF，以及非法 JPEG/尺寸拒绝。
- [x] App 只从当前最新结果及其冻结参数创建报告；选择新点或参数过期时禁用导出，多层视觉叠放不进入报告。
- [x] validation server 路由集合保持不变，未知 `/api/export-result` 继续 fail closed；没有服务器文件写入或任意目标路径。
- [ ] 受管浏览器分别触发真实 PNG 与 PDF 下载并在另一 PDF 阅读器中确认一页、可打开、内容完整。

### 30.3 当前执行证据

- [x] TypeScript project check 通过。
- [x] 前端全量 11 文件/73 项通过，其中 `browserExport`、`sessionCoverages`、`backend`、`MapView`、`App` 五个专项文件共 46 项。
- [x] Rust workspace `113 passed / 5 ignored`、rustfmt、Clippy `--all-targets -D warnings`、validation 管理脚本 `bash -n` 与 self-test 通过；功能提交 `6261f8dc22bdeeefcdd19e923582d72f5918fbb0` 已推送并完成干净的 validation stop/build/start，PID 2496862 只监听 `127.0.0.1:1421`，服务器与 Windows SSH 隧道 health 均为 HTTP 200。

详细边界与理由见 ADR 0019。

## 31. Windows 在线天地图与安装包（2026-08-01，自动化与交叉构建已通过）

本节严格区分服务器自动化/交叉构建与 Windows 实机/真实网络证据。以下勾选项只证明代码、测试和产物构建已经通过；没有有效个人 `tk`、Windows 10/11 运行记录和中国大陆真实 ISP 证据前，不得宣称桌面视觉或中国大陆网络可达性已经验收。

### 31.1 已通过的安全与契约门禁

- [x] Rust 单元测试覆盖 `vec/cva/img/cia`、规范瓦片坐标与矩阵边界、固定 Web Mercator WMTS URL、无 `tk` 禁用、输入校验、PNG/JPEG MIME 与签名、配额边界和错误响应不泄漏凭据。
- [x] Windows xwin all-target check、严格 Clippy 和 `test --no-run` 均通过，覆盖 Windows DPAPI FFI 与测试程序编译；非 Windows 回退测试确认不创建明文凭据文件。
- [x] 前端只信任固定 `Tianditu + tianditu:` 元数据和四个完整模板；设置入口只在 Tauri 模式出现，临时 `tk` 不进入 Web Storage、WebView URL、bootstrap 或渲染文本；原生代理仅按供应方协议把它加入固定 HTTPS 上游请求。
- [x] MapView 自动化覆盖普通地图 `vec+cva`、卫星图 `img+cia`、切换保持相机/覆盖层/发射点，以及未配置时 fail closed。
- [x] production/dev CSP 只放行 `tianditu:` 及其固定 Tauri localhost 映射，不增加任意远程 HTTPS 图片源或脚本源。
- [x] 在线瓦片强制 `no-store`，不写入持久缓存、不计入 2.5 GB DEM/WBM 配额，也不进入诊断 PNG/PDF。

### 31.2 已通过的自动化与打包门禁

- [x] 前端 TypeScript 与 production build 通过，前端全量为 `11 files / 79 tests`。
- [x] Rust workspace 为 `113 passed / 5 ignored`；rustfmt、workspace Clippy、validation 管理 self-test 均通过。
- [x] Windows xwin all-target check、严格 Clippy 与测试程序 `--no-run` 通过。
- [x] 基于源提交 `59ae5b188f48db52618846246de27eb0cfe6bbba` 的 Tauri Windows 交叉构建第二次执行退出 0。
- [x] standalone `HamHeatmap.exe` 为 `16,104,960 bytes`，SHA-256 `1146de0f7bbd0e409c676c3f75d5c7f6741700252418ebfcf15212c343bda7ed`。
- [x] NSIS `HamHeatmap_0.1.0_x64-setup.exe` 为 `217,258,090 bytes`，SHA-256 `46434fc5179ae8d5dd65acdb1c251907292aa689a10755aa6ac08a932d2c2000`；正式包内嵌离线 WebView2、采用当前用户安装且未签名。

### 31.3 仍需 Windows/真实网络验收

- [ ] 在 Windows 10 和 Windows 11 分别安装、启动、第二实例聚焦、卸载，并确认 SmartScreen/未签名提示和离线 WebView2 安装行为。
- [ ] 使用用户自己的有效天地图 `tk` 实测普通地图、卫星图、中文注记、动态比例尺、`tk` 替换/清除、断网、配额错误和重启后 DPAPI 恢复。
- [ ] 从至少一个中国大陆家庭或移动网络验证瓦片可达；该结果只说明测试时点和网络，不保证供应方长期可用。
- [ ] 在 Windows 实机检查 DevTools、日志、崩溃信息、bootstrap 和导出文件均不含 `tk`，并确认诊断 PNG/PDF 不包含在线底图。

详细架构和发行边界见 ADR 0020。

## 32. 全局 dBm 显示阈值与在线底图发行边界（2026-08-01，服务器证据已回填；浏览器与 Windows 待验）

本节定义本轮验收口径，不以设计或编译成功冒充浏览器性能、Windows 实机或 Release 完成。结果契约见 ADR 0021。

### 32.1 结果契约与显示语义

- [x] CalculationResult schema 4 同时携带 `mapOverlayPngDataUrl`、固定 `mapOverlayFilterEncoding="u8-dbm-floor-v1"` 和 Base64 bins；bootstrap schema 2、preview schema 1 保持不变。
- [x] bins 解码后长度严格等于 `mapOverlayWidth × mapOverlayHeight`；0 表示原本透明，1..81 与整数 `-140..-60 dBm` cutoff 在边界上下均满足 `value >= threshold`。
- [x] 缺字段、未知 encoding、非法 Base64、错误长度或尺寸不匹配时 fail closed，不显示可被误筛选的最终层，也不退化为像素查询接口。
- [x] 色标游标默认 `-140 dBm`，范围和键盘步长正确，显示负号与 dBm；无已完成结果时禁用或不执行渲染。
- [x] `-140` 保留原 PNG 可见 alpha；拖到 `-120` 时只保留 `>= -120 dBm`；`-60` 只保留最强区。透明像素始终透明，可见像素 RGB 不改变。
- [x] 同一阈值同步作用于最多 8 个已完成层；选择新点、参数变化、新计算和地图/卫星切换保留阈值，清空删除层但保留阈值。
- [ ] 阈值不作用于渐进 preview，不改变计算请求/次数、统计、缓存键、结果 PNG、当前导出身份或 PNG/PDF 字节。

### 32.2 MapLibre 生命周期与性能

- [x] 最终层以 `animate:false` CanvasSource 复用；首次 PNG 解码后拖动只更新 alpha 和一次纹理上传，不重新编码 PNG、不删除/增加 source/layer、不移动相机。
- [x] `requestAnimationFrame` 与至少 33 ms 间隔把连续 input 合并到最多 30 fps；相同整数值不重绘，迟到帧不能覆盖最新阈值。
- [x] 纯函数/DOM 测试覆盖 1 层和 8 层、clear/unmount 释放、style 未就绪后 desired-state 重放，以及 preview image source 与 final canvas source 不混用。
- [x] 服务器上的自动化或微基准记录 8×401×401 最坏 alpha 扫描耗时；只有实际浏览器拖动、控制台、WebGL 和长任务证据完成后才能说明“无明显卡顿”。
- [ ] 经 SSH 隧道在受管 validation 浏览器中连续拖动 `-140 → -60 → -120`，视觉确认弱像素动态剔除、地名/发射点层级不变、无控制台错误；该证据不外推为 Windows WebView2。
- [ ] Windows 10/11 实机对独立 EXE/NSIS 分别复测 8 层拖动、DPI/缩放、浅/深主题、GPU/软件渲染和内存。

### 32.3 Windows Release 与在线底图边界

- [x] GitHub Alpha Release 实际创建后上传独立 EXE、NSIS 安装包与 `SHA256SUMS.txt`；README 只链接 Releases 页面，不提前写不存在的 tag/资产 URL。
- [ ] 下载后的 SHA-256 与服务器受验产物一致；发布说明明确未签名、SmartScreen、Windows 实机和真实网络待验边界。
- [x] 本轮 Release 内容审计确认不含四省内部 PMTiles、EOxCloudless 离线副本、DEM/WBM、密钥、源码依赖或构建缓存。
- [ ] 当前及后续发行物只使用在线视觉底图，不包含离线地图包、四省 PMTiles 或在线瓦片副本；DEM/WBM 与计算缓存继续受十进制 2.5 GB 配额约束。

## 33. Windows 天地图显式连接自检（2026-08-01，自动化与交叉编译已通过）

### 33.1 已完成行为

- [x] 保存配置与连接自检分离；保存成功后才探测，探测失败不会删除已经保存的 DPAPI 配置。
- [x] 已保存配置可单独测试和重新测试；清除配置重置结果，busy 期间输入、关闭、保存、测试和清除均被阻断。
- [x] 探测固定 `vec/8/215/106`，复用正式代理的 HTTPS-only、零重定向、有限超时、2 MiB、MIME 与 PNG/JPEG 签名校验。
- [x] schema 1 只序列化 `schemaVersion/status`；六种状态固定，前端未知 schema/状态 fail closed，后端正文不回显。
- [x] 临时 `tk` 不进入 Web Storage、WebView URL、bootstrap、DOM 文本或测试结果；原生代理仅按供应方协议把它加入固定 HTTPS 上游请求，探测不写瓦片缓存、SQLite 或诊断导出。
- [x] 普通 preview 与 validation-server 不显示入口，也不会调用桌面探测命令。

### 33.2 执行证据与边界

- [x] TypeScript check、13 个前端测试文件/111 项测试和 production build 通过。
- [x] Rust workspace `114 passed / 5 ignored`、rustfmt 与 workspace Clippy `-D warnings` 通过。
- [x] 基于提交 `9b0fb79` 完成全新 Windows 交叉构建，并通过 `verify-windows-artifacts.sh`；独立 EXE 为 16,174,080 bytes / `a1968a48...8247`，NSIS 为 217,265,419 bytes / `4df826b0...bc0e`。
- [x] `v0.1.0-alpha.2` GitHub 预发行版已公开上传两个 EXE 与 `SHA256SUMS.txt`；包内审计确认不含离线地图、DEM/WBM、个人 `tk`、源码或构建缓存。
- [x] Windows xwin all-target check、测试程序 `--no-run` 与严格 Clippy 通过；只有既有缺失 MSVC PDB 的 LNK4099 非阻断警告。
- [ ] 服务器缺少 Tauri Linux 主机测试所需的 pkg-config/libdbus，因此本轮只执行 Windows 测试程序交叉编译，没有把该限制写成运行通过。
- [ ] 有效个人 `tk`、中国大陆真实网络、Windows 10/11 DPAPI/WebView2、弱网、额度与上游故障仍需实机验收。
## 34. 纯在线视觉底图迁移（2026-08-02，代码完成；受管运行待切换）

本节执行 ADR-0022。第 28、29 节中的 PMTiles、离线 places 和 PMTiles 故障回退只保留历史证据，不再是当前验收目标。

- [x] Windows/Tauri 保持天地图 `vec+cva` 普通地图、`img+cia` 卫星图、用户个人 `tk`、DPAPI、严格原生代理与 `no-store`。
- [x] validation 普通地图切换为同源天地图 `vec/cva` 主路径，卫星图保持同源 EOxCloudless；前端不再注册或请求 PMTiles。
- [x] 普通/卫星图使用在线中文注记，不再依赖 PMTiles `places`、本地地名文件或离线字体包。
- [x] 缺少 token/tk、断网、额度或上游失败时回退 WGS84 网格，并保留 camera、发射点、历史站点、200 km 圆、比例尺与热力图。
- [x] 已缓存完整 DEM/WBM 的区域在无网络、仅 WGS84 网格时仍能计算和导出无底图诊断报告；缺失或损坏计算资产继续阻断。
- [x] 天地图与 EOxCloudless 瓦片不进入 Rust/浏览器持久缓存、2.5 GB 配额、EXE、安装包、Release 或诊断导出。
- [x] 移除当前源码、依赖锁、新前端构建和测试构建中的 PMTiles/fflate/Range 路由；当前受管 release 二进制仍待重建，历史许可证据继续可查。
- [x] TypeScript、14 个前端测试文件/133 项测试、production build、Rust workspace `110 passed / 5 ignored`、rustfmt、严格 Clippy 及 Windows xwin all-target 检查通过。
- [ ] 通过受管流程删除服务器约 33 MB PMTiles runtime 资产，记录删除前后路径、字节数和缓存不变性。
- [ ] 完成受管 stop/build/start、bootstrap/HTTP、真实在线瓦片、浏览器和 Windows 10/11 实机验证。

当前事实：validation 天地图 token 尚未配置，受管运行服务尚未按本节目标重启，约 33 MB PMTiles runtime 资产尚未删除。当前运行中的旧进程因此不能作为纯在线实现的受管验收证据。

## 35. 四语言国际化与 GitHub 介绍页（2026-08-02）

- [x] 支持 `en`、`zh-CN`、`zh-TW`、`ja-JP`，英文为 fallback；无效存储值安全回退，系统语言映射与 `<html lang>` 更新有自动化覆盖。
- [x] 语言选择写入 `hamheatmap.locale.v1`，切换后无需重启；发射点、参数、最多 8 个热力图、阈值、MapLibre camera 和当前工作流状态不丢失。
- [x] App、ParameterPanel、MapView、参数校验、前端错误、缓存/下载/导出对话框和诊断 PNG/PDF 的用户可见文本均从四语言资源读取。
- [x] 四个资源的 key 集完全一致；测试中不存在缺 key、直接显示翻译 key 或依赖测试主机 locale 的结果。
- [x] 取消识别集中到稳定 helper，优先识别结构化 code，并只为现有 Rust/HTTP 协议保留兼容消息，不在业务组件中匹配界面语言。
- [x] 地图实例不因语言切换重建；在线瓦片、中文供应方注记、attribution、发射点、范围、比例尺与热力图层级不改变。
- [x] NSIS 配置包含 English、SimpChinese、TradChinese、Japanese；Windows 交叉配置检查通过，真实安装器四语言仍待 Windows 实机验收。
- [x] `README.md` 为英文 canonical，`README.zh-Hans.md`、`README.zh-Hant.md`、`README.ja.md` 内容完整且顶部互链。
- [x] README 事实清单检查 Release tag、资产名/大小/SHA-256、测试计数、关键产品常量、稳定章节和相对链接；GitHub Actions 执行该检查。

## 36. 第一方署名与许可证门禁（2026-08-02）

- [x] 所有第一方源码、测试、脚本、workflow、Cargo manifest、HTML、CSS 与原创 SVG 均包含项目名、创建者/主开发者、SPDX 版权与 Apache-2.0 标识。
- [x] shebang、HTML doctype 与 XML declaration 保持在语法要求的位置，C/C++、Rust、TypeScript、CSS、Shell、Python、PowerShell、YAML、TOML、HTML 与 SVG 使用各自合法注释语法。
- [x] third_party/**、所有 lock、app/src-tauri/gen/**、LICENSE 与第三方归属内容没有被批量盖上 Arsenic-er 版权。
- [x] AUTHORS.md、NOTICE、.github/CODEOWNERS、npm/Cargo/Tauri metadata 与四语言 README 统一记录 Arsenic-er 为项目创建者及主开发者。
- [x] Tauri bundle 资源包含 AUTHORS.md、NOTICE、LICENSE 与 THIRD_PARTY_LICENSES.md，并保持 English、SimpChinese、TradChinese、Japanese 四语言 NSIS 配置。
- [x] scripts/check-source-attribution.py 使用 Git tracked + untracked allowlist，漏头、第三方误盖章和未分类文件均 fail closed；GitHub Actions documentation job 执行该检查。
- [x] 当前前端证据更新为 16 files / 145 tests；README 四语言事实检查必须继续通过。

## 37. 双点链路通视分析（2026-08-02，源码与自动化已通过；受管运行、Windows 与性能待验）

本节执行 ADR 0024。当前源码已经形成独立的 link 请求/结果、Rust 分析核心、Tauri/validation 路由、四语言 React 工作区和 SVG 剖面；本次最终门禁确认为前端 16 files / 145 tests 与 Rust workspace 131 passed / 5 ignored。以下勾选只代表代码与自动化证据，不代表受管 validation 进程已经重建/重启、公开服务已经部署、真实 200 km 缓存链路已经跑通，或 Windows 实机已经验收。

### 37.1 请求、路径与数据完整性

- [x] 覆盖与链路使用独立请求/结果 schema；链路不复用覆盖 `CalculationRequest` 的中心点、固定圆或手动 TX 地面海拔字段。
- [ ] 地图第一次单击选择 TX、第二次选择 RX 已实现；Rust 权威路径严格使用 WGS84 并接受 `1,000..=200,000 m`，但前端禁用按钮前仍以 mean-Earth haversine 做近似范围预检，因此 WGS84 精确边界端到端门禁尚未关闭。
- [x] Rust 使用 `interval_count=ceil(D/90)`、样本数 `interval_count+1`、间距不超过 90 m，所有内部样本用 WGS84 direct，首尾点精确。
- [ ] 路径专用资产 planner 尚未实现；当前 `AppService::analyze_link` 复用以 TX 为中心的 200 km `plan_glo90_region`，会准备多于路径和插值余量的 DEM/WBM 单元。缺失、损坏和 NoData 继续 fail closed。
- [x] 原始 DEM 样本只构造一次 ITM PFL；曲率显示字段不修改 PFL，F1/P.526 诊断不追加到 ITM 基本损耗。
- [ ] validation exact-ID 运行框架已接入 `/api/link-analysis`，但尚缺链路专属取消、迟到结果和真实数据失败恢复烟测。

### 37.2 曲率、菲涅尔与解析值

- [ ] 固定 `k=4/3`、`Re=6,371,008.8 m` 与精确正弦隆起公式已实现并有短路径单测；200 km 中点 `588.6 ± 0.5 m` 的 Rust 回归仍需补齐。
- [ ] F1 公式及 145/435 MHz 相对变化已有单测，SVG fixture 覆盖 145 MHz / 200 km / 321.5 m；145.00 MHz 与 435.00 MHz 的 200 km Rust 解析值双回归仍需补齐。
- [x] F1 两端为 0 且不参与归一化最小值；内部 `0.60` 和 `-1.0` 分类边界包含等号并有上下测试。
- [ ] 频率、增益与几何不变性已有定向测试；AGL、功率、门限对几何/预算的完整正交敏感性矩阵尚未完成。
- [ ] 平地与单山脊已有核心测试；双山脊、纯曲率遮挡和海面端到端合成剖面仍待补齐。

### 37.3 极化、链路预算与三分类

- [x] 同极化失配为 0 dB，正交极化使用公开的 20 dB 规划损耗；结果与四语言 UI 均显示该假设。
- [x] ITM 使用 TX 极化，20 dB 只在链路预算中扣除一次，不修改 PFL 或 ITM 输出。
- [x] RX 规划门限默认 `-120 dBm` 且可编辑；结果序列化并显示 ITM 基本损耗、失配、预测 RX dBm、门限与 margin。
- [ ] 负 margin、正 margin 与增益 +10 dB 已有自动化；`margin=0` 精确边界和 TX 功率 10 倍的显式端到端回归仍需补齐。
- [x] `direct-los` 要求充分 F1 净空对应的 DirectLineOfSight 几何、ITM LineOfSight 和非负 margin。
- [x] 不满足全部 direct 条件但 margin 非负归 `obstructed-usable`。
- [x] 负 margin 归 `predicted-unavailable`；未知 ITM 模式 fail closed，文案不把预算不足误写成“完全遮挡”。
- [x] 三类结果与免责声明均称当前输入、DEM、标准大气、模型和门限下的规划预测，不保证现场通联。

### 37.4 SVG、动态距离刻度与状态隔离

- [x] SVG 使用“曲率抬高地形 + 端点直线射线”的等价表示，并绘制完整 +/-1.0 F1、0.6 F1、最严重净空点；游标显示原始 DEM、地球隆起、射线和 F1。
- [x] X 轴动态步长使用 `1/2/5 × 10^n` 且始终包含 0 和 D；200 km fixture 与精确端点有自动化。
- [ ] 小于 10 km 的 m 标签、窄窗口 ResizeObserver、真实字体标签裁切和 DPI 仍需受管浏览器/Windows 视觉验收。
- [x] 游标二分查找最近的完整权威样本，不重算分类或 ITM。
- [ ] SVG 游标、resize、主题和语言本身不调用后端；地图/卫星切换、运行中取消与所有组合的零调用门禁仍待补齐。
- [x] “清空链路”不清热力图/阈值/覆盖点；模式与语言切换保留链路端点、参数、结果和覆盖状态。
- [x] `en`、`zh-CN`、`zh-TW`、`ja-JP` 资源保持 363-key parity。
- [ ] 浅/深主题、1080×700、Windows DPI 缩放和 validation 浏览器视觉仍待验收。
- [ ] 已缓存 200 km 单链路 2 秒目标与真实 DEM/WBM/ITM 分阶段耗时尚未取得运行证据。

### 37.5 本轮执行证据与发布边界

- [x] `scripts/node-project.sh --prefix app test`：16 test files / 145 passed。
- [x] `scripts/cargo-project.sh test --workspace --all-targets --locked`：131 passed / 5 ignored。
- [ ] 受管 validation 进程尚未按链路源码 stop/build/start；没有链路 live HTTP、真实缓存或受管浏览器验收，也没有公开部署。
- [ ] v0.1.0-alpha.2 不含本功能；新的 Windows EXE/NSIS 尚未构建，Windows 10/11 WebView2 实机尚未验收。
