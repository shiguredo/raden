# カーニング適用機能を追加する

Created: 2026-05-11
Model: Kimi K2.6

## 根拠

現状のテキスト描画は各グリフの advance を単純に累積するだけであり、文字間隔の微調整（カーニング）が適用されていない。プロポーショナルフォントではカーニングがないと「AV」「To」等の組み合わせで不自然な隙間が生じ、品質が著しく低下する。Blend2D では `apply_kerning()` で kern テーブルと GPOS の pair adjustment に対応している。

## 概要

`kern` テーブル（legacy）および GPOS テーブルの Pair Adjustment（Lookup Type 2）に基づいて、グリフペア間のカーニングを適用する。

## 現状の問題

- `measure_text()` も `fill_text()` もカーニングを考慮していない
- kern テーブルはパース済みのはずだが、公開 API からアクセスできない
- GPOS テーブルは未パースまたは未利用

## 対応内容

1. `kern` テーブルのパースを確認し、必要に応じて修正・拡張する
2. `GPOS` テーブルの Pair Adjustment（Lookup Type 2）をパースする
3. `Font` に以下を追加する
   - `kern(glyph_id1: u16, glyph_id2: u16) -> f64`（デザインユニットでのカーニング量）
   - `kern_scaled(glyph_id1: u16, glyph_id2: u16) -> f64`（スケール済み）
4. `fill_text` / `measure_text` / `stroke_text` 内でカーニングを適用する
   - グリフを左から右へ配置する際、前後のグリフペアに対して `kern_scaled` を加算する
5. PBT で「カーニング適用後の advance ≥ カーニング適用前の advance」等の不変条件を検証する
6. Fuzzing で不正な kern/GPOS テーブルに対するクラッシュ耐性を検証する

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
- テスト: `tests/test_font.rs` / `pbt/tests/prop_font.rs` / `fuzz/`
- 依存: 0022-add-text-measurement（measure_text に反映）
- 0001-enhance-font-module-maturity.md でも言及されている課題
