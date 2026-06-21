# Font::measure_text の Vec::new() アロケーションを再利用ベースに最適化する

- Priority: Low
- Category: refactor
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/refactor-measure-text-buffer-reuse
- Polished: {YYYY-MM-DD}

## 目的

`Font::measure_text` 内で毎回 `Vec::new()` してグリフ列を捨てている非対称を解消し、`Context::fill_text` 側と同じくバッファ再利用パターンに揃える。本最適化は描画ホットパス外だが、0024-0026 で `measure_text` の呼び出し頻度・呼び出し経路が増える前提で整理する。

## 優先度根拠

Low。実害はないが、設計の対称性が `Context::fill_text` (バッファ再利用) と `Font::measure_text` (毎回確保) で割れているのは Don't live with broken windows に該当する。0023 のレビューで重要 (改善寄り) として指摘された。

## 現状

`src/font/mod.rs` の `Font::measure_text` で毎回 `Vec::new()` を確保:

```rust
pub fn measure_text(&self, text: &str) -> TextMetrics {
    let mut buf: Vec<(u16, f64)> = Vec::new();
    self.glyph_run_for_text(text, &mut buf);
    // ...
}
```

一方、`src/api/context.rs` の `Context::fill_text` は `tmp_glyph_run` を `std::mem::take` で再利用している (`context.rs:1149-1151` 周辺)。

`glyph_run_for_text` は `buf.clear()` 起点なので、外部からバッファを渡せる設計になっている (`pub(crate)` メソッド) が、`measure_text` 側はこれを活かしていない。

## 設計方針

以下のいずれかを選ぶ:

1. `Font` 側にバッファを `RefCell<Vec<(u16, f64)>>` で持たせる
   - `Font::measure_text` 内で `self.tmp_glyph_run.borrow_mut()` を使い、`glyph_run_for_text` に渡す。
   - 利点: `Font` を共有しても各呼び出しで安全。
   - 欠点: `Font` の API に内部可変性が露出する。`Font` の `Clone` 実装 (現状なし) を検討する場合に注意が必要。
2. `Context` 経由の `measure_text` バリアントを追加
   - `Context` の `tmp_glyph_run` を流用する `Context::measure_text(&Font, &str)` を追加。
   - 利点: 既存 `Font::measure_text` の API を変更せず、`Context` を持つ呼び出し元のみが恩恵を受ける。
   - 欠点: API が 2 系統になる。

最終判断は polish 段階で、`Font` の API 設計方針 (`Send + Sync` 要件、`Clone` の必要性) を踏まえて行う。

## 完了条件

- `Font::measure_text` が呼び出しごとに `Vec::new()` を確保しない実装になっている、または `Context::measure_text` バリアントが提供されている。
- ベースライン benchmark 計測基盤 (0001 メタ issue tracked) が確定した後にアロケート削減の効果を測定し、本 issue の解決方法に追記する (測定値の取得は本 issue 範囲)。

## 解決方法

polish 段階で確定する。
