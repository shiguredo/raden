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
