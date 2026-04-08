use raden::{CompOp, Context, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};
use raw_player::{
    Event, KEYCODE_ESCAPE, Renderer, Texture, VideoFormat, Window, init, poll_event, quit,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // raden で描画
    let mut img = Image::new(1280, 720, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);

    // 青背景 (SrcCopy で全面上書き)
    ctx.set_comp_op(CompOp::SrcCopy);
    ctx.set_fill_style(Rgba32::new(0x20, 0x20, 0x60, 0xFF));
    ctx.fill_rect(&Rect::new(0.0, 0.0, 1280.0, 720.0));

    // 半透明矩形を重ねる (SrcOver)
    ctx.set_comp_op(CompOp::SrcOver);

    // 不透明赤矩形
    ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
    ctx.fill_rect(&Rect::new(10.0, 10.0, 400.0, 300.0));

    // 半透明緑矩形
    ctx.set_fill_style(Rgba32::new(0x00, 0xFF, 0x00, 0x80));
    ctx.fill_rect(&Rect::new(200.0, 150.0, 400.0, 300.0));

    // 半透明青矩形
    ctx.set_fill_style(Rgba32::new(0x00, 0x00, 0xFF, 0x80));
    ctx.fill_rect(&Rect::new(500.0, 300.0, 400.0, 300.0));

    ctx.end();

    // BMP 出力
    img.write_to_file("output.bmp")?;

    // raw_player で表示 (PRGB32 は LE で BGRA バイト順 → VideoFormat::Bgra)
    init()?;
    let window = Window::new("raden basic drawing", 1280, 720)?;
    let mut renderer = Renderer::new(&window)?;
    let mut texture = Texture::new(&renderer, VideoFormat::Bgra, 1280, 720)?;
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

    unsafe {
        quit();
    }
    Ok(())
}
