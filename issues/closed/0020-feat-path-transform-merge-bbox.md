# Path の座標変換・結合・バウンディングボックス取得を追加する

Created: 2026-04-09
Completed: 2026-04-09
Model: Opus 4.6

## 概要

Blend2D の `BLPath::translate` / `transform` / `add_path` / `add_transformed_path` / `get_bounding_box` / `get_control_box` 相当が未実装。

## 根拠

- 部品化したパスを他のパスに組み込む（ロゴ、テキストアウトラインの再配置等）使い方ができない
- レイアウト計算でバウンディングボックスが必要になる場面が多く、`fill_path` 内部ではすでに計算済みなので公開するだけでも価値がある
- パス自体への行列適用ができれば、毎フレーム `Context::transform` を切り替えなくてもキャッシュ済みパスを再利用できる

## 想定スコープ

- `Path::translate(dx, dy)` / `Path::transform(&Matrix2D)` を追加（in-place、コニック重みは不変）
- `Path::add_path(&Path)` / `Path::add_path_translated(&Path, dx, dy)` / `Path::add_path_transformed(&Path, &Matrix2D)` を追加
- `Path::bounding_box() -> Option<Rect>` を追加（空パスは None。`fill_path` 内部の計算ロジックを抽出して共有）
- `Path::control_box() -> Option<Rect>` を追加（コントロール点のみのバウンディング）
- PBT: `transform(IDENTITY)` で不変、`translate` 後の `bounding_box` が元の box を平行移動した結果と一致、`add_path` 後のコマンド数が和
- `docs/BLEND2D.md` および `CHANGES.md` の更新

## 注意点

- 既存の `EdgeBuilder` / `fill_path` 内 BBox 計算と重複させず、共有可能なヘルパに切り出す
- コニック曲線の重みは行列適用で値が変わらないことを確認する

## 解決方法

- `Path::translate(dx, dy)`: `points` / `cur` / `sub_start` / `last_quad_cp` / `last_cubic_cp2` の全頂点を in-place で平行移動
- `Path::transform(&Matrix2D)`: 同じ箇所に `Matrix2D::map_point` を適用。コニックの重みは行列適用で不変なので変更しない
- `Path::add_path(&other)`: コマンドを 1 つずつ replay する実装。これにより `cur` / `sub_start` / `last_*` の内部状態が自然に更新される
- `Path::add_path_translated(&other, dx, dy)`: 各頂点に dx/dy を加えて replay
- `Path::add_path_transformed(&other, &Matrix2D)`: 各頂点に行列を適用して replay
- `Path::control_box() -> Option<Rect>`: 全頂点の AABB を計算 (空パスは `None`)
- `Path::bounding_box() -> Option<Rect>`: 現状は `control_box` のエイリアス。曲線の厳密な bbox は今後対応 (ベジェ曲線の境界計算が必要なため別 issue 予定)
- `tests/test_path.rs` の `transform_merge_bbox` モジュールで以下を検証:
  - `translate` で全頂点が平行移動
  - `transform(IDENTITY)` が no-op
  - `transform(scaling(2, 2))` で bbox が 2 倍
  - `add_path` でコマンド数が和になる
  - `add_path_translated` で頂点が平行移動して追加
  - `add_path_transformed` で行列が適用される
  - 空パスの `bounding_box` が `None`
  - 通常 bbox が頂点範囲と一致
  - `translate` 後の bbox が平行移動
  - `transform` でコニック重みが不変
- 注意点で挙げた「`fill_path` 内 BBox 計算との共有ヘルパ化」は実施していない (`fill_path` 側は曲線フラット化後の AABB を扱っており、`Path::control_box` とは目的が異なるため別ロジックのままとした)
- `docs/BLEND2D.md` の該当行 4 箇所と課題一覧 7 を更新、`CHANGES.md` を更新
