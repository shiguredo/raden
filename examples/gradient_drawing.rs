use raden::{
    CompOp, Context, ExtendMode, Gradient, Image, PipelineRuntime, PixelFormat, Rect, Rgba32,
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

    // =========================================================================
    // 上段: Linear グラデーション (Pad / Repeat / Reflect)
    // =========================================================================
    let cell_w = 360.0;
    let cell_h = 200.0;
    let margin = 20.0;
    let base_y = 30.0;

    let extend_modes = [
        (ExtendMode::Pad, "Pad"),
        (ExtendMode::Repeat, "Repeat"),
        (ExtendMode::Reflect, "Reflect"),
    ];

    for (i, &(mode, _label)) in extend_modes.iter().enumerate() {
        let x = margin + (cell_w + margin) * i as f64;
        let y = base_y;

        // グラデーションの始点と終点を矩形の中央 1/3 に設定し、
        // 範囲外処理の違いが見えるようにする
        let gx0 = x + cell_w * 0.33;
        let gx1 = x + cell_w * 0.67;

        let mut grad = Gradient::new_linear(gx0, y, gx1, y);
        grad.add_stop(0.0, Rgba32::rgb(0x00, 0x80, 0xFF));
        grad.add_stop(0.5, Rgba32::rgb(0xFF, 0xFF, 0x00));
        grad.add_stop(1.0, Rgba32::rgb(0xFF, 0x20, 0x60));
        grad.set_extend_mode(mode);

        ctx.set_fill_style_gradient(&grad);
        ctx.fill_rect(&Rect::new(x, y, cell_w, cell_h));
    }

    // =========================================================================
    // 下段左: Radial グラデーション
    // =========================================================================
    let radial_x = margin;
    let radial_y = base_y + cell_h + margin;
    let radial_w = (width as f64 - margin * 3.0) / 2.0;
    let radial_h = height as f64 - radial_y - margin;

    let cx = radial_x + radial_w * 0.5;
    let cy = radial_y + radial_h * 0.5;
    let r = radial_w.min(radial_h) * 0.45;

    // 焦点を中心からずらす
    let fx = cx - r * 0.3;
    let fy = cy - r * 0.2;

    let mut radial = Gradient::new_radial(cx, cy, fx, fy, 0.0, r);
    radial.add_stop(0.0, Rgba32::rgb(0xFF, 0xFF, 0xFF));
    radial.add_stop(0.3, Rgba32::rgb(0xFF, 0xA0, 0x00));
    radial.add_stop(0.7, Rgba32::rgb(0xFF, 0x20, 0x60));
    radial.add_stop(1.0, Rgba32::rgb(0x20, 0x00, 0x40));

    ctx.set_fill_style_gradient(&radial);
    ctx.fill_rect(&Rect::new(radial_x, radial_y, radial_w, radial_h));

    // =========================================================================
    // 下段右: Conic グラデーション
    // =========================================================================
    let conic_x = radial_x + radial_w + margin;
    let conic_y = radial_y;
    let conic_w = radial_w;
    let conic_h = radial_h;

    let ccx = conic_x + conic_w * 0.5;
    let ccy = conic_y + conic_h * 0.5;

    let mut conic = Gradient::new_conic(ccx, ccy, 0.0);
    conic.add_stop(0.0, Rgba32::rgb(0xFF, 0x00, 0x00));
    conic.add_stop(0.17, Rgba32::rgb(0xFF, 0xFF, 0x00));
    conic.add_stop(0.33, Rgba32::rgb(0x00, 0xFF, 0x00));
    conic.add_stop(0.50, Rgba32::rgb(0x00, 0xFF, 0xFF));
    conic.add_stop(0.67, Rgba32::rgb(0x00, 0x00, 0xFF));
    conic.add_stop(0.83, Rgba32::rgb(0xFF, 0x00, 0xFF));
    conic.add_stop(1.0, Rgba32::rgb(0xFF, 0x00, 0x00));

    ctx.set_fill_style_gradient(&conic);
    ctx.fill_rect(&Rect::new(conic_x, conic_y, conic_w, conic_h));

    ctx.end();

    // BMP 出力
    img.write_to_file("gradient_output.bmp")?;

    // raw_player で表示
    init()?;
    let window = Window::new("raden gradient drawing", width as i32, height as i32)?;
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
