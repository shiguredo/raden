# PixelFormat に Xrgb32 と A8 を追加する

Created: 2026-04-01
Model: Composer

## 概要

Blend2D の `BLFormat` にある `Xrgb32`（アルファなし 32bpp）と `A8`（アルファのみ）に相当する `PixelFormat` variant と、`Image`・レンダリングパイプラインとの整合を追加する。

## 根拠

他ライブラリやフォーマットとのバッファ共有、マスク専用バッファ、RGB 固定表現での転送で必要になる。現状 `Prgb32` のみでは連携の選択肢が狭い。

## 大枠の作業

- フォーマット定義とストライド・ピクセルサイズの規約
- 変換経路（`premultiply` や `blit` との関係）の有無
- 対応する読み書き・テスト（フォーマットごとの最小ケース）
- `docs/BLEND2D.md` の更新

Completed: 2026-04-01

## 解決方法

- `PixelFormat` に `Xrgb32` と `A8` を追加し、`bytes_per_pixel` を実装した。
- `pipeline::a8` で A8 宛てのスカラ合成を提供し、`PipelineRuntime` で形式に応じて分岐した。JIT キーは `Xrgb32` を `Prgb32` に正規化して共有する。
- `Context` のオフセット計算を `bytes_per_pixel()` ベースにし、A8 ではパターン・グラデ・`fill_path` を制限した。
- `docs/BLEND2D.md` の `BLFormat` 行と課題一覧を更新した。
