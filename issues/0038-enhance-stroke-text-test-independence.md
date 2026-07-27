# stroke_text の matches_manual_path テストに独立した検証経路を追加する

- Priority: Low
- Category: refactor
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/refactor-stroke-text-test-independence
- Polished: {YYYY-MM-DD}

## 目的

`tests/test_context.rs` の `stroke_text::matches_manual_path` がトートロジー的検証 (`stroke_text` 内部実装と同じロジックを手書きで再現してピクセル一致を確認している) になっている問題を解消し、独立した検証経路 (例: `Font::measure_text` 経由の cursor_x 一致、または `fill_text` との Path 一致) を追加して検出力を上げる。

## 優先度根拠

Low。実害はないが、PBT 的検出力が弱いため、内部実装のリファクタリングで問題が発生しても本テストが検知しない可能性がある。0025 / 0026 で `glyph_run_for_text` → `shape()` 置換やカーニング適用が入ったとき、検証側がついていけるよう改善しておきたい。

## 現状

`tests/test_context.rs:537-589` の `matches_manual_path` テスト:

```rust
let mut path = Path::new();
let mut cursor_x = baseline_x;
for ch in "ABC".chars() {
    let glyph_id = font.map_char_to_glyph(ch);
    if glyph_id != 0 {
        let _ = font.append_glyph_outline(glyph_id, cursor_x, baseline_y, &mut path);
    }
    cursor_x += font.glyph_advance(glyph_id);
}
```

これは `Font::glyph_run_for_text` (`src/font/mod.rs:249-256`) の中身を素朴に展開したものであり、`stroke_text` 側も同じ `glyph_run_for_text` 経由で動作するため、両者の一致は「`glyph_run_for_text` が変わらない限り常に成立」する trivial な不変条件になる。

## 設計方針

以下のいずれかを選ぶ。

1. `Font::measure_text` 経由の独立検証
   - 任意の文字列 `text` に対し「`stroke_text` 完了時の cursor_x (= 最終 advance 累積) が `Font::measure_text(text).advance + x` と一致する」ことを検証。
   - cursor_x を観測するには `#[cfg(test)]` ハーネス、または `Context::build_text_path` (issue 0036 と関連) を `pub(crate)` 化して PBT 検証する経路が必要。
2. `Context::fill_text` との Path 一致検証
   - 同じ文字列に対し `fill_text` と `stroke_text` の内部 Path が一致し、描画関数だけ差し替わっていることを確認。
   - 0036 (`build_text_path` 共通化) が先行する前提なら、共通ヘルパーから返る Path を直接アサートできる。
3. PBT 追加
   - `pbt/tests/prop_font/main.rs` (または `tests/prop_context.rs` のような新規ファイル) に PBT を追加。
   - 例: `prop_stroke_text_advance_matches_measure_text` (`Context::stroke_text` 後の状態が `measure_text` と整合する不変条件)。

0036 との連携が現実的。0036 で `build_text_path` を `pub(crate)` 化したあと、本 issue で PBT を追加する流れが自然。

## 完了条件

- `stroke_text` の動作を `Font::glyph_run_for_text` の素朴展開とは独立した経路で検証するテスト or PBT が追加されている。
- 既存テスト (`matches_manual_path` を含む 5 件) は変更しないか、もしくは独立検証に置き換える。

## 解決方法

polish 段階で確定する。
