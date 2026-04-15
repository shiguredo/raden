use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{InstBuilder, MemFlags, Type, Value};
use cranelift_frontend::FunctionBuilder;

use super::{
    block_args, emit_expand_packed_coverage_i32x4, emit_extract_channels_simd,
    emit_pack_channels_simd,
};

// =============================================================================
// パイプライン構築関数
// =============================================================================
//
// 全ての build_xxx 関数は同一のループ構造を持つ:
//
// ```
// entry:
//   ループ不変値の事前計算 (チャネル抽出、splat ベクタ)
//   simd_count = count / 4    (ushr 2)
//   remainder  = count % 4    (band 3)
//   simd_count > 0 ? → simd_loop : scalar_check
//
// simd_loop(current_dst, [current_cov,] simd_i):
//   4 ピクセルを I32X4 で並列処理
//   dst += 16, [cov += 4]
//   simd_i+1 < simd_count ? → simd_loop : scalar_check
//
// scalar_check(current_dst, [current_cov]):
//   remainder > 0 ? → scalar_loop : exit
//
// scalar_loop(current_dst, [current_cov,] scalar_i):
//   1 ピクセルをスカラ処理
//   dst += 4, [cov += 1]
//   scalar_i+1 < remainder ? → scalar_loop : exit
//
// exit: return
// ```
//
// Cranelift は SSA 形式のため、ループ変数 (current_dst, simd_i 等) は
// ブロックパラメータとして渡される。phi ノードの代わりにブロック引数を使う。

/// Porter-Duff SrcCopy パイプラインを構築する。
///
/// ## 合成式
///
/// ```text
/// out = src  (dst を完全に上書き)
/// ```
///
/// 最もシンプルなパイプライン。SIMD ループでは `splat` した src_solid を
/// 128-bit ストアで 4 ピクセル一括書き込みする。
///
/// ## 使用場面
///
/// - fill_rect() で不透明色の矩形を描画する場合
/// - 背景色の塗りつぶし (animation.rs の SrcCopy 背景描画)
///
/// ## 性能特性
///
/// SIMD ループは 1 命令/反復 (128-bit store) のため、メモリ帯域が律速。
/// 1280x720 の全画面塗りつぶしで ~0.1ms (DDR4-3200 の帯域上限に近い)。
pub(super) fn build_src_copy(mut bcx: FunctionBuilder, ptr_type: Type) {
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    // 関数引数を受け取り、ループ不変値を事前計算する。
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0]; // *mut u8: 書き込み先
    let src_solid = bcx.block_params(entry)[1]; // u32: PRGB32 ソース色
    let count = bcx.block_params(entry)[2]; // usize: ピクセル数

    // count を 4 で割って SIMD 反復回数と余りを計算する。
    // ushr 2 は count / 4、band 3 は count % 4 と等価。
    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    // src_solid を I32X4 の全 4 レーンにブロードキャスト。
    // SIMD ループで 128-bit ストアするためのループ不変ベクタ。
    let src_vec = bcx.ins().splat(types::I32X4, src_solid);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    let args_simd = block_args(&[dst, zero]);
    let args_scalar = block_args(&[dst]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    // 4 ピクセル (16 バイト) を一括ストアする。
    // ブロックパラメータ: (current_dst: ptr, simd_i: ptr)
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let current_dst = bcx.block_params(simd_loop)[0];
    let simd_i = bcx.block_params(simd_loop)[1];

    // 128-bit ストア: 4 ピクセルを一括書き込み
    bcx.ins().store(MemFlags::new(), src_vec, current_dst, 0);

    // ポインタを 16 バイト (4 ピクセル) 進める
    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_dst = bcx.ins().iadd(current_dst, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(simd_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, simd_count);
    let args_loop = block_args(&[next_dst, next_si]);
    let args_check = block_args(&[next_dst]);
    bcx.ins()
        .brif(cont, simd_loop, &args_loop, scalar_check, &args_check);

    // === scalar_check ブロック ===
    // 余りピクセル (0-3) の有無を判定する。
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let current_dst = bcx.block_params(scalar_check)[0];
    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    let args_scalar = block_args(&[current_dst, zero]);
    bcx.ins()
        .brif(has_remainder, scalar_loop, &args_scalar, exit, &[]);

    // === scalar_loop ブロック ===
    // 1 ピクセル (4 バイト) ずつストアする。
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let current_dst = bcx.block_params(scalar_loop)[0];
    let scalar_i = bcx.block_params(scalar_loop)[1];

    // 32-bit ストア: 1 ピクセル書き込み
    bcx.ins().store(MemFlags::new(), src_solid, current_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(current_dst, four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(scalar_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_si]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}

/// Porter-Duff SrcCopy + カバレッジパイプラインを構築する。
///
/// ## 合成式
///
/// ```text
/// out_c = div255(src_c * cov)
///       = (src_c * cov * 257 + 257) >> 16
/// ```
///
/// ここで div255 は Blend2D 方式の高速近似除算。
/// `x / 255` の正確な整数除算は `(x * 257 + 257) >> 16` で近似できる。
/// これは 0 <= x <= 255*255 = 65025 の範囲で正確。
///
/// ## 例: cov=128, src_r=200 の場合
///
/// ```text
/// exact:  200 * 128 / 255 = 100.39... → 100
/// approx: (200 * 128 * 257 + 257) >> 16
///        = (6579200 + 257) >> 16
///        = 6579457 >> 16
///        = 100  ✓
/// ```
///
/// ## SIMD ループの命令数
///
/// cov=0xFF 高速パス (図形内部の大部分):
///   cov ロード + 比較 + 分岐 + 128-bit ストア = 約 5 命令/4 ピクセル
///
/// 通常パス (アンチエイリアス境界):
///   cov ロード + 比較 + 分岐 + 展開 (4) + div255 × 4ch (16) + パック (6) + ストア
///   = 約 31 命令/4 ピクセル
///
/// 円や矩形の内部ピクセルは cov=0xFF が大半を占めるため、
/// 高速パスにより平均命令数が大幅に削減される。
pub(super) fn build_src_copy_cov(mut bcx: FunctionBuilder, ptr_type: Type) {
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

    // ソース色を ARGB チャネルに分解する (ループ不変)。
    // PRGB32 形式: 0xAARRGGBB (premultiplied alpha)
    let src_a = bcx.ins().ushr_imm(src_solid, 24);
    let src_a = bcx.ins().band_imm(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm(src_solid, 16);
    let src_r = bcx.ins().band_imm(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm(src_solid, 8);
    let src_g = bcx.ins().band_imm(src_g, 0xFF);
    let src_b = bcx.ins().band_imm(src_solid, 0xFF);

    // 各チャネルを I32X4 にブロードキャストする (SIMD ループ用の不変ベクタ)。
    // splat 命令は x86_64 では vpbroadcastd に対応する。
    let src_a_vec = bcx.ins().splat(types::I32X4, src_a);
    let src_r_vec = bcx.ins().splat(types::I32X4, src_r);
    let src_g_vec = bcx.ins().splat(types::I32X4, src_g);
    let src_b_vec = bcx.ins().splat(types::I32X4, src_b);
    // div255 近似に使う定数 257。src * cov * 257 + 257 で /255 を実現。
    let c257_scalar = bcx.ins().iconst(types::I32, 257);
    let c257_vec = bcx.ins().splat(types::I32X4, c257_scalar);
    // cov=0xFF 高速パス用: src_solid を I32X4 にブロードキャスト。
    // cov=0xFF のとき div255(src * 255) = src なので、src をそのままストアできる。
    let src_vec = bcx.ins().splat(types::I32X4, src_solid);
    // 全カバレッジバイトが 0xFF かの判定用。4 バイト = 0xFFFFFFFF = -1 (i32)。
    let all_ff = bcx.ins().iconst(types::I32, -1);

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    let args_simd = block_args(&[dst, coverage, zero]);
    let args_scalar = block_args(&[dst, coverage]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    // 4 ピクセル分のカバレッジをロードし、全て 0xFF なら高速パスに分岐する。
    //
    // cov=0xFF 高速パスの根拠:
    //   div255(src_c * 255) = (src_c * 255 * 257 + 257) >> 16 = src_c
    //   (証明: src_c * 65535 + 257 を 65536 で割ると src_c + 余り)
    //   よって cov=0xFF のとき、計算結果は src_solid そのものになる。
    //   16 命令の div255 計算を 1 命令の 128-bit ストアに置換できる。
    //
    // ブロックパラメータ: (current_dst, current_cov, simd_i)
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let current_dst = bcx.block_params(simd_loop)[0];
    let current_cov = bcx.block_params(simd_loop)[1];
    let simd_i = bcx.block_params(simd_loop)[2];

    // 4 バイトのカバレッジを 1 つの i32 としてロード。
    // 全バイトが 0xFF なら 0xFFFFFFFF = -1 (i32) になる。
    let packed_cov = bcx.ins().load(types::I32, MemFlags::new(), current_cov, 0);
    let is_all_ff = bcx.ins().icmp(IntCC::Equal, packed_cov, all_ff);
    bcx.ins().brif(is_all_ff, simd_fast, &[], simd_slow, &[]);

    // === simd_fast ブロック (cov=0xFF 高速パス) ===
    // cov=0xFF: src_solid を 128-bit ストアするだけ (1 命令)。
    bcx.switch_to_block(simd_fast);
    bcx.ins().store(MemFlags::new(), src_vec, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_slow ブロック (通常カバレッジ計算) ===
    // packed_cov を I32X4 に展開し、div255(src_c * cov) を計算する。
    bcx.switch_to_block(simd_slow);
    let cov_vec = emit_expand_packed_coverage_i32x4(&mut bcx, packed_cov);

    // 各チャネルに対して div255(src_c * cov) を計算する。
    // out_c = (src_c * cov * 257 + 257) >> 16
    //
    // 乗算の順序は src_c * cov を先に計算し、その結果に 257 を掛ける。
    // src_c (0-255) * cov (0-255) = 0-65025 で I32 に収まる。
    // (0-65025) * 257 = 0-16_711_425 で I32 に収まる。
    // + 257 → 0-16_711_682 で I32 に収まる。
    // >> 16 → 0-255。
    let ca = bcx.ins().imul(src_a_vec, cov_vec);
    let ca = bcx.ins().imul(ca, c257_vec);
    let ca = bcx.ins().iadd(ca, c257_vec);
    let out_a = bcx.ins().ushr_imm(ca, 16);

    let cr = bcx.ins().imul(src_r_vec, cov_vec);
    let cr = bcx.ins().imul(cr, c257_vec);
    let cr = bcx.ins().iadd(cr, c257_vec);
    let out_r = bcx.ins().ushr_imm(cr, 16);

    let cg = bcx.ins().imul(src_g_vec, cov_vec);
    let cg = bcx.ins().imul(cg, c257_vec);
    let cg = bcx.ins().iadd(cg, c257_vec);
    let out_g = bcx.ins().ushr_imm(cg, 16);

    let cb = bcx.ins().imul(src_b_vec, cov_vec);
    let cb = bcx.ins().imul(cb, c257_vec);
    let cb = bcx.ins().iadd(cb, c257_vec);
    let out_b = bcx.ins().ushr_imm(cb, 16);

    let result = emit_pack_channels_simd(&mut bcx, out_a, out_r, out_g, out_b);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_next ブロック (高速・通常パス合流) ===
    // ポインタ更新: dst += 16 (4 ピクセル), cov += 4 (4 バイト)
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
    // 1 ピクセルのスカラ処理。SIMD ループと同一の合成式。
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let current_dst = bcx.block_params(scalar_loop)[0];
    let current_cov = bcx.block_params(scalar_loop)[1];
    let scalar_i = bcx.block_params(scalar_loop)[2];

    // 1 バイトのカバレッジをロードして I32 にゼロ拡張
    let cov_u8 = bcx.ins().load(types::I8, MemFlags::new(), current_cov, 0);
    let cov = bcx.ins().uextend(types::I32, cov_u8);

    // out_c = (src_c * cov * 257 + 257) >> 16 (スカラ版)
    let ca = bcx.ins().imul(src_a, cov);
    let ca = bcx.ins().imul(ca, c257_scalar);
    let ca = bcx.ins().iadd(ca, c257_scalar);
    let out_a = bcx.ins().ushr_imm(ca, 16);

    let cr = bcx.ins().imul(src_r, cov);
    let cr = bcx.ins().imul(cr, c257_scalar);
    let cr = bcx.ins().iadd(cr, c257_scalar);
    let out_r = bcx.ins().ushr_imm(cr, 16);

    let cg = bcx.ins().imul(src_g, cov);
    let cg = bcx.ins().imul(cg, c257_scalar);
    let cg = bcx.ins().iadd(cg, c257_scalar);
    let out_g = bcx.ins().ushr_imm(cg, 16);

    let cb = bcx.ins().imul(src_b, cov);
    let cb = bcx.ins().imul(cb, c257_scalar);
    let cb = bcx.ins().iadd(cb, c257_scalar);
    let out_b = bcx.ins().ushr_imm(cb, 16);

    // スカラ版パック: シフト + OR で ARGB32 に結合
    let result = bcx.ins().ishl_imm(out_a, 24);
    let tmp = bcx.ins().ishl_imm(out_r, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(out_g, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, out_b);

    bcx.ins().store(MemFlags::new(), result, current_dst, 0);

    // ポインタ更新: dst += 4, cov += 1
    let four = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(current_dst, four);
    let one_ptr = bcx.ins().iconst(ptr_type, 1);
    let next_cov = bcx.ins().iadd(current_cov, one_ptr);
    let next_si = bcx.ins().iadd(scalar_i, one_ptr);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_cov, next_si]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}

/// Porter-Duff SrcOver + カバレッジパイプラインを構築する。
///
/// ## 合成式
///
/// これは最も複雑なパイプラインで、カバレッジとアルファブレンドの両方を処理する。
///
/// ```text
/// ステップ 1: カバレッジ適用 (src にカバレッジを乗算)
///   cov_src_c = div255(src_c * cov) = (src_c * cov * 257 + 257) >> 16
///
/// ステップ 2: 逆アルファ計算
///   inv_alpha = 256 - cov_src_a
///
///   Blend2D 方式: 256 - src_a (255 でなく 256) を使うことで、
///   >> 8 シフトだけで /256 除算ができる。誤差は最大 1/256 ≈ 0.4% で
///   視覚的に区別不可能。/255 の正確な除算は乗算が必要で遅い。
///
/// ステップ 3: SrcOver 合成
///   out_c = cov_src_c + (dst_c * inv_alpha) >> 8
///
///   >> 8 は /256 の近似。正確な /255 は (x * 257) >> 16 だが、
///   >> 8 の方が高速で、結果の誤差は最大 1 (0.4%)。
/// ```
///
/// ## 合成の導出
///
/// Porter-Duff SrcOver の定義: out = src * cov + dst * (1 - src_a * cov)
///
/// premultiplied alpha では src_c にはすでに alpha が乗算されているため:
/// - cov_src = src * cov / 255 (カバレッジを適用した src)
/// - inv_alpha = 1 - cov_src_a / 255 ≈ (256 - cov_src_a) / 256
/// - out = cov_src + dst * inv_alpha
///
/// ## SIMD ループの命令数
///
/// ```text
/// cov=0xFF 高速パス (図形内部の大部分):
///   cov ロード + 比較 + 分岐 + dst ロード + チャネル抽出 (8) + SrcOver (12)
///   + パック (6) + ストア = 約 32 命令/4 ピクセル = 約 8 命令/ピクセル
///   (カバレッジ展開 5 + div255 16 + 動的 inv_alpha 1 = 22 命令を省略)
///
/// 通常パス (アンチエイリアス境界):
///   cov ロード + 比較 + 分岐 + 展開 (4) + div255 × 4ch (16) + inv_alpha (1)
///   + dst ロード + チャネル抽出 (8) + SrcOver × 4ch (12) + パック (6) + ストア
///   = 約 53 命令/4 ピクセル = 約 13 命令/ピクセル
/// ```
pub(super) fn build_src_over_cov(mut bcx: FunctionBuilder, ptr_type: Type) {
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
    let src_a = bcx.ins().ushr_imm(src_solid, 24);
    let src_a = bcx.ins().band_imm(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm(src_solid, 16);
    let src_r = bcx.ins().band_imm(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm(src_solid, 8);
    let src_g = bcx.ins().band_imm(src_g, 0xFF);
    let src_b = bcx.ins().band_imm(src_solid, 0xFF);

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
    // cov=0xFF 高速パス用: ソース色の inv_alpha をループ不変値として事前計算。
    // cov=0xFF のとき cov_src_a = src_a なので inv_alpha = 256 - src_a。
    // これは build_src_over のループ不変 inv_alpha と同一。
    let inv_alpha_src = bcx.ins().isub(c256_scalar, src_a);
    let inv_alpha_src_vec = bcx.ins().splat(types::I32X4, inv_alpha_src);
    // 全カバレッジバイトが 0xFF かの判定用
    let all_ff = bcx.ins().iconst(types::I32, -1);

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_simd = bcx.ins().icmp(IntCC::NotEqual, simd_count, zero);
    let args_simd = block_args(&[dst, coverage, zero]);
    let args_scalar = block_args(&[dst, coverage]);
    bcx.ins()
        .brif(has_simd, simd_loop, &args_simd, scalar_check, &args_scalar);

    // === simd_loop ブロック ===
    // 4 ピクセル分のカバレッジをロードし、全て 0xFF なら高速パスに分岐する。
    //
    // cov=0xFF 高速パスの根拠:
    //   cov_src_c = div255(src_c * 255) = src_c
    //   inv_alpha = 256 - src_a (ループ不変)
    //   out_c = src_c + (dst_c * (256 - src_a)) >> 8
    //   これは build_src_over の SIMD ループ本体と同一。
    //   カバレッジ展開 (5 命令) + div255 (16 命令) + 動的 inv_alpha (1 命令)
    //   = 22 命令を省略し、ループ不変の inv_alpha_src_vec で直接合成できる。
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let current_dst = bcx.block_params(simd_loop)[0];
    let current_cov = bcx.block_params(simd_loop)[1];
    let simd_i = bcx.block_params(simd_loop)[2];

    // 4 バイトのカバレッジを 1 つの i32 としてロード。
    // 全バイトが 0xFF なら 0xFFFFFFFF = -1 (i32) になる。
    let packed_cov = bcx.ins().load(types::I32, MemFlags::new(), current_cov, 0);
    let is_all_ff = bcx.ins().icmp(IntCC::Equal, packed_cov, all_ff);
    bcx.ins().brif(is_all_ff, simd_fast, &[], simd_slow, &[]);

    // === simd_fast ブロック (cov=0xFF 高速パス) ===
    // cov_src_c = src_c、inv_alpha = 256 - src_a (ループ不変値を使用)。
    // build_src_over の SIMD ループと同一の合成式。
    bcx.switch_to_block(simd_fast);
    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    // out_c = src_c + (dst_c * inv_alpha_src) >> 8
    let da = bcx.ins().imul(dst_a_v, inv_alpha_src_vec);
    let da = bcx.ins().ushr_imm(da, 8);
    let out_a = bcx.ins().iadd(src_a_vec, da);

    let dr = bcx.ins().imul(dst_r_v, inv_alpha_src_vec);
    let dr = bcx.ins().ushr_imm(dr, 8);
    let out_r = bcx.ins().iadd(src_r_vec, dr);

    let dg = bcx.ins().imul(dst_g_v, inv_alpha_src_vec);
    let dg = bcx.ins().ushr_imm(dg, 8);
    let out_g = bcx.ins().iadd(src_g_vec, dg);

    let db = bcx.ins().imul(dst_b_v, inv_alpha_src_vec);
    let db = bcx.ins().ushr_imm(db, 8);
    let out_b = bcx.ins().iadd(src_b_vec, db);

    let result = emit_pack_channels_simd(&mut bcx, out_a, out_r, out_g, out_b);
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_slow ブロック (通常カバレッジ + SrcOver 合成) ===
    bcx.switch_to_block(simd_slow);
    let cov_vec = emit_expand_packed_coverage_i32x4(&mut bcx, packed_cov);

    // --- ステップ 1: cov_src_c = div255(src_c * cov) ---
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

    // --- ステップ 2: inv_alpha = 256 - cov_src_a ---
    let inv_alpha_v = bcx.ins().isub(c256_vec, cov_src_a);

    // --- ステップ 3: dst ロード + SrcOver 合成 ---
    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    // out_c = cov_src_c + (dst_c * inv_alpha) >> 8
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
    // 1 ピクセルのスカラ合成。SIMD ループと同一の 3 ステップを実行する。
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let current_dst = bcx.block_params(scalar_loop)[0];
    let current_cov = bcx.block_params(scalar_loop)[1];
    let scalar_i = bcx.block_params(scalar_loop)[2];

    // カバレッジ 1 バイトロード
    let cov_u8 = bcx.ins().load(types::I8, MemFlags::new(), current_cov, 0);
    let cov = bcx.ins().uextend(types::I32, cov_u8);

    // ステップ 1: cov_src_c = div255(src_c * cov) (スカラ版)
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

    // ステップ 2: inv_alpha = 256 - cov_src_a (スカラ版)
    let inv_alpha = bcx.ins().isub(c256_scalar, cov_src_a);

    // ステップ 3: SrcOver 合成 (スカラ版)
    let dst_pixel = bcx.ins().load(types::I32, MemFlags::new(), current_dst, 0);

    let dst_a_s = bcx.ins().ushr_imm(dst_pixel, 24);
    let dst_a_s = bcx.ins().band_imm(dst_a_s, 0xFF);
    let dst_r_s = bcx.ins().ushr_imm(dst_pixel, 16);
    let dst_r_s = bcx.ins().band_imm(dst_r_s, 0xFF);
    let dst_g_s = bcx.ins().ushr_imm(dst_pixel, 8);
    let dst_g_s = bcx.ins().band_imm(dst_g_s, 0xFF);
    let dst_b_s = bcx.ins().band_imm(dst_pixel, 0xFF);

    // out_c = cov_src_c + (dst_c * inv_alpha) >> 8
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

    // スカラ版パック
    let result = bcx.ins().ishl_imm(out_a, 24);
    let tmp = bcx.ins().ishl_imm(out_r, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(out_g, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, out_b);

    bcx.ins().store(MemFlags::new(), result, current_dst, 0);

    // ポインタ更新
    let four = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(current_dst, four);
    let one_ptr = bcx.ins().iconst(ptr_type, 1);
    let next_cov = bcx.ins().iadd(current_cov, one_ptr);
    let next_si = bcx.ins().iadd(scalar_i, one_ptr);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    let args_loop = block_args(&[next_dst, next_cov, next_si]);
    bcx.ins().brif(cont, scalar_loop, &args_loop, exit, &[]);

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}

/// Porter-Duff SrcOver パイプラインを構築する (カバレッジなし)。
///
/// ## 合成式
///
/// ```text
/// inv_alpha = 256 - src_a
/// out_c = src_c + (dst_c * inv_alpha) >> 8
/// ```
///
/// SrcOver はアルファブレンドの基本演算。半透明のソース色を既存の dst に重ねる。
/// premultiplied alpha の利点は、合成式が加算 1 回 + 乗算 1 回で済むこと。
/// straight alpha では乗算 2 回 + 除算 1 回が必要。
///
/// ## inv_alpha = 256 - src_a の理由
///
/// 正確には `inv_alpha = (255 - src_a)` だが、>>8 で除算するために
/// 256 ベースに変換する。`dst_c * (256 - src_a) >> 8` は
/// `dst_c * (255 - src_a) / 255` の高速近似。
///
/// src_a=0 → inv_alpha=256 → dst_c * 256 >> 8 = dst_c (dst そのまま)
/// src_a=255 → inv_alpha=1 → dst_c * 1 >> 8 = 0 (dst 消滅、src で置換)
/// src_a=128 → inv_alpha=128 → dst_c * 128 >> 8 = dst_c / 2
///
/// ## 使用場面
///
/// - fill_rect() で半透明色の矩形を重ねる場合
/// - inv_alpha はループ不変 (src_a は固定) なので entry で 1 回だけ計算
///
/// ## 最適化
///
/// - AG/RB インターリーブ: 4 チャネル分離の代わりに AG (0x00AA00GG) と
///   RB (0x00RR00BB) の 2 ペアに分け、imul 2 回で全チャネルを合成。
///   255 * 256 = 65280 < 65536 のためチャネル間の干渉なし。
///   26 → 13 命令/4px。
/// - 4x ループアンロール: 16px/反復でループ制御オーバーヘッドを削減。
pub(super) fn build_src_over(mut bcx: FunctionBuilder, ptr_type: Type) {
    let entry = bcx.create_block();
    let unroll_loop = bcx.create_block();
    let tail_check = bcx.create_block();
    let tail_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    // === entry ブロック ===
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let src_solid = bcx.block_params(entry)[1];
    let count = bcx.block_params(entry)[2];

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
    let src_ag_vec = bcx.ins().splat(types::I32X4, src_ag);
    let src_rb_vec = bcx.ins().splat(types::I32X4, src_rb);
    let inv_alpha_vec = bcx.ins().splat(types::I32X4, inv_alpha);
    let mask_vec = bcx.ins().splat(types::I32X4, mask_00ff00ff);

    // ループカウント
    // count16 = count / 16 (16px = 4x I32X4 per iteration)
    // tail_quads = (count % 16) / 4 (残り 4px チャンクの数 0-3)
    // remainder = count % 4 (残り 0-3px)
    let count16 = bcx.ins().ushr_imm(count, 4);
    let tail_quads = bcx.ins().band_imm(count, 0xF);
    let tail_quads = bcx.ins().ushr_imm(tail_quads, 2);
    let remainder = bcx.ins().band_imm(count, 3);
    let zero = bcx.ins().iconst(ptr_type, 0);

    let has_unroll = bcx.ins().icmp(IntCC::NotEqual, count16, zero);
    bcx.ins().brif(
        has_unroll,
        unroll_loop,
        &block_args(&[dst, zero]),
        tail_check,
        &block_args(&[dst]),
    );

    // === unroll_loop ブロック (16px/反復 = 4x I32X4) ===
    bcx.append_block_param(unroll_loop, ptr_type);
    bcx.append_block_param(unroll_loop, ptr_type);
    bcx.switch_to_block(unroll_loop);
    let current_dst = bcx.block_params(unroll_loop)[0];
    let unroll_i = bcx.block_params(unroll_loop)[1];

    // チャンク 0: offset 0
    let px0 = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let r0 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px0,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r0, current_dst, 0);

    // チャンク 1: offset 16
    let px1 = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 16);
    let r1 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px1,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r1, current_dst, 16);

    // チャンク 2: offset 32
    let px2 = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 32);
    let r2 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px2,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r2, current_dst, 32);

    // チャンク 3: offset 48
    let px3 = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 48);
    let r3 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px3,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r3, current_dst, 48);

    let sixty_four = bcx.ins().iconst(ptr_type, 64);
    let next_dst = bcx.ins().iadd(current_dst, sixty_four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_i = bcx.ins().iadd(unroll_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_i, count16);
    bcx.ins().brif(
        cont,
        unroll_loop,
        &block_args(&[next_dst, next_i]),
        tail_check,
        &block_args(&[next_dst]),
    );

    // === tail_check ブロック ===
    bcx.append_block_param(tail_check, ptr_type);
    bcx.switch_to_block(tail_check);
    let current_dst = bcx.block_params(tail_check)[0];
    let has_tail = bcx.ins().icmp(IntCC::NotEqual, tail_quads, zero);
    bcx.ins().brif(
        has_tail,
        tail_loop,
        &block_args(&[current_dst, zero]),
        scalar_check,
        &block_args(&[current_dst]),
    );

    // === tail_loop ブロック (4px/反復) ===
    bcx.append_block_param(tail_loop, ptr_type);
    bcx.append_block_param(tail_loop, ptr_type);
    bcx.switch_to_block(tail_loop);
    let current_dst = bcx.block_params(tail_loop)[0];
    let tail_i = bcx.block_params(tail_loop)[1];

    let px = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let r = emit_src_over_ag_rb_simd(
        &mut bcx,
        px,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r, current_dst, 0);

    let sixteen = bcx.ins().iconst(ptr_type, 16);
    let next_dst = bcx.ins().iadd(current_dst, sixteen);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_ti = bcx.ins().iadd(tail_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_ti, tail_quads);
    bcx.ins().brif(
        cont,
        tail_loop,
        &block_args(&[next_dst, next_ti]),
        scalar_check,
        &block_args(&[next_dst]),
    );

    // === scalar_check ブロック ===
    bcx.append_block_param(scalar_check, ptr_type);
    bcx.switch_to_block(scalar_check);
    let current_dst = bcx.block_params(scalar_check)[0];
    let has_remainder = bcx.ins().icmp(IntCC::NotEqual, remainder, zero);
    bcx.ins().brif(
        has_remainder,
        scalar_loop,
        &block_args(&[current_dst, zero]),
        exit,
        &[],
    );

    // === scalar_loop ブロック (AG/RB スカラ版) ===
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let current_dst = bcx.block_params(scalar_loop)[0];
    let scalar_i = bcx.block_params(scalar_loop)[1];

    let dst_pixel = bcx.ins().load(types::I32, MemFlags::new(), current_dst, 0);

    // AG/RB 分解 → 合成 → パック (スカラ版)
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

    bcx.ins().store(MemFlags::new(), result, current_dst, 0);

    let four = bcx.ins().iconst(ptr_type, 4);
    let next_dst = bcx.ins().iadd(current_dst, four);
    let one = bcx.ins().iconst(ptr_type, 1);
    let next_si = bcx.ins().iadd(scalar_i, one);
    let cont = bcx.ins().icmp(IntCC::UnsignedLessThan, next_si, remainder);
    bcx.ins().brif(
        cont,
        scalar_loop,
        &block_args(&[next_dst, next_si]),
        exit,
        &[],
    );

    // === exit ブロック ===
    bcx.switch_to_block(exit);
    bcx.ins().return_(&[]);

    bcx.seal_all_blocks();
    bcx.finalize();
}

/// AG/RB インターリーブによる SrcOver 合成 (SIMD)。
///
/// 4 チャネル分離 (26 命令) の代わりに、AG (0x00AA00GG) と RB (0x00RR00BB) の
/// 2 ペアに分けて 2 回の imul で全チャネルを合成する (13 命令)。
///
/// 各チャネルは 8-bit、inv_alpha は 0-256 の 9-bit なので、
/// チャネル積の最大値は 255 * 256 = 65280 (16-bit) でペア間の干渉なし。
pub(super) fn emit_src_over_ag_rb_simd(
    bcx: &mut FunctionBuilder,
    dst_pixels: Value,
    src_ag_vec: Value,
    src_rb_vec: Value,
    inv_alpha_vec: Value,
    mask_vec: Value,
) -> Value {
    let dst_ag = bcx.ins().ushr_imm(dst_pixels, 8);
    let dst_ag = bcx.ins().band(dst_ag, mask_vec);
    let dst_rb = bcx.ins().band(dst_pixels, mask_vec);

    let tmp_ag = bcx.ins().imul(dst_ag, inv_alpha_vec);
    let tmp_ag = bcx.ins().ushr_imm(tmp_ag, 8);
    let tmp_ag = bcx.ins().band(tmp_ag, mask_vec);
    let out_ag = bcx.ins().iadd(src_ag_vec, tmp_ag);

    let tmp_rb = bcx.ins().imul(dst_rb, inv_alpha_vec);
    let tmp_rb = bcx.ins().ushr_imm(tmp_rb, 8);
    let tmp_rb = bcx.ins().band(tmp_rb, mask_vec);
    let out_rb = bcx.ins().iadd(src_rb_vec, tmp_rb);

    let result = bcx.ins().ishl_imm(out_ag, 8);
    bcx.ins().bor(result, out_rb)
}
