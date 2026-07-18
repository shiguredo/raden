use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Type};
use cranelift_frontend::FunctionBuilder;

use super::block_args;
use super::core_pipelines::emit_src_over_ag_rb_simd;

// =============================================================================
// Box パイプライン (矩形塗りつぶし専用、y ループ内蔵)
// =============================================================================

/// SrcOver 矩形専用パイプラインを構築する。
///
/// y ループを JIT 内に含み、4x SIMD アンロール (16px/反復) で処理する。
///
/// ## IR 構造
///
/// ```text
/// entry:
///   ループ不変値 (src_ag/rb_vec, inv_alpha_vec 等) を計算
///   count16 = width / 16, tail_quads = (width % 16) / 4, remainder = width % 4
///   → y_loop
///
/// y_loop(scanline_dst, y_i):
///   count16 > 0? → unroll_loop : tail_check
///
/// unroll_loop(x_dst, unroll_i):
///   16px (4x I32X4) を処理
///   → unroll_loop or tail_check
///
/// tail_check(x_dst):
///   tail_quads > 0? → tail_loop : scalar_check
///
/// tail_loop(x_dst, tail_i):
///   4px (1x I32X4) を処理
///   → tail_loop or scalar_check
///
/// scalar_check(x_dst):
///   remainder > 0? → scalar_loop : y_advance
///
/// scalar_loop(x_dst, scalar_i):
///   1px を処理
///   → scalar_loop or y_advance
///
/// y_advance:
///   next_scanline = scanline_dst + stride
///   next_y < height? → y_loop : exit
///
/// exit: return
/// ```
pub(super) fn build_src_over_box(mut bcx: FunctionBuilder, ptr_type: Type) {
    let vec_type = types::I32X4;

    let entry = bcx.create_block();
    let y_loop = bcx.create_block();
    let unroll_loop = bcx.create_block();
    let tail_check = bcx.create_block();
    let tail_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let y_advance = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let src_solid = bcx.block_params(entry)[1];
    let width = bcx.block_params(entry)[2];
    let height = bcx.block_params(entry)[3];
    let stride = bcx.block_params(entry)[4];

    // AG/RB 分解 (ソース、ループ不変)
    let mask_00ff00ff = bcx.ins().iconst(types::I32, 0x00FF00FFu32 as i64);
    let src_ag = bcx.ins().ushr_imm(src_solid, 8);
    let src_ag = bcx.ins().band(src_ag, mask_00ff00ff);
    let src_rb = bcx.ins().band(src_solid, mask_00ff00ff);

    // inv_alpha = 256 - src_a
    let src_a = bcx.ins().ushr_imm(src_solid, 24);
    let src_a = bcx.ins().band_imm(src_a, 0xFF);
    let c256 = bcx.ins().iconst(types::I32, 256);
    let inv_alpha = bcx.ins().isub(c256, src_a);

    // SIMD 用ループ不変ベクタ
    let src_ag_vec = bcx.ins().splat(vec_type, src_ag);
    let src_rb_vec = bcx.ins().splat(vec_type, src_rb);
    let inv_alpha_vec = bcx.ins().splat(vec_type, inv_alpha);
    let mask_vec = bcx.ins().splat(vec_type, mask_00ff00ff);

    // ループカウント (width は全スキャンラインで同じ)
    let count16 = bcx.ins().ushr_imm(width, 4);
    let tail_quads = bcx.ins().band_imm(width, 0xF);
    let tail_quads = bcx.ins().ushr_imm(tail_quads, 2);
    let remainder = bcx.ins().band_imm(width, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    // SIMD 不変値を y_loop のブロックパラメータとして渡す。
    // Cranelift の sink 最適化が不変値をループ内に移動するのを防ぐため、
    // entry → y_loop と y_advance → y_loop の両方で同じ値を渡す。
    bcx.ins().jump(
        y_loop,
        &block_args(&[dst, zero, src_ag_vec, src_rb_vec, inv_alpha_vec, mask_vec]),
    );

    // === y_loop ブロック ===
    // ブロックパラメータでループ不変 SIMD ベクタを保持し、
    // レジスタに固定させることで y ループ先頭での再計算を防ぐ。
    bcx.append_block_param(y_loop, ptr_type); // scanline_dst
    bcx.append_block_param(y_loop, ptr_type); // y_i
    bcx.append_block_param(y_loop, vec_type); // src_ag_vec
    bcx.append_block_param(y_loop, vec_type); // src_rb_vec
    bcx.append_block_param(y_loop, vec_type); // inv_alpha_vec
    bcx.append_block_param(y_loop, vec_type); // mask_vec
    bcx.switch_to_block(y_loop);
    let scanline_dst = bcx.block_params(y_loop)[0];
    let y_i = bcx.block_params(y_loop)[1];
    let src_ag_vec = bcx.block_params(y_loop)[2];
    let src_rb_vec = bcx.block_params(y_loop)[3];
    let inv_alpha_vec = bcx.block_params(y_loop)[4];
    let mask_vec = bcx.block_params(y_loop)[5];

    let has_unroll = bcx.ins().icmp(IntCC::NotEqual, count16, zero);
    bcx.ins().brif(
        has_unroll,
        unroll_loop,
        &block_args(&[scanline_dst, zero]),
        tail_check,
        &block_args(&[scanline_dst]),
    );

    // === unroll_loop ブロック (16px/反復 = 4x I32X4) ===
    bcx.append_block_param(unroll_loop, ptr_type); // x_dst
    bcx.append_block_param(unroll_loop, ptr_type); // unroll_i
    bcx.switch_to_block(unroll_loop);
    let x_dst = bcx.block_params(unroll_loop)[0];
    let unroll_i = bcx.block_params(unroll_loop)[1];

    // チャンク 0: offset 0
    let px0 = bcx.ins().load(vec_type, MemFlagsData::new(), x_dst, 0);
    let r0 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px0,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlagsData::new(), r0, x_dst, 0);

    // チャンク 1: offset 16
    let px1 = bcx.ins().load(vec_type, MemFlagsData::new(), x_dst, 16);
    let r1 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px1,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlagsData::new(), r1, x_dst, 16);

    // チャンク 2: offset 32
    let px2 = bcx.ins().load(vec_type, MemFlagsData::new(), x_dst, 32);
    let r2 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px2,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlagsData::new(), r2, x_dst, 32);

    // チャンク 3: offset 48
    let px3 = bcx.ins().load(vec_type, MemFlagsData::new(), x_dst, 48);
    let r3 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px3,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlagsData::new(), r3, x_dst, 48);

    let sixty_four = bcx.ins().iconst(ptr_type, 64);
    let next_x_dst = bcx.ins().iadd(x_dst, sixty_four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_i = bcx.ins().iadd(unroll_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_i, count16);
    bcx.ins().brif(
        cont,
        unroll_loop,
        &block_args(&[next_x_dst, next_i]),
        tail_check,
        &block_args(&[next_x_dst]),
    );

    // === tail_check ブロック ===
    bcx.append_block_param(tail_check, ptr_type);
    bcx.switch_to_block(tail_check);
    let x_dst = bcx.block_params(tail_check)[0];
    let has_tail = bcx.ins().icmp(IntCC::NotEqual, tail_quads, zero);
    bcx.ins().brif(
        has_tail,
        tail_loop,
        &block_args(&[x_dst, zero]),
        scalar_check,
        &block_args(&[x_dst]),
    );

    // === tail_loop ブロック (4px/反復) ===
    bcx.append_block_param(tail_loop, ptr_type); // x_dst
    bcx.append_block_param(tail_loop, ptr_type); // tail_i
    bcx.switch_to_block(tail_loop);
    let x_dst = bcx.block_params(tail_loop)[0];
    let tail_i = bcx.block_params(tail_loop)[1];

    let px = bcx.ins().load(vec_type, MemFlagsData::new(), x_dst, 0);
    let r = emit_src_over_ag_rb_simd(
        &mut bcx,
        px,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlagsData::new(), r, x_dst, 0);

    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_x_dst = bcx.ins().iadd(x_dst, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_ti = bcx.ins().iadd(tail_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_ti, tail_quads);
    bcx.ins().brif(
        cont,
        tail_loop,
        &block_args(&[next_x_dst, next_ti]),
        scalar_check,
        &block_args(&[next_x_dst]),
    );

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let x_dst = bcx.block_params(scalar_check)[0];
    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    bcx.ins().brif(
        has_remainder,
        scalar_loop,
        &block_args(&[x_dst, zero]),
        y_advance,
        &[],
    );

    // === scalar_loop ブロック (1px) ===
    bcx.append_block_param(scalar_loop, ptr_type); // x_dst
    bcx.append_block_param(scalar_loop, ptr_type); // scalar_i
    bcx.switch_to_block(scalar_loop);
    let x_dst = bcx.block_params(scalar_loop)[0];
    let scalar_i = bcx.block_params(scalar_loop)[1];

    let dst_pixel = bcx.ins().load(types::I32, MemFlagsData::new(), x_dst, 0);
    let dst_ag = bcx.ins().ushr_imm(dst_pixel, 8);
    let dst_ag = bcx.ins().band(dst_ag, mask_00ff00ff);
    let dst_rb = bcx.ins().band(dst_pixel, mask_00ff00ff);

    let tmp_ag = bcx.ins().imul(dst_ag, inv_alpha);
    let tmp_ag = bcx.ins().ushr_imm(tmp_ag, 8);
    let tmp_ag = bcx.ins().band(tmp_ag, mask_00ff00ff);
    let out_ag = bcx.ins().iadd(src_ag, tmp_ag);

    let tmp_rb = bcx.ins().imul(dst_rb, inv_alpha);
    let tmp_rb = bcx.ins().ushr_imm(tmp_rb, 8);
    let tmp_rb = bcx.ins().band(tmp_rb, mask_00ff00ff);
    let out_rb = bcx.ins().iadd(src_rb, tmp_rb);

    let result = bcx.ins().ishl_imm(out_ag, 8);
    let result = bcx.ins().bor(result, out_rb);

    bcx.ins().store(MemFlagsData::new(), result, x_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next_x_dst = bcx.ins().iadd(x_dst, four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(scalar_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    bcx.ins().brif(
        cont,
        scalar_loop,
        &block_args(&[next_x_dst, next_si]),
        y_advance,
        &[],
    );

    // === y_advance ブロック ===
    bcx.switch_to_block(y_advance);
    let next_scanline = bcx.ins().iadd(scanline_dst, stride);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_y = bcx.ins().iadd(y_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_y, height);
    // SIMD 不変値をそのまま渡し戻す (レジスタ固定)
    bcx.ins().brif(
        cont,
        y_loop,
        &block_args(&[
            next_scanline,
            next_y,
            src_ag_vec,
            src_rb_vec,
            inv_alpha_vec,
            mask_vec,
        ]),
        exit,
        &[],
    );

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}

/// SrcCopy 矩形専用パイプラインを構築する。
///
/// y ループを JIT 内に含み、4x SIMD アンロール (16px/反復) で処理する。
/// SrcCopy は splat 済みベクタをストアするだけなので非常に高速。
pub(super) fn build_src_copy_box(mut bcx: FunctionBuilder, ptr_type: Type) {
    let entry = bcx.create_block();
    let y_loop = bcx.create_block();
    let unroll_loop = bcx.create_block();
    let tail_check = bcx.create_block();
    let tail_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let y_advance = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let src_solid = bcx.block_params(entry)[1];
    let width = bcx.block_params(entry)[2];
    let height = bcx.block_params(entry)[3];
    let stride = bcx.block_params(entry)[4];

    let src_vec = bcx.ins().splat(types::I32X4, src_solid);

    let count16 = bcx.ins().ushr_imm(width, 4);
    let tail_quads = bcx.ins().band_imm(width, 0xF);
    let tail_quads = bcx.ins().ushr_imm(tail_quads, 2);
    let remainder = bcx.ins().band_imm(width, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    bcx.ins().jump(y_loop, &block_args(&[dst, zero]));

    // === y_loop ブロック ===
    bcx.append_block_param(y_loop, ptr_type);
    bcx.append_block_param(y_loop, ptr_type);
    bcx.switch_to_block(y_loop);
    let scanline_dst = bcx.block_params(y_loop)[0];
    let y_i = bcx.block_params(y_loop)[1];

    let has_unroll = bcx.ins().icmp(IntCC::NotEqual, count16, zero);
    bcx.ins().brif(
        has_unroll,
        unroll_loop,
        &block_args(&[scanline_dst, zero]),
        tail_check,
        &block_args(&[scanline_dst]),
    );

    // === unroll_loop ブロック (16px/反復) ===
    bcx.append_block_param(unroll_loop, ptr_type);
    bcx.append_block_param(unroll_loop, ptr_type);
    bcx.switch_to_block(unroll_loop);
    let x_dst = bcx.block_params(unroll_loop)[0];
    let unroll_i = bcx.block_params(unroll_loop)[1];

    bcx.ins().store(MemFlagsData::new(), src_vec, x_dst, 0);
    bcx.ins().store(MemFlagsData::new(), src_vec, x_dst, 16);
    bcx.ins().store(MemFlagsData::new(), src_vec, x_dst, 32);
    bcx.ins().store(MemFlagsData::new(), src_vec, x_dst, 48);

    let sixty_four = bcx.ins().iconst(ptr_type, 64);
    let next_x_dst = bcx.ins().iadd(x_dst, sixty_four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_i = bcx.ins().iadd(unroll_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_i, count16);
    bcx.ins().brif(
        cont,
        unroll_loop,
        &block_args(&[next_x_dst, next_i]),
        tail_check,
        &block_args(&[next_x_dst]),
    );

    // === tail_check ブロック ===
    bcx.append_block_param(tail_check, ptr_type);
    bcx.switch_to_block(tail_check);
    let x_dst = bcx.block_params(tail_check)[0];
    let has_tail = bcx.ins().icmp(IntCC::NotEqual, tail_quads, zero);
    bcx.ins().brif(
        has_tail,
        tail_loop,
        &block_args(&[x_dst, zero]),
        scalar_check,
        &block_args(&[x_dst]),
    );

    // === tail_loop ブロック (4px/反復) ===
    bcx.append_block_param(tail_loop, ptr_type);
    bcx.append_block_param(tail_loop, ptr_type);
    bcx.switch_to_block(tail_loop);
    let x_dst = bcx.block_params(tail_loop)[0];
    let tail_i = bcx.block_params(tail_loop)[1];

    bcx.ins().store(MemFlagsData::new(), src_vec, x_dst, 0);

    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_x_dst = bcx.ins().iadd(x_dst, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_ti = bcx.ins().iadd(tail_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_ti, tail_quads);
    bcx.ins().brif(
        cont,
        tail_loop,
        &block_args(&[next_x_dst, next_ti]),
        scalar_check,
        &block_args(&[next_x_dst]),
    );

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let x_dst = bcx.block_params(scalar_check)[0];
    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    bcx.ins().brif(
        has_remainder,
        scalar_loop,
        &block_args(&[x_dst, zero]),
        y_advance,
        &[],
    );

    // === scalar_loop ブロック (1px) ===
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let x_dst = bcx.block_params(scalar_loop)[0];
    let scalar_i = bcx.block_params(scalar_loop)[1];

    bcx.ins().store(MemFlagsData::new(), src_solid, x_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next_x_dst = bcx.ins().iadd(x_dst, four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(scalar_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    bcx.ins().brif(
        cont,
        scalar_loop,
        &block_args(&[next_x_dst, next_si]),
        y_advance,
        &[],
    );

    // === y_advance ブロック ===
    bcx.switch_to_block(y_advance);
    let next_scanline = bcx.ins().iadd(scanline_dst, stride);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_y = bcx.ins().iadd(y_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_y, height);
    bcx.ins().brif(
        cont,
        y_loop,
        &block_args(&[next_scanline, next_y]),
        exit,
        &[],
    );

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}
