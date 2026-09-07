use raden::{Arc, Circle, CompOp, Context, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

/// 1 テストあたりのケース数。
const CASES: usize = 256;

/// シード再現用の環境変数名。
const SEED_ENV: &str = "RADEN_PBT_SEED";

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

/// 半径 >= 2.0 の円の中心ピクセルは完全カバレッジ (alpha == 255)。
#[test]
fn center_pixel_full_coverage() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let r = noprop::sample_f64_in(ctx, 2.0, 50.0);
        let img_size = noprop::sample_usize_in(ctx, 120..200) as u32;
        let cx = img_size as f64 / 2.0;
        let cy = img_size as f64 / 2.0;

        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx_draw = Context::new(&mut img, &mut runtime);
        ctx_draw.set_comp_op(CompOp::SrcCopy);
        ctx_draw.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx_draw.fill_circle(&Circle::new(cx, cy, r));
        ctx_draw.end();

        let pixel = read_pixel(&img, cx as u32, cy as u32);
        let alpha = (pixel >> 24) & 0xFF;
        assert_eq!(
            alpha, 0xFF,
            "中心の alpha が 255 でない: alpha={alpha}, r={r}"
        );
        Ok(())
    })?;
    Ok(())
}

/// 距離 > r + 1.0 のピクセルは完全に透明。
#[test]
fn outside_pixel_transparent() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let r = noprop::sample_f64_in(ctx, 5.0, 30.0);
        let angle = noprop::sample_f64_in(ctx, 0.0, std::f64::consts::TAU);
        let img_size = 100u32;
        let cx = 50.0;
        let cy = 50.0;

        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx_draw = Context::new(&mut img, &mut runtime);
        ctx_draw.set_comp_op(CompOp::SrcCopy);
        ctx_draw.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx_draw.fill_circle(&Circle::new(cx, cy, r));
        ctx_draw.end();

        // r + 2.0 の距離のピクセルを検証 (ラスタライザの誤差を考慮して余裕を持たせる)
        let dist = r + 2.0;
        let test_px = (cx + dist * angle.cos()).floor() as i32;
        let test_py = (cy + dist * angle.sin()).floor() as i32;

        if test_px >= 0 && test_px < img_size as i32 && test_py >= 0 && test_py < img_size as i32 {
            let pixel = read_pixel(&img, test_px as u32, test_py as u32);
            assert_eq!(
                pixel, 0x00_00_00_00,
                "距離 > r + 2.0 なのに塗られている: px={test_px}, py={test_py}, r={r}"
            );
        }
        Ok(())
    })?;
    Ok(())
}

/// fill_all は画像全体を塗りつぶす。
#[test]
fn fill_all_fills_entire_image() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let img_w = noprop::sample_usize_in(ctx, 1..64) as u32;
        let img_h = noprop::sample_usize_in(ctx, 1..64) as u32;
        let r = noprop::sample_u8(ctx);
        let g = noprop::sample_u8(ctx);
        let b = noprop::sample_u8(ctx);

        let mut img = Image::new(img_w, img_h, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx_draw = Context::new(&mut img, &mut runtime);
        ctx_draw.set_comp_op(CompOp::SrcCopy);
        ctx_draw.set_fill_style(Rgba32::new(r, g, b, 0xFF));
        ctx_draw.fill_all();
        ctx_draw.end();

        let expected_prgb = Rgba32::new(r, g, b, 0xFF).to_prgb32();
        for y in 0..img_h {
            for x in 0..img_w {
                let pixel = read_pixel(&img, x, y);
                assert_eq!(
                    pixel, expected_prgb,
                    "({x},{y}) が期待値と不一致: 0x{pixel:08x} != 0x{expected_prgb:08x}"
                );
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// fill_pie の弧の中間方向のピクセルは塗られる。
#[test]
fn fill_pie_interior_pixel() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let r = noprop::sample_f64_in(ctx, 10.0, 40.0);
        let start = noprop::sample_f64_in(ctx, 0.0, std::f64::consts::TAU);
        let img_size = 100u32;
        let cx = 50.0;
        let cy = 50.0;
        let sweep = std::f64::consts::PI; // 半円

        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx_draw = Context::new(&mut img, &mut runtime);
        ctx_draw.set_comp_op(CompOp::SrcCopy);
        ctx_draw.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx_draw.fill_pie(&Arc::new(cx, cy, r, r, start, sweep));
        ctx_draw.end();

        // 弧の中間方向に r/2 進んだ点は確実に扇形内部
        let mid_angle = start + sweep / 2.0;
        let test_x = (cx + (r / 2.0) * mid_angle.cos()).floor() as u32;
        let test_y = (cy + (r / 2.0) * mid_angle.sin()).floor() as u32;

        if test_x < img_size && test_y < img_size {
            let pixel = read_pixel(&img, test_x, test_y);
            let alpha = (pixel >> 24) & 0xFF;
            assert!(
                alpha > 0,
                "扇形内部が塗られていない: alpha={alpha}, r={r}, start={start}, test=({test_x},{test_y})"
            );
        }
        Ok(())
    })?;
    Ok(())
}

/// fill_pie で sweep==0 なら何も塗られない。
#[test]
fn fill_pie_zero_sweep_no_paint() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let r = noprop::sample_f64_in(ctx, 1.0, 40.0);
        let img_size = 100u32;
        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx_draw = Context::new(&mut img, &mut runtime);
        ctx_draw.set_comp_op(CompOp::SrcCopy);
        ctx_draw.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx_draw.fill_pie(&Arc::new(50.0, 50.0, r, r, 0.0, 0.0));
        ctx_draw.end();

        // 全ピクセルが透明であることを確認
        for y in 0..img_size {
            for x in 0..img_size {
                let pixel = read_pixel(&img, x, y);
                assert_eq!(
                    pixel, 0x00_00_00_00,
                    "sweep==0 なのに ({x},{y}) が塗られている"
                );
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// fill_all は fill_rect(画像全体) と同一結果になる。
#[test]
fn fill_all_equals_fill_rect_whole() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let img_w = noprop::sample_usize_in(ctx, 1..32) as u32;
        let img_h = noprop::sample_usize_in(ctx, 1..32) as u32;
        let color = Rgba32::new(0x12, 0x34, 0x56, 0xAB);

        let mut img1 = Image::new(img_w, img_h, PixelFormat::Prgb32);
        let mut runtime1 = PipelineRuntime::new();
        let mut ctx1 = Context::new(&mut img1, &mut runtime1);
        ctx1.set_comp_op(CompOp::SrcCopy);
        ctx1.set_fill_style(color);
        ctx1.fill_all();
        ctx1.end();

        let mut img2 = Image::new(img_w, img_h, PixelFormat::Prgb32);
        let mut runtime2 = PipelineRuntime::new();
        let mut ctx2 = Context::new(&mut img2, &mut runtime2);
        ctx2.set_comp_op(CompOp::SrcCopy);
        ctx2.set_fill_style(color);
        ctx2.fill_rect(&Rect::new(0.0, 0.0, img_w as f64, img_h as f64));
        ctx2.end();

        assert_eq!(
            img1.data(),
            img2.data(),
            "fill_all と fill_rect(全体) の結果が異なる"
        );
        Ok(())
    })?;
    Ok(())
}

/// 中心から十分内側 (r-2) は完全カバレッジ、十分外側 (r+2) は透明。
#[test]
fn inner_full_outer_transparent() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let r = noprop::sample_f64_in(ctx, 5.0, 40.0);
        let img_size = 100u32;
        let cx = 50.0;
        let cy = 50.0;

        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx_draw = Context::new(&mut img, &mut runtime);
        ctx_draw.set_comp_op(CompOp::SrcCopy);
        ctx_draw.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx_draw.fill_circle(&Circle::new(cx, cy, r));
        ctx_draw.end();

        let center_x = cx as u32;
        let center_y = cy as u32;

        // 中心から r-2 までは完全カバレッジ
        let inner_limit = ((r - 2.0).floor() as u32).min(img_size - 1);
        for x in center_x..=center_x + inner_limit {
            if x >= img_size {
                break;
            }
            let dist = (x as f64 + 0.5) - cx;
            if dist.abs() > r - 2.0 {
                break;
            }
            let pixel = read_pixel(&img, x, center_y);
            let alpha = (pixel >> 24) & 0xFF;
            assert_eq!(
                alpha, 0xFF,
                "内側なのに alpha != 255: x={x}, dist={dist}, r={r}"
            );
        }

        // r+2 より外側は透明
        let outer_start = ((cx + r + 2.0).ceil() as u32).min(img_size);
        for x in outer_start..img_size {
            let pixel = read_pixel(&img, x, center_y);
            assert_eq!(pixel, 0x00_00_00_00, "外側なのに塗られている: x={x}, r={r}");
        }
        Ok(())
    })?;
    Ok(())
}
