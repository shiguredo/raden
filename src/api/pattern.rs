// =============================================================================
// pattern.rs -- 画像パターン塗りつぶし
// =============================================================================
//
// Blend2D の BLPattern に対応する画像パターンスタイル。
// 画像をタイル状に繰り返してパスや矩形を塗りつぶす。
//
// Phase 1: 並進のみ、Nearest 補間、Pad/Repeat 拡張モード。

use crate::api::gradient::ExtendMode;

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
    /// X オフセット (並進)。
    tx: f64,
    /// Y オフセット (並進)。
    ty: f64,
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
            tx: 0.0,
            ty: 0.0,
            extend_mode: ExtendMode::Repeat,
        }
    }

    /// パターンの原点オフセットを設定する。
    pub fn set_origin(&mut self, tx: f64, ty: f64) -> &mut Self {
        self.tx = tx;
        self.ty = ty;
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

    /// 描画用に事前計算されたパターン状態を生成する。
    pub(crate) fn prepare(&self) -> PreparedPattern<'_> {
        // ソース画像の全不透明判定
        let opaque = self.data.chunks_exact(4).all(|px| px[3] == 0xFF);

        // Nearest サンプリングの整数ピクセル位相に合わせるため原点を丸める。
        // サブピクセル並進は表現しない (Bilinear 等を追加する場合は別経路で扱う)。
        PreparedPattern {
            data: &self.data,
            width: self.width as i32,
            height: self.height as i32,
            src_stride: self.src_stride,
            tx: self.tx.round() as i32,
            ty: self.ty.round() as i32,
            extend_mode: self.extend_mode,
            opaque,
        }
    }
}

/// 描画用に事前計算されたパターン状態。
pub(crate) struct PreparedPattern<'a> {
    data: &'a [u8],
    width: i32,
    height: i32,
    src_stride: usize,
    tx: i32,
    ty: i32,
    extend_mode: ExtendMode,
    opaque: bool,
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
            let sy = self.map_coord(dy - self.ty, sh);
            let dst_row = unsafe { (dst.add(row * dst_stride)) as *mut u32 };

            if sy < 0 {
                // Pad モードで範囲外 → 端のピクセル行を使用 (sy=0 or sh-1)
                continue;
            }

            let src_row = &self.data[sy as usize * self.src_stride..];

            if self.opaque {
                for x in 0..width {
                    let dx = x0 + x as i32;
                    let sx = self.map_coord(dx - self.tx, sw);
                    if sx >= 0 {
                        let pixel =
                            u32::from_ne_bytes(src_row[sx as usize * 4..][..4].try_into().unwrap());
                        unsafe {
                            *dst_row.add(x) = pixel;
                        }
                    }
                }
            } else {
                for x in 0..width {
                    let dx = x0 + x as i32;
                    let sx = self.map_coord(dx - self.tx, sw);
                    if sx >= 0 {
                        let pixel =
                            u32::from_ne_bytes(src_row[sx as usize * 4..][..4].try_into().unwrap());
                        blend_pixel_src_over(dst_row, x, pixel);
                    }
                }
            }
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

        let sy = self.map_coord(y - self.ty, sh);
        if sy < 0 {
            span.fill(0);
            return;
        }

        let src_row = &self.data[sy as usize * self.src_stride..];

        for (i, pixel) in span.iter_mut().enumerate() {
            let dx = x_start + i as i32;
            let sx = self.map_coord(dx - self.tx, sw);
            if sx >= 0 {
                *pixel = u32::from_ne_bytes(src_row[sx as usize * 4..][..4].try_into().unwrap());
            } else {
                *pixel = 0;
            }
        }
    }

    /// 座標を拡張モードに従ってマッピングする。
    /// 範囲内なら 0..size-1 の値を返す。Pad モードで範囲外なら -1。
    #[inline(always)]
    fn map_coord(&self, coord: i32, size: i32) -> i32 {
        match self.extend_mode {
            ExtendMode::Pad => coord.clamp(0, size - 1),
            ExtendMode::Repeat => ((coord % size) + size) % size,
            ExtendMode::Reflect => {
                let double = size * 2;
                let c = ((coord % double) + double) % double;
                if c >= size { double - 1 - c } else { c }
            }
        }
    }
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
