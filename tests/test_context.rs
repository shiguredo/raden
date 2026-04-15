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

mod round_rect {
    use raden::{
        CompOp, Context, Image, Path, PipelineRuntime, PixelFormat, Rect, Rgba32, RoundRect,
    };

    fn render_with<F: FnOnce(&mut Context)>(f: F) -> Image {
        let mut img = Image::new(32, 32, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        f(&mut ctx);
        ctx.end();
        img
    }

    fn read(img: &Image, x: u32, y: u32) -> u8 {
        let off = y as usize * img.stride() + x as usize * 4;
        img.data()[off + 3]
    }

    /// rx==ry==0 のとき通常の矩形と一致する。
    #[test]
    fn zero_radius_matches_rect() {
        let r = render_with(|ctx| {
            ctx.fill_rect(&Rect::new(4.0, 4.0, 24.0, 24.0));
        });
        let rr = render_with(|ctx| {
            ctx.fill_round_rect(&RoundRect::new(4.0, 4.0, 24.0, 24.0, 0.0, 0.0));
        });
        assert_eq!(r.data(), rr.data());
    }

    /// 半径を幅/高さの半分以上に指定するとクランプされ、楕円相当 (角がすべて丸くなる) になる。
    #[test]
    fn radius_clamps_to_half_extent() {
        let img = render_with(|ctx| {
            ctx.fill_round_rect(&RoundRect::new(0.0, 0.0, 32.0, 32.0, 100.0, 100.0));
        });
        // 角はクリップされて透明
        assert_eq!(read(&img, 0, 0), 0);
        assert_eq!(read(&img, 31, 0), 0);
        assert_eq!(read(&img, 0, 31), 0);
        assert_eq!(read(&img, 31, 31), 0);
        // 中央は塗りつぶし
        assert_eq!(read(&img, 16, 16), 0xFF);
    }

    /// 角丸矩形は 4 隅とも丸まっている (端点ピクセルが透明)。
    #[test]
    fn corners_are_rounded() {
        let img = render_with(|ctx| {
            ctx.fill_round_rect(&RoundRect::new(2.0, 2.0, 28.0, 28.0, 8.0, 8.0));
        });
        // 4 隅は透明
        assert_eq!(read(&img, 2, 2), 0);
        assert_eq!(read(&img, 29, 2), 0);
        assert_eq!(read(&img, 2, 29), 0);
        assert_eq!(read(&img, 29, 29), 0);
        // 辺の中点は塗りつぶし
        assert_eq!(read(&img, 16, 2), 0xFF);
        assert_eq!(read(&img, 2, 16), 0xFF);
    }

    /// add_round_rect(rx==0) は矩形と同じコマンド数 (move + 3 line + close)。
    #[test]
    fn add_round_rect_zero_radius_command_count() {
        let mut p = Path::new();
        p.add_round_rect(0.0, 0.0, 10.0, 10.0, 0.0, 0.0);
        assert_eq!(p.cmds().len(), 5);
    }
}

mod triangle_polygon {
    use raden::{
        CompOp, Context, Image, Path, PipelineRuntime, PixelFormat, Point, Rgba32, Triangle,
    };

    fn read(img: &Image, x: u32, y: u32) -> u8 {
        let off = y as usize * img.stride() + x as usize * 4;
        img.data()[off + 3]
    }

    /// fill_triangle と fill_polygon([3 点]) のラスタ結果が一致する。
    #[test]
    fn triangle_matches_3_point_polygon() {
        let make_image = || Image::new(32, 32, PixelFormat::Prgb32);

        let mut img1 = make_image();
        let mut runtime1 = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img1, &mut runtime1);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.fill_triangle(&Triangle::new(4.0, 4.0, 28.0, 8.0, 16.0, 28.0));
        ctx.end();

        let mut img2 = make_image();
        let mut runtime2 = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img2, &mut runtime2);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.fill_polygon(&[
            Point::new(4.0, 4.0),
            Point::new(28.0, 8.0),
            Point::new(16.0, 28.0),
        ]);
        ctx.end();

        assert_eq!(img1.data(), img2.data());
    }

    /// add_polygon の点が 3 点未満なら何も追加されない。
    #[test]
    fn add_polygon_too_few_points_noop() {
        let mut p = Path::new();
        p.add_polygon(&[Point::new(0.0, 0.0), Point::new(1.0, 1.0)]);
        assert_eq!(p.cmds().len(), 0);
    }

    /// 三角形の重心は塗りつぶされる。
    #[test]
    fn triangle_centroid_filled() {
        let mut img = Image::new(32, 32, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.fill_triangle(&Triangle::new(4.0, 4.0, 28.0, 8.0, 16.0, 28.0));
        ctx.end();

        // 重心 ((4+28+16)/3, (4+8+28)/3) ≈ (16, 13)
        assert_eq!(read(&img, 16, 13), 0xFF);
    }
}

mod alpha {
    use raden::{CompOp, Context, Gradient, Image, PipelineRuntime, PixelFormat, Rect, Rgba32};

    fn read(img: &Image, x: u32, y: u32) -> u32 {
        let off = y as usize * img.stride() + x as usize * 4;
        let d = img.data();
        u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
    }

    /// global_alpha = 1.0 + fill_alpha = 1.0 では出力が変化しない (回帰テスト)。
    #[test]
    fn unit_alpha_matches_no_alpha() {
        let render = |use_alpha: bool| {
            let mut img = Image::new(4, 4, PixelFormat::Prgb32);
            let mut runtime = PipelineRuntime::new();
            let mut ctx = Context::new(&mut img, &mut runtime);
            ctx.set_comp_op(CompOp::SrcCopy);
            if use_alpha {
                ctx.set_global_alpha(1.0);
                ctx.set_fill_alpha(1.0);
            }
            ctx.set_fill_style(Rgba32::new(0xC0, 0x40, 0x80, 0xFF));
            ctx.fill_rect(&Rect::new(0.0, 0.0, 4.0, 4.0));
            ctx.end();
            img
        };
        assert_eq!(render(true).data(), render(false).data());
    }

    /// global_alpha = 0.0 では描画が起きない (キャンバスは透明のまま)。
    #[test]
    fn zero_global_alpha_skips_draw() {
        let mut img = Image::new(4, 4, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_global_alpha(0.0);
        ctx.set_fill_style(Rgba32::new(0xFF, 0x00, 0x00, 0xFF));
        ctx.fill_rect(&Rect::new(0.0, 0.0, 4.0, 4.0));
        ctx.end();
        for y in 0..4 {
            for x in 0..4 {
                assert_eq!(read(&img, x, y), 0);
            }
        }
    }

    /// fill_alpha = 0.5 で SrcCopy 描画した結果は半透明 (アルファ ≈ 128)。
    #[test]
    fn half_fill_alpha_solid_src_copy() {
        let mut img = Image::new(2, 2, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_alpha(0.5);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.fill_rect(&Rect::new(0.0, 0.0, 2.0, 2.0));
        ctx.end();

        let p = read(&img, 0, 0);
        let a = (p >> 24) & 0xFF;
        let r = (p >> 16) & 0xFF;
        // alpha は約 128、premultiplied で R チャネルも 128
        assert!((120..=132).contains(&a), "alpha was {}", a);
        assert!((120..=132).contains(&r), "red was {}", r);
    }

    /// global_alpha は fill_alpha と乗算される。
    #[test]
    fn global_and_fill_alpha_multiply() {
        let mut img = Image::new(2, 2, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_global_alpha(0.5);
        ctx.set_fill_alpha(0.5);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.fill_rect(&Rect::new(0.0, 0.0, 2.0, 2.0));
        ctx.end();

        let a = (read(&img, 0, 0) >> 24) & 0xFF;
        // 0.5 * 0.5 = 0.25 → 約 64
        assert!((58..=70).contains(&a), "alpha was {}", a);
    }

    /// グラデーションにも fill_alpha が適用される (左端の不透明度が下がる)。
    #[test]
    fn alpha_applies_to_gradient() {
        let mut img = Image::new(8, 4, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_alpha(0.5);
        let mut grad = Gradient::new_linear(0.0, 0.0, 8.0, 0.0);
        grad.add_stop(0.0, Rgba32::rgb(0xFF, 0xFF, 0xFF));
        grad.add_stop(1.0, Rgba32::rgb(0xFF, 0xFF, 0xFF));
        ctx.set_fill_style_gradient(&grad);
        ctx.fill_rect(&Rect::new(0.0, 0.0, 8.0, 4.0));
        ctx.end();

        let a = (read(&img, 4, 2) >> 24) & 0xFF;
        assert!((120..=132).contains(&a), "alpha was {}", a);
    }

    /// stroke_alpha は stroke 経路にだけ適用される (fill には影響しない)。
    #[test]
    fn stroke_alpha_independent_from_fill() {
        let mut img = Image::new(8, 8, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_fill_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.set_stroke_style(Rgba32::new(0xFF, 0xFF, 0xFF, 0xFF));
        ctx.set_stroke_alpha(0.5);
        ctx.fill_rect(&Rect::new(0.0, 0.0, 8.0, 1.0));
        ctx.set_stroke_width(1.0);
        let mut path = raden::Path::new();
        path.move_to(0.0, 4.5);
        path.line_to(8.0, 4.5);
        ctx.stroke_path(&path);
        ctx.end();

        // 上段 (fill) は不透明
        assert_eq!((read(&img, 0, 0) >> 24) & 0xFF, 0xFF);
        // 中段 (stroke) はおよそ 128
        let a = (read(&img, 4, 4) >> 24) & 0xFF;
        assert!((120..=132).contains(&a), "stroke alpha was {}", a);
    }

    /// save / restore で alpha 状態が復元される。
    #[test]
    fn save_restore_preserves_alpha() {
        let mut img = Image::new(2, 2, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_global_alpha(0.7);
        ctx.set_fill_alpha(0.3);
        ctx.set_stroke_alpha(0.4);
        ctx.save();
        ctx.set_global_alpha(1.0);
        ctx.set_fill_alpha(1.0);
        ctx.set_stroke_alpha(1.0);
        ctx.restore();
        assert!((ctx.global_alpha() - 0.7).abs() < 1e-12);
        assert!((ctx.fill_alpha() - 0.3).abs() < 1e-12);
        assert!((ctx.stroke_alpha() - 0.4).abs() < 1e-12);
    }

    /// 範囲外の値はクランプされる。
    #[test]
    fn alpha_clamps_to_unit_range() {
        let mut img = Image::new(2, 2, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_global_alpha(2.0);
        ctx.set_fill_alpha(-1.0);
        assert_eq!(ctx.global_alpha(), 1.0);
        assert_eq!(ctx.fill_alpha(), 0.0);
    }
}
