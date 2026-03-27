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
