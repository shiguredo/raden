# カーニング適用機能を追加する

- Priority: Medium
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-font-kerning

## 目的

グリフペア間のカーニングを適用し、プロポーショナルフォントでのテキスト描画品質を向上させる。

## 現状

- `measure_text()` も `fill_text()` もカーニングを考慮していない
- kern テーブル（legacy）は未パース
- GPOS テーブルは未パースまたは未利用

## 設計方針

- `kern` テーブル（legacy）を新規パースする。フォーマットは Format 0（グリフペアのサブテーブル）をサポート
- GPOS テーブルの Pair Adjustment（Lookup Type 2）は 0026 で実装し、本 issue では `kern` テーブルのみを対象とする
- カーニング量はデザインユニットで保持し、`Font` レベルでスケール済み値を返す
- `fill_text` / `measure_text` / `stroke_text` 内でカーニングを適用する
- `FontFeatureSettings` の "kern" feature が off の場合はカーニングを適用しない（0026 で実装）

## 完了条件

- `Font::kern()` / `Font::kern_scaled()` でグリフペアのカーニング量が取得できる
- `fill_text` / `measure_text` / `stroke_text` でカーニングが適用されている
- 単体テストと PBT で正しさを検証している

## 解決方法

1. `kern` テーブルを新規パースする
   - Format 0（グリフペアのサブテーブル）をサポート
   - カバレッジビット（horizontal/vertical/minimum/cross-stream/override）を考慮
   - グリフペアの検索は線形探索で実装し、大きなフォントでの性能は今後検討
2. `Font` に以下を追加する
   - `kern(glyph_id1: u16, glyph_id2: u16) -> f64`（デザインユニットでのカーニング量）
   - `kern_scaled(glyph_id1: u16, glyph_id2: u16) -> f64`（スケール済み）
3. `fill_text` / `measure_text` / `stroke_text` 内でカーニングを適用する
   - グリフを左から右へ配置する際、前後のグリフペアに対して `kern_scaled` を加算する
4. PBT で「カーニング適用後の advance ≥ カーニング適用前の advance」等の不変条件を検証する
5. Fuzzing で不正な kern テーブルに対するクラッシュ耐性を検証する

## 変更対象ファイル

- `src/font/tables.rs`: kern テーブルパースの追加
- `src/font/mod.rs`: `kern()` / `kern_scaled()` の追加
- `src/api/context.rs`: `fill_text` / `measure_text` / `stroke_text` へのカーニング適用
- `tests/test_font.rs`: 単体テストの追加
- `pbt/tests/prop_font/main.rs`: PBT の追加
- `fuzz/`: Fuzzing ターゲットの追加
- `docs/BLEND2D.md`: Font API セクションの更新

## エッジケース

- kern テーブル不在: カーニング量 0.0
- kern テーブルが Format 0 以外: カーニング量 0.0
- `size=0`: スケール済みカーニング量 0.0

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
- テスト: `tests/test_font.rs` / `pbt/tests/prop_font/main.rs` / `fuzz/`
- 依存: 0022-add-text-measurement（measure_text に反映）
- 0001-enhance-font-module-maturity.md でも言及されている課題
