# ADR-0017：私有验证平台使用区域 PMTiles 底图

- 日期：2026-07-31
- 状态：已采纳，仅限私有内部验证；自动化、浏览器与受管部署证据待补

## 背景

私有 validation 平台需要一个不依赖每次在线取瓦片、体积可控且可重复的底图，用于核对热力图与道路、水体、土地覆盖之间的相对位置。现有天地图同源代理仍受 token 和外网可用性影响，适合作为联网 fallback 与历史验证路径，不适合作为唯一的内部验证基线。

四省验证区域 Protomaps PMTiles 的固定基线为：

| 项目 | 值 |
| --- | --- |
| source build | 20260731 |
| bbox | 107.5,18,125.5,33.5 |
| zoom | 0-9 |
| 文件大小 | 33,044,072 bytes |
| SHA-256 | 5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0 |
| tile payload | gzip 压缩 MVT |
| 区域 tile / archive entry | 939 / 837 |
| 相对 2.5 GB 配额 | 1.32% |

归档原始内容仍含 boundaries 以及 Natural Earth / OpenStreetMap 来源内容；样式隐藏不等于物理删除。因此本决策只覆盖私有内部验证，不作公开发行结论。

## 决策

1. 私有 validation 平台把上述区域 PMTiles 作为内部验证底图；天地图保留为联网 fallback 和历史验证路径。
2. PMTiles 只经同源、受管、回环绑定的 HTTP Range 端点读取。客户端不得依赖整包下载；服务端必须正确处理合法 Range，并对无效或越界 Range 失败关闭。
3. MapLibre 只显示 earth、landcover、landuse、water、roads。不得显示 boundaries、places、pois。
4. 图层白名单只是显示约束，不是归档清洗。当前原始资产只用于私有内部验证，不纳入正式 EXE。
5. 地图上必须持续可见 © OpenStreetMap contributors。PMTiles JavaScript 4.4.1 按 BSD-3-Clause 记录；其传递依赖 fflate 按 MIT 记录；源数据按 ODbL Produced Work 处理；landcover 上游署名要求仍需确认。
6. 本 ADR 只确定架构边界。自动化测试、真实浏览器、HTTP Range、受管部署和视觉结果必须由实际证据回填；在此之前不得标记为通过。

## 后果

- 内部验证获得固定、较小的地图资产，归档体积占 2.5 GB 上限的 1.32%。
- validation server 和前端需要新增 PMTiles Range 与协议接入，并维护严格显示图层白名单和可见署名。
- 当前接入只记录内部工程可用性，不外推公开发行结论。
- 天地图代理文档与既有证据继续保留，但其角色变为 fallback / 历史验证。

## 验证记录

实现与运行证据统一记录在 docs/21-protomaps-four-province-basemap.md。截至本 ADR 建立时，自动化、浏览器与受管部署证据均为待补。
