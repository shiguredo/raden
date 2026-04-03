//! blit、矩形クリップ、画像パターン (フィルタ・変換)、拡張パス命令 (smooth / conic / arc)、行列 (回転・せん断) のサンプル。

use raden::{
    Circle, CompOp, Context, Image, Matrix2D, Path, Pattern, PatternFilter, PipelineRuntime,
    PixelFormat, Rect, Rgba32, StrokeCap, StrokeJoin, premultiply_rgba,
};
use raw_player::{
    Event, KEYCODE_ESCAPE, Renderer, Texture, VideoFormat, Window, init, poll_event, quit,
};

/// チェッカー模様の `Prgb32` 画像 (blit 用。回転や拡大の違いが分かる)。
fn checkerboard_tile(size: u32) -> Image {
    let mut img = Image::new(size, size, PixelFormat::Prgb32);
    let stride = img.stride();
    let data = img.data_mut();
    let c0 = premultiply_rgba(0xC8, 0x50, 0x50, 0xFF);
    let c1 = premultiply_rgba(0x50, 0xC8, 0xC8, 0xFF);
    let cell = 8u32;
    for y in 0..size {
        for x in 0..size {
            let ix = x / cell;
            let iy = y / cell;
            let c = if (ix + iy).is_multiple_of(2) { c0 } else { c1 };
            let off = y as usize * stride + x as usize * 4;
            data[off..off + 4].copy_from_slice(&c.to_le_bytes());
        }
    }
    img
}

/// 横ストライプの `Prgb32` 画像 (パターン塗り専用。チェッカーとの見分け用)。
fn horizontal_stripe_tile(size: u32) -> Image {
    let mut img = Image::new(size, size, PixelFormat::Prgb32);
    let stride = img.stride();
    let data = img.data_mut();
    let c0 = premultiply_rgba(0xFF, 0xA0, 0x20, 0xFF);
    let c1 = premultiply_rgba(0x20, 0x40, 0xE8, 0xFF);
    let band = 6u32;
    for y in 0..size {
        for x in 0..size {
            let c = if (y / band).is_multiple_of(2) { c0 } else { c1 };
            let off = y as usize * stride + x as usize * 4;
            data[off..off + 4].copy_from_slice(&c.to_le_bytes());
        }
    }
    img
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let width = 1280u32;
    let height = 720u32;
    let mut img = Image::new(width, height, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);

    let tile = checkerboard_tile(56);
    let stripe = horizontal_stripe_tile(48);

    // クリップ窓: 円の中心を窓より左にずらし、円が窓の左辺で「真っ直ぐに切れる」ようにする
    let clip_window = Rect::new(420.0, 80.0, 280.0, 220.0);

    // 背景
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0x12, 0x14, 0x22, 0xFF));
    ctx.fill_rect(&Rect::new(0.0, 0.0, width as f64, height as f64));
    ctx.set_comp_op(CompOp::SrcOver);

    // --- blit: 行列で回転した転送 (Nearest)。タイルを大きめにして斜めが分かるようにする ---
    let half = tile.width() as f64 * 0.5;
    ctx.save();
    ctx.post_translate(220.0, 180.0);
    ctx.post_rotate(0.85);
    ctx.blit_image_at(-half, -half, &tile);
    ctx.restore();

    // --- 矩形クリップ: 円を描くが窓の外は描かれない → 左辺が直線で円が欠ける (矩形二枚重ねに見えない) ---
    ctx.save();
    ctx.clip_to_rect(&clip_window);
    ctx.set_fill_style(Rgba32::new(0xFF, 0xDD, 0x20, 0xFF));
    // 中心を窓の左側に置き、円の一部だけが窓内に入る
    ctx.fill_circle(&Circle::new(330.0, 190.0, 175.0));
    ctx.restore();

    // --- パターン: ストライプ画像 + Bilinear + スケール (チェッカー blit とは柄が違う) ---
    let mut pat = Pattern::new(
        stripe.data(),
        stripe.width(),
        stripe.height(),
        stripe.stride(),
    );
    pat.set_filter(PatternFilter::Bilinear);
    pat.set_transform(Matrix2D::scaling(1.4, 1.4));
    ctx.set_fill_style_pattern(&pat);
    ctx.fill_rect(&Rect::new(80.0, 380.0, 320.0, 240.0));
    ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));

    // --- 拡張パス + せん断した座標系 ---
    ctx.save();
    ctx.post_translate(520.0, 360.0);
    ctx.post_skew(0.12, 0.0);

    let mut p = Path::new();
    // smooth_quad_to: 直前の quad_to が必要
    p.move_to(0.0, 0.0);
    p.quad_to(40.0, -60.0, 80.0, 0.0);
    p.smooth_quad_to(120.0, 0.0);

    // smooth_cubic_to: 直前の cubic_to が必要
    p.move_to(0.0, 80.0);
    p.cubic_to(10.0, 40.0, 50.0, 40.0, 60.0, 80.0);
    p.smooth_cubic_to(110.0, 120.0, 160.0, 80.0);

    // 円錐曲線 (有理二次)
    p.move_to(180.0, 20.0);
    p.conic_to(220.0, 60.0, 260.0, 20.0, 0.85);

    // 楕円弧 (arc_to)
    p.move_to(300.0, 80.0);
    p.arc_to(
        340.0,
        50.0,
        35.0,
        28.0,
        0.0,
        std::f64::consts::PI * 1.2,
        true,
    );

    ctx.set_fill_style(Rgba32::new(0x66, 0xAA, 0xFF, 0xD0));
    ctx.set_stroke_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
    ctx.set_stroke_width(2.0);
    ctx.set_stroke_cap(StrokeCap::Round);
    ctx.set_stroke_join(StrokeJoin::Round);
    ctx.fill_path(&p);
    ctx.stroke_path(&p);
    ctx.restore();

    // --- もう一枚 blit (SrcCopy で矩形を上書き。チェッカーを粗く拡大) ---
    ctx.save();
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.blit_image_rect(
        &Rect::new(900.0, 420.0, 160.0, 120.0),
        &tile,
        Some(Rect::new(
            0.0,
            0.0,
            tile.width() as f64,
            tile.height() as f64,
        )),
    );
    ctx.restore();

    ctx.end();

    img.write_to_file("blit_clip_pattern_paths_output.bmp")?;

    init()?;
    let window = Window::new(
        "raden: blit / clip / pattern / paths",
        width as i32,
        height as i32,
    )?;
    let mut renderer = Renderer::new(&window)?;
    let mut texture = Texture::new(&renderer, VideoFormat::Bgra, width as i32, height as i32)?;
    texture.update_packed(img.data(), img.stride() as i32)?;

    loop {
        renderer.clear()?;
        renderer.copy(&texture)?;
        renderer.present()?;
        if let Some(event) = poll_event() {
            match event {
                Event::Quit | Event::WindowClose => break,
                Event::KeyDown { keycode } if keycode == KEYCODE_ESCAPE => break,
                _ => {}
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    }

    // SAFETY: quit はメインスレッドから一度だけ呼び出す
    unsafe { quit() };
    Ok(())
}
