# Context のテンポラリバッファ取り回しを panic セーフな RAII ガードにする

- Priority: Low
- Category: fix
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/fix-context-buf-guard
- Polished: {YYYY-MM-DD}

## 目的

`Context::fill_text` / `stroke_text` / 各種 `fill_*` / `stroke_*` メソッドで使われている「`std::mem::take(&mut self.tmp_path)` で取り出して最後に `self.tmp_path = path;` で書き戻す」パターンが、途中で panic すると書き戻し前なので `tmp_path` / `tmp_glyph_run` / `stroke_path_buf` が捨てられる問題を、Drop で書き戻す RAII ガードに置き換えて解消する。

## 優先度根拠

Low。実害は「次回呼び出しでバッファ事前確保が無効になりコールド再起動」だけで、機能的な不具合ではない。ただし `tmp_*` の re-use の意図に反する状態であり、`Context` を長く使う利用者には潜在的なコストとして残る。0024 のレビューで「`fill_text` / `stroke_text` 共通の panic 経路問題」として指摘された。

## 現状

- `src/api/context.rs` 内に `std::mem::take(&mut self.tmp_path)` のパターンが多数 (13 箇所以上、 grep で確認可能)。
  - `fill_text` (L1149)、`stroke_text` (L1182)、`fill_circle` 系、`fill_polygon` 系、`fill_path` 内の前処理、`stroke_path` 内の `stroke_path_buf` 等。
- 取り出したあと `self.fill_path(&path)` / `self.stroke_path(&path)` を呼ぶ経路で、画像フォーマット未対応の `assert!` (`src/api/context.rs:822-826` の `assert_ne!(self.image.format(), PixelFormat::A8, ...)` 等) で panic しうる。
- panic が起きると `self.tmp_path = path;` まで到達せず、`tmp_path` は空のまま残る (`mem::take` で `Path::default()` に置き換わっているため)。

## 設計方針

以下のいずれかを選ぶ。

1. RAII ガード型を導入
   - `pub(crate) struct BufGuard<'a, T: Default> { src: &'a mut T, taken: T }` のような構造体を定義。
   - `BufGuard::new(src)` で `mem::take` し、`Drop` で `*self.src = std::mem::take(&mut self.taken);` で書き戻す。
   - 各 `mem::take` 呼び出しを `BufGuard::new(&mut self.tmp_path)` 等に置き換える。
   - 利点: panic safe、書き戻し漏れが構造上不可能。
   - 欠点: ライフタイムの取り回しが少し複雑 (借用構造が `BufGuard` を介すため `&mut path` の取り出し方が変わる)。
2. `defer!` マクロ的な対応
   - `scopeguard` 等の外部依存を導入するか、自前のマクロを書く。
   - 利点: 既存構造の変更が少ない。
   - 欠点: 外部依存は raden の依存最小化方針と整合しないため、自前マクロが現実的。

最終判断は polish 段階で行う。1 (RAII ガード) が型安全で妥当。

## 完了条件

- `tmp_path` / `tmp_glyph_run` / `stroke_path_buf` の `mem::take` パターンが、panic 経路でもバッファを復元する RAII ガードに置き換わっている。
- 単体テストで「`A8` 画像など panic 経路を踏ませ、その後 `tmp_path` が空でない (バッファが復元されている) こと」を `std::panic::catch_unwind` 等で検証。
- 既存テストが全て pass。

## 解決方法

polish 段階で確定する。
