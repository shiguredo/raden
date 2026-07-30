# Context::fill_text と Context::stroke_text の文字列 → Path 構築ロジックを共通化する

- Priority: Medium
- Category: refactor
- Created: 2026-06-22
- Completed: 2026-07-30
- Model: Opus 4.7
- Branch: feature/refactor-context-build-text-path
- Polished: {YYYY-MM-DD}

## 目的

0024 で `Context::stroke_text` を追加した結果、`fill_text` と `stroke_text` の本体ロジックが「描画関数 1 行違い」のコピーになっている状態を解消する。共通ヘルパー `Context::build_text_path` (仮称) を抽出し、`fill_text` / `stroke_text` 双方から呼ぶ形にする。0025 (カーニング適用) / 0026 (シェーピング適用) で同期改修が必要になるため、その前に共通化しておく。

## 優先度根拠

Medium。0025 (kerning) と 0026 (shaping) で `fill_text` / `measure_text` / `stroke_text` の 3 者を同期改修する仕様が 0001 メタ issue で確定済み。共通化されていないと改修漏れリスクが顕在化する。本 issue は 0025 着手前 (理想的には 0025 と同 PR 内で同時実施) に解消するのが望ましい。

## 現状

- `src/api/context.rs:1148-1172` の `fill_text` と同 `1174-1202` の `stroke_text` は、本体 22 行のうち 21 行が完全同型。
- 差分は `self.fill_path(&path)` ↔ `self.stroke_path(&path)` の 1 行のみ。
- 0024 の解決にあたって issue 0024 では「`fill_text` 本体のリファクタリングは 0022 のスコープ」と明記されたため、0024 単独 PR では共通化を見送った。0022 はすでに closed (0022-add-text-measurement.md) のため、本 issue として独立に扱う。
- 0024 のレビューで「`fill_text` / `stroke_text` の双子コード」として複数観点から指摘された。

## 設計方針

以下のいずれかを選ぶ。

1. `Context::build_text_path` (private または `pub(crate)`) を抽出
   - 文字列 → グリフ列 → Path 構築までを集約。
   - `fill_text` / `stroke_text` は「ヘルパー呼び出し → 描画関数」の 2-3 行に圧縮。
   - 0025 / 0026 で `glyph_run_for_text` の置換 / カーニング適用は本ヘルパー 1 箇所だけ触る。
2. クロージャ受け取り型ヘルパー
   - `fn render_text(&mut self, x, y, font, text, draw: impl FnOnce(&mut Self, &Path))` のような形。
   - 利点: 描画関数を引数で渡せるため柔軟。
   - 欠点: クロージャの借用構造が少し複雑。

`tmp_path` / `tmp_glyph_run` の取り回しは現状の `mem::take` パターンを維持するか、0035 (RAII ガード化) と組み合わせるかを polish 時に判断する。

## 完了条件

- `Context::fill_text` / `stroke_text` の文字列 → Path 構築ロジックが `Context::build_text_path` (`pub(crate)` または private) として一本化されている。
- 両関数の本体は「Path 構築 → 描画」の 2-3 行に圧縮されている。
- 既存テスト (`font_fill_text_integration` / `stroke_text::*` の計 6 件) が引き続き pass。
- 0025 / 0026 で「`measure_text` / `fill_text` / `stroke_text` の 3 者の同期改修が共通ヘルパー 1 箇所 + `measure_text` 1 箇所だけで済む」ことが構造上明らかになっている。

## 解決方法

0026（OpenType 基本シェーピング）実装の過程で、完了条件は既に満たされている。

- `src/api/context.rs` に private の `Context::build_text_path` が存在し、文字列 → `shape_into` → Path 構築を集約している
- `fill_text` / `stroke_text` は `build_text_path` 呼び出し後に `fill_path` / `stroke_path` する形に圧縮済み
- 設計方針 1（`build_text_path` 抽出）が採用された結果。0035（RAII ガード化）との組み合わせは本 issue の完了条件外のため未実施
- 本 issue 単独の追加実装は不要のため closed にする
