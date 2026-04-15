# 三角形とポリゴンを追加する

Created: 2026-04-09
Completed: 2026-04-09
Model: Opus 4.6

## 概要

`fill_triangle` / `stroke_triangle` / `fill_polygon` / `stroke_polygon` および対応する `Path::add_*` が未実装。

## 根拠

- ダミー映像のパターン生成（万華鏡的なタイル、デバッグ用幾何模様）で頻出
- `fill_geometry` のような汎用 API を将来追加する際の前段としても必要
- Path 経由で容易に実装できる

## 想定スコープ

- `Triangle { x0, y0, x1, y1, x2, y2 }` 型を追加
- `Path::add_triangle` / `Path::add_polygon(&[Point])` / `Path::add_polyline(&[Point])` を追加
- `Context::fill_triangle` / `stroke_triangle` / `fill_polygon` / `stroke_polygon` / `stroke_polyline` を追加
- PBT: 三角形は 3 点ポリゴンと一致、ポリゴンは閉じた Path と一致
- 退化ケース（同一点、共線）のテスト
- `docs/BLEND2D.md` および `CHANGES.md` の更新

## 解決方法

- `Triangle { x0, y0, x1, y1, x2, y2 }` 型を追加し `lib.rs` から再エクスポート
- `Path::add_triangle` / `add_polygon(&[Point])` / `add_polyline(&[Point])` を追加 (ポリゴンは閉じる、ポリラインは閉じない、3 点未満/2 点未満は no-op)
- `Context::fill_triangle` / `stroke_triangle` / `fill_polygon` / `stroke_polygon` / `stroke_polyline` を追加
- `tests/test_context.rs` の `triangle_polygon` モジュールで以下を検証:
  - `fill_triangle` と `fill_polygon` (3 点) のラスタ結果が完全一致
  - 2 点未満の `add_polygon` は no-op
  - 三角形の重心ピクセルが塗りつぶしされる
- `docs/BLEND2D.md` の該当行 8 箇所と `CHANGES.md` を更新
