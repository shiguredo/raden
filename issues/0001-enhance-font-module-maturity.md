# font モジュールの成熟度を正式リリースに向けて改善する

Created: 2026-03-28
Model: Opus 4.6

## 概要

font モジュールは TrueType アウトライン (glyf/loca) の最小限サポートを提供しているが、正式リリースに向けて以下の課題がある。

## 現状

- TrueType アウトライン (Simple Glyph / Compound Glyph) の描画に対応
- cmap Format 4 (BMP) / Format 12 (Full Unicode) に対応
- head, maxp, hhea, hmtx, loca, cmap, glyf テーブルのパースに対応
- TTC (TrueType Collection) に対応
- 公開 API: `FontData`, `FontFace`, `Font`, `FontError`

## 課題

### テストがない

font モジュールには PBT も単体テストも存在しない。テーブルパーサとグリフアウトライン変換の正しさが検証されていない。

- tables.rs: バイトパースのラウンドトリップや不正入力に対するエラーハンドリングの PBT が必要
- glyph.rs: Simple/Compound Glyph のアウトライン変換の正しさを検証するテストが必要
- cmap: Format 4 / Format 12 のルックアップ精度を検証するテストが必要

### OpenType (CFF) アウトラインに未対応

現状は TrueType アウトライン (glyf テーブル) のみ対応。CFF/CFF2 ベースの OpenType フォントは読み込めない。Blend2D は両方に対応している。

### カーニング (kern / GPOS) に未対応

文字間隔の調整ができないため、テキスト描画の品質が低い。

### テキストレイアウト API がない

現在はグリフ単位の低レベル API のみ。文字列を渡して描画する高レベル API (`fill_text` 相当) が存在しない。

### Fuzzing ターゲットがない

フォントファイルは外部入力であり、不正なデータに対するクラッシュ耐性を fuzzing で検証する必要がある。

## 対応方針

1. テストの追加 (PBT + 単体テスト + fuzzing)
2. テキストレイアウト API の追加
3. カーニング対応
4. CFF 対応は優先度低 (TrueType アウトラインでカバーできる範囲が広いため)
