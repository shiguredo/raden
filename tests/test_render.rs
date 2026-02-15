use raden::{Circle, CompOp, Context, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

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

#[test]
fn src_copy_opaque_red() {
    let mut img = Image::new(100, 100, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
    ctx.fill_rect(&Rect::new(10.0, 10.0, 20.0, 20.0));
    ctx.end();

    // 矩形内のピクセル: 不透明赤 = 0xFF_FF_00_00
    assert_eq!(read_pixel(&img, 10, 10), 0xFF_FF_00_00);
    assert_eq!(read_pixel(&img, 20, 20), 0xFF_FF_00_00);
    assert_eq!(read_pixel(&img, 29, 29), 0xFF_FF_00_00);

    // 矩形外のピクセル: ゼロ (初期値)
    assert_eq!(read_pixel(&img, 0, 0), 0x00_00_00_00);
    assert_eq!(read_pixel(&img, 9, 10), 0x00_00_00_00);
    assert_eq!(read_pixel(&img, 30, 10), 0x00_00_00_00);
}

#[test]
fn src_over_opaque_overwrites() {
    let mut img = Image::new(100, 100, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcOver);
    // 不透明色の SrcOver は SrcCopy と同一結果
    ctx.set_fill_style(Rgba32::new(0x00, 0xFF, 0x00, 0xFF));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 50.0, 50.0));
    ctx.end();

    assert_eq!(read_pixel(&img, 0, 0), 0xFF_00_FF_00);
    assert_eq!(read_pixel(&img, 49, 49), 0xFF_00_FF_00);
    assert_eq!(read_pixel(&img, 50, 0), 0x00_00_00_00);
}

#[test]
fn src_over_semi_transparent_blend() {
    let mut img = Image::new(100, 100, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);

    // まず不透明青で背景を塗る
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0x00, 0x00, 0xFF, 0xFF));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 100.0, 100.0));

    // 半透明赤を SrcOver で重ねる
    ctx.set_comp_op(CompOp::SrcOver);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0x80));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 100.0, 100.0));
    ctx.end();

    let pixel = read_pixel(&img, 50, 50);
    let a = (pixel >> 24) & 0xFF;
    let r = (pixel >> 16) & 0xFF;
    let g = (pixel >> 8) & 0xFF;
    let b = pixel & 0xFF;

    // Rgba32(0xFF, 0x00, 0x00, 0x80) の premultiplied:
    // src_a = 128, src_r = (255*128+128)/255 = 128, src_g = 0, src_b = 0
    // inv_alpha = 256 - 128 = 128
    // dst = (0xFF, 0x00, 0x00, 0xFF) = premultiplied blue
    // out_a = 128 + (255 * 128) >> 8 = 128 + 127 = 255
    // out_r = 128 + (0 * 128) >> 8 = 128
    // out_g = 0 + (0 * 128) >> 8 = 0
    // out_b = 0 + (255 * 128) >> 8 = 0 + 127 = 127
    assert!(a >= 254, "alpha: {a}"); // 255 か 254 (丸め誤差)
    assert!((126..=129).contains(&r), "red: {r}");
    assert_eq!(g, 0);
    assert!((126..=128).contains(&b), "blue: {b}");
}

#[test]
fn clip_rect_partially_outside() {
    let mut img = Image::new(50, 50, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
    // 矩形が画像の右下にはみ出す
    ctx.fill_rect(&Rect::new(40.0, 40.0, 100.0, 100.0));
    ctx.end();

    // クリップされた範囲内は白
    assert_eq!(read_pixel(&img, 40, 40), 0xFF_FF_FF_FF);
    assert_eq!(read_pixel(&img, 49, 49), 0xFF_FF_FF_FF);
    // クリップ範囲外は黒
    assert_eq!(read_pixel(&img, 39, 40), 0x00_00_00_00);
}

#[test]
fn clip_rect_fully_outside() {
    let mut img = Image::new(50, 50, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
    // 完全に画像外
    ctx.fill_rect(&Rect::new(100.0, 100.0, 50.0, 50.0));
    ctx.end();

    // 全ピクセルがゼロのまま
    assert_eq!(read_pixel(&img, 0, 0), 0x00_00_00_00);
    assert_eq!(read_pixel(&img, 49, 49), 0x00_00_00_00);
}

#[test]
fn clip_rect_negative_origin() {
    let mut img = Image::new(50, 50, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0x00, 0xFF, 0x00, 0xFF));
    // 左上が画像外
    ctx.fill_rect(&Rect::new(-10.0, -10.0, 20.0, 20.0));
    ctx.end();

    // (0,0) から (9,9) が描画される
    assert_eq!(read_pixel(&img, 0, 0), 0xFF_00_FF_00);
    assert_eq!(read_pixel(&img, 9, 9), 0xFF_00_FF_00);
    assert_eq!(read_pixel(&img, 10, 0), 0x00_00_00_00);
}

#[test]
fn bmp_output_header() {
    let mut img = Image::new(4, 4, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 4.0, 4.0));
    ctx.end();

    let path = std::env::temp_dir().join("raden_test_output.bmp");
    img.write_to_file(&path).unwrap();

    let data = std::fs::read(&path).unwrap();
    // BMP マジックナンバー
    assert_eq!(&data[0..2], b"BM");

    // ファイルサイズ
    let file_size = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
    assert_eq!(file_size as usize, data.len());

    // ピクセルデータオフセット = 14 + 40 + 12 = 66
    let pixel_offset = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);
    assert_eq!(pixel_offset, 66);

    // DIB ヘッダサイズ = 40
    let dib_size = u32::from_le_bytes([data[14], data[15], data[16], data[17]]);
    assert_eq!(dib_size, 40);

    // 幅 = 4
    let width = i32::from_le_bytes([data[18], data[19], data[20], data[21]]);
    assert_eq!(width, 4);

    // 高さ = -4 (top-down)
    let height = i32::from_le_bytes([data[22], data[23], data[24], data[25]]);
    assert_eq!(height, -4);

    // BPP = 32
    let bpp = u16::from_le_bytes([data[28], data[29]]);
    assert_eq!(bpp, 32);

    // BI_BITFIELDS
    let compression = u32::from_le_bytes([data[30], data[31], data[32], data[33]]);
    assert_eq!(compression, 3);

    std::fs::remove_file(&path).ok();
}

#[test]
fn fill_circle_basic() {
    let mut img = Image::new(100, 100, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
    // 中心 (50, 50)、半径 10 の円
    ctx.fill_circle(&Circle::new(50.0, 50.0, 10.0));
    ctx.end();

    // 中心は塗られている
    assert_eq!(read_pixel(&img, 50, 50), 0xFF_FF_00_00);
    // 半径内のピクセルも塗られている
    assert_eq!(read_pixel(&img, 55, 50), 0xFF_FF_00_00);
    assert_eq!(read_pixel(&img, 50, 45), 0xFF_FF_00_00);
    // 距離 > r + 1.0 のピクセルは塗られていない
    assert_eq!(read_pixel(&img, 50, 38), 0x00_00_00_00);
    assert_eq!(read_pixel(&img, 50, 62), 0x00_00_00_00);
    assert_eq!(read_pixel(&img, 38, 50), 0x00_00_00_00);
    assert_eq!(read_pixel(&img, 62, 50), 0x00_00_00_00);
}

#[test]
fn fill_circle_clip() {
    let mut img = Image::new(50, 50, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
    // 中心が右下端に近く、円が画像外にはみ出す
    ctx.fill_circle(&Circle::new(45.0, 45.0, 20.0));
    ctx.end();

    // クリップされた範囲内は白
    assert_eq!(read_pixel(&img, 45, 45), 0xFF_FF_FF_FF);
    assert_eq!(read_pixel(&img, 49, 45), 0xFF_FF_FF_FF);
    // 距離 > r + 1.0 の円外は黒
    assert_eq!(read_pixel(&img, 23, 45), 0x00_00_00_00);
}

#[test]
fn fill_circle_outside() {
    let mut img = Image::new(50, 50, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
    // 完全に画像外
    ctx.fill_circle(&Circle::new(200.0, 200.0, 10.0));
    ctx.end();

    // 全ピクセルがゼロのまま
    assert_eq!(read_pixel(&img, 0, 0), 0x00_00_00_00);
    assert_eq!(read_pixel(&img, 49, 49), 0x00_00_00_00);
}

#[test]
fn fill_circle_zero_radius() {
    let mut img = Image::new(50, 50, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
    // 半径 0
    ctx.fill_circle(&Circle::new(25.0, 25.0, 0.0));
    // 負の半径
    ctx.fill_circle(&Circle::new(25.0, 25.0, -5.0));
    ctx.end();

    // 何も描画されない
    assert_eq!(read_pixel(&img, 25, 25), 0x00_00_00_00);
}

#[test]
fn fill_circle_src_over() {
    let mut img = Image::new(100, 100, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);

    // まず不透明青で背景を塗る
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0x00, 0x00, 0xFF, 0xFF));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 100.0, 100.0));

    // 半透明赤の円を SrcOver で重ねる
    ctx.set_comp_op(CompOp::SrcOver);
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0x80));
    ctx.fill_circle(&Circle::new(50.0, 50.0, 30.0));
    ctx.end();

    // 円の中心はブレンドされている
    let pixel = read_pixel(&img, 50, 50);
    let a = (pixel >> 24) & 0xFF;
    let r = (pixel >> 16) & 0xFF;
    let b = pixel & 0xFF;
    assert!(a >= 254, "alpha: {a}");
    assert!((126..=129).contains(&r), "red: {r}");
    assert!((126..=128).contains(&b), "blue: {b}");

    // 距離 > r + 1.0 の円外は元の青のまま
    assert_eq!(read_pixel(&img, 0, 0), 0xFF_00_00_FF);
    assert_eq!(read_pixel(&img, 99, 99), 0xFF_00_00_FF);
}
