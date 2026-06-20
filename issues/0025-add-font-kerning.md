# カーニング適用機能を追加する

- Priority: Medium
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-font-kerning
- Polished: 2026-06-20

## 目的

グリフペア間のカーニングを適用し、プロポーショナルフォント (例: `(A, V)` / `(T, o)` / `(W, A)`) でのテキスト描画品質を向上させる。`kern` テーブル (legacy、Format 0) を新規パースし、`fill_text` / `measure_text` / `stroke_text` の 3 経路でカーニング量を `advance` に加算する。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L449 `BLFont::apply_kerning(BLGlyphBuffer&)`**: raden は `FontFace::kern(u16, u16) -> i16` / `Font::kern(u16, u16) -> f64` の個別取得 API と、`fill_text` / `measure_text` / `stroke_text` 内での自動適用で対応。Blend2D が `BLGlyphBuffer` (中間バッファ) に対して一括適用するのに対し、raden は隣接ペア走査で `advance` に加算する形 (0022 で `glyph_run_for_text` が「カーニング非適用の単体 advance」を提供することと整合)

差異の理由:

- raden は `BLGlyphBuffer` 相当 (`GlyphBuffer`) を 0026 で初出するため、本 issue 時点では一括適用 API を持てない。0026 で `Font::shape()` が GPOS Pair Adjustment (kern テーブルを置換) を実装した時点で、本 issue の `kern` テーブル使用経路が GPOS Pair Adjustment 経路に統合される (本 issue のスコープではない、0026 のスコープ)
- 本 issue ではカーニングは無条件で適用する。`FontFeatureSettings` (`kern` feature の on/off 制御) は 0026 で実装

完了 PR で `docs/BLEND2D.md` L449 の raden 列に新規 API 名 (`Font::kern()` / `FontFace::kern()`) を追記する。状態列 (`未実装`) の最終整理は 0001 メタ issue の close PR でまとめて行う。

## 現状

- `measure_text()` (0022) も `fill_text()` (既存 + 0022 リファクタ済) も `stroke_text()` (0024) もカーニング非適用
- `Font::glyph_run_for_text` (0022 で追加された `pub(crate)`) の戻り値 `Vec<(u16, f64)>` の `f64` は「カーニング非適用の単体 advance」(0022 設計方針で確定)
- `kern` テーブル (legacy) は未パース。`tables.rs` に TAG 定数も `parse_kern` 関数もない
- GPOS テーブルは未パースまたは未利用 (0026 で対応予定)

## 設計方針

### `kern` テーブル Format 0

OpenType / Apple TrueType の `kern` テーブル仕様 (legacy):

- **kern テーブルヘッダ**: `version: uint16` (offset 0)、`nTables: uint16` (offset 2)
- **subtable header (Format 0)**: `version: uint16` / `length: uint16` / `coverage: uint16` (offset 6 から続く)。coverage の下位 8 ビットが format (0 = Format 0)、上位 8 ビットがフラグ (horizontal / minimum / cross-stream / override)
- **Format 0 subtable body**: `nPairs: uint16` / `searchRange: uint16` / `entrySelector: uint16` / `rangeShift: uint16` の後に `nPairs` 個の `(left: uint16, right: uint16, value: int16)` ペア配列

サポート方針:

- **Format 0 のみサポート**。Format 1 / 2 / 3 (Apple 拡張) は未対応 (テーブル不在扱いで `kern = 0`)
- coverage の **horizontal フラグ (bit 0)** のみ採用 (vertical / cross-stream は未対応)
- **minimum フラグ (bit 1)**、**cross-stream フラグ (bit 2)**、**override フラグ (bit 3)** はクリアされていることを前提とする (set されていれば本 subtable をスキップして次の subtable へ。Apple Reference Manual 準拠)
- グリフペアの検索は線形探索 (`nPairs` 件の `(left, right)` を順次比較。binary search 最適化は別の最適化 issue)

### API 設計

- `FontFace::kern(glyph_id1: u16, glyph_id2: u16) -> i16` (デザインユニット、テーブル不在時 / ペア未登録時は 0)
- `Font::kern(glyph_id1: u16, glyph_id2: u16) -> f64` (スケール済みピクセル、同じく 0 フォールバック)

### `glyph_run_for_text` の不変性維持

0022 の `glyph_run_for_text` (`pub(crate) fn(&str, &mut Vec<(u16, f64)>)`) の戻り値型 `Vec<(u16, f64)>` は本 issue でも変更しない。`f64` は「カーニング非適用の単体 advance」のまま保つ (0022 設計方針で確定)。これにより 0022 で書かれた PBT (`glyph_run_for_text` の total advance に対する結合性) が本 issue でも維持される。

カーニング適用は **呼び出し側 (`Font::measure_text` / `Context::fill_text` / `Context::stroke_text`) の責務** とし、`glyph_run_for_text` の隣接ペア `(glyph_id[i], glyph_id[i+1])` に対して `Font::kern(glyph_id[i], glyph_id[i+1])` を計算し、`advance[i]` に加算する。

### 値の意味的変化と CHANGE 種別

`Font::measure_text` の `TextMetrics.advance` の値の意味が「カーニング非適用の単体 advance 総和」から「カーニング適用済み」に変化する。0001 メタ issue「font 公開 API の安定化方針」L71 で「値の意味的変化は CHANGE 種別」と確定。本 issue では `CHANGES.md` に `CHANGE` 種別で「`TextMetrics.advance` の意味が変更される」を `## develop` トップ階層に記載する。

## 完了条件

### 追加される API

- `FontFace::kern(glyph_id1: u16, glyph_id2: u16) -> i16` (デザインユニット)
- `Font::kern(glyph_id1: u16, glyph_id2: u16) -> f64` (スケール済みピクセル)

### 既存 API の挙動変化

- `Font::measure_text` の `TextMetrics.advance` の意味が「カーニング非適用」から「カーニング適用済み」に変わる (`CHANGE` 種別)
- `Context::fill_text` の描画結果のグリフ間隔がカーニング適用後の advance に変わる (ピクセル単位の差が出るため `CHANGE` 種別か `UPDATE` か判定、0001 メタ issue L72「描画結果の差は polish 時に確定」に従い `CHANGE` 種別で扱う)
- `Context::stroke_text` の描画結果も同様に変わる (本 issue で 0024 で書かれた `stroke_text` の後追い改修を実施する。0001 メタ issue「依存関係」セクションで本後追い改修は 0025 のスコープと確定)

### docstring 更新

- `Font::measure_text` の docstring を「カーニング適用済み」に更新 (型と命名は維持)
- `glyph_run_for_text` の docstring は「カーニング非適用の単体 advance のまま」を維持 (0026 で `shape()` に置換されるまで不変)

### 0022 PBT の更新

- 0022 で書かれた「`glyph_run_for_text` の total advance に対する結合性 PBT」は本 issue でも維持 (`glyph_run_for_text` の advance は変わらないため)
- 0022 で書かれた「単一文字一致 / サイズ線形性 / 非負性」も維持
- ただし `Font::measure_text` の結果に対する PBT を追加する場合は「カーニング適用後」の値で書く

### テスト

- 単体テストには「Arial 環境での既知ペア (例: `(A, V)`) のカーニング量取得」と「カーニング適用後の `measure_text` 結果が適用前より小さい / 等しい」のみ残す
- PBT で以下を検証:
  - 全 0 への退化: kern テーブル不在のテストフォント (テスト用フォント選定で確定) で `Font::kern(gid1, gid2) == 0.0` を任意ペアで検証 → 単体テストで代替可能なら PBT は不要
  - **隣接ペア加算の関係**: 任意の文字列 `text` に対し `Font::measure_text(text).advance == sum(Font::glyph_run_for_text の advance) + sum(Font::kern(glyph_id[i], glyph_id[i+1]))` (相対許容範囲 `1e-9`)。ただし 0022 で `glyph_run_for_text` を `pub(crate)` のままにしているため、PBT 内から `glyph_run_for_text` を呼ぶ手段が要る (テストハーネス用に `pub(crate)` のままで `pbt/` クレートからアクセス可能か確認、または 0022 / 本 issue の polish で `pub(crate)` を `pub(in crate::font)` 程度に絞り、`#[cfg(test)]` でテスト用 wrapper を提供する)

### ドキュメント

- `docs/BLEND2D.md` L449 の raden 列に新規 API 名を追記
- `CHANGES.md` の `## develop` の **トップ階層** に `CHANGE` 種別で `TextMetrics.advance` の意味変化と `Context::fill_text` / `stroke_text` の描画結果変化を記載 (`shiguredo-changelog` スキル準拠、0001 メタ issue L71-72 に従う)
- `CHANGES.md` の `## develop` の `### misc` 直下に `ADD` 種別で 2 件 (`FontFace::kern` / `Font::kern`) を追記

### Fuzzing

- 不正な `kern` テーブル (length 不正、coverage 不正、nPairs オーバーフロー等) に対するクラッシュ耐性を fuzzing で検証
- Fuzz target: フォントファイル全体のバイト列を入力とする `parse_all` 経由の既存 fuzz target (0001 メタ issue「fuzzing 基盤と既存コード fuzz target」tracked で追加された) でカバー可能なら新規 target 不要。新規 target が必要なら本 issue で追加 (`fuzz/fuzz_targets/parse_kern.rs` 等)

## 解決方法

### 1. `kern` テーブルパースの追加 (`src/font/tables.rs`)

- `tables.rs:112-118` の TAG_* 群に `const TAG_KERN: u32 = tag(b"kern");` を追加
- `KernPair { left: u16, right: u16, value: i16 }` 構造体と `KernTable { pairs: Vec<KernPair> }` 構造体を新規定義 (`pub(crate)`)
- `parse_kern(data: &[u8], rec: TableRecord) -> Result<KernTable, FontError>` を新規作成:
  - テーブルヘッダ: `version` (offset 0, u16), `n_tables` (offset 2, u16)。version 0 (Apple Format 0) のみサポート
  - 各 subtable ヘッダ (offset 4 から開始): `version` (u16), `length` (u16), `coverage` (u16)。Format 0 (`coverage & 0xff == 0`) かつ horizontal (`coverage & 0x01 == 1`) かつ minimum / cross-stream / override がクリア (`coverage & 0x0e == 0`) のもののみ採用
  - Format 0 subtable body: `n_pairs` (u16), `search_range` (u16), `entry_selector` (u16), `range_shift` (u16)。続いて `n_pairs` 個の `(left: u16, right: u16, value: i16)` を読む
  - 複数の subtable がある場合、最初に採用条件を満たした subtable のペアのみ使用 (Apple Reference では複数 subtable の値を合算する実装も許容されるが本 issue では最初の 1 つに限定。0026 で GPOS と統合する際に再評価)
  - エラー時 (length 不正、nPairs オーバーフロー、データ不足) は寛容方針で `Ok(KernTable { pairs: vec![] })` を返す (`FontFace::from_data` 自体は失敗させない)
- `parse_all` で `dir.find(TAG_KERN)` を使い、`None` の場合 `KernTable { pairs: vec![] }` を返す (`dir.require` は使わない)
- `ParsedTables` に `pub kern: KernTable` を追加

### 2. `FontFace::kern()` 追加 (`src/font/mod.rs`)

```rust
impl FontFace {
    /// グリフペアのカーニング量 (デザインユニット)。
    /// kern テーブル不在 / ペア未登録の場合は 0。
    /// Format 0 / horizontal coverage のみサポート。
    pub fn kern(&self, glyph_id1: u16, glyph_id2: u16) -> i16 {
        // 線形探索 (nPairs は数百〜数千程度のため許容)
        self.tables
            .kern
            .pairs
            .iter()
            .find(|p| p.left == glyph_id1 && p.right == glyph_id2)
            .map(|p| p.value)
            .unwrap_or(0)
    }
}
```

### 3. `Font::kern()` 追加 (`src/font/mod.rs`)

```rust
impl Font {
    /// グリフペアのカーニング量 (スケール済みピクセル単位)。
    pub fn kern(&self, glyph_id1: u16, glyph_id2: u16) -> f64 {
        self.face.tables
            .kern
            .pairs
            .iter()
            .find(|p| p.left == glyph_id1 && p.right == glyph_id2)
            .map(|p| p.value as f64 * self.scale)
            .unwrap_or(0.0)
    }
}
```

### 4. `Font::measure_text` の修正 (`src/font/mod.rs`)

`glyph_run_for_text` の結果に対して隣接ペアのカーニング量を加算:

```rust
pub fn measure_text(&self, text: &str) -> TextMetrics {
    let mut run: Vec<(u16, f64)> = Vec::new();
    self.glyph_run_for_text(text, &mut run);

    let mut advance_total: f64 = run.iter().map(|&(_, a)| a).sum();
    for i in 0..run.len().saturating_sub(1) {
        advance_total += self.kern(run[i].0, run[i + 1].0);
    }
    // bounding_box は 0023 完了直後の PR で追加される。本 issue ではこの行で
    // bounding_box の計算ロジックも維持する (cursor_x にカーニング量を加算する形)。
    TextMetrics { advance: advance_total, /* bounding_box: ... */ }
}
```

`bounding_box` (0023 で追加) との関係は 0023 完了状態を前提に整合させる。

### 5. `Context::fill_text` / `Context::stroke_text` の修正 (`src/api/context.rs`)

`fill_text` (0022 でリファクタ済) と `stroke_text` (0024 で追加済) の両方で、`glyph_run_for_text` ループ内の `cursor_x += advance;` の前後にカーニング量を加算する:

```rust
let mut cursor_x = x;
for (i, &(glyph_id, advance)) in run.iter().enumerate() {
    if glyph_id != 0 {
        let _ = font.append_glyph_outline(glyph_id, cursor_x, y, &mut path);
    }
    cursor_x += advance;
    // 次のグリフがあれば、その前に kern を加算 (グリフ i の後、グリフ i+1 の前)。
    if i + 1 < run.len() {
        cursor_x += font.kern(glyph_id, run[i + 1].0);
    }
}
```

これにより `fill_text` / `stroke_text` の描画結果のグリフ間隔がカーニング適用済みになる。

### 6. fuzz target 追加 (前提 concrete issue「fuzzing 基盤」完了が前提)

`fuzz/fuzz_targets/parse_font.rs` (前提 concrete issue で追加済の `parse_all` 経由 target) でカバーされるなら新規 target 不要。新規 target が必要なら `fuzz/fuzz_targets/parse_kern.rs` を本 issue で追加。

### 7. CHANGES.md 更新

`CHANGES.md` の `## develop` のトップ階層に追加 (`CHANGE` 種別):

```
- [CHANGE] `Font::measure_text` の戻り値 `TextMetrics.advance` の意味が「カーニング非適用」から「kern テーブル適用済み」に変わる
  - @<author>
- [CHANGE] `Context::fill_text` / `Context::stroke_text` の描画結果のグリフ間隔が kern テーブル適用済みに変わる
  - @<author>
```

`## develop` の `### misc` 直下に `ADD` 種別で 2 件追加:

```
- [ADD] `FontFace::kern` を追加する
  - @<author>
- [ADD] `Font::kern` を追加する
  - @<author>
```

### 8. BLEND2D.md 更新

`docs/BLEND2D.md` L449 の raden 列に `FontFace::kern() / Font::kern() (内部で fill_text / measure_text / stroke_text に自動適用)` を追記する。状態列の最終整理は 0001 メタ issue の close PR でまとめて行う。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/tables.rs` | 既存編集 | `TAG_KERN` 定数、`KernPair` / `KernTable` 構造体、`parse_kern`、`ParsedTables` への `kern` 追加 |
| `src/font/mod.rs` | 既存編集 | `FontFace::kern()`、`Font::kern()`、`Font::measure_text` 内のカーニング加算 |
| `src/api/context.rs` | 既存編集 | `fill_text` / `stroke_text` 内のカーニング加算 |
| `tests/test_font.rs` | 既存編集 | 単体テスト追加 (Arial 環境での `(A, V)` ペア等) |
| `pbt/tests/prop_font/main.rs` | 既存編集 | PBT 追加 (隣接ペア加算関係、テーブル不在退化等) |
| `fuzz/fuzz_targets/parse_kern.rs` | 新規 (必要な場合のみ) | kern テーブル fuzz target |
| `docs/BLEND2D.md` | 既存編集 | L449 の raden 列に新規 API 追記 |
| `CHANGES.md` | 既存編集 | トップ階層に `CHANGE` 2 件、`### misc` 直下に `ADD` 2 件 |
| `src/lib.rs` | 変更不要 | `Font::kern` / `FontFace::kern` はメソッド追加のみ。新規型は導入しない |

## エッジケース

- kern テーブル不在: `KernTable { pairs: vec![] }` で `find` が `None`、カーニング量 0
- kern テーブルが Format 0 以外 (Format 1 / 2 / 3、Apple 拡張): subtable をスキップし、最終的に `pairs` が空ならカーニング量 0
- coverage の vertical / cross-stream / minimum / override フラグ set: subtable をスキップ
- グリフペアが kern テーブルにない: カーニング量 0
- `size == 0` の `Font`: `scale == 0` で `Font::kern` の戻り値が 0.0
- 単一文字 / 空文字列: 隣接ペアなし、カーニング加算なし
- `size < 0` / NaN / Inf: テストでロックしない (0021 / 0022 / 0023 と方針共有)

## 隣接 issue への影響

- **0022 への影響**: 0022 の `Font::measure_text` の docstring を「カーニング適用済み」に更新する責務は **本 issue にある**。0022 で書かれた PBT (`glyph_run_for_text` の total advance に対する結合性) は本 issue でも維持される (`glyph_run_for_text` の advance は変わらないため)。`Font::measure_text` の結果に対する PBT を別途追加する場合は「カーニング適用後」で書く
- **0024 への後追い改修**: 0024 で書かれた `stroke_text` の `cursor_x += advance;` ループにカーニング量加算を追加する。**この後追い改修は本 issue のスコープに含む** (0001 メタ issue「依存関係 (0024 / 0025 / 0026、部分並行可)」セクションで確定)
- **0026 への申し送り**: 0026 で GPOS テーブルの Pair Adjustment (Lookup Type 2) を実装し、本 issue の `kern` テーブル使用経路を GPOS Pair Adjustment 経路に置換する。**この置換責務は 0026 のスコープ**。`FontFace::kern` / `Font::kern` の API は GPOS 不在フォントへのフォールバックとして残すか削除するかは 0026 polish で確定 (削除すれば破壊的変更、残せば追加 API)。0026 で `FontFeatureSettings` (`kern` feature on/off 制御) も導入される

## テスト戦略

### 単体テスト (`tests/test_font.rs`)

`shiguredo-rust`「PBT 優先」を踏まえ、PBT で実現可能な不変条件は単体テストに書かない。

- `font_kern_known_pair`: Arial.ttf の既知ペア (例: `(A, V)` で負のカーニング量) に対し `face.kern(map_char_to_glyph('A'), map_char_to_glyph('V')) < 0` を検証 (Arial は OS/2 v4 + kern テーブルあり、`(A, V)` は典型的なネガティブカーニングペア)
- `font_kern_no_pair`: `face.kern(0, 0)` または存在しないペアに対し `== 0` を検証
- `font_measure_text_with_kerning`: `font.measure_text("AV").advance < font.glyph_advance(...) の単純総和` を検証 (カーニング適用後は単純総和より小さい)

`load_arial()` ヘルパー再利用、Arial 不在時はスキップ。

### PBT (`pbt/tests/prop_font/main.rs`)

- **テーブル不在退化**: kern テーブル不在フォント (テスト用フォント選定で別途確保) で任意ペアの `Font::kern` が 0 を返すことを検証 (テスト用フォントが揃わない場合は単体テストで代替可能)
- **隣接ペア加算の関係**: `Font::measure_text(text).advance == (glyph_run_for_text の total advance) + sum(Font::kern(glyph_id[i], glyph_id[i+1]))` を相対許容範囲 `1e-9` で検証。`glyph_run_for_text` を pbt クレートから呼ぶには `pub(crate)` のままだとアクセス不可。テストハーネス用に `#[cfg(test)]` の wrapper を提供するか、`Font::measure_text` の結果と `Font::kern` のみで検証 (text の各文字を `chars()` で走査して各 `glyph_advance` を合算する形に置き換え)

### 0022 PBT との関係

0022 で書かれた `glyph_run_for_text` の total advance に対する結合性 PBT は **本 issue でも維持** (`glyph_run_for_text` の advance は変わらないため)。0022 PBT の更新は不要。

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L449、`Font::measure_text` の意味的変化、0024 後追い改修の根拠、`FontFeatureSettings` は 0026 で導入の確定)
- 依存: `0022-add-text-measurement.md` (`glyph_run_for_text` の実装、`Font::measure_text` の存在)、`0024-add-stroke-text.md` (`stroke_text` への後追い改修対象)
- 依存される: `0026-add-opentype-basic-shaping.md` (GPOS Pair Adjustment で本 issue の `kern` テーブル使用経路を置換、`FontFeatureSettings` で `kern` feature on/off 制御)
- **着手前提**: 0022 と 0024 が close 済、0001 メタ issue の前提 concrete issue 群 (テスト用フォント選定、fuzzing 基盤、ベースライン benchmark 計測基盤、Compound Glyph point-matching 実装) が close 済
- **close 前提**: 上記に加え、0024 で書かれた `stroke_text` への後追い改修が本 issue 内で完了している
