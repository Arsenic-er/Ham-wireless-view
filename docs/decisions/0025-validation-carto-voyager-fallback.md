# ADR 0025：validation 无 token 时使用 CARTO Voyager / OSM 在线回退

- 状态：已接受；源码与 Rust 自动化已完成，受管重建和真实浏览器验证待执行
- 日期：2026-08-02
- 范围：私有 validation server 普通地图
- 不改变：Windows/Tauri 的用户自有天地图 Key、DPAPI 与 `tianditu:` 原生协议

## 背景

私有 validation 平台当前没有配置天地图 token。原行为把普通地图标记为 disabled 并只显示 WGS84 网格，无法满足在线开发期间对道路和详细地名的验证需求。该平台仍只通过 SSH 隧道在项目所有者控制的回环服务中使用，不是公开地图产品或 Windows 发行路线。

## 决策

validation server 在启动时按 token 文件状态选择且只选择一个普通地图 provider：

1. token 文件不存在时，bootstrap 返回启用的 `carto-voyager`、`base/labels`、`maxZoom=18` 和固定同源模板 `/api/basemap/carto/{layer}/{z}/{x}/{y}`。
2. `base` 固定映射到 `https://a.basemaps.cartocdn.com/rastertiles/voyager_nolabels/{z}/{x}/{y}.png`；`labels` 固定映射到同一主机的 `rastertiles/voyager_only_labels`。
3. token 文件合法存在时继续返回并使用原天地图 `vec/cva` 契约。无 token 状态拒绝天地图路径；有 token 状态拒绝 CARTO 路径，不能跨 provider 误路由。
4. token 文件存在但格式、权限或文件类型不合法时仍阻止服务器启动，不把错误配置静默解释为“未配置”。
5. 两条普通地图路径复用 HTTPS-only、零重定向、连接/接收/总超时、2 MiB、HTTP 200、MIME/图片签名和 `Cache-Control: no-store` 门禁。浏览器不能提交上游 URL、主机、路径后缀或查询参数。
6. bootstrap 只给出 provider、署名、图层和同源模板，不暴露 CARTO、天地图或卫星上游主机。CARTO 模式持续显示 `© OpenStreetMap contributors © CARTO`。
7. 在线瓦片不写入 Rust 数据根、SQLite、Service Worker、浏览器持久存储、2.5 GB 配额或 PNG/PDF 诊断导出。

## 后果

- 无 token 的 validation 普通地图可显示道路和在线地名；卫星图仍使用 EOxCloudless。
- CARTO/OSM 只作为私有 validation 的开发回退，不进入 Windows/Tauri provider、EXE 或离线地图包。
- 该接入不证明中国大陆公开地图合规、审图号、发行授权或中国大陆网络可达性。CARTO 服务条款、OSM 署名和实际使用限制仍需由使用方遵守。
- 上游故障或瓦片校验失败时前端仍可降级 WGS84 网格，并保留传播分析状态。
