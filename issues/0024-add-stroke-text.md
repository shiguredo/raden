# ストロークテキスト描画機能を追加する

Created: 2026-05-11
Model: Kimi K2.6

## 概要

`fill_text` による塗りつぶしテキスト描画は実装済みだが、輪郭線（ストローク）テキスト描画機能が未実装である。タイトル表示やエフェクト用途で必要になる。

## 根拠

ダミー映像生成において、文字の輪郭線を描画する需求は高い。Blend2D では `stroke_utf8_text()` / `stroke_utf16_text()` / `stroke_utf32_text()` を提供しており、raden でも最低限 `stroke_text` が必要である。

## 現状の問題

- `Context` に `fill_text(x, y, &Font, &str)` はあるが `stroke_text` がない
- `append_glyph_outline` でグリフのアウトラインを Path に追加し、`stroke_path` で描画するという迂回は可能だが、呼び出し側が文字単位のループと advance 計算を行う必要があり非効率
- `stroke_text` は `fill_text` と同様に内部でカーニングやシェーピングを適用すべきである

## 対応内容

1. `Context` に以下のメソッドを追加する
   - `stroke_text(x: f64, y: f64, font: &Font, text: &str)`
2. 実装方針
   - `fill_text` と同様に文字列を走査し、`map_char_to_glyph` → `append_glyph_outline` で Path を構築
   - 構築した Path を `stroke_path` で描画
   - `stroke_options`（幅・キャップ・ジョイン等）は `Context` の現在の設定を参照する
3. `fill_text` と共通する文字列処理ロジックを抽出し、内部メソッドとして共通化する
4. 単体テストで「stroke_text 後の画像に輪郭線が描画されることを目視確認するテスト」を追加する（ピクセルベースの厳密検証は困難なため、アウトライン一致で検証）

## 関連

- `docs/BLEND2D.md` Context API セクションの更新
- テスト: `tests/test_context.rs`
