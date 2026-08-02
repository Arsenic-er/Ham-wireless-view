# validation 在线普通地图代理：天地图主路径与 CARTO 无 token 回退

- 日期：2026-07-31
- 范围：私有 validation server 在线底图、MapLibre 动态比例尺和覆盖层状态重放
- 状态：既有天地图禁用态历史证据保留；2026-08-02 已完成无 token CARTO Voyager/OSM 回退源码和 Rust 自动化，受管重建、live 瓦片与浏览器视觉待执行
- 当前角色：ADR-0022 将天地图设为 validation 在线普通地图主路径；代码切换和部署完成前不得把目标写成运行现状
- 发布结论：不构成中国大陆公开地图合规、审图号、在线叠加授权或导出授权证明

## 1. 目的与边界

验证平台目标为使用天地图 Web Mercator 普通地图与中文注记；缺少 token 或请求失败时显示 WGS84 坐标网格，不再回退 PMTiles。浏览器只能访问同源路径：

- `/api/basemap/tianditu/vec/{z}/{x}/{y}`
- `/api/basemap/tianditu/cva/{z}/{x}/{y}`

服务器只允许上述两个图层、十进制整数坐标和 `0..18` 级缩放，并校验 `x/y < 2^z`。上游固定为天地图 HTTPS `vec_w/cva_w` WMTS，不接受浏览器提供的主机、URL、查询参数或 token，因此该端点不能用作通用代理或 SSRF 跳板。

## 2. bootstrap 契约

`GET /api/bootstrap` 保留原字段并新增：

```json
{
  "basemap": {
    "enabled": false,
    "providerId": "tianditu",
    "displayName": "天地图",
    "attribution": "天地图",
    "mode": "same-origin-proxy",
    "maxZoom": 18,
    "layers": [
      { "id": "vec", "displayName": "矢量底图" },
      { "id": "cva", "displayName": "中文注记" }
    ],
    "tilePathTemplate": "/api/basemap/tianditu/{layer}/{z}/{x}/{y}"
  }
}
```

令牌文件缺失时 `enabled=false`，合法瓦片请求返回 503；前端继续显示 WGS84 内部测试画布。服务器不会把令牌、令牌文件路径或上游 URL 返回给浏览器。

## 3. 安全配置 token

令牌固定保存在：

```text
.runtime/validation-platform/secrets/tianditu.token
```

`.runtime/` 已被 Git 忽略。目录权限为 700，文件权限为 600。不要把 token 放在命令行参数、环境变量、URL、日志或聊天消息中。

在服务器项目目录运行交互式隐藏输入：

```bash
scripts/validation-platform.sh basemap-token set
```

命令只在 TTY 中读取，不回显输入。token 必须为 16–128 个 ASCII 字母或数字；写入采用同目录临时文件、600 权限和原子替换，失败时清理临时副本。

检查与删除时不会输出 token：

```bash
scripts/validation-platform.sh basemap-token status
scripts/validation-platform.sh basemap-token clear
```

配置变化需要通过受管流程重启服务后生效。不要手工结束 PID；部署者应使用 `validation-platform.sh` 的 stop/build/start 命令。

## 4. 上游响应限制

代理设置：

- 仅 HTTPS，固定 `t0.tianditu.gov.cn`；
- 禁止跟随重定向；
- 全局超时 8 秒、连接超时 3 秒、接收体超时 5 秒；
- 单瓦片上限 2 MiB；
- 只接受 Content-Type 与 PNG/JPEG/WebP 文件签名一致的响应；
- 浏览器响应固定 `Cache-Control: no-store` 和现有安全响应头；
- 上游错误使用通用 502 信息，不记录内部 URL或 token。

## 5. 地图显示与动态比例尺

启用并通过前端信任检查后，MapLibre 添加两个 256 px raster source：

- `vec`：天地图矢量底图，位于程序生成的经纬网和分析图层下方；
- `cva`：中文注记，位于热力图上方、发射点标记下方。

前端只接受固定的 `tianditu + same-origin-proxy + vec/cva` 元数据和同源模板；其他 provider、模板、缩放或缺图层组合均 fail closed。界面显示 bootstrap 给出的“天地图”署名，不从瓦片内容推导或重绘行政边界。

右下角使用 MapLibre 原生 `ScaleControl`，固定为 metric 单位和 120 px 最大宽度。控件监听地图 move 事件，缩放和平移时自动重新计算地面距离，并在米与千米之间切换。发射点坐标卡保持在左下；浅色/深色主题只改变控件外观，不改变距离。

## 6. 清空残留缺陷

原 `MapView` 的 point/heatmap effect 在 `map.isStyleLoaded() === false` 时直接返回。若用户在 ImageSource 更新导致样式短暂未就绪期间清空，唯一一次 props 变化会被丢弃；样式恢复后依赖没有再次变化，旧 raster layer、source 和 Blob URL 可能残留。

修复后，MapView 用 refs 保存最新 basemap、point、result、preview 和 stale 状态，并维护 desired-state 同步函数：

1. load 建立基础图层后同步最新目标状态；
2. style 不可操作时设置 pending，不丢弃目标状态；
3. `styledata` 或 `idle` 到达时只重放 pending 状态，避免无条件 ImageSource 更新；
4. 清空时删除 heatmap layer/source，并清理 Blob URL lease；
5. 卸载时撤销同步入口并执行幂等清理。

## 7. 已有自动化证据

2026-07-31 在服务器工作区运行：

- 前端 `src/lib/basemap.test.ts` 与 `src/components/MapView.test.tsx`：2 个文件、4 项测试通过；
- Rust `hamheatmap-validation-server` 的 `basemap::tests`：4 项测试通过。

前端测试覆盖固定同源模板信任检查、`vec/cva` 添加/移除、右下公制比例尺，以及“已有覆盖层 → style 未就绪 → 清空 props → idle 恢复”的时序。清空回归使用真实 `MapOverlayBlobUrlLease`，确认 layer/source 删除且对象 URL 只撤销一次。

Rust 专项覆盖严格路径和矩阵边界、固定上游 URL、token 缺失/有效/无效文件的 fail-closed 行为，以及响应 MIME 与图片签名一致性。路由与 bootstrap 测试代码还断言 disabled、非法路径、查询注入、错误 HTTP 方法，以及元数据不泄露 token 或上游 URL。

全量门禁同时通过：

- 前端 9 个文件、56 项测试，TypeScript 与 validation Vite production build；
- Rust workspace all-targets locked test 与 Clippy `-D warnings`；
- Windows x64 xwin workspace/all-targets check；
- `bash -n`、validation platform self-test 和 `git diff --check`。

## 8. 受管禁用态部署

revision `6e9714c6cdcdeb54ff47e229d8d43b18bf32b3c6` 于 2026-07-31 完成受管 `stop → build → start → status/health`。构建时间为 `2026-07-31T12:19:55Z`，server SHA-256 为 `d5f57bd71de4f64c62359591edbbee9b23461461d63265b68dd2a5f9dac640f9`；进程 PID `2306446` 只监听 `127.0.0.1:1421`，argv 包含固定 token 文件路径。

`GET /api/bootstrap` 返回固定天地图元数据、`enabled=false`，且不包含 token 或上游主机。合法 `GET /api/basemap/tianditu/vec/3/6/3` 返回 HTTP 503、JSON 和 `Cache-Control: no-store`。这证明未配置 token 时服务不会伪造或回退到来源不明的瓦片；它不证明真实上游可用。

### Windows 显式连接自检

Windows/Tauri 设置界面增加“保存并测试”和“测试连接”。保存只说明 DPAPI 密文已经原子提交；随后或单独点击测试时，`probe_online_basemap` 才访问固定中国区域代表瓦片。探测复用正式瓦片代理的 HTTPS-only、零重定向、超时、2 MiB、MIME 与图片签名门禁，不写缓存、SQLite、浏览器存储或诊断报告。

结果契约固定为 schema 1 和六种脱敏状态：`reachable`、`not-configured`、`network`、`timeout`、`upstream-or-credential`、`invalid-content`。前端拒绝未知 schema/状态，全部中文提示由本地固定映射生成；响应不包含 token、上游 URL、响应正文、文件路径或供应方错误。由于供应方可能用相同 HTTP 状态表示无效 Key、额度、权限和服务故障，产品不得把 `upstream-or-credential` 说成已精确定位某一种原因。

该自检只证明测试时点的一张代表瓦片可以通过完整校验链，不承诺所有图层、缩放级别、地区或未来时刻持续可用。真实有效个人 `tk`、中国大陆 ISP、Windows DPAPI 和 WebView2 仍需实机验收。


## 9. 尚未验证与发布门槛

当前 token 未配置，因此没有请求真实天地图瓦片，也没有记录上游 HTTP 200、瓦片哈希、浏览器截图、控制台结果、弱网表现或调用额度。代码级测试不能替代真实上游烟雾。

仍需单独完成：

- 取得并按服务条款管理可用于本项目验证的 token；
- 配置 token 并通过受管重启激活底图，在 SSH 隧道内验证 `vec/cva`、缩放、比例尺、署名、热力图层级和清空；
- 确认天地图服务条款、调用额度、必要署名和测试用途；
- 为 Windows 桌面在线显示取得明确的服务、热力图叠加、应用分发和所需 PNG/PDF 导出授权；
- 取得有效审图号，并确认缩放、裁切、热力图叠加、标记、浅/深样式和导出是否需要送审；
- 将正式 provider 的版本、审图号、授权标志、覆盖范围、瓦片/完整性清单纳入发布门禁；
- 在 Windows 10/11 WebView2 实机验证网络、缓存、安装/卸载和导出。

在线代理响应为 `Cache-Control: no-store`，本切片没有离线底图缓存或正式地图导出。在门槛关闭前，界面的“天地图在线真实底图 · 内部验证”只描述瓦片来源和运行模式，不能解释为软件已经满足公开地图发行要求。

## 10. WGS84 降级与历史 PMTiles

天地图 token 缺失、断网、额度或上游失败时，地图只保留 WGS84 坐标网格、发射点、历史站点、200 km 圆、比例尺与热力图。已缓存完整 DEM/WBM 的区域仍可计算；在线地图状态不能替代或修改计算资产。

四省 PMTiles、Range 路由和本地 places 已退出当前目标；其自动化与资产证据继续记录在 docs/21-protomaps-four-province-basemap.md。服务器约 33 MB runtime 资产尚未删除。

validation 卫星图继续使用同源 EOxCloudless。卫星失败可先回退在线天地图普通地图；普通地图也不可用时进入 WGS84 网格。所有在线响应保持 `no-store`，不进入 2.5 GB DEM/WBM 与计算缓存。

当前受管服务尚未按上述目标重启，本节不能作为运行验证证据。

## 11. 2026-08-02：无 token CARTO Voyager / OSM 回退

ADR-0025 取代本文件中“token 缺失时 `enabled=false`”的现行行为，但不改写第 8 节已经发生的历史部署证据。validation server 启动时选择一个且仅一个普通地图 provider：

- token 文件合法存在：保持 `tianditu`、`vec/cva` 和 `/api/basemap/tianditu/{layer}/{z}/{x}/{y}`。
- token 文件不存在：返回启用的 `carto-voyager`、`base/labels`、`maxZoom=18` 和 `/api/basemap/carto/{layer}/{z}/{x}/{y}`。
- `base` 只访问固定 `https://a.basemaps.cartocdn.com/rastertiles/voyager_nolabels/`；`labels` 只访问同主机 `rastertiles/voyager_only_labels`。两者末尾只能是已校验的规范 `z/x/y.png`。
- CARTO 模式 bootstrap 显示 `CARTO Voyager / OpenStreetMap` 与 `© OpenStreetMap contributors © CARTO`，不得继续显示“天地图”名称或图层。
- 两个 provider 复用 HTTPS-only、零重定向、8 秒全局/3 秒连接/5 秒接收超时、2 MiB、MIME/签名和 `no-store` 门禁；bootstrap 不暴露任何上游主机。
- 当前 provider 的 parser 排斥另一个 provider 的路径，因此无 token 请求天地图或有 token 请求 CARTO 都是 404，不会落入错误上游。

Rust 专项已通过 26/26，覆盖 CARTO `base/labels` 严格路径、z0/z18、矩阵边界、非规范十进制、查询/额外分段、固定 URL、无 token 元数据、有 token 天地图保留、MIME/签名与 HTTP 路由。受管服务尚未重建或重启，因此本节不声明 live HTTP 200、地图可见或中国大陆网络可达。
