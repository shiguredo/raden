# テキストサイズ計測機能を追加する

- Priority: High
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-text-measurement
- Polished: 2026-06-20

## 目的

文字列全体の描画幅を事前に計測できるようにし、テキストの中央配置や右寄せ等のレイアウトを可能にする。

合わせて、`fill_text` と `measure_text` および後続 issue（0024 `stroke_text` / 0025 カーニング / 0026 OpenType シェーピング）で共有する文字列処理ヘルパー `glyph_run_for_text` を追加し、`fill_text` をそのヘルパーを使う形に書き換える。

## 現状

- `Context::fill_text` (`src/api/context.rs:1144-1169`) は描画のみを行い、サイズ情報を返さない。文字列処理ロジックは関数内にインライン展開されており、共通ヘルパーが存在しない
- `Font::glyph_advance(glyph_id)` (`src/font/mod.rs:141-149`) は個別グリフのスケール済み advance を返すが、文字列全体の計測には呼び出し側がループする必要がある
- `Font::glyph_advance` は `glyph_id >= hmtx.advance_widths.len()` のとき 0.0 を返す。`parse_hmtx` (`src/font/tables.rs:198-235`) で `advance_widths.len() == num_glyphs` が保証されるため、`cmap` が壊れていて `num_glyphs` 以上の glyph_id を返した場合のフォールバックとして機能する
- カーニングが未実装のため、単純な advance の足し合わせは実際の描画幅と一致しない可能性がある（この issue のスコープ外。0025 で対応）
- `pbt/tests/prop_font/main.rs` は未作成。0001 メタ issue tracked item「`pbt/tests/prop_font/main.rs` 初回作成」は 0021 に紐付けされている

## 設計方針

- `Font` に `measure_text()` メソッドを追加し、文字列全体のスケール済み advance を `TextMetrics` 構造体で返す
- 0022 で文字列処理の共通ヘルパー `glyph_run_for_text` を追加し、`measure_text` と `fill_text` の両方をこのヘルパーで実装し直す（メタ issue 0001 の決定。0001-enhance-font-module-maturity.md:94-100 を参照）
- 0024 の `stroke_text` および 0025 のカーニング適用、0026 のシェーピング統合はこのヘルパー（あるいはその後継 API）に対して行う
- `TextMetrics` は将来の拡張（`bounding_box` 追加: 0023 完了後、leading/trailing bearing 追加: 0026 完了後）に備えて `#[non_exhaustive]` 属性を付与し、フィールド追加が破壊的変更にならないようにする。同属性により外部クレートからの構造体リテラル construct および総当たりパターンマッチが禁止されるが、`TextMetrics` は `measure_text` の戻り値専用で外部 construct のユースケースを想定しないため、意図的にこの制約を課す
- カーニング適用は **呼び出し側（`fill_text` / `Font::measure_text` / `stroke_text`）の責務** とし、`glyph_run_for_text` の `advance` は「カーニング非適用の単体 advance」のまま保つ。0025 では `Font::measure_text` 内で `glyph_run_for_text` の結果に対して隣接ペア走査でカーニング量を加算する形になる。この方針により `glyph_run_for_text` の要素型を変えずに 0025 を実現できる

## 完了条件

- `Font::measure_text(text)` で文字列全体のスケール済み advance が `TextMetrics` として取得できる
- `fill_text` が `glyph_run_for_text` を使う形に書き換えられており、`measure_text` と `fill_text` の advance 計算ロジックが実装レベルで共有されている
- `TextMetrics` が `lib.rs` から `pub use` で re-export されている
- 単体テストと PBT で正しさを検証している
- `fill_text` 内のエラー黙殺と `glyph_id == 0` スキップに関するトレードオフが日本語コメントで明記されている（0001 tracked item「fill_text エラー黙殺コメント」の組み込み）

## 解決方法

### 1. 共通ヘルパー `glyph_run_for_text` を追加する

- `impl Font` 内に `pub(crate) fn glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` として配置する
  - 戻り値は `()`。`cmap` ルックアップも `glyph_advance` もエラーを返さないため `Result` は不要
  - 可視性は `pub(crate)` とし、公開 API として stabilize しない。0025 / 0026 で要素型を変更する余地（カーニング後 advance、グリフオフセット、シェーピング結果等）を残すため
  - バッファ型 `Vec<(u16, f64)>` は `(glyph_id, advance_in_pixels)` のペア列。`advance_in_pixels` はスケール済み（ピクセル単位）かつ **カーニング非適用の単体値**
- バッファのクリア責務: **ヘルパーの冒頭で `buf.clear()` する**（呼び出し側は `clear` 不要）。これにより呼び出し側コードがシンプルになり、`clear` 二重呼び出しを防ぐ
- 内部処理:
  - `text.chars()` で 1 文字ずつループ
  - 各文字を `map_char_to_glyph` で `glyph_id` に変換
  - **`glyph_id == 0`（cmap 未マッピング）の場合も含めて** `(glyph_id, glyph_advance(glyph_id))` を `buf` に push する
  - アウトライン構築・カーニング適用は呼び出し側の責務。これにより `fill_text` / `stroke_text` / `measure_text` で同一の advance 計算が共有される
- 改行（`\n`）等の制御文字は通常の文字と同様に扱う（cmap で未マップなら `glyph_id == 0`、マップされているなら通常グリフ）。複数行レイアウトは本 issue のスコープ外
- ドキュメントコメント（日本語）で次を明記する:
  - 呼び出し側（`fill_text` / `measure_text` / 将来の `stroke_text`）は `glyph_id == 0` を見て `append_glyph_outline` 呼び出しをスキップする
  - advance 加算は三者で共通（`glyph_id == 0` でも加算）
  - 戻り値の `advance` はカーニング非適用。0025 でカーニングを適用する場合は呼び出し側で隣接ペアの kern 量を加算する

### 2. `measure_text` を追加する

- `impl Font` 内に `pub fn measure_text(&self, text: &str) -> TextMetrics` を追加
- 内部実装は「`Vec<(u16, f64)>` を内部で 1 つ確保し、`self.glyph_run_for_text(text, &mut buf)` を呼んだ後、`buf.iter().map(|&(_, a)| a).sum()` を `TextMetrics.advance` に格納する」の **1 案に固定** する。これにより `measure_text` と `fill_text` が同じヘルパー経由となり、カーニング適用（0025）も同じ箇所に追加するだけで両者に反映される
- 呼び出しごとに `Vec` 1 個を割り当てるが、`measure_text` は描画ホットパスではないため許容する。将来ホットパス化したら `Font` 内部の thread-local バッファ等への置き換えを検討（本 issue のスコープ外）
- `measure_text` の docstring（日本語）で次を明記する:
  - 戻り値は文字列全体の水平 advance（ピクセル単位）
  - 0022 時点ではカーニングを適用しない。0025 で `kern` テーブル適用が追加される予定
  - `size == 0` で `advance == 0.0` を返す
  - 改行・制御文字も `cmap` ルックアップ + advance 加算で扱う（複数行レイアウトなし）
- `Font` の他メソッド（`size() -> f64`、`ascent() -> f64` 等）は raw `f64` を返すが、`measure_text` を構造体で返す根拠は `bounding_box` 等のフィールド追加予定があるため、将来の拡張余地を確保する目的で構造体型を採用する

### 3. `TextMetrics` 構造体を追加する

- `src/font/mod.rs` に定義:

  ```rust
  /// 文字列全体のメトリクス（ピクセル単位）。
  ///
  /// 0022 時点では `advance` のみを保持する。0023 完了後に `bounding_box` の追加を予定。
  #[derive(Debug, Clone, Copy, PartialEq)]
  #[non_exhaustive]
  pub struct TextMetrics {
      /// 文字列全体の水平アドバンス幅（ピクセル単位）。
      pub advance: f64,
  }
  ```

- `#[non_exhaustive]` により外部クレートからの構造体リテラル construct と総当たりパターンマッチが禁止される。これは意図的（外部 construct を想定せず、フィールド追加を非破壊変更にする）
- `PartialEq` は単体テストおよび PBT の `assert_eq!` / `prop_assert_eq!` で必要

### 4. `lib.rs` から re-export する

- `src/lib.rs` の `pub use font::{Font, FontData, FontError, FontFace};` を `pub use font::{Font, FontData, FontError, FontFace, TextMetrics};` に変更

### 5. `fill_text` を `glyph_run_for_text` を使う形に書き換える

`Context` の既存パターン（`tmp_path` を `std::mem::take` でテイクし、メソッド末尾で書き戻す。`src/api/context.rs:1077-1102` 等参照）に合わせる。借用衝突を避けるため、`tmp_glyph_run` も同じパターンで扱う。

書き換え後の擬似コード:

```rust
pub fn fill_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
    // tmp_path と tmp_glyph_run を同時に take する（&mut self.fill_path(&path) 呼び出しでの借用衝突を避けるため）
    let mut path = std::mem::take(&mut self.tmp_path);
    let mut run = std::mem::take(&mut self.tmp_glyph_run);
    path.clear();
    // run の clear は glyph_run_for_text 側で行うため呼び出し側では不要

    font.glyph_run_for_text(text, &mut run);

    let mut cursor_x = x;
    for &(glyph_id, advance) in run.iter() {
        // glyph_id == 0 (cmap 未マッピング) はアウトライン構築をスキップ。
        // ただし advance は加算してカーソルだけ進める（他のグリフ位置を維持するため）。
        // append_glyph_outline のエラーは let _ で黙殺（外部入力に対するクラッシュ耐性を優先し、
        // 部分欠落を許容する）。glyph_advance(0) の値はフォント依存で .notdef の実 advance だが、
        // フォントによっては 0 を返す場合もあるため、結果として一部の未マッピング文字が
        // 詰まって描画されることがある。
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

- `Context` 構造体に `tmp_glyph_run: Vec<(u16, f64)>` フィールドを追加し、`Context::new` で `Vec::new()` で初期化する。既存の `tmp_path` も `Path::new()` 初期化で `Vec::with_capacity` を使っていないため、`tmp_glyph_run` も同じ方針に揃える。初回呼び出しで `Vec` の reallocation が複数回発生するが、2 回目以降は capacity が維持されるため安定する。`Vec::with_capacity` による初期確保が望ましい場合は別の最適化 issue として扱う
- 既存挙動（描画結果・カーソル前進量）が一致することを既存単体テスト `font_fill_text_integration` で担保する。ただし同テストは「非ゼロピクセルが 1 つでも存在するか」しか検証していないため、ピクセル単位の同一性検証は本 issue のスコープ外（必要であれば別 issue で扱う）
- 上記擬似コード内の日本語コメントが完了条件「`fill_text` 内のエラー黙殺と `glyph_id == 0` スキップに関するトレードオフが日本語コメントで明記されている」に対応する

## 変更対象ファイル

- `src/font/mod.rs`: `TextMetrics` 構造体定義、`Font::measure_text()`、`Font::glyph_run_for_text()` の追加
- `src/lib.rs`: `TextMetrics` の re-export 追加
- `src/api/context.rs`: `Context` 構造体への `tmp_glyph_run` フィールド追加、`Context::new` での `Vec::new()` 初期化、`fill_text` の `glyph_run_for_text` を使う形への書き換え、エラー黙殺トレードオフの日本語コメント追記
- `tests/test_font.rs`: 単体テストの追加（既存 `load_arial()` ヘルパー再利用、Arial 不在時はスキップ）
- `pbt/tests/prop_font/main.rs`: PBT の追加（0021 で初回作成される前提。0021 未完了で 0022 単独着手の場合は本 issue で初回作成する。0022 ではテスト本数が少ないため `main.rs` に直接 PBT 関数を書く。サブモジュール分割（`pbt/README.md` のディレクトリモジュール命名規則）はテスト本数が増えてきた段階（0025 / 0026 着手時を想定）で導入し、その時点で 0021 ／ 0022 が書いた既存テストもサブモジュールに移動する。0021 とは事前に方針を共有する

## エッジケース

- 空文字列: `TextMetrics { advance: 0.0 }` を返す（`buf` も空、ループ 0 回）
- `size == 0`: `scale == 0` となり、全 `glyph_advance` が 0.0 を返すため、自然に `advance == 0.0` になる。特別扱い不要
- `size < 0` / `size.is_nan()` / `size.is_infinite()`: 現状の `Font::from_face` は弾かないため、`scale` 経由でこれらが伝播する。`Font::from_face` での弾き方は本 issue のスコープ外（0021 と方針を揃える）
- `glyph_id == 0`（cmap 未マッピング）: `(0, glyph_advance(0))` を `buf` に push し、advance は加算される。`fill_text` 側ではアウトライン構築をスキップする
- `glyph_id >= num_glyphs`（不正フォントで cmap が範囲外グリフ ID を返した場合）: `Font::glyph_advance` が 0.0 を返すため、advance に 0.0 が加算される（破壊的挙動にはならない）
- 改行・制御文字: 通常文字と同様に処理。複数行レイアウトは本 issue のスコープ外
- `scale` の浮動小数点挙動: `scale = size / units_per_em as f64` は除算 1 回で計算され、各 `glyph_advance(gid) = aw as f64 * scale` も乗算 1 回。`measure_text` の総和は左から順に加算されるため、同じ `Font` インスタンスでの `measure_text` 呼び出しは決定的（同入力→同出力）。サイズ違いの `Font` インスタンス間の線形性は丸めで完全一致しないため PBT では相対誤差で許容する（後述）
- `pbt/tests/prop_font/main.rs` が 0021 未完了で存在しない場合: 0022 で初回作成する。0021 と 0022 が真の並行着手で同ファイルを取り合うときは、先着が作成し後着がマージ・追記する

## テスト戦略

### 単体テスト (`tests/test_font.rs`)

`load_arial()` ヘルパーを再利用し、Arial 不在時はスキップする既存構造に揃える。次のケースを網羅:

- 空文字列で `measure_text("").advance == 0.0`
- 単一文字 `'A'` で `measure_text("A").advance == glyph_advance(map_char_to_glyph('A'))`
- 複数文字 `"Hello"` で `measure_text("Hello").advance` が「正の有限値」で、かつ `Hello` の各文字の `glyph_advance` 合計と一致する（同じヘルパー経由で計算するため厳密一致が成立）
- `'\n'` を含む文字列でも panic せず計測される（advance は `cmap` 結果に依存）
- `size == 0.0` の `Font` で `measure_text("Hello").advance == 0.0`

### PBT (`pbt/tests/prop_font/main.rs`)

`Font` を直接使い `Context` は経由しない。Arial が必要なため、テスト関数冒頭で `load_arial()` 相当のヘルパーを呼び、Arial 不在時は早期 `return` する（`proptest!` ブロック外で判定。`prop_assume!` でのスキップは proptest が低品質扱いするため避ける）。`load_arial()` は `tests/test_font.rs` 専用のため、`pbt/tests/prop_font/main.rs` 冒頭に同等ヘルパーを再定義する（テストヘルパーの公開化は本 issue のスコープ外）。

文字列戦略は **Arial の cmap に確実に含まれる ASCII printable + 半角スペース** に絞る（例: `prop::string::string_regex("[ -~]{0,32}").unwrap()`）。CJK 等 Arial に含まれない文字は `glyph_id == 0` 多数のフォールバックになり、検証が無意味になるため除外する。

検証する不変条件:

- 空文字列性: `measure_text("").advance == 0.0`（厳密一致）
- 単一文字: 任意の文字 `c` に対し `measure_text(c.to_string()).advance == glyph_advance(map_char_to_glyph(c))`（同じ式のため厳密一致）
- 結合性: 同じ `Font` インスタンスで任意の文字列 `a`, `b` に対し `measure_text(a + b).advance ≈ measure_text(a).advance + measure_text(b).advance`。加算順の違いによる浮動小数点誤差を相対許容範囲 `1e-9` 以内で検証（`(lhs - rhs).abs() <= (lhs.abs() + rhs.abs()) * 1e-9 + 1e-12`）。0025 でカーニングが入ると `(A, V)` のような隣接ペアで成立しなくなるため、0025 着手時に本不変条件は更新が必要
- サイズ線形性: 任意の `k ∈ [0.5, 4.0]`, `size ∈ [4.0, 200.0]`, 上記文字列戦略の `text` で `Font::from_face(face, k * size).measure_text(text).advance ≈ k * Font::from_face(face, size).measure_text(text).advance`。結合性 PBT と同じ判定式 `(lhs - rhs).abs() <= (lhs.abs() + rhs.abs()) * 1e-9 + 1e-12` を用いる
- 非負性: Arial では全グリフの `glyph_advance` が非負のため `measure_text(text).advance >= 0.0`

`measure_text` と `glyph_run_for_text` 自身の総和の一致はトートロジー（同じヘルパーから計算するため）なので PBT には含めない。

### 既存テストへの影響

- `font_fill_text_integration` (`tests/test_font.rs:81-101`) は `fill_text` のリファクタリング後も合格すべき。同テストは「非ゼロピクセルが少なくとも 1 つ存在するか」のみの緩い検証だが、リファクタリングは挙動を変えず実装の構造のみ変えるため通過するはず
- リファクタ前後でピクセル単位の同一性検証は本 issue のスコープ外（より厳密な回帰検出が必要であれば別 issue で扱う）
- `Cargo.toml` / `pbt/Cargo.toml` への依存追加は不要

## 影響範囲

- 公開 API: `TextMetrics`、`Font::measure_text` を追加。`lib.rs` から re-export。後方互換破壊はなし
- 内部 API: `Font::glyph_run_for_text` は `pub(crate)` のため公開 API への影響なし。`Context` 構造体に非公開フィールド `tmp_glyph_run` を追加（既存 `Context::new` のシグネチャは変えない）
- 既存挙動: `fill_text` の描画結果・カーソル前進量は変わらない（実装の構造のみ変更）
- CHANGES.md: `ADD` 種別で「`Font::measure_text` を追加する」「`TextMetrics` 構造体を追加する」を develop の `misc` 直下に記載する。`fill_text` の `glyph_run_for_text` 化リファクタリングは内部実装のみで挙動変化なしのため CHANGES.md には記載しない。記載方法は `shiguredo-changelog` スキルを参照

## 将来の拡張（本 issue のスコープ外）

- `bounding_box` を `TextMetrics` に追加（0023 完了後。`#[non_exhaustive]` のため非破壊で追加可能）
- カーニング適用による `advance` の値変化（0025 で実装。`measure_text` の docstring を「カーニング適用済み」に更新する。型と命名は維持）。0025 着手時に結合性 PBT も更新が必要
- シェーピング結果のメトリクス（leadingBearing / trailingBearing 等）の追加（0026 完了後。`#[non_exhaustive]` で非破壊追加）
- `glyph_run_for_text` の後継 API（0026 の `shape()`）への置換。`pub(crate)` のため破壊的変更扱いにならない
- `Context::measure_text` の wrapper の要否は本 issue では扱わない。0026 内の記述で `Context::measure_text` への言及があるが、本 issue では `Font::measure_text` のみを追加する。`Context` 側 wrapper の追加可否は 0026 内または別 issue で議論する

## 関連

- 0001-enhance-font-module-maturity.md（メタ issue。fill_text リファクタリング担当の決定は 0001:94-100 を参照）
- 並行可能（同時着手しても影響しない）: 0021、0023
- 依存される（本 issue 完了後に着手）: 0024（`glyph_run_for_text` を `stroke_text` で使用）、0025（カーニング適用）、0026（`glyph_run_for_text` を `shape()` で置換）
