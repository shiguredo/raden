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

- [ADD] Context に矩形クリッピング API (`clip_to_rect` / `restore_clipping`) を追加する
  - @voluntas
- [ADD] Context に描画状態の取得 API (`comp_op` / `fill_rule` / `stroke_*` / `fill_gradient` / `fill_pattern` / `matrix` 等) を追加する
  - @voluntas
- [ADD] Context に `rotate_around` / `skew` / `post_translate` / `post_scale` / `post_rotate` / `post_skew` / `post_transform` を追加する (`Matrix2D` への委譲)
  - @voluntas
- [ADD] `Matrix2D` に `skewing` / `skew` / `post_skew` を追加する
  - @voluntas

### misc
