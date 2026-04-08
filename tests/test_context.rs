use raden::{
    CompOp, Context, FillRule, Image, Matrix2D, PipelineRuntime, PixelFormat, StrokeCap, StrokeJoin,
};

#[test]
fn context_getters_match_setters() {
    let mut img = Image::new(32, 32, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);

    ctx.set_comp_op(CompOp::Xor);
    assert_eq!(ctx.comp_op(), CompOp::Xor);

    ctx.set_fill_rule(FillRule::EvenOdd);
    assert_eq!(ctx.fill_rule(), FillRule::EvenOdd);

    ctx.set_stroke_width(3.5);
    assert_eq!(ctx.stroke_width(), 3.5);

    ctx.set_stroke_miter_limit(2.0);
    assert_eq!(ctx.stroke_miter_limit(), 2.0);

    ctx.set_stroke_join(StrokeJoin::Round);
    assert_eq!(ctx.stroke_join(), StrokeJoin::Round);

    ctx.set_stroke_cap(StrokeCap::Round);
    assert_eq!(ctx.stroke_start_cap(), StrokeCap::Round);
    assert_eq!(ctx.stroke_end_cap(), StrokeCap::Round);

    ctx.set_stroke_dash_array(&[4.0, 2.0]);
    assert_eq!(ctx.stroke_dash_array(), [4.0, 2.0_f64]);

    ctx.set_stroke_dash_offset(1.25);
    assert_eq!(ctx.stroke_dash_offset(), 1.25);

    assert!(ctx.fill_gradient().is_none());
    assert!(ctx.fill_pattern().is_none());
}

#[test]
fn context_matrix_delegates_skew() {
    let mut img = Image::new(8, 8, PixelFormat::Prgb32);
    let mut runtime = PipelineRuntime::new();
    let mut ctx = Context::new(&mut img, &mut runtime);

    ctx.skew(0.5, 0.25);
    let mut m = Matrix2D::IDENTITY;
    m.skew(0.5, 0.25);
    assert_eq!(*ctx.matrix(), m);
}

mod clear {
    use raden::{Context, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

    fn read(img: &Image, x: u32, y: u32) -> u32 {
        let off = y as usize * img.stride() + x as usize * 4;
        let d = img.data();
        u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
    }

    /// `clear_all` はキャンバス全域を 0 にする。
    #[test]
    fn clear_all_zeros_canvas() {
        let mut img = Image::new(4, 4, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_all();
        ctx.clear_all();
        ctx.end();
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(read(&img, x, y), 0);
            }
        }
    }

    /// `clear_rect` は指定矩形のみを 0 にする。
    #[test]
    fn clear_rect_only_clears_target() {
        let mut img = Image::new(4, 4, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_all();
        ctx.clear_rect(&Rect::new(1.0, 1.0, 2.0, 2.0));
        ctx.end();
        // 矩形内はゼロ
        assert_eq!(read(&img, 1, 1), 0);
        assert_eq!(read(&img, 2, 2), 0);
        // 矩形外は赤のまま
        assert_eq!(read(&img, 0, 0), 0xFF_FF_00_00);
        assert_eq!(read(&img, 3, 3), 0xFF_FF_00_00);
    }

    /// `clear_rect` はクリップ領域外を変更しない。
    #[test]
    fn clear_rect_respects_clip() {
        let mut img = Image::new(4, 4, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_all();
        ctx.clip_to_rect(&Rect::new(0.0, 0.0, 2.0, 2.0));
        ctx.clear_rect(&Rect::new(0.0, 0.0, 4.0, 4.0));
        ctx.end();
        // クリップ内はクリア
        assert_eq!(read(&img, 0, 0), 0);
        assert_eq!(read(&img, 1, 1), 0);
        // クリップ外は保持
        assert_eq!(read(&img, 2, 2), 0xFF_FF_00_00);
        assert_eq!(read(&img, 3, 3), 0xFF_FF_00_00);
    }

    /// `clear_*` は comp_op を変更しない。
    #[test]
    fn clear_does_not_mutate_comp_op() {
        let mut img = Image::new(2, 2, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        let before = ctx.comp_op();
        ctx.clear_all();
        ctx.clear_rect(&Rect::new(0.0, 0.0, 1.0, 1.0));
        assert_eq!(ctx.comp_op(), before);
    }
}

mod ellipse {
    use raden::{
        Circle, CompOp, Context, Ellipse, Image, Path, PipelineRuntime, PixelFormat, Rgba32,
    };

    fn render_circle(r: f64) -> Image {
        let size = 64u32;
        let mut img = Image::new(size, size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.fill_circle(&Circle::new(32.0, 32.0, r));
        ctx.end();
        img
    }

    fn render_ellipse(rx: f64, ry: f64) -> Image {
        let size = 64u32;
        let mut img = Image::new(size, size, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.fill_ellipse(&Ellipse::new(32.0, 32.0, rx, ry));
        ctx.end();
        img
    }

    /// fill_ellipse(rx==ry) は fill_circle と一致する。
    #[test]
    fn ellipse_with_equal_radii_matches_circle() {
        let circle_img = render_circle(20.0);
        let ellipse_img = render_ellipse(20.0, 20.0);
        assert_eq!(circle_img.data(), ellipse_img.data());
    }

    /// 横長の楕円は水平軸方向に広がる。
    #[test]
    fn wide_ellipse_extends_horizontally() {
        let img = render_ellipse(28.0, 10.0);
        // 中心行で左右端付近のピクセルがオン
        let read = |x: u32, y: u32| {
            let off = y as usize * img.stride() + x as usize * 4;
            img.data()[off + 3]
        };
        assert!(read(6, 32) > 0, "left edge should be filled");
        assert!(read(58, 32) > 0, "right edge should be filled");
        // 上下端のピクセルは未塗りつぶし (ry=10 なので y=4 は範囲外)
        assert_eq!(read(32, 4), 0);
        assert_eq!(read(32, 60), 0);
    }

    /// add_ellipse(rx==ry) は add_circle と同じコマンド列を生成する。
    #[test]
    fn add_ellipse_equal_radii_matches_add_circle() {
        let mut p1 = Path::new();
        p1.add_circle(10.0, 20.0, 5.0);
        let mut p2 = Path::new();
        p2.add_ellipse(10.0, 20.0, 5.0, 5.0);
        assert_eq!(p1.cmds(), p2.cmds());
        assert_eq!(p1.points().len(), p2.points().len());
        for (a, b) in p1.points().iter().zip(p2.points()) {
            assert!((a.x - b.x).abs() < 1e-12);
            assert!((a.y - b.y).abs() < 1e-12);
        }
    }
}
