/// Blend2D 準拠フォント機能。
///
/// TrueType アウトライン (glyf/loca) の最小限サポートを提供する。
/// グリフのアウトラインは既存の `fill_path` パイプラインで描画する。
pub(crate) mod glyph;
pub(crate) mod tables;

use std::sync::Arc;

use crate::api::path::Path;
use crate::font::tables::ParsedTables;

/// フォントエラー。
#[derive(Debug)]
pub enum FontError {
    Io(std::io::Error),
    InvalidData(&'static str),
}

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FontError::Io(e) => write!(f, "I/O error: {e}"),
            FontError::InvalidData(msg) => write!(f, "invalid font data: {msg}"),
        }
    }
}

impl std::error::Error for FontError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            FontError::Io(e) => Some(e),
            FontError::InvalidData(_) => None,
        }
    }
}

impl From<std::io::Error> for FontError {
    fn from(e: std::io::Error) -> Self {
        FontError::Io(e)
    }
}

/// フォントファイルのバイトデータ。
pub struct FontData {
    data: Vec<u8>,
}

impl FontData {
    /// ファイルからフォントデータを読み込む。
    pub fn from_file(path: &str) -> Result<Self, FontError> {
        let data = std::fs::read(path)?;
        Ok(Self { data })
    }

    /// バイト列からフォントデータを作成する。
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self { data: bytes }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// パース済みフォントフェイス。フォントデータから解析された情報を保持する。
pub struct FontFace {
    data: Arc<Vec<u8>>,
    tables: ParsedTables,
}

impl FontFace {
    /// FontData からフォントフェイスを作成する。
    ///
    /// `index` は TTC 内のフォントインデックス。単体フォントの場合は 0。
    pub fn from_data(font_data: &FontData, index: u32) -> Result<Self, FontError> {
        let data = Arc::new(font_data.data.clone());
        let tables = tables::parse_all(&data, index)?;
        Ok(Self { data, tables })
    }

    pub fn units_per_em(&self) -> u16 {
        self.tables.units_per_em
    }

    pub fn ascent(&self) -> i16 {
        self.tables.ascent
    }

    pub fn descent(&self) -> i16 {
        self.tables.descent
    }

    pub fn line_gap(&self) -> i16 {
        self.tables.line_gap
    }

    /// OS/2 テーブル v2 以上の sCapHeight ( デザインユニット ) 。
    /// OS/2 不在 / ( v < 2 かつテーブル長 >= 78 ) / ( v2+ かつ 78 <= テーブル長 < 90 ) のいずれかで None 。
    /// テーブル長 < 78 の場合は FontFace::from_data がエラーになる。
    pub fn cap_height(&self) -> Option<i16> {
        self.tables.cap_height
    }

    /// OS/2 テーブル v2 以上の sxHeight ( デザインユニット ) 。
    /// None の条件は cap_height と同じ。
    pub fn x_height(&self) -> Option<i16> {
        self.tables.x_height
    }

    /// グリフ境界ボックス (デザインユニット、Y up、baseline 原点)。
    ///
    /// glyf テーブルのヘッダに記録された xMin / yMin / xMax / yMax (各 i16) を
    /// f64 にキャストして返す。点座標展開なしでヘッダから直接読む。
    /// 空グリフ / glyph_id 範囲外 / glyf 範囲外 / ヘッダ不足 /
    /// Compound Glyph bbox が `(0, 0, 0, 0)` のいずれかで `None` を返す
    /// (寛容方針。`append_glyph_outline` の `InvalidData` 経路も `None` に吸収する意図的差異)。
    pub fn glyph_bounds(&self, glyph_id: u16) -> Option<GlyphBounds> {
        let glyph_data = glyph::glyph_entry_slice(glyph_id, &self.tables, &self.data)
            .ok()
            .flatten()?;
        let raw = glyph::glyph_bbox_raw(glyph_data)?;
        Some(GlyphBounds::from_raw_scaled(raw, 1.0))
    }
}

/// サイズ指定済みフォント。描画に使用する。
pub struct Font {
    face: Arc<FontFaceInner>,
    size: f64,
    scale: f64,
}

/// FontFace の内部共有データ。
struct FontFaceInner {
    data: Arc<Vec<u8>>,
    tables: ParsedTables,
}

impl Font {
    /// FontFace とサイズからフォントを作成する。
    pub fn from_face(face: &FontFace, size: f64) -> Self {
        let scale = size / face.tables.units_per_em as f64;
        let inner = Arc::new(FontFaceInner {
            data: Arc::clone(&face.data),
            tables: face.tables.clone(),
        });
        Self {
            face: inner,
            size,
            scale,
        }
    }

    pub fn size(&self) -> f64 {
        self.size
    }

    pub fn scale(&self) -> f64 {
        self.scale
    }

    /// 文字 (Unicode コードポイント) をグリフ ID にマッピングする。
    pub fn map_char_to_glyph(&self, ch: char) -> u16 {
        self.face.tables.cmap.map(ch as u32)
    }

    /// グリフの水平アドバンス幅 (ピクセル単位) を返す。
    pub fn glyph_advance(&self, glyph_id: u16) -> f64 {
        let gid = glyph_id as usize;
        let aw = if gid < self.face.tables.hmtx.advance_widths.len() {
            self.face.tables.hmtx.advance_widths[gid]
        } else {
            0
        };
        aw as f64 * self.scale
    }

    /// グリフのアウトラインを Path に追加する。
    pub fn append_glyph_outline(
        &self,
        glyph_id: u16,
        offset_x: f64,
        offset_y: f64,
        path: &mut Path,
    ) -> Result<(), FontError> {
        glyph::append_glyph_outline(
            glyph_id,
            offset_x,
            offset_y,
            self.scale,
            &self.face.tables,
            &self.face.data,
            path,
        )
    }

    /// アセント (ピクセル単位)。
    pub fn ascent(&self) -> f64 {
        self.face.tables.ascent as f64 * self.scale
    }

    /// ディセント (ピクセル単位、負値)。
    pub fn descent(&self) -> f64 {
        self.face.tables.descent as f64 * self.scale
    }

    /// 行間 ( ピクセル単位 ) 。 hhea 必須テーブル由来のため常に値を返す。
    pub fn line_gap(&self) -> f64 {
        self.face.tables.line_gap as f64 * self.scale
    }

    /// cap_height のスケール済み値 ( ピクセル単位 ) 。
    /// FontFace::cap_height() が None ならば None を返す。
    pub fn cap_height(&self) -> Option<f64> {
        self.face.tables.cap_height.map(|v| v as f64 * self.scale)
    }

    /// x_height のスケール済み値 ( ピクセル単位 ) 。
    /// FontFace::x_height() が None ならば None を返す。
    pub fn x_height(&self) -> Option<f64> {
        self.face.tables.x_height.map(|v| v as f64 * self.scale)
    }

    /// グリフ境界ボックス (スケール済みピクセル単位、Y up、baseline 原点)。
    ///
    /// 描画時に行われる Y 反転 (画面座標系への変換) はここでは適用しない。
    /// 問い合わせ API であり描画変換の責務を持たないため、呼び出し側で座標系を選択できる。
    /// `None` 条件は `FontFace::glyph_bounds` と同一。
    pub fn glyph_bounds(&self, glyph_id: u16) -> Option<GlyphBounds> {
        let glyph_data = glyph::glyph_entry_slice(glyph_id, &self.face.tables, &self.face.data)
            .ok()
            .flatten()?;
        let raw = glyph::glyph_bbox_raw(glyph_data)?;
        Some(GlyphBounds::from_raw_scaled(raw, self.scale))
    }

    /// 文字列を内部的なグリフ列に変換する。
    ///
    /// `(glyph_id, advance_in_pixels)` のペアを `buf` に push する。
    /// `advance_in_pixels` はスケール済み (ピクセル単位) かつカーニング非適用の単体値。
    ///
    /// `glyph_id == 0` (cmap 未マッピング) でも `glyph_advance` を計算して `buf` に
    /// push する。呼び出し側が他のグリフ位置を維持できるよう、advance 加算側で
    /// `glyph_id == 0` を除外させないことを意図している。改行や制御文字も通常文字と
    /// 同様に扱う (cmap 結果に依存)。複数行レイアウトは扱わない。
    ///
    /// バッファのクリアは本関数の冒頭で行うため、呼び出し側でのクリアは不要。
    pub(crate) fn glyph_run_for_text(&self, text: &str, buf: &mut Vec<(u16, f64)>) {
        buf.clear();
        for ch in text.chars() {
            let glyph_id = self.map_char_to_glyph(ch);
            let advance = self.glyph_advance(glyph_id);
            buf.push((glyph_id, advance));
        }
    }

    /// 文字列全体のスケール済み水平アドバンス幅と境界ボックスを計測する。
    ///
    /// 戻り値の `TextMetrics` には `advance` (ピクセル単位の総アドバンス、カーニング未適用) と
    /// `bounding_box` (`TextMetrics::bounding_box` 参照) を格納する。
    /// `glyph_id == 0` (`.notdef`) の bbox も union に含めるため、`Context::fill_text` の
    /// 実描画範囲 (`.notdef` はスキップ) より広くなることがある。
    ///
    /// `size == 0` のときは `scale == 0` を経由して全 advance が 0.0 となるため
    /// 自然に `advance == 0.0` を返す。`size` が NaN や非有限のときは IEEE 754
    /// 算術の結果がそのまま伝播する (`Font::from_face` は size の検証を行わない)。
    /// 改行や制御文字も `cmap` ルックアップ + advance 加算で扱う (複数行レイアウトなし)。
    pub fn measure_text(&self, text: &str) -> TextMetrics {
        let mut buf: Vec<(u16, f64)> = Vec::new();
        self.glyph_run_for_text(text, &mut buf);

        // cursor_x は「i 文字目開始時点までの advance 累積」。ループ内で i 文字目の bbox を
        // cursor_x で translated してから advance を加算するため、advance 加算は bbox 計算の
        // 後で行う必要がある。ループ終了時の cursor_x は全 advance の総和となる。
        let mut cursor_x = 0.0;
        let mut bbox: Option<GlyphBounds> = None;
        for &(glyph_id, advance) in buf.iter() {
            if let Some(gb) = self.glyph_bounds(glyph_id) {
                let translated = gb.translated_x(cursor_x);
                bbox = Some(match bbox {
                    None => translated,
                    Some(b) => b.union(translated),
                });
            }
            cursor_x += advance;
        }
        TextMetrics {
            advance: cursor_x,
            bounding_box: bbox,
        }
    }
}

/// グリフの境界ボックス。
///
/// `FontFace::glyph_bounds` からは font デザイン座標 (Y up、baseline 原点)、
/// `Font::glyph_bounds` からはスケール済みピクセル単位 (Y up、baseline 原点)
/// が入る。描画時の Y 反転は適用されない (問い合わせ API であり描画変換の責務を
/// 持たないため、呼び出し側で行う)。
/// `PartialEq` は 4 × f64 の比較であり、IEEE 754 上 `NaN != NaN` のため
/// NaN を含む値では `PartialEq` 比較が false を返す点に注意 (`TextMetrics`
/// と同じ注意)。
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct GlyphBounds {
    pub x_min: f64,
    pub y_min: f64,
    pub x_max: f64,
    pub y_max: f64,
}

impl GlyphBounds {
    /// 共通ヘルパー (`glyph_entry_slice` + `glyph_bbox_raw`) の戻り値からスケール済み
    /// bbox を構築する。`FontFace::glyph_bounds` は `scale = 1.0`、`Font::glyph_bounds`
    /// は `Font::scale()` の値を渡す。
    pub(crate) fn from_raw_scaled(raw: (i16, i16, i16, i16), scale: f64) -> Self {
        let (x_min, y_min, x_max, y_max) = raw;
        Self {
            x_min: x_min as f64 * scale,
            y_min: y_min as f64 * scale,
            x_max: x_max as f64 * scale,
            y_max: y_max as f64 * scale,
        }
    }

    /// X 軸方向に `dx` だけ平行移動した bbox を返す。
    /// Y 軸は baseline 原点のまま不変。
    pub(crate) fn translated_x(self, dx: f64) -> Self {
        Self {
            x_min: self.x_min + dx,
            y_min: self.y_min,
            x_max: self.x_max + dx,
            y_max: self.y_max,
        }
    }

    /// 2 つの bbox の union (各成分の min/max) を返す。
    pub(crate) fn union(self, other: Self) -> Self {
        Self {
            x_min: self.x_min.min(other.x_min),
            y_min: self.y_min.min(other.y_min),
            x_max: self.x_max.max(other.x_max),
            y_max: self.y_max.max(other.y_max),
        }
    }
}

/// 文字列全体のメトリクス (ピクセル単位)。
///
/// `advance` (水平総アドバンス) と `bounding_box` (Y up、baseline 原点、オプショナル)
/// を保持する。`#[non_exhaustive]` を付与しているため、将来のフィールド追加は外部
/// クレートに対して非破壊となる。
/// `advance: f64` および `GlyphBounds` 内 4 つの f64 は IEEE 754 上 `NaN != NaN`
/// のため、`PartialEq` 比較は NaN を含む値で false を返す点に注意。
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct TextMetrics {
    /// 文字列全体の水平アドバンス幅 (ピクセル単位)。
    pub advance: f64,
    /// 文字列全体の bbox (Y up、baseline 原点、ピクセル単位)。
    /// 各文字の `glyph_bounds` を advance 累積で水平オフセットして union を取った値。
    /// 空文字列、または全ての `glyph_bounds` が `None` のときは `None` を返す
    /// (「bbox が存在しない」を面積 0 の bbox と意図的に区別する)。
    /// `Path::bounding_box` (画面座標 Y down) とは座標系も意味も異なる。
    pub bounding_box: Option<GlyphBounds>,
}
