# ストロークテキスト描画機能を追加する

- Priority: High
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-stroke-text

## 目的

`fill_text` と同様に、テキストの輪郭線（ストローク）を描画できるようにする。タイトル表示やエフェクト用途で必要になる。

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
   - `fill_text` と同様に文字列を走査し、`map_char_to_glyph` → `append_glyph_outline` で Path を構築
   - 構築した Path を `stroke_path` で描画
   - `stroke_options`（幅・キャップ・ジョイン等）は `Context` の現在の設定を参照する
3. `fill_text` と共通する文字列処理ロジックを抽出し、内部メソッドとして共通化する
4. 単体テストで「stroke_text 後の画像に輪郭線が描画されることを目視確認するテスト」を追加する（ピクセルベースの厳密検証は困難なため、アウトライン一致で検証）

## 変更対象ファイル

- `src/api/context.rs`: `stroke_text()` の追加、文字列処理ロジックの共通化
- `tests/test_context.rs`: 単体テストの追加
- `docs/BLEND2D.md`: Context API セクションの更新

## 関連

- `docs/BLEND2D.md` Context API セクションの更新
