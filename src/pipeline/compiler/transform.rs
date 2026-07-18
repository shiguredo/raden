use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Type};
use cranelift_frontend::FunctionBuilder;

use super::block_args;

// =============================================================================
// エッジ座標変換 (F64X2 SIMD)
// =============================================================================
//
// 各エッジ (x0, y0, x1, y1) を 2D アフィン変換行列で変換する。
// F64X2 SIMD で 1 点 (x, y) を 1 ベクタとして処理し、2 点/エッジ = 2 ベクタ/エッジ。
//
// アルゴリズム:
//   col_a = [m00, m01]  (ループ不変)
//   col_c = [m10, m11]  (ループ不変)
//   trans = [m20, m21]  (ループ不変)
//
//   result = x_splat * col_a + y_splat * col_c + trans

pub(super) fn build_transform_edges(mut bcx: FunctionBuilder, ptr_type: Type) {
    let entry = bcx.create_block();
    let loop_body = bcx.create_block();
    let exit = bcx.create_block();

    // -- entry ブロック --
    bcx.append_block_params_for_function_params(entry);
    bcx.switch_to_block(entry);
    bcx.seal_block(entry);

    let edges_ptr = bcx.block_params(entry)[0]; // *mut f64
    let count = bcx.block_params(entry)[1]; // usize
    let m00 = bcx.block_params(entry)[2]; // f64
    let m01 = bcx.block_params(entry)[3];
    let m10 = bcx.block_params(entry)[4];
    let m11 = bcx.block_params(entry)[5];
    let m20 = bcx.block_params(entry)[6];
    let m21 = bcx.block_params(entry)[7];

    // ループ不変定数を F64X2 で構築
    // col_a = [m00, m01]
    let col_a = bcx.ins().splat(types::F64X2, m00);
    let col_a = bcx.ins().insertlane(col_a, m01, 1);
    // col_c = [m10, m11]
    let col_c = bcx.ins().splat(types::F64X2, m10);
    let col_c = bcx.ins().insertlane(col_c, m11, 1);
    // trans = [m20, m21]
    let trans = bcx.ins().splat(types::F64X2, m20);
    let trans = bcx.ins().insertlane(trans, m21, 1);

    // count == 0 なら即座に return
    let zero = bcx.ins().iconst(ptr_type, 0);
    let is_zero = bcx.ins().icmp(IntCC::Equal, count, zero);
    bcx.ins().brif(
        is_zero,
        exit,
        &[],
        loop_body,
        &block_args(&[edges_ptr, count]),
    );

    // -- loop_body ブロック --
    // ループ変数: ptr (現在のエッジポインタ), remaining (残りエッジ数)
    bcx.append_block_param(loop_body, ptr_type); // ptr
    bcx.append_block_param(loop_body, ptr_type); // remaining
    bcx.switch_to_block(loop_body);

    let ptr = bcx.block_params(loop_body)[0];
    let remaining = bcx.block_params(loop_body)[1];

    let mem = MemFlagsData::trusted();

    // 点 (x0, y0) をロード: load F64X2 from ptr+0
    let p0 = bcx.ins().load(types::F64X2, mem, ptr, 0);
    // x0 を抽出して splat
    let x0 = bcx.ins().extractlane(p0, 0);
    let x0_splat = bcx.ins().splat(types::F64X2, x0);
    // y0 を抽出して splat
    let y0 = bcx.ins().extractlane(p0, 1);
    let y0_splat = bcx.ins().splat(types::F64X2, y0);
    // result0 = x0 * col_a + y0 * col_c + trans
    let r0 = bcx.ins().fmul(x0_splat, col_a);
    let r0 = bcx.ins().fma(y0_splat, col_c, r0);
    let r0 = bcx.ins().fadd(r0, trans);

    // 点 (x1, y1) をロード: load F64X2 from ptr+16
    let p1 = bcx.ins().load(types::F64X2, mem, ptr, 16);
    let x1 = bcx.ins().extractlane(p1, 0);
    let x1_splat = bcx.ins().splat(types::F64X2, x1);
    let y1 = bcx.ins().extractlane(p1, 1);
    let y1_splat = bcx.ins().splat(types::F64X2, y1);
    let r1 = bcx.ins().fmul(x1_splat, col_a);
    let r1 = bcx.ins().fma(y1_splat, col_c, r1);
    let r1 = bcx.ins().fadd(r1, trans);

    // 結果を書き戻し
    bcx.ins().store(mem, r0, ptr, 0);
    bcx.ins().store(mem, r1, ptr, 16);

    // ptr += 32 (次のエッジ), remaining -= 1
    let stride = bcx.ins().iconst(ptr_type, 32);
    let next_ptr = bcx.ins().iadd(ptr, stride);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_remaining = bcx.ins().isub(remaining, one);

    let done = bcx.ins().icmp(IntCC::Equal, next_remaining, zero);
    bcx.ins().brif(
        done,
        exit,
        &[],
        loop_body,
        &block_args(&[next_ptr, next_remaining]),
    );

    // -- exit ブロック --
    bcx.switch_to_block(exit);
    bcx.seal_block(exit);
    bcx.seal_block(loop_body);
    bcx.ins().return_(&[]);

    bcx.finalize();
}
