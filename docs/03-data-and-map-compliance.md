# HamHeatmap 数据与地图合规设计

- 文档版本：0.1-draft
- 日期：2026-07-16
- 状态：地图合规路线为 P0 公开发布阻断项

## 1. 原则

计算数据与显示地图严格分离：

- DEM 和水体掩膜只服务于传播计算，不在地图上直接可视化。
- 显示底图必须来自合法渠道，边界表示合规，具有有效审图信息，并取得桌面应用与离线缓存所需授权。
- 开发期底图、国际开源边界和计算数据均不能因为“开源”而自动进入面向中国大陆公众的发行版。
- 地图合规由正式发行物负责，包括应用界面、截图、PNG、PDF、宣传图片和项目网站。

本文是工程控制文件，不代替专业法律意见或自然资源主管部门的审核结论。

## 2. 数据分类

| 数据 | 用途 | 是否显示 | 候选来源 | 发布条件 |
|---|---|---:|---|---|
| DEM | 地形剖面与接收点海拔 | 否 | Copernicus GLO-90 | 许可、来源声明、版本锁定 |
| 水体掩膜 | 陆地/水体电气参数 | 否 | Copernicus DEM GLO-90 WBM 2021_1 / AWS COG | 许可、来源声明、版本锁定、校验和 |
| 基础底图 | 点选和空间参照 | 是 | 经审核/授权的中国大陆地图服务或数据包 | 审图号、授权、审核确认 |
| 中国大陆有效区 | 限制发射点选择 | 不单独显示 | 与合规底图一致的授权边界 | 不得自绘或使用冲突边界 |
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

### 5.2 公开发行阶段

必须同时满足：

1. 底图来自自然资源主管部门标准地图、天地图授权服务或具备资质的地图编制/服务单位。
2. 数据授权明确允许 Windows 桌面应用、离线缓存、应用分发和导出图片/PDF。
3. 地图界面显著保留来源、审图号和授权要求的署名。
4. 不自行修改国界、省级行政边界或重要岛屿表示。
5. 对缩放、裁切、样式切换、热力图叠加、点标记和导出报告是否构成需重新审核的编辑进行正式确认。
6. 如果需要送审，只有取得批准后才能移除“内部测试版”标记。
7. 浅色/深色地图样式分别在授权和审核范围内；不能靠程序反色生成未经确认的新地图样式。

自然资源部标准地图服务说明指出：直接使用标准地图需标注审图号，对内容编辑，包括放大、缩小和裁切，公开使用前需要送审。因此静态标准地图不能直接被假定为可任意缩放的交互离线底图。

## 6. 地图提供者接口

```text
CompliantBasemapProvider
├─ provider_id
├─ dataset_version
├─ review_number
├─ attribution_text
├─ allowed_offline
├─ allowed_export
├─ allowed_styles
├─ coverage_polygon
├─ tile_manifest
└─ integrity_manifest
```

生产构建在以下任一条件不满足时拒绝启动地图：

- `review_number` 为空；
- 离线授权未确认；
- 导出授权未确认；
- 缓存清单签名/校验失败；
- 数据版本不在发布白名单。

## 7. 缓存与数据最小化

- 所有持久数据总量硬上限 2,500,000,000 字节。
- 预算建议：底图 500 MB、DEM 1.55 GB、水体 150 MB、索引与计算缓存 200 MB、下载安全余量 100 MB。
- 实际分配由下载清单决定，但总量不可突破。
- 临时下载文件也计入上限。
- 缓存页只显示区域覆盖和大小，不显示 DEM 高程预览。
- 删除数据前明确告知哪些离线区域将失效。

## 8. 署名与导出

应用地图界面和导出文件均保留：

- 底图来源与审图号。
- DEM 数据来源和版本。
- 水体数据来源和版本。
- ITM 模型名称和版本。
- 软件版本和生成时间。

不得以裁切、主题、紧凑布局或导出模板为由隐藏必要署名。

当前内部 Alpha 只输出无行政边界、无底图的局部等距诊断报告，并强制显示“内部测试，不得公开发布”、ITM/数据版本和模型限制。这不构成公开地图导出授权，也不允许把诊断报告用于宣传或正式发行。

正式 provider 必须在发行清单中同时给出非空审图号、署名、离线授权和导出授权；任一字段缺失时生产构建必须拒绝带底图地图导出，不能仅显示警告后继续。

Copernicus 数据署名以发布时适用的许可文本为准。当前工程记录的基础措辞为：`produced using Copernicus WorldDEM-90 © DLR e.V. 2010-2014 and © Airbus Defence and Space GmbH 2014-2018 provided under COPERNICUS by the European Union and ESA; all rights reserved`；若发布布尔 WBM 或其他派生物，还需明确标注应用进行了改编。

## 9. 发布检查清单

- [ ] 底图供应者身份、资质与授权文件已归档。
- [ ] 有效审图号已写入发行清单。
- [ ] 浅色、深色样式均在授权/审核范围内。
- [ ] 离线缓存与重新分发权已书面确认。
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
