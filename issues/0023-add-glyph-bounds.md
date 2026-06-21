# グリフ境界ボックス取得機能を追加する

- Priority: High
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-glyph-bounds
- Polished: 2026-06-21

## 目的

個別グリフの境界ボックスを取得する `FontFace::glyph_bounds()` / `Font::glyph_bounds()` を追加し、グリフクリッピング判定や `TextMetrics::bounding_box` (0022 が 2026-06-20 に closed したため本 issue で追加可能になった) の算出を可能にする。glyf テーブルのグリフヘッダ 10 バイト (`numberOfContours` + `xMin` / `yMin` / `xMax` / `yMax`、各 i16) から bbox を直接読み出し、`append_glyph_outline` (Simple Glyph の点座標展開 / Compound の再帰) より定数時間で済む。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L452 `BLFont::get_glyph_bounds(const uint32_t* glyph_data, intptr_t glyph_advance, BLBoxI* boxes, size_t count)`**: raden は **個別取得**版 `FontFace::glyph_bounds(u16) -> Option<GlyphBounds>` / `Font::glyph_bounds(u16) -> Option<GlyphBounds>` で対応。Blend2D が一括取得 (`count` 個の glyph_id を `boxes[]` に書く) なのに対し、raden は個別取得に絞る (個別 API の方がシンプルで、呼び出し側がループで一括相当を構成できる。`measure_text` が文字列長 N で N 回呼び出すが `measure_text` は描画ホットパスではないため許容)。一括版は font モジュール安定化フェーズで別 issue として検討

差異の理由:

- Blend2D の `BLBoxI` は `(x0, y0, x1, y1)` の i32 4 つ、raden の `GlyphBounds` は `(x_min, y_min, x_max, y_max)` の f64 4 つ。raden は `FontFace` 版と `Font` 版で別々の構造体を定義せず同一の `GlyphBounds` を返す。Blend2D の整数版互換 (`BLBoxI` 互換) は font モジュール安定化前の別 issue で扱う
- Blend2D の `boxes[i].reset()` (0 で埋める) が「空グリフ」と「実際の bbox が (0, 0, 0, 0)」を区別しないのに対し、raden は `Option::None` で明示的に区別する
- 座標系: raden は glyf ヘッダを Y up・baseline 原点のまま保持する (設計方針セクション参照)。Blend2D 側の座標系取り扱いの詳細は `docs/BLEND2D.md` の対応表で別途確定する (本 issue では Blend2D 側の挙動は確定させない)

完了 PR で `docs/BLEND2D.md` L452 の raden 列に `FontFace::glyph_bounds(u16) -> Option<GlyphBounds>` / `Font::glyph_bounds(u16) -> Option<GlyphBounds>` を追記する。状態列 (`未実装`) の最終整理は 0001 メタ issue の close PR でまとめて行う。

0021 との型方針差異: 0021 はデザインユニットを `Option<i16>` で返す (`cap_height` 等、スカラ値)。本 issue は `GlyphBounds { f64 × 4 }` を `FontFace` / `Font` 両方で返す。0021 L282 が「0023 polish 時に `f64` ↔ `i16` の方針差異を再検討すべき」と指摘していたため、本段落で正面から回答する。理由: (1) 既存 `FontFace::ascent() -> i16` 等のスカラメトリクスは i16 で一貫するが、`GlyphBounds` は 4 値構造体であり `FontFace` 版 (`i16 × 4`) と `Font` 版 (`f64 × 4`) の 2 型に分けると `TextMetrics::bounding_box` (f64 系、0022 の `advance: f64` と一貫させる) への変換が `FontFace` 側で必要になり煩雑、(2) `FontFace::glyph_bounds` は i16 → f64 キャストのみで済むため `GlyphBounds { f64 × 4 }` 統一のコストは無視できる、(3) `BLBoxI` 互換 (i32) は font モジュール安定化前の別 issue で扱う前提。スカラ値 (0021) と構造体 (本 issue) で方針を分けるのは値の数と `Font` 側スケール済み値の表現の都合による

## 現状

- `Font::append_glyph_outline(glyph_id, offset_x, offset_y, &mut Path)` (`src/font/mod.rs:152-168`) は Path を構築するため、bbox だけ欲しい場合にオーバーヘッドが大きい (Simple Glyph では点座標展開、Compound では再帰呼び出し)
- glyf テーブルの各グリフヘッダには bbox (`xMin` / `yMin` / `xMax` / `yMax`、各 i16) が記録されているが、公開 API からアクセスできない
- `glyph::append_glyph_recursive` (`src/font/glyph.rs:82-127`) 内の `glyf_start` / `glyf_end` 計算・グリフデータスライス取得・ヘッダ長チェック (`glyph.rs:94-118`) は `glyph_bounds` も同じロジックを必要とするため共通ヘルパーへの抽出が望ましい。なお `glyph::append_glyph_outline` (`glyph.rs:68-79`) は `GlyphTransform::new` を作って `append_glyph_recursive` に委譲する薄い wrapper であり、loca/glyf ロジックは持たない
- `pbt/tests/prop_font/main.rs` は 0022 が新規作成済み (4 PBT: `single_char_advance` / `concatenation` / `size_linearity` / `non_negative`)。`pbt/Cargo.toml` の `[[test]] prop_font` エントリも 0022 が追加済み。本 issue は既存ファイルへの追記のみ

## 設計方針

### `GlyphBounds` 型

- font モジュール内に `GlyphBounds { x_min: f64, y_min: f64, x_max: f64, y_max: f64 }` を定義し、`api::context::Rect` への依存を避ける (`Rect` は `(x, y, w, h)` の xywh 形式で Blend2D `BLBoxI` / `BLBox` の `(x0, y0, x1, y1)` と異なる)
- `FontFace::glyph_bounds()` も `Font::glyph_bounds()` も同じ `GlyphBounds` 型を返す
- `#[non_exhaustive]` を付与 (0001 メタ issue L82 で確定した対象 4 型 `TextMetrics` / `GlyphBounds` / `GlyphBuffer` / `FontFeatureSettings` のうちの 1 つ。フィールド追加が非破壊だが、外部 construct と総当たりパターンマッチは禁止される。意図的)

### 座標系

- `FontFace::glyph_bounds(glyph_id) -> Option<GlyphBounds>`: glyf ヘッダの xMin / yMin / xMax / yMax を **font デザイン座標 (Y up、baseline 原点) のまま** 返す。i16 → f64 キャストするが意味は変えない
- `Font::glyph_bounds(glyph_id) -> Option<GlyphBounds>`: デザイン値に `scale()` を乗じた **Y up・baseline 原点 (offset 加算なし) のピクセル値** を返す。描画時の Y 反転 (`GlyphTransform::apply` の `-ty * scale + offset_y`) はここでは適用しない (問い合わせ API であり描画変換の責務を持たないため、呼び出し側が座標系を選択できる)

### 共通ヘルパー

`src/font/glyph.rs` 内に `pub(crate) fn glyph_entry_slice` を新規定義し、`append_glyph_recursive` 内 (`glyph.rs:94-118`、loca offset 計算 / glyf データ取得 / ヘッダ長チェック) と `glyph_bounds` の両方で利用する。`append_glyph_recursive` の冒頭 (depth チェック直後) の該当ロジックを本ヘルパー呼び出しに置き換える。

```rust
/// glyf テーブルから glyph_id のグリフデータスライスを取得する。
///
/// 戻り値:
/// - `Ok(Some(&[u8]))`: アウトライン入りグリフのスライス (10 バイト以上のヘッダを含む。
///   空グリフ以外はヘッダ長チェックまで本関数で行う)
/// - `Ok(None)`: 空グリフ (`glyf_start == glyf_end`)
/// - `Err(_)`: glyph_id 範囲外 / glyf 範囲外 / ヘッダ不足 (glyph_data.len() < 10)
pub(crate) fn glyph_entry_slice<'a>(
    glyph_id: u16,
    tables: &ParsedTables,
    data: &'a [u8],
) -> Result<Option<&'a [u8]>, FontError>;
```

10 バイト未満チェック (`glyph_data.len() < 10`) は `append_glyph_recursive` 側 (`glyph.rs:116-118`) から本ヘルパーに移動する。

### Compound Glyph bbox の `(0, 0, 0, 0)` 判定

glyf テーブルの Compound Glyph (`numberOfContours < 0`) は、フォント作成ツールが bbox を未計算のまま `(0, 0, 0, 0)` で埋めるケースがある (OpenType 仕様は bbox を座標の min/max として定義するが、実フォントでは未計算のまま埋める違反が観測される)。本判定は **i16 raw 値** で行い、`(0, 0, 0, 0)` の場合は `None` を返す (有効な bbox と区別できないため。子グリフを再帰評価して合成 bbox を計算する案は Blend2D の内部実装として知られているが、コスト増・複雑化のため本 issue では採用せず、対応グリフがあれば font 安定化前の別 issue で実装)。テスト用フォント選定 (0001 メタ issue tracked item) で `(0, 0, 0, 0)` 埋めの Compound Glyph を持つフォントが選定された段階で、本判定の実機検証を別 PR で追加する。

Simple Glyph (`numberOfContours >= 0`) はヘッダの bbox が点座標の min/max として常に正しく書かれるため `(0, 0, 0, 0)` 判定は適用しない (`numberOfContours == 0` の Simple Glyph の扱いはエッジケースセクション参照)。

## 完了条件

### 追加される API

- `GlyphBounds { x_min: f64, y_min: f64, x_max: f64, y_max: f64 }` (`#[non_exhaustive]`、`Debug` / `Clone` / `Copy` / `PartialEq` derive)
- `FontFace::glyph_bounds(glyph_id: u16) -> Option<GlyphBounds>` (デザインユニット、Y up)
- `Font::glyph_bounds(glyph_id: u16) -> Option<GlyphBounds>` (スケール済みピクセル単位、Y up)

### `TextMetrics::bounding_box` 追加責務 (本 issue の PR で実施)

0001 メタ issue L90 (段階拡張表「`bounding_box` の追加責務は 0023」) / 0022「将来の拡張」(L245) で確定した責務。0022 が 2026-06-20 に closed したため本 issue の PR で実施可能になった。`TextMetrics` に `bounding_box: Option<GlyphBounds>` を追加し、`Font::measure_text` 内で文字列全体の bbox を計算する。

- `src/font/mod.rs` の `TextMetrics` に `pub bounding_box: Option<GlyphBounds>` を追加 (`#[non_exhaustive]` のためフィールド構造変更は非破壊)
- `Font::measure_text` 内で `glyph_run_for_text` ループ中に各 `glyph_bounds(glyph_id)` を呼び、`cursor_x` (advance 累積) で水平オフセットして累積 union を計算
- 空文字列 / 空グリフのみの場合は `None`
- 単体テストと PBT を 0022 既存テストに追加 (空文字列で `None`、単一文字で `font.glyph_bounds(font.map_char_to_glyph(c))` を `cursor_x = 0` でオフセットした値と一致 等)
- `TextMetrics` の doc comment (`src/font/mod.rs:217-222`) の「現状は `advance` のみを保持する」を更新し、`bounding_box` 追加と `GlyphBounds` の f64 の `NaN != NaN` 挙動 (PartialEq 比較の注意) を明記する

### 既存 API の挙動維持

- `Font::append_glyph_outline` のシグネチャ・出力は変わらない (内部実装で `glyph_entry_slice` ヘルパー経由に差し替えるのみ)

### テスト

- 単体テストには「Arial 環境での既知特性の存在確認」のみ残す (PBT で実現可能な不変条件は単体テストに書かない、`shiguredo-rust`「PBT 優先」原則)
- PBT で `glyph_bounds` の不変条件 (`x_min <= x_max`、`y_min <= y_max`)、スケール線形性、`Path::control_box()` 包含を検証

### ドキュメント

- `docs/BLEND2D.md` L452 の raden 列に `FontFace::glyph_bounds(u16) -> Option<GlyphBounds>` / `Font::glyph_bounds(u16) -> Option<GlyphBounds>` を追記
- `CHANGES.md` の `## develop` の `### misc` 直下に `ADD` 種別で 4 件追記 (`GlyphBounds` / `FontFace::glyph_bounds` / `Font::glyph_bounds` / `TextMetrics::bounding_box` フィールド追加)

## 解決方法

実装順 (依存順): step 1 (`GlyphBounds`) → step 2 (共通ヘルパー) → step 3 (`FontFace::glyph_bounds`) → step 4 (`Font::glyph_bounds`) → step 5 (`lib.rs` re-export) → step 6 (`TextMetrics::bounding_box`) → step 7 (CHANGES.md) → step 8 (BLEND2D.md)。

### 1. `GlyphBounds` 構造体を追加

`src/font/mod.rs` に定義:

```rust
/// グリフの境界ボックス。
///
/// `FontFace::glyph_bounds` からは font デザイン座標 (Y up、baseline 原点)、
/// `Font::glyph_bounds` からはスケール済みピクセル単位 (Y up、baseline 原点) が入る。
/// 描画時の Y 反転は適用されない (問い合わせ API であり描画変換の責務を持たないため、呼び出し側で行う)。
/// `PartialEq` は 4 × f64 の比較であり、IEEE 754 上 `NaN != NaN` のため
/// NaN を含む値では `PartialEq` 比較が false を返す (0022 の `TextMetrics` と同じ注意)。
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct GlyphBounds {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}
```

### 2. 共通ヘルパー群を追加

`src/font/glyph.rs` 内に 2 つの `pub(crate)` ヘルパーを新規定義する。

`glyph_entry_slice` は設計方針セクションのシグネチャの通り。`append_glyph_recursive` (`glyph.rs:82-127`) 内の `glyph_id` 範囲チェック・loca offset 計算・glyf データ取得・ヘッダ長チェック (`glyph.rs:94-118`) を抽出し、`append_glyph_recursive` の冒頭 (depth チェック直後) を本ヘルパー呼び出しに置き換える (空グリフ時の早期 return も維持)。既存のエラーメッセージ ("glyph id out of range", "glyph data out of range", "glyph header too short") はそのまま引き継ぐ。`glyph::append_glyph_outline` (`glyph.rs:68-79`) は薄い wrapper のため変更しない。

`glyph_bbox_raw` は 10 バイトヘッダから bbox を読み出し、Compound Glyph の `(0, 0, 0, 0)` 判定を行う。`FontFace::glyph_bounds` と `Font::glyph_bounds` のヘッダパース重複を排除するために導入する。既存の `be_i16` helper (`tables.rs`、`glyph.rs:7` で import 済み) を使う。

```rust
/// グリフヘッダ 10 バイトから bbox を読み出す。
///
/// 戻り値: `(x_min, y_min, x_max, y_max)` の i16 4 値。
/// Compound Glyph (`number_of_contours < 0`) で bbox が `(0, 0, 0, 0)` のときは `None`
/// (有効な bbox と区別できないため)。Simple Glyph では `(0, 0, 0, 0)` も有効。
/// `glyph_data` は 10 バイト以上であること (`glyph_entry_slice` が保証)。
pub(crate) fn glyph_bbox_raw(glyph_data: &[u8]) -> Option<(i16, i16, i16, i16)> {
    let number_of_contours = be_i16(glyph_data, 0).ok()?;
    let x_min = be_i16(glyph_data, 2).ok()?;
    let y_min = be_i16(glyph_data, 4).ok()?;
    let x_max = be_i16(glyph_data, 6).ok()?;
    let y_max = be_i16(glyph_data, 8).ok()?;
    if number_of_contours < 0 && x_min == 0 && y_min == 0 && x_max == 0 && y_max == 0 {
        return None;
    }
    Some((x_min, y_min, x_max, y_max))
}
```

`glyph_bounds` 側では `glyph_entry_slice` のエラー時に `None` を返す (寛容方針)。

### 3. `FontFace::glyph_bounds()` を追加

```rust
impl FontFace {
    /// グリフ境界ボックス (デザインユニット、Y up、baseline 原点)。
    /// 空グリフ・glyph_id 範囲外・Compound bbox (0,0,0,0) は None。
    pub fn glyph_bounds(&self, glyph_id: u16) -> Option<GlyphBounds> {
        let glyph_data = glyph::glyph_entry_slice(glyph_id, &self.tables, &self.data).ok()??;
        let (x_min, y_min, x_max, y_max) = glyph::glyph_bbox_raw(glyph_data)?;
        Some(GlyphBounds {
            x_min: x_min as f64,
            y_min: y_min as f64,
            x_max: x_max as f64,
            y_max: y_max as f64,
        })
    }
}
```

エラー時 (`glyph_id` 範囲外、glyf 範囲外、ヘッダ不足) は `None` を返す (寛容方針、エッジケースセクション参照)。

### 4. `Font::glyph_bounds()` を追加

```rust
impl Font {
    /// グリフ境界ボックス (スケール済みピクセル単位、Y up、baseline 原点)。
    /// 描画時の Y 反転は適用されない。
    /// 空グリフ・glyph_id 範囲外・Compound bbox (0,0,0,0) は None (FontFace::glyph_bounds と同一条件)。
    pub fn glyph_bounds(&self, glyph_id: u16) -> Option<GlyphBounds> {
        // FontFaceInner 経由でテーブルとデータにアクセス (Arc deref)。
        let glyph_data = glyph::glyph_entry_slice(
            glyph_id, &self.face.tables, &self.face.data,
        ).ok()??;
        let (x_min, y_min, x_max, y_max) = glyph::glyph_bbox_raw(glyph_data)?;
        Some(GlyphBounds {
            x_min: x_min as f64 * self.scale,
            y_min: y_min as f64 * self.scale,
            x_max: x_max as f64 * self.scale,
            y_max: y_max as f64 * self.scale,
        })
    }
}
```

`Font.face` は `Arc<FontFaceInner>` のため `FontFace::glyph_bounds()` を直接呼べない (`FontFace` は別構造体、`FontFaceInner` は `Font` 専用の private 構造体)。ヘッダパースと Compound 判定は `glyph_bbox_raw` で共通化し、スケール乗算だけ差し替える。

### 5. `lib.rs` re-export

`src/lib.rs` の `pub use font::{Font, FontData, FontError, FontFace, TextMetrics};` を alphabetical 順で:

```rust
pub use font::{Font, FontData, FontError, FontFace, GlyphBounds, TextMetrics};
```

### 6. `TextMetrics::bounding_box` 追加 (本 issue の PR で実施)

`TextMetrics` 定義に `pub bounding_box: Option<GlyphBounds>` を追加し、`TextMetrics` 構造体の doc comment (`src/font/mod.rs:217-222`) の「現状は `advance` のみを保持する」を更新する。あわせて `Font::measure_text` メソッドの doc comment (`src/font/mod.rs:200-208`) も更新し、`bounding_box` フィールドの説明 (空文字列で `None`、`size == 0` で全座標 0.0、Compound (0,0,0,0) で `None`) と NaN / Inf 伝播の注意を追記する。`Font::measure_text` を以下に変更:

```rust
pub fn measure_text(&self, text: &str) -> TextMetrics {
    let mut run: Vec<(u16, f64)> = Vec::new();
    self.glyph_run_for_text(text, &mut run);

    let mut cursor_x = 0.0;
    let mut bbox: Option<GlyphBounds> = None;
    for &(glyph_id, advance) in run.iter() {
        if let Some(gb) = self.glyph_bounds(glyph_id) {
            let translated = GlyphBounds {
                x_min: gb.x_min + cursor_x,
                y_min: gb.y_min,
                x_max: gb.x_max + cursor_x,
                y_max: gb.y_max,
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
        cursor_x += advance;
    }
    // `cursor_x` はループ終了時に advance 累積値と一致する。
    // `#[non_exhaustive]` は同一クレート内では構造体リテラル構築を許可する。
    TextMetrics { advance: cursor_x, bounding_box: bbox }
}
```

0022 既存 PBT (`measure_text` の `advance` に対する諸検証) は変更不要だが、`bounding_box` 不変条件 (空文字列で `None`、単一文字で `glyph_bounds` の値と一致等) を追加する。

### 7. CHANGES.md 更新

`CHANGES.md` の `## develop` の `### misc` 直下に追加 (`shiguredo-changelog` スキル準拠。`@<author>` は実装者の GitHub ユーザー名で置換):

```
- [ADD] `GlyphBounds` 構造体を追加する
  - @<author>
- [ADD] `FontFace::glyph_bounds` を追加する
  - @<author>
- [ADD] `Font::glyph_bounds` を追加する
  - @<author>
- [ADD] `TextMetrics` に `bounding_box` フィールドを追加する
  - @<author>
```

### 8. BLEND2D.md 更新

`docs/BLEND2D.md` L452 の raden 列 (現在 `なし`) を `FontFace::glyph_bounds(u16) -> Option<GlyphBounds>` / `Font::glyph_bounds(u16) -> Option<GlyphBounds>` に書き換える。状態列 (`未実装`) は本 issue では変更せず、0001 メタ issue の close PR でまとめて整理する。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/mod.rs` | 既存編集 | `GlyphBounds` 構造体定義、`FontFace::glyph_bounds()`、`Font::glyph_bounds()`、`TextMetrics::bounding_box` フィールド追加、`TextMetrics` doc comment 更新、`Font::measure_text` 内 bbox 計算 |
| `src/font/glyph.rs` | 既存編集 | `glyph_entry_slice` / `glyph_bbox_raw` ヘルパー追加、`append_glyph_recursive` 内の重複ロジックを置換 |
| `src/lib.rs` | 既存編集 | `pub use font::{... GlyphBounds, ...}` を alphabetical 順で追加 |
| `tests/test_font.rs` | 既存編集 | 単体テスト追加 (Arial 環境で `face.glyph_bounds(font.map_char_to_glyph('A')).is_some()` 等。`map_char_to_glyph` は `Font` のメソッドのため `Font` を経由して glyph_id を取得し `FontFace::glyph_bounds` に渡す) |
| `pbt/tests/prop_font/main.rs` | 既存編集 | 0022 が作成済み。本 issue は `glyph_bounds` 系 PBT を既存ファイルに追記 |
| `docs/BLEND2D.md` | 既存編集 | L452 の raden 列を `FontFace::glyph_bounds(u16) -> Option<GlyphBounds>` / `Font::glyph_bounds(u16) -> Option<GlyphBounds>` に書き換え |
| `CHANGES.md` | 既存編集 | `### misc` 直下に `ADD` 4 件追加 |

## エッジケース

- `glyph_id` が `num_glyphs` 以上 / glyf データ範囲外 / ヘッダ不足 (`glyph_data.len() < 10`): いずれも `None` (寛容方針。`append_glyph_outline` の `InvalidData` 返却と挙動が異なるが、bbox 問い合わせは欠落グリフでも `None` を返すことで利用者側のエラーハンドリング負荷を軽減する意図的差異)
- 空グリフ (`glyf_start == glyf_end`): `None` (アウトラインが存在しないグリフに bbox を返さない方針)
- Compound Glyph で bbox が `(0, 0, 0, 0)`: `None` (Simple Glyph では `(0, 0, 0, 0)` を有効値として扱う)
- `numberOfContours == 0` の Simple Glyph (空アウトライン・loca エントリあり): bbox が `(0, 0, 0, 0)` になりうるが `Some(GlyphBounds { 0.0, 0.0, 0.0, 0.0 })` を返す (空グリフとは異なり loca エントリが存在するため `None` にはしない)
- `size == 0` の `Font::glyph_bounds`: 有効な Simple Glyph で全フィールドが 0.0 の `Some` を返す (Compound (0,0,0,0) 判定は i16 raw 値で行うため scale = 0 でも判定は変わらない)。`size == 0` と `numberOfContours == 0` の両方が重なると `Some(0,0,0,0)` になるが、これは面積 0 の無意味な bbox であり実用上 `None` と同等だが、`Some` と `None` は `Option` として区別可能なため呼び出し側で必要に応じて扱いを判断できる。テストではロックしない (0021 / 0022 と方針共有)
- `glyph_id == 0` (cmap 未マッピング・`.notdef`): `FontFace::glyph_bounds` / `Font::glyph_bounds` は `.notdef` の bbox を返す (loca エントリが存在すれば)。`Font::measure_text` の `bounding_box` 計算では `glyph_id == 0` の bbox も union に含める (0022 の `advance` 加算と同じ方針、メトリクス上の bbox として扱う)。`Context::fill_text` は `glyph_id == 0` のアウトライン描画をスキップするため、`bounding_box` は描画領域と完全には一致しない点に注意
- `size < 0` / NaN / Inf: テストでロックしない (0021 / 0022 と方針共有)

## 隣接 issue への影響

- **0026 への影響**: 本 issue で追加する単体テスト・PBT が `Font::from_face` シグネチャ拡張 (0001 メタ issue 段階拡張表) で書き換え対象になる。書き換え責務は 0026 のスコープに含む

## テスト戦略

### 単体テスト (`tests/test_font.rs`)

`shiguredo-rust`「PBT 優先」を踏まえ、PBT で実現可能な不変条件は単体テストに書かない。単体テストに残すのは Arial 環境での既知特性の存在確認のみ:

- `font_glyph_bounds_some`: Arial.ttf で `Font::from_face(&face, 48.0)` を構築し `face.glyph_bounds(font.map_char_to_glyph('A')).is_some()` を検証 (`map_char_to_glyph` は `Font` のメソッドのため `Font` を経由して glyph_id を取得する)
- `font_glyph_bounds_invalid_id`: `face.glyph_bounds(u16::MAX).is_none()` を検証 (寛容方針確認)

`load_arial()` ヘルパーを再利用、Arial 不在時はスキップ。

### PBT (`pbt/tests/prop_font/main.rs`)

`Font` を直接使い `Context` は経由しない。Arial 不在時は `proptest!` 外で早期 `return` (既存の `load_arial()` / `close_enough` ヘルパーを再利用)。`prop_assume!` は避ける (proptest が低品質扱いするため)。

文字列戦略: Arial の cmap に確実に含まれる ASCII printable + 半角スペース (既存 PBT と同じ `proptest::char::range(' ', '~')` で単一 ASCII 文字、`pbt/tests/prop_font/main.rs:38` 参照)。スペースは空グリフで `glyph_bounds` が `None` を返しうるが、各不変条件は `if let Some(gb) = ...` で `Some` 側のみ検証するため `None` ではスキップされる (fail にならない)。検証する不変条件:

- **`glyph_bounds` 不変条件**: `font.glyph_bounds(gid)` が `Some(gb)` のとき `gb.x_min <= gb.x_max` かつ `gb.y_min <= gb.y_max`。`gid` は `font.map_char_to_glyph(ch)` (`ch in proptest::char::range(' ', '~')`) で生成
- **スケール線形性**: `size in 0.001f64..1000.0` で `FontFace::glyph_bounds(gid)` が `Some(face_gb)` のとき `Font::glyph_bounds(gid)` も `Some` で各座標 = `face_gb` の各座標 × `Font::scale()` (相対許容範囲 `(lhs - rhs).abs() <= (lhs.abs() + rhs.abs()) * 1e-9 + 1e-12`)。`FontFace` 側が `None` のとき `Font` 側も `None` となることも検証する
- **`TextMetrics::bounding_box` 不変条件**: (a) 空文字列で `measure_text("").bounding_box == None`、(b) 単一文字 `c` で `measure_text(c.to_string()).bounding_box == font.glyph_bounds(font.map_char_to_glyph(c))` (`cursor_x == 0.0` のためオフセットなしで一致)、(c) 2 文字以上の文字列 `text` で `measure_text(text).bounding_box` が `Some(bbox)` のとき、文字列の各グリフ `gid_i` の `font.glyph_bounds(gid_i)` が `Some(gb_i)` なら `cursor_x_i` オフセット済みの `gb_i` が `bbox` 内に含まれることを検証する (union 累積ロジックの検証)
- **`Path::control_box()` 包含**: `font.glyph_bounds(gid)` が `Some` のとき、`font.append_glyph_outline(gid, 0.0, 0.0, &mut path)` が `Ok(())` を返した場合のみ検証する (`Err` の場合はその文字をスキップ、Arial では発生しない前提だが型上は `Result` のためガードする)。`path.control_box()` (`Option<Rect>`、`Rect` は `x` / `y` / `w` / `h` フィールド、画面座標 Y down) が `None` の場合 (例: `numberOfContours == 0` で点が追加されない) はスキップする。`Some(rect)` の場合は以下の手順で `font.glyph_bounds(gid)` (Y up ピクセル) と比較し、`control_box ⊆ glyph_bounds` を検証する。(1) `Rect` から `(x0, y0, x1, y1)` へ変換 (`x0 = rect.x`, `y0 = rect.y`, `x1 = rect.x + rect.w`, `y1 = rect.y + rect.h`)、(2) Y 反転 (`y_min_up = -y1`, `y_max_up = -y0`)、(3) `font.glyph_bounds(gid)` の `(x_min, y_min, x_max, y_max)` と比較し、許容範囲 `eps = (x_min.abs() + x_max.abs() + y_min.abs() + y_max.abs()) * 1e-9 + 1e-12` で `x_min - eps <= x0 && x_max + eps >= x1 && y_min - eps <= y_min_up && y_max + eps >= y_max_up` を検証 (Simple Glyph では exact 一致するが、Compound Glyph で f2dot14 行列の浮動小数点丸めが生じるため許容範囲を設ける)。TrueType glyf ヘッダの bbox は制御点を含む全点座標の min/max として書かれるため、`control_box` が `glyph_bounds` 内に収まる

PBT で `Path` を使用するため `use raden::Path;` の import 追記が必要なことに注意する (`pbt/tests/prop_font/main.rs:7` の既存 imports に追加)。

### 既存テストへの影響

- `append_glyph_recursive` の内部実装は `glyph_entry_slice` 経由に差し替えるが、`Font::append_glyph_outline` のシグネチャ・出力は変わらない。既存 `font_glyph_outline_to_path` / `font_space_glyph_has_no_outline` / `font_fill_text_integration` は合格を維持

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L452、`#[non_exhaustive]` 方針 (L82)、`TextMetrics::bounding_box` 追加責務の根拠 (L90))
- 並行可能: `0021-add-font-scaled-metrics.md` (0001 メタ issue L178-180 の並行着手可能ブロック。`pbt/tests/prop_font/main.rs` は 0022 が作成済みのため両 issue は既存ファイルへ追記する)
- 依存される: `0026-add-opentype-basic-shaping.md` (`Font::from_face` シグネチャ拡張で本 issue のテストが書き換え対象。書き換え責務は 0026 のスコープ)
- **close 前提**: 0001 メタ issue の前提 concrete issue 群のうち「テスト用フォント選定」「Compound Glyph point-matching 実装」が完了し、Compound Glyph の `(0, 0, 0, 0)` 判定の妥当性が実機で検証できること。ただし 0022 がこれらの前提なしに closed になった先例 (Arial 依存テストのスキップで CI を通す) に倣い、本 issue も Arial 依存テストで着手可能 (前提未完了分は close 前に別 PR で検証を追加する)
