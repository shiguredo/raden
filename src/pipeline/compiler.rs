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

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::immediates::Ieee32;
use cranelift_codegen::ir::instructions::BlockArg;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, Endianness, InstBuilder, MemFlags, Type, Value};
use cranelift_codegen::settings;
use cranelift_codegen::settings::Configurable;
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};

use super::cache::{PipelineBoxFn, PipelineCovFn, PipelineFn, SweepFn};
use super::key::PipelineKey;
use crate::api::style::{CompOp, FillRule};

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
fn block_args(values: &[Value]) -> Vec<BlockArg> {
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
fn emit_extract_channels_simd(
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
fn emit_pack_channels_simd(
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
fn emit_expand_packed_coverage_i32x4(bcx: &mut FunctionBuilder, packed_i32: Value) -> Value {
    // i32 → I32X4 の lane 0 に配置 (他レーンはゼロ)
    let vec = bcx.ins().scalar_to_vector(types::I32X4, packed_i32);
    // I32X4 → I8X16 にビット再解釈 (LE フラグ必須)
    let le_flags = MemFlags::new().with_endianness(Endianness::Little);
    let vec_i8 = bcx.ins().bitcast(types::I8X16, le_flags, vec);
    // I8X16 → I16X8 → I32X4 の 2 段階ゼロ拡張
    let vec_i16 = bcx.ins().uwiden_low(vec_i8);
    bcx.ins().uwiden_low(vec_i16)
}

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
fn build_src_copy(mut bcx: FunctionBuilder, ptr_type: Type) {
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
fn build_src_copy_cov(mut bcx: FunctionBuilder, ptr_type: Type) {
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
fn build_src_over_cov(mut bcx: FunctionBuilder, ptr_type: Type) {
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
fn build_src_over(mut bcx: FunctionBuilder, ptr_type: Type) {
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
fn emit_src_over_ag_rb_simd(
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
fn build_src_over_box(mut bcx: FunctionBuilder, ptr_type: Type) {
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
    let px0 = bcx.ins().load(vec_type, MemFlags::new(), x_dst, 0);
    let r0 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px0,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r0, x_dst, 0);

    // チャンク 1: offset 16
    let px1 = bcx.ins().load(vec_type, MemFlags::new(), x_dst, 16);
    let r1 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px1,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r1, x_dst, 16);

    // チャンク 2: offset 32
    let px2 = bcx.ins().load(vec_type, MemFlags::new(), x_dst, 32);
    let r2 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px2,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r2, x_dst, 32);

    // チャンク 3: offset 48
    let px3 = bcx.ins().load(vec_type, MemFlags::new(), x_dst, 48);
    let r3 = emit_src_over_ag_rb_simd(
        &mut bcx,
        px3,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r3, x_dst, 48);

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

    let px = bcx.ins().load(vec_type, MemFlags::new(), x_dst, 0);
    let r = emit_src_over_ag_rb_simd(
        &mut bcx,
        px,
        src_ag_vec,
        src_rb_vec,
        inv_alpha_vec,
        mask_vec,
    );
    bcx.ins().store(MemFlags::new(), r, x_dst, 0);

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

    let dst_pixel = bcx.ins().load(types::I32, MemFlags::new(), x_dst, 0);
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

    bcx.ins().store(MemFlags::new(), result, x_dst, 0);

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
fn build_src_copy_box(mut bcx: FunctionBuilder, ptr_type: Type) {
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

    bcx.ins().store(MemFlags::new(), src_vec, x_dst, 0);
    bcx.ins().store(MemFlags::new(), src_vec, x_dst, 16);
    bcx.ins().store(MemFlags::new(), src_vec, x_dst, 32);
    bcx.ins().store(MemFlags::new(), src_vec, x_dst, 48);

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

    bcx.ins().store(MemFlags::new(), src_vec, x_dst, 0);

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

    bcx.ins().store(MemFlags::new(), src_solid, x_dst, 0);

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

// =============================================================================
// Sweep 関数
// =============================================================================

/// shifted 値を fill rule に応じてカバレッジ値に変換する JIT コードを生成する。
///
/// - NonZero: iabs → umin(c255) (2 命令)
/// - EvenOdd: iabs → band_imm(511) → c512 - val → umin(val, folded) → umin(c255) (5 命令)
fn emit_fill_rule_convert(
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
fn build_sweep(mut bcx: FunctionBuilder, ptr_type: Type, fill_rule: FillRule) {
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

// =============================================================================
// Porter-Duff 合成パイプライン (カバレッジなし)
// =============================================================================

/// Clear パイプライン (カバレッジなし)。out = 0。
fn build_clear(mut bcx: FunctionBuilder, ptr_type: Type) {
    let entry = bcx.create_block();
    let simd_loop = bcx.create_block();
    let scalar_check = bcx.create_block();
    let scalar_loop = bcx.create_block();
    let exit = bcx.create_block();

    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    let dst = bcx.block_params(entry)[0];
    let count = bcx.block_params(entry)[2];

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
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
    bcx.ins().store(MemFlags::new(), zero_vec, cur, 0);
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
    bcx.ins().store(MemFlags::new(), zero_i32, cur, 0);
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
    bcx.finalize();
}

/// DstCopy パイプライン (カバレッジなし)。out = dst (何もしない)。
fn build_dst_copy(mut bcx: FunctionBuilder, _ptr_type: Type) {
    let entry = bcx.create_block();
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize();
}

/// 汎用 Porter-Duff パイプライン (カバレッジなし) を構築する。
fn build_generic_compose(
    mut bcx: FunctionBuilder,
    ptr_type: Type,
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

    let src_a = bcx.ins().ushr_imm(src_solid, 24);
    let src_a = bcx.ins().band_imm(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm(src_solid, 16);
    let src_r = bcx.ins().band_imm(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm(src_solid, 8);
    let src_g = bcx.ins().band_imm(src_g, 0xFF);
    let src_b = bcx.ins().band_imm(src_solid, 0xFF);

    let src_a_vec = bcx.ins().splat(types::I32X4, src_a);
    let src_r_vec = bcx.ins().splat(types::I32X4, src_r);
    let src_g_vec = bcx.ins().splat(types::I32X4, src_g);
    let src_b_vec = bcx.ins().splat(types::I32X4, src_b);
    let c256_scalar = bcx.ins().iconst(types::I32, 256);
    let c256_vec = bcx.ins().splat(types::I32X4, c256_scalar);
    let mask_0xff = bcx.ins().iconst(types::I32, 0xFF);
    let mask_0xff_vec = bcx.ins().splat(types::I32X4, mask_0xff);

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
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

    let dst_px = bcx.ins().load(types::I32X4, MemFlags::new(), cur_dst, 0);
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
    bcx.ins().store(MemFlags::new(), result, cur_dst, 0);

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

    let dst_px = bcx.ins().load(types::I32, MemFlags::new(), cur_dst, 0);
    let da = bcx.ins().ushr_imm(dst_px, 24);
    let da = bcx.ins().band_imm(da, 0xFF);
    let dr = bcx.ins().ushr_imm(dst_px, 16);
    let dr = bcx.ins().band_imm(dr, 0xFF);
    let dg = bcx.ins().ushr_imm(dst_px, 8);
    let dg = bcx.ins().band_imm(dg, 0xFF);
    let db = bcx.ins().band_imm(dst_px, 0xFF);
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
    let result = bcx.ins().ishl_imm(oa, 24);
    let tmp = bcx.ins().ishl_imm(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlags::new(), result, cur_dst, 0);

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
    bcx.finalize();
}

// =============================================================================
// 合成演算関数 (SIMD / スカラ)
// =============================================================================

fn compose_src_in_simd(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(src_r, f);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(src_g, f);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(src_b, f);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

fn compose_src_in_scalar(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(src_r, f);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(src_g, f);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(src_b, f);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

fn compose_src_out_simd(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(src_r, inv);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(src_g, inv);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(src_b, inv);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

fn compose_src_out_scalar(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(src_r, inv);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(src_g, inv);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(src_b, inv);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

fn compose_src_atop_simd(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let t = bcx.ins().imul(dst_a, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    let oa = bcx.ins().iadd(oa, t);
    let or = bcx.ins().imul(src_r, da_f);
    let or = bcx.ins().ushr_imm(or, 8);
    let t = bcx.ins().imul(dst_r, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    let or = bcx.ins().iadd(or, t);
    let og = bcx.ins().imul(src_g, da_f);
    let og = bcx.ins().ushr_imm(og, 8);
    let t = bcx.ins().imul(dst_g, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    let og = bcx.ins().iadd(og, t);
    let ob = bcx.ins().imul(src_b, da_f);
    let ob = bcx.ins().ushr_imm(ob, 8);
    let t = bcx.ins().imul(dst_b, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    let ob = bcx.ins().iadd(ob, t);
    (oa, or, og, ob)
}

fn compose_src_atop_scalar(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let t = bcx.ins().imul(dst_a, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    let oa = bcx.ins().iadd(oa, t);
    let or = bcx.ins().imul(src_r, da_f);
    let or = bcx.ins().ushr_imm(or, 8);
    let t = bcx.ins().imul(dst_r, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    let or = bcx.ins().iadd(or, t);
    let og = bcx.ins().imul(src_g, da_f);
    let og = bcx.ins().ushr_imm(og, 8);
    let t = bcx.ins().imul(dst_g, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    let og = bcx.ins().iadd(og, t);
    let ob = bcx.ins().imul(src_b, da_f);
    let ob = bcx.ins().ushr_imm(ob, 8);
    let t = bcx.ins().imul(dst_b, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    let ob = bcx.ins().iadd(ob, t);
    (oa, or, og, ob)
}

fn compose_dst_over_simd(
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
    let t = bcx.ins().ushr_imm(t, 8);
    let oa = bcx.ins().iadd(dst_a, t);
    let t = bcx.ins().imul(src_r, inv);
    let t = bcx.ins().ushr_imm(t, 8);
    let or = bcx.ins().iadd(dst_r, t);
    let t = bcx.ins().imul(src_g, inv);
    let t = bcx.ins().ushr_imm(t, 8);
    let og = bcx.ins().iadd(dst_g, t);
    let t = bcx.ins().imul(src_b, inv);
    let t = bcx.ins().ushr_imm(t, 8);
    let ob = bcx.ins().iadd(dst_b, t);
    (oa, or, og, ob)
}

fn compose_dst_over_scalar(
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
    let t = bcx.ins().ushr_imm(t, 8);
    let oa = bcx.ins().iadd(dst_a, t);
    let t = bcx.ins().imul(src_r, inv);
    let t = bcx.ins().ushr_imm(t, 8);
    let or = bcx.ins().iadd(dst_r, t);
    let t = bcx.ins().imul(src_g, inv);
    let t = bcx.ins().ushr_imm(t, 8);
    let og = bcx.ins().iadd(dst_g, t);
    let t = bcx.ins().imul(src_b, inv);
    let t = bcx.ins().ushr_imm(t, 8);
    let ob = bcx.ins().iadd(dst_b, t);
    (oa, or, og, ob)
}

fn compose_dst_in_simd(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(dst_r, f);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(dst_g, f);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(dst_b, f);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

fn compose_dst_in_scalar(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(dst_r, f);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(dst_g, f);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(dst_b, f);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

fn compose_dst_out_simd(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(dst_r, inv);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(dst_g, inv);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(dst_b, inv);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

fn compose_dst_out_scalar(
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
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(dst_r, inv);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(dst_g, inv);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(dst_b, inv);
    let ob = bcx.ins().ushr_imm(ob, 8);
    (oa, or, og, ob)
}

fn compose_dst_atop_simd(
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
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(src_a, inv_da);
    let u = bcx.ins().ushr_imm(u, 8);
    let oa = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_r, sa_f);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(src_r, inv_da);
    let u = bcx.ins().ushr_imm(u, 8);
    let or = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_g, sa_f);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(src_g, inv_da);
    let u = bcx.ins().ushr_imm(u, 8);
    let og = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_b, sa_f);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(src_b, inv_da);
    let u = bcx.ins().ushr_imm(u, 8);
    let ob = bcx.ins().iadd(t, u);
    (oa, or, og, ob)
}

fn compose_dst_atop_scalar(
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
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(src_a, inv_da);
    let u = bcx.ins().ushr_imm(u, 8);
    let oa = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_r, sa_f);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(src_r, inv_da);
    let u = bcx.ins().ushr_imm(u, 8);
    let or = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_g, sa_f);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(src_g, inv_da);
    let u = bcx.ins().ushr_imm(u, 8);
    let og = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(dst_b, sa_f);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(src_b, inv_da);
    let u = bcx.ins().ushr_imm(u, 8);
    let ob = bcx.ins().iadd(t, u);
    (oa, or, og, ob)
}

fn compose_xor_simd(
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
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(dst_a, inv_sa);
    let u = bcx.ins().ushr_imm(u, 8);
    let oa = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_r, inv_da);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(dst_r, inv_sa);
    let u = bcx.ins().ushr_imm(u, 8);
    let or = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_g, inv_da);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(dst_g, inv_sa);
    let u = bcx.ins().ushr_imm(u, 8);
    let og = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_b, inv_da);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(dst_b, inv_sa);
    let u = bcx.ins().ushr_imm(u, 8);
    let ob = bcx.ins().iadd(t, u);
    (oa, or, og, ob)
}

fn compose_xor_scalar(
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
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(dst_a, inv_sa);
    let u = bcx.ins().ushr_imm(u, 8);
    let oa = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_r, inv_da);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(dst_r, inv_sa);
    let u = bcx.ins().ushr_imm(u, 8);
    let or = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_g, inv_da);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(dst_g, inv_sa);
    let u = bcx.ins().ushr_imm(u, 8);
    let og = bcx.ins().iadd(t, u);
    let t = bcx.ins().imul(src_b, inv_da);
    let t = bcx.ins().ushr_imm(t, 8);
    let u = bcx.ins().imul(dst_b, inv_sa);
    let u = bcx.ins().ushr_imm(u, 8);
    let ob = bcx.ins().iadd(t, u);
    (oa, or, og, ob)
}

fn compose_plus_simd(
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

fn compose_plus_scalar(
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

fn build_src_in(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_src_in_simd, compose_src_in_scalar);
}

fn build_src_out(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_src_out_simd, compose_src_out_scalar);
}

fn build_src_atop(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_src_atop_simd,
        compose_src_atop_scalar,
    );
}

fn build_dst_over(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_dst_over_simd,
        compose_dst_over_scalar,
    );
}

fn build_dst_in(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_dst_in_simd, compose_dst_in_scalar);
}

fn build_dst_out(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_dst_out_simd, compose_dst_out_scalar);
}

fn build_dst_atop(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_dst_atop_simd,
        compose_dst_atop_scalar,
    );
}

fn build_xor(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_xor_simd, compose_xor_scalar);
}

/// Plus パイプライン (カバレッジなし)。out = min(src + dst, 255)。
fn build_plus(mut bcx: FunctionBuilder, ptr_type: Type) {
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

    let src_a = bcx.ins().ushr_imm(src_solid, 24);
    let src_a = bcx.ins().band_imm(src_a, 0xFF);
    let src_r = bcx.ins().ushr_imm(src_solid, 16);
    let src_r = bcx.ins().band_imm(src_r, 0xFF);
    let src_g = bcx.ins().ushr_imm(src_solid, 8);
    let src_g = bcx.ins().band_imm(src_g, 0xFF);
    let src_b = bcx.ins().band_imm(src_solid, 0xFF);

    let sa_v = bcx.ins().splat(types::I32X4, src_a);
    let sr_v = bcx.ins().splat(types::I32X4, src_r);
    let sg_v = bcx.ins().splat(types::I32X4, src_g);
    let sb_v = bcx.ins().splat(types::I32X4, src_b);

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
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
    let dp = bcx.ins().load(types::I32X4, MemFlags::new(), cur, 0);
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
    bcx.ins().store(MemFlags::new(), result, cur, 0);
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
    let dp = bcx.ins().load(types::I32, MemFlags::new(), cur, 0);
    let da = bcx.ins().ushr_imm(dp, 24);
    let da = bcx.ins().band_imm(da, 0xFF);
    let dr = bcx.ins().ushr_imm(dp, 16);
    let dr = bcx.ins().band_imm(dr, 0xFF);
    let dg = bcx.ins().ushr_imm(dp, 8);
    let dg = bcx.ins().band_imm(dg, 0xFF);
    let db = bcx.ins().band_imm(dp, 0xFF);
    let oa = bcx.ins().iadd(src_a, da);
    let oa = bcx.ins().umin(oa, c255);
    let or = bcx.ins().iadd(src_r, dr);
    let or = bcx.ins().umin(or, c255);
    let og = bcx.ins().iadd(src_g, dg);
    let og = bcx.ins().umin(og, c255);
    let ob = bcx.ins().iadd(src_b, db);
    let ob = bcx.ins().umin(ob, c255);
    let result = bcx.ins().ishl_imm(oa, 24);
    let tmp = bcx.ins().ishl_imm(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlags::new(), result, cur, 0);
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
    bcx.finalize();
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
fn build_clear_cov(mut bcx: FunctionBuilder, ptr_type: Type) {
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

    let simd_count = bcx.ins().ushr_imm(count, 2);
    let remainder = bcx.ins().band_imm(count, 3);
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

    let packed_cov = bcx.ins().load(types::I32, MemFlags::new(), current_cov, 0);
    let is_all_ff = bcx.ins().icmp(IntCC::Equal, packed_cov, all_ff);
    bcx.ins().brif(is_all_ff, simd_fast, &[], simd_slow, &[]);

    // === simd_fast ブロック (cov=0xFF → out=0) ===
    bcx.switch_to_block(simd_fast);
    bcx.ins().store(MemFlags::new(), zero_vec, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_slow ブロック (out = dst * (256 - cov) >> 8) ===
    bcx.switch_to_block(simd_slow);
    let cov_vec = emit_expand_packed_coverage_i32x4(&mut bcx, packed_cov);
    let inv_cov_vec = bcx.ins().isub(c256_vec, cov_vec);

    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
    let (dst_a_v, dst_r_v, dst_g_v, dst_b_v) =
        emit_extract_channels_simd(&mut bcx, dst_pixels, mask_0xff_vec);

    let out_a = bcx.ins().imul(dst_a_v, inv_cov_vec);
    let out_a = bcx.ins().ushr_imm(out_a, 8);
    let out_r = bcx.ins().imul(dst_r_v, inv_cov_vec);
    let out_r = bcx.ins().ushr_imm(out_r, 8);
    let out_g = bcx.ins().imul(dst_g_v, inv_cov_vec);
    let out_g = bcx.ins().ushr_imm(out_g, 8);
    let out_b = bcx.ins().imul(dst_b_v, inv_cov_vec);
    let out_b = bcx.ins().ushr_imm(out_b, 8);

    let result = emit_pack_channels_simd(&mut bcx, out_a, out_r, out_g, out_b);
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

    let cov_u8 = bcx.ins().load(types::I8, MemFlags::new(), cur_cov, 0);
    let cov = bcx.ins().uextend(types::I32, cov_u8);
    let inv_cov = bcx.ins().isub(c256_scalar, cov);

    let dp = bcx.ins().load(types::I32, MemFlags::new(), cur_dst, 0);
    let da = bcx.ins().ushr_imm(dp, 24);
    let da = bcx.ins().band_imm(da, 0xFF);
    let dr = bcx.ins().ushr_imm(dp, 16);
    let dr = bcx.ins().band_imm(dr, 0xFF);
    let dg = bcx.ins().ushr_imm(dp, 8);
    let dg = bcx.ins().band_imm(dg, 0xFF);
    let db = bcx.ins().band_imm(dp, 0xFF);

    let oa = bcx.ins().imul(da, inv_cov);
    let oa = bcx.ins().ushr_imm(oa, 8);
    let or = bcx.ins().imul(dr, inv_cov);
    let or = bcx.ins().ushr_imm(or, 8);
    let og = bcx.ins().imul(dg, inv_cov);
    let og = bcx.ins().ushr_imm(og, 8);
    let ob = bcx.ins().imul(db, inv_cov);
    let ob = bcx.ins().ushr_imm(ob, 8);

    let result = bcx.ins().ishl_imm(oa, 24);
    let tmp = bcx.ins().ishl_imm(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlags::new(), result, cur_dst, 0);

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
    bcx.finalize();
}

/// DstCopy + カバレッジ。out = dst (何もしない)。
fn build_dst_copy_cov(mut bcx: FunctionBuilder, _ptr_type: Type) {
    let entry = bcx.create_block();
    bcx.switch_to_block(entry);
    bcx.append_block_params_for_function_params(entry);
    bcx.ins().return_(&[]);
    bcx.seal_all_blocks();
    bcx.finalize();
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
fn build_generic_compose_cov(
    mut bcx: FunctionBuilder,
    ptr_type: Type,
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
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.append_block_param(simd_loop, ptr_type);
    bcx.switch_to_block(simd_loop);
    let current_dst = bcx.block_params(simd_loop)[0];
    let current_cov = bcx.block_params(simd_loop)[1];
    let simd_i = bcx.block_params(simd_loop)[2];

    let packed_cov = bcx.ins().load(types::I32, MemFlags::new(), current_cov, 0);
    let is_all_ff = bcx.ins().icmp(IntCC::Equal, packed_cov, all_ff);
    bcx.ins().brif(is_all_ff, simd_fast, &[], simd_slow, &[]);

    // === simd_fast ブロック (cov=0xFF: src をそのまま使用) ===
    bcx.switch_to_block(simd_fast);
    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
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
    bcx.ins().store(MemFlags::new(), result, current_dst, 0);
    bcx.ins().jump(simd_next, &[]);

    // === simd_slow ブロック (cov!=0xFF: div255(src * cov) を計算して使用) ===
    bcx.switch_to_block(simd_slow);
    let cov_vec = emit_expand_packed_coverage_i32x4(&mut bcx, packed_cov);

    // cov_src_c = div255(src_c * cov) = (src_c * cov * 257 + 257) >> 16
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

    let dst_pixels = bcx
        .ins()
        .load(types::I32X4, MemFlags::new(), current_dst, 0);
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
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.append_block_param(scalar_loop, ptr_type);
    bcx.switch_to_block(scalar_loop);
    let cur_dst = bcx.block_params(scalar_loop)[0];
    let cur_cov = bcx.block_params(scalar_loop)[1];
    let scalar_i = bcx.block_params(scalar_loop)[2];

    let cov_u8 = bcx.ins().load(types::I8, MemFlags::new(), cur_cov, 0);
    let cov = bcx.ins().uextend(types::I32, cov_u8);

    // cov_src_c = div255(src_c * cov) = (src_c * cov * 257 + 257) >> 16
    let ca = bcx.ins().imul(src_a, cov);
    let ca = bcx.ins().imul(ca, c257_scalar);
    let ca = bcx.ins().iadd(ca, c257_scalar);
    let cov_sa = bcx.ins().ushr_imm(ca, 16);

    let cr = bcx.ins().imul(src_r, cov);
    let cr = bcx.ins().imul(cr, c257_scalar);
    let cr = bcx.ins().iadd(cr, c257_scalar);
    let cov_sr = bcx.ins().ushr_imm(cr, 16);

    let cg = bcx.ins().imul(src_g, cov);
    let cg = bcx.ins().imul(cg, c257_scalar);
    let cg = bcx.ins().iadd(cg, c257_scalar);
    let cov_sg = bcx.ins().ushr_imm(cg, 16);

    let cb = bcx.ins().imul(src_b, cov);
    let cb = bcx.ins().imul(cb, c257_scalar);
    let cb = bcx.ins().iadd(cb, c257_scalar);
    let cov_sb = bcx.ins().ushr_imm(cb, 16);

    let dp = bcx.ins().load(types::I32, MemFlags::new(), cur_dst, 0);
    let da = bcx.ins().ushr_imm(dp, 24);
    let da = bcx.ins().band_imm(da, 0xFF);
    let dr = bcx.ins().ushr_imm(dp, 16);
    let dr = bcx.ins().band_imm(dr, 0xFF);
    let dg = bcx.ins().ushr_imm(dp, 8);
    let dg = bcx.ins().band_imm(dg, 0xFF);
    let db = bcx.ins().band_imm(dp, 0xFF);

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

    let result = bcx.ins().ishl_imm(oa, 24);
    let tmp = bcx.ins().ishl_imm(or, 16);
    let result = bcx.ins().bor(result, tmp);
    let tmp = bcx.ins().ishl_imm(og, 8);
    let result = bcx.ins().bor(result, tmp);
    let result = bcx.ins().bor(result, ob);
    bcx.ins().store(MemFlags::new(), result, cur_dst, 0);

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
    bcx.finalize();
}

fn build_src_in_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_src_in_simd, compose_src_in_scalar);
}

fn build_src_out_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_src_out_simd, compose_src_out_scalar);
}

fn build_src_atop_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_src_atop_simd,
        compose_src_atop_scalar,
    );
}

fn build_dst_over_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_dst_over_simd,
        compose_dst_over_scalar,
    );
}

fn build_dst_in_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_dst_in_simd, compose_dst_in_scalar);
}

fn build_dst_out_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_dst_out_simd, compose_dst_out_scalar);
}

fn build_dst_atop_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_dst_atop_simd,
        compose_dst_atop_scalar,
    );
}

fn build_xor_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_xor_simd, compose_xor_scalar);
}

fn build_plus_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_plus_simd, compose_plus_scalar);
}

// =============================================================================
// ブレンドモード用ヘルパー関数
// =============================================================================

/// SrcOver アルファ: out_a = src_a + (dst_a * (256 - src_a)) >> 8
/// I32 / I32X4 両方で動作する (Cranelift の型多相性)。
fn emit_srcover_alpha(bcx: &mut FunctionBuilder, src_a: Value, dst_a: Value, c256: Value) -> Value {
    let inv_sa = bcx.ins().isub(c256, src_a);
    let t = bcx.ins().imul(dst_a, inv_sa);
    let t = bcx.ins().ushr_imm(t, 8);
    bcx.ins().iadd(src_a, t)
}

/// blend_c に端項を加算: out_c = blend_c + (src_c * inv_da) >> 8 + (dst_c * inv_sa) >> 8
fn emit_blend_with_edges(
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

fn compose_minus_simd(
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

fn compose_minus_scalar(
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

fn compose_modulate_simd(
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

fn compose_modulate_scalar(
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

fn compose_screen_simd(
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

fn compose_screen_scalar(
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

fn compose_exclusion_simd(
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

fn compose_exclusion_scalar(
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

fn compose_darken_simd(
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

fn compose_darken_scalar(
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

fn compose_lighten_simd(
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

fn compose_lighten_scalar(
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

fn compose_difference_simd(
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

fn compose_difference_scalar(
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

fn compose_multiply_simd(
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

fn compose_multiply_scalar(
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

fn compose_linear_burn_simd(
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

fn compose_linear_burn_scalar(
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
fn emit_overlay_blend_simd(
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

fn emit_overlay_blend_scalar(
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

fn compose_overlay_simd(
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

fn compose_overlay_scalar(
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

fn emit_hard_light_blend_simd(
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

fn emit_hard_light_blend_scalar(
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

fn compose_hard_light_simd(
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

fn compose_hard_light_scalar(
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

fn emit_pin_light_blend_simd(
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

fn emit_pin_light_blend_scalar(
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

fn compose_pin_light_simd(
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

fn compose_pin_light_scalar(
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

fn compose_linear_light_simd(
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

fn compose_linear_light_scalar(
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

fn emit_color_dodge_blend_f32x4(
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

fn compose_color_dodge_simd(
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

fn compose_color_dodge_scalar(
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

fn emit_color_burn_blend_f32x4(
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

fn compose_color_burn_simd(
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

fn compose_color_burn_scalar(
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

fn emit_soft_light_blend_f32x4(
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

fn compose_soft_light_simd(
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

fn compose_soft_light_scalar(
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
fn emit_soft_light_scalar_channel(
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

// =============================================================================
// ブレンドモード build 関数
// =============================================================================

fn build_minus(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_minus_simd, compose_minus_scalar);
}

fn build_modulate(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_modulate_simd,
        compose_modulate_scalar,
    );
}

fn build_multiply(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_multiply_simd,
        compose_multiply_scalar,
    );
}

fn build_screen(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_screen_simd, compose_screen_scalar);
}

fn build_overlay(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_overlay_simd, compose_overlay_scalar);
}

fn build_darken(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_darken_simd, compose_darken_scalar);
}

fn build_lighten(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(bcx, ptr_type, compose_lighten_simd, compose_lighten_scalar);
}

fn build_color_dodge(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_color_dodge_simd,
        compose_color_dodge_scalar,
    );
}

fn build_color_burn(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_color_burn_simd,
        compose_color_burn_scalar,
    );
}

fn build_linear_burn(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_linear_burn_simd,
        compose_linear_burn_scalar,
    );
}

fn build_linear_light(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_linear_light_simd,
        compose_linear_light_scalar,
    );
}

fn build_pin_light(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_pin_light_simd,
        compose_pin_light_scalar,
    );
}

fn build_hard_light(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_hard_light_simd,
        compose_hard_light_scalar,
    );
}

fn build_soft_light(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_soft_light_simd,
        compose_soft_light_scalar,
    );
}

fn build_difference(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_difference_simd,
        compose_difference_scalar,
    );
}

fn build_exclusion(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose(
        bcx,
        ptr_type,
        compose_exclusion_simd,
        compose_exclusion_scalar,
    );
}

// カバレッジ付き build 関数

fn build_minus_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_minus_simd, compose_minus_scalar);
}

fn build_modulate_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_modulate_simd,
        compose_modulate_scalar,
    );
}

fn build_multiply_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_multiply_simd,
        compose_multiply_scalar,
    );
}

fn build_screen_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_screen_simd, compose_screen_scalar);
}

fn build_overlay_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_overlay_simd, compose_overlay_scalar);
}

fn build_darken_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_darken_simd, compose_darken_scalar);
}

fn build_lighten_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(bcx, ptr_type, compose_lighten_simd, compose_lighten_scalar);
}

fn build_color_dodge_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_color_dodge_simd,
        compose_color_dodge_scalar,
    );
}

fn build_color_burn_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_color_burn_simd,
        compose_color_burn_scalar,
    );
}

fn build_linear_burn_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_linear_burn_simd,
        compose_linear_burn_scalar,
    );
}

fn build_linear_light_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_linear_light_simd,
        compose_linear_light_scalar,
    );
}

fn build_pin_light_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_pin_light_simd,
        compose_pin_light_scalar,
    );
}

fn build_hard_light_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_hard_light_simd,
        compose_hard_light_scalar,
    );
}

fn build_soft_light_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_soft_light_simd,
        compose_soft_light_scalar,
    );
}

fn build_difference_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_difference_simd,
        compose_difference_scalar,
    );
}

fn build_exclusion_cov(bcx: FunctionBuilder, ptr_type: Type) {
    build_generic_compose_cov(
        bcx,
        ptr_type,
        compose_exclusion_simd,
        compose_exclusion_scalar,
    );
}

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

fn build_transform_edges(mut bcx: FunctionBuilder, ptr_type: Type) {
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

    let mem = MemFlags::trusted();

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
