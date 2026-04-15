use raden::{
    Circle, CompOp, Context, Image, Line, Path, PipelineRuntime, PixelFormat, Rect, Rgba32,
    StrokeCap, StrokeJoin,
};
use raw_player::{
    Event, KEYCODE_ESCAPE, Renderer, Texture, VideoFormat, Window, init, poll_event, quit,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let width = 1280u32;
    let height = 720u32;
    let mut img = Image::new(width, height, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);

    // 暗い背景
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0x1A, 0x1A, 0x2E, 0xFF));
    ctx.fill_rect(&Rect::new(0.0, 0.0, width as f64, height as f64));

    ctx.set_comp_op(CompOp::SrcOver);

    // --- 線分のストローク (異なるキャップ) ---
    ctx.save();

    // Butt キャップ
    ctx.set_stroke_style(Rgba32::new(0xFF, 0x60, 0x60, 0xFF));
    ctx.set_stroke_width(8.0);
    ctx.set_stroke_cap(StrokeCap::Butt);
    ctx.stroke_line(&Line::new(50.0, 80.0, 350.0, 80.0));

    // Square キャップ
    ctx.set_stroke_style(Rgba32::new(0x60, 0xFF, 0x60, 0xFF));
    ctx.set_stroke_cap(StrokeCap::Square);
    ctx.stroke_line(&Line::new(50.0, 120.0, 350.0, 120.0));

    // Round キャップ
    ctx.set_stroke_style(Rgba32::new(0x60, 0x60, 0xFF, 0xFF));
    ctx.set_stroke_cap(StrokeCap::Round);
    ctx.stroke_line(&Line::new(50.0, 160.0, 350.0, 160.0));

    ctx.restore();

    // --- 矩形のストローク (異なるジョイン) ---
    ctx.save();

    // Bevel ジョイン
    ctx.set_stroke_style(Rgba32::new(0xFF, 0xA0, 0x00, 0xFF));
    ctx.set_stroke_width(6.0);
    ctx.set_stroke_cap(StrokeCap::Butt);
    ctx.set_stroke_join(StrokeJoin::Bevel);
    ctx.stroke_rect(&Rect::new(50.0, 220.0, 200.0, 150.0));

    // MiterBevel ジョイン
    ctx.set_stroke_style(Rgba32::new(0x00, 0xFF, 0xA0, 0xFF));
    ctx.set_stroke_join(StrokeJoin::MiterBevel);
    ctx.stroke_rect(&Rect::new(300.0, 220.0, 200.0, 150.0));

    // Round ジョイン
    ctx.set_stroke_style(Rgba32::new(0xA0, 0x00, 0xFF, 0xFF));
    ctx.set_stroke_join(StrokeJoin::Round);
    ctx.stroke_rect(&Rect::new(550.0, 220.0, 200.0, 150.0));

    ctx.restore();

    // --- 円のストローク ---
    ctx.save();

    ctx.set_stroke_style(Rgba32::new(0xFF, 0xFF, 0x00, 0xC0));
    ctx.set_stroke_width(4.0);
    ctx.stroke_circle(&Circle::new(900.0, 300.0, 80.0));

    ctx.restore();

    // --- 複雑なパスのストローク ---
    ctx.save();

    // 三角形パス (開いたパス)
    ctx.set_stroke_style(Rgba32::new(0xFF, 0x80, 0xC0, 0xFF));
    ctx.set_stroke_width(5.0);
    ctx.set_stroke_cap(StrokeCap::Round);
    ctx.set_stroke_join(StrokeJoin::Round);
    let mut triangle = Path::new();
    triangle.move_to(150.0, 450.0);
    triangle.line_to(300.0, 600.0);
    triangle.line_to(0.0, 600.0);
    triangle.close();
    ctx.stroke_path(&triangle);

    // 星形パス (開いたパス)
    ctx.set_stroke_style(Rgba32::new(0xFF, 0xD7, 0x00, 0xFF));
    ctx.set_stroke_width(3.0);
    ctx.set_stroke_cap(StrokeCap::Round);
    ctx.set_stroke_join(StrokeJoin::MiterBevel);
    ctx.set_stroke_miter_limit(10.0);
    let mut star = Path::new();
    let cx = 500.0;
    let cy = 550.0;
    let outer_r = 80.0;
    let inner_r = 35.0;
    for i in 0..10 {
        let angle = std::f64::consts::PI * 2.0 * (i as f64) / 10.0 - std::f64::consts::FRAC_PI_2;
        let r = if i % 2 == 0 { outer_r } else { inner_r };
        let x = cx + angle.cos() * r;
        let y = cy + angle.sin() * r;
        if i == 0 {
            star.move_to(x, y);
        } else {
            star.line_to(x, y);
        }
    }
    star.close();
    ctx.stroke_path(&star);

    ctx.restore();

    // --- 異なる幅の線分 ---
    ctx.save();

    ctx.set_stroke_cap(StrokeCap::Round);
    ctx.set_stroke_join(StrokeJoin::Round);
    let widths = [1.0, 2.0, 4.0, 8.0, 12.0];
    for (i, &w) in widths.iter().enumerate() {
        let y = 480.0 + i as f64 * 30.0;
        let brightness = 0x80 + (i as u8) * 0x18;
        ctx.set_stroke_style(Rgba32::new(brightness, brightness, 0xFF, 0xFF));
        ctx.set_stroke_width(w);
        ctx.stroke_line(&Line::new(700.0, y, 1000.0, y));
    }

    ctx.restore();

    // --- 半透明ストローク ---
    ctx.save();

    ctx.set_stroke_style(Rgba32::new(0xFF, 0x00, 0x00, 0x60));
    ctx.set_stroke_width(20.0);
    ctx.set_stroke_cap(StrokeCap::Round);
    ctx.stroke_circle(&Circle::new(1100.0, 550.0, 100.0));

    ctx.set_stroke_style(Rgba32::new(0x00, 0xFF, 0x00, 0x60));
    ctx.stroke_circle(&Circle::new(1150.0, 500.0, 100.0));

    ctx.restore();

    ctx.end();

    // BMP 出力
    img.write_to_file("stroke_output.bmp")?;

    // raw_player で表示
    init()?;
    let window = Window::new("raden stroke drawing", width as i32, height as i32)?;
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
