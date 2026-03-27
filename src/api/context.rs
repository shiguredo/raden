use crate::api::gradient::Gradient;
use crate::api::image::Image;
use crate::api::matrix::Matrix2D;
use crate::api::path::Path;
use crate::api::stroke::{StrokeOptions, StrokeWorkspace, stroke_to_fill_with_workspace};
use crate::api::style::{CompOp, FillRule, Rgba32, StrokeCap, StrokeJoin};
use crate::font::Font;
use crate::pipeline::key::{FetchType, FillType};
use crate::pipeline::runtime::PipelineRuntime;
use crate::raster::analytic::AnalyticRasterizer;
use crate::raster::edge_builder::EdgeBuilder;

/// 浮動小数点の矩形。(x, y) は左上、(w, h) はサイズ。
#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
}

/// 円。(cx, cy) は中心、r は半径。
#[derive(Debug, Clone, Copy)]
pub struct Circle {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
}

impl Circle {
    pub fn new(cx: f64, cy: f64, r: f64) -> Self {
        Self { cx, cy, r }
    }
}

/// 線分。(x0, y0) から (x1, y1) への線分。
#[derive(Debug, Clone, Copy)]
pub struct Line {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Line {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self { x0, y0, x1, y1 }
    }
}

/// 円弧。Blend2D の `BLArc` と同一レイアウト。
///
/// - `(cx, cy)`: 中心
/// - `(rx, ry)`: X/Y 方向の半径 (楕円弧対応)
/// - `start`: 開始角度 (ラジアン、3 時方向が 0、時計回りが正)
/// - `sweep`: 掃引角度 (ラジアン、正が時計回り)
#[derive(Debug, Clone, Copy)]
pub struct Arc {
    pub cx: f64,
    pub cy: f64,
    pub rx: f64,
    pub ry: f64,
    pub start: f64,
    pub sweep: f64,
}

impl Arc {
    pub fn new(cx: f64, cy: f64, rx: f64, ry: f64, start: f64, sweep: f64) -> Self {
        Self {
            cx,
            cy,
            rx,
            ry,
            start,
            sweep,
        }
    }
}

/// 整数の矩形 (クリッピング済み)。
struct BoxI {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

/// save() / restore() で保存・復元される描画状態。
#[allow(dead_code)]
struct ContextState {
    comp_op: CompOp,
    fill_rule: FillRule,
    fill_color_prgb32: u32,
    fill_gradient: Option<Gradient>,
    stroke_color_prgb32: u32,
    stroke_width: f64,
    stroke_start_cap: StrokeCap,
    stroke_end_cap: StrokeCap,
    stroke_join: StrokeJoin,
    stroke_miter_limit: f64,
    matrix: Matrix2D,
}

pub struct Context<'a> {
    image: &'a mut Image,
    runtime: &'a mut PipelineRuntime,
    comp_op: CompOp,
    fill_rule: FillRule,
    fill_color_prgb32: u32,
    fill_gradient: Option<Gradient>,
    stroke_color_prgb32: u32,
    stroke_width: f64,
    stroke_start_cap: StrokeCap,
    stroke_end_cap: StrokeCap,
    stroke_join: StrokeJoin,
    stroke_miter_limit: f64,
    matrix: Matrix2D,
    state_stack: Vec<ContextState>,
    tmp_path: Path,
    stroke_path_buf: Path,
    stroke_workspace: StrokeWorkspace,
    edge_buf: Vec<(f64, f64, f64, f64)>,
    rasterizer: AnalyticRasterizer,
    /// グラデーションスパン計算用の一時バッファ。
    gradient_span_buf: Vec<u32>,
}

impl<'a> Context<'a> {
    pub fn new(image: &'a mut Image, runtime: &'a mut PipelineRuntime) -> Self {
        Self {
            image,
            runtime,
            comp_op: CompOp::SrcOver,
            fill_rule: FillRule::default(),
            fill_color_prgb32: 0,
            fill_gradient: None,
            stroke_color_prgb32: 0,
            stroke_width: 1.0,
            stroke_start_cap: StrokeCap::default(),
            stroke_end_cap: StrokeCap::default(),
            stroke_join: StrokeJoin::default(),
            stroke_miter_limit: 4.0,
            matrix: Matrix2D::IDENTITY,
            state_stack: Vec::new(),
            tmp_path: Path::new(),
            stroke_path_buf: Path::new(),
            stroke_workspace: StrokeWorkspace::new(),
            edge_buf: Vec::new(),
            rasterizer: AnalyticRasterizer::new(),
            gradient_span_buf: Vec::new(),
        }
    }

    /// 現在の描画状態をスタックに保存する。
    pub fn save(&mut self) {
        self.state_stack.push(ContextState {
            comp_op: self.comp_op,
            fill_rule: self.fill_rule,
            fill_color_prgb32: self.fill_color_prgb32,
            fill_gradient: self.fill_gradient.clone(),
            stroke_color_prgb32: self.stroke_color_prgb32,
            stroke_width: self.stroke_width,
            stroke_start_cap: self.stroke_start_cap,
            stroke_end_cap: self.stroke_end_cap,
            stroke_join: self.stroke_join,
            stroke_miter_limit: self.stroke_miter_limit,
            matrix: self.matrix,
        });
    }

    /// スタックから描画状態を復元する。スタックが空の場合は何もしない。
    pub fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.comp_op = state.comp_op;
            self.fill_rule = state.fill_rule;
            self.fill_color_prgb32 = state.fill_color_prgb32;
            self.fill_gradient = state.fill_gradient;
            self.stroke_color_prgb32 = state.stroke_color_prgb32;
            self.stroke_width = state.stroke_width;
            self.stroke_start_cap = state.stroke_start_cap;
            self.stroke_end_cap = state.stroke_end_cap;
            self.stroke_join = state.stroke_join;
            self.stroke_miter_limit = state.stroke_miter_limit;
            self.matrix = state.matrix;
        }
    }

    pub fn set_comp_op(&mut self, op: CompOp) {
        self.comp_op = op;
    }

    /// 塗りつぶし規則を設定する。
    pub fn set_fill_rule(&mut self, rule: FillRule) {
        self.fill_rule = rule;
    }

    pub fn set_fill_style(&mut self, color: Rgba32) {
        self.fill_color_prgb32 = color.to_prgb32();
        self.fill_gradient = None;
    }

    /// 塗りつぶしスタイルをグラデーションに設定する。
    pub fn set_fill_style_gradient(&mut self, gradient: &Gradient) {
        self.fill_gradient = Some(gradient.clone());
    }

    /// ストローク色を設定する。
    pub fn set_stroke_style(&mut self, color: Rgba32) {
        self.stroke_color_prgb32 = color.to_prgb32();
    }

    /// 平行移動を現在の変換行列に後乗算で適用する。
    pub fn translate(&mut self, tx: f64, ty: f64) {
        self.matrix.translate(tx, ty);
    }

    /// スケーリングを現在の変換行列に後乗算で適用する。
    pub fn scale(&mut self, sx: f64, sy: f64) {
        self.matrix.scale(sx, sy);
    }

    /// 回転を現在の変換行列に後乗算で適用する。角度はラジアン。
    pub fn rotate(&mut self, angle: f64) {
        self.matrix.rotate(angle);
    }

    /// 任意の行列を現在の変換行列に後乗算で適用する。
    pub fn apply_matrix(&mut self, m: &Matrix2D) {
        self.matrix = self.matrix.multiply(m);
    }

    /// 変換行列を単位行列にリセットする。
    pub fn reset_matrix(&mut self) {
        self.matrix.reset();
    }

    /// 現在の変換を meta matrix に確定しリセットする (Blend2D 互換)。
    ///
    /// 現在は meta matrix を分離管理していないため、単に行列をリセットする。
    pub fn user_to_meta(&mut self) {
        self.matrix.reset();
    }

    pub fn fill_rect(&mut self, rect: &Rect) {
        // 変換行列が identity でない場合、矩形をパスに変換して fill_path にフォールバック
        if !self.matrix.is_identity() {
            let mut path = std::mem::take(&mut self.tmp_path);
            path.clear();
            path.move_to(rect.x, rect.y);
            path.line_to(rect.x + rect.w, rect.y);
            path.line_to(rect.x + rect.w, rect.y + rect.h);
            path.line_to(rect.x, rect.y + rect.h);
            path.close();
            self.fill_path(&path);
            self.tmp_path = path;
            return;
        }

        // グラデーションの場合はラスタライザを経由せず直接描画する
        if let Some(ref gradient) = self.fill_gradient {
            let Some(boxi) = self.clip_rect(rect) else {
                return;
            };
            let prepared = gradient.prepare(&self.matrix);
            let stride = self.image.stride();
            let base = self.image.data_ptr_mut();
            let width = (boxi.x1 - boxi.x0) as usize;
            let offset = boxi.y0 as usize * stride + boxi.x0 as usize * 4;
            let dst = unsafe { base.add(offset) };
            let height = (boxi.y1 - boxi.y0) as usize;

            // 融合パス: fetch + blend を 1 ループで処理し中間バッファを排除する
            prepared.fill_rect(dst, stride, boxi.x0, boxi.y0, width, height);
            return;
        }

        let prgb32 = self.fill_color_prgb32;
        let alpha = prgb32 >> 24;

        // alpha ファストパス: SrcOver で完全透明なら描画不要
        if self.comp_op == CompOp::SrcOver && alpha == 0 {
            return;
        }

        let Some(boxi) = self.clip_rect(rect) else {
            return;
        };

        // alpha ファストパス: SrcOver で完全不透明なら SrcCopy と等価
        let effective_op = if self.comp_op == CompOp::SrcOver && alpha == 255 {
            CompOp::SrcCopy
        } else {
            self.comp_op
        };

        let stride = self.image.stride();
        let base = self.image.data_ptr_mut();
        let width = (boxi.x1 - boxi.x0) as usize;
        let height = (boxi.y1 - boxi.y0) as usize;
        let offset = boxi.y0 as usize * stride + boxi.x0 as usize * 4;
        let dst = unsafe { base.add(offset) };

        // Box パイプライン: y ループを JIT 内に含み間接呼び出しを排除
        if let Some(box_fn) =
            self.runtime
                .get_or_compile_box(self.image.format(), effective_op, FetchType::Solid)
        {
            unsafe {
                box_fn(dst, prgb32, width, height, stride);
            }
            return;
        }

        // フォールバック: scanline ごとに呼び出し
        let pipeline_fn = self.runtime.get_or_compile(
            self.image.format(),
            effective_op,
            FillType::BoxA,
            FetchType::Solid,
        );

        for y in 0..height {
            let dst_row = unsafe { dst.add(y * stride) };
            unsafe {
                pipeline_fn(dst_row, prgb32, width);
            }
        }
    }

    pub fn fill_path(&mut self, path: &Path) {
        // グラデーションの場合は透明チェックをスキップ
        if self.fill_gradient.is_none() {
            // alpha ファストパス: SrcOver で完全透明なら描画不要
            if self.comp_op == CompOp::SrcOver && (self.fill_color_prgb32 >> 24) == 0 {
                return;
            }
        }

        // バウンディングボックスを計算
        let points = path.points();
        if points.is_empty() {
            return;
        }

        let has_transform = !self.matrix.is_identity();

        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;

        if has_transform {
            // 変換がある場合: 原座標の bbox の 4 頂点を変換し、AABB を算出
            let mut raw_min_x = f64::INFINITY;
            let mut raw_min_y = f64::INFINITY;
            let mut raw_max_x = f64::NEG_INFINITY;
            let mut raw_max_y = f64::NEG_INFINITY;
            for p in points {
                raw_min_x = raw_min_x.min(p.x);
                raw_min_y = raw_min_y.min(p.y);
                raw_max_x = raw_max_x.max(p.x);
                raw_max_y = raw_max_y.max(p.y);
            }
            // bbox の 4 頂点を変換
            let corners = [
                self.matrix.map_point(raw_min_x, raw_min_y),
                self.matrix.map_point(raw_max_x, raw_min_y),
                self.matrix.map_point(raw_max_x, raw_max_y),
                self.matrix.map_point(raw_min_x, raw_max_y),
            ];
            for (cx, cy) in &corners {
                min_x = min_x.min(*cx);
                min_y = min_y.min(*cy);
                max_x = max_x.max(*cx);
                max_y = max_y.max(*cy);
            }
        } else {
            for p in points {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }
        }

        // 画像境界でクリップ
        let img_w = self.image.width() as i32;
        let img_h = self.image.height() as i32;
        let clip_x0 = (min_x.floor() as i32).max(0);
        let clip_y0 = (min_y.floor() as i32).max(0);
        let clip_x1 = (max_x.ceil() as i32 + 1).min(img_w);
        let clip_y1 = (max_y.ceil() as i32 + 1).min(img_h);

        if clip_x0 >= clip_x1 || clip_y0 >= clip_y1 {
            return;
        }

        // Path を平坦化して線分を収集
        let mut edge_buf = std::mem::take(&mut self.edge_buf);
        edge_buf.clear();
        EdgeBuilder::flatten(path, |x0, y0, x1, y1| {
            edge_buf.push((x0, y0, x1, y1));
        });

        if edge_buf.is_empty() {
            self.edge_buf = edge_buf;
            return;
        }

        // 変換がある場合: JIT transform_edges でエッジ座標を一括変換
        if has_transform {
            let transform_fn = self.runtime.get_or_compile_transform_edges();
            let m = &self.matrix;
            unsafe {
                transform_fn(
                    edge_buf.as_mut_ptr().cast::<f64>(),
                    edge_buf.len(),
                    m.m00,
                    m.m01,
                    m.m10,
                    m.m11,
                    m.m20,
                    m.m21,
                );
            }
        }

        let sweep_fn = self.runtime.get_or_compile_sweep(self.fill_rule);
        let stride = self.image.stride();
        let base = self.image.data_ptr_mut();

        // グラデーション描画は描画時に PreparedGradient を生成してスカラ合成する。
        // 単色描画は既存の JIT パイプラインを使用する。
        let prepared_gradient = self.fill_gradient.as_ref().map(|g| g.prepare(&self.matrix));

        if let Some(ref gradient) = prepared_gradient {
            let span_cov_fn = self.runtime.get_or_compile_span_cov(
                self.image.format(),
                self.comp_op,
                FetchType::Solid,
            );
            let mut span_buf = std::mem::take(&mut self.gradient_span_buf);

            self.rasterizer.rasterize(
                &edge_buf,
                clip_x0,
                clip_y0,
                clip_x1,
                clip_y1,
                sweep_fn,
                |y, x_start, coverage| {
                    span_buf.resize(coverage.len(), 0);
                    gradient.fetch_span_linear_fixed(x_start, y, &mut span_buf);

                    let offset = y as usize * stride + x_start as usize * 4;
                    let dst_row = unsafe { base.add(offset) };
                    unsafe {
                        span_cov_fn(
                            dst_row,
                            span_buf.as_ptr(),
                            coverage.len(),
                            coverage.as_ptr(),
                        );
                    }
                },
            );

            self.gradient_span_buf = span_buf;
        } else {
            // 単色: 既存の JIT パイプラインを使用
            let pipeline_cov_fn = self.runtime.get_or_compile_cov(
                self.image.format(),
                self.comp_op,
                FetchType::Solid,
            );
            let prgb32 = self.fill_color_prgb32;

            self.rasterizer.rasterize(
                &edge_buf,
                clip_x0,
                clip_y0,
                clip_x1,
                clip_y1,
                sweep_fn,
                |y, x_start, coverage| {
                    let offset = y as usize * stride + x_start as usize * 4;
                    let dst_row = unsafe { base.add(offset) };
                    unsafe {
                        pipeline_cov_fn(dst_row, prgb32, coverage.len(), coverage.as_ptr());
                    }
                },
            );
        }

        self.edge_buf = edge_buf;
    }

    /// 画像全体を現在の塗りつぶし色で塗りつぶす。
    pub fn fill_all(&mut self) {
        let w = self.image.width() as f64;
        let h = self.image.height() as f64;
        self.fill_rect(&Rect::new(0.0, 0.0, w, h));
    }

    /// 扇形 (pie) を塗りつぶす。
    ///
    /// 中心から弧を経由して中心に戻る閉じた領域を塗りつぶす。
    pub fn fill_pie(&mut self, arc: &Arc) {
        if arc.rx <= 0.0 || arc.ry <= 0.0 || arc.sweep == 0.0 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_pie(arc.cx, arc.cy, arc.rx, arc.ry, arc.start, arc.sweep);
        self.fill_path(&path);
        self.tmp_path = path;
    }

    pub fn fill_circle(&mut self, circle: &Circle) {
        if circle.r <= 0.0 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_circle(circle.cx, circle.cy, circle.r);
        self.fill_path(&path);
        self.tmp_path = path;
    }

    /// テキストを塗りつぶし描画する。
    ///
    /// (x, y) はベースライン左端の位置。全グリフを 1 つの Path に結合し、
    /// `fill_path` 1 回で一括描画する (Blend2D と同じアプローチ)。
    pub fn fill_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();

        let mut cursor_x = x;
        let baseline_y = y;

        for ch in text.chars() {
            let glyph_id = font.map_char_to_glyph(ch);
            if glyph_id == 0 {
                // 未定義グリフはスキップ (スペースは glyph_id != 0 だがアウトラインなし)
                cursor_x += font.glyph_advance(glyph_id);
                continue;
            }

            // アウトラインを Path に追加 (エラーは無視してスキップ)
            let _ = font.append_glyph_outline(glyph_id, cursor_x, baseline_y, &mut path);
            cursor_x += font.glyph_advance(glyph_id);
        }

        if !path.is_empty() {
            self.fill_path(&path);
        }

        self.tmp_path = path;
    }

    /// ストローク幅を設定する。
    pub fn set_stroke_width(&mut self, width: f64) {
        self.stroke_width = width;
    }

    /// ストロークの端点形状を始点・終点の両方に一括設定する。
    pub fn set_stroke_cap(&mut self, cap: StrokeCap) {
        self.stroke_start_cap = cap;
        self.stroke_end_cap = cap;
    }

    /// ストロークの始点キャップを設定する。
    pub fn set_stroke_start_cap(&mut self, cap: StrokeCap) {
        self.stroke_start_cap = cap;
    }

    /// ストロークの終点キャップを設定する。
    pub fn set_stroke_end_cap(&mut self, cap: StrokeCap) {
        self.stroke_end_cap = cap;
    }

    /// ストロークの接続形状を設定する。
    pub fn set_stroke_join(&mut self, join: StrokeJoin) {
        self.stroke_join = join;
    }

    /// ストロークのマイターリミットを設定する。
    pub fn set_stroke_miter_limit(&mut self, limit: f64) {
        self.stroke_miter_limit = limit;
    }

    /// パスをストローク描画する。
    ///
    /// stroke_to_fill でストロークを塗りつぶしパスに変換し、
    /// stroke_color_prgb32 で fill_path を呼び出す。
    pub fn stroke_path(&mut self, path: &Path) {
        let mut stroke_buf = std::mem::take(&mut self.stroke_path_buf);
        stroke_buf.clear();
        let options = StrokeOptions {
            width: self.stroke_width,
            start_cap: self.stroke_start_cap,
            end_cap: self.stroke_end_cap,
            join: self.stroke_join,
            miter_limit: self.stroke_miter_limit,
        };
        let mut workspace = std::mem::take(&mut self.stroke_workspace);
        stroke_to_fill_with_workspace(path, &options, &mut stroke_buf, &mut workspace);
        self.stroke_workspace = workspace;
        // fill 色を一時的にストローク色に差し替えて描画する
        let saved_fill = self.fill_color_prgb32;
        self.fill_color_prgb32 = self.stroke_color_prgb32;
        self.fill_path(&stroke_buf);
        self.fill_color_prgb32 = saved_fill;
        self.stroke_path_buf = stroke_buf;
    }

    /// 矩形をストローク描画する。
    pub fn stroke_rect(&mut self, rect: &Rect) {
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.move_to(rect.x, rect.y);
        path.line_to(rect.x + rect.w, rect.y);
        path.line_to(rect.x + rect.w, rect.y + rect.h);
        path.line_to(rect.x, rect.y + rect.h);
        path.close();
        self.stroke_path(&path);
        self.tmp_path = path;
    }

    /// 円をストローク描画する。
    pub fn stroke_circle(&mut self, circle: &Circle) {
        if circle.r <= 0.0 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_circle(circle.cx, circle.cy, circle.r);
        self.stroke_path(&path);
        self.tmp_path = path;
    }

    /// 線分をストローク描画する。
    pub fn stroke_line(&mut self, line: &Line) {
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.move_to(line.x0, line.y0);
        path.line_to(line.x1, line.y1);
        self.stroke_path(&path);
        self.tmp_path = path;
    }

    /// 描画を終了する。現在は何もしないが、将来のバッファフラッシュ用。
    pub fn end(&mut self) {}

    /// Rect(f64) を画像境界でクリップした BoxI(i32) に変換する。
    /// 完全に画像外の場合は None を返す。
    fn clip_rect(&self, rect: &Rect) -> Option<BoxI> {
        let x0 = rect.x.floor() as i32;
        let y0 = rect.y.floor() as i32;
        let x1 = (rect.x + rect.w).ceil() as i32;
        let y1 = (rect.y + rect.h).ceil() as i32;

        let img_w = self.image.width() as i32;
        let img_h = self.image.height() as i32;

        let x0 = x0.max(0);
        let y0 = y0.max(0);
        let x1 = x1.min(img_w);
        let y1 = y1.min(img_h);

        if x0 >= x1 || y0 >= y1 {
            return None;
        }

        Some(BoxI { x0, y0, x1, y1 })
    }
}
