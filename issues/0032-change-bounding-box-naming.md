# Path::bounding_box と TextMetrics::bounding_box の API 命名・doc 整合を取る

- Priority: Low
- Category: change
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/change-bounding-box-naming
- Polished: {YYYY-MM-DD}

## 目的

raden 内に同名の `bounding_box` API が 2 系統存在し、座標系・型・意味が異なる現状を整理し、利用者の混同を防ぐ。

- `Path::bounding_box() -> Option<Rect>` (画面座標 Y down、制御点ベース)
- `TextMetrics::bounding_box: Option<GlyphBounds>` (Y up、baseline 原点、glyf ヘッダ bbox の union)

font モジュール本格安定化 (0001 メタ issue) の前に命名方針を確定させる。

## 優先度根拠

Low。0023 で `TextMetrics::bounding_box` の doc に「`Path::bounding_box` (画面座標 Y down) とは座標系も意味も異なる」と注意書きを追加済みのため即時の問題はない。ただし font モジュール公開 API 安定化の前段として整理が必要。

## 現状

- `src/api/path.rs` の `Path::bounding_box() -> Option<Rect>` (`Rect` は `x` / `y` / `w` / `h` の xywh 形式、画面座標 Y down、内部実装は `control_box` のエイリアス)
- `src/font/mod.rs` の `TextMetrics::bounding_box: Option<GlyphBounds>` (`GlyphBounds` は `x_min` / `y_min` / `x_max` / `y_max` の min/max 形式、Y up、baseline 原点)

両者は意図的に同名にされたわけではなく、それぞれの API 設計で独立に決まった結果として衝突している。

## 設計方針

以下のいずれかを選ぶ:

1. `TextMetrics::bounding_box` を改名
   - `TextMetrics::glyph_run_bounds` または `TextMetrics::ink_box` 等に改名。
   - 利点: 同名衝突を解消。
   - 欠点: 0023 で公開した API の改名は破壊的変更 (font モジュール未安定方針で許容される)。`docs/BLEND2D.md` L455 の表記も更新。
2. doc に注意書きを強化 (現状維持に近い)
   - 0023 で追加した注意書きをさらに目立たせる (`Path::bounding_box` 側にも追記)。
   - 利点: 破壊的変更なし。
   - 欠点: 命名の衝突自体は残る。
3. `Path::bounding_box` を改名 (より既存影響が大きい)
   - 名前を `Path::control_box` 等に改名。
   - 既存利用者への影響が大きいため、本案は基本的に採用しない。

最終判断は polish 段階で行う。0001 メタ issue の font モジュール公開 API 安定化方針と整合させる。

## 完了条件

- `bounding_box` 命名の整合方針が決定され、コードと doc に反映されている。
- 改名する場合は、`docs/BLEND2D.md` L455 の表記、`CHANGES.md`、PBT / 単体テストが追従している。

## 解決方法

polish 段階で確定する。
