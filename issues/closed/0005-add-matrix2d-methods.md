# Matrix2D に post 系メソッドと中心点指定 rotate を追加する

Created: 2026-03-28
Completed: 2026-03-28
Model: Opus 4.6

## 概要

Blend2D の Matrix2D API には `post_translate`、`post_scale`、`post_rotate`、`post_transform`、`rotate(angle, cx, cy)` が存在するが、raden の Matrix2D にはこれらに相当するメソッドがなかった。

## 根拠

Blend2D API 準拠を目指す上で、前乗算系 (post) メソッドと中心点指定回転は基本的な変換操作であり、tiger デモを含む多くのサンプルで必要となる。

## 対応内容

### 追加したメソッド

| メソッド | 説明 |
|---|---|
| `rotate_around(angle, cx, cy)` | 中心点まわりの回転 (後乗算) |
| `post_translate(tx, ty)` | 平行移動の前乗算 |
| `post_scale(sx, sy)` | スケーリングの前乗算 |
| `post_rotate(angle)` | 回転の前乗算 |
| `post_transform(m)` | 任意行列の前乗算 |

### 最適化

既存の `translate`、`scale`、`rotate`、`rotate_around` を行列乗算の展開により最適化した。

- `translate`: 63 µs → 9.4 µs (-85%)
- `scale`: 63 µs → 12.5 µs (-80%)
- `rotate_around`: 134 µs → 87 µs (-35%)
- `post_translate`、`post_scale` も展開済みで multiply 不要

`rotate` 系は `sin_cos` が支配的でこれ以上のスカラー最適化は困難。

### PBT

8 テスト追加 (計 17 テスト):

- 各 post 系メソッドが `Op * self` と等価であることの検証
- `rotate_around` が手動合成と一致すること
- `rotate_around` で中心点が不動点になること
- `rotate_around` で中心点からの距離が保存されること
- `post_translate` のラウンドトリップ

## 解決方法

`src/api/matrix.rs` に 5 メソッドを追加し、既存の `translate`/`scale`/`rotate`/`rotate_around` を行列乗算の展開で最適化した。`pbt/tests/prop_matrix.rs` に 8 テスト追加。`benches/matrix.rs` にベンチマークを追加した。
