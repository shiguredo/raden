# OpenType 基本シェーピング機能を追加する

- Priority: Medium
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.7 Code
- Branch: feature/add-opentype-basic-shaping
- Polished: 2026-06-21

## 目的

OpenType の GSUB / GPOS レイアウト機能を適用し、リガチャ (例: `fi` → `ﬁ`) や GPOS Pair Adjustment によるカーニング等の高度なテキスト表現を可能にする。`Font::shape(text) -> GlyphBuffer` を新規追加し、0022 で導入された `glyph_run_for_text` (`pub(crate)`) を完全に置換する。`fill_text` / `measure_text` / `stroke_text` の 3 経路をシェーピング適用形に書き換える。

本 issue は `add` カテゴリ (API 追加が主目的) として扱うが、`Font::measure_text` および `Context::fill_text` / `Context::stroke_text` の値・描画結果に意味的変化を生じるため、`CHANGES.md` には `CHANGE` 種別も併記する。これは 0001 メタ issue「font 公開 API の安定化方針」で「font モジュールの公開 API は本 issue close までは未安定とみなし、0021-0027 で破壊的変更を許容する」「破壊的変更 (戻り値の意味変化等) は `CHANGES.md` の `## develop` トップ階層に `CHANGE` 種別で記載する」と確定済みのため、`add` カテゴリ concrete issue 内で `CHANGE` 種別を併発する設計が許容される。同方針により `Category: change` の別 issue は起票しない (0025 と同じ設計)。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L444 `BLFont::feature_settings()` / `set_feature_settings()` / `reset_feature_settings()`**: raden は `FontFeatureSettings` 構造体、`Font::with_features(&FontFace, f64, FontFeatureSettings) -> Self` (構築時設定)、`Font::feature_settings(&self) -> &FontFeatureSettings` (getter)、`Font::set_feature_settings(&mut self, FontFeatureSettings)` (mutable setter) で対応。`reset_feature_settings()` は `Font::set_feature_settings(FontFeatureSettings::default())` で代替するため独立 API は提供しない (対象外)
- **L446 `BLFont::shape(BLGlyphBuffer&)`**: raden は `Font::shape(text: &str) -> GlyphBuffer` (`Font` 内蔵の `feature_settings` を使用、Blend2D 互換寄り) と `Font::shape_into(text: &str, buf: &mut GlyphBuffer) -> ()` (バッファ再利用版、`Context::fill_text` / `stroke_text` 経路で使用) の 2 API で対応
- **L447 `BLFont::map_text_to_glyphs(BLGlyphBuffer&)`**: raden は `Font::shape()` 内部処理として対応 (公開 API は `Font::map_char_to_glyph(char)`)
- **L448 `BLFont::position_glyphs(BLGlyphBuffer&)`**: 同上 (`Font::glyph_advance(u16)` の公開 API は残る)
- **L450 (部分) `BLFont::apply_gsub(BLGlyphBuffer&, BLBitArray&)` / `apply_gpos(BLGlyphBuffer&, BLBitArray&)`**: raden は `Font::shape()` の内部実装として基本 Lookup Type のみ対応 (GSUB Single Substitution / Ligature Substitution、GPOS Single Adjustment / Pair Adjustment、両者の Extension Lookup wrapping)。`BLBitArray` 相当 (lookup index 選択ビットマスク) と GSUB / GPOS の Context / Chaining Context Lookup は font モジュール安定化前の別 issue で検討
- **L456 `BLGlyphBuffer`**: raden は `GlyphBuffer` を新規定義 (本 issue で初出、SoA レイアウト、後述)

差異の理由:

- raden は `BLGlyphBuffer` の in-place 操作と異なり、`Font::shape()` が新規 `GlyphBuffer` を返す API と、`Font::shape_into(&mut buf)` のバッファ再利用版を併設する (前者は Blend2D 互換寄りのエルゴノミクス、後者は `Context::fill_text` でのアロケーション再利用)
- `BLBitArray` (lookup 選択ビットマスク) と GSUB / GPOS の Context / Chaining Context Lookup は本 issue では対応せず、font モジュール安定化前の別 issue で検討

完了 PR で `docs/BLEND2D.md` L444 / L446 / L447 / L448 / L450 / L456 の raden 列に新規 API 名を追記する。状態列 (`未実装` / `差異あり`) は本 issue では変更せず、0001 メタ issue の close PR でまとめて整理する。

## 現状

- テキスト描画は `cmap` による文字 → グリフ変換と単純な `advance` 累積のみ (0022 で `glyph_run_for_text` が導入され、0025 で `kern` テーブル経由のカーニングが加算されている状態)
- リガチャが適用されない (例: `"fi"` は別グリフ `f` + `i` のままで、合字 `ﬁ` (U+FB01) として描画されない)
- GSUB テーブルは未パース・未利用
- GPOS テーブルは未パース・未利用 (0025 の `kern` テーブル経由のみカーニングが効く)
- `BLGlyphBuffer` 相当の中間バッファがない
- `FontFeatureSettings` (`kern` / `liga` / `clig` 等の feature on/off 制御) が未実装

## 設計方針

### `GlyphBuffer` のレイアウト

Blend2D の `BLGlyphBuffer` (`~/src/blend2d/blend2d/core/glyphbuffer.h:50-120`) は `content: uint32_t*` (glyph IDs), `info: BLGlyphInfo*` (cluster 含む), `placement: BLGlyphPlacement*` の 3 配列 SoA 構造。raden の `GlyphBuffer` も SoA レイアウトを採用する:

```rust
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct GlyphBuffer {
    /// グリフ ID 列 (シェーピング後)。
    pub glyph_ids: Vec<u16>,
    /// 各グリフの placement (offset_x, offset_y) と advance のペア (ピクセル単位 f64)。
    pub placements: Vec<GlyphPlacement>,
    /// 各グリフの元コードポイント (cluster)。Blend2D / HarfBuzz の慣行に合わせ
    /// **UTF-8 byte index** を保持する (`text.char_indices()` の byte offset)。
    /// リガチャ後の caret 位置 / 選択範囲 / コピー用。リガチャ後の cluster 値は
    /// リガチャを構成した最初の char の byte index を採用する (HarfBuzz と同じ慣行)。
    pub clusters: Vec<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[non_exhaustive]
pub struct GlyphPlacement {
    pub offset_x: f64,
    pub offset_y: f64,
    pub advance: f64,
}
```

- **SoA** を採用 (Blend2D 互換)
- **配列長一致は `Font::shape` / `Font::shape_into` の事後条件** (`glyph_ids.len() == placements.len() == clusters.len()`)。docstring に明示。`pub` フィールドのため外部からの個別 `push` で長さ破綻は可能だが、`Font::shape` の戻り値を `Context::fill_text` 内でのみ消費する設計で破綻リスクを抑える
- **`Default` 派生** は `Context::tmp_shape_buffer` の `std::mem::take` に必要
- **`#[non_exhaustive]`** は 0001 メタ issue で確定した `non_exhaustive` 対象に `GlyphBuffer` / `FontFeatureSettings` が含まれる方針に従う。`GlyphPlacement` は 0001 対象 4 型外だが、Blend2D `BLGlyphPlacement` がフィールド変動 (Y placement / advance 等) するため本 issue で同方針を適用する
- **placement / advance は f64** (Blend2D の `BLPointI` (i32) ではない)。raden の他 API との整合性優先。整数化は別 issue
- **`cluster` 配列を含める**。BiDi / 複雑スクリプトは本 issue 対象外だが、ラテン系リガチャ (`fi` / `fl` 等) でも caret 位置決定に必要

### `FontFeatureSettings` と `Font` の feature 保持

```rust
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FontFeatureSettings {
    /// kern feature の on/off (デフォルト: true)。
    pub kern: bool,
    /// liga feature の on/off (デフォルト: true)。標準リガチャ (fi / fl 等)。
    pub liga: bool,
    /// clig feature の on/off (デフォルト: true)。文脈依存リガチャ。
    pub clig: bool,
}

impl Default for FontFeatureSettings {
    fn default() -> Self {
        Self { kern: true, liga: true, clig: true }
    }
}
```

`Font` 構造体に `feature_settings: FontFeatureSettings` フィールドを追加し、`Font::with_features` / `Font::set_feature_settings` で更新可能にする。`Font::shape(text: &str)` は **`Font` 内蔵の `feature_settings` を参照** する設計を採用する (Blend2D 互換: `BLFont::shape(BLGlyphBuffer&)` は引数 features を取らず `BLFont` 状態を参照)。

一時的に異なる feature で shape したい場合は `font.clone_with_features(...)` を作って shape する流れ (`Font` 自体は `Clone` 実装を本 issue で同時追加する。後述「`Font` の `Clone` 派生」)。または `font.set_feature_settings(...)` で書き換えてから shape する。

### `Font::from_face` シグネチャ維持

`Font::from_face(&FontFace, f64) -> Self` のシグネチャは本 issue で変更しない。内部実装で `Self::with_features(face, size, FontFeatureSettings::default())` を呼ぶ形にリファクタする。0021-0025 で書かれた `Font::from_face(&face, size)` 呼び出しは全て変更不要。

### サポートする GSUB / GPOS Lookup Type

本 issue で対応する Lookup Type:

- **GSUB Lookup Type 1: Single Substitution** (Format 1 / 2 両方。1 グリフ → 1 グリフ)
- **GSUB Lookup Type 4: Ligature Substitution** (N グリフ → 1 グリフ。fi / fl 等)
- **GSUB Lookup Type 7: Extension Substitution** (上記 Type 1 / 4 を内部にラップ。主要フォント Noto Sans / Source Code Pro 等で必須)
- **GPOS Lookup Type 1: Single Adjustment** (Format 1 / 2 両方。個別グリフの位置調整)
- **GPOS Lookup Type 2: Pair Adjustment** (Format 1 / 2 両方。ペアグリフの位置調整。kern テーブルの後継。Format 1 は個別ペア、Format 2 はクラスベース行列)
- **GPOS Lookup Type 9: Extension Positioning** (上記 Type 1 / 2 を内部にラップ)
- **Coverage Table Format 1 (glyph 列挙) / Format 2 (range 連続) の両方**

Extension Lookup (Type 7 / 9) を対応必須にする理由: 主要フォント (Noto Sans / Source Code Pro / Roboto / SF Pro 等) では subtable サイズが 64KB を超えるため、`liga` / `kern` を含むほぼ全ての Lookup が Extension でラップされている。Extension を非対応にすると主要フォントでリガチャ・カーニングが全く効かない状態になる。Extension の実装は薄く (内部の実 Lookup Type を unwrap して再帰呼び出しするだけ)、本 issue のスコープに含める。

非対応 (将来の課題):

- GSUB Lookup Type 2 (Multiple Substitution)、3 (Alternate Substitution)、5/6 (Context / Chaining Context)、8 (Reverse Chaining)
- GPOS Lookup Type 3 (Cursive)、4 (Mark to Base)、5 (Mark to Ligature)、6 (Mark to Mark)、7/8 (Context / Chaining Context)
- 複雑スクリプト (Arabic / Indic / Hebrew 等)
- 双方向テキスト (BiDi)
- ValueRecord の Device Table (低 PPEM でのピクセル単位調整)

### Script / Feature 選択

ラテン script のみを対象とする本 issue の単純化:

- **ScriptList**: `'DFLT'` を最優先、なければ `'latn'` を採用。それ以外の script は無視
- **LangSys**: DefaultLangSys のみを採用。LangSys (en-US / fr-FR 等の言語固有) は無視
- **Required Feature Index**: 無視 (本 issue では `liga` / `clig` / `kern` のみ採用するため Required は不要)
- **FeatureList**: `FontFeatureSettings` で有効な feature tag (`liga` / `clig` / `kern`) と `FeatureList` のエントリを線形探索で照合 (高々 数件のため二分探索不要)。複数の同名 feature がある場合は最初の 1 つを採用
- **LookupList**: 採用した feature が参照する Lookup インデックスを順次評価。サポート外の Lookup Type に遭遇したら当該 Lookup を skip して次へ

### ValueRecord の処理

GPOS Single Adjustment / Pair Adjustment で使う `ValueRecord` は可変長で、`ValueFormat: u16` ビットマスクで含まれるフィールドが決まる。本 issue では以下の 4 ビットのみ採用 (Device Table 関連ビットは無視):

- bit 0 (0x0001): XPlacement (i16)
- bit 1 (0x0002): YPlacement (i16)
- bit 2 (0x0004): XAdvance (i16)
- bit 3 (0x0008): YAdvance (i16)

bits 4-7 (各種 Device Table のオフセット) は本 issue では解釈せず、`ValueFormat` で set されていれば「読み飛ばす」処理だけ行う (各 2 バイトを進める)。

### `apply_gsub` / `apply_gpos` / `apply_kern_fallback` ヘルパー

`src/font/shape.rs` (新規 `pub(crate)` モジュール) に以下を定義:

関数シグネチャは `font: &Font` を共通で受け取る形に統一する (Single / Pair Adjustment の `scale` も `font.scale()` 経由で取得、Ligature 置換時の合字グリフ advance も `font.glyph_advance` で取得):

- `pub(crate) fn apply_gsub(gsub: &GsubTable, buf: &mut GlyphBuffer, features: &FontFeatureSettings, font: &Font) -> ()`
  - `liga` / `clig` feature が enable なら GSUB Single Substitution と Ligature Substitution Lookup を順次適用
  - Ligature 適用時は `glyph_ids` / `placements` / `clusters` の 3 配列を同期 drain で再構築 (リガチャを構成した最初の cluster 値を採用)。合字グリフの advance は `font.glyph_advance(lig.ligature_glyph)` で取得
  - Extension Lookup は内部の実 Lookup Type を unwrap して再帰呼び出し
- `pub(crate) fn apply_gpos(gpos: &GposTable, buf: &mut GlyphBuffer, font: &Font) -> bool`
  - `features.kern` の有無判定は呼び出し側 (`Font::shape_into`) の責務とし、本関数内では行わない
  - GPOS `kern` feature 紐付き Lookup (Single Adjustment / Pair Adjustment) を順次適用
  - `ValueRecord` の (XPlacement, YPlacement) を `placements[i].offset_x`, `offset_y` に加算 (デザインユニットを `font.scale()` でスケール)
  - `ValueRecord` の (XAdvance) を `placements[i].advance` に加算 (本 issue ではテキストは水平のみ、YAdvance は読み込むが加算しない)
  - 戻り値 `bool` は「`kern` feature 紐付き Lookup が 1 件でも存在し処理を試みたか」(`gpos.kern_lookups.is_empty()` の否定。kern フォールバック判定用、二重適用回避)
  - Extension Lookup は内部の実 Lookup Type を unwrap して再帰呼び出し
- `pub(crate) fn apply_kern_fallback(kern: &KernTable, buf: &mut GlyphBuffer, font: &Font) -> ()`
  - 0025 で実装した `KernTable::lookup` を使い、隣接ペア `(glyph_ids[i], glyph_ids[i+1])` の値を `placements[i].advance` に加算 (デザインユニットを `font.scale()` でスケール)

### kern テーブル経路の扱い

0025 で実装された `FontFace::kern` / `Font::kern` API と `fill_text` / `measure_text` / `stroke_text` 内の `kern` 経由のカーニング適用は、本 issue の GPOS Pair Adjustment 経路に統合する。

- **`Font::shape()` 経路**: `apply_gpos` 内で **`kern` feature 紐付き Lookup が 1 件でも存在し処理を試みたか** (戻り値 `bool`、`gpos.kern_lookups.is_empty()` の否定) を判定し、true の場合は kern フォールバックを行わない (GPOS が effective なら kern テーブルは無視。HarfBuzz / Microsoft OpenType の慣行)。`kern_lookups` が空のフォント (GPOS テーブル不在、または GPOS はあるが `kern` feature 不在) では `apply_kern_fallback` を呼ぶ
- **`FontFace::kern` / `Font::kern` API**: 戻り値型 (`i16` / `f64`) と「`glyph_id == 0` を含むペア検索対象」「`size == 0` で 0.0」等の挙動は維持。docstring を「GPOS Pair Adjustment Lookup を優先、不在時 kern テーブルにフォールバック」に更新。`FontFace::kern` 内部実装はペア単位で `gpos.lookup_pair_adjustment(g1, g2).or_else(|| kern.lookup(g1, g2)).unwrap_or(0)` パターンに書き換える (`gpos.lookup_pair_adjustment` を `GposTable` に `pub(crate)` で追加)。`Font::shape()` 経路 (lookup_indices 全体での切替) と `FontFace::kern` 経路 (ペアごとの切替) で粒度が異なる点は意図的で、後者は単一ペア取得の利便性 API として残す
- **`fill_text` / `measure_text` / `stroke_text`**: 0025 で実装された `Font::kern` 直接呼び出しを削除し、`Font::shape` / `Font::shape_into` 経由 (`placements[].advance` が GPOS / kern 適用済みの advance を含む) に置換

### `compute_bounding_box` ヘルパー

`Font::measure_text` の `TextMetrics.bounding_box` (0023 で追加されたフィールド) を `Font::shape` 経由の `GlyphBuffer` から計算するため、`src/font/mod.rs` 内に private fn として追加:

```rust
fn compute_bounding_box(buffer: &GlyphBuffer, font: &Font) -> Option<GlyphBounds> {
    let mut cursor_x = 0.0;
    let mut bbox: Option<GlyphBounds> = None;
    for (i, &glyph_id) in buffer.glyph_ids.iter().enumerate() {
        let placement = buffer.placements[i];
        if let Some(gb) = font.glyph_bounds(glyph_id) {
            let translated = GlyphBounds {
                x_min: gb.x_min + cursor_x + placement.offset_x,
                y_min: gb.y_min + placement.offset_y,
                x_max: gb.x_max + cursor_x + placement.offset_x,
                y_max: gb.y_max + placement.offset_y,
            };
            bbox = Some(match bbox {
                None => translated,
                Some(b) => GlyphBounds {
                    x_min: b.x_min.min(translated.x_min),
                    y_min: b.y_min.min(translated.y_min),
                    x_max: b.x_max.max(translated.x_max),
                    y_max: b.y_max.max(translated.y_max),
                },
            });
        }
        cursor_x += placement.advance;
    }
    bbox
}
```

リガチャで `glyph_ids.len() < text.chars().count()` になった場合、合字グリフ ID の `glyph_bounds` を取得して bbox に union する。0023 で書かれた `Path::control_box() ⊆ glyph_bounds` PBT は `shape()` 経由でも維持される (個別グリフの `glyph_bounds` 自体は不変、cursor_x 進行と offset_x 加算で位置調整するだけ)。

### `Font` の `Clone` 派生

本 issue で `Font` に `#[derive(Clone)]` を追加する。理由: 「一時的に異なる feature で shape したい」場合、`font.clone_with_features(new_features).shape(text)` のような流れが必要。`Font.face: Arc<FontFaceInner>` (`Arc::clone` で軽量)、`size: f64` (`Copy`)、`scale: f64` (`Copy`)、`feature_settings: FontFeatureSettings` (本 issue で追加、`Clone`) で全フィールドが `Clone` 可能なため `#[derive(Clone)]` で問題ない。

`Font::clone_with_features(&self, features: FontFeatureSettings) -> Self` も同時追加する。内部実装は `let mut clone = self.clone(); clone.feature_settings = features; clone`。

### `Font::measure_text` の結合性 PBT に関する整理

0025 で `prop_concatenation` (`pbt/tests/prop_font/main.rs:51-67` 相当) と `prop_non_negative` (`main.rs:91-100` 相当) は **既に削除済み**。本 issue で 0022 PBT を改めて削除する作業は無い。

ただし 0022 で書かれて 0025 polish 完了時点で「維持される」と明示された:

- `prop_single_char_advance`: 単一文字一致 `measure_text(c).advance == glyph_advance(map_char_to_glyph(c))`
- `prop_size_linearity`: サイズ線形性

の 2 件は、本 issue で `Font::shape` 経由になることで GPOS Single Adjustment (単体グリフの advance 補正) が適用されると等式が破綻する可能性がある。0025 polish 完了時点では「Arial 等 GPOS Single Adjustment 不在のフォントでは維持される」前提で維持されていたが、本 issue ではテスト用フォントが GSUB / GPOS 付きフォントに切り替わり、Single Adjustment を持つフォントが想定される。Arial 限定維持は脆弱で false positive リスクが高いため、本 issue で両方とも削除する。0025 polish 完了内容を本 issue で上書きする形になるが、`shape()` 導入による設計変化として正当化される (`Font::measure_text` の意味自体が変わるため、依存する PBT も再構成が必要)。

0025 で追加された `prop_kern_pair_addition` 相当 (「隣接ペア加算の関係」PBT) は `measure_text(text).advance == sum(glyph_advance) + sum(Font::kern)` を検証するが、本 issue で `shape()` 経由になると:

- GSUB Ligature でグリフ数が減ると等式が成り立たない
- GPOS Single Adjustment で advance が変わると等式が成り立たない
- GPOS Pair Adjustment が `Font::kern` と一致しない可能性 (`Font::kern` は kern テーブル経由、shape は GPOS 経由)

これも本 issue で削除する。代わりに「`Font::shape(text).placements.iter().map(|p| p.advance).sum() == Font::measure_text(text).advance」(shape と measure_text の総和一致) PBT を追加する (実装の二重定義になるが、`measure_text` 内部が `shape().placements.advance.sum()` と等しいことを保証する oracle test として有用)。

### `TextMetrics::leading_bearing` / `trailing_bearing` の追加

0001 メタ issue 段階拡張表 L91 で「`TextMetrics` フィールド (追加) 0026 時点: `leading_bearing` / `trailing_bearing` 追加 (`BLTextMetrics` 互換に近づく)」と確定済み。本 issue で `TextMetrics` に以下のフィールドを追加する:

- `pub leading_bearing: f64`: 最初のグリフの `glyph_bounds.x_min + placements[0].offset_x` (テキスト左端からの「空白」幅)
- `pub trailing_bearing: f64`: 最後のグリフの `placements[last].advance - (glyph_bounds.x_max + placements[last].offset_x)` (テキスト右端からの「空白」幅)

空文字列および全グリフが `glyph_bounds = None` (空グリフのみ) の場合は両方 `0.0` を返す。

### 値の意味的変化と CHANGE 種別

- `Font::measure_text` の `TextMetrics.advance` の意味が「Microsoft OpenType `kern` v0 適用済み」(0025) から「OpenType GSUB / GPOS シェーピング適用済み」に変化する (`CHANGE` 種別)
- `Font::measure_text` の `TextMetrics.bounding_box` の意味が「カーニング適用済み bbox」から「シェーピング適用済み bbox」に変化する (`CHANGE` 種別。0023 で追加されたフィールド)
- `Context::fill_text` / `Context::stroke_text` の描画結果のグリフ列・位置がシェーピング適用済みに変わる (`CHANGE` 種別)
- `FontFace::kern` / `Font::kern` の戻り値の意味が「Microsoft OpenType `kern` v0 のみ」(0025) から「GPOS Pair Adjustment 優先 / kern テーブルフォールバック」に変化する (`CHANGE` 種別)

## 完了条件

### 追加される API

- `GlyphBuffer` 構造体 (SoA レイアウト、`#[non_exhaustive]`、`Default` 派生、上記参照)
- `GlyphPlacement` 構造体 (`#[non_exhaustive]`、`Default` 派生)
- `FontFeatureSettings` 構造体 (`#[non_exhaustive]`、`Default` 実装)
- `Font::with_features(&FontFace, f64, FontFeatureSettings) -> Self`
- `Font::set_feature_settings(&mut self, FontFeatureSettings)`
- `Font::feature_settings(&self) -> &FontFeatureSettings` (getter)
- `Font::clone_with_features(&self, FontFeatureSettings) -> Self`
- `Font::shape(text: &str) -> GlyphBuffer` (`Font` 内蔵 `feature_settings` を使用)
- `Font::shape_into(&self, text: &str, buf: &mut GlyphBuffer)` (バッファ再利用版)
- `TextMetrics.leading_bearing: f64` フィールド追加 (`#[non_exhaustive]` のため非破壊)
- `TextMetrics.trailing_bearing: f64` フィールド追加 (`#[non_exhaustive]` のため非破壊)
- `Font` 構造体に `#[derive(Clone)]` 追加

### 削除される内部 API

- `Font::glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` (`pub(crate)`、0022 で追加) を削除
- `Context.tmp_glyph_run: Vec<(u16, f64)>` フィールドを `Context.tmp_shape_buffer: GlyphBuffer` に置換 (`Context::new` での初期化を `GlyphBuffer::default()` に変更)

### 既存 API のシグネチャ維持と意味的変化

- `Font::from_face(&FontFace, f64) -> Self` のシグネチャは変更しない (内部実装で `Self::with_features(face, size, FontFeatureSettings::default())` を呼ぶ形に変更)
- `Font::measure_text(&str) -> TextMetrics`: シグネチャ不変だが、`TextMetrics.advance` の意味が「カーニング適用済み」(0025) から「シェーピング適用済み (GSUB + GPOS)」に変わる (`CHANGE` 種別)。`TextMetrics.bounding_box` も同様 (`CHANGE` 種別)
- `Context::fill_text` / `Context::stroke_text`: シグネチャ不変だが、描画結果がシェーピング適用済みのグリフ列・位置に変わる (`CHANGE` 種別)
- `FontFace::kern` / `Font::kern`: シグネチャ不変だが、GPOS Pair Adjustment 優先 / kern テーブルフォールバックの動作に変わる (`CHANGE` 種別)

### docstring 更新

- `Font::measure_text` の docstring (`src/font/mod.rs:200-208`) の「Microsoft OpenType `kern` v0 (Format 0 / horizontal) によるカーニングを隣接グリフペアに適用した advance を返す。詳細は `Font::kern` 参照。」(0025 で更新済) の **1 文のみ** を「OpenType GSUB / GPOS 基本シェーピング (`Font::shape` で適用) を適用した advance を返す。GPOS Pair Adjustment 不在のフォントでは Microsoft OpenType `kern` v0 にフォールバックする。詳細は `Font::shape` 参照。」に置換する。`size == 0` / NaN / 改行 等の他段落は維持
- `FontFace::kern` の docstring (`src/font/mod.rs` 0025 追加分) の「Microsoft OpenType `kern` テーブル v0 の Format 0 / horizontal subtable のみを参照する。」の 1 文を「GPOS Pair Adjustment Lookup を優先、不在時に Microsoft OpenType `kern` v0 にフォールバックする。」に置換する。他段落 (テーブル不在時 / glyph_id == 0 の取り扱い等) は維持
- `Font::kern` の docstring は同様に 1 文を置換

### 0001 メタ issue 段階拡張表の更新

本 issue close PR 内で `issues/0001-enhance-font-module-maturity.md` の「段階拡張で意味が変わる API」表 (L88-94) の以下の行を確定値で更新する責務を持つ:

- `TextMetrics.advance` 行: 「0026 時点 (close): f64 (シェーピング適用済み。GSUB + GPOS / Pair Adjustment 含む。`kern` テーブルは GPOS 不在フォントへのフォールバックとして残る)」(現状「残るかは 0026 で確定」を「残る」で確定)
- `TextMetrics` フィールド (追加) 行: 「`bounding_box` (0023 で追加済) / `leading_bearing` / `trailing_bearing` (本 issue で追加済)」
- `Font::glyph_run_for_text()` 行: 「本メソッドは本 issue で削除済み」
- `Font::from_face` シグネチャ行: 「シグネチャは `(&FontFace, f64) -> Self` のまま維持。`FontFeatureSettings` 対応は `Font::with_features` / `Font::set_feature_settings` の別メソッドで実現済み」
- `GlyphBuffer` レイアウト行: 「**SoA、glyph_ids: Vec<u16> / placements: Vec<GlyphPlacement> / clusters: Vec<u32> の 3 配列。placement / advance は f64、cluster は UTF-8 byte index**」(0001 で「0026 polish 時に本表を更新」と確定済み)

### 0021-0025 のテストへの影響

| テスト | 影響 | 対応 |
|---|---|---|
| `tests/test_font.rs::font_face_metrics` | 影響なし | 維持 |
| `tests/test_font.rs::font_char_to_glyph` | 影響なし | 維持 |
| `tests/test_font.rs::font_glyph_outline_to_path` | 影響なし | 維持 |
| `tests/test_font.rs::font_space_glyph_has_no_outline` | 影響なし | 維持 |
| `tests/test_font.rs::font_fill_text_integration` | `has_nonzero` のみ assert なので影響軽微 | 維持 |
| `tests/test_font.rs::font_measure_text_empty` | 空文字列 → advance = 0.0 は維持 | 維持 |
| `tests/test_font.rs::font_measure_text_single_char` | GPOS Single Adjustment 不在の Arial 前提なら維持 | テスト用フォントを `load_shape_test_font().or_else(load_arial)` 順序にし、shape 経由でも `measure_text("A").advance == glyph_advance(map_char_to_glyph('A'))` が成り立つこと (GPOS Single Adjustment 不在前提) を assert |
| `tests/test_font.rs::font_measure_text_newline_no_panic` | 影響なし | 維持 |
| `tests/test_font.rs::font_glyph_advance_out_of_range_is_zero` | 影響なし | 維持 |
| 0021 で書かれた `font_face_cap_x_height` 等 | 影響なし | 維持 |
| 0023 で書かれた `font_glyph_bounds_some` / `font_glyph_bounds_invalid_id` | 影響なし | 維持 |
| 0024 で書かれた `context_stroke_text_renders` / `context_stroke_text_matches_manual` | `stroke_text` が `shape()` 経由になるため、手動再実装も `shape()` ベースに揃える | 本 issue で更新 |
| 0025 で書かれた `font_kern_known_pair` / `font_kern_no_pair` | `FontFace::kern` の動作が GPOS 優先・kern フォールバックに変わるため、テスト用フォントが GPOS Pair Adjustment を持つ場合は assertion 値が変わる | 本 issue で `FontFace::kern` の GPOS 優先動作を確認するテストに書き換え。GPOS Pair Adjustment 不在フォントでの kern fallback 動作を別途検証 |
| 0025 で書かれた `font_measure_text_with_kerning` | `Font::shape` 経由の advance に変わるため、`<` 不等号は維持されるが値の根拠が GPOS / kern fallback の混在になる | 本 issue でテスト用フォントを GSUB / GPOS 付きフォントに統一して更新 |

### 0022 PBT の更新

- `pbt/tests/prop_font/main.rs` の `prop_single_char_advance` 関数本体および `#[test] fn single_char_advance()` を本 issue で削除する (GPOS Single Adjustment で破綻可能性。行番号は 0025 close 後に動くため、関数名で参照する)
- `pbt/tests/prop_font/main.rs` の `prop_size_linearity` 関数本体および `#[test] fn size_linearity()` を本 issue で削除する (GPOS Single Adjustment / Pair Adjustment が size 線形でない場合に破綻可能性)
- 0025 で追加された「隣接ペア加算の関係」PBT を本 issue で削除する (GSUB Ligature / GPOS Single Adjustment / GPOS Pair Adjustment で破綻)
- 本 issue で以下の PBT を新規追加:
  - **`prop_shape_lengths_match`** (SoA 配列長一致): `shape(text).glyph_ids.len() == placements.len() == clusters.len()`
  - **`prop_shape_glyph_count_upper_bound`** (グリフ数上限): `shape(text).glyph_ids.len() <= text.chars().count()` (リガチャでグリフ数が減ることがある)
  - **`prop_shape_clusters_monotonic`** (cluster 単調増加): `shape(text).clusters` は単調増加で、`clusters[0] == 0`、`*clusters.last().unwrap() < text.len() as u32` (text の UTF-8 byte 長)
  - **`prop_shape_advance_sum_matches_measure_text`** (shape と measure_text の総和一致): `shape(text).placements.iter().map(|p| p.advance).sum::<f64>() == Font::measure_text(text).advance` (相対 + 絶対許容範囲、0022 の `close_enough` を再利用)
  - **`prop_shape_empty_text`** (空文字列): `shape("").glyph_ids.is_empty() && placements.is_empty() && clusters.is_empty()`

### 着手前提

- 0022 が close 済
- 0024 が close 済
- 0025 が close 済 (`Font::kern` / `FontFace::kern` / `KernTable` / `KernTable::lookup` を本 issue で再利用するため)
- 0023 が close 済 (`TextMetrics::bounding_box` フィールド / `compute_bounding_box` の引数となる `Font::glyph_bounds` を本 issue で使うため)
- 0001 メタ issue の前提 concrete issue 群 (fuzzing 基盤 / ベースライン benchmark 計測基盤 / Compound Glyph point-matching 実装) が close 済
- 「テスト用フォント選定」tracked の段階的選定 (0001 メタ issue) のうち、**GSUB / GPOS 付きフォントの追加選定が完了し配置場所が決定済** (tracked 全体の close は 0027 の CFF / CFF2 フォント追加にも依存するため本 issue 着手前提には含めない)。GSUB Ligature Substitution (`liga` feature) を持つフォントおよび GPOS Pair Adjustment Lookup を持つフォントを (同じフォントで両方カバーするのが望ましい) リポジトリに配置済み

### 開発初期テスト未カバーリスクの許容

GSUB / GPOS 付きフォントが未配置のまま本 issue を着手すると単体テスト (`font_shape_simple` / `font_shape_ligature` / `font_shape_feature_disabled` / `font_shape_kern` 等) が全件スキップされ、PBT も `Font::shape(text).glyph_ids.len() <= text.chars().count()` のような弱い不変条件しか検証できない状態になる (リガチャ実装漏れがあっても `==` で成立してパス)。

本 issue ではこのリスクを許容しない方針として、着手前提 (GSUB / GPOS 付きフォントの追加選定完了) を厳密に守る。開発手順上、本 issue 着手の最初の段階で GSUB / GPOS 付きフォントの配置を確認すること。配置がまだなら「テスト用フォント選定 tracked」の GSUB / GPOS 段階完了を先に進める。

### close 前提

- 上記着手前提を満たし、本 issue 内で `Font::shape` / `Font::shape_into` 追加、3 経路 (`fill_text` / `measure_text` / `stroke_text`) の書き換え、`glyph_run_for_text` 削除、0021-0025 テストの影響対応、0022 PBT (`prop_single_char_advance` / `prop_size_linearity`) と 0025 PBT (隣接ペア加算) の削除、新規 PBT 5 件の追加、0001 メタ issue 段階拡張表の更新が完了している
- CI でテストがパス
- fuzz target `parse_gsub.rs` / `parse_gpos.rs` を追加し `cargo fuzz build` でコンパイル可能 (24 時間連続実行収束は 0001 メタ issue の完了条件)

### ドキュメント

- `docs/BLEND2D.md` L444 / L446 / L447 / L448 / L450 / L456 の raden 列に新規 API 名を追記 (状態列は本 issue で変更しない、0001 メタ issue close PR で整理)
- `CHANGES.md` に `CHANGE` 種別と `ADD` 種別を追加 (詳細は解決方法 7 参照)

## 解決方法

### 1. GSUB / GPOS テーブルパースの追加 (`src/font/tables.rs`)

`TAG_GSUB: u32 = tag(b"GSUB")` / `TAG_GPOS: u32 = tag(b"GPOS")` 定数を追加 (`tables.rs:112-118` 周辺)。

構造体定義 (フィールド可視性は `KernTable` パターンに揃える):

```rust
#[derive(Clone, Default)]
pub(crate) struct GsubTable {
    /// `liga` feature index (FeatureList 内のインデックス)。本 issue は ScriptList で
    /// 'DFLT' を最優先、なければ 'latn' を採用済みの結果。複数 feature がある場合は最初の 1 つ。
    pub liga_lookups: Vec<u16>,
    pub clig_lookups: Vec<u16>,
    pub lookups: Vec<GsubLookup>,
}

#[derive(Clone)]
pub(crate) enum GsubLookup {
    Single(GsubSingleSubst),
    Ligature(GsubLigatureSubst),
    Unsupported,
}

#[derive(Clone)]
pub(crate) struct GsubSingleSubst {
    pub coverage: Coverage,
    pub format: GsubSingleSubstFormat,
}

#[derive(Clone)]
pub(crate) enum GsubSingleSubstFormat {
    /// Format 1: delta_glyph_id 加算
    DeltaGlyphID(i16),
    /// Format 2: substitute glyph IDs 配列
    SubstituteGlyphIDs(Vec<u16>),
}

#[derive(Clone)]
pub(crate) struct GsubLigatureSubst {
    pub coverage: Coverage,
    /// LigatureSet[Coverage Index] のリスト。各エントリは Ligature の配列
    pub ligature_sets: Vec<Vec<GsubLigature>>,
}

#[derive(Clone)]
pub(crate) struct GsubLigature {
    pub ligature_glyph: u16,
    /// 合字を構成する **2 番目以降のグリフ ID のみ** を保持する (N-1 要素)。
    /// 1 番目のグリフは parent Lookup の Coverage Table が指し示すグリフのため省略。
    /// `apply_gsub_lookup` で `buf.glyph_ids[i + 1 + k] == component_glyph_ids[k]` の
    /// 形で照合する (i は Coverage マッチ位置)。
    pub component_glyph_ids: Vec<u16>,
}

#[derive(Clone, Default)]
pub(crate) struct GposTable {
    pub kern_lookups: Vec<u16>,
    pub lookups: Vec<GposLookup>,
}

#[derive(Clone)]
pub(crate) enum GposLookup {
    Single(GposSingleAdjust),
    Pair(GposPairAdjust),
    Unsupported,
}

#[derive(Clone)]
pub(crate) struct GposSingleAdjust {
    pub coverage: Coverage,
    pub format: GposSingleAdjustFormat,
}

#[derive(Clone)]
pub(crate) enum GposSingleAdjustFormat {
    /// Format 1: 全 Coverage に同じ ValueRecord
    Uniform(ValueRecord),
    /// Format 2: Coverage Index 別の ValueRecord
    PerGlyph(Vec<ValueRecord>),
}

#[derive(Clone)]
pub(crate) struct GposPairAdjust {
    pub coverage: Coverage,
    pub format: GposPairAdjustFormat,
}

#[derive(Clone)]
pub(crate) enum GposPairAdjustFormat {
    /// Format 1: PairSet[Coverage Index] のリスト
    PairSet(Vec<Vec<GposPair>>),
    /// Format 2: Class1 × Class2 マトリクス
    Class {
        class_def_1: ClassDef,
        class_def_2: ClassDef,
        class1_count: u16,
        class2_count: u16,
        records: Vec<(ValueRecord, ValueRecord)>, // [class1 * class2_count + class2]
    },
}

#[derive(Clone)]
pub(crate) struct GposPair {
    pub second_glyph: u16,
    pub value1: ValueRecord,
    pub value2: ValueRecord,
}

#[derive(Clone, Default)]
pub(crate) struct ValueRecord {
    pub x_placement: i16,
    pub y_placement: i16,
    pub x_advance: i16,
    pub y_advance: i16,
}

#[derive(Clone)]
pub(crate) enum Coverage {
    /// Format 1: glyph 列挙
    GlyphList(Vec<u16>),
    /// Format 2: (start, end, start_coverage_index) のタプル列
    RangeList(Vec<(u16, u16, u16)>),
}

#[derive(Clone)]
pub(crate) enum ClassDef {
    Format1 { start_glyph: u16, class_values: Vec<u16> },
    Format2 { ranges: Vec<(u16, u16, u16)> }, // (start, end, class)
}

impl GsubTable {
    pub(crate) fn lookup_at(&self, index: u16) -> Option<&GsubLookup> {
        self.lookups.get(index as usize)
    }
}

impl GposTable {
    pub(crate) fn lookup_at(&self, index: u16) -> Option<&GposLookup> {
        self.lookups.get(index as usize)
    }

    /// (g1, g2) のペアに対する Pair Adjustment の xAdvance 値を探索する。
    /// kern feature に紐付く Pair Adjustment Lookup のみ走査。本 issue で `FontFace::kern` /
    /// `Font::kern` の内部実装で使う。
    pub(crate) fn lookup_pair_adjustment(&self, g1: u16, g2: u16) -> Option<i16> {
        for &lookup_idx in &self.kern_lookups {
            let Some(GposLookup::Pair(pair)) = self.lookup_at(lookup_idx) else { continue };
            if let Some(v) = pair.lookup(g1, g2) {
                return Some(v.x_advance);
            }
        }
        None
    }
}

impl GposPairAdjust {
    pub(crate) fn lookup(&self, g1: u16, g2: u16) -> Option<&ValueRecord> {
        let coverage_index = self.coverage.index_of(g1)?;
        match &self.format {
            GposPairAdjustFormat::PairSet(sets) => {
                let set = sets.get(coverage_index as usize)?;
                set.iter().find(|p| p.second_glyph == g2).map(|p| &p.value1)
            }
            GposPairAdjustFormat::Class { class_def_1, class_def_2, class1_count, class2_count, records } => {
                let class1 = class_def_1.class_of(g1);
                let class2 = class_def_2.class_of(g2);
                if class1 >= *class1_count || class2 >= *class2_count { return None }
                let idx = (class1 as usize) * (*class2_count as usize) + class2 as usize;
                records.get(idx).map(|(v1, _v2)| v1)
            }
        }
    }
}

impl Coverage {
    pub(crate) fn index_of(&self, glyph_id: u16) -> Option<u16> {
        match self {
            Coverage::GlyphList(list) => list.binary_search(&glyph_id).ok().map(|i| i as u16),
            Coverage::RangeList(ranges) => {
                for &(start, end, start_idx) in ranges {
                    if glyph_id >= start && glyph_id <= end {
                        return Some(start_idx + (glyph_id - start));
                    }
                }
                None
            }
        }
    }
}

impl ClassDef {
    pub(crate) fn class_of(&self, glyph_id: u16) -> u16 {
        match self {
            ClassDef::Format1 { start_glyph, class_values } => {
                if glyph_id < *start_glyph { return 0 }
                class_values.get((glyph_id - start_glyph) as usize).copied().unwrap_or(0)
            }
            ClassDef::Format2 { ranges } => {
                for &(start, end, class) in ranges {
                    if glyph_id >= start && glyph_id <= end { return class }
                }
                0
            }
        }
    }
}
```

`parse_gsub` / `parse_gpos` の擬似コード (寛容方針、`Result` でラップせず `GsubTable` / `GposTable` 直返し。Extension Lookup を再帰的に unwrap):

```text
fn parse_gsub(data: &[u8], rec: TableRecord) -> GsubTable:
    let mut out = GsubTable::default()
    let off = rec.offset as usize
    let table_end_u64 = off as u64 + rec.length as u64
    if table_end_u64 > data.len() as u64 or rec.length < 10:
        return out
    let table_end = table_end_u64 as usize

    // GSUB Header v1.0: version (u32) + scriptList (u16) + featureList (u16) + lookupList (u16) = 10 バイト
    // v1.1 では featureVariations オフセット (u32) が末尾に追加されるが本 issue では無視
    let version = read_u32(data, off)?
    if version != 0x00010000 and version != 0x00010001:
        return out
    let script_list_off = off + read_u16(data, off + 4)? as usize
    let feature_list_off = off + read_u16(data, off + 6)? as usize
    let lookup_list_off = off + read_u16(data, off + 8)? as usize

    // 1. ScriptList から 'DFLT' を優先、なければ 'latn' を選ぶ。DefaultLangSys のみ。
    // find_script(data, script_list_off, table_end) -> Option<usize> (Script Table の絶対オフセット)
    // tag 'DFLT' (0x44464C54) を優先、なければ 'latn' (0x6C61746E)。両方無ければ None
    let Some(script_off) = find_script(data, script_list_off, table_end) else { return out }
    let default_lang_sys_off = read_u16(data, script_off)? as usize
    if default_lang_sys_off == 0:
        return out
    let lang_sys_off = script_off + default_lang_sys_off

    // 2. DefaultLangSys から FeatureIndex 列を取得 (RequiredFeatureIndex は無視)
    let feature_count = read_u16(data, lang_sys_off + 4)? as usize
    let feature_indices: Vec<u16> = (0..feature_count)
        .map(|i| read_u16(data, lang_sys_off + 6 + i * 2))
        .collect()

    // 3. FeatureList から FeatureRecord を順次評価し、tag が 'liga' / 'clig' のものを採用
    let total_features = read_u16(data, feature_list_off)? as usize
    for &fi in &feature_indices:
        if fi as usize >= total_features: continue
        let feature_record_off = feature_list_off + 2 + (fi as usize) * 6
        let tag = read_u32(data, feature_record_off)?
        let feature_table_off = feature_list_off + read_u16(data, feature_record_off + 4)? as usize
        // Feature Table の LookupIndex 列
        let lookup_index_count = read_u16(data, feature_table_off + 2)? as usize
        let lookup_indices: Vec<u16> = (0..lookup_index_count)
            .map(|i| read_u16(data, feature_table_off + 4 + i * 2))
            .collect()
        match tag:
            0x6C696761 /* 'liga' */ => out.liga_lookups.extend(lookup_indices),
            0x636C6967 /* 'clig' */ => out.clig_lookups.extend(lookup_indices),
            _ => continue

    // 4. LookupList から各 Lookup を parse。サポート外 Type は GsubLookup::Unsupported
    let lookup_count = read_u16(data, lookup_list_off)? as usize
    out.lookups = Vec::new()
    for i in 0..lookup_count:
        let lookup_off = lookup_list_off + read_u16(data, lookup_list_off + 2 + i * 2)? as usize
        out.lookups.push(parse_gsub_lookup(data, lookup_off, table_end))

    return out

fn parse_gsub_lookup(data: &[u8], lookup_off: usize, table_end: usize) -> GsubLookup:
    let lookup_type = read_u16(data, lookup_off)?
    let _lookup_flag = read_u16(data, lookup_off + 2)?
    let subtable_count = read_u16(data, lookup_off + 4)? as usize
    // 本 issue では最初の subtable のみ処理 (簡略化)。複数 subtable の累積は font 安定化前の別 issue
    if subtable_count == 0: return GsubLookup::Unsupported
    let subtable_off = lookup_off + read_u16(data, lookup_off + 6)? as usize

    match lookup_type:
        1 => parse_gsub_single(data, subtable_off),
        4 => parse_gsub_ligature(data, subtable_off),
        7 => {
            // Extension Substitution
            let ext_format = read_u16(data, subtable_off)?
            if ext_format != 1: return GsubLookup::Unsupported
            let inner_lookup_type = read_u16(data, subtable_off + 2)?
            let inner_subtable_off = subtable_off + read_u32(data, subtable_off + 4)? as usize
            match inner_lookup_type:
                1 => parse_gsub_single(data, inner_subtable_off),
                4 => parse_gsub_ligature(data, inner_subtable_off),
                _ => GsubLookup::Unsupported,
        }
        _ => GsubLookup::Unsupported

fn parse_gsub_single(data, subtable_off) -> GsubLookup:
    let format = read_u16(data, subtable_off)?
    let coverage_off = subtable_off + read_u16(data, subtable_off + 2)? as usize
    let coverage = parse_coverage(data, coverage_off)
    match format:
        1 => {
            let delta = read_i16(data, subtable_off + 4)?
            GsubLookup::Single(GsubSingleSubst { coverage, format: GsubSingleSubstFormat::DeltaGlyphID(delta) })
        }
        2 => {
            let glyph_count = read_u16(data, subtable_off + 4)? as usize
            let subs: Vec<u16> = (0..glyph_count).map(|i| read_u16(data, subtable_off + 6 + i * 2)).collect()
            GsubLookup::Single(GsubSingleSubst { coverage, format: GsubSingleSubstFormat::SubstituteGlyphIDs(subs) })
        }
        _ => GsubLookup::Unsupported

fn parse_gsub_ligature(data, subtable_off) -> GsubLookup:
    let format = read_u16(data, subtable_off)?
    if format != 1: return GsubLookup::Unsupported
    let coverage_off = subtable_off + read_u16(data, subtable_off + 2)? as usize
    let coverage = parse_coverage(data, coverage_off)
    let ligature_set_count = read_u16(data, subtable_off + 4)? as usize
    let mut ligature_sets: Vec<Vec<GsubLigature>> = Vec::new()
    for i in 0..ligature_set_count:
        let set_off = subtable_off + read_u16(data, subtable_off + 6 + i * 2)? as usize
        let ligature_count = read_u16(data, set_off)? as usize
        let mut ligatures: Vec<GsubLigature> = Vec::new()
        for j in 0..ligature_count:
            let lig_off = set_off + read_u16(data, set_off + 2 + j * 2)? as usize
            let ligature_glyph = read_u16(data, lig_off)?
            let component_count = read_u16(data, lig_off + 2)? as usize
            // component_glyph_ids[0] は coverage が指すグリフなので省略、ここでは [1..component_count] を読む
            let component_glyph_ids: Vec<u16> = (1..component_count)
                .map(|k| read_u16(data, lig_off + 4 + (k - 1) * 2))
                .collect()
            ligatures.push(GsubLigature { ligature_glyph, component_glyph_ids })
        ligature_sets.push(ligatures)
    GsubLookup::Ligature(GsubLigatureSubst { coverage, ligature_sets })

fn parse_coverage(data: &[u8], coverage_off: usize) -> Coverage:
    let format = read_u16(data, coverage_off)?
    match format:
        1 => {
            let glyph_count = read_u16(data, coverage_off + 2)? as usize
            let glyphs: Vec<u16> = (0..glyph_count).map(|i| read_u16(data, coverage_off + 4 + i * 2)).collect()
            Coverage::GlyphList(glyphs)
        }
        2 => {
            let range_count = read_u16(data, coverage_off + 2)? as usize
            let ranges: Vec<(u16, u16, u16)> = (0..range_count).map(|i| {
                let base = coverage_off + 4 + i * 6
                (read_u16(data, base)?, read_u16(data, base + 2)?, read_u16(data, base + 4)?)
            }).collect()
            Coverage::RangeList(ranges)
        }
        _ => Coverage::GlyphList(Vec::new())  // 未知の format は空 Coverage 扱い (寛容)
```

`parse_gpos` は同じパターンだが、feature tag として `'kern'` (0x6B65726E) のみを採用し、Lookup Type 1 (Single Adjustment) / Type 2 (Pair Adjustment) / Type 9 (Extension) をパースする。`ValueRecord` の読み出しヘルパー:

```text
fn read_value_record(data: &[u8], off: usize, value_format: u16) -> (ValueRecord, usize):
    let mut vr = ValueRecord::default()
    let mut cursor = off
    if value_format & 0x0001 != 0:
        vr.x_placement = read_i16(data, cursor)?; cursor += 2
    if value_format & 0x0002 != 0:
        vr.y_placement = read_i16(data, cursor)?; cursor += 2
    if value_format & 0x0004 != 0:
        vr.x_advance = read_i16(data, cursor)?; cursor += 2
    if value_format & 0x0008 != 0:
        vr.y_advance = read_i16(data, cursor)?; cursor += 2
    // bits 4-7 (Device Table offsets) は読み飛ばし
    for bit in 4..8:
        if value_format & (1 << bit) != 0:
            cursor += 2  // 2 バイトのオフセットを読み飛ばす
    (vr, cursor - off)
```

GPOS Pair Adjustment Format 1 / 2、Single Adjustment Format 1 / 2、Coverage、ClassDef のパースは上記パターンに準じる (Microsoft OpenType Specification GPOS 章を参照しつつ、本 issue の擬似コードに沿って実装)。

すべての `read_u16` / `read_i16` / `read_u32` の `?` は Rust 実装時に `let Ok(v) = expr else { return ... };` パターンに展開する (寛容方針、途中まで読めた `out` を返す)。

`parse_all` で:

```rust
let gsub = dir.find(TAG_GSUB).map(|rec| parse_gsub(data, rec)).unwrap_or_default();
let gpos = dir.find(TAG_GPOS).map(|rec| parse_gpos(data, rec)).unwrap_or_default();
```

を、0025 close 後に `parse_all` 内に存在する `let kern = ...;` (0025 で `cmap` の直後に挿入済) の直後、`Ok(ParsedTables { ... })` 構造体リテラル直前に追加する。`ParsedTables` 構造体リテラルにも `gsub,` / `gpos,` フィールドを `kern,` の直後に追加する (構造体リテラル更新を忘れるとコンパイルエラーになるため必須)。`ParsedTables` 構造体定義 (0025 close 後の状態) には `pub gsub: GsubTable` / `pub gpos: GposTable` を `kern: KernTable` の直後に追加する。

`Vec::with_capacity` は使わず `Vec::new()` から push する (`shiguredo-rust` 規約、`parse_kern` と同じ方針)。**擬似コード内に登場する `.collect::<Vec<_>>()` パターン (例: `let feature_indices: Vec<u16> = (0..feature_count).map(|i| read_u16(...)).collect()` 等) は Rust 実装時に `let mut v = Vec::new(); for i in 0..count { v.push(read_u16(...)) }; let v = v;` 形式に展開し、`collect` の `size_hint` 経由での暗黙の事前確保を避ける**。既存 `tables.rs` 内の他パーサ (`TableDirectory::parse` / `parse_hmtx` / `parse_cmap_format4` / `parse_cmap_format12`) は規約と異なる `Vec::with_capacity` を使っているが、本 issue では撤去せず別 issue で扱う (0025 と同方針)。

### 2. 構造体定義 (`src/font/mod.rs`)

`GlyphBuffer` / `GlyphPlacement` / `FontFeatureSettings` を上記「設計方針」のとおり定義する (`Default` 派生・`#[non_exhaustive]` 付与済み)。`pub use font::{... GlyphBuffer, GlyphPlacement, FontFeatureSettings, ...}` を `src/lib.rs:20` の `pub use font::{Font, FontData, FontError, FontFace, GlyphBounds, TextMetrics};` に alphabetical 順で追加する (`FontFeatureSettings` / `GlyphBuffer` / `GlyphPlacement` を挿入)。

`Font` 構造体 (`src/font/mod.rs:99-104`) に `feature_settings: FontFeatureSettings` フィールドを `scale: f64` の直後に追加 (private)。`#[derive(Clone)]` を追加。

### 3. `Font::with_features()` / `set_feature_settings()` / `feature_settings()` / `clone_with_features()` 追加

```rust
impl Font {
    pub fn with_features(face: &FontFace, size: f64, features: FontFeatureSettings) -> Self {
        let scale = size / face.tables.units_per_em as f64;
        let inner = Arc::new(FontFaceInner {
            data: Arc::clone(&face.data),
            tables: face.tables.clone(),
        });
        Self { face: inner, size, scale, feature_settings: features }
    }

    pub fn from_face(face: &FontFace, size: f64) -> Self {
        Self::with_features(face, size, FontFeatureSettings::default())
    }

    pub fn set_feature_settings(&mut self, features: FontFeatureSettings) {
        self.feature_settings = features;
    }

    pub fn feature_settings(&self) -> &FontFeatureSettings {
        &self.feature_settings
    }

    pub fn clone_with_features(&self, features: FontFeatureSettings) -> Self {
        let mut clone = self.clone();
        clone.feature_settings = features;
        clone
    }
}
```

### 4. `Font::shape()` / `Font::shape_into()` 追加

```rust
impl Font {
    /// 新規 GlyphBuffer に shape 結果を入れて返す (Blend2D `BLFont::shape` 互換寄り)。
    pub fn shape(&self, text: &str) -> GlyphBuffer {
        let mut buf = GlyphBuffer::default();
        self.shape_into(text, &mut buf);
        buf
    }

    /// 既存 GlyphBuffer を再利用して shape する (Context::fill_text / stroke_text 経路で使う)。
    pub fn shape_into(&self, text: &str, buf: &mut GlyphBuffer) {
        buf.glyph_ids.clear();
        buf.placements.clear();
        buf.clusters.clear();

        // 1. cmap で text → グリフ ID 列に変換 (cluster は UTF-8 byte index)
        for (byte_idx, ch) in text.char_indices() {
            let gid = self.map_char_to_glyph(ch);
            buf.glyph_ids.push(gid);
            buf.clusters.push(byte_idx as u32);
            buf.placements.push(GlyphPlacement {
                offset_x: 0.0,
                offset_y: 0.0,
                advance: self.glyph_advance(gid),
            });
        }

        // 2. GSUB 適用 (features.liga / features.clig が apply_gsub 内で参照される)
        shape::apply_gsub(&self.face.tables.gsub, buf, &self.feature_settings, self);

        // 3. GPOS 適用 (kern feature 有効時のみ)。Pair Adjustment 経路を試みたかを bool で返す
        let gpos_kern_attempted = if self.feature_settings.kern {
            shape::apply_gpos(&self.face.tables.gpos, buf, self)
        } else {
            false
        };

        // 4. GPOS 経路を取らなかった (GPOS の kern_lookups が空) かつ kern feature 有効なら
        //    kern テーブルにフォールバック (二重適用を防ぐ)
        if self.feature_settings.kern && !gpos_kern_attempted {
            shape::apply_kern_fallback(&self.face.tables.kern, buf, self);
        }
    }
}
```

`apply_gsub` / `apply_gpos` / `apply_kern_fallback` の擬似コード:

```text
pub(crate) fn apply_gsub(gsub: &GsubTable, buf: &mut GlyphBuffer, features: &FontFeatureSettings, font: &Font):
    let mut lookup_indices: Vec<u16> = Vec::new()
    if features.liga: lookup_indices.extend(&gsub.liga_lookups)
    if features.clig: lookup_indices.extend(&gsub.clig_lookups)
    for &idx in &lookup_indices:
        if let Some(lookup) = gsub.lookup_at(idx):
            apply_gsub_lookup(lookup, buf, font)

fn apply_gsub_lookup(lookup: &GsubLookup, buf: &mut GlyphBuffer, font: &Font):
    match lookup:
        GsubLookup::Single(s) => {
            for i in 0..buf.glyph_ids.len():
                if let Some(_) = s.coverage.index_of(buf.glyph_ids[i]):
                    match &s.format:
                        DeltaGlyphID(d) => buf.glyph_ids[i] = ((buf.glyph_ids[i] as i32) + (*d as i32)) as u16,
                        SubstituteGlyphIDs(subs) => {
                            let ci = s.coverage.index_of(buf.glyph_ids[i]).unwrap()
                            if let Some(&sub) = subs.get(ci as usize):
                                buf.glyph_ids[i] = sub
        }
        GsubLookup::Ligature(l) => {
            let mut i = 0
            while i < buf.glyph_ids.len():
                if let Some(ci) = l.coverage.index_of(buf.glyph_ids[i]):
                    if let Some(ligatures) = l.ligature_sets.get(ci as usize):
                        // 各 Ligature を順に試し、最初にマッチしたものを採用 (greedy)
                        // matched は &GsubLigature の借用で保持 (Vec の clone 回避)
                        let mut matched: Option<&GsubLigature> = None
                        for lig in ligatures:
                            let n = lig.component_glyph_ids.len()
                            if i + 1 + n > buf.glyph_ids.len(): continue
                            let matches = (0..n).all(|k|
                                buf.glyph_ids[i + 1 + k] == lig.component_glyph_ids[k])
                            if matches:
                                matched = Some(lig)
                                break
                        if let Some(lig) = matched:
                            let consumed = 1 + lig.component_glyph_ids.len()
                            let ligature_glyph = lig.ligature_glyph
                            // i 番目を ligature_glyph に置換し、placement を合字 advance に更新
                            buf.glyph_ids[i] = ligature_glyph
                            buf.placements[i] = GlyphPlacement {
                                offset_x: 0.0,
                                offset_y: 0.0,
                                advance: font.glyph_advance(ligature_glyph),
                            }
                            // i+1..i+consumed を 3 配列同期 drain で削除
                            // (SoA 不変条件 glyph_ids.len() == placements.len() == clusters.len() を維持)
                            buf.glyph_ids.drain(i + 1 .. i + consumed)
                            buf.placements.drain(i + 1 .. i + consumed)
                            buf.clusters.drain(i + 1 .. i + consumed)
                            // cluster[i] は元の最初の char の byte index のまま (HarfBuzz 慣行)
                            i += 1
                            continue
                i += 1
        GsubLookup::Unsupported => (),
```

```text
pub(crate) fn apply_gpos(gpos: &GposTable, buf: &mut GlyphBuffer, font: &Font) -> bool:
    // kern_lookups が空なら GPOS 経路を試みなかったことを返し、呼び出し側で
    // kern テーブルフォールバックを実行させる
    if gpos.kern_lookups.is_empty(): return false
    let scale = font.scale()
    for &idx in &gpos.kern_lookups:
        if let Some(lookup) = gpos.lookup_at(idx):
            match lookup:
                GposLookup::Single(s) => apply_gpos_single(s, buf, scale),
                GposLookup::Pair(p) => apply_gpos_pair(p, buf, scale),
                GposLookup::Unsupported => ()
    // kern feature 紐付き Lookup が 1 件以上あった時点で kern fallback は不要
    // (実際に適用されたペア数によらず、二重適用を防ぐ)
    true

fn apply_gpos_single(s: &GposSingleAdjust, buf: &mut GlyphBuffer, scale: f64):
    for i in 0..buf.glyph_ids.len():
        if let Some(ci) = s.coverage.index_of(buf.glyph_ids[i]):
            let vr = match &s.format:
                Uniform(vr) => vr,
                PerGlyph(vrs) => match vrs.get(ci as usize): Some(v) => v, None => continue,
            buf.placements[i].offset_x += vr.x_placement as f64 * scale
            buf.placements[i].offset_y += vr.y_placement as f64 * scale
            buf.placements[i].advance += vr.x_advance as f64 * scale
            // y_advance は本 issue では無視 (水平テキスト前提)

fn apply_gpos_pair(p: &GposPairAdjust, buf: &mut GlyphBuffer, scale: f64):
    for i in 0..buf.glyph_ids.len().saturating_sub(1):
        let g1 = buf.glyph_ids[i]
        let g2 = buf.glyph_ids[i + 1]
        if let Some(vr) = p.lookup(g1, g2):
            buf.placements[i].advance += vr.x_advance as f64 * scale
            // value2 (g2 側) の適用は将来課題、本 issue では value1 のみ

pub(crate) fn apply_kern_fallback(kern: &KernTable, buf: &mut GlyphBuffer, font: &Font):
    let scale = font.scale()
    for i in 0..buf.glyph_ids.len().saturating_sub(1):
        if let Some(v) = kern.lookup(buf.glyph_ids[i], buf.glyph_ids[i + 1]):
            buf.placements[i].advance += v as f64 * scale
```

### 5. `Font::measure_text` の修正 (`src/font/mod.rs`)

```rust
pub fn measure_text(&self, text: &str) -> TextMetrics {
    let buffer = self.shape(text);
    let advance: f64 = buffer.placements.iter().map(|p| p.advance).sum();
    let bounding_box = compute_bounding_box(&buffer, self);
    let leading_bearing = compute_leading_bearing(&buffer, self);
    let trailing_bearing = compute_trailing_bearing(&buffer, self);
    TextMetrics { advance, bounding_box, leading_bearing, trailing_bearing }
}

fn compute_leading_bearing(buffer: &GlyphBuffer, font: &Font) -> f64 {
    if buffer.glyph_ids.is_empty() { return 0.0 }
    let glyph_id = buffer.glyph_ids[0];
    let placement = buffer.placements[0];
    match font.glyph_bounds(glyph_id) {
        Some(gb) => gb.x_min + placement.offset_x,
        None => 0.0,
    }
}

fn compute_trailing_bearing(buffer: &GlyphBuffer, font: &Font) -> f64 {
    if buffer.glyph_ids.is_empty() { return 0.0 }
    let last = buffer.glyph_ids.len() - 1;
    let glyph_id = buffer.glyph_ids[last];
    let placement = buffer.placements[last];
    match font.glyph_bounds(glyph_id) {
        Some(gb) => placement.advance - (gb.x_max + placement.offset_x),
        None => 0.0,
    }
}
```

`compute_bounding_box` は「設計方針」セクションの擬似コードに従い `src/font/mod.rs` に private fn として追加。

### 6. `Context::fill_text` / `Context::stroke_text` の修正 (`src/api/context.rs`)

`Context` 構造体の `tmp_glyph_run: Vec<(u16, f64)>` フィールドを削除し、`tmp_shape_buffer: GlyphBuffer` フィールドに置換する。`Context::new` の `tmp_glyph_run: Vec::new(),` を `tmp_shape_buffer: GlyphBuffer::default(),` に変更する (行番号は 0022 / 0024 / 0025 close 後の状態に依存するため、本 issue 着手時に `grep tmp_glyph_run src/api/context.rs` で確認する)。

`fill_text` (`src/api/context.rs:1148-1172`) を以下に書き換える:

```rust
pub fn fill_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
    let mut path = std::mem::take(&mut self.tmp_path);
    let mut buffer = std::mem::take(&mut self.tmp_shape_buffer);
    path.clear();

    font.shape_into(text, &mut buffer);

    let mut cursor_x = x;
    for (i, &glyph_id) in buffer.glyph_ids.iter().enumerate() {
        let placement = buffer.placements[i];
        if glyph_id != 0 {
            let _ = font.append_glyph_outline(
                glyph_id,
                cursor_x + placement.offset_x,
                y + placement.offset_y,
                &mut path,
            );
        }
        cursor_x += placement.advance;
    }

    if !path.is_empty() {
        self.fill_path(&path);
    }

    self.tmp_path = path;
    self.tmp_shape_buffer = buffer;
}
```

`stroke_text` (0024 で追加、0025 で kern 加算追記済) を同様に書き換える (`self.fill_path(&path)` を `self.stroke_path(&path)` に差し替えるのみ。0025 で追加した `cursor_x += font.kern(...)` 隣接ペア加算ロジックは `font.shape_into` 内部に統合されたため、本 issue で削除する)。

### 7. CHANGES.md 更新

`shiguredo-changelog` スキルと 0001 メタ issue「font 公開 API の安定化方針」に従い、`## develop` 直下 (`### misc` の前) に `CHANGE` 種別、`### misc` 直下に `ADD` 種別を追加する。0022 / 0025 で既に書かれているエントリは維持する。`@<author>` は実装者の GitHub ユーザー名 (例: `@voluntas`) で置換する。

`## develop` 直下に追加する `CHANGE` エントリ:

```
- [CHANGE] `Font::measure_text` の戻り値 `TextMetrics.advance` を OpenType GSUB / GPOS シェーピング適用済みの値に変更する
  - @<author>
- [CHANGE] `Font::measure_text` の戻り値 `TextMetrics.bounding_box` を OpenType GSUB / GPOS シェーピング適用済みの bbox に変更する
  - @<author>
- [CHANGE] `Context::fill_text` の描画結果のグリフ列・位置を OpenType GSUB / GPOS シェーピング適用済みに変更する
  - @<author>
- [CHANGE] `Context::stroke_text` の描画結果のグリフ列・位置を OpenType GSUB / GPOS シェーピング適用済みに変更する
  - @<author>
- [CHANGE] `FontFace::kern` / `Font::kern` の動作を GPOS Pair Adjustment 優先・kern テーブルフォールバックに変更する
  - @<author>
```

`## develop` の `### misc` 直下に追加する `ADD` エントリ:

```
- [ADD] `GlyphBuffer` 構造体を追加する
  - @<author>
- [ADD] `GlyphPlacement` 構造体を追加する
  - @<author>
- [ADD] `FontFeatureSettings` 構造体を追加する
  - @<author>
- [ADD] `Font::with_features` を追加する
  - @<author>
- [ADD] `Font::set_feature_settings` を追加する
  - @<author>
- [ADD] `Font::feature_settings` を追加する
  - @<author>
- [ADD] `Font::clone_with_features` を追加する
  - @<author>
- [ADD] `Font::shape` を追加する
  - @<author>
- [ADD] `Font::shape_into` を追加する
  - @<author>
- [ADD] `TextMetrics` に `leading_bearing` / `trailing_bearing` フィールドを追加する
  - @<author>
- [ADD] `Font` に `Clone` 実装を追加する
  - @<author>
```

### 8. BLEND2D.md 更新

`docs/BLEND2D.md` の raden 列に新規 API 名を以下のとおり追記する:

- L444 (`feature_settings()` / `set_feature_settings()` / `reset_feature_settings()`): raden 列に `FontFeatureSettings` / `Font::with_features()` / `Font::set_feature_settings()` / `Font::feature_settings()` を追記
- L446 (`shape(BLGlyphBuffer&)`): raden 列に `Font::shape(text) -> GlyphBuffer` / `Font::shape_into(text, &mut GlyphBuffer)` を追記
- L447 (`map_text_to_glyphs(BLGlyphBuffer&)`): raden 列の 0022 で追記された `Font::glyph_run_for_text (pub(crate))` 文字列を削除し、`Font::shape() / Font::shape_into() の内部処理`に置換 (公開 API は `Font::map_char_to_glyph(char) -> u16` 維持)
- L448 (`position_glyphs(BLGlyphBuffer&)`): raden 列の 0022 で追記された `Font::glyph_run_for_text (pub(crate))` 文字列を削除し、`Font::shape() / Font::shape_into() の内部処理`に置換 (公開 API は `Font::glyph_advance(u16) -> f64` 維持)
- L450 (`apply_gsub` / `apply_gpos`): raden 列に `Font::shape() の内部実装 (GSUB Type 1/4 + GPOS Type 1/2 + Extension Type 7/9 + Coverage Format 1/2)` を追記
- L456 (`BLGlyphBuffer`): raden 列に `GlyphBuffer (SoA: glyph_ids + placements + clusters)` を追記

状態列 (`未実装` / `差異あり`) は本 issue では変更しない。L455 (`get_text_metrics`、`TextMetrics`) も本 issue で `leading_bearing` / `trailing_bearing` を追加するが、状態列および raden 列の追記は 0001 メタ issue の close PR でまとめて整理する (本 issue では更新しない)。

### 9. 0001 メタ issue の段階拡張表と依存関係セクション更新

本 issue close PR 内で `issues/0001-enhance-font-module-maturity.md` の以下 2 セクションを確定値で更新する責務を持つ:

- 「段階拡張で意味が変わる API」表の各行 (詳細は「完了条件 → 0001 メタ issue 段階拡張表の更新」を参照)
- 「依存関係」セクションで「0026 → 0022, 0025」となっている記述を「0026 → 0022, 0023, 0024, 0025」に同期更新する (本 issue は `Font::glyph_bounds` を `compute_bounding_box` 経由で使い、`Context::stroke_text` (0024) もシェーピング統合するため、概念的依存に揃える)

### 10. `src/font/shape.rs` モジュール宣言

`src/font/mod.rs` 冒頭 (`pub(crate) mod glyph;` / `pub(crate) mod tables;` の直後) に `pub(crate) mod shape;` を追加する。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/tables.rs` | 既存編集 | `TAG_GSUB` / `TAG_GPOS` 定数、`GsubTable` / `GposTable` / `GsubLookup` / `GposLookup` / `ValueRecord` / `Coverage` / `ClassDef` 等の構造体定義、`parse_gsub` / `parse_gpos` および補助 fn、`ParsedTables` への追加と `parse_all` 組み込み |
| `src/font/mod.rs` | 既存編集 | `GlyphBuffer` / `GlyphPlacement` / `FontFeatureSettings` 構造体定義、`Font` への `feature_settings` フィールド追加と `#[derive(Clone)]`、`Font::with_features` / `set_feature_settings` / `feature_settings` / `clone_with_features` / `shape` / `shape_into`、`Font::from_face` の `with_features` 経由実装、`Font::measure_text` のシェーピング統合、`compute_bounding_box` / `compute_leading_bearing` / `compute_trailing_bearing` private fn、`Font::glyph_run_for_text` の削除、`TextMetrics` への `leading_bearing` / `trailing_bearing` フィールド追加、`pub(crate) mod shape;` の宣言追加、docstring 1 文置換 (`Font::measure_text` / `FontFace::kern` / `Font::kern`) |
| `src/font/shape.rs` | 新規 | `apply_gsub` / `apply_gpos` / `apply_kern_fallback` / `apply_gsub_lookup` / `apply_gpos_single` / `apply_gpos_pair` の `pub(crate) fn` 実装 |
| `src/api/context.rs` | 既存編集 | `Context.tmp_glyph_run` フィールドを `tmp_shape_buffer: GlyphBuffer` に置換、`Context::new` の初期化変更、`fill_text` / `stroke_text` を `Font::shape_into` 経由に書き換え (0025 で追加した `cursor_x += font.kern(...)` 隣接ペア加算の削除を含む) |
| `src/lib.rs` | 既存編集 | `pub use font::{... FontFeatureSettings, GlyphBuffer, GlyphPlacement, ...}` を alphabetical 順で追加 |
| `tests/test_font.rs` / `tests/test_context.rs` | 既存編集 | 単体テスト追加 (`font_shape_*` 系)、0021-0025 で書かれたテストのうちシェーピング影響を受けるものを更新 (完了条件「0021-0025 のテストへの影響」表参照) |
| `pbt/tests/prop_font/main.rs` | 既存編集 | 0022 PBT 2 件 (`prop_single_char_advance` / `prop_size_linearity` 関数本体と `#[test]`) および 0025 で追加した「隣接ペア加算」PBT の削除、本 issue で追加する 5 件の PBT (`prop_shape_lengths_match` / `prop_shape_glyph_count_upper_bound` / `prop_shape_clusters_monotonic` / `prop_shape_advance_sum_matches_measure_text` / `prop_shape_empty_text`) の追加 |
| `fuzz/fuzz_targets/parse_gsub.rs` / `parse_gpos.rs` | 新規 | GSUB / GPOS テーブル fuzz target (`parse_all` 経由ではテーブル深部 (Coverage Format 2、Class Definition Format 2) に到達しにくいため独立 target を追加) |
| `docs/BLEND2D.md` | 既存編集 | L444 / L446 / L447 / L448 / L450 / L456 の raden 列に新規 API 追記 (状態列は変更しない) |
| `issues/0001-enhance-font-module-maturity.md` | 既存編集 | 段階拡張表 (`TextMetrics.advance` / `TextMetrics` フィールド / `Font::glyph_run_for_text()` / `Font::from_face` シグネチャ / `GlyphBuffer` レイアウト) の各行を確定値で更新、および「依存関係」セクション「0026 → 0022, 0025」を「0026 → 0022, 0023, 0024, 0025」に同期更新 |
| `CHANGES.md` | 既存編集 | `## develop` 直下に `CHANGE` 5 件、`### misc` 直下に `ADD` 11 件 |

## エッジケース

- `text` が空文字列: `GlyphBuffer` の 3 配列全てが空、`measure_text` は `TextMetrics { advance: 0.0, bounding_box: None, leading_bearing: 0.0, trailing_bearing: 0.0 }`
- GSUB テーブル不在: `apply_gsub` は何もせず、`glyph_ids` は cmap 経由のまま
- GPOS テーブル不在: `apply_gpos` は `false` を返す (`kern_lookups` が空のため `gpos_kern_attempted = false`)。kern feature 有効なら `apply_kern_fallback` が呼ばれる
- GPOS / kern テーブル両方不在: カーニング適用なし (両者とも 0)
- `FontFeatureSettings::default()` で `kern: true` / `liga: true` / `clig: true` だが、フォントが対応していない feature は適用されない (GSUB / GPOS の LookupList を解釈した結果として何も起きない)
- リガチャ非対応フォント (GSUB 不在 / Ligature Substitution 不在): GSUB 適用がスキップされ、`glyph_ids.len() == text.chars().count()`
- 単一文字: 隣接ペアなし、Pair Adjustment は適用されないが Single Adjustment / Single Substitution は適用される
- `size == 0`: `scale == 0` で全 `placements[].advance == 0.0`、`offset_*` も 0
- `size < 0` / NaN / Inf: テストでロックしない (0021 / 0022 と方針共有)
- Extension Lookup の内部 Lookup Type がさらに Extension (Type 7 / 9): 仕様上は許容されないが、念のため再帰の場合は 1 段まで対応し、それ以上ネストされた場合は `Unsupported` 扱いとする
- Coverage Table Format 1 で glyph_ids が **昇順でない** 場合: `binary_search` が誤動作するため、本 issue のパース時にバリデーション (`is_sorted()` チェック) は行わず、不正データに対しては予測不能な挙動 (検索失敗) を許容する。`parse_all` 経由 fuzz target で検出される

## 隣接 issue への影響

- **0022 への影響**: `Font::glyph_run_for_text` を削除し、`Font::shape` / `Font::shape_into` に置換。`Font::measure_text` の内部実装を `shape` 経由に書き換え。0022 で書かれた残りの PBT 2 件 (`prop_single_char_advance` / `prop_size_linearity`) を削除。`Font::measure_text` の docstring 1 文を置換
- **0023 への影響**: `Font::glyph_bounds` は変更不要 (個別グリフの bbox 取得のため)。0023 で追加された `TextMetrics::bounding_box` の計算が `shape()` 経由になるため、`compute_bounding_box` ヘルパー (本 issue で導入) で再実装。0023 で書かれた PBT (個別 glyph_bounds の不変条件) は維持される
- **0024 への影響**: `Context::stroke_text` を `Font::shape_into` 経由に書き換え。0024 のテストはシェーピング適用後の描画結果に変わるため更新
- **0025 への影響**: `fill_text` / `measure_text` / `stroke_text` 内の `Font::kern` 直接呼び出しを削除し、`Font::shape_into` 経由 (内部で GPOS Pair Adjustment または kern フォールバック) に置換。`FontFace::kern` / `Font::kern` API は legacy フォント向けに維持し、docstring を「GPOS 優先、不在時 kern フォールバック」に 1 文置換更新。0025 で追加された「隣接ペア加算」PBT を削除
- **0001 メタ issue への影響**: 段階拡張表 (`TextMetrics.advance` / `TextMetrics` フィールド (追加) / `Font::glyph_run_for_text()` / `Font::from_face` シグネチャ / `GlyphBuffer` レイアウト の各行) と「依存関係」セクション (「0026 → 0022, 0025」を「0026 → 0022, 0023, 0024, 0025」に拡張) を確定値で更新する責務は本 issue にある
- **0027 への申し送り (重要)**: 0027 (CFF / CFF2 アウトライン対応) が `Font::shape` / `GlyphBuffer` / `compute_bounding_box` を CFF / CFF2 経路でも流用するため、本 issue で導入する `Font::shape` / `Font::shape_into` および `compute_bounding_box` ヘルパーは CFF / CFF2 outline でも動作するように `font.glyph_advance` / `font.glyph_bounds` 経由でアクセスする (アウトライン形式に依存しない API 設計)。0027 polish 時に確認する

## テスト戦略

### 単体テスト (`tests/test_font.rs` / `tests/test_context.rs`)

`load_shape_test_font()` ヘルパーを新規追加 (GSUB Ligature / GPOS Pair Adjustment 対応の OFL ライセンスフォントを読み込む。本 issue 着手時に「テスト用フォント選定」tracked で確定したパスから読み込む)。0025 の `load_kern_test_font()` と同じく `Option<FontFace>` を返し、不在時はテストをスキップする。

- `font_shape_simple`: `shape("Hello")` の `glyph_ids.len() == 5` を検証 (GSUB Single Substitution / Ligature 適用前提でグリフ数が変わらないことを確認)
- `font_shape_ligature`: `shape("fi")` の `glyph_ids.len() == 1` かつ `clusters[0] == 0` を strict に検証 (リガチャ実装漏れがあれば `len == 2` で fail。`load_shape_test_font` で `fi` リガチャを持つフォントを使う前提)
- `font_shape_feature_disabled`: `font.clone_with_features(FontFeatureSettings { liga: false, ..Default::default() }).shape("fi")` の `glyph_ids.len() == 2` を検証 (リガチャ無効化)
- `font_shape_kern`: `shape("AV")` の `placements[0].advance < glyph_advance('A')` を strict に検証 (GPOS Pair Adjustment または kern fallback でネガティブカーニングが適用される前提)
- `font_shape_empty`: `shape("").glyph_ids.is_empty()` 等
- `font_shape_into_reuse`: `shape_into` で既存 buffer を再利用しても結果が一致することを検証 (clear → shape_into → 結果取得 → 別文字列で shape_into → 結果取得 で 2 回の結果が独立)
- `font_with_features_and_set`: `Font::with_features` / `Font::set_feature_settings` で設定した feature が `shape` 結果に反映されることを検証
- `font_measure_text_with_shaping`: `measure_text("AV").advance` が `shape("AV").placements.iter().map(|p| p.advance).sum()` と一致 (相対 + 絶対許容)
- `font_text_metrics_leading_trailing_bearing`: `measure_text("A")` の `leading_bearing` / `trailing_bearing` が `glyph_bounds('A')` から計算された値と一致

テストのアサーションメッセージは日本語で書く (`CLAUDE.md` 規約に従う)。

### PBT (`pbt/tests/prop_font/main.rs`)

本 issue で追加する PBT (`load_shape_test_font` 経由、不在時はテスト全件スキップ):

- **`prop_shape_lengths_match`**: 任意の ASCII printable 文字列 `text` に対し `shape(text).glyph_ids.len() == placements.len() == clusters.len()` (SoA 配列長一致)
- **`prop_shape_glyph_count_upper_bound`**: `shape(text).glyph_ids.len() <= text.chars().count()`
- **`prop_shape_clusters_monotonic`**: 空でない場合 `shape(text).clusters` は単調非減少、`clusters[0] == 0`、`*clusters.last().unwrap() < text.len() as u32`
- **`prop_shape_advance_sum_matches_measure_text`**: `shape(text).placements.iter().map(|p| p.advance).sum::<f64>()` と `measure_text(text).advance` が 0022 の `close_enough` (相対 `1e-9` + 絶対 `1e-12`) で一致
- **`prop_shape_empty_text`**: `shape("").glyph_ids.is_empty() && shape("").placements.is_empty() && shape("").clusters.is_empty()`

ASCII printable 戦略 (`ascii_printable_string`) は 0022 で定義済みのものを共用する。`close_enough` も同様。

### 削除する既存 PBT

- `prop_concatenation` 関数本体と `#[test]`: 0025 で削除済 (本 issue では追加削除なし)
- `prop_non_negative` 関数本体と `#[test]`: 0025 で削除済 (本 issue では追加削除なし)
- `prop_single_char_advance` 関数本体と `#[test] fn single_char_advance()`: 本 issue で削除 (GPOS Single Adjustment で破綻可能性)
- `prop_size_linearity` 関数本体と `#[test] fn size_linearity()`: 本 issue で削除 (GPOS Single Adjustment / Pair Adjustment が size 線形でない場合に破綻可能性)
- 0025 で追加した「隣接ペア加算の関係」PBT 関数本体と `#[test]`: 本 issue で削除 (GSUB Ligature / GPOS Single Adjustment / GPOS Pair Adjustment で破綻)

### Fuzzing

`parse_all` 経由の既存 fuzz target は GSUB / GPOS のテーブル深部 (Coverage Format 2 の range list、Class Definition Format 2 の range list、Pair Adjustment Format 2 のクラスマトリクス、Extension Lookup の入れ子) に到達しにくいため、独立 target `fuzz/fuzz_targets/parse_gsub.rs` / `parse_gpos.rs` を本 issue で追加する。target は `&[u8]` 入力をテーブル全体としてパースし、`parse_gsub(data, rec)` / `parse_gpos(data, rec)` をパニックなしで完走することを検証する (`rec.offset = 0`、`rec.length = data.len()` で呼び出す)。24 時間連続実行収束は 0001 メタ issue の完了条件で扱う。

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L444 / L446 / L447 / L448 / L450 / L456、段階拡張表の `Font::from_face` / `GlyphBuffer` / `TextMetrics` フィールド / `TextMetrics.advance` / `Font::glyph_run_for_text` の各行の確定責務)
- 依存: `0022-add-text-measurement.md` (`Font::measure_text` の存在、`glyph_run_for_text` の置換対象)、`0023-add-glyph-bounds.md` (`TextMetrics::bounding_box` の `shape()` 経由再計算対象、`Font::glyph_bounds` を `compute_bounding_box` 内で使用)、`0024-add-stroke-text.md` (`stroke_text` のシェーピング統合対象)、`0025-add-font-kerning.md` (`KernTable` / `KernTable::lookup` / `FontFace::kern` / `Font::kern` を GPOS 優先・kern フォールバック動作に統合)
- 非対応 (将来の課題): GSUB の Multiple / Alternate Substitution、Context / Chaining Context、複雑スクリプト (Arabic / Indic)、BiDi、マーク配置 (MarkToBase / MarkToMark)、Variable Fonts、`Font::shape_into` の `&mut Font` 不要化 (現状は `&self`)、ValueRecord の Device Table 解釈
