# ラスタライズの流れ

```text
Path
  ↓ EdgeBuilder::flatten()
  ↓   de Casteljau 再帰分割 (FLATNESS_TOLERANCE=0.25, MAX_RECURSION_DEPTH=16)
線分列 (x0, y0, x1, y1)
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
