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

    /// すべての頂点を `(dx, dy)` だけ平行移動する。`cur` / `sub_start` も同期する。
    pub fn translate(&mut self, dx: f64, dy: f64) {
        for p in self.points.iter_mut() {
            p.x += dx;
            p.y += dy;
        }
        self.cur.x += dx;
        self.cur.y += dy;
        self.sub_start.x += dx;
        self.sub_start.y += dy;
        if let Some(ref mut p) = self.last_quad_cp {
            p.x += dx;
            p.y += dy;
        }
        if let Some(ref mut p) = self.last_cubic_cp2 {
            p.x += dx;
            p.y += dy;
        }
    }

    /// すべての頂点に行列を適用する。コニックの重み (`w`) はアフィン変換で不変なので変更しない。
    pub fn transform(&mut self, m: &crate::api::matrix::Matrix2D) {
        for p in self.points.iter_mut() {
            let (x, y) = m.map_point(p.x, p.y);
            p.x = x;
            p.y = y;
        }
        let (cx, cy) = m.map_point(self.cur.x, self.cur.y);
        self.cur = Point::new(cx, cy);
        let (sx, sy) = m.map_point(self.sub_start.x, self.sub_start.y);
        self.sub_start = Point::new(sx, sy);
        if let Some(p) = self.last_quad_cp {
            let (x, y) = m.map_point(p.x, p.y);
            self.last_quad_cp = Some(Point::new(x, y));
        }
        if let Some(p) = self.last_cubic_cp2 {
            let (x, y) = m.map_point(p.x, p.y);
            self.last_cubic_cp2 = Some(Point::new(x, y));
        }
    }

    /// 別の Path のコマンド/頂点列をこの Path に追加する。
    ///
    /// 内部状態 (`cur` / `sub_start` / `last_*`) も自然に更新されるよう、コマンドを 1 つずつ
    /// replay する実装を取る。
    pub fn add_path(&mut self, other: &Path) {
        if other.is_empty() {
            return;
        }
        let mut pi = 0usize;
        let mut wi = 0usize;
        for &cmd in &other.cmds {
            match cmd {
                PathCmd::MoveTo => {
                    let p = other.points[pi];
                    pi += 1;
                    self.move_to(p.x, p.y);
                }
                PathCmd::LineTo => {
                    let p = other.points[pi];
                    pi += 1;
                    self.line_to(p.x, p.y);
                }
                PathCmd::QuadTo => {
                    let cp = other.points[pi];
                    let end = other.points[pi + 1];
                    pi += 2;
                    self.quad_to(cp.x, cp.y, end.x, end.y);
                }
                PathCmd::ConicTo => {
                    let cp = other.points[pi];
                    let end = other.points[pi + 1];
                    pi += 2;
                    let w = other.conic_weights[wi];
                    wi += 1;
                    self.conic_to(cp.x, cp.y, end.x, end.y, w);
                }
                PathCmd::CubicTo => {
                    let cp1 = other.points[pi];
                    let cp2 = other.points[pi + 1];
                    let end = other.points[pi + 2];
                    pi += 3;
                    self.cubic_to(cp1.x, cp1.y, cp2.x, cp2.y, end.x, end.y);
                }
                PathCmd::Close => {
                    self.close();
                }
            }
        }
    }

    /// 別の Path を平行移動して追加する。
    pub fn add_path_translated(&mut self, other: &Path, dx: f64, dy: f64) {
        let mut pi = 0usize;
        let mut wi = 0usize;
        for &cmd in &other.cmds {
            match cmd {
                PathCmd::MoveTo => {
                    let p = other.points[pi];
                    pi += 1;
                    self.move_to(p.x + dx, p.y + dy);
                }
                PathCmd::LineTo => {
                    let p = other.points[pi];
                    pi += 1;
                    self.line_to(p.x + dx, p.y + dy);
                }
                PathCmd::QuadTo => {
                    let cp = other.points[pi];
                    let end = other.points[pi + 1];
                    pi += 2;
                    self.quad_to(cp.x + dx, cp.y + dy, end.x + dx, end.y + dy);
                }
                PathCmd::ConicTo => {
                    let cp = other.points[pi];
                    let end = other.points[pi + 1];
                    pi += 2;
                    let w = other.conic_weights[wi];
                    wi += 1;
                    self.conic_to(cp.x + dx, cp.y + dy, end.x + dx, end.y + dy, w);
                }
                PathCmd::CubicTo => {
                    let cp1 = other.points[pi];
                    let cp2 = other.points[pi + 1];
                    let end = other.points[pi + 2];
                    pi += 3;
                    self.cubic_to(
                        cp1.x + dx,
                        cp1.y + dy,
                        cp2.x + dx,
                        cp2.y + dy,
                        end.x + dx,
                        end.y + dy,
                    );
                }
                PathCmd::Close => {
                    self.close();
                }
            }
        }
    }

    /// 別の Path に行列を適用して追加する。
    pub fn add_path_transformed(&mut self, other: &Path, m: &crate::api::matrix::Matrix2D) {
        let mut pi = 0usize;
        let mut wi = 0usize;
        let map = |p: Point| {
            let (x, y) = m.map_point(p.x, p.y);
            (x, y)
        };
        for &cmd in &other.cmds {
            match cmd {
                PathCmd::MoveTo => {
                    let (x, y) = map(other.points[pi]);
                    pi += 1;
                    self.move_to(x, y);
                }
                PathCmd::LineTo => {
                    let (x, y) = map(other.points[pi]);
                    pi += 1;
                    self.line_to(x, y);
                }
                PathCmd::QuadTo => {
                    let (cx, cy) = map(other.points[pi]);
                    let (ex, ey) = map(other.points[pi + 1]);
                    pi += 2;
                    self.quad_to(cx, cy, ex, ey);
                }
                PathCmd::ConicTo => {
                    let (cx, cy) = map(other.points[pi]);
                    let (ex, ey) = map(other.points[pi + 1]);
                    pi += 2;
                    let w = other.conic_weights[wi];
                    wi += 1;
                    self.conic_to(cx, cy, ex, ey, w);
                }
                PathCmd::CubicTo => {
                    let (c1x, c1y) = map(other.points[pi]);
                    let (c2x, c2y) = map(other.points[pi + 1]);
                    let (ex, ey) = map(other.points[pi + 2]);
                    pi += 3;
                    self.cubic_to(c1x, c1y, c2x, c2y, ex, ey);
                }
                PathCmd::Close => {
                    self.close();
                }
            }
        }
    }

    /// すべての頂点を含む軸並行バウンディングボックスを返す。空パスは `None`。
    ///
    /// 注意: これは制御点を含む `control_box` に相当する (Blend2D の `get_control_box`)。
    /// 厳密な曲線のバウンディングボックスではないため、ベジェ制御点が外側に出る場合は
    /// 実際のラスタライズ範囲よりも広めの矩形が返る。
    pub fn control_box(&self) -> Option<crate::api::context::Rect> {
        if self.points.is_empty() {
            return None;
        }
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for p in &self.points {
            if p.x < min_x {
                min_x = p.x;
            }
            if p.y < min_y {
                min_y = p.y;
            }
            if p.x > max_x {
                max_x = p.x;
            }
            if p.y > max_y {
                max_y = p.y;
            }
        }
        Some(crate::api::context::Rect::new(
            min_x,
            min_y,
            max_x - min_x,
            max_y - min_y,
        ))
    }

    /// `control_box` のエイリアス (Blend2D の `get_bounding_box` 相当だが、現状は制御点ベース)。
    pub fn bounding_box(&self) -> Option<crate::api::context::Rect> {
        self.control_box()
    }

    /// 三角形をパスに追加する (3 頂点を結んで閉じる)。
    pub fn add_triangle(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64) {
        self.move_to(x0, y0);
        self.line_to(x1, y1);
        self.line_to(x2, y2);
        self.close();
    }

    /// 点列を結んで閉じたポリゴンを追加する。点が 2 つ以下の場合は何もしない。
    pub fn add_polygon(&mut self, points: &[Point]) {
        if points.len() < 3 {
            return;
        }
        self.move_to(points[0].x, points[0].y);
        for p in &points[1..] {
            self.line_to(p.x, p.y);
        }
        self.close();
    }

    /// 点列を結んだ折れ線 (polyline) を追加する。閉じない。
    pub fn add_polyline(&mut self, points: &[Point]) {
        if points.len() < 2 {
            return;
        }
        self.move_to(points[0].x, points[0].y);
        for p in &points[1..] {
            self.line_to(p.x, p.y);
        }
    }

    /// 角丸矩形をパスに追加する。
    ///
    /// 角の半径 `rx` / `ry` は幅・高さの半分でクランプされる (Blend2D と同様)。
    /// 半径が 0 以下の場合は通常の矩形 (4 本の line_to + close) を追加する。
    pub fn add_round_rect(&mut self, x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let rx = rx.max(0.0).min(w * 0.5);
        let ry = ry.max(0.0).min(h * 0.5);
        if rx == 0.0 || ry == 0.0 {
            self.move_to(x, y);
            self.line_to(x + w, y);
            self.line_to(x + w, y + h);
            self.line_to(x, y + h);
            self.close();
            return;
        }
        let kx = rx * KAPPA;
        let ky = ry * KAPPA;
        let x1 = x + w;
        let y1 = y + h;

        // 上辺 (左上の弧の終点 → 右上の弧の始点)
        self.move_to(x + rx, y);
        self.line_to(x1 - rx, y);
        // 右上の角
        self.cubic_to(x1 - rx + kx, y, x1, y + ry - ky, x1, y + ry);
        // 右辺
        self.line_to(x1, y1 - ry);
        // 右下の角
        self.cubic_to(x1, y1 - ry + ky, x1 - rx + kx, y1, x1 - rx, y1);
        // 下辺
        self.line_to(x + rx, y1);
        // 左下の角
        self.cubic_to(x + rx - kx, y1, x, y1 - ry + ky, x, y1 - ry);
        // 左辺
        self.line_to(x, y + ry);
        // 左上の角
        self.cubic_to(x, y + ry - ky, x + rx - kx, y, x + rx, y);
        self.close();
    }

    /// 楕円を 4 本の cubic Bezier で近似して追加する。`add_circle` の `rx == ry` 版と一致する。
    pub fn add_ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64) {
        let kx = rx * KAPPA;
        let ky = ry * KAPPA;

        self.move_to(cx + rx, cy);
        self.cubic_to(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry);
        self.cubic_to(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy);
        self.cubic_to(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry);
        self.cubic_to(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy);
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
