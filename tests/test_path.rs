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

mod transform_merge_bbox {
    use raden::{Matrix2D, Path};

    fn make_square() -> Path {
        let mut p = Path::new();
        p.move_to(0.0, 0.0);
        p.line_to(2.0, 0.0);
        p.line_to(2.0, 2.0);
        p.line_to(0.0, 2.0);
        p.close();
        p
    }

    #[test]
    fn translate_shifts_all_points() {
        let mut p = make_square();
        p.translate(3.0, 4.0);
        let pts = p.points();
        assert!((pts[0].x - 3.0).abs() < 1e-12 && (pts[0].y - 4.0).abs() < 1e-12);
        assert!((pts[2].x - 5.0).abs() < 1e-12 && (pts[2].y - 6.0).abs() < 1e-12);
    }

    #[test]
    fn transform_identity_is_noop() {
        let mut p = make_square();
        let original: Vec<_> = p.points().to_vec();
        p.transform(&Matrix2D::IDENTITY);
        let after: Vec<_> = p.points().to_vec();
        assert_eq!(original, after);
    }

    #[test]
    fn transform_scale_doubles_extent() {
        let mut p = make_square();
        p.transform(&Matrix2D::scaling(2.0, 2.0));
        let bbox = p.bounding_box().unwrap();
        assert!((bbox.w - 4.0).abs() < 1e-12);
        assert!((bbox.h - 4.0).abs() < 1e-12);
    }

    #[test]
    fn add_path_appends_commands() {
        let a = make_square();
        let b = make_square();
        let mut combined = Path::new();
        combined.add_path(&a);
        combined.add_path(&b);
        assert_eq!(combined.cmds().len(), a.cmds().len() + b.cmds().len());
    }

    #[test]
    fn add_path_translated_shifts_appended_points() {
        let a = make_square();
        let mut combined = Path::new();
        combined.add_path_translated(&a, 10.0, 20.0);
        let pts = combined.points();
        assert!((pts[0].x - 10.0).abs() < 1e-12 && (pts[0].y - 20.0).abs() < 1e-12);
        assert!((pts[2].x - 12.0).abs() < 1e-12 && (pts[2].y - 22.0).abs() < 1e-12);
    }

    #[test]
    fn add_path_transformed_applies_matrix() {
        let a = make_square();
        let mut combined = Path::new();
        combined.add_path_transformed(&a, &Matrix2D::translation(5.0, 6.0));
        let pts = combined.points();
        assert!((pts[0].x - 5.0).abs() < 1e-12 && (pts[0].y - 6.0).abs() < 1e-12);
    }

    #[test]
    fn empty_path_has_no_bbox() {
        let p = Path::new();
        assert!(p.bounding_box().is_none());
    }

    #[test]
    fn bounding_box_matches_extent() {
        let p = make_square();
        let bbox = p.bounding_box().unwrap();
        assert!((bbox.x - 0.0).abs() < 1e-12);
        assert!((bbox.y - 0.0).abs() < 1e-12);
        assert!((bbox.w - 2.0).abs() < 1e-12);
        assert!((bbox.h - 2.0).abs() < 1e-12);
    }

    #[test]
    fn translate_then_bounding_box_shifts() {
        let mut p = make_square();
        p.translate(3.0, 4.0);
        let bbox = p.bounding_box().unwrap();
        assert!((bbox.x - 3.0).abs() < 1e-12);
        assert!((bbox.y - 4.0).abs() < 1e-12);
    }

    /// コニックの重みは行列適用で不変。
    #[test]
    fn transform_preserves_conic_weights() {
        let mut p = Path::new();
        p.move_to(0.0, 0.0);
        p.conic_to(1.0, 0.0, 1.0, 1.0, 0.7);
        let weights_before: Vec<_> = p.conic_weights().to_vec();
        p.transform(&Matrix2D::scaling(3.0, 3.0));
        let weights_after: Vec<_> = p.conic_weights().to_vec();
        assert_eq!(weights_before, weights_after);
    }
}
