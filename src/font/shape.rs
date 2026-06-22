/// OpenType GSUB / GPOS シェーピングの適用ロジック。
use crate::font::tables::{
    GposLookup, GposPairAdjust, GposSingleAdjust, GposSingleAdjustFormat, GposTable, GsubLigature,
    GsubLigatureSubst, GsubLookup, GsubSingleSubst, GsubSingleSubstFormat, GsubTable, TAG_CLIG,
    TAG_LIGA, ValueRecord,
};
use crate::font::{Font, FontFeatureSettings, GlyphBuffer, GlyphPlacement};

/// GSUB Lookup (Single Substitution / Ligature Substitution) を適用する。
/// `parse_gsub` 時点で重複は除去済みのため、ここでは feature 有効判定のみ行う。
pub(crate) fn apply_gsub(
    gsub: &GsubTable,
    buf: &mut GlyphBuffer,
    features: &FontFeatureSettings,
    font: &Font,
) {
    for &(tag, idx) in &gsub.applicable_lookups {
        let enabled = match tag {
            TAG_LIGA => features.liga,
            TAG_CLIG => features.clig,
            _ => false,
        };
        if !enabled {
            continue;
        }
        if let Some(lookup) = gsub.lookup_at(idx) {
            apply_gsub_lookup(lookup, buf, font);
        }
    }
}

fn apply_gsub_lookup(lookup: &GsubLookup, buf: &mut GlyphBuffer, font: &Font) {
    match lookup {
        GsubLookup::Single(singles) => {
            for s in singles {
                apply_gsub_single(s, buf, font);
            }
        }
        GsubLookup::Ligature(ligatures) => {
            for l in ligatures {
                apply_gsub_ligature(l, buf, font);
            }
        }
        GsubLookup::Unsupported => {}
    }
}

fn apply_gsub_single(s: &GsubSingleSubst, buf: &mut GlyphBuffer, font: &Font) {
    for (glyph_id, placement) in buf.glyph_ids.iter_mut().zip(buf.placements.iter_mut()) {
        let Some(ci) = s.coverage.index_of(*glyph_id) else {
            continue;
        };
        let new_gid = match &s.format {
            GsubSingleSubstFormat::DeltaGlyphID(d) => {
                // Microsoft OpenType Spec GSUB Type 1 Format 1 は modulo 65536 wrap を仕様化。
                // i32 加算後の as u16 キャストは natural wrap と等価。
                ((*glyph_id as i32) + (*d as i32)) as u16
            }
            GsubSingleSubstFormat::SubstituteGlyphIDs(subs) => match subs.get(ci as usize) {
                Some(&sub) => sub,
                None => continue,
            },
        };
        *glyph_id = new_gid;
        placement.advance = font.glyph_advance(new_gid);
    }
}

fn apply_gsub_ligature(l: &GsubLigatureSubst, buf: &mut GlyphBuffer, font: &Font) {
    let mut i = 0;
    while i < buf.len() {
        let Some(ci) = l.coverage.index_of(buf.glyph_ids[i]) else {
            i += 1;
            continue;
        };
        let Some(ligatures) = l.ligature_sets.get(ci as usize) else {
            i += 1;
            continue;
        };

        // 各 Ligature を順に試し、マッチするものの中で最長の component 数を持つ
        // ものを採用する。Microsoft OpenType spec GSUB Lookup Type 4 は
        // LigatureSet 内を "preference order" で格納することを規定しているが、
        // すべてのフォントが longest match 順を保証するとは限らないため、
        // 実装側で最長マッチを選択することで正しいシェーピング結果を得る。
        let mut matched: Option<&GsubLigature> = None;
        for lig in ligatures {
            let n = lig.component_glyph_ids.len();
            let Some(end) = i.checked_add(1).and_then(|v| v.checked_add(n)) else {
                continue;
            };
            if end > buf.len() {
                continue;
            }
            let matches = (0..n).all(|k| buf.glyph_ids[i + 1 + k] == lig.component_glyph_ids[k]);
            if matches {
                match matched {
                    Some(m) if m.component_glyph_ids.len() >= n => {}
                    _ => matched = Some(lig),
                }
            }
        }

        let Some(lig) = matched else {
            i += 1;
            continue;
        };

        let consumed = 1 + lig.component_glyph_ids.len();
        let Some(drain_end) = i.checked_add(consumed) else {
            i += 1;
            continue;
        };
        let ligature_glyph = lig.ligature_glyph;
        buf.glyph_ids[i] = ligature_glyph;
        buf.placements[i] = GlyphPlacement {
            offset_x: 0.0,
            offset_y: 0.0,
            advance: font.glyph_advance(ligature_glyph),
        };
        // i+1..drain_end を 3 配列同期 drain で削除
        buf.drain_range(i + 1, drain_end);
        // cluster[i] は元の最初の char の byte index のまま (HarfBuzz 慣行)
        i += 1;
    }
}

/// GPOS `kern` feature 紐付き Lookup を適用する。
/// Microsoft 形式の古い `kern` テーブル (v0) ではなく、GPOS Pair Adjustment
/// 由来のカーニングのみを扱う。
pub(crate) fn apply_gpos_kern_feature_lookups(
    gpos: &GposTable,
    buf: &mut GlyphBuffer,
    font: &Font,
) {
    let scale = font.scale();
    for &idx in &gpos.kern_lookups {
        if let Some(lookup) = gpos.lookup_at(idx) {
            apply_gpos_lookup(lookup, buf, scale);
        }
    }
}

fn apply_gpos_lookup(lookup: &GposLookup, buf: &mut GlyphBuffer, scale: f64) {
    match lookup {
        GposLookup::Single(singles) => {
            for s in singles {
                apply_gpos_single(s, buf, scale);
            }
        }
        GposLookup::Pair(pairs) => {
            for p in pairs {
                apply_gpos_pair(p, buf, scale);
            }
        }
        GposLookup::Unsupported => {}
    }
}

fn apply_gpos_single(s: &GposSingleAdjust, buf: &mut GlyphBuffer, scale: f64) {
    for i in 0..buf.len() {
        let Some(ci) = s.coverage.index_of(buf.glyph_ids[i]) else {
            continue;
        };
        let vr = match &s.format {
            GposSingleAdjustFormat::Uniform(vr) => vr,
            GposSingleAdjustFormat::PerGlyph(vrs) => match vrs.get(ci as usize) {
                Some(v) => v,
                None => continue,
            },
        };
        apply_value_record_to_placement(vr, &mut buf.placements[i], scale);
    }
}

fn apply_gpos_pair(p: &GposPairAdjust, buf: &mut GlyphBuffer, scale: f64) {
    for i in 0..buf.len().saturating_sub(1) {
        let g1 = buf.glyph_ids[i];
        let g2 = buf.glyph_ids[i + 1];
        // 本実装では GPOS Pair Adjustment の value1 (g1 側) のみ適用する。
        // value2 (g2 側) は将来の拡張として保持している。
        if let Some((value1, _value2)) = p.lookup(g1, g2) {
            apply_value_record_to_placement(value1, &mut buf.placements[i], scale);
        }
    }
}

fn apply_value_record_to_placement(vr: &ValueRecord, placement: &mut GlyphPlacement, scale: f64) {
    placement.offset_x += vr.x_placement as f64 * scale;
    placement.offset_y += vr.y_placement as f64 * scale;
    placement.advance += vr.x_advance as f64 * scale;
    // y_advance は水平テキスト前提のため無視
}
