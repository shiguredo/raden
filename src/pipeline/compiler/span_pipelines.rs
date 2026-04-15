// =============================================================================
// span_pipelines.rs -- スパンソース用パイプライン
// =============================================================================
//
// グラデーション等のピクセルごとに色が異なるソースを合成するパイプライン。
// core_pipelines.rs の単色版と同一の合成式だが、src_solid の代わりに
// src_span ポインタからピクセルごとの色を読み出す。

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Type};
use cranelift_frontend::FunctionBuilder;

use super::{
    block_args, emit_expand_packed_coverage_i32x4, emit_extract_channels_simd,
    emit_pack_channels_simd,
};

/// SrcOver スパンパイプラインを構築する (カバレッジなし)。
///
/// ## シグネチャ
///
/// ```text
/// fn pipeline_span(dst: *mut u8, src_span: *const u32, count: usize)
/// ```
///
/// ## 合成式
///
/// ```text
/// inv_alpha = 256 - src_a    (ピクセルごとに異なる)
/// out_c = src_c + (dst_c * inv_alpha) >> 8
/// ```
pub(super) fn build_src_over_span(mut bcx: FunctionBuilder, ptr_type: Type) {
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let src_span = bcx.block_params(entry)[1];
    let count = bcx.block_params(entry)[2];

    let c256_scalar = bcx.ins().iconst(types::I32, 256);
    let c256_vec = bcx.ins().splat(types::I32X4, c256_scalar);
    let mask_0xff = bcx.ins().iconst(types::I32, 0xFF);
    let mask_0xff_vec = bcx.ins().splat(types::I32X4, mask_0xff);

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    let args_simd = block_args(&[dst, src_span, zero]);
    let args_scalar = block_args(&[dst, src_span]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    // 4 ピクセルを I32X4 で並列処理
    bcx.append_block_param(simd_loop, ptr_type); // current_dst
    bcx.append_block_param(simd_loop, ptr_type); // current_src
    bcx.append_block_param(simd_loop, ptr_type); // simd_i
    bcx.switch_to_block(simd_loop);
    let current_dst = bcx.block_params(simd_loop)[0];
    let current_src = bcx.block_params(simd_loop)[1];
    let simd_i = bcx.block_params(simd_loop)[2];

    // ソース 4 ピクセルをロード
    let src_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_src, 0);
    let (src_a_vec, src_r_vec, src_g_vec, src_b_vec) =
        emit_extract_channels_simd(&mut bcx, src_pixels, mask_0xff_vec);

    // inv_alpha = 256 - src_a (ピクセルごと)
    let inv_alpha_vec = bcx.ins().isub(c256_vec, src_a_vec);

    // デスティネーション 4 ピクセルをロード
    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    // out_c = src_c + (dst_c * inv_alpha) >> 8
    let da = bcx.ins().imul(dst_a_v, inv_alpha_vec);
    let da = bcx.ins().ushr_imm(da, 8);
    let out_a = bcx.ins().iadd(src_a_vec, da);

    let dr = bcx.ins().imul(dst_r_v, inv_alpha_vec);
    let dr = bcx.ins().ushr_imm(dr, 8);
    let out_r = bcx.ins().iadd(src_r_vec, dr);

    let dg = bcx.ins().imul(dst_g_v, inv_alpha_vec);
    let dg = bcx.ins().ushr_imm(dg, 8);
    let out_g = bcx.ins().iadd(src_g_vec, dg);

    let db = bcx.ins().imul(dst_b_v, inv_alpha_vec);
    let db = bcx.ins().ushr_imm(db, 8);
    let out_b = bcx.ins().iadd(src_b_vec, db);

    let result = emit_pack_channels_simd(&mut bcx, out_a, out_r, out_g, out_b);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);

    // ポインタ更新
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_dst = bcx.ins().iadd(current_dst, sixteen);
    let next_src = bcx.ins().iadd(current_src, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(simd_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, simd_count);
    let args_loop = block_args(&[next_dst, next_src, next_si]);
    let args_check = block_args(&[next_dst, next_src]);
    bcx.ins()
        .brif(cont, simd_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let current_dst = bcx.block_params(scalar_check)[0];
    let current_src = bcx.block_params(scalar_check)[1];
    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    let args_scalar = block_args(&[current_dst, current_src, zero]);
    bcx.ins()
        .brif(has_remainder, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック ===
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let current_dst = bcx.block_params(scalar_loop)[0];
    let current_src = bcx.block_params(scalar_loop)[1];
    let scalar_i = bcx.block_params(scalar_loop)[2];

    // ソース 1 ピクセルをロード
    let src_pixel = bcx.ins().load(types::I32, MemFlags::new(), current_src, 0);
    let src_a = bcx.ins().ushr_imm(src_pixel, 24);
    let src_a = bcx.ins().band_imm(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm(src_pixel, 16);
    let src_r = bcx.ins().band_imm(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm(src_pixel, 8);
    let src_g = bcx.ins().band_imm(src_g, 0xFF);
    let src_b = bcx.ins().band_imm(src_pixel, 0xFF);

    let inv_alpha = bcx.ins().isub(c256_scalar, src_a);

    let dst_pixel = bcx.ins().load(types::I32, MemFlags::new(), current_dst, 0);
    let dst_a_s = bcx.ins().ushr_imm(dst_pixel, 24);
    let dst_a_s = bcx.ins().band_imm(dst_a_s, 0xFF);
    let dst_r_s = bcx.ins().ushr_imm(dst_pixel, 16);
    let dst_r_s = bcx.ins().band_imm(dst_r_s, 0xFF);
    let dst_g_s = bcx.ins().ushr_imm(dst_pixel, 8);
    let dst_g_s = bcx.ins().band_imm(dst_g_s, 0xFF);
    let dst_b_s = bcx.ins().band_imm(dst_pixel, 0xFF);

    let da = bcx.ins().imul(dst_a_s, inv_alpha);
    let da = bcx.ins().ushr_imm(da, 8);
    let out_a = bcx.ins().iadd(src_a, da);

    let dr = bcx.ins().imul(dst_r_s, inv_alpha);
    let dr = bcx.ins().ushr_imm(dr, 8);
    let out_r = bcx.ins().iadd(src_r, dr);

    let dg = bcx.ins().imul(dst_g_s, inv_alpha);
    let dg = bcx.ins().ushr_imm(dg, 8);
    let out_g = bcx.ins().iadd(src_g, dg);

    let db = bcx.ins().imul(dst_b_s, inv_alpha);
    let db = bcx.ins().ushr_imm(db, 8);
    let out_b = bcx.ins().iadd(src_b, db);

    let result = bcx.ins().ishl_imm(out_a, 24);
    let tmp = bcx.ins().ishl_imm(out_r, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(out_g, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, out_b);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(current_dst, four);
    let next_src = bcx.ins().iadd(current_src, four);
    let one_ptr = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(scalar_i, one_ptr);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_src, next_si]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}

/// SrcOver スパンパイプラインを構築する (カバレッジ付き)。
///
/// ## シグネチャ
///
/// ```text
/// fn pipeline_span_cov(dst: *mut u8, src_span: *const u32, count: usize, coverage: *const u8)
/// ```
pub(super) fn build_src_over_span_cov(mut bcx: FunctionBuilder, ptr_type: Type) {
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
    let src_span = bcx.block_params(entry)[1];
    let count = bcx.block_params(entry)[2];
    let coverage = bcx.block_params(entry)[3];

    let c257_scalar = bcx.ins().iconst(types::I32, 257);
    let c257_vec = bcx.ins().splat(types::I32X4, c257_scalar);
    let c256_scalar = bcx.ins().iconst(types::I32, 256);
    let c256_vec = bcx.ins().splat(types::I32X4, c256_scalar);
    let mask_0xff = bcx.ins().iconst(types::I32, 0xFF);
    let mask_0xff_vec = bcx.ins().splat(types::I32X4, mask_0xff);
    let all_ff = bcx.ins().iconst(types::I32, -1);

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    let args_simd = block_args(&[dst, src_span, coverage, zero]);
    let args_scalar = block_args(&[dst, src_span, coverage]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    bcx.append_block_param(simd_loop, ptr_type); // current_dst
    bcx.append_block_param(simd_loop, ptr_type); // current_src
    bcx.append_block_param(simd_loop, ptr_type); // current_cov
    bcx.append_block_param(simd_loop, ptr_type); // simd_i
    bcx.switch_to_block(simd_loop);
    let current_dst = bcx.block_params(simd_loop)[0];
    let current_src = bcx.block_params(simd_loop)[1];
    let current_cov = bcx.block_params(simd_loop)[2];
    let simd_i = bcx.block_params(simd_loop)[3];

    let packed_cov = bcx.ins().load(types::I32, MemFlags::new(), current_cov, 0);
    let is_all_ff = bcx.ins().icmp(IntCC::Equal, packed_cov, all_ff);
    bcx.ins().brif(is_all_ff, simd_fast, &[], simd_slow, &[]);

    // === simd_fast ブロック (cov=0xFF) ===
    // カバレッジが全て 255 の場合、coverage 適用をスキップして直接 SrcOver 合成
    bcx.switch_to_block(simd_fast);
    let src_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_src, 0);
    let (src_a_vec, src_r_vec, src_g_vec, src_b_vec) =
        emit_extract_channels_simd(&mut bcx, src_pixels, mask_0xff_vec);

    let inv_alpha_vec = bcx.ins().isub(c256_vec, src_a_vec);

    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    let da = bcx.ins().imul(dst_a_v, inv_alpha_vec);
    let da = bcx.ins().ushr_imm(da, 8);
    let out_a = bcx.ins().iadd(src_a_vec, da);

    let dr = bcx.ins().imul(dst_r_v, inv_alpha_vec);
    let dr = bcx.ins().ushr_imm(dr, 8);
    let out_r = bcx.ins().iadd(src_r_vec, dr);

    let dg = bcx.ins().imul(dst_g_v, inv_alpha_vec);
    let dg = bcx.ins().ushr_imm(dg, 8);
    let out_g = bcx.ins().iadd(src_g_vec, dg);

    let db = bcx.ins().imul(dst_b_v, inv_alpha_vec);
    let db = bcx.ins().ushr_imm(db, 8);
    let out_b = bcx.ins().iadd(src_b_vec, db);

    let result = emit_pack_channels_simd(&mut bcx, out_a, out_r, out_g, out_b);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_slow ブロック (通常カバレッジ) ===
    bcx.switch_to_block(simd_slow);
    let cov_vec = emit_expand_packed_coverage_i32x4(&mut bcx, packed_cov);

    let src_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_src, 0);
    let (src_a_vec, src_r_vec, src_g_vec, src_b_vec) =
        emit_extract_channels_simd(&mut bcx, src_pixels, mask_0xff_vec);

    // cov_src_c = div255(src_c * cov)
    let ca = bcx.ins().imul(src_a_vec, cov_vec);
    let ca = bcx.ins().imul(ca, c257_vec);
    let ca = bcx.ins().iadd(ca, c257_vec);
    let cov_src_a = bcx.ins().ushr_imm(ca, 16);

    let cr = bcx.ins().imul(src_r_vec, cov_vec);
    let cr = bcx.ins().imul(cr, c257_vec);
    let cr = bcx.ins().iadd(cr, c257_vec);
    let cov_src_r = bcx.ins().ushr_imm(cr, 16);

    let cg = bcx.ins().imul(src_g_vec, cov_vec);
    let cg = bcx.ins().imul(cg, c257_vec);
    let cg = bcx.ins().iadd(cg, c257_vec);
    let cov_src_g = bcx.ins().ushr_imm(cg, 16);

    let cb = bcx.ins().imul(src_b_vec, cov_vec);
    let cb = bcx.ins().imul(cb, c257_vec);
    let cb = bcx.ins().iadd(cb, c257_vec);
    let cov_src_b = bcx.ins().ushr_imm(cb, 16);

    let inv_alpha_v = bcx.ins().isub(c256_vec, cov_src_a);

    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    let da = bcx.ins().imul(dst_a_v, inv_alpha_v);
    let da = bcx.ins().ushr_imm(da, 8);
    let out_a = bcx.ins().iadd(cov_src_a, da);

    let dr = bcx.ins().imul(dst_r_v, inv_alpha_v);
    let dr = bcx.ins().ushr_imm(dr, 8);
    let out_r = bcx.ins().iadd(cov_src_r, dr);

    let dg = bcx.ins().imul(dst_g_v, inv_alpha_v);
    let dg = bcx.ins().ushr_imm(dg, 8);
    let out_g = bcx.ins().iadd(cov_src_g, dg);

    let db = bcx.ins().imul(dst_b_v, inv_alpha_v);
    let db = bcx.ins().ushr_imm(db, 8);
    let out_b = bcx.ins().iadd(cov_src_b, db);

    let result = emit_pack_channels_simd(&mut bcx, out_a, out_r, out_g, out_b);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_next ブロック ===
    bcx.switch_to_block(simd_next);
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_dst = bcx.ins().iadd(current_dst, sixteen);
    let next_src = bcx.ins().iadd(current_src, sixteen);
    let four_ptr = bcx.ins().iconst(ptr_type, 4);
    let next_cov = bcx.ins().iadd(current_cov, four_ptr);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(simd_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, simd_count);
    let args_loop = block_args(&[next_dst, next_src, next_cov, next_si]);
    let args_check = block_args(&[next_dst, next_src, next_cov]);
    bcx.ins()
        .brif(cont, simd_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let current_dst = bcx.block_params(scalar_check)[0];
    let current_src = bcx.block_params(scalar_check)[1];
    let current_cov = bcx.block_params(scalar_check)[2];
    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    let args_scalar = block_args(&[current_dst, current_src, current_cov, zero]);
    bcx.ins()
        .brif(has_remainder, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック ===
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let current_dst = bcx.block_params(scalar_loop)[0];
    let current_src = bcx.block_params(scalar_loop)[1];
    let current_cov = bcx.block_params(scalar_loop)[2];
    let scalar_i = bcx.block_params(scalar_loop)[3];

    // ソース 1 ピクセルをロード
    let src_pixel = bcx.ins().load(types::I32, MemFlags::new(), current_src, 0);
    let src_a = bcx.ins().ushr_imm(src_pixel, 24);
    let src_a = bcx.ins().band_imm(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm(src_pixel, 16);
    let src_r = bcx.ins().band_imm(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm(src_pixel, 8);
    let src_g = bcx.ins().band_imm(src_g, 0xFF);
    let src_b = bcx.ins().band_imm(src_pixel, 0xFF);

    // カバレッジ 1 バイトロード
    let cov_u8 = bcx.ins().load(types::I8, MemFlags::new(), current_cov, 0);
    let cov = bcx.ins().uextend(types::I32, cov_u8);

    // cov_src_c = div255(src_c * cov)
    let ca = bcx.ins().imul(src_a, cov);
    let ca = bcx.ins().imul(ca, c257_scalar);
    let ca = bcx.ins().iadd(ca, c257_scalar);
    let cov_src_a = bcx.ins().ushr_imm(ca, 16);

    let cr = bcx.ins().imul(src_r, cov);
    let cr = bcx.ins().imul(cr, c257_scalar);
    let cr = bcx.ins().iadd(cr, c257_scalar);
    let cov_src_r = bcx.ins().ushr_imm(cr, 16);

    let cg = bcx.ins().imul(src_g, cov);
    let cg = bcx.ins().imul(cg, c257_scalar);
    let cg = bcx.ins().iadd(cg, c257_scalar);
    let cov_src_g = bcx.ins().ushr_imm(cg, 16);

    let cb = bcx.ins().imul(src_b, cov);
    let cb = bcx.ins().imul(cb, c257_scalar);
    let cb = bcx.ins().iadd(cb, c257_scalar);
    let cov_src_b = bcx.ins().ushr_imm(cb, 16);

    let inv_alpha = bcx.ins().isub(c256_scalar, cov_src_a);

    let dst_pixel = bcx.ins().load(types::I32, MemFlags::new(), current_dst, 0);
    let dst_a_s = bcx.ins().ushr_imm(dst_pixel, 24);
    let dst_a_s = bcx.ins().band_imm(dst_a_s, 0xFF);
    let dst_r_s = bcx.ins().ushr_imm(dst_pixel, 16);
    let dst_r_s = bcx.ins().band_imm(dst_r_s, 0xFF);
    let dst_g_s = bcx.ins().ushr_imm(dst_pixel, 8);
    let dst_g_s = bcx.ins().band_imm(dst_g_s, 0xFF);
    let dst_b_s = bcx.ins().band_imm(dst_pixel, 0xFF);

    let da = bcx.ins().imul(dst_a_s, inv_alpha);
    let da = bcx.ins().ushr_imm(da, 8);
    let out_a = bcx.ins().iadd(cov_src_a, da);

    let dr = bcx.ins().imul(dst_r_s, inv_alpha);
    let dr = bcx.ins().ushr_imm(dr, 8);
    let out_r = bcx.ins().iadd(cov_src_r, dr);

    let dg = bcx.ins().imul(dst_g_s, inv_alpha);
    let dg = bcx.ins().ushr_imm(dg, 8);
    let out_g = bcx.ins().iadd(cov_src_g, dg);

    let db = bcx.ins().imul(dst_b_s, inv_alpha);
    let db = bcx.ins().ushr_imm(db, 8);
    let out_b = bcx.ins().iadd(cov_src_b, db);

    let result = bcx.ins().ishl_imm(out_a, 24);
    let tmp = bcx.ins().ishl_imm(out_r, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(out_g, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, out_b);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(current_dst, four);
    let next_src = bcx.ins().iadd(current_src, four);
    let one_ptr = bcx.ins().iconst(ptr_type, 1);
    let next_cov = bcx.ins().iadd(current_cov, one_ptr);
    let next_si = bcx.ins().iadd(scalar_i, one_ptr);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_src, next_cov, next_si]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}
