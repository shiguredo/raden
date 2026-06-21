# Font::measure_text の bounding_box における .notdef (glyph_id == 0) の取扱いを確定する

- Priority: Low
- Category: change
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/change-measure-text-notdef-policy
- Polished: {YYYY-MM-DD}

## 目的

`Font::measure_text` の `bounding_box` 計算において、`glyph_id == 0` (.notdef) の bbox を含めるかどうかを正式に確定する。現状の「含める」設計が `Context::fill_text` (`.notdef` スキップ) と乖離しているため、bounding_box の意味づけを揃える。

## 優先度根拠

Low。現状の挙動は 0023 で doc 追記済みで明示されている。ただし font モジュール本格安定化前に「実描画範囲と bbox の関係」を確定させる必要があり、決定が後ろ倒しになると 0026 (シェーピング) で再度議論になる。

## 現状

`src/font/mod.rs` の `Font::measure_text`:

```rust
for &(glyph_id, advance) in buf.iter() {
    if let Some(gb) = self.glyph_bounds(glyph_id) {
        let translated = gb.translated_x(cursor_x);
        bbox = Some(match bbox { ... });
    }
    cursor_x += advance;
}
```

`glyph_bounds(glyph_id == 0)` は `.notdef` グリフの bbox を返すため、union に含まれる。

一方、`Context::fill_text` (src/api/context.rs) は `glyph_id == 0` のアウトライン描画をスキップする (`fill_text` の現状は 0001 メタ issue「Context::fill_text の制限」L29-32 参照)。

結果として `bounding_box` は実描画範囲より広くなる場合がある。0023 で doc に「`Context::fill_text` の実描画範囲 (`.notdef` はスキップ) より広くなることがある」と明記したが、利用者が「描画範囲」と誤解するリスクは残る。

## 設計方針

以下のいずれかに統一する:

1. `.notdef` をスキップ (描画範囲と一致)
   - `measure_text` の union 計算で `if glyph_id == 0 { cursor_x += advance; continue; }` を入れる。
   - `Context::fill_text` の描画範囲と完全に一致する。
   - 欠点: `bounding_box` が「メトリクス上の bbox」より「実描画範囲」に寄る。Blend2D との対応 (Blend2D は .notdef を描画する場合がある) との乖離も増える。
2. `.notdef` を含める (現状維持、doc 強化)
   - 現状の動作を維持し、`Font::measure_text` の doc にさらに目立つ注意書きを追加。
   - 欠点: 利用者の誤解リスクが残る。
3. 別 API を追加
   - `Font::measure_text_for_drawing(&str) -> TextMetrics` のような描画範囲版を別途追加。
   - 欠点: API 二重化。

最終判断は polish 段階で行うが、現状の `bounding_box` は「メトリクス上の bbox」として導入された経緯 (0001 メタ issue 段階拡張表) から、案 2 (現状維持 + doc 強化) が無難。

## 完了条件

- `measure_text` の `.notdef` 取扱いが正式に決定され、コードと doc が整合している。
- PBT で「`measure_text.bounding_box` と `Context::fill_text` で塗られるピクセル領域の関係 (内包 / 一致 / 別物)」が決定された方針に従って検証されている。

## 解決方法

polish 段階で確定する。
