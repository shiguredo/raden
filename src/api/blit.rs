//! 画像転送 (blit) の内部実装。

use crate::api::image::Image;
use crate::api::matrix::Matrix2D;
use crate::api::style::CompOp;
use crate::pixel::PixelFormat;

use crate::api::context::Rect;

/// デバイス座標 (ix, iy) をユーザ空間へ写し、dst 矩形内を src 矩形へ写像してサンプルする Nearest 転送。
pub(crate) fn blit_image_rect_scoped(
    dst: &mut Image,
    dst_rect_user: &Rect,
    src: &Image,
    src_rect: Option<Rect>,
    matrix: &Matrix2D,
    clip_x0: i32,
    clip_y0: i32,
    clip_x1: i32,
    clip_y1: i32,
    comp_op: CompOp,
) {
    assert!(
        matches!(src.format(), PixelFormat::Prgb32 | PixelFormat::Xrgb32),
        "blit source must be Prgb32 or Xrgb32"
    );
    assert!(
        matches!(
            dst.format(),
            PixelFormat::Prgb32 | PixelFormat::Xrgb32 | PixelFormat::A8
        ),
        "blit destination must be Prgb32, Xrgb32, or A8"
    );
    assert!(
        matches!(comp_op, CompOp::SrcOver | CompOp::SrcCopy),
        "blit supports only SrcOver and SrcCopy"
    );

    let sr = src_rect.unwrap_or_else(|| {
        Rect::new(
            0.0,
            0.0,
            src.width() as f64,
            src.height() as f64,
        )
    });

    if dst_rect_user.w <= 0.0
        || dst_rect_user.h <= 0.0
        || sr.w <= 0.0
        || sr.h <= 0.0
    {
        return;
    }

    let Some(inv) = matrix.invert() else {
        return;
    };

    let dst_stride = dst.stride();
    let dst_base = dst.data_ptr_mut();
    let dst_fmt = dst.format();
    let dst_bpp = dst_fmt.bytes_per_pixel();

    let src_stride = src.stride();
    let src_data = src.data();
    let sw = src.width() as i32;
    let sh = src.height() as i32;

    for iy in clip_y0..clip_y1 {
        for ix in clip_x0..clip_x1 {
            let (ux, uy) = inv.map_point(ix as f64 + 0.5, iy as f64 + 0.5);
            if ux < dst_rect_user.x
                || ux >= dst_rect_user.x + dst_rect_user.w
                || uy < dst_rect_user.y
                || uy >= dst_rect_user.y + dst_rect_user.h
            {
                continue;
            }
            let tu = sr.x + (ux - dst_rect_user.x) / dst_rect_user.w * sr.w;
            let tv = sr.y + (uy - dst_rect_user.y) / dst_rect_user.h * sr.h;
            let sx = tu.floor() as i32;
            let sy = tv.floor() as i32;
            if sx < 0 || sy < 0 || sx >= sw || sy >= sh {
                continue;
            }
            let sp = read_prgb32(src_data, src_stride, sx as u32, sy as u32);
            let off = iy as usize * dst_stride + ix as usize * dst_bpp;
            let dp = unsafe { dst_base.add(off) };
            match dst_fmt {
                PixelFormat::Prgb32 | PixelFormat::Xrgb32 => {
                    let dst_u32 = dp.cast::<u32>();
                    match comp_op {
                        CompOp::SrcCopy => unsafe {
                            *dst_u32 = sp;
                        },
                        CompOp::SrcOver => {
                            blend_prgb32_src_over(dst_u32, sp);
                        }
                        _ => {}
                    }
                }
                PixelFormat::A8 => match comp_op {
                    CompOp::SrcCopy => {
                        unsafe {
                            *dp = (sp >> 24) as u8;
                        }
                    }
                    CompOp::SrcOver => {
                        let sa = (sp >> 24) & 0xFF;
                        if sa == 0 {
                            continue;
                        }
                        unsafe {
                            let d = *dp as u32;
                            let out = sa + (d * (255 - sa)) / 255;
                            *dp = out.min(255) as u8;
                        }
                    }
                    _ => {}
                },
            }
        }
    }
}

#[inline]
fn read_prgb32(data: &[u8], stride: usize, x: u32, y: u32) -> u32 {
    let o = y as usize * stride + x as usize * 4;
    u32::from_le_bytes(data[o..o + 4].try_into().unwrap())
}

fn blend_prgb32_src_over(dst: *mut u32, src: u32) {
    let sa = src >> 24;
    if sa == 0 {
        return;
    }
    if sa == 255 {
        unsafe {
            *dst = src;
        }
        return;
    }
    unsafe {
        let d = *dst;
        let inv_sa = 256 - sa;
        let out_a = sa + ((((d >> 24) & 0xFF) * inv_sa) >> 8);
        let out_r = ((src >> 16) & 0xFF) + ((((d >> 16) & 0xFF) * inv_sa) >> 8);
        let out_g = ((src >> 8) & 0xFF) + ((((d >> 8) & 0xFF) * inv_sa) >> 8);
        let out_b = (src & 0xFF) + (((d & 0xFF) * inv_sa) >> 8);
        *dst = (out_a << 24) | (out_r << 16) | (out_g << 8) | out_b;
    }
}
