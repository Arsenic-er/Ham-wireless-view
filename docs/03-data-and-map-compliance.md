# HamHeatmap 数据与地图合规设计

- 文档版本：0.1-draft
- 日期：2026-07-16
- 状态：地图合规路线为 P0 公开发布阻断项

四省 PMTiles 已退出当前产品目标；本文保留其历史来源、署名和使用边界，同时把现行视觉底图路线固定为纯在线。当前天地图 token 未配置，运行服务尚未按 ADR-0022 重启，约 33 MB 历史 runtime 资产也尚未删除。

## 1. 原则

计算数据与显示地图严格分离：

- DEM 和水体掩膜只服务于传播计算，不在地图上直接可视化。
- 显示底图必须来自合法在线服务，边界表示合规，具有有效审图信息，并取得桌面应用、热力图叠加与所需署名授权。
- 开发期底图、国际开源边界和计算数据均不能因为“开源”而自动进入面向中国大陆公众的发行版。
- 地图合规由正式发行物负责，包括应用界面、截图、PNG、PDF、宣传图片和项目网站。

本文是工程控制文件，不代替专业法律意见或自然资源主管部门的审核结论。

## 2. 数据分类

| 数据 | 用途 | 是否显示 | 候选来源 | 发布条件 |
|---|---|---:|---|---|
| DEM | 地形剖面与接收点海拔 | 否 | Copernicus GLO-90 | 许可、来源声明、版本锁定 |
| 水体掩膜 | 陆地/水体电气参数 | 否 | Copernicus DEM GLO-90 WBM 2021_1 / AWS COG | 许可、来源声明、版本锁定、校验和 |
| 基础底图 | 点选和空间参照 | 是 | 经审核/授权的中国大陆在线地图服务 | 审图号、在线服务/叠加授权、审核确认 |
| 中国大陆有效区 | 限制发射点选择 | 不单独显示 | 与合规底图一致的授权边界 | 不得自绘或使用冲突边界 |
| 在线卫星视觉层 | 联网视觉参照 | 是，不参与分析 | EOxCloudless Sentinel-2 2025 EPSG:3857 WMTS | 可用性、非商业/商业授权、署名、同源代理验证 |
| 热力图 | 用户计算结果 | 是 | 本地生成 | 作为叠加内容纳入地图审核评估 |

## 3. 高程数据

首选 Copernicus DEM GLO-90，下载适配器优先使用 AWS Open Data 匿名 COG 镜像。要求：

- 每个瓦片保存数据集、发布版本、URL、大小和校验和。
- 下载和报告保留 Copernicus 要求的来源声明。

当前 DEM 与 WBM 缓存版本标识均固定为 `COP-DEM GLO-90 2021_1 / AWS COG`。AWS Open Data 对象没有随 HEAD 响应提供经认证的逐瓦片 SHA-256；工程版在固定 HTTPS 域名和 Content-Length 验证后计算本地 SHA-256，用于后续防损坏校验。这不能替代发布清单的来源认证，正式版必须携带经复核并签名的瓦片大小/哈希清单。
- 不把 DEM 用作底图山体阴影、等高线、三维地形或高程着色。
- 地图前端无法直接访问 DEM 原始文件。
- 海岸外缺失 DEM 的水面可以取 0 m；陆地区域 NoData 必须阻断计算。

## 4. 水体数据

应用内部只有两类：

```text
0 = land
1 = water
```

若源数据区分 ocean/lake/river，导入时全部映射为 `water`。所有水体使用同一组 `epsilon` 和 `sigma`；不模拟海水/淡水差异、镜面反射、多径、潮汐或导波。

首版已经选择 Copernicus DEM GLO-90 同产品 WBM：

- WBM 为与 DEM 对齐的 8-bit GeoTIFF；`0` 映射为 `land`，`1/2/3`（海洋/湖泊/河流）统一映射为 `water`。
- DEM 与 WBM 通过固定 AWS Open Data HTTPS 主机匿名按区域下载，使用相同版本标识并分别保存大小、URL 和 SHA-256。
- 只有同一 1° 地理单元的 DEM 与 WBM 都明确返回 `404` 时，才按官方“纯海洋区域没有瓦片、高程可视为 0”的产品约定生成本地全零 DEM 和全水体 WBM。单边缺失或其他网络错误必须阻断，不能被误判为海洋。
- 生成的纯海洋资产具有固定编码版本和本地 SHA-256，使用原子写入并完整计入 2.5 GB 配额。

WBM 只用于计算，不作为可见水系底图。可见水系由合规底图提供。对 WBM 的布尔折叠和纯海洋本地生成属于改编/派生处理，发布时必须按 Copernicus 许可保留来源与改编声明。

## 5. 底图合规路线

### 5.1 开发阶段

- 可以使用 Natural Earth 或无边界坐标画布进行内部功能诊断。
- 开发构建必须显示“内部测试底图，不得公开发布”。
- 不制作含未经确认国界的宣传截图或公开演示包。
- 私有 validation 平台有 token 时通过同源代理显示天地图 `vec/cva`；无 token 时通过固定同源代理显示 CARTO Voyager / OpenStreetMap `base/labels` 作为内部开发回退。两者都必须保留来源和内部验证标记。在线服务可访问不等于已经取得桌面应用、热力图叠加或 PNG/PDF 导出授权，也不自动关闭审图门槛。
- 私有 validation 可测试 EOxCloudless 在线卫星视觉层，但必须显著署名、保持 `no-store`、不作离线预取，并把非商业许可与商业授权状态作为独立未关闭项。

### 5.2 历史：私有四省 PMTiles 验证

以下内容只保留已发生的资产与许可证据，不再定义当前 provider 或产品回退路径；现行决策见 ADR-0022。

- 固定归档：source build 20260731，bbox 107.5,18,125.5,33.5，z0-9，33,044,072 bytes，SHA-256 5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0。
- 归档含 939 个 region tiles、837 个 archive entries，占 2.5 GB 上限的 1.32%；tile payload 为 gzip 压缩 MVT。
- 只经回环 validation server 的同源 HTTP Range 读取；可信显示层限于 earth、landcover、landuse、water、roads、places。
- `places` 只显示省、主要城市、县区和乡镇，按简体中文、本地名、英文回退；z0-9 数据不保证村级、自然村或街道级完整性。
- boundaries 与 pois 不显示，但原始归档仍含 boundaries 以及 Natural Earth/OSM 内容。当前只用于私有验证、不纳入正式 EXE，且不作公开发行结论。
- 地名字形由 MapLibre TinySDF 从明确的本机中文字体栈生成；无 glyph URL，不新增字体或地名资产，也不发起第三方字体请求。
- 地图持续显示 © OpenStreetMap contributors；源数据按 ODbL Produced Work 记录，landcover 上游署名要求仍待确认。
- PMTiles JavaScript 4.4.1 为 BSD-3-Clause，传递依赖 fflate 为 MIT；它们在代码完全移除前继续保留许可证记录。天地图现为 validation 在线普通地图主路径。

### 5.3 EOxCloudless 在线卫星视觉层

- 已采用 EOxCloudless Sentinel-2 2025 `s2cloudless-2025_3857` WMTS，EPSG:3857 z0-14；只由固定同源、`no-store`、固定 HTTPS、零重定向代理读取，不允许浏览器直连、任意上游、凭据透传或离线批量抓取。
- 2025 免费 WM(T)S 的当前官方说明为非商业 CC BY-NC-SA 4.0；交互地图必须持续显示 `EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)`。任何商业使用必须先取得 EOX 适用商业授权。
- 该服务只提供在线视觉背景。卫星像素不得进入 DEM/WBM、陆水分类、ITM、覆盖统计、缓存准备或导出分析结论；“看起来像山、水或城市”不能替代计算数据。
- 瓦片不写入项目持久缓存或浏览器持久存储；上游不可达时回退在线天地图普通地图，再失败则显示 WGS84 坐标网格。因此 2.5 GB DEM/WBM 与计算缓存硬上限不变。
- Google Maps / Google Satellite 因 API key、计费账号、调用条款和离线缓存/再分发限制未采用。
- 2026-08-01 已完成固定同源代理、严格路由、真实 JPEG、`no-store`、署名 UI 与断网/source error 自动回退的自动化和 live HTTP 验证；商业授权与真实浏览器视觉仍未关闭。

### 5.4 Windows 在线天地图

- 桌面发行版由用户自行提供天地图 Key；项目、Git、EXE 和安装包不附带共享 Key，也不承诺替用户取得服务配额或授权。
- 普通地图固定组合 `vec_w+cva_w`，卫星图固定组合 `img_w+cia_w`，矩阵集为 Web Mercator `w`。传播坐标继续保持 WGS84，禁止以 GCJ-02 裸瓦片静默替换。
- WebView 只请求 `tianditu:` 自定义协议。Windows 原生层以 DPAPI 当前用户作用域保存密文并向固定 HTTPS 上游代请求；bootstrap、日志、URL 和错误正文均不得出现 Key。
- 原生层限制图层、瓦片坐标、缩放、主机、重定向、超时、2 MiB 响应、MIME 和图片签名，所有响应 `no-store`。在线瓦片不进入 2.5 GB 配额、离线包、缓存管理或诊断导出。
- 在线展示不等于允许批量下载、长期缓存、再分发或把底图嵌入导出。相关能力必须依据用户账号、供应方现行条款和独立授权决定。
- 国内服务可降低中国大陆网络访问障碍，但实际可用性仍取决于用户网络、Key 状态、配额和供应方服务；发布说明不得保证始终可访问。

### 5.5 公开发行阶段

必须同时满足：

1. 底图来自自然资源主管部门标准地图、天地图授权服务或具备资质的地图编制/服务单位。
2. 数据授权明确允许 Windows 桌面在线显示、热力图叠加、应用分发，以及项目实际需要的截图/导出行为。
3. 地图界面显著保留来源、审图号和授权要求的署名。
4. 不自行修改国界、省级行政边界或重要岛屿表示。
5. 对缩放、裁切、样式切换、热力图叠加、点标记和导出报告是否构成需重新审核的编辑进行正式确认。
6. 如果需要送审，只有取得批准后才能移除“内部测试版”标记。
7. 浅色/深色地图样式分别在授权和审核范围内；不能靠程序反色生成未经确认的新地图样式。

自然资源部标准地图服务说明指出：直接使用标准地图需标注审图号，对内容编辑，包括放大、缩小和裁切，公开使用前需要送审。因此静态标准地图不能直接被假定为可任意缩放的交互离线底图。

### 5.6 纯在线底图与无网边界

- 当前产品不规划离线视觉底图、离线地图包、批量瓦片下载、地图导入或地图缓存管理。
- 天地图、CARTO Voyager/OSM 与 EOxCloudless 响应统一 `no-store`，不得进入 EXE、安装包、Release、Rust 数据根、SQLite、Service Worker 或浏览器持久存储。
- Windows/Tauri 在线普通/卫星底图失败时只降级到 WGS84 坐标网格；私有 validation 的无 token 状态可以显式选择并署名固定 CARTO Voyager/OSM provider。任何环境都不静默切换 PMTiles、未知供应商或其他坐标系。
- 已缓存完整 DEM/WBM 的区域仍可在无网络时运行 ITM、显示分析层并导出无底图诊断 PNG/PDF；未缓存或损坏的计算资产继续阻断计算。
- 四省 PMTiles 必须排除在公开发行物之外；服务器约 33 MB 历史 runtime 资产尚未删除，后续须通过独立受管清理记录实际释放空间。
- 取消离线底图不改变 DEM/WBM、partial、索引和计算缓存的十进制 2.5 GB 硬上限。

## 6. 地图提供者接口

```text
CompliantBasemapProvider
├─ provider_id
├─ dataset_version
├─ review_number
├─ attribution_text
├─ allowed_online_display
├─ allowed_analysis_overlay
├─ allowed_export
├─ allowed_styles
├─ coverage_polygon
└─ service_capabilities
```

私有 validation 的 `BasemapInfo` 只是在线验证元数据子集：enabled、provider、署名、同源模式、最大缩放、provider 对应的 `vec/cva` 或 `base/labels` 和路径模板。它刻意不伪造 `review_number`、在线叠加/导出授权或覆盖多边形，因此不能被当作 `CompliantBasemapProvider`。当前 token 未配置，天地图真实瓦片未验证；CARTO 回退的受管重建和 live 烟雾也尚未执行。

私有 PMTiles bootstrap 元数据、相对 Range URL、bbox、zoom、大小与 SHA-256 只属于历史 validation 证据。现行 bootstrap 必须把当前普通 provider（有 token 的天地图或无 token 的 CARTO Voyager/OSM）、EOxCloudless 和 WGS84 降级能力明确区分，不得把 CARTO 元数据伪称为天地图。

token 只能保存在 Git 忽略的项目运行目录，通过静默交互和 `0600` 普通非符号链接文件管理；bootstrap、浏览器、日志和文档不得包含 token。浏览器只访问同源路径，上游固定 HTTPS 请求由回环 validation server 代发。该设计降低凭据暴露，不改变地图内容和使用方式的许可/审核义务。

生产构建在以下任一条件不满足时拒绝启动在线 provider，并回退 WGS84 网格：

- `review_number` 为空；
- 在线显示或热力图叠加授权未确认；
- 所需导出授权未确认；
- 数据版本不在发布白名单。

## 7. 缓存与数据最小化

- 所有持久数据总量硬上限 2,500,000,000 字节。
- 不为视觉底图分配持久预算；2.5 GB 全部用于 DEM、WBM、partial、SQLite/索引、计算缓存和安全余量。
- 实际分配由下载清单决定，但总量不可突破。
- 临时下载文件也计入上限。
- 缓存页只显示区域覆盖和大小，不显示 DEM 高程预览。
- 删除数据前明确告知哪些离线区域将失效。

- 天地图、CARTO Voyager/OSM 与 EOxCloudless 在线瓦片必须为 `no-store`，不得进入持久缓存、缓存配额统计或离线预取；十进制 2,500,000,000 字节上限不因地图模式改变。
- 四省 PMTiles 只作为尚未清理的历史 runtime 资产记录；它不属于现行预算，且公开 Release 检查必须确认未被误打包。

## 8. 署名与导出

应用地图界面和导出文件均保留：

- 底图来源与审图号。
- DEM 数据来源和版本。
- 水体数据来源和版本。
- ITM 模型名称和版本。
- 软件版本和生成时间。

不得以裁切、主题、紧凑布局或导出模板为由隐藏必要署名。

当前内部 Alpha 只输出无行政边界、无底图的局部等距诊断报告，并强制显示“内部测试，不得公开发布”、ITM/数据版本和模型限制。这不构成公开地图导出授权，也不允许把诊断报告用于宣传或正式发行。

正式 provider 必须在发行清单中同时给出非空审图号、署名、在线显示/叠加授权和所需导出授权；任一字段缺失时生产构建必须拒绝带底图地图导出，不能仅显示警告后继续。

Copernicus 数据署名以发布时适用的许可文本为准。当前工程记录的基础措辞为：`produced using Copernicus WorldDEM-90 © DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH 2014-2018 provided under COPERNICUS by the European Union and ESA; all rights reserved`；若发布布尔 WBM 或其他派生物，还需明确标注应用进行了改编。

历史 PMTiles 验证曾要求显示 © OpenStreetMap contributors；PMTiles/fflate 与 landcover 署名 caveat 在依赖和资产完全移除前继续记录于 THIRD_PARTY_LICENSES.md。

卫星模式必须在地图界面就近持续显示 `EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)`。本设计不授权离线打包 EOxCloudless，也不把在线查看权外推为 PNG/PDF 导出、再分发或商业使用权。

## 9. 发布检查清单

- [ ] 底图供应者身份、资质与授权文件已归档。
- [ ] 有效审图号已写入发行清单。
- [ ] 浅色、深色样式均在授权/审核范围内。
- [ ] 在线显示、热力图叠加、必要署名与应用分发权已书面确认。
- [ ] 发布产物检查确认不含四省内部 PMTiles、其他离线地图或在线瓦片副本。
- [ ] PNG/PDF 导出权已书面确认。
- [ ] 中国大陆有效区与底图使用同一合规边界来源。
- [ ] 热力图叠加和发射点标记已纳入审核评估。
- [ ] 应用、导出、网站和 README 的地图截图均通过检查。
- [ ] Copernicus 与其他数据署名完整。
- [ ] 生产构建不含 Natural Earth/OSM 开发边界包。

## 10. 官方依据

- 自然资源部标准地图服务：https://bzdt.tianditu.gov.cn/
- 《地图管理条例》：https://xzfg.moj.gov.cn/front/law/detail?LawID=421&Query=
- Copernicus DEM：https://dataspace.copernicus.eu/explore-data/data-collections/copernicus-contributing-missions/collections-description/COP-DEM
- Copernicus DEM Product Handbook（WBM 编码与产品结构）：https://dataspace.copernicus.eu/sites/default/files/media/files/2024-06/geo1988-copernicusdem-spe-002_producthandbook_i5.0.pdf
- AWS Copernicus DEM：https://registry.opendata.aws/copernicus-dem/
- OSM 标准瓦片策略：https://operations.osmfoundation.org/policies/tiles/
- Natural Earth 使用条款：https://www.naturalearthdata.com/about/terms-of-use/
- EOxCloudless WMTS 使用说明：https://cloudless.eox.at/documentation/usage
- EOxCloudless 许可摘要：https://cloudless.eox.at/documentation/license
- EOxCloudless 非商业许可与 2025 署名：https://cloudless.eox.at/license-non-commercial
