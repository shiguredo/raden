# docs/BLEND2D.md L455 の TextMetrics 表記を bounding_box フィールドを含めて更新する

- Priority: Low
- Category: refactor
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/refactor-blend2d-doc-text-metrics
- Polished: {YYYY-MM-DD}

## 目的

0023 で `TextMetrics::bounding_box: Option<GlyphBounds>` を追加したが、`docs/BLEND2D.md` L455 の Blend2D 対応表は依然 `TextMetrics { advance: f64 }` の表記のままで実装と乖離している。これを実装に整合させる。

## 優先度根拠

Low。ドキュメント表記の追従漏れであり、実害はない。ただし `docs/BLEND2D.md` は raden の Blend2D 互換度を示す根拠資料であり、実装と乖離した記述を放置するのは Don't live with broken windows に該当する。0023 のレビューで指摘された。

## 現状

`docs/BLEND2D.md` L455:

```
| `get_text_metrics(BLGlyphBuffer&, BLTextMetrics&)` | `Font::measure_text(&str)` -> `TextMetrics` | 差異あり: raden は `&str` 入力で `TextMetrics { advance: f64 }` を返す個別取得 (`#[non_exhaustive]`)。Blend2D の `BLTextMetrics` (`advance: BLPoint`, `leading_bearing` 等の 4 フィールド) には将来段階的に拡張予定 |
```

実装には `bounding_box: Option<GlyphBounds>` フィールドが追加されているため、`{ advance: f64 }` の表記は古い。

## 設計方針

以下のいずれかを選ぶ:

1. フィールドを列挙して更新
   - `TextMetrics { advance: f64, bounding_box: Option<GlyphBounds> }` のように現在のフィールドを列挙。
   - 利点: 実装と完全一致。
   - 欠点: 0026 でさらに `leading_bearing` / `trailing_bearing` 等が追加された場合に再度更新が必要 (0001 メタ issue 段階拡張表 L91 で予告)。
2. 簡略化
   - 「raden は `&str` 入力で `TextMetrics` を返す個別取得」程度に省略し、フィールドの詳細は `TextMetrics` 構造体の rustdoc に集約。
   - 利点: 表が短くなり、フィールド追加に強い。
   - 欠点: 表だけで「現在何が入っているか」が分からない。

最終判断は polish 段階で、`docs/BLEND2D.md` 全体の他行の記述粒度と整合させて決める。

## 完了条件

- `docs/BLEND2D.md` L455 の raden 列が実装と整合している (`bounding_box` フィールドが反映されている、または簡略化されている)。

## 解決方法

polish 段階で確定する。
