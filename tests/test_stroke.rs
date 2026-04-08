use raden::api::stroke::{StrokeOptions, stroke_to_fill};
use raden::{Path, PathCmd, StrokeCap, StrokeJoin};

/// 水平線分 + Butt キャップの輪郭が正確に 4 点の矩形になる。
#[test]
fn stroke_line_point_count() {
    let mut input = Path::new();
    input.move_to(10.0, 50.0);
    input.line_to(90.0, 50.0);

    let options = StrokeOptions {
        width: 4.0,
        start_cap: StrokeCap::Butt,
        end_cap: StrokeCap::Butt,
        join: StrokeJoin::Bevel,
        miter_limit: 4.0,
        ..Default::default()
    };

    let mut output = Path::new();
    stroke_to_fill(&input, &options, &mut output);

    // Butt キャップ + 1 本の線分: MoveTo + 3 LineTo + Close = 4 点の矩形
    let cmds = output.cmds();
    assert_eq!(cmds[0], PathCmd::MoveTo);

    // LineTo の数を数える (MoveTo と Close を除く)
    let line_count = cmds.iter().filter(|&&c| c == PathCmd::LineTo).count();
    assert_eq!(
        line_count, 3,
        "Butt キャップの水平線分は 4 点 (3 LineTo) の矩形になるべき"
    );

    // 点の y 座標が width/2 = 2.0 だけオフセットしていることを確認する
    let pts = output.points();
    let half_width = 2.0;
    let eps = 1e-10;

    // 最初の点は (10, 48) または (10, 52) のいずれか
    let first = pts[0];
    assert!(
        (first.y - (50.0 - half_width)).abs() < eps || (first.y - (50.0 + half_width)).abs() < eps,
        "最初の点の y={} は 48 または 52 であるべき",
        first.y
    );
}

/// Square キャップが width/2 だけ延長されている。
#[test]
fn stroke_square_cap_extends() {
    let mut input = Path::new();
    input.move_to(20.0, 50.0);
    input.line_to(80.0, 50.0);

    let width = 6.0;
    let half_width = width / 2.0;
    let options = StrokeOptions {
        width,
        start_cap: StrokeCap::Square,
        end_cap: StrokeCap::Square,
        join: StrokeJoin::Bevel,
        miter_limit: 4.0,
        ..Default::default()
    };

    let mut output = Path::new();
    stroke_to_fill(&input, &options, &mut output);

    // Square キャップでは、端点から接線方向に half_width だけ延長される
    // 水平線分 (20,50)→(80,50) の場合:
    // 始点側: x = 20 - 3 = 17
    // 終点側: x = 80 + 3 = 83
    let pts = output.points();
    let min_x = pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let max_x = pts.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);

    let eps = 1e-10;
    assert!(
        (min_x - (20.0 - half_width)).abs() < eps,
        "最小 x={} は {} であるべき",
        min_x,
        20.0 - half_width
    );
    assert!(
        (max_x - (80.0 + half_width)).abs() < eps,
        "最大 x={} は {} であるべき",
        max_x,
        80.0 + half_width
    );
}

/// Round キャップが CubicTo を含む。
#[test]
fn stroke_round_cap_has_curves() {
    let mut input = Path::new();
    input.move_to(10.0, 50.0);
    input.line_to(90.0, 50.0);

    let options = StrokeOptions {
        width: 4.0,
        start_cap: StrokeCap::Round,
        end_cap: StrokeCap::Round,
        join: StrokeJoin::Bevel,
        miter_limit: 4.0,
        ..Default::default()
    };

    let mut output = Path::new();
    stroke_to_fill(&input, &options, &mut output);

    let cubic_count = output
        .cmds()
        .iter()
        .filter(|&&c| c == PathCmd::CubicTo)
        .count();

    // Round キャップは半円を 2 つの cubic bezier で近似する × 2 キャップ = 4 以上の CubicTo
    assert!(
        cubic_count >= 4,
        "Round キャップは 4 つ以上の CubicTo を含むべき: {} 個",
        cubic_count
    );
}

// ストロークスタイルにグラデーション/パターンを設定したときの描画テスト
mod stroke_style_gradient_pattern {
    use raden::{
        CompOp, Context, ExtendMode, Gradient, Image, Matrix2D, Path, Pattern, PatternFilter,
        PipelineRuntime, PixelFormat, Rgba32,
    };

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

    /// ストロークにリニアグラデーションを適用すると、線に沿って色が変化する。
    #[test]
    fn stroke_linear_gradient_varies_along_line() {
        let mut img = Image::new(64, 16, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);

        let mut grad = Gradient::new_linear(0.0, 0.0, 64.0, 0.0);
        grad.add_stop(0.0, Rgba32::rgb(0xFF, 0x00, 0x00));
        grad.add_stop(1.0, Rgba32::rgb(0x00, 0x00, 0xFF));
        ctx.set_stroke_style_gradient(&grad);
        ctx.set_stroke_width(8.0);

        let mut path = Path::new();
        path.move_to(0.0, 8.0);
        path.line_to(64.0, 8.0);
        ctx.stroke_path(&path);
        ctx.end();

        // 左端は赤寄り、右端は青寄り
        let left = read_pixel(&img, 4, 8);
        let right = read_pixel(&img, 60, 8);
        let left_r = (left >> 16) & 0xFF;
        let left_b = left & 0xFF;
        let right_r = (right >> 16) & 0xFF;
        let right_b = right & 0xFF;
        assert!(left_r > left_b, "left should be red-ish: {:08x}", left);
        assert!(right_b > right_r, "right should be blue-ish: {:08x}", right);
    }

    /// set_stroke_style(color) はグラデーション/パターンをクリアする。
    #[test]
    fn set_stroke_style_clears_gradient_and_pattern() {
        let mut img = Image::new(4, 4, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);

        let mut grad = Gradient::new_linear(0.0, 0.0, 4.0, 0.0);
        grad.add_stop(0.0, Rgba32::rgb(0xFF, 0x00, 0x00));
        grad.add_stop(1.0, Rgba32::rgb(0x00, 0x00, 0xFF));
        ctx.set_stroke_style_gradient(&grad);
        assert!(ctx.stroke_gradient().is_some());

        ctx.set_stroke_style(Rgba32::new(0x10, 0x20, 0x30, 0xFF));
        assert!(ctx.stroke_gradient().is_none());
        assert!(ctx.stroke_pattern().is_none());
    }

    /// save / restore はストロークのグラデーション/パターンも含めて復元する。
    #[test]
    fn save_restore_preserves_stroke_gradient() {
        let mut img = Image::new(4, 4, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);

        let mut grad = Gradient::new_linear(0.0, 0.0, 4.0, 0.0);
        grad.add_stop(0.0, Rgba32::rgb(0xFF, 0x00, 0x00));
        grad.add_stop(1.0, Rgba32::rgb(0x00, 0xFF, 0x00));
        ctx.set_stroke_style_gradient(&grad);

        ctx.save();
        ctx.set_stroke_style(Rgba32::new(0, 0, 0, 0xFF));
        assert!(ctx.stroke_gradient().is_none());
        ctx.restore();
        assert!(ctx.stroke_gradient().is_some());
    }

    /// ストロークパターンは線の領域にテクスチャを貼り付ける。
    #[test]
    fn stroke_pattern_applies_texture() {
        // 2x1 の赤・青タイル
        let mut tile = vec![0u8; 8];
        tile[0..4].copy_from_slice(&0xFF_FF_00_00u32.to_le_bytes());
        tile[4..8].copy_from_slice(&0xFF_00_00_FFu32.to_le_bytes());

        let mut pat = Pattern::new(&tile, 2, 1, 8);
        pat.set_filter(PatternFilter::Nearest);
        pat.set_extend_mode(ExtendMode::Repeat);
        pat.set_transform(Matrix2D::IDENTITY);

        let mut img = Image::new(8, 8, PixelFormat::Prgb32);
        let mut runtime = PipelineRuntime::new();
        let mut ctx = Context::new(&mut img, &mut runtime);
        ctx.set_comp_op(CompOp::SrcCopy);
        ctx.set_stroke_style_pattern(&pat);
        ctx.set_stroke_width(4.0);

        let mut path = Path::new();
        path.move_to(0.0, 4.0);
        path.line_to(8.0, 4.0);
        ctx.stroke_path(&path);
        ctx.end();

        // 線上の偶数 x は赤、奇数 x は青 (Repeat)
        assert_eq!(read_pixel(&img, 0, 4), 0xFF_FF_00_00);
        assert_eq!(read_pixel(&img, 1, 4), 0xFF_00_00_FF);
        assert_eq!(read_pixel(&img, 2, 4), 0xFF_FF_00_00);
        assert_eq!(read_pixel(&img, 3, 4), 0xFF_00_00_FF);
    }
}
