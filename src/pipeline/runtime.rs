use crate::api::style::{CompOp, FillRule};

use super::cache::{
    PipelineBoxFn, PipelineCache, PipelineCovFn, PipelineFn, SweepFn, TransformEdgesFn,
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
}

impl PipelineRuntime {
    pub fn new() -> Self {
        Self {
            cache: PipelineCache::new(),
            compiler: PipelineCompiler::new(),
            sweep_non_zero: None,
            sweep_even_odd: None,
            transform_edges_fn: None,
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
        let key = PipelineKey::new(dst_format, comp_op, fill_type, fetch_type);
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
        let key = PipelineKey::new(dst_format, comp_op, FillType::Mask, fetch_type);
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
        if !matches!(comp_op, CompOp::SrcOver | CompOp::SrcCopy) {
            return None;
        }
        let key = PipelineKey::new(dst_format, comp_op, FillType::BoxA, fetch_type);
        match self.cache.get_box(&key) {
            Some(f) => Some(f),
            None => {
                let f = self.compiler.compile_box(&key, comp_op);
                self.cache.insert_box(key, f);
                Some(f)
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

    /// JIT sweep 関数を取得またはコンパイルする。
    ///
    /// FillRule ごとに別関数をキャッシュする。
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
