# 変更履歴

- UPDATE
  - 後方互換がある変更
- ADD
  - 後方互換がある追加
- CHANGE
  - 後方互換のない変更
- FIX
  - バグ修正

## develop

- [ADD] `RoundRect` 型と `Context::fill_round_rect` / `stroke_round_rect`、`Path::add_round_rect` を追加する
  - @voluntas
- [ADD] `Ellipse` 型と `Context::fill_ellipse` / `stroke_ellipse`、`Path::add_ellipse` を追加する
  - @voluntas
- [ADD] Context に `clear_all` / `clear_rect` を追加し、現在のクリップ領域内をピクセル値 0 で直接クリアできるようにする
  - @voluntas
- [ADD] Context に `set_stroke_style_gradient` / `set_stroke_style_pattern` と `stroke_gradient` / `stroke_pattern` getter を追加し、ストロークでもグラデーション/パターンを使用できるようにする
  - @voluntas
- [UPDATE] `Pattern` の `prepare` にコンテキスト行列を渡し、`PatternFilter`（Nearest / Bilinear）、`set_transform` によるアフィンと `set_origin` による原点指定をサポートする
  - @voluntas
- [ADD] Context に `blit_image_rect` / `blit_image_at` を追加し、逆行列に基づく Nearest サンプリングで `SrcOver` / `SrcCopy` 転送を行う
  - @voluntas
- [ADD] `PixelFormat` に `Xrgb32` と `A8` を追加し、宛て先形式に応じた A8 パイプラインを追加する
  - @voluntas
- [ADD] `Path` に `smooth_quad_to` / `smooth_cubic_to` / `conic_to` / `arc_to` と `PathCmd::ConicTo`、ストローク・ラスタライズの平坦化を追加する
  - @voluntas
- [ADD] Context に矩形クリッピング API (`clip_to_rect` / `restore_clipping`) を追加する
  - @voluntas
- [ADD] Context に描画状態の取得 API (`comp_op` / `fill_rule` / `stroke_*` / `fill_gradient` / `fill_pattern` / `matrix` 等) を追加する
  - @voluntas
- [ADD] Context に `rotate_around` / `skew` / `post_translate` / `post_scale` / `post_rotate` / `post_skew` / `post_transform` を追加する (`Matrix2D` への委譲)
  - @voluntas
- [ADD] `Matrix2D` に `skewing` / `skew` / `post_skew` を追加する
  - @voluntas

### misc

- [UPDATE] CI / Release の Slack 通知を `shiguredo/github-actions` の `slack-notify`（Composite Action）に切り替える
  - @voluntas
- `EdgeBuilder::flatten` にコニック重み列と `PathCmd::ConicTo` 件数の `debug_assert` を追加し、`README` / `docs/BLEND2D.md` に `fill_rect` と `comp_op` の関係を記載する
  - @voluntas
- パターンの `fill_rect` で `CompOp`（`SrcOver` / `SrcCopy`）を反映し、`blit` の退化矩形を早期リターンし、ストロークのコニック重みに `debug_assert` を追加する
  - @voluntas
- `src/pipeline/a8.rs` の Clippy の不要キャストを削除する
  - @voluntas
