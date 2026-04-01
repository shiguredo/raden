# Context に Blend2D 相当の getter を追加する

Created: 2026-04-01
Model: Composer

## 概要

`Context` について、`comp_op` / `fill_rule` / `stroke_width` / `stroke_miter_limit` / `stroke_join` / ストロークキャップ / ダッシュ関連など、現在は setter のみまたは未公開の状態を取得できる API を追加する。

## 根拠

Blend2D からの移植や、状態を保存して復元するコードでは現在の合成モード・塗りつぶしルール・ストローク設定を読み取る必要がある。getter がないとデバッグやラッパー実装のコストが高い。パイプライン JIT の本質を変えず、内部状態の読み出しを公開するだけで済む。

## 大枠の作業

- 対象となる状態フィールドを列挙し、Blend2D の対応 API と名前・意味を揃える
- 公開 API とドキュメント（`docs/BLEND2D.md`）の更新
- 取得値が setter と一致することを単体テストまたは PBT で検証

Completed: 2026-04-01

## 解決方法

- `Context` に `comp_op` / `fill_rule` / `fill_color_prgb32` / `fill_gradient` / `fill_pattern` / `stroke_color_prgb32` / `stroke_width` / `stroke_miter_limit` / `stroke_join` / `stroke_start_cap` / `stroke_end_cap` / `stroke_dash_array` / `stroke_dash_offset` / `matrix` を追加した。
- `tests/test_context.rs` で setter と getter の一致を検証した。
- `docs/BLEND2D.md` の該当表を更新した。
