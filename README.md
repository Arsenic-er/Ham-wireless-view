# HamHeatmap

HamHeatmap（业余无线电传播热力图）是一个面向中国大陆业余无线电爱好者的开源 Windows 桌面工具。用户在地图上选择一个发射点，软件根据频率、功率、天线增益、高度、极化、地形和陆地/水面参数，生成固定半径 200 km、1 km 像素的预测接收功率热力图。

## 当前状态

需求基线已经确认，Phase 0、计算核心最小可行性验证、Phase 1 DEM/WBM 缓存闭环和陆地/统一水体传播建模已经通过。Phase 2 已完成 React/MapLibre 主界面、地图点选、参数与主题、Tauri→Rust 真实计算、区域准备与安全缓存删除、服务器 Windows 构建，以及带强制内部水印的离线 PNG/PDF 诊断报告。地图显示已改为独立的 401×401、轴对齐 EPSG:3857 反向重采样覆盖层；中国代表性纬度的最大定位误差为 739.625 m，低于 1 km 门槛，自动化验收已经通过。合规底图、正式地图导出、代码签名和 Windows 10/11 完整实机验收仍未完成。

为便于在不向 Windows 本机同步开发资源的前提下验证真实缓存和传播流程，项目新增了只监听服务器回环地址的私有浏览器验证平台。它通过 SSH 隧道访问，具有醒目的内部验证与远程处理提示，不开放公网端口，也不提供文件导出。该平台不替代 Windows/WebView2、原生保存对话框或公开地图合规验收。

2026-07-27 的恢复与取消切片补强了缓存重启整理、精确配额边界、操作取消线性化，以及私有平台管理锁和 runner 生命周期声明。基于提交和构建 revision `6d7bbc54fd477f0f4167d1044d4c9ec31eed969d`，加固版已完成真实 `stop → build → start`、重复启动、readiness 和 HTTP 取消后重算验收；十进制 2.5 GB 实体压力、整机重启、Windows 实机和地图合规仍是独立门槛。

后续私有平台协议切片为计算与下载引入服务端生成的 operation capability、精确 ID 取消、有限期状态快照和约 250 ms 的非重叠 HTTP 进度轮询，用来隔离多标签页操作并复用现有进度界面。同步长请求仍是结果的唯一权威来源，状态端点不保存 PNG 或错误详情。该协议已在 full build revision `867c25aeb2091055b56d1259f6ad7293d21f7495` 上完成代码回归、受管 `stop → build → start`、readiness 与两次真实回环烟雾；通过 SSH 隧道确认浏览器可见进度和控制台状态仍待执行，不能从受管 HTTP 结果外推。

本次切片实现了发射点地面海拔的“DEM 自动/手动覆盖”：可空请求兼容旧调用，手动范围为 `-500..9000 m AMSL`，中心 DEM 在两种模式下都必须验证，且只替换 ITM PFL 的发射端首样点；天线 AGL、其余 DEM 与 WBM 不变。计算结果独立升级为 schema 3 并冻结有效海拔及 `dem/manual` 来源，bootstrap 保持 schema 2，内部报告使用冻结结果。

同一切片把 GLO-90 下载 Agent 收紧为 HTTPS-only、零重定向和有限分阶段/总超时，并让取消、读取错误与 early EOF 都可靠同步/登记 partial 后再返回；没有后台自动重试，下一次用户操作只在强 ETag/Range 条件匹配时续传。自动化门禁已通过 Rust workspace 95 项、前端 46 项、真实 HTTPS 3/3、Clippy、类型检查、前端构建和 xwin 检查。revision `2e4411de809d1f78b6dd1407d51a2351d58b02ed` 已完成受管部署；成都 DEM 自动 `526.3442993164062 m` 与手动 `1500.0 m` 的原始/覆盖层 PNG 哈希均发生变化，schema 3 恢复烟测也通过。SSH 隧道浏览器视觉和 Windows 实机仍待验证。

首轮结果见 `docs/05-phase0-validation-report.md`，真实地形最小可行性结果见 `docs/06-minimum-viability-validation.md`，缓存闭环见 `docs/07-phase1-cache-validation.md`，陆地/水体结果见 `docs/08-land-water-validation.md`，桌面首切片见 `docs/09-phase2-desktop-slice.md`，下载与缓存交互见 `docs/10-phase2-download-cache-slice.md`。通过桌面服务契约运行成都真实缓存时，125,628 个像素约 9.75 秒完成；下载状态烟雾测试确认 50 个 DEM/WBM 资产无需重复下载并正确进入 ready。上述数字仍不是 Windows 整机验收。

2026-07-28 新增真实参数敏感性门禁：同一成都 DEM/WBM 上逐像素验证 10 倍功率约 +10 dB、收发增益精确平移，并确认 145/435 MHz、收发天线高度和水平/垂直极化都会产生非统一的空间变化。相同输入的原始栅格、报告 PNG 和 EPSG:3857 地图覆盖 PNG 哈希一致；每个参数场景的两种 PNG 均与基线不同。测试使用一致缓存快照，真实缓存的 SQLite、DEM/WBM、总字节和完整清单保持不变；这证明模型参数接线和响应，不替代外场校准。

2026-07-31 新增私有 validation 平台天地图在线同源代理、右下动态公制比例尺和 MapLibre desired-state 重放，并以 revision `6e9714c6cdcdeb54ff47e229d8d43b18bf32b3c6` 完成受管构建、启动和回环 readiness。浏览器不接触 `tk` 或上游 URL；token 由项目运行目录中的 `0600` 私密文件管理。当前未配置 token，bootstrap 正确返回 `enabled=false`，瓦片端点返回 503；尚未取得真实瓦片或完成浏览器视觉烟雾。该在线验证路径也不提供离线底图或正式地图导出，审图号、服务/离线/导出授权仍是公开发布门槛。

同日，私有 validation 平台决定以四省区域 Protomaps PMTiles 作为内部验证主底图，天地图改作联网 fallback 与历史验证。固定归档为 source build 20260731、bbox 107.5,18,125.5,33.5、z0-9、33,044,072 bytes（2.5 GB 的 1.32%），SHA-256 为 5bda49bf909a5b9fae931353edf5aea82ba35be9f8187128643b972eed4c87d0；只允许同源 HTTP Range 读取。可信样式显示 earth、landcover、landuse、water、roads、places，并持续显示 © OpenStreetMap contributors；places 按缩放级别提供省、市、县区和乡镇中文地名，不承诺村级完整性。原始归档仍含 boundaries 和 Natural Earth/OSM 内容，因此只能私有内部验证，不是中国大陆公开地图合规底图，也不得纳入正式 EXE。

2026-08-01 又加入“地图 / 卫星”切换：联网卫星背景采用 EOxCloudless Sentinel-2 Cloudless 2025，同源代理固定 HTTPS 上游并返回 no-store；本地 places 地名、传播热力图和发射点继续叠加，卫星影像不参与 DEM/WBM 或 ITM 计算。浏览器断网或卫星 source 出错会自动退回离线地图。本轮通过 frontend check、9 files/65 tests、Rust workspace 113 passed/5 ignored、validation server 28/28、Clippy、前端构建和管理 self-test；live 四省卫星瓦片返回 JPEG 与 Cache-Control: no-store。真实浏览器视觉和 Windows 实机字体可读性仍待用户验收，详情见 docs/21-protomaps-four-province-basemap.md。

> [!WARNING]
> 当前版本是未签名的内部 Alpha。传播结果是模型估算，尚未经过外场测量校准，不得作为生命安全、应急指挥或法规合规决策的唯一依据。仓库公开的是源代码；这不代表当前占位地图或导出报告已经满足中国大陆公开地图发行要求。

## MVP

- 144 MHz 与 430 MHz，具体频率可输入两位小数。
- Longley–Rice / NTIA ITM 点对点地形传播。
- 基地台→手台、手台→基地台预设。
- 水平/垂直极化。
- 发射点地面海拔默认读取 DEM，可在 `-500..9000 m AMSL` 内手动覆盖；发射天线高度仍为 AGL。
- 200 km 圆形范围、1 km 输出、dBm 固定色标。
- 地形只用于隐藏计算，不在底图显示。
- 浅色/深色 UI。
- 区域数据缓存和离线计算，所有持久数据硬上限 2.5 GB。
- 带强制水印的内部诊断 PNG/PDF；正式合规地图导出待底图授权与审图号。
- Windows 10/11 64 位。

## 文档

- `docs/01-product-requirements.md`：产品需求和验收标准。
- `docs/02-technical-design.md`：架构、计算、缓存与实施阶段。
- `docs/03-data-and-map-compliance.md`：数据来源、许可和中国大陆地图合规门槛。
- `docs/04-test-plan.md`：模型、UI、数据、性能和发布测试。
- `docs/05-phase0-validation-report.md`：ITM、GLO-90、真实路径与性能验证结果。
- `docs/06-minimum-viability-validation.md`：真实 200 km 全圆计算、确定性、性能和模型敏感性验收。
- `docs/07-phase1-cache-validation.md`：任意点瓦片规划、配额、下载、续传、迁移和缓存计算闭环。
- `docs/08-land-water-validation.md`：WBM、纯海洋瓦片、陆水参数混合及成都/青岛真实验证。
- `docs/09-phase2-desktop-slice.md`：桌面首切片、Rust IPC 契约、视觉与最小窗口验证。
- `docs/10-phase2-download-cache-slice.md`：下载确认、进度/取消、区域列表和引用安全删除验证。
- `docs/11-windows-cross-build.md`：服务器 Windows 交叉构建、静态 CRT、产物和实机门槛。
- `docs/12-phase2-export-slice.md`：内部诊断 PNG/PDF、原生保存与正式地图导出边界。
- `docs/13-web-mercator-overlay-validation.md`：MapLibre 四角映射误差、Web Mercator 覆盖层方法与验收记录。
- `docs/14-windows-cross-build-gpu273312.md`：新服务器固定工具链、Windows 产物与 PE/安装包审计。
- `docs/15-private-validation-platform.md`：SSH 隧道私有验证平台、三态前端、运行边界与验证记录。
- `docs/16-recovery-and-cancellation-validation.md`：缓存重启恢复、配额边界、取消线性化和私有平台管理脚本加固记录。
- `docs/17-manual-elevation-and-download-transport-validation.md`：手动发射点地面海拔、有界下载、schema 3 与受管真实计算证据。
- `docs/18-progressive-coverage-preview-validation.md`：渐进式覆盖预览、双传输契约、真实成都运行与 Windows 待验边界。
- `docs/19-parameter-sensitivity-validation.md`：真实成都逐像素参数矩阵、双 PNG 确定性和缓存快照不变性证据。
- `docs/20-tianditu-basemap-proxy.md`：天地图在线同源代理、token 边界、动态比例尺、清空重放与未验证门槛。
- `docs/decisions/`：带证据的工程决策记录。
- docs/21-protomaps-four-province-basemap.md：四省 PMTiles 资产、Range、图层白名单、许可边界与待验证矩阵。

## 开发与构建

当前唯一规范开发工作区是 `gpu-273312`（`ubuntu@150.65.181.202`）上的 `/home/ubuntu/hamheatmap`；源码、依赖、缓存、构建与验证产物均保留在服务器，Windows 本机只接收最终可执行程序。

克隆公开仓库：

```bash
git clone https://github.com/Arsenic-er/Ham-wireless-view.git
cd Ham-wireless-view
```

推荐把 Rust、Node、Windows SDK 与下载缓存放在项目自己的 `.tools` 下。Rust 核心检查：

```bash
scripts/cargo-project.sh test --workspace --all-targets --locked
scripts/cargo-project.sh clippy --workspace --all-targets --locked -- -D warnings
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- single-path --terrain ridge --frequency 145
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- benchmark --threads 4 --terrain flat --frequency 145
```

十进制 2.5 GB 硬上限与崩溃恢复压力测试不会包含在普通测试中。它要求服务器至少有 4 GB 可用空间，会在项目的 `.runtime/cache-stress/` 顺序写入真实非稀疏数据、强制子进程退出，并在结束时删除专用运行目录：

```bash
scripts/cache-durability-stress.sh
```

脚本拒绝使用非空的压力测试目录，不会读取、修改或清理 `.runtime/validation-platform/data/` 的真实验证缓存。

真实参数敏感性矩阵保持显式 opt-in；它在短暂独占真实缓存时复制一致快照，随后只对快照运行，逐像素验证功率、增益、频率、高度和极化：

```bash
scripts/parameter-sensitivity-smoke.sh
```

真实 DEM 样本不提交到 Git。可重复下载并验证后运行：

```bash
scripts/fetch-glo90-sample.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- inspect-dem
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- dem-path --frequency 145
```

真实 200 km 验证会准备成都周边的 GLO-90 DEM 与 WBM，并生成五张诊断热力图：

```bash
scripts/fetch-glo90-chengdu-region.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- validate --threads 4
```

诊断输出位于 `reports/mvp/`，不含底图、边界或审图信息，不用于公开地图发布。

通用缓存命令会先显示预计下载量；只有显式增加 `--yes` 才开始下载：

```bash
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache plan --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5 --yes
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache status
```

桌面前端使用项目内固定的 Node.js 24.18.0。完成一次项目内工具安装后，可运行：

```bash
scripts/install-node-project.sh
scripts/node-project.sh install --prefix app
scripts/node-project.sh --prefix app run check
scripts/node-project.sh --prefix app test
scripts/node-project.sh --prefix app run build
scripts/node-project.sh --prefix app run dev
```

## 私有服务器验证平台

验证平台只供项目内部开发使用。它把 validation 构建的 React 前端与复用 `hamheatmap-app-service` 的 Linux HTTP 桥接器放在同一个源站，允许检查真实数据准备、缓存管理和传播计算。服务器进程固定监听 `127.0.0.1:1421`；不要改为 `0.0.0.0`，不要占用 Cockpit 的 `9090`，也不要在云防火墙中开放新的公网端口。

在服务器构建并启动：

```bash
cd /home/ubuntu/hamheatmap
scripts/validation-platform.sh build
scripts/validation-platform.sh start
scripts/validation-platform.sh status
scripts/validation-platform.sh health
scripts/validation-platform.sh self-test
```

在 Windows PowerShell 建立 SSH 隧道，并保持该终端窗口运行：

```powershell
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 1421:127.0.0.1:1421 ubuntu@150.65.181.202
```

然后在本机浏览器打开 `http://127.0.0.1:1421`。该地址是 SSH 隧道的本机入口，不是服务器公网端口。验证结束后在服务器停止：

validation 浏览器在开始计算、下载估算或下载前，先向服务器领取不可猜测的短期 operation ID。长请求携带该 ID；浏览器以同一 ID 轮询状态并只可取消自己的计算或下载。状态轮询是 `POST` JSON，不提供“当前任务”或任务列表，也不返回热力图、URL、服务器路径或详细错误：

```text
operation-ticket → long request + operation-status polling
                 → exact-ID cancel（可选）
                 → long response is authoritative → operation-ack
```

```bash
scripts/validation-platform.sh stop
```

三种前端模式具有不同权限：

| 模式 | 数据准备/缓存/计算 | 文件导出 | 用途 |
|---|---:|---:|---|
| Windows Tauri | 是 | 是 | 最终桌面行为 |
| 私有 validation server | 是 | 否 | 服务器真实核心验证 |
| 普通浏览器 preview | 否 | 否 | 纯界面与视觉检查 |

验证模式会把坐标、无线电参数和计算请求发送到用户控制的 JAIST 服务器，因此它是桌面“坐标和结果只留本机”原则的明确内部测试例外。只使用测试坐标，不要输入不愿离开本机的敏感位置。服务器仍通过共享 `AppService` 执行固定来源、2.5 GB 配额、DEM/WBM 完整性和 ITM 规则；浏览器没有服务器文件路径权限，服务端也没有导出端点。

所有运行数据、PID、日志和构建元数据都在项目的 `.runtime/validation-platform/`，不会写入 Windows 本机，也不使用 Docker、系统服务或系统级运行目录。`start` 可安全重复执行并在 SSH 断开后继续运行，但服务器整机重启后仍需要手动执行；`self-test` 检查脚本不变量。适配新协议后的 `validation-recovery-smoke.sh` 已连续两次验证 ticket 消费、未知 ID/错 family 不可取消、busy 不消费 reserved ticket、状态进度、终态确认、无半结果、同票重算恢复、唯一可解码 `401×401` 双 PNG，以及最终 gate/health 与临时目录清理；浏览器可见进度仍单列待验。日志、进程控制与运行证据见 `docs/15-private-validation-platform.md` 和 `docs/16-recovery-and-cancellation-validation.md`。

`validation-progressive-preview-smoke.sh` 已在同一成都真实缓存上连续两次取得 2 张不同的部分覆盖层，并验证最终 schema 3 双 PNG、终态无 PNG、ack 和缓存不变性。预览不可导出，最终结果是唯一权威响应；Windows WebView2↔Rust Channel 仍需实机验收。完整证据见 `docs/18-progressive-coverage-preview-validation.md`。

Tauri 壳层位于 `app/src-tauri/`。JAIST Linux 负责前端、共享 Rust 服务、浏览器视觉回归和内部 Windows 交叉构建；正式发布仍必须在 Windows 10/11 验证 WebView2、安装包和文件系统行为。

新服务器首次使用或项目内工具缓存丢失时，先恢复固定版本的 Windows 交叉工具链，再生成 Windows 单文件 EXE 与内嵌 WebView2 的离线安装包：

```bash
scripts/install-windows-cross-tools.sh
scripts/tauri-windows-cross.sh
```

恢复脚本只写入服务器项目的 `.tools/`，不使用系统级安装，也不会向 Windows 桌面下载源码、SDK 或构建缓存。LLVM 完整工具包、归档和 xwin SDK 合计约占 14 GB；Windows release target 与 WebView2 离线安装器还会额外占用空间。

该交叉构建只用于内部 Alpha；它不替代 Windows 10/11 实机、代码签名和地图合规验收。详细记录见 `docs/11-windows-cross-build.md`。

## 重要限制

该软件是规划与教学工具，不保证实际通联。MVP 不考虑建筑、植被、城市杂波、外部干扰、实时天气、异常传播、水面反射或馈线损耗。

面向中国大陆公开发行前，必须完成合规底图授权、审核和审图号检查。开发底图或国际开源边界不能直接进入正式发行版。

## 技术栈

桌面端使用 Tauri 2.11.5、React 19.2.7、TypeScript 7.0.2、Vite 8.1.4 和 MapLibre GL JS 5.24.0。后端使用 Rust、内嵌 SQLite、NTIA 官方 ITM C++ v1.4、纯 Rust `tiff` 和 rustls HTTPS。

区域底图验证客户端新增 PMTiles JavaScript 4.4.1（BSD-3-Clause）及其传递依赖 fflate 0.8.3（MIT）。

## 许可证

项目源代码采用 Apache License 2.0。地图、DEM、水体和第三方依赖分别遵守其自身许可与署名要求。
