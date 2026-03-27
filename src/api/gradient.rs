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
            GradientValues::Radial(v) => PreparedGradientKind::Radial {
                cx: v.x0,
                cy: v.y0,
                r0: v.r0,
                r_diff: v.r1 - v.r0,
                inv,
            },
            GradientValues::Conic(v) => PreparedGradientKind::Conic {
                cx: v.x0,
                cy: v.y0,
                angle_offset: v.angle,
                inv,
            },
        };

        PreparedGradient {
            lut,
            extend_mode: self.extend_mode,
            kind,
        }
    }
}

// =============================================================================
// 内部型
// =============================================================================

/// LUT サイズ。Blend2D のデフォルトに合わせる。
const LUT_SIZE: usize = 256;

/// 描画用に事前計算されたグラデーション状態。
#[derive(Clone)]
pub(crate) struct PreparedGradient {
    lut: Vec<u32>,
    extend_mode: ExtendMode,
    kind: PreparedGradientKind,
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
        cx: f64,
        cy: f64,
        r0: f64,
        r_diff: f64,
        inv: Matrix2D,
    },
    Conic {
        cx: f64,
        cy: f64,
        angle_offset: f64,
        inv: Matrix2D,
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
                r_diff,
                inv,
            } => {
                for (i, pixel) in span.iter_mut().enumerate() {
                    let px = (x_start + i as i32) as f64 + 0.5;
                    let py = y as f64 + 0.5;
                    let ux = inv.m00 * px + inv.m10 * py + inv.m20;
                    let uy = inv.m01 * px + inv.m11 * py + inv.m21;

                    let dx = ux - cx;
                    let dy = uy - cy;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let t = if r_diff.abs() < 1e-10 {
                        if dist <= *r0 { 0.0 } else { 1.0 }
                    } else {
                        (dist - r0) / r_diff
                    };
                    let idx = self.t_to_index(t);
                    *pixel = self.lut[idx];
                }
            }
            PreparedGradientKind::Conic {
                cx,
                cy,
                angle_offset,
                inv,
            } => {
                for (i, pixel) in span.iter_mut().enumerate() {
                    let px = (x_start + i as i32) as f64 + 0.5;
                    let py = y as f64 + 0.5;
                    let ux = inv.m00 * px + inv.m10 * py + inv.m20;
                    let uy = inv.m01 * px + inv.m11 * py + inv.m21;

                    let angle = (uy - cy).atan2(ux - cx) - angle_offset;
                    // [0, 1) に正規化
                    let t = angle / (2.0 * std::f64::consts::PI);
                    let t = t - t.floor();
                    let idx = self.t_to_index(t);
                    *pixel = self.lut[idx];
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

/// SrcOver: out = src + dst * (1 - srcA)
///
/// 全ピクセルが完全にカバーされる場合 (coverage=255) に使用する。
/// LLVM が自動ベクタ化するため、Cranelift JIT 版と同等以上の性能。
pub(crate) fn blend_span_src_over(dst: *mut u8, src_span: &[u32]) {
    let dst_pixels = dst as *mut u32;
    for (i, &s) in src_span.iter().enumerate() {
        let sa = s >> 24;
        if sa == 0 {
            continue;
        }
        if sa == 255 {
            unsafe {
                *dst_pixels.add(i) = s;
            }
        } else {
            let d = unsafe { *dst_pixels.add(i) };
            let inv_sa = 256 - sa;
            let out_a = sa + ((((d >> 24) & 0xFF) * inv_sa) >> 8);
            let out_r = ((s >> 16) & 0xFF) + ((((d >> 16) & 0xFF) * inv_sa) >> 8);
            let out_g = ((s >> 8) & 0xFF) + ((((d >> 8) & 0xFF) * inv_sa) >> 8);
            let out_b = (s & 0xFF) + (((d & 0xFF) * inv_sa) >> 8);
            unsafe {
                *dst_pixels.add(i) = (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b;
            }
        }
    }
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
