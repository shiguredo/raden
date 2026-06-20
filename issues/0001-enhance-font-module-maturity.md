# font モジュールの成熟度を正式リリースに向けて改善する (メタ issue)

- Priority: High
- Category: other（メタ issue）
- Created: 2026-03-28
- Model: Opus 4.6
- Polished: 2026-06-20

## 目的

font モジュールを正式リリース可能な品質にする。raden は Blend2D 互換の 2D レンダリングライブラリであり、本 issue は `docs/BLEND2D.md` のフォント API 一覧 (L402-457) およびストロークテキスト操作 (L175-176) を完了条件の基準として、concrete issue 0021-0026 と tracked items の進捗を束ねるトラッカーとして機能する。

## 現状

### 実装済み

- TrueType アウトライン (Simple Glyph / Compound Glyph) の描画。Compound Glyph の `ARGS_ARE_XY_VALUES == 0` (point-matching) は未実装で原点フォールバック
- cmap Format 4 (BMP) / Format 12 (Full Unicode) のパース
- head / maxp / hhea / hmtx / loca / cmap / glyf テーブルのパース
- TTC (TrueType Collection) のパース (`face_index` 範囲外は `InvalidData` を返す)
- 公開 API:
  - `FontData`: `from_file()`, `from_bytes()`, `data()`
  - `FontFace`: `from_data()`, `units_per_em()`, `ascent()`, `descent()`, `line_gap()`
  - `Font`: `from_face()`, `size()`, `scale()`, `map_char_to_glyph()`, `glyph_advance()`, `append_glyph_outline()`, `ascent()`, `descent()`
  - `FontError`
  - `Context::fill_text()` (エラー黙殺・未マッピング文字スキップでクラッシュ耐性を優先)

### `Context::fill_text` の制限

- `append_glyph_outline` のエラーは `let _ =` で黙殺。不正フォントや point-matching 未実装による合成位置誤りでグリフが欠落しうるが、advance はそのまま進む
- `glyph_id == 0` (cmap 未マッピング) は `.notdef` を描画せずスキップし、`glyph_id == 0` の advance でカーソルだけ進める。`.notdef` の advance はフォント依存 (Arial 等は正の値、フォントによっては 0)

### テスト

- `tests/test_font.rs` に単体テスト 5 件: `font_face_metrics` / `font_char_to_glyph` / `font_glyph_outline_to_path` / `font_space_glyph_has_no_outline` / `font_fill_text_integration`
- 全テストが macOS の Arial.ttf に依存し、Arial 不在時はスキップ。CI の Ubuntu / Windows ジョブでは全テストがスキップ
- PBT (`pbt/tests/prop_font/main.rs`) は未作成
- fuzzing 基盤 (`fuzz/Cargo.toml`) は未作成 (`fuzz/.gitkeep` のみ。`Cargo.toml` で `exclude = ["fuzz"]`)

## Blend2D との対応

raden の存在意義は Blend2D 互換のため、`docs/BLEND2D.md` のフォント API (L402-457) とストロークテキスト操作 (L175-176) を本 issue の基準とする。close 時点で対象行が「実装済み」「別 issue で対応」「対象外 (理由)」のいずれかに確定していること (完了条件「BLEND2D.md 同期」参照)。

### Blend2D との対応表: concrete issue 0021-0026 で解消する行

各 concrete issue 完了時に該当行を `実装済み` に更新する。

| BLEND2D.md 行 | Blend2D API | 解消 concrete issue | 追加される raden API |
|---|---|---|---|
| L443 (部分) | `metrics()` / `design_metrics()` の line_gap / cap_height / x_height | 0021 | `Font::line_gap()`, `FontFace::cap_height()` / `x_height()`, `Font::cap_height()` / `x_height()` |
| L447 (部分) | `map_text_to_glyphs(BLGlyphBuffer&)` | 0022 (間接) / 0026 (完全) | `Font::glyph_run_for_text()` (`pub(crate)`) → `Font::shape()` |
| L448 (部分) | `position_glyphs(BLGlyphBuffer&)` | 0022 (間接) / 0026 (完全) | 同上 |
| L455 | `get_text_metrics(BLGlyphBuffer&, BLTextMetrics&)` | 0022 | `Font::measure_text()`, `TextMetrics` |
| L452 | `get_glyph_bounds(...)` | 0023 | `FontFace::glyph_bounds()`, `Font::glyph_bounds()`, `GlyphBounds` |
| L175 (部分) | `stroke_utf8_text(...)` | 0024 | `Context::stroke_text()` (UTF-16 / UTF-32 / `stroke_glyph_run` は対象外) |
| L449 | `apply_kerning(BLGlyphBuffer&)` | 0025 | `FontFace::kern()`, `Font::kern()` |
| L446 | `shape(BLGlyphBuffer&)` | 0026 | `Font::shape()`, `GlyphBuffer` |
| L444 | `feature_settings()` / `set_feature_settings()` | 0026 | `FontFeatureSettings`、`Font::from_face` のシグネチャ拡張または別メソッド (具体は 0026 で確定) |
| L450 (部分) | `apply_gsub` / `apply_gpos` | 0026 (基本のみ) | (`shape()` 内部) |
| L456 | `BLGlyphBuffer` | 0026 | `GlyphBuffer` |

### スコープ外行の判定

上表に含まれない L402-457 / L175-176 の全行は close 前に「別 issue で対応」または「対象外 (理由)」に確定させる。代表例として、`BLFontFace::data()` / `weight()` / `family_name()` / `face()` / `set_size()` 以外の variation 系 / 縦書きメトリクス / underline / strikethrough / `BLFontMetrics` の `x_min..y_max` / `BLFontDesignMetrics` の `lowest_ppem` / `h_min_lsb` / `glyph_bounding_box` / `BLFontFace::create_from_file` / テーブル直接アクセス / 一括 API / `Font::matrix()` (raden は等方スケールのみ。回転 / スキューは `Context::transform` で処理) / `Font::scale()` (raden 独自) / UTF-16 / UTF-32 / LATIN1 / WCHAR テキスト経路 (raden は Rust `&str` UTF-8 入力に統一) / `stroke_glyph_run` などがある。判定は本 issue close PR でまとめて確定 (`docs/BLEND2D.md` を直接編集)。

## font 公開 API の安定化方針

raden 全体のバージョンは `Cargo.toml` で `2026.1.1` (CalVer) を採用しており、SemVer ベースの「安定 / 未安定」を明示する全体方針はリポジトリに存在しない。本 issue は font モジュールに限定して以下を方針として固定する。

- font モジュールの公開 API は本 issue close までは **未安定** とみなし、0021-0026 で破壊的変更を許容する
- 破壊的変更 (シグネチャ変更・型変更・戻り値の意味変化・公開構造体フィールドの意味変化・差分テストで `tolerance = 0` でも fail するピクセル変化) は `CHANGES.md` の `## develop` の **トップ階層** に `CHANGE` 種別で記載する (`shiguredo-changelog` スキル準拠)
- 描画結果の微差 (差分テストで `tolerance ≤ 1/255` で pass、変更率 1% 未満) を伴う内部実装リファクタリングは `## develop` の `### misc` 直下に `UPDATE` 種別で記載する。判定が難しい場合は対応 concrete issue の polish で確定する
- 純粋に内部実装のみで描画結果が変わらない変更は CHANGES.md に記載しない
- 新規追加される公開型は `src/lib.rs` 末尾付近の `pub use font::{Font, FontData, FontError, FontFace};` 行に alphabetical 順で追加する
- 将来フィールド追加で破壊的変更を避けたい公開構造体には `#[non_exhaustive]` を付与する (`TextMetrics`, `GlyphBounds`, `GlyphBuffer`, `FontFeatureSettings` が該当)
- font モジュールの本格安定化は raden 全体 SemVer / 安定化方針 (別 issue。tracked items 参照) の確定後に再評価する

### 段階拡張で意味が変わる API

| API | 0022 時点 (close) | 0025 時点 (close) | 0026 時点 (close) | 追加責務を持つ concrete issue |
|---|---|---|---|---|
| `TextMetrics.advance` | f64 (カーニング非適用の単体 advance 総和) | f64 (カーニング適用済み) | f64 (シェーピング適用済み。GSUB + GPOS / Pair Adjustment 含む。`kern` テーブルは GPOS 不在フォントへのフォールバックとして残るかは 0026 で確定) | 0022 / 0025 / 0026 で意味が変化 |
| `TextMetrics` フィールド | `advance` のみ | 同上 | `bounding_box` / `leading_bearing` / `trailing_bearing` 追加 | `bounding_box` の追加責務は 0023 完了直後の PR (0023 のスコープに含める)。`leading_bearing` / `trailing_bearing` 追加は 0026 |
| `Font::glyph_run_for_text()` | `pub(crate) fn(&str, &mut Vec<(u16, f64)>)` | 同上 | `Font::shape()` に置換し本メソッドは削除。`fill_text` / `measure_text` / `stroke_text` の呼び出し元は全て `shape()` 経由に置換 | 0026 |
| `Font::from_face` シグネチャ | `(&FontFace, f64) -> Self` | 同上 | `FontFeatureSettings` 対応 (`from_face` 拡張または `Font::with_features` 等の別メソッド) | 0026 |
| `GlyphBuffer` レイアウト | (未追加) | (未追加) | 0026 で初出。SoA / AoS、placement / advance の整数 / 浮動小数点、`cluster` 配列の有無を 0026 で確定 (確定時に本行を更新) | 0026 |

`BLTextMetrics` 互換の `Point` 化、`BLGlyphBuffer` 互換 SoA レイアウト化、`BLGlyphPlacement` 互換の整数化は本 issue のスコープ外で、font モジュール安定化前に別 issue で確定させる (該当 tracked item は本 issue では持たない。raden 全体安定化方針別 issue と並行で起票判断)。

## Concrete issue 一覧

| issue | ファイル | タイトル | 状態 | 優先度 |
|-------|--------|---------|------|--------|
| 0021 | `0021-add-font-scaled-metrics.md` | Font にスケール済みメトリクス取得を追加する | open | High |
| 0022 | `0022-add-text-measurement.md` | テキストサイズ計測機能を追加する | open | High |
| 0023 | `0023-add-glyph-bounds.md` | グリフ境界ボックス取得機能を追加する | open | High |
| 0024 | `0024-add-stroke-text.md` | ストロークテキスト描画機能を追加する | open | High |
| 0025 | `0025-add-font-kerning.md` | カーニング適用機能を追加する | open | Medium |
| 0026 | `0026-add-opentype-basic-shaping.md` | OpenType 基本シェーピング機能を追加する | open | Medium |

状態列の用語は `shiguredo-issues` スキルに従う (`open` / `closed` / `pending`)。Polish 進捗は各 issue ファイルの `Polished:` フィールドで管理する。

## Tracked items

本 issue で追跡するが、concrete issue ファイルとして未作成の項目。番号は `SEQUENCE` に従う (現状 SEQUENCE = 27)。

| 項目 | 概要 | メモ |
|------|------|------|
| 既存コード PBT | `tables.rs` / `glyph.rs` の既存パース関数群の PBT | open |
| fuzzing 基盤と既存コード fuzz target | cargo-fuzz 初期化、CI 定期実行、`parse_all` 経由の fuzz target 追加 | open |
| CFF / CFF2 調査 | OS 標準フォントの形式調査と対応可否判断 | open |
| テスト用フォント選定 | 全プラットフォームで利用可能な配布許容フォントの選定とリポジトリ追加 | open。PBT への組み込みは 0021 並行ブロック内で実施 |
| ベースライン benchmark 計測基盤 | font 関連 bench ファイルの作成と criterion 設定 | open |
| Compound Glyph point-matching 実装 | `ARGS_ARE_XY_VALUES == 0` ケースの実装 (Bug カテゴリ) | open |
| `pbt/tests/prop_font/main.rs` 初回作成 | ディレクトリ構成 (`pbt/README.md` 規則) | 0021 のサブタスク |
| raden 全体 SemVer / 安定化方針確定 | font モジュール本格安定化の前提となる raden 全体方針 | open。本 issue close PR 内で別 issue として起票し、起票完了で本 tracked も完了扱い |

`fill_text` 内のエラー黙殺・`glyph_id == 0` スキップのトレードオフを日本語コメントで明記する作業は **0022 のスコープに完全内包** する (0022 の解決方法に組み込み済み)。tracked item として独立追跡しない。

### tracked: 既存コード PBT

- **含まれる**: `tables.rs` の `parse_head` / `parse_maxp` / `parse_hhea` / `parse_hmtx` / `parse_loca` / cmap Format 4 / cmap Format 12 パース / `ttc_font_offset`、`glyph.rs` の `parse_simple_glyph` / `parse_compound_glyph` / `emit_contour` の PBT
- **含まれない**: 0021-0026 が追加するテーブル (OS/2 / kern / GSUB / GPOS) の PBT、cmap Format 0/2/6 等の他フォーマット、パニック耐性 (fuzzing 基盤 tracked の役割。`pbt/README.md` 方針)
- **完了基準**: 上記関数全てに不変条件 PBT があり、`cargo test -p pbt --test prop_font` が CI でパス

### tracked: fuzzing 基盤と既存コード fuzz target

- **含まれる**: `fuzz/Cargo.toml` 作成、`fuzz/fuzz_targets/` 初期化、`.gitignore` に `fuzz/corpus/` `fuzz/artifacts/` 追加、CI cron 設定、`parse_all` 経由の既存コード fuzz target 1 個 (TTC / TableDirectory / 各種パーサに到達する入口) の追加
- **含まれない**: 0021-0026 が追加するテーブルに対する個別 fuzz target (0021 / 0025 / 0026 が自身のスコープで追加)
- **完了基準**: `cargo fuzz run` が手動実行可能、CI cron 稼働、`cargo check --manifest-path fuzz/Cargo.toml` が CI に追加されコンパイル安全性が保たれる

### tracked: CFF / CFF2 調査

- **含まれる**: macOS 14+ / Windows 11 / Ubuntu 24.04 LTS の標準バンドルフォントを、各 OS で対象パッケージリストを定めて調査 (Ubuntu は `fonts-*` 主要パッケージ群)。実バイナリで `CFF ` テーブル (tag 0x43464620) / `CFF2` テーブル (tag 0x43464632) の有無を確認し割合を算出
- **含まれない**: CFF / CFF2 パーサ本体の実装 (調査結果次第で別 concrete issue として起票)
- **完了基準**: 各 OS の調査結果と判定 (対応 / 対象外) を CFF 調査 concrete issue 内のレポートに記録。判定基準 (対応すべきと判断する閾値) は CFF 調査 concrete issue 起票時に固定する

### tracked: テスト用フォント選定

- **含まれる**: SIL OFL 等の配布許容ライセンスを持ち、各 concrete issue が必要とするテーブル (OS/2 / kern / GSUB / GPOS) を充足し、BMP 外文字および CJK 統合漢字のカバレッジを持つフォントの選定とリポジトリ追加 (置き場所決定を含む)
- **含まれない**: `load_arial()` ヘルパー撤去、`pbt/tests/prop_font/main.rs` への組み込み (PBT 組み込みは 0021 並行ブロック内で行う)
- **完了基準**: 全プラットフォームで利用可能なフォントがリポジトリに含まれる PR がマージ済み
- **段階的選定**: 着手前準備で TTF アウトラインのみのフォントを先行選定。kern 付きフォントは 0025 着手時、GSUB / GPOS 付きフォントは 0026 着手時に追加選定する

### tracked: ベースライン benchmark 計測基盤

- **含まれる**: 既存 `benches/` の命名規則 (`fill_*` / `stroke_*` 等の操作種別直接型) に従う font 系 bench ファイル (例: `fill_text.rs`) の作成、criterion 設定 (既存 `[[bench]]` と同様 `harness = false` + `name = ...` の 2 行形式、`autobenches = false` 環境下で `Cargo.toml` への `[[bench]]` エントリ追加が必須)、空 bench 関数で `cargo bench` がエラーなく実行可能な状態
- **含まれない**: 実際のベースライン値取得 (各 concrete issue の実装完了ごとに実施)
- **完了基準**: `cargo bench` が成功し、後続の concrete issue が bench を追記できる土台が整っている

### tracked: Compound Glyph point-matching 実装

- **含まれる**: `ARGS_ARE_XY_VALUES == 0` ケース (point-matching) の実装。参照グリフの point 座標解決と子グリフの合成位置決定
- **含まれない**: Compound Glyph の他の制限 (例: metrics 引き継ぎフラグ未対応)
- **完了基準**: 和文 TrueType (MS ゴシック等) で破綻していた Compound Glyph が正しく描画される PBT / 視覚回帰テストがパス

### tracked: `pbt/tests/prop_font/main.rs` 初回作成

- **含まれる**: `pbt/README.md` のディレクトリモジュール規則に従う `prop_font/main.rs` 構成の初回作成。`pbt/Cargo.toml` への `[[test]] name = "prop_font" path = "tests/prop_font/main.rs"` エントリ追加 (Cargo はディレクトリ配下の `main.rs` を自動認識しないため明示的指定が必須)
- **含まれない**: 個別 PBT の追加 (各 concrete issue のスコープ)
- **完了基準**: ファイル存在 + `[[test]]` エントリ追加 + `cargo test -p pbt --test prop_font` がパス

### tracked: raden 全体 SemVer / 安定化方針確定

- **含まれる**: raden プロジェクト全体のバージョニング (CalVer の継続 or SemVer 移行) と公開 API 凍結の方針を扱う別 issue の起票。本 issue 自体では中身を決めない
- **含まれない**: 個別モジュールの安定化判定 (font 以外)
- **完了基準**: 別 issue 番号が確定し、本 issue 備考に追記済み。当該別 issue の close は本 issue 完了条件に含めない

## 着手順序と依存関係

### 前提 concrete issue (0021 着手前に concrete issue 化して close)

以下 4 件を concrete issue として **順次起票** (起票直後に SEQUENCE をインクリメントしてコミット。複数人で分担する場合は事前に担当順序を合意し、各担当は前担当の SEQUENCE インクリメントコミットを取り込んでから起票する):

1. テスト用フォント選定 (TTF アウトラインのみのフォントを先行選定とリポジトリ追加)
2. fuzzing 基盤と既存コード fuzz target
3. ベースライン benchmark 計測基盤
4. Compound Glyph point-matching 実装 (Bug カテゴリのため他と並行で close まで完了)

起票時に Concrete issue 一覧テーブルに新規行を追加し、Tracked items テーブルから対応行を削除する。

raden 全体 SemVer / 安定化方針確定 tracked は本 issue close PR の中で別 issue として起票し、起票だけで完了扱いとする (Concrete issue 一覧テーブルには追加せず、本 issue 備考に別 issue 番号を追記)。

### 並行着手可能ブロック (0021 / 0022 / 0023)

0021 / 0022 / 0023 は相互に依存しないため並行着手可能。`pbt/tests/prop_font/main.rs` は本ブロック内で先着が作成する (0021 が原則担当だが、0022 / 0023 が先着した場合は当該担当が作成し 0021 がマージする)。

### 依存関係 (0024 / 0025 / 0026、部分並行可)

- 0024 → 0022 (`glyph_run_for_text` を使用)
- 0025 → 0022, 0024 (`fill_text` / `measure_text` / `stroke_text` 3 者にカーニング適用)
- 0026 → 0022, 0025 (`glyph_run_for_text` を `shape()` で置換、GPOS Pair Adjustment で `kern` テーブルを置換)

close 順は 0024 → 0025 → 0026 の直列を原則とする。ただし 0025 は `stroke_text` 以外 (`fill_text` / `measure_text`) について 0024 と並行着手可能。0024 で書かれた `stroke_text` は 0025 完了時にカーニング適用へ改修する必要があり、この後追い改修は **0025 のスコープに含む** (本 issue は 0025 close まで close できないため、0025 polish 時に 0025 解決方法へ反映される)。

### 最終 PR (本 issue を close する PR)

0021-0026 と前提 concrete issue の全 concrete issue が close、CFF / CFF2 判定が確定、raden 全体 SemVer / 安定化方針別 issue が起票済の後、`docs/BLEND2D.md` のフォント API 一覧 (L402-457) およびストロークテキスト操作 (L175-176) の全行を確定状態 (`実装済み` / `別 issue` / `対象外`) に揃える PR で本 issue を close する。

## 備考

- 本 issue はメタ issue のため `Branch` フィールドを持たない (複数ブランチにまたがる)
- ファイル名 `enhance-font-module-maturity` は作成時の命名を保持する (`git mv` は行わない。他ブランチや進行中 issue からの参照リンクを安定させるため)
- 本 issue 内の各テーブル (Concrete issue 一覧 / Tracked items / Blend2D との対応表) は、対象 concrete issue を close する PR の中で同 PR で更新する。最終 PR (本 issue を close する PR) では `docs/BLEND2D.md` の確定状態化と raden 全体 SemVer 別 issue 起票・本備考への番号追記を行う
- raden 全体 SemVer / 安定化方針別 issue 番号: (本 issue close PR で追記する)

## 解決方法

(本 issue の close 時に追記する。完了した全 concrete issue の番号と完了日、Tracked items の最終状態、当初予定との差異、CFF / CFF2 判定結果、`docs/BLEND2D.md:402-457` および L175-176 の最終状態サマリを含める)

## 完了条件

以下すべてを満たした時点で本 issue を close する。

### concrete issue

- 0021 / 0022 / 0023 / 0024 / 0025 / 0026 がすべて closed で、CI でテストがパス
- 0025 close 時点で「0024 完了済の `stroke_text` にカーニング適用済」が確認できる (本 issue 完了の前提)
- Tracked items のうち concrete issue 化対象 (既存コード PBT / fuzzing 基盤と既存コード fuzz target / CFF・CFF2 調査 / テスト用フォント選定 / ベースライン benchmark 計測基盤 / Compound Glyph point-matching 実装 / `pbt/tests/prop_font/main.rs` 初回作成) が、closed または `issues/pending/` 移動済み (CFF 調査は pending 移動も完了扱い)
- raden 全体 SemVer / 安定化方針確定 tracked は別 issue 起票・本 issue 備考への番号追記で完了扱い

### BLEND2D.md 同期

- `docs/BLEND2D.md:402-457` および L175-176 の全行が以下のいずれかに確定:
  - `実装済み` (raden 側のメソッド名を併記)
  - `別 issue で対応 (issue 番号)`
  - `対象外 (理由)`
- `未実装` および `差異あり` 状態のままの行が残っていない。検証コマンド例: `sed -n '402,457p;175,176p' docs/BLEND2D.md | grep -nE '未実装|差異あり'` がマッチ 0 件

### パフォーマンス基準

- ベースライン benchmark で測定: PPEM = 16 / N = 100 文字 / `Context::fill_text` 1 ループ所要時間が 1ms 未満、N = 1000 文字でも 10ms 未満 (線形性)。測定対象フォントはテスト用フォント選定で確定する
- 各 concrete issue の実装完了ごとにベースライン値を取得し、回帰がないことを確認

### テスト品質

- PBT で font モジュールの各パーサの不変条件が検証され、`cargo test -p pbt --test prop_font` がパス
- fuzzing で font テーブルパーサのクラッシュ耐性が検証されている。本 issue close 直前の直近 1 回の 24 時間連続実行で新規クラッシュ 0 件 (修正が発生したら修正反映後の連続実行が新規クラッシュ 0 件で 24 時間に到達すればよい。複数回修正が必要な場合の総実行時間は本完了条件のスコープ外)

### CFF / CFF2 判断

- CFF / CFF2 調査の結論が **対応** または **非対応** のいずれかに排他的に確定:
  - 対応の場合: CFF / CFF2 パーサ実装の concrete issue が起票され、本 issue Concrete issue 一覧に追加され、closed まで完了
  - 非対応の場合: CFF 調査 concrete issue が `issues/pending/` 移動済み、BLEND2D.md 同期で `OTTO` sfnt version の行に「対象外 (調査結果により対応見送り)」が記載済み

### クロスプラットフォーム

- `.github/workflows/ci.yml` の matrix 全構成 (`ubuntu-24.04` / `ubuntu-24.04-arm` / `ubuntu-22.04` / `ubuntu-22.04-arm` / `macos-26` / `macos-15` / `windows-2025`) で font テストがパス。GitHub Actions の現行 macOS ランナーはいずれも arm64 (x86_64 ランナーは現状提供されていない)
- 全プラットフォームで利用可能なテスト用フォントバイナリがリポジトリに含まれる
