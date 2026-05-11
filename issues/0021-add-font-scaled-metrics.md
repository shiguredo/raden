# Font にスケール済みメトリクス取得を追加する

- Priority: High
- Created: 2026-05-11
- Model: Kimi K2.6
- Branch: feature/add-font-scaled-metrics

## 目的

`Font` （サイズ指定済みインスタンス）からスケール済みメトリクスを取得できるようにし、テキスト描画時の行間計算や精密なレイアウトを可能にする。

## 現状

- `FontFace::line_gap()` は実装済みだが、`Font::line_gap()` が未実装
- `cap_height` / `x_height` は OS/2 テーブル（version 2+ の `sCapHeight` / `sxHeight`）に存在するが、現状 OS/2 テーブルのパースが未実装
- `cap_height` / `x_height` が存在しないフォント（OS/2 version < 2 やフィールドが 0 の場合）の扱いが未定義

## 設計方針

- OS/2 テーブルを新規パースし、`ParsedTables` に `cap_height` / `x_height` を追加する
- 存在しないメトリクスは `Option` で表現し、呼び出し側で明示的に処理する
- スケール済み値はデザイン値に `Font::scale()` を乗じて計算する

## 完了条件

- `Font::line_gap()` / `cap_height()` / `x_height()` が利用可能である
- `line_gap()` は `f64` を返し、OS/2 テーブルの有無に関わらず常に利用可能である
- `cap_height()` / `x_height()` は OS/2 テーブル不在時や version < 2 の場合は `None` を返す
- 単体テストと PBT で正しさを検証している

## 解決方法

1. `Font` に以下のメソッドを追加する
   - `line_gap() -> f64`（`FontFace::line_gap()` × `scale()`）
2. OS/2 テーブルをパースし、`ParsedTables` に以下を追加する
   - `cap_height: Option<i16>`
   - `x_height: Option<i16>`
   - OS/2 テーブルの version フィールドは offset 0 の `u16` (version 0 ~ 5)
   - version >= 2 の場合のみ `sCapHeight` (offset 88) と `sxHeight` (offset 86) を読み取る
   - version < 2 の場合やフィールド値が 0 の場合は `None`
3. `FontFace` に以下のメソッドを追加する
    - `cap_height() -> Option<i16>`（design units、存在しない場合は `None`）
    - `x_height() -> Option<i16>`（design units、存在しない場合は `None`）
4. `Font` に以下のメソッドを追加する
    - `cap_height() -> Option<f64>`（design units に `scale()` を乗じる、存在しない場合は `None`）
    - `x_height() -> Option<f64>`（design units に `scale()` を乗じる、存在しない場合は `None`）
5. OS/2 テーブルが存在しない場合や version < 2 の場合は `None` を返す
6. テスト:
   - PBT: `pbt/tests/prop_font/main.rs` で「スケール済み値 = デザイン値 × scale」の関係を検証
   - OS/2 テーブルを持つテスト用フォントで `cap_height` / `x_height` の検証

## 変更対象ファイル

- `src/font/tables.rs`: OS/2 テーブルパースの追加
- `src/font/mod.rs`: `Font::line_gap()` / `cap_height()` / `x_height()` の追加
- `tests/test_font.rs`: 単体テストの追加
- `pbt/tests/prop_font/main.rs`: PBT の追加

## エッジケース

- `size = 0` の場合 `scale = 0` となり、すべてのスケール済みメトリクスは 0.0 を返す。`Font::from_face` で `size <= 0` を拒否する設計変更は本 issue のスコープ外（別 issue で検討）
- OS/2 テーブル不在時: `cap_height()` / `x_height()` は `None`
- OS/2 version < 2: `cap_height` / `x_height` フィールドが存在しないため `None`
- OS/2 フィールド値が 0 の場合: `None` とする（0 は「未定義」の意味で使われることがある）
- OS/2 テーブルの最小サイズ: version >= 2 の場合、テーブル長が 90 バイト以上であることを検証する (`sCapHeight` は offset 88、`sxHeight` は offset 86)
- `ParsedTables` にフィールドを追加しても `Clone` コストは無視できる（`i16` 2 つ分）

## 注意事項

- `FontFaceInner` と `FontFace` の構造が重複している（data + tables）。将来的なリファクタリングで統合を検討するが、本 issue では対象外とする
- `cap_height` / `x_height` が `None` の場合のフォールバック戦略（例: 小文字 'x' の bbox から `x_height` を推定）は本 issue では対象外とする

## 関連

- 0001-enhance-font-module-maturity.md
