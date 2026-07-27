# テスト用フォントをリポジトリに同梱する

- Priority: High
- Category: add
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/add-bundled-test-fonts
- Polished: {YYYY-MM-DD}

## 目的

0001 メタ issue の Tracked items「テスト用フォント選定」(`issues/0001-enhance-font-module-maturity.md` L138-143) を完了させる。0025 / 0026 / 0027 着手前提として明示されている「kern 付き / GSUB+GPOS 付き / CFF+CFF2 付きフォントのリポジトリ配置」を 1 つの issue にまとめ、段階的選定 (kern → GSUB+GPOS → CFF+CFF2) を本 issue 内で順次完了させる。

## 優先度根拠

High。0025 (カーニング) / 0026 (シェーピング) / 0027 (CFF/CFF2 対応) の **3 つの concrete issue 全ての着手前提** であり、本 issue が closed にならない限り 0025 以降が auto-resolve で進められない。0001 メタ issue 完了条件の前提でもある。直近 (2026-06-22) の auto-resolve 実行で 0023 / 0024 を closed 後、0025 着手時に本 tracked item の未着手がブロッカーとなり停止した。

## 現状

- リポジトリ内に `.ttf` / `.otf` ファイルは一切ない。`tests/fixtures/` や `fonts/` 等のフォント配置ディレクトリも未作成。
- `tests/test_font.rs:7-14` と `tests/test_context.rs` (`mod stroke_text` 内 L497-504) が macOS の `/System/Library/Fonts/Supplemental/Arial.ttf` を直接参照し、Linux / Windows の CI ではスキップされている。
- 0001 メタ issue L138-143 の Tracked items「テスト用フォント選定」が、本 issue 起票まで concrete issue 化されておらず宙吊りになっていた。
- 0001 メタ issue L165-172「前提 concrete issue」セクションでも「以下 4 件を concrete issue として **順次起票**」の 1 つ目に「テスト用フォント選定」が挙げられているが起票されていなかった。

## 設計方針

### フォント選定基準

以下を **すべて満たす** ように、1 つまたは複数のフォントを選定する。

- **ライセンス**: SIL OFL (Open Font License) 1.1、Apache-2.0、CC0、Bitstream Vera license 等の配布許容ライセンス。OFL の `Reserved Font Name` 制約を読み、リポジトリ同梱時の名前変更要否を確認する。
- **必要テーブル**:
  - `kern` テーブル v0 (Format 0 / horizontal) で `(A, V)` / `(T, o)` / `(W, A)` 等のネガティブカーニングペアを持つ (0025 単体テスト `font_kern_known_pair` / `font_measure_text_with_kerning` の検証に必要)
  - `GSUB` テーブル (リガチャ含む。`fi` / `fl` リガチャがある一般的なフォント)
  - `GPOS` テーブル (Pair Adjustment Lookup Type 2)
  - `CFF` または `CFF2` アウトラインを持つフォント (0027 用)。TrueType `glyf` 系とは別のフォントになる可能性が高い
- **カバレッジ**:
  - BMP 外文字 (Plane 1 以降、U+10000-) を含む (0026 の文字列処理境界値テスト用)
  - CJK 統合漢字 (基本ブロック + 拡張ブロックのうち少なくとも基本)
  - ASCII 印字可能文字 (` ` - `~`、現在の Arial 依存テストの代替)

### 候補フォント

候補は polish 段階で最終選定する。以下は検討時の参考。

- **Noto Sans / Noto Sans CJK**: SIL OFL。kern / GSUB / GPOS 完備。CJK + BMP 外もカバー。Noto Sans CJK は CFF base のため 0027 にも使える可能性 (要確認)
- **Source Sans 3 / Source Serif 4 / Source Code Pro**: SIL OFL。Adobe 製で kern / GSUB / GPOS あり
- **Roboto / Roboto Mono**: Apache-2.0。kern / GSUB / GPOS あり
- **DejaVu Sans / Serif / Mono**: Bitstream Vera license (配布許容)。kern / GSUB / GPOS あり
- **Liberation Sans / Serif / Mono**: SIL OFL 2.x。kern / GSUB / GPOS あり

1 フォントで全要件を満たせるかは polish 時に確認する。複数フォントの組み合わせも許容する (例: TrueType 系で kern / GSUB / GPOS + CFF 系で 0027)。

### 配置とリポジトリ構成

- **ディレクトリ**: `tests/fixtures/fonts/` を新規作成。Rust の慣行 (`tests/fixtures/`) に従う。
- **ファイル数**: 必要最小限。リポジトリサイズ膨張を避けるため、TrueType と CFF/CFF2 で各 1 〜 2 フォントに抑える。
- **ライセンスファイル**: 各フォントごとに LICENSE ファイル (OFL.txt 等) を同梱。
- **`Cargo.toml`**:
  - `include` リストにテストフォントを **含めない** (`cargo publish` で配布物が肥大化するのを避ける)。現状の `include = ["/LICENSE", "/README.md", "/src/**"]` を維持する。
  - テストフォントは `tests/fixtures/fonts/` 配下に置き、cargo workspace 内のテストからのみ参照可能とする。

### 既存 Arial 依存の扱い

- **本 issue のスコープ内**: 同梱フォントを置き、`tests/common/font.rs` (または既存ヘルパー位置) に `load_kern_test_font()` / `load_shape_test_font()` 等を追加。
- **本 issue のスコープ外**: 既存 `load_arial()` の撤去・全テスト切り替えは別 issue (0037 `tests/common/font.rs` 集約と組み合わせて) で扱う。0001 メタ issue L141 でも「`load_arial()` ヘルパー撤去、`pbt/tests/prop_font/main.rs` への組み込み」は本 tracked のスコープ外と確定。
- 短期的には `load_arial()` と新ヘルパーが共存する。Arial 経路のテストは macOS 限定で動き続け、新ヘルパーは全プラットフォームで動く。

### 0037 (tests/common/font.rs 集約) との関係

順序は以下のいずれか:

1. **本 issue が先**: 同梱フォントを `tests/fixtures/fonts/` に置き、`load_kern_test_font()` 等を `tests/test_font.rs` に追加。0037 で `tests/common/font.rs` を作成して `load_arial()` と新ヘルパーをまとめて集約。
2. **0037 が先**: `tests/common/font.rs` を先に作成し、本 issue で同梱フォントと新ヘルパーを直接 `tests/common/font.rs` に置く。

polish 段階で決定する。順序選択は完了条件に影響しない (どちらでも tracked closable)。

### CI 検証

リポジトリに同梱したフォントが全プラットフォーム (Linux / macOS / Windows) で正しくロードできることを、`tests/test_font.rs` に新規単体テスト 1 件以上で検証する。例: `font_bundled_test_font_loads`。

## 完了条件

- `tests/fixtures/fonts/` (または同等) に SIL OFL 等の配布許容ライセンスのフォントが配置されている。
- 配置されたフォントが kern / GSUB / GPOS / CFF / CFF2 のすべてを (1 つまたは複数フォントの組み合わせで) 充足する。
- BMP 外文字と CJK 統合漢字 (基本ブロック) のカバレッジを持つ。
- 各フォントの LICENSE ファイルが同梱されている。
- `Cargo.toml` の `include` / `exclude` 設定が適切 (テストフォントが `cargo publish` 配布物に含まれない)。
- 全プラットフォーム (Linux / macOS / Windows) で同梱フォントがロードできることを `tests/test_font.rs` の単体テスト 1 件以上で確認 (CI 全構成で pass)。
- `load_kern_test_font()` / `load_shape_test_font()` / `load_cff_test_font()` 等のヘルパーが定義され、0025 / 0026 / 0027 のテスト作成時に再利用可能。
- 0001 メタ issue の Tracked items「テスト用フォント選定」が close 可能になる (`issues/0001-enhance-font-module-maturity.md` の Concrete issue 一覧テーブルに本 issue を追加し、Tracked items テーブルから「テスト用フォント選定」行を削除する責務は本 issue close PR が持つ)。
- 0025 / 0026 / 0027 の「着手前提」項目のうち「kern 付きフォント」「GSUB / GPOS 付きフォント」「CFF / CFF2 フォント」がそれぞれ解消される。

## 解決方法

polish 段階で確定する。

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。本 issue は Tracked items「テスト用フォント選定」を完了させる責務を持つ。close PR で 0001 の Concrete issue 一覧テーブル更新と Tracked items テーブルからの削除を行う)
- `0025-add-font-kerning.md` (kern 付きフォント前提。本 issue 完了が着手前提)
- `0026-add-opentype-basic-shaping.md` (GSUB / GPOS 付きフォント前提。本 issue 完了が着手前提)
- `0027-add-cff-cff2-outline-support.md` (CFF / CFF2 フォント前提。本 issue 完了が着手前提)
- `0037-refactor-tests-common-font-helper.md` (`load_arial()` 集約 / 撤去。本 issue と連携、順序は polish 時に判断)
