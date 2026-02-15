mod format;

pub use format::PixelFormat;

/// RGBA を premultiplied ARGB (0xAARRGGBB) に変換する。
///
/// 各カラーチャネルに alpha を乗じ、+128 で四捨五入相当の丸めを行う。
pub const fn premultiply_rgba(r: u8, g: u8, b: u8, a: u8) -> u32 {
    let a32 = a as u32;
    let r_pre = ((r as u32) * a32 + 128) / 255;
    let g_pre = ((g as u32) * a32 + 128) / 255;
    let b_pre = ((b as u32) * a32 + 128) / 255;
    (a32 << 24) | (r_pre << 16) | (g_pre << 8) | b_pre
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_identity() {
        assert_eq!(premultiply_rgba(255, 128, 0, 255), 0xFF_FF_80_00);
    }

    #[test]
    fn transparent_zero() {
        assert_eq!(premultiply_rgba(255, 128, 64, 0), 0x00_00_00_00);
    }

    #[test]
    fn half_alpha() {
        let result = premultiply_rgba(200, 100, 50, 128);
        let a = (result >> 24) & 0xFF;
        let r = (result >> 16) & 0xFF;
        let g = (result >> 8) & 0xFF;
        let b = result & 0xFF;
        assert_eq!(a, 128);
        // (200 * 128 + 128) / 255 = 100
        assert_eq!(r, 100);
        // (100 * 128 + 128) / 255 = 50
        assert_eq!(g, 50);
        // (50 * 128 + 128) / 255 = 25
        assert_eq!(b, 25);
    }
}
