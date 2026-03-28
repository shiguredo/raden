use std::collections::HashMap;

use super::key::PipelineKey;

/// JIT コンパイル済みパイプライン関数のシグネチャ。
///
/// - `dst`: スキャンライン先頭ポインタ
/// - `src_solid`: premultiplied ARGB32 カラー
/// - `count`: 処理ピクセル数
pub type PipelineFn = unsafe extern "C" fn(dst: *mut u8, src_solid: u32, count: usize);

/// カバレッジ付きパイプライン関数のシグネチャ。
///
/// - `dst`: スキャンライン先頭ポインタ
/// - `src_solid`: premultiplied ARGB32 カラー
/// - `count`: 処理ピクセル数
/// - `coverage`: ピクセルごとのカバレッジ値 (0-255) の配列ポインタ
pub type PipelineCovFn =
    unsafe extern "C" fn(dst: *mut u8, src_solid: u32, count: usize, coverage: *const u8);

/// JIT コンパイル済みエッジ座標変換関数のシグネチャ。
///
/// エッジ配列の各 (x0, y0, x1, y1) を 2D アフィン変換行列で一括変換する。
/// F64X2 SIMD で 2 点ずつ処理する。
///
/// - `edges`: (f64, f64, f64, f64) タプル配列のポインタ (各エッジ 32 バイト)
/// - `count`: エッジ数
/// - `m00..m21`: 行列係数
pub type TransformEdgesFn = unsafe extern "C" fn(
    edges: *mut f64,
    count: usize,
    m00: f64,
    m01: f64,
    m10: f64,
    m11: f64,
    m20: f64,
    m21: f64,
);

/// 矩形塗りつぶし専用パイプライン関数のシグネチャ。
///
/// y ループを JIT 内に含み、scanline ごとの関数呼び出しオーバーヘッドを排除する。
/// ループ不変値 (splat 済みベクタ等) は関数内で 1 回だけ計算される。
///
/// - `dst`: 矩形左上ピクセルのポインタ
/// - `src_solid`: premultiplied ARGB32 カラー
/// - `width`: 矩形の幅 (ピクセル数)
/// - `height`: 矩形の高さ (スキャンライン数)
/// - `stride`: スキャンライン間のバイトストライド
pub type PipelineBoxFn =
    unsafe extern "C" fn(dst: *mut u8, src_solid: u32, width: usize, height: usize, stride: usize);

/// JIT コンパイル済み sweep 関数のシグネチャ。
///
/// area-cover パック値の prefix sum を計算し、算術右シフト 9 + abs + clamp(255) で
/// 0-255 カバレッジマスクを生成する。読み取ったセルは 0 にクリアされる。
///
/// - `cells`: area-cover パック値配列の先頭ポインタ (読み取り後にゼロクリアされる)
/// - `cov_buf`: カバレッジ出力バッファの先頭ポインタ
/// - `len`: 処理する要素数
pub type SweepFn = unsafe extern "C" fn(cells: *mut i32, cov_buf: *mut u8, len: usize);

/// JIT コンパイル済みスパンパイプライン関数のシグネチャ。
///
/// グラデーション等のピクセルごとに色が異なるソースを合成する。
/// `src_solid` の代わりにソース色の配列ポインタを受け取る。
///
/// - `dst`: スキャンライン先頭ポインタ
/// - `src_span`: ピクセルごとの premultiplied ARGB32 色配列ポインタ (count 要素)
/// - `count`: 処理ピクセル数
pub type PipelineSpanFn = unsafe extern "C" fn(dst: *mut u8, src_span: *const u32, count: usize);

/// カバレッジ付きスパンパイプライン関数のシグネチャ。
///
/// - `dst`: スキャンライン先頭ポインタ
/// - `src_span`: ピクセルごとの premultiplied ARGB32 色配列ポインタ (count 要素)
/// - `count`: 処理ピクセル数
/// - `coverage`: ピクセルごとのカバレッジ値 (0-255) の配列ポインタ
pub type PipelineSpanCovFn =
    unsafe extern "C" fn(dst: *mut u8, src_span: *const u32, count: usize, coverage: *const u8);

/// Radial グラデーション行描画 JIT 関数のシグネチャ。
///
/// F32X4 SIMD で 4 ピクセル分の sqrt を並列実行する。
pub type RadialGradientRowFn = unsafe extern "C" fn(
    dst_row: *mut u32,
    lut: *const u32,
    width: usize,
    ux_start: f32,
    uy_start: f32,
    cx: f32,
    cy: f32,
    r0: f32,
    inv_r_diff_max: f32,
    dux_dx: f32,
    duy_dx: f32,
);

/// コンパイル済みパイプライン関数のキャッシュ。
#[derive(Default)]
pub struct PipelineCache {
    map: HashMap<PipelineKey, PipelineFn>,
    cov_map: HashMap<PipelineKey, PipelineCovFn>,
    box_map: HashMap<PipelineKey, PipelineBoxFn>,
    span_map: HashMap<PipelineKey, PipelineSpanFn>,
    span_cov_map: HashMap<PipelineKey, PipelineSpanCovFn>,
}

impl PipelineCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &PipelineKey) -> Option<PipelineFn> {
        self.map.get(key).copied()
    }

    pub fn insert(&mut self, key: PipelineKey, func: PipelineFn) {
        self.map.insert(key, func);
    }

    pub fn get_cov(&self, key: &PipelineKey) -> Option<PipelineCovFn> {
        self.cov_map.get(key).copied()
    }

    pub fn insert_cov(&mut self, key: PipelineKey, func: PipelineCovFn) {
        self.cov_map.insert(key, func);
    }

    pub fn get_box(&self, key: &PipelineKey) -> Option<PipelineBoxFn> {
        self.box_map.get(key).copied()
    }

    pub fn insert_box(&mut self, key: PipelineKey, func: PipelineBoxFn) {
        self.box_map.insert(key, func);
    }

    pub fn get_span(&self, key: &PipelineKey) -> Option<PipelineSpanFn> {
        self.span_map.get(key).copied()
    }

    pub fn insert_span(&mut self, key: PipelineKey, func: PipelineSpanFn) {
        self.span_map.insert(key, func);
    }

    pub fn get_span_cov(&self, key: &PipelineKey) -> Option<PipelineSpanCovFn> {
        self.span_cov_map.get(key).copied()
    }

    pub fn insert_span_cov(&mut self, key: PipelineKey, func: PipelineSpanCovFn) {
        self.span_cov_map.insert(key, func);
    }
}
