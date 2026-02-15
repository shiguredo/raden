use std::time::Instant;

use raden::{Circle, CompOp, Context, Image, PipelineRuntime, PixelFormat, Rgba32};
use raw_player::{
    BLENDMODE_BLEND, Event, KEYCODE_ESCAPE, Renderer, Texture, VideoFormat, Window, init,
    poll_event, quit,
};

const NUM_WAVES: usize = 5;
const NUM_BALLS: usize = 8;
const NUM_SHAPES: usize = 6;

/// HSV を RGB に変換する (h: 0-360, s: 0-1, v: 0-1)。
fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

fn render_frame(ctx: &mut Context, t: f64, w: f64, h: f64) {
    // --- 背景: HSV で色相がゆっくり変化する暗い背景 (SrcCopy) ---
    ctx.set_comp_op(CompOp::SrcCopy);
    let bg_hue = (t * 20.0) % 360.0;
    let (br, bg, bb) = hsv_to_rgb(bg_hue, 0.3, 0.15);
    ctx.set_fill_style(Rgba32::rgb(br, bg, bb));
    ctx.fill_all();

    ctx.set_comp_op(CompOp::SrcOver);

    // --- ウェーブパターン: 横方向に並ぶ円が sin 波で上下に動く ---
    for wave_idx in 0..NUM_WAVES {
        let wi = wave_idx as f64;
        let wave_offset = wi * 0.5;
        let wave_amplitude = 50.0 + wi * 20.0;
        let wave_freq = 0.008 + wi * 0.002;
        let wave_speed = 2.0 + wi * 0.3;
        let wave_y_base = h * 0.3 + wi * 80.0;

        let wave_hue = (t * 60.0 + wi * 50.0) % 360.0;
        let (wr, wg, wb) = hsv_to_rgb(wave_hue, 0.8, 0.9);
        ctx.set_fill_style(Rgba32::new(wr, wg, wb, 150));

        let mut x = 0.0;
        while x < w {
            let y =
                wave_y_base + wave_amplitude * (wave_freq * x + t * wave_speed + wave_offset).sin();
            let radius = 8.0 + 4.0 * (t * 3.0 + x * 0.01).sin();
            ctx.fill_circle(&Circle::new(x, y, radius));
            x += 20.0;
        }
    }

    // --- バウンドする円: リサージュ曲線風に動く複数の半透明円 ---
    for ball_idx in 0..NUM_BALLS {
        let bi = ball_idx as f64;
        let freq_x = 0.5 + bi * 0.15;
        let freq_y = 0.7 + bi * 0.12;
        let phase_x = bi * std::f64::consts::PI / 4.0;
        let phase_y = bi * std::f64::consts::PI / 3.0;

        let bx = w * 0.5 + (w * 0.35) * (t * freq_x + phase_x).sin();
        let by = h * 0.5 + (h * 0.3) * (t * freq_y + phase_y).sin();
        let ball_radius = 30.0 + 15.0 * (t * 4.0 + bi).sin();

        let ball_hue = (bi * 45.0 + t * 100.0) % 360.0;
        let (cr, cg, cb) = hsv_to_rgb(ball_hue, 1.0, 1.0);
        ctx.set_fill_style(Rgba32::new(cr, cg, cb, 200));
        ctx.fill_circle(&Circle::new(bx, by, ball_radius));

        // 光沢効果 (小さい白い円、左上寄りに配置)
        ctx.save();
        let hl_radius = ball_radius * 0.3;
        ctx.set_fill_style(Rgba32::new(255, 255, 255, 100));
        ctx.fill_circle(&Circle::new(
            bx - ball_radius * 0.3,
            by - ball_radius * 0.3,
            hl_radius,
        ));
        ctx.restore();
    }

    // --- 回転する円群: 中央周囲を周回する円 ---
    let center_x = w * 0.5;
    let center_y = h * 0.5;
    for shape_idx in 0..NUM_SHAPES {
        let si = shape_idx as f64;
        let angle = t * (1.0 + si * 0.2) + si * std::f64::consts::PI / 3.0;
        let dist = 150.0 + 50.0 * (t * 2.0 + si).sin();
        let sx = center_x + dist * angle.cos();
        let sy = center_y + dist * angle.sin();

        let shape_hue = (si * 60.0 + t * 80.0) % 360.0;
        let (sr, sg, sb) = hsv_to_rgb(shape_hue, 0.9, 0.95);

        let shape_radius = 25.0 + 15.0 * (t * 3.0 + si).sin();
        ctx.set_fill_style(Rgba32::new(sr, sg, sb, 180));
        ctx.fill_circle(&Circle::new(sx, sy, shape_radius));
    }
}

/// SDL レンダラーのテキスト描画でオーバーレイ情報を表示する。
fn render_text_overlay(
    renderer: &mut Renderer,
    t: f64,
    frame_count: u64,
    current_fps: f64,
    width: u32,
    height: u32,
    target_fps: u32,
) -> Result<(), raw_player::Error> {
    renderer.set_draw_blend_mode(BLENDMODE_BLEND)?;

    // 経過時間をミリ秒で大きく表示 (影付き)
    let elapsed_ms = (t * 1000.0) as u64;
    let time_text = format!("{:08} ms", elapsed_ms);
    // 影
    renderer.set_draw_color(0, 0, 0, 200)?;
    renderer.debug_text(42.0, height as f32 * 0.82 + 2.0, &time_text)?;
    // 本体
    renderer.set_draw_color(255, 255, 255, 255)?;
    renderer.debug_text(40.0, height as f32 * 0.82, &time_text)?;

    // 情報表示
    let info_text = format!("{}x{} | {} FPS | raden circle", width, height, target_fps);
    renderer.set_draw_color(255, 255, 255, 200)?;
    renderer.debug_text(10.0, 10.0, &info_text)?;

    // フレーム番号
    let frame_text = format!("Frame: {:06}", frame_count);
    renderer.set_draw_color(200, 200, 200, 255)?;
    renderer.debug_text(10.0, 26.0, &frame_text)?;

    // 実際の FPS (緑)
    let fps_text = format!("FPS: {:.1}", current_fps);
    renderer.set_draw_color(150, 255, 150, 255)?;
    renderer.debug_text(10.0, 42.0, &fps_text)?;

    Ok(())
}

/// ヘッドレスベンチマーク: ウィンドウなしで指定秒数分のフレームを描画し、gen 時間を計測する。
fn run_benchmark(width: u32, height: u32, fps: u32, duration_secs: f64) {
    let mut img = Image::new(width, height, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();

    let w = width as f64;
    let h = height as f64;
    let total_frames = (fps as f64 * duration_secs) as u64;
    let dt = 1.0 / fps as f64;

    // ウォームアップ (JIT コンパイルのコストを除外する)
    {
        let mut ctx = Context::new(&mut img, &mut runtime);
        render_frame(&mut ctx, 0.0, w, h);
        ctx.end();
    }

    let mut gen_times: Vec<f64> = Vec::with_capacity(total_frames as usize);

    for frame in 0..total_frames {
        let t = frame as f64 * dt;
        let gen_start = Instant::now();
        {
            let mut ctx = Context::new(&mut img, &mut runtime);
            render_frame(&mut ctx, t, w, h);
            ctx.end();
        }
        gen_times.push(gen_start.elapsed().as_secs_f64() * 1000.0);
    }

    gen_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let sum: f64 = gen_times.iter().sum();
    let avg = sum / gen_times.len() as f64;
    let median = gen_times[gen_times.len() / 2];
    let p95 = gen_times[(gen_times.len() as f64 * 0.95) as usize];
    let p99 = gen_times[(gen_times.len() as f64 * 0.99) as usize];
    let min = gen_times[0];
    let max = gen_times[gen_times.len() - 1];

    println!(
        "--- benchmark result ({}x{} {}fps, {} frames, {:.1}s) ---",
        width, height, fps, total_frames, duration_secs
    );
    println!("  avg:    {:.2} ms", avg);
    println!("  median: {:.2} ms", median);
    println!("  p95:    {:.2} ms", p95);
    println!("  p99:    {:.2} ms", p99);
    println!("  min:    {:.2} ms", min);
    println!("  max:    {:.2} ms", max);
}

/// コマンドライン引数からオプション値を取得する。
fn get_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|pos| args.get(pos + 1).cloned())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    let width: u32 = get_arg(&args, "--width")
        .map(|v| v.parse().expect("--width の値が不正です"))
        .unwrap_or(1280);
    let height: u32 = get_arg(&args, "--height")
        .map(|v| v.parse().expect("--height の値が不正です"))
        .unwrap_or(720);
    let fps: u32 = get_arg(&args, "--fps")
        .map(|v| v.parse().expect("--fps の値が不正です"))
        .unwrap_or(60);

    // --duration <秒> でヘッドレスベンチマークモードを起動する
    if let Some(duration_str) = get_arg(&args, "--duration") {
        let secs: f64 = duration_str.parse().expect("--duration の値が不正です");
        run_benchmark(width, height, fps, secs);
        return Ok(());
    }

    let mut img = Image::new(width, height, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();

    let w = width as f64;
    let h = height as f64;

    init()?;
    let window = Window::new("raden animation", width as i32, height as i32)?;
    let mut renderer = Renderer::new(&window)?;
    let mut texture = Texture::new(&renderer, VideoFormat::Bgra, width as i32, height as i32)?;

    let frame_interval = std::time::Duration::from_secs_f64(1.0 / fps as f64);
    let start = Instant::now();
    let mut frame_count: u64 = 0;
    let mut stats_time = Instant::now();
    let mut stats_frame_count: u64 = 0;
    let mut gen_time_sum = std::time::Duration::ZERO;
    let mut current_fps: f64 = 0.0;

    loop {
        let frame_start = Instant::now();

        // イベント処理
        while let Some(event) = poll_event() {
            match event {
                Event::Quit | Event::WindowClose => {
                    quit();
                    return Ok(());
                }
                Event::KeyDown { keycode } if keycode == KEYCODE_ESCAPE => {
                    quit();
                    return Ok(());
                }
                _ => {}
            }
        }

        // フレーム描画
        let t = start.elapsed().as_secs_f64();
        let gen_start = Instant::now();
        {
            let mut ctx = Context::new(&mut img, &mut runtime);
            render_frame(&mut ctx, t, w, h);
            ctx.end();
        }
        gen_time_sum += gen_start.elapsed();

        // テクスチャ更新と表示
        texture.update_packed(img.data(), img.stride() as i32)?;
        renderer.clear()?;
        renderer.copy(&texture)?;

        // テキストオーバーレイ (SDL レンダラーで描画)
        render_text_overlay(
            &mut renderer,
            t,
            frame_count,
            current_fps,
            width,
            height,
            fps,
        )?;

        renderer.present()?;

        frame_count += 1;
        stats_frame_count += 1;

        // 毎秒統計出力
        let stats_elapsed = stats_time.elapsed();
        if stats_elapsed >= std::time::Duration::from_secs(1) {
            current_fps = stats_frame_count as f64 / stats_elapsed.as_secs_f64();
            let avg_gen_ms = gen_time_sum.as_secs_f64() / stats_frame_count as f64 * 1000.0;
            println!(
                "Frame {}: FPS={:.1}, gen={:.2}ms",
                frame_count, current_fps, avg_gen_ms
            );
            stats_time = Instant::now();
            stats_frame_count = 0;
            gen_time_sum = std::time::Duration::ZERO;
        }

        // フレームペーシング
        let elapsed = frame_start.elapsed();
        if elapsed < frame_interval {
            std::thread::sleep(frame_interval - elapsed);
        }
    }
}
