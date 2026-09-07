# 変更履歴

- UPDATE
  - 後方互換がある変更
- ADD
  - 後方互換がある追加
- CHANGE
  - 後方互換のない変更
- FIX
  - バグ修正

## develop

### misc

- [CHANGE] MSRV を 1.94 から 1.95 に引き上げる
  - @voluntas
- [CHANGE] PBT に noprop を導入する
  - @voluntas
- [UPDATE] cranelift を 0.133.3 から 0.135.1 に更新する
  - @voluntas
- [UPDATE] `Pattern::prepare` のアルファ値不透明判定を `chunks_exact` から `as_chunks` に変更する
  - @voluntas


## 2026.2.0

**リリース日**: 2026-07-27

- [CHANGE] `Font::measure_text` の戻り値 `advance` を OpenType GSUB / GPOS シェーピング適用済みに変更する
  - @voluntas
- [CHANGE] `Font::measure_text` の戻り値 `TextMetrics.bounding_box` を OpenType GSUB / GPOS シェーピング適用済みの bbox に変更する
  - @voluntas
- [CHANGE] `Context::fill_text` の描画結果のグリフ列・位置を OpenType GSUB / GPOS シェーピング適用済みに変更する
  - @voluntas
- [CHANGE] `Context::stroke_text` の描画結果のグリフ列・位置を OpenType GSUB / GPOS シェーピング適用済みに変更する
  - @voluntas
- [ADD] `examples/font_shaping.rs` を追加する
  - @voluntas
- [ADD] `Font` に `Clone` 実装を追加する
  - @voluntas
- [ADD] `FontFeatureSettings` 構造体を追加する
  - @voluntas
- [ADD] `GlyphBuffer` 構造体を追加する
  - @voluntas
- [ADD] `GlyphPlacement` 構造体を追加する
  - @voluntas
- [ADD] `Font::shape` を追加する
  - @voluntas
- [ADD] `Font::shape_into` を追加する
  - @voluntas
- [ADD] `Font::with_features` を追加する
  - @voluntas
- [ADD] `Font::clone_with_features` を追加する
  - @voluntas
- [ADD] `Font::set_feature_settings` を追加する
  - @voluntas
- [ADD] `Font::feature_settings` を追加する
  - @voluntas
- [ADD] `TextMetrics` に `leading_bearing` / `trailing_bearing` を追加する
  - @voluntas
- [ADD] OpenType `GSUB` / `GPOS` テーブルパースを追加する
  - @voluntas
- [ADD] OpenType 基本シェーピングを追加する
  - @voluntas
- [ADD] `TextMetrics` 構造体を追加する
  - @voluntas
- [ADD] `Font::measure_text` を追加する
  - @voluntas
- [ADD] `FontFace::cap_height` を追加する
  - @voluntas
- [ADD] `FontFace::x_height` を追加する
  - @voluntas
- [ADD] `Font::line_gap` を追加する
  - @voluntas
- [ADD] `Font::cap_height` を追加する
  - @voluntas
- [ADD] `Font::x_height` を追加する
  - @voluntas
- [ADD] `GlyphBounds` 構造体を追加する
  - @voluntas
- [ADD] `FontFace::glyph_bounds` を追加する
  - @voluntas
- [ADD] `Font::glyph_bounds` を追加する
  - @voluntas
- [ADD] `Context::stroke_text` を追加する
  - @voluntas
- [UPDATE] `README.md` のフォント機能説明を最新の実装に合わせて更新する
  - @voluntas
- [UPDATE] `docs/BLEND2D.md` のフォント機能比較を最新の実装に合わせて更新する
  - @voluntas
- [UPDATE] `FontData::from_file` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
  - @sile
- [UPDATE] `codec::bmp::write_bmp` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
  - @sile
- [UPDATE] `Image::write_to_file` の引数を `<P: AsRef<Path>>(path: P)` 形式に変更する
  - @sile
- [FIX] `cmap` format 4 の glyph_id_array オフセット境界判定を修正する
  - @voluntas

## 2026.1.1

**リリース日**: 2026-04-16

- [FIX] `include` に `"/src"` を含めていなかったのを修正する
  - @voluntas

## 2026.1.0

**リリース日**: 2026-04-15
