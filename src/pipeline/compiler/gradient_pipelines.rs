// =============================================================================
// gradient_pipelines.rs -- グラデーション用 JIT パイプライン
// =============================================================================
//
// F32X4 SIMD を使用してグラデーションの内部ループを高速化する。
// 4 ピクセル分の演算を並列実行し、LUT lookup のみスカラーで行う。

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value};
use cranelift_frontend::FunctionBuilder;

use super::{
    block_args, emit_expand_packed_coverage_i32x4, emit_extract_channels_simd,
    emit_pack_channels_simd,
};

/// LUT からスカラーインデックスで 1 ピクセルをロードする。
#[inline(always)]
fn emit_lut_lookup(
    bcx: &mut FunctionBuilder,
    lut: Value,
    idx_i32: Value,
    four: Value,
    ptr_type: Type,
) -> Value {
    let ext_idx = if ptr_type == types::I64 {
        bcx.ins().sextend(types::I64, idx_i32)
    } else {
        idx_i32
    };
    let off = bcx.ins().imul(ext_idx, four);
    let addr = bcx.ins().iadd(lut, off);
    bcx.ins().load(types::I32, MemFlags::new(), addr, 0)
}

/// Radial グラデーション行描画の SIMD パイプラインを構築する (不透明 LUT 専用)。
///
/// ## シグネチャ
///
/// ```text
/// fn radial_row(
///     dst_row: *mut u32,     // 出力行ポインタ
///     lut: *const u32,       // LUT ポインタ (256 エントリ)
///     width: usize,          // ピクセル数
///     ux_start: f32,         // 行頭ユーザー X 座標
///     uy_start: f32,         // 行頭ユーザー Y 座標
///     cx: f32,               // 中心 X
///     cy: f32,               // 中心 Y
///     r0: f32,               // 内側半径
///     inv_r_diff_max: f32,   // inv_r_diff * max_idx (事前乗算)
///     dux_dx: f32,           // X 方向のユーザー座標増分 (X 成分)
///     duy_dx: f32,           // X 方向のユーザー座標増分 (Y 成分)
/// )
/// ```
///
/// ## アルゴリズム
///
/// F32X4 で 4 ピクセル分の距離計算 (sqrt) を並列実行する。
/// LUT lookup は gather 命令非対応のためスカラーで 4 回行い、結果を I32X4 にパックする。
pub(super) fn build_radial_row_opaque(mut bcx: FunctionBuilder, ptr_type: Type) {
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);

    let dst_row = bcx.block_params(entry)[0]; // ptr
    let lut = bcx.block_params(entry)[1]; // ptr
    let width = bcx.block_params(entry)[2]; // usize
    let ux_start = bcx.block_params(entry)[3]; // f32
    let uy_start = bcx.block_params(entry)[4]; // f32
    let cx = bcx.block_params(entry)[5]; // f32
    let cy = bcx.block_params(entry)[6]; // f32
    let r0 = bcx.block_params(entry)[7]; // f32
    let inv_rdm = bcx.block_params(entry)[8]; // f32 (inv_r_diff * max_idx)
    let dux_dx = bcx.block_params(entry)[9]; // f32
    let duy_dx = bcx.block_params(entry)[10]; // f32

    // SIMD 定数の準備
    let cx_vec = bcx.ins().splat(types::F32X4, cx);
    let cy_vec = bcx.ins().splat(types::F32X4, cy);
    let r0_vec = bcx.ins().splat(types::F32X4, r0);
    let irdm_vec = bcx.ins().splat(types::F32X4, inv_rdm);
    let zero_f32 = bcx.ins().f32const(0.0);
    let zero_vec = bcx.ins().splat(types::F32X4, zero_f32);
    let max_f32 = bcx.ins().f32const(255.0);
    let max_vec = bcx.ins().splat(types::F32X4, max_f32);

    // 4 ピクセル分のオフセットベクトル [0, dux, 2*dux, 3*dux]
    let dux2 = bcx.ins().fadd(dux_dx, dux_dx);
    let dux3 = bcx.ins().fadd(dux2, dux_dx);
    let dux4_f32 = bcx.ins().fadd(dux2, dux2);
    let dux4_vec = bcx.ins().splat(types::F32X4, dux4_f32);

    let duy2 = bcx.ins().fadd(duy_dx, duy_dx);
    let duy3 = bcx.ins().fadd(duy2, duy_dx);
    let duy4_f32 = bcx.ins().fadd(duy2, duy2);
    let duy4_vec = bcx.ins().splat(types::F32X4, duy4_f32);

    // 初期 ux/uy ベクトル
    let ux1 = bcx.ins().fadd(ux_start, dux_dx);
    let ux2 = bcx.ins().fadd(ux_start, dux2);
    let ux3 = bcx.ins().fadd(ux_start, dux3);

    // F32X4 を構築: [ux_start, ux1, ux2, ux3]
    let ux_init = bcx.ins().scalar_to_vector(types::F32X4, ux_start);
    let ux_init = bcx.ins().insertlane(ux_init, ux1, 1);
    let ux_init = bcx.ins().insertlane(ux_init, ux2, 2);
    let ux_init = bcx.ins().insertlane(ux_init, ux3, 3);

    let uy1 = bcx.ins().fadd(uy_start, duy_dx);
    let uy2 = bcx.ins().fadd(uy_start, duy2);
    let uy3 = bcx.ins().fadd(uy_start, duy3);

    let uy_init = bcx.ins().scalar_to_vector(types::F32X4, uy_start);
    let uy_init = bcx.ins().insertlane(uy_init, uy1, 1);
    let uy_init = bcx.ins().insertlane(uy_init, uy2, 2);
    let uy_init = bcx.ins().insertlane(uy_init, uy3, 3);

    // ループカウント
    let simd_count = bcx.ins().ushr_imm(width, 2);
    let remainder = bcx.ins().band_imm(width, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);
    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);

    let args_simd = block_args(&[dst_row, zero, ux_init, uy_init]);
    let args_scalar = block_args(&[dst_row, ux_start, uy_start]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    bcx.append_block_param(simd_loop, ptr_type); // current_dst
    bcx.append_block_param(simd_loop, ptr_type); // simd_i
    bcx.append_block_param(simd_loop, types::F32X4); // ux_vec
    bcx.append_block_param(simd_loop, types::F32X4); // uy_vec
    bcx.switch_to_block(simd_loop);

    let current_dst = bcx.block_params(simd_loop)[0];
    let simd_i = bcx.block_params(simd_loop)[1];
    let ux_vec = bcx.block_params(simd_loop)[2];
    let uy_vec = bcx.block_params(simd_loop)[3];

    // dx = ux - cx, dy = uy - cy
    let dx_vec = bcx.ins().fsub(ux_vec, cx_vec);
    let dy_vec = bcx.ins().fsub(uy_vec, cy_vec);

    // dist_sq = dx*dx + dy*dy
    let dx_sq = bcx.ins().fmul(dx_vec, dx_vec);
    let dy_sq = bcx.ins().fmul(dy_vec, dy_vec);
    let dist_sq = bcx.ins().fadd(dx_sq, dy_sq);

    // dist = sqrt(dist_sq) -- F32X4 SIMD sqrt!
    let dist_vec = bcx.ins().sqrt(dist_sq);

    // t = (dist - r0) * inv_r_diff_max
    let t_raw = bcx.ins().fsub(dist_vec, r0_vec);
    let t_vec = bcx.ins().fmul(t_raw, irdm_vec);

    // clamp to [0, 255]
    let t_vec = bcx.ins().fmax(t_vec, zero_vec);
    let t_vec = bcx.ins().fmin(t_vec, max_vec);

    // F32X4 → I32X4 (飽和変換)
    let idx_vec = bcx.ins().fcvt_to_sint_sat(types::I32X4, t_vec);

    // 4 つのインデックスを抽出してスカラー LUT lookup
    let i0 = bcx.ins().extractlane(idx_vec, 0);
    let i1 = bcx.ins().extractlane(idx_vec, 1);
    let i2 = bcx.ins().extractlane(idx_vec, 2);
    let i3 = bcx.ins().extractlane(idx_vec, 3);

    // LUT アドレス計算: lut + idx * 4 (定数はループ内で再定義)
    let four_loop = bcx.ins().iconst(ptr_type, 4);

    let p0 = emit_lut_lookup(&mut bcx, lut, i0, four_loop, ptr_type);
    let p1 = emit_lut_lookup(&mut bcx, lut, i1, four_loop, ptr_type);
    let p2 = emit_lut_lookup(&mut bcx, lut, i2, four_loop, ptr_type);
    let p3 = emit_lut_lookup(&mut bcx, lut, i3, four_loop, ptr_type);

    // I32X4 にパックしてストア
    let result = bcx.ins().scalar_to_vector(types::I32X4, p0);
    let result = bcx.ins().insertlane(result, p1, 1);
    let result = bcx.ins().insertlane(result, p2, 2);
    let result = bcx.ins().insertlane(result, p3, 3);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);

    // ポインタとベクトルを更新
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_dst = bcx.ins().iadd(current_dst, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(simd_i, one);
    let next_ux = bcx.ins().fadd(ux_vec, dux4_vec);
    let next_uy = bcx.ins().fadd(uy_vec, duy4_vec);

    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, simd_count);

    // scalar_check に渡す ux_start: 次のスカラー開始値 = extractlane(next_ux, 0)
    let scalar_ux = bcx.ins().extractlane(next_ux, 0);
    let scalar_uy = bcx.ins().extractlane(next_uy, 0);

    let args_loop = block_args(&[next_dst, next_si, next_ux, next_uy]);
    let args_check = block_args(&[next_dst, scalar_ux, scalar_uy]);
    bcx.ins()
        .brif(cont, simd_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type); // current_dst
    bcx.append_block_param(scalar_check, types::F32); // ux
    bcx.append_block_param(scalar_check, types::F32); // uy
    bcx.switch_to_block(scalar_check);
    let current_dst = bcx.block_params(scalar_check)[0];
    let ux_scalar = bcx.block_params(scalar_check)[1];
    let uy_scalar = bcx.block_params(scalar_check)[2];

    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    let args_scalar = block_args(&[current_dst, zero, ux_scalar, uy_scalar]);
    bcx.ins()
        .brif(has_remainder, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック ===
    bcx.append_block_param(scalar_loop, ptr_type); // current_dst
    bcx.append_block_param(scalar_loop, ptr_type); // scalar_i
    bcx.append_block_param(scalar_loop, types::F32); // ux
    bcx.append_block_param(scalar_loop, types::F32); // uy
    bcx.switch_to_block(scalar_loop);

    let current_dst = bcx.block_params(scalar_loop)[0];
    let scalar_i = bcx.block_params(scalar_loop)[1];
    let ux = bcx.block_params(scalar_loop)[2];
    let uy = bcx.block_params(scalar_loop)[3];

    let dx = bcx.ins().fsub(ux, cx);
    let dy = bcx.ins().fsub(uy, cy);
    let dx_sq = bcx.ins().fmul(dx, dx);
    let dy_sq = bcx.ins().fmul(dy, dy);
    let dist_sq = bcx.ins().fadd(dx_sq, dy_sq);
    let dist = bcx.ins().sqrt(dist_sq);
    let t_raw = bcx.ins().fsub(dist, r0);
    let t = bcx.ins().fmul(t_raw, inv_rdm);
    let zero_s = bcx.ins().f32const(0.0);
    let max_s = bcx.ins().f32const(255.0);
    let t = bcx.ins().fmax(t, zero_s);
    let t = bcx.ins().fmin(t, max_s);
    let idx = bcx.ins().fcvt_to_sint_sat(types::I32, t);
    let four_s = bcx.ins().iconst(ptr_type, 4);
    let pixel = emit_lut_lookup(&mut bcx, lut, idx, four_s, ptr_type);
    bcx.ins().store(MemFlags::new(), pixel, current_dst, 0);

    let four_bytes = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(current_dst, four_bytes);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(scalar_i, one);
    let next_ux = bcx.ins().fadd(ux, dux_dx);
    let next_uy = bcx.ins().fadd(uy, duy_dx);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_si, next_ux, next_uy]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}

/// Linear グラデーション + カバレッジ融合パイプラインを構築する (Pad モード、不透明 LUT)。
///
/// ## シグネチャ
///
/// ```text
/// fn linear_gradient_cov(
///     dst: *mut u8,          // 出力ピクセルポインタ
///     lut: *const u32,       // LUT ポインタ (256 エントリ)
///     count: usize,          // ピクセル数
///     coverage: *const u8,   // カバレッジ配列
///     t_start: i64,          // 固定小数点 t (16.48 format)
///     dt_dx: i64,            // 固定小数点 dt/dx
/// )
/// ```
///
/// fetch (固定小数点 t → LUT) + coverage + SrcOver blend を 1 パスで処理し、
/// 中間バッファへのキャッシュ汚染を排除する。
pub(super) fn build_linear_gradient_cov_opaque(mut bcx: FunctionBuilder, ptr_type: Type) {
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

    let dst = bcx.block_params(entry)[0]; // ptr
    let lut = bcx.block_params(entry)[1]; // ptr
    let count = bcx.block_params(entry)[2]; // usize
    let coverage = bcx.block_params(entry)[3]; // ptr
    let t_start = bcx.block_params(entry)[4]; // i64
    let dt_dx = bcx.block_params(entry)[5]; // i64

    // ループ不変定数
    let c257_scalar = bcx.ins().iconst(types::I32, 257);
    let c257_vec = bcx.ins().splat(types::I32X4, c257_scalar);
    let c256_scalar = bcx.ins().iconst(types::I32, 256);
    let c256_vec = bcx.ins().splat(types::I32X4, c256_scalar);
    let mask_0xff = bcx.ins().iconst(types::I32, 0xFF);
    let mask_0xff_vec = bcx.ins().splat(types::I32X4, mask_0xff);
    let all_ff = bcx.ins().iconst(types::I32, -1);

    // 固定小数点定数
    let max_fixed = bcx.ins().iconst(types::I64, 255 << 16);
    let zero_i64 = bcx.ins().iconst(types::I64, 0);
    let frac_bits = bcx.ins().iconst(types::I32, 16);

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);
    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);

    let args_simd = block_args(&[dst, coverage, zero, t_start]);
    let args_scalar = block_args(&[dst, coverage, t_start]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    // 4 ピクセル: LUT lookup (スカラー) → I32X4 パック → cov チェック → blend
    bcx.append_block_param(simd_loop, ptr_type); // current_dst
    bcx.append_block_param(simd_loop, ptr_type); // current_cov
    bcx.append_block_param(simd_loop, ptr_type); // simd_i
    bcx.append_block_param(simd_loop, types::I64); // t
    bcx.switch_to_block(simd_loop);

    let current_dst = bcx.block_params(simd_loop)[0];
    let current_cov = bcx.block_params(simd_loop)[1];
    let simd_i = bcx.block_params(simd_loop)[2];
    let t = bcx.block_params(simd_loop)[3];

    // 4 ピクセル分の LUT lookup (固定小数点 → インデックス → スカラーロード)
    let four_lut = bcx.ins().iconst(ptr_type, 4);
    let t1 = bcx.ins().iadd(t, dt_dx);
    let t2 = bcx.ins().iadd(t1, dt_dx);
    let t3 = bcx.ins().iadd(t2, dt_dx);

    let idx0 = emit_fixed_to_index(&mut bcx, t, zero_i64, max_fixed, frac_bits);
    let idx1 = emit_fixed_to_index(&mut bcx, t1, zero_i64, max_fixed, frac_bits);
    let idx2 = emit_fixed_to_index(&mut bcx, t2, zero_i64, max_fixed, frac_bits);
    let idx3 = emit_fixed_to_index(&mut bcx, t3, zero_i64, max_fixed, frac_bits);

    let p0 = emit_lut_lookup(&mut bcx, lut, idx0, four_lut, ptr_type);
    let p1 = emit_lut_lookup(&mut bcx, lut, idx1, four_lut, ptr_type);
    let p2 = emit_lut_lookup(&mut bcx, lut, idx2, four_lut, ptr_type);
    let p3 = emit_lut_lookup(&mut bcx, lut, idx3, four_lut, ptr_type);

    // I32X4 にパック
    let src_pixels = bcx.ins().scalar_to_vector(types::I32X4, p0);
    let src_pixels = bcx.ins().insertlane(src_pixels, p1, 1);
    let src_pixels = bcx.ins().insertlane(src_pixels, p2, 2);
    let src_pixels = bcx.ins().insertlane(src_pixels, p3, 3);

    // カバレッジ判定: 全 0xFF なら高速パス
    let packed_cov = bcx.ins().load(types::I32, MemFlags::new(), current_cov, 0);
    let is_all_ff = bcx.ins().icmp(IntCC::Equal, packed_cov, all_ff);
    bcx.ins().brif(is_all_ff, simd_fast, &[], simd_slow, &[]);

    // === simd_fast ブロック (cov=0xFF) ===
    // 不透明 LUT + 全カバレッジ → SrcOver 合成
    bcx.switch_to_block(simd_fast);
    let (src_a, src_r, src_g, src_b) =
        emit_extract_channels_simd(&mut bcx, src_pixels, mask_0xff_vec);
    let inv_alpha = bcx.ins().isub(c256_vec, src_a);

    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let (dst_a, dst_r, dst_g, dst_b) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    let tmp = bcx.ins().imul(dst_a, inv_alpha);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let oa = bcx.ins().iadd(src_a, tmp);
    let tmp = bcx.ins().imul(dst_r, inv_alpha);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let or = bcx.ins().iadd(src_r, tmp);
    let tmp = bcx.ins().imul(dst_g, inv_alpha);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let og = bcx.ins().iadd(src_g, tmp);
    let tmp = bcx.ins().imul(dst_b, inv_alpha);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let ob = bcx.ins().iadd(src_b, tmp);

    let result = emit_pack_channels_simd(&mut bcx, oa, or, og, ob);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_slow ブロック (通常カバレッジ) ===
    bcx.switch_to_block(simd_slow);
    let cov_vec = emit_expand_packed_coverage_i32x4(&mut bcx, packed_cov);
    let (src_a, src_r, src_g, src_b) =
        emit_extract_channels_simd(&mut bcx, src_pixels, mask_0xff_vec);

    // cov_src_c = div255(src_c * cov)
    let cov_src_a = emit_div255_simd(&mut bcx, src_a, cov_vec, c257_vec);
    let cov_src_r = emit_div255_simd(&mut bcx, src_r, cov_vec, c257_vec);
    let cov_src_g = emit_div255_simd(&mut bcx, src_g, cov_vec, c257_vec);
    let cov_src_b = emit_div255_simd(&mut bcx, src_b, cov_vec, c257_vec);

    let inv_alpha_v = bcx.ins().isub(c256_vec, cov_src_a);
    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let (dst_a, dst_r, dst_g, dst_b) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    let tmp = bcx.ins().imul(dst_a, inv_alpha_v);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let oa = bcx.ins().iadd(cov_src_a, tmp);
    let tmp = bcx.ins().imul(dst_r, inv_alpha_v);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let or = bcx.ins().iadd(cov_src_r, tmp);
    let tmp = bcx.ins().imul(dst_g, inv_alpha_v);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let og = bcx.ins().iadd(cov_src_g, tmp);
    let tmp = bcx.ins().imul(dst_b, inv_alpha_v);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let ob = bcx.ins().iadd(cov_src_b, tmp);

    let result = emit_pack_channels_simd(&mut bcx, oa, or, og, ob);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_next ブロック ===
    bcx.switch_to_block(simd_next);
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_dst = bcx.ins().iadd(current_dst, sixteen);
    let four_ptr = bcx.ins().iconst(ptr_type, 4);
    let next_cov = bcx.ins().iadd(current_cov, four_ptr);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(simd_i, one);
    let next_t = bcx.ins().iadd(t3, dt_dx); // t + 4*dt

    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, simd_count);
    let args_loop = block_args(&[next_dst, next_cov, next_si, next_t]);
    let args_check = block_args(&[next_dst, next_cov, next_t]);
    bcx.ins()
        .brif(cont, simd_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type); // current_dst
    bcx.append_block_param(scalar_check, ptr_type); // current_cov
    bcx.append_block_param(scalar_check, types::I64); // t
    bcx.switch_to_block(scalar_check);
    let current_dst = bcx.block_params(scalar_check)[0];
    let current_cov = bcx.block_params(scalar_check)[1];
    let t = bcx.block_params(scalar_check)[2];

    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    let args_scalar = block_args(&[current_dst, current_cov, zero, t]);
    bcx.ins()
        .brif(has_remainder, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック ===
    bcx.append_block_param(scalar_loop, ptr_type); // current_dst
    bcx.append_block_param(scalar_loop, ptr_type); // current_cov
    bcx.append_block_param(scalar_loop, ptr_type); // scalar_i
    bcx.append_block_param(scalar_loop, types::I64); // t
    bcx.switch_to_block(scalar_loop);
    let current_dst = bcx.block_params(scalar_loop)[0];
    let current_cov = bcx.block_params(scalar_loop)[1];
    let scalar_i = bcx.block_params(scalar_loop)[2];
    let t = bcx.block_params(scalar_loop)[3];

    // 固定小数点 → LUT
    let four_s = bcx.ins().iconst(ptr_type, 4);
    let idx = emit_fixed_to_index(&mut bcx, t, zero_i64, max_fixed, frac_bits);
    let src = emit_lut_lookup(&mut bcx, lut, idx, four_s, ptr_type);

    // カバレッジ
    let cov_u8 = bcx.ins().load(types::I8, MemFlags::new(), current_cov, 0);
    let cov = bcx.ins().uextend(types::I32, cov_u8);

    // src チャネル分解
    let tmp = bcx.ins().ushr_imm(src, 24);
    let src_a = bcx.ins().band_imm(tmp, 0xFF);
    let tmp = bcx.ins().ushr_imm(src, 16);
    let src_r = bcx.ins().band_imm(tmp, 0xFF);
    let tmp = bcx.ins().ushr_imm(src, 8);
    let src_g = bcx.ins().band_imm(tmp, 0xFF);
    let src_b = bcx.ins().band_imm(src, 0xFF);

    // cov_src = div255(src * cov)
    let csa = emit_div255_scalar(&mut bcx, src_a, cov, c257_scalar);
    let csr = emit_div255_scalar(&mut bcx, src_r, cov, c257_scalar);
    let csg = emit_div255_scalar(&mut bcx, src_g, cov, c257_scalar);
    let csb = emit_div255_scalar(&mut bcx, src_b, cov, c257_scalar);

    // SrcOver blend
    let inv_a = bcx.ins().isub(c256_scalar, csa);
    let dst_pixel = bcx.ins().load(types::I32, MemFlags::new(), current_dst, 0);
    let tmp = bcx.ins().ushr_imm(dst_pixel, 24);
    let da = bcx.ins().band_imm(tmp, 0xFF);
    let tmp = bcx.ins().ushr_imm(dst_pixel, 16);
    let dr = bcx.ins().band_imm(tmp, 0xFF);
    let tmp = bcx.ins().ushr_imm(dst_pixel, 8);
    let dg = bcx.ins().band_imm(tmp, 0xFF);
    let db = bcx.ins().band_imm(dst_pixel, 0xFF);

    let tmp = bcx.ins().imul(da, inv_a);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let oa = bcx.ins().iadd(csa, tmp);
    let tmp = bcx.ins().imul(dr, inv_a);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let or = bcx.ins().iadd(csr, tmp);
    let tmp = bcx.ins().imul(dg, inv_a);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let og = bcx.ins().iadd(csg, tmp);
    let tmp = bcx.ins().imul(db, inv_a);
    let tmp = bcx.ins().ushr_imm(tmp, 8);
    let ob = bcx.ins().iadd(csb, tmp);

    let result = bcx.ins().ishl_imm(oa, 24);
    let tmp = bcx.ins().ishl_imm(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);

    // ポインタ更新
    let four_bytes = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(current_dst, four_bytes);
    let one_ptr = bcx.ins().iconst(ptr_type, 1);
    let next_cov = bcx.ins().iadd(current_cov, one_ptr);
    let next_si = bcx.ins().iadd(scalar_i, one_ptr);
    let next_t = bcx.ins().iadd(t, dt_dx);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_cov, next_si, next_t]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}

/// 固定小数点 t → クランプ済み LUT インデックス (I32)。
#[inline(always)]
fn emit_fixed_to_index(
    bcx: &mut FunctionBuilder,
    t: Value,
    zero: Value,
    max_fixed: Value,
    frac_bits: Value,
) -> Value {
    // Pad: clamp(t, 0, max_fixed) >> FRAC_BITS
    let clamped = bcx.ins().smax(t, zero);
    let clamped = bcx.ins().smin(clamped, max_fixed);
    let shifted = bcx.ins().sshr(clamped, frac_bits);
    bcx.ins().ireduce(types::I32, shifted)
}

/// div255 の SIMD 版: (src * cov * 257 + 257) >> 16
#[inline(always)]
fn emit_div255_simd(bcx: &mut FunctionBuilder, src: Value, cov: Value, c257: Value) -> Value {
    let tmp = bcx.ins().imul(src, cov);
    let tmp = bcx.ins().imul(tmp, c257);
    let tmp = bcx.ins().iadd(tmp, c257);
    bcx.ins().ushr_imm(tmp, 16)
}

/// div255 のスカラー版: (src * cov * 257 + 257) >> 16
#[inline(always)]
fn emit_div255_scalar(bcx: &mut FunctionBuilder, src: Value, cov: Value, c257: Value) -> Value {
    let tmp = bcx.ins().imul(src, cov);
    let tmp = bcx.ins().imul(tmp, c257);
    let tmp = bcx.ins().iadd(tmp, c257);
    bcx.ins().ushr_imm(tmp, 16)
}
