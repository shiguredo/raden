# テキストサイズ計測機能を追加する

- Priority: High
- Category: add
- Created: 2026-05-11
- Completed: 2026-06-20
- Model: Kimi K2.6
- Branch: feature/add-text-measurement
- Polished: 2026-06-20

## 目的

文字列全体のスケール済み水平 advance を事前に計測できる `Font::measure_text()` を追加し、テキストの中央配置や右寄せ等のレイアウトを可能にする。

合わせて、`fill_text` / `measure_text` および後続 concrete issue (0024 `stroke_text` / 0025 カーニング / 0026 OpenType シェーピング) で共有する文字列処理ヘルパー `glyph_run_for_text` を追加し、`fill_text` をそのヘルパーを使う形に書き換える (0001 メタ issue「依存関係 (0024 / 0025 / 0026、部分並行可)」セクションと「Blend2D との対応表」L447 / L448 / L455 行で、本 issue がリファクタリング担当となることが確定)。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する。

- **L447 `BLFont::map_text_to_glyphs(BLGlyphBuffer&)`**: `glyph_run_for_text` で間接対応 (`pub(crate)` で内部実装、`cmap` ルックアップ部分を担当)。0026 で `Font::shape()` に置換される段階拡張対象
- **L448 `BLFont::position_glyphs(BLGlyphBuffer&)`**: 同上 (advance 計算部分を担当)
- **L455 `BLFont::get_text_metrics(BLGlyphBuffer&, BLTextMetrics&)`**: `Font::measure_text()` + `TextMetrics` で対応。Blend2D の `BLTextMetrics` (`advance: BLPoint`, `leading_bearing: BLPoint`, `trailing_bearing: BLPoint`, `bounding_box: BLBox` の 4 フィールド) に対し、0022 時点の `TextMetrics` は `advance: f64` の 1 フィールドのみ

差異と段階拡張の方針 (0001 メタ issue「段階拡張で意味が変わる API」表の `TextMetrics.advance` / `TextMetrics` フィールド / `Font::glyph_run_for_text()` 行を参照):

- `TextMetrics.advance` は **0022 時点ではカーニング非適用の単体 advance 総和** だが、0025 でカーニング適用済み、0026 でシェーピング適用済みに意味が変わる (`#[non_exhaustive]` でフィールド構造の非破壊性は保てるが、値の意味的変化は `CHANGE` 種別)
- `TextMetrics` フィールドは 0023 完了直後の PR で `bounding_box` 追加 (0023 のスコープ)、0026 完了で `leading_bearing` / `trailing_bearing` 追加で `BLTextMetrics` 互換に近づく
- `BLTextMetrics::advance` の `BLPoint` 化 (水平垂直両対応) と `BLGlyphBuffer` 互換 SoA レイアウトは font モジュール安定化前に別 issue で確定 (0001 メタ issue スコープ外)
- `glyph_run_for_text` は `pub(crate) fn(&str, &mut Vec<(u16, f64)>)` から 0026 で `Font::shape()` に置換され、本メソッドは削除される

完了 PR で `docs/BLEND2D.md` L447 / L448 / L455 の raden 列に新規 API 名を追記する。状態列 (`未実装` / `差異あり`) の最終整理は 0001 メタ issue の close PR でまとめて行う。

## 現状

- `Context::fill_text` (`src/api/context.rs:1144-1169`) は描画のみで戻り値なし。文字列処理ロジックがインライン展開され、共通ヘルパー不在
- `Font::glyph_advance(glyph_id)` (`src/font/mod.rs:141-149`) は個別グリフのスケール済み advance を返す。文字列全体の計測には呼び出し側がループする必要がある。`glyph_id >= advance_widths.len()` のとき 0.0 にフォールバック (`cmap` が壊れて範囲外を返した場合の保険)
- カーニングは未実装。advance の単純総和は実描画幅と一致しない可能性 (0025 で対応)
- `pbt/tests/prop_font/main.rs` は未作成 (0001 メタ issue で 0021 のサブタスクと確定。並行ブロックで先着が作成)

## 設計方針

- `Font::measure_text(text: &str) -> TextMetrics` を追加し、文字列全体のスケール済み advance を `TextMetrics` 構造体で返す
- 共通ヘルパー `Font::glyph_run_for_text` を `pub(crate)` で追加し、`measure_text` / `fill_text` の両方をこのヘルパーで実装し直す
- 0024 の `stroke_text` 追加・0025 のカーニング適用・0026 のシェーピング統合はこのヘルパー (あるいは 0026 で `shape()` に置換された後継 API) に対して行う
- `TextMetrics` は将来の拡張に備えて `#[non_exhaustive]` を付与 (0001 メタ issue「font 公開 API の安定化方針」で確定した `non_exhaustive` 方針対象型のうちの 1 つ)。フィールド追加が破壊的変更にならない一方、外部クレートからの構造体リテラル construct と総当たりパターンマッチは禁止される (`measure_text` 戻り値専用で外部 construct を想定しないため意図的)。なお値の意味的変化 (0025 でカーニング適用、0026 でシェーピング適用) は `#[non_exhaustive]` では守られず `CHANGE` 種別となる (0001 メタ issue L71-72 の定義に従う)
- カーニング適用は **呼び出し側 (`fill_text` / `Font::measure_text` / `stroke_text`) の責務**。`glyph_run_for_text` の `advance` は「カーニング非適用の単体 advance」のまま保つ。0025 では `Font::measure_text` 内で `glyph_run_for_text` の結果に対して隣接ペア走査でカーニング量を加算する形になる
- `Context::measure_text` ラッパーの要否は本 issue では扱わない (将来の拡張セクション参照)

## 完了条件

### 追加される API

- `Font::measure_text(text: &str) -> TextMetrics`
- `TextMetrics` 構造体 (`advance: f64`、`#[non_exhaustive]`)
- `Font::glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` (`pub(crate)`)

### 既存 API の挙動維持

- `Context::fill_text` のシグネチャ・描画結果は変わらない (内部実装のみ `glyph_run_for_text` 経由に差し替え)
- `font_fill_text_integration` (`tests/test_font.rs:81-101`) が合格を維持

### テスト

- 単体テストと PBT で正しさを検証
- 0022 時点では「結合性」(`measure_text(a + b) ≈ measure_text(a) + measure_text(b)`) を PBT で検証する。**この不変条件は 0025 でカーニング適用、0026 でリガチャ適用が入ると `(A, V)` などの隣接ペアで破綻する**。0025 / 0026 着手時に本 PBT を更新するか削除する責務は 0025 / 0026 にあり、両 issue 完了条件として明示される (本 issue「将来の拡張」セクション参照)

### ドキュメント

- `fill_text` 内のエラー黙殺 (`let _ =`) と `glyph_id == 0` スキップのトレードオフを日本語コメントで明記 (0001 メタ issue で「`fill_text` エラー黙殺コメント整備」を本 issue のスコープに完全内包と確定。本 issue 解決方法 step 5 で組み込む)
- `docs/BLEND2D.md` L447 / L448 / L455 の raden 列に新規 API 名を追記
- `CHANGES.md` の `## develop` の `### misc` 直下に `ADD` 種別で 2 件を追記 (`TextMetrics` / `Font::measure_text`)

## 解決方法

実装順 (依存順、ボトムアップ): step 1 (`TextMetrics`) → step 2 (`glyph_run_for_text`) → step 3 (`measure_text`) → step 4 (re-export) → step 5 (`fill_text` 書き換え)。

### 1. `TextMetrics` 構造体を追加する

`src/font/mod.rs` に定義:

```rust
/// 文字列全体のメトリクス (ピクセル単位)。
///
/// 0022 時点では `advance` のみ (カーニング非適用)。
/// 将来の拡張: `bounding_box` (0023 完了直後の PR、0023 のスコープ)、
/// `leading_bearing` / `trailing_bearing` (0026 完了で Blend2D BLTextMetrics 互換に近づく)。
/// `advance` の意味は 0025 でカーニング適用済み、0026 でシェーピング適用済みに変わる
/// (`#[non_exhaustive]` でフィールド非破壊だが、値の意味的変化は CHANGE 種別)。
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct TextMetrics {
    /// 文字列全体の水平アドバンス幅 (ピクセル単位)。
    pub advance: f64,
}
```

- `#[non_exhaustive]` により外部クレートからの構造体リテラル construct と総当たりパターンマッチが禁止される (意図的)
- `PartialEq` は単体テストおよび PBT の `assert_eq!` / `prop_assert_eq!` で必要

### 2. 共通ヘルパー `glyph_run_for_text` を追加する

- `impl Font` 内に `pub(crate) fn glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` として配置
  - 戻り値は `()`。`cmap` ルックアップも `glyph_advance` もエラーを返さないため `Result` 不要
  - 可視性は `pub(crate)` とし公開 API として stabilize しない。0025 / 0026 で要素型を変更する余地 (カーニング後 advance、グリフオフセット、シェーピング結果等) を残すため
  - バッファ型 `Vec<(u16, f64)>` は `(glyph_id, advance_in_pixels)` のペア列。`advance_in_pixels` はスケール済み (ピクセル単位) かつ **カーニング非適用の単体値**
- バッファのクリア責務: **ヘルパー冒頭で `buf.clear()` する** (呼び出し側は `clear` 不要)。`clear` 二重呼び出しを防ぐ
- 内部処理:
  - `text.chars()` で 1 文字ずつループ
  - 各文字を `map_char_to_glyph` で `glyph_id` に変換
  - **`glyph_id == 0` (cmap 未マッピング) も含めて** `(glyph_id, glyph_advance(glyph_id))` を `buf` に push する
  - アウトライン構築・カーニング適用は呼び出し側の責務
- 改行 (`\n`) 等の制御文字は通常文字と同様に扱う (cmap で未マップなら `glyph_id == 0`、マップされているなら通常グリフ)。複数行レイアウトは本 issue スコープ外
- ドキュメントコメント (日本語) で次を明記:
  - 呼び出し側 (`fill_text` / `measure_text` / 将来の `stroke_text`) は `glyph_id == 0` を見て `append_glyph_outline` 呼び出しをスキップする
  - advance 加算は三者で共通 (`glyph_id == 0` でも加算)
  - 戻り値の `advance` はカーニング非適用。0025 でカーニング適用時は呼び出し側で隣接ペアの kern 量を加算する。0026 で `Font::shape()` に置換される予定

### 3. `Font::measure_text()` を追加する

- `impl Font` 内に `pub fn measure_text(&self, text: &str) -> TextMetrics` を追加
- 内部実装は「`Vec<(u16, f64)>` を内部で 1 つ確保し、`self.glyph_run_for_text(text, &mut buf)` を呼んだ後、`buf.iter().map(|&(_, a)| a).sum()` を `TextMetrics.advance` に格納」の **1 案固定**。これにより `measure_text` と `fill_text` が同じヘルパー経由となり、0025 のカーニング適用は同じ箇所に追加するだけで両者に反映される
- 呼び出しごとに `Vec` 1 個を割り当てるが、`measure_text` は描画ホットパスではないため許容。将来ホットパス化時は thread-local バッファ等への置き換えを別 issue で検討
- docstring (日本語) で次を明記:
  - 戻り値は文字列全体の水平 advance (ピクセル単位)
  - 0022 時点ではカーニングを適用しない。0025 で `kern` テーブル適用、0026 でシェーピング適用が追加される予定
  - `size == 0` で `advance == 0.0` を返す
  - 改行・制御文字も `cmap` ルックアップ + advance 加算で扱う (複数行レイアウトなし)

### 4. `lib.rs` から re-export する

`src/lib.rs` の `pub use font::{Font, FontData, FontError, FontFace};` を、0001 メタ issue で確定した alphabetical 順を踏まえて以下に変更:

```rust
pub use font::{Font, FontData, FontError, FontFace, TextMetrics};
```

### 5. `fill_text` を `glyph_run_for_text` を使う形に書き換える

`Context` 既存パターン (`tmp_path` を `std::mem::take` でテイクし、メソッド末尾で書き戻す。`src/api/context.rs:1077-1102` 等参照) に合わせる。借用衝突を避けるため `tmp_glyph_run` も同じパターンで扱う。

```rust
pub fn fill_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
    // tmp_path と tmp_glyph_run を同時に take する。
    // 後段の self.fill_path(&path) は &mut self を要求するため、
    // self.tmp_path / self.tmp_glyph_run を借用したまま fill_path を呼ぶと借用衝突する。
    // mem::take で所有を取り出すパターンは tmp_path / stroke_path_buf 等の既存処理と同じ。
    let mut path = std::mem::take(&mut self.tmp_path);
    let mut run = std::mem::take(&mut self.tmp_glyph_run);
    path.clear();
    // run の clear は glyph_run_for_text 側で行うため呼び出し側では不要

    font.glyph_run_for_text(text, &mut run);

    let mut cursor_x = x;
    for &(glyph_id, advance) in run.iter() {
        // glyph_id == 0 (cmap 未マッピング) はアウトライン構築をスキップ。
        // ただし advance は加算してカーソルだけ進める (他のグリフ位置を維持するため)。
        // append_glyph_outline のエラーは let _ で黙殺 (外部入力に対するクラッシュ耐性を優先し、
        // 部分欠落を許容する)。glyph_advance(0) は .notdef の advance を返し、
        // フォントによっては 0、Arial 等の典型的なフォントでは非 0 のため、
        // 結果として一部の未マッピング文字が詰まって描画されることがある。
        if glyph_id != 0 {
            let _ = font.append_glyph_outline(glyph_id, cursor_x, y, &mut path);
        }
        cursor_x += advance;
    }

    if !path.is_empty() {
        self.fill_path(&path);
    }

    self.tmp_path = path;
    self.tmp_glyph_run = run;
}
```

- `Context` 構造体 (`src/api/context.rs` 内 `pub struct Context { ... }`) の `tmp_path: Path` フィールド直後に `tmp_glyph_run: Vec<(u16, f64)>` を追加する。`Context::new` の `tmp_path: Path::new()` 直後で `tmp_glyph_run: Vec::new()` を初期化する。既存テンポラリ Vec フィールド (`edge_buf: Vec::new()` 等) の初期化パターンに揃え、`Vec::with_capacity` は使わない。初回呼び出しで `Vec` の reallocation が発生するが 2 回目以降は capacity が維持される。`Vec::with_capacity` 検討は別の最適化 issue で扱う
- 既存挙動 (描画結果・カーソル前進量) が一致することを既存単体テスト `font_fill_text_integration` で担保する。ただし同テストは「非ゼロピクセルが 1 つでも存在するか」しか検証していないため、ピクセル単位の同一性検証は本 issue のスコープ外
- 上記擬似コード内の日本語コメントが完了条件「`fill_text` 内のエラー黙殺コメント整備」に対応する

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/font/mod.rs` | 既存編集 | `TextMetrics` 構造体定義、`Font::measure_text()`、`Font::glyph_run_for_text()` の追加 |
| `src/lib.rs` | 既存編集 | `TextMetrics` の re-export 追加 (alphabetical 順) |
| `src/api/context.rs` | 既存編集 | `Context` 構造体への `tmp_glyph_run` フィールド追加、`Context::new` での `Vec::new()` 初期化、`fill_text` の `glyph_run_for_text` 経由への書き換え、エラー黙殺トレードオフの日本語コメント追記 |
| `tests/test_font.rs` | 既存編集 | 単体テスト追加 (既存 `load_arial()` 再利用、Arial 不在時スキップ) |
| `pbt/tests/prop_font/main.rs` | 新規 or 既存編集 | 0021 で初回作成済み前提。0022 が並行ブロック内で先着なら本 issue で初回作成。0022 ではテスト本数が少ないため `main.rs` 直書きで進め、サブモジュール分割は 0025 / 0026 着手時にテスト本数が増えた段階で実施 |
| `pbt/Cargo.toml` | 既存編集 (先着の場合) | `[[test]] name = "prop_font" path = "tests/prop_font/main.rs"` (0021 が先着なら本 issue では不要) |
| `docs/BLEND2D.md` | 既存編集 | L447 / L448 / L455 の raden 列に新規 API 追記 |
| `CHANGES.md` | 既存編集 | `### misc` 直下に `ADD` 2 件追加 (`TextMetrics` / `Font::measure_text`) |

## エッジケース

- 空文字列: `TextMetrics { advance: 0.0 }` を返す (`buf` も空、ループ 0 回)
- `size == 0`: `scale == 0` で全 `glyph_advance` が 0.0、自然に `advance == 0.0`。特別扱い不要
- `size < 0` / `size.is_nan()` / `size.is_infinite()`: 現状 `Font::from_face` は弾かず `scale` 経由で伝播する。`Font::from_face` での弾き方は本 issue スコープ外 (0021 と方針を揃え、別 issue 起票予定)
- `glyph_id == 0` (cmap 未マッピング): `(0, glyph_advance(0))` を `buf` に push し advance は加算される。`fill_text` 側ではアウトライン構築をスキップ
- `glyph_id >= num_glyphs` (壊れた cmap): `Font::glyph_advance` が 0.0 を返すため advance に 0.0 が加算される (破壊的挙動にはならない)
- 改行・制御文字: 通常文字と同様に処理。複数行レイアウトはスコープ外
- `scale` の浮動小数点挙動: `scale = size / units_per_em as f64` は除算 1 回、各 `glyph_advance(gid) = aw as f64 * scale` も乗算 1 回。`measure_text` の総和は左から順加算で同入力→同出力。サイズ違いの `Font` 間の線形性は丸めで完全一致しないため PBT では相対誤差で許容

## テスト戦略

### 単体テスト (`tests/test_font.rs`)

`shiguredo-rust` スキル「PBT 優先」を踏まえ、PBT で実現可能な不変条件は単体テストに書かない。単体テストには以下のみ残す:

- 空文字列で `measure_text("").advance == 0.0` (境界値)
- `'\n'` を含む文字列でも panic せず計測される (制御文字経路、Arial の cmap 結果に依存)

`load_arial()` ヘルパーを再利用、Arial 不在時はスキップ。

### PBT (`pbt/tests/prop_font/main.rs`)

`Font` を直接使い `Context` は経由しない。Arial 不在時は `proptest!` 外で早期 `return`。冒頭に `fn load_arial() -> Option<FontFace>` ヘルパーを再定義する (`tests/test_font.rs:5-13` の既存ヘルパーは `Option<(FontData, FontFace)>` を返すが、`FontFace` が `Arc<Vec<u8>>` を所有するため PBT 側では `FontFace` のみで十分)。`prop_assume!` は proptest が低品質扱いするため避ける。

文字列戦略: **Arial cmap に確実に含まれる ASCII printable + 半角スペース** に絞る (例: `prop::string::string_regex("[ -~]{0,32}").unwrap()`)。CJK 等 Arial に含まれない文字は `glyph_id == 0` 多数フォールバックで検証が無意味になるため除外。

検証する不変条件:

- **単一文字一致**: 任意の文字 `c` に対し `measure_text(c.to_string()).advance == glyph_advance(map_char_to_glyph(c))` (同じ式のため厳密一致)
- **結合性 (`glyph_run_for_text` の total advance に限定)**: 同じ `Font` インスタンスで任意の文字列 `a`, `b` に対し、`glyph_run_for_text` の総和 (`buf.iter().map(|&(_, a)| a).sum()`) が `glyph_run_for_text(a+b)` と `glyph_run_for_text(a) + glyph_run_for_text(b)` で一致することを相対許容範囲で検証 (`(lhs - rhs).abs() <= (lhs.abs() + rhs.abs()) * 1e-9 + 1e-12`)。`measure_text` ではなく `glyph_run_for_text` を直接呼ぶことで、0025 でカーニングが `Font::measure_text` に追加されても本不変条件 (カーニング非適用の生 advance 総和) は変わらず維持される。0026 で `glyph_run_for_text` が `Font::shape()` に置換される時点で本 PBT を削除または `shape()` の戻り値型に合わせて更新する責務は 0026 のスコープに含む
- **サイズ線形性**: 任意の `k ∈ [0.5, 4.0]`, `size ∈ [4.0, 200.0]`, 文字列戦略の `text` で `Font::from_face(face, k * size).measure_text(text).advance ≈ k * Font::from_face(face, size).measure_text(text).advance`。結合性と同じ相対誤差判定式
- **非負性**: Arial では全グリフの `glyph_advance` が非負のため `measure_text(text).advance >= 0.0`

`measure_text` と `glyph_run_for_text` 自身の総和の一致はトートロジー (同じヘルパー経由) のため PBT 不要。

### 既存テストへの影響

- `font_fill_text_integration` (`tests/test_font.rs:81-101`) は `fill_text` リファクタリング後も合格すべき (緩い検証なので構造変更のみでは通過)
- ピクセル単位の同一性検証は本 issue スコープ外

## 影響範囲

- 公開 API: `TextMetrics`, `Font::measure_text` を追加。`lib.rs` から re-export。後方互換破壊なし
- 内部 API: `Font::glyph_run_for_text` は `pub(crate)`。`Context` 構造体に非公開フィールド `tmp_glyph_run` 追加 (`Context::new` シグネチャ不変)
- 既存挙動: `fill_text` の描画結果・カーソル前進量は変わらない (実装の構造のみ変更)
- CHANGES.md: `### misc` 直下に `ADD` 種別で「`Font::measure_text` を追加する」「`TextMetrics` 構造体を追加する」を追加。`fill_text` の内部実装差し替えは描画結果が変わらないため CHANGES.md には記載しない (0001 メタ issue の「font 公開 API の安定化方針」に従う)

## 将来の拡張 (本 issue のスコープ外)

- `bounding_box` を `TextMetrics` に追加: **0023 完了直後の PR で 0023 のスコープとして実施** (0023 polish 時に解決方法に明記)
- カーニング適用による `advance` の値変化: 0025 で実装。`measure_text` docstring を「カーニング適用済み」に更新。型と命名は維持。**0025 のスコープに本 PBT の「結合性」更新作業を含む** (`(A, V)` のような隣接ペアで結合性が破綻するため、検証対象を「`glyph_run_for_text` の total advance (カーニング適用前)」に限定するか結合性 PBT を削除する)
- シェーピング結果のメトリクス (`leading_bearing` / `trailing_bearing` 等): 0026 完了で `BLTextMetrics` 互換に近づく (`#[non_exhaustive]` で非破壊追加)。0025 と同様に PBT「結合性」がリガチャで破綻するため、**0026 のスコープに本 PBT 削除または更新作業を含む**
- `glyph_run_for_text` の `Font::shape()` 置換: 0026 で本メソッドを削除し全呼び出し元 (`fill_text` / `measure_text` / `stroke_text`) を `shape()` 経由に置換 (`pub(crate)` のため破壊的変更扱いにならない)
- `Context::measure_text` ラッパー: 本 issue では追加しない。font モジュール安定化前に別 issue で議論

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L447 / L448 / L455、段階拡張表の `TextMetrics.advance` / `TextMetrics` フィールド / `Font::glyph_run_for_text()` 行、依存関係 (0024 / 0025 / 0026、部分並行可) セクションが本 issue の根拠)
- 並行可能 (相互の API には依存しないが `pbt/tests/prop_font/main.rs` 初回作成権は先着の 1 issue のみ): `0021-add-font-scaled-metrics.md` (原則担当)、`0023-add-glyph-bounds.md`。0001 メタ issue の並行ブロック規約に従い、0022 が先着の場合は本 issue で `pbt/tests/prop_font/main.rs` 初回作成と `pbt/Cargo.toml` の `[[test]]` エントリ追加を行い、0021 / 0023 はマージ・追記する
- 依存される: `0024-add-stroke-text.md` (`glyph_run_for_text` を `stroke_text` で使用)、`0025-add-font-kerning.md` (カーニング適用と本 issue「結合性 PBT」の更新)、`0026-add-opentype-basic-shaping.md` (`glyph_run_for_text` を `shape()` で置換、`TextMetrics` フィールド追加)

## 解決方法

- `src/font/mod.rs`:
  - `pub(crate) fn glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` を追加。文字列を 1 文字ずつ `map_char_to_glyph` で glyph_id に変換し、`(glyph_id, glyph_advance(glyph_id))` を `buf` に push する。`glyph_id == 0` も含めて push し、advance 計算ロジックを `fill_text` / `measure_text` で共有する
  - `pub fn measure_text(&self, text: &str) -> TextMetrics` を追加。内部で `Vec<(u16, f64)>` を 1 つ確保し、`glyph_run_for_text` の総和を `TextMetrics.advance` に格納する
  - `TextMetrics { advance: f64 }` 構造体 (`#[derive(Debug, Clone, Copy, PartialEq)]` + `#[non_exhaustive]`) を追加
- `src/lib.rs`: `pub use font::{..., TextMetrics};` を alphabetical 順で追加
- `src/api/context.rs`:
  - `Context` 構造体に `tmp_glyph_run: Vec<(u16, f64)>` フィールドを `tmp_path` 直後に追加
  - `Context::new` で `Vec::new()` 初期化を `tmp_path: Path::new()` 直後に追加
  - `fill_text` を `glyph_run_for_text` 経由に書き換え。借用衝突回避のため `tmp_path` / `tmp_glyph_run` を `mem::take` で取り出し、末尾で書き戻す既存パターンに揃える
- `tests/test_font.rs`:
  - `load_arial()` ヘルパーを `Option<FontFace>` を返す形に統一
  - 単体テスト 4 件追加: `font_measure_text_empty` / `font_measure_text_single_char` / `font_measure_text_newline_no_panic` / `font_glyph_advance_out_of_range_is_zero`
- `pbt/tests/prop_font/main.rs` を新規作成。`ascii_printable_string` 共有戦略と `close_enough` 相対誤差判定で 4 PBT (`single_char_advance` / `concatenation` / `size_linearity` / `non_negative`) を実装
- `pbt/Cargo.toml`: `[[test]] name = "prop_font" path = "tests/prop_font/main.rs"` を追加 (Cargo はディレクトリ配下の `main.rs` を自動認識しないため明示)
- `docs/BLEND2D.md`: L447 / L448 / L455 の raden 列に新規 API 名を追記
- `CHANGES.md`: `## develop` の `### misc` 直下に `[ADD] TextMetrics 構造体を追加する` / `[ADD] Font::measure_text を追加する` を追記
