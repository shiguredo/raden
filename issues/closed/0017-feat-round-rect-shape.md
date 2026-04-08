# 角丸矩形 (RoundRect) を追加する

Created: 2026-04-09
Completed: 2026-04-09
Model: Opus 4.6

## 概要

Blend2D の `BLRoundRect` 相当が未実装。`fill_round_rect` / `stroke_round_rect` / `Path::add_round_rect` がない。

## 根拠

- UI 風要素やダミー映像の装飾で頻出する基本形状
- Path への変換は直線 4 本 + 4 隅のキュービックベジェ近似で済むため実装コストは低い
- Blend2D 互換性の穴を埋める

## 想定スコープ

- `RoundRect { x, y, w, h, rx, ry }` 型を追加
- `Path::add_round_rect(&RoundRect)` を追加
- `Context::fill_round_rect(&RoundRect)` / `Context::stroke_round_rect(&RoundRect)` を追加
- 半径が幅/高さの半分を超えた場合のクランプ仕様を Blend2D に合わせる
- 単体テスト: `rx == ry == 0` で通常の矩形と一致、半径が極端な値の境界条件
- PBT: ラウンドトリップ、バウンディングボックスが元の矩形と一致
- `docs/BLEND2D.md` および `CHANGES.md` の更新

## 解決方法

- `RoundRect { x, y, w, h, rx, ry }` 型を追加し `lib.rs` から再エクスポート
- `Path::add_round_rect(x, y, w, h, rx, ry)` を追加
  - `rx` / `ry` を `[0, w/2]` / `[0, h/2]` でクランプ (Blend2D 仕様)
  - 半径が 0 の場合は通常の矩形 (4 本の `line_to` + `close`) として追加
  - それ以外は 4 辺の `line_to` と 4 隅の `cubic_to` (kappa 近似) で構成
- `Context::fill_round_rect` / `stroke_round_rect` を追加 (内部で `add_round_rect` → `fill_path` / `stroke_path`)
- `tests/test_context.rs` の `round_rect` モジュールで以下を検証:
  - `rx==ry==0` のとき `fill_rect` と完全一致
  - 大きすぎる半径が幅/高さの半分にクランプされ、4 隅が透明になる
  - 通常の半径指定で 4 隅が透明、辺の中点が塗りつぶし
  - `add_round_rect(0, 0)` のコマンド数が 5 (move + 3 line + close)
- `docs/BLEND2D.md` の該当行 4 箇所と `CHANGES.md` を更新
