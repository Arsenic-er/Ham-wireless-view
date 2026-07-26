# HamHeatmap MVP 测试计划

- 文档版本：0.1-draft
- 日期：2026-07-16

## 1. 测试层级

1. Rust/C++ 单元与回归测试。
2. 合成地形集成测试。
3. 真实 DEM 区域测试。
4. Tauri 桌面端到端测试。
5. Windows 10/11 64 位实机测试。
6. 数据许可、署名和地图合规发布检查。

## 2. 模型测试

- NTIA ITM 官方 point-to-point 样例全部通过。
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
- 只保留距中心不超过 200 km 的像素。
- 圆外结果透明且不能导出为有效像素。
- 发射点必须位于合规中国大陆有效区。
- 海岸和边境站点仍生成完整 200 km 圆。
- 接收点海拔逐像素从 DEM 取得。
- 发射点手动海拔只覆盖该点，不修改 DEM。
- 原始报告栅格保持 `401×401` 局部等距样本；地图另生成 `401×401`、轴对齐 EPSG:3857 覆盖层。
- 地图图像边界相对首末输出像素中心各扩展半个像素；精确 200 km 连续边界点必须位于图像域内，199 km 内侧 alpha 可见。若最近像素中心透明，`3×3` 邻域内最近可见中心误差不得超过该处 WGS-84 实算一个输出像素对角线。
- 纬度 18°、30.5°、40°、54° 的代表性覆盖层定位误差均小于 1 km。
- 反向重采样对有限邻域重新归一化；圆外、原始 NaN、原始域外和无有限贡献像素透明。

## 4. UI 测试

- 首次启动默认跟随系统主题。
- 浅色/深色切换即时生效并持久保存。
- 两种主题下色标数值和颜色顺序一致。
- 底图不存在 hillshade、等高线、坡度、高程着色或 3D terrain。
- 热力图像素不响应 hover/click，也不存在像素查询命令。
- MapView 只使用独立 Web Mercator 覆盖层字段；内部报告继续使用原始局部等距 PNG，二者不能误接。
- 发射点信息显示坐标、海拔、Maidenhead 与缓存状态。
- 参数变化后旧结果变淡、显示过期状态且禁止导出。
- 计算时参数锁定；取消后恢复。
- Rust worker 在接收点之间和长剖面采样期间响应取消；取消后不编码或保留半成品。
- 清空删除点和热力图，不删除参数和缓存。
- validation-server 模式中，计算与下载进度由约 250 ms 的非重叠状态轮询驱动，并复用 Tauri 已有的进度监听接口。
- 多标签页同时存在时，取消只影响本页 exact operation ID；旧 ID、旧 generation 或迟到 poll 不能覆盖新任务进度/结果。

## 5. 数据与缓存测试

- 缺数据且在线时先显示预计大小，再经用户确认下载。
- 缺数据且离线时禁止计算。
- 下载中断后可恢复或安全重下单个瓦片。
- 同一地理单元的 DEM/WBM 成对 `404` 时生成可校验纯海洋资产；单边 `404` 或其他错误必须阻断。
- 校验失败的数据不能进入 ready 状态。
- 临时文件计入配额。
- 持久数据总量永不超过 2,500,000,000 字节。
- 空间不足和配额不足分别提示。
- 删除区域后对应离线计算失效，其他区域不受影响。
- 缓存管理不展示 DEM 图像或高程预览。

## 6. 导出测试

- 内部诊断 PNG 固定为 `1600×1100`，包含完整 200 km 圆、发射点、100 km 比例尺、精确 dBm 色标、输入、统计、版本、时区、限制和不可移除水印。
- 内部诊断 PDF 为 A4 横向单页，嵌入同一报告 PNG，解析后页数和页面尺寸正确。
- 非 PNG MIME、非法 Base64、非 `1600×1100` 图像和超限负载均被 Rust 拒绝。
- 保存取消不创建文件；写入失败不覆盖已有目标且不留下临时文件。
- 浅色/深色主题均可导出；热力图颜色不变。
- 参数过期、计算取消或数据错误时禁止导出。
- 导出图没有像素查询信息或交互残留。
- 正式地图 PNG/PDF 只有在底图供应者清单同时具备审图号、署名、离线授权和导出授权时才能启用。

## 7. 性能测试

基准硬件为普通四核 Windows 10/11 64 位电脑：

- 已缓存区域目标 60 秒内完成。
- UI 事件响应延迟目标小于 100 ms。
- 进度至少每秒更新一次。
- 计算峰值内存目标小于 2 GB，最终以基准结果确定。
- 1、2、4、8、16 线程记录剖面提取、ITM、渲染和总耗时。

性能目标未达成时优先优化缓存、分块、剖面复用和 FFI，不改变 1 km 输出要求。

## 8. 隐私与安全测试

- 抓包确认坐标、功率和结果不上传。
- 下载只访问白名单 HTTPS 域名。
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

完整记录见 `12-phase2-export-slice.md`。正式地图导出继续受底图、审图号、署名、离线授权和导出授权硬门槛约束。

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

- `CacheStore::open` 在执行硬上限判定前先整理索引和文件系统：同步未及时写入 SQLite 的 partial 长度，删除 ready/missing/corrupt 资产的陈旧 partial，并把“最终文件已改名、索引仍为 downloading”的资产校验后推进为 ready。
- 只有与 downloading 状态、期望大小、强 ETag 和 Range 能力一致的 partial 才允许续传；不可信或超出期望大小的 partial 被删除或标记损坏。
- 精确到自定义测试上限的可信 partial 可以重开；再增长 1 byte 必须拒绝。区域和资产元数据写入执行预留空间检查，批量区域写入使用事务，失败后不能留下半个区域或孤立资产记录。
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
