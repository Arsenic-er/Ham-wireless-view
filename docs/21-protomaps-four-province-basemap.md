# 四省 Protomaps PMTiles 内部验证记录

- 日期：2026-07-31；地名与卫星视图需求更新：2026-08-01
- 范围：私有 validation 平台
- 状态：2026-07-31 的五层无地名底图基线已验证；2026-08-01 的 places 地名显示与地图/卫星切换实现、测试和部署尚未完成
- 决策：docs/decisions/0017-private-regional-pmtiles-basemap.md、docs/decisions/0018-local-place-labels-and-satellite-view.md

## 1. 结论边界

本记录为四省区域 PMTiles 接入提供可复查的工程基线；当前开发保留 OpenStreetMap 署名和上游许可记录，不以中国大陆公开发行合规作为功能验收阻塞。

## 2. 固定资产事实

| 检查项 | 固定值 |
| --- | --- |
| source build | 20260731 |
| bbox | 107.5,18,125.5,33.5 |
| zoom | 0-9 |
| 文件大小 | 33,044,072 bytes |
| SHA-256 | 5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0 |
| payload | gzip 压缩 MVT |
| region tiles | 939 |
| archive entries | 837 |
| 2.5 GB 配额占比 | 1.32% |

这些值是本轮接入的期望基线。后续部署必须重新计算哈希和大小并与本表精确比较；未产生执行日志前，不把比较写为通过。

## 3. 读取与显示约束

- 归档只允许通过同源 HTTP Range 读取，不允许前端整包抓取。
- 瓦片格式为 gzip 压缩 MVT；服务端不得自行解压后以错误的媒体或长度元数据响应。
- 2026-08-01 目标白名单包含 earth、landcover、landuse、water、roads、places 六个 source layer；boundaries 与 pois 仍不得进入 MapLibre 可见样式。
- `places` 只渲染省级、主要城市、县区和乡镇，按 `name:zh-Hans`、本地 `name`、`name:en` 回退；不保证村级、自然村或街道名称完整。
- 样式不配置 glyph URL，由 MapLibre TinySDF 从显式系统中文字体栈生成字形；不新增字体包、glyph PBF 或地名资产。
- 热力图必须低于地名注记，发射点必须位于全部视觉层之上。
- 地图必须持续显示 © OpenStreetMap contributors，包括热力图覆盖后的正常视图。
- PMTiles 文件在服务启动时缺失时可回退到需要 token 和网络的天地图代理；运行期间不做自动故障切换。

## 4. 许可与使用边界

原始 PMTiles 归档仍包含 boundaries 以及 Natural Earth / OpenStreetMap 来源内容；显示白名单不等于物理删除。当前边界保持为：

- 只允许回环绑定、经 SSH 访问的私有内部验证，不对公网暴露 Range 端点；
- 当前归档不纳入正式 EXE，且本记录不作公开发行结论；
- 源数据按 ODbL Produced Work 处理，并保留 OSM 可见署名；
- landcover 的上游数据许可与署名链仍需确认；
- PMTiles JavaScript 4.4.1 为 BSD-3-Clause；传递依赖 fflate 为 MIT。

## 5. 验证矩阵

已执行项按本轮真实日志与响应回填；没有浏览器证据的项目继续保持未勾选。

### 5.1 资产与协议

- [x] HEAD 与 bootstrap 报告文件大小 33,044,072 bytes。
- [x] 固定归档已重新计算 SHA-256，并与基线精确一致。
- [x] live header 为 PMTiles v3，bootstrap 返回固定 bbox 与 maxZoom 9。
- [x] PMTiles JavaScript 4.4.1 的 getHeader 已读取并解析 header 与 archive directory。
- [x] live GET 单段 Range 返回 206、正确 Content-Range 与精确长度；HEAD 另行确认 Accept-Ranges: bytes。
- [x] Rust 自动化覆盖缺 Range、越界、多段、开放区间、后缀区间和超过 8 MiB 的 Range，均按固定契约拒绝。
- [x] PMTiles JavaScript 4.4.1 的 getZxy(5,26,13) 已通过 live Range 读取并返回 117,880-byte MVT tile。

### 5.2 自动化

- [x] PMTiles 协议注册与卸载由前端测试覆盖。
- [x] bootstrap/底图能力信息能区分 PMTiles 主验证源、启动时天地图回退与禁用态。
- [x] 历史提交 db052e6 的样式只引用 earth、landcover、landuse、water、roads 五个 source layer。
- [x] 历史提交 db052e6 的 boundaries、places、pois 都不出现在可见 style layer 中；该项不代表新地名需求已完成。
- [x] 当前实现引用 places 作为可信第六 source layer，同时继续拒绝 boundaries 与 pois；错误顺序也 fail closed。
- [x] 当前实现覆盖简中/本地名/英文回退、系统字体 TinySDF、地名高于热力图及发射点最高层级。
- [x] 前端测试确认内部验证署名文本存在；真实浏览器可读性仍在 5.3 单列待验。
- [x] 既有地图 desired-state 与延迟清空重放测试继续通过。

### 5.3 真实浏览器

- [ ] 首屏、缩放和平移会产生 Range 请求，且不会下载完整 33,044,072 bytes 归档。
- [ ] 道路、水体、土地覆盖与传播热力图空间对齐。
- [ ] 不显示 boundaries/POI；省、主要城市、县区和乡镇地名按 zoom 与碰撞规则可读，且热力图不遮字、发射点不被文字遮挡。
- [ ] © OpenStreetMap contributors 清晰可见且未被控件遮挡。
- [ ] 动态公制比例尺、清空残留、渐进预览与最终覆盖层行为正常。
- [ ] 控制台无 PMTiles、MVT、CORS、Range 或 WebGL 错误。

### 5.4 受管部署

- [x] clean commit db052e6 的受管 stop/build/start、health 与 bootstrap 全链路成功。
- [x] Range 端点由只监听 127.0.0.1:1421 的 validation server 提供。
- [x] SSH 端口转发后的同源端点与 Range 访问成功；浏览器视觉烟测仍在 5.3 单列待验。
- [x] 重启后仍从同一受控资产读取，SHA-256 未漂移。
- [x] 当前 validation 前端与服务器构建产物不包含原始 PMTiles；正式 EXE/安装包尚未生成。

## 6. 已执行证据

### 6.1 自动化门禁

| 检查 | 结果 |
| --- | --- |
| frontend check | PASS |
| frontend tests | 9 files / 65 tests PASS |
| Rust workspace | 113 passed / 5 ignored |
| workspace Clippy with -D warnings | PASS |
| validation server targeted tests | 28 / 28 PASS |
| validation platform bash -n + self-test | PASS |

这些结果覆盖当前地名与卫星功能源码；最终提交完成后推送到 origin/codex/new-server-projection-validation。

### 6.2 受管运行与 HTTP

- 2026-08-01 的受管 stop/build/start 成功；服务只监听 127.0.0.1:1421，health 正常。
- 管理脚本重新构建 validation 前端与 release server，并通过 PID 身份、回环 readiness 与管理 self-test。
- bootstrap 返回 enabled=true、providerId=protomaps、六个 source layer、resourcePath=/api/basemap/pmtiles/four-provinces.pmtiles、bounds=[107.5,18,125.5,33.5]、maxZoom=9、archiveBytes=33044072，以及固定 satellite 能力。
- bootstrap/cache 报告 total=367,720,813 bytes、cap=2,500,000,000 bytes；卫星 live 请求前后没有新增持久卫星资产。
- 实际安装的 PMTiles JavaScript 4.4.1 客户端对 live endpoint 执行 getHeader 和 getZxy(5,26,13) 成功，tile payload 为 117,880 bytes。
- live GET bytes=0-7 返回 206，Content-Range 为 bytes 0-7/33044072，8-byte body 十六进制为 50 4d 54 69 6c 65 73 03。
- live HEAD 返回 200，Content-Length 为 33044072，Accept-Ranges 为 bytes。
- live GET /api/basemap/satellite/6/52/26 返回 200、image/jpeg、Content-Length 17,586、Cache-Control: no-store；body SHA-256 为 27583d4a910359cb18acb812e08bf2f19ecd2f3c50aaea004ecf8627a4507e7b。
- live bootstrap 不包含 tiles.maps.eox.at；浏览器只获得 /api/basemap/satellite/{z}/{x}/{y}。
- 约 108 MB 的试验目录已删除；本轮底图试验资产只保留约 33 MB 的 PMTiles 归档。该清理不改变上面的整个运行数据根总量口径。
- scripts/prepare-four-provinces-pmtiles.sh 已通过完整 HTTP Range 提取、精确大小/SHA/verify、幂等快路径、坏哈希与符号链接拒绝及失败清理测试；未下载全球 137 GB 归档。

### 6.3 真实浏览器仍待完成

自动浏览器视觉因 Codex 桌面 node_repl 的 Windows sandbox ACL 故障未执行成功，因此 5.3 全部保持未勾选，也没有把前端单元测试外推为截图、WebGL、控制台或用户可见层级证据。

本机 127.0.0.1:1421 的既有连接仍可访问受管服务；用户可以直接刷新页面进行人工验证。此可访问性本身不算浏览器视觉通过。

## 7. 证据回填格式

每条证据至少记录：UTC 时间、revision、构建标识、命令或浏览器步骤、关键响应头/断言、结果、日志或截图路径。未执行、没有日志或只有设计推断的项目必须保持未勾选。

## 8. 2026-08-01 地名资产核查与实现

本轮以服务器现有固定归档和已安装的 PMTiles JavaScript 4.4.1、`@mapbox/vector-tile` 进行只读检查，没有下载或改写地图数据：

- metadata 已确认 `places` 含 `name:zh-Hans`、`name`、`name:en`、`kind`、`kind_detail`、`min_zoom`、`population_rank` 与 `capital` 等字段。
- 扫描 bbox 对应的全部 z9 范围得到 675 个有数据 tile、29,458 个 place feature occurrence；按 kind/kind_detail/name 组合去重得到约 11,555 个名称键。该数值不是行政实体计数，只用于证明固定资产已有地名容量。
- 其中 216 个名称缺少 `name:zh-Hans` 但具有本地 `name`，因此必须保留本地名回退。福州、厦门、杭州、南昌和广州抽样均包含中文城市、县区和大量乡镇名称。
- 乡镇 feature 可出现在 z9 tile 并在 MapLibre z10 overzoom 显示；当前归档没有村级、自然村或街道名称完整性保证。

因此当前实现复用同一个 33,044,072-byte PMTiles 的 `places`，不建立第二份地名数据库，不扩大归档，不修改 2.5 GB 配额。MapLibre 不设置 glyph URL，使用 TinySDF 与 `Microsoft YaHei, Noto Sans CJK SC, PingFang SC, sans-serif` 系统字体栈；本轮零新增字体/地名持久数据。

### 8.1 地图/卫星切换

- “地图”继续使用固定 PMTiles；“卫星”已通过同源代理显示 EOxCloudless Sentinel-2 2025 EPSG:3857 WMTS z0-14。
- 卫星 raster 低于本地 places、热力图和发射点；准确顺序仍为 heatmap 低于地名、发射点最高。
- 卫星代理固定 HTTPS、零重定向、`no-store`，断网或 source error 自动回退 PMTiles；不写入持久缓存，因此 2.5 GB 上限不变。
- 卫星只作视觉背景，不是 DEM/WBM 或传播分析数据。Google 未采用，原因是 API key、计费及缓存/再分发限制。
- UI 持续显示 EOx 2025 官方署名；非商业/商业授权仍需在发行前复核。详细决策见 ADR 0018，完整门禁见 docs/04-test-plan.md 第 29 节。

地名 symbol layer、卫星代理、切换 UI、断网/source error 回退和真实 EOX JPEG 已形成自动化、受管部署与 live HTTP 证据。真实 Windows 字体、四省代表城市的碰撞密度、缩放平移、WebGL 控制台和最终视觉仍由用户在浏览器中验收。
