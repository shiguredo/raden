# OpenType 基本シェーピング機能を追加する

Created: 2026-05-11
Model: Kimi K2.6

## 根拠

現状のテキスト描画は cmap による文字→グリフ変換と advance の累積のみであり、OpenType の高度なレイアウト機能（GSUB/GPOS）が適用されていない。リガチャ（fi → ﬁ 等）、コンテクスチュアルサブスティテューション、マーク配置等が行われないため、ラテン文字以外の品質が著しく低下する。Blend2D では `shape()` / `apply_gsub()` / `apply_gpos()` を提供している。

## 概要

`GSUB` テーブルの基本 Lookup Type（Single, Multiple, Ligature, Alternate）と `GPOS` テーブルの基本 Lookup Type（Single Adjustment, Pair Adjustment）をサポートし、テキスト描画パイプラインに統合する。

## 現状の問題

- リガチャが適用されない（例: "fi" が個別の "f" + "i" のまま）
- アラビア語・ヒンディー語等の複雑スクリプトは未対応（本 issue のスコープ外）
- `BLGlyphBuffer` 相当の中間バッファがない

## 対応内容

1. `GlyphBuffer` 構造体を定義する
   - 文字列（UCS4 コードポイント列）またはグリフ ID 列を保持
   - 各グリフの `advance` / `offset` を保持
2. `Font` に `shape(text: &str) -> GlyphBuffer` を追加する
   - cmap で文字→グリフ変換
   - GSUB でグリフ置換（リガチャ等）
   - GPOS でグリフ位置調整（マーク配置等は後回し）
3. `Context` の `fill_text` / `stroke_text` / `measure_text` を `shape` を使う形に変更する
4. `FontFeatureSettings` 構造体を追加し、features を on/off できるようにする（最低限 "kern", "liga" 等）
5. PBT で「shape 後のグリフ数 ≤ shape 前の文字数」等の不変条件を検証する

## 非対応（将来の課題）

- GSUB/GPOS の Context / Chaining Context Lookup
- マーク配置（MarkToBase, MarkToMark 等）
- 複雑スクリプト（Arabic, Indic 等）
- 双方向テキスト（BiDi）

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
- テスト: `tests/test_font.rs` / `pbt/tests/prop_font.rs`
- 依存: 0025-add-font-kerning（GPOS の Pair Adjustment と関連）
- 0001-enhance-font-module-maturity.md でも言及されている課題
