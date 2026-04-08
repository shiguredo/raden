# Path の座標変換・結合・バウンディングボックス取得を追加する

Created: 2026-04-09
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
