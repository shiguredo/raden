# Path に円弧・スムーズ曲線・コニック（必要なら）を追加する

Created: 2026-04-01
Model: Composer

## 概要

`Path` に Blend2D 相当の `arc_to` / `arc_quadrant_to` / `elliptic_arc_to` / `smooth_quad_to` / `smooth_cubic_to` などを追加する。`PathCmd` の `Conic` および `WEIGHT`（コニック用重み）を実データとして使う場合は、フラット化やレンダリングパスまで含めて設計する。

## 根拠

SVG パスや既存ベクタアセットとの互換性、および Blend2D との API 準拠。中期的には実装・テスト範囲が広がるが、表現力の中心になる。

## 進捗メモ (2026-04-01)

`smooth_quad_to` / `smooth_cubic_to` / `conic_to` / `arc_to`、`PathCmd::ConicTo`、ストローク・`EdgeBuilder` の平坦化は実装済み。`elliptic_arc_to` / `arc_quadrant_to` は未実装のまま。

## 大枠の作業

- コマンド列・頂点列の拡張と `PathCmd` の意味の確定（欠番だった Conic の扱い）
- フラットナーまたは直接ラスタライズまでのどこまでをスコープにするかの切り分け
- 幾何の単体テスト（既知の弧・SVG arc サンプル）と PBT での不変条件
- `docs/BLEND2D.md` の更新
