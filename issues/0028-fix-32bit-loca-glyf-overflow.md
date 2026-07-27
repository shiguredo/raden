# 32-bit プラットフォームで loca/glyf 範囲チェックがオーバーフローし panic しうるのを修正する

- Priority: Medium
- Category: fix
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/fix-32bit-loca-glyf-overflow
- Polished: {YYYY-MM-DD}

## 目的

`src/font/glyph.rs` の `glyph_entry_slice` および `append_glyph_recursive` の境界チェックが、32-bit プラットフォーム (`usize == u32`) でオーバーフローして range out-of-bounds panic を引き起こしうる問題を修正する。`glyph_bounds` 系の寛容方針 (`Err → None`) が崩れないよう、すべての経路で安全な範囲チェックに揃える。

## 優先度根拠

Medium。raden は現状 64-bit 想定で実害は稀だが、`glyph_bounds` の寛容方針と矛盾する致命的 panic 経路として残ること、および 32-bit 環境への対応方針が明文化されていないため、font モジュール本格安定化前に判断を確定させる必要がある。0023 のレビューで重要として指摘された。

## 現状

`src/font/glyph.rs` の `glyph_entry_slice` (および同等のロジックを持つ既存 `append_glyph_recursive`) で以下の計算が行われている。

```rust
let abs_start = tables.glyf_offset as usize + glyf_start as usize;
let abs_end = tables.glyf_offset as usize + glyf_end as usize;
```

- `tables.glyf_offset` (u32) + `glyf_start` (u32) は 32-bit プラットフォームで合計が `usize::MAX (= 2^32 - 1)` を超えると wrap する。
- `abs_end > data.len()` の境界検査が wrap 後の値で誤って通り、続く `&data[abs_start..abs_end]` で range out-of-bounds panic が発生する。
- `tables.rs` の `TableDirectory::parse` は `offset + length <= data.len()` を `u64` 演算で検証しているが、loca の `glyf_start` 値はテーブル中の任意の `u32` のため独立に大きくなりうる。
- raden は現状 64-bit 想定だが、`Cargo.toml` / `CODEBASE.md` / `README.md` 等で 32-bit 不対応が明文化されていない。

## 設計方針

以下のいずれかを選ぶ:

1. `u64` 演算 + `checked_add` に変更
   - `glyph_entry_slice` 内で `glyf_offset` / `glyf_start` / `glyf_end` を `u64` にキャストして加算し、`checked_add` で wrap を検出。`data.len() as u64` と比較し、範囲外なら `Err(FontError::InvalidData(...))` を返す。
   - 32-bit 環境でも安全に動作する。
2. 32-bit 不対応を明示
   - `Cargo.toml` に `#[cfg(not(target_pointer_width = "64"))] compile_error!(...)` 相当の構造、または `CODEBASE.md` / `README.md` で 32-bit 不対応を明文化。
   - 32-bit 環境でのビルドを禁止する。

raden が現状 64-bit のみを想定していること、font 以外のモジュール (cranelift JIT) も 64-bit 前提であることを踏まえると、案 2 (32-bit 不対応の明文化) が現実的だが、`glyph_bounds` の寛容方針との整合性を考えると案 1 (u64 演算) も妥当。最終判断は polish 段階で行う。

## 完了条件

- `glyph_entry_slice` および `append_glyph_recursive` の loca/glyf 範囲計算が、32-bit プラットフォームでも panic を引き起こさない (`Err` / `None` を返す)、または 32-bit 不対応が明文化されている。
- 案 1 を選んだ場合: PBT または単体テストで「u32::MAX に近い `glyf_offset` / `glyf_start` 入力で `Err` が返る」ことを検証。
- 案 2 を選んだ場合: 32-bit ビルドが `compile_error!` または equivalents で禁止される。

## 解決方法

polish 段階で確定する。
