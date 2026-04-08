# 楕円 (Ellipse) を追加する

Created: 2026-04-09
Model: Opus 4.6

## 概要

`Circle` はあるが `Ellipse` がない。Blend2D は `BLEllipse` および `fill_ellipse` / `stroke_ellipse` / `Path::add_ellipse` を持つ。

## 根拠

- 円があって楕円がない状態は API の欠落感が強い
- 内部的には Path への 4 本のキュービックベジェ近似で実装可能で、`add_circle` 実装と対称化できる
- ダミー映像のレイアウト要素として頻出

## 想定スコープ

- `Ellipse { cx, cy, rx, ry }` 型を追加
- `Path::add_ellipse(cx, cy, rx, ry)` を追加（`add_circle` と同じ近似手法）
- `Context::fill_ellipse(&Ellipse)` / `Context::stroke_ellipse(&Ellipse)` を追加
- PBT: `rx == ry` のとき `Circle` と一致すること、ラウンドトリップ
- `docs/BLEND2D.md` および `CHANGES.md` の更新
