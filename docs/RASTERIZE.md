# ラスタライズの流れ

## 単色描画 (fill_path)

```text
Path
  ↓ EdgeBuilder::flatten()
  ↓   de Casteljau 再帰分割 (FLATNESS_TOLERANCE=0.25, MAX_RECURSION_DEPTH=16)
線分列 (x0, y0, x1, y1)
  ↓ [変換行列がある場合] JIT transform_edges (F64X2 SIMD)
  ↓ AnalyticRasterizer::rasterize()
  ↓   1. Y の最小値でエッジをソート
  ↓   2. scanline ごとに交差エッジの area-cover パック値を cells バッファに蓄積
  ↓   3. JIT コンパイルされた sweep 関数で prefix sum → sar(9) → FillRule 変換のカバレッジマスクを生成
  ↓   4. 非ゼロ区間をコールバックで返す
コールバック (y, x_start, &[u8])
  ↓ Context::fill_path() 内
  ↓   非ゼロカバレッジ区間全体を pipeline_cov_fn に一括で渡す
  ↓   (cov=255 のピクセルは JIT パイプライン内の高速パスで処理)
ピクセルバッファ書き込み
```

## グラデーション描画 (fill_rect)

```text
Gradient::prepare(&matrix) → PreparedGradient
  ↓ グラデーション種別で分岐

[Linear]  融合パス: 固定小数点 fetch + blend (中間バッファなし)
  ↓   不透明 LUT: 4px アンロール + 直接ストア
  ↓   半透明 LUT: SrcOver blend

[Radial]  JIT F32X4 SIMD パス (不透明 LUT の場合)
  ↓   Rust 外側ループ: 行ごとに ux_start/uy_start を事前計算
  ↓   JIT 内側ループ: F32X4 で 4 ピクセル分の sqrt を並列実行
  ↓   LUT lookup はスカラー (gather 命令非対応)
  ↓   結果を I32X4 にパックして 128-bit ストア

[Conic]   Rust f32 パス
  ↓   fast_atan2_f32 多項式近似 (~5x 高速化)
  ↓   4px アンロール
  ↓   不透明 LUT: 直接ストア
```

## グラデーション描画 (fill_path)

```text
[Linear + Pad + 不透明 LUT]  融合 JIT パス (linear_gradient_cov)
  ↓   ラスタライザのコールバック内で直接 JIT 関数を呼び出す
  ↓   固定小数点 t → LUT lookup → coverage → SrcOver を 1 パスで処理
  ↓   中間バッファ (span_buf) を完全に排除
  ↓   SIMD ループ: 4 ピクセル LUT lookup + I32X4 SrcOver + cov=0xFF 高速パス

[その他]  スパンバッファ経由
  ↓   fetch_span_linear_fixed / fetch_span → span_buf
  ↓   span_cov_fn (JIT) → coverage 適用 + SrcOver blend
```

## パターン描画

```text
Pattern::prepare() → PreparedPattern

[fill_rect]  融合パス: 座標マッピング + LUT + blend (中間バッファなし)
  ↓   不透明ソース: 直接ストア
  ↓   半透明ソース: SrcOver blend

[fill_path]  スパンバッファ経由
  ↓   fetch_span → span_buf
  ↓   span_cov_fn (JIT) → coverage 適用 + SrcOver blend
```

## 単色描画 (fill_rect)

```text
[identity 変換]  JIT box パイプライン (y ループ内包)
  ↓   SrcOver / SrcCopy: 専用 JIT 関数 (4x SIMD アンロール)
  ↓   その他の CompOp: scanline ごとに pipeline_fn を呼び出し

[非 identity 変換]  fill_path にフォールバック
```

## ストローク描画

```text
Path
  ↓ stroke_to_fill_with_workspace()
  ↓   1. Path を平坦化して線分列に変換
  ↓   2. [ダッシュパターンがある場合] apply_dash() でサブパスを分断
  ↓   3. サブパス単位でストローク輪郭を生成
  ↓      - 法線ベクトル計算
  ↓      - 接合処理 (Miter/Bevel/Round)
  ↓      - キャップ処理 (Butt/Square/Round)
ストローク輪郭パス
  ↓ fill_path() で描画 (ストローク色で塗りつぶし)
```
