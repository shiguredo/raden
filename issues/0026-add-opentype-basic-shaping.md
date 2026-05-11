# OpenType 基本シェーピング機能を追加する

- Priority: Medium
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-opentype-basic-shaping

## 目的

OpenType の GSUB/GPOS レイアウト機能を適用し、リガチャ等の高度なテキスト表現を可能にする。

## 現状

- テキスト描画は cmap による文字→グリフ変換と advance の累積のみ
- リガチャが適用されない（例: "fi" が個別の "f" + "i" のまま）
- `BLGlyphBuffer` 相当の中間バッファがない

## 設計方針

- `GlyphBuffer` 構造体を定義し、シェーピング結果を保持する
- GSUB の基本 Lookup Type（Single, Multiple, Ligature, Alternate）をサポート
- GPOS の基本 Lookup Type（Single Adjustment, Pair Adjustment）をサポート
- `FontFeatureSettings` 構造体を追加し、features を on/off できるようにする
- 複雑スクリプト（Arabic, Indic 等）と BiDi は本 issue のスコープ外とする

## 完了条件

- `Font::shape(text)` でシェーピング結果（`GlyphBuffer`）が取得できる
- リガチャ（"fi" → "ﬁ" 等）が適用される
- `fill_text` / `stroke_text` / `measure_text` でシェーピング結果が使用される
- 単体テストと PBT で正しさを検証している

## 解決方法

1. `GlyphBuffer` 構造体を定義する
   - グリフ ID 列を保持
   - 各グリフの `advance` / `offset` を保持
2. `Font` に `shape(text: &str) -> GlyphBuffer` を追加する
   - cmap で文字→グリフ変換
   - GSUB でグリフ置換（リガチャ等）
   - GPOS でグリフ位置調整（マーク配置等は後回し）
3. `Context` の `fill_text` / `stroke_text` / `measure_text` を `shape` を使う形に変更する
4. `FontFeatureSettings` 構造体を追加し、features を on/off できるようにする（最低限 "kern", "liga" 等）
5. PBT で「shape 後のグリフ数 ≤ shape 前の文字数」等の不変条件を検証する

## 変更対象ファイル

- `src/font/tables.rs`: GSUB/GPOS テーブルパースの追加
- `src/font/mod.rs`: `GlyphBuffer` / `shape()` / `FontFeatureSettings` の追加
- `src/api/context.rs`: `fill_text` / `stroke_text` / `measure_text` のシェーピング対応
- `tests/test_font.rs`: 単体テストの追加
- `pbt/tests/prop_font/main.rs`: PBT の追加
- `docs/BLEND2D.md`: Font API セクションの更新

## 非対応（将来の課題）

- GSUB/GPOS の Context / Chaining Context Lookup
- マーク配置（MarkToBase, MarkToMark 等）
- 複雑スクリプト（Arabic, Indic 等）
- 双方向テキスト（BiDi）

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
- テスト: `tests/test_font.rs` / `pbt/tests/prop_font/main.rs`
- 依存: 0025-add-font-kerning（GPOS の Pair Adjustment と関連）
- 0001-enhance-font-module-maturity.md でも言及されている課題
