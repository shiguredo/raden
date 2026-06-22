/// Blend2D 準拠フォント機能。
///
/// TrueType アウトライン (glyf/loca) の最小限サポートを提供する。
/// グリフのアウトラインは既存の `fill_path` パイプラインで描画する。
pub(crate) mod glyph;
pub(crate) mod shape;
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
    pub(crate) data: Arc<Vec<u8>>,
    pub(crate) tables: Arc<ParsedTables>,
}

impl FontFace {
    /// FontData からフォントフェイスを作成する。
    ///
    /// `index` は TTC 内のフォントインデックス。単体フォントの場合は 0。
    pub fn from_data(font_data: &FontData, index: u32) -> Result<Self, FontError> {
        let data = Arc::new(font_data.data.clone());
        let tables = Arc::new(tables::parse_all(&data, index)?);
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

    /// OS/2 テーブル v2 以上の sCapHeight （デザインユニット）。
    /// OS/2 不在 / ( v < 2 かつテーブル長 >= 78 ) / ( v2+ かつ 78 <= テーブル長 < 90 ) のいずれかで None 。
    /// テーブル長 < 78 の場合は FontFace::from_data がエラーになる。
    pub fn cap_height(&self) -> Option<i16> {
        self.tables.cap_height
    }

    /// OS/2 テーブル v2 以上の sxHeight （デザインユニット）。
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
#[derive(Clone)]
pub struct Font {
    face: Arc<FontFace>,
    size: f64,
    scale: f64,
    feature_settings: FontFeatureSettings,
}

impl Font {
    /// FontFace とサイズ ・ feature 設定からフォントを作成する。
    pub fn with_features(face: &FontFace, size: f64, features: FontFeatureSettings) -> Self {
        let scale = size / face.tables.units_per_em as f64;
        let face = Arc::new(FontFace {
            data: Arc::clone(&face.data),
            tables: Arc::clone(&face.tables),
        });
        Self {
            face,
            size,
            scale,
            feature_settings: features,
        }
    }

    /// FontFace とサイズからフォントを作成する。
    pub fn from_face(face: &FontFace, size: f64) -> Self {
        Self::with_features(face, size, FontFeatureSettings::default())
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

    /// 行間（ピクセル単位）。hhea 必須テーブル由来のため常に値を返す。
    pub fn line_gap(&self) -> f64 {
        self.face.tables.line_gap as f64 * self.scale
    }

    /// cap_height のスケール済み値（ピクセル単位）。
    /// FontFace::cap_height() が None ならば None を返す。
    pub fn cap_height(&self) -> Option<f64> {
        self.face.tables.cap_height.map(|v| v as f64 * self.scale)
    }

    /// x_height のスケール済み値（ピクセル単位）。
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

    /// 現在の feature 設定を変更する。
    pub fn set_feature_settings(&mut self, features: FontFeatureSettings) {
        self.feature_settings = features;
    }

    /// 現在の feature 設定への参照を返す。
    pub fn feature_settings(&self) -> &FontFeatureSettings {
        &self.feature_settings
    }

    /// 同じ FontFace / size を持ち、feature 設定だけ異なる Font を複製する。
    pub fn clone_with_features(&self, features: FontFeatureSettings) -> Self {
        let mut clone = self.clone();
        clone.feature_settings = features;
        clone
    }

    /// 新規 GlyphBuffer に shape 結果を入れて返す (Blend2D `BLFont::shape` 互換寄り)。
    pub fn shape(&self, text: &str) -> GlyphBuffer {
        let mut buf = GlyphBuffer::default();
        self.shape_into(text, &mut buf);
        buf
    }

    /// 既存 GlyphBuffer を再利用して shape する (Context::fill_text / stroke_text 経路で使う)。
    pub fn shape_into(&self, text: &str, buf: &mut GlyphBuffer) {
        buf.clear();

        // 1. cmap で text → グリフ ID 列に変換 (cluster は UTF-8 byte index)
        for (byte_idx, ch) in text.char_indices() {
            let gid = self.map_char_to_glyph(ch);
            // `byte_idx` は `usize` だが、cluster は `u32` で保持する。
            // `u32::MAX` を超える巨大文字列では上位ビットが truncate される。
            buf.push_glyph(
                gid,
                GlyphPlacement {
                    offset_x: 0.0,
                    offset_y: 0.0,
                    advance: self.glyph_advance(gid),
                },
                byte_idx as u32,
            );
        }

        // 2. GSUB 適用
        shape::apply_gsub(&self.face.tables.gsub, buf, &self.feature_settings, self);

        // 3. GPOS 適用 (kern feature 有効時のみ)
        if self.feature_settings.kern {
            shape::apply_gpos_kern_feature_lookups(&self.face.tables.gpos, buf, self);
        }
    }

    /// 文字列全体のスケール済み水平アドバンス幅と境界ボックスを計測する。
    ///
    /// 戻り値の `TextMetrics` には `advance` (ピクセル単位の総アドバンス、
    /// OpenType GSUB / GPOS シェーピング適用済み) と `bounding_box`、
    /// `leading_bearing` / `trailing_bearing` を格納する。
    /// `glyph_id == 0` (`.notdef`) の bbox も union に含めるため、`Context::fill_text` の
    /// 実描画範囲 (`.notdef` はスキップ) より広くなることがある。
    ///
    /// `size == 0` のときは `scale == 0` を経由して全 advance が 0.0 となるため
    /// 自然に `advance == 0.0` を返す。`size` が NaN や非有限のときは IEEE 754
    /// 算術の結果がそのまま伝播する (`Font::from_face` は size の検証を行わない)。
    /// 改行や制御文字も `cmap` ルックアップ + advance 加算で扱う (複数行レイアウトなし)。
    pub fn measure_text(&self, text: &str) -> TextMetrics {
        let buffer = self.shape(text);
        let advance: f64 = buffer.placements.iter().map(|p| p.advance).sum();
        let bounding_box = compute_bounding_box(&buffer, self);
        let leading_bearing = compute_leading_bearing(&buffer, self);
        let trailing_bearing = compute_trailing_bearing(&buffer, self);
        TextMetrics {
            advance,
            bounding_box,
            leading_bearing,
            trailing_bearing,
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

    /// Y 軸方向に `dy` だけ平行移動した bbox を返す。
    /// X 軸は baseline 原点のまま不変。
    pub(crate) fn translated_y(self, dy: f64) -> Self {
        Self {
            x_min: self.x_min,
            y_min: self.y_min + dy,
            x_max: self.x_max,
            y_max: self.y_max + dy,
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
/// `advance` (水平総アドバンス) 、`bounding_box` (Y up、baseline 原点、オプショナル)、
/// `leading_bearing` / `trailing_bearing` を保持する。`#[non_exhaustive]` を付与して
/// いるため、将来のフィールド追加は外部クレートに対して非破壊となる。
/// `advance: f64` および `GlyphBounds` 内 4 つの f64 は IEEE 754 上 `NaN != NaN`
/// のため、`PartialEq` 比較は NaN を含む値で false を返す点に注意。
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct TextMetrics {
    /// 文字列全体の水平アドバンス幅 (ピクセル単位)。
    pub advance: f64,
    /// 文字列全体の bbox (Y up、baseline 原点、ピクセル単位)。
    /// 各文字の `glyph_bounds` を advance 累積 + placement offset でオフセットして
    /// union を取った値。
    /// 空文字列、または全ての `glyph_bounds` が `None` のときは `None` を返す
    /// (「bbox が存在しない」を面積 0 の bbox と意図的に区別する)。
    /// `Path::bounding_box` (画面座標 Y down) とは座標系も意味も異なる。
    pub bounding_box: Option<GlyphBounds>,
    /// 最初のグリフの左側ベアリング (ピクセル単位)。
    pub leading_bearing: f64,
    /// 最後のグリフの右側ベアリング (ピクセル単位)。
    pub trailing_bearing: f64,
}

/// OpenType feature 設定。
///
/// 現状 raden は GSUB / GPOS による基本シェーピングのみをサポートする。
/// `kern` には GPOS Pair Adjustment 由来のカーニングを含む。Microsoft 形式の
/// 古い `kern` テーブル (v0) には非対応。
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct FontFeatureSettings {
    /// カーニング (GPOS Pair Adjustment) を有効にする。
    pub kern: bool,
    /// 標準合字 (liga) を有効にする。
    pub liga: bool,
    /// 文脈依存合字 (clig) を有効にする。
    pub clig: bool,
}

impl Default for FontFeatureSettings {
    fn default() -> Self {
        // `Font::from_face` 経由で作られたフォントがデフォルトでシェーピングを
        // 有効にするため、全 feature ON をデフォルトとする。
        Self {
            kern: true,
            liga: true,
            clig: true,
        }
    }
}

impl FontFeatureSettings {
    /// 全 feature を無効にした設定を返す。
    pub fn none() -> Self {
        Self {
            kern: false,
            liga: false,
            clig: false,
        }
    }

    /// `kern` feature の on/off を設定した新しいインスタンスを返す。
    pub fn with_kern(mut self, enabled: bool) -> Self {
        self.kern = enabled;
        self
    }

    /// `liga` feature の on/off を設定した新しいインスタンスを返す。
    pub fn with_liga(mut self, enabled: bool) -> Self {
        self.liga = enabled;
        self
    }

    /// `clig` feature の on/off を設定した新しいインスタンスを返す。
    pub fn with_clig(mut self, enabled: bool) -> Self {
        self.clig = enabled;
        self
    }
}

/// シェーピング結果を格納するバッファ。
///
/// `Font::shape` / `Font::shape_into` により生成された場合、
/// `glyph_ids.len() == placements.len() == clusters.len()` の事後条件を満たす。
/// フィールドは `pub(crate)` であるため、raden クレート外からは直接変更できない。
/// 長さを変える操作は `push_glyph` / `drain_range` / `clear` 経由で行い、
/// 3 配列の長さ一致を保つ。
#[derive(Debug, Clone, Default, PartialEq)]
#[non_exhaustive]
pub struct GlyphBuffer {
    /// グリフ ID 列。
    pub(crate) glyph_ids: Vec<u16>,
    /// 各グリフの配置情報。
    pub(crate) placements: Vec<GlyphPlacement>,
    /// 各グリフが元テキストのどの UTF-8 byte index に対応するか。
    /// `u32::MAX` を超える byte index を持つ巨大な文字列では、上位ビットが
    /// truncate される。
    pub(crate) clusters: Vec<u32>,
}

impl GlyphBuffer {
    /// グリフ数を返す。
    pub fn len(&self) -> usize {
        self.glyph_ids.len()
    }

    /// バッファが空かどうかを返す。
    pub fn is_empty(&self) -> bool {
        self.glyph_ids.is_empty()
    }

    /// 指定インデックスのグリフ ID を返す。
    pub fn glyph_id(&self, index: usize) -> Option<u16> {
        self.glyph_ids.get(index).copied()
    }

    /// 指定インデックスの配置情報を返す。
    pub fn placement(&self, index: usize) -> Option<GlyphPlacement> {
        self.placements.get(index).copied()
    }

    /// 指定インデックスの cluster (UTF-8 byte index) を返す。
    pub fn cluster(&self, index: usize) -> Option<u32> {
        self.clusters.get(index).copied()
    }

    /// `(glyph_id, placement, cluster)` を順に走査するイテレータを返す。
    pub fn iter(&self) -> GlyphBufferIter<'_> {
        GlyphBufferIter {
            buffer: self,
            index: 0,
        }
    }

    /// 内部不変条件 `glyph_ids.len() == placements.len() == clusters.len()` を検証する。
    ///
    /// 通常 `Font::shape` / `Font::shape_into` 経由で生成されたバッファは常に満たすが、
    /// テスト等で内部状態を確認する用途で公開している。
    pub fn is_well_formed(&self) -> bool {
        self.glyph_ids.len() == self.placements.len()
            && self.placements.len() == self.clusters.len()
    }

    /// 3 配列を空にする。
    pub(crate) fn clear(&mut self) {
        self.glyph_ids.clear();
        self.placements.clear();
        self.clusters.clear();
    }

    /// グリフ ID、配置、cluster を 1 組追加する。
    pub(crate) fn push_glyph(&mut self, glyph_id: u16, placement: GlyphPlacement, cluster: u32) {
        self.glyph_ids.push(glyph_id);
        self.placements.push(placement);
        self.clusters.push(cluster);
    }

    /// 指定範囲 `[start, end)` の 3 配列を同時に削除する。
    pub(crate) fn drain_range(&mut self, start: usize, end: usize) {
        debug_assert!(start <= end, "drain_range: start ({start}) <= end ({end})");
        debug_assert!(
            end <= self.glyph_ids.len(),
            "drain_range: end ({end}) <= len ({})",
            self.glyph_ids.len()
        );
        self.glyph_ids.drain(start..end);
        self.placements.drain(start..end);
        self.clusters.drain(start..end);
    }
}

/// `GlyphBuffer::iter` が返すイテレータ。
#[derive(Debug, Clone)]
pub struct GlyphBufferIter<'a> {
    buffer: &'a GlyphBuffer,
    index: usize,
}

impl<'a> Iterator for GlyphBufferIter<'a> {
    type Item = (u16, GlyphPlacement, u32);

    fn next(&mut self) -> Option<Self::Item> {
        let gid = self.buffer.glyph_ids.get(self.index).copied()?;
        let placement = self.buffer.placements.get(self.index).copied()?;
        let cluster = self.buffer.clusters.get(self.index).copied()?;
        self.index += 1;
        Some((gid, placement, cluster))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.buffer.glyph_ids.len().saturating_sub(self.index);
        (len, Some(len))
    }
}

impl<'a> ExactSizeIterator for GlyphBufferIter<'a> {}

/// グリフ単位の配置情報。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[non_exhaustive]
pub struct GlyphPlacement {
    /// X 軸方向のオフセット (ピクセル単位)。
    pub offset_x: f64,
    /// Y 軸方向のオフセット (ピクセル単位、Y up)。
    pub offset_y: f64,
    /// 水平アドバンス (ピクセル単位)。
    pub advance: f64,
}

/// `GlyphBuffer` 全体の bbox を計算する。
///
/// 各グリフの `glyph_bounds` を「advance 累積 + placement offset」で平行移動し、
/// union を取る。`glyph_id == 0` (`.notdef`) も含める。
fn compute_bounding_box(buffer: &GlyphBuffer, font: &Font) -> Option<GlyphBounds> {
    debug_assert_eq!(buffer.glyph_ids.len(), buffer.placements.len());
    debug_assert_eq!(buffer.placements.len(), buffer.clusters.len());
    let mut cursor_x = 0.0;
    let mut bbox: Option<GlyphBounds> = None;
    for (gid, placement, _cluster) in buffer.iter() {
        if let Some(gb) = font.glyph_bounds(gid) {
            let translated = gb
                .translated_x(cursor_x + placement.offset_x)
                .translated_y(placement.offset_y);
            bbox = Some(match bbox {
                None => translated,
                Some(b) => b.union(translated),
            });
        }
        cursor_x += placement.advance;
    }
    bbox
}

/// 最初のグリフの左側ベアリングを計算する。
///
/// 最初のグリフの bbox x_min を placement offset と一緒に返す。bbox が None の
/// 場合は 0.0 を返す。
fn compute_leading_bearing(buffer: &GlyphBuffer, font: &Font) -> f64 {
    let Some((gid, placement)) = buffer.glyph_ids.first().zip(buffer.placements.first()) else {
        return 0.0;
    };
    font.glyph_bounds(*gid)
        .map(|gb| gb.x_min + placement.offset_x)
        .unwrap_or(0.0)
}

/// 最後のグリフの右側ベアリングを計算する。
///
/// 最後のグリフの advance から「右端までの距離 (offset_x + x_max)」を引いた
/// 余白を返す。bbox が None の場合は 0.0 を返す。
fn compute_trailing_bearing(buffer: &GlyphBuffer, font: &Font) -> f64 {
    let Some(last_idx) = buffer.glyph_ids.len().checked_sub(1) else {
        return 0.0;
    };
    let gid = buffer.glyph_ids[last_idx];
    let placement = buffer.placements[last_idx];
    font.glyph_bounds(gid)
        .map(|gb| placement.advance - placement.offset_x - gb.x_max)
        .unwrap_or(0.0)
}
