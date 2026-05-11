# テキストサイズ計測機能を追加する

- Priority: High
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-text-measurement

## 目的

文字列全体の描画幅を事前に計測できるようにし、テキストの中央配置や右寄せ等のレイアウトを可能にする。

## 現状

- `fill_text` は描画のみを行い、サイズ情報を返さない
- `glyph_advance(glyph_id)` は個別グリフの advance を返すが、文字列全体の計測には呼び出し側がループする必要がある
- カーニングが未実装のため、単純な advance の足し合わせでは実際の描画幅と一致しない可能性がある（この issue のスコープ外）

## 設計方針

- `Font` に `measure_text()` メソッドを追加し、文字列全体の advance を返す
- 内部実装は `map_char_to_glyph` → `glyph_advance` のループとし、`fill_text` と同一のロジックを使用する
- `bounding_box` は現時点では含めず、0023 でグリフ境界ボックスを実装後に拡張する

## 完了条件

- `Font::measure_text(text)` で文字列全体の advance が取得できる
- `fill_text` と同一の advance 計算ロジックを使用し、整合性が保たれている
- 単体テストと PBT で正しさを検証している

## 解決方法

1. `Font` に文字列処理の共通ヘルパーを追加する
   - `glyph_run_for_text(text: &str, buf: &mut Vec<(u16, f64)>)`（glyph_id と advance のペア列を呼び出し側のバッファに書き込む）
   - これを `measure_text`、`fill_text`（0024 で共通化）、`stroke_text`（0024）で共用する
   - 呼び出し側でバッファを再利用し、ヒープ割り当てを回避する
2. `Font` に以下のメソッドを追加する
   - `measure_text(text: &str) -> TextMetrics`
3. `TextMetrics` 構造体を `src/font/mod.rs` に定義する
   - `advance: f64`（文字列全体の進行幅）
4. 実装方針
   - `glyph_run_for_text` でグリフ列を取得し、advance の総和を `TextMetrics` に格納
   - `glyph_id == 0`（未定義グリフ）も `glyph_advance(0)` の値を加算し、`fill_text` と同一挙動を維持
5. エッジケース
   - 空文字列: `advance=0.0`
   - `size=0`: `advance=0.0`
6. `bounding_box` は現時点では含めない（0023 でグリフ境界ボックスを実装後に追加）

## 変更対象ファイル

- `src/font/mod.rs`: `glyph_run_for_text()` / `measure_text()` / `TextMetrics` の追加
- `src/api/context.rs`: `fill_text` を `glyph_run_for_text` を使う形に変更（0024 と競合するため、0022 実装時に対応）
- `tests/test_font.rs`: 単体テストの追加
- `pbt/tests/prop_font/main.rs`: PBT の追加
- `docs/BLEND2D.md`: Font API セクションの更新

## テスト戦略

- 単体テスト: 空文字列、単一文字、複数文字の境界値を網羅
- PBT: `measure_text(text).advance` と `text.chars().map(|c| font.glyph_advance(font.map_char_to_glyph(c))).sum::<f64>()` の一致を検証

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
