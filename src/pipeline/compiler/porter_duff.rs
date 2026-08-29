use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value};
use cranelift_codegen::isa::TargetFrontendConfig;
use cranelift_frontend::FunctionBuilder;

use super::{
    block_args, emit_expand_packed_coverage_i32x4, emit_extract_channels_simd,
    emit_pack_channels_simd,
};

// =============================================================================
// Porter-Duff 合成パイプライン (カバレッジなし)
// =============================================================================

/// Clear パイプライン (カバレッジなし)。out = 0。
pub(super) fn build_clear(mut bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    let ptr_type = frontend_config.pointer_type();
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let count = bcx.block_params(entry)[2];

    let simd_count = bcx.ins().ushr_imm_u(count, 2);
    let remainder = bcx.ins().band_imm_u(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);
    let zero_i32 = bcx.ins().iconst(types::I32, 0);
    let zero_vec = bcx.ins().splat(types::I32X4, zero_i32);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    bcx.ins().brif(
        has_simd,
        simd_loop,
        &block_args(&[dst, zero]),
        scalar_check,
        &block_args(&[dst]),
    );

    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let cur = bcx.block_params(simd_loop)[0];
    let si = bcx.block_params(simd_loop)[1];
    bcx.ins().store(MemFlagsData::new(), zero_vec, cur, 0);
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next = bcx.ins().iadd(cur, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let nsi = bcx.ins().iadd(si, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, nsi, simd_count);
    bcx.ins().brif(
        cont,
        simd_loop,
        &block_args(&[next, nsi]),
        scalar_check,
        &block_args(&[next]),
    );

    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let cur = bcx.block_params(scalar_check)[0];
    let has_rem = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    bcx.ins()
        .brif(has_rem, scalar_loop, &block_args(&[cur, zero]), exit, &[]);

    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let cur = bcx.block_params(scalar_loop)[0];
    let si = bcx.block_params(scalar_loop)[1];
    bcx.ins().store(MemFlagsData::new(), zero_i32, cur, 0);
    let four = bcx.ins().iconst(ptr_type, 4);
    let next = bcx.ins().iadd(cur, four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let nsi = bcx.ins().iadd(si, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, nsi, remainder);
    bcx.ins()
        .brif(cont, scalar_loop, &block_args(&[next, nsi]), exit, &[]);

    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize(frontend_config);
}

/// DstCopy パイプライン (カバレッジなし)。out = dst (何もしない)。
pub(super) fn build_dst_copy(mut bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    let entry = bcx.create_block();
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize(frontend_config);
}

/// 汎用 Porter-Duff パイプライン (カバレッジなし) を構築する。
pub(super) fn build_generic_compose(
    mut bcx: FunctionBuilder,
    frontend_config: TargetFrontendConfig,
    compose_simd: fn(
        &mut FunctionBuilder,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
    ) -> (Value, Value, Value, Value),
    compose_scalar: fn(
        &mut FunctionBuilder,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
    ) -> (Value, Value, Value, Value),
) {
    let ptr_type = frontend_config.pointer_type();
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let src_solid = bcx.block_params(entry)[1];
    let count = bcx.block_params(entry)[2];

    let src_a = bcx.ins().ushr_imm_u(src_solid, 24);
    let src_a = bcx.ins().band_imm_u(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm_u(src_solid, 16);
    let src_r = bcx.ins().band_imm_u(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm_u(src_solid, 8);
    let src_g = bcx.ins().band_imm_u(src_g, 0xFF);
    let src_b = bcx.ins().band_imm_u(src_solid, 0xFF);

    let src_a_vec = bcx.ins().splat(types::I32X4, src_a);
    let src_r_vec = bcx.ins().splat(types::I32X4, src_r);
    let src_g_vec = bcx.ins().splat(types::I32X4, src_g);
    let src_b_vec = bcx.ins().splat(types::I32X4, src_b);
    let c256_scalar = bcx.ins().iconst(types::I32, 256);
    let c256_vec = bcx.ins().splat(types::I32X4, c256_scalar);
    let mask_0xff = bcx.ins().iconst(types::I32, 0xFF);
    let mask_0xff_vec = bcx.ins().splat(types::I32X4, mask_0xff);

    let simd_count = bcx.ins().ushr_imm_u(count, 2);
    let remainder = bcx.ins().band_imm_u(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    bcx.ins().brif(
        has_simd,
        simd_loop,
        &block_args(&[dst, zero]),
        scalar_check,
        &block_args(&[dst]),
    );

    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let cur_dst = bcx.block_params(simd_loop)[0];
    let si = bcx.block_params(simd_loop)[1];

    let dst_px = bcx
        .ins()
        .load(types::I32X4, MemFlagsData::new(), cur_dst, 0);
    let (da, dr, dg, db) = emit_extract_channels_simd(&mut bcx, dst_px, mask_0xff_vec);
    let (oa, or, og, ob) = compose_simd(
        &mut bcx,
        src_a_vec,
        src_r_vec,
        src_g_vec,
        src_b_vec,
        da,
        dr,
        dg,
        db,
        c256_vec,
        mask_0xff_vec,
    );
    let result = emit_pack_channels_simd(&mut bcx, oa, or, og, ob);
    bcx.ins().store(MemFlagsData::new(), result, cur_dst, 0);

    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next = bcx.ins().iadd(cur_dst, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let nsi = bcx.ins().iadd(si, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, nsi, simd_count);
    bcx.ins().brif(
        cont,
        simd_loop,
        &block_args(&[next, nsi]),
        scalar_check,
        &block_args(&[next]),
    );

    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let cur_dst = bcx.block_params(scalar_check)[0];
    let has_rem = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    bcx.ins().brif(
        has_rem,
        scalar_loop,
        &block_args(&[cur_dst, zero]),
        exit,
        &[],
    );

    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let cur_dst = bcx.block_params(scalar_loop)[0];
    let si = bcx.block_params(scalar_loop)[1];

    let dst_px = bcx.ins().load(types::I32, MemFlagsData::new(), cur_dst, 0);
    let da = bcx.ins().ushr_imm_u(dst_px, 24);
    let da = bcx.ins().band_imm_u(da, 0xFF);
    let dr = bcx.ins().ushr_imm_u(dst_px, 16);
    let dr = bcx.ins().band_imm_u(dr, 0xFF);
    let dg = bcx.ins().ushr_imm_u(dst_px, 8);
    let dg = bcx.ins().band_imm_u(dg, 0xFF);
    let db = bcx.ins().band_imm_u(dst_px, 0xFF);
    let (oa, or, og, ob) = compose_scalar(
        &mut bcx,
        src_a,
        src_r,
        src_g,
        src_b,
        da,
        dr,
        dg,
        db,
        c256_scalar,
    );
    let result = bcx.ins().ishl_imm_u(oa, 24);
    let tmp = bcx.ins().ishl_imm_u(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm_u(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlagsData::new(), result, cur_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next = bcx.ins().iadd(cur_dst, four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let nsi = bcx.ins().iadd(si, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, nsi, remainder);
    bcx.ins()
        .brif(cont, scalar_loop, &block_args(&[next, nsi]), exit, &[]);

    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize(frontend_config);
}

// =============================================================================
// 合成演算関数 (SIMD / スカラ)
// =============================================================================

pub(super) fn compose_src_in_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    _dr: Value,
    _dg: Value,
    _db: Value,
    _c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let one = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one);
    let f = bcx.ins().iadd(dst_a, one_v);
    let oa = bcx.ins().imul(src_a, f);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(src_r, f);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(src_g, f);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(src_b, f);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_src_in_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    _dr: Value,
    _dg: Value,
    _db: Value,
    _c256: Value,
) -> (Value, Value, Value, Value) {
    let one = bcx.ins().iconst(types::I32, 1);
    let f = bcx.ins().iadd(dst_a, one);
    let oa = bcx.ins().imul(src_a, f);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(src_r, f);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(src_g, f);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(src_b, f);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_src_out_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    _dr: Value,
    _dg: Value,
    _db: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let inv = bcx.ins().isub(c256, dst_a);
    let oa = bcx.ins().imul(src_a, inv);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(src_r, inv);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(src_g, inv);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(src_b, inv);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_src_out_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    src_r: Value,
    src_g: Value,
    src_b: Value,
    dst_a: Value,
    _dr: Value,
    _dg: Value,
    _db: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let inv = bcx.ins().isub(c256, dst_a);
    let oa = bcx.ins().imul(src_a, inv);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(src_r, inv);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(src_g, inv);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(src_b, inv);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_src_atop_simd(
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
    let one = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one);
    let da_f = bcx.ins().iadd(dst_a, one_v);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let oa = bcx.ins().imul(src_a, da_f);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let t = bcx.ins().imul(dst_a, inv_sa);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let oa = bcx.ins().iadd(oa, t);
    let or = bcx.ins().imul(src_r, da_f);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let t = bcx.ins().imul(dst_r, inv_sa);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let or = bcx.ins().iadd(or, t);
    let og = bcx.ins().imul(src_g, da_f);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let t = bcx.ins().imul(dst_g, inv_sa);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let og = bcx.ins().iadd(og, t);
    let ob = bcx.ins().imul(src_b, da_f);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    let t = bcx.ins().imul(dst_b, inv_sa);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let ob = bcx.ins().iadd(ob, t);
    (oa, or, og, ob)
}

pub(super) fn compose_src_atop_scalar(
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
    let one = bcx.ins().iconst(types::I32, 1);
    let da_f = bcx.ins().iadd(dst_a, one);
    let inv_sa = bcx.ins().isub(c256, src_a);
    let oa = bcx.ins().imul(src_a, da_f);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let t = bcx.ins().imul(dst_a, inv_sa);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let oa = bcx.ins().iadd(oa, t);
    let or = bcx.ins().imul(src_r, da_f);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let t = bcx.ins().imul(dst_r, inv_sa);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let or = bcx.ins().iadd(or, t);
    let og = bcx.ins().imul(src_g, da_f);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let t = bcx.ins().imul(dst_g, inv_sa);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let og = bcx.ins().iadd(og, t);
    let ob = bcx.ins().imul(src_b, da_f);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    let t = bcx.ins().imul(dst_b, inv_sa);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let ob = bcx.ins().iadd(ob, t);
    (oa, or, og, ob)
}

pub(super) fn compose_dst_over_simd(
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
    let inv = bcx.ins().isub(c256, dst_a);
    let t = bcx.ins().imul(src_a, inv);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let oa = bcx.ins().iadd(dst_a, t);
    let t = bcx.ins().imul(src_r, inv);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let or = bcx.ins().iadd(dst_r, t);
    let t = bcx.ins().imul(src_g, inv);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let og = bcx.ins().iadd(dst_g, t);
    let t = bcx.ins().imul(src_b, inv);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let ob = bcx.ins().iadd(dst_b, t);
    (oa, or, og, ob)
}

pub(super) fn compose_dst_over_scalar(
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
    let inv = bcx.ins().isub(c256, dst_a);
    let t = bcx.ins().imul(src_a, inv);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let oa = bcx.ins().iadd(dst_a, t);
    let t = bcx.ins().imul(src_r, inv);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let or = bcx.ins().iadd(dst_r, t);
    let t = bcx.ins().imul(src_g, inv);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let og = bcx.ins().iadd(dst_g, t);
    let t = bcx.ins().imul(src_b, inv);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let ob = bcx.ins().iadd(dst_b, t);
    (oa, or, og, ob)
}

pub(super) fn compose_dst_in_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    _sr: Value,
    _sg: Value,
    _sb: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    _c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let one = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one);
    let f = bcx.ins().iadd(src_a, one_v);
    let oa = bcx.ins().imul(dst_a, f);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(dst_r, f);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(dst_g, f);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(dst_b, f);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_dst_in_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    _sr: Value,
    _sg: Value,
    _sb: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    _c256: Value,
) -> (Value, Value, Value, Value) {
    let one = bcx.ins().iconst(types::I32, 1);
    let f = bcx.ins().iadd(src_a, one);
    let oa = bcx.ins().imul(dst_a, f);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(dst_r, f);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(dst_g, f);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(dst_b, f);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_dst_out_simd(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    _sr: Value,
    _sg: Value,
    _sb: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
    _mask: Value,
) -> (Value, Value, Value, Value) {
    let inv = bcx.ins().isub(c256, src_a);
    let oa = bcx.ins().imul(dst_a, inv);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(dst_r, inv);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(dst_g, inv);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(dst_b, inv);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_dst_out_scalar(
    bcx: &mut FunctionBuilder,
    src_a: Value,
    _sr: Value,
    _sg: Value,
    _sb: Value,
    dst_a: Value,
    dst_r: Value,
    dst_g: Value,
    dst_b: Value,
    c256: Value,
) -> (Value, Value, Value, Value) {
    let inv = bcx.ins().isub(c256, src_a);
    let oa = bcx.ins().imul(dst_a, inv);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(dst_r, inv);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(dst_g, inv);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(dst_b, inv);
    let ob = bcx.ins().ushr_imm_u(ob, 8);
    (oa, or, og, ob)
}

pub(super) fn compose_dst_atop_simd(
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
    let one = bcx.ins().iconst(types::I32, 1);
    let one_v = bcx.ins().splat(types::I32X4, one);
    let sa_f = bcx.ins().iadd(src_a, one_v);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let t = bcx.ins().imul(dst_a, sa_f);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(src_a, inv_da);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let oa = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_r, sa_f);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(src_r, inv_da);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let or = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_g, sa_f);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(src_g, inv_da);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let og = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_b, sa_f);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(src_b, inv_da);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let ob = bcx.ins().iadd(t, u);
    (oa, or, og, ob)
}

pub(super) fn compose_dst_atop_scalar(
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
    let one = bcx.ins().iconst(types::I32, 1);
    let sa_f = bcx.ins().iadd(src_a, one);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let t = bcx.ins().imul(dst_a, sa_f);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(src_a, inv_da);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let oa = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_r, sa_f);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(src_r, inv_da);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let or = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_g, sa_f);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(src_g, inv_da);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let og = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_b, sa_f);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(src_b, inv_da);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let ob = bcx.ins().iadd(t, u);
    (oa, or, og, ob)
}

pub(super) fn compose_xor_simd(
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
    let inv_sa = bcx.ins().isub(c256, src_a);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let t = bcx.ins().imul(src_a, inv_da);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(dst_a, inv_sa);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let oa = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_r, inv_da);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(dst_r, inv_sa);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let or = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_g, inv_da);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(dst_g, inv_sa);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let og = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_b, inv_da);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(dst_b, inv_sa);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let ob = bcx.ins().iadd(t, u);
    (oa, or, og, ob)
}

pub(super) fn compose_xor_scalar(
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
    let inv_sa = bcx.ins().isub(c256, src_a);
    let inv_da = bcx.ins().isub(c256, dst_a);
    let t = bcx.ins().imul(src_a, inv_da);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(dst_a, inv_sa);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let oa = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_r, inv_da);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(dst_r, inv_sa);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let or = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_g, inv_da);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(dst_g, inv_sa);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let og = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_b, inv_da);
    let t = bcx.ins().ushr_imm_u(t, 8);
    let u = bcx.ins().imul(dst_b, inv_sa);
    let u = bcx.ins().ushr_imm_u(u, 8);
    let ob = bcx.ins().iadd(t, u);
    (oa, or, og, ob)
}

pub(super) fn compose_plus_simd(
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
    mask: Value,
) -> (Value, Value, Value, Value) {
    // mask = splat(0xFF) = c255_vec
    let oa = bcx.ins().iadd(src_a, dst_a);
    let oa = bcx.ins().umin(oa, mask);
    let or = bcx.ins().iadd(src_r, dst_r);
    let or = bcx.ins().umin(or, mask);
    let og = bcx.ins().iadd(src_g, dst_g);
    let og = bcx.ins().umin(og, mask);
    let ob = bcx.ins().iadd(src_b, dst_b);
    let ob = bcx.ins().umin(ob, mask);
    (oa, or, og, ob)
}

pub(super) fn compose_plus_scalar(
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
    let c255 = bcx.ins().iconst(types::I32, 255);
    let oa = bcx.ins().iadd(src_a, dst_a);
    let oa = bcx.ins().umin(oa, c255);
    let or = bcx.ins().iadd(src_r, dst_r);
    let or = bcx.ins().umin(or, c255);
    let og = bcx.ins().iadd(src_g, dst_g);
    let og = bcx.ins().umin(og, c255);
    let ob = bcx.ins().iadd(src_b, dst_b);
    let ob = bcx.ins().umin(ob, c255);
    (oa, or, og, ob)
}

pub(super) fn build_src_in(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_src_in_simd,
        compose_src_in_scalar,
    );
}

pub(super) fn build_src_out(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_src_out_simd,
        compose_src_out_scalar,
    );
}

pub(super) fn build_src_atop(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_src_atop_simd,
        compose_src_atop_scalar,
    );
}

pub(super) fn build_dst_over(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_dst_over_simd,
        compose_dst_over_scalar,
    );
}

pub(super) fn build_dst_in(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_dst_in_simd,
        compose_dst_in_scalar,
    );
}

pub(super) fn build_dst_out(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_dst_out_simd,
        compose_dst_out_scalar,
    );
}

pub(super) fn build_dst_atop(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_dst_atop_simd,
        compose_dst_atop_scalar,
    );
}

pub(super) fn build_xor(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(bcx, frontend_config, compose_xor_simd, compose_xor_scalar);
}

/// Plus パイプライン (カバレッジなし)。out = min(src + dst, 255)。
pub(super) fn build_plus(mut bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    let ptr_type = frontend_config.pointer_type();
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let src_solid = bcx.block_params(entry)[1];
    let count = bcx.block_params(entry)[2];

    let c255 = bcx.ins().iconst(types::I32, 0xFF);
    let c255_vec = bcx.ins().splat(types::I32X4, c255);

    let src_a = bcx.ins().ushr_imm_u(src_solid, 24);
    let src_a = bcx.ins().band_imm_u(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm_u(src_solid, 16);
    let src_r = bcx.ins().band_imm_u(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm_u(src_solid, 8);
    let src_g = bcx.ins().band_imm_u(src_g, 0xFF);
    let src_b = bcx.ins().band_imm_u(src_solid, 0xFF);

    let sa_v = bcx.ins().splat(types::I32X4, src_a);
    let sr_v = bcx.ins().splat(types::I32X4, src_r);
    let sg_v = bcx.ins().splat(types::I32X4, src_g);
    let sb_v = bcx.ins().splat(types::I32X4, src_b);

    let simd_count = bcx.ins().ushr_imm_u(count, 2);
    let remainder = bcx.ins().band_imm_u(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    bcx.ins().brif(
        has_simd,
        simd_loop,
        &block_args(&[dst, zero]),
        scalar_check,
        &block_args(&[dst]),
    );

    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let cur = bcx.block_params(simd_loop)[0];
    let si = bcx.block_params(simd_loop)[1];
    let dp = bcx.ins().load(types::I32X4, MemFlagsData::new(), cur, 0);
    let (da, dr, dg, db) = emit_extract_channels_simd(&mut bcx, dp, c255_vec);
    let oa = bcx.ins().iadd(sa_v, da);
    let oa = bcx.ins().umin(oa, c255_vec);
    let or = bcx.ins().iadd(sr_v, dr);
    let or = bcx.ins().umin(or, c255_vec);
    let og = bcx.ins().iadd(sg_v, dg);
    let og = bcx.ins().umin(og, c255_vec);
    let ob = bcx.ins().iadd(sb_v, db);
    let ob = bcx.ins().umin(ob, c255_vec);
    let result = emit_pack_channels_simd(&mut bcx, oa, or, og, ob);
    bcx.ins().store(MemFlagsData::new(), result, cur, 0);
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next = bcx.ins().iadd(cur, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let nsi = bcx.ins().iadd(si, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, nsi, simd_count);
    bcx.ins().brif(
        cont,
        simd_loop,
        &block_args(&[next, nsi]),
        scalar_check,
        &block_args(&[next]),
    );

    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let cur = bcx.block_params(scalar_check)[0];
    let has_rem = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    bcx.ins()
        .brif(has_rem, scalar_loop, &block_args(&[cur, zero]), exit, &[]);

    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let cur = bcx.block_params(scalar_loop)[0];
    let si = bcx.block_params(scalar_loop)[1];
    let dp = bcx.ins().load(types::I32, MemFlagsData::new(), cur, 0);
    let da = bcx.ins().ushr_imm_u(dp, 24);
    let da = bcx.ins().band_imm_u(da, 0xFF);
    let dr = bcx.ins().ushr_imm_u(dp, 16);
    let dr = bcx.ins().band_imm_u(dr, 0xFF);
    let dg = bcx.ins().ushr_imm_u(dp, 8);
    let dg = bcx.ins().band_imm_u(dg, 0xFF);
    let db = bcx.ins().band_imm_u(dp, 0xFF);
    let oa = bcx.ins().iadd(src_a, da);
    let oa = bcx.ins().umin(oa, c255);
    let or = bcx.ins().iadd(src_r, dr);
    let or = bcx.ins().umin(or, c255);
    let og = bcx.ins().iadd(src_g, dg);
    let og = bcx.ins().umin(og, c255);
    let ob = bcx.ins().iadd(src_b, db);
    let ob = bcx.ins().umin(ob, c255);
    let result = bcx.ins().ishl_imm_u(oa, 24);
    let tmp = bcx.ins().ishl_imm_u(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm_u(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlagsData::new(), result, cur, 0);
    let four = bcx.ins().iconst(ptr_type, 4);
    let next = bcx.ins().iadd(cur, four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let nsi = bcx.ins().iadd(si, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, nsi, remainder);
    bcx.ins()
        .brif(cont, scalar_loop, &block_args(&[next, nsi]), exit, &[]);

    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize(frontend_config);
}

// =============================================================================
// カバレッジ付きパイプライン (新規 CompOp)
// =============================================================================

/// Clear + カバレッジ。out = dst * (1 - cov/255)。
/// cov=255 → out=0、cov=0 → out=dst。
///
/// ## SIMD 最適化
///
/// ```text
/// entry → simd_loop → simd_fast (cov=0xFF → store zero) → simd_next
///                    → simd_slow (dst * (256-cov) >> 8)  → simd_next
///          scalar_check → scalar_loop → exit
/// ```
///
/// cov=0xFF 高速パス: dst を読む必要すらなく、ゼロベクタをストアするだけ。
pub(super) fn build_clear_cov(mut bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    let ptr_type = frontend_config.pointer_type();
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let simd_fast = bcx.create_block();
    let simd_slow = bcx.create_block();
    let simd_next = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let count = bcx.block_params(entry)[2];
    let coverage = bcx.block_params(entry)[3];

    let c256_scalar = bcx.ins().iconst(types::I32, 256);
    let c256_vec = bcx.ins().splat(types::I32X4, c256_scalar);
    let mask_0xff = bcx.ins().iconst(types::I32, 0xFF);
    let mask_0xff_vec = bcx.ins().splat(types::I32X4, mask_0xff);
    let zero_i32 = bcx.ins().iconst(types::I32, 0);
    let zero_vec = bcx.ins().splat(types::I32X4, zero_i32);
    let all_ff = bcx.ins().iconst(types::I32, -1);

    let simd_count = bcx.ins().ushr_imm_u(count, 2);
    let remainder = bcx.ins().band_imm_u(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    let args_simd = block_args(&[dst, coverage, zero]);
    let args_scalar = block_args(&[dst, coverage]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let current_dst = bcx.block_params(simd_loop)[0];
    let current_cov = bcx.block_params(simd_loop)[1];
    let simd_i = bcx.block_params(simd_loop)[2];

    let packed_cov = bcx
        .ins()
        .load(types::I32, MemFlagsData::new(), current_cov, 0);
    let is_all_ff = bcx.ins().icmp(IntCC::Equal, packed_cov, all_ff);
    bcx.ins().brif(is_all_ff, simd_fast, &[], simd_slow, &[]);

    // === simd_fast ブロック (cov=0xFF → out=0) ===
    bcx.switch_to_block(simd_fast);
    bcx.ins()
        .store(MemFlagsData::new(), zero_vec, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_slow ブロック (out = dst * (256 - cov) >> 8) ===
    bcx.switch_to_block(simd_slow);
    let cov_vec = emit_expand_packed_coverage_i32x4(&mut bcx, packed_cov);
    let inv_cov_vec = bcx.ins().isub(c256_vec, cov_vec);

    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlagsData::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    let out_a = bcx.ins().imul(dst_a_v, inv_cov_vec);
    let out_a = bcx.ins().ushr_imm_u(out_a, 8);
    let out_r = bcx.ins().imul(dst_r_v, inv_cov_vec);
    let out_r = bcx.ins().ushr_imm_u(out_r, 8);
    let out_g = bcx.ins().imul(dst_g_v, inv_cov_vec);
    let out_g = bcx.ins().ushr_imm_u(out_g, 8);
    let out_b = bcx.ins().imul(dst_b_v, inv_cov_vec);
    let out_b = bcx.ins().ushr_imm_u(out_b, 8);

    let result = emit_pack_channels_simd(&mut bcx, out_a, out_r, out_g, out_b);
    bcx.ins().store(MemFlagsData::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_next ブロック ===
    bcx.switch_to_block(simd_next);
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_dst = bcx.ins().iadd(current_dst, sixteen);
    let four_ptr = bcx.ins().iconst(ptr_type, 4);
    let next_cov = bcx.ins().iadd(current_cov, four_ptr);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(simd_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, simd_count);
    let args_loop = block_args(&[next_dst, next_cov, next_si]);
    let args_check = block_args(&[next_dst, next_cov]);
    bcx.ins()
        .brif(cont, simd_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let current_dst = bcx.block_params(scalar_check)[0];
    let current_cov = bcx.block_params(scalar_check)[1];
    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    let args_scalar = block_args(&[current_dst, current_cov, zero]);
    bcx.ins()
        .brif(has_remainder, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック ===
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let cur_dst = bcx.block_params(scalar_loop)[0];
    let cur_cov = bcx.block_params(scalar_loop)[1];
    let scalar_i = bcx.block_params(scalar_loop)[2];

    let cov_u8 = bcx.ins().load(types::I8, MemFlagsData::new(), cur_cov, 0);
    let cov = bcx.ins().uextend(types::I32, cov_u8);
    let inv_cov = bcx.ins().isub(c256_scalar, cov);

    let dp = bcx.ins().load(types::I32, MemFlagsData::new(), cur_dst, 0);
    let da = bcx.ins().ushr_imm_u(dp, 24);
    let da = bcx.ins().band_imm_u(da, 0xFF);
    let dr = bcx.ins().ushr_imm_u(dp, 16);
    let dr = bcx.ins().band_imm_u(dr, 0xFF);
    let dg = bcx.ins().ushr_imm_u(dp, 8);
    let dg = bcx.ins().band_imm_u(dg, 0xFF);
    let db = bcx.ins().band_imm_u(dp, 0xFF);

    let oa = bcx.ins().imul(da, inv_cov);
    let oa = bcx.ins().ushr_imm_u(oa, 8);
    let or = bcx.ins().imul(dr, inv_cov);
    let or = bcx.ins().ushr_imm_u(or, 8);
    let og = bcx.ins().imul(dg, inv_cov);
    let og = bcx.ins().ushr_imm_u(og, 8);
    let ob = bcx.ins().imul(db, inv_cov);
    let ob = bcx.ins().ushr_imm_u(ob, 8);

    let result = bcx.ins().ishl_imm_u(oa, 24);
    let tmp = bcx.ins().ishl_imm_u(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm_u(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlagsData::new(), result, cur_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(cur_dst, four);
    let one_ptr = bcx.ins().iconst(ptr_type, 1);
    let next_cov = bcx.ins().iadd(cur_cov, one_ptr);
    let next_si = bcx.ins().iadd(scalar_i, one_ptr);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_cov, next_si]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize(frontend_config);
}

/// DstCopy + カバレッジ。out = dst (何もしない)。
pub(super) fn build_dst_copy_cov(mut bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    let entry = bcx.create_block();
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize(frontend_config);
}

/// 汎用 Porter-Duff カバレッジ付きパイプラインを構築する。
///
/// cov 適用後の `cov_src = div255(src * cov)` を「新しい src」として既存の
/// compose_simd / compose_scalar に渡す。cov=0xFF のとき cov_src = src なので
/// ループ不変のソースチャネルをそのまま使える (高速パス)。
///
/// ## ブロック構成
///
/// ```text
/// entry → simd_loop → simd_fast (cov=0xFF)  → simd_next
///                    → simd_slow (cov!=0xFF) → simd_next
///          scalar_check → scalar_loop → exit
/// ```
pub(super) fn build_generic_compose_cov(
    mut bcx: FunctionBuilder,
    frontend_config: TargetFrontendConfig,
    compose_simd: fn(
        &mut FunctionBuilder,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
    ) -> (Value, Value, Value, Value),
    compose_scalar: fn(
        &mut FunctionBuilder,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
        Value,
    ) -> (Value, Value, Value, Value),
) {
    let ptr_type = frontend_config.pointer_type();
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let simd_fast = bcx.create_block();
    let simd_slow = bcx.create_block();
    let simd_next = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let src_solid = bcx.block_params(entry)[1];
    let count = bcx.block_params(entry)[2];
    let coverage = bcx.block_params(entry)[3];

    // ソースチャネル分解 (ループ不変)
    let src_a = bcx.ins().ushr_imm_u(src_solid, 24);
    let src_a = bcx.ins().band_imm_u(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm_u(src_solid, 16);
    let src_r = bcx.ins().band_imm_u(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm_u(src_solid, 8);
    let src_g = bcx.ins().band_imm_u(src_g, 0xFF);
    let src_b = bcx.ins().band_imm_u(src_solid, 0xFF);

    // SIMD 用ループ不変ベクタ
    let src_a_vec = bcx.ins().splat(types::I32X4, src_a);
    let src_r_vec = bcx.ins().splat(types::I32X4, src_r);
    let src_g_vec = bcx.ins().splat(types::I32X4, src_g);
    let src_b_vec = bcx.ins().splat(types::I32X4, src_b);
    let c257_scalar = bcx.ins().iconst(types::I32, 257);
    let c257_vec = bcx.ins().splat(types::I32X4, c257_scalar);
    let c256_scalar = bcx.ins().iconst(types::I32, 256);
    let c256_vec = bcx.ins().splat(types::I32X4, c256_scalar);
    let mask_0xff = bcx.ins().iconst(types::I32, 0xFF);
    let mask_0xff_vec = bcx.ins().splat(types::I32X4, mask_0xff);
    let all_ff = bcx.ins().iconst(types::I32, -1);

    let simd_count = bcx.ins().ushr_imm_u(count, 2);
    let remainder = bcx.ins().band_imm_u(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    let args_simd = block_args(&[dst, coverage, zero]);
    let args_scalar = block_args(&[dst, coverage]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let current_dst = bcx.block_params(simd_loop)[0];
    let current_cov = bcx.block_params(simd_loop)[1];
    let simd_i = bcx.block_params(simd_loop)[2];

    let packed_cov = bcx
        .ins()
        .load(types::I32, MemFlagsData::new(), current_cov, 0);
    let is_all_ff = bcx.ins().icmp(IntCC::Equal, packed_cov, all_ff);
    bcx.ins().brif(is_all_ff, simd_fast, &[], simd_slow, &[]);

    // === simd_fast ブロック (cov=0xFF: src をそのまま使用) ===
    bcx.switch_to_block(simd_fast);
    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlagsData::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);
    let (oa, or, og, ob) = compose_simd(
        &mut bcx,
        src_a_vec,
        src_r_vec,
        src_g_vec,
        src_b_vec,
        dst_a_v,
        dst_r_v,
        dst_g_v,
        dst_b_v,
        c256_vec,
        mask_0xff_vec,
    );
    let result = emit_pack_channels_simd(&mut bcx, oa, or, og, ob);
    bcx.ins().store(MemFlagsData::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_slow ブロック (cov!=0xFF: div255(src * cov) を計算して使用) ===
    bcx.switch_to_block(simd_slow);
    let cov_vec = emit_expand_packed_coverage_i32x4(&mut bcx, packed_cov);

    // cov_src_c = div255(src_c * cov) = (src_c * cov * 257 + 257) >> 16
    let ca = bcx.ins().imul(src_a_vec, cov_vec);
    let ca = bcx.ins().imul(ca, c257_vec);
    let ca = bcx.ins().iadd(ca, c257_vec);
    let cov_src_a = bcx.ins().ushr_imm_u(ca, 16);

    let cr = bcx.ins().imul(src_r_vec, cov_vec);
    let cr = bcx.ins().imul(cr, c257_vec);
    let cr = bcx.ins().iadd(cr, c257_vec);
    let cov_src_r = bcx.ins().ushr_imm_u(cr, 16);

    let cg = bcx.ins().imul(src_g_vec, cov_vec);
    let cg = bcx.ins().imul(cg, c257_vec);
    let cg = bcx.ins().iadd(cg, c257_vec);
    let cov_src_g = bcx.ins().ushr_imm_u(cg, 16);

    let cb = bcx.ins().imul(src_b_vec, cov_vec);
    let cb = bcx.ins().imul(cb, c257_vec);
    let cb = bcx.ins().iadd(cb, c257_vec);
    let cov_src_b = bcx.ins().ushr_imm_u(cb, 16);

    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlagsData::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);
    let (oa, or, og, ob) = compose_simd(
        &mut bcx,
        cov_src_a,
        cov_src_r,
        cov_src_g,
        cov_src_b,
        dst_a_v,
        dst_r_v,
        dst_g_v,
        dst_b_v,
        c256_vec,
        mask_0xff_vec,
    );
    let result = emit_pack_channels_simd(&mut bcx, oa, or, og, ob);
    bcx.ins().store(MemFlagsData::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_next ブロック (高速・通常パス合流) ===
    bcx.switch_to_block(simd_next);
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_dst = bcx.ins().iadd(current_dst, sixteen);
    let four_ptr = bcx.ins().iconst(ptr_type, 4);
    let next_cov = bcx.ins().iadd(current_cov, four_ptr);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(simd_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, simd_count);
    let args_loop = block_args(&[next_dst, next_cov, next_si]);
    let args_check = block_args(&[next_dst, next_cov]);
    bcx.ins()
        .brif(cont, simd_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let current_dst = bcx.block_params(scalar_check)[0];
    let current_cov = bcx.block_params(scalar_check)[1];
    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    let args_scalar = block_args(&[current_dst, current_cov, zero]);
    bcx.ins()
        .brif(has_remainder, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック ===
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let cur_dst = bcx.block_params(scalar_loop)[0];
    let cur_cov = bcx.block_params(scalar_loop)[1];
    let scalar_i = bcx.block_params(scalar_loop)[2];

    let cov_u8 = bcx.ins().load(types::I8, MemFlagsData::new(), cur_cov, 0);
    let cov = bcx.ins().uextend(types::I32, cov_u8);

    // cov_src_c = div255(src_c * cov) = (src_c * cov * 257 + 257) >> 16
    let ca = bcx.ins().imul(src_a, cov);
    let ca = bcx.ins().imul(ca, c257_scalar);
    let ca = bcx.ins().iadd(ca, c257_scalar);
    let cov_sa = bcx.ins().ushr_imm_u(ca, 16);

    let cr = bcx.ins().imul(src_r, cov);
    let cr = bcx.ins().imul(cr, c257_scalar);
    let cr = bcx.ins().iadd(cr, c257_scalar);
    let cov_sr = bcx.ins().ushr_imm_u(cr, 16);

    let cg = bcx.ins().imul(src_g, cov);
    let cg = bcx.ins().imul(cg, c257_scalar);
    let cg = bcx.ins().iadd(cg, c257_scalar);
    let cov_sg = bcx.ins().ushr_imm_u(cg, 16);

    let cb = bcx.ins().imul(src_b, cov);
    let cb = bcx.ins().imul(cb, c257_scalar);
    let cb = bcx.ins().iadd(cb, c257_scalar);
    let cov_sb = bcx.ins().ushr_imm_u(cb, 16);

    let dp = bcx.ins().load(types::I32, MemFlagsData::new(), cur_dst, 0);
    let da = bcx.ins().ushr_imm_u(dp, 24);
    let da = bcx.ins().band_imm_u(da, 0xFF);
    let dr = bcx.ins().ushr_imm_u(dp, 16);
    let dr = bcx.ins().band_imm_u(dr, 0xFF);
    let dg = bcx.ins().ushr_imm_u(dp, 8);
    let dg = bcx.ins().band_imm_u(dg, 0xFF);
    let db = bcx.ins().band_imm_u(dp, 0xFF);

    let (oa, or, og, ob) = compose_scalar(
        &mut bcx,
        cov_sa,
        cov_sr,
        cov_sg,
        cov_sb,
        da,
        dr,
        dg,
        db,
        c256_scalar,
    );

    let result = bcx.ins().ishl_imm_u(oa, 24);
    let tmp = bcx.ins().ishl_imm_u(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm_u(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlagsData::new(), result, cur_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(cur_dst, four);
    let one_ptr = bcx.ins().iconst(ptr_type, 1);
    let next_cov = bcx.ins().iadd(cur_cov, one_ptr);
    let next_si = bcx.ins().iadd(scalar_i, one_ptr);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_cov, next_si]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize(frontend_config);
}

pub(super) fn build_src_in_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_src_in_simd,
        compose_src_in_scalar,
    );
}

pub(super) fn build_src_out_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_src_out_simd,
        compose_src_out_scalar,
    );
}

pub(super) fn build_src_atop_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_src_atop_simd,
        compose_src_atop_scalar,
    );
}

pub(super) fn build_dst_over_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_dst_over_simd,
        compose_dst_over_scalar,
    );
}

pub(super) fn build_dst_in_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_dst_in_simd,
        compose_dst_in_scalar,
    );
}

pub(super) fn build_dst_out_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_dst_out_simd,
        compose_dst_out_scalar,
    );
}

pub(super) fn build_dst_atop_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_dst_atop_simd,
        compose_dst_atop_scalar,
    );
}

pub(super) fn build_xor_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(bcx, frontend_config, compose_xor_simd, compose_xor_scalar);
}

pub(super) fn build_plus_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(bcx, frontend_config, compose_plus_simd, compose_plus_scalar);
}
