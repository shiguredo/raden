// =============================================================================
// pipeline/compiler.rs -- Cranelift JIT パイプラインコンパイラ
// =============================================================================
//
// ## 概要
//
// このモジュールは Cranelift JIT コンパイラを使用して、ピクセル合成パイプラインを
// ネイティブコードにコンパイルする。Blend2D が AsmJit で実現している JIT パイプラインを、
// Cranelift の SSA IR + レジスタアロケータで同等以上の性能で実現することを目指す。
//
// ## アーキテクチャ
//
// ```
//  PipelineCompiler::compile() / compile_cov()
//    │
//    ├── Cranelift ISA 設定 (opt_level=speed, ホスト CPU 自動検出)
//    ├── 関数シグネチャ定義 (ABI: SystemV / Windows)
//    ├── IR 生成 (build_xxx 関数が SSA IR を構築)
//    │     ├── entry ブロック: ループ不変値の事前計算
//    │     ├── simd_loop: I32X4 で 4 ピクセル並列処理
//    │     ├── scalar_check: 余りピクセル判定
//    │     └── scalar_loop: 1 ピクセルずつ処理
//    ├── Cranelift 最適化パス + レジスタ割り当て
//    └── ネイティブコード生成 → 関数ポインタとして返却
// ```
//
// ## SIMD 戦略
//
// Cranelift の I32X4 型 (128-bit SIMD) を使用する。これは以下にマッピングされる:
// - x86_64: SSE2 (全 x86_64 CPU で利用可能)
// - AArch64: NEON (全 AArch64 CPU で利用可能)
//
// 4 ピクセルを並列処理するため、全ての合成関数は以下の構造を持つ:
// 1. メインの SIMD ループ: count/4 回、4 ピクセルずつ処理
// 2. スカラの余りループ: count%4 回、1 ピクセルずつ処理
//
// Blend2D は AVX2 で 8 ピクセル並列処理を実現しているが、Cranelift は現在
// I32X4 (128-bit) までしかサポートしていないため、4 ピクセルが上限。
// ただし、Cranelift の SSA 最適化 + レジスタアロケータにより、
// ループ内の命令スケジューリングは AsmJit 手書きコードに匹敵する。
//
// ## Blend2D との比較
//
// | 項目 | Blend2D (AsmJit) | raden (Cranelift) |
// |---|---|---|
// | SIMD 幅 | 256-bit (AVX2) / 128-bit (SSE2) | 128-bit (I32X4) |
// | JIT バックエンド | AsmJit (直接コード生成) | Cranelift (SSA IR → 最適化 → コード生成) |
// | /255 近似 | (x * 257 + 257) >> 16 | 同一方式 |
// | inv_alpha | 256 - src_a | 同一方式 |
// | ループ構造 | 8px SIMD + 余り | 4px SIMD + 余り |
// | 最適化 | 手動命令選択 | Cranelift opt_level=speed |
//
// ## パイプライン関数の種類
//
// | 関数 | 合成演算 | カバレッジ | 用途 |
// |---|---|---|---|
// | build_src_copy | SrcCopy | なし | fill_rect() |
// | build_src_over | SrcOver | なし | fill_rect() |
// | build_src_copy_cov | SrcCopy | あり | fill_circle() / fill_path() |
// | build_src_over_cov | SrcOver | あり | fill_circle() / fill_path() |
//
// =============================================================================

mod blend_build;
mod blend_modes;
mod box_pipelines;
mod core_pipelines;
mod porter_duff;
mod sweep;
mod transform;

use cranelift_codegen::ir::instructions::BlockArg;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, Endianness, InstBuilder, MemFlags, Value};
use cranelift_codegen::settings;
use cranelift_codegen::settings::Configurable;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};

use super::cache::{PipelineBoxFn, PipelineCovFn, PipelineFn, SweepFn};
use super::key::PipelineKey;
use crate::api::style::{CompOp, FillRule};

use blend_build::*;
use box_pipelines::*;
use core_pipelines::*;
use porter_duff::*;
use sweep::build_sweep;
use transform::build_transform_edges;

/// Cranelift JIT を使用してパイプライン関数を生成するコンパイラ。
///
/// ## 設計方針
///
/// 各パイプラインごとに独立した `JITModule` を生成する。
/// Cranelift の `JITModule` はコード生成後に `finalize_definitions()` で
/// 実行可能メモリに配置されるが、`JITModule` がドロップされるとそのメモリも
/// 解放されてしまう。そのため、生成した全モジュールを `modules` ベクタに保持し、
/// 関数ポインタが常に有効であることを保証する。
///
/// ## パフォーマンス特性
///
/// - 初回コンパイル: ~1-5ms (Cranelift の IR 構築 + 最適化 + コード生成)
/// - 2 回目以降: PipelineCache 経由で O(1) ルックアップ
/// - 生成されるコードのサイズ: ~200-800 bytes/パイプライン
#[derive(Default)]
pub struct PipelineCompiler {
    modules: Vec<JITModule>,
}

impl PipelineCompiler {
    pub fn new() -> Self {
        Self::default()
    }

    /// カバレッジなしパイプライン関数を JIT コンパイルする。
    ///
    /// ## シグネチャ
    ///
    /// ```text
    /// fn pipeline(dst: *mut u8, src_solid: u32, count: usize)
    /// ```
    ///
    /// - `dst`: 書き込み先ピクセルバッファ (PRGB32 形式、4 バイト/ピクセル)
    /// - `src_solid`: premultiplied ARGB32 形式のソース色 (0xAARRGGBB)
    /// - `count`: 処理するピクセル数
    ///
    /// fill_rect() から呼ばれ、矩形領域の各スキャンラインに対して実行される。
    /// カバレッジ処理が不要なため、最もシンプルで高速。
    pub fn compile(&mut self, key: &PipelineKey, comp_op: CompOp) -> PipelineFn {
        let mut flag_builder = settings::builder();
        // JIT コードはプロセス内で直接呼び出すため、PIC (Position Independent Code) は不要。
        // これにより GOT/PLT 経由の間接呼び出しが排除される。
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        // Cranelift の最適化パスを有効化する。
        // opt_level=speed により以下の最適化が適用される:
        // - 命令結合 (iadd + imul → lea 等)
        // - 不要な mov 除去
        // - SIMD 命令の最適選択
        // - ループ内定数の巻き上げ
        flag_builder.set("opt_level", "speed").unwrap();
        let isa_builder = cranelift_native::builder().expect("host machine is not supported");
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));

        let ptr_type = module.target_config().pointer_type();

        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type)); // dst: *mut u8
        sig.params.push(AbiParam::new(types::I32)); // src_solid: u32
        sig.params.push(AbiParam::new(ptr_type)); // count: usize

        let func_name = format!("pipeline_{:#x}", key.value());
        let func_id = module
            .declare_function(&func_name, Linkage::Local, &sig)
            .unwrap();

        let mut ctx = module.make_context();
        let mut func_ctx = FunctionBuilderContext::new();
        ctx.func.signature = sig;

        {
            let bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            match comp_op {
                CompOp::SrcOver => build_src_over(bcx, ptr_type),
                CompOp::SrcCopy => build_src_copy(bcx, ptr_type),
                CompOp::Clear => build_clear(bcx, ptr_type),
                CompOp::DstCopy => build_dst_copy(bcx, ptr_type),
                CompOp::Plus => build_plus(bcx, ptr_type),
                CompOp::SrcIn => build_src_in(bcx, ptr_type),
                CompOp::SrcOut => build_src_out(bcx, ptr_type),
                CompOp::SrcAtop => build_src_atop(bcx, ptr_type),
                CompOp::DstOver => build_dst_over(bcx, ptr_type),
                CompOp::DstIn => build_dst_in(bcx, ptr_type),
                CompOp::DstOut => build_dst_out(bcx, ptr_type),
                CompOp::DstAtop => build_dst_atop(bcx, ptr_type),
                CompOp::Xor => build_xor(bcx, ptr_type),
                CompOp::Minus => build_minus(bcx, ptr_type),
                CompOp::Modulate => build_modulate(bcx, ptr_type),
                CompOp::Multiply => build_multiply(bcx, ptr_type),
                CompOp::Screen => build_screen(bcx, ptr_type),
                CompOp::Overlay => build_overlay(bcx, ptr_type),
                CompOp::Darken => build_darken(bcx, ptr_type),
                CompOp::Lighten => build_lighten(bcx, ptr_type),
                CompOp::ColorDodge => build_color_dodge(bcx, ptr_type),
                CompOp::ColorBurn => build_color_burn(bcx, ptr_type),
                CompOp::LinearBurn => build_linear_burn(bcx, ptr_type),
                CompOp::LinearLight => build_linear_light(bcx, ptr_type),
                CompOp::PinLight => build_pin_light(bcx, ptr_type),
                CompOp::HardLight => build_hard_light(bcx, ptr_type),
                CompOp::SoftLight => build_soft_light(bcx, ptr_type),
                CompOp::Difference => build_difference(bcx, ptr_type),
                CompOp::Exclusion => build_exclusion(bcx, ptr_type),
            }
        }

        if std::env::var("RADEN_DUMP_PIPELINE").is_ok() {
            ctx.set_disasm(true);
        }

        module.define_function(func_id, &mut ctx).unwrap();

        if let Some(disasm) = ctx.compiled_code().unwrap().vcode.as_ref() {
            eprintln!("=== {} ===\n{}", func_name, disasm);
        }

        module.clear_context(&mut ctx);
        module.finalize_definitions().unwrap();

        let code = module.get_finalized_function(func_id);
        let func: PipelineFn = unsafe { std::mem::transmute(code) };

        self.modules.push(module);
        func
    }

    /// カバレッジ付きパイプライン関数を JIT コンパイルする。
    ///
    /// ## シグネチャ
    ///
    /// ```text
    /// fn pipeline_cov(dst: *mut u8, src_solid: u32, count: usize, coverage: *const u8)
    /// ```
    ///
    /// - `coverage`: カバレッジマスク (0-255 のバイト配列、count 要素)
    ///   - 0: ピクセル完全に図形外 (合成しない)
    ///   - 255: ピクセル完全に図形内 (カバレッジ乗算なしと等価)
    ///   - 1-254: ピクセルが図形境界にかかる (アンチエイリアシング)
    ///
    /// fill_circle() / fill_path() のアンチエイリアスレンダリングで使用。
    /// AnalyticRasterizer が生成するカバレッジマスクと組み合わせて動作する。
    pub fn compile_cov(&mut self, key: &PipelineKey, comp_op: CompOp) -> PipelineCovFn {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        flag_builder.set("opt_level", "speed").unwrap();
        let isa_builder = cranelift_native::builder().expect("host machine is not supported");
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));

        let ptr_type = module.target_config().pointer_type();

        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type)); // dst: *mut u8
        sig.params.push(AbiParam::new(types::I32)); // src_solid: u32
        sig.params.push(AbiParam::new(ptr_type)); // count: usize
        sig.params.push(AbiParam::new(ptr_type)); // coverage: *const u8

        let func_name = format!("pipeline_cov_{:#x}", key.value());
        let func_id = module
            .declare_function(&func_name, Linkage::Local, &sig)
            .unwrap();

        let mut ctx = module.make_context();
        let mut func_ctx = FunctionBuilderContext::new();
        ctx.func.signature = sig;

        {
            let bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            match comp_op {
                CompOp::SrcOver => build_src_over_cov(bcx, ptr_type),
                CompOp::SrcCopy => build_src_copy_cov(bcx, ptr_type),
                CompOp::Clear => build_clear_cov(bcx, ptr_type),
                CompOp::DstCopy => build_dst_copy_cov(bcx, ptr_type),
                CompOp::Plus => build_plus_cov(bcx, ptr_type),
                CompOp::SrcIn => build_src_in_cov(bcx, ptr_type),
                CompOp::SrcOut => build_src_out_cov(bcx, ptr_type),
                CompOp::SrcAtop => build_src_atop_cov(bcx, ptr_type),
                CompOp::DstOver => build_dst_over_cov(bcx, ptr_type),
                CompOp::DstIn => build_dst_in_cov(bcx, ptr_type),
                CompOp::DstOut => build_dst_out_cov(bcx, ptr_type),
                CompOp::DstAtop => build_dst_atop_cov(bcx, ptr_type),
                CompOp::Xor => build_xor_cov(bcx, ptr_type),
                CompOp::Minus => build_minus_cov(bcx, ptr_type),
                CompOp::Modulate => build_modulate_cov(bcx, ptr_type),
                CompOp::Multiply => build_multiply_cov(bcx, ptr_type),
                CompOp::Screen => build_screen_cov(bcx, ptr_type),
                CompOp::Overlay => build_overlay_cov(bcx, ptr_type),
                CompOp::Darken => build_darken_cov(bcx, ptr_type),
                CompOp::Lighten => build_lighten_cov(bcx, ptr_type),
                CompOp::ColorDodge => build_color_dodge_cov(bcx, ptr_type),
                CompOp::ColorBurn => build_color_burn_cov(bcx, ptr_type),
                CompOp::LinearBurn => build_linear_burn_cov(bcx, ptr_type),
                CompOp::LinearLight => build_linear_light_cov(bcx, ptr_type),
                CompOp::PinLight => build_pin_light_cov(bcx, ptr_type),
                CompOp::HardLight => build_hard_light_cov(bcx, ptr_type),
                CompOp::SoftLight => build_soft_light_cov(bcx, ptr_type),
                CompOp::Difference => build_difference_cov(bcx, ptr_type),
                CompOp::Exclusion => build_exclusion_cov(bcx, ptr_type),
            }
        }

        module.define_function(func_id, &mut ctx).unwrap();
        module.clear_context(&mut ctx);
        module.finalize_definitions().unwrap();

        let code = module.get_finalized_function(func_id);
        let func: PipelineCovFn = unsafe { std::mem::transmute(code) };

        self.modules.push(module);
        func
    }

    /// 矩形塗りつぶし専用パイプライン関数を JIT コンパイルする。
    ///
    /// ## シグネチャ
    ///
    /// ```text
    /// fn pipeline_box(dst: *mut u8, src_solid: u32, width: usize, height: usize, stride: usize)
    /// ```
    ///
    /// y ループを JIT 内に含むことで:
    /// - scanline ごとの間接呼び出しオーバーヘッドを排除
    /// - splat 済みベクタ等のループ不変値を全スキャンラインで再利用
    /// - 4x SIMD アンロール (16px/反復) で内部ループのオーバーヘッドを最小化
    pub fn compile_box(&mut self, key: &PipelineKey, comp_op: CompOp) -> PipelineBoxFn {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        flag_builder.set("opt_level", "speed").unwrap();
        let isa_builder = cranelift_native::builder().expect("host machine is not supported");
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));

        let ptr_type = module.target_config().pointer_type();

        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type)); // dst: *mut u8
        sig.params.push(AbiParam::new(types::I32)); // src_solid: u32
        sig.params.push(AbiParam::new(ptr_type)); // width: usize
        sig.params.push(AbiParam::new(ptr_type)); // height: usize
        sig.params.push(AbiParam::new(ptr_type)); // stride: usize

        let func_name = format!("pipeline_box_{:#x}", key.value());
        let func_id = module
            .declare_function(&func_name, Linkage::Local, &sig)
            .unwrap();

        let mut ctx = module.make_context();
        let mut func_ctx = FunctionBuilderContext::new();
        ctx.func.signature = sig;

        {
            let bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            match comp_op {
                CompOp::SrcOver => build_src_over_box(bcx, ptr_type),
                CompOp::SrcCopy => build_src_copy_box(bcx, ptr_type),
                _ => unreachable!("compile_box supports only SrcOver and SrcCopy"),
            }
        }

        if std::env::var("RADEN_DUMP_PIPELINE").is_ok() {
            ctx.set_disasm(true);
        }

        module.define_function(func_id, &mut ctx).unwrap();

        if let Some(disasm) = ctx.compiled_code().unwrap().vcode.as_ref() {
            eprintln!("=== {} ===\n{}", func_name, disasm);
        }

        module.clear_context(&mut ctx);
        module.finalize_definitions().unwrap();

        let code = module.get_finalized_function(func_id);
        let func: PipelineBoxFn = unsafe { std::mem::transmute(code) };

        self.modules.push(module);
        func
    }

    /// sweep 関数を JIT コンパイルする。
    ///
    /// ## シグネチャ
    ///
    /// ```text
    /// fn sweep(cells: *const i32, cov_buf: *mut u8, len: usize)
    /// ```
    ///
    /// prefix sum + abs + clamp(255) を計算し、結果を cov_buf に書き込む。
    /// 4 要素アンロールで 4 バイトを 1 つの i32 ストアに統合する。
    pub fn compile_sweep(&mut self, fill_rule: FillRule) -> SweepFn {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        flag_builder.set("opt_level", "speed").unwrap();
        let isa_builder = cranelift_native::builder().expect("host machine is not supported");
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));

        let ptr_type = module.target_config().pointer_type();

        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type)); // cells: *const i32
        sig.params.push(AbiParam::new(ptr_type)); // cov_buf: *mut u8
        sig.params.push(AbiParam::new(ptr_type)); // len: usize

        let name = match fill_rule {
            FillRule::NonZero => "jit_sweep_non_zero",
            FillRule::EvenOdd => "jit_sweep_even_odd",
        };
        let func_id = module.declare_function(name, Linkage::Local, &sig).unwrap();

        let mut ctx = module.make_context();
        let mut func_ctx = FunctionBuilderContext::new();
        ctx.func.signature = sig;

        {
            let bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            build_sweep(bcx, ptr_type, fill_rule);
        }

        module.define_function(func_id, &mut ctx).unwrap();
        module.clear_context(&mut ctx);
        module.finalize_definitions().unwrap();

        let code = module.get_finalized_function(func_id);
        let func: SweepFn = unsafe { std::mem::transmute(code) };

        self.modules.push(module);
        func
    }

    /// エッジ座標変換関数を JIT コンパイルする。
    ///
    /// ## シグネチャ
    ///
    /// ```text
    /// fn transform_edges(edges: *mut f64, count: usize,
    ///     m00: f64, m01: f64, m10: f64, m11: f64, m20: f64, m21: f64)
    /// ```
    ///
    /// F64X2 SIMD で各エッジの 2 点 (x0,y0), (x1,y1) を一括変換する。
    pub fn compile_transform_edges(&mut self) -> super::cache::TransformEdgesFn {
        let mut flag_builder = settings::builder();
        flag_builder.set("use_colocated_libcalls", "false").unwrap();
        flag_builder.set("is_pic", "false").unwrap();
        flag_builder.set("opt_level", "speed").unwrap();
        let isa_builder = cranelift_native::builder().expect("host machine is not supported");
        let isa = isa_builder
            .finish(settings::Flags::new(flag_builder))
            .unwrap();
        let mut module = JITModule::new(JITBuilder::with_isa(isa, default_libcall_names()));

        let ptr_type = module.target_config().pointer_type();

        let mut sig = module.make_signature();
        sig.params.push(AbiParam::new(ptr_type)); // edges: *mut f64
        sig.params.push(AbiParam::new(ptr_type)); // count: usize
        sig.params.push(AbiParam::new(types::F64)); // m00
        sig.params.push(AbiParam::new(types::F64)); // m01
        sig.params.push(AbiParam::new(types::F64)); // m10
        sig.params.push(AbiParam::new(types::F64)); // m11
        sig.params.push(AbiParam::new(types::F64)); // m20
        sig.params.push(AbiParam::new(types::F64)); // m21

        let func_id = module
            .declare_function("jit_transform_edges", Linkage::Local, &sig)
            .unwrap();

        let mut ctx = module.make_context();
        let mut func_ctx = FunctionBuilderContext::new();
        ctx.func.signature = sig;

        {
            let bcx = FunctionBuilder::new(&mut ctx.func, &mut func_ctx);
            build_transform_edges(bcx, ptr_type);
        }

        module.define_function(func_id, &mut ctx).unwrap();
        module.clear_context(&mut ctx);
        module.finalize_definitions().unwrap();

        let code = module.get_finalized_function(func_id);
        let func: super::cache::TransformEdgesFn = unsafe { std::mem::transmute(code) };

        self.modules.push(module);
        func
    }
}

// =============================================================================
// SIMD ヘルパー関数
// =============================================================================
//
// 以下の 3 つのヘルパーは全ての SIMD パイプラインで共有される。
// Cranelift の FunctionBuilder に対して SIMD 命令列を emit する。

/// SSA Value のスライスから Cranelift の BlockArg スライスを生成する。
///
/// Cranelift の `brif` 命令はブロック引数として `BlockArg` の配列を要求する。
/// SSA 形式のため、ループ変数はブロックパラメータとして渡す必要があり、
/// この変換は全てのブロック間遷移で必要になる。
pub(super) fn block_args(values: &[Value]) -> Vec<BlockArg> {
    values.iter().map(|v| BlockArg::Value(*v)).collect()
}

/// I32X4 ベクタから ARGB チャネルを個別の I32X4 に抽出する。
///
/// ## 入力
///
/// `pixel_vec`: 4 ピクセルの PRGB32 値が格納された I32X4 ベクタ
/// ```text
/// lane[0] = 0xAARRGGBB  (ピクセル 0)
/// lane[1] = 0xAARRGGBB  (ピクセル 1)
/// lane[2] = 0xAARRGGBB  (ピクセル 2)
/// lane[3] = 0xAARRGGBB  (ピクセル 3)
/// ```
///
/// ## 出力
///
/// 各チャネルが独立した I32X4 ベクタに分離される:
/// ```text
/// a = [A0, A1, A2, A3]  (各 0-255)
/// r = [R0, R1, R2, R3]  (各 0-255)
/// g = [G0, G1, G2, G3]  (各 0-255)
/// b = [B0, B1, B2, B3]  (各 0-255)
/// ```
///
/// ## 生成される命令 (x86_64)
///
/// ```asm
/// ; alpha チャネル
/// vpsrld  xmm_a, xmm_pixel, 24    ; 各レーンを 24 ビット右シフト
/// vpand   xmm_a, xmm_a, xmm_0xff  ; 0xFF でマスク
/// ; red チャネル (同様に 16 ビットシフト + マスク)
/// ; green チャネル (同様に 8 ビットシフト + マスク)
/// ; blue チャネル (マスクのみ)
/// ```
///
/// 計 8 命令 (4 シフト + 4 マスク) で 4 ピクセルの全チャネルを抽出。
pub(super) fn emit_extract_channels_simd(
    bcx: &mut FunctionBuilder,
    pixel_vec: Value,
    mask_0xff_vec: Value,
) -> (Value, Value, Value, Value) {
    // alpha: bits[31:24] → ushr 24 → band 0xFF
    let a = bcx.ins().ushr_imm(pixel_vec, 24);
    let a = bcx.ins().band(a, mask_0xff_vec);
    // red: bits[23:16] → ushr 16 → band 0xFF
    let r = bcx.ins().ushr_imm(pixel_vec, 16);
    let r = bcx.ins().band(r, mask_0xff_vec);
    // green: bits[15:8] → ushr 8 → band 0xFF
    let g = bcx.ins().ushr_imm(pixel_vec, 8);
    let g = bcx.ins().band(g, mask_0xff_vec);
    // blue: bits[7:0] → band 0xFF のみ
    let b = bcx.ins().band(pixel_vec, mask_0xff_vec);
    (a, r, g, b)
}

/// 4 チャネルの I32X4 ベクタを ARGB32 形式の I32X4 にパックする。
///
/// emit_extract_channels_simd の逆操作。
///
/// ## 入力
///
/// ```text
/// a = [A0, A1, A2, A3]  (各 0-255)
/// r = [R0, R1, R2, R3]  (各 0-255)
/// g = [G0, G1, G2, G3]  (各 0-255)
/// b = [B0, B1, B2, B3]  (各 0-255)
/// ```
///
/// ## 出力
///
/// ```text
/// result = [0xA0R0G0B0, 0xA1R1G1B1, 0xA2R2G2B2, 0xA3R3G3B3]
/// ```
///
/// ## 生成される命令 (x86_64)
///
/// ```asm
/// vpslld  xmm_result, xmm_a, 24   ; A << 24
/// vpslld  xmm_tmp, xmm_r, 16      ; R << 16
/// vpor    xmm_result, xmm_result, xmm_tmp
/// vpslld  xmm_tmp, xmm_g, 8       ; G << 8
/// vpor    xmm_result, xmm_result, xmm_tmp
/// vpor    xmm_result, xmm_result, xmm_b  ; B はシフト不要
/// ```
///
/// 計 6 命令 (3 シフト + 3 OR)。
pub(super) fn emit_pack_channels_simd(
    bcx: &mut FunctionBuilder,
    a: Value,
    r: Value,
    g: Value,
    b: Value,
) -> Value {
    let result = bcx.ins().ishl_imm(a, 24);
    let tmp = bcx.ins().ishl_imm(r, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(g, 8);
    let result = bcx.ins().bor(result, tmp);
    bcx.ins().bor(result, b)
}

/// 既にロード済みの packed i32 カバレッジ値を I32X4 に展開する。
///
/// cov=0xFF 高速パスでは packed i32 を先にロードして全 0xFF かを判定する。
/// 全 0xFF でない場合のみ、この関数で I32X4 に展開して通常の合成計算を行う。
///
/// ## 変換チェーン
///
/// ```text
/// packed_i32 = 0xCov3_Cov2_Cov1_Cov0  (リトルエンディアン)
///            ↓ scalar_to_vector(I32X4)
/// vec = [packed_i32, 0, 0, 0]
///            ↓ bitcast(I8X16, LE)  -- レーン数変更のため LE フラグが必須
/// vec_i8 = [Cov0, Cov1, Cov2, Cov3, 0, ..., 0]
///            ↓ uwiden_low()  -- I8X16 → I16X8
/// vec_i16 = [Cov0, Cov1, Cov2, Cov3, 0, 0, 0, 0]
///            ↓ uwiden_low()  -- I16X8 → I32X4
/// cov_vec = [Cov0, Cov1, Cov2, Cov3]  (I32X4、各 0-255)
/// ```
///
/// ## bitcast の LE フラグについて
///
/// Cranelift の bitcast 命令でレーン数が変わる場合 (I32X4 → I8X16)、
/// バイトオーダーの指定が必須。Little Endian を指定することで、
/// リトルエンディアンメモリレイアウトと一致するバイト解釈になる。
/// LE フラグなしでは "Byte order specifier required" エラーが発生する。
///
/// ## 生成される命令 (x86_64)
///
/// ```asm
/// movd     xmm0, eax            ; スカラ → xmm 下位 32 bit
/// pmovzxbd xmm0, xmm0           ; byte → dword ゼロ拡張 (SSE4.1)
/// ; または SSE2 では:
/// pxor     xmm1, xmm1
/// punpcklbw xmm0, xmm1          ; byte → word
/// punpcklwd xmm0, xmm1          ; word → dword
/// ```
pub(super) fn emit_expand_packed_coverage_i32x4(
    bcx: &mut FunctionBuilder,
    packed_i32: Value,
) -> Value {
    // i32 → I32X4 の lane 0 に配置 (他レーンはゼロ)
    let vec = bcx.ins().scalar_to_vector(types::I32X4, packed_i32);
    // I32X4 → I8X16 にビット再解釈 (LE フラグ必須)
    let le_flags = MemFlags::new().with_endianness(Endianness::Little);
    let vec_i8 = bcx.ins().bitcast(types::I8X16, le_flags, vec);
    // I8X16 → I16X8 → I32X4 の 2 段階ゼロ拡張
    let vec_i16 = bcx.ins().uwiden_low(vec_i8);
    bcx.ins().uwiden_low(vec_i16)
}
