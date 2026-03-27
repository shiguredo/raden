# 画像パターン塗りつぶしを実装する

Created: 2026-03-28
Model: Opus 4.6

## 概要

Blend2D の `BLPattern` に相当する画像パターン塗りつぶしが raden に存在しない。画像をタイル状に繰り返してパスを塗りつぶす機能を実装する必要がある。

## 根拠

- docs/BLEND2D.md の課題一覧でグラデーションと並んで記載されている
- パターンはテクスチャ付き背景やタイル模様の生成に必要
- `blit_image` が未実装の現状では、パターンが画像を描画面に配置する唯一の手段となりうる
- Blend2D API 準拠を目指す以上、`BLPattern` 相当の機能は必要

## 現状

- `set_fill_style()` / `set_stroke_style()` は `Rgba32` (単色) のみ受け付ける
- `FetchType` は `Solid = 0` しか定義されていない
- `Image` 型は存在するが、パターンソースとして使う仕組みがない

## Blend2D の実装

### 型定義

- **BLPattern**: image (BLImage), area (BLRectI), transform (BLMatrix2D), extend_mode (BLExtendMode)
- **BLPatternQuality**: Nearest (0), Bilinear (1)
- **BLExtendMode**: グラデーションと共通の Pad/Repeat/Reflect に加え、X/Y 独立モード (計 9 種類)

### パイプライン側の FetchData

- **Simple** (並進のみ): tx, ty, rx, ry (リピート幅/高さ), バイリニア重み, 垂直エクステンドデータ
- **Affine** (アフィン変換): xx, xy, yx, yy (変換ステップ), tx, ty (オフセット), min/max 境界, タイル幅/高さ

### 処理の流れ

1. パターンのソース画像とエリアを設定する
2. 変換マトリックスと拡張モードを設定する
3. パイプライン実行時、各ピクセル座標をソース画像座標に変換する
4. 拡張モードに従って座標をラップし、ソースピクセルを取得する
5. 補間品質に応じて Nearest または Bilinear で色を決定する

## 対応方針

### 1. Pattern 型の実装

- `Pattern` 構造体: image (Image 参照), area (領域), extend_mode, transform
- `ExtendMode` は 0002 (グラデーション) で実装する Pad/Repeat/Reflect を共用する
- パターン専用の X/Y 独立拡張モードは後回しにしてよい

### 2. Style の汎用化

- `set_fill_style()` / `set_stroke_style()` が `Pattern` も受け取れるようにする
- `FetchType` に `PatternSimple`, `PatternAffine` を追加する
- 0002 (グラデーション) で Style の汎用化を先に行っていれば、追加は容易

### 3. パイプラインの拡張

- Simple (並進) パターンの fetch 関数を Cranelift JIT で実装する
- まずは Nearest 補間のみ対応し、Bilinear は後続で追加する
- Affine パターンは優先度低 (並進で多くのユースケースをカバーできる)

### 4. テスト

- PBT: パターン座標のラップ処理 (Repeat/Reflect) のプロパティ検証
- 単体テスト: 既知のソース画像に対するピクセル取得の正しさ

## 前提条件

- 0002 (グラデーション) の基盤実装 (ExtendMode, Style 汎用化, FetchType 拡張, パイプラインシグネチャ変更) が先に完了していること

## 優先順位

1. Simple + Nearest (並進・最近傍 — 最小実装)
2. Simple + Bilinear (並進・バイリニア補間)
3. Affine + Nearest (アフィン変換)
4. X/Y 独立拡張モード
