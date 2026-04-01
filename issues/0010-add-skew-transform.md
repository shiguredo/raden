# skew 変換を Context / Matrix2D に追加する

Created: 2026-04-01
Model: Composer

## 概要

Blend2D の `skew` および `Matrix2D::make_skewing` 相当の、せん断（シアー）変換を `Context` と `Matrix2D` に追加する。必要なら `post_skew` 系との対応関係も整理する。

## 根拠

2D の表現力（斜めテキスト、疑似 3D、UI の歪み表現）に直結し、Blend2D との API 差分としても明確な欠落である。

## 大枠の作業

- `Matrix2D` にせん断行列の生成・既存 `multiply` / `map_point` との整合
- `Context::skew`（および Blend2D と揃えるなら `post_skew` の有無の設計）
- 既存の `translate` / `rotate` / `apply_matrix` との合成順のテスト
- `docs/BLEND2D.md` の更新
