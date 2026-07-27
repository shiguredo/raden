# PBT ヘルパー ascii_printable_string_range を追加して文字列戦略の直書きを排除する

- Priority: Low
- Category: refactor
- Created: 2026-06-22
- Model: Opus 4.7
- Branch: feature/refactor-pbt-string-helper
- Polished: {YYYY-MM-DD}

## 目的

`pbt/tests/prop_font/main.rs` の既存ヘルパー `ascii_printable_string(max_len)` は `[ -~]{0,max_len}` を生成する (min_len = 0 固定)。0023 PBT で `min_len = 2` が必要になり `proptest::string::string_regex("[!-~]{2,16}")` を直接書く必要があった。これを `ascii_printable_string_range(min_len, max_len)` に集約し、0024-0026 で同様の min_len 指定が必要になっても直書きを増やさない設計にする。

## 優先度根拠

Low。実害はないが、DRY 原則と PBT のメンテナンス性向上のために整理が必要。0024-0026 で同様のパターンが追加される前に共通化する。

## 現状

- `pbt/tests/prop_font/main.rs:22-25` の `ascii_printable_string(max_len)`: `[ -~]{0,max_len}` を生成。
- `pbt/tests/prop_font/main.rs:261-262` (0023 で追加): `proptest::string::string_regex("[!-~]{2,16}").expect("...")` を直書き。
- 0024 (stroke_text) / 0025 (kerning) / 0026 (shaping) で `min_len = 1` / `min_len = 2` のバリアント要求が高い確率で発生する。

## 設計方針

以下を `pbt/tests/prop_font/main.rs` に追加:

```rust
fn ascii_printable_string_range(min_len: usize, max_len: usize) -> impl Strategy<Value = String> {
    proptest::string::string_regex(&format!("[ -~]{{{min_len},{max_len}}}"))
        .expect("ASCII printable 文字列戦略の正規表現が不正")
}

fn ascii_printable_string(max_len: usize) -> impl Strategy<Value = String> {
    ascii_printable_string_range(0, max_len)
}
```

さらに、空グリフ除外版 (`[!-~]`) も同様に `ascii_printable_string_excluding_space_range(min_len, max_len)` 等で提供するかを検討する。

0023 の `prop_measure_text_bounding_box_contains` 内の直書きを本ヘルパーに置き換える。

## 完了条件

- `ascii_printable_string_range(min_len, max_len)` が `pbt/tests/prop_font/main.rs` に追加されている。
- 既存 `ascii_printable_string(max_len)` は `ascii_printable_string_range(0, max_len)` を内部で呼び出すよう変更されている。
- 0023 で追加した `proptest::string::string_regex("[!-~]{2,16}")` 等の直書きをヘルパー呼び出しに置き換えている。
- PBT 全 17 件以上が引き続き pass する。

## 解決方法

polish 段階で確定する。
