# Font にスケール済みメトリクス取得を追加する

Created: 2026-05-11
Model: Kimi K2.6

## 概要

現状 `FontFace` は `units_per_em()` / `ascent()` / `descent()` といったデザインメトリクスを提供しているが、`Font`（サイズ指定済みインスタンス）からはスケール済みメトリクスを取得できない。テキスト描画時のベースライン計算や行間制御には `Font` レベルでのスケール済みメトリクスが必須である。

## 根拠

`fill_text(x, y, &font, text)` を使ってテキストを描画する際、`y` は通常ベースライン位置を指す。ベースライン位置を正しく計算するには `Font::ascent()` / `Font::descent()` / `Font::line_gap()` といったスケール済み値が必要である。現状は `FontFace::ascent() * font.scale()` のように呼び出し側で計算する必要があり、API の一貫性が欠けている。

## 現状の問題

- `Font::scale()` は公開されているが、呼び出し側が毎回 `face.ascent() * font.scale()` を計算する必要がある
- `line_gap()` / `cap_height()` / `x_height()` 等が `FontFace` にも `Font` にも未実装
- テキストの行高を計算する際の情報が不足している

## 対応内容

1. `Font` に以下のメソッドを追加する
   - `ascent() -> f64`
   - `descent() -> f64`
   - `line_gap() -> f64`
   - `cap_height() -> f64`（存在する場合）
   - `x_height() -> f64`（存在する場合）
2. これらは `FontFace` のデザインメトリクスに `scale()` を乗じた値を返す
3. `FontFace` に不足しているメトリクス（`line_gap` / `cap_height` / `x_height` 等）のパースを追加する
4. PBT で「スケール済み値 ≒ デザイン値 × scale」の関係を検証する

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
- テスト: `tests/test_font.rs` / `pbt/tests/prop_font.rs`
