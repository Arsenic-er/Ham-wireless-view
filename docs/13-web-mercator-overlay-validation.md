# Web Mercator 地图覆盖层验证

- 日期：2026-07-24
- 工作区：`gpu-273312`（`ubuntu@150.65.181.202`）的 `/home/ubuntu/hamheatmap`
- 状态：已验收（地图覆盖层几何与自动化回归）
- 对应决策：`decisions/0011-web-mercator-map-overlay.md`

## 1. 验证目标

本验证只回答一个工程问题：固定 1 km 的局部等距传播栅格进入 MapLibre 后，热力图的地图位置是否仍满足小于 1 km 的显示几何误差门槛。

验证不评估 ITM 的外场精度，不改变原始 dBm 数值，也不构成中国大陆底图、审图号、离线授权或正式导出的合规验收。

## 2. 旧四角方案的复核方法

旧方案把原始 `401×401` PNG 的四个 WGS-84 对角点直接作为 MapLibre `image` source 坐标。复核程序重现 MapLibre 的关键几何过程：

1. 从代表性发射点生成局部等距 `401×401` 栅格的真实 WGS-84 样本位置。
2. 把四个图像角点转换为 Web Mercator。
3. 按 MapLibre 图像四顶点、两个三角形的对角线划分，在三角形内部做仿射插值。
4. 将插值所得地图位置与同一原始样本的真实 Web Mercator 位置比较，统计最大、P95 和 RMS 平面误差。

代表性纬度沿用同一经度和固定 200 km 半径，以隔离纬度引起的投影差异。复核结果：

| 中心纬度 | 最大误差 | P95 | RMS |
| ---: | ---: | ---: | ---: |
| 18.0° | 2.035 km | 1.734 km | 1.043 km |
| 25.0° | 2.919 km | — | — |
| 30.5° | 3.685 km | — | — |
| 35.0° | 4.378 km | — | — |
| 40.0° | 5.244 km | — | — |
| 45.0° | 6.246 km | — | — |
| 50.0° | 7.439 km | — | — |
| 54.0° | 8.587 km | — | — |

最大误差通常接近图像中心，而不是只出现在外缘。这说明问题来自非线性投影被两个仿射三角形近似，不是简单的角点顺序或半像素偏移。旧方案在全部代表性纬度上均超过 1 km 门槛，不能进入发布版。

## 3. 新覆盖层方法

传播计算先产生规范的 `401×401` dBm 栅格和原始 PNG。地图显示在 Rust 侧额外生成一张 `401×401`、轴对齐 EPSG:3857 覆盖层：

1. 求覆盖原始样本域的 Web Mercator 轴对齐边界。
2. 以输出栅格的 Web Mercator 像素间距向四侧各扩展半个像素，使首末像素中心与原始边界采样语义对齐。
3. 遍历每个目标像素中心，执行 EPSG:3857 → WGS-84 反算。
4. 对发射点与目标点做测地反算，换算为局部等距 `x/y` 和原始栅格小数索引。
5. 对有限 dBm 邻域执行 NaN 感知双线性重采样；有限权重重新归一化。
6. 距离大于 200 km、原始域外、没有有限贡献或低于固定透明阈值的像素写为透明。
7. 服务契约把该 PNG、投影、宽高和四角作为独立地图字段返回；原始 PNG 仍供内部报告使用。

MapLibre 对轴对齐 Web Mercator 矩形的两个三角形插值与目标栅格本身一致，因此浏览器不再承担局部等距到 Web Mercator 的非线性变换。

## 4. 自动化验收指标

必须全部满足：

- 地图覆盖层和原始报告 PNG 均为 `401×401`，相同输入生成确定性字节输出。
- 四角在 EPSG:3857 中满足同一北边界、南边界、东边界和西边界，构成轴对齐矩形。
- 纬度 18°、30.5°、40°、54° 的代表性测试中，有效地图像素中心的定位误差 `< 1.0 km`。
- 发射点中心与四个 199 km 内侧方向的 alpha 可见；精确 200 km 连续边界点位于半像素扩展图像域内。若最近栅格中心透明，`3×3` 邻域必须存在误差不超过该处 WGS-84 实算一个输出像素对角线的可见中心。
- 圆外像素透明；原始 NaN 和无有限邻域透明；部分有限邻域按有效权重重新归一化。
- 固定 dBm 色标阈值和透明规则保持稳定。
- 合成东西/南北 dBm 梯度经重采样后保持正确方向。
- 真实 `serde_json` 结果包含 14 个 camelCase 字段；前端 `buildMapOverlayImageSpec` 使用地图覆盖层 URL/四角并与原始报告 PNG/四角分离。
- Rust 格式、Clippy、工作区测试、TypeScript 检查、前端测试和前端生产构建全部通过。

## 5. 实测结果

代表性纬度的最大样本中心定位误差：

| 中心纬度 | 新覆盖层最大误差 | 判定 |
| ---: | ---: | --- |
| 18.0° | 711.655 m | 通过 |
| 30.5° | 716.127 m | 通过 |
| 40.0° | 725.742 m | 通过 |
| 54.0° | 739.625 m | 通过 |

全组最大值为 `739.625 m`，低于 `1,000 m` 门槛 `260.375 m`。四个代表性纬度全部通过。

覆盖层专项自动化证据：

- `map_overlay_geometry_stays_within_one_kilometre_across_china_latitudes` 输出并断言上述四个最大误差。
- `map_overlay_corners_are_an_axis_aligned_mercator_rectangle` 验证 EPSG:3857 轴对齐四角、半像素中心语义和发射点中心定位。
- `map_overlay_pixel_sampling_matches_absolute_affine_field_and_image_uv` 以绝对仿射 dBm 场验证反向采样误差 `< 2e-5 dB`，并验证 MapLibre 图像 UV 像素中心与 Web Mercator 样本中心偏差 `< 1e-6 m`。
- `map_overlay_keeps_circle_transparency_and_cardinal_interior_visible` 验证四角透明、中心及东/南/西/北四个 199 km 内侧方向可见。
- `exact_cardinal_200_km_points_fit_bounds_with_one_ground_pixel_raster_tolerance` 验证四个代表性纬度上精确 200 km 的连续东/南/西/北边界点均位于半像素扩展图像域内。部分边界点的最近像素中心透明，但 `3×3` 邻域内最近可见中心始终处于一个该地 WGS-84 实算输出像素对角线内；最差为纬度 18°向南 `1012.102 m < 1431.578 m`。
- `map_overlay_render_preserves_synthetic_east_and_north_gradient_directions` 验证合成东西与南北 dBm 梯度经覆盖层重采样后方向不反转。
- `map_overlay_bilinear_sampling_renormalizes_around_nan` 验证有限邻域权重重新归一化以及无有限邻域输出 NaN/透明。
- `fixed_color_thresholds_and_transparency_are_stable` 验证固定 dBm 色标边界和透明阈值。
- `map_overlay_png_is_deterministic_and_fixed_size` 验证覆盖层 PNG 字节确定、宽高固定 `401×401`。
- `calculation_contract_schema_includes_map_overlay` 验证桌面服务契约版本包含独立地图覆盖层。
- `calculation_result_serializes_all_overlay_fields_in_camel_case` 对真实 `serde_json` 结果断言 14 个字段及 camelCase 覆盖层键，并拒绝 snake_case 泄漏。
- `mapOverlay.test.ts` 的 `buildMapOverlayImageSpec` 测试断言 MapLibre 使用 `mapOverlayPngDataUrl`/`mapOverlayCorners` 而非 `heatmapPngDataUrl`/`imageCorners`，并在四角契约不完整时拒绝交给 MapLibre；内部报告继续使用原始 PNG。

全工作区回归结果：

- `scripts/cargo-project.sh test --workspace --all-targets --locked`：`57 passed`、`0 failed`、`3 ignored`；忽略项是显式联网 GLO-90 测试。
- `scripts/cargo-project.sh fmt --all -- --check`：通过。
- `scripts/cargo-project.sh clippy --workspace --all-targets --locked --offline -- -D warnings`：全工作区通过。
- `scripts/node-project.sh --prefix app run check`：TypeScript 检查通过。
- `scripts/node-project.sh --prefix app test -- --run`：前端 `4` 个测试文件、`13` 个测试全部通过。
- `scripts/node-project.sh --prefix app run build`：Vite production build 通过。
- `git diff --check`：通过。

## 6. 通过判定

第 4 节指标已经全部满足。四个代表性纬度的最大样本中心误差为 `711.655–739.625 m`，总体最大值 `739.625 m < 1 km`；绝对仿射 dBm/MapLibre UV、轴对齐四角、半像素边界、199 km 内侧 alpha、精确 200 km 连续边界的单像素栅格容差、NaN 透明、确定性 PNG、14 字段序列化和前端字段分离均有自动化证据。因此地图覆盖层的自动化栅格几何风险在当前 1 km 输出语义内关闭。

本次通过不覆盖以下项目：release 优化构建下的覆盖层反向重采样与第二张 PNG 编码耗时、Windows 10/11 WebView2 中 MapLibre 的实机几何回归、合规底图供应者与审图号、正式地图 PNG/PDF 导出、代码签名，以及传播结果的外场测量校准。这些项目仍保持未关闭。
