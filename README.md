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

API は [Blend2D](https://blend2d.com/) にできるだけ寄せており、Blend2D の利用経験があればそのまま使い始められるのライブラリを目指しています。

<https://github.com/user-attachments/assets/23145334-3847-4317-ae81-81b51dda689b>

raden と [raw-player-rs](https://github.com/shiguredo/raw-player-rs) を組み合わせて 120 fps での映像なども描画可能です。

## 目的

GPU を利用できない CI 環境において、 CPU のみを利用して 1080p / 120fps 以上の複雑なダミー映像を高速に生成するためのライブラリとして開発しています。

## 現在の機能

### 描画 API (`Context`)

| メソッド | 説明 |
|---|---|
| `fill_all()` | 画像全体を現在の塗りつぶし色で塗りつぶす |
| `fill_rect(&Rect)` | 矩形塗りつぶし。クリッピング付き |
| `fill_path(&Path)` | 任意パス塗りつぶし。ベジェ平坦化 + ラスタライズ |
| `fill_circle(&Circle)` | 円塗りつぶし。内部で Path に変換 |
| `fill_pie(&Arc)` | 扇形塗りつぶし。中心から弧を経由して中心に戻る閉じた領域 |
| `fill_text(x, y, &Font, text)` | テキスト描画。全グリフを 1 つの Path に結合して `fill_path` で一括描画 |
| `stroke_line(&Line)` | 線分のストローク描画 |
| `stroke_rect(&Rect)` | 矩形のストローク描画 |
| `stroke_circle(&Circle)` | 円のストローク描画 |
| `stroke_path(&Path)` | 任意パスのストローク描画。stroke-to-fill 変換後 `fill_path` で描画 |
| `translate(tx, ty)` | 平行移動を現在の変換行列に適用する |
| `scale(sx, sy)` | スケーリングを現在の変換行列に適用する |
| `rotate(angle)` | 回転を現在の変換行列に適用する (ラジアン) |
| `save()` | 現在の描画状態をスタックに保存する |
| `restore()` | スタックから描画状態を復元する |

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

### フィルスタイル / ストロークスタイル

| 種類 | 説明 |
|---|---|
| `set_fill_rule(FillRule)` | 塗りつぶし規則を設定する (`NonZero` / `EvenOdd`) |
| `set_fill_style(Rgba32)` | 塗りつぶし色を設定する |
| `set_stroke_style(Rgba32)` | ストローク色を設定する。fill と独立 |

### ストロークパラメータ

| メソッド | 説明 |
|---|---|
| `set_stroke_width(f64)` | ストローク幅を設定する |
| `set_stroke_cap(StrokeCap)` | 線端の形状 (`Butt`, `Square`, `Round`) |
| `set_stroke_join(StrokeJoin)` | 接合部の形状 (`Bevel`, `MiterBevel`, `Round`) |
| `set_stroke_miter_limit(f64)` | マイター限界値を設定する |

### フォント

| 型 | 説明 |
|---|---|
| `FontData` | フォントファイルのバイトデータ。`from_file(path)` または `from_bytes(bytes)` で作成 |
| `FontFace` | パース済みフォントフェイス。TrueType テーブル (head, hhea, hmtx, cmap, loca, glyf) を解析 |
| `Font` | サイズ指定済みフォント。`from_face(&FontFace, size)` で作成し `fill_text` に渡す |

### パス (`Path`)

| コマンド | 説明 |
|---|---|
| `move_to(x, y)` | サブパス開始点を設定する |
| `line_to(x, y)` | 直線を追加する |
| `quad_to(cpx, cpy, x, y)` | 2 次ベジェ曲線を追加する |
| `cubic_to(cp1x, cp1y, cp2x, cp2y, x, y)` | 3 次ベジェ曲線を追加する |
| `close()` | サブパスを閉じる |
| `add_circle(cx, cy, r)` | 円を 4 本の cubic Bezier で近似して追加する (Blend2D 互換の KAPPA 定数使用) |

### その他

| 機能 | 説明 |
|---|---|
| `Image` | 画像バッファ管理。`new(width, height, format)` で生成、`data()` でバイト列参照 |
| `PixelFormat::Prgb32` | 32-bit premultiplied ARGB。現在唯一のフォーマット |
| BMP 出力 | `Image::write_to_file()` で BI_BITFIELDS 形式の top-down BMP を出力する |

## サンプル

### basic_drawing

矩形の描画、BMP 出力、raw_player でのウィンドウ表示を行うサンプル。

```bash
cargo run --example basic_drawing
```

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

### stroke_drawing

ストローク描画のサンプル。キャップ、ジョイン、パスストローク、半透明ストロークを描画する。

```bash
cargo run --example stroke_drawing
```

### raden_player

[raw-player](https://github.com/shiguredo/raw-player) の `blend2d_player.py` と同等のアニメーションを
`raw_player::VideoPlayer` API で表示する。

```bash
cargo run --example raden_player --release
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

- `PixelFormat` は現在 `Prgb32` のみ
- フィルスタイルは単色のみ (グラデーション、画像パターン未対応)
- フォントは TrueType アウトライン (glyf/loca) のみ対応 (CFF, OpenType Layout 未対応)
- ダッシュ線 (dash_array / dash_offset) は未対応

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
