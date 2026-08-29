---
name: raden
description: 時雨堂の 2D ベクターグラフィックスライブラリ raden の使い方・API リファレンス。Cranelift JIT による CPU 専用ラスタライズ、Blend2D 互換 API (Context / Path / Gradient / Pattern / Font / Matrix2D)、OpenType 基本シェーピング (GSUB / GPOS、liga / kern / clig)、29 種類の合成モード (Porter-Duff + ブレンド)、フィル・ストローク・Blit 描画、PixelFormat (Prgb32 / Xrgb32 / A8)、BMP 出力に関する質問時に使用。CI 環境で GPU を使わず 1080p 120fps のダミー映像を生成したい場面で参照。
license: Apache-2.0
---

# Raden

時雨堂が開発する 2D ベクターグラフィックスライブラリ。Cranelift JIT を用いて CPU のみでラスタライズを行う。API は Blend2D にできるだけ寄せており、Blend2D 経験者がそのまま使い始められる。

## 目的と前提

- GPU が使えない CI 環境 (GitHub Actions など) で 1080p / 120fps 以上のダミー映像を生成する
- 対応プラットフォーム: Windows (x86_64) / macOS (arm64) / Ubuntu (x86_64 / arm64)
- 本格用途には Blend2D を推奨 (raden は実験的プロジェクト、API は予告なく変更される可能性がある)

## 依存関係

- cranelift-codegen / cranelift-frontend / cranelift-jit / cranelift-module / cranelift-native (~0.135)
- Rust edition 2024 / rust-version 1.95

## 公開型

```rust
// lib.rs の pub use
Context, Image, Matrix2D, Path, PathCmd, Point
Arc, Circle, Ellipse, Line, Rect, RoundRect, Triangle
Gradient, GradientStop, GradientValues, ExtendMode
LinearGradientValues, RadialGradientValues, ConicGradientValues
Pattern, PatternFilter
CompOp, FillRule, Rgba32, StrokeCap, StrokeJoin
Font, FontData, FontFace, FontError, FontFeatureSettings
GlyphBounds, GlyphBuffer, GlyphBufferIter, GlyphPlacement, TextMetrics
PipelineRuntime, PixelFormat, premultiply_rgba
```

## 最小コード例

```rust
use raden::{Circle, CompOp, Context, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut image = Image::new(320, 240, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut image, &mut runtime);

    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::rgb(0x20, 0x20, 0x60));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 320.0, 240.0));

    ctx.set_comp_op(CompOp::SrcOver);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x40, 0x40, 0xC0));
    ctx.fill_circle(&Circle::new(160.0, 120.0, 72.0));

    ctx.end();
    image.write_to_file("output.bmp")?;
    Ok(())
}
```

`Context::new` は `&mut Image` と `&mut PipelineRuntime` を借りる。`PipelineRuntime` は JIT コンパイル済みパイプラインのキャッシュを保持するため、フレームを跨いで再利用する。描画終了時は `ctx.end()` を呼ぶ。

## 描画 API

### フィル描画 (Context)

| メソッド | 説明 |
|---|---|
| `fill_all()` | 画像全体をフィルスタイルで塗りつぶす (内部で全画像矩形の `fill_rect`。クリップ適用) |
| `fill_rect(&Rect)` | 矩形塗りつぶし (クリッピング付き) |
| `fill_path(&Path)` | 任意パス塗りつぶし (ベジェ平坦化 + ラスタライズ) |
| `fill_circle(&Circle)` | 円塗りつぶし |
| `fill_ellipse(&Ellipse)` | 楕円塗りつぶし |
| `fill_round_rect(&RoundRect)` | 角丸矩形塗りつぶし |
| `fill_triangle(&Triangle)` | 三角形塗りつぶし |
| `fill_polygon(&[Point])` | ポリゴン塗りつぶし (3 点未満は no-op) |
| `fill_pie(&Arc)` | 扇形塗りつぶし |
| `fill_text(x, y, &Font, text)` | テキスト描画。OpenType GSUB / GPOS シェーピング適用後、全グリフを 1 つの Path に結合して `fill_path` |

### ストローク描画 (Context)

| メソッド | 説明 |
|---|---|
| `stroke_line(&Line)` | 線分 |
| `stroke_rect(&Rect)` | 矩形 |
| `stroke_circle(&Circle)` | 円 |
| `stroke_ellipse(&Ellipse)` | 楕円 |
| `stroke_round_rect(&RoundRect)` | 角丸矩形 |
| `stroke_triangle(&Triangle)` | 三角形 |
| `stroke_polygon(&[Point])` | ポリゴン (閉じる) |
| `stroke_polyline(&[Point])` | 折れ線 (閉じない) |
| `stroke_path(&Path)` | 任意パス (stroke-to-fill 変換後 `fill_path`) |
| `stroke_text(x, y, &Font, text)` | テキスト輪郭のストローク。シェーピング適用は `fill_text` と同じ |

### クリア / Blit (Context)

| メソッド | 説明 |
|---|---|
| `clear_all()` | 現在のクリップ領域をピクセル値 0 で書き換える (`comp_op` は変更しない) |
| `clear_rect(&Rect)` | 指定矩形をピクセル値 0 で書き換える (デバイス座標、変換行列は無視) |
| `blit_image_at(x, y, &Image)` | ソース画像全体を (x, y) に Nearest 転送 |
| `blit_image_rect(&Rect, &Image, Option<Rect>)` | ソース矩形を宛先矩形に Nearest 転送。`CompOp` は `SrcOver` / `SrcCopy` のみ |

### 変換 (Context)

後乗算 (`matrix = matrix * T`) は Blend2D / SVG / Canvas と互換。前乗算版 (`post_*`) も提供する。

| メソッド | 説明 |
|---|---|
| `translate(tx, ty)` | 平行移動 (後乗算) |
| `scale(sx, sy)` | スケーリング (後乗算) |
| `rotate(angle)` | 回転 (ラジアン、後乗算) |
| `rotate_around(angle, cx, cy)` | 指定中心まわりの回転 (後乗算) |
| `skew(kx, ky)` | せん断 (係数は接線、後乗算) |
| `apply_matrix(&Matrix2D)` | 任意行列 (後乗算) |
| `post_translate` / `post_scale` / `post_rotate` / `post_skew` / `post_transform` | 前乗算版 |
| `reset_matrix()` | 単位行列にリセット |
| `user_to_meta()` | Blend2D 互換エントリポイント (raden はユーザ行列をリセットするのみ) |
| `matrix()` | 現在の変換行列への参照 |

### 状態管理 / クリッピング (Context)

| メソッド | 説明 |
|---|---|
| `save()` | 描画状態 (スタイル、変換、クリップ等) をスタックに保存 |
| `restore()` | スタックから描画状態を復元 |
| `clip_to_rect(&Rect)` | クリップ領域を指定矩形との積集合に縮小 (拡大不可) |
| `restore_clipping()` | クリップ領域を画像境界 (メタクリップ) に戻す |

### スタイル設定 (Context)

| メソッド | 説明 |
|---|---|
| `set_fill_style(Rgba32)` | フィルを単色に |
| `set_fill_style_gradient(&Gradient)` | フィルをグラデーションに |
| `set_fill_style_pattern(&Pattern)` | フィルを画像パターンに |
| `set_stroke_style(Rgba32)` | ストロークを単色に |
| `set_stroke_style_gradient(&Gradient)` | ストロークをグラデーションに |
| `set_stroke_style_pattern(&Pattern)` | ストロークを画像パターンに |
| `set_fill_rule(FillRule)` | 塗りつぶし規則 (`NonZero` / `EvenOdd`) |
| `set_comp_op(CompOp)` | 合成モード |
| `set_global_alpha(a)` | 全描画に乗算されるグローバルアルファ ([0, 1]) |
| `set_fill_alpha(a)` | フィル個別アルファ ([0, 1]) |
| `set_stroke_alpha(a)` | ストローク個別アルファ ([0, 1]) |

ゲッター: `comp_op` / `fill_rule` / `fill_color_prgb32` / `fill_gradient` / `fill_pattern` / `stroke_color_prgb32` / `stroke_gradient` / `stroke_pattern` / `stroke_width` / `stroke_miter_limit` / `stroke_join` / `stroke_start_cap` / `stroke_end_cap` / `stroke_dash_array` / `stroke_dash_offset` / `global_alpha` / `fill_alpha` / `stroke_alpha`。

### ストロークパラメータ

| メソッド | 説明 |
|---|---|
| `set_stroke_width(f64)` | ストローク幅 |
| `set_stroke_cap(StrokeCap)` | 線端の形状 (`Butt` / `Square` / `Round`) |
| `set_stroke_start_cap` / `set_stroke_end_cap` | 始端・終端を個別設定 |
| `set_stroke_join(StrokeJoin)` | 接合部の形状 (`MiterClip` / `MiterBevel` / `MiterRound` / `Bevel` / `Round`) |
| `set_stroke_miter_limit(f64)` | マイター限界値 (デフォルト 4.0) |
| `set_stroke_dash_array(&[f64])` | ダッシュパターン (SVG 準拠、奇数パターンは 2 回繰り返す) |
| `set_stroke_dash_offset(f64)` | ダッシュパターンの開始オフセット |

## グラデーション (Gradient)

| 種類 | 説明 |
|---|---|
| Linear | 2 点間の線形グラデーション |
| Radial | 中心・焦点・半径による放射グラデーション |
| Conic | 中心と角度による円錐グラデーション |

範囲外処理 (`ExtendMode`): `Pad` (デフォルト) / `Repeat` / `Reflect`。

- `fill_rect` でグラデーションを塗ると `set_comp_op` の値を参照せず SrcOver 相当で合成する
- `fill_path` 経由では `CompOp` がパイプラインで適用される

## パターン (Pattern)

画像をタイルとして繰り返す塗りつぶし。`set_origin` / `set_transform`、`PatternFilter` (`Nearest` / `Bilinear`)、`ExtendMode` (`Pad` / `Repeat` / `Reflect`) に対応。

- `fill_rect` でパターンを塗るときは `set_comp_op` が `SrcOver` または `SrcCopy` のみ対応 (それ以外はパニック)
- `fill_path` 経由では全 `CompOp` が適用できる

## 合成モード (CompOp)

Porter-Duff 基本セット + Clear + Plus の 13 種類とブレンド 16 種類の計 29 種類。

### Porter-Duff 合成

| モード | 値 | 式 |
|---|---|---|
| `SrcOver` | 0 | `out = src + dst * (1 - srcA)` (デフォルト) |
| `SrcCopy` | 1 | `out = src` |
| `SrcIn` | 2 | `out = src * dstA` |
| `SrcOut` | 3 | `out = src * (1 - dstA)` |
| `SrcAtop` | 4 | `out = src * dstA + dst * (1 - srcA)` |
| `DstOver` | 5 | `out = dst + src * (1 - dstA)` |
| `DstCopy` | 6 | `out = dst` |
| `DstIn` | 7 | `out = dst * srcA` |
| `DstOut` | 8 | `out = dst * (1 - srcA)` |
| `DstAtop` | 9 | `out = dst * srcA + src * (1 - dstA)` |
| `Xor` | 10 | `out = src * (1 - dstA) + dst * (1 - srcA)` |
| `Clear` | 11 | `out = 0` |
| `Plus` | 12 | `out = min(src + dst, 1)` |

### ブレンドモード

| モード | 値 | 説明 |
|---|---|---|
| `Minus` | 13 | 飽和減算。`out = max(dst - src, 0)` |
| `Modulate` | 14 | チャネル乗算。`out = src * dst` |
| `Multiply` | 15 | Multiply ブレンド |
| `Screen` | 16 | `out = src + dst - src * dst` |
| `Overlay` | 17 | 暗部 Multiply、明部 Screen |
| `Darken` | 18 | `min(src * dstA, dst * srcA)` |
| `Lighten` | 19 | `max(src * dstA, dst * srcA)` |
| `ColorDodge` | 20 | 覆い焼き (除算ベース) |
| `ColorBurn` | 21 | 焼き込み (除算ベース) |
| `LinearBurn` | 22 | `max(src + dst - srcA * dstA, 0)` |
| `LinearLight` | 23 | LinearDodge + LinearBurn |
| `PinLight` | 24 | 条件分岐 + min/max |
| `HardLight` | 25 | Overlay の src/dst 入替 |
| `SoftLight` | 26 | 除算 + sqrt |
| `Difference` | 27 | `abs(src * dstA - dst * srcA)` |
| `Exclusion` | 28 | `out = src + dst - 2 * src * dst` |

## 塗りつぶし規則 (FillRule)

| ルール | 説明 |
|---|---|
| `NonZero` | ワインディングナンバーが非ゼロなら内側 (デフォルト) |
| `EvenOdd` | ワインディングナンバーが奇数なら内側 |

## フォント

| 型 | 説明 |
|---|---|
| `FontData` | フォントファイルのバイトデータ。`from_file(path)` (`AsRef<Path>`) / `from_bytes(bytes)` |
| `FontFace` | パース済みフォントフェイス。テーブル: head / maxp / hhea / hmtx / cmap / loca / glyf / OS/2 / GSUB / GPOS。`units_per_em` / `ascent` / `descent` / `line_gap` / `cap_height` / `x_height` / `glyph_bounds` |
| `Font` | サイズ指定済み。`from_face` / `with_features` で作成。`fill_text` / `stroke_text` に渡す |
| `FontFeatureSettings` | liga / kern / clig の ON/OFF。デフォルトは全 ON。`none()` / `with_kern` / `with_liga` / `with_clig` |
| `GlyphBuffer` | `shape` / `shape_into` の結果。`glyph_id` / `placement` / `cluster` / `iter` |
| `GlyphBufferIter` | `GlyphBuffer::iter` の戻り値。`(glyph_id, GlyphPlacement, cluster)` |
| `GlyphPlacement` | advance / offset_x / offset_y (ピクセル単位、offset は Y up) |
| `GlyphBounds` | x_min / y_min / x_max / y_max。Y up、baseline 原点。描画時の Y 反転は適用しない |
| `TextMetrics` | `measure_text` の結果。advance / bounding_box / leading_bearing / trailing_bearing (シェーピング適用済み) |

### Font の主なメソッド

| メソッド | 説明 |
|---|---|
| `from_face(&FontFace, size)` | デフォルト feature (全 ON) で作成 |
| `with_features(&FontFace, size, FontFeatureSettings)` | feature 指定で作成 |
| `clone_with_features(FontFeatureSettings)` | face / size を維持して feature だけ変えた複製 |
| `set_feature_settings` / `feature_settings` | feature の変更・参照 |
| `shape(&str) -> GlyphBuffer` | cmap → GSUB → GPOS (kern 有効時) でシェーピング |
| `shape_into(&str, &mut GlyphBuffer)` | 既存バッファを再利用して shape |
| `measure_text(&str) -> TextMetrics` | シェーピング適用済みの advance / bbox / bearing |
| `map_char_to_glyph` / `glyph_advance` / `append_glyph_outline` | 単一グリフ操作 |
| `ascent` / `descent` / `line_gap` / `cap_height` / `x_height` / `glyph_bounds` | スケール済みメトリクス (ピクセル) |

シェーピング対応範囲: GSUB Single / Ligature (Type 1/4)、GPOS Single / Pair (Type 1/2)。`kern` は GPOS Pair Adjustment。Microsoft `kern` テーブル v0 と可変フォントは未対応。

## パス (Path)

| コマンド | 説明 |
|---|---|
| `move_to(x, y)` | サブパス開始点 |
| `line_to(x, y)` | 直線 |
| `quad_to(cpx, cpy, x, y)` | 2 次ベジェ |
| `smooth_quad_to(x, y)` | 直前 `quad_to` の制御点を反射したスムーズ 2 次ベジェ |
| `cubic_to(cp1x, cp1y, cp2x, cp2y, x, y)` | 3 次ベジェ |
| `smooth_cubic_to(cp2x, cp2y, x, y)` | 直前 `cubic_to` の第 2 制御点を反射したスムーズ 3 次ベジェ |
| `conic_to(cx, cy, ex, ey, w)` | 有理 2 次ベジェ (重み `w > 0`) |
| `arc_to(cx, cy, rx, ry, start, sweep, force_move_to)` | 楕円弧 (cubic Bezier 近似) |
| `close()` | サブパスを閉じる |
| `add_circle(cx, cy, r)` | 円を 4 本の cubic Bezier で近似 (Blend2D 互換 KAPPA) |
| `add_ellipse(cx, cy, rx, ry)` | 楕円 |
| `add_round_rect(x, y, w, h, rx, ry)` | 角丸矩形 (`rx` / `ry` は幅・高さの半分でクランプ) |
| `add_triangle(x0, y0, x1, y1, x2, y2)` | 三角形 |
| `add_polygon(&[Point])` | 閉じたポリゴン |
| `add_polyline(&[Point])` | 折れ線 (閉じない) |
| `add_pie(cx, cy, rx, ry, start, sweep)` | 扇形 |
| `translate(dx, dy)` / `transform(&Matrix2D)` | 全頂点を変換 |
| `add_path(&Path)` / `add_path_translated` / `add_path_transformed` | 別パスを取り込む |
| `control_box()` / `bounding_box()` | 制御点ベースの bbox (曲線の厳密 bbox は未対応) |

## Image と PixelFormat

- `Image::new(width, height, PixelFormat)` で生成
- `data()` / `data_mut()` でバイト列参照
- `write_to_file(path)` (`AsRef<Path>`) で BI_BITFIELDS 形式 top-down BMP 出力

| 形式 | 説明 |
|---|---|
| `PixelFormat::Prgb32` | 32-bit premultiplied ARGB (デフォルト想定) |
| `PixelFormat::Xrgb32` | 32-bit XRGB。アルファ未使用で `Prgb32` と同一 JIT を共有 |
| `PixelFormat::A8` | 8-bit アルファ専用。単色 `fill_rect` とスカラ合成のみ対応 |

## JIT パイプライン概要

- 29 種類全ての合成モードに対して完全カバレッジ版 + カバレッジ付き版を JIT 生成
- I32X4 (128-bit 整数 SIMD) による 4 ピクセル並列合成 (x86_64: SSE2 / AArch64: NEON)
- F32X4 による Radial グラデーションの 4 並列 sqrt
- Linear グラデーションの固定小数点最適化
- Linear `fill_path` は fetch + coverage + blend を融合 (中間バッファなし)
- sweep (prefix sum + FillRule 変換) を JIT 生成
- エッジ座標変換の F64X2 SIMD JIT
- `PipelineRuntime` がパイプラインキャッシュを保持し、同一パラメータは再コンパイルしない

## 現状の制約

- `PixelFormat::A8`: 単色 `fill_rect` とスカラ合成 (`SrcOver` / `SrcCopy` / `Clear`) のみ。`fill_path` / グラデ / パターンは未対応
- フォント: TrueType アウトライン (glyf / loca) に対応。OpenType Layout のうち GSUB Single/Ligature (Type 1/4) と GPOS Single/Pair (Type 1/2) による基本シェーピング (liga / kern / clig) に対応。CFF / CFF2 アウトライン、Microsoft `kern` テーブル v0、可変フォントは未対応
- クリッピング: 矩形のみ。パスクリッピングは未対応
- `blit_image_*`: Nearest 補間のみ、`CompOp` は `SrcOver` / `SrcCopy` のみ
- `fill_rect` + グラデ: `CompOp` を無視して SrcOver 相当で合成
- `fill_rect` + パターン: `SrcOver` / `SrcCopy` 以外はパニック
- 曲線パスの厳密なバウンディングボックスは未対応 (制御点ベース)

## 典型的な使い方

### グラデーションで矩形を塗る

```rust
use raden::{CompOp, Context, Gradient, GradientStop, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

let mut image = Image::new(640, 480, PixelFormat::Prgb32);
let mut runtime = PipelineRuntime::new();
let mut ctx = Context::new(&mut image, &mut runtime);

let grad = Gradient::linear(0.0, 0.0, 640.0, 0.0)
    .add_stop(GradientStop::new(0.0, Rgba32::rgb(0xFF, 0x00, 0x00)))
    .add_stop(GradientStop::new(1.0, Rgba32::rgb(0x00, 0x00, 0xFF)));

ctx.set_comp_op(CompOp::SrcCopy);
ctx.set_fill_style_gradient(&grad);
ctx.fill_rect(&Rect::new(0.0, 0.0, 640.0, 480.0));
ctx.end();
```

### パスをストロークする

```rust
use raden::{CompOp, Context, Image, Path, PipelineRuntime, PixelFormat, Rgba32, StrokeCap, StrokeJoin};

let mut image = Image::new(320, 240, PixelFormat::Prgb32);
let mut runtime = PipelineRuntime::new();
let mut ctx = Context::new(&mut image, &mut runtime);

let mut path = Path::new();
path.move_to(20.0, 120.0);
path.cubic_to(80.0, 20.0, 240.0, 220.0, 300.0, 120.0);

ctx.set_comp_op(CompOp::SrcOver);
ctx.set_stroke_style(Rgba32::rgb(0xFF, 0xFF, 0xFF));
ctx.set_stroke_width(4.0);
ctx.set_stroke_cap(StrokeCap::Round);
ctx.set_stroke_join(StrokeJoin::Round);
ctx.stroke_path(&path);
ctx.end();
```

### テキスト描画と feature 切り替え

```rust
use raden::{
    CompOp, Context, Font, FontData, FontFace, FontFeatureSettings, Image, PipelineRuntime,
    PixelFormat, Rgba32,
};

let font_data = FontData::from_file("path/to/font.ttf")?;
let face = FontFace::from_data(&font_data, 0)?;
let font = Font::from_face(&face, 32.0); // liga / kern / clig はデフォルト ON
let font_plain = font.clone_with_features(FontFeatureSettings::none());

let metrics = font.measure_text("ffi AV");
let shaped = font.shape("ffi AV");

let mut image = Image::new(640, 200, PixelFormat::Prgb32);
let mut runtime = PipelineRuntime::new();
let mut ctx = Context::new(&mut image, &mut runtime);
ctx.set_comp_op(CompOp::SrcOver);
ctx.set_fill_style(Rgba32::rgb(0x00, 0x00, 0x00));
ctx.fill_text(20.0, 80.0, &font, "ffi AV");
ctx.fill_text(20.0, 160.0, &font_plain, "ffi AV");
ctx.end();
```

### 状態の保存・復元でサブシーンを分離

```rust
ctx.save();
ctx.translate(100.0, 100.0);
ctx.rotate(std::f64::consts::FRAC_PI_4);
ctx.fill_rect(&Rect::new(-20.0, -20.0, 40.0, 40.0));
ctx.restore(); // 変換・スタイル・クリップを元に戻す
```

### フレームを跨いで PipelineRuntime を使い回す

```rust
let mut image = Image::new(1920, 1080, PixelFormat::Prgb32);
let mut runtime = PipelineRuntime::new();

for frame in 0..120 {
    let mut ctx = Context::new(&mut image, &mut runtime);
    // 描画処理...
    ctx.end();
    // image.data() を表示・送信
}
```

`PipelineRuntime` を毎フレーム作り直すと JIT コンパイルが走り直すので性能が落ちる。フレームループの外側で 1 つだけ保持する。

## サンプル

| 例 | 説明 |
|---|---|
| `basic_drawing` | 矩形描画、BMP 出力、raw_player 表示 |
| `animation` | 円の大量描画アニメーション。`--width` / `--height` / `--fps` / `--duration` |
| `gradient_drawing` | Linear / Radial / Conic グラデーション |
| `stroke_drawing` | キャップ・ジョイン・パスストローク |
| `font_shaping` | liga / kern / clig ON/OFF の比較描画 |
| `tiger` | AmanithVG tiger (240+ パス) |
| `raden_player` | raw_player 相当アニメーション |
| `blit_clip_pattern_paths` | blit / clip / pattern / 拡張パス / 行列 |
