# ストロークのダッシュ線 (dash_array / dash_offset) を実装する

Created: 2026-03-28
Completed: 2026-03-28
Model: Opus 4.6

## 概要

Blend2D の dash_array / dash_offset に相当するストロークの点線・破線描画機能が raden に存在しない。

## 根拠

- docs/BLEND2D.md の課題一覧に記載されている
- ストロークの点線・破線はダミー映像生成の UI 表現 (枠線、区切り線) に必要

## 解決方法

- StrokeOptions に dash_array (Vec<f64>) と dash_offset (f64) を追加
- apply_dash: 平坦化済みサブパスをダッシュパターンで分断するプリプロセス
- SVG 準拠で奇数要素のパターンは 2 回繰り返す
- Context に set_stroke_dash_array / set_stroke_dash_offset API を追加
- save/restore でダッシュ設定を保存・復元する
