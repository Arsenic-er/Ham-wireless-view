# HamHeatmap Phase 1 DEM 缓存验证报告

- 日期：2026-07-16
- 状态：本报告记录 DEM 缓存闭环；后续 WBM/陆水传播已在 `08-land-water-validation.md` 通过，桌面 UI 和公开发行仍未完成
- 主机：JAIST `gpu-753856`，Ubuntu 22.04.3 LTS，x86_64

## 1. 已关闭的风险

本轮把成都专用脚本推进为任意 WGS84 发射点可复用的后端流程：

```text
发射点 → 200 km WGS84 外接范围 → GLO-90 瓦片计划
       → SQLite 区域/瓦片索引 → 预计下载量 → 用户确认
       → partial/Range 下载 → 长度与 SHA-256 → 原子 ready
       → 只加载本区域瓦片 → ITM 覆盖计算
```

缓存根覆盖应用的全部持久数据，硬上限固定为十进制 2,500,000,000 bytes。实际目录扫描是最终依据，因此 SQLite、自身锁文件、未登记文件和 partial 均不能绕过配额。

## 2. 任意点规划

以 `(30.5°N, 103.5°E)` 为中心，WGS84 椭球上每 0.5° 方位采样 200 km 周界，再增加一个 GLO-90 像素的双线性插值余量：

- 范围：south 28.694857、west 101.415693、north 32.304643、east 105.584307。
- 瓦片：`N28..N32 / E101..E105`，共 25 个，与手工验证区域完全一致。
- 区域 ID：`glo90-2021_1-r200-lat+30500000-lon+103500000`。

规划器不携带、绘制或推断政治边界。服务范围选择将来由合规底图和产品层控制；数据层只处理合法 WGS84 坐标和反经线/极区安全条件。

## 3. 下载与完整性

真实 AWS HEAD 探测得到 25 个对象合计 132,164,681 bytes；没有 `--yes` 时只显示预计量，不开始下载。

两项显式网络测试均通过：

1. 下载 `N30E103` 真实瓦片，验证固定 HTTPS 白名单、Content-Length、流式写入、同目录原子改名、SQLite ready、SHA-256 和 GeoTIFF 解码。
2. 首个 128 KiB 块写入后主动取消，确认 partial 被计入配额；第二次请求使用 HTTP Range 续传，完成后 partial 归零并通过同样校验。

自动化测试还确认相似域名、URL 用户信息、查询参数和片段会被拒绝。

当前 AWS 对象没有经认证的逐瓦片 SHA-256 响应。首次获取依赖 TLS、固定主机和长度，随后保存本地 SHA-256 以发现磁盘损坏；正式发行必须补充签名的固定版本清单，不能把本地首次哈希描述为来源认证。

## 4. 配额与索引

25 个历史瓦片先整体解码，再逐文件同卷原子移动到 `data/dem/2021_1-aws-cog/`，最后写入 SQLite 并重新逐文件校验。迁移后：

| 项目 | 字节/数量 |
|---|---:|
| 数据根实际总量 | 132,205,641 bytes |
| 已登记 DEM | 132,164,681 bytes |
| SQLite、锁和其他元数据 | 40,960 bytes |
| 可用配额 | 2,367,794,359 bytes |
| ready / partial / corrupt | 25 / 0 / 0 |

配额预检为 SQLite 和事务保留最多 16 MB 安全余量，并独立检查文件系统可用空间。共享瓦片通过 `region_assets` 多对多关系保留；删除一个区域只删除没有其他区域引用的瓦片。活动计算区域拒绝删除，不进行静默 LRU 淘汰。

## 5. 缓存到计算的闭环

默认验证命令不再扫描历史目录，而是：规划区域、打开缓存、复核 25 个文件大小和 SHA-256、只解码规划中的瓦片、标记区域活动、完成计算后释放。

四线程 release 结果：

- 缓存校验和 25 瓦片加载：3.902 s。
- 真实 145 MHz/20 m：8.781 s，其中传播 8.636 s。
- 平地对照：1.990 s，其中传播 1.883 s。
- 真实 435 MHz/20 m：8.753 s，其中传播 8.637 s。
- 真实 145 MHz/80 m：8.475 s，其中传播 8.334 s。
- 完整墙钟：31.91 s。
- 峰值 RSS：157,920 KiB。

四张 PNG 的 SHA-256 与缓存实现前完全一致，证明数据迁移、按计划加载和完整性检查没有改变传播栅格：

- `faa346bf40719914c369e48a65a522e8e8bf7e888baf63a247c15e9223a325b8`
- `ed217a5578648dc5e9eaa021cb976a1cbd11c86a290afb49e8c029afa3e9a234`
- `0438923605c69c1c8972a7fd78699cf0183f381de5e8e59625235352f19879b9`
- `0801cad76b55dcffe34b4c48c1e869e839a80a97aa2c611243f589c98a49e39f`

## 6. 测试与命令

缓存 crate 有 9 个默认自动化测试和 2 个显式网络测试。全工作区原有模型、地形和覆盖测试保持通过。

```bash
scripts/cargo-project.sh test --workspace --all-targets --locked
scripts/cargo-project.sh test -p hamheatmap-cache --test live_glo90 -- --ignored
scripts/cargo-project.sh clippy --workspace --all-targets --locked -- -D warnings

scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache plan --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache status
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- validate --threads 4
```

## 7. 尚未关闭

- Windows 10/11 的文件锁、SQLite、原子改名和 Range 续传实机回归。
- UI 中的下载确认、进度、取消、区域列表和删除交互。
- 2.5 GB 边界附近的完整压力与断电恢复测试。
- 经复核并签名的 DEM 大小/SHA-256 发布清单。
- 合规底图、PNG/PDF 公开导出和审图要求。

水体数据、纯海洋单元和传播参数契约已由后续 ADR-0006 与 `08-land-water-validation.md` 关闭。因此下一工程阶段可以开始桌面 UI 骨架，同时继续保留 Windows 缓存实机回归、2.5 GB 压力和签名数据清单作为发布前阻断项。
