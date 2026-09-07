use raden::Matrix2D;
use raden::PipelineRuntime;
use raden::api::matrix::transform_edges_reference;

/// 1 テストあたりのケース数。
const CASES: usize = 256;

/// シード再現用の環境変数名。
const SEED_ENV: &str = "RADEN_PBT_SEED";

const EPS: f64 = 1e-10;

fn approx_eq(a: f64, b: f64, eps: f64) -> bool {
    (a - b).abs() < eps
}

fn matrix_approx_eq(a: &Matrix2D, b: &Matrix2D, eps: f64) -> bool {
    approx_eq(a.m00, b.m00, eps)
        && approx_eq(a.m01, b.m01, eps)
        && approx_eq(a.m10, b.m10, eps)
        && approx_eq(a.m11, b.m11, eps)
        && approx_eq(a.m20, b.m20, eps)
        && approx_eq(a.m21, b.m21, eps)
}

/// テスト用の座標をサンプリングする。
fn sample_coord(ctx: &mut noprop::TestCaseContext) -> f64 {
    noprop::sample_f64_in(ctx, -1000.0, 1000.0)
}

/// テスト用の行列をサンプリングする。
fn sample_matrix(ctx: &mut noprop::TestCaseContext) -> Matrix2D {
    let m00 = noprop::sample_f64_in(ctx, -10.0, 10.0);
    let m01 = noprop::sample_f64_in(ctx, -10.0, 10.0);
    let m10 = noprop::sample_f64_in(ctx, -10.0, 10.0);
    let m11 = noprop::sample_f64_in(ctx, -10.0, 10.0);
    let m20 = noprop::sample_f64_in(ctx, -100.0, 100.0);
    let m21 = noprop::sample_f64_in(ctx, -100.0, 100.0);
    Matrix2D::new(m00, m01, m10, m11, m20, m21)
}

/// テスト用のエッジをサンプリングする。
fn sample_edge(ctx: &mut noprop::TestCaseContext) -> (f64, f64, f64, f64) {
    (
        sample_coord(ctx),
        sample_coord(ctx),
        sample_coord(ctx),
        sample_coord(ctx),
    )
}

/// テスト用のエッジ配列をサンプリングする。
fn sample_edges(ctx: &mut noprop::TestCaseContext) -> Vec<(f64, f64, f64, f64)> {
    let len = noprop::sample_usize_in(ctx, 0..50);
    (0..len).map(|_| sample_edge(ctx)).collect()
}

/// テスト用の非ゼロスケールをサンプリングする。
fn sample_nonzero_scale(ctx: &mut noprop::TestCaseContext) -> f64 {
    if noprop::sample_bool(ctx) {
        noprop::sample_f64_in(ctx, 0.01, 10.0)
    } else {
        noprop::sample_f64_in(ctx, -10.0, -0.01)
    }
}

/// 単位行列で変換しても座標が変わらない。
#[test]
fn identity_no_change() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let x = sample_coord(ctx);
        let y = sample_coord(ctx);
        let m = Matrix2D::IDENTITY;
        let (xp, yp) = m.map_point(x, y);
        assert!(approx_eq(xp, x, EPS), "x: {} -> {}", x, xp);
        assert!(approx_eq(yp, y, EPS), "y: {} -> {}", y, yp);
        Ok(())
    })?;
    Ok(())
}

/// 平行移動で全座標が (tx, ty) だけ増加する。
#[test]
fn translation_offset() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let x = sample_coord(ctx);
        let y = sample_coord(ctx);
        let tx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let ty = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let m = Matrix2D::translation(tx, ty);
        let (xp, yp) = m.map_point(x, y);
        assert!(
            approx_eq(xp, x + tx, EPS),
            "x: {} + {} = {} != {}",
            x,
            tx,
            x + tx,
            xp
        );
        assert!(
            approx_eq(yp, y + ty, EPS),
            "y: {} + {} = {} != {}",
            y,
            ty,
            y + ty,
            yp
        );
        Ok(())
    })?;
    Ok(())
}

/// scale(sx, sy) で座標が sx, sy 倍になる。
#[test]
fn scale_multiply() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let x = sample_coord(ctx);
        let y = sample_coord(ctx);
        let sx = noprop::sample_f64_in(ctx, -10.0, 10.0);
        let sy = noprop::sample_f64_in(ctx, -10.0, 10.0);
        let m = Matrix2D::scaling(sx, sy);
        let (xp, yp) = m.map_point(x, y);
        assert!(approx_eq(xp, x * sx, 1e-6), "x*sx: {} != {}", x * sx, xp);
        assert!(approx_eq(yp, y * sy, 1e-6), "y*sy: {} != {}", y * sy, yp);
        Ok(())
    })?;
    Ok(())
}

/// 回転で原点からの距離が保存される。
#[test]
fn rotation_preserves_distance() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let x = sample_coord(ctx);
        let y = sample_coord(ctx);
        let angle = noprop::sample_f64_in(ctx, -std::f64::consts::TAU, std::f64::consts::TAU);
        let m = Matrix2D::rotation(angle);
        let (xp, yp) = m.map_point(x, y);
        let dist_before = (x * x + y * y).sqrt();
        let dist_after = (xp * xp + yp * yp).sqrt();
        let eps = dist_before * 1e-10 + 1e-10;
        assert!(
            approx_eq(dist_before, dist_after, eps),
            "distance: {} != {} (eps={})",
            dist_before,
            dist_after,
            eps
        );
        Ok(())
    })?;
    Ok(())
}

/// 行列乗算は結合的: (A * B) * C ≈ A * (B * C)。
#[test]
fn multiply_associative() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let a = sample_matrix(ctx);
        let b = sample_matrix(ctx);
        let c = sample_matrix(ctx);
        let ab_c = a.multiply(&b).multiply(&c);
        let a_bc = a.multiply(&b.multiply(&c));
        assert!(
            matrix_approx_eq(&ab_c, &a_bc, 1e-6),
            "(A*B)*C != A*(B*C):\n  lhs: {:?}\n  rhs: {:?}",
            ab_c,
            a_bc
        );
        Ok(())
    })?;
    Ok(())
}

/// scale(s) → scale(1/s) で元に戻る。
#[test]
fn round_trip_scale() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let x = sample_coord(ctx);
        let y = sample_coord(ctx);
        let sx = sample_nonzero_scale(ctx);
        let sy = sample_nonzero_scale(ctx);
        let forward = Matrix2D::scaling(sx, sy);
        let inverse = Matrix2D::scaling(1.0 / sx, 1.0 / sy);
        let combined = forward.multiply(&inverse);
        let (xp, yp) = combined.map_point(x, y);
        let eps = x.abs() * 1e-10 + 1e-10;
        assert!(approx_eq(xp, x, eps), "x: {} != {} (eps={})", xp, x, eps);
        let eps = y.abs() * 1e-10 + 1e-10;
        assert!(approx_eq(yp, y, eps), "y: {} != {} (eps={})", yp, y, eps);
        Ok(())
    })?;
    Ok(())
}

/// translate(tx, ty) → translate(-tx, -ty) で元に戻る。
#[test]
fn round_trip_translate() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let x = sample_coord(ctx);
        let y = sample_coord(ctx);
        let tx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let ty = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let forward = Matrix2D::translation(tx, ty);
        let inverse = Matrix2D::translation(-tx, -ty);
        let combined = forward.multiply(&inverse);
        let (xp, yp) = combined.map_point(x, y);
        assert!(approx_eq(xp, x, EPS), "x: {} != {}", xp, x);
        assert!(approx_eq(yp, y, EPS), "y: {} != {}", yp, y);
        Ok(())
    })?;
    Ok(())
}

/// rotate(θ) → rotate(-θ) で元に戻る。
#[test]
fn round_trip_rotate() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let x = sample_coord(ctx);
        let y = sample_coord(ctx);
        let angle = noprop::sample_f64_in(ctx, -std::f64::consts::TAU, std::f64::consts::TAU);
        let forward = Matrix2D::rotation(angle);
        let inverse = Matrix2D::rotation(-angle);
        let combined = forward.multiply(&inverse);
        let (xp, yp) = combined.map_point(x, y);
        let eps_x = x.abs() * 1e-10 + 1e-10;
        let eps_y = y.abs() * 1e-10 + 1e-10;
        assert!(
            approx_eq(xp, x, eps_x),
            "x: {} != {} (eps={})",
            xp,
            x,
            eps_x
        );
        assert!(
            approx_eq(yp, y, eps_y),
            "y: {} != {} (eps={})",
            yp,
            y,
            eps_y
        );
        Ok(())
    })?;
    Ok(())
}

/// post_translate は T(tx,ty) * self と等価。
#[test]
fn post_translate_is_premultiply() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let m = sample_matrix(ctx);
        let tx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let ty = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let mut actual = m;
        actual.post_translate(tx, ty);
        let expected = Matrix2D::translation(tx, ty).multiply(&m);
        assert!(
            matrix_approx_eq(&actual, &expected, EPS),
            "post_translate != T * M:\n  actual: {:?}\n  expected: {:?}",
            actual,
            expected
        );
        Ok(())
    })?;
    Ok(())
}

/// post_scale は S(sx,sy) * self と等価。
#[test]
fn post_scale_is_premultiply() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let m = sample_matrix(ctx);
        let sx = noprop::sample_f64_in(ctx, -10.0, 10.0);
        let sy = noprop::sample_f64_in(ctx, -10.0, 10.0);
        let mut actual = m;
        actual.post_scale(sx, sy);
        let expected = Matrix2D::scaling(sx, sy).multiply(&m);
        assert!(
            matrix_approx_eq(&actual, &expected, 1e-8),
            "post_scale != S * M:\n  actual: {:?}\n  expected: {:?}",
            actual,
            expected
        );
        Ok(())
    })?;
    Ok(())
}

/// post_rotate は R(angle) * self と等価。
#[test]
fn post_rotate_is_premultiply() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let m = sample_matrix(ctx);
        let angle = noprop::sample_f64_in(ctx, -std::f64::consts::TAU, std::f64::consts::TAU);
        let mut actual = m;
        actual.post_rotate(angle);
        let expected = Matrix2D::rotation(angle).multiply(&m);
        assert!(
            matrix_approx_eq(&actual, &expected, 1e-8),
            "post_rotate != R * M:\n  actual: {:?}\n  expected: {:?}",
            actual,
            expected
        );
        Ok(())
    })?;
    Ok(())
}

/// skew(kx, ky) は (0, 1) を (kx, 1) に写す。
#[test]
fn skew_maps_unit_y() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let kx = noprop::sample_f64_in(ctx, -2.0, 2.0);
        let ky = noprop::sample_f64_in(ctx, -2.0, 2.0);
        let mut m = Matrix2D::IDENTITY;
        m.skew(kx, ky);
        let (xp, yp) = m.map_point(0.0, 1.0);
        assert!(approx_eq(xp, kx, EPS), "x: {} != {}", xp, kx);
        assert!(approx_eq(yp, 1.0, EPS), "y: {} != 1", yp);
        Ok(())
    })?;
    Ok(())
}

/// skew(kx, ky) は (1, 0) を (1, ky) に写す。
#[test]
fn skew_maps_unit_x() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let kx = noprop::sample_f64_in(ctx, -2.0, 2.0);
        let ky = noprop::sample_f64_in(ctx, -2.0, 2.0);
        let mut m = Matrix2D::IDENTITY;
        m.skew(kx, ky);
        let (xp, yp) = m.map_point(1.0, 0.0);
        assert!(approx_eq(xp, 1.0, EPS), "x: {} != 1", xp);
        assert!(approx_eq(yp, ky, EPS), "y: {} != {}", yp, ky);
        Ok(())
    })?;
    Ok(())
}

/// post_skew は S(kx, ky) * self と等価。
#[test]
fn post_skew_is_premultiply() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let m = sample_matrix(ctx);
        let kx = noprop::sample_f64_in(ctx, -2.0, 2.0);
        let ky = noprop::sample_f64_in(ctx, -2.0, 2.0);
        let mut actual = m;
        actual.post_skew(kx, ky);
        let expected = Matrix2D::skewing(kx, ky).multiply(&m);
        assert!(
            matrix_approx_eq(&actual, &expected, 1e-8),
            "post_skew != S * M:\n  actual: {:?}\n  expected: {:?}",
            actual,
            expected
        );
        Ok(())
    })?;
    Ok(())
}

/// post_transform は m * self と等価。
#[test]
fn post_transform_is_premultiply() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let a = sample_matrix(ctx);
        let b = sample_matrix(ctx);
        let mut actual = a;
        actual.post_transform(&b);
        let expected = b.multiply(&a);
        assert!(
            matrix_approx_eq(&actual, &expected, 1e-8),
            "post_transform != B * A:\n  actual: {:?}\n  expected: {:?}",
            actual,
            expected
        );
        Ok(())
    })?;
    Ok(())
}

/// rotate_around は translate → rotate → translate の合成と等価。
#[test]
fn rotate_around_matches_manual() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let m = sample_matrix(ctx);
        let angle = noprop::sample_f64_in(ctx, -std::f64::consts::TAU, std::f64::consts::TAU);
        let cx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let cy = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let mut actual = m;
        actual.rotate_around(angle, cx, cy);

        let mut expected = m;
        expected.translate(-cx, -cy);
        expected.rotate(angle);
        expected.translate(cx, cy);

        assert!(
            matrix_approx_eq(&actual, &expected, 1e-8),
            "rotate_around != manual:\n  actual: {:?}\n  expected: {:?}",
            actual,
            expected
        );
        Ok(())
    })?;
    Ok(())
}

/// rotate_around で中心点が不動点になる。
#[test]
fn rotate_around_fixed_point() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let angle = noprop::sample_f64_in(ctx, -std::f64::consts::TAU, std::f64::consts::TAU);
        let cx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let cy = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let mut m = Matrix2D::IDENTITY;
        m.rotate_around(angle, cx, cy);
        let (xp, yp) = m.map_point(cx, cy);
        let eps = cx.abs().max(cy.abs()) * 1e-10 + 1e-10;
        assert!(approx_eq(xp, cx, eps), "cx: {} != {} (eps={})", xp, cx, eps);
        assert!(approx_eq(yp, cy, eps), "cy: {} != {} (eps={})", yp, cy, eps);
        Ok(())
    })?;
    Ok(())
}

/// rotate_around で中心点からの距離が保存される。
#[test]
fn rotate_around_preserves_distance() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let x = sample_coord(ctx);
        let y = sample_coord(ctx);
        let angle = noprop::sample_f64_in(ctx, -std::f64::consts::TAU, std::f64::consts::TAU);
        let cx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let cy = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let mut m = Matrix2D::IDENTITY;
        m.rotate_around(angle, cx, cy);
        let (xp, yp) = m.map_point(x, y);
        let dist_before = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
        let dist_after = ((xp - cx).powi(2) + (yp - cy).powi(2)).sqrt();
        let eps = dist_before * 1e-10 + 1e-10;
        assert!(
            approx_eq(dist_before, dist_after, eps),
            "distance: {} != {} (eps={})",
            dist_before,
            dist_after,
            eps
        );
        Ok(())
    })?;
    Ok(())
}

/// post_translate(tx,ty) → post_translate(-tx,-ty) で元に戻る。
#[test]
fn round_trip_post_translate() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let m = sample_matrix(ctx);
        let tx = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let ty = noprop::sample_f64_in(ctx, -100.0, 100.0);
        let mut actual = m;
        actual.post_translate(tx, ty);
        actual.post_translate(-tx, -ty);
        assert!(
            matrix_approx_eq(&actual, &m, EPS),
            "round trip failed:\n  actual: {:?}\n  expected: {:?}",
            actual,
            m
        );
        Ok(())
    })?;
    Ok(())
}

/// JIT 版とリファレンス版の結果が一致する。
#[test]
fn jit_matches_reference() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let edges = sample_edges(ctx);
        let m = sample_matrix(ctx);
        let mut runtime = PipelineRuntime::new();
        let transform_fn = runtime.get_or_compile_transform_edges();

        // リファレンス版
        let mut ref_edges = edges.clone();
        transform_edges_reference(&mut ref_edges, &m);

        // JIT 版
        let mut jit_edges = edges;
        if !jit_edges.is_empty() {
            unsafe {
                transform_fn(
                    jit_edges.as_mut_ptr().cast::<f64>(),
                    jit_edges.len(),
                    m.m00,
                    m.m01,
                    m.m10,
                    m.m11,
                    m.m20,
                    m.m21,
                );
            }
        }

        // 比較
        for (i, (r, j)) in ref_edges.iter().zip(jit_edges.iter()).enumerate() {
            let eps = 1e-10;
            assert!(
                approx_eq(r.0, j.0, eps)
                    && approx_eq(r.1, j.1, eps)
                    && approx_eq(r.2, j.2, eps)
                    && approx_eq(r.3, j.3, eps),
                "edge[{}] mismatch:\n  ref: {:?}\n  jit: {:?}",
                i,
                r,
                j
            );
        }
        Ok(())
    })?;
    Ok(())
}
