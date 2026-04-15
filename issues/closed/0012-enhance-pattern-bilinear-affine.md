# Pattern の Bilinear 補間と Affine 変換を実装する

Created: 2026-04-01
Model: Composer

## 概要

現状の `Pattern`（Nearest 補間・並進のみ）に、Bilinear 補間および（Blend2D に合わせた）アフィン変換を追加する。パイプライン側のサンプリングと整合させる。

## 根拠

タイルパターンのジャギー低減や、回転・スケールを伴う塗りつぶしで必要になる。グラデーションと並ぶ、見た目品質に直結する拡張である。

## 大枠の作業

- `Pattern` のパラメータ表現（行列・補間モード）の設計
- JIT / 固定小数点パスでのサンプラ実装と既存 Nearest とのテスト分離
- 参照画像または数値一致の単体テスト
- `docs/BLEND2D.md` の更新

Completed: 2026-04-01

## 解決方法

- `PatternFilter`（Nearest / Bilinear）、`set_transform`（`Matrix2D`）、既存の `set_origin` を追加した。
- `prepare(matrix)` で `transform * inv(matrix)` を合成し、デバイス座標からテクスチャ座標へ写して `fill_rect` / `fetch_span` でサンプリングする。
- Pad / Repeat / Reflect に対応した Nearest と Bilinear（PRGB チャンネル線形）を実装した。
- `tests/test_pattern.rs` でスケール行列による Nearest / Bilinear の差を検証した。
- `docs/BLEND2D.md` の `Pattern` 行と課題一覧、`docs/RASTERIZE.md` を更新した。
