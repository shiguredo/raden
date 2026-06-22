# テスト用フォントのダウンロード機構を追加する

- Priority: High
- Category: add
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: {Git-Flow のブランチ名}
- Polished: {YYYY-MM-DD}

## 目的

raden の font 関連テスト・bench が macOS 限定 `Arial.ttf` に依存している状況を解消するため、CI 全プラットフォームで利用可能なテスト用フォント (Source Sans 3 / Source Serif 4 等) を **必要なタイミングで外部からダウンロードする仕組み** をリポジトリに追加する。

**前提ルール (raden プロジェクト共通)**: フォントバイナリ (.ttf / .otf / .ttc / .woff / .woff2 等) および LICENSE 全文は **リポジトリにコミットしない**。代わりに取得スクリプト・`build.rs`・CI ステップ・Makefile target・テストヘルパー等の「**ダウンロードする仕組み**」をリポジトリに置く。ダウンロード先キャッシュは `.gitignore` で除外する。

本 issue 着手により 0001 メタ issue の Tracked items「テスト用フォント選定」を完了させ、0025 (kern) / 0026 (GSUB+GPOS) / 0027 (CFF / CFF2) の着手前提を一括解消する。

## 優先度根拠

High。0025 / 0026 / 0027 の 3 つの concrete issue の着手前提であり、本 issue が close されない限り 0025 以降の auto-resolve が進められない。0001 メタ issue 完了条件「全プラットフォームで利用可能なテスト用フォントが CI で使える」の前提でもある。

## 現状

- リポジトリ内に `.ttf` / `.otf` / `.ttc` / `.woff` / `.woff2` は 1 件もない (`find . -type f \( -name "*.ttf" -o -name "*.otf" -o -name "*.ttc" \) -not -path "*/target/*" -not -path "*/.git/*"` で 0 件)。`tests/fixtures/` / `tests/common/` / `fonts/` ディレクトリも未作成。
- `.gitattributes` はリポジトリルートに未作成。
- `tests/test_font.rs` の `load_arial()` 関数と Arial 依存テスト 11 件、`tests/test_context.rs` の `mod stroke_text` 内 `load_arial()` 関数と利用箇所、`pbt/tests/prop_font/main.rs` の `load_arial()` 関数と多数の利用箇所が macOS の `/System/Library/Fonts/Supplemental/Arial.ttf` を直接参照する。Linux / Windows の CI ではすべてスキップされている。具体的な行番号は実装着手時に grep で確認する。
- 現状 raden が実装済みのフォントテーブルは `head` / `maxp` / `hhea` / `hmtx` / `loca` / `cmap` / `glyf` / `OS/2` および TTC のみ。kern / GSUB / GPOS / CFF / CFF2 は **未パース** (`src/font/tables.rs` の `TAG_*` 定数と `parse_all` の `dir.require(TAG_GLYF)` 経路で確認)。
- `Cargo.toml` の `include = ["/LICENSE", "/README.md", "/src/**"]` (`Cargo.toml:11`) はホワイトリスト方式で、`tests/` 配下と `.gitignore` の追記は `cargo publish` 配布物から自動的に除外される。
- 0001 メタ issue の Tracked items「テスト用フォント選定」が、本 issue で concrete issue 化される。

## 設計方針

### フォント本体は同梱しない (確定)

本 issue 内で以下は **絶対にしない**:

- `tests/fixtures/fonts/` 等のディレクトリにフォントバイナリを置く
- `LICENSE.txt` (フォント由来) を同梱する
- `.gitattributes` でフォントバイナリ拡張子の binary 宣言を追加する (フォントが存在しないため不要)

これは raden プロジェクト共通ルールであり、本 issue では覆さない。覆す必要が出た場合は本 issue を停止してユーザーに確認する。

### ダウンロード機構の方式選定 (polish で 1 つに確定)

以下の方式から polish 段階で 1 つ (または併用) を確定する。各方式の長短と確定基準は polish で深掘りする。

#### 方式 A: テストヘルパー内で初回起動時にダウンロード + ローカルキャッシュ

- `tests/common/font_fetch.rs` のような共有ヘルパーを新規作成
- キャッシュ先候補: `target/test-fonts/` または `$CARGO_TARGET_DIR/test-fonts/` (`.gitignore` で除外)
- SHA-256 整合性検証 (不一致なら再ダウンロード)
- メリット: `cargo test` で完結。CI / ローカル開発の差がない。`cargo publish` 配布物には混入しない (テストコードのみ)
- デメリット: テスト初回起動が遅い。ネットワーク必須 (オフライン環境で不可)

#### 方式 B: `build.rs` で download-on-build

- ワークスペース直下に `build.rs` を新設 (もしくは別 helper crate)
- ダウンロード先候補: `OUT_DIR` 配下
- メリット: ビルド時に解決され、テスト時はキャッシュ済み
- デメリット: 通常のビルドに不要な build.rs 実行が増える。`cargo publish` 時にダウンロードが走る可能性 (回避策が必要)

#### 方式 C: 取得スクリプト (`scripts/fetch-test-fonts.sh` + PowerShell 版)

- ローカル開発: 開発者が手動で `./scripts/fetch-test-fonts.sh` を実行
- CI: ワークフローで本スクリプトを呼ぶ
- ダウンロード先候補: `tests/fixtures/fonts/` (`.gitignore` 除外)
- メリット: 仕組みが明示的でデバッグしやすい
- デメリット: ローカルで手動実行を忘れるとテスト fail。Windows / Unix で別スクリプトが必要

#### 方式 D: CI ワークフロー直接 (`.github/workflows/ci.yml`)

- 方式 C と類似だが、ワークフロー側に `curl` / `Invoke-WebRequest` ステップを直接書く
- メリット: CI 側は完全自動
- デメリット: ローカルでテストを動かすには別途仕組み (A / B / C) が必要 (= 単独採用不可)

#### 方式比較の polish 観点

- `cargo test` 単独で動くか (CI と差を作らないか)
- ネットワーク不通時の挙動
- `cargo publish` への影響
- 既存 `load_arial()` 経路との共存

### 取得対象フォント (polish で SHA-256 と URL を確定)

| 用途 | フォント | 形式 | ライセンス | release タグ (想定) |
|---|---|---|---|---|
| TrueType (GSUB liga / GPOS Pair Adjustment 検証) | Source Sans 3 Regular | TTF | SIL OFL 1.1 | `3.052R` |
| OpenType CFF (CFF アウトライン検証) | Source Serif 4 Regular | OTF (CFF) | SIL OFL 1.1 | `4.005R` |

実装着手時に `git ls-remote --tags https://github.com/adobe-fonts/source-sans` および `https://github.com/adobe-fonts/source-serif` で最新の `R` 付き release タグを再確認する。`R` 付きが正規 release バイナリを含むタグであり、数字のみのタグは release バイナリを含まない場合がある。タグが想定と異なる場合は **本 issue を停止しユーザーに報告** する (採用フォントの再選定は polish 担当の責務)。

URL (想定):

- Source Sans 3: `https://github.com/adobe-fonts/source-sans/raw/refs/tags/3.052R/TTF/SourceSans3-Regular.ttf`
- Source Serif 4: `https://github.com/adobe-fonts/source-serif/raw/refs/tags/4.005R/OTF/SourceSerif4-Regular.otf`

SHA-256 は polish 段階で実機取得して確定する。再現性のため本 issue 本文にも記載する。

### ライセンス

- フォント本体を **再配布しない** ため、SIL OFL 1.1 Section 4 (Permission Notice 同梱要件) は発生しない (再配布者ではなく、各開発者 / CI が原本リポジトリから直接取得するため)
- LICENSE 本文はリポジトリに同梱しない。代わりに次のいずれかで「テストフォントは Adobe Source Sans / Source Serif (SIL OFL 1.1) をダウンロードする」旨を明記する (polish で確定):
  - リポジトリの `README.md` のテストセクション
  - `tests/common/README.md` を新設
  - ダウンロード機構 (スクリプト / `build.rs`) のコメント or 出力メッセージ

### 既存 Arial 依存テストの扱い

- 本 issue のスコープ内: ダウンロード機構の追加と、その仕組みを使う **新規スモークテスト 1〜3 件** を `tests/test_font.rs` に追加する。
- 本 issue のスコープ外:
  - 既存 Arial 依存テスト全件 (`tests/test_font.rs` / `tests/test_context.rs` / `pbt/tests/prop_font/main.rs`) の切り替え → 0037 (またはその後続) のスコープ
  - 0025 / 0026 / 0027 issue 本文内のパスプレースホルダの置換 → 各 issue 着手時に該当 issue の polish (または実装着手時) で確定する
- 短期的には `load_arial()` 経路 (macOS 限定) と本 issue の新規スモークテスト (全プラットフォーム) が共存する。

## 完了条件

### リポジトリ作業ツリーの不変条件

- `git ls-files | grep -E '\.(ttf|otf|ttc|woff2?)$'` が 0 件 (フォントバイナリが 1 つもコミットされていない)
- `git ls-files | grep -iE '(SourceSans|SourceSerif).*LICENSE'` が 0 件 (フォント由来 LICENSE が同梱されていない)
- `.gitignore` にダウンロード先キャッシュディレクトリの除外設定が追加されている

### 動作

- ダウンロード機構が CI 全 7 構成 (`ubuntu-24.04` / `ubuntu-24.04-arm` / `ubuntu-22.04` / `ubuntu-22.04-arm` / `macos-26` / `macos-15` / `windows-2025`) で動作する
- ローカルで `cargo test` 単独実行時にも (方式 A の場合) または明示的セットアップ後 (方式 C の場合) にダウンロード機構が動作する

### 取得対象の検証

- ダウンロードしたフォントが要求テーブルを持つことを確認する仕組みがある:
  - Source Sans 3: GSUB (`liga` feature) / GPOS (Pair Adjustment Lookup) / cmap (Format 4 または Format 12)
  - Source Serif 4: `CFF` テーブル
- 検証方式は polish で確定 (ヘルパー内 sfnt table directory パース / 外部 `ttx` ステップ / 等)
- 要件不足が検出された場合は **auto-resolve は停止しユーザーに報告** する

### テスト

- ダウンロード機構を使う新規テストが少なくとも 1 件 `tests/test_font.rs` に追加され、CI 全 7 構成で pass する
- 既存 `load_arial()` 経路は本 issue では変更しない

### 0001 メタ issue 更新

(polish で確定。少なくとも以下を扱う)

- `## Concrete issue 一覧` テーブルへの 0039 行追加
- `## Tracked items` テーブルからの「テスト用フォント選定」行削除
- `### tracked: テスト用フォント選定` サブセクション全体の削除
- `### 前提 concrete issue` リストからの「テスト用フォント選定」項目削除と番号繰り上げ
- `### パフォーマンス基準` / `### CFF / CFF2 判断` / `### クロスプラットフォーム` 各完了条件項目の「テスト用フォント選定」参照書き換え

### 0037 / 0027 への申し送り

(polish で確定。0037 はヘルパー集約のスコープに本 issue のダウンロード機構を組み込むかを判断する材料を申し送る。0027 は CFF テストで本 issue のダウンロード経路を参照する旨を申し送る)

### 0025 / 0026 への申し送り

(polish で確定。各 issue 内のテスト用フォント参照を本 issue のダウンロード経路に書き換える指示を申し送る)

## 解決方法

polish 段階で確定する。本書き直し時点での確定事項は以下のみ:

1. フォント本体・LICENSE 全文をリポジトリにコミットしない (確定)
2. ダウンロード機構をリポジトリに置く (確定)
3. 取得対象は Source Sans 3 Regular (TTF) と Source Serif 4 Regular (OTF / CFF) の 2 つ (確定。release タグは着手時再確認)

実装方式 (A / B / C / D)・キャッシュディレクトリ・SHA-256 整合性検証の有無・ライセンス明記場所・既存 issue への申し送り詳細は polish で確定する。

## エッジケース

- **ネットワーク不通 (CI / ローカル)**: ダウンロード失敗時の挙動 (retry 回数 / cache miss 時 fail-fast / mirror URL 切り替え) を polish で確定
- **release タグ変動**: Source ファミリの release タグが想定 (`3.052R` / `4.005R`) と異なる場合、auto-resolve は停止しユーザーに報告。SHA-256 で固定するか、最新タグ自動追従するかは polish で確定
- **オフライン開発**: ローカルキャッシュがあればネットワーク不要、なければ初回失敗を許容するかを polish で確定
- **取得フォントが要求テーブル不足**: 検証フェーズで検出 → auto-resolve 停止
- **`cargo publish` への影響**: 方式 B (build.rs) の場合に publish 時のダウンロード暴走を回避する条件分岐が必要 (方式選定 polish で確定)

## 変更対象ファイル

(polish で方式選定後に確定。以下は方式に依存しない確定項目のみ)

| ファイル | 種別 | 内容 |
|---|---|---|
| `.gitignore` | 既存編集 | ダウンロード先キャッシュディレクトリの除外設定追加 |
| `tests/test_font.rs` | 既存編集 | ダウンロード機構を使うスモークテスト 1〜3 件を追加 |
| `issues/0001-enhance-font-module-maturity.md` | 既存編集 | Concrete issue 一覧追加・Tracked items 行削除・関連完了条件項目書き換え |

(方式 A の場合: `tests/common/font_fetch.rs` 等の新規ファイル。方式 B の場合: `build.rs`。方式 C の場合: `scripts/fetch-test-fonts.sh` 等。詳細は polish で)

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。本 issue は Tracked items「テスト用フォント選定」を完了させる責務を持つ)
- `0025-add-font-kerning.md` (kern 付きフォント前提。本 issue 完了が着手前提)
- `0026-add-opentype-basic-shaping.md` (GSUB / GPOS 付きフォント前提。本 issue 完了が着手前提)
- `0027-add-cff-cff2-outline-support.md` (CFF / CFF2 フォント前提。本 issue 完了が着手前提)
- `0037-refactor-tests-common-font-helper.md` (`load_arial()` 集約)
