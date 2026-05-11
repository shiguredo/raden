# テキストサイズ計測機能を追加する

Created: 2026-05-11
Model: Kimi K2.6

## 概要

`fill_text` によるテキスト描画は可能だが、文字列全体の幅・高さを事前に計測する API がない。テキストを画面中央に配置する際など、描画前にサイズを知る必要がある。

## 根拠

ダミー映像生成において、テキストを「画面中央に配置」「右寄せ」「指定矩形内に収める」といったレイアウトは必須である。これらを行うには描画前にテキストの `bounding_box` や `advance` を取得できる必要がある。Blend2D では `get_text_metrics()` でこれを提供している。

## 現状の問題

- `fill_text` は描画のみを行い、サイズ情報を返さない
- `glyph_advance(glyph_id)` は個別グリフの advance を返すが、文字列全体の計測には呼び出し側がループする必要がある
- カーニングが未実装のため、単純な advance の足し合わせでは実際の描画幅と一致しない可能性がある

## 対応内容

1. `Font` に以下のメソッドを追加する
   - `measure_text(text: &str) -> TextMetrics`
2. `TextMetrics` 構造体を定義する
   - `advance: f64`（文字列全体の進行幅）
   - `bounding_box: Rect`（文字列全体のバウンディングボックス）
   - `ascent: f64` / `descent: f64`（行高計算用）
3. 実装は `map_char_to_glyph` → `glyph_advance` のループで計算（カーニング未実装時は advance の累積）
4. 将来カーニングが実装された際は `measure_text` 内でカーニングを適用する
5. PBT で「measure_text の advance ≒ 各 glyph_advance の総和」の関係を検証する

## 関連

- `docs/BLEND2D.md` Font API セクションの更新
- テスト: `tests/test_font.rs` / `pbt/tests/prop_font.rs`
- 依存: 0021-add-font-scaled-metrics（行高計算に必要）
