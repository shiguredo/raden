# グリフ境界ボックス取得機能を追加する

- Priority: High
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-glyph-bounds
- Polished: 2026-06-20

## 目的

個別グリフの境界ボックスを取得する `FontFace::glyph_bounds()` / `Font::glyph_bounds()` を追加し、グリフクリッピング判定や `TextMetrics::bounding_box` (0022 完了直後の PR で 0023 のスコープとして追加) の算出を可能にする。glyf テーブルのグリフヘッダ 10 バイト (xMin / yMin / xMax / yMax) から bbox を直接読み出し、`append_glyph_outline` (Simple Glyph の点座標展開 / Compound の再帰) より定数時間で済む。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L452 `BLFont::get_glyph_bounds(const uint32_t* glyph_data, intptr_t glyph_advance, BLBoxI* boxes, size_t count)`**: raden は **個別取得**版 `FontFace::glyph_bounds(u16) -> Option<GlyphBounds>` / `Font::glyph_bounds(u16) -> Option<GlyphBounds>` で対応。Blend2D が一括取得 (`count` 個の glyph_id を `boxes[]` に書く) なのに対し、raden は個別取得に絞る (主用途がクリッピング判定であり `Vec<GlyphBounds>` 返却の無駄を避ける)。一括版は font モジュール安定化フェーズで別 issue として検討

差異の理由:

- Blend2D の `BLBoxI` は `(x0, y0, x1, y1)` の i32 4 つ、raden の `GlyphBounds` は `(x_min, y_min, x_max, y_max)` の f64 4 つ。raden は `FontFace` / `Font` で同一の構造体を使い分けず、Blend2D の整数版互換 (`BLBoxI` 互換) は font モジュール安定化前の別 issue で扱う
- Blend2D の `boxes[i].reset()` (0 で埋める) が「空グリフ」と「実際の bbox が (0, 0, 0, 0)」を区別しないのに対し、raden は `Option::None` で明示的に区別する
- 座標系: Blend2D の `get_glyph_bounds` は font 座標 (Y up、baseline 原点) を画面座標 (Y down) に反転して返す。raden は glyf ヘッダのデザインユニットの符号と原点をそのまま保持 (Y up、baseline 原点) する。画面座標への反転は呼び出し側 (描画パイプライン `GlyphTransform::apply` で `-ty * scale + offset_y`) で行う

完了 PR で `docs/BLEND2D.md` L452 の raden 列に `glyph_bounds()` を追記する。状態列 (`未実装`) の最終整理は 0001 メタ issue の close PR でまとめて行う。

0021 との型方針差異: 0021 はデザインユニットを `Option<i16>` で返す (`cap_height` 等)。本 issue は `GlyphBounds { f64 × 4 }` を `FontFace` / `Font` 両方で返す。理由: (1) `GlyphBounds` は 4 値構造体のため `FontFace` 版と `Font` 版の 2 型を持つコストが大きい、(2) 0022 の `TextMetrics::bounding_box` も f64 系で一貫させるため、(3) `BLBoxI` 互換 (i32) は font モジュール安定化前の別 issue で扱う前提。

## 現状

- `Font::append_glyph_outline(glyph_id, offset_x, offset_y, &mut Path)` (`src/font/mod.rs:152-168`) は Path を構築するため、bbox だけ欲しい場合にオーバーヘッドが大きい (Simple Glyph では点座標展開、Compound では再帰呼び出し)
- glyf テーブルの各グリフヘッダには bbox (`xMin` / `yMin` / `xMax` / `yMax`、各 i16) が記録されているが、公開 API からアクセスできない
- `glyph::append_glyph_outline` 内の `glyf_start` / `glyf_end` 計算とグリフデータスライス取得は `src/font/glyph.rs:94-114` 周辺で行われており、`glyph_bounds` も同じロジックを必要とするため共通ヘルパーへの抽出が望ましい
- `pbt/tests/prop_font/main.rs` は未作成 (0001 メタ issue で 0021 のサブタスクと確定。並行ブロックで先着が作成)

## 設計方針

### `GlyphBounds` 型

- font モジュール内に `GlyphBounds { x_min: f64, y_min: f64, x_max: f64, y_max: f64 }` を定義し、`api::context::Rect` への依存を避ける (`Rect` は `(x, y, width, height)` の xywh 形式で Blend2D `BLBox` の `(x0, y0, x1, y1)` と異なる)
- `FontFace::glyph_bounds()` も `Font::glyph_bounds()` も同じ `GlyphBounds` 型を返す
- `#[non_exhaustive]` を付与 (0001 メタ issue L75 で確定した対象 4 型のうちの 1 つ。フィールド追加が非破壊だが、外部 construct と総当たりパターンマッチは禁止される。意図的)

### 座標系

- `FontFace::glyph_bounds(glyph_id) -> Option<GlyphBounds>`: glyf ヘッダの xMin / yMin / xMax / yMax を **font デザイン座標 (Y up、baseline 原点) のまま** 返す。i16 → f64 キャストするが意味は変えない
- `Font::glyph_bounds(glyph_id) -> Option<GlyphBounds>`: デザイン値に `scale()` を乗じた **Y up・baseline 原点 (offset 加算なし) のピクセル値** を返す。描画時の Y 反転 (`GlyphTransform::apply` の `-ty * scale + offset_y`) はここでは適用しない

### 共通ヘルパー

`src/font/glyph.rs` 内に `pub(crate) fn glyph_entry_slice` を新規定義し、`append_glyph_outline` 内 (`glyph.rs:94-114` 周辺) と `glyph_bounds` の両方で利用する。

```rust
/// glyf テーブルから glyph_id のグリフデータスライスを取得する。
///
/// 戻り値:
/// - `Ok(Some(&[u8]))`: アウトライン入りグリフのスライス (10 バイト以上のヘッダ含む)
/// - `Ok(None)`: 空グリフ (`glyf_start == glyf_end`)
/// - `Err(_)`: glyph_id 範囲外 / glyf 範囲外 / header 不足
pub(crate) fn glyph_entry_slice<'a>(
    glyph_id: u16,
    tables: &ParsedTables,
    data: &'a [u8],
) -> Result<Option<&'a [u8]>, FontError>;
```

`glyph_bounds` 側ではエラー時に `None` を返す (寛容方針)。

### Compound Glyph bbox の `(0, 0, 0, 0)` 判定

glyf テーブルの Compound Glyph (`numberOfContours < 0`) はフォントによっては bbox を `(0, 0, 0, 0)` で埋めて省略するケースがある。本判定は **i16 raw 値** で行い、`(0, 0, 0, 0)` の場合は `None` を返す (有効な bbox と区別できないため。子グリフを再帰評価して合成 bbox を計算する案は Blend2D の挙動だが、コスト増・複雑化のため本 issue では採用せず、対応グリフがあれば font 安定化前の別 issue で実装)。

Simple Glyph (`numberOfContours >= 0`) はヘッダの bbox が点座標の min/max として常に正しく書かれるため `(0, 0, 0, 0)` 判定は適用しない。

## 完了条件

### 追加される API

- `GlyphBounds { x_min: f64, y_min: f64, x_max: f64, y_max: f64 }` (`#[non_exhaustive]`、`Debug` / `Clone` / `Copy` / `PartialEq` derive)
- `FontFace::glyph_bounds(glyph_id: u16) -> Option<GlyphBounds>` (デザインユニット、Y up)
- `Font::glyph_bounds(glyph_id: u16) -> Option<GlyphBounds>` (スケール済みピクセル単位、Y up)

### 0022 への追加責務 (0023 完了直後の PR で 0023 のスコープに含む)

0001 メタ issue L83 / 0022「将来の拡張」で確定: `TextMetrics` に `bounding_box: Option<GlyphBounds>` を追加し、`Font::measure_text` 内で文字列全体の bbox を計算する。

- `src/font/mod.rs` の `TextMetrics` に `pub bounding_box: Option<GlyphBounds>` を追加 (`#[non_exhaustive]` のためフィールド構造変更は非破壊)
- `Font::measure_text` 内で `glyph_run_for_text` ループ中に各 `glyph_bounds(glyph_id)` を呼び、`cursor_x` (advance 累積) で水平オフセットして累積 union を計算
- 空文字列 / 空グリフのみの場合は `None`
- 単体テストと PBT を 0022 既存テストに追加 (空文字列で `None`、単一文字で `glyph_bounds(map_char_to_glyph(c))` を `cursor_x = 0` でオフセットした値と一致 等)

### 既存 API の挙動維持

- `Font::append_glyph_outline` のシグネチャ・出力は変わらない (内部実装で `glyph_entry_slice` ヘルパー経由に差し替えるのみ)

### テスト

- 単体テストには「Arial 環境での既知特性の存在確認」のみ残す (PBT で実現可能な不変条件は単体テストに書かない、`shiguredo-rust`「PBT 優先」原則)
- PBT で `glyph_bounds` の不変条件 (`x_min <= x_max`、`y_min <= y_max`)、スケール線形性、`Path::control_box()` 包含を検証

### ドキュメント

- `docs/BLEND2D.md` L452 の raden 列に新規 API 名を追記
- `CHANGES.md` の `## develop` の `### misc` 直下に `ADD` 種別で 4 件追記 (`GlyphBounds` / `FontFace::glyph_bounds` / `Font::glyph_bounds` / `TextMetrics::bounding_box` フィールド追加)

## 解決方法

実装順 (依存順): step 1 (`GlyphBounds`) → step 2 (共通ヘルパー) → step 3 (`FontFace::glyph_bounds`) → step 4 (`Font::glyph_bounds`) → step 5 (`lib.rs` re-export) → step 6 (`TextMetrics::bounding_box`) → step 7 (CHANGES.md) → step 8 (BLEND2D.md)。

### 1. `GlyphBounds` 構造体を追加

`src/font/mod.rs` に定義:

```rust
/// グリフの境界ボックス。座標系は font デザイン座標 (Y up、baseline 原点)。
/// 描画時の Y 反転は適用されない。
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct GlyphBounds {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}
```

### 2. 共通ヘルパー `glyph_entry_slice` を追加

`src/font/glyph.rs` 内で、現状の `append_glyph_outline` の `glyph_id` 範囲チェック・loca offset 計算・glyf データ取得部分 (`glyph.rs:94-114` 付近) を抽出。`append_glyph_outline` の冒頭を本ヘルパーに置き換え、空グリフ時の早期 return も維持する。

### 3. `FontFace::glyph_bounds()` を追加

```rust
impl FontFace {
    /// グリフ境界ボックス (デザインユニット、Y up、baseline 原点)。
    /// 空グリフ・glyph_id 範囲外・Compound bbox (0,0,0,0) は None。
    pub fn glyph_bounds(&self, glyph_id: u16) -> Option<GlyphBounds> {
        let glyph_data = glyph::glyph_entry_slice(glyph_id, &self.tables, &self.data).ok()??;
        // ヘッダ 10 バイト (Big Endian):
        //   number_of_contours: i16 (offset 0..2)
        //   xMin: i16 (offset 2..4), yMin: i16 (offset 4..6)
        //   xMax: i16 (offset 6..8), yMax: i16 (offset 8..10)
        let number_of_contours = i16::from_be_bytes([glyph_data[0], glyph_data[1]]);
        let x_min = i16::from_be_bytes([glyph_data[2], glyph_data[3]]);
        let y_min = i16::from_be_bytes([glyph_data[4], glyph_data[5]]);
        let x_max = i16::from_be_bytes([glyph_data[6], glyph_data[7]]);
        let y_max = i16::from_be_bytes([glyph_data[8], glyph_data[9]]);
        // Compound (number_of_contours < 0) で bbox が (0,0,0,0) なら無効扱い。
        // Simple Glyph では (0,0,0,0) も有効として扱う (上記設計方針参照)。
        if number_of_contours < 0 && x_min == 0 && y_min == 0 && x_max == 0 && y_max == 0 {
            return None;
        }
        Some(GlyphBounds {
            x_min: x_min as f64,
            y_min: y_min as f64,
            x_max: x_max as f64,
            y_max: y_max as f64,
        })
    }
}
```

エラー時 (`glyph_id` 範囲外、glyf 範囲外、ヘッダ不足) は `None` を返す (寛容方針)。`append_glyph_outline` の `InvalidData` 返却と挙動が異なるが、bbox 問い合わせは欠落グリフでも可能とする意図的な差異。

### 4. `Font::glyph_bounds()` を追加

```rust
impl Font {
    /// グリフ境界ボックス (スケール済みピクセル単位、Y up、baseline 原点)。
    /// 描画時の Y 反転は適用されない。
    pub fn glyph_bounds(&self, glyph_id: u16) -> Option<GlyphBounds> {
        // FontFaceInner 経由でテーブルとデータにアクセス (Arc deref)。
        let glyph_data = glyph::glyph_entry_slice(
            glyph_id, &self.face.tables, &self.face.data,
        ).ok()??;
        let number_of_contours = i16::from_be_bytes([glyph_data[0], glyph_data[1]]);
        let x_min = i16::from_be_bytes([glyph_data[2], glyph_data[3]]);
        let y_min = i16::from_be_bytes([glyph_data[4], glyph_data[5]]);
        let x_max = i16::from_be_bytes([glyph_data[6], glyph_data[7]]);
        let y_max = i16::from_be_bytes([glyph_data[8], glyph_data[9]]);
        if number_of_contours < 0 && x_min == 0 && y_min == 0 && x_max == 0 && y_max == 0 {
            return None;
        }
        Some(GlyphBounds {
            x_min: x_min as f64 * self.scale,
            y_min: y_min as f64 * self.scale,
            x_max: x_max as f64 * self.scale,
            y_max: y_max as f64 * self.scale,
        })
    }
}
```

`Font.face` は `Arc<FontFaceInner>` のため `FontFace::glyph_bounds()` を直接呼べない (`FontFace` は別構造体)。同じロジックを書き、スケール乗算だけ差し替える。

### 5. `lib.rs` re-export

`src/lib.rs` の `pub use font::{Font, FontData, FontError, FontFace, TextMetrics};` を alphabetical 順で:

```rust
pub use font::{Font, FontData, FontError, FontFace, GlyphBounds, TextMetrics};
```

### 6. `TextMetrics::bounding_box` 追加 (0023 完了直後の PR で 0023 のスコープ)

`TextMetrics` 定義に `pub bounding_box: Option<GlyphBounds>` を追加。`Font::measure_text` を以下に変更:

```rust
pub fn measure_text(&self, text: &str) -> TextMetrics {
    let mut run: Vec<(u16, f64)> = Vec::new();
    self.glyph_run_for_text(text, &mut run);

    let mut cursor_x = 0.0;
    let mut bbox: Option<GlyphBounds> = None;
    let mut advance_total = 0.0;
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
        advance_total += advance;
    }
    TextMetrics { advance: advance_total, bounding_box: bbox }
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

`docs/BLEND2D.md` L452 の raden 列に `glyph_bounds()` を追記する。状態列 (`未実装`) の最終整理は 0001 メタ issue の close PR でまとめて行う。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/mod.rs` | 既存編集 | `GlyphBounds` 構造体定義、`FontFace::glyph_bounds()`、`Font::glyph_bounds()`、`TextMetrics::bounding_box` フィールド追加、`Font::measure_text` 内 bbox 計算 |
| `src/font/glyph.rs` | 既存編集 | `glyph_entry_slice` ヘルパー追加、`append_glyph_outline` 内の重複ロジックを置換 |
| `src/lib.rs` | 既存編集 | `pub use font::{... GlyphBounds, ...}` を alphabetical 順で追加 |
| `tests/test_font.rs` | 既存編集 | 単体テスト追加 (Arial 環境で `glyph_bounds(map_char_to_glyph('A')).is_some()` 等) |
| `pbt/tests/prop_font/main.rs` | 新規 or 既存編集 | 並行ブロック内で先着が新規作成。0021 が原則担当だが、0023 が先着の場合は本 issue で初回作成 |
| `pbt/Cargo.toml` | 既存編集 (先着の場合のみ) | `[[test]] name = "prop_font" path = "tests/prop_font/main.rs"` |
| `docs/BLEND2D.md` | 既存編集 | L452 の raden 列に新規 API 追記 |
| `CHANGES.md` | 既存編集 | `### misc` 直下に `ADD` 4 件追加 |

## エッジケース

- `glyph_id` が `num_glyphs` 以上: `None` (寛容方針。`append_glyph_outline` の `InvalidData` 返却と挙動が異なるが、bbox 問い合わせは欠落グリフでも可能とする意図的差異)
- 空グリフ (`glyf_start == glyf_end`): `None` (アウトラインが存在しないグリフに bbox を返さない方針)
- Compound Glyph で bbox が `(0, 0, 0, 0)`: `None` (Simple Glyph では `(0, 0, 0, 0)` を有効値として扱う)
- `size == 0` の `Font::glyph_bounds`: 全フィールドが 0.0 の `Some` を返す (Compound (0,0,0,0) 判定は i16 raw 値で行うため scale = 0 でも判定は変わらない)
- `size < 0` / NaN / Inf: テストでロックしない (0021 / 0022 と方針共有)

## 隣接 issue への影響

- **0022 への影響**: `TextMetrics::bounding_box` フィールド追加責務を本 issue 完了直後の PR で実施 (解決方法 step 6 参照)
- **0026 への影響**: 本 issue で追加する単体テスト・PBT が `Font::from_face` シグネチャ拡張 (0001 メタ issue 段階拡張表) で書き換え対象になる。書き換え責務は 0026 のスコープに含む
- **PBT ファイル共有**: 並行ブロック内で 0021 / 0022 / 0023 のうち先着が `pbt/tests/prop_font/main.rs` を作成

## テスト戦略

### 単体テスト (`tests/test_font.rs`)

`shiguredo-rust`「PBT 優先」を踏まえ、PBT で実現可能な不変条件は単体テストに書かない。単体テストに残すのは Arial 環境での既知特性の存在確認のみ:

- `font_glyph_bounds_some`: Arial.ttf で `face.glyph_bounds(face.map_char_to_glyph('A')).is_some()` を検証
- `font_glyph_bounds_invalid_id`: `face.glyph_bounds(u16::MAX).is_none()` を検証 (寛容方針確認)

`load_arial()` ヘルパーを再利用、Arial 不在時はスキップ。

### PBT (`pbt/tests/prop_font/main.rs`)

`Font` を直接使い `Context` は経由しない。Arial 不在時は `proptest!` 外で早期 `return` (冒頭に `fn load_arial() -> Option<FontFace>` ヘルパーを再定義)。`prop_assume!` は避ける。

文字列戦略: Arial の cmap に確実に含まれる ASCII printable + 半角スペース (例: `prop::char::range('!', '~')` で単一 ASCII 文字)。検証する不変条件:

- **`glyph_bounds` 不変条件**: `gb.x_min <= gb.x_max` かつ `gb.y_min <= gb.y_max` (Some の場合)
- **スケール線形性**: `size in 0.001f64..1000.0` で `Font::glyph_bounds(gid)` の各座標 = `FontFace::glyph_bounds(gid)` の各座標 × `Font::scale()` (相対許容範囲 `(lhs - rhs).abs() <= (lhs.abs() + rhs.abs()) * 1e-9 + 1e-12`)
- **`Path::control_box()` 包含**: `face.glyph_bounds(gid)` が `Some` のとき、`face.append_glyph_outline(gid, 0.0, 0.0, &mut path)` で構築した `path.control_box()` (画面座標 Y down) を Y 反転 (`y_min_screen = -y_max_design * scale`、`y_max_screen = -y_min_design * scale` の逆変換) してから `face.glyph_bounds(gid)` (Y up デザイン値) と比較し、`control_box ⊆ glyph_bounds` を相対許容範囲で検証。TrueType glyf ヘッダの bbox は制御点を含む全点座標の min/max として書かれるため、`control_box` が `glyph_bounds` 内に収まる

### 既存テストへの影響

- `append_glyph_outline` の内部実装は `glyph_entry_slice` 経由に差し替えるが、関数のシグネチャ・出力は変わらない。既存 `font_glyph_outline_to_path` / `font_space_glyph_has_no_outline` / `font_fill_text_integration` は合格を維持

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L452、`#[non_exhaustive]` 方針、`TextMetrics::bounding_box` 追加責務の根拠)
- 並行可能 (相互の API には依存しないが `pbt/tests/prop_font/main.rs` 初回作成権は先着の 1 issue のみ): `0021-add-font-scaled-metrics.md` (原則担当)、`0022-add-text-measurement.md`
- 0022 へのスコープ拡張: 本 issue close 直後の PR で `TextMetrics::bounding_box` を追加 (0022 完了が前提)
- 依存される: `0026-add-opentype-basic-shaping.md` (`Font::from_face` シグネチャ拡張で本 issue のテストが書き換え対象。書き換え責務は 0026 のスコープ)
- **着手前提**: 0001 メタ issue の前提 concrete issue 群のうち「テスト用フォント選定」「Compound Glyph point-matching 実装」(Compound bbox 判定の妥当性確認のため) が完了
- **close 前提**: 上記に加え 0022 が close 済 (`TextMetrics::bounding_box` 追加の前提)
