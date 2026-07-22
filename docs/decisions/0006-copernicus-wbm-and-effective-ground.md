# ADR-0006：Copernicus WBM 与陆水等效地表参数

- 状态：Accepted
- 日期：2026-07-16

## 背景

ITM 点对点调用对整条路径接收一组相对介电常数 `epsilon` 和电导率 `sigma`，而产品要求海洋、湖泊和河流只作为同一个 `water` 类别，同时必须让水面与陆地使用不同电气参数。数据还要与约 90 m DEM 剖面对齐、支持匿名按区域获取、离线运行并受整个应用十进制 2.5 GB 硬配额约束。

Copernicus DEM GLO-90 产品随 DEM 提供对齐的 8-bit Water Body Mask（WBM）。产品手册定义 `0 = no water`、`1 = ocean`、`2 = lake`、`3 = river`。AWS Open Data 镜像可匿名获取同版本 DEM/WBM，但 GLO-90 只覆盖全球陆地，纯海洋地理单元可能没有任何对象。

## 决策

1. 数据集固定为 `COP-DEM GLO-90 2021_1 / AWS COG`；每个规划地理单元同时登记一个 DEM 资产和一个 WBM 资产。
2. WBM 通过包含采样坐标的分类像素读取，不对类别做双线性插值。源值 `0` 映射为 `land`，`1/2/3` 全部映射为 `water`；其他值作为完整性错误。
3. 同一固定地理单元的 DEM 和 WBM 只有在两个官方 URL 都明确返回 `404` 时才分类为纯海洋。此时生成确定性的 1200×1200、WGS-84、Deflate GeoTIFF：DEM 全为 `0 m`，WBM 全为源类别 `1`。单边 `404`、超时、权限错误和服务器错误都阻断准备流程。
4. 生成资产和网络资产使用同一 `.partial`、`sync_all`、同目录原子提交、SHA-256、SQLite ready 状态和配额规则；生成编码版本固定为 `generated:uniform-ocean-v1`。
5. 模型默认版本固定为 `land-water-v1`：
   - `climate = 5`、`N_0 = 301`、`mdvar = 12`、时间/位置/情景为 50/50/50；
   - 陆地 `epsilon = 15`、`sigma = 0.005 S/m`；
   - 统一水体 `epsilon = 81`、`sigma = 0.010 S/m`。
6. 对每条约 90 m 采样的剖面计算水体样本比例 `f`，再使用线性等效值：

   ```text
   epsilon = 15 + (81 - 15) × f
   sigma   = 0.005 + (0.010 - 0.005) × f
   ```

   `f = 0` 和 `f = 1` 必须精确等于两端参数。
7. NTIA 表中的平均海水电导率为 `5.0 S/m`，但产品明确要求所有水体只使用一组参数。首版选择淡水型 `81 / 0.010 S/m` 作为统一且偏保守的默认值，避免把河流和湖泊按高导电海水处理。该取舍写入模型版本，不静默更改。
8. 不从 WBM 推导可见海岸线、国界或服务范围。WBM 只进入隐藏传播计算；地图显示继续由独立的合规底图提供者负责。

## 依据

- Copernicus DEM Product Handbook：WBM 数据类型、像素编码和与 DEM 对齐关系。
- AWS Registry of Open Data：GLO-90 全球陆地覆盖、海洋区域无瓦片且高程可视为零、匿名对象布局和许可入口。
- NTIA Report 82-100, *A Guide to the Use of the ITS Irregular Terrain Model in the Area Prediction Mode*，Table 3：平均陆地、淡水和海水的 `epsilon/sigma` 工程值。
- NTIA ITM v1.4 官方实现：`epsilon` 与 `sigma` 是点对点模型的公共输入。

## 结果

- DEM 与 WBM 同网格、同版本，避免额外矢量栅格化、海岸线错位和大型运行时依赖。
- WBM 通常远小于 DEM；成都 25 张 WBM 只增加 833,007 bytes。
- 纯海洋生成补齐沿海 200 km 完整圆，同时成对 `404` 约束避免把偶发缺失静默变成海洋。
- 线性混合可解释、端点可测试，但不是分段地表电磁传播的严格物理解；输出仍属于规划预测，不等同外场测量。
- 单一淡水型水体值会低估真实海水高导电性的某些影响。未来只有在获得足够实测基准并发布新模型版本、回归报告和迁移说明后才能调整。
- WBM 的布尔折叠与纯海洋生成属于派生处理；发行版必须保留 Copernicus 来源、版本和改编声明。

## 官方来源

- https://dataspace.copernicus.eu/sites/default/files/media/files/2024-06/geo1988-copernicusdem-spe-002_producthandbook_i5.0.pdf
- https://registry.opendata.aws/copernicus-dem/
- https://its.ntia.gov/publications/download/82-100_ocr.pdf
- https://github.com/NTIA/itm
