/// TrueType テーブルパーサ。
///
/// sfnt ヘッダからテーブルディレクトリをパースし、
/// head, maxp, hhea, hmtx, loca, cmap, glyf, OS/2 の各テーブルを読み出す。
/// OS/2 テーブルは cap_height / x_height の取得にのみ利用する。
use crate::font::FontError;

// ---------------------------------------------------------------------------
// バイト読み取りヘルパー
// ---------------------------------------------------------------------------

fn read_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    let end = offset + 2;
    if end > data.len() {
        return Err(FontError::InvalidData("unexpected end of data (u16)"));
    }
    Ok(u16::from_be_bytes([data[offset], data[offset + 1]]))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    let end = offset + 2;
    if end > data.len() {
        return Err(FontError::InvalidData("unexpected end of data (i16)"));
    }
    Ok(i16::from_be_bytes([data[offset], data[offset + 1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let end = offset + 4;
    if end > data.len() {
        return Err(FontError::InvalidData("unexpected end of data (u32)"));
    }
    Ok(u32::from_be_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

// ---------------------------------------------------------------------------
// テーブルディレクトリ
// ---------------------------------------------------------------------------

/// テーブルの位置情報。
#[derive(Debug, Clone, Copy)]
struct TableRecord {
    offset: u32,
    length: u32,
}

/// テーブルディレクトリ。tag → (offset, length) のマッピング。
struct TableDirectory {
    records: Vec<(u32, TableRecord)>,
}

impl TableDirectory {
    fn parse(data: &[u8], start: usize) -> Result<Self, FontError> {
        if data.len() < start + 12 {
            return Err(FontError::InvalidData("data too short for sfnt header"));
        }

        let sfnt_version = read_u32(data, start)?;
        // TrueType: 0x00010000, OpenType with TrueType outlines: 0x00010000
        // 'true' (0x74727565) も許容 (Apple)
        if sfnt_version != 0x0001_0000 && sfnt_version != 0x7472_7565 {
            return Err(FontError::InvalidData("unsupported sfnt version"));
        }

        let num_tables = read_u16(data, start + 4)? as usize;
        let header_end = start + 12 + num_tables * 16;
        if data.len() < header_end {
            return Err(FontError::InvalidData("data too short for table directory"));
        }

        let mut records = Vec::with_capacity(num_tables);
        for i in 0..num_tables {
            let base = start + 12 + i * 16;
            let tag = read_u32(data, base)?;
            let offset = read_u32(data, base + 8)?;
            let length = read_u32(data, base + 12)?;

            // テーブル範囲が実データに収まるか検証
            let end = offset as u64 + length as u64;
            if end > data.len() as u64 {
                return Err(FontError::InvalidData("table record exceeds data length"));
            }

            records.push((tag, TableRecord { offset, length }));
        }

        Ok(Self { records })
    }

    fn find(&self, tag: u32) -> Option<TableRecord> {
        self.records
            .iter()
            .find(|(t, _)| *t == tag)
            .map(|(_, r)| *r)
    }

    fn require(&self, tag: u32) -> Result<TableRecord, FontError> {
        self.find(tag)
            .ok_or(FontError::InvalidData("required table not found"))
    }
}

/// 4 文字の ASCII タグを u32 に変換する。
const fn tag(b: &[u8; 4]) -> u32 {
    ((b[0] as u32) << 24) | ((b[1] as u32) << 16) | ((b[2] as u32) << 8) | (b[3] as u32)
}

const TAG_HEAD: u32 = tag(b"head");
const TAG_MAXP: u32 = tag(b"maxp");
const TAG_HHEA: u32 = tag(b"hhea");
const TAG_HMTX: u32 = tag(b"hmtx");
const TAG_LOCA: u32 = tag(b"loca");
const TAG_CMAP: u32 = tag(b"cmap");
const TAG_GLYF: u32 = tag(b"glyf");
const TAG_OS2: u32 = tag(b"OS/2");

// ---------------------------------------------------------------------------
// head テーブル
// ---------------------------------------------------------------------------

pub(crate) struct HeadTable {
    pub units_per_em: u16,
    pub index_to_loc_format: i16,
}

fn parse_head(data: &[u8], rec: TableRecord) -> Result<HeadTable, FontError> {
    let off = rec.offset as usize;
    if rec.length < 54 {
        return Err(FontError::InvalidData("head table too short"));
    }
    let units_per_em = read_u16(data, off + 18)?;
    if units_per_em == 0 {
        return Err(FontError::InvalidData("units_per_em is zero"));
    }
    let index_to_loc_format = read_i16(data, off + 50)?;
    Ok(HeadTable {
        units_per_em,
        index_to_loc_format,
    })
}

// ---------------------------------------------------------------------------
// maxp テーブル
// ---------------------------------------------------------------------------

fn parse_maxp(data: &[u8], rec: TableRecord) -> Result<u16, FontError> {
    let off = rec.offset as usize;
    if rec.length < 6 {
        return Err(FontError::InvalidData("maxp table too short"));
    }
    read_u16(data, off + 4)
}

// ---------------------------------------------------------------------------
// hhea テーブル
// ---------------------------------------------------------------------------

pub(crate) struct HheaTable {
    pub ascent: i16,
    pub descent: i16,
    pub line_gap: i16,
    pub number_of_h_metrics: u16,
}

fn parse_hhea(data: &[u8], rec: TableRecord) -> Result<HheaTable, FontError> {
    let off = rec.offset as usize;
    if rec.length < 36 {
        return Err(FontError::InvalidData("hhea table too short"));
    }
    let ascent = read_i16(data, off + 4)?;
    let descent = read_i16(data, off + 6)?;
    let line_gap = read_i16(data, off + 8)?;
    let number_of_h_metrics = read_u16(data, off + 34)?;
    Ok(HheaTable {
        ascent,
        descent,
        line_gap,
        number_of_h_metrics,
    })
}

// ---------------------------------------------------------------------------
// OS/2 テーブル
// ---------------------------------------------------------------------------

/// OS/2 テーブルから取得する cap / x 高さ。
/// OS/2 v2 以上の `sCapHeight` / `sxHeight` フィールドに対応する。
struct Os2Table {
    cap_height: Option<i16>,
    x_height: Option<i16>,
}

fn parse_os2(data: &[u8], rec: TableRecord) -> Result<Os2Table, FontError> {
    let off = rec.offset as usize;
    // Microsoft 拡張形式の OS/2 version 0 最小サイズは 78 バイト。
    // Apple 旧形式 68 バイトは現代の OS バンドルフォントに存在しないためエラーとする。
    if rec.length < 78 {
        return Err(FontError::InvalidData("OS/2 table too short"));
    }
    let version = read_u16(data, off)?;
    if version < 2 {
        // v0 / v1 では sCapHeight / sxHeight が存在しない。
        return Ok(Os2Table {
            cap_height: None,
            x_height: None,
        });
    }
    // OpenType 仕様 OS/2 テーブル v2+ の sxHeight (offset 86) + sCapHeight (offset 88) を読むには 90 バイト必要。
    // 将来の OS/2 バージョンでも offset が変わらない前提とする（変更時は要修正）。
    // 不足時は寛容方針で None を返し、FontFace::from_data 自体は成功させる。
    if rec.length < 90 {
        return Ok(Os2Table {
            cap_height: None,
            x_height: None,
        });
    }
    let x_height = Some(read_i16(data, off + 86)?);
    let cap_height = Some(read_i16(data, off + 88)?);
    Ok(Os2Table {
        cap_height,
        x_height,
    })
}

// ---------------------------------------------------------------------------
// hmtx テーブル
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) struct HmtxTable {
    /// advance_width[i] (i < number_of_h_metrics のときは各グリフ固有、
    /// i >= number_of_h_metrics のときは最後の値を使う)。
    pub advance_widths: Vec<u16>,
    #[allow(dead_code)]
    pub lsbs: Vec<i16>,
}

fn parse_hmtx(
    data: &[u8],
    rec: TableRecord,
    num_h_metrics: u16,
    num_glyphs: u16,
) -> Result<HmtxTable, FontError> {
    let off = rec.offset as usize;
    let nh = num_h_metrics as usize;
    let ng = num_glyphs as usize;

    // longHorMetric[nh] + leftSideBearing[ng - nh]
    let min_len = nh * 4 + (ng.saturating_sub(nh)) * 2;
    if (rec.length as usize) < min_len {
        return Err(FontError::InvalidData("hmtx table too short"));
    }

    let mut advance_widths = Vec::with_capacity(ng);
    let mut lsbs = Vec::with_capacity(ng);

    for i in 0..nh {
        let base = off + i * 4;
        advance_widths.push(read_u16(data, base)?);
        lsbs.push(read_i16(data, base + 2)?);
    }

    // 残りのグリフは最後の advance_width を継承
    let last_aw = advance_widths.last().copied().unwrap_or(0);
    let extra_base = off + nh * 4;
    for i in 0..(ng - nh) {
        advance_widths.push(last_aw);
        lsbs.push(read_i16(data, extra_base + i * 2)?);
    }

    Ok(HmtxTable {
        advance_widths,
        lsbs,
    })
}

// ---------------------------------------------------------------------------
// loca テーブル
// ---------------------------------------------------------------------------

fn parse_loca(
    data: &[u8],
    rec: TableRecord,
    num_glyphs: u16,
    index_to_loc_format: i16,
) -> Result<Vec<u32>, FontError> {
    let off = rec.offset as usize;
    let n = num_glyphs as usize + 1; // loca は num_glyphs + 1 エントリ

    let offsets = if index_to_loc_format == 0 {
        // short format: u16 * 2
        let min_len = n * 2;
        if (rec.length as usize) < min_len {
            return Err(FontError::InvalidData("loca table too short (short)"));
        }
        (0..n)
            .map(|i| read_u16(data, off + i * 2).map(|v| v as u32 * 2))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        // long format: u32
        let min_len = n * 4;
        if (rec.length as usize) < min_len {
            return Err(FontError::InvalidData("loca table too short (long)"));
        }
        (0..n)
            .map(|i| read_u32(data, off + i * 4))
            .collect::<Result<Vec<_>, _>>()?
    };

    Ok(offsets)
}

// ---------------------------------------------------------------------------
// cmap テーブル
// ---------------------------------------------------------------------------

/// Unicode → GlyphID ルックアップ。
#[derive(Clone)]
pub(crate) enum CmapLookup {
    Format4(CmapFormat4),
    Format12(CmapFormat12),
}

#[derive(Clone)]
pub(crate) struct CmapFormat4 {
    segments: Vec<CmapFormat4Segment>,
    /// glyphIdArray 全体 (id_range_offset で参照する)。
    glyph_id_array: Vec<u16>,
    /// glyphIdArray のバイトオフセット (id_range_offset の基点計算用)。
    glyph_id_array_offset: usize,
    /// idRangeOffset 配列のバイトオフセット。
    id_range_offset_base: usize,
}

#[derive(Clone)]
struct CmapFormat4Segment {
    end_code: u16,
    start_code: u16,
    id_delta: i16,
    id_range_offset: u16,
}

#[derive(Clone)]
pub(crate) struct CmapFormat12 {
    groups: Vec<CmapFormat12Group>,
}

#[derive(Clone)]
struct CmapFormat12Group {
    start_char: u32,
    end_char: u32,
    start_glyph_id: u32,
}

impl CmapLookup {
    pub fn map(&self, codepoint: u32) -> u16 {
        match self {
            CmapLookup::Format4(f4) => f4.map(codepoint),
            CmapLookup::Format12(f12) => f12.map(codepoint),
        }
    }
}

impl CmapFormat4 {
    fn map(&self, codepoint: u32) -> u16 {
        if codepoint > 0xFFFF {
            return 0;
        }
        let cp = codepoint as u16;

        // 二分探索で該当セグメントを見つける
        let idx = match self.segments.binary_search_by(|seg| seg.end_code.cmp(&cp)) {
            Ok(i) => i,
            Err(i) => {
                if i >= self.segments.len() {
                    return 0;
                }
                i
            }
        };

        let seg = &self.segments[idx];
        if cp < seg.start_code {
            return 0;
        }

        if seg.id_range_offset == 0 {
            // id_delta 方式
            (cp as i32 + seg.id_delta as i32) as u16
        } else {
            // id_range_offset 方式
            // 仕様: glyphId = glyphIdArray[
            //   idRangeOffset[i]/2 + (cp - startCode) + &idRangeOffset[i] - glyphIdArrayStart
            // ]
            // 実装: idRangeOffset のバイト位置からの相対オフセットで計算
            let seg_byte_offset = self.id_range_offset_base + idx * 2;
            let target_byte_offset =
                seg_byte_offset + seg.id_range_offset as usize + (cp - seg.start_code) as usize * 2;
            let array_index = (target_byte_offset - self.glyph_id_array_offset) / 2;

            if array_index >= self.glyph_id_array.len() {
                return 0;
            }

            let glyph_id = self.glyph_id_array[array_index];
            if glyph_id == 0 {
                0
            } else {
                (glyph_id as i32 + seg.id_delta as i32) as u16
            }
        }
    }
}

impl CmapFormat12 {
    fn map(&self, codepoint: u32) -> u16 {
        // 二分探索
        let idx = match self
            .groups
            .binary_search_by(|g| g.start_char.cmp(&codepoint))
        {
            Ok(i) => i,
            Err(0) => return 0,
            Err(i) => i - 1,
        };

        let g = &self.groups[idx];
        if codepoint > g.end_char {
            return 0;
        }

        let glyph_id = g.start_glyph_id + (codepoint - g.start_char);
        glyph_id as u16
    }
}

fn parse_cmap(data: &[u8], rec: TableRecord) -> Result<CmapLookup, FontError> {
    let off = rec.offset as usize;
    if rec.length < 4 {
        return Err(FontError::InvalidData("cmap table too short"));
    }

    let num_tables = read_u16(data, off + 2)? as usize;
    if rec.length < (4 + num_tables * 8) as u32 {
        return Err(FontError::InvalidData(
            "cmap table too short for encoding records",
        ));
    }

    // 最適なサブテーブルを選択する
    // 優先: platformID=3 encodingID=10 (UCS-4) > platformID=3 encodingID=1 (UCS-2)
    //       platformID=0 encodingID=3..6 も許容
    let mut best_offset: Option<u32> = None;
    let mut best_priority = 0u8;

    for i in 0..num_tables {
        let base = off + 4 + i * 8;
        let platform_id = read_u16(data, base)?;
        let encoding_id = read_u16(data, base + 2)?;
        let subtable_offset = read_u32(data, base + 4)?;

        let priority = match (platform_id, encoding_id) {
            (3, 10) => 4,    // Windows UCS-4
            (0, 4..=6) => 3, // Unicode full
            (3, 1) => 2,     // Windows UCS-2
            (0, 3) => 1,     // Unicode BMP
            _ => 0,
        };

        if priority > best_priority {
            best_priority = priority;
            best_offset = Some(subtable_offset);
        }
    }

    let subtable_off =
        off + best_offset.ok_or(FontError::InvalidData("no suitable cmap subtable"))? as usize;

    if subtable_off + 2 > data.len() {
        return Err(FontError::InvalidData("cmap subtable offset out of range"));
    }

    let format = read_u16(data, subtable_off)?;

    match format {
        4 => parse_cmap_format4(data, subtable_off),
        12 => parse_cmap_format12(data, subtable_off),
        _ => Err(FontError::InvalidData("unsupported cmap format")),
    }
}

fn parse_cmap_format4(data: &[u8], off: usize) -> Result<CmapLookup, FontError> {
    if off + 14 > data.len() {
        return Err(FontError::InvalidData("cmap format 4 header too short"));
    }
    let length = read_u16(data, off + 2)? as usize;
    if off + length > data.len() {
        return Err(FontError::InvalidData("cmap format 4 length exceeds data"));
    }

    let seg_count_x2 = read_u16(data, off + 6)? as usize;
    let seg_count = seg_count_x2 / 2;

    // 各配列のオフセット
    let end_codes_off = off + 14;
    // reservedPad: 2 bytes after end_codes
    let start_codes_off = end_codes_off + seg_count * 2 + 2;
    let id_deltas_off = start_codes_off + seg_count * 2;
    let id_range_offsets_off = id_deltas_off + seg_count * 2;
    let glyph_id_array_off = id_range_offsets_off + seg_count * 2;

    let table_end = off + length;
    if glyph_id_array_off > table_end {
        return Err(FontError::InvalidData(
            "cmap format 4 arrays exceed table length",
        ));
    }

    let mut segments = Vec::with_capacity(seg_count);
    for i in 0..seg_count {
        let end_code = read_u16(data, end_codes_off + i * 2)?;
        let start_code = read_u16(data, start_codes_off + i * 2)?;
        let id_delta = read_i16(data, id_deltas_off + i * 2)?;
        let id_range_offset = read_u16(data, id_range_offsets_off + i * 2)?;

        segments.push(CmapFormat4Segment {
            end_code,
            start_code,
            id_delta,
            id_range_offset,
        });
    }

    // glyphIdArray
    let glyph_id_count = (table_end - glyph_id_array_off) / 2;
    let mut glyph_id_array = Vec::with_capacity(glyph_id_count);
    for i in 0..glyph_id_count {
        glyph_id_array.push(read_u16(data, glyph_id_array_off + i * 2)?);
    }

    Ok(CmapLookup::Format4(CmapFormat4 {
        segments,
        glyph_id_array,
        glyph_id_array_offset: glyph_id_array_off,
        id_range_offset_base: id_range_offsets_off,
    }))
}

fn parse_cmap_format12(data: &[u8], off: usize) -> Result<CmapLookup, FontError> {
    // format 12 は fixed32 format (先頭 u16=12, 次の u16=0, 次 u32=length)
    if off + 16 > data.len() {
        return Err(FontError::InvalidData("cmap format 12 header too short"));
    }
    let num_groups = read_u32(data, off + 12)? as usize;
    let groups_off = off + 16;

    if groups_off + num_groups * 12 > data.len() {
        return Err(FontError::InvalidData("cmap format 12 groups exceed data"));
    }

    let mut groups = Vec::with_capacity(num_groups);
    for i in 0..num_groups {
        let base = groups_off + i * 12;
        let start_char = read_u32(data, base)?;
        let end_char = read_u32(data, base + 4)?;
        let start_glyph_id = read_u32(data, base + 8)?;
        groups.push(CmapFormat12Group {
            start_char,
            end_char,
            start_glyph_id,
        });
    }

    Ok(CmapLookup::Format12(CmapFormat12 { groups }))
}

// ---------------------------------------------------------------------------
// パース済みテーブル集合
// ---------------------------------------------------------------------------

/// フォントファイルからパースした全テーブル情報。
#[derive(Clone)]
pub(crate) struct ParsedTables {
    pub units_per_em: u16,
    #[allow(dead_code)]
    pub num_glyphs: u16,
    pub ascent: i16,
    pub descent: i16,
    pub line_gap: i16,
    pub cap_height: Option<i16>,
    pub x_height: Option<i16>,
    pub loca_offsets: Vec<u32>,
    pub glyf_offset: u32,
    #[allow(dead_code)]
    pub glyf_length: u32,
    pub cmap: CmapLookup,
    pub hmtx: HmtxTable,
}

/// TTC タグ 'ttcf'。
const TAG_TTCF: u32 = tag(b"ttcf");

/// TTC ファイルから指定インデックスのフォントオフセットを取得する。
fn ttc_font_offset(data: &[u8], index: u32) -> Result<usize, FontError> {
    if data.len() < 12 {
        return Err(FontError::InvalidData("TTC header too short"));
    }
    let num_fonts = read_u32(data, 8)?;
    if index >= num_fonts {
        return Err(FontError::InvalidData("TTC font index out of range"));
    }
    let offset_pos = 12 + index as usize * 4;
    if offset_pos + 4 > data.len() {
        return Err(FontError::InvalidData("TTC offset table too short"));
    }
    let offset = read_u32(data, offset_pos)? as usize;
    Ok(offset)
}

/// フォントデータの全テーブルをパースする。
///
/// `index` は TTC (TrueType Collection) 内のフォントインデックス。
/// 単体フォントの場合は 0 を指定する。
pub(crate) fn parse_all(data: &[u8], index: u32) -> Result<ParsedTables, FontError> {
    // TTC ヘッダを検出し、指定インデックスのフォントオフセットを取得する
    let sfnt_start = if data.len() >= 4 && read_u32(data, 0)? == TAG_TTCF {
        ttc_font_offset(data, index)?
    } else {
        0
    };

    let dir = TableDirectory::parse(data, sfnt_start)?;

    let head_rec = dir.require(TAG_HEAD)?;
    let maxp_rec = dir.require(TAG_MAXP)?;
    let hhea_rec = dir.require(TAG_HHEA)?;
    let hmtx_rec = dir.require(TAG_HMTX)?;
    let loca_rec = dir.require(TAG_LOCA)?;
    let cmap_rec = dir.require(TAG_CMAP)?;
    let glyf_rec = dir.require(TAG_GLYF)?;

    let head = parse_head(data, head_rec)?;
    let num_glyphs = parse_maxp(data, maxp_rec)?;
    let hhea = parse_hhea(data, hhea_rec)?;
    let hmtx = parse_hmtx(data, hmtx_rec, hhea.number_of_h_metrics, num_glyphs)?;
    let loca_offsets = parse_loca(data, loca_rec, num_glyphs, head.index_to_loc_format)?;
    let cmap = parse_cmap(data, cmap_rec)?;
    let os2 = match dir.find(TAG_OS2) {
        Some(rec) => parse_os2(data, rec)?,
        None => Os2Table {
            cap_height: None,
            x_height: None,
        },
    };

    Ok(ParsedTables {
        units_per_em: head.units_per_em,
        num_glyphs,
        ascent: hhea.ascent,
        descent: hhea.descent,
        line_gap: hhea.line_gap,
        cap_height: os2.cap_height,
        x_height: os2.x_height,
        loca_offsets,
        glyf_offset: glyf_rec.offset,
        glyf_length: glyf_rec.length,
        cmap,
        hmtx,
    })
}

// ---------------------------------------------------------------------------
// 公開ヘルパー (glyph.rs から利用)
// ---------------------------------------------------------------------------

/// バイト列から u16 (Big Endian) を読み取る。
pub(crate) fn be_u16(data: &[u8], offset: usize) -> Result<u16, FontError> {
    read_u16(data, offset)
}

/// バイト列から i16 (Big Endian) を読み取る。
pub(crate) fn be_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    read_i16(data, offset)
}

/// バイト列から u8 を読み取る。
pub(crate) fn be_u8(data: &[u8], offset: usize) -> Result<u8, FontError> {
    if offset >= data.len() {
        return Err(FontError::InvalidData("unexpected end of data (u8)"));
    }
    Ok(data[offset])
}

/// バイト列から i8 を読み取る。
pub(crate) fn be_i8(data: &[u8], offset: usize) -> Result<i8, FontError> {
    if offset >= data.len() {
        return Err(FontError::InvalidData("unexpected end of data (i8)"));
    }
    Ok(data[offset] as i8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 指定バージョン・長さの OS/2 テーブルバイト列を生成する。
    /// length >= 90 の場合、sxHeight ( offset 86 ) = 16、sCapHeight ( offset 88 ) = 32 を設定する。
    fn os2_bytes(version: u16, length: u32) -> Vec<u8> {
        let mut data = vec![0u8; length as usize];
        // version は先頭 u16 ( big endian ) 。
        data[0] = (version >> 8) as u8;
        data[1] = (version & 0xFF) as u8;
        if length >= 90 {
            data[86] = 0x00;
            data[87] = 0x10; // sxHeight = 16
            data[88] = 0x00;
            data[89] = 0x20; // sCapHeight = 32
        }
        data
    }

    #[test]
    fn parse_os2_v0_returns_none() {
        let data = os2_bytes(0, 78);
        let rec = TableRecord {
            offset: 0,
            length: 78,
        };
        let os2 = parse_os2(&data, rec).expect("version 0 の最小長は許容される");
        assert!(os2.cap_height.is_none());
        assert!(os2.x_height.is_none());
    }

    #[test]
    fn parse_os2_v1_returns_none() {
        let data = os2_bytes(1, 78);
        let rec = TableRecord {
            offset: 0,
            length: 78,
        };
        let os2 = parse_os2(&data, rec).expect("version 1 の最小長は許容される");
        assert!(os2.cap_height.is_none());
        assert!(os2.x_height.is_none());
    }

    #[test]
    fn parse_os2_v2_length_88_returns_none() {
        let data = os2_bytes(2, 88);
        let rec = TableRecord {
            offset: 0,
            length: 88,
        };
        let os2 = parse_os2(&data, rec).expect("v2+ かつ 78 <= length < 90 は許容される");
        assert!(os2.cap_height.is_none());
        assert!(os2.x_height.is_none());
    }

    #[test]
    fn parse_os2_v2_length_90_returns_values() {
        let data = os2_bytes(2, 90);
        let rec = TableRecord {
            offset: 0,
            length: 90,
        };
        let os2 = parse_os2(&data, rec).expect("v2+ かつ length >= 90 は許容される");
        assert_eq!(os2.x_height, Some(16));
        assert_eq!(os2.cap_height, Some(32));
    }

    #[test]
    fn parse_os2_v2_length_76_returns_error() {
        let data = os2_bytes(2, 76);
        let rec = TableRecord {
            offset: 0,
            length: 76,
        };
        assert!(parse_os2(&data, rec).is_err());
    }
}
