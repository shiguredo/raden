use proptest::prelude::*;
use raden::Matrix2D;
use raden::PipelineRuntime;
use raden::api::matrix::transform_edges_reference;

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

/// テスト用の行列 Strategy。スケール・せん断・平行移動を含む一般的な行列。
fn arb_matrix() -> impl Strategy<Value = Matrix2D> {
    (
        -10.0f64..10.0,
        -10.0f64..10.0,
        -10.0f64..10.0,
        -10.0f64..10.0,
        -100.0f64..100.0,
        -100.0f64..100.0,
    )
        .prop_map(|(m00, m01, m10, m11, m20, m21)| Matrix2D::new(m00, m01, m10, m11, m20, m21))
}

/// テスト用の座標 Strategy。
fn arb_coord() -> impl Strategy<Value = f64> {
    -1000.0f64..1000.0
}

/// テスト用のエッジ Strategy。
fn arb_edge() -> impl Strategy<Value = (f64, f64, f64, f64)> {
    (arb_coord(), arb_coord(), arb_coord(), arb_coord())
}

/// テスト用のエッジ配列 Strategy。
fn arb_edges() -> impl Strategy<Value = Vec<(f64, f64, f64, f64)>> {
    prop::collection::vec(arb_edge(), 0..50)
}

/// テスト用の非ゼロスケール Strategy。
fn arb_nonzero_scale() -> impl Strategy<Value = f64> {
    prop_oneof![0.01f64..10.0, -10.0f64..-0.01]
}

proptest! {
    /// 単位行列で変換しても座標が変わらない。
    #[test]
    fn identity_no_change(
        x in arb_coord(),
        y in arb_coord(),
    ) {
        let m = Matrix2D::IDENTITY;
        let (xp, yp) = m.map_point(x, y);
        prop_assert!(approx_eq(xp, x, EPS), "x: {} -> {}", x, xp);
        prop_assert!(approx_eq(yp, y, EPS), "y: {} -> {}", y, yp);
    }

    /// 平行移動で全座標が (tx, ty) だけ増加する。
    #[test]
    fn translation_offset(
        x in arb_coord(),
        y in arb_coord(),
        tx in -100.0f64..100.0,
        ty in -100.0f64..100.0,
    ) {
        let m = Matrix2D::translation(tx, ty);
        let (xp, yp) = m.map_point(x, y);
        prop_assert!(approx_eq(xp, x + tx, EPS), "x: {} + {} = {} != {}", x, tx, x + tx, xp);
        prop_assert!(approx_eq(yp, y + ty, EPS), "y: {} + {} = {} != {}", y, ty, y + ty, yp);
    }

    /// scale(sx, sy) で座標が sx, sy 倍になる。
    #[test]
    fn scale_multiply(
        x in arb_coord(),
        y in arb_coord(),
        sx in -10.0f64..10.0,
        sy in -10.0f64..10.0,
    ) {
        let m = Matrix2D::scaling(sx, sy);
        let (xp, yp) = m.map_point(x, y);
        prop_assert!(approx_eq(xp, x * sx, 1e-6), "x*sx: {} != {}", x * sx, xp);
        prop_assert!(approx_eq(yp, y * sy, 1e-6), "y*sy: {} != {}", y * sy, yp);
    }

    /// 回転で原点からの距離が保存される。
    #[test]
    fn rotation_preserves_distance(
        x in arb_coord(),
        y in arb_coord(),
        angle in -std::f64::consts::TAU..std::f64::consts::TAU,
    ) {
        let m = Matrix2D::rotation(angle);
        let (xp, yp) = m.map_point(x, y);
        let dist_before = (x * x + y * y).sqrt();
        let dist_after = (xp * xp + yp * yp).sqrt();
        let eps = dist_before * 1e-10 + 1e-10;
        prop_assert!(
            approx_eq(dist_before, dist_after, eps),
            "distance: {} != {} (eps={})", dist_before, dist_after, eps
        );
    }

    /// 行列乗算は結合的: (A * B) * C ≈ A * (B * C)。
    #[test]
    fn multiply_associative(
        a in arb_matrix(),
        b in arb_matrix(),
        c in arb_matrix(),
    ) {
        let ab_c = a.multiply(&b).multiply(&c);
        let a_bc = a.multiply(&b.multiply(&c));
        prop_assert!(
            matrix_approx_eq(&ab_c, &a_bc, 1e-6),
            "(A*B)*C != A*(B*C):\n  lhs: {:?}\n  rhs: {:?}", ab_c, a_bc
        );
    }

    /// scale(s) → scale(1/s) で元に戻る。
    #[test]
    fn round_trip_scale(
        x in arb_coord(),
        y in arb_coord(),
        sx in arb_nonzero_scale(),
        sy in arb_nonzero_scale(),
    ) {
        let forward = Matrix2D::scaling(sx, sy);
        let inverse = Matrix2D::scaling(1.0 / sx, 1.0 / sy);
        let combined = forward.multiply(&inverse);
        let (xp, yp) = combined.map_point(x, y);
        let eps = x.abs() * 1e-10 + 1e-10;
        prop_assert!(approx_eq(xp, x, eps), "x: {} != {} (eps={})", xp, x, eps);
        let eps = y.abs() * 1e-10 + 1e-10;
        prop_assert!(approx_eq(yp, y, eps), "y: {} != {} (eps={})", yp, y, eps);
    }

    /// translate(tx, ty) → translate(-tx, -ty) で元に戻る。
    #[test]
    fn round_trip_translate(
        x in arb_coord(),
        y in arb_coord(),
        tx in -100.0f64..100.0,
        ty in -100.0f64..100.0,
    ) {
        let forward = Matrix2D::translation(tx, ty);
        let inverse = Matrix2D::translation(-tx, -ty);
        let combined = forward.multiply(&inverse);
        let (xp, yp) = combined.map_point(x, y);
        prop_assert!(approx_eq(xp, x, EPS), "x: {} != {}", xp, x);
        prop_assert!(approx_eq(yp, y, EPS), "y: {} != {}", yp, y);
    }

    /// rotate(θ) → rotate(-θ) で元に戻る。
    #[test]
    fn round_trip_rotate(
        x in arb_coord(),
        y in arb_coord(),
        angle in -std::f64::consts::TAU..std::f64::consts::TAU,
    ) {
        let forward = Matrix2D::rotation(angle);
        let inverse = Matrix2D::rotation(-angle);
        let combined = forward.multiply(&inverse);
        let (xp, yp) = combined.map_point(x, y);
        let eps_x = x.abs() * 1e-10 + 1e-10;
        let eps_y = y.abs() * 1e-10 + 1e-10;
        prop_assert!(approx_eq(xp, x, eps_x), "x: {} != {} (eps={})", xp, x, eps_x);
        prop_assert!(approx_eq(yp, y, eps_y), "y: {} != {} (eps={})", yp, y, eps_y);
    }

    /// JIT 版とリファレンス版の結果が一致する。
    #[test]
    fn jit_matches_reference(
        edges in arb_edges(),
        m in arb_matrix(),
    ) {
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
                    m.m00, m.m01, m.m10, m.m11, m.m20, m.m21,
                );
            }
        }

        // 比較
        for (i, (r, j)) in ref_edges.iter().zip(jit_edges.iter()).enumerate() {
            let eps = 1e-10;
            prop_assert!(
                approx_eq(r.0, j.0, eps) && approx_eq(r.1, j.1, eps)
                    && approx_eq(r.2, j.2, eps) && approx_eq(r.3, j.3, eps),
                "edge[{}] mismatch:\n  ref: {:?}\n  jit: {:?}", i, r, j
            );
        }
    }
}
