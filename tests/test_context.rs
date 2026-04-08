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
