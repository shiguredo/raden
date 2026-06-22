use crate::api::blit;
use crate::api::gradient::Gradient;
use crate::api::image::Image;
use crate::api::matrix::Matrix2D;
use crate::api::path::{Path, Point};
use crate::api::pattern::Pattern;
use crate::api::stroke::{StrokeOptions, StrokeWorkspace, stroke_to_fill_with_workspace};
use crate::api::style::{CompOp, FillRule, Rgba32, StrokeCap, StrokeJoin};
use crate::font::{Font, GlyphBuffer};
use crate::pipeline::key::{FetchType, FillType};
use crate::pipeline::runtime::PipelineRuntime;
use crate::pixel::PixelFormat;
use crate::raster::analytic::AnalyticRasterizer;
use crate::raster::edge_builder::EdgeBuilder;

/// 0..=1 のアルファ値を 0..=255 の整数に丸める。範囲外はクランプする。
fn alpha_to_u8(a: f64) -> u8 {
    if !a.is_finite() || a <= 0.0 {
        0
    } else if a >= 1.0 {
        255
    } else {
        (a * 255.0 + 0.5) as u8
    }
}

/// premultiplied ARGB32 ピクセルを 0..=255 のアルファでスケールする。
/// すべてのチャネル (a/r/g/b) を等しく乗算するため premultiplied 不変条件は保たれる。
///
/// 除数は厳密に 255 ではなく `(v + 0x80 + (v >> 8)) >> 8` の近似式を使う。これは /255 を
/// 最大 1 LSB の誤差で近似しつつ、分岐なし・SIMD 化可能な形になる。
fn scale_prgb32(prgb32: u32, alpha: u8) -> u32 {
    if alpha == 0xFF {
        return prgb32;
    }
    if alpha == 0 {
        return 0;
    }
    let a = alpha as u32;
    let scale_chan = |c: u32| {
        let v = c * a + 0x80;
        (v + (v >> 8)) >> 8
    };
    let aa = scale_chan((prgb32 >> 24) & 0xFF);
    let r = scale_chan((prgb32 >> 16) & 0xFF);
    let g = scale_chan((prgb32 >> 8) & 0xFF);
    let b = scale_chan(prgb32 & 0xFF);
    (aa << 24) | (r << 16) | (g << 8) | b
}

/// `span` の各 premultiplied ARGB32 ピクセルに `alpha` を乗算する。
///
/// 内部ループを自動ベクトル化しやすい形に整え、alpha == 0 / 255 はホットループの外で早期
/// return する。aarch64 の場合は NEON で 8 ピクセルずつ処理する手書き版を使う。
#[inline]
fn scale_span_prgb32(span: &mut [u32], alpha: u8) {
    if alpha == 0xFF || span.is_empty() {
        return;
    }
    if alpha == 0 {
        span.fill(0);
        return;
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        scale_span_prgb32_neon(span, alpha);
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        scale_span_prgb32_scalar(span, alpha);
    }
}

#[allow(dead_code)]
#[inline]
fn scale_span_prgb32_scalar(span: &mut [u32], alpha: u8) {
    for px in span.iter_mut() {
        *px = scale_prgb32(*px, alpha);
    }
}

/// aarch64 NEON 実装: 一度に 4 ピクセル (16 u8 チャネル) を乗算する。
///
/// 近似式 `(v + 0x80 + (v >> 8)) >> 8` を u16 レーンで適用する。残りはスカラで処理する。
#[cfg(target_arch = "aarch64")]
#[target_feature(enable = "neon")]
unsafe fn scale_span_prgb32_neon(span: &mut [u32], alpha: u8) {
    use std::arch::aarch64::*;

    let a_u8 = vdup_n_u8(alpha);
    let c80 = vdupq_n_u16(0x80);

    let ptr = span.as_mut_ptr() as *mut u8;
    let len = span.len();
    let mut i = 0usize;
    while i + 4 <= len {
        unsafe {
            let p = ptr.add(i * 4);
            // 4 ピクセル = 16 u8 を読み込む
            let v = vld1q_u8(p);
            let lo = vget_low_u8(v);
            let hi = vget_high_u8(v);
            // u8 × u8 → u16 (乗算で 16bit に拡張)
            let lo16 = vmull_u8(lo, a_u8);
            let hi16 = vmull_u8(hi, a_u8);
            // (v + 0x80 + (v >> 8)) >> 8
            let lo16 = vaddq_u16(lo16, c80);
            let lo16 = vaddq_u16(lo16, vshrq_n_u16(lo16, 8));
            let lo_out = vshrn_n_u16(lo16, 8);
            let hi16 = vaddq_u16(hi16, c80);
            let hi16 = vaddq_u16(hi16, vshrq_n_u16(hi16, 8));
            let hi_out = vshrn_n_u16(hi16, 8);
            let out = vcombine_u8(lo_out, hi_out);
            vst1q_u8(p, out);
        }
        i += 4;
    }
    // 残りピクセルはスカラで処理する
    for px in &mut span[i..] {
        *px = scale_prgb32(*px, alpha);
    }
}

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

/// 楕円。中心 (cx, cy)、半径 (rx, ry)。
#[derive(Debug, Clone, Copy)]
pub struct Ellipse {
    pub cx: f64,
    pub cy: f64,
    pub rx: f64,
    pub ry: f64,
}

impl Ellipse {
    pub fn new(cx: f64, cy: f64, rx: f64, ry: f64) -> Self {
        Self { cx, cy, rx, ry }
    }
}

/// 角丸矩形。
#[derive(Debug, Clone, Copy)]
pub struct RoundRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rx: f64,
    pub ry: f64,
}

impl RoundRect {
    pub fn new(x: f64, y: f64, w: f64, h: f64, rx: f64, ry: f64) -> Self {
        Self { x, y, w, h, rx, ry }
    }
}

/// 三角形。
#[derive(Debug, Clone, Copy)]
pub struct Triangle {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl Triangle {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self {
            x0,
            y0,
            x1,
            y1,
            x2,
            y2,
        }
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
#[derive(Debug, Clone, Copy)]
struct BoxI {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

/// save() / restore() で保存・復元される描画状態。
struct ContextState {
    comp_op: CompOp,
    fill_rule: FillRule,
    fill_color_prgb32: u32,
    fill_gradient: Option<Gradient>,
    fill_pattern: Option<Pattern>,
    stroke_color_prgb32: u32,
    stroke_gradient: Option<Gradient>,
    stroke_pattern: Option<Pattern>,
    stroke_width: f64,
    stroke_start_cap: StrokeCap,
    stroke_end_cap: StrokeCap,
    stroke_join: StrokeJoin,
    stroke_miter_limit: f64,
    stroke_dash_array: Vec<f64>,
    stroke_dash_offset: f64,
    matrix: Matrix2D,
    global_alpha: f64,
    fill_alpha: f64,
    stroke_alpha: f64,
    /// ユーザー指定のクリップ領域
    clip_box: BoxI,
}

pub struct Context<'a> {
    image: &'a mut Image,
    runtime: &'a mut PipelineRuntime,
    comp_op: CompOp,
    fill_rule: FillRule,
    fill_color_prgb32: u32,
    fill_gradient: Option<Gradient>,
    fill_pattern: Option<Pattern>,
    stroke_color_prgb32: u32,
    stroke_gradient: Option<Gradient>,
    stroke_pattern: Option<Pattern>,
    stroke_width: f64,
    stroke_start_cap: StrokeCap,
    stroke_end_cap: StrokeCap,
    stroke_join: StrokeJoin,
    stroke_miter_limit: f64,
    stroke_dash_array: Vec<f64>,
    stroke_dash_offset: f64,
    matrix: Matrix2D,
    global_alpha: f64,
    fill_alpha: f64,
    stroke_alpha: f64,
    /// メタクリップ (画像境界)。restore_clipping で戻る先。
    meta_clip_box: BoxI,
    /// 現在のクリップ領域。clip_to_rect で縮小される。
    clip_box: BoxI,
    state_stack: Vec<ContextState>,
    tmp_path: Path,
    /// テキスト描画用の一時バッファ。
    /// `Font::shape_into` の出力先。`Context` 描画時に繰り返し `clear` して使う。
    tmp_shape_buffer: GlyphBuffer,
    stroke_path_buf: Path,
    stroke_workspace: StrokeWorkspace,
    edge_buf: Vec<(f64, f64, f64, f64)>,
    rasterizer: AnalyticRasterizer,
    /// グラデーション/パターンスパン計算用の一時バッファ。
    gradient_span_buf: Vec<u32>,
}

impl<'a> Context<'a> {
    pub fn new(image: &'a mut Image, runtime: &'a mut PipelineRuntime) -> Self {
        let meta_clip_box = BoxI {
            x0: 0,
            y0: 0,
            x1: image.width() as i32,
            y1: image.height() as i32,
        };
        Self {
            image,
            runtime,
            comp_op: CompOp::SrcOver,
            fill_rule: FillRule::default(),
            fill_color_prgb32: 0,
            fill_gradient: None,
            fill_pattern: None,
            stroke_color_prgb32: 0,
            stroke_gradient: None,
            stroke_pattern: None,
            stroke_width: 1.0,
            stroke_start_cap: StrokeCap::default(),
            stroke_end_cap: StrokeCap::default(),
            stroke_join: StrokeJoin::default(),
            stroke_miter_limit: 4.0,
            stroke_dash_array: Vec::new(),
            stroke_dash_offset: 0.0,
            matrix: Matrix2D::IDENTITY,
            global_alpha: 1.0,
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            meta_clip_box,
            clip_box: meta_clip_box,
            state_stack: Vec::new(),
            tmp_path: Path::new(),
            tmp_shape_buffer: GlyphBuffer::default(),
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
            fill_pattern: self.fill_pattern.clone(),
            stroke_color_prgb32: self.stroke_color_prgb32,
            stroke_gradient: self.stroke_gradient.clone(),
            stroke_pattern: self.stroke_pattern.clone(),
            stroke_width: self.stroke_width,
            stroke_start_cap: self.stroke_start_cap,
            stroke_end_cap: self.stroke_end_cap,
            stroke_join: self.stroke_join,
            stroke_miter_limit: self.stroke_miter_limit,
            stroke_dash_array: self.stroke_dash_array.clone(),
            stroke_dash_offset: self.stroke_dash_offset,
            matrix: self.matrix,
            global_alpha: self.global_alpha,
            fill_alpha: self.fill_alpha,
            stroke_alpha: self.stroke_alpha,
            clip_box: self.clip_box,
        });
    }

    /// スタックから描画状態を復元する。スタックが空の場合は何もしない。
    pub fn restore(&mut self) {
        if let Some(state) = self.state_stack.pop() {
            self.comp_op = state.comp_op;
            self.fill_rule = state.fill_rule;
            self.fill_color_prgb32 = state.fill_color_prgb32;
            self.fill_gradient = state.fill_gradient;
            self.fill_pattern = state.fill_pattern;
            self.stroke_color_prgb32 = state.stroke_color_prgb32;
            self.stroke_gradient = state.stroke_gradient;
            self.stroke_pattern = state.stroke_pattern;
            self.stroke_width = state.stroke_width;
            self.stroke_start_cap = state.stroke_start_cap;
            self.stroke_end_cap = state.stroke_end_cap;
            self.stroke_join = state.stroke_join;
            self.stroke_miter_limit = state.stroke_miter_limit;
            self.stroke_dash_array = state.stroke_dash_array;
            self.stroke_dash_offset = state.stroke_dash_offset;
            self.matrix = state.matrix;
            self.global_alpha = state.global_alpha;
            self.fill_alpha = state.fill_alpha;
            self.stroke_alpha = state.stroke_alpha;
            self.clip_box = state.clip_box;
        }
    }

    pub fn set_comp_op(&mut self, op: CompOp) {
        self.comp_op = op;
    }

    pub fn comp_op(&self) -> CompOp {
        self.comp_op
    }

    /// 塗りつぶし規則を設定する。
    pub fn set_fill_rule(&mut self, rule: FillRule) {
        self.fill_rule = rule;
    }

    pub fn fill_rule(&self) -> FillRule {
        self.fill_rule
    }

    /// 単色塗りつぶしの premultiplied ARGB32 (`set_fill_style` が設定した値)。
    pub fn fill_color_prgb32(&self) -> u32 {
        self.fill_color_prgb32
    }

    pub fn fill_gradient(&self) -> Option<&Gradient> {
        self.fill_gradient.as_ref()
    }

    pub fn fill_pattern(&self) -> Option<&Pattern> {
        self.fill_pattern.as_ref()
    }

    pub fn stroke_color_prgb32(&self) -> u32 {
        self.stroke_color_prgb32
    }

    pub fn stroke_gradient(&self) -> Option<&Gradient> {
        self.stroke_gradient.as_ref()
    }

    pub fn stroke_pattern(&self) -> Option<&Pattern> {
        self.stroke_pattern.as_ref()
    }

    pub fn stroke_width(&self) -> f64 {
        self.stroke_width
    }

    pub fn stroke_miter_limit(&self) -> f64 {
        self.stroke_miter_limit
    }

    pub fn stroke_join(&self) -> StrokeJoin {
        self.stroke_join
    }

    pub fn stroke_start_cap(&self) -> StrokeCap {
        self.stroke_start_cap
    }

    pub fn stroke_end_cap(&self) -> StrokeCap {
        self.stroke_end_cap
    }

    pub fn stroke_dash_array(&self) -> &[f64] {
        &self.stroke_dash_array
    }

    pub fn stroke_dash_offset(&self) -> f64 {
        self.stroke_dash_offset
    }

    pub fn matrix(&self) -> &Matrix2D {
        &self.matrix
    }

    pub fn global_alpha(&self) -> f64 {
        self.global_alpha
    }

    /// 全描画に乗算されるグローバルアルファを設定する。値域 [0, 1] でクランプする。
    pub fn set_global_alpha(&mut self, a: f64) {
        self.global_alpha = a.clamp(0.0, 1.0);
    }

    pub fn fill_alpha(&self) -> f64 {
        self.fill_alpha
    }

    /// fill 個別のアルファを設定する。値域 [0, 1] でクランプする。
    pub fn set_fill_alpha(&mut self, a: f64) {
        self.fill_alpha = a.clamp(0.0, 1.0);
    }

    pub fn stroke_alpha(&self) -> f64 {
        self.stroke_alpha
    }

    /// stroke 個別のアルファを設定する。値域 [0, 1] でクランプする。
    pub fn set_stroke_alpha(&mut self, a: f64) {
        self.stroke_alpha = a.clamp(0.0, 1.0);
    }

    /// fill 経路で適用される実効アルファ (global * fill) を 0..=255 で返す。
    fn effective_fill_alpha_u8(&self) -> u8 {
        alpha_to_u8(self.global_alpha * self.fill_alpha)
    }

    pub fn set_fill_style(&mut self, color: Rgba32) {
        self.fill_color_prgb32 = color.to_prgb32();
        self.fill_gradient = None;
        self.fill_pattern = None;
    }

    /// 塗りつぶしスタイルをグラデーションに設定する。
    pub fn set_fill_style_gradient(&mut self, gradient: &Gradient) {
        self.fill_gradient = Some(gradient.clone());
        self.fill_pattern = None;
    }

    /// 塗りつぶしスタイルをパターンに設定する。
    pub fn set_fill_style_pattern(&mut self, pattern: &Pattern) {
        self.fill_pattern = Some(pattern.clone());
        self.fill_gradient = None;
    }

    /// ストローク色を設定する。
    pub fn set_stroke_style(&mut self, color: Rgba32) {
        self.stroke_color_prgb32 = color.to_prgb32();
        self.stroke_gradient = None;
        self.stroke_pattern = None;
    }

    /// ストロークスタイルをグラデーションに設定する。
    pub fn set_stroke_style_gradient(&mut self, gradient: &Gradient) {
        self.stroke_gradient = Some(gradient.clone());
        self.stroke_pattern = None;
    }

    /// ストロークスタイルをパターンに設定する。
    pub fn set_stroke_style_pattern(&mut self, pattern: &Pattern) {
        self.stroke_pattern = Some(pattern.clone());
        self.stroke_gradient = None;
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

    /// 指定中心まわりの回転を現在の変換行列に後乗算で適用する。角度はラジアン。
    pub fn rotate_around(&mut self, angle: f64, cx: f64, cy: f64) {
        self.matrix.rotate_around(angle, cx, cy);
    }

    /// せん断を現在の変換行列に後乗算で適用する。係数は Blend2D の `skew` と同様 (接線)。
    pub fn skew(&mut self, kx: f64, ky: f64) {
        self.matrix.skew(kx, ky);
    }

    /// 平行移動を現在の変換行列に前乗算で適用する (`post_translate`)。
    pub fn post_translate(&mut self, tx: f64, ty: f64) {
        self.matrix.post_translate(tx, ty);
    }

    /// スケーリングを現在の変換行列に前乗算で適用する (`post_scale`)。
    pub fn post_scale(&mut self, sx: f64, sy: f64) {
        self.matrix.post_scale(sx, sy);
    }

    /// 回転を現在の変換行列に前乗算で適用する (`post_rotate`)。角度はラジアン。
    pub fn post_rotate(&mut self, angle: f64) {
        self.matrix.post_rotate(angle);
    }

    /// せん断を現在の変換行列に前乗算で適用する (`post_skew`)。
    pub fn post_skew(&mut self, kx: f64, ky: f64) {
        self.matrix.post_skew(kx, ky);
    }

    /// 任意の行列を現在の変換行列に前乗算で適用する (`post_transform`)。
    pub fn post_transform(&mut self, m: &Matrix2D) {
        self.matrix.post_transform(m);
    }

    /// クリップ領域を指定矩形との積集合に縮小する。
    ///
    /// 複数回呼び出すとクリップ領域は縮小のみされる (拡大はできない)。
    /// `save()` / `restore()` でクリップ状態も保存・復元される。
    pub fn clip_to_rect(&mut self, rect: &Rect) {
        let x0 = rect.x.floor() as i32;
        let y0 = rect.y.floor() as i32;
        let x1 = (rect.x + rect.w).ceil() as i32;
        let y1 = (rect.y + rect.h).ceil() as i32;

        self.clip_box.x0 = self.clip_box.x0.max(x0);
        self.clip_box.y0 = self.clip_box.y0.max(y0);
        self.clip_box.x1 = self.clip_box.x1.min(x1);
        self.clip_box.y1 = self.clip_box.y1.min(y1);
    }

    /// クリップ領域をメタクリップ (画像境界) にリセットする。
    pub fn restore_clipping(&mut self) {
        self.clip_box = self.meta_clip_box;
    }

    /// Blend2D の `userToMeta()` に相当する呼び出し口。
    ///
    /// Blend2D ではユーザ空間行列をメタ空間へ確定したうえでユーザ行列を単位に戻す。
    /// raden はメタ行列を別保持していないため、現状は **ユーザ相当の行列 (`matrix`) のみを単位にリセットする**。
    pub fn user_to_meta(&mut self) {
        self.matrix.reset();
    }

    /// 現在のクリップ領域全体をピクセル値 0 でクリアする。
    ///
    /// `set_comp_op(CompOp::Clear)` + `fill_all` と等価だが、合成モードや
    /// ストロークスタイルを汚さずに実行する。
    pub fn clear_all(&mut self) {
        let w = self.image.width() as f64;
        let h = self.image.height() as f64;
        self.clear_rect(&Rect::new(0.0, 0.0, w, h));
    }

    /// 指定矩形をピクセル値 0 でクリアする。
    ///
    /// 現在のクリップ領域との積集合のみを書き換える。変換行列は無視する
    /// (Blend2D の `clear_rect` も同様にデバイス座標で動作する)。
    pub fn clear_rect(&mut self, rect: &Rect) {
        let Some(boxi) = self.clip_rect(rect) else {
            return;
        };
        let bpp = self.image.format().bytes_per_pixel();
        let stride = self.image.stride();
        let base = self.image.data_ptr_mut();
        let row_bytes = (boxi.x1 - boxi.x0) as usize * bpp;
        for y in boxi.y0..boxi.y1 {
            let offset = y as usize * stride + boxi.x0 as usize * bpp;
            unsafe {
                std::ptr::write_bytes(base.add(offset), 0, row_bytes);
            }
        }
    }

    pub fn fill_rect(&mut self, rect: &Rect) {
        let eff_alpha = self.effective_fill_alpha_u8();
        // 実効アルファが 1.0 未満でグラデ/パターンの場合は span path 経由 (fill_path) を使う。
        // fill_rect の高速パスは alpha 適用に対応していないため。
        if eff_alpha != 0xFF && (self.fill_gradient.is_some() || self.fill_pattern.is_some()) {
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

        if self.image.format() == PixelFormat::A8
            && (self.fill_pattern.is_some() || self.fill_gradient.is_some())
        {
            panic!("pattern and gradient fills are not supported for A8 destination");
        }

        let bpp = self.image.format().bytes_per_pixel();

        // パターンの場合はラスタライザを経由せず直接描画する
        if let Some(ref pattern) = self.fill_pattern {
            let Some(boxi) = self.clip_rect(rect) else {
                return;
            };
            let prepared = pattern.prepare(&self.matrix, self.comp_op);
            let stride = self.image.stride();
            let base = self.image.data_ptr_mut();
            let width = (boxi.x1 - boxi.x0) as usize;
            let offset = boxi.y0 as usize * stride + boxi.x0 as usize * bpp;
            let dst = unsafe { base.add(offset) };
            let height = (boxi.y1 - boxi.y0) as usize;
            prepared.fill_rect(dst, stride, boxi.x0, boxi.y0, width, height);
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
            let offset = boxi.y0 as usize * stride + boxi.x0 as usize * bpp;
            let dst = unsafe { base.add(offset) };
            let height = (boxi.y1 - boxi.y0) as usize;

            // Radial 不透明の場合は JIT F32X4 SIMD パスを使用する
            if matches!(
                gradient.values(),
                crate::api::gradient::GradientValues::Radial(_)
            ) && prepared.is_opaque()
            {
                let radial_fn = self.runtime.get_or_compile_radial_row();
                prepared
                    .fill_rect_radial_jit(dst, stride, boxi.x0, boxi.y0, width, height, radial_fn);
                return;
            }

            // 融合パス: fetch + blend を 1 ループで処理し中間バッファを排除する
            prepared.fill_rect(dst, stride, boxi.x0, boxi.y0, width, height);
            return;
        }

        let prgb32 = scale_prgb32(self.fill_color_prgb32, eff_alpha);
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
        let offset = boxi.y0 as usize * stride + boxi.x0 as usize * bpp;
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
        assert_ne!(
            self.image.format(),
            PixelFormat::A8,
            "fill_path is not supported for A8 destination"
        );

        let bpp = self.image.format().bytes_per_pixel();
        let eff_alpha = self.effective_fill_alpha_u8();

        // 実効アルファが 0 の場合は SrcOver/SrcCopy 以外でも実質変更がないため省略するのは
        // 厳密ではない (Clear などはゼロを書く必要がある)。ここでは SrcOver のみ早期 return。
        if self.comp_op == CompOp::SrcOver && eff_alpha == 0 {
            return;
        }

        // グラデーションの場合は色アルファのチェックをスキップ
        if self.fill_gradient.is_none() && self.fill_pattern.is_none() {
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

        // クリップ領域でクリップ
        let clip_x0 = (min_x.floor() as i32).max(self.clip_box.x0);
        let clip_y0 = (min_y.floor() as i32).max(self.clip_box.y0);
        let clip_x1 = (max_x.ceil() as i32 + 1).min(self.clip_box.x1);
        let clip_y1 = (max_y.ceil() as i32 + 1).min(self.clip_box.y1);

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

        // パターン描画
        let prepared_pattern = self
            .fill_pattern
            .as_ref()
            .map(|p| p.prepare(&self.matrix, self.comp_op));
        if let Some(ref pattern) = prepared_pattern {
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
                    pattern.fetch_span(x_start, y, &mut span_buf);
                    scale_span_prgb32(&mut span_buf, eff_alpha);

                    let offset = y as usize * stride + x_start as usize * bpp;
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
            self.edge_buf = edge_buf;
            return;
        }

        // グラデーション描画は描画時に PreparedGradient を生成してスカラ合成する。
        // 単色描画は既存の JIT パイプラインを使用する。
        let prepared_gradient = self.fill_gradient.as_ref().map(|g| g.prepare(&self.matrix));

        if let Some(ref gradient) = prepared_gradient {
            // Linear + 不透明 LUT + Pad モードの場合は融合 JIT パスを使用する
            // 実効アルファが 1 未満のときはスパンバッファ経由にフォールバックする
            if eff_alpha == 0xFF && gradient.is_linear_pad() && gradient.is_opaque() {
                let linear_cov_fn = self.runtime.get_or_compile_linear_gradient_cov();
                gradient.fill_path_linear_jit(
                    &mut self.rasterizer,
                    &edge_buf,
                    clip_x0,
                    clip_y0,
                    clip_x1,
                    clip_y1,
                    sweep_fn,
                    stride,
                    base,
                    linear_cov_fn,
                );
                self.edge_buf = edge_buf;
                return;
            }

            // その他のグラデーション: スパンバッファ経由
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
                    scale_span_prgb32(&mut span_buf, eff_alpha);

                    let offset = y as usize * stride + x_start as usize * bpp;
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
            let prgb32 = scale_prgb32(self.fill_color_prgb32, eff_alpha);

            self.rasterizer.rasterize(
                &edge_buf,
                clip_x0,
                clip_y0,
                clip_x1,
                clip_y1,
                sweep_fn,
                |y, x_start, coverage| {
                    let offset = y as usize * stride + x_start as usize * bpp;
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

    /// 三角形を塗りつぶす。
    pub fn fill_triangle(&mut self, t: &Triangle) {
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_triangle(t.x0, t.y0, t.x1, t.y1, t.x2, t.y2);
        self.fill_path(&path);
        self.tmp_path = path;
    }

    /// ポリゴンを塗りつぶす。点列が 3 点未満なら何もしない。
    pub fn fill_polygon(&mut self, points: &[Point]) {
        if points.len() < 3 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_polygon(points);
        self.fill_path(&path);
        self.tmp_path = path;
    }

    /// 角丸矩形を塗りつぶす。
    pub fn fill_round_rect(&mut self, rr: &RoundRect) {
        if rr.w <= 0.0 || rr.h <= 0.0 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_round_rect(rr.x, rr.y, rr.w, rr.h, rr.rx, rr.ry);
        self.fill_path(&path);
        self.tmp_path = path;
    }

    /// 楕円を塗りつぶす。
    pub fn fill_ellipse(&mut self, ellipse: &Ellipse) {
        if ellipse.rx <= 0.0 || ellipse.ry <= 0.0 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_ellipse(ellipse.cx, ellipse.cy, ellipse.rx, ellipse.ry);
        self.fill_path(&path);
        self.tmp_path = path;
    }

    /// テキストを 1 つの `Path` に展開する共通処理。
    ///
    /// `tmp_path` / `tmp_shape_buffer` を一時的に奪い、展開後に返却する。
    /// glyph_id == 0 はアウトラインを構築せず advance のみ加算する。
    /// append_glyph_outline のエラーは let _ で黙殺し、外部入力に対する
    /// クラッシュ耐性を優先する (部分欠落を許容)。
    fn build_text_path(&mut self, x: f64, y: f64, font: &Font, text: &str) -> Path {
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        self.tmp_shape_buffer.clear();

        font.shape_into(text, &mut self.tmp_shape_buffer);

        let mut cursor_x = x;
        for (glyph_id, placement, _cluster) in self.tmp_shape_buffer.iter() {
            if glyph_id != 0 {
                // GlyphPlacement::offset_y はフォント設計座標系 (Y up) 、
                // append_glyph_outline は画面座標系 (Y down) を前提としているため、
                // 符号を反転して渡す。
                let _ = font.append_glyph_outline(
                    glyph_id,
                    cursor_x + placement.offset_x,
                    y - placement.offset_y,
                    &mut path,
                );
            }
            cursor_x += placement.advance;
        }

        path
    }

    /// テキストを塗りつぶし描画する。
    ///
    /// (x, y) はベースライン左端の位置。全グリフを 1 つの Path に結合し、
    /// `fill_path` 1 回で一括描画する (Blend2D と同じアプローチ)。
    pub fn fill_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
        let path = self.build_text_path(x, y, font, text);
        if !path.is_empty() {
            self.fill_path(&path);
        }
        self.tmp_path = path;
    }

    /// 文字列を現在の stroke 設定で描画する。
    ///
    /// (x, y) はベースライン左端の位置。全グリフを 1 つの Path に結合し、
    /// `stroke_path` 1 回で一括描画する (`fill_text` と同じアプローチ)。
    pub fn stroke_text(&mut self, x: f64, y: f64, font: &Font, text: &str) {
        let path = self.build_text_path(x, y, font, text);
        if !path.is_empty() {
            self.stroke_path(&path);
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

    /// ストロークのダッシュパターンを設定する。
    ///
    /// 交互に「描画区間」「空白区間」の長さを指定する。
    /// 空の配列を渡すと実線に戻る。
    pub fn set_stroke_dash_array(&mut self, dash_array: &[f64]) {
        self.stroke_dash_array = dash_array.to_vec();
    }

    /// ストロークのダッシュオフセットを設定する。
    pub fn set_stroke_dash_offset(&mut self, offset: f64) {
        self.stroke_dash_offset = offset;
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
            dash_array: self.stroke_dash_array.clone(),
            dash_offset: self.stroke_dash_offset,
        };
        let mut workspace = std::mem::take(&mut self.stroke_workspace);
        stroke_to_fill_with_workspace(path, &options, &mut stroke_buf, &mut workspace);
        self.stroke_workspace = workspace;
        // fill スタイルを一時的にストロークスタイルに差し替えて描画する。
        // ストローク側にグラデ/パターンが設定されていれば優先し、なければ単色を使う。
        // fill_alpha も一時的に stroke_alpha に差し替えて effective_fill_alpha が
        // stroke 経路の実効アルファを返すようにする。
        let saved_fill_color = self.fill_color_prgb32;
        let saved_fill_gradient = self.fill_gradient.take();
        let saved_fill_pattern = self.fill_pattern.take();
        let saved_fill_alpha = self.fill_alpha;
        self.fill_color_prgb32 = self.stroke_color_prgb32;
        self.fill_gradient = self.stroke_gradient.clone();
        self.fill_pattern = self.stroke_pattern.clone();
        self.fill_alpha = self.stroke_alpha;
        self.fill_path(&stroke_buf);
        self.fill_color_prgb32 = saved_fill_color;
        self.fill_gradient = saved_fill_gradient;
        self.fill_pattern = saved_fill_pattern;
        self.fill_alpha = saved_fill_alpha;
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

    /// 三角形をストローク描画する。
    pub fn stroke_triangle(&mut self, t: &Triangle) {
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_triangle(t.x0, t.y0, t.x1, t.y1, t.x2, t.y2);
        self.stroke_path(&path);
        self.tmp_path = path;
    }

    /// ポリゴンをストローク描画する (閉じる)。
    pub fn stroke_polygon(&mut self, points: &[Point]) {
        if points.len() < 3 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_polygon(points);
        self.stroke_path(&path);
        self.tmp_path = path;
    }

    /// 折れ線 (polyline) をストローク描画する。閉じない。
    pub fn stroke_polyline(&mut self, points: &[Point]) {
        if points.len() < 2 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_polyline(points);
        self.stroke_path(&path);
        self.tmp_path = path;
    }

    /// 角丸矩形をストローク描画する。
    pub fn stroke_round_rect(&mut self, rr: &RoundRect) {
        if rr.w <= 0.0 || rr.h <= 0.0 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_round_rect(rr.x, rr.y, rr.w, rr.h, rr.rx, rr.ry);
        self.stroke_path(&path);
        self.tmp_path = path;
    }

    /// 楕円をストローク描画する。
    pub fn stroke_ellipse(&mut self, ellipse: &Ellipse) {
        if ellipse.rx <= 0.0 || ellipse.ry <= 0.0 {
            return;
        }
        let mut path = std::mem::take(&mut self.tmp_path);
        path.clear();
        path.add_ellipse(ellipse.cx, ellipse.cy, ellipse.rx, ellipse.ry);
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

    /// ソース画像を `dst` 矩形に Nearest 転送する。`src_rect` が `None` のときはソース全体。
    ///
    /// 現在は `CompOp::SrcOver` と `CompOp::SrcCopy` のみ。ソースは `Prgb32` / `Xrgb32`、宛ては `Prgb32` / `Xrgb32` / `A8`。
    /// 変換行列は現在の `matrix` を用いる (ユーザ空間の `dst` をデバイスへ写像)。
    pub fn blit_image_rect(&mut self, dst: &Rect, src: &Image, src_rect: Option<Rect>) {
        let corners = [
            self.matrix.map_point(dst.x, dst.y),
            self.matrix.map_point(dst.x + dst.w, dst.y),
            self.matrix.map_point(dst.x + dst.w, dst.y + dst.h),
            self.matrix.map_point(dst.x, dst.y + dst.h),
        ];
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for (x, y) in corners {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let ix0 = (min_x.floor() as i32).max(self.clip_box.x0);
        let iy0 = (min_y.floor() as i32).max(self.clip_box.y0);
        let ix1 = (max_x.ceil() as i32).min(self.clip_box.x1);
        let iy1 = (max_y.ceil() as i32).min(self.clip_box.y1);
        if ix0 >= ix1 || iy0 >= iy1 {
            return;
        }
        blit::blit_image_rect_scoped(
            self.image,
            dst,
            src,
            src_rect,
            &self.matrix,
            ix0,
            iy0,
            ix1,
            iy1,
            self.comp_op,
        );
    }

    /// ソース画像全体を (x, y) に転送する。`blit_image_rect` のショートカット。
    pub fn blit_image_at(&mut self, x: f64, y: f64, src: &Image) {
        let r = Rect::new(x, y, src.width() as f64, src.height() as f64);
        self.blit_image_rect(&r, src, None);
    }

    /// 描画を終了する。現在は何もしないが、将来のバッファフラッシュ用。
    pub fn end(&mut self) {}

    /// Rect(f64) を現在のクリップ領域でクリップした BoxI(i32) に変換する。
    /// 完全にクリップ外の場合は None を返す。
    fn clip_rect(&self, rect: &Rect) -> Option<BoxI> {
        let x0 = rect.x.floor() as i32;
        let y0 = rect.y.floor() as i32;
        let x1 = (rect.x + rect.w).ceil() as i32;
        let y1 = (rect.y + rect.h).ceil() as i32;

        let x0 = x0.max(self.clip_box.x0);
        let y0 = y0.max(self.clip_box.y0);
        let x1 = x1.min(self.clip_box.x1);
        let y1 = y1.min(self.clip_box.y1);

        if x0 >= x1 || y0 >= y1 {
            return None;
        }

        Some(BoxI { x0, y0, x1, y1 })
    }
}
