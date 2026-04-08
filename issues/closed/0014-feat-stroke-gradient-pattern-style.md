# ストロークにグラデーション/パターンスタイルを追加する

Created: 2026-04-09
Completed: 2026-04-09
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

## 解決方法

- `Context` と `ContextState` に `stroke_gradient: Option<Gradient>` / `stroke_pattern: Option<Pattern>` を追加し、`save` / `restore` で他のスタイル状態と一緒にスタックする
- `set_stroke_style_gradient(&Gradient)` / `set_stroke_style_pattern(&Pattern)` を追加し、もう一方を排他的に `None` にする (fill 側と同じ規則)
- `set_stroke_style(Rgba32)` は単色設定時にグラデーション/パターンをクリアするよう変更
- `stroke_gradient()` / `stroke_pattern()` getter を追加し、fill 側 getter と対称化
- `stroke_path` 内で `stroke_to_fill_with_workspace` 後に fill スタイル (color / gradient / pattern) を一時退避し、stroke スタイルと差し替えてから `fill_path` を呼ぶ。これにより既存の fill 側ディスパッチ (単色 JIT / グラデ / パターン) をそのまま再利用する
- `tests/test_stroke.rs` にモジュール `stroke_style_gradient_pattern` を追加し、リニアグラデーションが線に沿って色変化すること、パターンが Repeat で適用されること、`set_stroke_style` がグラデ/パターンをクリアすること、`save` / `restore` が復元することを検証
- `docs/BLEND2D.md` の該当行と課題一覧 15、`CHANGES.md` を更新
