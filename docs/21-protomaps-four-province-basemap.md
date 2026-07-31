# 四省 Protomaps PMTiles 内部验证记录

- 日期：2026-07-31
- 范围：私有 validation 平台
- 状态：文件大小、SHA-256、PMTiles header/directory/MVT、自动化、HTTP Range、SSH 转发与受管运行已验证；真实浏览器视觉仍待人工确认
- 决策：docs/decisions/0017-private-regional-pmtiles-basemap.md

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
- 显示白名单仅包含 earth、landcover、landuse、water、roads。
- boundaries、places、pois 不得进入 MapLibre 可见样式。
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
- [x] 样式只引用 earth、landcover、landuse、water、roads 五个 source layer。
- [x] boundaries、places、pois 不出现在可见 style layer 中。
- [x] 前端测试确认内部验证署名文本存在；真实浏览器可读性仍在 5.3 单列待验。
- [x] 既有地图 desired-state 与延迟清空重放测试继续通过。

### 5.3 真实浏览器

- [ ] 首屏、缩放和平移会产生 Range 请求，且不会下载完整 33,044,072 bytes 归档。
- [ ] 道路、水体、土地覆盖与传播热力图空间对齐。
- [ ] 不显示边界、地名和 POI。
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
| frontend tests | 9 files / 62 tests PASS |
| Rust workspace all-targets locked offline | 112 passed / 5 ignored |
| workspace Clippy with -D warnings | PASS |
| validation server targeted tests | 27 / 27 PASS |
| validation platform bash -n + self-test | PASS |

这些结果覆盖功能提交 db052e6 的源码；提交后工作树干净，并已推送到 origin/codex/new-server-projection-validation。

### 6.2 受管运行与 HTTP

- clean stop/build/start 成功，PID 2372828，只监听 127.0.0.1:1421，health 正常。
- build metadata：revision=db052e6505830d48a51b2dd4711792163eba422b、built_at=2026-07-31T13:52:15Z、worktree_dirty=false、server SHA-256=cec01dc9d959334b9aba088b2ae91d1cc251f33c1d0592f81cdae54b95a97546。
- bootstrap 返回 enabled=true、providerId=protomaps、resourcePath=/api/basemap/pmtiles/four-provinces.pmtiles、bounds=[107.5,18,125.5,33.5]、maxZoom=9、archiveBytes=33044072。
- bootstrap/cache 报告 total=293,517,252 bytes；运行数据根约 293.5 MB。
- 实际安装的 PMTiles JavaScript 4.4.1 客户端对 live endpoint 执行 getHeader 和 getZxy(5,26,13) 成功，tile payload 为 117,880 bytes。
- live GET bytes=0-7 返回 206，Content-Range 为 bytes 0-7/33044072，8-byte body 十六进制为 50 4d 54 69 6c 65 73 03。
- live HEAD 返回 200，Content-Length 为 33044072，Accept-Ranges 为 bytes。
- 约 108 MB 的试验目录已删除；本轮底图试验资产只保留约 33 MB 的 PMTiles 归档。该清理不改变上面的整个运行数据根总量口径。
- scripts/prepare-four-provinces-pmtiles.sh 已通过完整 HTTP Range 提取、精确大小/SHA/verify、幂等快路径、坏哈希与符号链接拒绝及失败清理测试；未下载全球 137 GB 归档。

### 6.3 真实浏览器仍待完成

自动浏览器视觉因 Codex 桌面 node_repl 的 Windows sandbox ACL 故障未执行成功，因此 5.3 全部保持未勾选，也没有把前端单元测试外推为截图、WebGL、控制台或用户可见层级证据。

本机 127.0.0.1:1421 的既有连接仍可访问受管服务；用户可以直接刷新页面进行人工验证。此可访问性本身不算浏览器视觉通过。

## 7. 证据回填格式

每条证据至少记录：UTC 时间、revision、构建标识、命令或浏览器步骤、关键响应头/断言、结果、日志或截图路径。未执行、没有日志或只有设计推断的项目必须保持未勾选。
