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
    let end = offset
        .checked_add(2)
        .ok_or(FontError::InvalidData("offset overflow (u16)"))?;
    if end > data.len() {
        return Err(FontError::InvalidData("unexpected end of data (u16)"));
    }
    Ok(u16::from_be_bytes([data[offset], data[offset + 1]]))
}

fn read_i16(data: &[u8], offset: usize) -> Result<i16, FontError> {
    let end = offset
        .checked_add(2)
        .ok_or(FontError::InvalidData("offset overflow (i16)"))?;
    if end > data.len() {
        return Err(FontError::InvalidData("unexpected end of data (i16)"));
    }
    Ok(i16::from_be_bytes([data[offset], data[offset + 1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, FontError> {
    let end = offset
        .checked_add(4)
        .ok_or(FontError::InvalidData("offset overflow (u32)"))?;
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

        let mut records = Vec::new();
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
const TAG_GSUB: u32 = tag(b"GSUB");
const TAG_GPOS: u32 = tag(b"GPOS");

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
    // Microsoft 拡張形式の OS/2 version 0 最小サイズ は 78 バイト。
    // Apple 旧形式 68 バイト は現代の OS バンドルフォントに存在しないためエラーとする。
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
    #[expect(dead_code)]
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

    let mut advance_widths = Vec::new();
    let mut lsbs = Vec::new();

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
            let target_byte_offset = seg_byte_offset
                .checked_add(seg.id_range_offset as usize)
                .and_then(|v| v.checked_add((cp - seg.start_code) as usize * 2))
                .unwrap_or(usize::MAX);
            let Some(array_index_usize) =
                target_byte_offset.checked_sub(self.glyph_id_array_offset)
            else {
                return 0;
            };
            let array_index = array_index_usize / 2;

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
        if glyph_id > u16::MAX as u32 {
            return 0;
        }
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

    let mut segments = Vec::new();
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
    let mut glyph_id_array = Vec::new();
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

    let mut groups = Vec::new();
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
// GSUB / GPOS テーブル
// ---------------------------------------------------------------------------

#[derive(Clone, Default)]
pub(crate) struct GsubTable {
    /// DefaultLangSys 内の FeatureIndex 順に並べた、有効な置換 feature の Lookup 参照。
    /// タプルは (feature tag, lookup index)。`liga` / `clig` のみを含む。
    pub applicable_lookups: Vec<(u32, u16)>,
    pub lookups: Vec<GsubLookup>,
}

#[derive(Clone)]
pub(crate) enum GsubLookup {
    Single(Vec<GsubSingleSubst>),
    Ligature(Vec<GsubLigatureSubst>),
    Unsupported,
}

#[derive(Clone)]
pub(crate) struct GsubSingleSubst {
    pub coverage: Coverage,
    pub format: GsubSingleSubstFormat,
}

#[derive(Clone)]
pub(crate) enum GsubSingleSubstFormat {
    /// Format 1: delta_glyph_id 加算 (modulo 65536 wrap)
    DeltaGlyphID(i16),
    /// Format 2: substitute glyph IDs 配列
    SubstituteGlyphIDs(Vec<u16>),
}

#[derive(Clone)]
pub(crate) struct GsubLigatureSubst {
    pub coverage: Coverage,
    /// LigatureSet[Coverage Index] のリスト。各エントリは Ligature の配列。
    pub ligature_sets: Vec<Vec<GsubLigature>>,
}

#[derive(Clone)]
pub(crate) struct GsubLigature {
    pub ligature_glyph: u16,
    /// 合字を構成する 2 番目以降のグリフ ID のみを保持する (N-1 要素)。
    pub component_glyph_ids: Vec<u16>,
}

#[derive(Clone, Default)]
pub(crate) struct GposTable {
    /// `kern` feature が参照する Lookup インデックスのリスト (重複除去済み)。
    pub kern_lookups: Vec<u16>,
    pub lookups: Vec<GposLookup>,
}

#[derive(Clone)]
pub(crate) enum GposLookup {
    Single(Vec<GposSingleAdjust>),
    Pair(Vec<GposPairAdjust>),
    Unsupported,
}

#[derive(Clone)]
pub(crate) struct GposSingleAdjust {
    pub coverage: Coverage,
    pub format: GposSingleAdjustFormat,
}

#[derive(Clone)]
pub(crate) enum GposSingleAdjustFormat {
    /// Format 1: 全 Coverage に同じ ValueRecord
    Uniform(ValueRecord),
    /// Format 2: Coverage Index 別の ValueRecord
    PerGlyph(Vec<ValueRecord>),
}

#[derive(Clone)]
pub(crate) struct GposPairAdjust {
    pub coverage: Coverage,
    pub format: GposPairAdjustFormat,
}

#[derive(Clone)]
pub(crate) enum GposPairAdjustFormat {
    /// Format 1: PairSet[Coverage Index] のリスト
    PairSet(Vec<Vec<GposPair>>),
    /// Format 2: Class1 x Class2 マトリクス
    Class {
        class_def_1: ClassDef,
        class_def_2: ClassDef,
        class1_count: u16,
        class2_count: u16,
        records: Vec<(ValueRecord, ValueRecord)>,
    },
}

#[derive(Clone)]
pub(crate) struct GposPair {
    pub second_glyph: u16,
    pub value1: ValueRecord,
    pub value2: ValueRecord,
}

#[derive(Clone, Default)]
pub(crate) struct ValueRecord {
    pub x_placement: i16,
    pub y_placement: i16,
    pub x_advance: i16,
    pub y_advance: i16,
}

#[derive(Debug, Clone)]
pub(crate) enum Coverage {
    /// Format 1: glyph 列挙 (昇順前提、binary_search で検索)
    GlyphList(Vec<u16>),
    /// Format 2: (start, end, start_coverage_index) のタプル列
    RangeList(Vec<(u16, u16, u16)>),
}

#[derive(Clone)]
pub(crate) enum ClassDef {
    Format1 {
        start_glyph: u16,
        class_values: Vec<u16>,
    },
    /// (start, end, class) の範囲列。範囲外グリフは class = 0 として扱う
    Format2 { ranges: Vec<(u16, u16, u16)> },
}

impl GsubTable {
    pub(crate) fn lookup_at(&self, index: u16) -> Option<&GsubLookup> {
        self.lookups.get(index as usize)
    }
}

impl GposTable {
    pub(crate) fn lookup_at(&self, index: u16) -> Option<&GposLookup> {
        self.lookups.get(index as usize)
    }
}

impl GposPairAdjust {
    pub(crate) fn lookup(&self, g1: u16, g2: u16) -> Option<(&ValueRecord, &ValueRecord)> {
        let coverage_index = self.coverage.index_of(g1)?;
        match &self.format {
            GposPairAdjustFormat::PairSet(sets) => {
                let set = sets.get(coverage_index as usize)?;
                set.iter()
                    .find(|p| p.second_glyph == g2)
                    .map(|p| (&p.value1, &p.value2))
            }
            GposPairAdjustFormat::Class {
                class_def_1,
                class_def_2,
                class1_count,
                class2_count,
                records,
            } => {
                let class1 = class_def_1.class_of(g1);
                let class2 = class_def_2.class_of(g2);
                if class1 >= *class1_count || class2 >= *class2_count {
                    return None;
                }
                let idx = (class1 as usize) * (*class2_count as usize) + class2 as usize;
                records.get(idx).map(|(v1, v2)| (v1, v2))
            }
        }
    }
}

impl Coverage {
    /// Coverage に含まれるグリフ数を返す。
    pub(crate) fn glyph_count(&self) -> usize {
        match self {
            Coverage::GlyphList(list) => list.len(),
            Coverage::RangeList(ranges) => ranges
                .iter()
                .map(|(start, end, _)| {
                    if start > end {
                        0
                    } else {
                        (*end as usize).saturating_sub(*start as usize) + 1
                    }
                })
                .sum(),
        }
    }

    /// Coverage Format 1 で glyph_ids が昇順でない不正フォントの場合、
    /// `binary_search` は `Err(insertion_pos)` を返し、本関数は `.ok().map(...)`
    /// で `None` に縮退する。結果としてシェーピングは当該グリフで noop となる。
    ///
    /// CoverageIndex は `u16` だが、計算中にのみ `u32` を使い、結果が `u16` の
    /// 範囲を超える不正フォントでは `None` を返す。
    pub(crate) fn index_of(&self, glyph_id: u16) -> Option<u16> {
        match self {
            Coverage::GlyphList(list) => list.binary_search(&glyph_id).ok().map(|i| i as u16),
            Coverage::RangeList(ranges) => {
                for &(start, end, start_idx) in ranges {
                    if glyph_id >= start && glyph_id <= end {
                        let idx = (start_idx as u32) + (glyph_id as u32 - start as u32);
                        return (idx <= u16::MAX as u32).then_some(idx as u16);
                    }
                }
                None
            }
        }
    }
}

impl ClassDef {
    pub(crate) fn class_of(&self, glyph_id: u16) -> u16 {
        match self {
            ClassDef::Format1 {
                start_glyph,
                class_values,
            } => {
                if glyph_id < *start_glyph {
                    return 0;
                }
                class_values
                    .get((glyph_id - start_glyph) as usize)
                    .copied()
                    .unwrap_or(0)
            }
            ClassDef::Format2 { ranges } => {
                for &(start, end, class) in ranges {
                    if glyph_id >= start && glyph_id <= end {
                        return class;
                    }
                }
                0
            }
        }
    }
}

/// OpenType Layout feature tag 定数。
pub(crate) const TAG_LIGA: u32 = 0x6C69_6761; // 'liga'
pub(crate) const TAG_CLIG: u32 = 0x636C_6967; // 'clig'
pub(crate) const TAG_KERN: u32 = 0x6B65_726E; // 'kern'

/// OpenType Layout script tag 定数。
pub(crate) const TAG_DFLT: u32 = 0x4446_4C54; // 'DFLT'
pub(crate) const TAG_LATN: u32 = 0x6C61_746E; // 'latn'

/// GSUB / GPOS テーブル共通のヘッダ解析結果。
struct LayoutTableInfo {
    table_end: usize,
    feature_list_off: usize,
    feature_indices: Vec<u16>,
    lookup_list_off: usize,
}

/// GSUB / GPOS テーブルの共通ヘッダを解析し、DefaultLangSys 経由で有効となる
/// FeatureIndex 列と各リストへのオフセットを返す。
fn parse_layout_table_header(data: &[u8], rec: TableRecord) -> Option<LayoutTableInfo> {
    let off = rec.offset as usize;
    let table_end_u64 = off as u64 + rec.length as u64;
    if table_end_u64 > data.len() as u64 || rec.length < 10 {
        return None;
    }
    let table_end = table_end_u64 as usize;

    let Ok(version) = read_u32(data, off) else {
        return None;
    };
    if version != 0x0001_0000 && version != 0x0001_0001 {
        return None;
    }
    let Ok(script_list_rel) = read_u16(data, off + 4) else {
        return None;
    };
    let Ok(feature_list_rel) = read_u16(data, off + 6) else {
        return None;
    };
    let Ok(lookup_list_rel) = read_u16(data, off + 8) else {
        return None;
    };
    let script_list_off = off.checked_add(script_list_rel as usize)?;
    let feature_list_off = off.checked_add(feature_list_rel as usize)?;
    let lookup_list_off = off.checked_add(lookup_list_rel as usize)?;
    if script_list_off > table_end
        || feature_list_off.saturating_add(2) > table_end
        || lookup_list_off > table_end
    {
        return None;
    }

    let script_off = find_script(data, script_list_off, table_end)?;
    let Ok(default_lang_sys_rel) = read_u16(data, script_off) else {
        return None;
    };
    if default_lang_sys_rel == 0 {
        return None;
    }
    let lang_sys_off = script_off.checked_add(default_lang_sys_rel as usize)?;
    if lang_sys_off.saturating_add(4) > table_end {
        return None;
    }

    let Ok(feature_count) = read_u16(data, lang_sys_off + 4) else {
        return None;
    };
    let mut feature_indices: Vec<u16> = Vec::new();
    for i in 0..feature_count as usize {
        let Ok(idx) = read_u16(data, lang_sys_off + 6 + i * 2) else {
            return None;
        };
        feature_indices.push(idx);
    }

    Some(LayoutTableInfo {
        table_end,
        feature_list_off,
        feature_indices,
        lookup_list_off,
    })
}

/// FeatureList を走査し、有効な feature ごとに (tag, lookup_indices) を返す。
/// FeatureIndex が FeatureCount を超える場合は無視する。
fn read_feature_lookups(
    data: &[u8],
    feature_list_off: usize,
    table_end: usize,
    total_features: u16,
    feature_indices: &[u16],
) -> Vec<(u32, Vec<u16>)> {
    let mut out: Vec<(u32, Vec<u16>)> = Vec::new();
    for &fi in feature_indices {
        if fi as usize >= total_features as usize {
            continue;
        }
        let Some(feature_record_off) = feature_list_off.checked_add(2 + (fi as usize) * 6) else {
            continue;
        };
        if feature_record_off.saturating_add(6) > table_end {
            continue;
        }
        let Ok(tag) = read_u32(data, feature_record_off) else {
            continue;
        };
        let Ok(feature_table_rel) = read_u16(data, feature_record_off + 4) else {
            continue;
        };
        let Some(feature_table_off) = feature_list_off.checked_add(feature_table_rel as usize)
        else {
            continue;
        };
        if feature_table_off.saturating_add(4) > table_end {
            continue;
        }
        let Ok(lookup_index_count) = read_u16(data, feature_table_off + 2) else {
            continue;
        };
        let records_end = feature_table_off
            .saturating_add(4)
            .saturating_add(lookup_index_count as usize * 2);
        if records_end > table_end {
            continue;
        }
        let mut lookup_indices: Vec<u16> = Vec::new();
        for i in 0..lookup_index_count as usize {
            let Ok(idx) = read_u16(data, feature_table_off + 4 + i * 2) else {
                continue;
            };
            lookup_indices.push(idx);
        }
        out.push((tag, lookup_indices));
    }
    out
}

/// GSUB テーブルをパースする。
///
/// Microsoft OpenType spec "GSUB — Glyph Substitution Table" の
/// Section 4 (GSUB Header), Section 5 (Lookup Type 1: Single Substitution),
/// Section 8 (Lookup Type 4: Ligature Substitution),
/// Section 11 (Lookup Type 7: Extension Substitution) に準拠。
fn parse_gsub(data: &[u8], rec: TableRecord) -> GsubTable {
    let mut out = GsubTable::default();
    let Some(info) = parse_layout_table_header(data, rec) else {
        return out;
    };

    let Ok(total_features) = read_u16(data, info.feature_list_off) else {
        return out;
    };
    let feature_lookups = read_feature_lookups(
        data,
        info.feature_list_off,
        info.table_end,
        total_features,
        &info.feature_indices,
    );
    let mut applicable_lookup_set: std::collections::HashSet<(u32, u16)> =
        std::collections::HashSet::new();
    for (tag, lookup_indices) in feature_lookups {
        if tag == TAG_LIGA || tag == TAG_CLIG {
            for idx in lookup_indices {
                if applicable_lookup_set.insert((tag, idx)) {
                    out.applicable_lookups.push((tag, idx));
                }
            }
        }
    }

    if info.lookup_list_off.saturating_add(2) > info.table_end {
        return out;
    }
    let Ok(lookup_count) = read_u16(data, info.lookup_list_off) else {
        return out;
    };
    for i in 0..lookup_count as usize {
        let Some(rel_off) = info.lookup_list_off.checked_add(2 + i * 2) else {
            continue;
        };
        if rel_off.saturating_add(2) > info.table_end {
            continue;
        }
        let Ok(rel) = read_u16(data, rel_off) else {
            continue;
        };
        let Some(lookup_off) = info.lookup_list_off.checked_add(rel as usize) else {
            continue;
        };
        out.lookups
            .push(parse_gsub_lookup(data, lookup_off, info.table_end));
    }

    out
}

/// GPOS テーブルをパースする。
///
/// Microsoft OpenType spec "GPOS — Glyph Positioning Table" の
/// Section 4 (GPOS Header), Section 5 (Lookup Type 1: Single Adjustment),
/// Section 6 (Lookup Type 2: Pair Adjustment),
/// Section 8 (Lookup Type 9: Extension Positioning) に準拠。
fn parse_gpos(data: &[u8], rec: TableRecord) -> GposTable {
    let mut out = GposTable::default();
    let Some(info) = parse_layout_table_header(data, rec) else {
        return out;
    };

    let Ok(total_features) = read_u16(data, info.feature_list_off) else {
        return out;
    };
    let feature_lookups = read_feature_lookups(
        data,
        info.feature_list_off,
        info.table_end,
        total_features,
        &info.feature_indices,
    );
    let mut kern_lookup_set: std::collections::HashSet<u16> = std::collections::HashSet::new();
    for (tag, lookup_indices) in feature_lookups {
        if tag == TAG_KERN {
            for idx in lookup_indices {
                if kern_lookup_set.insert(idx) {
                    out.kern_lookups.push(idx);
                }
            }
        }
    }

    if info.lookup_list_off.saturating_add(2) > info.table_end {
        return out;
    }
    let Ok(lookup_count) = read_u16(data, info.lookup_list_off) else {
        return out;
    };
    for i in 0..lookup_count as usize {
        let Some(rel_off) = info.lookup_list_off.checked_add(2 + i * 2) else {
            continue;
        };
        if rel_off.saturating_add(2) > info.table_end {
            continue;
        }
        let Ok(rel) = read_u16(data, rel_off) else {
            continue;
        };
        let Some(lookup_off) = info.lookup_list_off.checked_add(rel as usize) else {
            continue;
        };
        out.lookups
            .push(parse_gpos_lookup(data, lookup_off, info.table_end));
    }

    out
}

/// 'DFLT' (0x44464C54) を優先、なければ 'latn' (0x6C61746E) を採用する。
fn find_script(data: &[u8], script_list_off: usize, table_end: usize) -> Option<usize> {
    if script_list_off.saturating_add(2) > table_end {
        return None;
    }
    let Ok(script_count) = read_u16(data, script_list_off) else {
        return None;
    };
    let mut dflt_off: Option<usize> = None;
    let mut latn_off: Option<usize> = None;
    for i in 0..script_count as usize {
        let Some(record_off) = script_list_off.checked_add(2 + i * 6) else {
            continue;
        };
        if record_off.saturating_add(6) > table_end {
            continue;
        }
        let Ok(tag) = read_u32(data, record_off) else {
            continue;
        };
        let Ok(rel) = read_u16(data, record_off + 4) else {
            continue;
        };
        let Some(script_off) = script_list_off.checked_add(rel as usize) else {
            continue;
        };
        if script_off.saturating_add(2) > table_end {
            continue;
        }
        match tag {
            TAG_DFLT => dflt_off = Some(script_off),
            TAG_LATN => latn_off = Some(script_off),
            _ => {}
        }
    }
    dflt_off.or(latn_off)
}

/// GSUB Lookup をパースする。
///
/// Microsoft OpenType spec "GSUB — Glyph Substitution Table" の
/// Section 3 (Lookup Table), Section 5 (Single Substitution),
/// Section 8 (Ligature Substitution), Section 11 (Extension Substitution) に準拠。
fn parse_gsub_lookup(data: &[u8], lookup_off: usize, table_end: usize) -> GsubLookup {
    // Lookup テーブルヘッダ (type + flag + subtableCount + 最初の subtable offset) は
    // 最低 8 バイト必要。table_end を超える offset は不正として Unsupported に縮退する。
    if lookup_off.saturating_add(8) > table_end {
        return GsubLookup::Unsupported;
    }
    let Ok(lookup_type) = read_u16(data, lookup_off) else {
        return GsubLookup::Unsupported;
    };
    // LookupFlag (L1 skip 等) は本実装では未使用だが、オフセット計算のために読み飛ばす。
    let _lookup_flag = read_u16(data, lookup_off + 2);
    let Ok(subtable_count) = read_u16(data, lookup_off + 4) else {
        return GsubLookup::Unsupported;
    };
    if subtable_count == 0 {
        return GsubLookup::Unsupported;
    }

    let mut singles: Vec<GsubSingleSubst> = Vec::new();
    let mut ligatures: Vec<GsubLigatureSubst> = Vec::new();
    for i in 0..subtable_count as usize {
        let Some(rel_off) = lookup_off.checked_add(6 + i * 2) else {
            continue;
        };
        let Ok(rel) = read_u16(data, rel_off) else {
            continue;
        };
        let Some(subtable_off) = lookup_off.checked_add(rel as usize) else {
            continue;
        };
        if subtable_off.saturating_add(4) > table_end {
            continue;
        }
        match lookup_type {
            1 => {
                if let Some(single) = parse_gsub_single(data, subtable_off, table_end) {
                    singles.push(single);
                }
            }
            4 => {
                if let Some(ligature) = parse_gsub_ligature(data, subtable_off, table_end) {
                    ligatures.push(ligature);
                }
            }
            7 => {
                let Ok(ext_format) = read_u16(data, subtable_off) else {
                    continue;
                };
                if ext_format != 1 {
                    continue;
                }
                if subtable_off.saturating_add(8) > table_end {
                    continue;
                }
                let Ok(inner_lookup_type) = read_u16(data, subtable_off + 2) else {
                    continue;
                };
                let Ok(inner_rel) = read_u32(data, subtable_off + 4) else {
                    continue;
                };
                let Some(inner_subtable_off) = subtable_off.checked_add(inner_rel as usize) else {
                    continue;
                };
                if inner_subtable_off.saturating_add(4) > table_end {
                    continue;
                }
                match inner_lookup_type {
                    1 => {
                        if let Some(single) = parse_gsub_single(data, inner_subtable_off, table_end)
                        {
                            singles.push(single);
                        }
                    }
                    4 => {
                        if let Some(ligature) =
                            parse_gsub_ligature(data, inner_subtable_off, table_end)
                        {
                            ligatures.push(ligature);
                        }
                    }
                    _ => {}
                }
            }
            _ => return GsubLookup::Unsupported,
        }
    }

    if !singles.is_empty() {
        GsubLookup::Single(singles)
    } else if !ligatures.is_empty() {
        GsubLookup::Ligature(ligatures)
    } else {
        GsubLookup::Unsupported
    }
}

fn parse_gsub_single(
    data: &[u8],
    subtable_off: usize,
    table_end: usize,
) -> Option<GsubSingleSubst> {
    let Ok(format) = read_u16(data, subtable_off) else {
        return None;
    };
    let Ok(coverage_rel) = read_u16(data, subtable_off + 2) else {
        return None;
    };
    let coverage_off = subtable_off.checked_add(coverage_rel as usize)?;
    let coverage = parse_coverage(data, coverage_off, table_end);
    match format {
        1 => {
            let Ok(delta) = read_i16(data, subtable_off + 4) else {
                return None;
            };
            Some(GsubSingleSubst {
                coverage,
                format: GsubSingleSubstFormat::DeltaGlyphID(delta),
            })
        }
        2 => {
            let Ok(glyph_count) = read_u16(data, subtable_off + 4) else {
                return None;
            };
            // GlyphCount は Coverage の glyph 数と一致する必要がある。
            if glyph_count as usize != coverage.glyph_count() {
                return None;
            }
            let subs_end = subtable_off
                .saturating_add(6)
                .saturating_add(glyph_count as usize * 2);
            if subs_end > table_end {
                return None;
            }
            let mut subs: Vec<u16> = Vec::new();
            for i in 0..glyph_count as usize {
                let glyph_off = subtable_off.checked_add(6 + i * 2)?;
                let Ok(g) = read_u16(data, glyph_off) else {
                    return None;
                };
                subs.push(g);
            }
            Some(GsubSingleSubst {
                coverage,
                format: GsubSingleSubstFormat::SubstituteGlyphIDs(subs),
            })
        }
        _ => None,
    }
}

fn parse_gsub_ligature(
    data: &[u8],
    subtable_off: usize,
    table_end: usize,
) -> Option<GsubLigatureSubst> {
    let Ok(format) = read_u16(data, subtable_off) else {
        return None;
    };
    if format != 1 {
        return None;
    }
    let Ok(coverage_rel) = read_u16(data, subtable_off + 2) else {
        return None;
    };
    let coverage_off = subtable_off.checked_add(coverage_rel as usize)?;
    let coverage = parse_coverage(data, coverage_off, table_end);
    let Ok(ligature_set_count) = read_u16(data, subtable_off + 4) else {
        return None;
    };
    // LigatureSetCount は Coverage の glyph 数と一致する必要がある。
    if ligature_set_count as usize != coverage.glyph_count() {
        return None;
    }
    let mut ligature_sets: Vec<Vec<GsubLigature>> = Vec::new();
    for i in 0..ligature_set_count as usize {
        let set_rel_off = subtable_off.checked_add(6 + i * 2)?;
        let Ok(set_rel) = read_u16(data, set_rel_off) else {
            return None;
        };
        let set_off = subtable_off.checked_add(set_rel as usize)?;
        if set_off.saturating_add(2) > table_end {
            return None;
        }
        let Ok(ligature_count) = read_u16(data, set_off) else {
            return None;
        };
        let mut ligatures: Vec<GsubLigature> = Vec::new();
        for j in 0..ligature_count as usize {
            let lig_rel_off = set_off.checked_add(2 + j * 2)?;
            let Ok(lig_rel) = read_u16(data, lig_rel_off) else {
                return None;
            };
            let lig_off = set_off.checked_add(lig_rel as usize)?;
            if lig_off.saturating_add(4) > table_end {
                return None;
            }
            let Ok(ligature_glyph) = read_u16(data, lig_off) else {
                return None;
            };
            let Ok(component_count) = read_u16(data, lig_off + 2) else {
                return None;
            };
            // component_count は合字を構成する全グリフ数 (1 番目 + 2 番目以降)。
            // 仕様上 2 以上である必要がある。不正値はスキップする。
            if component_count < 2 {
                continue;
            }
            // component_count が残りバイト数から許容される最大値を超える場合、
            // 不正フォントとして ligature 全体を棄却する (DoS 耐性)。
            let remaining_after_header = table_end - (lig_off + 4);
            let max_component_count = (remaining_after_header / 2).saturating_add(1);
            if component_count as usize > max_component_count {
                return None;
            }
            let mut component_glyph_ids: Vec<u16> = Vec::new();
            for k in 0..(component_count as usize - 1) {
                let comp_off = lig_off.saturating_add(4 + k * 2);
                if comp_off.saturating_add(2) > table_end {
                    return None;
                }
                let Ok(g) = read_u16(data, comp_off) else {
                    return None;
                };
                component_glyph_ids.push(g);
            }
            ligatures.push(GsubLigature {
                ligature_glyph,
                component_glyph_ids,
            });
        }
        ligature_sets.push(ligatures);
    }
    Some(GsubLigatureSubst {
        coverage,
        ligature_sets,
    })
}

/// GPOS Lookup をパースする。
///
/// Microsoft OpenType spec "GPOS — Glyph Positioning Table" の
/// Section 3 (Lookup Table), Section 5 (Single Adjustment),
/// Section 6 (Pair Adjustment), Section 8 (Extension Positioning) に準拠。
fn parse_gpos_lookup(data: &[u8], lookup_off: usize, table_end: usize) -> GposLookup {
    if lookup_off.saturating_add(8) > table_end {
        return GposLookup::Unsupported;
    }
    let Ok(lookup_type) = read_u16(data, lookup_off) else {
        return GposLookup::Unsupported;
    };
    // LookupFlag (L1 skip 等) は本実装では未使用だが、オフセット計算のために読み飛ばす。
    let _lookup_flag = read_u16(data, lookup_off + 2);
    let Ok(subtable_count) = read_u16(data, lookup_off + 4) else {
        return GposLookup::Unsupported;
    };
    if subtable_count == 0 {
        return GposLookup::Unsupported;
    }

    let mut singles: Vec<GposSingleAdjust> = Vec::new();
    let mut pairs: Vec<GposPairAdjust> = Vec::new();
    for i in 0..subtable_count as usize {
        let Some(rel_off) = lookup_off.checked_add(6 + i * 2) else {
            continue;
        };
        let Ok(rel) = read_u16(data, rel_off) else {
            continue;
        };
        let Some(subtable_off) = lookup_off.checked_add(rel as usize) else {
            continue;
        };
        if subtable_off.saturating_add(4) > table_end {
            continue;
        }
        match lookup_type {
            1 => {
                if let Some(single) = parse_gpos_single(data, subtable_off, table_end) {
                    singles.push(single);
                }
            }
            2 => {
                if let Some(pair) = parse_gpos_pair(data, subtable_off, table_end) {
                    pairs.push(pair);
                }
            }
            9 => {
                let Ok(ext_format) = read_u16(data, subtable_off) else {
                    continue;
                };
                if ext_format != 1 {
                    continue;
                }
                if subtable_off.saturating_add(8) > table_end {
                    continue;
                }
                let Ok(inner_lookup_type) = read_u16(data, subtable_off + 2) else {
                    continue;
                };
                let Ok(inner_rel) = read_u32(data, subtable_off + 4) else {
                    continue;
                };
                let Some(inner_subtable_off) = subtable_off.checked_add(inner_rel as usize) else {
                    continue;
                };
                if inner_subtable_off.saturating_add(4) > table_end {
                    continue;
                }
                match inner_lookup_type {
                    1 => {
                        if let Some(single) = parse_gpos_single(data, inner_subtable_off, table_end)
                        {
                            singles.push(single);
                        }
                    }
                    2 => {
                        if let Some(pair) = parse_gpos_pair(data, inner_subtable_off, table_end) {
                            pairs.push(pair);
                        }
                    }
                    _ => {}
                }
            }
            _ => return GposLookup::Unsupported,
        }
    }

    if !singles.is_empty() {
        GposLookup::Single(singles)
    } else if !pairs.is_empty() {
        GposLookup::Pair(pairs)
    } else {
        GposLookup::Unsupported
    }
}

fn parse_gpos_single(
    data: &[u8],
    subtable_off: usize,
    table_end: usize,
) -> Option<GposSingleAdjust> {
    let Ok(format) = read_u16(data, subtable_off) else {
        return None;
    };
    let Ok(coverage_rel) = read_u16(data, subtable_off + 2) else {
        return None;
    };
    let coverage_off = subtable_off.checked_add(coverage_rel as usize)?;
    let coverage = parse_coverage(data, coverage_off, table_end);
    let Ok(value_format) = read_u16(data, subtable_off + 4) else {
        return None;
    };
    // Device Table オフセット (bits 4-7) までは読み飛ばすが、それ以上の未知ビットは
    // 未対応とする。
    if value_format & !0x00FF != 0 {
        return None;
    }
    let value_size = value_record_expected_size(value_format);
    match format {
        1 => {
            let value_end = subtable_off.saturating_add(6).saturating_add(value_size);
            if value_end > table_end {
                return None;
            }
            let (vr, advanced) = read_value_record(data, subtable_off + 6, value_format);
            if advanced != value_size {
                return None;
            }
            Some(GposSingleAdjust {
                coverage,
                format: GposSingleAdjustFormat::Uniform(vr),
            })
        }
        2 => {
            let Ok(value_count) = read_u16(data, subtable_off + 6) else {
                return None;
            };
            // Format 2 の ValueCount は Coverage の glyph 数と一致する必要がある。
            if value_count as usize != coverage.glyph_count() {
                return None;
            }
            let values_end = subtable_off
                .saturating_add(8)
                .saturating_add(value_count as usize * value_size);
            if values_end > table_end {
                return None;
            }
            let mut values: Vec<ValueRecord> = Vec::new();
            let mut cursor = subtable_off.checked_add(8)?;
            for _ in 0..value_count as usize {
                let (vr, advanced) = read_value_record(data, cursor, value_format);
                if advanced != value_size {
                    return None;
                }
                values.push(vr);
                cursor = cursor.checked_add(advanced)?;
            }
            Some(GposSingleAdjust {
                coverage,
                format: GposSingleAdjustFormat::PerGlyph(values),
            })
        }
        _ => None,
    }
}

fn parse_gpos_pair(data: &[u8], subtable_off: usize, table_end: usize) -> Option<GposPairAdjust> {
    let Ok(format) = read_u16(data, subtable_off) else {
        return None;
    };
    let Ok(coverage_rel) = read_u16(data, subtable_off + 2) else {
        return None;
    };
    let coverage_off = subtable_off.checked_add(coverage_rel as usize)?;
    let coverage = parse_coverage(data, coverage_off, table_end);
    let Ok(value_format_1) = read_u16(data, subtable_off + 4) else {
        return None;
    };
    let Ok(value_format_2) = read_u16(data, subtable_off + 6) else {
        return None;
    };
    // Device Table オフセット (bits 4-7) までは読み飛ばすが、それ以上の未知ビットは
    // 未対応とする。
    if value_format_1 & !0x00FF != 0 || value_format_2 & !0x00FF != 0 {
        return None;
    }
    let value1_size = value_record_expected_size(value_format_1);
    let value2_size = value_record_expected_size(value_format_2);

    match format {
        1 => {
            let Ok(pair_set_count) = read_u16(data, subtable_off + 8) else {
                return None;
            };
            // Format 1 の PairSetCount は Coverage の glyph 数と一致する必要がある。
            if pair_set_count as usize != coverage.glyph_count() {
                return None;
            }
            let pair_set_array_end = subtable_off
                .saturating_add(10)
                .saturating_add(pair_set_count as usize * 2);
            if pair_set_array_end > table_end {
                return None;
            }
            let mut sets: Vec<Vec<GposPair>> = Vec::new();
            for i in 0..pair_set_count as usize {
                let set_rel_off = subtable_off.checked_add(10 + i * 2)?;
                let Ok(set_rel) = read_u16(data, set_rel_off) else {
                    return None;
                };
                let set_off = subtable_off.checked_add(set_rel as usize)?;
                if set_off.saturating_add(2) > table_end {
                    return None;
                }
                let Ok(pair_value_count) = read_u16(data, set_off) else {
                    return None;
                };
                // 各 PairValueRecord は second_glyph (2) + value1 + value2。
                let record_size = 2usize
                    .checked_add(value1_size)
                    .and_then(|v| v.checked_add(value2_size))?;
                let set_records_end = set_off
                    .saturating_add(2)
                    .saturating_add(pair_value_count as usize * record_size);
                if set_records_end > table_end {
                    return None;
                }
                // pair_value_count が残りバイト数から許容される最大値を超える場合、
                // 不正フォントとして PairSet 全体を棄却する (DoS 耐性)。
                let remaining_after_count = table_end - (set_off + 2);
                let max_pair_value_count = remaining_after_count / record_size;
                if pair_value_count as usize > max_pair_value_count {
                    return None;
                }
                let mut pairs: Vec<GposPair> = Vec::new();
                let mut cursor = set_off.checked_add(2)?;
                for _ in 0..pair_value_count as usize {
                    let Ok(second_glyph) = read_u16(data, cursor) else {
                        return None;
                    };
                    let value1_off = cursor.checked_add(2)?;
                    let (value1, advanced1) = read_value_record(data, value1_off, value_format_1);
                    if advanced1 != value1_size {
                        return None;
                    }
                    let value2_off = value1_off.checked_add(advanced1)?;
                    let (value2, advanced2) = read_value_record(data, value2_off, value_format_2);
                    if advanced2 != value2_size {
                        return None;
                    }
                    pairs.push(GposPair {
                        second_glyph,
                        value1,
                        value2,
                    });
                    let advanced = 2usize
                        .checked_add(advanced1)
                        .and_then(|v| v.checked_add(advanced2))?;
                    cursor = cursor.checked_add(advanced)?;
                }
                sets.push(pairs);
            }
            Some(GposPairAdjust {
                coverage,
                format: GposPairAdjustFormat::PairSet(sets),
            })
        }
        2 => {
            let Ok(rel_cd1) = read_u16(data, subtable_off + 8) else {
                return None;
            };
            let Ok(rel_cd2) = read_u16(data, subtable_off + 10) else {
                return None;
            };
            let class_def_1_off = subtable_off.checked_add(rel_cd1 as usize)?;
            let class_def_2_off = subtable_off.checked_add(rel_cd2 as usize)?;
            let Ok(class1_count) = read_u16(data, subtable_off + 12) else {
                return None;
            };
            let Ok(class2_count) = read_u16(data, subtable_off + 14) else {
                return None;
            };
            let class_def_1 = parse_class_def(data, class_def_1_off, table_end);
            let class_def_2 = parse_class_def(data, class_def_2_off, table_end);
            let total = (class1_count as u32)
                .checked_mul(class2_count as u32)
                .map(|v| v as usize)?;
            let record_size = value1_size.saturating_add(value2_size);
            // valueFormat1 / valueFormat2 が両方 0 の場合、レコードサイズは 0 となる。
            // その場合でも class1_count * class2_count は巨大になりうるため、
            // 値のないレコードを膨大に push しないよう空の records で early return する。
            if record_size == 0 {
                return Some(GposPairAdjust {
                    coverage,
                    format: GposPairAdjustFormat::Class {
                        class_def_1,
                        class_def_2,
                        class1_count,
                        class2_count,
                        records: Vec::new(),
                    },
                });
            }
            let total_bytes = total.checked_mul(record_size)?;
            let records_end = subtable_off
                .checked_add(16)
                .and_then(|off| off.checked_add(total_bytes))?;
            if records_end > table_end {
                return None;
            }
            let mut records: Vec<(ValueRecord, ValueRecord)> = Vec::new();
            let mut cursor = subtable_off.checked_add(16)?;
            for _ in 0..total {
                let (vr1, advanced1) = read_value_record(data, cursor, value_format_1);
                if advanced1 != value1_size {
                    return None;
                }
                cursor = cursor.checked_add(advanced1)?;
                let (vr2, advanced2) = read_value_record(data, cursor, value_format_2);
                if advanced2 != value2_size {
                    return None;
                }
                cursor = cursor.checked_add(advanced2)?;
                records.push((vr1, vr2));
            }
            Some(GposPairAdjust {
                coverage,
                format: GposPairAdjustFormat::Class {
                    class_def_1,
                    class_def_2,
                    class1_count,
                    class2_count,
                    records,
                },
            })
        }
        _ => None,
    }
}

/// `value_format` 内のセットされているビット数に応じた `ValueRecord` の固定サイズを返す。
/// Device Table オフセット (bits 4-7) も 2 バイトずつ占有する。
fn value_record_expected_size(value_format: u16) -> usize {
    (0..8).filter(|bit| value_format & (1 << bit) != 0).count() * 2
}

/// Coverage テーブルをパースする。
///
/// Microsoft OpenType spec "OpenType Layout Common Table Formats" の
/// "Coverage table" (Format 1 / Format 2) に準拠。
///
/// 読み取り途中でデータが尽きた場合、それまでに読めた Coverage エントリを
/// 返す (寛容方針)。呼び出し側は `glyph_count()` が期待値と一致しない可能性を
/// 考慮する。
fn parse_coverage(data: &[u8], off: usize, table_end: usize) -> Coverage {
    if off.saturating_add(2) > table_end {
        return Coverage::GlyphList(Vec::new());
    }
    let Ok(format) = read_u16(data, off) else {
        return Coverage::GlyphList(Vec::new());
    };
    match format {
        1 => {
            if off.saturating_add(4) > table_end {
                return Coverage::GlyphList(Vec::new());
            }
            let Ok(glyph_count) = read_u16(data, off + 2) else {
                return Coverage::GlyphList(Vec::new());
            };
            let mut glyphs: Vec<u16> = Vec::new();
            for i in 0..glyph_count as usize {
                let glyph_off = off.saturating_add(4 + i * 2);
                if glyph_off.saturating_add(2) > table_end {
                    return Coverage::GlyphList(glyphs);
                }
                let Ok(g) = read_u16(data, glyph_off) else {
                    return Coverage::GlyphList(glyphs);
                };
                glyphs.push(g);
            }
            Coverage::GlyphList(glyphs)
        }
        2 => {
            if off.saturating_add(4) > table_end {
                return Coverage::RangeList(Vec::new());
            }
            let Ok(range_count) = read_u16(data, off + 2) else {
                return Coverage::RangeList(Vec::new());
            };
            let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
            for i in 0..range_count as usize {
                let base = off.saturating_add(4 + i * 6);
                if base.saturating_add(6) > table_end {
                    return Coverage::RangeList(ranges);
                }
                let Ok(start) = read_u16(data, base) else {
                    return Coverage::RangeList(ranges);
                };
                let Ok(end) = read_u16(data, base + 2) else {
                    return Coverage::RangeList(ranges);
                };
                let Ok(start_idx) = read_u16(data, base + 4) else {
                    return Coverage::RangeList(ranges);
                };
                ranges.push((start, end, start_idx));
            }
            Coverage::RangeList(ranges)
        }
        _ => Coverage::GlyphList(Vec::new()),
    }
}

/// ClassDef テーブルをパースする。
///
/// Microsoft OpenType spec "OpenType Layout Common Table Formats" の
/// "Class Definition table" (Format 1 / Format 2) に準拠。
///
/// 読み取り途中でデータが尽きた場合、それまでに読めた ClassDef エントリを
/// 返す (寛容方針)。呼び出し側は不完全な ClassDef を扱う可能性を考慮する。
fn parse_class_def(data: &[u8], off: usize, table_end: usize) -> ClassDef {
    if off.saturating_add(2) > table_end {
        return ClassDef::Format2 { ranges: Vec::new() };
    }
    let Ok(format) = read_u16(data, off) else {
        return ClassDef::Format2 { ranges: Vec::new() };
    };
    match format {
        1 => {
            if off.saturating_add(6) > table_end {
                return ClassDef::Format2 { ranges: Vec::new() };
            }
            let Ok(start_glyph) = read_u16(data, off + 2) else {
                return ClassDef::Format2 { ranges: Vec::new() };
            };
            let Ok(glyph_count) = read_u16(data, off + 4) else {
                return ClassDef::Format2 { ranges: Vec::new() };
            };
            let mut class_values: Vec<u16> = Vec::new();
            for i in 0..glyph_count as usize {
                let class_off = off.saturating_add(6 + i * 2);
                if class_off.saturating_add(2) > table_end {
                    return ClassDef::Format1 {
                        start_glyph,
                        class_values,
                    };
                }
                let Ok(c) = read_u16(data, class_off) else {
                    return ClassDef::Format1 {
                        start_glyph,
                        class_values,
                    };
                };
                class_values.push(c);
            }
            ClassDef::Format1 {
                start_glyph,
                class_values,
            }
        }
        2 => {
            if off.saturating_add(4) > table_end {
                return ClassDef::Format2 { ranges: Vec::new() };
            }
            let Ok(range_count) = read_u16(data, off + 2) else {
                return ClassDef::Format2 { ranges: Vec::new() };
            };
            let mut ranges: Vec<(u16, u16, u16)> = Vec::new();
            for i in 0..range_count as usize {
                let base = off.saturating_add(4 + i * 6);
                if base.saturating_add(6) > table_end {
                    return ClassDef::Format2 { ranges };
                }
                let Ok(start) = read_u16(data, base) else {
                    return ClassDef::Format2 { ranges };
                };
                let Ok(end) = read_u16(data, base + 2) else {
                    return ClassDef::Format2 { ranges };
                };
                let Ok(class) = read_u16(data, base + 4) else {
                    return ClassDef::Format2 { ranges };
                };
                ranges.push((start, end, class));
            }
            ClassDef::Format2 { ranges }
        }
        _ => ClassDef::Format2 { ranges: Vec::new() },
    }
}

/// GPOS ValueRecord を読み取る。
///
/// Microsoft OpenType spec "GPOS — Glyph Positioning Table" の
/// Section 5 (Single Adjustment) / Section 6 (Pair Adjustment) で定義される
/// ValueRecord (XPlacement / YPlacement / XAdvance / YAdvance / Device Table offsets)
/// を `value_format` ビットマスクに従って読み取る。
///
/// 戻り値の `usize` は実際に消費したバイト数。読み取り途中でデータが尽きた場合、
/// それまでに読めたフィールドを含む `ValueRecord` と消費バイト数を返す (寛容方針)。
/// 呼び出し側は `advanced == value_record_expected_size(value_format)` で完全性を
/// 検証すること。
fn read_value_record(data: &[u8], off: usize, value_format: u16) -> (ValueRecord, usize) {
    let mut vr = ValueRecord::default();
    let mut cursor = off;
    if value_format & 0x0001 != 0 {
        let Ok(v) = read_i16(data, cursor) else {
            return (vr, cursor - off);
        };
        vr.x_placement = v;
        cursor += 2;
    }
    if value_format & 0x0002 != 0 {
        let Ok(v) = read_i16(data, cursor) else {
            return (vr, cursor - off);
        };
        vr.y_placement = v;
        cursor += 2;
    }
    if value_format & 0x0004 != 0 {
        let Ok(v) = read_i16(data, cursor) else {
            return (vr, cursor - off);
        };
        vr.x_advance = v;
        cursor += 2;
    }
    if value_format & 0x0008 != 0 {
        let Ok(v) = read_i16(data, cursor) else {
            return (vr, cursor - off);
        };
        vr.y_advance = v;
        cursor += 2;
    }
    // bits 4-7 (Device Table offsets) は読み飛ばし
    for bit in 4..8 {
        if value_format & (1 << bit) != 0 {
            cursor += 2;
        }
    }
    (vr, cursor - off)
}

// ---------------------------------------------------------------------------
// パース済みテーブル集合
// ---------------------------------------------------------------------------

/// フォントファイルからパースした全テーブル情報。
#[derive(Clone)]
pub(crate) struct ParsedTables {
    pub units_per_em: u16,
    #[expect(dead_code)]
    pub num_glyphs: u16,
    pub ascent: i16,
    pub descent: i16,
    pub line_gap: i16,
    pub cap_height: Option<i16>,
    pub x_height: Option<i16>,
    pub loca_offsets: Vec<u32>,
    pub glyf_offset: u32,
    #[expect(dead_code)]
    pub glyf_length: u32,
    pub cmap: CmapLookup,
    pub hmtx: HmtxTable,
    pub gsub: GsubTable,
    pub gpos: GposTable,
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
    let gsub = dir
        .find(TAG_GSUB)
        .map(|rec| parse_gsub(data, rec))
        .unwrap_or_default();
    let gpos = dir
        .find(TAG_GPOS)
        .map(|rec| parse_gpos(data, rec))
        .unwrap_or_default();
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
        gsub,
        gpos,
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
    /// length >= 90 の場合、sxHeight ( offset 86 ) = 16 、sCapHeight ( offset 88 ) = 32 を設定する。
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

    // -----------------------------------------------------------------------
    // GSUB / GPOS パーサの単体テスト
    // -----------------------------------------------------------------------

    fn be_u16(v: u16) -> [u8; 2] {
        v.to_be_bytes()
    }

    fn be_i16(v: i16) -> [u8; 2] {
        v.to_be_bytes()
    }

    #[test]
    fn parse_gsub_empty_returns_default() {
        let rec = TableRecord {
            offset: 0,
            length: 0,
        };
        let gsub = parse_gsub(&[], rec);
        assert!(gsub.applicable_lookups.is_empty());
        assert!(gsub.lookups.is_empty());
    }

    #[test]
    fn parse_gpos_empty_returns_default() {
        let rec = TableRecord {
            offset: 0,
            length: 0,
        };
        let gpos = parse_gpos(&[], rec);
        assert!(gpos.kern_lookups.is_empty());
        assert!(gpos.lookups.is_empty());
    }

    #[test]
    fn parse_coverage_format1() {
        let data = [
            be_u16(1), // format
            be_u16(2), // glyph count
            be_u16(10),
            be_u16(20),
        ]
        .concat();
        let cov = parse_coverage(&data, 0, data.len());
        assert_eq!(cov.glyph_count(), 2);
        assert_eq!(cov.index_of(10), Some(0));
        assert_eq!(cov.index_of(20), Some(1));
        assert_eq!(cov.index_of(15), None);
    }

    #[test]
    fn parse_coverage_format2() {
        let data = [
            be_u16(2), // format
            be_u16(1), // range count
            be_u16(5),
            be_u16(9),
            be_u16(0), // start coverage index
        ]
        .concat();
        let cov = parse_coverage(&data, 0, data.len());
        assert_eq!(cov.glyph_count(), 5);
        assert_eq!(cov.index_of(7), Some(2));
        assert_eq!(cov.index_of(4), None);
        assert_eq!(cov.index_of(10), None);
    }

    #[test]
    fn parse_class_def_format1() {
        let data = [
            be_u16(1), // format
            be_u16(3), // start glyph
            be_u16(2), // glyph count
            be_u16(0),
            be_u16(1),
        ]
        .concat();
        let cd = parse_class_def(&data, 0, data.len());
        assert_eq!(cd.class_of(3), 0);
        assert_eq!(cd.class_of(4), 1);
        assert_eq!(cd.class_of(5), 0);
    }

    #[test]
    fn parse_class_def_format2() {
        let data = [
            be_u16(2), // format
            be_u16(1), // range count
            be_u16(10),
            be_u16(15),
            be_u16(2), // class
        ]
        .concat();
        let cd = parse_class_def(&data, 0, data.len());
        assert_eq!(cd.class_of(9), 0);
        assert_eq!(cd.class_of(12), 2);
        assert_eq!(cd.class_of(16), 0);
    }

    #[test]
    fn read_value_record_reads_all_fields() {
        let data = [be_i16(1), be_i16(2), be_i16(3), be_i16(4)].concat();
        let (vr, advanced) = read_value_record(&data, 0, 0x000F);
        assert_eq!(vr.x_placement, 1);
        assert_eq!(vr.y_placement, 2);
        assert_eq!(vr.x_advance, 3);
        assert_eq!(vr.y_advance, 4);
        assert_eq!(advanced, 8);
    }

    #[test]
    fn parse_gsub_single_format1() {
        let data = [
            be_u16(1),  // format
            be_u16(6),  // coverage offset
            be_i16(10), // delta glyph id
            // coverage at offset 6
            be_u16(1),
            be_u16(1),
            be_u16(5),
        ]
        .concat();
        let single = parse_gsub_single(&data, 0, data.len()).expect("有効な single substitution");
        assert_eq!(single.coverage.index_of(5), Some(0));
        match single.format {
            GsubSingleSubstFormat::DeltaGlyphID(d) => assert_eq!(d, 10),
            _ => panic!("Format 1 であるはず"),
        }
    }

    #[test]
    fn parse_gsub_single_format2() {
        let data = [
            be_u16(2),  // format
            be_u16(10), // coverage offset
            be_u16(2),  // glyph count
            be_u16(100),
            be_u16(200), // substitute glyph ids
            // coverage at offset 10
            be_u16(1),
            be_u16(2),
            be_u16(1),
            be_u16(2),
        ]
        .concat();
        let single = parse_gsub_single(&data, 0, data.len()).expect("有効な single substitution");
        match single.format {
            GsubSingleSubstFormat::SubstituteGlyphIDs(v) => assert_eq!(v, vec![100, 200]),
            _ => panic!("Format 2 であるはず"),
        }
    }

    #[test]
    fn test_parse_gsub_ligature() {
        let data = [
            be_u16(1),  // format
            be_u16(8),  // coverage offset
            be_u16(1),  // ligature set count
            be_u16(14), // ligature set offset
            // coverage at offset 8
            be_u16(1),
            be_u16(1),
            be_u16(10),
            // ligature set at offset 14
            be_u16(1), // ligature count
            be_u16(8), // ligature offset (from set start)
            // padding to offset 22
            be_u16(0),
            be_u16(0),
            // ligature at offset 22
            be_u16(99), // ligature glyph
            be_u16(3),  // component count (including first glyph)
            be_u16(20),
            be_u16(30), // component glyph ids
        ]
        .concat();
        let lig = parse_gsub_ligature(&data, 0, data.len()).expect("有効な ligature substitution");
        assert_eq!(lig.coverage.index_of(10), Some(0));
        assert_eq!(lig.ligature_sets.len(), 1);
        let first = &lig.ligature_sets[0][0];
        assert_eq!(first.ligature_glyph, 99);
        assert_eq!(first.component_glyph_ids, vec![20, 30]);
    }

    #[test]
    fn parse_gpos_single_format1() {
        let data = [
            be_u16(1),      // format
            be_u16(10),     // coverage offset
            be_u16(0x0005), // value format: x_placement + x_advance
            be_i16(7),      // x_placement
            be_i16(11),     // x_advance
            // coverage at offset 10
            be_u16(1),
            be_u16(1),
            be_u16(5),
        ]
        .concat();
        let single = parse_gpos_single(&data, 0, data.len()).expect("有効な single adjustment");
        assert_eq!(single.coverage.index_of(5), Some(0));
        match single.format {
            GposSingleAdjustFormat::Uniform(vr) => {
                assert_eq!(vr.x_placement, 7);
                assert_eq!(vr.x_advance, 11);
            }
            _ => panic!("Format 1 であるはず"),
        }
    }

    #[test]
    fn parse_gpos_pair_format1() {
        let data = [
            be_u16(1),      // format
            be_u16(12),     // coverage offset
            be_u16(0x0001), // value format 1: x_placement
            be_u16(0x0000), // value format 2
            be_u16(1),      // pair set count
            be_u16(18),     // pair set offset
            // coverage at offset 12
            be_u16(1),
            be_u16(1),
            be_u16(10),
            // pair set at offset 18
            be_u16(1),  // pair value count
            be_u16(20), // second glyph
            be_i16(-5), // value1 x_placement
        ]
        .concat();
        let pair = parse_gpos_pair(&data, 0, data.len()).expect("有効な pair adjustment");
        match pair.format {
            GposPairAdjustFormat::PairSet(sets) => {
                assert_eq!(sets.len(), 1);
                assert_eq!(sets[0][0].second_glyph, 20);
                assert_eq!(sets[0][0].value1.x_placement, -5);
            }
            _ => panic!("Format 1 であるはず"),
        }
    }

    #[test]
    fn parse_gpos_pair_format2() {
        let data = [
            be_u16(2),      // format
            be_u16(32),     // coverage offset
            be_u16(0x0004), // value format 1: x_advance
            be_u16(0x0000), // value format 2
            be_u16(20),     // class def 1 offset
            be_u16(26),     // class def 2 offset
            be_u16(2),      // class1 count
            be_u16(1),      // class2 count
            // class records at offset 16
            be_i16(-10),
            be_i16(0),
            // class def 1 at offset 20
            be_u16(1),
            be_u16(10),
            be_u16(1),
            be_u16(1),
            // class def 2 at offset 26
            be_u16(1),
            be_u16(20),
            be_u16(1),
            be_u16(1),
            // coverage at offset 32
            be_u16(1),
            be_u16(1),
            be_u16(10),
        ]
        .concat();
        let pair = parse_gpos_pair(&data, 0, data.len()).expect("有効な pair adjustment");
        match pair.format {
            GposPairAdjustFormat::Class {
                class1_count,
                class2_count,
                records,
                ..
            } => {
                assert_eq!(class1_count, 2);
                assert_eq!(class2_count, 1);
                assert_eq!(records.len(), 2);
                assert_eq!(records[0].0.x_advance, -10);
                assert_eq!(records[1].0.x_advance, 0);
            }
            _ => panic!("Format 2 であるはず"),
        }
    }
}
