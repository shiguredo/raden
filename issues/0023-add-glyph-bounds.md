# グリフ境界ボックス取得機能を追加する

Created: 2026-05-11
Model: Kimi K2.6

## 概要

個別グリフの境界ボックス（bounding box）を取得する API がない。グリフクリッピングや精密なテキストレイアウトには必須である。

## 根拠

テキストを「指定した矩形内に収める」「個別グリフのサイズに基づいてカーソルを移動する」等の用途で、グリフ単位の bounding box が必要になる。Blend2D では `get_glyph_bounds()` でこれを提供している。特に `append_glyph_outline` で取得できるアウトラインから bbox を計算することは可能だが、呼び出し側が毎回 Path を構築するのは非効率である。

## 現状の問題

- `append_glyph_outline(glyph_id, offset_x, offset_y, &mut Path)` は Path を構築するため、単に bbox が欲しい場合にオーバーヘッドが大きい
- glyf テーブルから直接 bbox を読めるケース（Simple Glyph / Compound Glyph のヘッダに記載）があるが、公開 API からアクセスできない
- 文字列描画時のクリッピング判定等で個別グリフの bbox が必要

## 対応内容

1. `FontFace` に以下のメソッドを追加する
   - `glyph_bounds(glyph_id: u16) -> Option<Rect>`
2. 実装方針
   - glyf テーブルからグリフエントリを読み、ヘッダに記録された bbox（xMin, yMin, xMax, yMax）を返す
   - Compound Glyph の場合は再帰的に子グリフの bbox を合成して返す
   - 存在しない glyph_id の場合は `None`
3. `Font` にスケール済み版を追加する
   - `glyph_bounds(glyph_id: u16) -> Option<Rect>`（design 値に `scale()` を乗じる）
4. PBT で「glyph_bounds の範囲内に append_glyph_outline の結果が収まる」ことを検証する

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
- テスト: `tests/test_font.rs` / `pbt/tests/prop_font.rs`
