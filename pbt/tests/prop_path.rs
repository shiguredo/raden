use proptest::prelude::*;
use raden::{Path, PathCmd};

proptest! {
    /// clear 後は常に空。
    #[test]
    fn clear_then_empty(
        cx in -100.0f64..100.0,
        cy in -100.0f64..100.0,
        r in 0.1f64..100.0,
    ) {
        let mut path = Path::new();
        path.add_circle(cx, cy, r);
        prop_assert!(!path.is_empty());

        path.clear();
        prop_assert!(path.is_empty());
        prop_assert_eq!(path.len(), 0);
        prop_assert_eq!(path.points().len(), 0);
    }

    /// add_circle は常に 6 コマンド、13 点を生成する。
    #[test]
    fn add_circle_invariant(
        cx in -1000.0f64..1000.0,
        cy in -1000.0f64..1000.0,
        r in 0.001f64..1000.0,
    ) {
        let mut path = Path::new();
        path.add_circle(cx, cy, r);

        prop_assert_eq!(path.len(), 6);
        prop_assert_eq!(path.points().len(), 13);

        // コマンド列の構造は不変
        prop_assert_eq!(path.cmds()[0], PathCmd::MoveTo);
        prop_assert_eq!(path.cmds()[1], PathCmd::CubicTo);
        prop_assert_eq!(path.cmds()[2], PathCmd::CubicTo);
        prop_assert_eq!(path.cmds()[3], PathCmd::CubicTo);
        prop_assert_eq!(path.cmds()[4], PathCmd::CubicTo);
        prop_assert_eq!(path.cmds()[5], PathCmd::Close);
    }

    /// add_circle の開始点と最終 CubicTo の終点は一致する (閉じたパス)。
    #[test]
    fn add_circle_closed(
        cx in -100.0f64..100.0,
        cy in -100.0f64..100.0,
        r in 0.01f64..100.0,
    ) {
        let mut path = Path::new();
        path.add_circle(cx, cy, r);

        let pts = path.points();
        let start = pts[0];
        let end = pts[12]; // 最後の CubicTo の終点
        let eps = 1e-10;
        prop_assert!((start.x - end.x).abs() < eps, "x: start={}, end={}", start.x, end.x);
        prop_assert!((start.y - end.y).abs() < eps, "y: start={}, end={}", start.y, end.y);
    }

    /// add_circle の全点はバウンディングボックス内に収まる。
    #[test]
    fn add_circle_bounding_box(
        cx in -100.0f64..100.0,
        cy in -100.0f64..100.0,
        r in 0.01f64..100.0,
    ) {
        let mut path = Path::new();
        path.add_circle(cx, cy, r);

        for p in path.points() {
            prop_assert!(
                p.x >= cx - r - 1e-10 && p.x <= cx + r + 1e-10,
                "x={} outside [{}, {}]", p.x, cx - r, cx + r
            );
            prop_assert!(
                p.y >= cy - r - 1e-10 && p.y <= cy + r + 1e-10,
                "y={} outside [{}, {}]", p.y, cy - r, cy + r
            );
        }
    }
}
