/// グリフアウトライン → Path 変換。
///
/// glyf テーブルからグリフデータを読み出し、TrueType の 2 次ベジェアウトラインを
/// Path に変換する。Simple Glyph と Compound Glyph の両方に対応。
use crate::api::path::Path;
use crate::font::FontError;
use crate::font::tables::{ParsedTables, be_i8, be_i16, be_u8, be_u16};

/// Compound Glyph の最大再帰深度。
const MAX_COMPOUND_DEPTH: u32 = 16;

// glyf フラグビット
const ON_CURVE: u8 = 0x01;
const X_SHORT: u8 = 0x02;
const Y_SHORT: u8 = 0x04;
const REPEAT_FLAG: u8 = 0x08;
const X_SAME_OR_POS: u8 = 0x10;
const Y_SAME_OR_POS: u8 = 0x20;

// Compound フラグビット
const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
const ARGS_ARE_XY_VALUES: u16 = 0x0002;
const WE_HAVE_A_SCALE: u16 = 0x0008;
const MORE_COMPONENTS: u16 = 0x0020;
const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;

/// design units → pixel 変換パラメータ。
#[derive(Clone, Copy)]
struct GlyphTransform {
    offset_x: f64,
    offset_y: f64,
    scale: f64,
    m00: f64,
    m01: f64,
    m10: f64,
    m11: f64,
}

impl GlyphTransform {
    fn new(offset_x: f64, offset_y: f64, scale: f64) -> Self {
        Self {
            offset_x,
            offset_y,
            scale,
            m00: 1.0,
            m01: 0.0,
            m10: 0.0,
            m11: 1.0,
        }
    }

    /// design units 座標をピクセル座標に変換する。
    #[inline]
    fn apply(&self, x_design: i32, y_design: i32) -> (f64, f64) {
        let xd = x_design as f64;
        let yd = y_design as f64;
        let tx = xd * self.m00 + yd * self.m01;
        let ty = xd * self.m10 + yd * self.m11;
        (
            tx * self.scale + self.offset_x,
            -ty * self.scale + self.offset_y,
        )
    }
}

/// グリフアウトラインを Path に追加する。
pub(crate) fn append_glyph_outline(
    glyph_id: u16,
    offset_x: f64,
    offset_y: f64,
    scale: f64,
    tables: &ParsedTables,
    data: &[u8],
    path: &mut Path,
) -> Result<(), FontError> {
    let xform = GlyphTransform::new(offset_x, offset_y, scale);
    append_glyph_recursive(glyph_id, xform, tables, data, path, 0)
}

/// glyf テーブルから glyph_id のグリフデータスライスを取得する。
///
/// 戻り値:
/// - `Ok(Some(&[u8]))`: アウトライン入りグリフのスライス ( 10 バイト以上のヘッダを含む ) 。
/// - `Ok(None)`: 空グリフ ( `glyf_start == glyf_end` ) 。
/// - `Err(_)`: glyph_id 範囲外 / glyf 範囲外 / ヘッダ不足 ( `glyph_data.len() < 10` ) 。
pub(crate) fn glyph_entry_slice<'a>(
    glyph_id: u16,
    tables: &ParsedTables,
    data: &'a [u8],
) -> Result<Option<&'a [u8]>, FontError> {
    let gid = glyph_id as usize;
    if gid + 1 >= tables.loca_offsets.len() {
        return Err(FontError::InvalidData("glyph id out of range"));
    }

    let glyf_start = tables.loca_offsets[gid];
    let glyf_end = tables.loca_offsets[gid + 1];

    // 空グリフ (スペース等) は loca offset が等しい。
    if glyf_start == glyf_end {
        return Ok(None);
    }

    let abs_start = tables.glyf_offset as usize + glyf_start as usize;
    let abs_end = tables.glyf_offset as usize + glyf_end as usize;

    if abs_end > data.len() || abs_start >= abs_end {
        return Err(FontError::InvalidData("glyph data out of range"));
    }

    let glyph_data = &data[abs_start..abs_end];

    if glyph_data.len() < 10 {
        return Err(FontError::InvalidData("glyph header too short"));
    }

    Ok(Some(glyph_data))
}

/// グリフヘッダ 10 バイトから bbox を読み出す。
///
/// 戻り値: `(x_min, y_min, x_max, y_max)` の i16 4 値。
/// Compound Glyph (`number_of_contours < 0`) で bbox が `(0, 0, 0, 0)` のときは
/// `None` を返す (未計算のまま埋められた違反フォントを有効値と区別できないため)。
/// Simple Glyph では `(0, 0, 0, 0)` も有効値として `Some` を返す。
/// `glyph_data` の長さは `glyph_entry_slice` が 10 バイト以上を保証する。
pub(crate) fn glyph_bbox_raw(glyph_data: &[u8]) -> Option<(i16, i16, i16, i16)> {
    let number_of_contours = i16::from_be_bytes([glyph_data[0], glyph_data[1]]);
    let x_min = i16::from_be_bytes([glyph_data[2], glyph_data[3]]);
    let y_min = i16::from_be_bytes([glyph_data[4], glyph_data[5]]);
    let x_max = i16::from_be_bytes([glyph_data[6], glyph_data[7]]);
    let y_max = i16::from_be_bytes([glyph_data[8], glyph_data[9]]);
    if number_of_contours < 0 && x_min == 0 && y_min == 0 && x_max == 0 && y_max == 0 {
        return None;
    }
    Some((x_min, y_min, x_max, y_max))
}

/// 再帰的にグリフアウトラインを追加する (Compound 対応)。
fn append_glyph_recursive(
    glyph_id: u16,
    xform: GlyphTransform,
    tables: &ParsedTables,
    data: &[u8],
    path: &mut Path,
    depth: u32,
) -> Result<(), FontError> {
    if depth > MAX_COMPOUND_DEPTH {
        return Err(FontError::InvalidData("compound glyph recursion too deep"));
    }

    // glyph_id 範囲外 / glyf 範囲外 / ヘッダ不足のチェックは glyph_entry_slice 側に集約する。
    let glyph_data = match glyph_entry_slice(glyph_id, tables, data)? {
        Some(slice) => slice,
        // 空グリフ (スペース等) はアウトラインを追加せず Ok(()) を返す。
        None => return Ok(()),
    };

    let number_of_contours = be_i16(glyph_data, 0)?;

    if number_of_contours >= 0 {
        parse_simple_glyph(glyph_data, number_of_contours as usize, xform, path)
    } else {
        parse_compound_glyph(glyph_data, xform, tables, data, path, depth)
    }
}

/// Simple Glyph をパースして Path に変換する。
fn parse_simple_glyph(
    glyph_data: &[u8],
    number_of_contours: usize,
    xform: GlyphTransform,
    path: &mut Path,
) -> Result<(), FontError> {
    if number_of_contours == 0 {
        return Ok(());
    }

    // ヘッダ: number_of_contours(2) + xMin(2) + yMin(2) + xMax(2) + yMax(2) = 10 bytes
    let end_pts_off = 10;
    let end_pts_end = end_pts_off + number_of_contours * 2;

    if end_pts_end + 2 > glyph_data.len() {
        return Err(FontError::InvalidData(
            "simple glyph contour data too short",
        ));
    }

    // end_pts_of_contours を読み出す
    let mut end_pts = Vec::with_capacity(number_of_contours);
    for i in 0..number_of_contours {
        end_pts.push(be_u16(glyph_data, end_pts_off + i * 2)? as usize);
    }

    let num_points = end_pts.last().map_or(0, |&e| e + 1);
    if num_points == 0 {
        return Ok(());
    }

    // instruction_length + instructions をスキップ
    let instr_len = be_u16(glyph_data, end_pts_end)? as usize;
    let flags_off = end_pts_end + 2 + instr_len;

    if flags_off > glyph_data.len() {
        return Err(FontError::InvalidData(
            "simple glyph instructions exceed data",
        ));
    }

    // フラグのデコード (repeat 対応)
    let mut flags = Vec::with_capacity(num_points);
    let mut cursor = flags_off;
    while flags.len() < num_points {
        if cursor >= glyph_data.len() {
            return Err(FontError::InvalidData("simple glyph flags truncated"));
        }
        let flag = glyph_data[cursor];
        cursor += 1;
        flags.push(flag);

        if flag & REPEAT_FLAG != 0 {
            if cursor >= glyph_data.len() {
                return Err(FontError::InvalidData(
                    "simple glyph repeat count truncated",
                ));
            }
            let repeat_count = glyph_data[cursor] as usize;
            cursor += 1;
            for _ in 0..repeat_count {
                if flags.len() >= num_points {
                    break;
                }
                flags.push(flag);
            }
        }
    }

    // X 座標のデコード (delta → 累積)
    let mut x_coords = Vec::with_capacity(num_points);
    let mut x: i32 = 0;
    for &flag in &flags {
        if flag & X_SHORT != 0 {
            if cursor >= glyph_data.len() {
                return Err(FontError::InvalidData("simple glyph x data truncated"));
            }
            let dx = glyph_data[cursor] as i32;
            cursor += 1;
            if flag & X_SAME_OR_POS != 0 {
                x += dx;
            } else {
                x -= dx;
            }
        } else if flag & X_SAME_OR_POS == 0 {
            if cursor + 1 >= glyph_data.len() {
                return Err(FontError::InvalidData("simple glyph x data truncated"));
            }
            let dx = i16::from_be_bytes([glyph_data[cursor], glyph_data[cursor + 1]]) as i32;
            cursor += 2;
            x += dx;
        }
        // X_SAME_OR_POS && !X_SHORT → delta = 0, x unchanged
        x_coords.push(x);
    }

    // Y 座標のデコード (delta → 累積)
    let mut y_coords = Vec::with_capacity(num_points);
    let mut y: i32 = 0;
    for &flag in &flags {
        if flag & Y_SHORT != 0 {
            if cursor >= glyph_data.len() {
                return Err(FontError::InvalidData("simple glyph y data truncated"));
            }
            let dy = glyph_data[cursor] as i32;
            cursor += 1;
            if flag & Y_SAME_OR_POS != 0 {
                y += dy;
            } else {
                y -= dy;
            }
        } else if flag & Y_SAME_OR_POS == 0 {
            if cursor + 1 >= glyph_data.len() {
                return Err(FontError::InvalidData("simple glyph y data truncated"));
            }
            let dy = i16::from_be_bytes([glyph_data[cursor], glyph_data[cursor + 1]]) as i32;
            cursor += 2;
            y += dy;
        }
        y_coords.push(y);
    }

    // contour ごとにパスを生成
    let mut contour_start = 0;
    for &end_pt in &end_pts {
        let contour_end = end_pt + 1;
        if contour_end > num_points || contour_start >= contour_end {
            contour_start = contour_end;
            continue;
        }

        emit_contour(
            &flags[contour_start..contour_end],
            &x_coords[contour_start..contour_end],
            &y_coords[contour_start..contour_end],
            xform,
            path,
        );

        contour_start = contour_end;
    }

    Ok(())
}

/// 1 つの contour を Path に変換する。
///
/// TrueType の 2 次ベジェ規則:
/// - on → on: line_to
/// - on → off → on: quad_to(off, on)
/// - off → off: 暗黙の on-curve (2 つの off-curve の中点) を挿入
fn emit_contour(
    flags: &[u8],
    x_coords: &[i32],
    y_coords: &[i32],
    xform: GlyphTransform,
    path: &mut Path,
) {
    let n = flags.len();
    if n == 0 {
        return;
    }

    let tx = |i: usize| -> (f64, f64) { xform.apply(x_coords[i], y_coords[i]) };

    let on_curve = |i: usize| -> bool { flags[i] & ON_CURVE != 0 };

    // 最初の on-curve 点を探す
    // すべて off-curve の場合は最初の 2 点の中点を開始点とする
    let (start_x, start_y, first_idx);
    if on_curve(0) {
        let (sx, sy) = tx(0);
        start_x = sx;
        start_y = sy;
        first_idx = 1;
    } else if on_curve(n - 1) {
        let (sx, sy) = tx(n - 1);
        start_x = sx;
        start_y = sy;
        first_idx = 0;
    } else {
        // 最初の 2 つの off-curve 点の中点
        let (x0, y0) = tx(0);
        let (x1, y1) = tx(n - 1);
        start_x = (x0 + x1) * 0.5;
        start_y = (y0 + y1) * 0.5;
        first_idx = 0;
    };

    path.move_to(start_x, start_y);

    let mut i = first_idx;
    while i < n {
        if on_curve(i) {
            let (px, py) = tx(i);
            path.line_to(px, py);
            i += 1;
        } else {
            // off-curve 点
            let (cpx, cpy) = tx(i);
            let next = (i + 1) % n;

            if next == first_idx && first_idx == 0 && !on_curve(0) && !on_curve(n - 1) {
                // contour の終端に戻る (暗黙 on-curve = start)
                path.quad_to(cpx, cpy, start_x, start_y);
                i += 1;
            } else if next < n && on_curve(next) {
                // off → on
                let (ex, ey) = tx(next);
                path.quad_to(cpx, cpy, ex, ey);
                i += 2;
            } else {
                // off → off: 暗黙 on-curve (中点)
                let (nx, ny) = tx(next);
                let mid_x = (cpx + nx) * 0.5;
                let mid_y = (cpy + ny) * 0.5;
                path.quad_to(cpx, cpy, mid_x, mid_y);
                i += 1;
            }
        }
    }

    // contour を閉じる
    path.close();
}

/// Compound Glyph をパースする。
fn parse_compound_glyph(
    glyph_data: &[u8],
    xform: GlyphTransform,
    tables: &ParsedTables,
    data: &[u8],
    path: &mut Path,
    depth: u32,
) -> Result<(), FontError> {
    // ヘッダ 10 bytes をスキップ
    let mut cursor = 10usize;

    loop {
        if cursor + 4 > glyph_data.len() {
            return Err(FontError::InvalidData("compound glyph truncated"));
        }

        let comp_flags = be_u16(glyph_data, cursor)?;
        let glyph_index = be_u16(glyph_data, cursor + 2)?;
        cursor += 4;

        // 引数 (オフセット)
        let (arg1, arg2);
        if comp_flags & ARG_1_AND_2_ARE_WORDS != 0 {
            if cursor + 4 > glyph_data.len() {
                return Err(FontError::InvalidData("compound args truncated"));
            }
            if comp_flags & ARGS_ARE_XY_VALUES != 0 {
                arg1 = be_i16(glyph_data, cursor)? as f64;
                arg2 = be_i16(glyph_data, cursor + 2)? as f64;
            } else {
                arg1 = be_u16(glyph_data, cursor)? as f64;
                arg2 = be_u16(glyph_data, cursor + 2)? as f64;
            }
            cursor += 4;
        } else {
            if cursor + 2 > glyph_data.len() {
                return Err(FontError::InvalidData("compound args truncated"));
            }
            if comp_flags & ARGS_ARE_XY_VALUES != 0 {
                arg1 = be_i8(glyph_data, cursor)? as f64;
                arg2 = be_i8(glyph_data, cursor + 1)? as f64;
            } else {
                arg1 = be_u8(glyph_data, cursor)? as f64;
                arg2 = be_u8(glyph_data, cursor + 1)? as f64;
            }
            cursor += 2;
        }

        // 変換行列
        let (mut cm00, mut cm01, mut cm10, mut cm11) = (1.0f64, 0.0f64, 0.0f64, 1.0f64);

        if comp_flags & WE_HAVE_A_SCALE != 0 {
            if cursor + 2 > glyph_data.len() {
                return Err(FontError::InvalidData("compound scale truncated"));
            }
            let s = f2dot14(be_i16(glyph_data, cursor)?);
            cursor += 2;
            cm00 = s;
            cm11 = s;
        } else if comp_flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            if cursor + 4 > glyph_data.len() {
                return Err(FontError::InvalidData("compound xy scale truncated"));
            }
            cm00 = f2dot14(be_i16(glyph_data, cursor)?);
            cm11 = f2dot14(be_i16(glyph_data, cursor + 2)?);
            cursor += 4;
        } else if comp_flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            if cursor + 8 > glyph_data.len() {
                return Err(FontError::InvalidData("compound 2x2 matrix truncated"));
            }
            cm00 = f2dot14(be_i16(glyph_data, cursor)?);
            cm01 = f2dot14(be_i16(glyph_data, cursor + 2)?);
            cm10 = f2dot14(be_i16(glyph_data, cursor + 4)?);
            cm11 = f2dot14(be_i16(glyph_data, cursor + 6)?);
            cursor += 8;
        }

        // 親の変換行列と合成
        let new_m00 = xform.m00 * cm00 + xform.m01 * cm10;
        let new_m01 = xform.m00 * cm01 + xform.m01 * cm11;
        let new_m10 = xform.m10 * cm00 + xform.m11 * cm10;
        let new_m11 = xform.m10 * cm01 + xform.m11 * cm11;

        // XY オフセットを親の変換行列でピクセル空間に変換
        let dx = if comp_flags & ARGS_ARE_XY_VALUES != 0 {
            arg1
        } else {
            0.0
        };
        let dy = if comp_flags & ARGS_ARE_XY_VALUES != 0 {
            arg2
        } else {
            0.0
        };

        let child_xform = GlyphTransform {
            offset_x: xform.offset_x + (dx * xform.m00 + dy * xform.m01) * xform.scale,
            offset_y: xform.offset_y - (dx * xform.m10 + dy * xform.m11) * xform.scale,
            scale: xform.scale,
            m00: new_m00,
            m01: new_m01,
            m10: new_m10,
            m11: new_m11,
        };

        append_glyph_recursive(glyph_index, child_xform, tables, data, path, depth + 1)?;

        if comp_flags & MORE_COMPONENTS == 0 {
            break;
        }
    }

    Ok(())
}

/// F2Dot14 固定小数点数を f64 に変換する。
fn f2dot14(value: i16) -> f64 {
    value as f64 / 16384.0
}
