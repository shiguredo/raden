# blit_image 系 API を追加する

Created: 2026-04-01
Model: Composer

## 概要

Blend2D の `blit_image` に相当する、ソース `Image` をデスティネーションへ転送・矩形指定・スケーリング転送する描画 API を `Context` に追加する。

## 根拠

スプライト合成、オフスクリーンバッファの貼り付け、別解像度バッファとの合成など、2D レンダリングで頻繁に必要になる。現状はパターン経由などの迂回に頼ることになり、機能としての穴が大きい（`docs/BLEND2D.md` 課題一覧にも記載）。

## 大枠の作業

- 転送矩形・ソース矩形・合成モード・クリップとの整合を定義する（Blend2D の意味に準拠）
- パイプラインまたは既存の画像サンプリング経路との接続方針を決め、実装する
- 代表ケースの単体テスト（同一サイズコピー、スケール、部分矩形）と `docs/BLEND2D.md` の更新

Completed: 2026-04-01

## 解決方法

- `src/api/blit.rs` に `blit_image_rect_scoped` を実装し、`Context::blit_image_rect` / `blit_image_at` から呼び出す。
- 逆行列でデバイス→ユーザ空間に戻し、宛て先矩形とソース矩形の線形写像で Nearest サンプリングする。`CompOp` は `SrcOver` / `SrcCopy` のみ。
- `docs/BLEND2D.md` の Blit 操作表と課題一覧を更新した。
