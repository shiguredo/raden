# OpenType 基本シェーピング機能を追加する

- Priority: Medium
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-opentype-basic-shaping
- Polished: 2026-06-20

## 目的

OpenType の GSUB / GPOS レイアウト機能を適用し、リガチャ (例: `fi` → `ﬁ`) や GPOS Pair Adjustment によるカーニング等の高度なテキスト表現を可能にする。`Font::shape(text, &features) -> GlyphBuffer` を新規追加し、0022 で導入された `glyph_run_for_text` (`pub(crate)`) を完全に置換する。`fill_text` / `measure_text` / `stroke_text` の 3 経路をシェーピング適用形に書き換える。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L444 `BLFont::feature_settings()` / `set_feature_settings()`**: raden は `FontFeatureSettings` 構造体と `Font::with_features(&FontFace, f64, FontFeatureSettings)` または `Font::set_feature_settings(&mut self, FontFeatureSettings)` で対応 (シグネチャは本 issue polish で確定)
- **L446 `BLFont::shape(BLGlyphBuffer&)`**: raden は `Font::shape(text: &str, features: &FontFeatureSettings) -> GlyphBuffer` で対応 (Blend2D は `BLGlyphBuffer` への in-place 操作だが、raden は新規 `GlyphBuffer` を生成して返す形)
- **L450 (部分) `BLFont::apply_gsub(BLGlyphBuffer&, BLBitArray&)` / `apply_gpos(BLGlyphBuffer&, BLBitArray&)`**: raden は `Font::shape()` の内部実装として基本 Lookup Type のみ対応 (GSUB Single Substitution / Ligature Substitution、GPOS Single Adjustment / Pair Adjustment)。`BLBitArray` 相当 (lookup index 選択ビットマスク) は対応しない (font モジュール安定化前の別 issue で検討)
- **L456 `BLGlyphBuffer`**: raden は `GlyphBuffer` を新規定義 (本 issue で初出。レイアウトは下記「GlyphBuffer 設計」で確定)

差異の理由:

- raden は `BLGlyphBuffer` の in-place 操作と異なり、`Font::shape()` が新規 `GlyphBuffer` を返す API を採用する (Rust の所有権モデルとの整合)
- `BLBitArray` (lookup 選択ビットマスク) と GSUB / GPOS の Context / Chaining Context Lookup は本 issue では対応せず、font モジュール安定化前の別 issue で検討

完了 PR で `docs/BLEND2D.md` L444 / L446 / L450 / L456 の raden 列に新規 API 名を追記する。状態列 (`未実装`) の最終整理は 0001 メタ issue の close PR でまとめて行う。

## 現状

- テキスト描画は `cmap` による文字 → グリフ変換と単純な `advance` 累積のみ (0022 で `glyph_run_for_text` が導入され、0025 で `kern` テーブル経由のカーニングが加算されている状態)
- リガチャが適用されない (例: `"fi"` は別グリフ `f` + `i` のままで、合字 `ﬁ` (U+FB01) として描画されない)
- GSUB テーブルは未パース・未利用
- GPOS テーブルは未パース・未利用 (0025 の `kern` テーブル経由のみカーニングが効く)
- `BLGlyphBuffer` 相当の中間バッファがない (0001 メタ issue「課題一覧」L520-525 で確定済み)
- `FontFeatureSettings` (`kern` / `liga` / `clig` 等の feature on/off 制御) が未実装

## 設計方針

### `GlyphBuffer` のレイアウト (本 issue polish で確定)

Blend2D の `BLGlyphBuffer` (`~/src/blend2d/blend2d/core/glyphbuffer.h:50-120`) は `content: uint32_t*` (glyph IDs), `info: BLGlyphInfo*` (cluster 含む), `placement: BLGlyphPlacement*` の **3 配列 SoA** 構造。raden の `GlyphBuffer` も SoA レイアウトを採用する:

```rust
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct GlyphBuffer {
    /// グリフ ID 列 (シェーピング後)。
    pub glyph_ids: Vec<u16>,
    /// 各グリフの placement (offset_x, offset_y) と advance のペア (ピクセル単位 f64)。
    /// Blend2D の BLGlyphPlacement (BLPointI 整数) と異なり、raden は f64 を採用
    /// (raden の Path / Context API が f64 統一のため整合性優先)。
    /// 整数化 (BLPointI 互換) は font モジュール安定化前の別 issue で検討。
    pub placements: Vec<GlyphPlacement>,
    /// 各グリフの元コードポイント (cluster)。リガチャ後の caret 位置 / 選択範囲 / コピー用。
    /// 元の文字列の char index を u32 で保持。
    pub clusters: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct GlyphPlacement {
    pub offset_x: f64,
    pub offset_y: f64,
    pub advance: f64,
}
```

- **SoA** を採用 (Blend2D 互換)。glyph_ids / placements / clusters の 3 配列の長さは常に一致 (`shape()` の事後条件)
- **placement / advance は f64** (Blend2D の `BLPointI` (i32) ではない)。raden の他 API との整合性優先。整数化は別 issue
- **`cluster` 配列を含める**。BiDi / 複雑スクリプトは本 issue 対象外だが、ラテン系リガチャ (`fi` / `fl` 等) でも caret 位置決定に必要

### `Font::from_face` シグネチャ拡張 (本 issue polish で確定)

`Font::from_face(&FontFace, f64) -> Self` を維持し、`FontFeatureSettings` を渡すための **別メソッド** `Font::with_features(&FontFace, f64, FontFeatureSettings) -> Self` を新規追加する (`Font::from_face` のシグネチャ自体は変更しない)。

選択理由:

- 既存 `Font::from_face` 呼び出し (0021-0025 のテスト含む) の書き換え範囲を最小化する
- `Font::with_features` を新規メソッドにすれば、デフォルト feature 設定で `Font::from_face` を呼ぶ既存パスは変更不要
- 内部実装は `Font::from_face` が `Font::with_features(face, size, FontFeatureSettings::default())` を呼ぶ形にリファクタ

これにより 0021-0025 で書かれた `Font::from_face(&face, size)` 呼び出しは全て変更不要。0021-0025 のテストは「Font 自体に `feature_settings` フィールドが追加された」点のみ既存 API として影響を受けるが、メソッド呼び出しレベルでは互換性が保たれる。

### `FontFeatureSettings`

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

### サポートする GSUB / GPOS Lookup Type

- **GSUB Lookup Type 1: Single Substitution** (1 グリフ → 1 グリフ)
- **GSUB Lookup Type 4: Ligature Substitution** (N グリフ → 1 グリフ。fi / fl 等)
- **GPOS Lookup Type 1: Single Adjustment** (個別グリフの位置調整)
- **GPOS Lookup Type 2: Pair Adjustment** (ペアグリフの位置調整。kern テーブルの後継)

非対応 (将来の課題):

- GSUB Lookup Type 2 (Multiple Substitution)、3 (Alternate Substitution)、5/6 (Context / Chaining Context)、7 (Extension)、8 (Reverse Chaining)
- GPOS Lookup Type 3 (Cursive)、4 (Mark to Base)、5 (Mark to Ligature)、6 (Mark to Mark)、7/8 (Context / Chaining Context)
- 複雑スクリプト (Arabic / Indic / Hebrew 等)
- 双方向テキスト (BiDi)

### `kern` テーブル経路の扱い

0025 で実装された `FontFace::kern` / `Font::kern` および `fill_text` / `measure_text` / `stroke_text` 内の `kern` 経由のカーニング適用は、本 issue の GPOS Pair Adjustment 経路に **置換** する。

- **`Font::shape()` 経路**: GPOS Pair Adjustment が存在するフォントでは GPOS を優先 (kern テーブルは無視)。GPOS Pair Adjustment が存在しないフォント (legacy フォント) では `kern` テーブルをフォールバックとして使用
- **`FontFace::kern` / `Font::kern` API**: GPOS と kern テーブルのいずれかから値を返す統合 API として残す (削除しない)。docstring を「GPOS Pair Adjustment が優先、不在時 kern テーブルにフォールバック」に更新
- **`fill_text` / `measure_text` / `stroke_text`**: 0025 で実装された `kern` 直接呼び出しを削除し、`Font::shape()` 経由に置換 (`shape()` の戻り値の `placements[].advance` が GPOS 適用済み advance を含む)

### 0022 PBT の更新責務

0022 で書かれた `glyph_run_for_text` の total advance に対する結合性 PBT は、`shape()` 置換後は `glyph_run_for_text` 自体が削除されるため **本 issue で削除する**。代わりに `Font::shape(text, features).placements.iter().map(|p| p.advance).sum()` に対する PBT を追加する (リガチャで結合性が破綻するため、結合性は検証しない方針)。

## 完了条件

### 追加される API

- `GlyphBuffer` 構造体 (SoA レイアウト、`#[non_exhaustive]`、上記「GlyphBuffer のレイアウト」参照)
- `GlyphPlacement` 構造体 (`#[non_exhaustive]`)
- `FontFeatureSettings` 構造体 (`#[non_exhaustive]`、Default 実装)
- `Font::with_features(&FontFace, f64, FontFeatureSettings) -> Self`
- `Font::shape(text: &str, features: &FontFeatureSettings) -> GlyphBuffer`

### 削除される内部 API

- `Font::glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` (`pub(crate)`、0022 で追加) を削除
- `Context` の `tmp_glyph_run: Vec<(u16, f64)>` フィールドを `tmp_shape_buffer: GlyphBuffer` 等に置換

### 既存 API のシグネチャ維持と意味的変化

- `Font::from_face(&FontFace, f64) -> Self` のシグネチャは変更しない (内部実装で `Font::with_features(face, size, FontFeatureSettings::default())` を呼ぶ形に変更)
- `Font::measure_text(&str) -> TextMetrics`: シグネチャ不変だが、`TextMetrics.advance` の意味が「カーニング適用済み」(0025) から「シェーピング適用済み (GSUB + GPOS)」に変わる (`CHANGE` 種別)
- `Context::fill_text` / `Context::stroke_text`: シグネチャ不変だが、描画結果がシェーピング適用済みのグリフ列・位置に変わる (`CHANGE` 種別)
- `FontFace::kern` / `Font::kern`: シグネチャ不変だが、GPOS Pair Adjustment 優先 / kern テーブルフォールバックの動作に変わる (`CHANGE` 種別)

### 0022 PBT の更新

- 0022 PBT の「`glyph_run_for_text` の total advance に対する結合性」は本 issue で削除 (`glyph_run_for_text` 自体が削除されるため)
- 代わりに `Font::shape(text, features)` に対する基本的な不変条件 PBT を追加 (例: `shape("").glyph_ids.is_empty()`、`shape(c).glyph_ids.len() == 1` (リガチャを構成しない単一文字)、`shape(text).glyph_ids.len() <= text.chars().count()` (Ligature Substitution でグリフ数が減ることがある))

### 0021-0025 のテスト書き換え

0021-0025 で書かれた `Font::from_face(&face, size)` 呼び出しは **シグネチャが変わらないため変更不要** (本 issue の設計判断による)。新規 `Font::with_features` を使うテストは本 issue で追加する。0021-0025 で書かれたテストが GSUB / GPOS の影響で結果が変わる場合 (例: 0022 の `measure_text("fi")` がリガチャ適用でグリフ数が減る) は、各 issue のテストを本 issue で更新する。

### ドキュメント

- `docs/BLEND2D.md` L444 / L446 / L450 / L456 の raden 列に新規 API 名を追記
- `CHANGES.md` の `## develop` のトップ階層に `CHANGE` 種別で複数件 (`Font::measure_text`、`Context::fill_text` / `stroke_text`、`FontFace::kern` / `Font::kern` の意味的変化)
- `CHANGES.md` の `## develop` の `### misc` 直下に `ADD` 種別で複数件 (`GlyphBuffer` / `GlyphPlacement` / `FontFeatureSettings` / `Font::with_features` / `Font::shape`)

## 解決方法

### 1. GSUB / GPOS テーブルパース追加 (`src/font/tables.rs`)

- `TAG_GSUB` / `TAG_GPOS` 定数追加
- `GsubTable` / `GposTable` 構造体新規定義 (基本 Lookup Type のみ保持。Coverage Table / Lookup List / Feature List / Script List のサブセット)
- `parse_gsub` / `parse_gpos` を新規作成 (寛容方針で不正データは空テーブル扱い)
- `ParsedTables` に `pub gsub: GsubTable` / `pub gpos: GposTable` を追加
- 詳細な OpenType 仕様参照は実装時に Microsoft OpenType Specification を参照 (本 issue では仕様内容を詳細展開しない)

### 2. 構造体定義 (`src/font/mod.rs`)

`GlyphBuffer` / `GlyphPlacement` / `FontFeatureSettings` を上記「GlyphBuffer のレイアウト」「FontFeatureSettings」のとおり定義する。

### 3. `Font::with_features()` 追加 (`src/font/mod.rs`)

```rust
impl Font {
    pub fn with_features(face: &FontFace, size: f64, features: FontFeatureSettings) -> Self {
        // 既存 Font::from_face の処理 + feature_settings の保持
        let scale = size / face.tables.units_per_em as f64;
        let inner = Arc::new(FontFaceInner {
            data: Arc::clone(&face.data),
            tables: face.tables.clone(),
        });
        Self {
            face: inner,
            size,
            scale,
            feature_settings: features,
        }
    }
}

// Font::from_face は内部で with_features を呼ぶ形にリファクタ
impl Font {
    pub fn from_face(face: &FontFace, size: f64) -> Self {
        Self::with_features(face, size, FontFeatureSettings::default())
    }
}
```

`Font` 構造体に `feature_settings: FontFeatureSettings` フィールドを追加 (`Clone` 可能なため `Font` のコストには影響軽微)。

### 4. `Font::shape()` 追加 (`src/font/mod.rs`)

```rust
impl Font {
    pub fn shape(&self, text: &str, features: &FontFeatureSettings) -> GlyphBuffer {
        let mut buffer = GlyphBuffer {
            glyph_ids: Vec::new(),
            placements: Vec::new(),
            clusters: Vec::new(),
        };
        // 1. cmap でテキスト → グリフ ID 列に変換 (cluster 情報を保持)
        for (i, ch) in text.chars().enumerate() {
            let gid = self.map_char_to_glyph(ch);
            buffer.glyph_ids.push(gid);
            buffer.clusters.push(i as u32);
            buffer.placements.push(GlyphPlacement {
                offset_x: 0.0,
                offset_y: 0.0,
                advance: self.glyph_advance(gid),
            });
        }
        // 2. GSUB 適用 (features.liga / features.clig に従う)
        if features.liga || features.clig {
            apply_gsub_basic(&self.face.tables.gsub, &mut buffer, features);
        }
        // 3. GPOS 適用 (features.kern に従う)
        if features.kern {
            apply_gpos_basic(&self.face.tables.gpos, &mut buffer);
        }
        // 4. GPOS 不在で features.kern が true なら kern テーブルにフォールバック
        if features.kern && self.face.tables.gpos.is_empty() {
            apply_kern_fallback(&self.face.tables.kern, &mut buffer, self.scale);
        }
        buffer
    }
}
```

`apply_gsub_basic` / `apply_gpos_basic` / `apply_kern_fallback` は `src/font/shape.rs` (新規モジュール) に実装。

### 5. `Font::measure_text` の修正 (`src/font/mod.rs`)

```rust
pub fn measure_text(&self, text: &str) -> TextMetrics {
    let buffer = self.shape(text, &self.feature_settings);
    let advance: f64 = buffer.placements.iter().map(|p| p.advance).sum();
    let bounding_box = compute_bounding_box(&buffer, &self);
    TextMetrics { advance, bounding_box }
}
```

`bounding_box` は 0023 で追加されたフィールド。

### 6. `Context::fill_text` / `Context::stroke_text` の修正 (`src/api/context.rs`)

`glyph_run_for_text` を呼ぶ代わりに `Font::shape` を呼び、`GlyphBuffer.glyph_ids` / `placements` を使って描画:

```rust
pub fn fill_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
    let mut path = std::mem::take(&mut self.tmp_path);
    let mut buffer = std::mem::take(&mut self.tmp_shape_buffer);
    path.clear();
    buffer.glyph_ids.clear();
    buffer.placements.clear();
    buffer.clusters.clear();

    // Font::shape は新規 GlyphBuffer を返すため、ここではバッファ再利用のための
    // 専用 API (Font::shape_into(&mut buffer)) が将来必要。本 issue では新規生成で開始する
    // (パフォーマンス最適化は別 issue)。
    buffer = font.shape(text, &font.feature_settings);

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

`stroke_text` も同様に `self.stroke_path(&path)` に置換するのみ。

### 7. CHANGES.md 更新

トップ階層 (`CHANGE` 種別) と `### misc` 直下 (`ADD` 種別) に複数件追加 (本 issue では具体的なエントリ列挙を省略。実装時に列挙)。

### 8. BLEND2D.md 更新

`docs/BLEND2D.md` L444 / L446 / L450 / L456 の raden 列に新規 API 名を追記する。状態列の最終整理は 0001 メタ issue の close PR でまとめて行う。

### 9. 0001 メタ issue 段階拡張表の更新

0001 メタ issue「段階拡張で意味が変わる API」表の `GlyphBuffer` レイアウト行を「0026 で確定」から「**SoA、placement / advance は f64、cluster 配列を持つ**」に更新する責務は本 issue にある (0001 メタ issue 内で「0026 polish 時に本表を更新」と確定済み)。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/tables.rs` | 既存編集 | `TAG_GSUB` / `TAG_GPOS` 定数、`GsubTable` / `GposTable` 構造体、`parse_gsub` / `parse_gpos`、`ParsedTables` への追加 |
| `src/font/mod.rs` | 既存編集 | `GlyphBuffer` / `GlyphPlacement` / `FontFeatureSettings` 構造体定義、`Font::with_features` / `Font::shape`、`Font::from_face` のリファクタ、`Font::measure_text` のシェーピング統合、`Font::glyph_run_for_text` の削除 |
| `src/font/shape.rs` | 新規 | `apply_gsub_basic` / `apply_gpos_basic` / `apply_kern_fallback` |
| `src/api/context.rs` | 既存編集 | `Context` の `tmp_glyph_run` を `tmp_shape_buffer: GlyphBuffer` に置換、`fill_text` / `stroke_text` を `Font::shape` 経由に書き換え |
| `src/lib.rs` | 既存編集 | `pub use font::{... FontFeatureSettings, GlyphBuffer, GlyphPlacement, ...}` を alphabetical 順で追加 |
| `tests/test_font.rs` / `tests/test_context.rs` | 既存編集 | 単体テスト追加、0021-0025 で書かれたテストのうちシェーピング影響を受けるものを更新 |
| `pbt/tests/prop_font/main.rs` | 既存編集 | 0022 結合性 PBT の削除、`Font::shape` 基本 PBT の追加、0025 隣接ペア加算 PBT の更新 |
| `fuzz/fuzz_targets/parse_gsub.rs` / `parse_gpos.rs` | 新規 | GSUB / GPOS テーブル fuzz target |
| `docs/BLEND2D.md` | 既存編集 | L444 / L446 / L450 / L456 の raden 列に新規 API 追記 |
| `issues/0001-enhance-font-module-maturity.md` | 既存編集 | 段階拡張表の `GlyphBuffer` レイアウト行を確定値で更新 |
| `CHANGES.md` | 既存編集 | トップ階層に `CHANGE` 複数件、`### misc` 直下に `ADD` 複数件 |

## エッジケース

- `text` が空文字列: `GlyphBuffer` の 3 配列全てが空
- リガチャ非対応フォント (GSUB 不在 / Ligature Substitution 不在): GSUB 適用がスキップされ、`glyph_ids.len() == text.chars().count()`
- GPOS / kern テーブル両方不在: カーニング適用なし (両者とも 0)
- `FontFeatureSettings::default()` で `kern: true` / `liga: true` / `clig: true` だが、フォントが対応していない feature は適用されない
- `size == 0`: `scale == 0` で全 `placements[].advance == 0.0`、`offset_*` も 0
- `size < 0` / NaN / Inf: テストでロックしない (0021-0025 と方針共有)

## 隣接 issue への影響

- **0022 への影響**: `Font::glyph_run_for_text` を削除し、`Font::shape` に置換。`Font::measure_text` の内部実装を `shape` 経由に書き換え。0022 結合性 PBT (`glyph_run_for_text` の total advance) を削除。`Font::measure_text` の docstring を「シェーピング適用済み」に更新
- **0023 への影響**: `Font::glyph_bounds` は変更不要 (個別グリフの bbox 取得のため)。0023 で追加された `TextMetrics::bounding_box` の計算が `shape()` 経由になるため、`compute_bounding_box` ヘルパー (本 issue で導入) で再実装
- **0024 への影響**: `Context::stroke_text` を `Font::shape` 経由に書き換え。0024 のテストはシェーピング適用後の描画結果に変わるため、必要に応じて更新
- **0025 への影響**: `fill_text` / `measure_text` / `stroke_text` 内の `kern` 直接呼び出しを削除し、`Font::shape` 経由 (内部で GPOS Pair Adjustment または kern フォールバック) に置換。`FontFace::kern` / `Font::kern` API は維持し docstring を「GPOS 優先、不在時 kern フォールバック」に更新
- **0001 メタ issue への影響**: 段階拡張表の `GlyphBuffer` レイアウト行を確定値で更新する責務は本 issue にある

## テスト戦略

### 単体テスト (`tests/test_font.rs` / `tests/test_context.rs`)

- `font_shape_simple`: `shape("Hello", &default)` の `glyph_ids.len() == 5` を検証 (Arial では `Hello` がリガチャ非対応)
- `font_shape_ligature`: `shape("fi", &default)` の `glyph_ids.len() == 1` を検証 (リガチャ対応フォントで合字 `ﬁ` のグリフ ID が返ることを期待。Arial に `fi` リガチャがあれば検証)
- `font_shape_feature_disabled`: `shape("fi", &FontFeatureSettings { liga: false, ..default() })` で `glyph_ids.len() == 2` を検証 (リガチャ無効化)
- `font_shape_kern`: `shape("AV", &default)` の `placements[0].advance` が `glyph_advance('A')` + GPOS kern より小さい (Arial が GPOS を持つかフォント依存だが、kern フォールバックでも同様)

### PBT (`pbt/tests/prop_font/main.rs`)

- **基本不変条件**: `shape(text, features).glyph_ids.len() <= text.chars().count()` (リガチャでグリフ数が減ることがある)、`glyph_ids.len() == placements.len() == clusters.len()` (SoA 配列長一致)
- **空文字列**: `shape("", &default)` の 3 配列が空
- **feature 無効化の冪等性**: `shape(text, &all_off)` の `glyph_ids.len() == text.chars().count()` (全 feature off で cmap マッピングのみ)

### 既存 PBT の更新

- 0022 結合性 PBT は削除 (`glyph_run_for_text` 削除に伴う)
- 0025 隣接ペア加算 PBT は `Font::shape` 経由の advance 総和との比較に書き換え (kern が GPOS に置換される可能性を考慮)

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L444 / L446 / L450 / L456、段階拡張表の `Font::from_face` / `GlyphBuffer` 行の確定責務)
- 依存: `0022-add-text-measurement.md` (`Font::measure_text` の存在、`glyph_run_for_text` の置換)、`0025-add-font-kerning.md` (GPOS Pair Adjustment による kern テーブル経路の置換)
- **着手前提**: 0022 / 0023 / 0024 / 0025 が close 済、0001 メタ issue の前提 concrete issue 群 (テスト用フォント選定、fuzzing 基盤、ベースライン benchmark 計測基盤、Compound Glyph point-matching 実装) が close 済
- **close 前提**: 上記に加え、0001 メタ issue 段階拡張表の `GlyphBuffer` レイアウト行が確定値で更新済み
- 非対応 (将来の課題): GSUB の Multiple / Alternate Substitution、Context / Chaining Context、複雑スクリプト (Arabic / Indic)、BiDi、マーク配置 (MarkToBase / MarkToMark)
