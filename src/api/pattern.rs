// =============================================================================
// pattern.rs -- 画像パターン塗りつぶし
// =============================================================================
//
// Blend2D の BLPattern に対応する画像パターンスタイル。
// 画像をタイル状に繰り返してパスや矩形を塗りつぶす。
//
// ユーザ空間からテクスチャ空間への変換は `transform`、デバイス座標への適用は
// `prepare(matrix)` で `transform * inv(matrix)` を合成する。

use crate::api::gradient::ExtendMode;
use crate::api::matrix::Matrix2D;
use crate::api::style::CompOp;

/// パターンのピクセル補間モード。Blend2D の `BLPatternQuality` に相当。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatternFilter {
    /// 最近傍。
    #[default]
    Nearest,
    /// 双一次補間。
    Bilinear,
}

/// 画像パターン。
///
/// ソース画像の一部または全体をタイルとして繰り返す。
/// Blend2D の BLPattern に対応。
#[derive(Debug, Clone)]
pub struct Pattern {
    /// ソース画像のピクセルデータ (PRGB32, 4 バイト/ピクセル)。
    data: Vec<u8>,
    /// ソース画像の幅。
    width: u32,
    /// ソース画像の高さ。
    height: u32,
    /// ソース画像のストライド。
    src_stride: usize,
    /// ユーザ空間からテクスチャ空間 (連続座標) への変換。
    /// 既定は単位行列。`set_origin` は `translation(-tx, -ty)` に相当する。
    transform: Matrix2D,
    /// 補間モード。
    filter: PatternFilter,
    /// 拡張モード。
    extend_mode: ExtendMode,
}

impl Pattern {
    /// Image のデータからパターンを生成する。
    ///
    /// data は PRGB32 形式のピクセルデータ、stride はソース画像のストライド。
    pub fn new(data: &[u8], width: u32, height: u32, stride: usize) -> Self {
        Self {
            data: data.to_vec(),
            width,
            height,
            src_stride: stride,
            transform: Matrix2D::IDENTITY,
            filter: PatternFilter::Nearest,
            extend_mode: ExtendMode::Repeat,
        }
    }

    /// パターンの原点オフセットを設定する。
    ///
    /// デバイス座標 (dx, dy) でテクスチャ (0,0) が現れる位置を (tx, ty) に合わせる。
    /// 内部では `transform = translation(-tx, -ty)` として扱う。
    /// `set_transform` と併用する場合、**呼び出し順に依存せず、後から呼んだ方が `transform` 全体を置き換える**。
    pub fn set_origin(&mut self, tx: f64, ty: f64) -> &mut Self {
        self.transform = Matrix2D::translation(-tx, -ty);
        self
    }

    /// ユーザ空間からテクスチャ空間への変換行列を設定する。
    ///
    /// `set_origin` と併用する場合、**後から呼んだ方が `transform` 全体を置き換える**（合成ではない）。
    pub fn set_transform(&mut self, m: Matrix2D) -> &mut Self {
        self.transform = m;
        self
    }

    /// 補間モードを設定する。
    pub fn set_filter(&mut self, filter: PatternFilter) -> &mut Self {
        self.filter = filter;
        self
    }

    /// 拡張モードを設定する。
    pub fn set_extend_mode(&mut self, mode: ExtendMode) -> &mut Self {
        self.extend_mode = mode;
        self
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn extend_mode(&self) -> ExtendMode {
        self.extend_mode
    }

    pub fn filter(&self) -> PatternFilter {
        self.filter
    }

    pub fn transform(&self) -> Matrix2D {
        self.transform
    }

    /// 描画用に事前計算されたパターン状態を生成する。
    ///
    /// `matrix` は Context のユーザ→デバイス行列。`pattern * inv(matrix)` で
    /// デバイス座標からテクスチャ座標へ写す。
    ///
    /// `comp_op` は `fill_rect` 経路でのみ参照する。`fill_path` は `fetch_span` がソース色のみ返し、
    /// 合成はパイプライン側で行う。
    pub(crate) fn prepare(&self, matrix: &Matrix2D, comp_op: CompOp) -> PreparedPattern<'_> {
        // 行が stride 幅でパディングされていても、先頭 width×height の 4 バイト単位がピクセルとみなす。
        // 末尾に不足ピクセルがある場合は chunks_exact で末尾を無視する（不正なバッファでは不透明判定がずれる）。
        let opaque = self.data.chunks_exact(4).all(|px| px[3] == 0xFF);
        let inv = matrix.invert().unwrap_or(Matrix2D::IDENTITY);
        let to_tex = self.transform.multiply(&inv);

        PreparedPattern {
            data: &self.data,
            width: self.width as i32,
            height: self.height as i32,
            src_stride: self.src_stride,
            extend_mode: self.extend_mode,
            opaque,
            to_tex,
            filter: self.filter,
            comp_op,
        }
    }
}

/// 描画用に事前計算されたパターン状態。
pub(crate) struct PreparedPattern<'a> {
    data: &'a [u8],
    width: i32,
    height: i32,
    src_stride: usize,
    extend_mode: ExtendMode,
    opaque: bool,
    /// デバイス座標からテクスチャ空間 (連続) への変換。
    to_tex: Matrix2D,
    filter: PatternFilter,
    /// `fill_rect` のパターン塗りでのみ使用。`fetch_span` では使わない。
    comp_op: CompOp,
}

impl<'a> PreparedPattern<'a> {
    /// パターンを矩形に直接描画する (融合 fetch + blend)。
    pub(crate) fn fill_rect(
        &self,
        dst: *mut u8,
        dst_stride: usize,
        x0: i32,
        y0: i32,
        width: usize,
        height: usize,
    ) {
        let sw = self.width;
        let sh = self.height;
        if sw <= 0 || sh <= 0 {
            return;
        }

        for row in 0..height {
            let dy = y0 + row as i32;
            let dst_row = unsafe { (dst.add(row * dst_stride)) as *mut u32 };

            for x in 0..width {
                let dx = x0 + x as i32;
                let (tx, ty) = self
                    .to_tex
                    .map_point(dx as f64, dy as f64);
                let pixel = self.sample_prgb32(tx, ty);
                self.write_pixel_fill_rect(dst_row, x, pixel);
            }
        }
    }

    /// `fill_rect` 用: `comp_op` に応じて 1 ピクセル書き込む。
    fn write_pixel_fill_rect(&self, dst_row: *mut u32, x: usize, pixel: u32) {
        match self.comp_op {
            CompOp::SrcCopy => unsafe {
                *dst_row.add(x) = pixel;
            },
            CompOp::SrcOver => {
                if self.opaque {
                    unsafe {
                        *dst_row.add(x) = pixel;
                    }
                } else {
                    blend_pixel_src_over(dst_row, x, pixel);
                }
            }
            _ => panic!(
                "CompOp {:?} is not supported for pattern fill_rect; use SrcOver, SrcCopy, or fill_path",
                self.comp_op
            ),
        }
    }

    /// パターンスパンを計算してバッファに書き込む。fill_path 用。
    pub(crate) fn fetch_span(&self, x_start: i32, y: i32, span: &mut [u32]) {
        let sw = self.width;
        let sh = self.height;
        if sw <= 0 || sh <= 0 {
            span.fill(0);
            return;
        }

        for (i, pixel) in span.iter_mut().enumerate() {
            let dx = x_start + i as i32;
            let (tx, ty) = self
                .to_tex
                .map_point(dx as f64, y as f64);
            *pixel = self.sample_prgb32(tx, ty);
        }
    }

    #[inline]
    fn sample_prgb32(&self, tx: f64, ty: f64) -> u32 {
        match self.filter {
            PatternFilter::Nearest => self.sample_nearest(tx, ty),
            PatternFilter::Bilinear => self.sample_bilinear(tx, ty),
        }
    }

    fn read_px(&self, sx: i32, sy: i32) -> u32 {
        let row = &self.data[sy as usize * self.src_stride..];
        u32::from_ne_bytes(row[sx as usize * 4..][..4].try_into().unwrap())
    }

    fn sample_nearest(&self, tx: f64, ty: f64) -> u32 {
        let sx = self.nearest_index(tx, self.width);
        let sy = self.nearest_index(ty, self.height);
        self.read_px(sx, sy)
    }

    /// 連続座標を最近傍ピクセルインデックスに変換する (拡張モード適用)。
    fn nearest_index(&self, coord: f64, size: i32) -> i32 {
        let s = size as f64;
        match self.extend_mode {
            ExtendMode::Pad => {
                let c = coord.clamp(0.0, s - 1.0);
                (c + 0.5).floor() as i32
            }
            ExtendMode::Repeat => {
                let u = coord.rem_euclid(s);
                let idx = (u + 0.5).floor() as i32;
                idx.rem_euclid(size)
            }
            ExtendMode::Reflect => {
                let u = reflect_coord_float(coord, s);
                (u + 0.5).floor().clamp(0.0, s - 1.0) as i32
            }
        }
    }

    fn sample_bilinear(&self, tx: f64, ty: f64) -> u32 {
        let w = self.width;
        let h = self.height;
        let wf = w as f64;
        let hf = h as f64;

        match self.extend_mode {
            ExtendMode::Pad => {
                let px = tx.clamp(0.0, wf - 1.0);
                let py = ty.clamp(0.0, hf - 1.0);
                let x0 = px.floor() as i32;
                let y0 = py.floor() as i32;
                let x1 = (x0 + 1).min(w - 1);
                let y1 = (y0 + 1).min(h - 1);
                let fx = px - x0 as f64;
                let fy = py - y0 as f64;
                self.bilerp(x0, y0, x1, y1, fx, fy)
            }
            ExtendMode::Repeat => {
                let px = tx.rem_euclid(wf);
                let py = ty.rem_euclid(hf);
                bilinear_repeat(
                    px,
                    py,
                    w,
                    h,
                    |ix, iy| self.read_px(ix, iy),
                )
            }
            ExtendMode::Reflect => {
                let px = reflect_coord_float(tx, wf);
                let py = reflect_coord_float(ty, hf);
                let x0 = px.floor() as i32;
                let y0 = py.floor() as i32;
                let x1 = (x0 + 1).min(w - 1);
                let y1 = (y0 + 1).min(h - 1);
                let fx = px - x0 as f64;
                let fy = py - y0 as f64;
                self.bilerp(x0, y0, x1, y1, fx, fy)
            }
        }
    }

    fn bilerp(&self, x0: i32, y0: i32, x1: i32, y1: i32, fx: f64, fy: f64) -> u32 {
        let c00 = self.read_px(x0, y0);
        let c10 = self.read_px(x1, y0);
        let c01 = self.read_px(x0, y1);
        let c11 = self.read_px(x1, y1);
        lerp_rgba_premul(c00, c10, c01, c11, fx, fy)
    }
}

/// 反射モード用: 連続座標を [0, size) に折り返す。
fn reflect_coord_float(coord: f64, size: f64) -> f64 {
    if size <= 1.0 {
        return 0.0;
    }
    let double = size * 2.0;
    let c = coord.rem_euclid(double);
    if c >= size {
        double - 1.0 - c
    } else {
        c
    }
}

fn bilinear_repeat(
    px: f64,
    py: f64,
    w: i32,
    h: i32,
    mut get: impl FnMut(i32, i32) -> u32,
) -> u32 {
    let x0 = px.floor() as i32;
    let y0 = py.floor() as i32;
    let fx = px - x0 as f64;
    let fy = py - y0 as f64;
    let x1 = (x0 + 1).rem_euclid(w);
    let y1 = (y0 + 1).rem_euclid(h);
    let x0w = x0.rem_euclid(w);
    let y0w = y0.rem_euclid(h);
    let c00 = get(x0w, y0w);
    let c10 = get(x1, y0w);
    let c01 = get(x0w, y1);
    let c11 = get(x1, y1);
    lerp_rgba_premul(c00, c10, c01, c11, fx, fy)
}

/// PRGB32 前提の双一次補間 (チャンネルごとに線形、結果は丸めて整数に戻す)。
fn lerp_rgba_premul(c00: u32, c10: u32, c01: u32, c11: u32, fx: f64, fy: f64) -> u32 {
    let lerp_chan = |a: u32, b: u32| -> u32 {
        let v = a as f64 * (1.0 - fx) + b as f64 * fx;
        v.round().clamp(0.0, 255.0) as u32
    };

    let a0 = lerp_chan(c00 >> 24, c10 >> 24);
    let r0 = lerp_chan((c00 >> 16) & 0xFF, (c10 >> 16) & 0xFF);
    let g0 = lerp_chan((c00 >> 8) & 0xFF, (c10 >> 8) & 0xFF);
    let b0 = lerp_chan(c00 & 0xFF, c10 & 0xFF);

    let a1 = lerp_chan(c01 >> 24, c11 >> 24);
    let r1 = lerp_chan((c01 >> 16) & 0xFF, (c11 >> 16) & 0xFF);
    let g1 = lerp_chan((c01 >> 8) & 0xFF, (c11 >> 8) & 0xFF);
    let b1 = lerp_chan(c01 & 0xFF, c11 & 0xFF);

    let lerp2 = |u: u32, v: u32| -> u32 {
        let x = u as f64 * (1.0 - fy) + v as f64 * fy;
        x.round().clamp(0.0, 255.0) as u32
    };

    let a = lerp2(a0, a1);
    let r = lerp2(r0, r1);
    let g = lerp2(g0, g1);
    let b = lerp2(b0, b1);
    (a << 24) | (r << 16) | (g << 8) | b
}

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
