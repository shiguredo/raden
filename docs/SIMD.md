# 合成パイプラインの SIMD 戦略

## I32X4 (128-bit 整数 SIMD)

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

## F32X4 (128-bit 浮動小数点 SIMD)

Radial グラデーションでは Cranelift の F32X4 型を使用して 4 ピクセル分の sqrt を並列実行する:

```text
simd_loop:
  ux_vec, uy_vec: F32X4  (4 ピクセル分のユーザー座標)
  dx_vec = fsub(ux_vec, cx_vec)
  dy_vec = fsub(uy_vec, cy_vec)
  dist_sq = fadd(fmul(dx_vec, dx_vec), fmul(dy_vec, dy_vec))
  dist = fsqrt(dist_sq)         ← F32X4 SIMD sqrt (4 並列)
  t = fmul(fsub(dist, r0_vec), inv_r_diff_max_vec)
  t = fmin(fmax(t, 0.0), 255.0)
  idx = fcvt_to_sint_sat(I32X4, t)

  // LUT lookup はスカラー (Cranelift は gather 命令非対応)
  i0..i3 = extractlane(idx, 0..3)
  p0..p3 = load(lut + i * 4)

  // I32X4 にパックしてストア
  result = insertlane(insertlane(insertlane(scalar_to_vector(p0), p1, 1), p2, 2), p3, 3)
  store(result, dst)
```

### x86_64 での命令マッピング

| Cranelift IR | SSE/AVX | 備考 |
|---|---|---|
| `fsqrt(F32X4)` | `sqrtps` | 4 並列 sqrt。レイテンシ ~11 サイクル、スループット ~3 サイクル |
| `fmul(F32X4)` | `mulps` | |
| `fsub(F32X4)` | `subps` | |
| `fmax(F32X4)` | `maxps` | |
| `fmin(F32X4)` | `minps` | |
| `fcvt_to_sint_sat(I32X4, F32X4)` | `cvttps2dq` | F32→I32 飽和変換 |
| `extractlane(I32X4, n)` | `pextrd` / `movd` | スカラー抽出 |

### AArch64 での命令マッピング

| Cranelift IR | NEON | 備考 |
|---|---|---|
| `fsqrt(F32X4)` | `fsqrt v.4s` | 4 並列 sqrt |
| `fmul(F32X4)` | `fmul v.4s` | |
| `fcvt_to_sint_sat(I32X4, F32X4)` | `fcvtzs v.4s` | |
| `extractlane(I32X4, n)` | `mov w, v[n]` | |

## SIMD ヘルパー関数

| ヘルパー | 命令数 | 処理 |
|---|---|---|
| `emit_extract_channels_simd` | 8 | I32X4 (4 ピクセル ARGB) → (a, r, g, b) の 4 つの I32X4 に分離 |
| `emit_pack_channels_simd` | 6 | (a, r, g, b) の 4 つの I32X4 → I32X4 (4 ピクセル ARGB) にパック |
| `emit_expand_packed_coverage_i32x4` | 4 | 4 バイトカバレッジ (i32) → I32X4 に展開 (`scalar_to_vector` → `bitcast(I8X16)` → `uwiden_low` x2) |
| `emit_lut_lookup` | 3-4 | LUT からスカラーインデックスで 1 ピクセルをロード (sextend + imul + iadd + load) |
| `emit_fixed_to_index` | 3 | 固定小数点 t → クランプ済み LUT インデックス (smax + smin + sshr + ireduce) |
| `emit_div255_simd` | 4 | (src * cov * 257 + 257) >> 16 の SIMD 版 |

## Linear グラデーションの固定小数点最適化

Linear グラデーションの内部ループでは f64 演算を完全に排除し、i64 固定小数点 (16.16 format) で t 値を計算する:

```text
// 事前計算 (描画開始時に 1 回だけ)
scale = 255.0 * 65536.0
dt_dx_fixed = (dt_dx * scale) as i64
dt_dy_fixed = (dt_dy * scale) as i64
t_row0 = (t_origin_at_pixel_center * scale) as i64

// 内部ループ (ピクセルあたり)
idx = clamp(t >> 16, 0, 255)   // i64 shift + clamp → 整数インデックス
color = lut[idx]                // LUT lookup
t += dt_dx_fixed                // i64 加算のみ (浮動小数点演算ゼロ)
```

Pad モードでは `clamp` が使える。Repeat モードでは `t & ((256 << 16) - 1)` のビットマスクで剰余を計算 (LUT_SIZE=256 は 2 のべき乗)。
