use crate::api::path::{Path, PathCmd, Point};
use crate::api::style::{StrokeCap, StrokeJoin};

/// ストロークオプション。
pub struct StrokeOptions {
    pub width: f64,
    pub start_cap: StrokeCap,
    pub end_cap: StrokeCap,
    pub join: StrokeJoin,
    /// マイターリミット (デフォルト 4.0、Blend2D 準拠)。
    pub miter_limit: f64,
    /// ダッシュパターン。交互に「描画区間」「空白区間」の長さを指定する。
    /// 空の場合は実線。奇数要素の場合はパターンが 2 回繰り返される (SVG 準拠)。
    pub dash_array: Vec<f64>,
    /// ダッシュパターンの開始オフセット。
    pub dash_offset: f64,
}

impl Default for StrokeOptions {
    fn default() -> Self {
        Self {
            width: 1.0,
            start_cap: StrokeCap::default(),
            end_cap: StrokeCap::default(),
            join: StrokeJoin::default(),
            miter_limit: 4.0,
            dash_array: Vec::new(),
            dash_offset: 0.0,
        }
    }
}

/// 入力パスをストローク輪郭に変換し、output に追加する。
pub fn stroke_to_fill(input: &Path, options: &StrokeOptions, output: &mut Path) {
    let mut workspace = StrokeWorkspace::new();
    stroke_to_fill_with_workspace(input, options, output, &mut workspace);
}

/// ワークスペースを再利用してストローク輪郭に変換する。
pub fn stroke_to_fill_with_workspace(
    input: &Path,
    options: &StrokeOptions,
    output: &mut Path,
    workspace: &mut StrokeWorkspace,
) {
    let half_width = options.width * 0.5;
    if half_width <= 0.0 {
        return;
    }

    // 入力パスを平坦化して線分列に変換する
    flatten_into_workspace(input, workspace);

    // ダッシュパターンがある場合、サブパスをダッシュで分断する
    if !options.dash_array.is_empty() {
        apply_dash(workspace, &options.dash_array, options.dash_offset);
    }

    // サブパス単位で処理する
    for i in 0..workspace.subpath_ranges.len() {
        let range = workspace.subpath_ranges[i];
        let pts = &workspace.flat_points[range.start..range.end];
        if pts.len() < 2 {
            continue;
        }
        stroke_subpath_slice(
            output,
            pts,
            range.closed,
            half_width,
            options.start_cap,
            options.end_cap,
            options.join,
            options.miter_limit,
            &mut workspace.seg_normals,
        );
    }
}

/// ストローク処理の中間バッファ。呼び出し間で再利用してアロケーションを回避する。
pub struct StrokeWorkspace {
    flat_points: Vec<Point>,
    subpath_ranges: Vec<SubpathRange>,
    seg_normals: Vec<(usize, f64, f64)>,
    /// ダッシュ分断後の点列バッファ。
    dash_points: Vec<Point>,
    /// ダッシュ分断後のサブパス範囲。
    dash_ranges: Vec<SubpathRange>,
}

#[derive(Clone, Copy)]
struct SubpathRange {
    start: usize,
    end: usize,
    closed: bool,
}

impl StrokeWorkspace {
    pub fn new() -> Self {
        Self {
            flat_points: Vec::new(),
            subpath_ranges: Vec::new(),
            seg_normals: Vec::new(),
            dash_points: Vec::new(),
            dash_ranges: Vec::new(),
        }
    }
}

impl Default for StrokeWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

/// 入力パスを平坦化し、ワークスペースのバッファに書き込む。
/// ダッシュパターンを平坦化済みサブパスに適用する。
///
/// 各サブパスの線分列をダッシュパターンに従って分断し、
/// 「描画区間」のみを新しいサブパス群として workspace に書き戻す。
fn apply_dash(workspace: &mut StrokeWorkspace, dash_array: &[f64], dash_offset: f64) {
    // SVG 準拠: 奇数要素はパターンを 2 回繰り返す
    let pattern: Vec<f64> = if dash_array.len() % 2 == 1 {
        dash_array
            .iter()
            .chain(dash_array.iter())
            .copied()
            .collect()
    } else {
        dash_array.to_vec()
    };

    // パターンの合計長
    let pattern_len: f64 = pattern.iter().sum();
    if pattern_len <= 0.0 {
        return;
    }

    workspace.dash_points.clear();
    workspace.dash_ranges.clear();

    let original_ranges = workspace.subpath_ranges.clone();
    let original_points = workspace.flat_points.clone();

    for range in &original_ranges {
        let pts = &original_points[range.start..range.end];
        if pts.len() < 2 {
            continue;
        }

        // dash_offset をパターン内位置に正規化する
        let mut offset = ((dash_offset % pattern_len) + pattern_len) % pattern_len;
        let mut pat_idx = 0usize;
        // offset 分をスキップしてパターン位置を確定する
        while offset > 0.0 && pat_idx < pattern.len() {
            if offset < pattern[pat_idx] {
                break;
            }
            offset -= pattern[pat_idx];
            pat_idx = (pat_idx + 1) % pattern.len();
        }
        let mut remaining = pattern[pat_idx] - offset;
        let mut drawing = pat_idx.is_multiple_of(2); // 偶数インデックスが描画区間
        let mut dash_start: Option<usize> = None;

        if drawing {
            dash_start = Some(workspace.dash_points.len());
            workspace.dash_points.push(pts[0]);
        }

        // 各線分を走査する
        for seg in 0..pts.len() - 1 {
            let p0 = pts[seg];
            let p1 = pts[seg + 1];
            let dx = p1.x - p0.x;
            let dy = p1.y - p0.y;
            let seg_len = (dx * dx + dy * dy).sqrt();
            if seg_len < 1e-10 {
                continue;
            }
            let ux = dx / seg_len;
            let uy = dy / seg_len;

            let mut consumed = 0.0;

            while consumed < seg_len {
                let avail = seg_len - consumed;
                if remaining <= avail {
                    // パターン境界が線分内にある
                    consumed += remaining;
                    let split_x = p0.x + ux * consumed;
                    let split_y = p0.y + uy * consumed;
                    let split = Point::new(split_x, split_y);

                    if drawing {
                        // 描画区間の終了
                        workspace.dash_points.push(split);
                        if let Some(start) = dash_start.take() {
                            let end = workspace.dash_points.len();
                            if end - start >= 2 {
                                workspace.dash_ranges.push(SubpathRange {
                                    start,
                                    end,
                                    closed: false,
                                });
                            }
                        }
                    } else {
                        // 空白区間の終了 → 次の描画区間の開始
                        dash_start = Some(workspace.dash_points.len());
                        workspace.dash_points.push(split);
                    }

                    drawing = !drawing;
                    pat_idx = (pat_idx + 1) % pattern.len();
                    remaining = pattern[pat_idx];
                } else {
                    // 線分の残りがパターン区間内に収まる
                    remaining -= avail;
                    if drawing {
                        workspace.dash_points.push(p1);
                    }
                    break;
                }
            }
        }

        // 最後の描画区間を閉じる
        if drawing && let Some(start) = dash_start.take() {
            let end = workspace.dash_points.len();
            if end - start >= 2 {
                workspace.dash_ranges.push(SubpathRange {
                    start,
                    end,
                    closed: false,
                });
            }
        }
    }

    // ダッシュ結果で workspace を上書きする
    std::mem::swap(&mut workspace.flat_points, &mut workspace.dash_points);
    std::mem::swap(&mut workspace.subpath_ranges, &mut workspace.dash_ranges);
}

fn flatten_into_workspace(input: &Path, workspace: &mut StrokeWorkspace) {
    workspace.flat_points.clear();
    workspace.subpath_ranges.clear();

    let cmds = input.cmds();
    let points = input.points();
    let conic_w = input.conic_weights();
    let n_conic_cmds = cmds
        .iter()
        .filter(|&&c| c == PathCmd::ConicTo)
        .count();
    debug_assert_eq!(
        n_conic_cmds,
        conic_w.len(),
        "conic_weights length must match PathCmd::ConicTo count"
    );
    let mut conic_idx = 0usize;
    let mut subpath_start_idx: Option<usize> = None;
    let mut pt_idx = 0usize;
    let mut cur = Point::new(0.0, 0.0);
    let mut start = Point::new(0.0, 0.0);

    for &cmd in cmds {
        match cmd {
            PathCmd::MoveTo => {
                // 前のサブパスを確定する
                if let Some(sp_start) = subpath_start_idx.take() {
                    let len = workspace.flat_points.len() - sp_start;
                    if len >= 2 {
                        workspace.subpath_ranges.push(SubpathRange {
                            start: sp_start,
                            end: workspace.flat_points.len(),
                            closed: false,
                        });
                    }
                }
                let p = points[pt_idx];
                pt_idx += 1;
                start = p;
                cur = p;
                subpath_start_idx = Some(workspace.flat_points.len());
                workspace.flat_points.push(p);
            }
            PathCmd::LineTo => {
                let p = points[pt_idx];
                pt_idx += 1;
                if subpath_start_idx.is_some() {
                    workspace.flat_points.push(p);
                }
                cur = p;
            }
            PathCmd::CubicTo => {
                let cp1 = points[pt_idx];
                let cp2 = points[pt_idx + 1];
                let end = points[pt_idx + 2];
                pt_idx += 3;
                if subpath_start_idx.is_some() {
                    flatten_cubic_into(&mut workspace.flat_points, cur, cp1, cp2, end, 0);
                }
                cur = end;
            }
            PathCmd::QuadTo => {
                let cp = points[pt_idx];
                let end = points[pt_idx + 1];
                pt_idx += 2;
                if subpath_start_idx.is_some() {
                    flatten_quad_into(&mut workspace.flat_points, cur, cp, end, 0);
                }
                cur = end;
            }
            PathCmd::ConicTo => {
                let cp = points[pt_idx];
                let end = points[pt_idx + 1];
                pt_idx += 2;
                let w = conic_w[conic_idx];
                conic_idx += 1;
                if subpath_start_idx.is_some() {
                    flatten_conic_into(&mut workspace.flat_points, cur, cp, end, w);
                }
                cur = end;
            }
            PathCmd::Close => {
                if subpath_start_idx.is_some() && (cur.x != start.x || cur.y != start.y) {
                    workspace.flat_points.push(start);
                }
                cur = start;
                // 閉じたサブパスを確定する
                if let Some(sp_start) = subpath_start_idx.take() {
                    let len = workspace.flat_points.len() - sp_start;
                    if len >= 2 {
                        workspace.subpath_ranges.push(SubpathRange {
                            start: sp_start,
                            end: workspace.flat_points.len(),
                            closed: true,
                        });
                    }
                }
            }
        }
    }

    // 最後の開いたサブパスを確定する
    if let Some(sp_start) = subpath_start_idx.take() {
        let len = workspace.flat_points.len() - sp_start;
        if len >= 2 {
            workspace.subpath_ranges.push(SubpathRange {
                start: sp_start,
                end: workspace.flat_points.len(),
                closed: false,
            });
        }
    }
}

const FLATNESS_TOLERANCE: f64 = 0.25;
const MAX_RECURSION_DEPTH: u32 = 16;

/// 3 次ベジェ曲線を平坦化して points に追加する。
fn flatten_cubic_into(
    points: &mut Vec<Point>,
    p0: Point,
    p1: Point,
    p2: Point,
    p3: Point,
    depth: u32,
) {
    if depth >= MAX_RECURSION_DEPTH || is_flat_cubic(p0, p1, p2, p3) {
        points.push(p3);
        return;
    }

    let m01 = mid(p0, p1);
    let m12 = mid(p1, p2);
    let m23 = mid(p2, p3);
    let m012 = mid(m01, m12);
    let m123 = mid(m12, m23);
    let m0123 = mid(m012, m123);

    flatten_cubic_into(points, p0, m01, m012, m0123, depth + 1);
    flatten_cubic_into(points, m0123, m123, m23, p3, depth + 1);
}

/// 2 次ベジェ曲線を平坦化して points に追加する。
fn flatten_quad_into(points: &mut Vec<Point>, p0: Point, p1: Point, p2: Point, depth: u32) {
    if depth >= MAX_RECURSION_DEPTH || is_flat_quad(p0, p1, p2) {
        points.push(p2);
        return;
    }

    let m01 = mid(p0, p1);
    let m12 = mid(p1, p2);
    let m012 = mid(m01, m12);

    flatten_quad_into(points, p0, m01, m012, depth + 1);
    flatten_quad_into(points, m012, m12, p2, depth + 1);
}

/// 円錐曲線 (有理二次) を線分頂点列で近似する (EdgeBuilder と同じ分割数)。
fn flatten_conic_into(points: &mut Vec<Point>, p0: Point, p1: Point, p2: Point, w: f64) {
    const N: usize = 24;
    for i in 1..=N {
        let t = i as f64 / N as f64;
        points.push(eval_conic_stroke(p0, p1, p2, w, t));
    }
}

fn eval_conic_stroke(p0: Point, p1: Point, p2: Point, w: f64, t: f64) -> Point {
    let u = 1.0 - t;
    let denom = u * u + 2.0 * w * u * t + t * t;
    let x = (u * u * p0.x + 2.0 * w * u * t * p1.x + t * t * p2.x) / denom;
    let y = (u * u * p0.y + 2.0 * w * u * t * p1.y + t * t * p2.y) / denom;
    Point::new(x, y)
}

fn is_flat_cubic(p0: Point, p1: Point, p2: Point, p3: Point) -> bool {
    let dx = p3.x - p0.x;
    let dy = p3.y - p0.y;
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-12 {
        let d1_sq = (p1.x - p0.x).powi(2) + (p1.y - p0.y).powi(2);
        let d2_sq = (p2.x - p0.x).powi(2) + (p2.y - p0.y).powi(2);
        let tol_sq = FLATNESS_TOLERANCE * FLATNESS_TOLERANCE;
        return d1_sq <= tol_sq && d2_sq <= tol_sq;
    }

    let cross1 = (p1.x - p0.x) * dy - (p1.y - p0.y) * dx;
    let cross2 = (p2.x - p0.x) * dy - (p2.y - p0.y) * dx;
    let tol_sq = FLATNESS_TOLERANCE * FLATNESS_TOLERANCE * len_sq;

    cross1 * cross1 <= tol_sq && cross2 * cross2 <= tol_sq
}

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

fn mid(a: Point, b: Point) -> Point {
    Point::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5)
}

/// 線分の単位法線を計算する。ゼロ長の場合は (0, 0) を返す。
/// 2 回の除算を 1 回の除算 + 2 回の乗算に置換する。
fn unit_normal(p0: Point, p1: Point) -> (f64, f64) {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-12 {
        return (0.0, 0.0);
    }
    let inv_len = 1.0 / len;
    (-dy * inv_len, dx * inv_len)
}

/// サブパスをストローク輪郭に変換して output に書き込む。
/// seg_normals_buf は呼び出し間で再利用される。
#[allow(clippy::too_many_arguments)]
fn stroke_subpath_slice(
    output: &mut Path,
    pts: &[Point],
    closed: bool,
    half_width: f64,
    start_cap: StrokeCap,
    end_cap: StrokeCap,
    join: StrokeJoin,
    miter_limit: f64,
    seg_normals_buf: &mut Vec<(usize, f64, f64)>,
) {
    let n = pts.len();

    // ゼロ長の退化ケースを除去した有効な線分の法線を計算する
    seg_normals_buf.clear();
    for i in 0..n - 1 {
        let (nx, ny) = unit_normal(pts[i], pts[i + 1]);
        if nx == 0.0 && ny == 0.0 {
            continue;
        }
        seg_normals_buf.push((i, nx, ny));
    }

    if seg_normals_buf.is_empty() {
        return;
    }

    if closed {
        stroke_closed_subpath(output, pts, seg_normals_buf, half_width, join, miter_limit);
    } else {
        stroke_open_subpath(
            output,
            pts,
            seg_normals_buf,
            half_width,
            start_cap,
            end_cap,
            join,
            miter_limit,
        );
    }
}

/// 開いたサブパスのストローク輪郭を生成する。
/// 左輪郭 → end cap → 右輪郭 (逆順) → start cap → close で 1 つの閉じたコンターにする。
fn stroke_open_subpath(
    output: &mut Path,
    pts: &[Point],
    seg_normals: &[(usize, f64, f64)],
    half_width: f64,
    start_cap: StrokeCap,
    end_cap: StrokeCap,
    join: StrokeJoin,
    miter_limit: f64,
) {
    let first_seg = seg_normals[0];
    let last_seg = seg_normals[seg_normals.len() - 1];

    // 左輪郭 (順方向、+法線方向)
    let (_, n0x, n0y) = first_seg;
    let p_start = pts[first_seg.0];
    output.move_to(p_start.x + n0x * half_width, p_start.y + n0y * half_width);

    // 最初の線分の終点を左輪郭に追加
    let p_end = pts[first_seg.0 + 1];
    output.line_to(p_end.x + n0x * half_width, p_end.y + n0y * half_width);

    // 後続の線分: Join + 左輪郭ポイント
    for i in 1..seg_normals.len() {
        let (seg_idx, nx, ny) = seg_normals[i];
        let (_, prev_nx, prev_ny) = seg_normals[i - 1];
        let join_point = pts[seg_idx];

        emit_join(
            output,
            join,
            join_point,
            prev_nx,
            prev_ny,
            nx,
            ny,
            half_width,
            miter_limit,
            true,
        );

        let seg_end = pts[seg_idx + 1];
        output.line_to(seg_end.x + nx * half_width, seg_end.y + ny * half_width);
    }

    // End cap
    let (last_idx, last_nx, last_ny) = last_seg;
    let end_point = pts[last_idx + 1];
    emit_cap(
        output, end_cap, end_point, last_nx, last_ny, half_width, false,
    );

    // 右輪郭 (逆方向、-法線方向)
    output.line_to(
        end_point.x - last_nx * half_width,
        end_point.y - last_ny * half_width,
    );

    // 最後の線分の始点を右輪郭に追加
    let last_start = pts[last_idx];
    output.line_to(
        last_start.x - last_nx * half_width,
        last_start.y - last_ny * half_width,
    );

    // 逆方向に Join + 右輪郭ポイント
    for i in (0..seg_normals.len() - 1).rev() {
        let (seg_idx, nx, ny) = seg_normals[i];
        let (_, next_nx, next_ny) = seg_normals[i + 1];
        let join_point = pts[seg_idx + 1];

        // 逆方向なので法線を反転し、順序も逆になる
        emit_join(
            output,
            join,
            join_point,
            -next_nx,
            -next_ny,
            -nx,
            -ny,
            half_width,
            miter_limit,
            true,
        );

        let seg_start = pts[seg_idx];
        output.line_to(seg_start.x - nx * half_width, seg_start.y - ny * half_width);
    }

    // Start cap
    emit_cap(output, start_cap, p_start, n0x, n0y, half_width, true);

    output.close();
}

/// 閉じたサブパスのストローク輪郭を生成する。
/// 左輪郭と右輪郭を別々の閉じたコンターにする (NonZero fill rule で正しく描画される)。
fn stroke_closed_subpath(
    output: &mut Path,
    pts: &[Point],
    seg_normals: &[(usize, f64, f64)],
    half_width: f64,
    join: StrokeJoin,
    miter_limit: f64,
) {
    let num_segs = seg_normals.len();

    // 左輪郭 (外側、+法線方向)
    {
        let (first_idx, first_nx, first_ny) = seg_normals[0];
        let (_, last_nx, last_ny) = seg_normals[num_segs - 1];
        let first_point = pts[first_idx];

        // 最初の Join (最後の線分から最初の線分への接続)
        // まず仮の始点を置き、後で Join を処理する
        // 最後の線分→最初の線分の Join 点での左輪郭開始点を計算する
        let start_x = first_point.x + first_nx * half_width;
        let start_y = first_point.y + first_ny * half_width;
        output.move_to(start_x, start_y);

        // 最初の線分の終点
        let first_end = pts[first_idx + 1];
        output.line_to(
            first_end.x + first_nx * half_width,
            first_end.y + first_ny * half_width,
        );

        // 後続の線分
        for i in 1..num_segs {
            let (seg_idx, nx, ny) = seg_normals[i];
            let (_, prev_nx, prev_ny) = seg_normals[i - 1];
            let join_point = pts[seg_idx];

            emit_join(
                output,
                join,
                join_point,
                prev_nx,
                prev_ny,
                nx,
                ny,
                half_width,
                miter_limit,
                true,
            );

            let seg_end = pts[seg_idx + 1];
            output.line_to(seg_end.x + nx * half_width, seg_end.y + ny * half_width);
        }

        // 最後の Join: 最後の線分から最初の線分へ
        emit_join(
            output,
            join,
            first_point,
            last_nx,
            last_ny,
            first_nx,
            first_ny,
            half_width,
            miter_limit,
            true,
        );

        output.close();
    }

    // 右輪郭 (内側、-法線方向、逆回り)
    {
        let (first_idx, first_nx, first_ny) = seg_normals[0];
        let (_, last_nx, last_ny) = seg_normals[num_segs - 1];
        let first_point = pts[first_idx];

        let start_x = first_point.x - first_nx * half_width;
        let start_y = first_point.y - first_ny * half_width;
        output.move_to(start_x, start_y);

        // 逆方向: 最初の線分→最後の線分の Join
        emit_join(
            output,
            join,
            first_point,
            -first_nx,
            -first_ny,
            -last_nx,
            -last_ny,
            half_width,
            miter_limit,
            true,
        );

        // 最後の線分から逆順に
        for i in (1..num_segs).rev() {
            let (seg_idx, nx, ny) = seg_normals[i];
            let seg_start = pts[seg_idx];
            output.line_to(seg_start.x - nx * half_width, seg_start.y - ny * half_width);

            let (_, prev_nx, prev_ny) = seg_normals[i - 1];
            emit_join(
                output,
                join,
                seg_start,
                -nx,
                -ny,
                -prev_nx,
                -prev_ny,
                half_width,
                miter_limit,
                true,
            );
        }

        // 最初の線分の始点に戻る
        output.line_to(start_x, start_y);

        output.close();
    }
}

/// Join を出力する。
/// `left_side` が true なら左輪郭側 (+法線方向)。
fn emit_join(
    output: &mut Path,
    join: StrokeJoin,
    p: Point,
    prev_nx: f64,
    prev_ny: f64,
    next_nx: f64,
    next_ny: f64,
    half_width: f64,
    miter_limit: f64,
    left_side: bool,
) {
    let _ = left_side;

    // 前の法線と次の法線の外積で曲がる方向を判定する
    let cross = prev_nx * next_ny - prev_ny * next_nx;

    // 法線がほぼ同じ方向 (直線): Join 不要
    if cross.abs() < 1e-10 {
        output.line_to(p.x + next_nx * half_width, p.y + next_ny * half_width);
        return;
    }

    // 外側の Join を出力する
    // cross > 0: 左に曲がる (左輪郭が外側)
    // cross < 0: 右に曲がる (右輪郭が外側)
    // ここでは呼び出し側が法線の符号を制御しているので、常に「この輪郭側」の Join を出力する

    // マイター長を計算する (MiterClip / MiterBevel / MiterRound で共通)
    let dot = prev_nx * next_nx + prev_ny * next_ny;
    let sin_half_sq = (1.0 - dot) * 0.5;

    match join {
        StrokeJoin::Bevel => {
            // 前の線分の終点と次の線分の始点を直線で接続する (何もしない場合と同じ)
            output.line_to(p.x + next_nx * half_width, p.y + next_ny * half_width);
        }
        StrokeJoin::MiterClip | StrokeJoin::MiterBevel | StrokeJoin::MiterRound => {
            if sin_half_sq < 1e-12 {
                // ほぼ平行: Bevel
                output.line_to(p.x + next_nx * half_width, p.y + next_ny * half_width);
                return;
            }
            let miter_len = 1.0 / sin_half_sq.sqrt();

            if miter_len <= miter_limit {
                // マイター交点を計算する
                // 交点 = p + (prev_n + next_n).normalized() * half_width * miter_len
                let mx = prev_nx + next_nx;
                let my = prev_ny + next_ny;
                let m_len = (mx * mx + my * my).sqrt();
                if m_len > 1e-12 {
                    let scale = half_width * miter_len / m_len;
                    output.line_to(p.x + mx * scale, p.y + my * scale);
                }
                output.line_to(p.x + next_nx * half_width, p.y + next_ny * half_width);
            } else {
                // miter limit 超過時のフォールバック
                match join {
                    StrokeJoin::MiterClip => {
                        emit_miter_clip(
                            output,
                            p,
                            prev_nx,
                            prev_ny,
                            next_nx,
                            next_ny,
                            half_width,
                            miter_limit,
                        );
                        output.line_to(p.x + next_nx * half_width, p.y + next_ny * half_width);
                    }
                    StrokeJoin::MiterBevel => {
                        // Bevel にフォールバック
                        output.line_to(p.x + next_nx * half_width, p.y + next_ny * half_width);
                    }
                    StrokeJoin::MiterRound => {
                        // Round にフォールバック
                        emit_arc(output, p, prev_nx, prev_ny, next_nx, next_ny, half_width);
                        output.line_to(p.x + next_nx * half_width, p.y + next_ny * half_width);
                    }
                    _ => unreachable!(),
                }
            }
        }
        StrokeJoin::Round => {
            // 前の法線から次の法線まで円弧を cubic bezier で近似する
            emit_arc(output, p, prev_nx, prev_ny, next_nx, next_ny, half_width);
            output.line_to(p.x + next_nx * half_width, p.y + next_ny * half_width);
        }
    }
}

/// MiterClip のフォールバック: マイター限界距離で切り落とす。
///
/// マイター方向の二等分線に垂直な面でマイター三角形を切断し、
/// 2 つの切断点を出力する。
fn emit_miter_clip(
    output: &mut Path,
    p: Point,
    prev_nx: f64,
    prev_ny: f64,
    next_nx: f64,
    next_ny: f64,
    half_width: f64,
    miter_limit: f64,
) {
    // マイター方向 (二等分線) を正規化する
    let mx = prev_nx + next_nx;
    let my = prev_ny + next_ny;
    let m_len = (mx * mx + my * my).sqrt();
    if m_len < 1e-12 {
        // 正反対の法線 (180 度折り返し): Bevel にフォールバック
        return;
    }
    let mdx = mx / m_len;
    let mdy = my / m_len;

    // 切断距離と切断面の中心
    let clip_dist = miter_limit * half_width;
    let clip_cx = p.x + mdx * clip_dist;
    let clip_cy = p.y + mdy * clip_dist;

    // 切断面の法線に垂直な方向 (切断面に沿った方向)
    let perp_x = -mdy;
    let perp_y = mdx;

    // 前の法線のオフセット点から切断面への射影
    // v = (prev_n * hw) - (miter_dir * clip_dist) (p からの相対)
    let v1x = prev_nx * half_width - mdx * clip_dist;
    let v1y = prev_ny * half_width - mdy * clip_dist;
    let proj1 = v1x * perp_x + v1y * perp_y;

    // 次の法線のオフセット点から切断面への射影
    let v2x = next_nx * half_width - mdx * clip_dist;
    let v2y = next_ny * half_width - mdy * clip_dist;
    let proj2 = v2x * perp_x + v2y * perp_y;

    // 2 つの切断点を出力する
    output.line_to(clip_cx + perp_x * proj1, clip_cy + perp_y * proj1);
    output.line_to(clip_cx + perp_x * proj2, clip_cy + perp_y * proj2);
}

/// Cap を出力する。
fn emit_cap(
    output: &mut Path,
    cap: StrokeCap,
    p: Point,
    nx: f64,
    ny: f64,
    half_width: f64,
    is_start: bool,
) {
    // 接線方向 (法線の垂直方向)
    // 法線 (-dy, dx) の垂直 = (dx, dy) = 進行方向
    // しかし法線 (nx, ny) = (-dy/len, dx/len) なので
    // 接線 = (ny, -nx) が進行方向、(-ny, nx) が逆方向
    let (tx, ty) = if is_start {
        // 始点キャップ: 進行方向と逆 (-ny, nx) → 実際は (ny, -nx) の逆
        (-ny, nx)
    } else {
        // 終点キャップ: 進行方向 (ny, -nx)
        (ny, -nx)
    };

    match cap {
        StrokeCap::Butt => {
            // 何もしない: 左輪郭端と右輪郭端を直線で接続するだけ
        }
        StrokeCap::Square => {
            // 接線方向に half_width だけ延長した矩形
            let left_x = p.x + nx * half_width;
            let left_y = p.y + ny * half_width;
            let right_x = p.x - nx * half_width;
            let right_y = p.y - ny * half_width;

            output.line_to(left_x + tx * half_width, left_y + ty * half_width);
            output.line_to(right_x + tx * half_width, right_y + ty * half_width);
        }
        StrokeCap::Round => {
            // 半円を 2 つの cubic bezier で近似する
            // 法線方向から反対の法線方向まで
            if is_start {
                // -n → n 方向の半円 (start cap: 右→左)
                emit_semicircle(output, p, -nx, -ny, nx, ny, half_width);
            } else {
                // n → -n 方向の半円 (end cap: 左→右)
                emit_semicircle(output, p, nx, ny, -nx, -ny, half_width);
            }
        }
    }
}

/// 半円を 2 つの cubic bezier で近似して出力する。
/// d0 方向から d1 方向まで (180 度の円弧)。
fn emit_semicircle(
    output: &mut Path,
    center: Point,
    d0x: f64,
    d0y: f64,
    d1x: f64,
    d1y: f64,
    radius: f64,
) {
    // 中間方向を計算する (d0 と d1 の中間、90 度回転)
    // d0 → mid (90 度) → d1 (180 度)
    // d0 の 90 度回転 = d0 を垂直方向に
    let mid_x = (d0y + d1y) * 0.5;
    let mid_y = (-d0x - d1x) * 0.5;
    // 正規化
    let mid_len = (mid_x * mid_x + mid_y * mid_y).sqrt();
    let (mid_x, mid_y) = if mid_len > 1e-12 {
        (mid_x / mid_len, mid_y / mid_len)
    } else {
        // d0 と d1 が正反対の場合、垂直方向を使う
        (d0y, -d0x)
    };

    // 90 度の円弧の KAPPA 係数
    const ARC_KAPPA: f64 = 0.552_284_749_831;

    // 第 1 弧: d0 → mid
    let p0x = center.x + d0x * radius;
    let p0y = center.y + d0y * radius;
    let p3x = center.x + mid_x * radius;
    let p3y = center.y + mid_y * radius;

    // 制御点: p0 の接線方向に kappa、p3 の接線方向に kappa
    // d0 の接線方向 (進む方向) = 反時計回りで d0 を 90 度回転
    let t0x = -d0y;
    let t0y = d0x;
    // d0 → mid の方向に合わせて接線の符号を決める
    let sign0 = if t0x * (mid_x - d0x) + t0y * (mid_y - d0y) >= 0.0 {
        1.0
    } else {
        -1.0
    };

    let t1x = -mid_y;
    let t1y = mid_x;
    let sign1 = if t1x * (d0x - mid_x) + t1y * (d0y - mid_y) >= 0.0 {
        1.0
    } else {
        -1.0
    };

    output.cubic_to(
        p0x + sign0 * t0x * radius * ARC_KAPPA,
        p0y + sign0 * t0y * radius * ARC_KAPPA,
        p3x + sign1 * t1x * radius * ARC_KAPPA,
        p3y + sign1 * t1y * radius * ARC_KAPPA,
        p3x,
        p3y,
    );

    // 第 2 弧: mid → d1
    let q0x = p3x;
    let q0y = p3y;
    let q3x = center.x + d1x * radius;
    let q3y = center.y + d1y * radius;

    let t2x = -mid_y;
    let t2y = mid_x;
    let sign2 = if t2x * (d1x - mid_x) + t2y * (d1y - mid_y) >= 0.0 {
        1.0
    } else {
        -1.0
    };

    let t3x = -d1y;
    let t3y = d1x;
    let sign3 = if t3x * (mid_x - d1x) + t3y * (mid_y - d1y) >= 0.0 {
        1.0
    } else {
        -1.0
    };

    output.cubic_to(
        q0x + sign2 * t2x * radius * ARC_KAPPA,
        q0y + sign2 * t2y * radius * ARC_KAPPA,
        q3x + sign3 * t3x * radius * ARC_KAPPA,
        q3y + sign3 * t3y * radius * ARC_KAPPA,
        q3x,
        q3y,
    );
}

/// 2 つの法線間の円弧を cubic bezier で近似して出力する。
fn emit_arc(output: &mut Path, center: Point, n0x: f64, n0y: f64, n1x: f64, n1y: f64, radius: f64) {
    // 2 つの法線間の角度を計算する
    let dot = n0x * n1x + n0y * n1y;
    let angle = dot.clamp(-1.0, 1.0).acos();

    if angle < 1e-6 {
        return;
    }

    // 角度が 90 度以下なら 1 つの cubic bezier で近似する
    // 90 度超なら 2 つに分割する
    if angle <= std::f64::consts::FRAC_PI_2 + 0.01 {
        emit_arc_segment(output, center, n0x, n0y, n1x, n1y, radius);
    } else {
        // 中間法線を計算する
        let mx = n0x + n1x;
        let my = n0y + n1y;
        let m_len = (mx * mx + my * my).sqrt();
        if m_len < 1e-12 {
            return;
        }
        let mx = mx / m_len;
        let my = my / m_len;

        emit_arc_segment(output, center, n0x, n0y, mx, my, radius);
        emit_arc_segment(output, center, mx, my, n1x, n1y, radius);
    }
}

/// 1 つの円弧セグメントを cubic bezier で出力する。
fn emit_arc_segment(
    output: &mut Path,
    center: Point,
    n0x: f64,
    n0y: f64,
    n1x: f64,
    n1y: f64,
    radius: f64,
) {
    let dot = (n0x * n1x + n0y * n1y).clamp(-1.0, 1.0);
    let angle = dot.acos();
    if angle < 1e-6 {
        return;
    }

    // 円弧の cubic bezier 近似の制御点距離
    // k = (4/3) * tan(angle/4)
    let k = (4.0 / 3.0) * (angle / 4.0).tan();

    let p0x = center.x + n0x * radius;
    let p0y = center.y + n0y * radius;
    let p3x = center.x + n1x * radius;
    let p3y = center.y + n1y * radius;

    // p0 の接線方向 (法線の垂直方向、円弧が進む向き)
    let cross = n0x * n1y - n0y * n1x;
    let sign = if cross >= 0.0 { 1.0 } else { -1.0 };

    let t0x = -n0y * sign;
    let t0y = n0x * sign;
    let t1x = n1y * sign;
    let t1y = -n1x * sign;

    output.cubic_to(
        p0x + t0x * radius * k,
        p0y + t0y * radius * k,
        p3x + t1x * radius * k,
        p3y + t1y * radius * k,
        p3x,
        p3y,
    );
}
