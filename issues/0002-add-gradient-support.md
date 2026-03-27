# グラデーション (Linear / Radial / Conic) を実装する

Created: 2026-03-28
Model: Opus 4.6

## 概要

Blend2D の大きな特徴であるグラデーション塗りつぶしが raden に存在しない。Linear, Radial, Conic の 3 種類のグラデーションと、それらを支える基盤 (色停止点、拡張モード、LUT) を実装する必要がある。

## 根拠

- docs/BLEND2D.md の課題一覧で最優先として記載されている
- グラデーションはダミー映像生成において背景・オーバーレイ・UI 要素の表現に不可欠
- Blend2D API 準拠を目指す以上、`BLGradient` 相当の機能がないと API カバレッジが大きく不足する

## 現状

- `set_fill_style()` / `set_stroke_style()` は `Rgba32` (単色) のみ受け付ける
- `FetchType` は `Solid = 0` しか定義されていない
- パイプライン関数シグネチャは `src_solid: u32` で単色前提になっている

## Blend2D の実装

### 型定義

- **BLGradientType**: Linear (0), Radial (1), Conic (2)
- **BLGradientStop**: `offset: f64` (0.0-1.0) + `rgba: BLRgba64`
- **BLExtendMode**: Pad (0), Repeat (1), Reflect (2) — グラデーションはこの 3 つのみ
- **BLGradientQuality**: Nearest (0), Smooth (1), Dither (2)

### グラデーション値

- **Linear**: x0, y0, x1, y1 (開始点と終了点)
- **Radial**: x0, y0, x1, y1, r0, r1 (中心、焦点、中心半径、焦点半径)
- **Conic**: x0, y0, angle, repeat (中心、角度、リピート係数)

### LUT (ルックアップテーブル) キャッシュ

Blend2D はグラデーションの色停止点から PRGB32 の LUT を事前に生成しキャッシュする。LUT サイズは色停止点の分布に応じて 256/512/1024 から選択される。パイプライン実行時は座標から LUT インデックスを計算してピクセル値を取得する。

### パイプライン側の FetchData

各グラデーション種別ごとに座標→LUT インデックス変換に必要な係数を事前計算して `FetchData::Gradient` に格納する:

- **Linear**: pt (初期オフセット), dt (X ステップ), dy (Y ステップ), maxi, rori (Repeat/Reflect マスク)
- **Radial**: tx, ty, yx, yy, amul4, inv2a 等の 2 次方程式係数
- **Conic**: tx, ty, yx, yy, atan 近似係数, 角度オフセット

## 対応方針

### 1. 基盤の実装

- `GradientStop` 構造体: `offset: f64`, `color: Rgba32`
- `ExtendMode` 列挙型: Pad, Repeat, Reflect
- `GradientType` 列挙型: Linear, Radial, Conic
- `Gradient` 構造体: 種別、値、停止点リスト、拡張モード、変換マトリックス
- LUT 生成: 停止点から PRGB32 配列を補間生成する

### 2. Style の汎用化

- `set_fill_style()` / `set_stroke_style()` が `Rgba32` だけでなく `Gradient` も受け取れるようにする
- `FetchType` に `LinearGradient`, `RadialGradient`, `ConicGradient` を追加する

### 3. パイプラインの拡張

- パイプライン関数シグネチャを拡張して FetchData ポインタを受け取れるようにする
- 各グラデーション種別の fetch 関数を Cranelift JIT で実装する
- Linear → Radial → Conic の順で実装する (Linear が最も単純)

### 4. テスト

- PBT: GradientStop のソート・正規化のラウンドトリップ、LUT 生成の境界条件
- 単体テスト: 各グラデーション種別の座標→色変換の正しさ

## 優先順位

1. Linear Gradient (最も基本的で実装が単純)
2. Radial Gradient (2 次方程式の係数計算が必要)
3. Conic Gradient (atan 近似が必要)
