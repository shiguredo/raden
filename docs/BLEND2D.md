# Blend2D API との比較

Blend2D ソース: https://github.com/blend2d/blend2d の各ヘッダファイルに基づく。

## Context API

### ライフサイクル / 基本操作

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLContext::begin(image)` / `BLContext(image)` | `Context::new(image, runtime)` | 差異あり: Rust 慣例で `new`。JIT キャッシュをフレーム間で再利用するため `PipelineRuntime` を外から渡す |
| `end()` | `end()` | 一致 (現在は no-op。将来のバッファフラッシュ用に予約) |
| `flush(BLContextFlushFlags)` | なし | 不要: raden は全描画を同期実行するためコマンドバッファが存在しない |
| `reset()` | なし | 未実装 |
| `is_valid()` / `equals()` | なし | 未実装 |
| `context_type()` | なし | 未実装 |
| `target_size()` / `target_width()` / `target_height()` / `target_image()` | なし | 未実装: ターゲット情報の取得 |
| `thread_count()` | なし | 未実装: raden はシングルスレッド描画 |
| `accumulated_error_flags()` | なし | 未実装: 非同期描画中のエラーフラグ蓄積 |

### 状態管理

| Blend2D | raden | 状態 |
|---------|-------|------|
| `save()` / `restore()` | `save()` / `restore()` | 一致 |
| `save(cookie)` / `restore(cookie)` | なし | 未実装: 名前付き save/restore (特定の cookie でのみ restore 可能) |
| `saved_state_count()` | なし | 未実装 |

### 変換

| Blend2D | raden | 状態 |
|---------|-------|------|
| `translate(double, double)` | `translate(f64, f64)` | 一致 |
| `translate(BLPointI)` / `translate(BLPoint)` | なし | 未実装: Point 型オーバーロード |
| `scale(double)` / `scale(double, double)` / `scale(BLPoint)` | `scale(f64, f64)` | 差異あり: Blend2D は単一値オーバーロードと Point 型もある |
| `rotate(double)` | `rotate(f64)` | 一致 |
| `rotate(double, double, double)` / `rotate(double, BLPoint)` | `rotate_around(angle, cx, cy)` | 一致: `Context` は `Matrix2D::rotate_around` に委譲 |
| `skew(double, double)` / `skew(BLPoint)` | `skew(f64, f64)` | 一致: 係数は接線 (Blend2D の `skew` と同様)。`BLPoint` オーバーロードは未実装 |
| `apply_transform(const BLMatrix2D&)` | `apply_matrix(&Matrix2D)` | 一致 |
| `set_transform(const BLMatrix2D&)` / `reset_transform()` | `reset_matrix()` | 差異あり: raden は `set_transform` (任意行列を直接設定) がない |
| `user_to_meta()` | `user_to_meta()` | 差異あり: raden はメタ行列を別保持せず、ユーザ行列を単位にリセットするのみ |
| `meta_transform()` / `user_transform()` / `final_transform()` | なし | 未実装: 変換行列の取得 |
| `post_translate()` / `post_scale()` / `post_skew()` / `post_rotate()` / `post_transform()` | 同名 (`Context` / `Matrix2D`) | 実装済み: Blend2D の POST 系と同じ合成順 (`Matrix2D` に実装、`Context` は委譲) |

### レンダリングヒント / 近似オプション

| Blend2D | raden | 状態 |
|---------|-------|------|
| `hints()` / `set_hint()` / `set_hints()` | なし | 未実装 |
| `rendering_quality()` / `set_rendering_quality()` | なし | 未実装 |
| `gradient_quality()` / `set_gradient_quality()` | なし | 未実装 |
| `pattern_quality()` / `set_pattern_quality()` | なし | 未実装 |
| `approximation_options()` / `set_approximation_options()` | なし | 未実装 |
| `flatten_mode()` / `set_flatten_mode()` | なし | 未実装 |
| `flatten_tolerance()` / `set_flatten_tolerance()` | なし | 未実装 |

### 合成 / グローバルアルファ

| Blend2D | raden | 状態 |
|---------|-------|------|
| `comp_op()` / `set_comp_op()` | `comp_op()` / `set_comp_op()` | 一致 |
| `global_alpha()` / `set_global_alpha()` | `global_alpha()` / `set_global_alpha(f64)` | 一致 (値域 [0, 1] にクランプ) |

### スタイル (汎用)

| Blend2D | raden | 状態 |
|---------|-------|------|
| `style_type()` / `style_alpha()` | なし | 未実装 |
| `get_style()` / `get_transformed_style()` | なし | 未実装 |
| `set_style(slot, style)` / `set_style(slot, style, transform_mode)` | なし | 未実装: スロット指定のスタイル設定 |
| `disable_style()` | なし | 未実装 |
| `set_style_alpha()` | なし | 未実装 |
| `swap_styles()` | なし | 未実装: fill/stroke スタイルの交換 |

### フィルスタイル / オプション

| Blend2D | raden | 状態 |
|---------|-------|------|
| `set_fill_style(rgba32)` | `set_fill_style(Rgba32)` | 一致 |
| `set_fill_style(gradient)` | `set_fill_style_gradient(&Gradient)` | 実装済み: Linear/Radial/Conic グラデーション |
| `set_fill_style(pattern)` | `set_fill_style_pattern(&Pattern)` | 実装済み: 画像パターン塗りつぶし |
| `set_fill_style(style, transform_mode)` | なし | 未実装: 変換モード付きスタイル設定 |
| `fill_style_type()` / `get_fill_style()` / `get_transformed_fill_style()` | `fill_color_prgb32()` / `fill_gradient()` / `fill_pattern()` | 差異あり: 種別の統合取得や変換済みスタイルは未実装 |
| `disable_fill_style()` | なし | 未実装 |
| `fill_alpha()` / `set_fill_alpha()` | `fill_alpha()` / `set_fill_alpha(f64)` | 一致 (値域 [0, 1] にクランプ) |
| `fill_rule()` / `set_fill_rule()` | `fill_rule()` / `set_fill_rule(FillRule)` | 一致 |

### ストロークスタイル / オプション

| Blend2D | raden | 状態 |
|---------|-------|------|
| `set_stroke_style(rgba32)` | `set_stroke_style(Rgba32)` | 一致 |
| `set_stroke_style(gradient)` | `set_stroke_style_gradient(&Gradient)` | 実装済み: Linear/Radial/Conic グラデーション |
| `set_stroke_style(pattern)` | `set_stroke_style_pattern(&Pattern)` | 実装済み: 画像パターン |
| `stroke_style_type()` / `get_stroke_style()` / `get_transformed_stroke_style()` | `stroke_color_prgb32()` / `stroke_gradient()` / `stroke_pattern()` | 差異あり: 種別の統合取得や変換済みスタイルは未実装 |
| `disable_stroke_style()` | なし | 未実装 |
| `stroke_alpha()` / `set_stroke_alpha()` | `stroke_alpha()` / `set_stroke_alpha(f64)` | 一致 (値域 [0, 1] にクランプ) |
| `stroke_width()` / `set_stroke_width()` | `stroke_width()` / `set_stroke_width()` | 一致 |
| `stroke_miter_limit()` / `set_stroke_miter_limit()` | `stroke_miter_limit()` / `set_stroke_miter_limit()` | 一致 |
| `stroke_join()` / `set_stroke_join()` | `stroke_join()` / `set_stroke_join()` | 一致 |
| `stroke_start_cap()` / `stroke_end_cap()` | `stroke_start_cap()` / `stroke_end_cap()` | 一致 |
| `set_stroke_cap(position, cap)` / `set_stroke_start_cap()` / `set_stroke_end_cap()` / `set_stroke_caps()` | `set_stroke_cap()` / `set_stroke_start_cap()` / `set_stroke_end_cap()` | 一致: 一括/個別どちらでも設定可能 |
| `stroke_transform_order()` / `set_stroke_transform_order()` | なし | 未実装: `stroke_path` は `stroke_to_fill`（ユーザ空間）のあと `fill_path` で行列を適用する固定手順。Blend2D の `BLStrokeTransformOrder` 値との対応は未検証 |
| `stroke_dash_offset()` / `set_stroke_dash_offset()` | `stroke_dash_offset()` / `set_stroke_dash_offset(f64)` | 一致 |
| `stroke_dash_array()` / `set_stroke_dash_array()` | `stroke_dash_array()` / `set_stroke_dash_array(&[f64])` | 一致。SVG 準拠で奇数パターンは 2 回繰り返す |
| `stroke_options()` / `set_stroke_options()` | なし | 未実装: 一括取得/設定 |

### クリッピング

| Blend2D | raden | 状態 |
|---------|-------|------|
| `clip_to_rect(BLRectI)` / `clip_to_rect(BLRect)` / `clip_to_rect(x, y, w, h)` | `clip_to_rect(&Rect)` | 実装済み |
| `restore_clipping()` | `restore_clipping()` | 実装済み |

### クリア操作

| Blend2D | raden | 状態 |
|---------|-------|------|
| `clear_all()` | `clear_all()` | 一致 (現在のクリップ領域全体をピクセル値 0 で書き換え。`comp_op` は変更しない) |
| `clear_rect(BLRectI)` / `clear_rect(BLRect)` / `clear_rect(x, y, w, h)` | `clear_rect(&Rect)` | 差異あり: raden は `&Rect` 1 種のみ。デバイス座標で動作し変換行列は無視 |

### フィル操作

| Blend2D | raden | 状態 |
|---------|-------|------|
| `fill_all()` | `fill_all()` | 一致 |
| `fill_rect(BLRectI/BLRect/x,y,w,h)` | `fill_rect(&Rect)` | 差異あり: 単色は `comp_op` を参照。パターン `fill_rect` は `SrcOver`/`SrcCopy` のみ（他は panic）。グラデーションの `fill_rect` 高速パスは `Context::comp_op` を渡さず `PreparedGradient::fill_rect` が内部融合（`gradient.rs` コメントの SrcOver 融合）。**グラデーションの `fill_path` は `comp_op` を `span_cov` に渡して参照する**（経路が異なるので注意） |
| `fill_box(BLBoxI/BLBox/x0,y0,x1,y1)` | なし | 未実装: 2 点指定の矩形塗りつぶし |
| `fill_round_rect(BLRoundRect/...)` | `fill_round_rect(&RoundRect)` | 一致 |
| `fill_circle(BLCircle/cx,cy,r)` | `fill_circle(&Circle)` | 一致 |
| `fill_ellipse(BLEllipse/cx,cy,rx,ry)` | `fill_ellipse(&Ellipse)` | 一致 |
| `fill_triangle(BLTriangle/x0,y0,x1,y1,x2,y2)` | `fill_triangle(&Triangle)` | 一致 |
| `fill_pie(BLArc/cx,cy,r,start,sweep/cx,cy,rx,ry,start,sweep)` | `fill_pie(&Arc)` | 一致 |
| `fill_chord(BLArc/...)` | なし | 未実装: 弦で閉じた円弧 |
| `fill_polygon(BLPoint*/BLPointI*/BLArrayView)` | `fill_polygon(&[Point])` | 一致 (3 点未満は no-op) |
| `fill_rect_array(BLRect*/BLRectI*/BLArrayView)` | なし | 未実装: 複数矩形一括塗りつぶし |
| `fill_box_array(BLBox*/BLBoxI*/BLArrayView)` | なし | 未実装: 複数ボックス一括塗りつぶし |
| `fill_path(BLPath)` / `fill_path(BLPoint, BLPath)` | `fill_path(&Path)` | 差異あり: raden は origin 付きオーバーロードがない |
| `fill_geometry(BLGeometryType, data)` | なし | 未実装: ジオメトリ型の統合描画 API |
| `fill_mask(BLPointI/BLPoint, BLImage, ...)` | なし | 未実装: マスク画像を使った塗りつぶし描画 |

### フィルテキスト操作

| Blend2D | raden | 状態 |
|---------|-------|------|
| `fill_utf8_text(BLPointI/BLPoint, BLFont, const char*, size_t)` | `fill_text(x, y, &Font, &str)` | 差異あり: raden は UTF-8 のみ、名前は `fill_text` |
| `fill_utf16_text(...)` | なし | 未実装: UTF-16 |
| `fill_utf32_text(...)` | なし | 未実装: UTF-32 |
| `fill_glyph_run(BLPointI/BLPoint, BLFont, BLGlyphRun)` | なし | 未実装: GlyphRun |

### ストローク操作

| Blend2D | raden | 状態 |
|---------|-------|------|
| `stroke_rect(BLRectI/BLRect/x,y,w,h)` | `stroke_rect(&Rect)` | 一致 |
| `stroke_box(BLBoxI/BLBox/x0,y0,x1,y1)` | なし | 未実装 |
| `stroke_round_rect(BLRoundRect/...)` | `stroke_round_rect(&RoundRect)` | 一致 |
| `stroke_circle(BLCircle/cx,cy,r)` | `stroke_circle(&Circle)` | 一致 |
| `stroke_ellipse(BLEllipse/cx,cy,rx,ry)` | `stroke_ellipse(&Ellipse)` | 一致 |
| `stroke_triangle(BLTriangle/...)` | `stroke_triangle(&Triangle)` | 一致 |
| `stroke_pie(BLArc/...)` | なし | 未実装 |
| `stroke_chord(BLArc/...)` | なし | 未実装 |
| `stroke_arc(BLArc/...)` | なし | 未実装 |
| `stroke_line(BLLine/BLPoint,BLPoint/x0,y0,x1,y1)` | `stroke_line(&Line)` | 一致 |
| `stroke_polygon(BLPoint*/BLPointI*/BLArrayView)` | `stroke_polygon(&[Point])` | 一致 (閉じる) |
| `stroke_polyline(BLPoint*/BLPointI*/BLArrayView)` | `stroke_polyline(&[Point])` | 一致 (閉じない) |
| `stroke_rect_array(...)` / `stroke_box_array(...)` | なし | 未実装 |
| `stroke_path(BLPath)` / `stroke_path(BLPoint, BLPath)` | `stroke_path(&Path)` | 差異あり: raden は origin 付きオーバーロードがない |
| `stroke_geometry(BLGeometryType, data)` | なし | 未実装 |

### ストロークテキスト操作

| Blend2D | raden | 状態 |
|---------|-------|------|
| `stroke_utf8_text(...)` / `stroke_utf16_text(...)` / `stroke_utf32_text(...)` | なし | 未実装 |
| `stroke_glyph_run(...)` | なし | 未実装 |

### Blit 操作

| Blend2D | raden | 状態 |
|---------|-------|------|
| `blit_image(BLPointI/BLPoint, BLImage)` | `blit_image_at(x, y, &Image)` | 差異あり: `CompOp` は `SrcOver` / `SrcCopy` のみ。Nearest のみ |
| `blit_image(BLPointI/BLPoint, BLImage, BLRectI)` | `blit_image_rect` で `src_rect` 指定 | 同上 |
| `blit_image(BLRectI/BLRect, BLImage)` | `blit_image_rect(&Rect, &Image, None)` | 同上 |
| `blit_image(BLRectI/BLRect, BLImage, BLRectI)` | `blit_image_rect(&Rect, &Image, Some(Rect))` | 同上 |

## Path API

### パスコマンド

| Blend2D | raden | 状態 |
|---------|-------|------|
| `move_to(double, double)` / `move_to(BLPoint)` | `move_to(f64, f64)` | 一致 |
| `line_to(double, double)` / `line_to(BLPoint)` | `line_to(f64, f64)` | 一致 |
| `poly_to(const BLPoint*, size_t)` | なし | 未実装: 複数点への連続 line_to |
| `quad_to(...)` | `quad_to(cpx, cpy, x, y)` | 一致 |
| `smooth_quad_to(...)` | `smooth_quad_to(x, y)` | 実装済み |
| `cubic_to(...)` | `cubic_to(cp1x, cp1y, cp2x, cp2y, x, y)` | 一致 |
| `smooth_cubic_to(...)` | `smooth_cubic_to(cp2x, cp2y, x, y)` | 実装済み |
| `conic_to(cx, cy, ex, ey, w)` | `conic_to(cx, cy, ex, ey, w)` | 実装済み: `PathCmd::ConicTo`、重みは `conic_weights()` |
| `arc_to(cx, cy, rx, ry, start, sweep, force_move_to)` | `arc_to(...)` | 一致: `force_move_to` が true のとき `move_to`、false のとき `line_to` で弧の始点へ接続 |
| `arc_quadrant_to(x1, y1, x2, y2)` | なし | 未実装: 象限単位の円弧 (90 度以下) |
| `elliptic_arc_to(rx, ry, x_rotation, large_arc, sweep_flag, x1, y1)` | なし | 未実装: 楕円弧 (SVG arc コマンド相当) |
| `close()` | `close()` | 一致 |

### 情報取得

| Blend2D | raden | 状態 |
|---------|-------|------|
| `size()` / `capacity()` / `is_empty()` | `len()` / `is_empty()` | 差異あり: raden は `capacity()` がない |
| `command_data()` / `command_data_end()` | `cmds()` | 差異あり: raden はスライスを返す |
| `vertex_data()` / `vertex_data_end()` | `points()` | 差異あり: raden はスライスを返す |
| `view()` | なし | 未実装 |
| `equals(const BLPath&)` | なし | 未実装 |
| `get_info_flags(uint32_t*)` | なし | 未実装: パスの内容フラグ取得 |
| `get_bounding_box(BLBox*)` | `bounding_box() -> Option<Rect>` | 差異あり: 現在は制御点ベース (`control_box` のエイリアス)。曲線の厳密な bbox は今後対応 |
| `get_control_box(BLBox*)` | `control_box() -> Option<Rect>` | 一致 |
| `get_last_vertex(BLPoint*)` | なし | 未実装 |
| `get_figure_range(size_t, BLRange*)` | なし | 未実装 |
| `get_closest_vertex(...)` | なし | 未実装 |
| `hit_test(const BLPoint&, BLFillRule)` | なし | 未実装 |

### ライフサイクル / メモリ

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLPath()` | `Path::new()` | 一致 |
| `clear()` | `clear()` | 一致 |
| `reset()` | なし | 未実装: Blend2D では clear + メモリ解放 |
| `reserve(size_t)` / `shrink()` | なし | 未実装: 内部バッファの事前確保/縮小 |
| `assign()` / `assign_deep()` / `swap()` | なし | 未実装 |

### ジオメトリ追加

| Blend2D | raden | 状態 |
|---------|-------|------|
| `add_geometry(BLGeometryType, data, matrix, direction)` | なし | 未実装: 汎用ジオメトリ追加 |
| `add_path(BLPath)` / `add_path(BLPath, BLPoint)` / `add_path(BLPath, BLMatrix2D)` | `add_path(&Path)` / `add_path_translated(&Path, dx, dy)` / `add_path_transformed(&Path, &Matrix2D)` | 一致 |
| `add_reversed_path(BLPath, BLPathReverseMode)` | なし | 未実装 |
| `add_stroked_path(BLPath, BLStrokeOptions, BLApproximationOptions)` | なし | 未実装: ストローク結果を Path に追加 |
| `add_line(BLLine, direction)` | なし | 未実装: 線分をパスに追加 |
| `add_rect(BLRect/BLRectI/x,y,w,h, direction)` | なし | 未実装: `move_to` / `line_to` / `close` で代替可能 |
| `add_box(BLBox/BLBoxI/x0,y0,x1,y1, direction)` | なし | 未実装 |
| `add_round_rect(BLRoundRect, direction)` | `add_round_rect(x, y, w, h, rx, ry)` | 差異あり: raden は direction パラメータがない。半径は幅/高さの半分でクランプ |
| `add_circle(BLCircle, direction)` | `add_circle(cx, cy, r)` | 差異あり: raden は direction パラメータがない |
| `add_ellipse(BLEllipse, direction)` | `add_ellipse(cx, cy, rx, ry)` | 差異あり: raden は direction パラメータがない |
| `add_triangle(BLTriangle, direction)` | `add_triangle(x0, y0, x1, y1, x2, y2)` | 差異あり: raden は direction パラメータがない |
| `add_arc(BLArc, direction)` | なし | 未実装: 円弧 |
| `add_pie(BLArc, direction)` | `add_pie(cx, cy, rx, ry, start, sweep)` | 差異あり: raden は direction パラメータがない |
| `add_chord(BLArc, direction)` | なし | 未実装: 弦で閉じた円弧 |
| `add_polygon(BLPoint*/BLPointI*, direction)` | `add_polygon(&[Point])` | 差異あり: raden は direction パラメータがない |
| `add_polyline(BLPoint*/BLPointI*, direction)` | `add_polyline(&[Point])` | 差異あり: raden は direction パラメータがない |
| `add_rect_array(BLRect*/BLRectI*, direction)` | なし | 未実装 |
| `add_box_array(BLBox*/BLBoxI*, direction)` | なし | 未実装 |

### 変換 / 編集

| Blend2D | raden | 状態 |
|---------|-------|------|
| `translate(BLPoint)` / `translate(BLRange, BLPoint)` | `translate(dx, dy)` | 差異あり: raden は範囲指定なし、in-place で全頂点に適用 |
| `transform(BLMatrix2D)` / `transform(BLRange, BLMatrix2D)` | `transform(&Matrix2D)` | 差異あり: raden は範囲指定なし。コニックの重みは行列適用で不変 |
| `fit_to(BLRect, flags)` / `fit_to(BLRange, BLRect, flags)` | なし | 未実装 |
| `set_vertex_at(index, cmd, BLPoint)` | なし | 未実装: 頂点の個別変更 |
| `remove_range(BLRange)` | なし | 未実装 |

## ジオメトリ型

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLPoint` (double x, y) | `Point` (f64 x, y) | 一致 |
| `BLPointI` (int x, y) | なし | 未実装: 整数座標版 |
| `BLRect` (double x, y, w, h) | `Rect` (f64 x, y, w, h) | 一致 |
| `BLRectI` (int x, y, w, h) | なし | 未実装: 整数座標版 |
| `BLBox` (double x0, y0, x1, y1) | なし | 未実装: 2 点指定の矩形 |
| `BLBoxI` (int x0, y0, x1, y1) | なし | 未実装: 整数座標版。`contains()` メソッドあり |
| `BLSizeI` (int w, h) / `BLSize` (double w, h) | なし | 未実装: サイズ型 |
| `BLCircle` (double cx, cy, r) | `Circle` (f64 cx, cy, r) | 一致 |
| `BLEllipse` (double cx, cy, rx, ry) | `Ellipse` (f64 cx, cy, rx, ry) | 一致 |
| `BLRoundRect` (double x, y, w, h, rx, ry) | `RoundRect` (f64 x, y, w, h, rx, ry) | 一致 |
| `BLLine` (double x0, y0, x1, y1) | `Line` (f64 x0, y0, x1, y1) | 一致 |
| `BLTriangle` (double x0, y0, x1, y1, x2, y2) | `Triangle` (f64 x0, y0, x1, y1, x2, y2) | 一致 |
| `BLArc` (double cx, cy, rx, ry, start, sweep) | `Arc` (f64 cx, cy, rx, ry, start, sweep) | 一致 |
| `BLChord` | なし | 未実装: 円弧の弦で閉じた図形 (Blend2D では BLArc を共用) |
| `BLMatrix2D` | `Matrix2D` | 一致: 2D アフィン変換行列。Blend2D と同一レイアウト |
| Polyline / Polygon | `&[Point]` (`add_polygon` / `add_polyline`) | 差異あり: 専用型ではなくスライスを直接渡す |

## Matrix2D API

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLMatrix2D(m00, m01, m10, m11, m20, m21)` | `Matrix2D::new(m00, m01, m10, m11, m20, m21)` | 一致 |
| `make_identity()` / `reset_to_identity()` | `Matrix2D::IDENTITY` | 一致: raden は const で提供 |
| `make_translation(double, double)` / `reset_to_translation(...)` | `Matrix2D::translation(f64, f64)` | 一致 |
| `make_scaling(double)` / `make_scaling(double, double)` / `reset_to_scaling(...)` | `Matrix2D::scaling(f64, f64)` | 差異あり: raden は単一値オーバーロードがない |
| `make_rotation(double)` / `make_rotation(double, BLPoint)` / `reset_to_rotation(...)` | `Matrix2D::rotation(f64)` / `rotate_around(angle, cx, cy)` | 差異あり: ファクトリ名は `rotation` と `skewing`。指定点まわりは `rotate_around` |
| `make_skewing(double, double)` / `make_skewing(BLPoint)` / `reset_to_skewing(...)` | `Matrix2D::skewing(kx, ky)` | 差異あり: `BLPoint` オーバーロードは未実装 |
| `make_sin_cos(...)` / `reset_to_sin_cos(...)` | なし | 未実装: sin/cos 直接指定の回転行列 |
| `translate(...)` / `scale(...)` / `rotate(...)` | `translate(f64, f64)` / `scale(f64, f64)` / `rotate(f64)` | 一致 |
| `skew(double, double)` / `skew(BLPoint)` | `skew(kx, ky)` | 差異あり: `BLPoint` オーバーロードは未実装 |
| `transform(const BLMatrix2D&)` | `multiply(&Matrix2D)` | 差異あり: Blend2D はメソッド名 `transform`、raden は `multiply` |
| `post_translate(...)` / `post_scale(...)` / `post_skew(...)` / `post_rotate(...)` / `post_transform(...)` | 同名 | 実装済み |
| `reset()` | `reset()` | 一致 |
| `multiply()` (Blend2D は演算子オーバーロード) | `multiply(&Matrix2D)` | 一致 |
| `invert()` | `invert()` -> `Option<Self>` | 実装済み: 行列式がゼロの場合は None |
| `type()` -> `BLTransformType` | なし | 未実装: 行列種別判定 |
| `determinant()` | なし | 未実装: 行列式 |
| `map_point(double, double)` / `map_point(BLPoint)` | `map_point(f64, f64)` | 一致 |
| `map_vector(double, double)` / `map_vector(BLPoint)` | なし | 未実装: 平行移動を除いたベクトル変換 |
| `equals(const BLMatrix2D&)` | なし | 未実装: raden は `PartialEq` derive で `==` 演算子を提供 |
| なし | `is_identity()` | raden 独自: 単位行列かどうかの判定 |

## 色 / スタイル

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLRgba32` | `Rgba32` | 一致 |
| `BLRgba64` | なし | 未実装: 16-bit/チャネル色 |
| `BLRgba` (float r, g, b, a) | なし | 未実装: 128-bit 浮動小数点色 |
| `BLCompOp` (29 種類) | `CompOp` (29 種類) | 一致: 値 0-28 が完全一致 |
| `BLGradient` (Linear/Radial/Conic) | `Gradient` | 実装済み: Linear/Radial/Conic。LUT ベースの色補間、固定小数点 fetch、JIT F32X4 SIMD (Radial) |
| `BLPattern` | `Pattern` / `PatternFilter` | 実装済み: `set_filter`（Nearest / Bilinear）、`set_origin` / `set_transform`（アフィン）、`prepare` は `Context` の行列と合成 |
| `BLExtendMode` | `ExtendMode` | 実装済み: Pad, Repeat, Reflect。X/Y 独立モードは未実装 |

### Rgba32 メソッド

| Blend2D | raden | 状態 |
|---------|-------|------|
| コンストラクタ (u32/r,g,b,a/BLRgba64) | `Rgba32::new(r, g, b, a)` / `Rgba32::rgb(r, g, b)` | 差異あり: raden は u32 直接指定と BLRgba64 変換がない |
| `r()` / `g()` / `b()` / `a()` | `r()` / `g()` / `b()` / `a()` | 一致 |
| `setR()` / `setG()` / `setB()` / `setA()` | なし | 未実装: 個別チャネル設定 |
| `reset(...)` | なし | 未実装: 値の再設定 |
| `is_opaque()` / `is_transparent()` | `is_opaque()` / `is_transparent()` | 一致 |
| `equals(const BLRgba32&)` | なし | 未実装: raden は `PartialEq` derive で `==` 演算子を提供 |
| なし | `to_prgb32()` | raden 独自: premultiplied ARGB への変換 |

### CompOp 詳細

| Blend2D | raden | 値 | 状態 |
|---------|-------|-----|------|
| `SRC_OVER` | `SrcOver` | 0 | 一致 |
| `SRC_COPY` | `SrcCopy` | 1 | 一致 |
| `SRC_IN` | `SrcIn` | 2 | 一致 |
| `SRC_OUT` | `SrcOut` | 3 | 一致 |
| `SRC_ATOP` | `SrcAtop` | 4 | 一致 |
| `DST_OVER` | `DstOver` | 5 | 一致 |
| `DST_COPY` | `DstCopy` | 6 | 一致 |
| `DST_IN` | `DstIn` | 7 | 一致 |
| `DST_OUT` | `DstOut` | 8 | 一致 |
| `DST_ATOP` | `DstAtop` | 9 | 一致 |
| `XOR` | `Xor` | 10 | 一致 |
| `CLEAR` | `Clear` | 11 | 一致 |
| `PLUS` | `Plus` | 12 | 一致 |
| `MINUS` | `Minus` | 13 | 一致 |
| `MODULATE` | `Modulate` | 14 | 一致 |
| `MULTIPLY` | `Multiply` | 15 | 一致 |
| `SCREEN` | `Screen` | 16 | 一致 |
| `OVERLAY` | `Overlay` | 17 | 一致 |
| `DARKEN` | `Darken` | 18 | 一致 |
| `LIGHTEN` | `Lighten` | 19 | 一致 |
| `COLOR_DODGE` | `ColorDodge` | 20 | 一致 |
| `COLOR_BURN` | `ColorBurn` | 21 | 一致 |
| `LINEAR_BURN` | `LinearBurn` | 22 | 一致 |
| `LINEAR_LIGHT` | `LinearLight` | 23 | 一致 |
| `PIN_LIGHT` | `PinLight` | 24 | 一致 |
| `HARD_LIGHT` | `HardLight` | 25 | 一致 |
| `SOFT_LIGHT` | `SoftLight` | 26 | 一致 |
| `DIFFERENCE` | `Difference` | 27 | 一致 |
| `EXCLUSION` | `Exclusion` | 28 | 一致 |

## ストローク

### StrokeCap

| Blend2D | raden | 値 | 状態 |
|---------|-------|-----|------|
| `BL_STROKE_CAP_BUTT` | `StrokeCap::Butt` | 0 | 一致 (デフォルト) |
| `BL_STROKE_CAP_SQUARE` | `StrokeCap::Square` | 1 | 一致 |
| `BL_STROKE_CAP_ROUND` | `StrokeCap::Round` | 2 | 一致 |
| `BL_STROKE_CAP_ROUND_REV` | なし | 3 | 未実装: 反転丸キャップ |
| `BL_STROKE_CAP_TRIANGLE` | なし | 4 | 未実装: 三角形キャップ |
| `BL_STROKE_CAP_TRIANGLE_REV` | なし | 5 | 未実装: 反転三角形キャップ |

### StrokeJoin

| Blend2D | raden | 値 | 状態 |
|---------|-------|-----|------|
| `BL_STROKE_JOIN_MITER_CLIP` | `StrokeJoin::MiterClip` | 0 | 一致 (デフォルト) |
| `BL_STROKE_JOIN_MITER_BEVEL` | `StrokeJoin::MiterBevel` | 1 | 一致 |
| `BL_STROKE_JOIN_MITER_ROUND` | `StrokeJoin::MiterRound` | 2 | 一致 |
| `BL_STROKE_JOIN_BEVEL` | `StrokeJoin::Bevel` | 3 | 一致 |
| `BL_STROKE_JOIN_ROUND` | `StrokeJoin::Round` | 4 | 一致 |

### StrokeTransformOrder

| Blend2D | raden | 値 | 状態 |
|---------|-------|-----|------|
| `BL_STROKE_TRANSFORM_ORDER_AFTER` | なし | 0 | 未実装: raden は上記「ストロークスタイル / オプション」の `stroke_transform_order` 行のとおり固定実装 |
| `BL_STROKE_TRANSFORM_ORDER_BEFORE` | なし | 1 | 未実装 |

## フォント API

### BLFontData / FontData

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLFontData::create_from_file(path, flags)` | `FontData::from_file(path)` | 差異あり: raden は読み込みフラグがない |
| `BLFontData::create_from_data(void*, size, ...)` | `FontData::from_bytes(Vec<u8>)` | 差異あり: raden は所有権を取得 |
| `face_type()` / `face_count()` / `flags()` / `is_collection()` | なし | 未実装: フォントデータのメタ情報取得 |
| `get_table(face_index, table*, tag)` / `get_tables(...)` / `get_table_tags(...)` | なし | 未実装: テーブルへの直接アクセス |
| なし | `FontData::data()` -> `&[u8]` | raden 独自: 生バイト列の取得 |

### BLFontFace / FontFace

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLFontFace::create_from_file(path, flags)` | なし | 未実装: raden は FontData 経由の 2 段階設計 |
| `BLFontFace::create_from_data(BLFontData, face_index)` | `FontFace::from_data(&FontData, index)` | 一致 |
| `design_metrics()` | `units_per_em()` / `ascent()` / `descent()` / `line_gap()` / `cap_height()` / `x_height()` | 差異あり: Blend2D は構造体で一括取得、raden は個別メソッド |
| `face_type()` / `face_flags()` / `face_index()` / `face_info()` | なし | 未実装 |
| `outline_type()` / `diag_flags()` | なし | 未実装 |
| `unique_id()` | なし | 未実装 |
| `weight()` / `stretch()` / `style()` | なし | 未実装: フォントのウェイト/幅/スタイル |
| `units_per_em()` / `glyph_count()` | `units_per_em()` のみ | 差異あり: `glyph_count()` がない |
| `family_name()` / `full_name()` / `post_script_name()` / `subfamily_name()` | なし | 未実装: フォント名の取得 |
| `data()` | なし | 未実装: BLFontData への参照取得 |
| `panose_info()` / `coverage_info()` | なし | 未実装 |
| `has_face_flag(...)` / `has_*()` 系 (多数) | なし | 未実装: 機能フラグの問い合わせ |
| `has_feature_tag()` / `has_script_tag()` / `has_variation_tag()` | なし | 未実装 |
| `get_feature_tags()` / `get_script_tags()` / `get_variation_tags()` / `get_character_coverage()` | なし | 未実装 |

### BLFont / Font

| Blend2D | raden | 状態 |
|---------|-------|------|
| `create_from_face(BLFontFace, float)` | `Font::from_face(&FontFace, f64)` | 一致 |
| `create_from_face(BLFontFace, float, BLFontFeatureSettings)` | なし | 未実装: feature 付き生成 |
| `create_from_face(BLFontFace, float, BLFontFeatureSettings, BLFontVariationSettings)` | なし | 未実装: feature + variation 付き生成 |
| `size()` / `set_size(float)` | `size()` のみ | 差異あり: `set_size()` がない |
| `face()` | なし | 未実装: 元の FontFace への参照取得 |
| `face_type()` / `face_flags()` | なし | 未実装 |
| `weight()` / `stretch()` / `style()` | なし | 未実装 |
| `units_per_em()` | なし | 未実装: raden は `Font::scale()` で代替 |
| `matrix()` / `metrics()` / `design_metrics()` | `ascent()` / `descent()` / `line_gap()` / `cap_height()` / `x_height()` | 差異あり: Blend2D は構造体で一括取得 |
| `feature_settings()` / `set_feature_settings()` / `reset_feature_settings()` | なし | 未実装: OpenType feature 設定 |
| `variation_settings()` / `set_variation_settings()` / `reset_variation_settings()` | なし | 未実装: Variable Fonts 設定 |
| `shape(BLGlyphBuffer&)` | なし | 未実装: OpenType シェーピング |
| `map_text_to_glyphs(BLGlyphBuffer&)` | `map_char_to_glyph(char)` -> `u16` / `Font::glyph_run_for_text` (`pub(crate)`) | 差異あり: Blend2D はバッファ単位、raden は文字単位の公開 API と `pub(crate)` の内部ヘルパーで対応 |
| `position_glyphs(BLGlyphBuffer&)` | `glyph_advance(u16)` -> `f64` / `Font::glyph_run_for_text` (`pub(crate)`) | 差異あり: Blend2D はバッファ内全グリフを一括配置、raden は文字単位の公開 API `glyph_advance` と `pub(crate)` の内部ヘルパーで対応 |
| `apply_kerning(BLGlyphBuffer&)` | なし | 未実装: カーニング適用 |
| `apply_gsub(BLGlyphBuffer&, BLBitArray&)` / `apply_gpos(...)` | なし | 未実装: 個別 OpenType lookup 適用 |
| `get_glyph_outlines(...)` | `append_glyph_outline(glyph_id, offset_x, offset_y, &mut Path)` | 一致 |
| `get_glyph_bounds(...)` | `FontFace::glyph_bounds(u16) -> Option<GlyphBounds>` / `Font::glyph_bounds(u16) -> Option<GlyphBounds>` | 未実装: グリフ境界ボックスの一括取得 |
| `get_glyph_advances(...)` | なし | 未実装: グリフ advance 幅の一括取得 |
| `get_glyph_run_outlines(...)` | なし | 未実装: GlyphRun アウトラインの取得 |
| `get_text_metrics(BLGlyphBuffer&, BLTextMetrics&)` | `Font::measure_text(&str)` -> `TextMetrics` | 差異あり: raden は `&str` 入力で `TextMetrics { advance: f64 }` を返す個別取得 (`#[non_exhaustive]`)。Blend2D の `BLTextMetrics` (`advance: BLPoint`, `leading_bearing` 等の 4 フィールド) には将来段階的に拡張予定 |
| `BLGlyphBuffer` | なし | 未実装: グリフバッファ。raden は `fill_text` 内で 1 文字ずつ処理 |
| なし | `Font::scale()` -> `f64` | raden 独自: スケール係数取得 (size / units_per_em) |

## 画像 API

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLImage::create(w, h, format)` | `Image::new(w, h, format)` | 一致 |
| `BLImage::create_from_data(w, h, format, pixel_data, stride, ...)` | なし | 未実装: 外部バッファからの画像作成 |
| `width()` / `height()` | `width()` / `height()` | 一致 |
| `size()` -> `BLSizeI` | なし | 未実装: サイズの一括取得 |
| `format()` | `format()` | 一致 |
| `depth()` | なし | 未実装: ビット深度の取得 |
| `is_empty()` | なし | 未実装 |
| `equals(const BLImage&)` | なし | 未実装 |
| `get_data(BLImageData*)` / `make_mutable(BLImageData*)` | `data()` / `data_mut()` / `data_ptr_mut()` | 差異あり: Blend2D は構造体で返す |
| `convert(BLFormat)` | なし | 未実装: ピクセルフォーマット変換 |
| `read_from_file(path, ...)` | なし | 未実装: 画像ファイルの読み込み (PNG, JPEG 等) |
| `read_from_data(void*, size, ...)` | なし | 未実装: メモリバッファからの画像読み込み |
| `write_to_file(path, BLImageCodec)` | `write_to_file(path)` | 差異あり: raden は BMP 形式のみ。コーデック指定なし |
| `write_to_data(BLArray<uint8_t>&, BLImageCodec)` | なし | 未実装: メモリバッファへの画像書き込み |
| `BLPixelConverter` | なし | 未実装: ピクセルフォーマット間の変換 |
| `BLFormat` (Prgb32, Xrgb32, A8) | `PixelFormat` (Prgb32, Xrgb32, A8) | 一致: `A8` はパターン・グラデ塗りは未対応 (`fill_path` も未対応) |
| なし | `Image::stride()` | raden 独自: 行バイト数取得 |

## PathCmd の定義

| Blend2D | raden | 値 | 状態 |
|---------|-------|-----|------|
| `BL_PATH_CMD_MOVE` | `PathCmd::MoveTo` | 0 | 一致 |
| `BL_PATH_CMD_ON` | `PathCmd::LineTo` | 1 | 名前が異なる: Blend2D は「制御点上 (on-curve)」の意味、raden は用途を明示 |
| `BL_PATH_CMD_QUAD` | `PathCmd::QuadTo` | 2 | 一致 |
| `BL_PATH_CMD_CONIC` | `PathCmd::ConicTo` | 3 | 実装済み: 重みは `conic_weights` 列で保持 |
| `BL_PATH_CMD_CUBIC` | `PathCmd::CubicTo` | 4 | 一致 |
| `BL_PATH_CMD_CLOSE` | `PathCmd::Close` | 5 | 一致 |
| `BL_PATH_CMD_WEIGHT` | なし | 6 | 未実装: コニック曲線の重み値 (x 成分のみ使用) |

## raden 独自の公開 API

| API | 説明 |
|-----|------|
| `PipelineRuntime` | JIT コンパイル済みパイプラインのキャッシュ。Blend2D は内部で管理するが raden は外部から注入する設計 |
| `stroke_to_fill()` / `stroke_to_fill_with_workspace()` | パスのストローク輪郭を別の Path に変換する公開ユーティリティ |
| `StrokeOptions` / `StrokeWorkspace` | ストローク変換のオプションとワークスペース。dash_array / dash_offset を含む |
| `premultiply_rgba(r, g, b, a)` -> `u32` | RGBA から premultiplied ARGB への変換関数 |
| `Gradient` / `GradientStop` / `GradientValues` | グラデーション定義。Linear/Radial/Conic の 3 種別 |
| `LinearGradientValues` / `RadialGradientValues` / `ConicGradientValues` | 各グラデーション種別のパラメータ |
| `ExtendMode` | グラデーション/パターンの拡張モード (Pad, Repeat, Reflect) |
| `Pattern` / `PatternFilter` | 画像パターンと補間モード。行列は `set_transform`、原点は `set_origin` |

## 課題一覧

1. ~~**グラデーション / パターンが未実装**~~ → **実装済み**
   - Linear/Radial/Conic グラデーション (LUT ベース、固定小数点、JIT F32X4 SIMD)
   - 画像パターン (Nearest / Bilinear、`set_origin` / `set_transform`、コンテキスト行列と `prepare` で合成)

2. ~~**`PixelFormat` が `Prgb32` のみ**~~ → **`Xrgb32` / `A8` を追加済み**（`A8` 宛てはパターン・グラデ・`fill_path` 制限あり）

3. ~~**ダッシュ線 (`dash_array` / `dash_offset`) が未実装**~~ → **実装済み**
   - SVG 準拠のダッシュパターン分断アルゴリズム

4. ~~**クリッピングが未実装**~~ → **実装済み**
   - `clip_to_rect()` / `restore_clipping()` を実装済み

5. ~~**画像転送 (`blit_image`) が未実装**~~ → **`blit_image_rect` / `blit_image_at` を実装済み**（合成モード・フォーマットに制限あり）

6. **OpenType シェーピングが未実装**
   - `shape()` / `applyKerning()` / `applyGSub()` / `applyGPos()` がない
   - Variable Fonts も未対応
   - `BLGlyphBuffer` 相当がなく、raden は 1 文字ずつ処理

7. **Path の高度な操作が一部未実装**
   - 実装済み: `translate` / `transform` / `add_path` / `add_path_translated` / `add_path_transformed` / `bounding_box` / `control_box`
   - `bounding_box` は現在制御点ベース。ベジェ曲線の厳密な bbox は今後対応
   - 未実装: ヒットテスト (`hit_test`)、範囲指定 (`BLRange`) 版

8. ~~**Context の getter メソッドがない**~~ → **実装済み**
   - `comp_op()` / `fill_rule()` / `fill_color_prgb32()` / `fill_gradient()` / `fill_pattern()` / `stroke_color_prgb32()` / `stroke_width()` / `stroke_miter_limit()` / `stroke_join()` / `stroke_start_cap()` / `stroke_end_cap()` / `stroke_dash_array()` / `stroke_dash_offset()` / `matrix()`（`src/api/context.rs`）

9. **フォントモジュールのテストがない**
   - テーブルパーサ、グリフアウトライン変換、cmap ルックアップの PBT / 単体テスト / fuzzing が未整備

10. **画像出力が BMP のみ**
    - `write_to_file()` は BMP 形式のみ。Blend2D は BLImageCodec でコーデックを指定

11. ~~**個別アルファ (`fill_alpha` / `stroke_alpha`) とグローバルアルファが未実装**~~ → **実装済み (一部制限あり)**
    - `global_alpha` / `fill_alpha` / `stroke_alpha` を実装。実効アルファは `global_alpha * (fill_alpha or stroke_alpha)` で計算
    - 単色 fill/stroke、グラデーション fill/stroke、パターン fill/stroke の各経路で適用される
    - 制限: `blit_image_*` 系には未適用 (今後対応)。グラデ/パターンの `fill_rect` 高速パスは alpha != 1.0 のとき span path にフォールバックする (Linear gradient JIT cov、Radial JIT row 含む)

12. ~~**せん断変換 (`skew`) が未実装**~~ → **実装済み**
    - `Matrix2D::skewing` / `skew` / `post_skew`、`Context::skew` / `post_skew` (係数は接線)

13. **BLFontFace の詳細 API がほぼ未実装**
    - フォント名取得、機能フラグ問い合わせ、feature/script/variation タグ取得等

14. ~~**パターンの Bilinear 補間 / Affine 変換が未実装**~~ → **実装済み**
    - `PatternFilter::Nearest` / `Bilinear`、`Pattern::set_origin` / `set_transform`（アフィン）、`prepare` で `Context::matrix` と合成

15. ~~**ストロークのグラデーション/パターンスタイルが未実装**~~ → **実装済み**
    - `set_stroke_style_gradient` / `set_stroke_style_pattern` を実装済み (`stroke_path` 内で fill 側スタイルと一時差し替え)
