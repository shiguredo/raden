use proptest::prelude::*;
use raden::api::stroke::{StrokeOptions, stroke_to_fill};
use raden::{Path, StrokeCap, StrokeJoin};

/// 線分をストロークした面積が length * width に近いことを検証する。
/// Butt キャップでは矩形になるので面積は length * width に一致する。
#[test]
fn stroke_line_area() {
    proptest!(|(
        x0 in -50.0f64..50.0,
        y0 in -50.0f64..50.0,
        x1 in -50.0f64..50.0,
        y1 in -50.0f64..50.0,
        width in 0.5f64..10.0,
    )| {
        let dx = x1 - x0;
        let dy = y1 - y0;
        let length = (dx * dx + dy * dy).sqrt();

        // ゼロ長の線分はスキップする
        if length < 0.01 {
            return Ok(());
        }

        let mut input = Path::new();
        input.move_to(x0, y0);
        input.line_to(x1, y1);

        let options = StrokeOptions {
            width,
            start_cap: StrokeCap::Butt,
            end_cap: StrokeCap::Butt,
            join: StrokeJoin::Bevel,
            miter_limit: 4.0, ..Default::default()
        };

        let mut output = Path::new();
        stroke_to_fill(&input, &options, &mut output);

        // 出力パスが空でないことを確認する
        prop_assert!(!output.is_empty(), "ストローク出力が空");

        // Shoelace formula で面積を計算する
        let area = compute_path_area(&output);
        let expected_area = length * width;

        // 誤差 10% 以内
        let ratio = area / expected_area;
        prop_assert!(
            (0.9..=1.1).contains(&ratio),
            "面積比が許容範囲外: area={}, expected={}, ratio={}",
            area, expected_area, ratio
        );
    });
}

/// 閉じた矩形パスのストローク輪郭が空でないことを検証する。
#[test]
fn stroke_closed_path_symmetry() {
    proptest!(|(
        x in -50.0f64..50.0,
        y in -50.0f64..50.0,
        w in 5.0f64..50.0,
        h in 5.0f64..50.0,
        width in 0.5f64..5.0,
    )| {
        let mut input = Path::new();
        input.move_to(x, y);
        input.line_to(x + w, y);
        input.line_to(x + w, y + h);
        input.line_to(x, y + h);
        input.close();

        let options = StrokeOptions {
            width,
            start_cap: StrokeCap::Butt,
            end_cap: StrokeCap::Butt,
            join: StrokeJoin::Bevel,
            miter_limit: 4.0, ..Default::default()
        };

        let mut output = Path::new();
        stroke_to_fill(&input, &options, &mut output);

        prop_assert!(!output.is_empty(), "閉じたパスのストローク出力が空");
        prop_assert!(output.points().len() >= 4, "点が不足: {}", output.points().len());
    });
}

/// width=0 でストロークすると空パスになることを検証する。
#[test]
fn stroke_zero_width_empty() {
    proptest!(|(
        x0 in -50.0f64..50.0,
        y0 in -50.0f64..50.0,
        x1 in -50.0f64..50.0,
        y1 in -50.0f64..50.0,
    )| {
        let mut input = Path::new();
        input.move_to(x0, y0);
        input.line_to(x1, y1);

        let options = StrokeOptions {
            width: 0.0,
            start_cap: StrokeCap::Butt,
            end_cap: StrokeCap::Butt,
            join: StrokeJoin::Bevel,
            miter_limit: 4.0, ..Default::default()
        };

        let mut output = Path::new();
        stroke_to_fill(&input, &options, &mut output);

        prop_assert!(output.is_empty(), "width=0 なのに出力が空でない");
    });
}

/// ゼロ長線分、同一点 MoveTo 等の退化ケースでパニックしないことを検証する。
#[test]
fn stroke_degenerate_no_panic() {
    proptest!(|(
        x in -100.0f64..100.0,
        y in -100.0f64..100.0,
        width in 0.1f64..10.0,
    )| {
        // ゼロ長線分
        let mut input = Path::new();
        input.move_to(x, y);
        input.line_to(x, y);

        let options = StrokeOptions {
            width,
            start_cap: StrokeCap::Butt,
            end_cap: StrokeCap::Butt,
            join: StrokeJoin::Bevel,
            miter_limit: 4.0, ..Default::default()
        };

        let mut output = Path::new();
        stroke_to_fill(&input, &options, &mut output);

        // MoveTo のみ
        let mut input2 = Path::new();
        input2.move_to(x, y);

        output.clear();
        stroke_to_fill(&input2, &options, &mut output);

        // 空パス
        let input3 = Path::new();
        output.clear();
        stroke_to_fill(&input3, &options, &mut output);

        // 閉じたゼロ長パス
        let mut input4 = Path::new();
        input4.move_to(x, y);
        input4.close();

        output.clear();
        stroke_to_fill(&input4, &options, &mut output);
    });
}

/// 全 Cap タイプでパニックしないことを検証する。
#[test]
fn stroke_cap_types_no_panic() {
    proptest!(|(
        x0 in -50.0f64..50.0,
        y0 in -50.0f64..50.0,
        x1 in -50.0f64..50.0,
        y1 in -50.0f64..50.0,
        width in 0.5f64..10.0,
        cap_idx in 0u8..3,
    )| {
        let dx = x1 - x0;
        let dy = y1 - y0;
        if (dx * dx + dy * dy).sqrt() < 0.01 {
            return Ok(());
        }

        let cap = match cap_idx {
            0 => StrokeCap::Butt,
            1 => StrokeCap::Square,
            _ => StrokeCap::Round,
        };

        let mut input = Path::new();
        input.move_to(x0, y0);
        input.line_to(x1, y1);

        let options = StrokeOptions {
            width,
            start_cap: cap,
            end_cap: cap,
            join: StrokeJoin::Bevel,
            miter_limit: 4.0, ..Default::default()
        };

        let mut output = Path::new();
        stroke_to_fill(&input, &options, &mut output);

        prop_assert!(!output.is_empty(), "cap={:?} で出力が空", cap);
    });
}

/// 全 Join タイプでパニックしないことを検証する。
#[test]
fn stroke_join_types_no_panic() {
    proptest!(|(
        x0 in -30.0f64..30.0,
        y0 in -30.0f64..30.0,
        x1 in -30.0f64..30.0,
        y1 in -30.0f64..30.0,
        x2 in -30.0f64..30.0,
        y2 in -30.0f64..30.0,
        width in 0.5f64..5.0,
        join_idx in 0u8..5,
    )| {
        // 退化した線分をスキップする
        let d1 = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
        let d2 = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
        if d1 < 0.01 || d2 < 0.01 {
            return Ok(());
        }

        let join = match join_idx {
            0 => StrokeJoin::MiterClip,
            1 => StrokeJoin::MiterBevel,
            2 => StrokeJoin::MiterRound,
            3 => StrokeJoin::Bevel,
            _ => StrokeJoin::Round,
        };

        let mut input = Path::new();
        input.move_to(x0, y0);
        input.line_to(x1, y1);
        input.line_to(x2, y2);

        let options = StrokeOptions {
            width,
            start_cap: StrokeCap::Butt,
            end_cap: StrokeCap::Butt,
            join,
            miter_limit: 4.0, ..Default::default()
        };

        let mut output = Path::new();
        stroke_to_fill(&input, &options, &mut output);

        prop_assert!(!output.is_empty(), "join={:?} で出力が空", join);
    });
}

/// Shoelace formula でパスの面積 (絶対値) を計算する。
/// パスを平坦化して計算する。
fn compute_path_area(path: &Path) -> f64 {
    use raden::PathCmd;

    let cmds = path.cmds();
    let points = path.points();
    let conic_w = path.conic_weights();
    let mut conic_idx = 0usize;
    let mut total_area = 0.0;
    let mut pt_idx = 0usize;
    let mut polygon: Vec<(f64, f64)> = Vec::new();
    let mut cur = (0.0f64, 0.0f64);
    let mut start = (0.0f64, 0.0f64);

    for &cmd in cmds {
        match cmd {
            PathCmd::MoveTo => {
                // 前のポリゴンの面積を加算する
                total_area += shoelace_area(&polygon);
                polygon.clear();
                let p = points[pt_idx];
                pt_idx += 1;
                start = (p.x, p.y);
                cur = start;
                polygon.push(cur);
            }
            PathCmd::LineTo => {
                let p = points[pt_idx];
                pt_idx += 1;
                cur = (p.x, p.y);
                polygon.push(cur);
            }
            PathCmd::CubicTo => {
                // 面積計算用に中間点を追加して線分化する
                let cp1 = points[pt_idx];
                let cp2 = points[pt_idx + 1];
                let end = points[pt_idx + 2];
                pt_idx += 3;
                flatten_cubic_for_area(
                    &mut polygon,
                    cur,
                    (cp1.x, cp1.y),
                    (cp2.x, cp2.y),
                    (end.x, end.y),
                    0,
                );
                cur = (end.x, end.y);
            }
            PathCmd::QuadTo => {
                let cp = points[pt_idx];
                let end = points[pt_idx + 1];
                pt_idx += 2;
                flatten_quad_for_area(&mut polygon, cur, (cp.x, cp.y), (end.x, end.y), 0);
                cur = (end.x, end.y);
            }
            PathCmd::ConicTo => {
                let cp = points[pt_idx];
                let end = points[pt_idx + 1];
                pt_idx += 2;
                let w = conic_w[conic_idx];
                conic_idx += 1;
                flatten_conic_for_area(&mut polygon, cur, (cp.x, cp.y), (end.x, end.y), w);
                cur = (end.x, end.y);
            }
            PathCmd::Close => {
                if cur != start {
                    polygon.push(start);
                }
                cur = start;
            }
        }
    }

    total_area += shoelace_area(&polygon);
    total_area.abs()
}

fn shoelace_area(polygon: &[(f64, f64)]) -> f64 {
    if polygon.len() < 3 {
        return 0.0;
    }
    let mut area = 0.0;
    let n = polygon.len();
    for i in 0..n {
        let j = (i + 1) % n;
        area += polygon[i].0 * polygon[j].1;
        area -= polygon[j].0 * polygon[i].1;
    }
    area * 0.5
}

fn flatten_cubic_for_area(
    polygon: &mut Vec<(f64, f64)>,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    depth: u32,
) {
    if depth >= 8 {
        polygon.push(p3);
        return;
    }

    let m01 = ((p0.0 + p1.0) * 0.5, (p0.1 + p1.1) * 0.5);
    let m12 = ((p1.0 + p2.0) * 0.5, (p1.1 + p2.1) * 0.5);
    let m23 = ((p2.0 + p3.0) * 0.5, (p2.1 + p3.1) * 0.5);
    let m012 = ((m01.0 + m12.0) * 0.5, (m01.1 + m12.1) * 0.5);
    let m123 = ((m12.0 + m23.0) * 0.5, (m12.1 + m23.1) * 0.5);
    let m0123 = ((m012.0 + m123.0) * 0.5, (m012.1 + m123.1) * 0.5);

    flatten_cubic_for_area(polygon, p0, m01, m012, m0123, depth + 1);
    flatten_cubic_for_area(polygon, m0123, m123, m23, p3, depth + 1);
}

fn flatten_quad_for_area(
    polygon: &mut Vec<(f64, f64)>,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    depth: u32,
) {
    if depth >= 8 {
        polygon.push(p2);
        return;
    }

    let m01 = ((p0.0 + p1.0) * 0.5, (p0.1 + p1.1) * 0.5);
    let m12 = ((p1.0 + p2.0) * 0.5, (p1.1 + p2.1) * 0.5);
    let m012 = ((m01.0 + m12.0) * 0.5, (m01.1 + m12.1) * 0.5);

    flatten_quad_for_area(polygon, p0, m01, m012, depth + 1);
    flatten_quad_for_area(polygon, m012, m12, p2, depth + 1);
}

fn flatten_conic_for_area(
    polygon: &mut Vec<(f64, f64)>,
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    w: f64,
) {
    const N: usize = 24;
    for i in 1..=N {
        let t = i as f64 / N as f64;
        let u = 1.0 - t;
        let denom = u * u + 2.0 * w * u * t + t * t;
        let x = (u * u * p0.0 + 2.0 * w * u * t * p1.0 + t * t * p2.0) / denom;
        let y = (u * u * p0.1 + 2.0 * w * u * t * p1.1 + t * t * p2.1) / denom;
        polygon.push((x, y));
    }
}
