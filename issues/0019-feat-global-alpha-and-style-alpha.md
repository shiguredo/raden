# global_alpha / fill_alpha / stroke_alpha を追加する

Created: 2026-04-09
Model: Opus 4.6

## 概要

Blend2D の `global_alpha` / `fill_alpha` / `stroke_alpha` がいずれも未実装。フェードイン/アウトや半透明合成の表現に必須の API。

## 根拠

- ダミー映像でのフェード演出は最頻出の表現の一つで、現状は色の `a` を毎回書き換える必要がある
- グローバルアルファは合成パイプライン全体に影響するため、図形 API を増やす前 (issue 0016〜0018 着手前) に基盤として入れた方が後の手戻りが少ない
- fill / stroke 個別アルファは、同じ色値を共有しつつ透明度だけ独立で制御できる利便性がある

## 想定スコープ

- `Context` に `global_alpha()` / `set_global_alpha(f64)` / `fill_alpha()` / `set_fill_alpha(f64)` / `stroke_alpha()` / `set_stroke_alpha(f64)` を追加
- 値域 `[0.0, 1.0]` のクランプを Blend2D に合わせる
- 適用順: 最終的なカバレッジ ×（fill_alpha or stroke_alpha）× global_alpha
- 単色 fill / グラデ / パターン / blit のすべての経路に反映
- save/restore でスタックされること
- PBT: `global_alpha == 1.0` かつ `*_alpha == 1.0` で既存の出力と一致、`global_alpha == 0.0` で何も描画されない
- 単体テスト: アルファ値の境界、save/restore の挙動
- `docs/BLEND2D.md` および `CHANGES.md` の更新

## 注意点

- グラデーション fill_rect 高速パス、パターン fill_rect 高速パスでも忘れず適用する
- 既存ベンチに対する性能影響を計測する
