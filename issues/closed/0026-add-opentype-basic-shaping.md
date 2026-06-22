# OpenType 基本シェーピング機能を追加する

- Priority: Medium
- Category: add
- Created: 2026-05-11
- Completed: 2026-06-22
- Model: Kimi K2.7 Code
- Branch: feature/add-opentype-basic-shaping
- Polished: 2026-06-22

## 目的

OpenType の GSUB / GPOS レイアウト機能を適用し、リガチャ (例: Source Sans 3 で `ff` → `f_f` 合字) や GPOS Pair Adjustment によるカーニング等の高度なテキスト表現を可能にする。`Font::shape(text) -> GlyphBuffer` を新規追加し、0022 で導入された `glyph_run_for_text` (`pub(crate)`) を完全に置換する。`fill_text` / `measure_text` / `stroke_text` の 3 経路をシェーピング適用形に書き換える。

本 issue は `add` カテゴリ (API 追加が主目的) として扱うが、`Font::measure_text` および `Context::fill_text` / `Context::stroke_text` の値・描画結果に意味的変化を生じるため、`CHANGES.md` には `CHANGE` 種別も併記する。これは 0001 メタ issue「font 公開 API の安定化方針」で「font モジュールの公開 API は本 issue close までは未安定とみなし、0021-0027 で破壊的変更を許容する」「破壊的変更 (戻り値の意味変化等) は `CHANGES.md` の `## develop` トップ階層に `CHANGE` 種別で記載する」と確定済みのため、`add` カテゴリ concrete issue 内で `CHANGE` 種別を併発する設計が許容される。

raden は Microsoft OpenType `kern` テーブル v0 (Format 0 / horizontal) に **対応しない** 方針が 0025 closed (2026-06-22) で確定したため、本 issue は kern v0 経路を実装せず、**GPOS Pair Adjustment が raden 唯一のカーニング経路** となる。Source Sans 3 (本 issue のテスト基盤フォント) も GPOS Pair Adjustment Format 2 (Extension Type 9 ラップ経由) を使用しており、本方針で実用上不足は生じない。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L444 `BLFont::feature_settings()` / `set_feature_settings()` / `reset_feature_settings()`**: raden は `FontFeatureSettings` 構造体、`Font::with_features(&FontFace, f64, FontFeatureSettings) -> Self` (構築時設定)、`Font::feature_settings(&self) -> &FontFeatureSettings` (getter)、`Font::set_feature_settings(&mut self, FontFeatureSettings)` (mutable setter) で対応。`reset_feature_settings()` は `Font::set_feature_settings(FontFeatureSettings::default())` で代替するため独立 API は提供しない (対象外)
- **L446 `BLFont::shape(BLGlyphBuffer&)`**: raden は `Font::shape(&self, text: &str) -> GlyphBuffer` (`Font` 内蔵の `feature_settings` を使用、Blend2D 互換寄り) と `Font::shape_into(&self, text: &str, buf: &mut GlyphBuffer)` (バッファ再利用版、`Context::fill_text` / `stroke_text` 経路で使用) の 2 API で対応
- **L447 `BLFont::map_text_to_glyphs(BLGlyphBuffer&)`**: raden は `Font::shape()` 内部処理として対応 (公開 API は `Font::map_char_to_glyph(char)`)
- **L448 `BLFont::position_glyphs(BLGlyphBuffer&)`**: 同上 (`Font::glyph_advance(u16)` の公開 API は残る)
- **L449 `BLFont::apply_kerning(BLGlyphBuffer&)`**: raden は Microsoft OpenType `kern` v0 に **対応しない** (0025 closed で確定)。カーニングは GPOS Pair Adjustment (Lookup Type 2) を `Font::shape()` 内部で適用することで実現する。本 issue close PR で `docs/BLEND2D.md` L449 の raden 列を「raden は対応しない (Microsoft `kern` v0 非対応、GPOS Pair Adjustment は本 issue で実装)」に確定する責務を持つ (0025 close 後続作業として本 issue が引き継ぐ)
- **L450 (部分) `BLFont::apply_gsub(BLGlyphBuffer&, BLBitArray&)` / `apply_gpos(BLGlyphBuffer&, BLBitArray&)`**: raden は `Font::shape()` の内部実装として基本 Lookup Type のみ対応 (GSUB Single Substitution / Ligature Substitution、GPOS Single Adjustment / Pair Adjustment、両者の Extension Lookup wrapping)。`BLBitArray` 相当 (lookup index 選択ビットマスク) と GSUB / GPOS の Context / Chaining Context Lookup は font モジュール安定化前の別 issue で検討
- **L456 `BLGlyphBuffer`**: raden は `GlyphBuffer` を新規定義 (本 issue で初出、SoA レイアウト、後述)

差異の理由:

- raden は `BLGlyphBuffer` の in-place 操作と異なり、`Font::shape()` が新規 `GlyphBuffer` を返す API と、`Font::shape_into(&mut buf)` のバッファ再利用版を併設する (前者は Blend2D 互換寄りのエルゴノミクス、後者は `Context::fill_text` でのアロケーション再利用)
- `BLBitArray` (lookup 選択ビットマスク) と GSUB / GPOS の Context / Chaining Context Lookup は本 issue では対応せず、font モジュール安定化前の別 issue で検討

完了 PR で `docs/BLEND2D.md` L444 / L446 / L447 / L448 / L449 / L450 / L456 の raden 列に新規 API 名を追記する (L449 のみ raden 列を「raden は対応しない」に確定)。状態列 (`未実装` / `差異あり`) は本 issue では変更せず、0001 メタ issue の close PR でまとめて整理する。

## 現状

- テキスト描画は `cmap` による文字 → グリフ変換と単純な `advance` 累積のみ (0022 で `glyph_run_for_text` (`pub(crate)`) が導入された状態)
- リガチャが適用されない (例: Source Sans 3 では `"ff"` は別グリフ `f` + `f` のままで、合字 `f_f` (glyph ID 687) として描画されない)
- GSUB テーブルは未パース・未利用
- GPOS テーブルは未パース・未利用 (カーニング非適用)
- `kern` テーブルは未パース・未利用 (0025 closed で raden は Microsoft kern v0 に **対応しない** 方針を確定済み。本 issue でも実装しない)
- `BLGlyphBuffer` 相当の中間バッファがない
- `FontFeatureSettings` (`liga` / `clig` / `kern` 等の feature on/off 制御) が未実装
- `pbt/tests/prop_font/main.rs` には 0022 で書かれた 4 件の PBT (`prop_single_char_advance` / `prop_concatenation` / `prop_size_linearity` / `prop_non_negative`) が現存する。0025 closed (kern v0 非対応で実装なし) のため `prop_concatenation` / `prop_non_negative` の削除責務は移行されておらず、本 issue が初めて削除責務を負う

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
- **配列長一致は `Font::shape` / `Font::shape_into` の事後条件** (`glyph_ids.len() == placements.len() == clusters.len()`)。docstring に明示。`pub` フィールドのため外部からの個別 `push` で長さ破綻は可能だが、`Font::shape` の戻り値を `Context::fill_text` 内でのみ消費する設計で破綻リスクを抑える。`compute_bounding_box` 等の内部関数では `debug_assert_eq!` で SoA 不変条件を検証する
- **`Default` 派生** は `Context::tmp_shape_buffer` の `std::mem::take` に必要
- **`#[non_exhaustive]`** は 0001 メタ issue で確定した `non_exhaustive` 対象に `GlyphBuffer` / `FontFeatureSettings` が含まれる方針に従う。`GlyphPlacement` も Blend2D `BLGlyphPlacement` がフィールド変動 (Y placement / advance 等) するため `#[non_exhaustive]` を付与する。0001 メタ issue 対象 4 型に `GlyphPlacement` を加える形で本 issue close PR で 0001 を更新する
- **placement / advance は f64** (Blend2D の `BLPointI` (i32) ではない)。raden の他 API との整合性優先。整数化は別 issue
- **`cluster` 配列を含める**。BiDi / 複雑スクリプトは本 issue 対象外だが、ラテン系リガチャ (`ff` / `ft` 等) でも caret 位置決定に必要

### `FontFeatureSettings` と `Font` の feature 保持

```rust
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FontFeatureSettings {
    /// kern feature の on/off (デフォルト: true)。GPOS の `kern` feature 紐付き Lookup
    /// (Pair Adjustment が主だが、フォントによっては Single Adjustment も含む) の有効化。
    /// `kern: false` にすると `kern` feature 経由の GPOS Single Adjustment も skip される
    /// (これは意図通り。GPOS の他 feature 経由の Single Adjustment は本 issue 範囲外)。
    pub kern: bool,
    /// liga feature の on/off (デフォルト: true)。標準リガチャ (Source Sans 3 では ff / ft / fft)。
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

`kern` feature 名は GPOS Pair Adjustment Lookup を選択するためのもので、Microsoft `kern` テーブル v0 とは無関係 (raden は kern v0 非対応)。本 issue では `FontFeatureSettings.kern == true` で GPOS の `kern` feature 紐付き Lookup を適用する。

### `Font::from_face` シグネチャ維持

`Font::from_face(&FontFace, f64) -> Self` のシグネチャは本 issue で変更しない。内部実装で `Self::with_features(face, size, FontFeatureSettings::default())` を呼ぶ形にリファクタする。0021-0024 で書かれた `Font::from_face(&face, size)` 呼び出しは全て変更不要。

### サポートする GSUB / GPOS Lookup Type

本 issue で対応する Lookup Type:

- **GSUB Lookup Type 1: Single Substitution** (Format 1 / 2 両方。1 グリフ → 1 グリフ)
- **GSUB Lookup Type 4: Ligature Substitution** (N グリフ → 1 グリフ。Source Sans 3 の `ff` / `ft` / `fft` 等)
- **GSUB Lookup Type 7: Extension Substitution** (上記 Type 1 / 4 を内部にラップ)
- **GPOS Lookup Type 1: Single Adjustment** (Format 1 / 2 両方。個別グリフの位置調整)
- **GPOS Lookup Type 2: Pair Adjustment** (Format 1 / 2 両方。ペアグリフの位置調整。Source Sans 3 は Format 2 (クラスベース行列) が主体)
- **GPOS Lookup Type 9: Extension Positioning** (上記 Type 1 / 2 を内部にラップ)
- **Coverage Table Format 1 (glyph 列挙) / Format 2 (range 連続) の両方**

Extension Lookup (Type 7 / 9) を対応必須にする理由: 大規模フォントファミリ (Noto Sans CJK / Source Han Sans / Roboto Flex / SF Pro 等) では subtable サイズが 64KB を超えるため、`liga` / `kern` を含むほぼ全ての Lookup が Extension でラップされている。中規模フォント (Source Sans 3 / Source Serif 4 等) では GSUB は Type 7 を持たず Type 4 直接記述だが、**GPOS 側は Source Sans 3 でも Type 9 (Extension) ラップが必須** (実機 polish 検証で確認: Lookup #8 が Type 9 経由で内部 Type 2 Pair Adjustment Format 2 をラップする)。Extension を非対応にすると Source Sans 3 でも GPOS kern feature が動作せず、PBT `prop_shape_advance_sum_matches_measure_text` 等の検証が縮退する。

Extension の実装は薄く (内部の実 Lookup Type を unwrap して再帰呼び出しするだけ)、本 issue のスコープに含める。Extension の内部 Lookup Type がさらに Extension (Type 7 / 9) のケースは仕様上許容されないため `Unsupported` 扱いとし、再帰しない。

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

### `apply_gsub` / `apply_gpos` ヘルパー

`src/font/shape.rs` (新規 `pub(crate)` モジュール) に `pub(crate) fn apply_gsub` / `pub(crate) fn apply_gpos` を定義する。詳細な擬似コードと挙動 (Ligature 同期 drain、Extension 1 段 unwrap、GPOS の ValueRecord 加算、Pair vs Single ループ差異) は解決方法 step 4 参照。`apply_gpos` の戻り値は `()`、raden は kern v0 非対応のため GPOS 経路の有無による二重適用回避ロジックは不要。

### `compute_bounding_box` ヘルパー

0023 で `Font::measure_text` 内に実装された `bounding_box` 計算ロジックを `Font::shape` 経由の `GlyphBuffer` から計算する形に **置換** する。`src/font/mod.rs` 内に private fn として追加:

```rust
fn compute_bounding_box(buffer: &GlyphBuffer, font: &Font) -> Option<GlyphBounds> {
    debug_assert_eq!(
        buffer.glyph_ids.len(),
        buffer.placements.len(),
        "GlyphBuffer SoA 不変条件 (glyph_ids.len() == placements.len()) が破綻している"
    );
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

リガチャで `glyph_ids.len() < text.chars().count()` になった場合、合字グリフ ID の `glyph_bounds` を取得して bbox に union する。0023 で書かれた `prop_glyph_bounds_contains_path_control_box` PBT (個別 glyph_id 単位の `Path::control_box() ⊆ glyph_bounds`) は `shape()` 経由でも維持される (個別グリフの `glyph_bounds` 自体は不変)。一方 `prop_measure_text_bounding_box_contains` PBT (`measure_text` 経由) は `shape()` 経由で意味が変わるため、合字グリフ ID 経由の `glyph_bounds` union が正しく機能することを `compute_bounding_box` のロジックで保証する。

### `Font` の `Clone` 派生

本 issue で `Font` (`src/font/mod.rs:128-132`) に `#[derive(Clone)]` を追加する。理由: 「一時的に異なる feature で shape したい」場合、`font.clone_with_features(new_features).shape(text)` のような流れが必要。`Font.face: Arc<FontFaceInner>` (`Arc::clone` で軽量、`FontFaceInner` 自身の `Clone` は不要)、`size: f64` (`Copy`)、`scale: f64` (`Copy`)、`feature_settings: FontFeatureSettings` (本 issue で追加、`Clone`) で全フィールドが `Clone` 可能なため `#[derive(Clone)]` で問題ない。

`Font::clone_with_features(&self, features: FontFeatureSettings) -> Self` も同時追加する。内部実装は `let mut clone = self.clone(); clone.feature_settings = features; clone`。同一 `FontFace` から複数 feature の `Font` を作るケースで `tables.clone()` のディープコピーが発生するが、本 issue では既存 `Font::from_face` のパターンを維持し、メモリ最適化は別 issue で扱う。

### 0022 PBT の取り扱い整理

0022 で書かれた 4 件の PBT (`prop_single_char_advance` / `prop_concatenation` / `prop_size_linearity` / `prop_non_negative`) は本 issue でいずれも削除する。理由:

- `prop_concatenation` (実機 `pbt/tests/prop_font/main.rs:66-82`、`#[test]` は L368-371): `measure_text(a + b).advance == measure_text(a).advance + measure_text(b).advance` の結合性。GSUB Ligature と GPOS Pair Adjustment で隣接境界の advance が変化するため破綻する。docstring (L62-65) で「将来カーニング適用が `measure_text` に追加されると `(A, V)` 等の隣接ペアで破綻するため、その時点で本テストの更新が必要になる」と明示済 (本 issue がその時点に当たる)
- `prop_non_negative` (実機 L106-115、`#[test]` は L378-381): `measure_text(text).advance >= 0`。GPOS Pair Adjustment のネガティブ値で破綻する
- `prop_single_char_advance` (実機 L48-58): `measure_text(c).advance == glyph_advance(map_char_to_glyph(c))`。GPOS Single Adjustment で破綻可能性
- `prop_size_linearity` (実機 L85-103): `measure_text(text, k * size).advance == k * measure_text(text, size).advance`。GPOS Single Adjustment / Pair Adjustment が size に対し非線形な場合に破綻可能性

なお `prop_concatenation` / `prop_non_negative` は 0022 polish で「kern 適用 issue で削除する設計」と想定されていたが、0025 closed (raden は kern v0 非対応) で削除責務は移行されなかった。本 issue で初めて削除責務を負う。

### `TextMetrics::leading_bearing` / `trailing_bearing` の追加

0001 メタ issue 段階拡張表で「`TextMetrics` フィールド (追加) 0026 時点: `leading_bearing` / `trailing_bearing` 追加 (`BLTextMetrics` 互換に近づく)」と確定済み。本 issue で `TextMetrics` (`src/font/mod.rs:356-367`) に以下のフィールドを追加する (`#[non_exhaustive]` のためフィールド追加は非破壊):

- `pub leading_bearing: f64`: 最初のグリフの `glyph_bounds.x_min + placements[0].offset_x` (テキスト左端からの「空白」幅)
- `pub trailing_bearing: f64`: 最後のグリフの `placements[last].advance - (glyph_bounds.x_max + placements[last].offset_x)` (テキスト右端からの「空白」幅)

空文字列および全グリフが `glyph_bounds = None` (空グリフのみ) の場合は両方 `0.0` を返す。

### 値の意味的変化と CHANGE 種別

- `Font::measure_text` の `TextMetrics.advance` の値の意味が「カーニング非適用」(0022 時点) から「OpenType GSUB / GPOS シェーピング適用済み」に変化する (`CHANGE` 種別)
- `Font::measure_text` の `TextMetrics.bounding_box` (0023 で追加) の意味が「カーニング非適用 bbox」から「シェーピング適用済み bbox」に変化する (`CHANGE` 種別)
- `Context::fill_text` の描画結果のグリフ列・位置がシェーピング適用済みに変わる (`CHANGE` 種別)
- `Context::stroke_text` の描画結果のグリフ列・位置がシェーピング適用済みに変わる (`CHANGE` 種別)

0001 メタ issue「font 公開 API の安定化方針」セクションで「戻り値の意味変化・描画結果の差は `CHANGES.md` の `## develop` のトップ階層に `CHANGE` 種別で記載する」と確定済みの規則に従う。

## 完了条件

### 追加される API

- `GlyphBuffer` 構造体 (SoA レイアウト、`#[non_exhaustive]`、`Default` 派生)
- `GlyphPlacement` 構造体 (`#[non_exhaustive]`、`Default` 派生)
- `FontFeatureSettings` 構造体 (`#[non_exhaustive]`、`Default` 実装)
- `Font::with_features(&FontFace, f64, FontFeatureSettings) -> Self`
- `Font::set_feature_settings(&mut self, FontFeatureSettings)`
- `Font::feature_settings(&self) -> &FontFeatureSettings` (getter、参照返し)
- `Font::clone_with_features(&self, FontFeatureSettings) -> Self`
- `Font::shape(&self, text: &str) -> GlyphBuffer` (`Font` 内蔵 `feature_settings` を使用)
- `Font::shape_into(&self, text: &str, buf: &mut GlyphBuffer)` (バッファ再利用版)
- `TextMetrics.leading_bearing: f64` フィールド追加 (`#[non_exhaustive]` のため非破壊)
- `TextMetrics.trailing_bearing: f64` フィールド追加 (`#[non_exhaustive]` のため非破壊)
- `Font` 構造体に `#[derive(Clone)]` 追加

### 削除される内部 API

- `Font::glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` (`pub(crate)`、0022 で追加。実機 `src/font/mod.rs:249-256`) を削除
- `Context.tmp_glyph_run: Vec<(u16, f64)>` フィールド (実機 `src/api/context.rs:316-317`) を `Context.tmp_shape_buffer: GlyphBuffer` に置換 (`Context::new` での初期化 (実機 L360) を `GlyphBuffer::default()` に変更)

### 既存 API のシグネチャ維持と意味的変化

- `Font::from_face(&FontFace, f64) -> Self` のシグネチャは変更しない (内部実装で `Self::with_features(face, size, FontFeatureSettings::default())` を呼ぶ形に変更)
- `Font::measure_text(&str) -> TextMetrics`: シグネチャ不変だが、`TextMetrics.advance` / `TextMetrics.bounding_box` の意味が「カーニング非適用」(0022 時点) から「シェーピング適用済み (GSUB + GPOS)」に変わる (`CHANGE` 種別)
- `Context::fill_text` / `Context::stroke_text`: シグネチャ不変だが、描画結果がシェーピング適用済みのグリフ列・位置に変わる (`CHANGE` 種別)

### docstring 更新

- `Font::measure_text` の docstring (実機 `src/font/mod.rs:258-268`) のうち、`mod.rs:260` の `advance` 説明行に含まれる「カーニング未適用」の文言を「OpenType GSUB / GPOS シェーピング (`Font::shape` で適用済み)」に置換する。`size == 0` / NaN / 改行 等の他段落 (`mod.rs:262-268`) は維持する
- `glyph_run_for_text` (実機 `src/font/mod.rs:238-256`) は関数本体・docstring ともに削除する

### 着手前提

- 0022 が close 済
- 0023 が close 済 (`TextMetrics::bounding_box` フィールドが追加済み、`Font::glyph_bounds` が `compute_bounding_box` 内で利用可能)
- 0024 が close 済 (`Context::stroke_text` が追加済み、本 issue で `shape_into` 経由に書き換える)
- 0025 が close 済 (raden は Microsoft kern v0 に非対応の方針を確定。本 issue は GPOS Pair Adjustment が唯一のカーニング経路として実装する)
- 0039 が close 済 (`tests/helpers/font_fetch.rs::fetch_source_sans_3_bytes` / `fetch_source_serif_4_bytes` でフォントをダウンロード取得できる)
- 0001 メタ issue の前提 concrete issue 群 (fuzzing 基盤 / ベースライン benchmark 計測基盤 / Compound Glyph point-matching 実装) は本 issue 着手前に concrete 化と close を完了する責務を本 issue 着手者が確認する。0001 メタ issue L194 備考の「0025 / 0026 / 0027 / 0037 を `/polish-issue` で個別に再 polish」のうち、本 0026 polish は完了済み (0025 polish も完了済み、0027 / 0037 polish はそれぞれ後続で対応)

### close 前提

- 上記着手前提を満たし、本 issue 内で `Font::shape` / `Font::shape_into` 追加、3 経路 (`fill_text` / `measure_text` / `stroke_text`) の書き換え、`glyph_run_for_text` 削除、0021-0024 テストの影響対応、0022 PBT 4 件 (`prop_concatenation` / `prop_non_negative` / `prop_single_char_advance` / `prop_size_linearity`) の削除、新規 PBT 5 件の追加、0001 メタ issue 段階拡張表と依存関係セクションの更新が完了している
- CI でテストがパス

### ドキュメント

- `docs/BLEND2D.md` L444 / L446 / L447 / L448 / L450 / L456 の raden 列に新規 API 名を追記 (状態列は本 issue で変更しない、0001 メタ issue close PR で整理)
- `docs/BLEND2D.md` L449 (`apply_kerning`) の raden 列を「raden は対応しない (Microsoft `kern` v0 非対応、GPOS Pair Adjustment は本 issue で実装)」に確定する (0025 close 後続作業として本 issue が引き継ぐ)
- `CHANGES.md` に `CHANGE` 種別 4 件と `ADD` 種別 11 件を追加 (詳細は解決方法 7 参照)

### Fuzzing

fuzzing 基盤は現時点では未整備 (`fuzz/` ディレクトリは `.gitkeep` のみ) 。不正な GSUB / GPOS テーブル (Coverage Format 不正、ClassDef 不正、Lookup Type 不正、Extension の入れ子等) に対するクラッシュ耐性は、0001 メタ issue の fuzzing 基盤整備後に `parse_all` 経由の fuzz target でカバーする。GSUB / GPOS 個別の fuzz target は本 issue では追加しない (テーブル深部のカバレッジが薄い問題は 24 時間連続実行収束で対応、0001 メタ issue の完了条件で扱う。0025 の `parse_kern` fuzz が同じ方針を採った前例と整合)。

### 0001 メタ issue 更新責務

本 issue close PR 内で `issues/0001-enhance-font-module-maturity.md` の以下を確定値で更新する責務を持つ。0025 close 後続作業として本 issue が引き継ぐ責務 (0001 L106 / L177-181 / L194 / L49 Blend2D 対応表 L449 行) も同時に処理する。

- 「Blend2D との対応表」(L49 周辺) の L449 (`apply_kerning`) 行の raden 列を「raden は対応しない (kern v0 非対応)」に確定し、「対象 concrete issue」列を「0026」に更新
- 「段階拡張で意味が変わる API」表 (L87-94) の「0025 時点 (close)」列を全削除し、「0022 時点 (close)」「0026 時点 (close)」の 2 列構成に再構築する。`TextMetrics.advance` 行は「0026 時点 (close): f64 (シェーピング適用済み。GSUB + GPOS / Pair Adjustment 含む。Microsoft `kern` v0 は raden 非対応で 0025 closed で確定)」、`TextMetrics` フィールド (追加) 行は「`bounding_box` (0023 で追加済) / `leading_bearing` / `trailing_bearing` (本 issue で追加)」、`Font::glyph_run_for_text()` 行は「本メソッドは 0026 で削除済み」、`Font::from_face` シグネチャ行は「シグネチャは `(&FontFace, f64) -> Self` のまま維持。`FontFeatureSettings` 対応は `Font::with_features` / `Font::set_feature_settings` の別メソッドで実現済み」、`GlyphBuffer` レイアウト行は「SoA、glyph_ids: Vec<u16> / placements: Vec<GlyphPlacement> / clusters: Vec<u32> の 3 配列。placement / advance は f64、cluster は UTF-8 byte index」に確定
- 「Concrete issue 一覧」(L106 周辺) の 0025 行を `closed` ステータスに更新
- 「依存関係」セクション (L174-181) の 0025 関連記述を整理: 「0025 → 0022, 0024」行を削除、「0026 → 0022, 0025」を「0026 → 0022, 0023, 0024」に修正、「closed にする順序は 0024 → 0025 → 0026 → 0027」を「0024 → 0026 → 0027」に修正、「0024 で書かれた `stroke_text` は 0025 完了時にカーニング適用へ改修する必要があり、この後追い改修は 0025 のスコープに含む」を削除
- 「備考」(L194) の「0026 polish」項目を削除 (本 polish 完了で対応済み)
- `non_exhaustive` 対象 4 型に `GlyphPlacement` を加える (本 issue で同方針を適用したため)

### 0021-0024 のテストへの影響

| テスト | 影響 | 対応 |
|---|---|---|
| `tests/test_font.rs::font_face_metrics` | 影響なし | 維持 |
| `tests/test_font.rs::font_char_to_glyph` | 影響なし | 維持 |
| `tests/test_font.rs::font_glyph_outline_to_path` | 影響なし | 維持 |
| `tests/test_font.rs::font_space_glyph_has_no_outline` | 影響なし | 維持 |
| `tests/test_font.rs::font_fill_text_integration` | `has_nonzero` のみ assert なので影響軽微 | 維持 |
| `tests/test_font.rs::font_measure_text_empty` | 空文字列 → advance = 0.0 は維持 | 維持 |
| `tests/test_font.rs::font_measure_text_single_char` | Arial で GPOS Single Adjustment 不在前提なら維持 | 維持 (`load_arial` 経由のため Arial 限定。GPOS Single Adjustment を持たないフォントでは `measure_text("A").advance == glyph_advance(map_char_to_glyph('A'))` が成り立つ) |
| `tests/test_font.rs::font_measure_text_newline_no_panic` | 影響なし | 維持 |
| `tests/test_font.rs::font_glyph_advance_out_of_range_is_zero` | 影響なし | 維持 |
| 0021 で書かれた `font_face_cap_x_height` 等 | 影響なし | 維持 |
| 0023 で書かれた `font_glyph_bounds_some` / `font_glyph_bounds_invalid_id` | 影響なし | 維持 |
| 0024 で書かれた `tests/test_context.rs::stroke_text::renders` / `matches_manual_path` | `stroke_text` が `shape()` 経由になるため、手動再実装 (`matches_manual_path`、実機 `tests/test_context.rs:539-585`) も `shape()` ベースに揃える | 本 issue で更新 |
| 0039 で書かれた `tests/test_font.rs::fetch_source_sans_3_has_required_tables` / `fetch_source_serif_4_has_cff_table` | 影響なし | 維持 |

## 解決方法

### 1. GSUB / GPOS テーブルパースの追加 (`src/font/tables.rs`)

実機 `src/font/tables.rs:113-120` の TAG 定数群に `TAG_GSUB: u32 = tag(b"GSUB")` / `TAG_GPOS: u32 = tag(b"GPOS")` を追加する。

構造体定義 (フィールド可視性は既存 `HmtxTable` 等のパターンに揃える):

```rust
#[derive(Clone, Default)]
pub(crate) struct GsubTable {
    /// `liga` feature が参照する Lookup インデックスのリスト。
    /// ScriptList で 'DFLT' を最優先、なければ 'latn' を採用済みの結果。
    /// 複数 feature がある場合は最初の 1 つ。
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
    /// Format 1: delta_glyph_id 加算 (modulo 65536 wrap、`as u16` キャストが natural wrap と等価)
    DeltaGlyphID(i16),
    /// Format 2: substitute glyph IDs 配列
    SubstituteGlyphIDs(Vec<u16>),
}

#[derive(Clone)]
pub(crate) struct GsubLigatureSubst {
    pub coverage: Coverage,
    /// LigatureSet[Coverage Index] のリスト。各エントリは Ligature の配列。
    /// Microsoft OpenType 仕様によりフォントビルダーは component 数の降順
    /// (longest first) で格納する慣行 (Source Sans 3 / Adobe Source ファミリ
    /// は同慣行)。本 issue は最初マッチを採用する simple な実装で longest
    /// match 相当の結果を得る。
    pub ligature_sets: Vec<Vec<GsubLigature>>,
}

#[derive(Clone)]
pub(crate) struct GsubLigature {
    pub ligature_glyph: u16,
    /// 合字を構成する 2 番目以降のグリフ ID のみを保持する (N-1 要素)。
    /// 1 番目のグリフは parent Lookup の Coverage Table が指し示すグリフのため省略。
    pub component_glyph_ids: Vec<u16>,
}

#[derive(Clone, Default)]
pub(crate) struct GposTable {
    /// `kern` feature が参照する Lookup インデックスのリスト。
    /// (raden は Microsoft `kern` テーブル v0 を実装しないため、本フィールドは
    /// GPOS feature の意味で「kern」を指す。)
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
    /// Format 2: Class1 x Class2 マトリクス (Source Sans 3 は本 Format が主体)
    Class {
        class_def_1: ClassDef,
        class_def_2: ClassDef,
        class1_count: u16,
        class2_count: u16,
        records: Vec<(ValueRecord, ValueRecord)>,
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
    /// Format 1: glyph 列挙 (昇順前提、binary_search で検索)
    GlyphList(Vec<u16>),
    /// Format 2: (start, end, start_coverage_index) のタプル列
    RangeList(Vec<(u16, u16, u16)>),
}

#[derive(Clone)]
pub(crate) enum ClassDef {
    Format1 { start_glyph: u16, class_values: Vec<u16> },
    /// (start, end, class) の範囲列。範囲外グリフは class = 0 として扱う
    Format2 { ranges: Vec<(u16, u16, u16)> },
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
    /// Coverage Format 1 で glyph_ids が昇順でない不正フォントの場合、
    /// `binary_search` は `Err(insertion_pos)` を返し、本関数は `.ok().map(...)`
    /// で `None` に縮退する。結果としてシェーピングは当該グリフで noop となり、
    /// リガチャ・GPOS Pair Adjustment が適用されないだけで panic / UB は発生しない。
    /// 寛容方針として `is_sorted()` バリデーションは行わない。
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

`parse_gsub` / `parse_gpos` の擬似コード (寛容方針、`Result` でラップせず `GsubTable` / `GposTable` 直返し。Extension Lookup を 1 段のみ再帰的に unwrap):

```text
fn parse_gsub(data: &[u8], rec: TableRecord) -> GsubTable:
    // rec.offset は font face 単位の絶対 offset を指す (TTC の場合は
    // 各 face で異なる)。data は font file 全体 (TTC の場合も全体)。
    // 既存 parse_hmtx / parse_cmap_* と同じ慣行に従う。
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
    let Some(script_off) = find_script(data, script_list_off, table_end) else { return out }
    let default_lang_sys_off = read_u16(data, script_off)? as usize
    if default_lang_sys_off == 0:
        return out
    let lang_sys_off = script_off + default_lang_sys_off

    // 2. DefaultLangSys から FeatureIndex 列を取得 (RequiredFeatureIndex は無視)
    let feature_count = read_u16(data, lang_sys_off + 4)? as usize
    let mut feature_indices: Vec<u16> = Vec::new()
    for i in 0..feature_count:
        let Ok(idx) = read_u16(data, lang_sys_off + 6 + i * 2) else { return out }
        feature_indices.push(idx)

    // 3. FeatureList から FeatureRecord を順次評価し、tag が 'liga' / 'clig' のものを採用
    let total_features = read_u16(data, feature_list_off)? as usize
    for &fi in &feature_indices:
        if fi as usize >= total_features: continue
        let feature_record_off = feature_list_off + 2 + (fi as usize) * 6
        let tag = read_u32(data, feature_record_off)?
        let feature_table_off = feature_list_off + read_u16(data, feature_record_off + 4)? as usize
        let lookup_index_count = read_u16(data, feature_table_off + 2)? as usize
        let mut lookup_indices: Vec<u16> = Vec::new()
        for i in 0..lookup_index_count:
            let Ok(idx) = read_u16(data, feature_table_off + 4 + i * 2) else { return out }
            lookup_indices.push(idx)
        match tag:
            0x6C696761 /* 'liga' */ => out.liga_lookups.extend(lookup_indices),
            0x636C6967 /* 'clig' */ => out.clig_lookups.extend(lookup_indices),
            _ => continue

    // 4. LookupList から各 Lookup を parse。サポート外 Type は GsubLookup::Unsupported
    let lookup_count = read_u16(data, lookup_list_off)? as usize
    for i in 0..lookup_count:
        let Ok(rel_off) = read_u16(data, lookup_list_off + 2 + i * 2) else { return out }
        let lookup_off = lookup_list_off + rel_off as usize
        out.lookups.push(parse_gsub_lookup(data, lookup_off, table_end))

    return out

// find_script: 'DFLT' (0x44464C54) を優先、なければ 'latn' (0x6C61746E) を採用。
// 両方無ければ None。Microsoft OpenType ScriptList の DefaultLangSys 慣行に従う。
fn find_script(data: &[u8], script_list_off: usize, table_end: usize) -> Option<usize>:
    let Ok(script_count) = read_u16(data, script_list_off) else { return None }
    let mut dflt_off: Option<usize> = None
    let mut latn_off: Option<usize> = None
    for i in 0..script_count as usize:
        let record_off = script_list_off + 2 + i * 6
        let Ok(tag) = read_u32(data, record_off) else { continue }
        let Ok(rel) = read_u16(data, record_off + 4) else { continue }
        let script_off = script_list_off + rel as usize
        match tag:
            0x44464C54 /* 'DFLT' */ => dflt_off = Some(script_off),
            0x6C61746E /* 'latn' */ => latn_off = Some(script_off),
            _ => continue
    dflt_off.or(latn_off)

fn parse_gsub_lookup(data: &[u8], lookup_off: usize, table_end: usize) -> GsubLookup:
    let Ok(lookup_type) = read_u16(data, lookup_off) else { return GsubLookup::Unsupported }
    let _lookup_flag = read_u16(data, lookup_off + 2)
    let Ok(subtable_count) = read_u16(data, lookup_off + 4) else { return GsubLookup::Unsupported }
    // 本 issue では最初の subtable のみ処理 (簡略化)。複数 subtable の累積は font 安定化前の別 issue
    if subtable_count == 0: return GsubLookup::Unsupported
    let Ok(rel) = read_u16(data, lookup_off + 6) else { return GsubLookup::Unsupported }
    let subtable_off = lookup_off + rel as usize

    match lookup_type:
        1 => parse_gsub_single(data, subtable_off),
        4 => parse_gsub_ligature(data, subtable_off),
        7 => {
            // Extension Substitution: 内部の実 Lookup Type を 1 段のみ unwrap
            let Ok(ext_format) = read_u16(data, subtable_off) else { return GsubLookup::Unsupported }
            if ext_format != 1: return GsubLookup::Unsupported
            let Ok(inner_lookup_type) = read_u16(data, subtable_off + 2) else { return GsubLookup::Unsupported }
            let Ok(inner_rel) = read_u32(data, subtable_off + 4) else { return GsubLookup::Unsupported }
            let inner_subtable_off = subtable_off + inner_rel as usize
            match inner_lookup_type:
                1 => parse_gsub_single(data, inner_subtable_off),
                4 => parse_gsub_ligature(data, inner_subtable_off),
                _ => GsubLookup::Unsupported,  // 入れ子 Extension (Type 7) はサポート外
        }
        _ => GsubLookup::Unsupported

(parse_gsub_single, parse_gsub_ligature, parse_coverage の擬似コードは省略。
 各仕様 (Microsoft OpenType Specification GSUB 章) に従って実装する。
 Vec は `Vec::new()` から push 形式で構築する。)
```

`parse_gpos` は同じパターンだが、feature tag として `'kern'` (0x6B65726E) のみを採用し、Lookup Type 1 (Single Adjustment) / Type 2 (Pair Adjustment) / Type 9 (Extension) をパースする。Extension Type 9 の内部 Lookup Type は 1 段のみ unwrap し、入れ子 Extension はサポート外。Source Sans 3 では GPOS Pair Adjustment Format 2 (クラスベース行列) が主体のため、`parse_gpos_pair_format2` の擬似コードを示す:

```text
fn parse_gpos_pair_format2(data: &[u8], subtable_off: usize) -> GposPairAdjustFormat:
    let Ok(value_format_1) = read_u16(data, subtable_off + 4) else { return GposPairAdjustFormat::Class { class_def_1: empty(), class_def_2: empty(), class1_count: 0, class2_count: 0, records: Vec::new() } }
    let Ok(value_format_2) = read_u16(data, subtable_off + 6) else { return ... }
    let Ok(rel_cd1) = read_u16(data, subtable_off + 8) else { return ... }
    let Ok(rel_cd2) = read_u16(data, subtable_off + 10) else { return ... }
    let class_def_1_off = subtable_off + rel_cd1 as usize
    let class_def_2_off = subtable_off + rel_cd2 as usize
    let Ok(class1_count) = read_u16(data, subtable_off + 12) else { return ... }
    let Ok(class2_count) = read_u16(data, subtable_off + 14) else { return ... }
    let class_def_1 = parse_class_def(data, class_def_1_off)
    let class_def_2 = parse_class_def(data, class_def_2_off)
    let mut records: Vec<(ValueRecord, ValueRecord)> = Vec::new()
    let mut cursor = subtable_off + 16
    for _ in 0..(class1_count as usize * class2_count as usize):
        let (vr1, advanced1) = read_value_record(data, cursor, value_format_1)
        cursor += advanced1
        let (vr2, advanced2) = read_value_record(data, cursor, value_format_2)
        cursor += advanced2
        records.push((vr1, vr2))
    GposPairAdjustFormat::Class { class_def_1, class_def_2, class1_count, class2_count, records }

// ClassDef Format 1 / 2 両対応
fn parse_class_def(data: &[u8], off: usize) -> ClassDef:
    let Ok(format) = read_u16(data, off) else { return ClassDef::Format2 { ranges: Vec::new() } }
    match format:
        1 => {
            let Ok(start_glyph) = read_u16(data, off + 2) else { return ... }
            let Ok(glyph_count) = read_u16(data, off + 4) else { return ... }
            let mut class_values: Vec<u16> = Vec::new()
            for i in 0..glyph_count as usize:
                let Ok(c) = read_u16(data, off + 6 + i * 2) else { return ClassDef::Format1 { start_glyph, class_values } }
                class_values.push(c)
            ClassDef::Format1 { start_glyph, class_values }
        }
        2 => {
            let Ok(range_count) = read_u16(data, off + 2) else { return ... }
            let mut ranges: Vec<(u16, u16, u16)> = Vec::new()
            for i in 0..range_count as usize:
                let base = off + 4 + i * 6
                let Ok(start) = read_u16(data, base) else { return ClassDef::Format2 { ranges } }
                let Ok(end) = read_u16(data, base + 2) else { return ClassDef::Format2 { ranges } }
                let Ok(class) = read_u16(data, base + 4) else { return ClassDef::Format2 { ranges } }
                ranges.push((start, end, class))
            ClassDef::Format2 { ranges }
        }
        _ => ClassDef::Format2 { ranges: Vec::new() }  // 未知の format は空 ClassDef 扱い (寛容)
```

GPOS Single Adjustment Format 1 / 2 のパースと Pair Adjustment Format 1 のパースは Microsoft OpenType Specification GPOS 章に従って同パターンで実装する。

`ValueRecord` の読み出しヘルパー:

```text
fn read_value_record(data: &[u8], off: usize, value_format: u16) -> (ValueRecord, usize):
    let mut vr = ValueRecord::default()
    let mut cursor = off
    if value_format & 0x0001 != 0:
        let Ok(v) = read_i16(data, cursor) else { return (vr, cursor - off) }
        vr.x_placement = v; cursor += 2
    if value_format & 0x0002 != 0:
        let Ok(v) = read_i16(data, cursor) else { return (vr, cursor - off) }
        vr.y_placement = v; cursor += 2
    if value_format & 0x0004 != 0:
        let Ok(v) = read_i16(data, cursor) else { return (vr, cursor - off) }
        vr.x_advance = v; cursor += 2
    if value_format & 0x0008 != 0:
        let Ok(v) = read_i16(data, cursor) else { return (vr, cursor - off) }
        vr.y_advance = v; cursor += 2
    // bits 4-7 (Device Table offsets) は読み飛ばし
    for bit in 4..8:
        if value_format & (1 << bit) != 0:
            cursor += 2
    (vr, cursor - off)
```

GPOS Pair Adjustment Format 1 / 2、Single Adjustment Format 1 / 2、Coverage、ClassDef のパースは上記パターンに準じる。

擬似コード内の `Vec` 構築は `Vec::new()` から `push` 形式で書き、`.collect::<Vec<_>>()` パターンは使わない (`shiguredo-rust` 規約「`Vec::with_capacity` 禁止」。`collect` も内部で `size_hint` 経由の事前確保を行うため避ける)。一方 `Vec::extend(slice)` は size_hint が exact のため許容する。既存 `tables.rs` 内の他パーサ (`TableDirectory::parse` / `parse_hmtx` / `parse_cmap_format4` / `parse_cmap_format12`) は規約と異なる `Vec::with_capacity` を使っているが、本 issue では撤去せず別 issue で扱う (他既存パーサ群の慣行に揃える)。

`parse_all` (実機 `src/font/tables.rs:629-675`) の `let cmap = parse_cmap(data, cmap_rec)?;` (実機 L652) の直後、`let os2 = ...` の前に以下を追加する:

```rust
let gsub = dir.find(TAG_GSUB).map(|rec| parse_gsub(data, rec)).unwrap_or_default();
let gpos = dir.find(TAG_GPOS).map(|rec| parse_gpos(data, rec)).unwrap_or_default();
```

`ParsedTables` 構造体 (実機 `src/font/tables.rs:587-603`) に `pub gsub: GsubTable` / `pub gpos: GposTable` を末尾近くに追加し、`parse_all` 末尾の `Ok(ParsedTables { ... })` 構造体リテラルにも `gsub,` / `gpos,` フィールドを追加する (構造体リテラル更新を忘れるとコンパイルエラーになるため必須)。

### 2. 構造体定義 (`src/font/mod.rs`)

`GlyphBuffer` / `GlyphPlacement` / `FontFeatureSettings` を上記「設計方針」のとおり定義する (`Default` 派生・`#[non_exhaustive]` 付与済み)。

`Font` 構造体 (実機 `src/font/mod.rs:128-132`) に `feature_settings: FontFeatureSettings` フィールドを `scale: f64` の直後に追加 (private)。`#[derive(Clone)]` を追加。

`TextMetrics` 構造体 (実機 `src/font/mod.rs:356-367`) に `pub leading_bearing: f64` / `pub trailing_bearing: f64` フィールドを `bounding_box` の直後に追加する (`#[non_exhaustive]` のため非破壊)。

`src/lib.rs:20` の `pub use font::{Font, FontData, FontError, FontFace, GlyphBounds, TextMetrics};` を以下に拡張する (alphabetical 順):

```rust
pub use font::{Font, FontData, FontError, FontFace, FontFeatureSettings, GlyphBounds, GlyphBuffer, GlyphPlacement, TextMetrics};
```

### 3. `Font::with_features()` / `set_feature_settings()` / `feature_settings()` / `clone_with_features()` 追加

```rust
impl Font {
    pub fn with_features(face: &FontFace, size: f64, features: FontFeatureSettings) -> Self {
        // 既存 Font::from_face (実機 src/font/mod.rs:142-153) と同じパターンで
        // FontFace の Arc<FontFaceInner> を新規構築する。同一 FontFace から複数
        // feature の Font を作るケースで `tables.clone()` のディープコピーが
        // 発生するが、本 issue では既存パターンを維持し最適化は別 issue で扱う。
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

    /// FontFeatureSettings は `#[non_exhaustive]` で将来フィールド追加余地がある
    /// ため、参照返しで API 不変を保つ (`Clone` で値を取りたければ呼び出し側で
    /// `.clone()` する)。
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

        // 2. GSUB 適用: 各 Lookup は配列上のグリフを (Single) 置換 / (Ligature) 縮約
        //    する。部分適用 (一部グリフのみ置換) は許容する。失敗パスは continue で
        //    skip し、buf の状態は適用前後で SoA 不変条件 (3 配列同期) を維持する。
        shape::apply_gsub(&self.face.tables.gsub, buf, &self.feature_settings, self);

        // 3. GPOS 適用: リガチャ化後のグリフ列に対して走る。Pair Adjustment の
        //    Coverage が元のグリフ ID を対象としている場合、リガチャ後の合字グリフ
        //    はマッチしない (Microsoft OpenType 仕様の慣行)。部分適用は許容する。
        //    raden は Microsoft kern v0 非対応のため、GPOS が唯一のカーニング経路。
        if self.feature_settings.kern {
            shape::apply_gpos(&self.face.tables.gpos, buf, self);
        }
    }
}
```

`apply_gsub` / `apply_gpos` の擬似コード:

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
                let Some(ci) = s.coverage.index_of(buf.glyph_ids[i]) else { continue }
                match &s.format:
                    DeltaGlyphID(d) => {
                        // Microsoft OpenType Spec GSUB Type 1 Format 1 は modulo 65536 wrap を仕様化。
                        // i32 加算後の as u16 キャストは natural wrap (& 0xFFFF) と同等のため仕様に整合。
                        buf.glyph_ids[i] = ((buf.glyph_ids[i] as i32) + (*d as i32)) as u16
                    },
                    SubstituteGlyphIDs(subs) => {
                        if let Some(&sub) = subs.get(ci as usize):
                            buf.glyph_ids[i] = sub
        }
        GsubLookup::Ligature(l) => {
            let mut i = 0
            while i < buf.glyph_ids.len():
                if let Some(ci) = l.coverage.index_of(buf.glyph_ids[i]):
                    if let Some(ligatures) = l.ligature_sets.get(ci as usize):
                        // 各 Ligature を順に試し、最初にマッチしたものを採用 (greedy)。
                        // Microsoft OpenType 仕様により、LigatureSet 内の Ligature は
                        // フォントビルダーが component 数の降順 (longest first) で
                        // 格納する慣行のため、本実装では最初マッチを採用するだけで
                        // longest match 相当の結果が得られる。
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
                            buf.glyph_ids[i] = ligature_glyph
                            buf.placements[i] = GlyphPlacement {
                                offset_x: 0.0,
                                offset_y: 0.0,
                                advance: font.glyph_advance(ligature_glyph),
                            }
                            // i+1..i+consumed を 3 配列同期 drain で削除
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
pub(crate) fn apply_gpos(gpos: &GposTable, buf: &mut GlyphBuffer, font: &Font):
    let scale = font.scale()
    for &idx in &gpos.kern_lookups:
        if let Some(lookup) = gpos.lookup_at(idx):
            match lookup:
                GposLookup::Single(s) => apply_gpos_single(s, buf, scale),
                GposLookup::Pair(p) => apply_gpos_pair(p, buf, scale),
                GposLookup::Unsupported => ()

// Single Adjustment は単体グリフ対象のため最後のグリフも処理する
// (Pair Adjustment と異なり次のグリフは見ない)
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

// Pair Adjustment は隣接 2 グリフ対象のため最後のグリフペアは存在しない
// (i + 1 が範囲外になるため saturating_sub(1) でループ終端を 1 つ手前にする)
fn apply_gpos_pair(p: &GposPairAdjust, buf: &mut GlyphBuffer, scale: f64):
    for i in 0..buf.glyph_ids.len().saturating_sub(1):
        let g1 = buf.glyph_ids[i]
        let g2 = buf.glyph_ids[i + 1]
        if let Some(vr) = p.lookup(g1, g2):
            buf.placements[i].advance += vr.x_advance as f64 * scale
            // value2 (g2 側) は本 issue では適用しない (将来課題)。
            // GposPair 構造体には value2 を保持するが apply ロジックは value1 のみ。
```

### 5. `Font::measure_text` の修正 (`src/font/mod.rs`)

実機 `src/font/mod.rs:269-292` の `Font::measure_text` を以下に書き換える (0023 で実装された bbox 計算ロジックを `shape()` 経由に置換):

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

`Context` 構造体 (実機 `src/api/context.rs:316-317`) の `tmp_glyph_run: Vec<(u16, f64)>` フィールドを削除し、`tmp_shape_buffer: GlyphBuffer` フィールドに置換する。`Context::new` の初期化 (実機 L360) を `tmp_glyph_run: Vec::new()` から `tmp_shape_buffer: GlyphBuffer::default()` に変更する。

`fill_text` (実機 `src/api/context.rs:1148-1172`) を以下に書き換える:

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

`stroke_text` (実機 `src/api/context.rs:1178-1202`) を同様に書き換える (`self.fill_path(&path)` を `self.stroke_path(&path)` に差し替えるのみ)。0024 で実装された `stroke_text` には kern 加算ロジックが存在しないため (0025 closed で kern v0 非対応)、kern 加算の削除責務は発生しない。

0024 で書かれた `tests/test_context.rs::stroke_text::matches_manual_path` (実機 L539 周辺) の手動再実装ロジック (L566-574 周辺の `cmap` → `glyph_advance` 単純累積) を `font.shape("ABC")` の結果に基づく形に書き換える。テストフォントは **`load_arial()` 経由 (Arial.ttf 直参照、macOS 限定) のまま維持** する (本 issue では Source Sans 3 化しない)。Arial の `ABC` には GSUB Ligature / GPOS Pair Adjustment 対象ペアが存在しないため、`shape()` 経路でも `glyph_advance` 単純累積と同じ結果になるが、テストの目的は `stroke_text` 内部実装 (`shape_into`) と手動再現の整合性検証であり、仮に Arial で何らかの GPOS が入っても両者の結果が一致するため問題ない。

```rust
// font は load_arial() 経由で構築する (本 issue では Source Sans 3 化しない)。
let buffer = font.shape("ABC");
let mut cursor_x = baseline_x;
for (i, &gid) in buffer.glyph_ids.iter().enumerate() {
    let placement = buffer.placements[i];
    if gid != 0 {
        font.append_glyph_outline(
            gid,
            cursor_x + placement.offset_x,
            baseline_y + placement.offset_y,
            &mut expected_path,
        ).unwrap();
    }
    cursor_x += placement.advance;
}
```

### 7. CHANGES.md 更新

`shiguredo-changelog` スキルと 0001 メタ issue「font 公開 API の安定化方針」に従い、`## develop` 直下 (`### misc` の前、現状の `CHANGES.md` L12 `## develop` と L14 `### misc` の間) に `CHANGE` 種別、`### misc` 直下に `ADD` 種別を追加する。`@<author>` は実装者の GitHub ユーザー名 (例: `@voluntas`) で置換する。

`## develop` 直下に追加する `CHANGE` エントリ (4 件):

```
- [CHANGE] `Font::measure_text` の戻り値 `TextMetrics.advance` を OpenType GSUB / GPOS シェーピング適用済みの値に変更する
  - @<author>
- [CHANGE] `Font::measure_text` の戻り値 `TextMetrics.bounding_box` を OpenType GSUB / GPOS シェーピング適用済みの bbox に変更する
  - @<author>
- [CHANGE] `Context::fill_text` の描画結果のグリフ列・位置を OpenType GSUB / GPOS シェーピング適用済みに変更する
  - @<author>
- [CHANGE] `Context::stroke_text` の描画結果のグリフ列・位置を OpenType GSUB / GPOS シェーピング適用済みに変更する
  - @<author>
```

`## develop` の `### misc` 直下に追加する `ADD` エントリ (11 件):

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
- L447 (`map_text_to_glyphs(BLGlyphBuffer&)`): raden 列の 0022 で追記された `Font::glyph_run_for_text (pub(crate))` 文字列を削除し、`Font::shape() / Font::shape_into() の内部処理` に置換 (公開 API は `Font::map_char_to_glyph(char) -> u16` 維持)
- L448 (`position_glyphs(BLGlyphBuffer&)`): raden 列の 0022 で追記された `Font::glyph_run_for_text (pub(crate))` 文字列を削除し、`Font::shape() / Font::shape_into() の内部処理` に置換 (公開 API は `Font::glyph_advance(u16) -> f64` 維持)
- **L449 (`apply_kerning(BLGlyphBuffer&)`): raden 列を「raden は対応しない (Microsoft `kern` v0 非対応、GPOS Pair Adjustment は 0026 で実装)」に確定する** (0025 close 後続作業として本 issue が引き継ぐ)
- L450 (`apply_gsub` / `apply_gpos`): raden 列に `Font::shape() の内部実装 (GSUB Type 1/4 + GPOS Type 1/2 + Extension Type 7/9 + Coverage Format 1/2)` を追記
- L456 (`BLGlyphBuffer`): raden 列に `GlyphBuffer (SoA: glyph_ids + placements + clusters)` を追記

状態列 (`未実装` / `差異あり`) は本 issue では変更しない (L449 含む)。ただし L447 / L448 の状態列の説明文中の「`pub(crate)` の内部ヘルパー」(`glyph_run_for_text` を指す文言) は本 issue で `glyph_run_for_text` を削除するため不正確になる。L447 / L448 の説明文の `pub(crate) の内部ヘルパー` 部分のみ `Font::shape() / Font::shape_into() の内部処理` に書き換える (`差異あり` の判定自体は維持)。L455 (`get_text_metrics`、`TextMetrics`) は本 issue で `leading_bearing` / `trailing_bearing` を追加するが、状態列および raden 列の追記は 0001 メタ issue の close PR でまとめて整理する。

### 9. 0001 メタ issue の更新

本 issue close PR 内で `issues/0001-enhance-font-module-maturity.md` を以下のとおり更新する (0025 close 後続作業の引き継ぎを含む):

- **L49 周辺「Blend2D との対応表」L449 行**: raden 列を「raden は対応しない (kern v0 非対応)」に、「対象 concrete issue」列を「0026」に確定
- **L87-94「段階拡張で意味が変わる API」表**: 「0025 時点 (close)」列を全削除し「0022 時点 (close)」「0026 時点 (close)」の 2 列構成に再構築。`TextMetrics.advance` 行、`TextMetrics` フィールド (追加) 行、`Font::glyph_run_for_text()` 行 (「本メソッドは 0026 で削除済み」)、`Font::from_face` シグネチャ行 (「シグネチャは `(&FontFace, f64) -> Self` のまま維持。`FontFeatureSettings` 対応は `Font::with_features` / `Font::set_feature_settings` で実現済み」)、`GlyphBuffer` レイアウト行 (「SoA、glyph_ids: Vec<u16> / placements: Vec<GlyphPlacement> / clusters: Vec<u32> の 3 配列。placement / advance は f64、cluster は UTF-8 byte index」) を確定値で更新
- **L106 周辺「Concrete issue 一覧」**: 0025 行のステータスを `closed` に更新
- **L174-181「依存関係」セクション**: 「0025 → 0022, 0024」行を削除、「0026 → 0022, 0025」を「0026 → 0022, 0023, 0024」に修正、「closed にする順序は 0024 → 0025 → 0026 → 0027」を「0024 → 0026 → 0027」に修正、「0024 で書かれた `stroke_text` は 0025 完了時にカーニング適用へ改修する必要があり、この後追い改修は 0025 のスコープに含む」を削除
- **L194「備考」**: 「0026 polish」項目を削除 (本 polish 完了で対応済み)
- **`non_exhaustive` 対象**: 現行 0001 L82 の 4 型 (`TextMetrics` / `GlyphBounds` / `GlyphBuffer` / `FontFeatureSettings`) に `GlyphPlacement` を加えて 5 型に拡張する (本 issue で `GlyphPlacement` にも同方針を適用)

### 10. `src/font/shape.rs` モジュール宣言

`src/font/mod.rs` 冒頭 (`pub(crate) mod glyph;` / `pub(crate) mod tables;` の直後) に `pub(crate) mod shape;` を追加する。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/tables.rs` | 既存編集 | `TAG_GSUB` / `TAG_GPOS` 定数、`GsubTable` / `GposTable` / `GsubLookup` / `GposLookup` / `ValueRecord` / `Coverage` / `ClassDef` 等の構造体定義、`parse_gsub` / `parse_gpos` および補助 fn (`find_script` 等)、`ParsedTables` への追加と `parse_all` 組み込み |
| `src/font/mod.rs` | 既存編集 | `GlyphBuffer` / `GlyphPlacement` / `FontFeatureSettings` 構造体定義、`Font` への `feature_settings` フィールド追加と `#[derive(Clone)]`、`Font::with_features` / `set_feature_settings` / `feature_settings` / `clone_with_features` / `shape` / `shape_into`、`Font::from_face` の `with_features` 経由実装、`Font::measure_text` のシェーピング統合、`compute_bounding_box` / `compute_leading_bearing` / `compute_trailing_bearing` private fn、`Font::glyph_run_for_text` の削除、`TextMetrics` への `leading_bearing` / `trailing_bearing` フィールド追加、`pub(crate) mod shape;` の宣言追加、`Font::measure_text` の docstring 1 文置換 |
| `src/font/shape.rs` | 新規 | `apply_gsub` / `apply_gpos` / `apply_gsub_lookup` / `apply_gpos_single` / `apply_gpos_pair` の `pub(crate) fn` 実装 |
| `src/api/context.rs` | 既存編集 | `Context.tmp_glyph_run` フィールドを `tmp_shape_buffer: GlyphBuffer` に置換、`Context::new` の初期化変更、`fill_text` / `stroke_text` を `Font::shape_into` 経由に書き換え |
| `src/lib.rs` | 既存編集 | `pub use font::{... FontFeatureSettings, GlyphBuffer, GlyphPlacement, ...}` を alphabetical 順で追加 |
| `tests/test_font.rs` | 既存編集 | 単体テスト追加 (`font_shape_*` 系、Source Sans 3 を `fetch_source_sans_3_bytes` 経由で取得)、0021-0024 で書かれたテストのうちシェーピング影響を受けるものを更新 |
| `tests/test_context.rs` | 既存編集 | `stroke_text::matches_manual_path` の手動再実装を `Font::shape` ベースに書き換え |
| `pbt/tests/prop_font/main.rs` | 既存編集 | 0022 PBT 4 件 (`prop_concatenation` / `prop_non_negative` / `prop_single_char_advance` / `prop_size_linearity` の関数本体と `#[test]`) の削除、本 issue で追加する 5 件の PBT (`prop_shape_lengths_match` / `prop_shape_glyph_count_upper_bound` / `prop_shape_clusters_monotonic` / `prop_shape_advance_sum_matches_measure_text` / `prop_shape_empty_text`) の追加 (load_arial 経由で Arial に依存しない不変条件のみ検証) |
| `docs/BLEND2D.md` | 既存編集 | L444 / L446 / L447 / L448 / L449 / L450 / L456 の raden 列に新規 API 追記 (L449 は「対応しない」に確定、状態列は変更しない) |
| `issues/0001-enhance-font-module-maturity.md` | 既存編集 | 解決方法 9 の全更新 (L49 Blend2D 対応表 L449、L87-94 段階拡張表「0025 時点」列削除と 2 列化、L106 Concrete issue 一覧の 0025 closed 化、L174-181 依存関係セクション、L194 備考、`non_exhaustive` 対象 4 型への `GlyphPlacement` 追加) |
| `CHANGES.md` | 既存編集 | `## develop` 直下に `CHANGE` 4 件、`### misc` 直下に `ADD` 11 件 |
| `Cargo.toml` / `Cargo.lock` / `pbt/Cargo.toml` | 変更不要 | 本 issue では依存追加なし。`tests/` 配下は publish 配布物から除外済み |
| `tests/helpers/mod.rs` / `tests/helpers/font_fetch.rs` | 変更不要 | 既存 `pub mod font_fetch;` と `fetch_source_sans_3_bytes` をそのまま再利用 |
| `fuzz/fuzz_targets/parse_gsub.rs` / `parse_gpos.rs` | 追加しない | `parse_all` 経由の既存 fuzz target でカバーされる。テーブル深部のカバレッジ問題は 24 時間連続実行収束で対応 (0001 メタ issue 完了条件、0025 と同方針) |

## エッジケース

- `text` が空文字列: `GlyphBuffer` の 3 配列全てが空、`measure_text` は `TextMetrics { advance: 0.0, bounding_box: None, leading_bearing: 0.0, trailing_bearing: 0.0 }`
- GSUB テーブル不在: `apply_gsub` は何もせず、`glyph_ids` は cmap 経由のまま
- GPOS テーブル不在: `apply_gpos` 内の `gpos.kern_lookups` が空のため何もしない。カーニング非適用
- GPOS Pair Adjustment 不在のフォント: カーニング非適用 (raden は kern v0 にフォールバックしない)
- `FontFeatureSettings::default()` で `kern: true` / `liga: true` / `clig: true` だが、フォントが対応していない feature は適用されない (GSUB / GPOS の LookupList を解釈した結果として何も起きない)
- リガチャ非対応フォント (GSUB 不在 / Ligature Substitution 不在): GSUB 適用がスキップされ、`glyph_ids.len() == text.chars().count()`
- 単一文字: 隣接ペアなし、Pair Adjustment は適用されないが Single Adjustment / Single Substitution は適用される
- `size == 0`: `scale == 0` で全 `placements[].advance == 0.0`、`offset_*` も 0
- `size < 0` / NaN / Inf: テストでロックしない (0021 / 0022 と方針共有)
- Extension Lookup の内部 Lookup Type がさらに Extension (Type 7 / 9): 仕様上は許容されないため `Unsupported` 扱い (再帰しない)
- Coverage Table Format 1 で glyph_ids が **昇順でない** 不正フォント: `binary_search` は `Err(insertion_pos)` を返し、`Coverage::index_of` は `.ok().map(...)` で `None` に縮退する。シェーピングは当該グリフで noop となり panic / UB は発生しない。`is_sorted()` バリデーションは行わない (寛容方針)

## 隣接 issue への影響

- **0022 への影響**: `Font::glyph_run_for_text` を削除し、`Font::shape` / `Font::shape_into` に置換。`Font::measure_text` の内部実装を `shape` 経由に書き換え。`Font::measure_text` の docstring 1 文を置換
- **0023 への影響**: `Font::glyph_bounds` は変更不要 (個別グリフの bbox 取得のため)。0023 で実装された `TextMetrics::bounding_box` の計算ロジックを `shape()` 経由の `compute_bounding_box` ヘルパーで置換。0023 で書かれた PBT のうち、`prop_glyph_bounds_*` (個別 glyph_bounds の不変条件) は維持される。`prop_measure_text_bounding_box_*` (measure_text 経由) は `shape()` 経由になる影響を受けるが、本 issue では維持する方針 (合字グリフ ID 経由の glyph_bounds union が正しく機能することを `compute_bounding_box` のロジックで保証)
- **0024 への影響**: `Context::stroke_text` を `Font::shape_into` 経由に書き換え。0024 のテスト (`stroke_text::matches_manual_path` の手動再実装) はシェーピング適用後の描画結果に変わるため更新
- **0025 への影響**: なし (0025 は closed=非対応で実装が存在しない)。0025 close 後続作業として本 issue が `docs/BLEND2D.md` L449 raden 列確定責務と 0001 メタ issue 関連更新責務を引き継ぐ
- **0001 メタ issue への影響**: 段階拡張表の 2 列化、Concrete issue 一覧の 0025 closed 化、依存関係セクションの整理、備考 (L194) の更新、`non_exhaustive` 対象 4 型への `GlyphPlacement` 追加 (本 issue で確定値で更新)
- **0027 への申し送り (重要)**: 0027 (CFF / CFF2 アウトライン対応) が `Font::shape` / `GlyphBuffer` / `compute_bounding_box` を CFF / CFF2 経路でも流用するため、本 issue で導入する `Font::shape` / `Font::shape_into` および `compute_bounding_box` ヘルパーは CFF / CFF2 outline でも動作するように `font.glyph_advance` / `font.glyph_bounds` 経由でアクセスする (アウトライン形式に依存しない API 設計)。0027 polish 時に確認する

## テスト戦略

### 単体テスト (`tests/test_font.rs` / `tests/test_context.rs`)

Source Sans 3 (`fetch_source_sans_3_bytes`) を 0039 確定の panic 方式 (`unwrap_or_else(|e| panic!(...))`) で取得し、`FontFace::from_data` でロードしてテストする。`Option<FontFace>` を返すスキップ前提は採用しない (0039 流儀)。

`tests/test_font.rs` 末尾 (既存 `fetch_source_sans_3_has_required_tables` の近く) に `load_source_sans_3()` ヘルパーを定義する。**本 issue では `tests/test_font.rs` の 1 ファイル内に直接定義** (`tests/test_context.rs::matches_manual_path` の手動再実装は引き続き `load_arial` 経由で Arial を使い、本 issue で Source Sans 3 化はしない)。`tests/helpers/font.rs` への集約 (`load_arial` と `load_source_sans_3` を共通モジュールに統合) は 0037 polish で扱う:

```rust
fn load_source_sans_3() -> FontFace {
    let bytes = fetch_source_sans_3_bytes()
        .unwrap_or_else(|e| panic!("Source Sans 3 をダウンロードできる必要がある: {e:?}"));
    let data = FontData::from_bytes(bytes);
    FontFace::from_data(&data, 0)
        .expect("Source Sans 3 を FontFace としてロードできる必要がある")
}
```

追加する単体テスト (Source Sans 3 ベース):

- `font_shape_simple`: `shape("Hello")` の `glyph_ids.len() == 5` を検証 (Hello には Source Sans 3 のリガチャ対象ペアがないため、GSUB 適用後もグリフ数が変わらないことを確認)
- `font_shape_ligature_ff`: `shape("ff")` の `glyph_ids.len() == 1` かつ `clusters[0] == 0` を strict に検証。Source Sans 3 3.052R 実機で `f + f → f_f` リガチャ (glyph ID 687) が存在することを polish で確認済 (本 polish の必要性判断エージェントの検証結果)。`fi` は Source Sans 3 に存在しないため使用しない
- `font_shape_feature_disabled`: `font.clone_with_features(FontFeatureSettings { liga: false, ..Default::default() }).shape("ff")` の `glyph_ids.len() == 2` を検証 (リガチャ無効化で `f + f` が 2 グリフのまま)
- `font_shape_kern_av`: `shape("AV")` の `placements[0].advance` を検証。Source Sans 3 3.052R 実機で AV ペアは GPOS Pair Adjustment Format 2 (Extension Type 9 経由)、XAdvance = -14 デザインユニット (units_per_em = 1000)。size = 48px なら scale = 0.048、advance 減算は `-14 * 0.048 = -0.672 px`。`glyph_advance('A') - placements[0].advance` が `0.672 ± 1e-6` で一致することを `close_enough` (相対 + 絶対許容) で検証する。Extension Type 9 unwrap と Pair Adjustment Format 2 マッチングの両方の経路を本テストで通過する。kern fallback は raden が kern v0 非対応のため経由しない (本テストは GPOS のみを検証)
- `close_enough` ヘルパー: `tests/test_font.rs` 内に `fn close_enough(lhs: f64, rhs: f64) -> bool { (lhs - rhs).abs() < (lhs.abs() + rhs.abs()) * 1e-9 + 1e-12 }` を新規定義する (`pbt/tests/prop_font/main.rs:29-31` の同名 fn は pbt クレートの test target 内で参照不可のため、raden 本体クレートの test target 用に再定義する)。`font_shape_kern_av` と `font_measure_text_with_shaping` の両テストで使う
- `font_shape_empty`: `shape("").glyph_ids.is_empty()` 等
- `font_shape_into_reuse`: `shape_into` で既存 buffer を再利用しても結果が一致することを検証 (clear → shape_into → 結果取得 → 別文字列で shape_into → 結果取得 で 2 回の結果が独立)
- `font_with_features_and_set`: `Font::with_features` / `Font::set_feature_settings` で設定した feature が `shape` 結果に反映されることを検証
- `font_measure_text_with_shaping`: `measure_text("AV").advance` が `shape("AV").placements.iter().map(|p| p.advance).sum()` と一致 (相対 + 絶対許容)
- `font_text_metrics_leading_trailing_bearing`: `measure_text("A")` の `leading_bearing` / `trailing_bearing` が `glyph_bounds('A')` から計算された値と一致

テストのアサーションメッセージは日本語で書く (`CLAUDE.md` 規約に従う。0039 で確定済の「~である必要がある」文体)。

### PBT (`pbt/tests/prop_font/main.rs`)

PBT は pbt クレートが `tests/helpers/font_fetch.rs` を直接参照できない (別クレートのため `mod helpers;` 経由で読めない) ため、本 issue では **load_arial 経路** で Arial 依存の不変条件のみを検証する。Source Sans 3 固有の検証 (リガチャ ff、Pair Adjustment AV ペア) は単体テスト側 (`tests/test_font.rs`) に集約する。

追加する PBT (`load_arial` 経由、Arial 不在時はテスト全件スキップ):

- **`prop_shape_lengths_match`**: 任意の ASCII printable 文字列 `text` に対し `shape(text).glyph_ids.len() == placements.len() == clusters.len()` (SoA 配列長一致)
- **`prop_shape_glyph_count_upper_bound`**: `shape(text).glyph_ids.len() <= text.chars().count()` (リガチャでグリフ数が減ることがある)
- **`prop_shape_clusters_monotonic`**: 空でない場合 `shape(text).clusters` は単調非減少 (`a[i] <= a[i+1]`、HarfBuzz 慣行に従う)、`clusters[0] == 0`、`*clusters.last().unwrap() < text.len() as u32` (text の UTF-8 byte 長)
- **`prop_shape_advance_sum_matches_measure_text`**: `shape(text).placements.iter().map(|p| p.advance).sum::<f64>()` と `measure_text(text).advance` が 0022 の `close_enough` (相対 `1e-9` + 絶対 `1e-12`) で一致
- **`prop_shape_empty_text`**: `shape("").glyph_ids.is_empty() && shape("").placements.is_empty() && shape("").clusters.is_empty()`

ASCII printable 戦略 (`ascii_printable_string`) は 0022 で定義済みのものを共用する。`close_enough` も同様。Arial が GPOS Pair Adjustment / Ligature Substitution を持つかは検証時に確認するが、PBT は「不変条件」の検証で、フォント特性に依存しない (`prop_shape_lengths_match` 等は GSUB / GPOS の有無に関わらず成立する)。

### 削除する既存 PBT

本 issue で 0022 PBT 4 件すべてを削除する:

- `prop_concatenation` 関数本体 (実機 `pbt/tests/prop_font/main.rs:66-82`) と `#[test] fn concatenation()` (実機 L368-371): `Font::shape` 経由で GSUB Ligature / GPOS Pair Adjustment が隣接境界の advance を変化させるため結合性は破綻
- `prop_non_negative` 関数本体 (実機 L106-115) と `#[test] fn non_negative()` (実機 L378-381): GPOS Pair Adjustment のネガティブ値で破綻
- `prop_single_char_advance` 関数本体 (実機 L48-58): GPOS Single Adjustment で破綻可能性
- `prop_size_linearity` 関数本体 (実機 L85-103): GPOS Single Adjustment / Pair Adjustment が size に対し非線形な場合に破綻可能性

### Fuzzing

`parse_all` 経由の既存 fuzz target が GSUB / GPOS テーブルバイトも含むフォントファイル全体を入力とするため、本 issue の新規追加 `parse_gsub` / `parse_gpos` の panic 耐性は自動的にカバーされる。テーブル深部 (Coverage Format 2 の range list、Class Definition Format 2 の range list、Pair Adjustment Format 2 のクラスマトリクス、Extension Lookup の入れ子) に到達しにくい問題は 24 時間連続実行収束で対応 (0001 メタ issue 完了条件、0025 の `parse_kern` fuzz と同方針)。

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L444 / L446 / L447 / L448 / L449 / L450 / L456、段階拡張表の各行確定責務、依存関係セクション更新責務、L194 備考の本 polish 項目削除責務、`non_exhaustive` 対象 4 型への `GlyphPlacement` 追加責務)
- 依存: `closed/0022-add-text-measurement.md` (`Font::measure_text` の存在、`glyph_run_for_text` の置換対象、0022 PBT 4 件の削除対象)、`closed/0023-add-glyph-bounds.md` (`TextMetrics::bounding_box` の `shape()` 経由再計算対象、`Font::glyph_bounds` を `compute_bounding_box` 内で使用)、`closed/0024-add-stroke-text.md` (`stroke_text` のシェーピング統合対象、`stroke_text::matches_manual_path` の手動再実装更新対象)、`closed/0039-add-test-font-downloader.md` (`fetch_source_sans_3_bytes` で Source Sans 3 を取得)
- 関連 (実装依存なし。0025 closed の決定 (kern v0 非対応) を前提とし、0025 close 後続作業を引き継ぐ): `closed/0025-add-font-kerning.md` (本 issue は GPOS Pair Adjustment が唯一のカーニング経路として実装する。本 issue close PR で 0025 closed 後続作業 (`docs/BLEND2D.md` L449 / 0001 メタ issue 関連更新) を引き継ぐ)
- 非対応 (将来の課題): GSUB の Multiple / Alternate Substitution、Context / Chaining Context、複雑スクリプト (Arabic / Indic)、BiDi、マーク配置 (MarkToBase / MarkToMark)、Variable Fonts、ValueRecord の Device Table 解釈、ValueRecord の value2 適用 (Pair Adjustment の g2 側)、LookupList 内の複数 subtable の累積適用
