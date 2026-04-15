//! Raden アニメーションプレーヤー
//!
//! blend2d-rs の blend2d_player.rs と同等のアニメーションを
//! raden で描画し、raw-player-rs の VideoPlayer で表示する。
//!
//! フレーム生成は別スレッドで行い、メインスレッドは SDL イベント処理と
//! VSync 同期に専念する。これにより VSync ブロッキング中に次フレームの
//! 生成を並行して進められる。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use raden::{
    Circle, CompOp, Context, Font, FontData, FontFace, Image, PipelineRuntime, PixelFormat, Rect,
    Rgba32,
};
use raw_player::{KEYCODE_ESCAPE, VideoPlayer};

const WIDTH: i32 = 1920;
const HEIGHT: i32 = 1080;
const TARGET_FPS: i32 = 120;

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

/// raden でアニメーションフレームを生成する。
fn generate_frame(
    img: &mut Image,
    runtime: &mut PipelineRuntime,
    font_large: &Font,
    font_small: &Font,
    frame_number: u64,
    target_fps: i32,
    actual_fps: f64,
) {
    let w = img.width() as f64;
    let h = img.height() as f64;
    let t = frame_number as f64 / target_fps as f64;

    let mut ctx = Context::new(img, runtime);

    // --- 背景: HSV で色相がゆっくり変化する暗い背景 (SrcCopy) ---
    ctx.set_comp_op(CompOp::SrcCopy);
    let bg_hue = (t * 20.0) % 360.0;
    let (br, bg, bb) = hsv_to_rgb(bg_hue, 0.3, 0.15);
    ctx.set_fill_style(Rgba32::rgb(br, bg, bb));
    ctx.fill_rect(&Rect::new(0.0, 0.0, w, h));

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

    // --- 経過時間をミリ秒で大きく表示 (影付き) ---
    let elapsed_ms = (t * 1000.0) as u64;
    let time_text = format!("{elapsed_ms:08} ms");
    let text_x = w * 0.5 - 200.0;
    let text_y = h * 0.85;

    // 影
    ctx.set_fill_style(Rgba32::new(0, 0, 0, 200));
    ctx.fill_text(text_x + 3.0, text_y + 3.0, font_large, &time_text);
    // 本体
    ctx.set_fill_style(Rgba32::new(255, 255, 255, 255));
    ctx.fill_text(text_x, text_y, font_large, &time_text);

    // --- 情報表示 ---
    ctx.set_fill_style(Rgba32::new(255, 255, 255, 200));
    let info_text = format!("{}x{} | {} FPS | RAW BGRA", w as u32, h as u32, target_fps);
    ctx.fill_text(30.0, 50.0, font_small, &info_text);

    // フレーム番号
    ctx.set_fill_style(Rgba32::new(200, 200, 200, 255));
    let frame_text = format!("Frame: {frame_number:06}");
    ctx.fill_text(30.0, 95.0, font_small, &frame_text);

    // 実際の FPS (緑)
    ctx.set_fill_style(Rgba32::new(150, 255, 150, 255));
    let fps_text = format!("FPS: {actual_fps:.1}");
    ctx.fill_text(30.0, 140.0, font_small, &fps_text);

    ctx.end();
}

/// 生成スレッドからメインスレッドに渡すフレームデータ。
struct FrameData {
    pixels: Vec<u8>,
    pts_us: i64,
}

/// macOS のシステムフォントパス。
const FONT_PATH: &str = "/System/Library/Fonts/Helvetica.ttc";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // このサンプルは macOS のシステムフォントを使用する
    if !std::path::Path::new(FONT_PATH).exists() {
        eprintln!("フォントが見つかりません: {FONT_PATH}");
        eprintln!("このサンプルは macOS でのみ動作します");
        return Ok(());
    }

    let player = VideoPlayer::new(WIDTH, HEIGHT, "Raden Animation")?;
    println!("Renderer: {}", player.renderer_name());

    // ESC または q で終了
    player.set_key_callback(Some(|keycode: u32| -> bool {
        keycode != KEYCODE_ESCAPE && keycode != b'q' as u32
    }));

    player.play()?;

    println!("{WIDTH}x{HEIGHT} | {TARGET_FPS} FPS | RAW BGRA");
    println!("ESC または q キーで終了...");

    // フレームデータのチャネル (2 フレーム分のバッファ)
    let (tx, rx) = mpsc::sync_channel::<FrameData>(2);

    // 生成スレッドの停止フラグ
    let running = Arc::new(AtomicBool::new(true));
    let gen_running = Arc::clone(&running);

    // フレーム生成スレッド
    let gen_thread = std::thread::spawn(move || {
        let mut img = Image::new(WIDTH as u32, HEIGHT as u32, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();

        // フォントをロード
        let font_data = FontData::from_file(FONT_PATH).expect("フォントファイルの読み込みに失敗");
        let font_face = FontFace::from_data(&font_data, 0).expect("フォントフェイスの作成に失敗");
        let font_large = Font::from_face(&font_face, 72.0);
        let font_small = Font::from_face(&font_face, 32.0);

        let start_time = Instant::now();
        let frame_interval = Duration::from_secs_f64(1.0 / TARGET_FPS as f64);
        let mut next_frame_time = Instant::now();
        let mut frame_number: u64 = 0;

        while gen_running.load(Ordering::Relaxed) {
            // 次のフレーム時刻まで待機
            let now = Instant::now();
            if now < next_frame_time {
                std::thread::sleep(next_frame_time - now);
            }
            next_frame_time += frame_interval;

            // 現在の FPS を計算
            let total_elapsed = start_time.elapsed().as_secs_f64();
            let current_fps = if total_elapsed > 0.0 {
                frame_number as f64 / total_elapsed
            } else {
                0.0
            };

            // フレーム生成
            generate_frame(
                &mut img,
                &mut runtime,
                &font_large,
                &font_small,
                frame_number,
                TARGET_FPS,
                current_fps,
            );

            // ピクセルデータをチャネル経由でメインスレッドに送信
            // PTS は壁時計ベースで設定する。フレーム番号ベース (N * 8333us) だと
            // VSync ジッターで SDL クロックと乖離し、sync_threshold を超えて
            // ドロップされる。壁時計 PTS なら SDL クロックと自然に追従する。
            let pts_us = start_time.elapsed().as_micros() as i64;
            let frame_data = FrameData {
                pixels: img.data().to_vec(),
                pts_us,
            };
            if tx.send(frame_data).is_err() {
                break;
            }

            frame_number += 1;
        }
    });

    // メインスレッド: SDL イベント処理 + enqueue
    let start_time = Instant::now();
    let mut last_report = Instant::now();
    let mut frame_number: u64 = 0;

    // 計測用
    let mut poll_total_us: u64 = 0;
    let mut recv_total_us: u64 = 0;
    let mut enqueue_total_us: u64 = 0;
    let mut loop_total_us: u64 = 0;
    let mut recv_hit: u64 = 0;
    let mut recv_miss: u64 = 0;
    let mut loop_count: u64 = 0;

    while player.is_open() {
        let loop_start = Instant::now();

        let recv_start = Instant::now();
        let received = rx.try_recv();
        recv_total_us += recv_start.elapsed().as_micros() as u64;

        if let Ok(frame_data) = received {
            recv_hit += 1;
            let enq_start = Instant::now();
            player.enqueue_video_bgra_owned(frame_data.pixels, WIDTH, HEIGHT, frame_data.pts_us)?;
            enqueue_total_us += enq_start.elapsed().as_micros() as u64;
            frame_number += 1;
        } else {
            recv_miss += 1;
        }

        let poll_start = Instant::now();
        if !player.poll_events()? {
            break;
        }
        poll_total_us += poll_start.elapsed().as_micros() as u64;

        loop_total_us += loop_start.elapsed().as_micros() as u64;
        loop_count += 1;

        // 毎秒統計を表示
        if last_report.elapsed() >= Duration::from_secs(1) {
            let total_elapsed = start_time.elapsed().as_secs_f64();
            let current_fps = if total_elapsed > 0.0 {
                frame_number as f64 / total_elapsed
            } else {
                0.0
            };
            let stats = player.stats();
            let avg_poll = if loop_count > 0 {
                poll_total_us / loop_count
            } else {
                0
            };
            let avg_recv = if loop_count > 0 {
                recv_total_us / loop_count
            } else {
                0
            };
            let avg_enq = if recv_hit > 0 {
                enqueue_total_us / recv_hit
            } else {
                0
            };
            let avg_loop = if loop_count > 0 {
                loop_total_us / loop_count
            } else {
                0
            };
            println!(
                "Frame {frame_number}: FPS={current_fps:.1}, queue={}, drop={}, repeat={} | \
                 loop={avg_loop}us poll={avg_poll}us recv={avg_recv}us enq={avg_enq}us \
                 hit={recv_hit} miss={recv_miss} | \
                 tex={}us(max={}) cc={}us present={}us vsync={}us",
                stats.video_queue_size,
                stats.dropped_frames,
                stats.repeated_frames,
                stats.avg_texture_update_us,
                stats.max_texture_update_us,
                stats.avg_clear_copy_us,
                stats.avg_present_us,
                stats.avg_vsync_interval_us,
            );
            last_report = Instant::now();
        }
    }

    // 生成スレッドを停止
    running.store(false, Ordering::Relaxed);
    // チャネルを空にして生成スレッドの send ブロックを解除する
    while rx.try_recv().is_ok() {}
    gen_thread.join().unwrap();

    player.close();
    // SAFETY: quit はメインスレッドから一度だけ呼び出す
    unsafe { raw_player::quit() };

    // 最終統計
    let total_time = start_time.elapsed().as_secs_f64();
    if total_time > 0.0 {
        let stats = player.stats();
        println!();
        println!("生成フレーム数: {frame_number}");
        println!("平均 FPS: {:.2}", frame_number as f64 / total_time);
        println!("ドロップフレーム: {}", stats.dropped_frames);
        println!("リピートフレーム: {}", stats.repeated_frames);
    }

    Ok(())
}
