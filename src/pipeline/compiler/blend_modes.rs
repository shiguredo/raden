use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::immediates::Ieee32;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{Endianness, InstBuilder, MemFlags, Value};
use cranelift_frontend::FunctionBuilder;

// =============================================================================
// ブレンドモード用ヘルパー関数
// =============================================================================

/// SrcOver アルファ: out_a = src_a + (dst_a * (256 - src_a)) >> 8
/// I32 / I32X4 両方で動作する (Cranelift の型多相性)。
pub(super) fn emit_srcover_alpha(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    dst_a: Value,
    c256: Value,
) -> Value {
    let inv_sa = bcx.ins().isub(c256, src_a);
    let t = bcx.ins().imul(dst_a, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    bcx.ins().iadd(src_a, t)
}

/// blend_c に端項を加算: out_c = blend_c + (src_c * inv_da) >> 8 + (dst_c * inv_sa) >> 8
pub(super) fn emit_blend_with_edges(
    bcx: &mut FunctionBuilder,
    blend_c: Value,
    src_c: Value,
    dst_c: Value,
    inv_da: Value,
    inv_sa: Value,
) -> Value {
    let edge_s = bcx.ins().imul(src_c, inv_da);
    let edge_s = bcx.ins().ushr_imm(edge_s, 8);
    let edge_d = bcx.ins().imul(dst_c, inv_sa);
    let edge_d = bcx.ins().ushr_imm(edge_d, 8);
    let t = bcx.ins().iadd(blend_c, edge_s);
    bcx.ins().iadd(t, edge_d)
}

// =============================================================================
// ブレンドモード compose 関数
// =============================================================================

// -- Minus (13) ---------------------------------------------------------------

pub(super) fn compose_minus_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let zero_s = bcx.ins().iconst(types::I32, 0);
    let zero_v = bcx.ins().splat(types::I32X4, zero_s);
    let or = bcx.ins().isub(dst_r, src_r);
    let or = bcx.ins().smax(or, zero_v);
    let og = bcx.ins().isub(dst_g, src_g);
    let og = bcx.ins().smax(og, zero_v);
    let ob = bcx.ins().isub(dst_b, src_b);
    let ob = bcx.ins().smax(ob, zero_v);
    (oa, or, og, ob)
}

pub(super) fn compose_minus_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let zero = bcx.ins().iconst(types::I32, 0);
    let or = bcx.ins().isub(dst_r, src_r);
    let or = bcx.ins().smax(or, zero);
    let og = bcx.ins().isub(dst_g, src_g);
    let og = bcx.ins().smax(og, zero);
    let ob = bcx.ins().isub(dst_b, src_b);
    let ob = bcx.ins().smax(ob, zero);
    (oa, or, og, ob)
}

// -- Modulate (14) ------------------------------------------------------------

pub(super) fn compose_modulate_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    _c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    let fa = bcx.ins().iadd(dst_a, one_v);
    let oa = bcx.ins().imul(src_a, fa);
    let oa = bcx.ins().ushr_imm(oa, 8);
    let fr = bcx.ins().iadd(dst_r, one_v);
    let or = bcx.ins().imul(src_r, fr);
    let or = bcx.ins().ushr_imm(or, 8);
    let fg = bcx.ins().iadd(dst_g, one_v);
    let og = bcx.ins().imul(src_g, fg);
    let og = bcx.ins().ushr_imm(og, 8);
    let fb = bcx.ins().iadd(dst_b, one_v);
    let ob = bcx.ins().imul(src_b, fb);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_modulate_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    _c256: Value,
) -> (Value, Value, Value, Value) {
    let one = bcx.ins().iconst(types::I32, 1);
    let fa = bcx.ins().iadd(dst_a, one);
    let oa = bcx.ins().imul(src_a, fa);
    let oa = bcx.ins().ushr_imm(oa, 8);
    let fr = bcx.ins().iadd(dst_r, one);
    let or = bcx.ins().imul(src_r, fr);
    let or = bcx.ins().ushr_imm(or, 8);
    let fg = bcx.ins().iadd(dst_g, one);
    let og = bcx.ins().imul(src_g, fg);
    let og = bcx.ins().ushr_imm(og, 8);
    let fb = bcx.ins().iadd(dst_b, one);
    let ob = bcx.ins().imul(src_b, fb);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

// -- Screen (16) --------------------------------------------------------------

pub(super) fn compose_screen_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    _c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    // out_c = src_c + dst_c - (src_c * (dst_c + 1)) >> 8
    let fa = bcx.ins().iadd(dst_a, one_v);
    let t = bcx.ins().imul(src_a, fa);
    let t = bcx.ins().ushr_imm(t, 8);
    let oa = bcx.ins().iadd(src_a, dst_a);
    let oa = bcx.ins().isub(oa, t);
    let fr = bcx.ins().iadd(dst_r, one_v);
    let t = bcx.ins().imul(src_r, fr);
    let t = bcx.ins().ushr_imm(t, 8);
    let or = bcx.ins().iadd(src_r, dst_r);
    let or = bcx.ins().isub(or, t);
    let fg = bcx.ins().iadd(dst_g, one_v);
    let t = bcx.ins().imul(src_g, fg);
    let t = bcx.ins().ushr_imm(t, 8);
    let og = bcx.ins().iadd(src_g, dst_g);
    let og = bcx.ins().isub(og, t);
    let fb = bcx.ins().iadd(dst_b, one_v);
    let t = bcx.ins().imul(src_b, fb);
    let t = bcx.ins().ushr_imm(t, 8);
    let ob = bcx.ins().iadd(src_b, dst_b);
    let ob = bcx.ins().isub(ob, t);
    (oa, or, og, ob)
}

pub(super) fn compose_screen_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    _c256: Value,
) -> (Value, Value, Value, Value) {
    let one = bcx.ins().iconst(types::I32, 1);
    let fa = bcx.ins().iadd(dst_a, one);
    let t = bcx.ins().imul(src_a, fa);
    let t = bcx.ins().ushr_imm(t, 8);
    let oa = bcx.ins().iadd(src_a, dst_a);
    let oa = bcx.ins().isub(oa, t);
    let fr = bcx.ins().iadd(dst_r, one);
    let t = bcx.ins().imul(src_r, fr);
    let t = bcx.ins().ushr_imm(t, 8);
    let or = bcx.ins().iadd(src_r, dst_r);
    let or = bcx.ins().isub(or, t);
    let fg = bcx.ins().iadd(dst_g, one);
    let t = bcx.ins().imul(src_g, fg);
    let t = bcx.ins().ushr_imm(t, 8);
    let og = bcx.ins().iadd(src_g, dst_g);
    let og = bcx.ins().isub(og, t);
    let fb = bcx.ins().iadd(dst_b, one);
    let t = bcx.ins().imul(src_b, fb);
    let t = bcx.ins().ushr_imm(t, 8);
    let ob = bcx.ins().iadd(src_b, dst_b);
    let ob = bcx.ins().isub(ob, t);
    (oa, or, og, ob)
}

// -- Exclusion (28) -----------------------------------------------------------

pub(super) fn compose_exclusion_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    // out_c = src_c + dst_c - (2 * src_c * (dst_c + 1)) >> 8
    let fr = bcx.ins().iadd(dst_r, one_v);
    let t = bcx.ins().imul(src_r, fr);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let or = bcx.ins().iadd(src_r, dst_r);
    let or = bcx.ins().isub(or, t);
    let fg = bcx.ins().iadd(dst_g, one_v);
    let t = bcx.ins().imul(src_g, fg);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let og = bcx.ins().iadd(src_g, dst_g);
    let og = bcx.ins().isub(og, t);
    let fb = bcx.ins().iadd(dst_b, one_v);
    let t = bcx.ins().imul(src_b, fb);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let ob = bcx.ins().iadd(src_b, dst_b);
    let ob = bcx.ins().isub(ob, t);
    (oa, or, og, ob)
}

pub(super) fn compose_exclusion_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let one = bcx.ins().iconst(types::I32, 1);
    let fr = bcx.ins().iadd(dst_r, one);
    let t = bcx.ins().imul(src_r, fr);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let or = bcx.ins().iadd(src_r, dst_r);
    let or = bcx.ins().isub(or, t);
    let fg = bcx.ins().iadd(dst_g, one);
    let t = bcx.ins().imul(src_g, fg);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let og = bcx.ins().iadd(src_g, dst_g);
    let og = bcx.ins().isub(og, t);
    let fb = bcx.ins().iadd(dst_b, one);
    let t = bcx.ins().imul(src_b, fb);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let ob = bcx.ins().iadd(src_b, dst_b);
    let ob = bcx.ins().isub(ob, t);
    (oa, or, og, ob)
}

// -- Darken (18) --------------------------------------------------------------

pub(super) fn compose_darken_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_p1 = bcx.ins().iadd(src_a, one_v);
    // blend_c = min(src_c * (dst_a+1) >> 8, dst_c * (src_a+1) >> 8)
    let a = bcx.ins().imul(src_r, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_r, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_r = bcx.ins().umin(a, b);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let a = bcx.ins().imul(src_g, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_g, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_g = bcx.ins().umin(a, b);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let a = bcx.ins().imul(src_b, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_b, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_b = bcx.ins().umin(a, b);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_darken_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_p1 = bcx.ins().iadd(src_a, one);
    let a = bcx.ins().imul(src_r, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_r, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_r = bcx.ins().umin(a, b);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let a = bcx.ins().imul(src_g, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_g, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_g = bcx.ins().umin(a, b);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let a = bcx.ins().imul(src_b, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_b, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_b = bcx.ins().umin(a, b);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- Lighten (19) -------------------------------------------------------------

pub(super) fn compose_lighten_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_p1 = bcx.ins().iadd(src_a, one_v);
    let a = bcx.ins().imul(src_r, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_r, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_r = bcx.ins().umax(a, b);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let a = bcx.ins().imul(src_g, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_g, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_g = bcx.ins().umax(a, b);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let a = bcx.ins().imul(src_b, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_b, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_b = bcx.ins().umax(a, b);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_lighten_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_p1 = bcx.ins().iadd(src_a, one);
    let a = bcx.ins().imul(src_r, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_r, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_r = bcx.ins().umax(a, b);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let a = bcx.ins().imul(src_g, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_g, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_g = bcx.ins().umax(a, b);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let a = bcx.ins().imul(src_b, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_b, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let blend_b = bcx.ins().umax(a, b);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- Difference (27) ----------------------------------------------------------

pub(super) fn compose_difference_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_p1 = bcx.ins().iadd(src_a, one_v);
    // blend_c = abs((src_c*(dst_a+1))>>8 - (dst_c*(src_a+1))>>8)
    let a = bcx.ins().imul(src_r, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_r, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let diff = bcx.ins().isub(a, b);
    let blend_r = bcx.ins().iabs(diff);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let a = bcx.ins().imul(src_g, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_g, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let diff = bcx.ins().isub(a, b);
    let blend_g = bcx.ins().iabs(diff);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let a = bcx.ins().imul(src_b, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_b, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let diff = bcx.ins().isub(a, b);
    let blend_b = bcx.ins().iabs(diff);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_difference_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_p1 = bcx.ins().iadd(src_a, one);
    let a = bcx.ins().imul(src_r, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_r, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let diff = bcx.ins().isub(a, b);
    let blend_r = bcx.ins().iabs(diff);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let a = bcx.ins().imul(src_g, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_g, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let diff = bcx.ins().isub(a, b);
    let blend_g = bcx.ins().iabs(diff);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let a = bcx.ins().imul(src_b, da_p1);
    let a = bcx.ins().ushr_imm(a, 8);
    let b = bcx.ins().imul(dst_b, sa_p1);
    let b = bcx.ins().ushr_imm(b, 8);
    let diff = bcx.ins().isub(a, b);
    let blend_b = bcx.ins().iabs(diff);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- Multiply (15) ------------------------------------------------------------

pub(super) fn compose_multiply_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    // blend_c = (src_c * (dst_c + 1)) >> 8
    let f = bcx.ins().iadd(dst_r, one_v);
    let blend_r = bcx.ins().imul(src_r, f);
    let blend_r = bcx.ins().ushr_imm(blend_r, 8);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let f = bcx.ins().iadd(dst_g, one_v);
    let blend_g = bcx.ins().imul(src_g, f);
    let blend_g = bcx.ins().ushr_imm(blend_g, 8);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let f = bcx.ins().iadd(dst_b, one_v);
    let blend_b = bcx.ins().imul(src_b, f);
    let blend_b = bcx.ins().ushr_imm(blend_b, 8);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_multiply_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let f = bcx.ins().iadd(dst_r, one);
    let blend_r = bcx.ins().imul(src_r, f);
    let blend_r = bcx.ins().ushr_imm(blend_r, 8);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let f = bcx.ins().iadd(dst_g, one);
    let blend_g = bcx.ins().imul(src_g, f);
    let blend_g = bcx.ins().ushr_imm(blend_g, 8);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let f = bcx.ins().iadd(dst_b, one);
    let blend_b = bcx.ins().imul(src_b, f);
    let blend_b = bcx.ins().ushr_imm(blend_b, 8);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- LinearBurn (22) ----------------------------------------------------------

pub(super) fn compose_linear_burn_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let zero_s = bcx.ins().iconst(types::I32, 0);
    let zero_v = bcx.ins().splat(types::I32X4, zero_s);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    // sa_da = (src_a * (dst_a + 1)) >> 8
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    // out_c = max(src_c + dst_c - sa_da, 0)
    let t = bcx.ins().iadd(src_r, dst_r);
    let t = bcx.ins().isub(t, sa_da);
    let or = bcx.ins().smax(t, zero_v);
    let t = bcx.ins().iadd(src_g, dst_g);
    let t = bcx.ins().isub(t, sa_da);
    let og = bcx.ins().smax(t, zero_v);
    let t = bcx.ins().iadd(src_b, dst_b);
    let t = bcx.ins().isub(t, sa_da);
    let ob = bcx.ins().smax(t, zero_v);
    (oa, or, og, ob)
}

pub(super) fn compose_linear_burn_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let zero = bcx.ins().iconst(types::I32, 0);
    let one = bcx.ins().iconst(types::I32, 1);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    let t = bcx.ins().iadd(src_r, dst_r);
    let t = bcx.ins().isub(t, sa_da);
    let or = bcx.ins().smax(t, zero);
    let t = bcx.ins().iadd(src_g, dst_g);
    let t = bcx.ins().isub(t, sa_da);
    let og = bcx.ins().smax(t, zero);
    let t = bcx.ins().iadd(src_b, dst_b);
    let t = bcx.ins().isub(t, sa_da);
    let ob = bcx.ins().smax(t, zero);
    (oa, or, og, ob)
}

// -- Overlay (17) -------------------------------------------------------------

/// Overlay の 1 チャネル分の blend_c を計算する。
/// SIMD: if 2*dst_c < dst_a then 2*(src_c*(dst_c+1))>>8
///       else (sa*(da+1))>>8 - 2*((sa-sc)*((da-dc)+1))>>8
pub(super) fn emit_overlay_blend_simd(
    bcx: &mut FunctionBuilder,
    src_c: Value,
    dst_c: Value,
    src_a: Value,
    dst_a: Value,
    one_v: Value,
    sa_da: Value,
) -> Value {
    let two_dc = bcx.ins().ishl_imm(dst_c, 1);
    let cond = bcx.ins().icmp(IntCC::UnsignedLessThan, two_dc, dst_a);
    // true branch
    let dc_p1 = bcx.ins().iadd(dst_c, one_v);
    let t = bcx.ins().imul(src_c, dc_p1);
    let t = bcx.ins().ishl_imm(t, 1);
    let blend_true = bcx.ins().ushr_imm(t, 8);
    // false branch
    let diff_s = bcx.ins().isub(src_a, src_c);
    let diff_d = bcx.ins().isub(dst_a, dst_c);
    let diff_d_p1 = bcx.ins().iadd(diff_d, one_v);
    let t = bcx.ins().imul(diff_s, diff_d_p1);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let blend_false = bcx.ins().isub(sa_da, t);
    bcx.ins().bitselect(cond, blend_true, blend_false)
}

pub(super) fn emit_overlay_blend_scalar(
    bcx: &mut FunctionBuilder,
    src_c: Value,
    dst_c: Value,
    src_a: Value,
    dst_a: Value,
    one: Value,
    sa_da: Value,
) -> Value {
    let two_dc = bcx.ins().ishl_imm(dst_c, 1);
    let cond = bcx.ins().icmp(IntCC::UnsignedLessThan, two_dc, dst_a);
    let dc_p1 = bcx.ins().iadd(dst_c, one);
    let t = bcx.ins().imul(src_c, dc_p1);
    let t = bcx.ins().ishl_imm(t, 1);
    let blend_true = bcx.ins().ushr_imm(t, 8);
    let diff_s = bcx.ins().isub(src_a, src_c);
    let diff_d = bcx.ins().isub(dst_a, dst_c);
    let diff_d_p1 = bcx.ins().iadd(diff_d, one);
    let t = bcx.ins().imul(diff_s, diff_d_p1);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let blend_false = bcx.ins().isub(sa_da, t);
    bcx.ins().select(cond, blend_true, blend_false)
}

pub(super) fn compose_overlay_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    let blend_r = emit_overlay_blend_simd(bcx, src_r, dst_r, src_a, dst_a, one_v, sa_da);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let blend_g = emit_overlay_blend_simd(bcx, src_g, dst_g, src_a, dst_a, one_v, sa_da);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let blend_b = emit_overlay_blend_simd(bcx, src_b, dst_b, src_a, dst_a, one_v, sa_da);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_overlay_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    let blend_r = emit_overlay_blend_scalar(bcx, src_r, dst_r, src_a, dst_a, one, sa_da);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let blend_g = emit_overlay_blend_scalar(bcx, src_g, dst_g, src_a, dst_a, one, sa_da);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let blend_b = emit_overlay_blend_scalar(bcx, src_b, dst_b, src_a, dst_a, one, sa_da);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- HardLight (25) -----------------------------------------------------------
// Overlay と同一だが条件が 2*src_c < src_a (src と dst を入れ替え)

pub(super) fn emit_hard_light_blend_simd(
    bcx: &mut FunctionBuilder,
    src_c: Value,
    dst_c: Value,
    src_a: Value,
    dst_a: Value,
    one_v: Value,
    sa_da: Value,
) -> Value {
    let two_sc = bcx.ins().ishl_imm(src_c, 1);
    let cond = bcx.ins().icmp(IntCC::UnsignedLessThan, two_sc, src_a);
    let dc_p1 = bcx.ins().iadd(dst_c, one_v);
    let t = bcx.ins().imul(src_c, dc_p1);
    let t = bcx.ins().ishl_imm(t, 1);
    let blend_true = bcx.ins().ushr_imm(t, 8);
    let diff_s = bcx.ins().isub(src_a, src_c);
    let diff_d = bcx.ins().isub(dst_a, dst_c);
    let diff_d_p1 = bcx.ins().iadd(diff_d, one_v);
    let t = bcx.ins().imul(diff_s, diff_d_p1);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let blend_false = bcx.ins().isub(sa_da, t);
    bcx.ins().bitselect(cond, blend_true, blend_false)
}

pub(super) fn emit_hard_light_blend_scalar(
    bcx: &mut FunctionBuilder,
    src_c: Value,
    dst_c: Value,
    src_a: Value,
    dst_a: Value,
    one: Value,
    sa_da: Value,
) -> Value {
    let two_sc = bcx.ins().ishl_imm(src_c, 1);
    let cond = bcx.ins().icmp(IntCC::UnsignedLessThan, two_sc, src_a);
    let dc_p1 = bcx.ins().iadd(dst_c, one);
    let t = bcx.ins().imul(src_c, dc_p1);
    let t = bcx.ins().ishl_imm(t, 1);
    let blend_true = bcx.ins().ushr_imm(t, 8);
    let diff_s = bcx.ins().isub(src_a, src_c);
    let diff_d = bcx.ins().isub(dst_a, dst_c);
    let diff_d_p1 = bcx.ins().iadd(diff_d, one);
    let t = bcx.ins().imul(diff_s, diff_d_p1);
    let t = bcx.ins().ishl_imm(t, 1);
    let t = bcx.ins().ushr_imm(t, 8);
    let blend_false = bcx.ins().isub(sa_da, t);
    bcx.ins().select(cond, blend_true, blend_false)
}

pub(super) fn compose_hard_light_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    let blend_r = emit_hard_light_blend_simd(bcx, src_r, dst_r, src_a, dst_a, one_v, sa_da);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let blend_g = emit_hard_light_blend_simd(bcx, src_g, dst_g, src_a, dst_a, one_v, sa_da);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let blend_b = emit_hard_light_blend_simd(bcx, src_b, dst_b, src_a, dst_a, one_v, sa_da);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_hard_light_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    let blend_r = emit_hard_light_blend_scalar(bcx, src_r, dst_r, src_a, dst_a, one, sa_da);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let blend_g = emit_hard_light_blend_scalar(bcx, src_g, dst_g, src_a, dst_a, one, sa_da);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let blend_b = emit_hard_light_blend_scalar(bcx, src_b, dst_b, src_a, dst_a, one, sa_da);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- PinLight (24) ------------------------------------------------------------

pub(super) fn emit_pin_light_blend_simd(
    bcx: &mut FunctionBuilder,
    src_c: Value,
    dst_c: Value,
    src_a: Value,
    dst_a: Value,
    one_v: Value,
    sa_da: Value,
) -> Value {
    let two_sc = bcx.ins().ishl_imm(src_c, 1);
    let cond = bcx.ins().icmp(IntCC::UnsignedLessThan, two_sc, src_a);
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_p1 = bcx.ins().iadd(src_a, one_v);
    let dc_sa = bcx.ins().imul(dst_c, sa_p1);
    let dc_sa = bcx.ins().ushr_imm(dc_sa, 8);
    let sc2_da = bcx.ins().imul(two_sc, da_p1);
    let sc2_da = bcx.ins().ushr_imm(sc2_da, 8);
    // true: min(dc_sa, sc2_da)
    let blend_true = bcx.ins().umin(dc_sa, sc2_da);
    // false: max(dc_sa, sc2_da - sa_da) ; (2*Sc-Sa)*Da = sc2_da - sa_da
    let sc2_minus_sa_da = bcx.ins().isub(sc2_da, sa_da);
    let blend_false = bcx.ins().smax(dc_sa, sc2_minus_sa_da);
    bcx.ins().bitselect(cond, blend_true, blend_false)
}

pub(super) fn emit_pin_light_blend_scalar(
    bcx: &mut FunctionBuilder,
    src_c: Value,
    dst_c: Value,
    src_a: Value,
    dst_a: Value,
    one: Value,
    sa_da: Value,
) -> Value {
    let two_sc = bcx.ins().ishl_imm(src_c, 1);
    let cond = bcx.ins().icmp(IntCC::UnsignedLessThan, two_sc, src_a);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_p1 = bcx.ins().iadd(src_a, one);
    let dc_sa = bcx.ins().imul(dst_c, sa_p1);
    let dc_sa = bcx.ins().ushr_imm(dc_sa, 8);
    let sc2_da = bcx.ins().imul(two_sc, da_p1);
    let sc2_da = bcx.ins().ushr_imm(sc2_da, 8);
    let blend_true = bcx.ins().umin(dc_sa, sc2_da);
    let sc2_minus_sa_da = bcx.ins().isub(sc2_da, sa_da);
    let blend_false = bcx.ins().smax(dc_sa, sc2_minus_sa_da);
    bcx.ins().select(cond, blend_true, blend_false)
}

pub(super) fn compose_pin_light_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    let blend_r = emit_pin_light_blend_simd(bcx, src_r, dst_r, src_a, dst_a, one_v, sa_da);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let blend_g = emit_pin_light_blend_simd(bcx, src_g, dst_g, src_a, dst_a, one_v, sa_da);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let blend_b = emit_pin_light_blend_simd(bcx, src_b, dst_b, src_a, dst_a, one_v, sa_da);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_pin_light_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    let blend_r = emit_pin_light_blend_scalar(bcx, src_r, dst_r, src_a, dst_a, one, sa_da);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let blend_g = emit_pin_light_blend_scalar(bcx, src_g, dst_g, src_a, dst_a, one, sa_da);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let blend_b = emit_pin_light_blend_scalar(bcx, src_b, dst_b, src_a, dst_a, one, sa_da);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- LinearLight (23) ---------------------------------------------------------

pub(super) fn compose_linear_light_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one_s = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one_s);
    let zero_s = bcx.ins().iconst(types::I32, 0);
    let zero_v = bcx.ins().splat(types::I32X4, zero_s);
    let da_p1 = bcx.ins().iadd(dst_a, one_v);
    let sa_p1 = bcx.ins().iadd(src_a, one_v);
    // sa_da = (src_a * (dst_a+1)) >> 8
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    // blend_c = clamp(dc_sa + 2*sc_da - sa_da, 0, sa_da)
    let dc_sa = bcx.ins().imul(dst_r, sa_p1);
    let dc_sa = bcx.ins().ushr_imm(dc_sa, 8);
    let sc_da = bcx.ins().imul(src_r, da_p1);
    let sc_da = bcx.ins().ushr_imm(sc_da, 8);
    let sc_da_2 = bcx.ins().ishl_imm(sc_da, 1);
    let unclamped = bcx.ins().iadd(dc_sa, sc_da_2);
    let unclamped = bcx.ins().isub(unclamped, sa_da);
    let blend_r = bcx.ins().smax(unclamped, zero_v);
    let blend_r = bcx.ins().umin(blend_r, sa_da);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let dc_sa = bcx.ins().imul(dst_g, sa_p1);
    let dc_sa = bcx.ins().ushr_imm(dc_sa, 8);
    let sc_da = bcx.ins().imul(src_g, da_p1);
    let sc_da = bcx.ins().ushr_imm(sc_da, 8);
    let sc_da_2 = bcx.ins().ishl_imm(sc_da, 1);
    let unclamped = bcx.ins().iadd(dc_sa, sc_da_2);
    let unclamped = bcx.ins().isub(unclamped, sa_da);
    let blend_g = bcx.ins().smax(unclamped, zero_v);
    let blend_g = bcx.ins().umin(blend_g, sa_da);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let dc_sa = bcx.ins().imul(dst_b, sa_p1);
    let dc_sa = bcx.ins().ushr_imm(dc_sa, 8);
    let sc_da = bcx.ins().imul(src_b, da_p1);
    let sc_da = bcx.ins().ushr_imm(sc_da, 8);
    let sc_da_2 = bcx.ins().ishl_imm(sc_da, 1);
    let unclamped = bcx.ins().iadd(dc_sa, sc_da_2);
    let unclamped = bcx.ins().isub(unclamped, sa_da);
    let blend_b = bcx.ins().smax(unclamped, zero_v);
    let blend_b = bcx.ins().umin(blend_b, sa_da);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_linear_light_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let zero = bcx.ins().iconst(types::I32, 0);
    let da_p1 = bcx.ins().iadd(dst_a, one);
    let sa_p1 = bcx.ins().iadd(src_a, one);
    let sa_da = bcx.ins().imul(src_a, da_p1);
    let sa_da = bcx.ins().ushr_imm(sa_da, 8);
    let dc_sa = bcx.ins().imul(dst_r, sa_p1);
    let dc_sa = bcx.ins().ushr_imm(dc_sa, 8);
    let sc_da = bcx.ins().imul(src_r, da_p1);
    let sc_da = bcx.ins().ushr_imm(sc_da, 8);
    let sc_da_2 = bcx.ins().ishl_imm(sc_da, 1);
    let unclamped = bcx.ins().iadd(dc_sa, sc_da_2);
    let unclamped = bcx.ins().isub(unclamped, sa_da);
    let blend_r = bcx.ins().smax(unclamped, zero);
    let blend_r = bcx.ins().umin(blend_r, sa_da);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let dc_sa = bcx.ins().imul(dst_g, sa_p1);
    let dc_sa = bcx.ins().ushr_imm(dc_sa, 8);
    let sc_da = bcx.ins().imul(src_g, da_p1);
    let sc_da = bcx.ins().ushr_imm(sc_da, 8);
    let sc_da_2 = bcx.ins().ishl_imm(sc_da, 1);
    let unclamped = bcx.ins().iadd(dc_sa, sc_da_2);
    let unclamped = bcx.ins().isub(unclamped, sa_da);
    let blend_g = bcx.ins().smax(unclamped, zero);
    let blend_g = bcx.ins().umin(blend_g, sa_da);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let dc_sa = bcx.ins().imul(dst_b, sa_p1);
    let dc_sa = bcx.ins().ushr_imm(dc_sa, 8);
    let sc_da = bcx.ins().imul(src_b, da_p1);
    let sc_da = bcx.ins().ushr_imm(sc_da, 8);
    let sc_da_2 = bcx.ins().ishl_imm(sc_da, 1);
    let unclamped = bcx.ins().iadd(dc_sa, sc_da_2);
    let unclamped = bcx.ins().isub(unclamped, sa_da);
    let blend_b = bcx.ins().smax(unclamped, zero);
    let blend_b = bcx.ins().umin(blend_b, sa_da);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- ColorDodge (20) ----------------------------------------------------------
// F32X4 を使用して除算を実現する

pub(super) fn emit_color_dodge_blend_f32x4(
    bcx: &mut FunctionBuilder,
    src_c_f: Value,
    dst_c_f: Value,
    src_a_f: Value,
    dst_a_f: Value,
    one_f: Value,
    c255_f: Value,
) -> Value {
    // blend_c = min(dst_c*src_a / max(src_a-src_c, 1), dst_a) * src_a / 255
    let denom = bcx.ins().fsub(src_a_f, src_c_f);
    let denom = bcx.ins().fmax(denom, one_f);
    let numer = bcx.ins().fmul(dst_c_f, src_a_f);
    let ratio = bcx.ins().fdiv(numer, denom);
    let capped = bcx.ins().fmin(ratio, dst_a_f);
    let blend = bcx.ins().fmul(capped, src_a_f);
    bcx.ins().fdiv(blend, c255_f)
}

pub(super) fn compose_color_dodge_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let sa_f = bcx.ins().fcvt_from_uint(types::F32X4, src_a);
    let da_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_a);
    let one_fs = bcx.ins().f32const(Ieee32::with_float(1.0));
    let one_f = bcx.ins().splat(types::F32X4, one_fs);
    let c255_fs = bcx.ins().f32const(Ieee32::with_float(255.0));
    let c255_f = bcx.ins().splat(types::F32X4, c255_fs);
    let sr_f = bcx.ins().fcvt_from_uint(types::F32X4, src_r);
    let dr_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_r);
    let blend_r_f = emit_color_dodge_blend_f32x4(bcx, sr_f, dr_f, sa_f, da_f, one_f, c255_f);
    let blend_r = bcx.ins().fcvt_to_uint_sat(types::I32X4, blend_r_f);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let sg_f = bcx.ins().fcvt_from_uint(types::F32X4, src_g);
    let dg_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_g);
    let blend_g_f = emit_color_dodge_blend_f32x4(bcx, sg_f, dg_f, sa_f, da_f, one_f, c255_f);
    let blend_g = bcx.ins().fcvt_to_uint_sat(types::I32X4, blend_g_f);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let sb_f = bcx.ins().fcvt_from_uint(types::F32X4, src_b);
    let db_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_b);
    let blend_b_f = emit_color_dodge_blend_f32x4(bcx, sb_f, db_f, sa_f, da_f, one_f, c255_f);
    let blend_b = bcx.ins().fcvt_to_uint_sat(types::I32X4, blend_b_f);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_color_dodge_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let sa_p1 = bcx.ins().iadd(src_a, one);
    // blend_c = min(dst_c*src_a / max(src_a-src_c, 1), dst_a) * src_a / 255
    // 整数除算で計算
    let denom = bcx.ins().isub(src_a, src_r);
    let denom = bcx.ins().umax(denom, one);
    let numer = bcx.ins().imul(dst_r, src_a);
    let ratio = bcx.ins().udiv(numer, denom);
    let capped = bcx.ins().umin(ratio, dst_a);
    let blend_r = bcx.ins().imul(capped, sa_p1);
    let blend_r = bcx.ins().ushr_imm(blend_r, 8);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let denom = bcx.ins().isub(src_a, src_g);
    let denom = bcx.ins().umax(denom, one);
    let numer = bcx.ins().imul(dst_g, src_a);
    let ratio = bcx.ins().udiv(numer, denom);
    let capped = bcx.ins().umin(ratio, dst_a);
    let blend_g = bcx.ins().imul(capped, sa_p1);
    let blend_g = bcx.ins().ushr_imm(blend_g, 8);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let denom = bcx.ins().isub(src_a, src_b);
    let denom = bcx.ins().umax(denom, one);
    let numer = bcx.ins().imul(dst_b, src_a);
    let ratio = bcx.ins().udiv(numer, denom);
    let capped = bcx.ins().umin(ratio, dst_a);
    let blend_b = bcx.ins().imul(capped, sa_p1);
    let blend_b = bcx.ins().ushr_imm(blend_b, 8);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- ColorBurn (21) -----------------------------------------------------------

pub(super) fn emit_color_burn_blend_f32x4(
    bcx: &mut FunctionBuilder,
    src_c_f: Value,
    dst_c_f: Value,
    src_a_f: Value,
    dst_a_f: Value,
    one_f: Value,
    zero_f: Value,
    c255_f: Value,
) -> Value {
    // blend_c = max(dst_a - src_a*(dst_a-dst_c)/max(src_c, 1), 0) * src_a / 255
    let denom = bcx.ins().fmax(src_c_f, one_f);
    let da_minus_dc = bcx.ins().fsub(dst_a_f, dst_c_f);
    let numer = bcx.ins().fmul(src_a_f, da_minus_dc);
    let ratio = bcx.ins().fdiv(numer, denom);
    let inner = bcx.ins().fsub(dst_a_f, ratio);
    let inner = bcx.ins().fmax(inner, zero_f);
    let blend = bcx.ins().fmul(inner, src_a_f);
    bcx.ins().fdiv(blend, c255_f)
}

pub(super) fn compose_color_burn_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let sa_f = bcx.ins().fcvt_from_uint(types::F32X4, src_a);
    let da_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_a);
    let one_fs = bcx.ins().f32const(Ieee32::with_float(1.0));
    let one_f = bcx.ins().splat(types::F32X4, one_fs);
    let zero_fs = bcx.ins().f32const(Ieee32::with_float(0.0));
    let zero_f = bcx.ins().splat(types::F32X4, zero_fs);
    let c255_fs = bcx.ins().f32const(Ieee32::with_float(255.0));
    let c255_f = bcx.ins().splat(types::F32X4, c255_fs);
    let sr_f = bcx.ins().fcvt_from_uint(types::F32X4, src_r);
    let dr_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_r);
    let b_f = emit_color_burn_blend_f32x4(bcx, sr_f, dr_f, sa_f, da_f, one_f, zero_f, c255_f);
    let blend_r = bcx.ins().fcvt_to_uint_sat(types::I32X4, b_f);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let sg_f = bcx.ins().fcvt_from_uint(types::F32X4, src_g);
    let dg_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_g);
    let b_f = emit_color_burn_blend_f32x4(bcx, sg_f, dg_f, sa_f, da_f, one_f, zero_f, c255_f);
    let blend_g = bcx.ins().fcvt_to_uint_sat(types::I32X4, b_f);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let sb_f = bcx.ins().fcvt_from_uint(types::F32X4, src_b);
    let db_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_b);
    let b_f = emit_color_burn_blend_f32x4(bcx, sb_f, db_f, sa_f, da_f, one_f, zero_f, c255_f);
    let blend_b = bcx.ins().fcvt_to_uint_sat(types::I32X4, b_f);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_color_burn_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let one = bcx.ins().iconst(types::I32, 1);
    let zero = bcx.ins().iconst(types::I32, 0);
    let sa_p1 = bcx.ins().iadd(src_a, one);
    // blend_c = max(dst_a - src_a*(dst_a-dst_c)/max(src_c,1), 0) * src_a / 255
    let denom = bcx.ins().umax(src_r, one);
    let da_minus_dc = bcx.ins().isub(dst_a, dst_r);
    let numer = bcx.ins().imul(src_a, da_minus_dc);
    let ratio = bcx.ins().udiv(numer, denom);
    let inner = bcx.ins().isub(dst_a, ratio);
    let inner = bcx.ins().smax(inner, zero);
    let blend_r = bcx.ins().imul(inner, sa_p1);
    let blend_r = bcx.ins().ushr_imm(blend_r, 8);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let denom = bcx.ins().umax(src_g, one);
    let da_minus_dc = bcx.ins().isub(dst_a, dst_g);
    let numer = bcx.ins().imul(src_a, da_minus_dc);
    let ratio = bcx.ins().udiv(numer, denom);
    let inner = bcx.ins().isub(dst_a, ratio);
    let inner = bcx.ins().smax(inner, zero);
    let blend_g = bcx.ins().imul(inner, sa_p1);
    let blend_g = bcx.ins().ushr_imm(blend_g, 8);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let denom = bcx.ins().umax(src_b, one);
    let da_minus_dc = bcx.ins().isub(dst_a, dst_b);
    let numer = bcx.ins().imul(src_a, da_minus_dc);
    let ratio = bcx.ins().udiv(numer, denom);
    let inner = bcx.ins().isub(dst_a, ratio);
    let inner = bcx.ins().smax(inner, zero);
    let blend_b = bcx.ins().imul(inner, sa_p1);
    let blend_b = bcx.ins().ushr_imm(blend_b, 8);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

// -- SoftLight (26) -----------------------------------------------------------
// F32X4 で全計算を行う。条件選択は bitcast + icmp + bitselect で実現。
//
// Case 1 (2*Sc <= Sa): blend = Dc*Sa - (Sa-2*Sc)*Dc*(Da-Dc)/Da
// Case 2 (2*Sc > Sa, 4*Dc <= Da): D*Da = ((16*Dc/Da-12)*Dc/Da+4)*Dc
//                                  blend = Dc*Sa + (2*Sc-Sa)*(D*Da-Dc)
// Case 3 (2*Sc > Sa, 4*Dc > Da): D*Da = sqrt(Dc*Da)
//                                  blend = Dc*Sa + (2*Sc-Sa)*(D*Da-Dc)

pub(super) fn emit_soft_light_blend_f32x4(
    bcx: &mut FunctionBuilder,
    src_c_f: Value,
    dst_c_f: Value,
    src_a_f: Value,
    dst_a_f: Value,
    one_f: Value,
    zero_f: Value,
    c255_f: Value,
) -> Value {
    let two_fs = bcx.ins().f32const(Ieee32::with_float(2.0));
    let two_f = bcx.ins().splat(types::F32X4, two_fs);
    let four_fs = bcx.ins().f32const(Ieee32::with_float(4.0));
    let four_f = bcx.ins().splat(types::F32X4, four_fs);
    let twelve_fs = bcx.ins().f32const(Ieee32::with_float(12.0));
    let twelve_f = bcx.ins().splat(types::F32X4, twelve_fs);
    let sixteen_fs = bcx.ins().f32const(Ieee32::with_float(16.0));
    let sixteen_f = bcx.ins().splat(types::F32X4, sixteen_fs);

    let two_sc = bcx.ins().fmul(two_f, src_c_f);
    let da_safe = bcx.ins().fmax(dst_a_f, one_f);
    let dc_over_da = bcx.ins().fdiv(dst_c_f, da_safe);
    let dc_sa = bcx.ins().fmul(dst_c_f, src_a_f);
    let da_minus_dc = bcx.ins().fsub(dst_a_f, dst_c_f);

    // Case 1: Dc*Sa - (Sa-2*Sc)*Dc*(Da-Dc)/Da
    let factor1 = bcx.ins().fsub(src_a_f, two_sc);
    let t = bcx.ins().fmul(factor1, dst_c_f);
    let t = bcx.ins().fmul(t, da_minus_dc);
    let t = bcx.ins().fdiv(t, da_safe);
    let blend1 = bcx.ins().fsub(dc_sa, t);

    // Case 2: D*Da = ((16*Dc/Da-12)*Dc/Da+4)*Dc
    let d2 = bcx.ins().fmul(sixteen_f, dc_over_da);
    let d2 = bcx.ins().fsub(d2, twelve_f);
    let d2 = bcx.ins().fmul(d2, dc_over_da);
    let d2 = bcx.ins().fadd(d2, four_f);
    let d2_da = bcx.ins().fmul(d2, dst_c_f);

    // Case 3: D*Da = sqrt(Dc*Da)
    let dc_times_da = bcx.ins().fmul(dst_c_f, dst_a_f);
    let d3_da = bcx.ins().sqrt(dc_times_da);

    // 内側条件選択 (4*Dc <= Da)
    let four_dc = bcx.ins().fmul(four_f, dst_c_f);
    let diff_inner = bcx.ins().fsub(dst_a_f, four_dc);
    let diff_inner_i = bcx.ins().bitcast(
        types::I32X4,
        MemFlags::new().with_endianness(Endianness::Little),
        diff_inner,
    );
    let zero_i = bcx.ins().iconst(types::I32, 0);
    let zero_iv = bcx.ins().splat(types::I32X4, zero_i);
    let cond_inner = bcx
        .ins()
        .icmp(IntCC::SignedGreaterThanOrEqual, diff_inner_i, zero_iv);
    let d2_da_i = bcx.ins().bitcast(
        types::I32X4,
        MemFlags::new().with_endianness(Endianness::Little),
        d2_da,
    );
    let d3_da_i = bcx.ins().bitcast(
        types::I32X4,
        MemFlags::new().with_endianness(Endianness::Little),
        d3_da,
    );
    let d_da_i = bcx.ins().bitselect(cond_inner, d2_da_i, d3_da_i);
    let d_da = bcx.ins().bitcast(
        types::F32X4,
        MemFlags::new().with_endianness(Endianness::Little),
        d_da_i,
    );

    // Cases 2&3 共通: Dc*Sa + (2*Sc-Sa)*(D*Da-Dc)
    let factor23 = bcx.ins().fsub(two_sc, src_a_f);
    let d_da_minus_dc = bcx.ins().fsub(d_da, dst_c_f);
    let blend23 = bcx.ins().fmul(factor23, d_da_minus_dc);
    let blend23 = bcx.ins().fadd(dc_sa, blend23);

    // 外側条件選択 (2*Sc <= Sa)
    let diff_outer = bcx.ins().fsub(src_a_f, two_sc);
    let diff_outer_i = bcx.ins().bitcast(
        types::I32X4,
        MemFlags::new().with_endianness(Endianness::Little),
        diff_outer,
    );
    let cond_outer = bcx
        .ins()
        .icmp(IntCC::SignedGreaterThanOrEqual, diff_outer_i, zero_iv);
    let blend1_i = bcx.ins().bitcast(
        types::I32X4,
        MemFlags::new().with_endianness(Endianness::Little),
        blend1,
    );
    let blend23_i = bcx.ins().bitcast(
        types::I32X4,
        MemFlags::new().with_endianness(Endianness::Little),
        blend23,
    );
    let blend_i = bcx.ins().bitselect(cond_outer, blend1_i, blend23_i);
    let blend = bcx.ins().bitcast(
        types::F32X4,
        MemFlags::new().with_endianness(Endianness::Little),
        blend_i,
    );

    // / 255
    let result = bcx.ins().fdiv(blend, c255_f);
    bcx.ins().fmax(result, zero_f)
}

pub(super) fn compose_soft_light_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let sa_f = bcx.ins().fcvt_from_uint(types::F32X4, src_a);
    let da_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_a);
    let one_fs = bcx.ins().f32const(Ieee32::with_float(1.0));
    let one_f = bcx.ins().splat(types::F32X4, one_fs);
    let zero_fs = bcx.ins().f32const(Ieee32::with_float(0.0));
    let zero_f = bcx.ins().splat(types::F32X4, zero_fs);
    let c255_fs = bcx.ins().f32const(Ieee32::with_float(255.0));
    let c255_f = bcx.ins().splat(types::F32X4, c255_fs);
    let sr_f = bcx.ins().fcvt_from_uint(types::F32X4, src_r);
    let dr_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_r);
    let b_f = emit_soft_light_blend_f32x4(bcx, sr_f, dr_f, sa_f, da_f, one_f, zero_f, c255_f);
    let blend_r = bcx.ins().fcvt_to_uint_sat(types::I32X4, b_f);
    let or = emit_blend_with_edges(bcx, blend_r, src_r, dst_r, inv_da, inv_sa);
    let sg_f = bcx.ins().fcvt_from_uint(types::F32X4, src_g);
    let dg_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_g);
    let b_f = emit_soft_light_blend_f32x4(bcx, sg_f, dg_f, sa_f, da_f, one_f, zero_f, c255_f);
    let blend_g = bcx.ins().fcvt_to_uint_sat(types::I32X4, b_f);
    let og = emit_blend_with_edges(bcx, blend_g, src_g, dst_g, inv_da, inv_sa);
    let sb_f = bcx.ins().fcvt_from_uint(types::F32X4, src_b);
    let db_f = bcx.ins().fcvt_from_uint(types::F32X4, dst_b);
    let b_f = emit_soft_light_blend_f32x4(bcx, sb_f, db_f, sa_f, da_f, one_f, zero_f, c255_f);
    let blend_b = bcx.ins().fcvt_to_uint_sat(types::I32X4, b_f);
    let ob = emit_blend_with_edges(bcx, blend_b, src_b, dst_b, inv_da, inv_sa);
    (oa, or, og, ob)
}

pub(super) fn compose_soft_light_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let oa = emit_srcover_alpha(bcx, src_a, dst_a, c256);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let sa_f = bcx.ins().fcvt_from_uint(types::F32, src_a);
    let da_f = bcx.ins().fcvt_from_uint(types::F32, dst_a);
    let one_f = bcx.ins().f32const(Ieee32::with_float(1.0));
    let zero_f = bcx.ins().f32const(Ieee32::with_float(0.0));
    let two_f = bcx.ins().f32const(Ieee32::with_float(2.0));
    let four_f = bcx.ins().f32const(Ieee32::with_float(4.0));
    let twelve_f = bcx.ins().f32const(Ieee32::with_float(12.0));
    let sixteen_f = bcx.ins().f32const(Ieee32::with_float(16.0));
    let c255_f = bcx.ins().f32const(Ieee32::with_float(255.0));
    // 各チャネルを F32 で計算
    let or = emit_soft_light_scalar_channel(
        bcx, src_r, dst_r, sa_f, da_f, one_f, zero_f, two_f, four_f, twelve_f, sixteen_f, c255_f,
        inv_da, inv_sa,
    );
    let og = emit_soft_light_scalar_channel(
        bcx, src_g, dst_g, sa_f, da_f, one_f, zero_f, two_f, four_f, twelve_f, sixteen_f, c255_f,
        inv_da, inv_sa,
    );
    let ob = emit_soft_light_scalar_channel(
        bcx, src_b, dst_b, sa_f, da_f, one_f, zero_f, two_f, four_f, twelve_f, sixteen_f, c255_f,
        inv_da, inv_sa,
    );
    (oa, or, og, ob)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn emit_soft_light_scalar_channel(
    bcx: &mut FunctionBuilder,
    src_c: Value,
    dst_c: Value,
    sa_f: Value,
    da_f: Value,
    one_f: Value,
    zero_f: Value,
    two_f: Value,
    four_f: Value,
    twelve_f: Value,
    sixteen_f: Value,
    c255_f: Value,
    inv_da: Value,
    inv_sa: Value,
) -> Value {
    let sc_f = bcx.ins().fcvt_from_uint(types::F32, src_c);
    let dc_f = bcx.ins().fcvt_from_uint(types::F32, dst_c);
    let two_sc = bcx.ins().fmul(two_f, sc_f);
    let da_safe = bcx.ins().fmax(da_f, one_f);
    let dc_over_da = bcx.ins().fdiv(dc_f, da_safe);
    let dc_sa = bcx.ins().fmul(dc_f, sa_f);
    let da_minus_dc = bcx.ins().fsub(da_f, dc_f);
    // Case 1
    let factor1 = bcx.ins().fsub(sa_f, two_sc);
    let t = bcx.ins().fmul(factor1, dc_f);
    let t = bcx.ins().fmul(t, da_minus_dc);
    let t = bcx.ins().fdiv(t, da_safe);
    let blend1 = bcx.ins().fsub(dc_sa, t);
    // Case 2: D*Da = ((16*Dc/Da-12)*Dc/Da+4)*Dc
    let d2 = bcx.ins().fmul(sixteen_f, dc_over_da);
    let d2 = bcx.ins().fsub(d2, twelve_f);
    let d2 = bcx.ins().fmul(d2, dc_over_da);
    let d2 = bcx.ins().fadd(d2, four_f);
    let d2_da = bcx.ins().fmul(d2, dc_f);
    // Case 3: D*Da = sqrt(Dc*Da)
    let dc_times_da = bcx.ins().fmul(dc_f, da_f);
    let d3_da = bcx.ins().sqrt(dc_times_da);
    // 内側条件 (4*Dc <= Da)
    let four_dc = bcx.ins().fmul(four_f, dc_f);
    let cond_inner = bcx.ins().fcmp(
        cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual,
        four_dc,
        da_f,
    );
    let d_da = bcx.ins().select(cond_inner, d2_da, d3_da);
    // Cases 2&3
    let factor23 = bcx.ins().fsub(two_sc, sa_f);
    let d_da_minus_dc = bcx.ins().fsub(d_da, dc_f);
    let blend23 = bcx.ins().fmul(factor23, d_da_minus_dc);
    let blend23 = bcx.ins().fadd(dc_sa, blend23);
    // 外側条件 (2*Sc <= Sa)
    let cond_outer = bcx.ins().fcmp(
        cranelift_codegen::ir::condcodes::FloatCC::LessThanOrEqual,
        two_sc,
        sa_f,
    );
    let blend = bcx.ins().select(cond_outer, blend1, blend23);
    // / 255
    let blend = bcx.ins().fdiv(blend, c255_f);
    let blend = bcx.ins().fmax(blend, zero_f);
    let blend_i = bcx.ins().fcvt_to_uint_sat(types::I32, blend);
    // 端項
    emit_blend_with_edges(bcx, blend_i, src_c, dst_c, inv_da, inv_sa)
}
