use proptest::prelude::*;
use raden::{Arc, Circle, CompOp, Context, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

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

proptest! {
    /// 半径 >= 2.0 の円の中心ピクセルは完全カバレッジ (alpha == 255)。
    #[test]
    fn center_pixel_full_coverage(
        r in 2.0f64..50.0,
        img_size in 120u32..200,
    ) {
        let cx = img_size as f64 / 2.0;
        let cy = img_size as f64 / 2.0;

        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_circle(&Circle::new(cx, cy, r));
        ctx.end();

        let pixel = read_pixel(&img, cx as u32, cy as u32);
        let alpha = (pixel >> 24) & 0xFF;
        prop_assert_eq!(
            alpha, 0xFF,
            "中心の alpha が 255 でない: alpha={}, r={}",
            alpha, r
        );
    }

    /// 距離 > r + 1.0 のピクセルは完全に透明。
    #[test]
    fn outside_pixel_transparent(
        r in 5.0f64..30.0,
        angle in 0.0f64..std::f64::consts::TAU,
    ) {
        let img_size = 100u32;
        let cx = 50.0;
        let cy = 50.0;

        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_circle(&Circle::new(cx, cy, r));
        ctx.end();

        // r + 2.0 の距離のピクセルを検証 (ラスタライザの誤差を考慮して余裕を持たせる)
        let dist = r + 2.0;
        let test_px = (cx + dist * angle.cos()).floor() as i32;
        let test_py = (cy + dist * angle.sin()).floor() as i32;

        if test_px >= 0
            && test_px < img_size as i32
            && test_py >= 0
            && test_py < img_size as i32
        {
            let pixel = read_pixel(&img, test_px as u32, test_py as u32);
            prop_assert_eq!(
                pixel, 0x00_00_00_00,
                "距離 > r + 2.0 なのに塗られている: px={}, py={}, r={}",
                test_px, test_py, r
            );
        }
    }

    /// fill_all は画像全体を塗りつぶす。
    #[test]
    fn fill_all_fills_entire_image(
        img_w in 1u32..64,
        img_h in 1u32..64,
        r in 0u8..=255,
        g in 0u8..=255,
        b in 0u8..=255,
    ) {
        let mut img = Image::new(img_w, img_h, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(r, g, b, 0xFF));
        ctx.fill_all();
        ctx.end();

        let expected_prgb = Rgba32::new(r, g, b, 0xFF).to_prgb32();
        for y in 0..img_h {
            for x in 0..img_w {
                let pixel = read_pixel(&img, x, y);
                prop_assert_eq!(
                    pixel, expected_prgb,
                    "({},{}) が期待値と不一致: 0x{:08x} != 0x{:08x}",
                    x, y, pixel, expected_prgb
                );
            }
        }
    }

    /// fill_pie の弧の中間方向のピクセルは塗られる。
    #[test]
    fn fill_pie_interior_pixel(
        r in 10.0f64..40.0,
        start in 0.0f64..std::f64::consts::TAU,
    ) {
        let img_size = 100u32;
        let cx = 50.0;
        let cy = 50.0;
        let sweep = std::f64::consts::PI; // 半円

        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_pie(&Arc::new(cx, cy, r, r, start, sweep));
        ctx.end();

        // 弧の中間方向に r/2 進んだ点は確実に扇形内部
        let mid_angle = start + sweep / 2.0;
        let test_x = (cx + (r / 2.0) * mid_angle.cos()).floor() as u32;
        let test_y = (cy + (r / 2.0) * mid_angle.sin()).floor() as u32;

        if test_x < img_size && test_y < img_size {
            let pixel = read_pixel(&img, test_x, test_y);
            let alpha = (pixel >> 24) & 0xFF;
            prop_assert!(
                alpha > 0,
                "扇形内部が塗られていない: alpha={}, r={}, start={}, test=({},{})",
                alpha, r, start, test_x, test_y
            );
        }
    }

    /// fill_pie で sweep==0 なら何も塗られない。
    #[test]
    fn fill_pie_zero_sweep_no_paint(
        r in 1.0f64..40.0,
    ) {
        let img_size = 100u32;
        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_pie(&Arc::new(50.0, 50.0, r, r, 0.0, 0.0));
        ctx.end();

        // 全ピクセルが透明であることを確認
        for y in 0..img_size {
            for x in 0..img_size {
                let pixel = read_pixel(&img, x, y);
                prop_assert_eq!(
                    pixel, 0x00_00_00_00,
                    "sweep==0 なのに ({},{}) が塗られている",
                    x, y
                );
            }
        }
    }

    /// fill_all は fill_rect(画像全体) と同一結果になる。
    #[test]
    fn fill_all_equals_fill_rect_whole(
        img_w in 1u32..32,
        img_h in 1u32..32,
    ) {
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

        prop_assert_eq!(img1.data(), img2.data(), "fill_all と fill_rect(全体) の結果が異なる");
    }

    /// 中心から十分内側 (r-2) は完全カバレッジ、十分外側 (r+2) は透明。
    #[test]
    fn inner_full_outer_transparent(
        r in 5.0f64..40.0,
    ) {
        let img_size = 100u32;
        let cx = 50.0;
        let cy = 50.0;

        let mut img = Image::new(img_size, img_size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_circle(&Circle::new(cx, cy, r));
        ctx.end();

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
            prop_assert_eq!(
                alpha, 0xFF,
                "内側なのに alpha != 255: x={}, dist={}, r={}",
                x, dist, r
            );
        }

        // r+2 より外側は透明
        let outer_start = ((cx + r + 2.0).ceil() as u32).min(img_size);
        for x in outer_start..img_size {
            let pixel = read_pixel(&img, x, center_y);
            prop_assert_eq!(
                pixel, 0x00_00_00_00,
                "外側なのに塗られている: x={}, r={}",
                x, r
            );
        }
    }
}
