# Blend2D の tiger デモを examples に追加する

Created: 2026-03-28
Completed: 2026-03-28
Model: Opus 4.6

## 概要

Blend2D のデモとして有名な tiger (AmanithVG 由来のベクターグラフィックスデータ) を raden の examples に追加する。複雑なパス描画、フィル、ストローク、変換行列の組み合わせを実証するサンプルとして最適である。

## 根拠

現在の examples は基本図形やグラデーションなど単純な描画に留まっている。tiger は 240 以上のパスで構成される複雑なベクターグラフィックスであり、以下を実証できる:

- 大量の cubic_to によるパス描画の性能
- FillRule (NonZero / EvenOdd) の使い分け
- StrokeCap / StrokeJoin / miter_limit の多様な組み合わせ
- スケールと平行移動による画面中央配置
- save / restore による状態管理

## 対応内容

### ファイル構成

- `examples/tiger/main.rs` — パース、描画、ウィンドウ表示
- `examples/tiger/tiger_data.rs` — `bl_demo_tiger.h` から移植したコマンド / ポイントデータ (4142 コマンド、16988 ポイント)

### Blend2D デモとの違い

- 回転アニメーションは実装しなかった
- バウンディングボックスはハードコードせずパスの頂点から動的に計算する
- 変換チェーンは Blend2D のものをそのまま使わず、raden の行ベクトル規約に合わせて書き直した

### 実装の要点

- tiger データのコマンド配列とポイント配列をパースし、`TigerPath` 構造体 (Path + fill/stroke 情報) に変換する
- Y 座標は元データから `HEIGHT - y` で反転済み
- バウンディングボックスはハードコードせず、全パスの頂点から動的に計算する
- 変換順序は行ベクトル規約に従い `translate(-cx, -cy) → scale(s) → translate(cw/2, ch/2)` の順で適用する
- 静止画をウィンドウに表示する (ESC で終了)

## 解決方法

`examples/tiger/` ディレクトリを作成し、`bl_demo_tiger.h` のデータを Python スクリプトで Rust に変換した。パースロジックは Blend2D の `bl_demo_tiger.cpp` の init 関数を参考にしつつ、raden の API で素直に実装した。
