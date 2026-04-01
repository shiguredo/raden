/// 2D の点。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

/// パスコマンドの種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PathCmd {
    MoveTo = 0,
    LineTo = 1,
    /// 2 点消費 (cp, end)。
    QuadTo = 2,
    /// 有理二次ベジェ (円錐曲線)。2 点消費 (制御点, 終点) + `conic_weights` の対応する重み。
    ConicTo = 3,
    /// 3 点消費 (cp1, cp2, end)。
    CubicTo = 4,
    Close = 5,
}

/// 2D パス。コマンド列と点列で構成される。
pub struct Path {
    cmds: Vec<PathCmd>,
    points: Vec<Point>,
    /// `PathCmd::ConicTo` ごとの重み w (>= 0)。
    conic_weights: Vec<f64>,
    /// 現在点 (最後の `move_to` / `line_to` 等の終点)。
    cur: Point,
    /// 現在のサブパスの開始点。
    sub_start: Point,
    /// `smooth_quad_to` 用: 直前の二次ベジェの制御点。
    last_quad_cp: Option<Point>,
    /// `smooth_cubic_to` 用: 直前の三次ベジェの第 2 制御点。
    last_cubic_cp2: Option<Point>,
}

/// Blend2D と同一の KAPPA 定数。円を 4 本の cubic Bezier で近似する係数。
const KAPPA: f64 = 0.552_284_749_831;

impl Path {
    pub fn new() -> Self {
        Self {
            cmds: Vec::new(),
            points: Vec::new(),
            conic_weights: Vec::new(),
            cur: Point::new(0.0, 0.0),
            sub_start: Point::new(0.0, 0.0),
            last_quad_cp: None,
            last_cubic_cp2: None,
        }
    }

    /// `PathCmd::ConicTo` に対応する重み列 (読み取り専用)。
    pub fn conic_weights(&self) -> &[f64] {
        &self.conic_weights
    }

    pub fn clear(&mut self) {
        self.cmds.clear();
        self.points.clear();
        self.conic_weights.clear();
        self.last_quad_cp = None;
        self.last_cubic_cp2 = None;
    }

    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }

    /// コマンド数を返す。
    pub fn len(&self) -> usize {
        self.cmds.len()
    }

    pub fn cmds(&self) -> &[PathCmd] {
        &self.cmds
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    pub fn move_to(&mut self, x: f64, y: f64) {
        self.cmds.push(PathCmd::MoveTo);
        self.points.push(Point::new(x, y));
        let p = Point::new(x, y);
        self.cur = p;
        self.sub_start = p;
        self.last_quad_cp = None;
        self.last_cubic_cp2 = None;
    }

    pub fn line_to(&mut self, x: f64, y: f64) {
        self.cmds.push(PathCmd::LineTo);
        let p = Point::new(x, y);
        self.points.push(p);
        self.cur = p;
        self.last_quad_cp = None;
        self.last_cubic_cp2 = None;
    }

    /// 3 次ベジェ曲線を追加する。cp1, cp2 は制御点、x/y は終点。
    pub fn cubic_to(&mut self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64) {
        self.cmds.push(PathCmd::CubicTo);
        self.points.push(Point::new(cp1x, cp1y));
        self.points.push(Point::new(cp2x, cp2y));
        let end = Point::new(x, y);
        self.points.push(end);
        self.last_cubic_cp2 = Some(Point::new(cp2x, cp2y));
        self.last_quad_cp = None;
        self.cur = end;
    }

    /// 2 次ベジェ曲線を追加する。cp は制御点、x/y は終点。
    pub fn quad_to(&mut self, cpx: f64, cpy: f64, x: f64, y: f64) {
        self.cmds.push(PathCmd::QuadTo);
        let cp = Point::new(cpx, cpy);
        self.points.push(cp);
        let end = Point::new(x, y);
        self.points.push(end);
        self.last_quad_cp = Some(cp);
        self.last_cubic_cp2 = None;
        self.cur = end;
    }

    /// 前の二次ベジェの制御点を反射したスムーズ二次ベジェ。直前が `quad_to` でない場合はパニックする。
    pub fn smooth_quad_to(&mut self, x: f64, y: f64) {
        let cp = self
            .last_quad_cp
            .expect("smooth_quad_to requires a preceding quad_to");
        let cpx = 2.0 * self.cur.x - cp.x;
        let cpy = 2.0 * self.cur.y - cp.y;
        self.quad_to(cpx, cpy, x, y);
    }

    /// 前の三次ベジェの第 2 制御点を反射したスムーズ三次ベジェ。直前が `cubic_to` でない場合はパニックする。
    pub fn smooth_cubic_to(&mut self, cp2x: f64, cp2y: f64, x: f64, y: f64) {
        let cp2_prev = self
            .last_cubic_cp2
            .expect("smooth_cubic_to requires a preceding cubic_to");
        let cp1x = 2.0 * self.cur.x - cp2_prev.x;
        let cp1y = 2.0 * self.cur.y - cp2_prev.y;
        self.cubic_to(cp1x, cp1y, cp2x, cp2y, x, y);
    }

    /// 円錐曲線 (有理二次ベジェ)。重み w > 0。
    pub fn conic_to(&mut self, cx: f64, cy: f64, ex: f64, ey: f64, w: f64) {
        assert!(w > 0.0, "conic weight must be positive");
        self.cmds.push(PathCmd::ConicTo);
        self.points.push(Point::new(cx, cy));
        let end = Point::new(ex, ey);
        self.points.push(end);
        self.conic_weights.push(w);
        self.last_quad_cp = None;
        self.last_cubic_cp2 = None;
        self.cur = end;
    }

    /// 中心 (cx, cy)、半径 rx, ry、開始角・掃引角 (ラジアン) の楕円弧を現在点から接続する。
    /// `force_move_to` が true のとき弧の始点へ `move_to`、false のとき `line_to` で接続する。
    pub fn arc_to(
        &mut self,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        start: f64,
        sweep: f64,
        force_move_to: bool,
    ) {
        if rx <= 0.0 || ry <= 0.0 || sweep == 0.0 {
            return;
        }
        let sx = cx + rx * start.cos();
        let sy = cy + ry * start.sin();
        if force_move_to {
            self.move_to(sx, sy);
        } else {
            self.line_to(sx, sy);
        }
        add_arc_segments(self, cx, cy, rx, ry, start, sweep);
    }

    pub fn close(&mut self) {
        self.cmds.push(PathCmd::Close);
        self.cur = self.sub_start;
        self.last_quad_cp = None;
        self.last_cubic_cp2 = None;
    }

    /// 扇形 (pie) を追加する。中心→弧→中心の閉じたパス。
    ///
    /// - `(cx, cy)`: 中心
    /// - `(rx, ry)`: X/Y 方向の半径
    /// - `start`: 開始角度 (ラジアン)
    /// - `sweep`: 掃引角度 (ラジアン)
    pub fn add_pie(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, start: f64, sweep: f64) {
        if rx <= 0.0 || ry <= 0.0 || sweep == 0.0 {
            return;
        }

        // 中心から開始点への線
        let start_x = cx + rx * start.cos();
        let start_y = cy + ry * start.sin();
        self.move_to(cx, cy);
        self.line_to(start_x, start_y);

        // 円弧を cubic Bézier セグメントで近似して追加
        add_arc_segments(self, cx, cy, rx, ry, start, sweep);

        self.close();
    }

    /// 円を 4 本の cubic Bezier で近似して追加する。Blend2D と同一のアルゴリズム。
    pub fn add_circle(&mut self, cx: f64, cy: f64, r: f64) {
        let kx = r * KAPPA;
        let ky = r * KAPPA;

        // 右端 (cx+r, cy) から反時計回り
        self.move_to(cx + r, cy);

        // 右端 → 下端
        self.cubic_to(cx + r, cy + ky, cx + kx, cy + r, cx, cy + r);

        // 下端 → 左端
        self.cubic_to(cx - kx, cy + r, cx - r, cy + ky, cx - r, cy);

        // 左端 → 上端
        self.cubic_to(cx - r, cy - ky, cx - kx, cy - r, cx, cy - r);

        // 上端 → 右端
        self.cubic_to(cx + kx, cy - r, cx + r, cy - ky, cx + r, cy);

        self.close();
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

/// 円弧を cubic Bézier セグメントで近似して Path に追加する。
///
/// sweep を π/2 以下のセグメントに分割し、各セグメントを 1 本の cubic Bézier で近似する。
/// 制御点の算出は標準的な端点パラメータ化に基づく。
fn add_arc_segments(path: &mut Path, cx: f64, cy: f64, rx: f64, ry: f64, start: f64, sweep: f64) {
    let half_pi = std::f64::consts::FRAC_PI_2;
    let abs_sweep = sweep.abs();
    // π/2 以下に分割するセグメント数
    let n = (abs_sweep / half_pi).ceil() as usize;
    let n = n.max(1);
    let segment_sweep = sweep / n as f64;

    let mut angle = start;
    for _ in 0..n {
        add_arc_cubic(path, cx, cy, rx, ry, angle, segment_sweep);
        angle += segment_sweep;
    }
}

/// 1 セグメント (|sweep| ≤ π/2) の円弧を 1 本の cubic Bézier で近似する。
fn add_arc_cubic(path: &mut Path, cx: f64, cy: f64, rx: f64, ry: f64, start: f64, sweep: f64) {
    // k = (4/3) * tan(sweep/4) — 標準的な円弧近似係数
    let k = (4.0 / 3.0) * (sweep / 4.0).tan();

    let (sin0, cos0) = start.sin_cos();
    let end = start + sweep;
    let (sin1, cos1) = end.sin_cos();

    // 制御点 1: 開始点の接線方向
    let cp1x = cx + rx * (cos0 - k * sin0);
    let cp1y = cy + ry * (sin0 + k * cos0);

    // 制御点 2: 終点の接線方向 (逆向き)
    let cp2x = cx + rx * (cos1 + k * sin1);
    let cp2y = cy + ry * (sin1 - k * cos1);

    // 終点
    let ex = cx + rx * cos1;
    let ey = cy + ry * sin1;

    path.cubic_to(cp1x, cp1y, cp2x, cp2y, ex, ey);
}
