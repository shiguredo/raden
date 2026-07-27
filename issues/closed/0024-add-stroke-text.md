# ストロークテキスト描画機能を追加する

- Priority: High
- Category: add
- Created: 2026-05-11
- Model: Kimi K2.7 Code
- Branch: feature/add-stroke-text
- Polished: 2026-06-20
- Completed: 2026-06-22

## 目的

`Context::stroke_text(x, y, &Font, &str)` を追加し、`fill_text` と同様に文字列の輪郭線 (ストローク) を描画できるようにする。タイトル表示やエフェクト用途で必要になる。文字列処理は 0022 で追加された `glyph_run_for_text` (`pub(crate)`) を流用し、`fill_text` と共通の advance 計算ロジックを共有する。

## Blend2D との対応

本 issue は `docs/BLEND2D.md` の以下の行を解消する (0001 メタ issue「Blend2D との対応表」で確定済み)。

- **L175 `BLContext::stroke_utf8_text(BLPointI/BLPoint, BLFont, const char*, size_t)`**: raden は `Context::stroke_text(x: f64, y: f64, font: &Font, text: &str)` で対応。Rust の `&str` は UTF-8 保証のため Blend2D の `BLTextEncoding::UTF8` 経路に直接対応。`BLTextEncoding::UTF16` / `UTF32` / `LATIN1` / `WCHAR` 経路は raden では対象外 (0001 メタ issue「Blend2D との対応表のスコープ外」参照)

スコープ外 (本 issue では対応しない):

- **L175 `stroke_utf16_text(...)` / `stroke_utf32_text(...)`**: raden は Rust `&str` UTF-8 入力に統一 (`fill_text` も同方針)
- **L176 `stroke_glyph_run(...)`**: raden は `BLGlyphBuffer` 相当 (`GlyphBuffer`) を 0026 で初出するため、本 issue では `stroke_glyph_run` 互換 API を追加しない。font モジュール安定化前の別 issue で検討
- **L171-174 のテーブルヘッダー** (Markdown table header): 状態列の変更対象ではない

完了 PR で `docs/BLEND2D.md` L175 の raden 列に `Context::stroke_text(x, y, &Font, &str)` を追記する。状態列 (`未実装`) の最終整理は 0001 メタ issue の close PR でまとめて行う。

## 現状

- `Context::fill_text(x: f64, y: f64, font: &Font, text: &str)` (`src/api/context.rs:1144-1169`) は実装済み。文字列処理ロジックがインライン展開されていたが、0022 で `Font::glyph_run_for_text` (`pub(crate)`) を導入し、`fill_text` を同ヘルパー経由に書き換える予定 (0022 のスコープ)
- `Context::stroke_path(&Path)` は実装済み (`src/api/context.rs` 内)。`stroke_options` (`stroke_width` / `stroke_start_cap` / `stroke_end_cap` / `stroke_join` / `stroke_miter_limit` / `stroke_dash_array` / `stroke_dash_offset`) も既存 API として揃っている
- `Context::stroke_text` は未実装。呼び出し側が `append_glyph_outline` でグリフのアウトラインを Path に追加し `stroke_path` で描画する迂回は可能だが、文字単位のループ・advance 計算・エラー黙殺パターンを呼び出し側が再実装することになり非効率

## 設計方針

- `Context::stroke_text(x: f64, y: f64, font: &Font, text: &str)` を追加し、`fill_text` と同じ構造 (`glyph_run_for_text` ループ → `append_glyph_outline` で `Path` 構築 → 描画) で実装する
- 描画は `fill_path(&path)` の代わりに `stroke_path(&path)` を呼ぶ
- `stroke_options` (幅・キャップ・ジョイン・dash 等) は `Context` の現在の設定を参照する (`stroke_path` が既にこれらを参照しているため `stroke_text` 側で個別に渡す必要はない)
- 0022 が提供する `Font::glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>)` を使う。要素型 `(glyph_id, advance)` のうち `advance` は **0024 時点ではカーニング非適用の単体値**、0025 で `stroke_text` 側にカーニング適用が後追い改修される (本 issue 隣接 issue への影響セクション参照)
- **`fill_text` 本体のリファクタリング (`glyph_run_for_text` を使う形への書き換え) は 0022 のスコープであり、本 issue では行わない** (0001 メタ issue「依存関係」セクションで確定)
- `append_glyph_outline` のエラーは `let _ =` で黙殺し、`glyph_id == 0` (cmap 未マッピング) はアウトライン構築をスキップして advance のみ進める (`fill_text` と同じ寛容方針)
- `Context` のテンポラリバッファ (`tmp_path` / `tmp_glyph_run`) を再利用する (0022 で `tmp_glyph_run: Vec<(u16, f64)>` が `Context` に追加される。本 issue では `fill_text` と同じ `mem::take` パターンで借用衝突を回避)

## 完了条件

### 追加される API

- `Context::stroke_text(x: f64, y: f64, font: &Font, text: &str)`

### 既存 API の挙動維持

- `fill_text` のシグネチャ・描画結果は本 issue では変えない (`fill_text` 内部実装の `glyph_run_for_text` 化リファクタリングは 0022 のスコープ)
- `Context` の `stroke_*` 系設定 (`stroke_width` / `stroke_start_cap` 等) は変更しない

### テスト

- 単体テストには「`stroke_text` の手動再実装との一致確認」を残す (PBT で実現困難なため)
- PBT で「`stroke_text` 実行後の描画ピクセル数 ≧ 1 (非ゼロピクセルが残る)」「`stroke_text` のカーソル前進量が `Font::measure_text(text).advance` と一致 (カーニング非適用前提)」等の不変条件を検証

### ドキュメント

- `docs/BLEND2D.md` L175 の raden 列に新規 API 名 (`stroke_text`) を追記
- `CHANGES.md` の `## develop` の `### misc` 直下に `ADD` 種別で 1 件 (`Context::stroke_text`) を追記

## 解決方法

### 1. `Context::stroke_text()` を追加

`src/api/context.rs` 内の `Context` に以下を追加 (0022 の `fill_text` 書き換え後のパターンに揃える):

```rust
pub fn stroke_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
    // tmp_path と tmp_glyph_run を同時に take する。
    // 後段の self.stroke_path(&path) は &mut self を要求するため、
    // self.tmp_path / self.tmp_glyph_run を借用したまま stroke_path を呼ぶと借用衝突する。
    // mem::take で所有を取り出すパターンは fill_text / stroke_path_buf 等の既存処理と同じ。
    let mut path = std::mem::take(&mut self.tmp_path);
    let mut run = std::mem::take(&mut self.tmp_glyph_run);
    path.clear();
    // run の clear は glyph_run_for_text 側で行うため呼び出し側では不要

    font.glyph_run_for_text(text, &mut run);

    let mut cursor_x = x;
    for &(glyph_id, advance) in run.iter() {
        // glyph_id == 0 (cmap 未マッピング) はアウトライン構築をスキップ。
        // ただし advance は加算してカーソルだけ進める (他のグリフ位置を維持するため)。
        // append_glyph_outline のエラーは let _ で黙殺 (fill_text と同じ寛容方針)。
        if glyph_id != 0 {
            let _ = font.append_glyph_outline(glyph_id, cursor_x, y, &mut path);
        }
        cursor_x += advance;
    }

    if !path.is_empty() {
        self.stroke_path(&path);
    }

    self.tmp_path = path;
    self.tmp_glyph_run = run;
}
```

`fill_text` との違いは `self.fill_path(&path)` → `self.stroke_path(&path)` の 1 行のみ。

### 2. 0022 の `fill_text` リファクタリング前後との関係

- **0022 が先 / 同時着手の場合**: `fill_text` が既に `glyph_run_for_text` 経由に書き換えられているため、本 issue の `stroke_text` も同じパターンで実装。`tmp_glyph_run` フィールドは 0022 で `Context` に追加済み
- **0024 が 0022 より先に着手される場合**: 本 issue の `stroke_text` 内で `Font::glyph_run_for_text` を呼ぶが、`tmp_glyph_run` フィールドが未追加。本 issue でも `Context` 構造体への `tmp_glyph_run: Vec<(u16, f64)>` 追加と `Context::new` での `Vec::new()` 初期化を実施する (0001 メタ issue「依存関係」セクションで 0024 は 0022 に依存と確定。原則として 0022 close 後に 0024 を着手するが、`tmp_glyph_run` フィールド責務は 0022 / 0024 のうち先着が担う)

### 3. CHANGES.md 更新

`CHANGES.md` の `## develop` の `### misc` 直下に追加 (`shiguredo-changelog` スキル準拠。`@<author>` は実装者の GitHub ユーザー名で置換):

```
- [ADD] `Context::stroke_text` を追加する
  - @<author>
```

### 4. BLEND2D.md 更新

`docs/BLEND2D.md` L175 の raden 列に `Context::stroke_text(x, y, &Font, &str) (UTF-8 のみ)` を追記する。状態列の最終整理は 0001 メタ issue の close PR でまとめて行う。

## 変更対象ファイル

| ファイル | 種別 | 内容 |
|---|---|---|
| `src/api/context.rs` | 既存編集 | `Context::stroke_text()` の追加。0024 が 0022 より先着の場合は `tmp_glyph_run` フィールド追加と `Context::new` での初期化も含む |
| `tests/test_context.rs` | 既存編集 | 単体テストの追加 (既存 `Context` テストパターンに揃える) |
| `pbt/tests/prop_font/main.rs` | 既存編集 | PBT 追加 (`stroke_text` のカーソル前進量が `measure_text` と一致する不変条件等)。0021 / 0022 / 0023 が原則ファイル先着担当だが、本 issue は順次ブロックで実施されるため通常はファイル存在前提 |
| `docs/BLEND2D.md` | 既存編集 | L175 の raden 列に新規 API 追記 |
| `CHANGES.md` | 既存編集 | `### misc` 直下に `ADD` 1 件追加 |
| `src/lib.rs` | 変更不要 | 既存 `pub use api::context::Context;` 経路でメソッド追加のみのため変更不要。新規型は導入しない |

## エッジケース

- 空文字列: `glyph_run_for_text` が空バッファを返し、ループ 0 回、`path` 空のまま `stroke_path` 呼び出しなし
- `size == 0` の `Font`: 全 `glyph_advance` が 0.0、`append_glyph_outline` のパスは空に近いがエラーで黙殺。カーソル前進なし
- `glyph_id == 0` (cmap 未マッピング): アウトライン構築をスキップし advance のみ進める (`fill_text` と同じ挙動)
- `glyph_id >= num_glyphs` (壊れた cmap): `Font::glyph_advance` が 0.0 を返すため advance に 0.0 が加算される
- `stroke_width` が 0 / 負値: `stroke_path` 側の既存挙動に従う (本 issue のスコープ外)
- 改行・制御文字: 通常文字と同様に処理 (`fill_text` と同じ。複数行レイアウトは別 issue)

## 隣接 issue への影響

- **0025 への申し送り (重要)**: 0025 (font kerning) で `fill_text` / `measure_text` / `stroke_text` の 3 者にカーニングを適用する。0024 で書かれた `stroke_text` は 0025 完了時にカーニング適用へ改修される。**この後追い改修は 0025 のスコープに含む** (0001 メタ issue「依存関係」セクション、0025 polish 時に 0025 解決方法へ反映される)。本 issue は「カーニング非適用の単体 advance を `glyph_run_for_text` 経由で使う」状態で close する
- **0026 への申し送り**: 0026 で `glyph_run_for_text` が `Font::shape()` に置換され、本 issue の `stroke_text` 内の `font.glyph_run_for_text(text, &mut run)` 呼び出しが `font.shape(text, ...)` に置換される (この置換責務は 0026 のスコープ)。本 issue のテストは 0026 で `Font::from_face` シグネチャ拡張時に書き換え対象になる (書き換え責務も 0026 のスコープ)
- **0022 との関係**: 本 issue は `glyph_run_for_text` (`pub(crate)`) に依存。0022 が先着の場合は `tmp_glyph_run` が `Context` に追加済み、本 issue では追加不要。0024 が先着の場合は本 issue で追加

## テスト戦略

### 単体テスト (`tests/test_context.rs`)

既存 `Context` テストパターン (Arial 不在時スキップ、画像バッファ検証等) に揃える。`tests/test_font.rs:5-13` 相当の `load_arial()` を再利用または再定義する (どちらでも可、既存テストパターンに合わせる)。

- `context_stroke_text_renders`: Arial 環境で `stroke_text("Hello")` 実行後に非ゼロピクセルが存在することを検証
- `context_stroke_text_matches_manual`: `stroke_text("ABC")` の描画結果が、`append_glyph_outline` + `cursor_x += advance` + `stroke_path` を手動で組み合わせた結果と画像バッファレベルで一致することを検証 (内部実装の正当性確認)

### PBT (`pbt/tests/prop_font/main.rs`)

`Context` を直接使うため `Path::control_box` 等の比較は難しい。本 issue では `Font::glyph_run_for_text` ベースで以下を検証する (`stroke_text` 自体は描画副作用のため PBT が薄くなる):

- **カーソル前進量一致**: 任意の Arial cmap 確実な ASCII 文字列に対し、`stroke_text` 後の最終 cursor_x (本 issue の内部実装では `x + sum(advances)`) が `Font::measure_text(text).advance + x` と一致 (カーニング非適用前提)。実装上は `stroke_text` のテストハーネスとして cursor_x を返すヘルパーを `#[cfg(test)]` で公開する (本番 API は変更しない) か、または `Font::measure_text` の結果と `glyph_run_for_text` の総和の一致 (0022 PBT で既に検証) を引用する形で本 PBT は省略する

省略する場合は単体テストの `context_stroke_text_matches_manual` で代替する。

### 既存テストへの影響

- 0022 で `fill_text` がリファクタリングされる影響は本 issue では受けない (本 issue は `stroke_text` 追加のみ)
- `Context` の既存 `stroke_path` テストは変更不要

## 関連

- `0001-enhance-font-module-maturity.md` (メタ issue。Blend2D 対応マトリクスで本 issue が担当する L175、依存関係 (0024 → 0022)、0025 後追い改修の根拠)
- 依存: `0022-add-text-measurement.md` (`glyph_run_for_text` の実装、`tmp_glyph_run` フィールド)
- 依存される: `0025-add-font-kerning.md` (本 issue で書かれた `stroke_text` にカーニング適用を後追い改修。改修責務は 0025 のスコープ)、`0026-add-opentype-basic-shaping.md` (`glyph_run_for_text` を `shape()` で置換、テスト書き換え)
- **着手前提**: 0022 が close 済 (`glyph_run_for_text` 利用のため)。0001 メタ issue の前提 concrete issue 群のうち「テスト用フォント選定」「Compound Glyph point-matching 実装」が完了
- **close 前提**: 上記のみ
