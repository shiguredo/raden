# 矩形クリッピング API を追加する

Created: 2026-03-28
Completed: 2026-03-28
Model: Opus 4.6

## 概要

Context に矩形クリッピング API を追加する。現状は画像境界での内部クリップのみ存在し、ユーザーが描画領域を制限する手段がない。

## 根拠

- Blend2D API 準拠: `clip_to_rect` / `restore_clipping` は Blend2D の基本 API
- 実用性: UI モック（ヘッダー/サイドバー内描画）、タイル描画、ビューポート制限で必要
- 実装コストが低い: 既存の内部 `clip_rect()` メソッドを拡張するだけで基本動作は実現可能

## Blend2D の対応 API

```cpp
BLResult clip_to_rect(const BLRectI& rect);
BLResult clip_to_rect(const BLRect& rect);
BLResult clip_to_rect(double x, double y, double w, double h);
BLResult restore_clipping();
```

- save/restore でクリップ状態も保存・復元される
- 複数回の `clip_to_rect` は積集合（クリップ領域が縮小のみ）
- Blend2D でもパスクリッピング（任意形状）は未実装

## 追加する API

```rust
pub fn clip_to_rect(&mut self, rect: &Rect)
pub fn restore_clipping(&mut self)
```

- `clip_to_rect`: ユーザー指定矩形と現在のクリップ領域の積集合を新しいクリップ領域とする
- `restore_clipping`: メタクリップ（画像境界）にリセットする
- save/restore 対象に含める（ContextState に `clip_box` を追加）

## 実装方針

1. まず動くものを作る（素朴な実装）
2. ベンチマークを取りながら必要に応じて JIT/SIMD で最適化する

## スコープ外

- パスクリッピング（任意形状でのクリップ）: Blend2D でも未実装であり、実装コストが桁違いに高い

## 解決方法

- `Context` に `meta_clip_box`（画像境界、不変）と `clip_box`（現在のクリップ領域）を追加
- `clip_to_rect(&Rect)`: 現在の `clip_box` と指定矩形の積集合を新しい `clip_box` とする
- `restore_clipping()`: `clip_box` を `meta_clip_box` にリセット
- `ContextState` に `clip_box` を追加し、`save()` / `restore()` で保存・復元対象にする
- 内部の `clip_rect()` と `fill_path()` のクリップ計算を画像境界直接参照から `clip_box` 参照に変更
