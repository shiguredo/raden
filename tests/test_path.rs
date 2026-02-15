use raden::{Path, PathCmd, Point};

#[test]
fn add_circle_cmd_count() {
    let mut path = Path::new();
    path.add_circle(50.0, 50.0, 10.0);

    // MoveTo + CubicTo x4 + Close = 6 コマンド
    assert_eq!(path.len(), 6);
    assert_eq!(path.cmds()[0], PathCmd::MoveTo);
    assert_eq!(path.cmds()[1], PathCmd::CubicTo);
    assert_eq!(path.cmds()[2], PathCmd::CubicTo);
    assert_eq!(path.cmds()[3], PathCmd::CubicTo);
    assert_eq!(path.cmds()[4], PathCmd::CubicTo);
    assert_eq!(path.cmds()[5], PathCmd::Close);
}

#[test]
fn add_circle_point_count() {
    let mut path = Path::new();
    path.add_circle(50.0, 50.0, 10.0);

    // MoveTo: 1 点 + CubicTo x4: 3 点 x4 = 12 点 → 計 13 点
    assert_eq!(path.points().len(), 13);
}

#[test]
fn add_circle_cardinal_points() {
    let cx = 50.0;
    let cy = 50.0;
    let r = 10.0;
    let mut path = Path::new();
    path.add_circle(cx, cy, r);

    let pts = path.points();
    let eps = 1e-10;

    // 開始点: 右端 (cx+r, cy)
    assert!((pts[0].x - (cx + r)).abs() < eps);
    assert!((pts[0].y - cy).abs() < eps);

    // 最初の CubicTo の終点 (index 3): 下端 (cx, cy+r)
    assert!((pts[3].x - cx).abs() < eps);
    assert!((pts[3].y - (cy + r)).abs() < eps);

    // 2 番目の CubicTo の終点 (index 6): 左端 (cx-r, cy)
    assert!((pts[6].x - (cx - r)).abs() < eps);
    assert!((pts[6].y - cy).abs() < eps);

    // 3 番目の CubicTo の終点 (index 9): 上端 (cx, cy-r)
    assert!((pts[9].x - cx).abs() < eps);
    assert!((pts[9].y - (cy - r)).abs() < eps);

    // 4 番目の CubicTo の終点 (index 12): 右端に戻る (cx+r, cy)
    assert!((pts[12].x - (cx + r)).abs() < eps);
    assert!((pts[12].y - cy).abs() < eps);
}

#[test]
fn add_circle_kappa_control_points() {
    let cx = 0.0;
    let cy = 0.0;
    let r = 1.0;
    let kappa = 0.552_284_749_831;
    let mut path = Path::new();
    path.add_circle(cx, cy, r);

    let pts = path.points();
    let eps = 1e-10;

    // 最初の CubicTo: 右端→下端
    // cp1 = (r, r*kappa), cp2 = (r*kappa, r)
    assert!((pts[1].x - r).abs() < eps);
    assert!((pts[1].y - (r * kappa)).abs() < eps);
    assert!((pts[2].x - (r * kappa)).abs() < eps);
    assert!((pts[2].y - r).abs() < eps);
}

#[test]
fn clear_and_is_empty() {
    let mut path = Path::new();
    assert!(path.is_empty());

    path.add_circle(0.0, 0.0, 10.0);
    assert!(!path.is_empty());

    path.clear();
    assert!(path.is_empty());
    assert_eq!(path.len(), 0);
    assert_eq!(path.points().len(), 0);
}

#[test]
fn manual_path_construction() {
    let mut path = Path::new();
    path.move_to(0.0, 0.0);
    path.line_to(10.0, 0.0);
    path.line_to(10.0, 10.0);
    path.close();

    assert_eq!(path.len(), 4);
    assert_eq!(path.cmds()[0], PathCmd::MoveTo);
    assert_eq!(path.cmds()[1], PathCmd::LineTo);
    assert_eq!(path.cmds()[2], PathCmd::LineTo);
    assert_eq!(path.cmds()[3], PathCmd::Close);

    assert_eq!(path.points().len(), 3);
    assert_eq!(path.points()[0], Point::new(0.0, 0.0));
    assert_eq!(path.points()[1], Point::new(10.0, 0.0));
    assert_eq!(path.points()[2], Point::new(10.0, 10.0));
}
