# Raden

[![raden](https://img.shields.io/crates/v/raden.svg)](https://crates.io/crates/raden)
[![Documentation](https://docs.rs/raden/badge.svg)](https://docs.rs/raden)
[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

> [!IMPORTANT]
> 性能や安定性、信頼性が必要な場合は Blend2D を利用するべきです。このライブラリは実験的なプロジェクトであり、API や内部実装は予告なく大幅に変更される可能性があります。

## About Shiguredo's open source software

We will not respond to PRs or issues that have not been discussed on Discord. Also, Discord is only available in Japanese.

Please read <https://github.com/shiguredo/oss> before use.

## 時雨堂のオープンソースソフトウェアについて

利用前に <https://github.com/shiguredo/oss> をお読みください。

## 概要

Raden は [Cranelift](https://cranelift.dev/) を利用した 2D ベクターグラフィックスライブラリです。CPU のみで利用できます。

API は [Blend2D](https://blend2d.com/) にできるだけ寄せており、Blend2D の利用経験があればそのまま使い始められるライブラリを目指しています。

<https://github.com/user-attachments/assets/23145334-3847-4317-ae81-81b51dda689b>

raden と [raw-player-rs](https://github.com/shiguredo/raw-player-rs) を組み合わせて 120 fps での映像なども描画可能です。

## 目的

GPU を利用できない CI 環境において、CPU のみを利用して 1080p / 120fps 以上の複雑なダミー映像を高速に生成するためのライブラリとして開発しています。

## 現在の機能

### フィル描画 API (`Context`)

| メソッド | 説明 |
|---|---|
| `fill_all()` | 画像全体を現在のフィルスタイルで塗りつぶす |
| `fill_rect(&Rect)` | 矩形塗りつぶし。クリッピング付き |
| `fill_path(&Path)` | 任意パス塗りつぶし。ベジェ平坦化 + ラスタライズ |
| `fill_circle(&Circle)` | 円塗りつぶし。内部で Path に変換 |
| `fill_ellipse(&Ellipse)` | 楕円塗りつぶし |
| `fill_round_rect(&RoundRect)` | 角丸矩形塗りつぶし |
| `fill_triangle(&Triangle)` | 三角形塗りつぶし |
| `fill_polygon(&[Point])` | ポリゴン塗りつぶし (3 点未満は no-op) |
| `fill_pie(&Arc)` | 扇形塗りつぶし。中心から弧を経由して中心に戻る閉じた領域 |
| `fill_text(x, y, &Font, text)` | テキスト描画。OpenType GSUB / GPOS シェーピング適用後、全グリフを 1 つの Path に結合して `fill_path` で一括描画 |

### ストローク描画 API (`Context`)

| メソッド | 説明 |
|---|---|
| `stroke_line(&Line)` | 線分のストローク描画 |
| `stroke_rect(&Rect)` | 矩形のストローク描画 |
| `stroke_circle(&Circle)` | 円のストローク描画 |
| `stroke_ellipse(&Ellipse)` | 楕円のストローク描画 |
| `stroke_round_rect(&RoundRect)` | 角丸矩形のストローク描画 |
| `stroke_triangle(&Triangle)` | 三角形のストローク描画 |
| `stroke_polygon(&[Point])` | ポリゴンのストローク描画 (閉じる) |
| `stroke_polyline(&[Point])` | 折れ線のストローク描画 (閉じない) |
| `stroke_path(&Path)` | 任意パスのストローク描画。stroke-to-fill 変換後 `fill_path` で描画 |
| `stroke_text(x, y, &Font, text)` | テキストの輪郭をストローク描画。シェーピング適用は `fill_text` と同じ |

### クリア / Blit (`Context`)

| メソッド | 説明 |
|---|---|
| `clear_all()` | 現在のクリップ領域全体をピクセル値 0 で書き換える (`comp_op` は変更しない) |
| `clear_rect(&Rect)` | 指定矩形をピクセル値 0 で書き換える (デバイス座標、変換行列は無視) |
| `blit_image_at(x, y, &Image)` | ソース画像全体を (x, y) に Nearest 転送する。ソースは `Prgb32` / `Xrgb32`、宛先は `Prgb32` / `Xrgb32` / `A8`。`CompOp` は `SrcOver` / `SrcCopy` のみ。`global_alpha` は未適用 |
| `blit_image_rect(&Rect, &Image, Option<Rect>)` | ソース矩形を宛先矩形に Nearest 転送する。制限は `blit_image_at` と同じ |

### 変換 (`Context`)

後乗算 (`matrix = matrix * T`) は Blend2D / SVG / Canvas 互換で、前乗算 (`post_*`) も提供する。

| メソッド | 説明 |
|---|---|
| `translate(tx, ty)` | 平行移動を後乗算で適用する |
| `scale(sx, sy)` | スケーリングを後乗算で適用する |
| `rotate(angle)` | 回転 (ラジアン) を後乗算で適用する |
| `rotate_around(angle, cx, cy)` | 指定中心まわりの回転を後乗算で適用する |
| `skew(kx, ky)` | せん断 (係数は接線) を後乗算で適用する |
| `apply_matrix(&Matrix2D)` | 任意行列を後乗算で適用する |
| `post_translate(tx, ty)` / `post_scale(sx, sy)` / `post_rotate(angle)` / `post_skew(kx, ky)` / `post_transform(&Matrix2D)` | 前乗算版 |
| `reset_matrix()` | 変換行列を単位行列にリセットする |
| `user_to_meta()` | Blend2D 互換のエントリポイント。raden はメタ行列を別保持しないため、ユーザ行列を単位にリセットするのみ |

### 状態管理 / クリッピング (`Context`)

| メソッド | 説明 |
|---|---|
| `save()` | 現在の描画状態 (スタイル、変換、クリップ等) をスタックに保存する |
| `restore()` | スタックから描画状態を復元する |
| `clip_to_rect(&Rect)` | クリップ領域を指定矩形との積集合に縮小する (拡大不可) |
| `restore_clipping()` | クリップ領域を画像境界 (メタクリップ) に戻す |
| `comp_op()` / `fill_rule()` / `matrix()` | 現在の合成モード、塗り規則、変換行列を取得する |
| `fill_color_prgb32()` / `fill_gradient()` / `fill_pattern()` | 現在のフィルスタイルを取得する |
| `stroke_color_prgb32()` / `stroke_gradient()` / `stroke_pattern()` | 現在のストロークスタイルを取得する |
| `stroke_width()` / `stroke_miter_limit()` / `stroke_join()` / `stroke_start_cap()` / `stroke_end_cap()` / `stroke_dash_array()` / `stroke_dash_offset()` | 現在のストロークパラメータを取得する |
| `global_alpha()` / `fill_alpha()` / `stroke_alpha()` | 現在のアルファ設定を取得する |

### フィルスタイル / ストロークスタイル

| 種類 | 説明 |
|---|---|
| `set_fill_style(Rgba32)` | フィルを単色に設定する |
| `set_fill_style_gradient(&Gradient)` | フィルをグラデーションに設定する (Linear / Radial / Conic) |
| `set_fill_style_pattern(&Pattern)` | フィルを画像パターンに設定する |
| `set_stroke_style(Rgba32)` | ストロークを単色に設定する |
| `set_stroke_style_gradient(&Gradient)` | ストロークをグラデーションに設定する |
| `set_stroke_style_pattern(&Pattern)` | ストロークを画像パターンに設定する |
| `set_fill_rule(FillRule)` | 塗りつぶし規則を設定する (`NonZero` / `EvenOdd`) |
| `set_comp_op(CompOp)` | 合成モードを設定する |
| `set_global_alpha(a)` | 全描画に乗算されるグローバルアルファを設定する (値域 [0, 1]) |
| `set_fill_alpha(a)` | フィル個別アルファを設定する (値域 [0, 1]) |
| `set_stroke_alpha(a)` | ストローク個別アルファを設定する (値域 [0, 1]) |

### グラデーション (`Gradient`)

| 種類 | 説明 |
|---|---|
| Linear | 2 点間の線形グラデーション |
| Radial | 中心・焦点・半径による放射グラデーション |
| Conic | 中心と角度による円錐グラデーション |

範囲外処理モード (`ExtendMode`): `Pad` (デフォルト) / `Repeat` / `Reflect`

`fill_rect` のグラデーション高速パス (変換行列が identity かつ実効フィルアルファが 1.0) は `set_comp_op` を参照せず、内部で SrcOver 相当の融合のみを行う。変換行列が非 identity、または実効アルファが 1.0 未満のときは `fill_path` にフォールバックし、`CompOp` をパイプラインで適用する。

### パターン (`Pattern`)

画像をタイルとして繰り返す塗りつぶし。`set_origin` / `set_transform`、`PatternFilter` (`Nearest` / `Bilinear`)、`ExtendMode` (`Pad` / `Repeat` / `Reflect`) に対応する。デフォルトの `ExtendMode` は `Repeat` (グラデーションのデフォルト `Pad` とは異なる)。

`fill_rect` のパターン高速パス (変換行列が identity かつ実効フィルアルファが 1.0) は `CompOp` が `SrcOver` / `SrcCopy` のみ対応 (それ以外はパニック)。変換行列が非 identity、または実効アルファが 1.0 未満のときは `fill_path` にフォールバックし、`CompOp` をパイプライン経由で適用できる。

### 合成モード (`CompOp`)

Porter-Duff 基本セット + Clear + Plus の 13 種類と、ブレンドモード 16 種類の計 29 種類。

#### Porter-Duff 合成

| モード | 値 | 説明 |
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

#### ブレンドモード

| モード | 値 | 説明 |
|---|---|---|
| `Minus` | 13 | 飽和減算。`out = max(dst - src, 0)` |
| `Modulate` | 14 | チャネル乗算。`out = src * dst` |
| `Multiply` | 15 | Multiply ブレンド |
| `Screen` | 16 | `out = src + dst - src * dst` |
| `Overlay` | 17 | 条件分岐。暗部は Multiply、明部は Screen |
| `Darken` | 18 | `blend = min(src * dstA, dst * srcA)` |
| `Lighten` | 19 | `blend = max(src * dstA, dst * srcA)` |
| `ColorDodge` | 20 | 覆い焼き。除算ベース |
| `ColorBurn` | 21 | 焼き込み。除算ベース |
| `LinearBurn` | 22 | `out = max(src + dst - srcA * dstA, 0)` |
| `LinearLight` | 23 | LinearDodge + LinearBurn の組み合わせ |
| `PinLight` | 24 | 条件分岐 + min/max |
| `HardLight` | 25 | Overlay の src/dst 入替。条件分岐 |
| `SoftLight` | 26 | 柔らかい光。除算 + sqrt |
| `Difference` | 27 | `blend = abs(src * dstA - dst * srcA)` |
| `Exclusion` | 28 | `out = src + dst - 2 * src * dst` |

### 塗りつぶし規則 (`FillRule`)

| ルール | 説明 |
|---|---|
| `NonZero` | ワインディングナンバーが非ゼロなら内側 (デフォルト) |
| `EvenOdd` | ワインディングナンバーが奇数なら内側 |

### ストロークパラメータ

| メソッド | 説明 |
|---|---|
| `set_stroke_width(f64)` | ストローク幅を設定する |
| `set_stroke_cap(StrokeCap)` | 線端の形状を一括設定 (`Butt`, `Square`, `Round`) |
| `set_stroke_start_cap(StrokeCap)` | 始端の形状を個別設定 |
| `set_stroke_end_cap(StrokeCap)` | 終端の形状を個別設定 |
| `set_stroke_join(StrokeJoin)` | 接合部の形状 (`MiterClip`, `MiterBevel`, `MiterRound`, `Bevel`, `Round`) |
| `set_stroke_miter_limit(f64)` | マイター限界値を設定する (デフォルト 4.0) |
| `set_stroke_dash_array(&[f64])` | ダッシュパターンを設定する (SVG 準拠、奇数パターンは 2 回繰り返す) |
| `set_stroke_dash_offset(f64)` | ダッシュパターンの開始オフセットを設定する |

### フォント

| 型 | 説明 |
|---|---|
| `FontData` | フォントファイルのバイトデータ。`from_file(path)` (`AsRef<Path>`) または `from_bytes(bytes)` で作成。`data()` で生バイト列を取得 |
| `FontFace` | パース済みフォントフェイス。TrueType / OpenType テーブル (head, maxp, hhea, hmtx, cmap, loca, glyf, OS/2, GSUB, GPOS) を解析。`units_per_em` / `ascent` / `descent` / `line_gap` / `cap_height` / `x_height` / `glyph_bounds` を提供 |
| `Font` | サイズ指定済みフォント。`from_face` / `with_features` で作成し `fill_text` / `stroke_text` に渡す。`size` / `scale` / `map_char_to_glyph` / `glyph_advance` / `append_glyph_outline`、`shape` / `shape_into` / `measure_text`、スケール済みメトリクス (`ascent` / `descent` / `line_gap` / `cap_height` / `x_height` / `glyph_bounds`)、`feature_settings` / `set_feature_settings` / `clone_with_features` で liga / kern / clig を切り替え可能 |
| `FontError` | フォント読み込み / パース失敗時のエラー型 |
| `FontFeatureSettings` | liga / kern / clig の OpenType Layout feature ON/OFF 設定。デフォルトは全 ON。`none()` / `with_kern` / `with_liga` / `with_clig` |
| `GlyphBuffer` | `Font::shape` / `Font::shape_into` の結果。内部配列は `pub(crate)`。クレート外は `len` / `is_empty` / `glyph_id` / `placement` / `cluster` / `iter` / `is_well_formed` で参照 |
| `GlyphBufferIter` | `GlyphBuffer::iter` が返すイテレータ。`(glyph_id, GlyphPlacement, cluster)` |
| `GlyphPlacement` | 1 グリフの配置情報。advance / offset_x / offset_y (ピクセル単位、offset は Y up) |
| `GlyphBounds` | グリフ境界ボックス (x_min / y_min / x_max / y_max)。Y up、baseline 原点。描画時の Y 反転は適用しない |
| `TextMetrics` | `Font::measure_text` の結果。advance / bounding_box / leading_bearing / trailing_bearing (いずれもシェーピング適用済み) |

### パス (`Path`)

| コマンド | 説明 |
|---|---|
| `move_to(x, y)` | サブパス開始点を設定する |
| `line_to(x, y)` | 直線を追加する |
| `quad_to(cpx, cpy, x, y)` | 2 次ベジェ曲線を追加する |
| `smooth_quad_to(x, y)` | 直前 `quad_to` の制御点を反射したスムーズ 2 次ベジェを追加する |
| `cubic_to(cp1x, cp1y, cp2x, cp2y, x, y)` | 3 次ベジェ曲線を追加する |
| `smooth_cubic_to(cp2x, cp2y, x, y)` | 直前 `cubic_to` の第 2 制御点を反射したスムーズ 3 次ベジェを追加する |
| `conic_to(cx, cy, ex, ey, w)` | 有理 2 次ベジェ (円錐曲線、重み `w > 0`) を追加する |
| `arc_to(cx, cy, rx, ry, start, sweep, force_move_to)` | 楕円弧を現在点から cubic Bezier 近似で接続する |
| `close()` | サブパスを閉じる |
| `clear()` / `len()` / `is_empty()` / `cmds()` / `points()` / `conic_weights()` | パス内容の消去と参照 |
| `add_circle(cx, cy, r)` | 円を 4 本の cubic Bezier で近似して追加する (Blend2D 互換の KAPPA 定数使用) |
| `add_ellipse(cx, cy, rx, ry)` | 楕円を追加する |
| `add_round_rect(x, y, w, h, rx, ry)` | 角丸矩形を追加する (`rx` / `ry` は幅・高さの半分でクランプ) |
| `add_triangle(x0, y0, x1, y1, x2, y2)` | 三角形を追加する |
| `add_polygon(&[Point])` | 点列を結んで閉じたポリゴンを追加する |
| `add_polyline(&[Point])` | 点列を結んだ折れ線 (閉じない) を追加する |
| `add_pie(cx, cy, rx, ry, start, sweep)` | 扇形を追加する |
| `translate(dx, dy)` | 全頂点を平行移動する |
| `transform(&Matrix2D)` | 全頂点に行列を適用する |
| `add_path(&Path)` / `add_path_translated(&Path, dx, dy)` / `add_path_transformed(&Path, &Matrix2D)` | 別パスを取り込む |
| `control_box()` / `bounding_box()` | 制御点ベースのバウンディングボックスを返す (曲線の厳密な bbox は未対応) |

### その他

| 機能 | 説明 |
|---|---|
| `Image` | 画像バッファ管理。`new(width, height, format)` で生成、`width` / `height` / `stride` / `format` / `data()` / `data_mut()` で参照 |
| `PixelFormat::Prgb32` | 32-bit premultiplied ARGB (デフォルト想定) |
| `PixelFormat::Xrgb32` | 32-bit XRGB。アルファは未使用で、合成は `Prgb32` と同一 JIT を共有する |
| `PixelFormat::A8` | 8-bit アルファ専用。単色 `fill_rect` / `clear_*` / blit 宛先に対応。合成はスカラ実装で `SrcOver` / `SrcCopy` / `Clear` / `Plus`。`fill_path`、グラデ/パターン塗りは未対応 |
| BMP 出力 | `Image::write_to_file()` で BI_BITFIELDS 形式の top-down BMP を出力する |

## JIT パイプライン

描画パイプラインは Cranelift JIT コンパイラでネイティブコードを生成する。

- 29 種類の合成モード全てに対して完全カバレッジ版 + カバレッジ付き版のパイプラインを JIT 生成
- I32X4 (128-bit 整数 SIMD) による 4 ピクセル並列合成 (x86_64: SSE2, AArch64: NEON)
- F32X4 (128-bit 浮動小数点 SIMD) による Radial グラデーションの 4 並列 sqrt
- Linear グラデーションの固定小数点最適化 (内部ループで浮動小数点演算ゼロ)
- Linear fill_path の fetch + coverage + blend 融合パイプライン (中間バッファ排除)
- sweep 関数 (prefix sum + FillRule 変換) の JIT 生成
- エッジ座標変換の F64X2 SIMD JIT
- パイプラインキャッシュにより同一パラメータの関数は再コンパイルしない

## サンプル

### basic_drawing

矩形の描画、BMP 出力、raw_player でのウィンドウ表示を行うサンプル。

```bash
cargo run --example basic_drawing
```

[![Image from Gyazo](https://i.gyazo.com/a3a5eb75c5e8450ecbdf2aa291ea2ae1.png)](https://gyazo.com/a3a5eb75c5e8450ecbdf2aa291ea2ae1)

### animation

円を大量に描画するアニメーション。SDL3 ウィンドウで表示する。

```bash
# 通常実行 (デフォルト: 1280x720, 60fps)
cargo run --example animation --release

# 解像度と FPS を指定
cargo run --example animation --release -- --width 1920 --height 1080 --fps 120

# ヘッドレスベンチマークモード (ウィンドウなし、指定秒数分のフレームを描画して統計出力)
cargo run --example animation --release -- --duration 5
cargo run --example animation --release -- --width 1920 --height 1080 --fps 120 --duration 5
```

### gradient_drawing

グラデーション描画のサンプル。Linear (Pad / Repeat / Reflect)、Radial、Conic の 5 パターンを 1 枚に並べて描画する。

```bash
cargo run --example gradient_drawing
```

[![Image from Gyazo](https://i.gyazo.com/bdf92cfbbea23d857d32628cb32c6838.jpg)](https://gyazo.com/bdf92cfbbea23d857d32628cb32c6838)

### stroke_drawing

ストローク描画のサンプル。キャップ、ジョイン、パスストローク、半透明ストロークを描画する。

```bash
cargo run --example stroke_drawing
```

[![Image from Gyazo](https://i.gyazo.com/8f2c7ae6e5fa641c8881e15a13563195.png)](https://gyazo.com/8f2c7ae6e5fa641c8881e15a13563195)

### font_shaping

OpenType Layout によるシェーピング効果を可視化するサンプル。同じ文字列を `liga` / `kern` / `clig` ON/OFF で上下に並べて BMP 出力する。

```bash
cargo run --example font_shaping -- path/to/font.ttf output.bmp "ffi AV fl"
```

### tiger

AmanithVG 由来の tiger ベクターグラフィックスを描画するサンプル。240 以上のパスによるフィル・ストロークの組み合わせを実演する。

```bash
cargo run --example tiger --release
```

[![Image from Gyazo](https://i.gyazo.com/8220dfa48f64e0239fd67c5d78439216.png)](https://gyazo.com/8220dfa48f64e0239fd67c5d78439216)

### raden_player

[raw-player](https://github.com/shiguredo/raw-player) の `blend2d_player.py` と同等のアニメーションを
`raw_player::VideoPlayer` API で表示する。

```bash
cargo run --example raden_player --release
```

### blit_clip_pattern_paths

`blit_image_rect`、`clip_to_rect`、パターン塗り (`PatternFilter` と `set_transform`)、拡張パス命令 (`smooth_quad_to` / `smooth_cubic_to` / `conic_to` / `arc_to`)、行列 (`rotate_around` / `skew`) を組み合わせたサンプル。

```bash
cargo run --example blit_clip_pattern_paths --release
```

## 最小コード例

```rust
use raden::{Circle, CompOp, Context, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut image = Image::new(320, 240, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut image, &mut runtime);

    // 背景を塗る
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::rgb(0x20, 0x20, 0x60));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 320.0, 240.0));

    // 半透明の円を重ねる
    ctx.set_comp_op(CompOp::SrcOver);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x40, 0x40, 0xC0));
    ctx.fill_circle(&Circle::new(160.0, 120.0, 72.0));

    ctx.end();
    image.write_to_file("output.bmp")?;
    Ok(())
}
```

## 制約

- Rust 1.95 以上が必要 (`Cargo.toml` の `rust-version`)
- `PixelFormat::A8` は単色 `fill_rect` / `clear_*` / blit 宛先とスカラ合成 (`SrcOver` / `SrcCopy` / `Clear` / `Plus`) に対応。`fill_path`・グラデーション塗り・パターン塗りは未対応
- フォントは TrueType アウトライン (glyf/loca) に対応。OpenType Layout のうち GSUB Single/Ligature (Type 1/4) と GPOS Single/Pair (Type 1/2) による基本シェーピング (liga / kern / clig) に対応。CFF / CFF2 アウトライン、Microsoft `kern` テーブル v0、可変フォントは未対応。Compound Glyph の point-matching (`ARGS_ARE_XY_VALUES == 0`) は原点フォールバック
- クリッピングは矩形のみ対応 (パスクリッピングは未対応)
- `blit_image_*` は Nearest 補間、`CompOp` は `SrcOver` / `SrcCopy` のみ。ソースは `Prgb32` / `Xrgb32`、宛先は `Prgb32` / `Xrgb32` / `A8`。`global_alpha` は blit に未適用

## ライセンス

```text
Copyright 2026-2026, Shiguredo Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
```
