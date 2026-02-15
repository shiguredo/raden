# 合成パイプラインの SIMD 戦略

合成パイプラインは Cranelift の I32X4 型 (128-bit SIMD) を使用して 4 ピクセル並列処理する。
I32X4 は x86_64 では SSE2、AArch64 では NEON に自動マッピングされる。

```text
entry:
  ループ不変値の事前計算 (チャネル抽出、splat ベクタ)
  simd_count = count / 4, remainder = count % 4

simd_loop: 4 ピクセルを I32X4 で並列処理
  → cov=0xFF なら高速パス (div255 計算をスキップ)
  → それ以外は通常パス (カバレッジ展開 + div255)

scalar_loop: 余り 1-3 ピクセルをスカラ処理

exit: return
```

cov パイプラインでは 4 バイトのカバレッジを i32 として一括ロードし、`0xFFFFFFFF` と比較して
全ピクセルが完全カバレッジ (cov=255) なら高速パスに分岐する。図形内部の大部分は cov=255 の
ため、div255 の 16 命令を省略して大幅に高速化される。

## cov パイプライン内の SIMD ヘルパー

| ヘルパー | 命令数 | 処理 |
|---|---|---|
| `emit_extract_channels_simd` | 8 | I32X4 (4 ピクセル ARGB) → (a, r, g, b) の 4 つの I32X4 に分離 |
| `emit_pack_channels_simd` | 6 | (a, r, g, b) の 4 つの I32X4 → I32X4 (4 ピクセル ARGB) にパック |
| `emit_expand_packed_coverage_i32x4` | 4 | 4 バイトカバレッジ (i32) → I32X4 に展開 (`scalar_to_vector` → `bitcast(I8X16)` → `uwiden_low` x2) |
