//! `src/api/pattern.rs` の単体テスト (PBT で書きにくい数値一致)。

use raden::{
    CompOp, Context, ExtendMode, Image, Matrix2D, Pattern, PatternFilter, PipelineRuntime,
    PixelFormat, Rect, Rgba32,
};

fn read_pixel(img: &Image, x: u32, y: u32) -> u32 {
    let offset = y as usize * img.stride() + x as usize * 4;
    let data = img.data();
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// 2x1 の赤・青タイル。ユーザ X を半分スケールしてテクスチャへ写し、隣接ピクセルで中間色になること。
#[test]
fn bilinear_half_scale_samples_between_pixels() {
    let mut tile = vec![0u8; 8];
    tile[0..4].copy_from_slice(&0xFF_FF_00_00u32.to_le_bytes());
    tile[4..8].copy_from_slice(&0xFF_00_00_FFu32.to_le_bytes());

    let mut pat = Pattern::new(&tile, 2, 1, 8);
    pat.set_filter(PatternFilter::Bilinear);
    pat.set_extend_mode(ExtendMode::Pad);
    // device x を 0.5 倍してテクスチャ空間へ (x=0 -> tx=0, x=1 -> tx=0.5)
    pat.set_transform(Matrix2D::scaling(0.5, 1.0));

    let mut img = Image::new(2, 1, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style_pattern(&pat);
    ctx.fill_rect(&Rect::new(0.0, 0.0, 2.0, 1.0));
    ctx.end();

    assert_eq!(read_pixel(&img, 0, 0), 0xFF_FF_00_00);
    // tx = 0.5 の双一次: R と B の中間 (チャンネルごとに丸め)
    let mid = read_pixel(&img, 1, 0);
    assert_eq!((mid >> 24) & 0xFF, 0xFF);
    assert_eq!((mid >> 16) & 0xFF, 0x80);
    assert_eq!((mid >> 8) & 0xFF, 0x00);
    assert_eq!(mid & 0xFF, 0x80);
}

/// 同じ設定で Nearest は端点に丸めて両端の纯色になる。
#[test]
fn nearest_half_scale_snaps_to_texel() {
    let mut tile = vec![0u8; 8];
    tile[0..4].copy_from_slice(&0xFF_FF_00_00u32.to_le_bytes());
    tile[4..8].copy_from_slice(&0xFF_00_00_FFu32.to_le_bytes());

    let mut pat = Pattern::new(&tile, 2, 1, 8);
    pat.set_filter(PatternFilter::Nearest);
    pat.set_extend_mode(ExtendMode::Pad);
    pat.set_transform(Matrix2D::scaling(0.5, 1.0));

    let mut img = Image::new(2, 1, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style_pattern(&pat);
    ctx.fill_rect(&Rect::new(0.0, 0.0, 2.0, 1.0));
    ctx.end();

    assert_eq!(read_pixel(&img, 0, 0), 0xFF_FF_00_00);
    assert_eq!(read_pixel(&img, 1, 0), 0xFF_00_00_FF);
}

/// `fill_rect` + パターンで `SrcCopy` のとき、宛てをソース色で置き換える（SrcOver の合成ではない）。
#[test]
fn fill_rect_pattern_src_copy_replaces_destination() {
    let mut img = Image::new(2, 1, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0x00, 0x00, 0xFF, 0xFF));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 2.0, 1.0));

    let mut tile = vec![0u8; 4];
    // premultiplied 半透明赤 (A=128, R=128)
    tile.copy_from_slice(&0x80_80_00_00u32.to_le_bytes());
    let mut pat = Pattern::new(&tile, 1, 1, 4);
    pat.set_extend_mode(ExtendMode::Pad);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style_pattern(&pat);
    ctx.fill_rect(&Rect::new(0.0, 0.0, 2.0, 1.0));
    ctx.end();

    assert_eq!(read_pixel(&img, 0, 0), 0x80_80_00_00);
    assert_eq!(read_pixel(&img, 1, 0), 0x80_80_00_00);
}
