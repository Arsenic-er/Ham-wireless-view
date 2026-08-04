# HamHeatmap

[English](README.md) | [简体中文](README.zh-Hans.md) | [繁體中文](README.zh-Hant.md) | **日本語**

<!-- locale: ja -->
<!-- canonical: README.md -->

HamHeatmap（アマチュア無線伝搬ヒートマップ）は、中国本土のアマチュア無線家向けに開発しているオープンソースの Windows デスクトップアプリです。地図上で送信地点を 1 か所選択すると、周波数、出力、アンテナ利得、高さ、偏波、地形、陸地／水面のパラメーターを用いて、固定半径 200 km、1 km グリッドの予測受信電力ヒートマップを生成します。

英語版 [`README.md`](README.md) が正規の紹介ページです。このページは完全な日本語版です。

<!-- section:current-status -->
<!-- synchronized-tests: frontend-files=17 frontend=154 rust=133 ignored=5 -->
## 現在の状況

### オンライン検証版

- 非公開の validation サービスはサーバー上で稼働し、`127.0.0.1:1421` のみをリッスンします。アクセスには SSH トンネルが必要です。
- 現在のソースは、validation に有効な token がある場合は同一オリジンの Tianditu 通常地図を使い、token がない場合は CARTO Voyager base + labels を自動選択します。衛星モードは EOxCloudless と現在のオンライン地名レイヤー（token なしでは CARTO labels）を使います。通常地図、地名、衛星画像の障害は相互に分離され、影響を受けていないレイヤーは利用を継続できます。利用可能な 2 つの表示ベースが両方失われた場合のみ WGS84 座標グリッドへ戻ります。管理下のループバックサービスは clean commit d7bd31a から再ビルド済みで、health schema 1、CARTO base/labels、EOx 衛星、旧 Tianditu の拒否、クエリ注入の拒否、no-store の確認に合格しています。ブラウザー表示は未検証です。
- -140..-60 dBm、1 dB 刻みの全体表示しきい値スライダーを実装済みです。弱いピクセルを動的に非表示にしますが、伝搬計算の再実行や統計、本回のエクスポート内容の変更は行いません。現在、フロントエンド 17 files / 154 tests、Rust workspace 133 passed / 5 ignored と、実キャッシュを使う 3 組の HTTP スモークテストに合格しています。8 レイヤー時のサーバー CPU マイクロベンチマークは P95 5.982 ms です。Codex Windows ACL 障害のため管理ブラウザーでのドラッグ操作は未検証で、Windows WebView2 実機試験も未完了です。
- 旧 4 省 PMTiles ルートは製品対象から外れ、過去の技術証拠としてのみ残しています。EOxCloudless は validation と現在の Windows ソースで使うオンライン衛星レイヤーであり、その画像を Windows リリース資産へ同梱しません。

- 現在のソースには、1–200 km の経路を対象とする独立したオンライン 2 地点 TX/RX リンク解析モードを実装済みです。計算完了後、非モーダルの浮動地形／フレネルプロファイルが自動で開きます。外側をクリックすると 42% の透明度になり地図操作は継続でき、プロファイルをクリックすると復帰し、閉じるボタンまたは Escape で閉じ、完了済み結果から再度開けます。管理下のプロセスにはこのソースが含まれていますが、実リンク解析、プロファイルダイアログ、ブラウザー操作は未検証です。公開サービスは展開していません。 カバレッジ TX とリンク TX/RX の初期地点選択ガイドは地図上部のコンパクトなステータスバーになり、地図クリックを妨げません。

### Windows Alpha

- 現在の Windows/Tauri ソースは、Tianditu `tk` なしで CARTO Voyager のオンライン地図・地名と EOxCloudless Sentinel-2 衛星画像を既定表示します。個人用 Tianditu `tk` は任意の上書き設定で、設定時は現在のユーザーの DPAPI で暗号化します。消去後は公共ベースマップを引き続き使用します。オンラインタイルは 2.5 GB キャッシュにも診断エクスポートにも入りません。Alpha 2 で導入した Tianditu 接続テストも維持します。
- v0.1.0-alpha.2 はコミット 9b0fb79 からクロスビルドして GitHub Releases に公開済みです。単体 EXE は 16,174,080 bytes、オフライン WebView2 コンポーネントを内蔵する NSIS インストーラーは 217,265,419 bytes です。
- Release には `SHA256SUMS.txt` も含まれます。2 つの Windows 成果物はいずれも未署名で、Windows 10/11 実機、SmartScreen、インストール／アンインストール、中国本土の実ネットワークでの検証は未完了です。
- Windows 製品はオンライン表示用ベースマップのみを使用し、オフライン地図パッケージの計画・配布は行いません。オンラインタイルは永続化しません。DEM/WBM、partial ファイル、インデックス、計算キャッシュには変更不可の 10 進 2.5 GB 上限が適用されます。キャッシュ済み地域ではオフラインでも計算を続行し、WGS84 座標グリッド上に結果を表示できます。

- このリンク解析ソースは v0.1.0-alpha.2 より新しく、新しい Windows リリースにはまだパッケージ化していません。

<!-- section:windows-download -->
## Windows 版のダウンロード

[v0.1.0-alpha.2 リリースページ](https://github.com/Arsenic-er/Ham-wireless-view/releases/tag/v0.1.0-alpha.2)からダウンロードしてください。

- `HamHeatmap.exe`：単体アプリ。対象 PC に WebView2 Runtime がインストール済みである必要があります。
- `HamHeatmap_*_x64-setup.exe`：ユーザー単位のインストーラー。オフライン WebView2 コンポーネントを内蔵するため、ファイルサイズが大きくなります。
- `SHA256SUMS.txt`：ダウンロードファイルの完全性を検証するチェックサムです。

SHA-256：`HamHeatmap.exe` は a1968a48bca419d58680adca31759284f7971d36c590503451212114c3808247、インストーラーは 4df826b0eb96cd5a69f3c6a3a6d2b9d248c067fe60be34bb9bcd2e7bbe0fbc0e です。

> [!WARNING]
> 現在の版は未署名の内部 Alpha です。伝搬結果はモデルによる推定値であり、フィールド測定による校正は未実施です。人命安全、緊急指揮、法規適合判断の唯一の根拠として使用しないでください。ソースリポジトリの公開は、現在のオンラインベースマップ統合やエクスポートレポートが中国本土の公開地図配布要件を満たすことを意味しません。

<!-- section:mvp -->
## MVP

- 独立したオンライン 2 地点 TX/RX リンク解析。1–200 km の経路に対応し、実 DEM/WBM と WGS84 上で ≤ 90 m 間隔のサンプルを使用。
- k = 4/3 の有効地球曲率、完全な第 1 フレネルゾーン（F1）、60% F1 クリアランス境界を描画し、NTIA ITM 損失と組み合わせます。
- TX/RX のアンテナ高、利得、水平／垂直偏波を個別に編集可能。RX の計画しきい値も編集でき、既定値は -120 dBm です。直交偏波には明示的でバージョン管理された 20 dB の計画上の仮定を適用します。
- responsive SVG の地形プロファイルに、動的な距離目盛、見通し直線、完全な F1 包絡、クリアランス境界を表示。
- 安定した結果コード `direct-los`、`obstructed-usable`、`predicted-unavailable` は、現在の入力、DEM、標準大気、しきい値に基づく計画予測であり、現地交信を保証しません。リンク解析をクリアしてもカバレッジヒートマップは消去しません。
- 144 MHz と 430 MHz 帯に対応し、周波数は小数点以下 2 桁まで入力可能。
- Longley–Rice / NTIA ITM の地点間地形伝搬。
- 基地局→ハンディ機、ハンディ機→基地局のプリセット。
- 水平偏波／垂直偏波。
- 送信地点の地表標高は既定で DEM から取得し、`-500..9000 m AMSL` の範囲で手動上書き可能。送信アンテナ高は AGL のままです。
- 固定 200 km の円形半径、1 km 出力グリッド、固定 dBm カラースケール。
- 地形は非表示の計算にのみ使用し、ベースマップには描画しません。
- ライト／ダーク UI。
- Windows デスクトップは `tk` なしで CARTO Voyager 地図／地名と EOxCloudless 衛星画像を既定表示します。有効な個人用 Tianditu `tk` は既定を任意に上書きし、DPAPI で暗号化します。タイルは永続化せずエクスポートにも含めません。
- 地域データのキャッシュとオフライン計算。すべての永続データに 2.5 GB のハード上限を適用。
- 強制透かし入りの内部診断用 PNG/PDF。正式な適合地図エクスポートには、ベースマップの許諾と審図番号が必要。
- Windows 10/11 64-bit。

<!-- section:documentation -->
## ドキュメント

技術文書の大部分は現在、簡体字中国語で記述されています。そこに記録されたコマンド、パス、検証証拠が引き続き技術上の事実資料です。

- `docs/01-product-requirements.md`：製品要件と受け入れ基準。
- `docs/02-technical-design.md`：アーキテクチャ、計算、キャッシュ、実装フェーズ。
- `docs/03-data-and-map-compliance.md`：データソース、ライセンス、中国本土の地図コンプライアンスゲート。
- `docs/04-test-plan.md`：モデル、UI、データ、性能、リリーステスト。
- `docs/05-phase0-validation-report.md`：ITM、GLO-90、実経路、性能の検証。
- `docs/06-minimum-viability-validation.md`：実 200 km 全円計算、決定性、性能、モデル感度の受け入れ。
- `docs/07-phase1-cache-validation.md`：任意地点のタイル計画、容量、ダウンロード、再開、移行、キャッシュ計算。
- `docs/08-land-water-validation.md`：WBM、海洋のみのタイル、陸水パラメーター混合、成都／青島の検証。
- `docs/09-phase2-desktop-slice.md`：最初のデスクトップスライス、Rust IPC 契約、表示、最小ウィンドウの検証。
- `docs/10-phase2-download-cache-slice.md`：ダウンロード確認、進捗／キャンセル、地域一覧、参照安全な削除。
- `docs/11-windows-cross-build.md`：サーバー上の Windows クロスビルド、静的 CRT、成果物、実機ゲート。
- `docs/12-phase2-export-slice.md`：内部診断 PNG/PDF、ネイティブ保存、正式地図エクスポートの境界。
- `docs/13-web-mercator-overlay-validation.md`：MapLibre の四隅マッピング誤差と Web Mercator オーバーレイの受け入れ。
- `docs/14-windows-cross-build-gpu273312.md`：現サーバーの固定ツールチェーンと PE／インストーラー監査。
- `docs/15-private-validation-platform.md`：SSH トンネル検証プラットフォーム、3 つのフロントエンドモード、運用境界、証拠。
- `docs/16-recovery-and-cancellation-validation.md`：キャッシュ復旧、容量境界、キャンセルの線形化、管理スクリプトの強化。
- `docs/17-manual-elevation-and-download-transport-validation.md`：送信地点標高の手動設定、制限付きダウンロード、schema 3、管理下の実計算証拠。
- `docs/18-progressive-coverage-preview-validation.md`：段階的プレビュー、二重転送契約、成都での実行、未完了の Windows ゲート。
- `docs/19-parameter-sensitivity-validation.md`：成都のピクセル単位パラメーター行列、2 枚の PNG の決定性、不変キャッシュスナップショット。
- `docs/20-tianditu-basemap-proxy.md`：Tianditu 同一オリジンプロキシ、token 境界、動的縮尺、クリア／再生動作、未完了ゲート。
- [`docs/decisions/0024-point-to-point-link-analysis.md`](docs/decisions/0024-point-to-point-link-analysis.md)：地点間リンク解析の固定契約、数式、分類、検証境界。
- `docs/decisions/`：証拠に基づく技術判断。
- [`docs/21-protomaps-four-province-basemap.md`](docs/21-protomaps-four-province-basemap.md)：製品対象から外れた 4 省 PMTiles の過去の検証証拠。

<!-- section:development -->
## 開発とビルド

正規の開発ワークスペースは、`gpu-273312`（`ubuntu@150.65.181.202`）上の `/home/ubuntu/hamheatmap` だけです。ソース、依存関係、キャッシュ、ビルド、検証成果物はサーバーに保持し、Windows PC へ渡すのは最終実行ファイルのみです。

公開リポジトリをクローンします。

```bash
git clone https://github.com/Arsenic-er/Ham-wireless-view.git
cd Ham-wireless-view
```

Rust、Node、Windows SDK、ダウンロードキャッシュは、プロジェクト内の `.tools` に配置してください。Rust の主要ゲートは次のとおりです。

```bash
scripts/cargo-project.sh test --workspace --all-targets --locked
scripts/cargo-project.sh clippy --workspace --all-targets --locked -- -D warnings
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- single-path --terrain ridge --frequency 145
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- benchmark --threads 4 --terrain flat --frequency 145
```

10 進 2.5 GB ハード上限とクラッシュ復旧のストレステストは通常テストに含めません。サーバーに 4 GB 以上の空き容量が必要で、`.runtime/cache-stress/` に実体のある非スパースデータを順次書き込み、子プロセスを強制終了し、完了時に専用実行ディレクトリを削除します。

```bash
scripts/cache-durability-stress.sh
```

このスクリプトは空でないストレステスト用ディレクトリを拒否し、`.runtime/validation-platform/data/` の実 validation キャッシュを読み取り、変更、削除しません。

実データのパラメーター感度行列は明示的な opt-in です。実キャッシュを短時間排他使用して一貫したスナップショットを作成し、そのスナップショットだけを対象に、出力、利得、周波数、高さ、偏波をピクセル単位で検証します。

```bash
scripts/parameter-sensitivity-smoke.sh
```

実 DEM サンプルは Git にコミットしません。再現可能なサンプルをダウンロードして検証後、次を実行します。

```bash
scripts/fetch-glo90-sample.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- inspect-dem
scripts/cargo-project.sh run --release --locked -p hamheatmap-phase0 -- dem-path --frequency 145
```

実 200 km 検証は、成都周辺の GLO-90 DEM と WBM を準備し、5 枚の診断ヒートマップを生成します。

```bash
scripts/fetch-glo90-chengdu-region.sh
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- validate --threads 4
```

診断出力は `reports/mvp/` に保存されます。ベースマップ、境界、審図情報を含まず、公開地図配布には使用しません。

一般キャッシュコマンドは、まず推定ダウンロード量を表示します。`--yes` を明示的に追加した場合のみダウンロードを開始します。

```bash
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache plan --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache prepare --lat 30.5 --lon 103.5 --yes
scripts/cargo-project.sh run --release --locked -p hamheatmap-mvp -- cache status
```

デスクトップフロントエンドは、プロジェクトで固定した Node.js 24.18.0 を使用します。プロジェクト内ツールを一度インストールした後、次を実行できます。

```bash
scripts/install-node-project.sh
scripts/node-project.sh install --prefix app
scripts/node-project.sh --prefix app run check
scripts/node-project.sh --prefix app test
scripts/node-project.sh --prefix app run build
scripts/node-project.sh --prefix app run dev
```

<!-- section:validation-platform -->
## 非公開サーバー検証プラットフォーム

上記のソースレベルのリンク解析変更は、管理下の validation プロセスへまだ再ビルドされておらず、そのプロセスも再起動していません。この節のコマンドは既存の非公開プラットフォームを説明するもので、公開展開を示しません。

検証プラットフォームは内部開発専用です。validation 用 React ビルドと、`hamheatmap-app-service` を再利用する Linux HTTP ブリッジを同一オリジンで提供し、実データの準備、キャッシュ管理、伝搬計算を確認できます。プロセスは `127.0.0.1:1421` 固定です。`0.0.0.0` への変更、Cockpit の `9090` の再利用、クラウドファイアウォールでの新規ポート開放は禁止です。

サーバーでビルドして起動します。

```bash
cd /home/ubuntu/hamheatmap
scripts/validation-platform.sh build
scripts/validation-platform.sh start
scripts/validation-platform.sh status
scripts/validation-platform.sh health
scripts/validation-platform.sh self-test
```

Windows PowerShell で SSH トンネルを作成し、そのターミナルを開いたままにします。

```powershell
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 1421:127.0.0.1:1421 ubuntu@150.65.181.202
```

ローカルブラウザーで `http://127.0.0.1:1421` を開きます。これはトンネルのローカルエンドポイントであり、サーバーの公開ポートではありません。検証後はサーバー上で停止します。

```bash
scripts/validation-platform.sh stop
```

計算、ダウンロード見積もり、ダウンロードの前に、validation ブラウザーは推測困難で短期間有効な operation ID を取得します。長時間リクエストとポーリングは同じ ID を使い、キャンセルはその計算またはダウンロードだけに作用します。状態ポーリングは JSON `POST` を使用し、「現在の操作」や操作一覧を公開せず、ヒートマップ、URL、サーバーパス、詳細エラーも返しません。

3 つのフロントエンドモードは権限が異なります。

| モード | 準備／キャッシュ／計算 | ファイル出力 | 用途 |
|---|---:|---:|---|
| Windows Tauri | 可 | 可 | 最終デスクトップ動作 |
| 非公開 validation server | 可 | 可 | 実コア検証、ブラウザー内診断ダウンロード |
| 通常ブラウザー preview | 不可 | 不可 | UI と表示確認のみ |

validation では、座標、無線パラメーター、計算リクエストを Windows ブラウザーからユーザー管理の JAIST サーバーへ送信します。これはデスクトップ版の「座標と結果をローカルに保持する」原則に対する明示的な内部テスト例外です。テスト座標だけを使用してください。診断ファイルはブラウザー内で生成してダウンロードし、エクスポート用エンドポイントやサーバー側ファイル書き込みは使用しません。実行データ、PID、ログ、ビルドメタデータは `.runtime/validation-platform/` 内に保存し、Docker、システムサービス、システムレベルの実行ディレクトリは使用しません。プロトコルと検証証拠は `docs/15-private-validation-platform.md`、`docs/16-recovery-and-cancellation-validation.md`、`docs/18-progressive-coverage-preview-validation.md` を参照してください。

Tauri シェルは `app/src-tauri/` にあります。JAIST Linux はフロントエンド、共有 Rust サービス、ブラウザー表示回帰、内部 Windows クロスビルドを担当します。正式リリースには、Windows 10/11 上での WebView2、インストーラー、ファイルシステム動作の検証が必要です。

固定したプロジェクト内 Windows クロスツールチェーンを復元し、単一ファイル EXE とオフライン WebView2 コンポーネント内蔵インストーラーをビルドします。

```bash
scripts/install-windows-cross-tools.sh
scripts/tauri-windows-cross.sh
```

復元スクリプトが書き込むのは、サーバープロジェクトの `.tools/` だけです。LLVM、アーカイブ、xwin SDK は合計約 14 GB を使用し、Windows release target とオフライン WebView2 インストーラーには追加容量が必要です。このクロスビルドは内部 Alpha ゲートであり、Windows 実機、コード署名、地図コンプライアンスの受け入れ試験を代替しません。

<!-- section:limitations -->
## 重要な制限

3 種類のリンク結果は、選択した入力、実 DEM/WBM、標準大気の k = 4/3 仮定、編集可能なしきい値に基づく計画予測です。現地交信を保証せず、現行モデルは建物、植生、局所クラッター、干渉、リアルタイム大気条件も追加しません。

HamHeatmap は計画・学習用ツールであり、実際の交信を保証しません。MVP は建物、植生、都市クラッター、外部干渉、リアルタイム気象、異常伝搬、水面反射、給電線損失をモデル化しません。

中国本土で一般公開する前に、ベースマップの許諾、審査、審図番号の確認を完了する必要があります。開発用ベースマップや国際的なオープンソース境界データを、そのまま正式版に含めることはできません。

<!-- section:technology -->
## 技術構成

デスクトップアプリは Tauri 2.11.5、React 19.2.7、TypeScript 7.0.2、Vite 8.1.4、MapLibre GL JS 5.24.0 を使用します。バックエンドは Rust、組み込み SQLite、NTIA 公式 ITM C++ v1.4、Pure Rust の `tiff`、rustls HTTPS を使用します。

現在の表示用ベースマップはすべてオンラインです。PMTiles JavaScript 4.4.1 と fflate 0.8.3 は現行ソースおよび次回ビルド対象から削除済みで、4 省アーカイブも配布しません。現在公開中の Alpha 2 には、この 2 つの旧 JavaScript 依存関係が残っていますが、オフライン地図アーカイブは含まれません。

<!-- section:author -->
## 開発者

プロジェクト作成者・リード開発者：[Arsenic-er](https://github.com/Arsenic-er)。

<!-- section:license -->
## ライセンス

プロジェクトのソースコードは [Apache License 2.0](LICENSE) で提供します。地図、DEM、水域データ、第三者依存関係には、それぞれのライセンスと帰属表示要件が適用されます。詳細は [`THIRD_PARTY_LICENSES.md`](THIRD_PARTY_LICENSES.md) を参照してください。
プロジェクトの著者情報とリリース帰属表示は [AUTHORS.md](AUTHORS.md) と [NOTICE](NOTICE) に記録しています。
