# 角丸矩形 (RoundRect) を追加する

Created: 2026-04-09
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
