// =============================================================================
// gradient.rs -- グラデーション
// =============================================================================
//
// Linear / Radial / Conic の 3 種類のグラデーションを提供する。
// Blend2D の BLGradient API に準拠した設計。
//
// Phase 1: 純 Rust スカラ実装 (LUT + スカラ合成)。
// Phase 2 以降で JIT/SIMD 化する。

use crate::api::matrix::Matrix2D;
use crate::api::style::Rgba32;

// =============================================================================
// 公開型
// =============================================================================

/// グラデーションの色停止点。
#[derive(Debug, Clone, Copy)]
pub struct GradientStop {
    /// 0.0-1.0 の相対位置。
    pub offset: f64,
    /// 色。
    pub color: Rgba32,
}

/// 範囲外処理モード。Blend2D の BLExtendMode (シンプルモード) に対応。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum ExtendMode {
    /// 端の色で埋める (デフォルト)。
    #[default]
    Pad = 0,
    /// 繰り返す。
    Repeat = 1,
    /// 反転して繰り返す。
    Reflect = 2,
}

/// Linear Gradient の定義値。Blend2D の BLLinearGradientValues に対応。
#[derive(Debug, Clone, Copy)]
pub struct LinearGradientValues {
    /// 開始 X。
    pub x0: f64,
    /// 開始 Y。
    pub y0: f64,
    /// 終了 X。
    pub x1: f64,
    /// 終了 Y。
    pub y1: f64,
}

/// Radial Gradient の定義値。Blend2D の BLRadialGradientValues に対応。
#[derive(Debug, Clone, Copy)]
pub struct RadialGradientValues {
    /// 中心 X。
    pub x0: f64,
    /// 中心 Y。
    pub y0: f64,
    /// 焦点 X。
    pub x1: f64,
    /// 焦点 Y。
    pub y1: f64,
    /// 中心半径。
    pub r0: f64,
    /// 焦点半径。
    pub r1: f64,
}

/// Conic Gradient の定義値。Blend2D の BLConicGradientValues に対応。
#[derive(Debug, Clone, Copy)]
pub struct ConicGradientValues {
    /// 中心 X。
    pub x0: f64,
    /// 中心 Y。
    pub y0: f64,
    /// 開始角度 (ラジアン)。
    pub angle: f64,
}

/// グラデーション値の種別。
#[derive(Debug, Clone)]
pub enum GradientValues {
    Linear(LinearGradientValues),
    Radial(RadialGradientValues),
    Conic(ConicGradientValues),
}

/// グラデーション。
///
/// 色停止点と拡張モードを保持する。座標はユーザー空間で指定する。
/// 描画時に Context の変換行列を反映した PreparedGradient に変換される。
#[derive(Debug, Clone)]
pub struct Gradient {
    values: GradientValues,
    stops: Vec<GradientStop>,
    extend_mode: ExtendMode,
}

impl Gradient {
    /// Linear Gradient を生成する。
    pub fn new_linear(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        Self {
            values: GradientValues::Linear(LinearGradientValues { x0, y0, x1, y1 }),
            stops: Vec::new(),
            extend_mode: ExtendMode::Pad,
        }
    }

    /// Radial Gradient を生成する。
    pub fn new_radial(x0: f64, y0: f64, x1: f64, y1: f64, r0: f64, r1: f64) -> Self {
        Self {
            values: GradientValues::Radial(RadialGradientValues {
                x0,
                y0,
                x1,
                y1,
                r0,
                r1,
            }),
            stops: Vec::new(),
            extend_mode: ExtendMode::Pad,
        }
    }

    /// Conic Gradient を生成する。
    pub fn new_conic(x0: f64, y0: f64, angle: f64) -> Self {
        Self {
            values: GradientValues::Conic(ConicGradientValues { x0, y0, angle }),
            stops: Vec::new(),
            extend_mode: ExtendMode::Pad,
        }
    }

    /// 色停止点を追加する。offset は 0.0-1.0 にクランプされる。
    pub fn add_stop(&mut self, offset: f64, color: Rgba32) -> &mut Self {
        self.stops.push(GradientStop {
            offset: offset.clamp(0.0, 1.0),
            color,
        });
        self.stops
            .sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap());
        self
    }

    /// 拡張モードを設定する。
    pub fn set_extend_mode(&mut self, mode: ExtendMode) -> &mut Self {
        self.extend_mode = mode;
        self
    }

    pub fn values(&self) -> &GradientValues {
        &self.values
    }

    pub fn stops(&self) -> &[GradientStop] {
        &self.stops
    }

    pub fn extend_mode(&self) -> ExtendMode {
        self.extend_mode
    }

    /// 描画用に事前計算されたグラデーション状態を生成する。
    ///
    /// matrix は Context の変換行列 (ユーザー空間 → デバイス空間)。
    /// グラデーション座標をデバイス空間に変換するために逆行列を使用する。
    pub(crate) fn prepare(&self, matrix: &Matrix2D) -> PreparedGradient {
        let lut = generate_lut(&self.stops);
        let inv = matrix.invert().unwrap_or(Matrix2D::IDENTITY);

        let kind = match &self.values {
            GradientValues::Linear(v) => {
                let dx = v.x1 - v.x0;
                let dy = v.y1 - v.y0;
                let len_sq = dx * dx + dy * dy;
                if len_sq < 1e-10 {
                    // 退化: 開始色で塗りつぶす
                    PreparedGradientKind::Linear {
                        dt_dx: 0.0,
                        dt_dy: 0.0,
                        t_origin: 0.0,
                    }
                } else {
                    // ユーザー空間での係数: t = a*ux + b*uy + c
                    let a = dx / len_sq;
                    let b = dy / len_sq;
                    let c = -(v.x0 * dx + v.y0 * dy) / len_sq;

                    // デバイス空間での係数に変換
                    PreparedGradientKind::Linear {
                        dt_dx: a * inv.m00 + b * inv.m01,
                        dt_dy: a * inv.m10 + b * inv.m11,
                        t_origin: a * inv.m20 + b * inv.m21 + c,
                    }
                }
            }
            GradientValues::Radial(v) => {
                let r_diff = v.r1 - v.r0;
                let inv_r_diff = if r_diff.abs() < 1e-10 {
                    0.0
                } else {
                    1.0 / r_diff
                };
                PreparedGradientKind::Radial {
                    cx: v.x0,
                    cy: v.y0,
                    r0: v.r0,
                    inv_r_diff,
                    dux_dx: inv.m00,
                    duy_dx: inv.m01,
                    dux_dy: inv.m10,
                    duy_dy: inv.m11,
                    ux_origin: inv.m20,
                    uy_origin: inv.m21,
                }
            }
            GradientValues::Conic(v) => PreparedGradientKind::Conic {
                cx: v.x0,
                cy: v.y0,
                angle_offset: v.angle,
                dux_dx: inv.m00,
                duy_dx: inv.m01,
                dux_dy: inv.m10,
                duy_dy: inv.m11,
                ux_origin: inv.m20,
                uy_origin: inv.m21,
            },
        };

        let lut_opaque = lut.iter().all(|&c| (c >> 24) == 0xFF);

        PreparedGradient {
            lut,
            extend_mode: self.extend_mode,
            kind,
            lut_opaque,
        }
    }
}

// =============================================================================
// 内部型
// =============================================================================

/// LUT サイズ。Blend2D のデフォルトに合わせる。
const LUT_SIZE: usize = 256;

/// 固定小数点の小数ビット数。
const FRAC_BITS: u32 = 16;

/// 描画用に事前計算されたグラデーション状態。
#[derive(Clone)]
pub(crate) struct PreparedGradient {
    lut: Vec<u32>,
    extend_mode: ExtendMode,
    kind: PreparedGradientKind,
    /// LUT の全エントリが完全不透明 (alpha=255) かどうか。
    /// true の場合、SrcOver 合成を省略して直接ストアできる。
    lut_opaque: bool,
}

#[derive(Clone)]
enum PreparedGradientKind {
    Linear {
        /// t の X 方向増分 (デバイス空間)。
        dt_dx: f64,
        /// t の Y 方向増分 (デバイス空間)。
        dt_dy: f64,
        /// (0, 0) での t 値。
        t_origin: f64,
    },
    Radial {
        /// ユーザー空間の中心 X (逆変換後の比較用)。
        cx: f64,
        /// ユーザー空間の中心 Y。
        cy: f64,
        r0: f64,
        inv_r_diff: f64,
        /// デバイス→ユーザー変換の X 方向増分。
        dux_dx: f64,
        duy_dx: f64,
        /// 行頭のユーザー座標計算用。
        dux_dy: f64,
        duy_dy: f64,
        ux_origin: f64,
        uy_origin: f64,
    },
    Conic {
        /// ユーザー空間の中心 X。
        cx: f64,
        /// ユーザー空間の中心 Y。
        cy: f64,
        angle_offset: f64,
        /// デバイス→ユーザー変換の X 方向増分。
        dux_dx: f64,
        duy_dx: f64,
        /// 行頭のユーザー座標計算用。
        dux_dy: f64,
        duy_dy: f64,
        ux_origin: f64,
        uy_origin: f64,
    },
}

impl PreparedGradient {
    /// スパンのグラデーション色を PRGB32 で計算する。
    ///
    /// (x_start, y) はデバイス座標。ピクセル中心 (+0.5) を使用する。
    pub(crate) fn fetch_span(&self, x_start: i32, y: i32, span: &mut [u32]) {
        match &self.kind {
            PreparedGradientKind::Linear {
                dt_dx,
                dt_dy,
                t_origin,
            } => {
                // ピクセル中心でのt値を計算
                let mut t = dt_dx * (x_start as f64 + 0.5) + dt_dy * (y as f64 + 0.5) + t_origin;
                for pixel in span.iter_mut() {
                    let idx = self.t_to_index(t);
                    *pixel = self.lut[idx];
                    t += dt_dx;
                }
            }
            PreparedGradientKind::Radial {
                cx,
                cy,
                r0,
                inv_r_diff,
                dux_dx,
                duy_dx,
                dux_dy,
                duy_dy,
                ux_origin,
                uy_origin,
            } => {
                // f32 に変換して内部ループを高速化する
                let cx_f = *cx as f32;
                let cy_f = *cy as f32;
                let r0_f = *r0 as f32;
                let inv_r_diff_f = *inv_r_diff as f32;
                let dux_dx_f = *dux_dx as f32;
                let duy_dx_f = *duy_dx as f32;
                let max_idx_f = (LUT_SIZE - 1) as f32;
                let px0 = x_start as f64 + 0.5;
                let py = y as f64 + 0.5;
                let mut ux = (dux_dx * px0 + dux_dy * py + ux_origin) as f32;
                let mut uy = (duy_dx * px0 + duy_dy * py + uy_origin) as f32;

                for pixel in span.iter_mut() {
                    let dx = ux - cx_f;
                    let dy = uy - cy_f;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let idx =
                        ((dist - r0_f) * inv_r_diff_f * max_idx_f).clamp(0.0, max_idx_f) as usize;
                    *pixel = self.lut[idx];
                    ux += dux_dx_f;
                    uy += duy_dx_f;
                }
            }
            PreparedGradientKind::Conic {
                cx,
                cy,
                angle_offset,
                dux_dx,
                duy_dx,
                dux_dy,
                duy_dy,
                ux_origin,
                uy_origin,
            } => {
                let cx_f = *cx as f32;
                let cy_f = *cy as f32;
                let angle_off_f = *angle_offset as f32;
                let dux_dx_f = *dux_dx as f32;
                let duy_dx_f = *duy_dx as f32;
                let inv_2pi_f: f32 = 1.0 / (2.0 * std::f32::consts::PI);
                let max_idx_f = (LUT_SIZE - 1) as f32;
                let px0 = x_start as f64 + 0.5;
                let py = y as f64 + 0.5;
                let mut ux = (dux_dx * px0 + dux_dy * py + ux_origin) as f32;
                let mut uy = (duy_dx * px0 + duy_dy * py + uy_origin) as f32;

                for pixel in span.iter_mut() {
                    let dx = ux - cx_f;
                    let dy = uy - cy_f;
                    let angle = fast_atan2_f32(dy, dx) - angle_off_f;
                    let t = angle * inv_2pi_f;
                    let idx = ((t - t.floor()) * max_idx_f) as usize;
                    *pixel = self.lut[idx.min(LUT_SIZE - 1)];
                    ux += dux_dx_f;
                    uy += duy_dx_f;
                }
            }
        }
    }

    /// グラデーションを矩形に直接描画する (融合 fetch + blend)。
    ///
    /// 種別に応じた最適化パスにディスパッチする。
    /// LUT が全不透明かどうか。
    pub(crate) fn is_opaque(&self) -> bool {
        self.lut_opaque
    }

    pub(crate) fn fill_rect(
        &self,
        dst: *mut u8,
        stride: usize,
        x0: i32,
        y0: i32,
        width: usize,
        height: usize,
    ) {
        match &self.kind {
            PreparedGradientKind::Linear { .. } => {
                self.fill_rect_linear(dst, stride, x0, y0, width, height);
            }
            PreparedGradientKind::Radial { .. } => {
                self.fill_rect_radial(dst, stride, x0, y0, width, height);
            }
            PreparedGradientKind::Conic { .. } => {
                self.fill_rect_conic(dst, stride, x0, y0, width, height);
            }
        }
    }

    /// Linear グラデーションを矩形に直接描画する。
    ///
    /// fetch (固定小数点 t → LUT) と blend (SrcOver) を融合し、
    /// 中間バッファへの書き込み・読み戻しによるキャッシュ汚染を排除する。
    fn fill_rect_linear(
        &self,
        dst: *mut u8,
        stride: usize,
        x0: i32,
        y0: i32,
        width: usize,
        height: usize,
    ) {
        let PreparedGradientKind::Linear {
            dt_dx,
            dt_dy,
            t_origin,
        } = &self.kind
        else {
            return;
        };

        let max_idx = (LUT_SIZE - 1) as f64;
        let scale = max_idx * ((1u64 << FRAC_BITS) as f64);

        let dt_dx_fixed = (dt_dx * scale) as i64;
        let dt_dy_fixed = (dt_dy * scale) as i64;

        // 最初の行、最初のピクセル中心での t (固定小数点)
        let t_row0 =
            ((dt_dx * (x0 as f64 + 0.5) + dt_dy * (y0 as f64 + 0.5) + t_origin) * scale) as i64;
        let t_row_start = t_row0;

        // LUT が全不透明の場合、SrcOver 合成を省略して直接ストアする。
        // 4 ピクセルアンロールで書き込みスループットを向上させる。
        if self.lut_opaque {
            self.fill_rect_linear_opaque(
                dst,
                stride,
                width,
                height,
                t_row_start,
                dt_dx_fixed,
                dt_dy_fixed,
            );
        } else {
            self.fill_rect_linear_blend(
                dst,
                stride,
                width,
                height,
                t_row_start,
                dt_dx_fixed,
                dt_dy_fixed,
            );
        }
    }

    /// LUT が全不透明の場合の高速パス。SrcOver 合成を省略して直接ストアする。
    fn fill_rect_linear_opaque(
        &self,
        dst: *mut u8,
        stride: usize,
        width: usize,
        height: usize,
        mut t_row_start: i64,
        dt_dx_fixed: i64,
        dt_dy_fixed: i64,
    ) {
        let lut = &self.lut;
        let dt_dx4 = dt_dx_fixed * 4;

        for row in 0..height {
            let dst_row = unsafe { (dst.add(row * stride)) as *mut u32 };

            match self.extend_mode {
                ExtendMode::Pad => {
                    let max_fixed = (LUT_SIZE as i64 - 1) << FRAC_BITS;
                    let simd_width = width / 4;
                    let remainder = width - simd_width * 4;

                    // 4 ピクセルアンロール
                    let mut t0 = t_row_start;
                    let mut t1 = t_row_start + dt_dx_fixed;
                    let mut t2 = t_row_start + dt_dx_fixed * 2;
                    let mut t3 = t_row_start + dt_dx_fixed * 3;

                    for chunk in 0..simd_width {
                        let x = chunk * 4;
                        let i0 = (t0.clamp(0, max_fixed) >> FRAC_BITS) as usize;
                        let i1 = (t1.clamp(0, max_fixed) >> FRAC_BITS) as usize;
                        let i2 = (t2.clamp(0, max_fixed) >> FRAC_BITS) as usize;
                        let i3 = (t3.clamp(0, max_fixed) >> FRAC_BITS) as usize;

                        unsafe {
                            *dst_row.add(x) = lut[i0];
                            *dst_row.add(x + 1) = lut[i1];
                            *dst_row.add(x + 2) = lut[i2];
                            *dst_row.add(x + 3) = lut[i3];
                        }

                        t0 += dt_dx4;
                        t1 += dt_dx4;
                        t2 += dt_dx4;
                        t3 += dt_dx4;
                    }

                    // 余りピクセル
                    let mut t = t0;
                    for x in (width - remainder)..width {
                        let idx = (t.clamp(0, max_fixed) >> FRAC_BITS) as usize;
                        unsafe {
                            *dst_row.add(x) = lut[idx];
                        }
                        t += dt_dx_fixed;
                    }
                }
                ExtendMode::Repeat => {
                    let cycle_mask = ((LUT_SIZE as u64) << FRAC_BITS) - 1;
                    let mut t = t_row_start;
                    for x in 0..width {
                        let idx = (((t as u64) & cycle_mask) >> FRAC_BITS) as usize;
                        unsafe {
                            *dst_row.add(x) = lut[idx];
                        }
                        t += dt_dx_fixed;
                    }
                }
                ExtendMode::Reflect => {
                    let cycle_mask = ((LUT_SIZE as u64 * 2) << FRAC_BITS) - 1;
                    let mut t = t_row_start;
                    for x in 0..width {
                        let t_abs = t.unsigned_abs();
                        let t_mod = ((t_abs & cycle_mask) >> FRAC_BITS) as usize;
                        let idx = if t_mod > 255 { 511 - t_mod } else { t_mod };
                        unsafe {
                            *dst_row.add(x) = lut[idx];
                        }
                        t += dt_dx_fixed;
                    }
                }
            }

            t_row_start += dt_dy_fixed;
        }
    }

    /// 半透明を含む LUT の場合の通常パス。SrcOver 合成を行う。
    fn fill_rect_linear_blend(
        &self,
        dst: *mut u8,
        stride: usize,
        width: usize,
        height: usize,
        mut t_row_start: i64,
        dt_dx_fixed: i64,
        dt_dy_fixed: i64,
    ) {
        let lut = &self.lut;

        for row in 0..height {
            let mut t = t_row_start;
            let dst_row = unsafe { (dst.add(row * stride)) as *mut u32 };

            match self.extend_mode {
                ExtendMode::Pad => {
                    let max_fixed = (LUT_SIZE as i64 - 1) << FRAC_BITS;
                    for x in 0..width {
                        let idx = (t.clamp(0, max_fixed) >> FRAC_BITS) as usize;
                        blend_pixel_src_over(dst_row, x, lut[idx]);
                        t += dt_dx_fixed;
                    }
                }
                ExtendMode::Repeat => {
                    let cycle_mask = ((LUT_SIZE as u64) << FRAC_BITS) - 1;
                    for x in 0..width {
                        let idx = (((t as u64) & cycle_mask) >> FRAC_BITS) as usize;
                        blend_pixel_src_over(dst_row, x, lut[idx]);
                        t += dt_dx_fixed;
                    }
                }
                ExtendMode::Reflect => {
                    let cycle_mask = ((LUT_SIZE as u64 * 2) << FRAC_BITS) - 1;
                    for x in 0..width {
                        let t_abs = t.unsigned_abs();
                        let t_mod = ((t_abs & cycle_mask) >> FRAC_BITS) as usize;
                        let idx = if t_mod > 255 { 511 - t_mod } else { t_mod };
                        blend_pixel_src_over(dst_row, x, lut[idx]);
                        t += dt_dx_fixed;
                    }
                }
            }

            t_row_start += dt_dy_fixed;
        }
    }

    /// Linear グラデーションのスパンを固定小数点で計算し、JIT span_cov 用のバッファに書き込む。
    /// Radial グラデーションを矩形に直接描画する。
    ///
    /// f32 演算 + 4 ピクセルアンロールで sqrt スループットを向上させる。
    /// LUT が 256 エントリなので f32 精度で十分。
    fn fill_rect_radial(
        &self,
        dst: *mut u8,
        stride: usize,
        x0: i32,
        y0: i32,
        width: usize,
        height: usize,
    ) {
        let PreparedGradientKind::Radial {
            cx,
            cy,
            r0,
            inv_r_diff,
            dux_dx,
            duy_dx,
            dux_dy,
            duy_dy,
            ux_origin,
            uy_origin,
        } = &self.kind
        else {
            return;
        };

        let lut = &self.lut;
        let opaque = self.lut_opaque;

        // f32 に変換 (LUT 256 エントリに対して十分な精度)
        let cx_f = *cx as f32;
        let cy_f = *cy as f32;
        let r0_f = *r0 as f32;
        let inv_r_diff_f = *inv_r_diff as f32;
        let dux_dx_f = *dux_dx as f32;
        let duy_dx_f = *duy_dx as f32;
        let max_idx_f = (LUT_SIZE - 1) as f32;

        let px0 = x0 as f64 + 0.5;

        for row in 0..height {
            let y = y0 + row as i32;
            let py = y as f64 + 0.5;
            let ux_start = (dux_dx * px0 + dux_dy * py + ux_origin) as f32;
            let uy_start = (duy_dx * px0 + duy_dy * py + uy_origin) as f32;
            let dst_row = unsafe { (dst.add(row * stride)) as *mut u32 };

            if opaque {
                self.fill_radial_row_opaque(
                    dst_row,
                    lut,
                    width,
                    ux_start,
                    uy_start,
                    cx_f,
                    cy_f,
                    r0_f,
                    inv_r_diff_f,
                    dux_dx_f,
                    duy_dx_f,
                    max_idx_f,
                );
            } else {
                self.fill_radial_row_blend(
                    dst_row,
                    lut,
                    width,
                    ux_start,
                    uy_start,
                    cx_f,
                    cy_f,
                    r0_f,
                    inv_r_diff_f,
                    dux_dx_f,
                    duy_dx_f,
                    max_idx_f,
                );
            }
        }
    }

    /// Radial 不透明行: f32 4px アンロール + 直接ストア。
    #[inline(always)]
    fn fill_radial_row_opaque(
        &self,
        dst_row: *mut u32,
        lut: &[u32],
        width: usize,
        mut ux0: f32,
        mut uy0: f32,
        cx: f32,
        cy: f32,
        r0: f32,
        inv_r_diff: f32,
        dux_dx: f32,
        duy_dx: f32,
        max_idx: f32,
    ) {
        let dux4 = dux_dx * 4.0;
        let duy4 = duy_dx * 4.0;
        let mut ux1 = ux0 + dux_dx;
        let mut ux2 = ux0 + dux_dx * 2.0;
        let mut ux3 = ux0 + dux_dx * 3.0;
        let mut uy1 = uy0 + duy_dx;
        let mut uy2 = uy0 + duy_dx * 2.0;
        let mut uy3 = uy0 + duy_dx * 3.0;

        let simd_width = width / 4;
        let remainder = width - simd_width * 4;

        for chunk in 0..simd_width {
            let x = chunk * 4;

            let dx0 = ux0 - cx;
            let dy0 = uy0 - cy;
            let dx1 = ux1 - cx;
            let dy1 = uy1 - cy;
            let dx2 = ux2 - cx;
            let dy2 = uy2 - cy;
            let dx3 = ux3 - cx;
            let dy3 = uy3 - cy;

            let d0 = (dx0 * dx0 + dy0 * dy0).sqrt();
            let d1 = (dx1 * dx1 + dy1 * dy1).sqrt();
            let d2 = (dx2 * dx2 + dy2 * dy2).sqrt();
            let d3 = (dx3 * dx3 + dy3 * dy3).sqrt();

            let i0 = ((d0 - r0) * inv_r_diff * max_idx).clamp(0.0, max_idx) as usize;
            let i1 = ((d1 - r0) * inv_r_diff * max_idx).clamp(0.0, max_idx) as usize;
            let i2 = ((d2 - r0) * inv_r_diff * max_idx).clamp(0.0, max_idx) as usize;
            let i3 = ((d3 - r0) * inv_r_diff * max_idx).clamp(0.0, max_idx) as usize;

            unsafe {
                *dst_row.add(x) = lut[i0];
                *dst_row.add(x + 1) = lut[i1];
                *dst_row.add(x + 2) = lut[i2];
                *dst_row.add(x + 3) = lut[i3];
            }

            ux0 += dux4;
            ux1 += dux4;
            ux2 += dux4;
            ux3 += dux4;
            uy0 += duy4;
            uy1 += duy4;
            uy2 += duy4;
            uy3 += duy4;
        }

        // 余りピクセル
        for i in 0..remainder {
            let x = width - remainder + i;
            let dx = ux0 - cx;
            let dy = uy0 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((dist - r0) * inv_r_diff * max_idx).clamp(0.0, max_idx) as usize;
            unsafe {
                *dst_row.add(x) = lut[idx];
            }
            ux0 += dux_dx;
            uy0 += duy_dx;
        }
    }

    /// Radial 半透明行: SrcOver 合成。
    #[inline(always)]
    fn fill_radial_row_blend(
        &self,
        dst_row: *mut u32,
        lut: &[u32],
        width: usize,
        mut ux: f32,
        mut uy: f32,
        cx: f32,
        cy: f32,
        r0: f32,
        inv_r_diff: f32,
        dux_dx: f32,
        duy_dx: f32,
        max_idx: f32,
    ) {
        for x in 0..width {
            let dx = ux - cx;
            let dy = uy - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let idx = ((dist - r0) * inv_r_diff * max_idx).clamp(0.0, max_idx) as usize;
            blend_pixel_src_over(dst_row, x, lut[idx]);
            ux += dux_dx;
            uy += duy_dx;
        }
    }

    /// Conic グラデーションを矩形に直接描画する。
    ///
    /// f32 演算 + 4 ピクセルアンロールで fast_atan2 のスループットを向上させる。
    fn fill_rect_conic(
        &self,
        dst: *mut u8,
        stride: usize,
        x0: i32,
        y0: i32,
        width: usize,
        height: usize,
    ) {
        let PreparedGradientKind::Conic {
            cx,
            cy,
            angle_offset,
            dux_dx,
            duy_dx,
            dux_dy,
            duy_dy,
            ux_origin,
            uy_origin,
        } = &self.kind
        else {
            return;
        };

        let lut = &self.lut;
        let opaque = self.lut_opaque;

        let cx_f = *cx as f32;
        let cy_f = *cy as f32;
        let angle_off_f = *angle_offset as f32;
        let dux_dx_f = *dux_dx as f32;
        let duy_dx_f = *duy_dx as f32;
        let inv_2pi_f: f32 = 1.0 / (2.0 * std::f32::consts::PI);
        let max_idx_f = (LUT_SIZE - 1) as f32;

        let px0 = x0 as f64 + 0.5;
        let dux4 = dux_dx_f * 4.0;
        let duy4 = duy_dx_f * 4.0;

        for row in 0..height {
            let y = y0 + row as i32;
            let py = y as f64 + 0.5;
            let ux_start = (dux_dx * px0 + dux_dy * py + ux_origin) as f32;
            let uy_start = (duy_dx * px0 + duy_dy * py + uy_origin) as f32;
            let dst_row = unsafe { (dst.add(row * stride)) as *mut u32 };

            let simd_width = width / 4;
            let remainder = width - simd_width * 4;

            let mut ux0 = ux_start;
            let mut ux1 = ux_start + dux_dx_f;
            let mut ux2 = ux_start + dux_dx_f * 2.0;
            let mut ux3 = ux_start + dux_dx_f * 3.0;
            let mut uy0 = uy_start;
            let mut uy1 = uy_start + duy_dx_f;
            let mut uy2 = uy_start + duy_dx_f * 2.0;
            let mut uy3 = uy_start + duy_dx_f * 3.0;

            for chunk in 0..simd_width {
                let x = chunk * 4;

                let a0 = fast_atan2_f32(uy0 - cy_f, ux0 - cx_f) - angle_off_f;
                let a1 = fast_atan2_f32(uy1 - cy_f, ux1 - cx_f) - angle_off_f;
                let a2 = fast_atan2_f32(uy2 - cy_f, ux2 - cx_f) - angle_off_f;
                let a3 = fast_atan2_f32(uy3 - cy_f, ux3 - cx_f) - angle_off_f;

                let t0 = a0 * inv_2pi_f;
                let t1 = a1 * inv_2pi_f;
                let t2 = a2 * inv_2pi_f;
                let t3 = a3 * inv_2pi_f;

                let i0 = ((t0 - t0.floor()) * max_idx_f) as usize;
                let i1 = ((t1 - t1.floor()) * max_idx_f) as usize;
                let i2 = ((t2 - t2.floor()) * max_idx_f) as usize;
                let i3 = ((t3 - t3.floor()) * max_idx_f) as usize;

                if opaque {
                    unsafe {
                        *dst_row.add(x) = lut[i0.min(255)];
                        *dst_row.add(x + 1) = lut[i1.min(255)];
                        *dst_row.add(x + 2) = lut[i2.min(255)];
                        *dst_row.add(x + 3) = lut[i3.min(255)];
                    }
                } else {
                    blend_pixel_src_over(dst_row, x, lut[i0.min(255)]);
                    blend_pixel_src_over(dst_row, x + 1, lut[i1.min(255)]);
                    blend_pixel_src_over(dst_row, x + 2, lut[i2.min(255)]);
                    blend_pixel_src_over(dst_row, x + 3, lut[i3.min(255)]);
                }

                ux0 += dux4;
                ux1 += dux4;
                ux2 += dux4;
                ux3 += dux4;
                uy0 += duy4;
                uy1 += duy4;
                uy2 += duy4;
                uy3 += duy4;
            }

            // 余りピクセル
            for i in 0..remainder {
                let x = width - remainder + i;
                let dx = ux0 - cx_f;
                let dy = uy0 - cy_f;
                let angle = fast_atan2_f32(dy, dx) - angle_off_f;
                let t = angle * inv_2pi_f;
                let idx = ((t - t.floor()) * max_idx_f) as usize;
                if opaque {
                    unsafe {
                        *dst_row.add(x) = lut[idx.min(255)];
                    }
                } else {
                    blend_pixel_src_over(dst_row, x, lut[idx.min(255)]);
                }
                ux0 += dux_dx_f;
                uy0 += duy_dx_f;
            }
        }
    }

    /// Radial グラデーションを JIT F32X4 SIMD で矩形に描画する (不透明 LUT 専用)。
    pub(crate) fn fill_rect_radial_jit(
        &self,
        dst: *mut u8,
        stride: usize,
        x0: i32,
        y0: i32,
        width: usize,
        height: usize,
        row_fn: crate::pipeline::cache::RadialGradientRowFn,
    ) {
        let PreparedGradientKind::Radial {
            cx,
            cy,
            r0,
            inv_r_diff,
            dux_dx,
            duy_dx,
            dux_dy,
            duy_dy,
            ux_origin,
            uy_origin,
        } = &self.kind
        else {
            return;
        };

        let cx_f = *cx as f32;
        let cy_f = *cy as f32;
        let r0_f = *r0 as f32;
        let inv_r_diff_max_f = (*inv_r_diff * (LUT_SIZE - 1) as f64) as f32;
        let dux_dx_f = *dux_dx as f32;
        let duy_dx_f = *duy_dx as f32;
        let lut_ptr = self.lut.as_ptr();
        let px0 = x0 as f64 + 0.5;

        for row in 0..height {
            let y = y0 + row as i32;
            let py = y as f64 + 0.5;
            let ux_start = (dux_dx * px0 + dux_dy * py + ux_origin) as f32;
            let uy_start = (duy_dx * px0 + duy_dy * py + uy_origin) as f32;
            let dst_row = unsafe { (dst.add(row * stride)) as *mut u32 };

            unsafe {
                row_fn(
                    dst_row,
                    lut_ptr,
                    width,
                    ux_start,
                    uy_start,
                    cx_f,
                    cy_f,
                    r0_f,
                    inv_r_diff_max_f,
                    dux_dx_f,
                    duy_dx_f,
                );
            }
        }
    }

    pub(crate) fn fetch_span_linear_fixed(&self, x_start: i32, y: i32, span: &mut [u32]) {
        let PreparedGradientKind::Linear {
            dt_dx,
            dt_dy,
            t_origin,
        } = &self.kind
        else {
            self.fetch_span(x_start, y, span);
            return;
        };

        let max_idx = (LUT_SIZE - 1) as f64;
        let scale = max_idx * ((1u64 << FRAC_BITS) as f64);

        let dt_dx_fixed = (dt_dx * scale) as i64;
        let mut t =
            ((dt_dx * (x_start as f64 + 0.5) + dt_dy * (y as f64 + 0.5) + t_origin) * scale) as i64;

        let lut = &self.lut;

        match self.extend_mode {
            ExtendMode::Pad => {
                let max_fixed = (max_idx as i64) << FRAC_BITS;
                for pixel in span.iter_mut() {
                    let idx = (t.clamp(0, max_fixed) >> FRAC_BITS) as usize;
                    *pixel = lut[idx];
                    t += dt_dx_fixed;
                }
            }
            ExtendMode::Repeat => {
                let cycle_mask = ((LUT_SIZE as u64) << FRAC_BITS) - 1;
                for pixel in span.iter_mut() {
                    let idx = (((t as u64) & cycle_mask) >> FRAC_BITS) as usize;
                    *pixel = lut[idx];
                    t += dt_dx_fixed;
                }
            }
            ExtendMode::Reflect => {
                let cycle_mask = ((LUT_SIZE as u64 * 2) << FRAC_BITS) - 1;
                for pixel in span.iter_mut() {
                    let t_abs = t.unsigned_abs();
                    let t_mod = ((t_abs & cycle_mask) >> FRAC_BITS) as usize;
                    let idx = if t_mod > 255 { 511 - t_mod } else { t_mod };
                    *pixel = lut[idx];
                    t += dt_dx_fixed;
                }
            }
        }
    }

    /// t 値を LUT インデックスに変換する。
    #[inline(always)]
    fn t_to_index(&self, t: f64) -> usize {
        let t = match self.extend_mode {
            ExtendMode::Pad => t.clamp(0.0, 1.0),
            ExtendMode::Repeat => {
                let t = t - t.floor();
                if t < 0.0 { t + 1.0 } else { t }
            }
            ExtendMode::Reflect => {
                let t = t.abs();
                let period = (t * 0.5).floor();
                let t = t - period * 2.0;
                if t > 1.0 { 2.0 - t } else { t }
            }
        };
        let idx = (t * (LUT_SIZE - 1) as f64).round() as usize;
        idx.min(LUT_SIZE - 1)
    }
}

// =============================================================================
// LUT 生成
// =============================================================================

/// 色停止点から PRGB32 形式の LUT を生成する。
fn generate_lut(stops: &[GradientStop]) -> Vec<u32> {
    if stops.is_empty() {
        return vec![0; LUT_SIZE];
    }
    if stops.len() == 1 {
        return vec![stops[0].color.to_prgb32(); LUT_SIZE];
    }

    let mut lut = Vec::with_capacity(LUT_SIZE);
    for i in 0..LUT_SIZE {
        let t = i as f64 / (LUT_SIZE - 1) as f64;
        let color = interpolate_stops(stops, t);
        lut.push(color);
    }
    lut
}

/// 色停止点間を補間して PRGB32 色を返す。
fn interpolate_stops(stops: &[GradientStop], t: f64) -> u32 {
    if t <= stops[0].offset {
        return stops[0].color.to_prgb32();
    }
    let last = stops.len() - 1;
    if t >= stops[last].offset {
        return stops[last].color.to_prgb32();
    }

    for i in 0..last {
        if t <= stops[i + 1].offset {
            let range = stops[i + 1].offset - stops[i].offset;
            if range < 1e-10 {
                return stops[i + 1].color.to_prgb32();
            }
            let local_t = (t - stops[i].offset) / range;
            return lerp_prgb32(stops[i].color, stops[i + 1].color, local_t);
        }
    }
    stops[last].color.to_prgb32()
}

// =============================================================================
// Rust スカラ合成関数
// =============================================================================
//
// fill_rect 等のカバレッジ不要なパスで使用する。
// LLVM の自動ベクタ化により、JIT スパンパイプラインと同等以上の性能が出る。

/// 1 ピクセルの SrcOver 合成。
#[inline(always)]
fn blend_pixel_src_over(dst_row: *mut u32, x: usize, src: u32) {
    let sa = src >> 24;
    if sa == 0 {
        return;
    }
    if sa == 255 {
        unsafe {
            *dst_row.add(x) = src;
        }
    } else {
        let d = unsafe { *dst_row.add(x) };
        let inv_sa = 256 - sa;
        let out_a = sa + ((((d >> 24) & 0xFF) * inv_sa) >> 8);
        let out_r = ((src >> 16) & 0xFF) + ((((d >> 16) & 0xFF) * inv_sa) >> 8);
        let out_g = ((src >> 8) & 0xFF) + ((((d >> 8) & 0xFF) * inv_sa) >> 8);
        let out_b = (src & 0xFF) + (((d & 0xFF) * inv_sa) >> 8);
        unsafe {
            *dst_row.add(x) = (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b;
        }
    }
}

// =============================================================================
// 高速 atan2 近似
// =============================================================================

/// atan2 の多項式近似。標準ライブラリの atan2 (~50-100 サイクル) の代わりに使用する。
///
/// 高速 atan2 近似 (f32)。
#[inline(always)]
fn fast_atan2_f32(y: f32, x: f32) -> f32 {
    let ax = x.abs();
    let ay = y.abs();

    if ax < 1e-7 && ay < 1e-7 {
        return 0.0;
    }

    let (z, base) = if ax >= ay {
        (ay / ax, 0.0f32)
    } else {
        (ax / ay, std::f32::consts::FRAC_PI_2)
    };

    let z2 = z * z;
    // atan(z) の minimax 多項式
    let p = -0.046_496_475_f32;
    let p = p * z2 + 0.159_314_22;
    let p = p * z2 - 0.327_622_76;
    let result = (p * z2 + 1.0) * z;

    let result = if ax >= ay { result } else { base - result };
    let result = if x < 0.0 {
        std::f32::consts::PI - result
    } else {
        result
    };
    if y < 0.0 { -result } else { result }
}

/// 2 色間を premultiplied ARGB32 空間で線形補間する。
fn lerp_prgb32(c0: Rgba32, c1: Rgba32, t: f64) -> u32 {
    let p0 = c0.to_prgb32();
    let p1 = c1.to_prgb32();

    // 8.8 固定小数点で補間
    let t_fixed = (t * 256.0) as u32;
    let inv_t = 256 - t_fixed;

    let a = (((p0 >> 24) & 0xFF) * inv_t + ((p1 >> 24) & 0xFF) * t_fixed) >> 8;
    let r = (((p0 >> 16) & 0xFF) * inv_t + ((p1 >> 16) & 0xFF) * t_fixed) >> 8;
    let g = (((p0 >> 8) & 0xFF) * inv_t + ((p1 >> 8) & 0xFF) * t_fixed) >> 8;
    let b = ((p0 & 0xFF) * inv_t + (p1 & 0xFF) * t_fixed) >> 8;

    (a << 24) | (r << 16) | (g << 8) | b
}
