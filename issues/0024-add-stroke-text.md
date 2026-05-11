# ストロークテキスト描画機能を追加する

- Priority: High
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-stroke-text

## 目的

`fill_text` と同様に、テキストの輪郭線（ストローク）を描画できるようにする。タイトル表示やエフェクト用途 で必要になる。

## 現状

- `Context` に `fill_text(x, y, &Font, &str)` はあるが `stroke_text` がない
- `append_glyph_outline` でグリフのアウトラインを Path に追加し、`stroke_path` で描画するという迂回は可能だが、呼び出し側が文字単位のループと advance 計算を行う必要があり非効率

## 設計方針

- `fill_text` と同様に文字列を走査し、`map_char_to_glyph` → `append_glyph_outline` で Path を構築
- 構築した Path を `stroke_path` で描画
- `stroke_options`（幅・キャップ・ジョイン等）は `Context` の現在の設定を参照する
- `fill_text` と共通する文字列処理ロジックを抽出し、内部メソッドとして共通化する

## 完了条件

- `Context::stroke_text(x, y, &font, text)` でテキストの輪郭線が描画できる
- `stroke_options`（幅・キャップ・ジョイン等）は `Context` の現在の設定を参照する
- 単体テストで正しさを検証している

## 解決方法

1. `Context` に以下のメソッドを追加する
   - `stroke_text(x: f64, y: f64, font: &Font, text: &str)`
2. 実装方針
   - `fill_text` (`src/api/context.rs:1144-1169`) と同様に文字列を走査し、`map_char_to_glyph` → `append_glyph_outline` で Path を構築
   - 構築した Path を `stroke_path` で描画
   - `stroke_options`（幅・キャップ・ジョイン等）は `Context` の現在の設定を参照する (`stroke_width`, `stroke_start_cap`, `stroke_end_cap`, `stroke_join`, `stroke_miter_limit`, `stroke_dash_array`, `stroke_dash_offset`)
3. 文字列処理は 0022 で追加した `glyph_run_for_text` を使用する
4. `fill_text` の `glyph_run_for_text` を使ったリファクタリングは 0022 で実施する (0024 では `stroke_text` の追加のみ)
5. 単体テストでは `stroke_text` 実行後に輪郭線が描画されることを検証する
   - `stroke_text` の結果が、手動で `append_glyph_outline` + `stroke_path` を組み合わせた場合と一致することを検証する
6. `fill_text` と同様に `append_glyph_outline` のエラーは無視してスキップする

## 変更対象ファイル

- `src/api/context.rs`: `stroke_text()` の追加、`fill_text` を `glyph_run_for_text` を使う形に変更
- `tests/test_context.rs`: 単体テストの追加

## 依存

- 0022-add-text-measurement（`glyph_run_for_text` の実装）

## 関連

- 0001-enhance-font-module-maturity.md
