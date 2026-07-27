# カーニング適用機能を追加する

- Priority: Medium
- Category: add
- Created: 2026-05-11
- Completed: 2026-06-22
- Model: Kimi K2.7 Code
- Branch: feature/add-font-kerning
- Polished: 2026-06-21

## 目的

グリフペア間のカーニングを適用し、プロポーショナルフォント (例: `(A, V)` / `(T, o)` / `(W, A)`) でのテキスト描画品質を向上させる。Microsoft OpenType `kern` テーブル v0 (Format 0 / horizontal) を新規パースし、`fill_text` / `measure_text` / `stroke_text` の 3 経路でカーニング量を `advance` に加算する。

本 issue は `add` カテゴリ (API 追加が主目的) として扱うが、`Font::measure_text` および `Context::fill_text` / `Context::stroke_text` の値・描画結果に意味的変化を生じ、また `Font::measure_text` の結合性 (隣接 advance の和) 保証を取り下げるため、`CHANGES.md` には `CHANGE` 種別も併記する。これは 0001 メタ issue「font 公開 API の安定化方針」で「font モジュールの公開 API は本 issue close までは未安定とみなし、0021-0027 で破壊的変更を許容する」「破壊的変更 (戻り値の意味変化等) は `CHANGES.md` の `## develop` トップ階層に `CHANGE` 種別で記載する」と確定済みのため、`add` カテゴリ concrete issue 内で `CHANGE` 種別を併発する設計が許容される。同方針により `Category: change` の別 issue は起票しない (0026 も同じ設計で 0001 由来)。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L449 `BLFont::apply_kerning(BLGlyphBuffer&)`**: raden は `FontFace::kern(u16, u16) -> i16` / `Font::kern(u16, u16) -> f64` の個別取得 API と、`fill_text` / `measure_text` / `stroke_text` 内での自動適用で対応。raden は `BLGlyphBuffer` 相当 (`GlyphBuffer`) を 0026 で初出するため、本 issue 時点では一括適用 API を持たず、隣接ペア走査で `advance` に加算する形で対応する

申し送り:

- 本 issue ではカーニングは無条件で適用する。`FontFeatureSettings` (`kern` feature の on/off 制御) は 0026 のスコープ
- 0026 で `Font::shape()` が GPOS Pair Adjustment (kern テーブルの後継) を実装した時点で、本 issue の `kern` テーブル使用経路は GPOS 不在フォントへのフォールバックとして残る (0026 polish 済で確定)

完了 PR で `docs/BLEND2D.md` L449 の raden 列に新規 API 名 (`Font::kern()` / `FontFace::kern()`) を追記する。状態列 (`未実装: カーニング適用`) は本 issue では変更せず、0001 メタ issue の close PR でまとめて整理する。

## 現状

- `measure_text()` (0022) も `fill_text()` (0022 でリファクタ済) も `stroke_text()` (0024 で追加予定) もカーニング非適用
- `Font::glyph_run_for_text` (0022 で追加された `pub(crate)`) の出力バッファ `&mut Vec<(u16, f64)>` の `f64` 要素は「カーニング非適用の単体 advance」(0022 設計方針で確定)
- `kern` テーブルは未パース。`src/font/tables.rs` に TAG 定数も `parse_kern` 関数もない
- GPOS テーブルは未パースまたは未利用 (0026 で対応予定)

## 設計方針

### Microsoft OpenType `kern` テーブル v0 (Format 0) を採用

Microsoft OpenType `kern` v0 仕様を採用する。Apple `kern` v1 (`version == 0x00010000`、ヘッダ・subtable レイアウトが Microsoft 版と異なる) は **テーブル全体をスキップ** して空 `KernTable` を返す。

レイアウト:

- **kern テーブルヘッダ (4 バイト)**: `version: uint16` (offset 0)、`n_tables: uint16` (offset 2)
- **subtable ヘッダ (6 バイト)**: `sub_version: uint16` (Microsoft v0 では 0) / `length: uint16` / `coverage: uint16`。最初の subtable は `rec.offset + 4` から始まる
- **subtable 全体長**: subtable ヘッダの `length` フィールドは **subtable 全体の長さ (ヘッダ 6 + body 含む)**。次の subtable は `current_offset + length` で求める
- **Format 0 body (8 + n_pairs * 6 バイト)**: `n_pairs: uint16` / `search_range: uint16` / `entry_selector: uint16` / `range_shift: uint16` の後に `n_pairs` 個の `(left: uint16, right: uint16, value: int16)` ペア配列。raden は線形探索のため `search_range` / `entry_selector` / `range_shift` は読み捨てる

coverage フィールド (u16) のビット配置 (Microsoft kern v0、bit 番号は u16 全体に対する位置):

- bit 0: horizontal (1 ならば水平 kerning)
- bit 1: minimum (set されていれば不採用)
- bit 2: cross-stream (set されていれば不採用)
- bit 3: override (set されていれば不採用)
- bits 4-7: reserved (本 issue では reserved 非ゼロでも採用する。将来拡張への耐性および HarfBuzz / FreeType の慣行に合わせる)
- bits 8-15: format (0 ならば Format 0)

採用条件:

- **最初に Format 0 / horizontal / `sub_version == 0` の全条件を満たし、かつ `n_pairs > 0` でペア配列を持つ subtable のペアのみ採用する** (Microsoft 仕様は複数 subtable の累積適用も許容するが、本 issue では単純化のため最初の 1 つに限定。`n_pairs == 0` の空 subtable で「採用済み」とすると後続の有効ペアを取りこぼすため `n_pairs > 0` 必須。HarfBuzz / FreeType も同じ慣行。0026 で GPOS Pair Adjustment 経路に統合されるため本 issue 限定の妥協)
- 採用条件を満たさない subtable は不採用としてスキップ (`current_offset += length` で次の subtable へ)
- グリフペアの検索は線形探索 (`n_pairs` 件の `(left, right)` を順次比較)

### API 設計

- `FontFace::kern(glyph_id1: u16, glyph_id2: u16) -> i16` (デザインユニット、テーブル不在時 / ペア未登録時は 0)
- `Font::kern(glyph_id1: u16, glyph_id2: u16) -> f64` (スケール済みピクセル、同じく 0 フォールバック)

`glyph_id == 0` (.notdef) を含むペアもカーニング探索の対象とする (ペア未登録時の 0 フォールバックで自然に処理される)。

`FontFace::kern` と `Font::kern` の探索ロジックは共通化するため、`KernTable` に `pub(crate) fn lookup(&self, left: u16, right: u16) -> Option<i16>` ヘルパーを定義し、両者から呼び出す (重複排除)。

### `glyph_run_for_text` の不変性維持

0022 で追加された `Font::glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` (`pub(crate)`) の出力バッファ要素型 `(u16, f64)` は本 issue でも変更しない。`f64` は「カーニング非適用の単体 advance」のまま保ち、docstring (`/Users/voluntas/shiguredo/raden/src/font/mod.rs:180-190`) も維持する。

カーニング適用は **呼び出し側 (`Font::measure_text` / `Context::fill_text` / `Context::stroke_text`) の責務** とし、`glyph_run_for_text` の出力に対して隣接ペア `(glyph_id[i], glyph_id[i+1])` に対して `Font::kern(glyph_id[i], glyph_id[i+1])` を計算し、`advance` 累積に加算する。

実装内 (`measure_text` / `fill_text` / `stroke_text`) のカーニング加算は `Font::kern` (戻り値 `f64`) を使う。`FontFace::kern` (戻り値 `i16`) は公開 API 用で、内部加算には使わない。

### 0022 / 0023 / 0024 PBT・テストへの影響

- 0022 で追加された `pbt/tests/prop_font/main.rs:51-67` の `prop_concatenation` 関数本体および `pbt/tests/prop_font/main.rs:107-110` の `#[test] fn concatenation()` は、本 issue で `measure_text(a + b)` の境界に kern が適用されるため不変条件が破綻する。両方を本 issue で削除する
- 0022 で追加された `pbt/tests/prop_font/main.rs:91-100` の `prop_non_negative` 関数本体および `pbt/tests/prop_font/main.rs:117-120` の `#[test] fn non_negative()` も本 issue で削除する (kern 適用後の advance 総和の非負性は使用フォントの kern 値域に依存し、proptest で生成された文字列で偶発的に負値となるケースを許容しないため)
- 0026 で `Font::shape()` 経路に対する結合性・非負性類似 PBT を再導入するか否かは 0026 polish で確定する (本 issue は 0026 の判断を拘束しない)
- 0024 で追加された `tests/test_context.rs` の `context_stroke_text_matches_manual` の手動再実装ロジックを本 issue でカーニング適用済みに揃える

### 値の意味的変化と CHANGE 種別

- `Font::measure_text` の `TextMetrics.advance` の値の意味が「カーニング非適用の単体 advance 総和」から「Microsoft OpenType `kern` v0 適用済み」に変化する (`CHANGE` 種別)
- `Font::measure_text` の結合性 `measure_text(a + b).advance == measure_text(a).advance + measure_text(b).advance` の不変条件を本 issue で取り下げる (隣接ペアにカーニングが適用されるため公開 API としては保証しない。`CHANGE` 種別)
- `Context::fill_text` / `Context::stroke_text` の描画結果のグリフ間隔がカーニング適用後の advance に変わる (`(A, V)` 等の典型ペアで差分テスト `tolerance = 0` で確実に fail するため `CHANGE` 種別)

0001 メタ issue「font 公開 API の安定化方針」セクションで「戻り値の意味変化・描画結果の差は `CHANGES.md` の `## develop` のトップ階層に `CHANGE` 種別で記載する」と確定済みの規則に従う。

## 完了条件

### 追加される API

- `FontFace::kern(glyph_id1: u16, glyph_id2: u16) -> i16` (デザインユニット)
- `Font::kern(glyph_id1: u16, glyph_id2: u16) -> f64` (スケール済みピクセル)

### 既存 API の挙動変化

- `Font::measure_text` の `TextMetrics.advance` の意味が「カーニング非適用」から「`kern` テーブル適用済み」に変わる (`CHANGE` 種別)
- `Font::measure_text` の結合性保証を取り下げる (`CHANGE` 種別)
- `Context::fill_text` の描画結果のグリフ間隔がカーニング適用後の advance に変わる (`CHANGE` 種別)
- `Context::stroke_text` の描画結果のグリフ間隔がカーニング適用後の advance に変わる (`CHANGE` 種別、0024 で追加された `stroke_text` への後追い改修を本 issue のスコープに含む)

### docstring 更新

- `Font::measure_text` の docstring (`/Users/voluntas/shiguredo/raden/src/font/mod.rs:200-208`) のうち、`mod.rs:203` の「カーニングは適用されない (`kern` テーブルは未対応)。」の **1 文のみ** を「Microsoft OpenType `kern` v0 (Format 0 / horizontal) によるカーニングを隣接グリフペアに適用した advance を返す。詳細は `Font::kern` 参照。」に置換する。`mod.rs:203` 行頭の「総アドバンスを格納する。」および空行前後 (`mod.rs:200-202`、`mod.rs:204-208`) は維持する
- `glyph_run_for_text` の docstring (`/Users/voluntas/shiguredo/raden/src/font/mod.rs:180-190`) は現状のまま維持

### 着手前提

- 0022 が close 済
- 0024 が close 済 (`stroke_text` への適用は 0024 で `stroke_text` が存在する状態でないと完了できないため、必ず 0024 close 後に着手する。0001 メタ issue「依存関係」セクションでは「0025 は `stroke_text` 以外について 0024 と並行着手可能」とあるが、本 issue は 3 経路を 1 PR で同時改修する方針のため 0024 close 済を厳密な着手前提とする。本 issue close PR で 0001 メタ issue 該当行も「0024 → 0025 直列」に同期更新する責務を持つ)
- 0001 メタ issue の前提 concrete issue 群 (fuzzing 基盤 / ベースライン benchmark 計測基盤 / Compound Glyph point-matching 実装) が close 済
- 「テスト用フォント選定」tracked の段階的選定 (0001 メタ issue) のうち、**kern 付きフォントの追加選定が完了し配置場所が決定済** (tracked 全体の close は 0026 / 0027 のフォント追加にも依存するため本 issue 着手前提には含めない。kern 段階のみ完了していれば足りる)。kern テーブル付きフォントの配置場所 (`tests/fixtures/` 等) は当該追加選定で確定する

### 開発初期テスト未カバーリスクの許容

kern 付きフォントが未配置のまま本 issue を着手すると単体テスト 3 件 (`font_kern_known_pair` / `font_kern_no_pair` / `font_measure_text_with_kerning`) が全件スキップされ、PBT「隣接ペア加算の関係」も `Font::kern == 0` のケース (kern 不在環境) で `measure_text == 単体 advance 総和` に縮退し、`parse_kern` ロジックの正しさが `parse_all` 経由の fuzz target (クラッシュ耐性のみ) しか検証されない状態になる。

本 issue ではこのリスクを許容しない方針として、着手前提 (kern 付きフォントの追加選定完了) を厳密に守る。開発手順上、本 issue 着手の最初の段階で kern 付きフォントの配置を確認すること。配置がまだなら「テスト用フォント選定 tracked」の kern 段階完了を先に進める。

### close 前提

- 上記着手前提を満たし、本 issue 内で `fill_text` / `measure_text` / `stroke_text` 3 経路へのカーニング適用、0022 PBT 削除、0024 テスト更新が完了し、CI でテストがパス

### ドキュメント

- `docs/BLEND2D.md` L449 の raden 列に新規 API 名を追記 (状態列は本 issue で変更しない)
- `CHANGES.md` に `CHANGE` 4 件・`ADD` 2 件を追加 (詳細は解決方法 7 参照)

### Fuzzing

不正な `kern` テーブル (length 不正、coverage 不正、n_pairs オーバーフロー等) に対するクラッシュ耐性は、前提 concrete issue「fuzzing 基盤と既存コード fuzz target」で追加された `parse_all` 経由の fuzz target で自動的にカバーされる (`parse_all` 内から `parse_kern` が呼ばれるため、フォントファイル全体のバイト列を入力とする target に kern バイトも含まれる)。新規 fuzz target は本 issue では追加しない。

## 解決方法

raden は Microsoft OpenType `kern` テーブル v0 (Format 0 / horizontal) に対応しない方針を確定し、本 issue を closed にする。

### 対応しない理由

- **GPOS Pair Adjustment が現代 OpenType の事実上の標準**: HarfBuzz / FreeType / Blend2D もカーニングは GPOS Pair Adjustment (Lookup Type 2) を主経路に置く。raden は 0026 (OpenType 基本シェーピング機能を追加する) で GPOS Pair Adjustment を実装することで、文字列描画時のカーニング機能を十分にカバーできる。
- **kern v0 を持つ現代フォントが稀**: Adobe Fonts (Source Sans 3 / Source Serif 4) と Google Fonts (Lato 等) はいずれも GPOS Pair Adjustment のみで `kern` テーブル v0 を持たない (本 polish 時の実機ダンプで確認)。`kern` v0 を持つフォントは DejaVu / Liberation 等の legacy 寄りに限られ、いずれも zip / tar.gz 配布で `.ttf` 直 URL を提供せず、テスト基盤整備のコスト (アーカイブ展開対応 fetch helper、legacy フォント選定、ライセンス検証) が本 issue 本来の規模を実質倍化させる。
- **本筋からの逸脱**: legacy kern v0 を維持するためのテスト基盤投資は、raden の本筋 (Blend2D 互換の現代的 2D レンダリングライブラリ) から逸脱した投資である。0026 の GPOS Pair Adjustment が「文字列描画でカーニングが効く」要件を満たすため、kern v0 への追加対応は実用上の不足を生まない。

### 後続作業

本 issue を closed にすることに伴う以下の書き換えは、0026 polish のスコープで扱う。

- `issues/0026-add-opentype-basic-shaping.md`: 「`FontFace::kern` / `Font::kern` を 0025 で提供する前提」を「GPOS Pair Adjustment 経路で完結する」に書き換える。
- `issues/0001-enhance-font-module-maturity.md`: 「依存関係」「Blend2D 対応マトリクス」「concrete issue 一覧」から 0025 を除外する。
- `docs/BLEND2D.md` L449 `BLFont::apply_kerning` 行の raden 列を「`kern` 非対応 (GPOS Pair Adjustment は 0026 で実装)」に確定する。

`CHANGES.md` は本 issue では更新しない (実装変更がないため)。
