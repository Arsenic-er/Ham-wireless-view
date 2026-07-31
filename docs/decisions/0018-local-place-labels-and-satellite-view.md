# ADR-0018：离线中文地名与可切换的在线卫星视图

- 日期：2026-08-01
- 状态：已采纳设计，代码、真实服务与浏览器验证待完成

## 背景

固定四省 PMTiles 已能稳定显示 earth、landcover、landuse、water 和 roads，但 2026-07-31 的可信样式主动隐藏了归档中已有的 places，用户无法通过地名识别位置。只读资产核查确认 places 已携带简体中文、本地名、英文、类型和最小缩放字段，并覆盖大量城市、县区与乡镇，因此当前需求不需要第二份地名数据库或更大的地图归档。

用户还需要“地图 / 卫星”视觉切换。卫星视图的目的只是帮助辨认现实地表，不承担 DEM、WBM、地形遮挡、陆水参数或传播分析职责。桌面软件仍是离线优先；在线卫星服务不可用时必须保留可用的离线地图。

## 决策

### 1. 复用 PMTiles places

1. 把 places 从隐藏层改为可信第六 source layer；earth、landcover、landuse、water、roads 保持不变，boundaries 与 pois 继续不渲染。
2. places 只建立省级、主要城市、县区和乡镇 symbol layer。当前 z0-9 资产不保证村、自然村、社区、道路门牌或街道名称完整性，产品和验收不得作此承诺。
3. 名称按 `name:zh-Hans`、本地 `name`、`name:en` 顺序回退。无可用名称时不生成空标签。
4. zoom、`kind`、`kind_detail`、`min_zoom`、人口/首府信息和 symbol collision 共同控制密度；低缩放优先省与主要城市，高缩放逐步加入县区与乡镇。

### 2. 本地字形而非 glyph 服务

1. MapLibre style 不设置 glyph URL。MapLibre GL JS 5.24.0 在缺少 glyph URL 时使用 TinySDF 在客户端生成字形。
2. 地图构造显式设置中文系统字体栈：`Microsoft YaHei, Noto Sans CJK SC, PingFang SC, sans-serif`。
3. 本阶段不下载或打包 WOFF/TTF、glyph PBF、字体服务或第二份地名文件；不新增外部字体网络域名，也不放宽 CSP。
4. 系统字体差异可能造成跨平台字宽和碰撞细微变化；Windows 10/11 简体中文可读性必须实机验证。若未来实机证明字体缺失，再单独决策固定字体资产，不在本 ADR 中预先占用空间。

### 3. 图层顺序

渲染顺序固定为：

```text
基础地表 / 道路 / 水体
经纬网
传播热力图
200 km 范围与地名注记
发射点
```

热力图无论是渐进预览、最终结果还是过期结果都不得覆盖地名；发射点始终最高。地图状态重放、主题切换、清空和 provider 切换必须保持该顺序。

### 4. 地图 / 卫星切换

1. “地图”使用固定 PMTiles 离线底图；“卫星”使用 EOxCloudless Sentinel-2 2025 的 `s2cloudless-2025_3857` WMTS，EPSG:3857，z0-14。
2. 前端只请求固定同源模板 `/api/basemap/satellite/{z}/{x}/{y}`。受管后端固定上游为 `https://tiles.maps.eox.at` 和 Sentinel-2 2025 WMTS path，TileMatrix/TileRow/TileCol 分别映射 z/y/x；2026-08-01 已以真实 JPEG 响应复核路径与 MIME。
3. 代理只允许规范十进制坐标与 z0-14，使用 HTTPS、禁止重定向、设置有界超时和响应体上限、校验 JPEG MIME/签名，且返回 `Cache-Control: no-store`。浏览器不能提交上游 URL、host、query、token 或 header。
4. 卫星 raster 只替换地图视觉背景，本地 PMTiles places 可继续作为上层地名。切换保留 camera、发射点、200 km 圆、热力图、参数和计算状态。
5. 浏览器断网或卫星 source 请求失败时自动切回 PMTiles 地图并显示非阻塞提示；不得留下空白画布，也不得把失败内容缓存。
6. EOxCloudless 瓦片不写入 Rust 数据根、SQLite、浏览器持久存储、Service Worker 或离线包，不进入缓存管理；十进制 2,500,000,000 字节硬上限及现有预算不变。
7. 卫星模式持续显示：`EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2025)`。2025 免费 WM(T)S 当前按非商业 CC BY-NC-SA 4.0 使用；商业用途必须取得适用 EOX 商业授权。
8. 卫星像素不得进入 DEM/WBM、地形剖面、陆水比例、ITM、缓存准备、统计或导出分析结论。卫星图是视觉参照，不是“更真实的传播数据”。

## 未采用方案

- 独立地名数据库：现有 PMTiles 已满足省、市、县区和乡镇目标，重复数据会增加版本、缓存和一致性负担。
- 远程 glyph 服务或全量 glyph PBF：本阶段 TinySDF 与系统字体已能零网络渲染中文；远程服务破坏离线能力，全量 PBF 增加资产与路由复杂度。
- 立即内置固定中文字体：会增加安装体积、许可清单和加载时序；先以 Windows 实机证据判断是否必要。
- Google Maps / Google Satellite：未采用，因为需要 API key 和计费账号，调用及离线缓存/再分发限制也与零凭据、离线优先、可控缓存目标不匹配。
- 离线打包卫星影像：z0-14 区域影像会显著增加体积并引入再分发授权，不符合当前 2.5 GB 预算和按需在线视觉参照定位。

## 后果

- 用户无需新增地图下载即可获得省、市、区县和乡镇级中文地名；PMTiles 文件大小、SHA-256 和 2.5 GB 配额不变。
- 字形生成完全本地且不新增字体数据，但不同 Windows 字体环境需要真实可读性验证。
- 在线卫星模式增加一个严格受限的 outbound provider 和失败回退状态；它不能成为离线主路径或计算依赖。
- EOx 服务可用性、速率与许可变化属于外部风险。provider 必须可禁用，禁用后地图与传播计算仍完整可用。
- Google 不是回退；在线卫星失败时唯一产品级回退是固定 PMTiles 地图。

## 验证与未关闭项

完整门禁见 docs/04-test-plan.md 第 29 节，资产与历史 PMTiles 证据见 docs/21-protomaps-four-province-basemap.md。至少需要：严格 metadata/路由测试、名称回退与 layer-order 单测、无外部 glyph 请求、浅深主题、四省代表城市 z4/z8/z10 浏览器视觉、EOX live JPEG、`no-store`、断网回退、缓存前后字节一致，以及地图/卫星切换前后传播结果字节一致。

截至 2026-08-01，地名渲染、卫星代理、切换 UI、断网/source error 回退、EOX live JPEG/no-store、全量自动化与受管部署已验证。Windows 字体实际可读性、四省代表城市碰撞密度、WebGL 控制台和地图/卫星切换前后的计算字节一致性仍未关闭；本 ADR 不替代这些浏览器与计算证据。
