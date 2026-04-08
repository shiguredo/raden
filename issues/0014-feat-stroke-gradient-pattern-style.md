# ストロークにグラデーション/パターンスタイルを追加する

Created: 2026-04-09
Model: Opus 4.6

## 概要

`Context::set_stroke_style` は現在 `Rgba32` のみ対応で、グラデーション/パターンを stroke に適用できない。fill 側はすでに `set_fill_style_gradient` / `set_fill_style_pattern` が実装済みのため、対称となる API を stroke にも追加する。

## 根拠

- Blend2D は `set_stroke_style(gradient/pattern)` を備えており、API 準拠の観点で穴になっている
- ダミー映像生成では線にグラデーションを乗せたい場面が多く、表現力に直結する
- `stroke_path` は内部で `stroke_to_fill` → `fill_path` の経路を取るため、fill 側のグラデ/パターン経路を再利用できる見込みが立っている (`docs/BLEND2D.md` のフィル/ストローク節を参照)

## 想定スコープ

- `Context` に `set_stroke_style_gradient` / `set_stroke_style_pattern` を追加
- 内部状態 (`StrokeStyle` 列挙) を新設し、`stroke_path` / `stroke_rect` / `stroke_circle` / `stroke_line` 経路で fill 側のスタイルディスパッチを共有する
- getter (`stroke_gradient()` / `stroke_pattern()`) を追加し、`fill_*` の getter と対称にする
- PBT: fill と stroke で同じグラデ/パターンを適用したときにラスタ結果が `stroke_to_fill` 後の fill と一致することを確認
- `docs/BLEND2D.md` および `CHANGES.md` の更新

## 注意点

- パターン `fill_rect` 高速パスが `SrcOver` / `SrcCopy` のみであることに合わせ、stroke 側も最初は同じ制約で良い
- グラデ `fill_path` は `comp_op` を `span_cov` に渡す経路。stroke 経由でも崩れないか確認する
