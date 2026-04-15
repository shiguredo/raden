//! A8 宛て先のスカラ合成。JIT は 4 バイト/ピクセル前提のため、A8 のみ本モジュールで処理する。

#![allow(unsafe_op_in_unsafe_fn)]

use crate::api::style::CompOp;
use crate::pipeline::cache::{PipelineBoxFn, PipelineCovFn, PipelineFn};
use crate::pixel::PixelFormat;

/// JIT 用キーに載せる形式を正規化する。`Xrgb32` は `Prgb32` と同一 JIT を共有する。
pub fn jit_dst_format(fmt: PixelFormat) -> PixelFormat {
    match fmt {
        PixelFormat::Xrgb32 => PixelFormat::Prgb32,
        PixelFormat::Prgb32 => PixelFormat::Prgb32,
        PixelFormat::A8 => {
            panic!("jit_dst_format must not be called for A8 destination");
        }
    }
}

unsafe extern "C" fn a8_src_over_line(dst: *mut u8, src: u32, count: usize) {
    let sa = (src >> 24) & 0xFF;
    if sa == 0 {
        return;
    }
    if sa == 255 {
        for i in 0..count {
            *dst.add(i) = 0xFF;
        }
        return;
    }
    for i in 0..count {
        let d = *dst.add(i) as u32;
        let out = sa + (d * (255 - sa)) / 255;
        *dst.add(i) = out.min(255) as u8;
    }
}

unsafe extern "C" fn a8_src_copy_line(dst: *mut u8, src: u32, count: usize) {
    let sa = ((src >> 24) & 0xFF) as u8;
    for i in 0..count {
        *dst.add(i) = sa;
    }
}

unsafe extern "C" fn a8_clear_line(dst: *mut u8, _src: u32, count: usize) {
    for i in 0..count {
        *dst.add(i) = 0;
    }
}

unsafe extern "C" fn a8_plus_line(dst: *mut u8, src: u32, count: usize) {
    let sa = (src >> 24) & 0xFF;
    for i in 0..count {
        let d = *dst.add(i) as u32;
        let out = (sa + d).min(255);
        *dst.add(i) = out as u8;
    }
}

unsafe extern "C" fn a8_src_over_cov(dst: *mut u8, src: u32, count: usize, coverage: *const u8) {
    let sa0 = (src >> 24) & 0xFF;
    for i in 0..count {
        let cov = *coverage.add(i) as u32;
        if cov == 0 {
            continue;
        }
        let sa = (sa0 * cov) / 255;
        if sa == 0 {
            continue;
        }
        let d = *dst.add(i) as u32;
        let out = sa + (d * (255 - sa)) / 255;
        *dst.add(i) = out.min(255) as u8;
    }
}

unsafe extern "C" fn a8_src_copy_cov(dst: *mut u8, src: u32, count: usize, coverage: *const u8) {
    let sa0 = (src >> 24) & 0xFF;
    for i in 0..count {
        let cov = *coverage.add(i) as u32;
        if cov == 0 {
            continue;
        }
        let sa = (sa0 * cov) / 255;
        *dst.add(i) = sa.min(255) as u8;
    }
}

unsafe extern "C" fn a8_clear_cov(dst: *mut u8, _src: u32, count: usize, coverage: *const u8) {
    for i in 0..count {
        let cov = *coverage.add(i) as u32;
        if cov == 0 {
            continue;
        }
        let d = *dst.add(i) as u32;
        let cleared = (d * (255 - cov)) / 255;
        *dst.add(i) = cleared.min(255) as u8;
    }
}

unsafe extern "C" fn a8_plus_cov(dst: *mut u8, src: u32, count: usize, coverage: *const u8) {
    let sa0 = (src >> 24) & 0xFF;
    for i in 0..count {
        let cov = *coverage.add(i) as u32;
        if cov == 0 {
            continue;
        }
        let sa = (sa0 * cov) / 255;
        let d = *dst.add(i) as u32;
        let out = (sa + d).min(255);
        *dst.add(i) = out as u8;
    }
}

unsafe extern "C" fn a8_box_src_over(
    dst: *mut u8,
    src: u32,
    width: usize,
    height: usize,
    stride: usize,
) {
    for row in 0..height {
        let p = dst.add(row * stride);
        a8_src_over_line(p, src, width);
    }
}

unsafe extern "C" fn a8_box_src_copy(
    dst: *mut u8,
    src: u32,
    width: usize,
    height: usize,
    stride: usize,
) {
    for row in 0..height {
        let p = dst.add(row * stride);
        a8_src_copy_line(p, src, width);
    }
}

unsafe extern "C" fn a8_box_clear(
    dst: *mut u8,
    _src: u32,
    width: usize,
    height: usize,
    stride: usize,
) {
    for row in 0..height {
        let p = dst.add(row * stride);
        a8_clear_line(p, 0, width);
    }
}

unsafe extern "C" fn a8_box_plus(
    dst: *mut u8,
    src: u32,
    width: usize,
    height: usize,
    stride: usize,
) {
    for row in 0..height {
        let p = dst.add(row * stride);
        a8_plus_line(p, src, width);
    }
}

pub fn pipeline_fn(op: CompOp) -> PipelineFn {
    match op {
        CompOp::SrcOver => a8_src_over_line,
        CompOp::SrcCopy => a8_src_copy_line,
        CompOp::Clear => a8_clear_line,
        CompOp::Plus => a8_plus_line,
        _ => panic!("CompOp is not supported for A8 destination"),
    }
}

pub fn pipeline_cov_fn(op: CompOp) -> PipelineCovFn {
    match op {
        CompOp::SrcOver => a8_src_over_cov,
        CompOp::SrcCopy => a8_src_copy_cov,
        CompOp::Clear => a8_clear_cov,
        CompOp::Plus => a8_plus_cov,
        _ => panic!("CompOp is not supported for A8 destination"),
    }
}

pub fn pipeline_box_fn(op: CompOp) -> Option<PipelineBoxFn> {
    Some(match op {
        CompOp::SrcOver => a8_box_src_over,
        CompOp::SrcCopy => a8_box_src_copy,
        CompOp::Clear => a8_box_clear,
        CompOp::Plus => a8_box_plus,
        _ => return None,
    })
}
