# Path に楕円弧・象限弧を追加する

- Priority: Medium
- Created: 2026-04-01
- Model: Composer

## 目的

`Path` に Blend2D 相当の `elliptic_arc_to` / `arc_quadrant_to` を追加し、SVG パスとの互換性を高める。

## 根拠

SVG パスや既存ベクタアセットとの互換性、および Blend2D との API 準拠。

## 実装済み (テスト未作成)

以下の機能は `src/api/path.rs` にコードがあるが、PBT テストが未作成:

- `smooth_quad_to(x, y)`: スムーズ二次ベジェ (path.rs:135)
- `smooth_cubic_to(cp2x, cp2y, x, y)`: スムーズ三次ベジェ (path.rs:145)
- `conic_to(cx, cy, ex, ey, w)`: 円錐曲線 (path.rs:155)
- `arc_to(cx, cy, rx, ry, start, sweep, force_move_to)`: 円弧 (path.rs:169)
- `PathCmd::ConicTo` と `conic_weights`

注意: `smooth_quad_to` / `smooth_cubic_to` は直前コマンドが不正な場合にパニックする (`expect`)。SVG 仕様では制御点 = 現在点として動作すべきだが、現在の実装はパニックする。

## 未実装

### `arc_quadrant_to(x1, y1, x2, y2)`

象限単位の円弧 (90 度以下)。Blend2D の `BLPath::arc_quadrant_to` 相当。角度が 90 度を超える場合の扱いは Blend2D の動作を確認して定義する。

### `elliptic_arc_to(rx, ry, x_rotation, large_arc, sweep_flag, x1, y1)`

楕円弧 (SVG arc コマンド相当)。Blend2D の `BLPath::elliptic_arc_to` 相当。`large_arc` / `sweep_flag` の 4 パターンのエッジケースを明確に定義する。

## 完了条件

- `arc_quadrant_to` が実装されている
- `elliptic_arc_to` が実装されている
- `arc_to` / `smooth_quad_to` / `smooth_cubic_to` / `conic_to` の PBT が `pbt/tests/prop_path.rs` に存在する
- `arc_quadrant_to` / `elliptic_arc_to` の PBT が存在する

## 変更対象ファイル

- `src/api/path.rs`: `arc_quadrant_to` / `elliptic_arc_to` の追加
- `pbt/tests/prop_path.rs`: PBT の追加

## エッジケース

- `elliptic_arc_to`: `rx == ry` の場合は円弧に帰着
- `elliptic_arc_to`: `rx` または `ry` が 0 の場合は線分に帰着
- `arc_quadrant_to`: 角度が 90 度を超える場合の扱い (Blend2D の動作を確認)
