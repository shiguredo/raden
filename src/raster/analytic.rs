use crate::api::style::FillRule;
use crate::pipeline::cache::SweepFn;

/// Analytic Rasterizer。セルバッファに area-cover パック値を蓄積し、
/// prefix sum sweep でカバレッジマスクを生成する。
///
/// ## セルエンコーディング (Blend2D / AGG 方式)
///
/// 1 セルあたり i32 1 個に cover と area をパックして格納する:
///
/// ```text
/// cells[x]   += cover * 512 - area
/// cells[x+1] += area
/// ```
///
/// - `cover` = エッジの Y 方向通過量 × 256 × direction
/// - `area`  = (fx_start + fx_end) × dy × 65536 × direction
///
/// sweep 時は `min(abs(sar(accumulated, 9)), 255)` で 0-255 カバレッジに変換する。
pub struct AnalyticRasterizer {
    /// area-cover パック値のセルバッファ。幅 + 1 要素 (area スピルオーバー用)。
    cells: Vec<i32>,
    /// sweep 結果のカバレッジマスク。
    cov_buf: Vec<u8>,
    /// Y ソート用バッファ。呼び出し間で再利用する。
    edge_sort_buf: Vec<(f64, f64, f64, f64)>,
}

impl AnalyticRasterizer {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            cov_buf: Vec::new(),
            edge_sort_buf: Vec::new(),
        }
    }

    /// エッジ群をラスタライズし、スキャンラインごとにコールバックを呼ぶ。
    ///
    /// コールバック `f(y, x_start, coverage_slice)` は非ゼロカバレッジ区間に対して呼ばれる。
    /// `sweep_fn` は JIT コンパイル済みの sweep 関数。
    #[allow(clippy::too_many_arguments)]
    pub fn rasterize<F>(
        &mut self,
        edges: &[(f64, f64, f64, f64)],
        clip_x0: i32,
        clip_y0: i32,
        clip_x1: i32,
        clip_y1: i32,
        sweep_fn: SweepFn,
        mut f: F,
    ) where
        F: FnMut(i32, i32, &[u8]),
    {
        let width = (clip_x1 - clip_x0) as usize;
        if width == 0 {
            return;
        }

        // cells[x+1] 書き込みのため 1 要素余分に確保する
        self.cells.resize(width + 1, 0);
        self.cov_buf.resize(width, 0);

        // エッジを Y の最小値でソートする
        let mut sorted = std::mem::take(&mut self.edge_sort_buf);
        sorted.clear();
        sorted.extend_from_slice(edges);
        sort_edges_by_y_min(&mut sorted);

        // 完了済みエッジをスキップするインデックス
        let mut next_edge_idx: usize = 0;

        let mut ctx = ScanlineCtx {
            top: 0.0,
            bottom: 0.0,
            clip_x0,
            dirty_min: width + 1,
            dirty_max: 0,
            right_clipped: false,
        };

        for y in clip_y0..clip_y1 {
            ctx.top = y as f64;
            ctx.bottom = (y + 1) as f64;

            // 前スキャンラインで書き込んだセルだけクリアする
            if ctx.dirty_min < ctx.dirty_max {
                self.cells[ctx.dirty_min..ctx.dirty_max].fill(0);
            }
            ctx.dirty_min = width + 1;
            ctx.dirty_max = 0;
            ctx.right_clipped = false;

            // scanline_top 以下で完了しているエッジをスキップする
            while next_edge_idx < sorted.len() {
                let (_, y0, _, y1) = sorted[next_edge_idx];
                let y_max = y0.max(y1);
                if y_max > ctx.top {
                    break;
                }
                next_edge_idx += 1;
            }

            // next_edge_idx からイテレートし、y_min >= scanline_bottom で打ち切る
            for &(x0, y0, x1, y1) in &sorted[next_edge_idx..] {
                let y_min = y0.min(y1);
                if y_min >= ctx.bottom {
                    break;
                }
                add_edge(&mut self.cells, x0, y0, x1, y1, &mut ctx);
            }

            // sweep してカバレッジマスクを生成する。
            //
            // dirty_min..dirty_max はセルバッファ内で非ゼロ delta が書かれた範囲。
            // sweep は可視ピクセル範囲 (0..width) に限定する。
            // dirty_max は cells[x+1] 書き込みにより width+1 に達し得るが、
            // sweep_max は width を超えない。
            //
            // ただし、エッジが画像右端でクリップされた場合 (right_clipped=true)、
            // 閉じる側の delta が失われるため prefix sum が dirty_max を超えても
            // 非ゼロのまま。この場合は sweep_max を width まで拡張する。
            let sweep_dirty_min = ctx.dirty_min.min(width);
            if sweep_dirty_min < ctx.dirty_max {
                let sweep_max = if ctx.right_clipped {
                    width
                } else {
                    ctx.dirty_max.min(width)
                };
                if sweep_dirty_min < sweep_max {
                    let sweep_len = sweep_max - sweep_dirty_min;

                    // JIT sweep: prefix sum + sar(9) + abs + clamp(255) を計算する
                    unsafe {
                        sweep_fn(
                            self.cells[sweep_dirty_min..].as_ptr(),
                            self.cov_buf[sweep_dirty_min..].as_mut_ptr(),
                            sweep_len,
                        );
                    }

                    // 非ゼロ範囲を検出する
                    let sweep_slice = &self.cov_buf[sweep_dirty_min..sweep_max];
                    if let Some(first) = sweep_slice.iter().position(|&c| c != 0) {
                        let last = sweep_slice.iter().rposition(|&c| c != 0).unwrap();
                        let start = sweep_dirty_min + first;
                        let end = sweep_dirty_min + last + 1;
                        f(y, clip_x0 + start as i32, &self.cov_buf[start..end]);
                    }
                }
            }
        }

        self.edge_sort_buf = sorted;
    }
}

impl Default for AnalyticRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

/// エッジを min(y0, y1) の昇順でソートする。
fn sort_edges_by_y_min(edges: &mut [(f64, f64, f64, f64)]) {
    edges.sort_unstable_by(|a, b| {
        let a_y_min = a.1.min(a.3);
        let b_y_min = b.1.min(b.3);
        debug_assert!(!a_y_min.is_nan(), "エッジの Y 座標が NaN");
        debug_assert!(!b_y_min.is_nan(), "エッジの Y 座標が NaN");
        a_y_min.total_cmp(&b_y_min)
    });
}

/// スキャンライン処理に必要なコンテキスト。
struct ScanlineCtx {
    top: f64,
    bottom: f64,
    clip_x0: i32,
    /// dirty 範囲の下限 (書き込んだセルの最小インデックス)。
    dirty_min: usize,
    /// dirty 範囲の上限 (書き込んだセルの最大インデックス + 1)。
    /// cells[x+1] 書き込みにより width+1 に達し得る。
    dirty_max: usize,
    /// エッジの X 座標がセルバッファの右端を超えた場合に true。
    /// 閉じる側の delta が書き込まれないため、prefix sum が 0 に戻らない。
    /// この場合 sweep 範囲を width まで拡張する必要がある。
    right_clipped: bool,
}

/// セルバッファに area-cover パック値を書き込む。
///
/// idx が範囲外の場合:
/// - idx < 0: 左クリップ。cover 寄与を cells[0] に繰り越す。
/// - idx >= cells.len(): 右クリップ。right_clipped フラグを立てる。
fn write_cell(cells: &mut [i32], idx: isize, value: i32, ctx: &mut ScanlineCtx) {
    if idx >= 0 && (idx as usize) < cells.len() {
        let i = idx as usize;
        cells[i] += value;
        if i < ctx.dirty_min {
            ctx.dirty_min = i;
        }
        if i + 1 > ctx.dirty_max {
            ctx.dirty_max = i + 1;
        }
    } else if idx < 0 {
        // 左クリップ: 値を cells[0] に繰り越す
        cells[0] += value;
        if 0 < ctx.dirty_min {
            ctx.dirty_min = 0;
        }
        if 1 > ctx.dirty_max {
            ctx.dirty_max = 1;
        }
    } else {
        ctx.right_clipped = true;
    }
}

/// 1 セル分の area-cover パック値を計算してセルバッファに書き込む。
///
/// `x_start`, `x_end` はセル内のエッジ端点 (ピクセル座標)。
/// `y_start`, `y_end` はスキャンライン内のクリップ済み Y 座標。
#[allow(clippy::too_many_arguments)]
fn emit_cell(
    cells: &mut [i32],
    cell_x: i32,
    x_start: f64,
    x_end: f64,
    y_start: f64,
    y_end: f64,
    direction: f64,
    ctx: &mut ScanlineCtx,
) {
    let fx_start = x_start - cell_x as f64;
    let fx_end = x_end - cell_x as f64;
    let dy = y_end - y_start;

    // cover: 整数累積方式で計算する。
    // (dy * 256.0) as i32 をセルごとに独立計算すると切り捨て誤差が蓄積し、
    // 複数セルにまたがるエッジの cover 合計が正確な値より小さくなる。
    // y_start, y_end を個別に量子化してその差を取ることで、
    // セル分割した cover の合計が常にエッジ全体の cover と一致する。
    let direction_sign = if direction > 0.0 { 1i32 } else { -1i32 };
    let cover = ((y_end * 256.0) as i32 - (y_start * 256.0) as i32) * direction_sign;
    let area = ((fx_start + fx_end) * dy * 65536.0 * direction) as i32;

    let delta = cover * 512 - area;
    let idx = (cell_x - ctx.clip_x0) as isize;

    write_cell(cells, idx, delta, ctx);
    write_cell(cells, idx + 1, area, ctx);
}

/// 1 本の線分がスキャンラインと交差する部分の area-cover パック値をセルバッファに書き込む。
///
/// エッジが複数セルをまたぐ場合、整数 X 境界で分割して各セルに emit_cell を呼ぶ。
fn add_edge(cells: &mut [i32], x0: f64, y0: f64, x1: f64, y1: f64, ctx: &mut ScanlineCtx) {
    // 水平線はスキップ (cover に寄与しない)
    if y0 == y1 {
        return;
    }

    // Y 方向にクリップ
    let (ey0, ey1) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
    if ey1 <= ctx.top || ey0 >= ctx.bottom {
        return;
    }

    // 方向: 下向き (y0 < y1) = +1, 上向き (y0 > y1) = -1
    let direction: f64 = if y0 < y1 { 1.0 } else { -1.0 };

    // このスキャンラインでクリップされた Y 範囲
    let clipped_top = ey0.max(ctx.top);
    let clipped_bottom = ey1.min(ctx.bottom);

    // クリップ後の X 座標を計算する。
    // y0 != y1 は上で確認済みなので dy_total は非ゼロ。
    // 2 回の除算を 1 回の除算 + 2 回の乗算に置換する。
    let dy_total = y1 - y0;
    let dx_total = x1 - x0;
    let inv_dy_total = 1.0 / dy_total;
    let t_top = (clipped_top - y0) * inv_dy_total;
    let t_bot = (clipped_bottom - y0) * inv_dy_total;
    let x_at_top = x0 + t_top * dx_total;
    let x_at_bot = x0 + t_bot * dx_total;

    // 単一セル判定
    let ex_top = x_at_top.floor() as i32;
    let ex_bot = x_at_bot.floor() as i32;

    if ex_top == ex_bot {
        // 単一セル: cover と area を計算して書き込み
        emit_cell(
            cells,
            ex_top,
            x_at_top,
            x_at_bot,
            clipped_top,
            clipped_bottom,
            direction,
            ctx,
        );
        return;
    }

    // 複数セル: 整数 X 境界で分割
    let dx = x_at_bot - x_at_top;
    let dy = clipped_bottom - clipped_top;
    let slope_y_per_x = if dx.abs() > 1e-12 { dy / dx } else { 0.0 };

    let mut x_cur = x_at_top;
    let mut y_cur = clipped_top;

    if dx > 0.0 {
        // 右方向に走査
        for cell_x in ex_top..=ex_bot {
            let x_next = if cell_x == ex_bot {
                x_at_bot
            } else {
                (cell_x + 1) as f64
            };
            let y_next = clipped_top + (x_next - x_at_top) * slope_y_per_x;

            emit_cell(cells, cell_x, x_cur, x_next, y_cur, y_next, direction, ctx);

            x_cur = x_next;
            y_cur = y_next;
        }
    } else {
        // 左方向に走査
        for cell_x in (ex_bot..=ex_top).rev() {
            let x_next = if cell_x == ex_bot {
                x_at_bot
            } else {
                cell_x as f64
            };
            let y_next = clipped_top + (x_next - x_at_top) * slope_y_per_x;

            emit_cell(cells, cell_x, x_cur, x_next, y_cur, y_next, direction, ctx);

            x_cur = x_next;
            y_cur = y_next;
        }
    }
}

/// dirty range に限定した prefix sum でカバレッジマスクを生成する (リファレンス実装)。
///
/// area-cover パック値を prefix sum で累積し、算術右シフト 9 で 256 スケールに
/// 変換してから fill_rule に応じた変換で 0-255 カバレッジにする。
///
/// PBT テストで JIT sweep との出力一致を検証するために残す。
///
/// 非ゼロ範囲の (start, end) を返す。全てゼロなら None。
pub fn sweep_reference(
    cells: &[i32],
    cov_buf: &mut [u8],
    dirty_min: usize,
    dirty_max: usize,
    fill_rule: FillRule,
) -> Option<(usize, usize)> {
    let mut cover: i32 = 0;
    let mut start = dirty_max; // 番兵値: dirty_max はこの関数の有効範囲上限
    let mut end = 0usize;

    for i in dirty_min..dirty_max {
        cover += cells[i];
        let shifted = (cover >> 9) as i64;
        let cov = match fill_rule {
            FillRule::NonZero => shifted.unsigned_abs().min(255) as u8,
            FillRule::EvenOdd => {
                // 2 winding 周期 (= 512) で折り返す
                let val = shifted.unsigned_abs() & 511;
                let folded = 512 - val;
                val.min(folded).min(255) as u8
            }
        };
        cov_buf[i] = cov;
        if cov > 0 {
            if start == dirty_max {
                start = i;
            }
            end = i + 1;
        }
    }

    if start < end {
        Some((start, end))
    } else {
        None
    }
}
