use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{Endianness, InstBuilder, MemFlagsData, Type, Value};
use cranelift_frontend::FunctionBuilder;

use super::block_args;
use crate::api::style::FillRule;

// =============================================================================
// Sweep 関数
// =============================================================================

/// shifted 値を fill rule に応じてカバレッジ値に変換する JIT コードを生成する (スカラー版)。
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

/// shifted 値を fill rule に応じてカバレッジ値に変換する JIT コードを生成する (SIMD I32X4 版)。
///
/// スカラー版と同一のロジックを I32X4 上で 4 要素並列に実行する。
/// NonZero: 2 SIMD 命令、EvenOdd: 5 SIMD 命令で 4 要素同時処理。
fn emit_fill_rule_convert_simd(
    bcx: &mut FunctionBuilder,
    shifted: Value,
    c255_vec: Value,
    fill_rule: FillRule,
) -> Value {
    match fill_rule {
        FillRule::NonZero => {
            let abs_val = bcx.ins().iabs(shifted);
            bcx.ins().umin(abs_val, c255_vec)
        }
        FillRule::EvenOdd => {
            let abs_val = bcx.ins().iabs(shifted);
            let c511 = bcx.ins().iconst(types::I32, 511);
            let c511_vec = bcx.ins().splat(types::I32X4, c511);
            let val = bcx.ins().band(abs_val, c511_vec);
            let c512 = bcx.ins().iconst(types::I32, 512);
            let c512_vec = bcx.ins().splat(types::I32X4, c512);
            let folded = bcx.ins().isub(c512_vec, val);
            let min_vf = bcx.ins().umin(val, folded);
            bcx.ins().umin(min_vf, c255_vec)
        }
    }
}

/// JIT sweep 関数を構築する。
///
/// prefix sum (累積和) + abs + clamp(255) を計算し、結果を u8 バッファに書き込む。
///
/// ## SIMD 最適化
///
/// prefix sum はセル間の逐次依存があるためスカラーで計算するが、
/// 4 要素分の prefix sum 結果を I32X4 にパックし:
///
/// 1. fill_rule 変換 (sshr + iabs + umin) を SIMD I32X4 で 4 要素並列実行
/// 2. unarrow (I32X4 → I16X8 → I8X16) で効率的にバイトパック
///
/// これにより、4 要素あたり:
/// - fill_rule 変換: 4 × スカラー → 1 × SIMD (命令数 1/4)
/// - バイトパック: shift/or 7 命令 → unarrow 2 命令 + bitcast + extractlane
///
/// ## IR 構造 (5 ブロック)
///
/// ```text
/// entry:
///   count4 = len >> 2, rem = len & 3
///   count4 > 0 ? → main_loop : scalar_check
///
/// main_loop(cells_p, cov_p, i, cover, c255_vec, zero_vec):
///   c0..c3 = load.i32 (4 セル個別ロード)
///   cover を逐次加算し 4 つの prefix sum 値を取得
///   I32X4 にパック → sshr_imm(9) → fill_rule_convert_simd
///   unarrow で I32X4 → I8X16 → extractlane(0) で 4 バイト packed i32
///   store.i32(cov_p, packed)
///   i+1 < count4 ? → main_loop : scalar_check
///
/// scalar_check / scalar_loop / exit: 余り 1-3 ピクセルをスカラー処理
/// ```
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
    let c255_vec = bcx.ins().splat(types::I32X4, c255);
    let cover_init = bcx.ins().iconst(types::I32, 0);
    let zero_vec = bcx.ins().splat(types::I32X4, cover_init);

    let has_main = bcx.ins().icmp(IntCC::NotEqual, count4, zero);
    let args_main = block_args(&[cells, cov_buf, zero, cover_init, c255_vec, zero_vec]);
    let args_scalar = block_args(&[cells, cov_buf, cover_init]);
    bcx.ins()
        .brif(has_main, main_loop, &args_main, scalar_check, &args_scalar);

    // === main_loop ブロック (SIMD fill_rule 変換 + unarrow パック) ===
    // ブロックパラメータ: (cells_p, cov_p, i, cover, c255_vec, zero_vec)
    // c255_vec, zero_vec はループ不変。ブロックパラメータで保持して sink 最適化を回避する。
    bcx.append_block_param(main_loop, ptr_type); // cells_p
    bcx.append_block_param(main_loop, ptr_type); // cov_p
    bcx.append_block_param(main_loop, ptr_type); // i
    bcx.append_block_param(main_loop, types::I32); // cover
    bcx.append_block_param(main_loop, types::I32X4); // c255_vec
    bcx.append_block_param(main_loop, types::I32X4); // zero_vec
    bcx.switch_to_block(main_loop);
    let cells_p = bcx.block_params(main_loop)[0];
    let cov_p = bcx.block_params(main_loop)[1];
    let i = bcx.block_params(main_loop)[2];
    let cover = bcx.block_params(main_loop)[3];
    let c255_vec_loop = bcx.block_params(main_loop)[4];
    let zero_vec_loop = bcx.block_params(main_loop)[5];

    // 4 セルを個別にロードする (prefix sum の逐次依存のため I32X4 一括ロードは不可)
    let c0 = bcx.ins().load(types::I32, MemFlagsData::new(), cells_p, 0);
    let c1 = bcx.ins().load(types::I32, MemFlagsData::new(), cells_p, 4);
    let c2 = bcx.ins().load(types::I32, MemFlagsData::new(), cells_p, 8);
    let c3 = bcx.ins().load(types::I32, MemFlagsData::new(), cells_p, 12);

    // セルを読み取った直後にゼロクリアする (Blend2D 方式)。
    // これによりラスタライザ側での fill(0) が不要になる。
    // zero_vec_loop はブロックパラメータのループ不変値を再利用する。
    bcx.ins()
        .store(MemFlagsData::new(), zero_vec_loop, cells_p, 0);

    // スカラー prefix sum: 逐次依存のためスカラーで計算
    let cover0 = bcx.ins().iadd(cover, c0);
    let cover1 = bcx.ins().iadd(cover0, c1);
    let cover2 = bcx.ins().iadd(cover1, c2);
    let cover3 = bcx.ins().iadd(cover2, c3);

    // 4 つの prefix sum 値を I32X4 にパックする
    let vec = bcx.ins().scalar_to_vector(types::I32X4, cover0);
    let vec = bcx.ins().insertlane(vec, cover1, 1);
    let vec = bcx.ins().insertlane(vec, cover2, 2);
    let vec = bcx.ins().insertlane(vec, cover3, 3);

    // SIMD で sshr(9) + fill_rule 変換を 4 要素並列実行
    let shifted_vec = bcx.ins().sshr_imm(vec, 9);
    let cov_vec = emit_fill_rule_convert_simd(&mut bcx, shifted_vec, c255_vec_loop, fill_rule);

    // I32X4 → 4 バイトにパック: unarrow で段階的にナロウイングする
    // (値は 0-255 なので unsigned saturating narrowing で損失なし)
    let le_flags = MemFlagsData::new().with_endianness(Endianness::Little);
    // I32X4 → I16X8 (上位半分はゼロ)
    let narrow16 = bcx.ins().unarrow(cov_vec, zero_vec_loop);
    // I16X8 → I8X16 (上位部分はゼロ)
    let zero_i16x8 = bcx.ins().bitcast(types::I16X8, le_flags, zero_vec_loop);
    let narrow8 = bcx.ins().unarrow(narrow16, zero_i16x8);
    // I8X16 の最初の 4 バイトを i32 として取得
    let packed_i32x4 = bcx.ins().bitcast(types::I32X4, le_flags, narrow8);
    let packed = bcx.ins().extractlane(packed_i32x4, 0);

    // 4 バイト一括ストア
    bcx.ins().store(MemFlagsData::new(), packed, cov_p, 0);

    // ポインタ更新: cells_p += 16 (4 * i32), cov_p += 4
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_cells_p = bcx.ins().iadd(cells_p, sixteen);
    let four_ptr = bcx.ins().iconst(ptr_type, 4);
    let next_cov_p = bcx.ins().iadd(cov_p, four_ptr);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_i = bcx.ins().iadd(i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_i, count4);
    let args_loop = block_args(&[
        next_cells_p,
        next_cov_p,
        next_i,
        cover3,
        c255_vec_loop,
        zero_vec_loop,
    ]);
    let args_check = block_args(&[next_cells_p, next_cov_p, cover3]);
    bcx.ins()
        .brif(cont, main_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.append_block_param(scalar_check, types::I32);
    bcx.switch_to_block(scalar_check);
    let cells_p = bcx.block_params(scalar_check)[0];
    let cov_p = bcx.block_params(scalar_check)[1];
    let cover = bcx.block_params(scalar_check)[2];
    let has_rem = bcx.ins().icmp(IntCC::NotEqual, rem, zero);
    let args_scalar = block_args(&[cells_p, cov_p, zero, cover]);
    bcx.ins()
        .brif(has_rem, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック (余り 1-3 ピクセル) ===
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, types::I32);
    bcx.switch_to_block(scalar_loop);
    let cells_p = bcx.block_params(scalar_loop)[0];
    let cov_p = bcx.block_params(scalar_loop)[1];
    let j = bcx.block_params(scalar_loop)[2];
    let cover = bcx.block_params(scalar_loop)[3];

    let cell_val = bcx.ins().load(types::I32, MemFlagsData::new(), cells_p, 0);
    // セルを読み取った直後にゼロクリアする
    let i32_zero_s = bcx.ins().iconst(types::I32, 0);
    bcx.ins().store(MemFlagsData::new(), i32_zero_s, cells_p, 0);
    let cover = bcx.ins().iadd(cover, cell_val);
    let shifted = bcx.ins().sshr_imm(cover, 9);
    let clamped = emit_fill_rule_convert(&mut bcx, shifted, c255, fill_rule);
    bcx.ins().istore8(MemFlagsData::new(), clamped, cov_p, 0);

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
