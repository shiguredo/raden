//! フォントのシェーピング機能を比較表示するサンプル。
//!
//! 指定されたフォントファイルを読み込み、同じ文字列を
//! - デフォルト feature（liga / kern / clig ON）
//! - feature 無効（liga / kern / clig OFF）
//!
//! の 2 パターンで描画して 1 枚の BMP 画像に保存する。
//!
//! 実行例:
//!
//! ```text
//! cargo run --example font_shaping /path/to/font.ttf output.bmp "ffi AV fl"
//! ```

use std::env;

use raden::{
    Context, Font, FontData, FontFace, FontFeatureSettings, Image, PipelineRuntime, PixelFormat,
    Rgba32,
};

const WIDTH: u32 = 1400;
const HEIGHT: u32 = 500;
const FONT_SIZE: f64 = 96.0;
const MARGIN_X: f64 = 32.0;
const MARGIN_Y: f64 = 120.0;
const TEXT_Y: f64 = 220.0;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: cargo run --example font_shaping <font-path> <output-bmp> [text]");
        return Ok(());
    }

    let font_path = &args[1];
    let output_path = &args[2];
    let text = args.get(3).map(|s| s.as_str()).unwrap_or("ffi AV fl");

    let font_bytes = std::fs::read(font_path)?;
    let font_data = FontData::from_bytes(font_bytes);
    let face = FontFace::from_data(&font_data, 0)?;

    let font_default = Font::from_face(&face, FONT_SIZE);
    let font_no_features =
        Font::from_face(&face, FONT_SIZE).clone_with_features(FontFeatureSettings::none());

    let mut img = Image::new(WIDTH, HEIGHT, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);

    // 背景を白で塗りつぶす。
    ctx.set_fill_style(Rgba32::rgb(255, 255, 255));
    ctx.fill_rect(&raden::Rect::new(0.0, 0.0, WIDTH as f64, HEIGHT as f64));

    // ラベルは小さめのフォントで描画する。
    let label_font = Font::from_face(&face, 24.0);
    ctx.set_fill_style(Rgba32::rgb(64, 64, 64));
    ctx.fill_text(
        MARGIN_X,
        48.0,
        &label_font,
        "default features (liga / kern / clig ON)",
    );
    ctx.fill_text(
        MARGIN_X,
        TEXT_Y + FONT_SIZE + 48.0,
        &label_font,
        "features disabled",
    );

    // 本文は黒で描画する。
    ctx.set_fill_style(Rgba32::rgb(0, 0, 0));
    ctx.fill_text(MARGIN_X, TEXT_Y, &font_default, text);
    ctx.fill_text(
        MARGIN_X,
        TEXT_Y + FONT_SIZE + MARGIN_Y,
        &font_no_features,
        text,
    );

    ctx.end();

    img.write_to_file(output_path)?;

    println!(
        "Saved shaped text comparison to {} (text: {:?})",
        output_path, text
    );

    Ok(())
}
