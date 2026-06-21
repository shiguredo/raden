# GlyphBounds の操作ヘルパー (translated_x / union / from_raw_scaled) の公開化を検討する

- Priority: Low
- Category: change
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/change-glyph-bounds-public-helpers
- Polished: {YYYY-MM-DD}

## 目的

0023 で `pub(crate)` として導入した `GlyphBounds::from_raw_scaled` / `translated_x` / `union` を、公開 API として提供するかどうかを正式に確定する。`#[non_exhaustive]` 構造体である `GlyphBounds` を外部から加工する手段が現状他にないため、利用者が文字列描画時に bbox を組み立てたい場合の操作性に直結する。

## 優先度根拠

Low。0024 (stroke_text) / 0025 (kerning) / 0026 (shaping) で外部利用者が文字列描画時に bbox を加工する場面が想定されるため、それらの issue 着手前に判断を確定させたい。

## 現状

`src/font/mod.rs` で以下の `pub(crate)` メソッドを定義済み:

```rust
impl GlyphBounds {
    pub(crate) fn from_raw_scaled(raw: (i16, i16, i16, i16), scale: f64) -> Self { ... }
    pub(crate) fn translated_x(self, dx: f64) -> Self { ... }
    pub(crate) fn union(self, other: Self) -> Self { ... }
}
```

`GlyphBounds` 自体は `pub` で `lib.rs:20` から re-export されている。外部利用者は `GlyphBounds` を取得できるが、`#[non_exhaustive]` のため自分でリテラル構築できず、`translated_x` / `union` も呼べないため加工手段が無い。

## 設計方針

以下のいずれかを選ぶ:

1. `pub` 公開化
   - `translated_x` を `translated(dx: f64, dy: f64) -> Self` に統一 (Y 軸対称性確保)。
   - `union` を `&self, &Self` または `self, Self` (Copy 型なので self でも問題ないが、`pub` 化なら一貫性のため要検討) に整理。
   - `from_raw_scaled` は内部用途のみのため `pub(crate)` のまま、または削除して `Self::from_design(raw: GlyphBoundsRaw)` のような利用者向け API に変更。
   - PBT で代数法則 (`union` の結合性・対称性・冪等性、`translated` の合成性) を検証。
2. `pub(crate)` 維持
   - 内部ヘルパーとして閉じたまま。外部利用者は `Font::measure_text` の戻り値 (`TextMetrics::bounding_box`) を使う前提。
   - `GlyphBounds` の加工が必要になった時点で公開化を再検討。

最終判断は polish 段階で、`Font` API 全体の安定化方針 (0001 メタ issue) との整合を見て行う。

## 完了条件

- `GlyphBounds` の操作ヘルパーの公開化方針が決定され、コードに反映されている。
- 公開化する場合は、`translated` / `union` の代数法則 PBT が追加されている。

## 解決方法

polish 段階で確定する。
