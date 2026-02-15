# Blend2D API との比較

## Context API

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLContext::begin(image)` | `Context::new(image, runtime)` | 差異あり: Rust 慣例で `new`。JIT キャッシュをフレーム間で再利用するため `PipelineRuntime` を外から渡す |
| `end()` | `end()` | 一致 (現在は no-op。将来のバッファフラッシュ用に予約) |
| `flush()` | なし | 不要: raden は全描画を同期実行するためコマンドバッファが存在しない |
| `save()` / `restore()` | `save()` / `restore()` | 一致 |
| `save()` / `restore()` with cookie | なし | 未実装: 名前付き save/restore (特定の cookie でのみ restore 可能) |
| `set_comp_op()` | `set_comp_op()` | 一致 |
| `set_fill_style(rgba32)` | `set_fill_style(Rgba32)` | 一致 |
| `set_fill_style(gradient)` | なし | 未実装: Linear/Radial/Conic グラデーション |
| `set_fill_style(pattern)` | なし | 未実装: 画像パターン塗りつぶし |
| `set_stroke_style(rgba32)` | `set_stroke_style(Rgba32)` | 一致 |
| `get_fill_style()` / `get_stroke_style()` | なし | 未実装: 現在のスタイルの取得 |
| `swap_styles()` | なし | 未実装: fill/stroke スタイルの交換 |
| `set_global_alpha()` | なし | 未実装: 全描画に適用されるグローバル透明度 |
| `set_fill_rule()` | `set_fill_rule(FillRule)` | 一致 (`NonZero` / `EvenOdd`) |
| `fill_all()` | `fill_all()` | 一致 |
| `fill_rect()` | `fill_rect()` | 一致 |
| `fill_circle()` | `fill_circle()` | 一致 |
| `fill_ellipse()` | なし | 未実装: `add_circle` は実装済みだが楕円は未対応 |
| `fill_round_rect()` | なし | 未実装: 角丸矩形 |
| `fill_pie()` | `fill_pie()` | 一致: Arc 型で扇形を塗りつぶし |
| `fill_path()` | `fill_path()` | 一致 |
| `fill_text()` | `fill_text()` | 一致 |
| `fill_mask()` | なし | 未実装: マスク画像を使った塗りつぶし描画 |
| `fill_geometry()` | なし | 未実装: ジオメトリ型の統合描画 API |
| `stroke_rect()` | `stroke_rect()` | 一致 |
| `stroke_circle()` | `stroke_circle()` | 一致 |
| `stroke_path()` | `stroke_path()` | 一致 |
| `stroke_line()` | `stroke_line()` | 一致 |
| `stroke_text()` | なし | 未実装: テキストのストローク描画 |
| `stroke_ellipse()` | なし | 未実装: 楕円ストローク |
| `stroke_round_rect()` | なし | 未実装: 角丸矩形ストローク |
| `stroke_geometry()` | なし | 未実装: ジオメトリ型の統合ストローク API |
| `set_stroke_width()` | `set_stroke_width()` | 一致 |
| `set_stroke_cap()` | `set_stroke_cap()` / `set_stroke_start_cap()` / `set_stroke_end_cap()` | 一致: 一括/個別どちらでも設定可能 |
| `set_stroke_join()` | `set_stroke_join()` | 一致 |
| `set_stroke_miter_limit()` | `set_stroke_miter_limit()` | 一致 |
| `set_stroke_dash_array()` | なし | 未実装: 点線/破線パターン |
| `set_stroke_transform_order()` | なし | 未実装: 現在は stroke-before-transform 動作のみ |
| `clear_all()` / `clear_rect()` | なし | 未実装: `CompOp::Clear` + `fill_rect` で代替可能 |
| `clip_to_rect()` / `restore_clipping()` | なし | 未実装: 矩形クリッピング |
| `translate()` / `scale()` / `rotate()` | `translate()` / `scale()` / `rotate()` | 一致: `Matrix2D` による座標変換。`apply_matrix()` / `reset_matrix()` / `user_to_meta()` も提供 |
| `blit_image()` / `blit_scaled_image()` | なし | 未実装: 画像の直接転送 / スケーリング転送 |
| `set_hint()` | なし | 未実装: レンダリング品質ヒント (Gradient/Pattern quality 等) |
| `accumulated_error_flags()` | なし | 未実装: 非同期描画中のエラーフラグ蓄積 |
| テキスト描画 (UTF-8/16/32/GlyphRun) | `fill_text()` (UTF-8 のみ) | 差異あり: Blend2D はエンコーディング別に API を提供 |
| `set_approximation_options()` | なし | 未実装: カーブの近似設定 (flatten tolerance 等) |

## Path API

| Blend2D | raden | 状態 |
|---------|-------|------|
| `move_to()` | `move_to()` | 一致 |
| `line_to()` | `line_to()` | 一致 |
| `cubic_to()` | `cubic_to()` | 一致 |
| `quad_to()` | `quad_to()` | 一致 |
| `smooth_quad_to()` | なし | 未実装: 前の二次ベジェの制御点を反射した smooth curve |
| `smooth_cubic_to()` | なし | 未実装: 前の三次ベジェの制御点を反射した smooth curve |
| `conic_to()` | なし | 未実装: 円錐曲線 (PathCmd 値 3 は欠番で確保済み) |
| `arc_to()` | なし | 未実装: 円弧 |
| `arc_quadrant_to()` | なし | 未実装: 象限単位の円弧 (90 度以下) |
| `elliptic_arc_to()` | なし | 未実装: 楕円弧 (SVG arc コマンド相当) |
| `close()` | `close()` | 一致 |
| `clear()` | `clear()` | 一致 |
| `is_empty()` / `len()` | `is_empty()` / `len()` | 一致 |
| なし | `cmds()` / `points()` | raden 独自: パス内部データへのアクセサ |
| `add_circle()` | `add_circle()` | 一致 |
| なし | `add_pie()` | raden 独自: 扇形を Path に追加 (`fill_pie` 内部で使用) |
| `add_rect()` | なし | 未実装: `move_to` / `line_to` / `close` で代替可能 |
| `add_round_rect()` | なし | 未実装: 角丸矩形 (cubic_to による近似が必要) |
| `add_ellipse()` | なし | 未実装: 楕円 |
| `add_path()` | なし | 未実装: 他の Path のコマンドを結合 |
| `add_translated_path()` | なし | 未実装: オフセット付きで別の Path を追加 |
| `add_transformed_path()` | なし | 未実装: 変換行列を適用して別の Path を追加 |
| `add_reversed_path()` | なし | 未実装: 反転させて別の Path を追加 |
| `add_stroked_path()` | なし | 未実装: ストローク結果を Path に追加 |
| `translate()` / `transform()` | なし | 未実装: パス自体の座標変換 |
| `fit_to()` | なし | 未実装: バウンディングボックスへのフィット |
| `remove_range()` | なし | 未実装: パス範囲の削除 |
| `reserve()` / `shrink()` | なし | 未実装: 内部バッファの事前確保/縮小 |
| `get_bounding_box()` | なし | 未実装: `fill_path` 内部ではバウンディングボックスを計算済み |
| `get_control_box()` | なし | 未実装: コントロール点のバウンディングボックス取得 |
| `get_info_flags()` | なし | 未実装: パスの内容フラグ取得 |
| `get_figure_range()` | なし | 未実装: 指定インデックスの figure 範囲取得 |
| `get_last_vertex()` | なし | 未実装: 最後の頂点座標取得 |
| `get_closest_vertex()` | なし | 未実装: 指定点に最も近い頂点を検索 |
| `hit_test()` | なし | 未実装: 点がパス内部にあるか判定 |

## ジオメトリ型

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLPoint` (f64) | `Point` (f64) | 一致 |
| `BLPointI` (i32) | なし | 未実装: 整数座標版 |
| `BLRect` (x, y, w, h) | `Rect` (x, y, w, h) | 一致 |
| `BLRectI` | なし | 未実装: 整数座標版 |
| `BLBox` (x0, y0, x1, y1) | なし | 未実装: 2 点指定の矩形 (Rect は x,y,w,h 方式) |
| `BLBoxI` | なし | 未実装: 整数座標版 |
| `BLSize` / `BLSizeI` | なし | 未実装: サイズ型 |
| `BLCircle` | `Circle` | 一致 |
| `BLEllipse` | なし | 未実装: 楕円 |
| `BLRoundRect` | なし | 未実装: 角丸矩形 |
| `BLLine` | `Line` | 一致 |
| `BLTriangle` | なし | 未実装: 三角形 (Path で代替可能) |
| `BLArc` | `Arc` | 一致: 円弧 (cx, cy, rx, ry, start, sweep) |
| `BLChord` | なし | 未実装: 円弧の弦で閉じた図形 |
| `BLMatrix2D` | `Matrix2D` | 一致: 2D アフィン変換行列。Blend2D と同一レイアウト |
| Polyline / Polygon | なし | 未実装: 複数点列 (Path で代替可能) |

## Matrix2D API

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLMatrix2D::make_identity()` | `Matrix2D::IDENTITY` | 一致: raden は const で提供 |
| `BLMatrix2D::make_translation()` | `Matrix2D::translation()` | 一致 |
| `BLMatrix2D::make_scaling()` | `Matrix2D::scaling()` | 一致 |
| `BLMatrix2D::make_rotation()` | `Matrix2D::rotation()` | 一致 |
| `translate()` / `scale()` / `rotate()` | `translate()` / `scale()` / `rotate()` | 一致 |
| `reset()` | `reset()` | 一致 |
| `multiply()` | `multiply()` | 一致 |
| `map_point_d()` | `map_point()` | 一致 |
| なし | `is_identity()` | raden 独自: 単位行列かどうかの判定 |
| `invert()` | なし | 未実装: 逆行列計算 |
| `type()` | なし | 未実装: 行列種別判定 (IDENTITY/TRANSLATE/SCALE/AFFINE 等) |
| `map_point_d_array()` | なし | 未実装: 複数点のバッチ変換 |
| Post-transform 系 (`post_translate` 等) | なし | 未実装: 行列右掛け (POST_TRANSLATE/POST_SCALE/POST_ROTATE 等) |
| `skew()` / `post_skew()` | なし | 未実装: せん断変換 |

## 色 / スタイル

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLRgba32` | `Rgba32` | 一致 |
| `BLRgba64` | なし | 未実装: 16-bit/チャネル色 |
| `BLRgba` (float) | なし | 未実装: 浮動小数点色 |
| `BLCompOp` (31 種類) | `CompOp` (29 種類) | 部分対応: HSL 系 (Hue, Saturation) 2 種類は未実装 |
| `BLGradient` (Linear/Radial/Conic) | なし | 未実装: グラデーション塗りつぶし |
| `BLPattern` | なし | 未実装: 画像パターン塗りつぶし |
| `BLExtendMode` | なし | 未実装: グラデーション/パターンの繰り返しモード (Pad, Repeat, Reflect) |

### Rgba32 メソッド

| Blend2D | raden | 状態 |
|---------|-------|------|
| コンストラクタ | `Rgba32::new(r, g, b, a)` / `Rgba32::rgb(r, g, b)` | 一致 |
| チャネル取得 | `r()` / `g()` / `b()` / `a()` | 一致 |
| なし | `is_opaque()` / `is_transparent()` | raden 独自: 透明度判定 |
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
| `HUE` | なし | 29 | 未実装: HSL 系 |
| `SATURATION` | なし | 30 | 未実装: HSL 系 |

## ストローク

| Blend2D | raden | 状態 |
|---------|-------|------|
| `width` | `width` | 一致 |
| `start_cap` / `end_cap` (別々) | `start_cap` / `end_cap` (別々) | 一致: `set_stroke_cap()` で一括設定、`set_stroke_start_cap()` / `set_stroke_end_cap()` で個別設定も可能 |
| `BLStrokeCap::Butt` | `StrokeCap::Butt` | 一致 |
| `BLStrokeCap::Square` | `StrokeCap::Square` | 一致 |
| `BLStrokeCap::Round` | `StrokeCap::Round` | 一致 |
| `BLStrokeCap::RoundRev` | なし | 未実装: 反転丸キャップ |
| `BLStrokeCap::Triangle` | なし | 未実装: 三角形キャップ |
| `BLStrokeCap::TriangleRev` | なし | 未実装: 反転三角形キャップ |
| `BLStrokeJoin::MiterClip` | `StrokeJoin::MiterClip` | 一致 (デフォルト、値 0) |
| `BLStrokeJoin::MiterBevel` | `StrokeJoin::MiterBevel` | 一致 (値 1) |
| `BLStrokeJoin::MiterRound` | `StrokeJoin::MiterRound` | 一致 (値 2) |
| `BLStrokeJoin::Bevel` | `StrokeJoin::Bevel` | 一致 (値 3) |
| `BLStrokeJoin::Round` | `StrokeJoin::Round` | 一致 (値 4) |
| `miter_limit` | `miter_limit` | 一致 |
| `dash_array` / `dash_offset` | なし | 未実装: 点線/破線パターン |
| `BLStrokeTransformOrder` | なし | 未実装: 現在は stroke-before-transform 動作のみ |

## フォント API

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLFontData` | `FontData` | 一致: `from_file()` / `from_bytes()` でバイト列を保持 |
| `BLFontFace::createFromFile()` | `FontFace::from_data()` | 類似: raden は `FontData` でバイト列を保持し、そこから `FontFace` を生成する 2 段階設計。TTC (index 指定) 対応 |
| `BLFontFace::designMetrics()` | `FontFace::units_per_em()` / `ascent()` / `descent()` / `line_gap()` | 類似: Blend2D は構造体で一括取得、raden は個別メソッド |
| `BLFont::createFromFace()` | `Font::from_face()` | 一致 |
| `BLFont::size()` | `Font::size()` | 一致 |
| なし | `Font::scale()` | raden 独自: スケール係数取得 (size / units_per_em) |
| `BLFont::metrics()` | `Font::ascent()` / `Font::descent()` | 類似: Blend2D は構造体で一括取得、raden は個別メソッド (ピクセル単位) |
| `BLFont::shape()` | なし | 未実装: OpenType シェーピング (GSUB/GPOS) |
| `BLFont::mapTextToGlyphs()` | `Font::map_char_to_glyph()` | 差異あり: Blend2D はバッファ単位、raden は文字単位 |
| `BLFont::positionGlyphs()` | `Font::glyph_advance()` | 類似: Blend2D はバッファ内全グリフを一括配置、raden は 1 グリフずつ advance 幅を取得 |
| `BLFont::getGlyphOutlines()` | `Font::append_glyph_outline()` | 一致 |
| `BLFont::getGlyphBounds()` | なし | 未実装: グリフ境界ボックスの一括取得 |
| `BLFont::getGlyphAdvances()` | なし | 未実装: グリフ advance 幅の一括取得 |
| `BLFont::getTextMetrics()` | なし | 未実装: テキストメトリクスの一括取得 |
| `BLFont::getGlyphRunOutlines()` | なし | 未実装: GlyphRun アウトラインの取得 |
| `BLFont::applyKerning()` | なし | 未実装: カーニング適用 |
| `BLFont::applyGSub()` / `applyGPos()` | なし | 未実装: 個別 OpenType lookup 適用 |
| `BLGlyphBuffer` | なし | 未実装: グリフバッファ。raden は `fill_text` 内で 1 文字ずつ処理 |
| `BLFont::setFeatureSettings()` | なし | 未実装: OpenType feature (liga, kern 等) の有効化/無効化 |
| `BLFont::setVariationSettings()` | なし | 未実装: Variable Fonts (フォントバリエーション) |

## 画像 API

| Blend2D | raden | 状態 |
|---------|-------|------|
| `BLImage::create()` | `Image::new()` | 一致 |
| `BLImage::width()` / `height()` | `Image::width()` / `height()` | 一致 |
| `BLImage::format()` | `Image::format()` | 一致 |
| なし | `Image::stride()` | raden 独自: 行バイト数取得 |
| `BLImage::getData()` | `Image::data()` / `data_mut()` / `data_ptr_mut()` | 一致 |
| `BLImage::writeToFile()` | `Image::write_to_file()` | 一致 |
| `BLImage::readFromFile()` | なし | 未実装: 画像ファイルの読み込み (PNG, JPEG 等) |
| `BLImage::readFromData()` | なし | 未実装: メモリバッファからの画像読み込み |
| `BLImage::writeToData()` | なし | 未実装: メモリバッファへの画像書き込み |
| `BLImage::scale()` | なし | 未実装: 画像のリサイズ |
| `BLImage::convert()` | なし | 未実装: ピクセルフォーマット変換 |
| `BLPixelConverter` | なし | 未実装: ピクセルフォーマット間の変換 |
| `BLFormat` (Prgb32, Xrgb32, A8) | `PixelFormat` (Prgb32 のみ) | 不足: `Xrgb32` (アルファなし) と `A8` (アルファのみ) が未対応 |

## PathCmd の定義

| Blend2D | raden | 備考 |
|---------|-------|------|
| `MOVE` (0) | `MoveTo` (0) | 一致 |
| `ON` (1) | `LineTo` (1) | 名前が異なる: Blend2D は「制御点上 (on-curve)」の意味、raden は用途を明示 |
| `QUAD` (2) | `QuadTo` (2) | 値は一致 |
| `CONIC` (3) | なし | 未実装: 値 3 は欠番として確保済み |
| `CUBIC` (4) | `CubicTo` (4) | 値は一致 |
| `CLOSE` (5) | `Close` (5) | 値は一致 |

## 課題一覧

1. **`CompOp` が部分対応**
   - Porter-Duff 基本セット + Clear + Plus の 13 種類 + ブレンドモード 16 種類を JIT 実装済み
   - HSL 系 (Hue, Saturation) 2 種類は未実装

2. **グラデーション / パターンが未実装**
   - Blend2D の大きな特徴であるグラデーション (Linear, Radial, Conic) とパターンがない

3. **`PixelFormat` が `Prgb32` のみ**
   - Blend2D は `Xrgb32` と `A8` もサポートしている

4. **ダッシュ線 (`dash_array` / `dash_offset`) が未実装**
   - ストロークの点線/破線パターンが使えない

5. **クリッピングが未実装**
   - `clip_to_rect()` 等がない

6. **`BLGlyphBuffer` 相当がない**
   - Blend2D ではテキスト処理をバッファ単位で行う
   - raden は 1 文字ずつ処理している

7. **画像転送 (`blit_image`) が未実装**
   - 画像の直接転送やスケーリング転送がない

8. **OpenType シェーピングが未実装**
   - `shape()` / `applyKerning()` / `applyGSub()` / `applyGPos()` がない
   - Variable Fonts も未対応

9. **Path の高度な操作が未実装**
   - パス自体の座標変換 (`translate` / `transform`)
   - パスの結合 (`add_path` / `add_transformed_path`)
   - バウンディングボックス取得 (`get_bounding_box`)
   - ヒットテスト (`hit_test`)
