# global_alpha / fill_alpha / stroke_alpha を追加する

Created: 2026-04-09
Completed: 2026-04-09
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

## 解決方法

- `Context` と `ContextState` に `global_alpha` / `fill_alpha` / `stroke_alpha` (`f64`、初期値 1.0) を追加し、save/restore に組み込む
- setter は `clamp(0.0, 1.0)` で値域を制限する
- 内部ヘルパとして以下を追加:
  - `alpha_to_u8(f64) -> u8`: 実効アルファを 0..=255 に丸める
  - `scale_prgb32(prgb32, alpha) -> u32`: premultiplied ARGB の 4 チャネルを等倍スケール
  - `Context::effective_fill_alpha_u8()`: `global * fill` を 0..=255 で返す
- 適用箇所:
  - `fill_rect` 単色路: `prgb32 = scale_prgb32(self.fill_color_prgb32, eff_alpha)` で JIT に渡す
  - `fill_rect` のグラデ/パターン直接路: 実効アルファ != 255 のとき `fill_path` 経由 (span path) にフォールバックする (高速パスは alpha 適用未対応)
  - `fill_path` 単色路: 上記と同じくスケールして JIT cov に渡す
  - `fill_path` グラデ/パターン span 路: `fetch_span` 後に span_buf を `scale_prgb32` でループスケール
  - Linear gradient JIT cov 融合パス: 実効アルファ != 255 のときスキップして span path に流す
  - Radial gradient JIT row 融合パス (`fill_rect`): 同上 (フォールバック側で処理される)
  - SrcOver で実効アルファが 0 の場合は `fill_path` で早期 return
- `stroke_path` は fill 状態と一緒に `fill_alpha` も `stroke_alpha` に一時差し替えするため、`effective_fill_alpha_u8` がそのまま stroke 経路の実効アルファを返す
- 制限事項:
  - `blit_image_*` は今回スコープ外 (高速パス内部に手を入れる必要があるため)
  - JIT 高速パス (Linear cov / Radial row) は実効アルファ != 1 のとき span path にフォールバックするので alpha 使用時の性能は劣化する
- `tests/test_context.rs` に `alpha` モジュールを追加し、以下を検証:
  - 単位アルファでの出力が無設定と一致 (回帰)
  - `global_alpha == 0.0` で描画スキップ
  - `fill_alpha == 0.5` で premultiplied アルファが約 128
  - `global_alpha * fill_alpha` の乗算
  - グラデーションへの適用
  - `stroke_alpha` が fill に影響しない独立性
  - save/restore での復元
  - 範囲外値のクランプ
- `docs/BLEND2D.md` の該当行 3 箇所と課題一覧 11 を更新、`CHANGES.md` を更新
