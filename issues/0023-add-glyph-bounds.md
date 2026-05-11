# グリフ境界ボックス取得機能を追加する

- Priority: High
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-glyph-bounds

## 目的

個別グリフの境界ボックスを取得できるようにし、グリフクリッピングや精密なテキストレイアウトを可能にする。

## 現状

- `append_glyph_outline(glyph_id, offset_x, offset_y, &mut Path)` は Path を構築するため、単に bbox が欲しい場合にオーバーヘッドが大きい
- glyf テーブルの各グリフヘッダ（Simple/Compound 共通）に bbox（xMin, yMin, xMax, yMax）が記録されているが、公開 API からアクセスできない
- 文字列描画時のクリッピング判定等で個別グリフの bbox が必要

## 設計方針

- glyf テーブルのグリフヘッダから直接 bbox を読み出す（Simple/Compound 共通）
- `font` モジュール内に `GlyphBounds` 構造体を定義し、`api::context::Rect` への依存を避ける
- スケール済み値はデザイン値に `Font::scale()` を乗じて計算する

## 完了条件

- `FontFace::glyph_bounds()` / `Font::glyph_bounds()` でグリフの bbox が取得できる
- glyf ヘッダから直接読み出しており、`append_glyph_outline` より高速である
- 単体テストと PBT で正しさを検証している

## 解決方法

1. `font` モジュール内に `GlyphBounds` 構造体を定義する
   - `x_min: f64`, `y_min: f64`, `x_max: f64`, `y_max: f64`
2. `FontFace` に以下のメソッドを追加する
   - `glyph_bounds(glyph_id: u16) -> Option<GlyphBounds>`（design units）
3. `Font` にスケール済み版を追加する
   - `glyph_bounds(glyph_id: u16) -> Option<GlyphBounds>`（design 値に `scale()` を乗じる）
4. 実装方針
   - glyf テーブルからグリフエントリを読み、ヘッダの xMin/yMin/xMax/yMax を返す
   - Simple Glyph と Compound Glyph の両方でヘッダに bbox が記録されている
   - `append_glyph_outline` と glyf_start/glyf_end の計算部分を共通ヘルパーに抽出する
   - 存在しない glyph_id の場合は `None`
   - 空グリフ（`glyf_start == glyf_end`）の場合は `Some(GlyphBounds{0,0,0,0})`
5. PBT で「glyph_bounds の範囲内に append_glyph_outline の結果が収まる」ことを検証する
   - `Path::control_box()` を使って bbox を取得し、包含関係を検証
   - `control_box()` は制御点ベースの近似であるため、厳密な包含ではなく「glyph_bounds が control_box を含むか、または近似的に一致する」を検証

## 変更対象ファイル

- `src/font/mod.rs`: `GlyphBounds` 構造体、`glyph_bounds()` の追加
- `src/font/glyph.rs`: グリフヘッダから bbox を読むヘルパーの追加
- `tests/test_font.rs`: 単体テストの追加
- `pbt/tests/prop_font/main.rs`: PBT の追加
- `docs/BLEND2D.md`: Font API セクションの更新

## エッジケース

- `glyph_id` が `num_glyphs` 以上: `None`
- 空グリフ: `Some(GlyphBounds{0,0,0,0})`
- `size=0`: スケール済み値はすべて 0.0

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
