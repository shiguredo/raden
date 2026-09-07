use raden::{Path, PathCmd};

/// 1 テストあたりのケース数。
const CASES: usize = 256;

/// シード再現用の環境変数名。
const SEED_ENV: &str = "RADEN_PBT_SEED";

/// clear 後は常に空。
#[test]
fn clear_then_empty() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let cx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let cy = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let r = noprop::sample_f64_in(ctx, 0.1, 100.0);

        let mut path = Path::new();
        path.add_circle(cx, cy, r);
        assert!(!path.is_empty());

        path.clear();
        assert!(path.is_empty());
        assert_eq!(path.len(), 0);
        assert_eq!(path.points().len(), 0);
        Ok(())
    })?;
    Ok(())
}

/// add_circle は常に 6 コマンド、13 点を生成する。
#[test]
fn add_circle_invariant() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let cx = noprop::sample_f64_in(ctx, -1000.0, 1000.0);
        let cy = noprop::sample_f64_in(ctx, -1000.0, 1000.0);
        let r = noprop::sample_f64_in(ctx, 0.001, 1000.0);

        let mut path = Path::new();
        path.add_circle(cx, cy, r);

        assert_eq!(path.len(), 6);
        assert_eq!(path.points().len(), 13);

        // コマンド列の構造は不変
        assert_eq!(path.cmds()[0], PathCmd::MoveTo);
        assert_eq!(path.cmds()[1], PathCmd::CubicTo);
        assert_eq!(path.cmds()[2], PathCmd::CubicTo);
        assert_eq!(path.cmds()[3], PathCmd::CubicTo);
        assert_eq!(path.cmds()[4], PathCmd::CubicTo);
        assert_eq!(path.cmds()[5], PathCmd::Close);
        Ok(())
    })?;
    Ok(())
}

/// add_circle の開始点と最終 CubicTo の終点は一致する (閉じたパス)。
#[test]
fn add_circle_closed() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let cx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let cy = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let r = noprop::sample_f64_in(ctx, 0.01, 100.0);

        let mut path = Path::new();
        path.add_circle(cx, cy, r);

        let pts = path.points();
        let start = pts[0];
        let end = pts[12]; // 最後の CubicTo の終点
        let eps = 1e-10;
        assert!(
            (start.x - end.x).abs() < eps,
            "x: start={}, end={}",
            start.x,
            end.x
        );
        assert!(
            (start.y - end.y).abs() < eps,
            "y: start={}, end={}",
            start.y,
            end.y
        );
        Ok(())
    })?;
    Ok(())
}

/// add_circle の全点はバウンディングボックス内に収まる。
#[test]
fn add_circle_bounding_box() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let cx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let cy = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let r = noprop::sample_f64_in(ctx, 0.01, 100.0);

        let mut path = Path::new();
        path.add_circle(cx, cy, r);

        for p in path.points() {
            assert!(
                p.x >= cx - r - 1e-10 && p.x <= cx + r + 1e-10,
                "x={} outside [{}, {}]",
                p.x,
                cx - r,
                cx + r
            );
            assert!(
                p.y >= cy - r - 1e-10 && p.y <= cy + r + 1e-10,
                "y={} outside [{}, {}]",
                p.y,
                cy - r,
                cy + r
            );
        }
        Ok(())
    })?;
    Ok(())
}
