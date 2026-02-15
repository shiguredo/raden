use crate::api::path::{Path, PathCmd, Point};

/// Blend2D 同等の平坦度閾値。
const FLATNESS_TOLERANCE: f64 = 0.25;

/// de Casteljau 再帰分割の最大深度。
const MAX_RECURSION_DEPTH: u32 = 16;

/// Path を線分に平坦化する。
pub struct EdgeBuilder;

impl EdgeBuilder {
    /// Path を平坦化し、各線分に対してコールバック f(x0, y0, x1, y1) を呼ぶ。
    pub fn flatten<F>(path: &Path, mut f: F)
    where
        F: FnMut(f64, f64, f64, f64),
    {
        let cmds = path.cmds();
        let points = path.points();
        let mut pt_idx = 0usize;

        // サブパス開始点 (Close で戻る先)
        let mut start = Point::new(0.0, 0.0);
        // 現在の点
        let mut cur = Point::new(0.0, 0.0);

        for &cmd in cmds {
            match cmd {
                PathCmd::MoveTo => {
                    let p = points[pt_idx];
                    pt_idx += 1;
                    start = p;
                    cur = p;
                }
                PathCmd::LineTo => {
                    let p = points[pt_idx];
                    pt_idx += 1;
                    f(cur.x, cur.y, p.x, p.y);
                    cur = p;
                }
                PathCmd::CubicTo => {
                    let cp1 = points[pt_idx];
                    let cp2 = points[pt_idx + 1];
                    let end = points[pt_idx + 2];
                    pt_idx += 3;
                    flatten_cubic(&mut f, cur, cp1, cp2, end, 0);
                    cur = end;
                }
                PathCmd::QuadTo => {
                    let cp = points[pt_idx];
                    let end = points[pt_idx + 1];
                    pt_idx += 2;
                    flatten_quadratic(&mut f, cur, cp, end, 0);
                    cur = end;
                }
                PathCmd::Close => {
                    if cur.x != start.x || cur.y != start.y {
                        f(cur.x, cur.y, start.x, start.y);
                    }
                    cur = start;
                }
            }
        }
    }
}

/// 3 次ベジェ曲線を de Casteljau で再帰的に平坦化する。
fn flatten_cubic<F>(f: &mut F, p0: Point, p1: Point, p2: Point, p3: Point, depth: u32)
where
    F: FnMut(f64, f64, f64, f64),
{
    if depth >= MAX_RECURSION_DEPTH || is_flat(p0, p1, p2, p3) {
        f(p0.x, p0.y, p3.x, p3.y);
        return;
    }

    // de Casteljau t=0.5 分割
    let m01 = mid(p0, p1);
    let m12 = mid(p1, p2);
    let m23 = mid(p2, p3);
    let m012 = mid(m01, m12);
    let m123 = mid(m12, m23);
    let m0123 = mid(m012, m123);

    flatten_cubic(f, p0, m01, m012, m0123, depth + 1);
    flatten_cubic(f, m0123, m123, m23, p3, depth + 1);
}

/// 制御点 p1, p2 が始点-終点直線に十分近いかを判定する (外積ベース)。
fn is_flat(p0: Point, p1: Point, p2: Point, p3: Point) -> bool {
    let dx = p3.x - p0.x;
    let dy = p3.y - p0.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-12 {
        // 始点と終点がほぼ同じ場合は制御点までの距離で判定
        let d1_sq = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
        let d2_sq = (p2.x - p0.x).powi(2) + (p2.y - p0.y).powi(2);
        let tol_sq = FLATNESS_TOLERANCE * FLATNESS_TOLERANCE;
        return d1_sq <= tol_sq && d2_sq <= tol_sq;
    }

    // cp1, cp2 から始点-終点直線への距離の 2 乗
    let cross1 = (p1.x - p0.x) * dy - (p1.y - p0.y) * dx;
    let cross2 = (p2.x - p0.x) * dy - (p2.y - p0.y) * dx;
    let tol_sq = FLATNESS_TOLERANCE * FLATNESS_TOLERANCE * len_sq;

    cross1 * cross1 <= tol_sq && cross2 * cross2 <= tol_sq
}

/// 2 次ベジェ曲線を de Casteljau で再帰的に平坦化する。
fn flatten_quadratic<F>(f: &mut F, p0: Point, p1: Point, p2: Point, depth: u32)
where
    F: FnMut(f64, f64, f64, f64),
{
    if depth >= MAX_RECURSION_DEPTH || is_flat_quad(p0, p1, p2) {
        f(p0.x, p0.y, p2.x, p2.y);
        return;
    }

    // de Casteljau t=0.5 分割
    let m01 = mid(p0, p1);
    let m12 = mid(p1, p2);
    let m012 = mid(m01, m12);

    flatten_quadratic(f, p0, m01, m012, depth + 1);
    flatten_quadratic(f, m012, m12, p2, depth + 1);
}

/// 制御点 p1 が始点-終点直線に十分近いかを判定する (外積ベース)。
fn is_flat_quad(p0: Point, p1: Point, p2: Point) -> bool {
    let dx = p2.x - p0.x;
    let dy = p2.y - p0.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-12 {
        let d_sq = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
        return d_sq <= FLATNESS_TOLERANCE * FLATNESS_TOLERANCE;
    }

    let cross = (p1.x - p0.x) * dy - (p1.y - p0.y) * dx;
    let tol_sq = FLATNESS_TOLERANCE * FLATNESS_TOLERANCE * len_sq;
    cross * cross <= tol_sq
}

/// 2 点の中点を返す。
fn mid(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}
