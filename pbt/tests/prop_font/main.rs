//! font モジュールの PBT 。
//! Arial 不在時は早期 return する。

use raden::{Font, FontData, FontFace, FontFeatureSettings, GlyphBounds, Path};

/// 1 テストあたりのケース数。
const CASES: usize = 256;

/// シード再現用の環境変数名。
const SEED_ENV: &str = "RADEN_PBT_SEED";

/// macOS の Arial.ttf を読み込むヘルパー。CI 環境ではスキップする。
fn load_arial() -> Option<FontFace> {
    let path = "/System/Library/Fonts/Supplemental/Arial.ttf";
    if !std::path::Path::new(path).exists() {
        return None;
    }
    let data = FontData::from_file(path).ok()?;
    FontFace::from_data(&data, 0).ok()
}

/// ASCII printable + 半角スペースの文字列をサンプリングする。
fn sample_ascii_printable_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=max_len);
    noprop::sample_ascii_printable_string(ctx, len)
}

/// `'!'..='~'` の文字をサンプリングする（空グリフのスペースを除外）。
fn sample_ascii_graphic_char(ctx: &mut noprop::TestCaseContext) -> char {
    char::from_u32(noprop::sample_usize_in(ctx, ('!' as usize)..=('~' as usize)) as u32)
        .expect("ASCII graphic 範囲のコードポイント")
}

/// `[!-~]` の文字列をサンプリングする。
fn sample_ascii_graphic_string(
    ctx: &mut noprop::TestCaseContext,
    min_len: usize,
    max_len: usize,
) -> String {
    let len = noprop::sample_usize_in(ctx, min_len..=max_len);
    (0..len).map(|_| sample_ascii_graphic_char(ctx)).collect()
}

/// 制御文字を含む ASCII 文字列をサンプリングする。
fn sample_ascii_string(ctx: &mut noprop::TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=max_len);
    noprop::sample_ascii_string(ctx, len)
}

/// 浮動小数点の相対許容判定。`measure_text` の加算順違いによる丸めを吸収する。
/// 値域が size に比例して増えるため絶対誤差ではなく相対誤差を採用する。
fn close_enough(lhs: f64, rhs: f64) -> bool {
    (lhs - rhs).abs() < (lhs.abs() + rhs.abs()) * 1e-9 + 1e-12
}

/// スケール済みメトリクスの許容判定。
/// 設計に従い、絶対誤差 1e-9 に加えて相対誤差 1e-12 を許容する。
/// 小さい値 (size = 0.001) では絶対誤差が支配的で、大きい値 (size = 1000) でも
/// 相対誤差 1e-12 は十分に厳しいため、線形性のずれを検出できる。
fn metric_close_enough(lhs: f64, rhs: f64) -> bool {
    (lhs - rhs).abs() < 1e-9 + (lhs.abs() + rhs.abs()) * 1e-12
}

/// bbox 包含判定の許容範囲。各成分の絶対値合計に相対係数を掛け、絶対下限を加える。
/// 文字列が長くなれば bbox の各成分が大きくなり、許容範囲も比例して大きくなる。
fn bbox_eps(bbox: GlyphBounds) -> f64 {
    (bbox.x_min.abs() + bbox.x_max.abs() + bbox.y_min.abs() + bbox.y_max.abs()) * 1e-9 + 1e-12
}

/// `line_gap` のスケール線形性 : `Font::line_gap()` は `face.line_gap() * scale` と一致する。
fn prop_line_gap_scale_linearity() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let design = face.line_gap();
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let size = noprop::sample_f64_in(ctx, 0.001, 1000.0);
        let font = Font::from_face(&face, size);
        let actual = font.line_gap();
        let expected = design as f64 * font.scale();
        assert!(
            metric_close_enough(actual, expected),
            "line_gap = {}, 期待値 = {}, size = {}",
            actual,
            expected,
            size,
        );
        Ok(())
    })?;
    Ok(())
}

/// `cap_height` / `x_height` のスケール線形性を検証する共通ヘルパー。
fn prop_opt_metric_scale_linearity(
    face: &FontFace,
    metric_name: &'static str,
    design_value: Option<i16>,
    scaled_getter: impl Fn(&Font) -> Option<f64>,
) -> noprop::TestResult {
    let Some(design) = design_value else {
        return Ok(());
    };
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let size = noprop::sample_f64_in(ctx, 0.001, 1000.0);
        let font = Font::from_face(face, size);
        let actual = scaled_getter(&font).unwrap_or_else(|| panic!("{metric_name} は Some のはず"));
        let expected = design as f64 * font.scale();
        assert!(
            metric_close_enough(actual, expected),
            "{metric_name} = {}, 期待値 = {}, size = {}",
            actual,
            expected,
            size,
        );
        Ok(())
    })?;
    Ok(())
}

/// `glyph_bounds` の不変条件 : `Some` のとき `x_min <= x_max` かつ `y_min <= y_max` 。
fn prop_glyph_bounds_ordering() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let font = Font::from_face(&face, 48.0);
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let ch = noprop::sample_ascii_printable_char(ctx);
        let gid = font.map_char_to_glyph(ch);
        if let Some(gb) = font.glyph_bounds(gid) {
            assert!(
                gb.x_min <= gb.x_max,
                "x_min = {}, x_max = {}, ch = {:?}",
                gb.x_min,
                gb.x_max,
                ch,
            );
            assert!(
                gb.y_min <= gb.y_max,
                "y_min = {}, y_max = {}, ch = {:?}",
                gb.y_min,
                gb.y_max,
                ch,
            );
        }
        Ok(())
    })?;
    Ok(())
}

/// `glyph_bounds` のスケール線形性 : `Font::glyph_bounds` の各座標 =
/// `FontFace::glyph_bounds` の各座標 × `Font::scale()` 。
/// `FontFace` 側が `None` のとき `Font` 側も `None` となることもあわせて検証する。
fn prop_glyph_bounds_scale_linearity() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let ch = noprop::sample_ascii_printable_char(ctx);
        let size = noprop::sample_f64_in(ctx, 0.001, 1000.0);
        let font = Font::from_face(&face, size);
        let gid = font.map_char_to_glyph(ch);
        match (face.glyph_bounds(gid), font.glyph_bounds(gid)) {
            (Some(fgb), Some(gb)) => {
                let scale = font.scale();
                assert!(
                    metric_close_enough(gb.x_min, fgb.x_min * scale),
                    "x_min = {}, 期待値 = {}, ch = {:?}, size = {}",
                    gb.x_min, fgb.x_min * scale, ch, size,
                );
                assert!(
                    metric_close_enough(gb.y_min, fgb.y_min * scale),
                    "y_min = {}, 期待値 = {}, ch = {:?}, size = {}",
                    gb.y_min, fgb.y_min * scale, ch, size,
                );
                assert!(
                    metric_close_enough(gb.x_max, fgb.x_max * scale),
                    "x_max = {}, 期待値 = {}, ch = {:?}, size = {}",
                    gb.x_max, fgb.x_max * scale, ch, size,
                );
                assert!(
                    metric_close_enough(gb.y_max, fgb.y_max * scale),
                    "y_max = {}, 期待値 = {}, ch = {:?}, size = {}",
                    gb.y_max, fgb.y_max * scale, ch, size,
                );
            }
            (None, None) => {
                // 両方 None も整合
            }
            (Some(_), None) | (None, Some(_)) => {
                panic!(
                    "FontFace と Font の glyph_bounds の Some/None が一致しない ch = {:?}, size = {}",
                    ch, size,
                );
            }
        }
        Ok(())
    })?;
    Ok(())
}

/// `measure_text` の `bounding_box` 単一文字検証 : 単一文字 `c` で
/// `cursor_x == 0.0` のため `font.glyph_bounds(font.map_char_to_glyph(c))` と
/// 完全に一致する。
/// 文字戦略は `'!'..='~'` (空グリフの ` ` を除外) として、`(None, None)` 一致経路に
/// 偏らず Some 値同士の一致を高密度で検証する。
fn prop_measure_text_bounding_box_single_char() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let font = Font::from_face(&face, 48.0);
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let ch = sample_ascii_graphic_char(ctx);
        let metrics = font.measure_text(&ch.to_string());
        let gid = font.map_char_to_glyph(ch);
        let expected = font.glyph_bounds(gid);
        assert_eq!(metrics.bounding_box, expected);
        Ok(())
    })?;
    Ok(())
}

/// `measure_text` の `bounding_box` 包含検証 : 2 文字以上 16 文字以下の文字列で、
/// `shape` 後の各グリフ配置 (cursor_x + placement.offset) で翻訳した bbox が
/// 全体 `bounding_box` に含まれる (union 累積の妥当性) 。
/// 空文字列 (None になる境界値) は単体テスト、単一文字一致は別 PBT が担当する。
/// 文字戦略は `[!-~]` (空グリフの ` ` を除外) とする。
fn prop_measure_text_bounding_box_contains() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let font = Font::from_face(&face, 48.0);
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let text = sample_ascii_graphic_string(ctx, 2, 16);
        let metrics = font.measure_text(&text);
        let buffer = font.shape(&text);
        let Some(bbox) = metrics.bounding_box else {
            // 全文字が空グリフのケース。`[!-~]` 戦略では発生しないが防御として残す。
            return Ok(());
        };
        let eps = bbox_eps(bbox);
        let mut cursor_x = 0.0_f64;
        for (gid, placement, _cluster) in buffer.iter() {
            if let Some(gb) = font.glyph_bounds(gid) {
                let translated_x_min = gb.x_min + cursor_x + placement.offset_x;
                let translated_y_min = gb.y_min + placement.offset_y;
                let translated_x_max = gb.x_max + cursor_x + placement.offset_x;
                let translated_y_max = gb.y_max + placement.offset_y;
                assert!(
                    bbox.x_min - eps <= translated_x_min && translated_x_max <= bbox.x_max + eps,
                    "X 範囲外 gid = {}, cursor_x = {}, 翻訳 = ({}, {}), bbox = ({}, {})",
                    gid,
                    cursor_x,
                    translated_x_min,
                    translated_x_max,
                    bbox.x_min,
                    bbox.x_max,
                );
                assert!(
                    bbox.y_min - eps <= translated_y_min && translated_y_max <= bbox.y_max + eps,
                    "Y 範囲外 gid = {}, 翻訳 = ({}, {}), bbox = ({}, {})",
                    gid,
                    translated_y_min,
                    translated_y_max,
                    bbox.y_min,
                    bbox.y_max,
                );
            }
            cursor_x += placement.advance;
        }
        Ok(())
    })?;
    Ok(())
}

/// `glyph_bounds` が `Path::control_box()` を包含する : TrueType glyf ヘッダの
/// bbox は制御点を含む全点座標の min/max として書かれるため、`append_glyph_outline`
/// で生成された Path の制御点 bbox (Y down) を Y up に反転した範囲が
/// `glyph_bounds` (Y up) に含まれる。
/// size を `prop_size_linearity` と同じ `4.0..200.0` の範囲で振る。
/// Arial の ASCII 範囲では Simple Glyph のみで `i16 * scale` の単純乗算のため
/// 厳密に exact 一致するが、Compound Glyph (将来別フォント対応時) や f2dot14 行列で
/// 浮動小数点丸めが乗る経路を捕まえるための保険として許容範囲を設ける。
fn prop_glyph_bounds_contains_path_control_box() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let ch = noprop::sample_ascii_printable_char(ctx);
        let size = noprop::sample_f64_in(ctx, 4.0, 200.0);
        let font = Font::from_face(&face, size);
        let gid = font.map_char_to_glyph(ch);
        let Some(gb) = font.glyph_bounds(gid) else {
            return Ok(());
        };
        let mut path = Path::new();
        // Arial の ASCII 範囲では失敗しない前提のため、発生したらテスト失敗で顕在化させる。
        font.append_glyph_outline(gid, 0.0, 0.0, &mut path)
            .expect("Arial の ASCII 範囲で append_glyph_outline は失敗しない");
        let Some(rect) = path.control_box() else {
            // numberOfContours == 0 等で点が追加されない場合は control_box が None になる。
            return Ok(());
        };
        // Rect は Y down (画面座標)、glyph_bounds は Y up。
        // Rect の (x, y, w, h) → (x0, y0, x1, y1) (Y down) → Y up へ反転。
        let x0 = rect.x;
        let y0 = rect.y;
        let x1 = rect.x + rect.w;
        let y1 = rect.y + rect.h;
        let y_min_up = -y1;
        let y_max_up = -y0;
        let eps = bbox_eps(gb);
        assert!(
            gb.x_min - eps <= x0 && x1 <= gb.x_max + eps,
            "X 範囲外 ch = {:?}, size = {}, control_box X = ({}, {}), glyph_bounds X = ({}, {})",
            ch,
            size,
            x0,
            x1,
            gb.x_min,
            gb.x_max,
        );
        assert!(
            gb.y_min - eps <= y_min_up && y_max_up <= gb.y_max + eps,
            "Y 範囲外 ch = {:?}, size = {}, control_box Y up = ({}, {}), glyph_bounds Y = ({}, {})",
            ch,
            size,
            y_min_up,
            y_max_up,
            gb.y_min,
            gb.y_max,
        );
        Ok(())
    })?;
    Ok(())
}

/// 符号 ( Arial 限定 ) : line_gap は非負、cap_height / x_height は正値。
fn prop_font_metrics_sign() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let size = noprop::sample_f64_in(ctx, 0.001, 1000.0);
        let font = Font::from_face(&face, size);
        assert!(
            font.line_gap() >= 0.0,
            "line_gap = {}, size = {}",
            font.line_gap(),
            size,
        );
        let cap_height = font.cap_height().expect("cap_height は Some のはず");
        assert!(
            cap_height > 0.0,
            "cap_height = {}, size = {}",
            cap_height,
            size
        );
        let x_height = font.x_height().expect("x_height は Some のはず");
        assert!(x_height > 0.0, "x_height = {}, size = {}", x_height, size);
        Ok(())
    })?;
    Ok(())
}

#[test]
fn line_gap_scale_linearity() -> noprop::TestResult {
    prop_line_gap_scale_linearity()?;
    Ok(())
}

#[test]
fn cap_height_scale_linearity() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    prop_opt_metric_scale_linearity(&face, "cap_height", face.cap_height(), |font| {
        font.cap_height()
    })?;
    Ok(())
}

#[test]
fn x_height_scale_linearity() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    prop_opt_metric_scale_linearity(&face, "x_height", face.x_height(), |font| font.x_height())?;
    Ok(())
}

#[test]
fn font_metrics_sign() -> noprop::TestResult {
    prop_font_metrics_sign()?;
    Ok(())
}

#[test]
fn glyph_bounds_ordering() -> noprop::TestResult {
    prop_glyph_bounds_ordering()?;
    Ok(())
}

#[test]
fn glyph_bounds_scale_linearity() -> noprop::TestResult {
    prop_glyph_bounds_scale_linearity()?;
    Ok(())
}

#[test]
fn measure_text_bounding_box_single_char() -> noprop::TestResult {
    prop_measure_text_bounding_box_single_char()?;
    Ok(())
}

#[test]
fn measure_text_bounding_box_contains() -> noprop::TestResult {
    prop_measure_text_bounding_box_contains()?;
    Ok(())
}

#[test]
fn glyph_bounds_contains_path_control_box() -> noprop::TestResult {
    prop_glyph_bounds_contains_path_control_box()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// OpenType 基本シェーピングの不変条件
//
// Arial は GSUB / GPOS レイアウトテーブルを含まないため、liga / kern 無効時の
// shape は cmap + advance のみの恒等変換となる。本節では shape API の構造的不変条件を
// Arial で検証し、具体的な合字 / カーニング動作は tests/test_font.rs の
// Source Sans 3 ベーステストで検証する。
// ---------------------------------------------------------------------------

/// `Font::shape` の戻り値である `GlyphBuffer` の 3 つの Vec 長が一致する。
fn prop_shape_buffer_lengths_equal() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let font = Font::from_face(&face, 48.0);
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let text = sample_ascii_printable_string(ctx, 32);
        let buffer = font.shape(&text);
        assert!(
            buffer.is_well_formed(),
            "GlyphBuffer の 3 配列長が一致する必要がある"
        );
        assert!(!buffer.is_empty() || text.is_empty());
        Ok(())
    })?;
    Ok(())
}

/// 空文字列を shape すると、空の `GlyphBuffer` が返される。
/// PBT では空文字列を生成しにくいため、通常テストとして配置する。
fn test_shape_empty_text() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    let buffer = font.shape("");
    assert!(buffer.is_empty());
}

/// `clusters` は UTF-8 byte index で単調非減少であり、先頭は 0、末尾はテキスト長以下である。
fn prop_shape_clusters_monotonic() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let font = Font::from_face(&face, 48.0);
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let text = sample_ascii_printable_string(ctx, 32);
        let buffer = font.shape(&text);
        assert!(
            buffer.is_well_formed(),
            "GlyphBuffer の 3 配列長が一致する必要がある"
        );
        if buffer.is_empty() {
            return Ok(());
        }
        assert_eq!(buffer.cluster(0), Some(0));
        let mut prev = 0;
        for i in 1..buffer.len() {
            let Some(curr) = buffer.cluster(i) else {
                continue;
            };
            assert!(prev <= curr, "clusters が単調非減少ではない");
            prev = curr;
        }
        assert!(
            buffer.cluster(buffer.len().saturating_sub(1)).unwrap_or(0) <= text.len() as u32,
            "末尾の cluster がテキスト長を超えている"
        );
        Ok(())
    })?;
    Ok(())
}

/// Arial は GSUB / GPOS テーブルを持たないため、default features でも
/// shape は cmap による 1 対 1 変換と同等であり、グリフ数が元テキストの char 数と一致する。
fn prop_shape_arial_default_preserves_char_count() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let font = Font::from_face(&face, 48.0);
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let text = sample_ascii_printable_string(ctx, 32);
        let buffer = font.shape(&text);
        assert_eq!(buffer.len(), text.chars().count());
        Ok(())
    })?;
    Ok(())
}

/// `measure_text` の advance は、`shape` 後の各グリフの advance の総和と一致する。
/// `measure_text` は内部で `shape` を呼んでいるが、両者が将来の改修で食い違う
/// リスクを防ぐ不変条件として保持する。
fn prop_shape_advance_sum_matches_measure_text() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let font = Font::from_face(&face, 48.0);
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let text = sample_ascii_printable_string(ctx, 32);
        let buffer = font.shape(&text);
        let expected: f64 = buffer.iter().map(|(_, p, _)| p.advance).sum();
        let metrics = font.measure_text(&text);
        assert!(
            close_enough(metrics.advance, expected),
            "measure_text.advance = {}, shape 後の advance 総和 = {}",
            metrics.advance,
            expected,
        );
        Ok(())
    })?;
    Ok(())
}

/// 全 feature 無効時、`measure_text` の advance は cmap 1 対 1 変換後の
/// 各文字 `glyph_advance(map_char_to_glyph(ch))` の総和と一致する。
/// この不変条件は GSUB / GPOS が適用されない状態での `Font::shape` の
/// 整合性を保証する。
fn prop_measure_text_advance_no_features_matches_glyph_advance_sum() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let font = Font::from_face(&face, 48.0).clone_with_features(FontFeatureSettings::none());
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let text = sample_ascii_printable_string(ctx, 32);
        let metrics = font.measure_text(&text);
        let expected: f64 = text
            .chars()
            .map(|ch| font.glyph_advance(font.map_char_to_glyph(ch)))
            .sum();
        assert!(
            close_enough(metrics.advance, expected),
            "advance = {}, 期待値 = {}",
            metrics.advance,
            expected,
        );
        Ok(())
    })?;
    Ok(())
}

/// `measure_text` の数値フィールドが NaN / Inf にならない。
/// 文字列戦略は全 ASCII (制御文字含む) で、空文字列・改行・未マッピング文字の
/// 経路もカバーする。
fn prop_measure_text_values_finite() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let text = sample_ascii_string(ctx, 32);
        let size = noprop::sample_f64_in(ctx, 0.0, 1000.0);
        let font = Font::from_face(&face, size);
        let metrics = font.measure_text(&text);
        assert!(
            metrics.advance.is_finite(),
            "advance = {} (有限値である必要がある)",
            metrics.advance
        );
        assert!(
            metrics.leading_bearing.is_finite(),
            "leading_bearing = {} (有限値である必要がある)",
            metrics.leading_bearing,
        );
        assert!(
            metrics.trailing_bearing.is_finite(),
            "trailing_bearing = {} (有限値である必要がある)",
            metrics.trailing_bearing,
        );
        if let Some(bbox) = metrics.bounding_box {
            assert!(
                bbox.x_min.is_finite(),
                "bbox.x_min = {} (有限値である必要がある)",
                bbox.x_min
            );
            assert!(
                bbox.x_max.is_finite(),
                "bbox.x_max = {} (有限値である必要がある)",
                bbox.x_max
            );
            assert!(
                bbox.y_min.is_finite(),
                "bbox.y_min = {} (有限値である必要がある)",
                bbox.y_min
            );
            assert!(
                bbox.y_max.is_finite(),
                "bbox.y_max = {} (有限値である必要がある)",
                bbox.y_max
            );
        }
        Ok(())
    })?;
    Ok(())
}

/// `clone_with_features` は face / size / scale を保持し feature 設定だけ変更する。
fn prop_clone_with_features_preserves_size_and_scale() -> noprop::TestResult {
    let Some(face) = load_arial() else {
        return Ok(());
    };
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let size = noprop::sample_f64_in(ctx, 0.001, 1000.0);
        let kern = noprop::sample_bool(ctx);
        let liga = noprop::sample_bool(ctx);
        let clig = noprop::sample_bool(ctx);
        let font = Font::from_face(&face, size);
        let features = FontFeatureSettings::none()
            .with_kern(kern)
            .with_liga(liga)
            .with_clig(clig);
        let cloned = font.clone_with_features(features.clone());
        assert_eq!(cloned.size(), font.size());
        assert_eq!(cloned.scale(), font.scale());
        assert_eq!(cloned.feature_settings(), &features);
        Ok(())
    })?;
    Ok(())
}

#[test]
fn shape_buffer_lengths_equal() -> noprop::TestResult {
    prop_shape_buffer_lengths_equal()?;
    Ok(())
}

#[test]
fn shape_empty_text() {
    test_shape_empty_text();
}

#[test]
fn shape_clusters_monotonic() -> noprop::TestResult {
    prop_shape_clusters_monotonic()?;
    Ok(())
}

#[test]
fn shape_default_preserves_char_count() -> noprop::TestResult {
    prop_shape_arial_default_preserves_char_count()?;
    Ok(())
}

#[test]
fn shape_advance_sum_matches_measure_text() -> noprop::TestResult {
    prop_shape_advance_sum_matches_measure_text()?;
    Ok(())
}

#[test]
fn measure_text_advance_no_features_matches_glyph_advance_sum() -> noprop::TestResult {
    prop_measure_text_advance_no_features_matches_glyph_advance_sum()?;
    Ok(())
}

#[test]
fn measure_text_values_finite() -> noprop::TestResult {
    prop_measure_text_values_finite()?;
    Ok(())
}

#[test]
fn clone_with_features_preserves_size_and_scale() -> noprop::TestResult {
    prop_clone_with_features_preserves_size_and_scale()?;
    Ok(())
}
