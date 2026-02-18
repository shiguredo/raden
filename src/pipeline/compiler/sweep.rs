use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value};
use cranelift_frontend::FunctionBuilder;

use super::block_args;
use crate::api::style::FillRule;

// =============================================================================
// Sweep 関数
// =============================================================================

/// shifted 値を fill rule に応じてカバレッジ値に変換する JIT コードを生成する。
///
/// - NonZero: iabs → umin(c255) (2 命令)
/// - EvenOdd: iabs → band_imm(511) → c512 - val → umin(val, folded) → umin(c255) (5 命令)
pub(super) fn emit_fill_rule_convert(
    bcx: &mut FunctionBuilder,
    shifted: Value,
    c255: Value,
    fill_rule: FillRule,
) -> Value {
    match fill_rule {
        FillRule::NonZero => {
            let abs_val = bcx.ins().iabs(shifted);
            bcx.ins().umin(abs_val, c255)
        }
        FillRule::EvenOdd => {
            let abs_val = bcx.ins().iabs(shifted);
            let val = bcx.ins().band_imm(abs_val, 511);
            let c512 = bcx.ins().iconst(types::I32, 512);
            let folded = bcx.ins().isub(c512, val);
            let min_vf = bcx.ins().umin(val, folded);
            bcx.ins().umin(min_vf, c255)
        }
    }
}

/// JIT sweep 関数を構築する。
///
/// prefix sum (累積和) + abs + clamp(255) を計算し、結果を u8 バッファに書き込む。
/// 4 要素アンロールにより 4 バイトを 1 つの i32 ストアに統合する。
///
/// ## IR 構造 (5 ブロック)
///
/// ```text
/// entry:
///   count4 = len >> 2, rem = len & 3
///   count4 > 0 ? → main_loop : scalar_check
///
/// main_loop(cells_p, cov_p, i, cover):
///   c0..c3 = load.i32(cells_p, offset 0/4/8/12)
///   cover を逐次加算、各段で iabs → umin(255)
///   packed = v0 | (v1<<8) | (v2<<16) | (v3<<24)
///   store.i32(cov_p, packed)
///   i+1 < count4 ? → main_loop : scalar_check
///
/// scalar_check(cells_p, cov_p, cover):
///   rem > 0 ? → scalar_loop : exit
///
/// scalar_loop(cells_p, cov_p, j, cover):
///   load → iadd → iabs → umin(255) → istore8
///   j+1 < rem ? → scalar_loop : exit
///
/// exit: return
/// ```
///
/// prefix sum は逐次依存のため SIMD 化不可だが、4 要素アンロールで:
/// - 4 byte ストア → 1 i32 ストアに統合
/// - 命令パイプラインが abs + clamp + shift を並列実行可能
pub(super) fn build_sweep(mut bcx: FunctionBuilder, ptr_type: Type, fill_rule: FillRule) {
    let entry = bcx.create_block();
    let main_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let cells = bcx.block_params(entry)[0]; // *const i32
    let cov_buf = bcx.block_params(entry)[1]; // *mut u8
    let len = bcx.block_params(entry)[2]; // usize

    let count4 = bcx.ins().ushr_imm(len, 2);
    let rem = bcx.ins().band_imm(len, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);
    let c255 = bcx.ins().iconst(types::I32, 255);
    let cover_init = bcx.ins().iconst(types::I32, 0);

    let has_main = bcx.ins().icmp(IntCC::NotEqual, count4, zero);
    let args_main = block_args(&[cells, cov_buf, zero, cover_init]);
    let args_scalar = block_args(&[cells, cov_buf, cover_init]);
    bcx.ins()
        .brif(has_main, main_loop, &args_main, scalar_check, &args_scalar);

    // === main_loop ブロック (4 要素アンロール) ===
    // ブロックパラメータ: (cells_p, cov_p, i, cover)
    bcx.append_block_param(main_loop, ptr_type); // cells_p
    bcx.append_block_param(main_loop, ptr_type); // cov_p
    bcx.append_block_param(main_loop, ptr_type); // i
    bcx.append_block_param(main_loop, types::I32); // cover
    bcx.switch_to_block(main_loop);
    let cells_p = bcx.block_params(main_loop)[0];
    let cov_p = bcx.block_params(main_loop)[1];
    let i = bcx.block_params(main_loop)[2];
    let cover = bcx.block_params(main_loop)[3];

    // c0..c3 = load.i32(cells_p, offset 0/4/8/12)
    let c0 = bcx.ins().load(types::I32, MemFlags::new(), cells_p, 0);
    let c1 = bcx.ins().load(types::I32, MemFlags::new(), cells_p, 4);
    let c2 = bcx.ins().load(types::I32, MemFlags::new(), cells_p, 8);
    let c3 = bcx.ins().load(types::I32, MemFlags::new(), cells_p, 12);

    // cover を逐次加算し、各段で sshr(9) + fill_rule 変換
    let cover = bcx.ins().iadd(cover, c0);
    let shifted0 = bcx.ins().sshr_imm(cover, 9);
    let v0 = emit_fill_rule_convert(&mut bcx, shifted0, c255, fill_rule);

    let cover = bcx.ins().iadd(cover, c1);
    let shifted1 = bcx.ins().sshr_imm(cover, 9);
    let v1 = emit_fill_rule_convert(&mut bcx, shifted1, c255, fill_rule);

    let cover = bcx.ins().iadd(cover, c2);
    let shifted2 = bcx.ins().sshr_imm(cover, 9);
    let v2 = emit_fill_rule_convert(&mut bcx, shifted2, c255, fill_rule);

    let cover = bcx.ins().iadd(cover, c3);
    let shifted3 = bcx.ins().sshr_imm(cover, 9);
    let v3 = emit_fill_rule_convert(&mut bcx, shifted3, c255, fill_rule);

    // packed = v0 | (v1<<8) | (v2<<16) | (v3<<24)
    let packed = v0;
    let tmp = bcx.ins().ishl_imm(v1, 8);
    let packed = bcx.ins().bor(packed, tmp);
    let tmp = bcx.ins().ishl_imm(v2, 16);
    let packed = bcx.ins().bor(packed, tmp);
    let tmp = bcx.ins().ishl_imm(v3, 24);
    let packed = bcx.ins().bor(packed, tmp);

    // 4 バイト一括ストア
    bcx.ins().store(MemFlags::new(), packed, cov_p, 0);

    // ポインタ更新: cells_p += 16 (4 * i32), cov_p += 4
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_cells_p = bcx.ins().iadd(cells_p, sixteen);
    let four_ptr = bcx.ins().iconst(ptr_type, 4);
    let next_cov_p = bcx.ins().iadd(cov_p, four_ptr);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_i = bcx.ins().iadd(i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_i, count4);
    let args_loop = block_args(&[next_cells_p, next_cov_p, next_i, cover]);
    let args_check = block_args(&[next_cells_p, next_cov_p, cover]);
    bcx.ins()
        .brif(cont, main_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    // ブロックパラメータ: (cells_p, cov_p, cover)
    bcx.append_block_param(scalar_check, ptr_type); // cells_p
    bcx.append_block_param(scalar_check, ptr_type); // cov_p
    bcx.append_block_param(scalar_check, types::I32); // cover
    bcx.switch_to_block(scalar_check);
    let cells_p = bcx.block_params(scalar_check)[0];
    let cov_p = bcx.block_params(scalar_check)[1];
    let cover = bcx.block_params(scalar_check)[2];
    let has_rem = bcx.ins().icmp(IntCC::NotEqual, rem, zero);
    let args_scalar = block_args(&[cells_p, cov_p, zero, cover]);
    bcx.ins()
        .brif(has_rem, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック (余り 1-3 ピクセル) ===
    // ブロックパラメータ: (cells_p, cov_p, j, cover)
    bcx.append_block_param(scalar_loop, ptr_type); // cells_p
    bcx.append_block_param(scalar_loop, ptr_type); // cov_p
    bcx.append_block_param(scalar_loop, ptr_type); // j
    bcx.append_block_param(scalar_loop, types::I32); // cover
    bcx.switch_to_block(scalar_loop);
    let cells_p = bcx.block_params(scalar_loop)[0];
    let cov_p = bcx.block_params(scalar_loop)[1];
    let j = bcx.block_params(scalar_loop)[2];
    let cover = bcx.block_params(scalar_loop)[3];

    // load → iadd → sshr(9) → fill_rule 変換 → istore8
    let cell_val = bcx.ins().load(types::I32, MemFlags::new(), cells_p, 0);
    let cover = bcx.ins().iadd(cover, cell_val);
    let shifted = bcx.ins().sshr_imm(cover, 9);
    let clamped = emit_fill_rule_convert(&mut bcx, shifted, c255, fill_rule);
    bcx.ins().istore8(MemFlags::new(), clamped, cov_p, 0);

    // ポインタ更新: cells_p += 4, cov_p += 1
    let four_bytes = bcx.ins().iconst(ptr_type, 4);
    let next_cells_p = bcx.ins().iadd(cells_p, four_bytes);
    let one_byte = bcx.ins().iconst(ptr_type, 1);
    let next_cov_p = bcx.ins().iadd(cov_p, one_byte);
    let next_j = bcx.ins().iadd(j, one_byte);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_j, rem);
    let args_loop = block_args(&[next_cells_p, next_cov_p, next_j, cover]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}
