# font モジュールの成熟度を正式リリースに向けて改善する (メタ issue)

- Priority: High
- Created: 2026-03-28
- Model: Opus 4.6

## 目的

font モジュールを正式リリース可能な品質にする。個別の具体的な課題は下記の concrete issue に分割済みであり、本 issue は進捗のトラッカーとして機能する。

## 現状

- TrueType アウトライン (Simple Glyph / Compound Glyph) の描画に対応
- cmap Format 4 (BMP) / Format 12 (Full Unicode) に対応
- head, maxp, hhea, hmtx, loca, cmap, glyf テーブルのパースに対応
- TTC (TrueType Collection) に対応
- 公開 API: `FontData`, `FontFace`, `Font`, `FontError`
- `Context::fill_text` によるテキスト描画に対応済み
- 単体テスト (`tests/test_font.rs`) が 5 つ存在 (メトリクス、cmap、アウトライン、fill_text 統合)

## 残存課題

### PBT / fuzzing が未整備

テーブルパーサとグリフアウトライン変換の正しさを PBT で検証していない。フォントファイルは外部入力であり、不正なデータに対するクラッシュ耐性を fuzzing で検証する必要がある。

- tables.rs: バイトパースのラウンドトリップや不正入力に対するエラーハンドリングの PBT が必要
- glyph.rs: Simple/Compound Glyph のアウトライン変換の正しさを検証するテストが必要
- cmap: Format 4 / Format 12 のルックアップ精度を検証するテストが必要
- fuzzing: `fuzz/` に `.gitkeep` のみ存在。font テーブルパーサの fuzzing ターゲットが未作成

### OpenType (CFF) アウトラインに未対応

CFF/CFF2 ベースの OpenType フォントは読み込めない。CFF 対応は concrete issue として未登録。

## Concrete issue 一覧

| issue | タイトル | 状態 | 優先度 |
|-------|---------|------|--------|
| 0021 | Font にスケール済みメトリクス取得を追加する | 未着手 | High |
| 0022 | テキストサイズ計測機能を追加する | 未着手 | High |
| 0023 | グリフ境界ボックス取得機能を追加する | 未着手 | High |
| 0024 | ストロークテキスト描画機能を追加する | 未着手 | High |
| 0025 | カーニング適用機能を追加する | 未着手 | Medium |
| 0026 | OpenType 基本シェーピング機能を追加する | 未着手 | Medium |

### 依存関係

```
0021 (独立)
0022 (独立)
0023 (独立)
0024 → 0022 に依存
0025 → 0022, 0024 に依存
0026 → 0022, 0025 に依存
```

着手順序: 0021 / 0022 / 0023 (並行) → 0024 → 0025 → 0026

## 完了条件

- 0021 ~ 0026 がすべて完了すること
- PBT で font モジュールの主要パーサのラウンドトリップが検証されること
- fuzzing で font テーブルパーサのクラッシュ耐性が検証されること
- CFF 対応の要不要を判断すること
  - macOS / Windows / Linux の主要フォントのアウトライン形式を調査し、CFF フォントの使用率を確認する
  - CFF 対応が必要なら concrete issue を作成し、不要なら pending に移動する
