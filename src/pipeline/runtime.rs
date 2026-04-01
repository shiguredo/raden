use crate::api::style::{CompOp, FillRule};

use super::a8;
use super::cache::{
    LinearGradientCovFn, PipelineBoxFn, PipelineCache, PipelineCovFn, PipelineFn,
    PipelineSpanCovFn, PipelineSpanFn, RadialGradientRowFn, SweepFn, TransformEdgesFn,
};
use super::compiler::PipelineCompiler;
use super::key::{FetchType, FillType, PipelineKey};
use crate::pixel::PixelFormat;

/// JIT コンパイル済みパイプラインのキャッシュとコンパイラを統合した実行環境。
///
/// フレームをまたいで再利用することで、毎フレームの JIT コンパイルを回避する。
pub struct PipelineRuntime {
    cache: PipelineCache,
    compiler: PipelineCompiler,
    sweep_non_zero: Option<SweepFn>,
    sweep_even_odd: Option<SweepFn>,
    transform_edges_fn: Option<TransformEdgesFn>,
    radial_row_fn: Option<RadialGradientRowFn>,
    linear_cov_fn: Option<LinearGradientCovFn>,
}

impl PipelineRuntime {
    pub fn new() -> Self {
        Self {
            cache: PipelineCache::new(),
            compiler: PipelineCompiler::new(),
            sweep_non_zero: None,
            sweep_even_odd: None,
            transform_edges_fn: None,
            radial_row_fn: None,
            linear_cov_fn: None,
        }
    }

    /// キャッシュにあればそれを返し、なければコンパイルしてキャッシュに入れて返す。
    pub fn get_or_compile(
        &mut self,
        dst_format: PixelFormat,
        comp_op: CompOp,
        fill_type: FillType,
        fetch_type: FetchType,
    ) -> PipelineFn {
        if dst_format == PixelFormat::A8 {
            return a8::pipeline_fn(comp_op);
        }
        let key_fmt = a8::jit_dst_format(dst_format);
        let key = PipelineKey::new(key_fmt, comp_op, fill_type, fetch_type);
        match self.cache.get(&key) {
            Some(f) => f,
            None => {
                let f = self.compiler.compile(&key, comp_op);
                self.cache.insert(key, f);
                f
            }
        }
    }

    /// カバレッジ付きパイプライン関数を取得またはコンパイルする。
    pub fn get_or_compile_cov(
        &mut self,
        dst_format: PixelFormat,
        comp_op: CompOp,
        fetch_type: FetchType,
    ) -> PipelineCovFn {
        if dst_format == PixelFormat::A8 {
            return a8::pipeline_cov_fn(comp_op);
        }
        let key_fmt = a8::jit_dst_format(dst_format);
        let key = PipelineKey::new(key_fmt, comp_op, FillType::Mask, fetch_type);
        match self.cache.get_cov(&key) {
            Some(f) => f,
            None => {
                let f = self.compiler.compile_cov(&key, comp_op);
                self.cache.insert_cov(key, f);
                f
            }
        }
    }

    /// 矩形塗りつぶし専用パイプライン関数を取得またはコンパイルする。
    ///
    /// y ループを JIT 内に含む専用シグネチャの関数を返す。
    /// SrcOver / SrcCopy のみサポートし、それ以外は None を返す。
    pub fn get_or_compile_box(
        &mut self,
        dst_format: PixelFormat,
        comp_op: CompOp,
        fetch_type: FetchType,
    ) -> Option<PipelineBoxFn> {
        if dst_format == PixelFormat::A8 {
            return a8::pipeline_box_fn(comp_op);
        }
        if !matches!(comp_op, CompOp::SrcOver | CompOp::SrcCopy) {
            return None;
        }
        let key_fmt = a8::jit_dst_format(dst_format);
        let key = PipelineKey::new(key_fmt, comp_op, FillType::BoxA, fetch_type);
        match self.cache.get_box(&key) {
            Some(f) => Some(f),
            None => {
                let f = self.compiler.compile_box(&key, comp_op);
                self.cache.insert_box(key, f);
                Some(f)
            }
        }
    }

    /// スパンパイプライン関数を取得またはコンパイルする (カバレッジなし)。
    pub fn get_or_compile_span(
        &mut self,
        dst_format: PixelFormat,
        comp_op: CompOp,
        fetch_type: FetchType,
    ) -> PipelineSpanFn {
        assert_ne!(
            dst_format,
            PixelFormat::A8,
            "span pipeline is not supported for A8 destination"
        );
        let key_fmt = a8::jit_dst_format(dst_format);
        let key = PipelineKey::new(key_fmt, comp_op, FillType::BoxA, fetch_type);
        match self.cache.get_span(&key) {
            Some(f) => f,
            None => {
                let f = self.compiler.compile_span(&key, comp_op);
                self.cache.insert_span(key, f);
                f
            }
        }
    }

    /// カバレッジ付きスパンパイプライン関数を取得またはコンパイルする。
    pub fn get_or_compile_span_cov(
        &mut self,
        dst_format: PixelFormat,
        comp_op: CompOp,
        fetch_type: FetchType,
    ) -> PipelineSpanCovFn {
        assert_ne!(
            dst_format,
            PixelFormat::A8,
            "span_cov pipeline is not supported for A8 destination"
        );
        let key_fmt = a8::jit_dst_format(dst_format);
        let key = PipelineKey::new(key_fmt, comp_op, FillType::Mask, fetch_type);
        match self.cache.get_span_cov(&key) {
            Some(f) => f,
            None => {
                let f = self.compiler.compile_span_cov(&key, comp_op);
                self.cache.insert_span_cov(key, f);
                f
            }
        }
    }

    /// JIT エッジ座標変換関数を取得またはコンパイルする。
    ///
    /// 行列係数はパラメータ渡しなのでキャッシュは 1 関数のみ。
    pub fn get_or_compile_transform_edges(&mut self) -> TransformEdgesFn {
        match self.transform_edges_fn {
            Some(f) => f,
            None => {
                let f = self.compiler.compile_transform_edges();
                self.transform_edges_fn = Some(f);
                f
            }
        }
    }

    /// Linear グラデーション + カバレッジ融合 JIT 関数を取得またはコンパイルする。
    pub fn get_or_compile_linear_gradient_cov(&mut self) -> LinearGradientCovFn {
        match self.linear_cov_fn {
            Some(f) => f,
            None => {
                let f = self.compiler.compile_linear_gradient_cov();
                self.linear_cov_fn = Some(f);
                f
            }
        }
    }

    /// Radial グラデーション行描画の JIT 関数を取得またはコンパイルする。
    pub fn get_or_compile_radial_row(&mut self) -> RadialGradientRowFn {
        match self.radial_row_fn {
            Some(f) => f,
            None => {
                let f = self.compiler.compile_radial_row();
                self.radial_row_fn = Some(f);
                f
            }
        }
    }

    pub fn get_or_compile_sweep(&mut self, fill_rule: FillRule) -> SweepFn {
        let slot = match fill_rule {
            FillRule::NonZero => &mut self.sweep_non_zero,
            FillRule::EvenOdd => &mut self.sweep_even_odd,
        };
        match *slot {
            Some(f) => f,
            None => {
                let f = self.compiler.compile_sweep(fill_rule);
                *slot = Some(f);
                f
            }
        }
    }
}

impl Default for PipelineRuntime {
    fn default() -> Self {
        Self::new()
    }
}
