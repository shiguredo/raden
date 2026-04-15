# JIT パイプラインの詳細

`PipelineCompiler` は合成モードとフィルタイプの組み合わせに応じて、Cranelift IR から
ネイティブコードを生成する。

## 合成パイプライン

29 種類の CompOp それぞれに完全カバレッジ版とカバレッジ付き版のパイプラインを生成する:

| 関数 | 用途 | 処理内容 |
|---|---|---|
| `build_src_copy` | SrcCopy + 完全カバレッジ区間 | `dst[i] = src_solid` |
| `build_src_over` | SrcOver + 完全カバレッジ区間 | `dst[i] = src + dst * (256 - src_a) >> 8` |
| `build_clear` | Clear + 完全カバレッジ区間 | `dst[i] = 0` |
| `build_dst_copy` | DstCopy + 完全カバレッジ���間 | nop |
| `build_plus` | Plus + 完全カバレッジ区間 | `dst[i] = min(src + dst, 255)` |
| `build_<op>` | SrcIn/SrcOut/SrcAtop/DstOver/DstIn/DstOut/DstAtop/Xor | 汎用合成テンプレート (SIMD + スカラ) |
| `build_src_copy_cov` | SrcCopy + カバレッジ付き | `dst[i] = div255(src * cov)` |
| `build_src_over_cov` | SrcOver + カバレッジ付き | `cov_src = div255(src * cov)` → `dst[i] = cov_src + dst * (256 - cov_src_a) >> 8` |
| `build_<op>_cov` | 全 CompOp + カバレッジ付き | 汎用合成テンプレート (SIMD + cov=0xFF 高速パス + カバレッジ適用 + 合成) |

## スパンパイプライン

グラデーション/パターン等のピクセルごとに色が異なるソースを合成する:

| 関数 | 用途 | 処理内容 |
|---|---|---|
| `build_src_over_span` | SrcOver + スパンソース + カバレッジなし | ソース配列から読み出して SrcOver |
| `build_src_over_span_cov` | SrcOver + スパンソース + カバレッジ付き | ソース配列 + cov=0xFF 高速パス + SrcOver |

## グラデーション JIT パイプライン

グラデーション描画に特化した JIT パイプライン。fetch (LUT lookup) + blend を 1 つの関数に融合し、中間バッファを排除する:

| 関数 | 用途 | SIMD 型 | 処理内容 |
|---|---|---|---|
| `build_radial_row_opaque` | Radial + 不透明 LUT + カバレッジなし | F32X4 | 4 ピクセル分の sqrt を F32X4 で並列実行。LUT lookup はスカラー (gather 非対応) |
| `build_linear_gradient_cov_opaque` | Linear + 不透明 LUT + Pad + カバレッジ付き | I32X4 | 固定小数点 t → LUT → I32X4 パック → cov=0xFF 高速パス → SrcOver SIMD blend |

## sweep パイプライン

| 関数 | 用途 | 処理内容 |
|---|---|---|
| `compile_sweep` | prefix sum カバレッジ生成 | FillRule に応じて NonZero (`min(abs(cover >> 9), 255)`) または EvenOdd (2 winding 周期折り返し) |

## ユーティリティ JIT 関数

| 関数 | 用途 | 処理内容 |
|---|---|---|
| `compile_transform_edges` | エッジ座標変換 | F64X2 で 2 点 (x0,y0,x1,y1) を一括アフィン変換 |

## パイプラインキーとキャッシュ

合成パイプラインのキーは `PipelineKey` に `dst_format | comp_op | fill_type | fetch_type` を
ビットパックして識別する。同一キーの関数は `PipelineCache` に保存され再コンパイルされない。
sweep 関数は `FillRule` ごとに別関数をコンパイルし、`PipelineRuntime` で NonZero / EvenOdd 各 1 つをキャッシュする。
グラデーション JIT 関数は種別ごとに 1 つずつキャッシュする (パラメータは引数渡し)。

## div255 近似

JIT パイプライン内の /255 除算は Blend2D と同じ整数近似を使用する:

```text
div255(x) = (x * 257 + 257) >> 16
```

`x` の範囲は 0-65025 (= 255 * 255)。乗算結果は最大 16,711,682 で i32 に収まる。

## sweep 関数の JIT 戦略

sweep 関数は area-cover パック値の prefix sum → sar(9) → FillRule 変換をスカラで実行し、出力を 4 要素アンロールで最適化する。FillRule ごとに別関数を JIT コンパイルし、内部ループに分岐を入れない:

- **NonZero**: `iabs → umin(255)` (2 命令/ピクセル)
- **EvenOdd**: `iabs → band(511) → sub(512, val) → umin(val, folded) → umin(255)` (5 命令/ピクセル)

```text
entry:
  main_count = len / 4, remainder = len % 4

main_loop: 4 要素アンロール
  for j in 0..4:
    cover += cells[i*4+j]
    v[j] = fill_rule_convert(cover >> 9)  // NonZero or EvenOdd
  packed = v[0] | (v[1]<<8) | (v[2]<<16) | (v[3]<<24)
  store_i32(packed, cov_buf + i*4)    // 4 バイトを 1 回の i32 ストアで書き込み

scalar_loop: 余り 1-3 要素
  cover += cells[i], cov_buf[i] = fill_rule_convert(cover >> 9)

exit: return
```

## グラデーション最適化の性能推移

1920x1080 fill_rect での計測値:

| 段階 | Linear | Radial | Conic |
|------|--------|--------|-------|
| 初期実装 (f64 スカラ) | 4.58 ms | 8.05 ms | 24.8 ms |
| fill_rect 直接描画 | 3.03 ms | 6.77 ms | 23.2 ms |
| fetch+blend 融合 + 固定小数点 | 1.67 ms | — | — |
| 不透明高速パス + 4px アンロール | 929 µs | — | — |
| f32 変換 + 融合 fill_rect | — | 3.15 ms | 8.10 ms |
| Cranelift JIT F32X4 SIMD | — | **1.50 ms** | — |
| 最終 | **929 µs** | **1.50 ms** | **8.10 ms** |

参考: Solid SrcOver = 611-666 µs
