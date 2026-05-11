# カーニング適用機能を追加する

- Priority: Medium
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-font-kerning

## 目的

グリフペア間のカーニングを適用し、プロポーショナルフォント でのテキスト描画品質を向上させる。

## 現状

- `measure_text()` も `fill_text()` もカーニングを考慮していない
- kern テーブル（legacy）は未パース
- GPOS テーブルは未パースまたは未利用

## 設計方針

- `kern` テーブル（legacy）を新規パースする。フォーマットは Format 0（グリフペアのサブテーブル）をサポート
- GPOS テーブルの Pair Adjustment（Lookup Type 2）は 0026 で実装し、本 issue では `kern` テーブルのみを対象とする
- カーニング量はデザインユニットで保持し、`Font` レベルでスケール済み値を返す
- `fill_text` / `measure_text` / `stroke_text` 内でカーニングを適用する
- 本 issue ではカーニングは無条件で適用する。`FontFeatureSettings` による on/off 制御は 0026 で実装する

## 完了条件

- `FontFace::kern()` / `Font::kern()` でグリフペアのカーニング量が取得できる
- `fill_text` / `measure_text` / `stroke_text` でカーニングが適用されている
- 単体テストと PBT で正しさを検証している

## 解決方法

1. `kern` テーブルを新規パースする
   - Format 0（グリフペアのサブテーブル）をサポート
   - カバレッジビット（horizontal / vertical / minimum / cross-stream / override）を考慮
   - グリフペアの検索は線形探索で実装する
2. `FontFace` / `Font` に以下を追加する
   - `FontFace::kern(glyph_id1: u16, glyph_id2: u16) -> i16`（デザインユニットでのカーニング量、テーブル不在時は 0）
   - `Font::kern(glyph_id1: u16, glyph_id2: u16) -> f64`（スケール済み、テーブル不在時は 0.0）
3. `fill_text` / `measure_text` / `stroke_text` 内でカーニングを適用する
   - `glyph_run_for_text` (0022) で取得したグリフ列の隣接ペア `(glyph_id[i], glyph_id[i+1])` に対して `Font::kern(glyph_id[i], glyph_id[i+1])` を計算する
   - カーニング量を `advance[i]` に加算する（グリフ i の後にカーニング量を適用）
   - 最終的な advance 総和 = `sum(advance[i]) + sum(kern(glyph_id[i], glyph_id[i+1]))` となる
4. PBT で「カーニング適用後の advance 総和 = カーニング適用前の advance 総和 + 全隣接ペアのカーニング量の総和」の関係を検証する
5. Fuzzing で不正な kern テーブルに対するクラッシュ耐性を検証する
   - fuzzing ターゲットはフォントファイル全体のバイト列を入力とする

## 変更対象ファイル

- `src/font/tables.rs`: kern テーブルパースの追加
- `src/font/mod.rs`: `FontFace::kern()` / `Font::kern()` の追加
- `src/api/context.rs`: `fill_text` / `measure_text` / `stroke_text` へのカーニング適用
- `tests/test_font.rs`: 単体テストの追加
- `pbt/tests/prop_font/main.rs`: PBT の追加
- `fuzz/`: Fuzzing ターゲットの追加

## エッジケース

- kern テーブル不在: カーニング量 0.0
- kern テーブルが Format 0 以外: カーニング量 0.0
- `size=0`: スケール済みカーニング量 0.0

## 関連

- 0001-enhance-font-module-maturity.md
- テスト: `tests/test_font.rs` / `pbt/tests/prop_font/main.rs` / `fuzz/`
- 依存: 0022-add-text-measurement（`glyph_run_for_text` と `measure_text` の実装）
- 依存: 0024-add-stroke-text（`stroke_text` へのカーニング適用）
