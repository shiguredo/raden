//! font モジュールの PBT 。
//!
//! Arial 不在時は `proptest!` の外側で早期 return する。
//! `prop_assume!` は proptest が低品質扱いするため避ける。

use proptest::prelude::*;
use raden::{Font, FontData, FontFace, GlyphBounds, Path};

/// macOS の Arial.ttf を読み込むヘルパー。CI 環境ではスキップする。
fn load_arial() -> Option<FontFace> {
    let path = "/System/Library/Fonts/Supplemental/Arial.ttf";
    if !std::path::Path::new(path).exists() {
        return None;
    }
    let data = FontData::from_file(path).ok()?;
    FontFace::from_data(&data, 0).ok()
}

/// Arial の cmap に確実に含まれる ASCII printable + 半角スペースに絞った文字列戦略。
/// 3 つの文字列ベース PBT (concatenation, size_linearity, non_negative) で共有する。
/// `prop_single_char_advance` は単一文字戦略 `proptest::char::range(' ', '~')` を直接利用する。
fn ascii_printable_string(max_len: usize) -> impl Strategy<Value = String> {
    proptest::string::string_regex(&format!("[ -~]{{0,{max_len}}}"))
        .expect("ASCII printable 文字列戦略の正規表現が不正")
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

/// 単一文字の `measure_text` が `glyph_advance(map_char_to_glyph(c))` と一致する。
fn prop_single_char_advance() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    proptest!(|(ch in proptest::char::range(' ', '~'))| {
        let metrics = font.measure_text(&ch.to_string());
        let expected = font.glyph_advance(font.map_char_to_glyph(ch));
        prop_assert_eq!(metrics.advance, expected);
    });
}

/// 結合性: `measure_text(a + b).advance ≈ measure_text(a).advance + measure_text(b).advance` 。
///
/// 現状の `measure_text` は `glyph_run_for_text` の total advance を返すため、
/// 本不変条件はカーニング非適用の生 advance 総和に対する結合性として成立する。
/// 将来カーニング適用が `measure_text` に追加されると `(A, V)` 等の隣接ペアで
/// 破綻するため、その時点で本テストの更新が必要になる。
fn prop_concatenation() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    proptest!(|(
        a in ascii_printable_string(16),
        b in ascii_printable_string(16),
    )| {
        let lhs = font.measure_text(&format!("{a}{b}")).advance;
        let rhs = font.measure_text(&a).advance + font.measure_text(&b).advance;
        prop_assert!(
            close_enough(lhs, rhs),
            "左辺 = {lhs}, 右辺 = {rhs}, 文字列 a = {a:?}, 文字列 b = {b:?}",
        );
    });
}

/// サイズ線形性: `Font(face, k * size).measure_text(text).advance ≈ k * Font(face, size).measure_text(text).advance` 。
fn prop_size_linearity() {
    let Some(face) = load_arial() else {
        return;
    };
    proptest!(|(
        text in ascii_printable_string(16),
        size in 4.0f64..200.0,
        k in 0.5f64..4.0,
    )| {
        // size がパラメータのため Font は反復ごとに構築する。
        let base = Font::from_face(&face, size).measure_text(&text).advance;
        let scaled = Font::from_face(&face, k * size).measure_text(&text).advance;
        let expected = k * base;
        prop_assert!(
            close_enough(scaled, expected),
            "スケール後 = {scaled}, 期待値 = {expected}, 文字列 = {text:?}, サイズ = {size}, 倍率 = {k}",
        );
    });
}

/// 非負性: Arial は全グリフが非負 advance のため `measure_text(text).advance >= 0.0` 。
fn prop_non_negative() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    proptest!(|(text in ascii_printable_string(64))| {
        let advance = font.measure_text(&text).advance;
        prop_assert!(advance >= 0.0, "advance = {advance}, 文字列 = {text:?}");
    });
}

/// `line_gap` のスケール線形性 : `Font::line_gap()` は `face.line_gap() * scale` と一致する。
fn prop_line_gap_scale_linearity() {
    let Some(face) = load_arial() else {
        return;
    };
    let design = face.line_gap();
    proptest!(|(size in 0.001f64..1000.0)| {
        let font = Font::from_face(&face, size);
        let actual = font.line_gap();
        let expected = design as f64 * font.scale();
        prop_assert!(
            metric_close_enough(actual, expected),
            "line_gap = {}, 期待値 = {}, size = {}",
            actual, expected, size,
        );
    });
}

/// `cap_height` / `x_height` のスケール線形性を検証する共通ヘルパー。
fn prop_opt_metric_scale_linearity(
    face: &FontFace,
    metric_name: &'static str,
    design_value: Option<i16>,
    scaled_getter: impl Fn(&Font) -> Option<f64>,
) {
    let Some(design) = design_value else {
        return;
    };
    proptest!(|(size in 0.001f64..1000.0)| {
        let font = Font::from_face(face, size);
        let actual = scaled_getter(&font)
            .unwrap_or_else(|| panic!("{metric_name} は Some のはず"));
        let expected = design as f64 * font.scale();
        prop_assert!(
            metric_close_enough(actual, expected),
            "{metric_name} = {}, 期待値 = {}, size = {}",
            actual, expected, size,
        );
    });
}

/// `glyph_bounds` の不変条件 : `Some` のとき `x_min <= x_max` かつ `y_min <= y_max` 。
fn prop_glyph_bounds_ordering() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    proptest!(|(ch in proptest::char::range(' ', '~'))| {
        let gid = font.map_char_to_glyph(ch);
        if let Some(gb) = font.glyph_bounds(gid) {
            prop_assert!(
                gb.x_min <= gb.x_max,
                "x_min = {}, x_max = {}, ch = {:?}",
                gb.x_min, gb.x_max, ch,
            );
            prop_assert!(
                gb.y_min <= gb.y_max,
                "y_min = {}, y_max = {}, ch = {:?}",
                gb.y_min, gb.y_max, ch,
            );
        }
    });
}

/// `glyph_bounds` のスケール線形性 : `Font::glyph_bounds` の各座標 =
/// `FontFace::glyph_bounds` の各座標 × `Font::scale()` 。
/// `FontFace` 側が `None` のとき `Font` 側も `None` となることもあわせて検証する。
fn prop_glyph_bounds_scale_linearity() {
    let Some(face) = load_arial() else {
        return;
    };
    proptest!(|(
        ch in proptest::char::range(' ', '~'),
        size in 0.001f64..1000.0,
    )| {
        let font = Font::from_face(&face, size);
        let gid = font.map_char_to_glyph(ch);
        match (face.glyph_bounds(gid), font.glyph_bounds(gid)) {
            (Some(fgb), Some(gb)) => {
                let scale = font.scale();
                prop_assert!(
                    metric_close_enough(gb.x_min, fgb.x_min * scale),
                    "x_min = {}, 期待値 = {}, ch = {:?}, size = {}",
                    gb.x_min, fgb.x_min * scale, ch, size,
                );
                prop_assert!(
                    metric_close_enough(gb.y_min, fgb.y_min * scale),
                    "y_min = {}, 期待値 = {}, ch = {:?}, size = {}",
                    gb.y_min, fgb.y_min * scale, ch, size,
                );
                prop_assert!(
                    metric_close_enough(gb.x_max, fgb.x_max * scale),
                    "x_max = {}, 期待値 = {}, ch = {:?}, size = {}",
                    gb.x_max, fgb.x_max * scale, ch, size,
                );
                prop_assert!(
                    metric_close_enough(gb.y_max, fgb.y_max * scale),
                    "y_max = {}, 期待値 = {}, ch = {:?}, size = {}",
                    gb.y_max, fgb.y_max * scale, ch, size,
                );
            }
            (None, None) => {
                // 両方 None も整合
            }
            (Some(_), None) | (None, Some(_)) => {
                prop_assert!(
                    false,
                    "FontFace と Font の glyph_bounds の Some/None が一致しない ch = {:?}, size = {}",
                    ch, size,
                );
            }
        }
    });
}

/// `measure_text` の `bounding_box` 単一文字検証 : 単一文字 `c` で
/// `cursor_x == 0.0` のため `font.glyph_bounds(font.map_char_to_glyph(c))` と
/// 完全に一致する。
/// 文字戦略は `'!'..='~'` (空グリフの ` ` を除外) として、`(None, None)` 一致経路に
/// 偏らず Some 値同士の一致を高密度で検証する。
fn prop_measure_text_bounding_box_single_char() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    proptest!(|(ch in proptest::char::range('!', '~'))| {
        let metrics = font.measure_text(&ch.to_string());
        let gid = font.map_char_to_glyph(ch);
        let expected = font.glyph_bounds(gid);
        prop_assert_eq!(metrics.bounding_box, expected);
    });
}

/// `measure_text` の `bounding_box` 包含検証 : 2 文字以上 16 文字以下の文字列で、
/// 各文字の翻訳済み bbox が全体 `bounding_box` に含まれる (union 累積の妥当性) 。
/// 空文字列 (None になる境界値) は単体テスト、単一文字一致は別 PBT が担当する。
/// 文字戦略は `[!-~]` (空グリフの ` ` を除外) として、proptest のシュリンクが
/// 全文字スペースの最短入力に偏らないようにする。
fn prop_measure_text_bounding_box_contains() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    let strategy = proptest::string::string_regex("[!-~]{2,16}")
        .expect("ASCII printable 文字列戦略の正規表現が不正");
    proptest!(|(text in strategy)| {
        let metrics = font.measure_text(&text);
        let Some(bbox) = metrics.bounding_box else {
            // 全文字が空グリフのケース。`[!-~]` 戦略では発生しないが防御として残す。
            return Ok(());
        };
        let eps = bbox_eps(bbox);
        let mut cursor_x = 0.0_f64;
        for ch in text.chars() {
            let gid = font.map_char_to_glyph(ch);
            if let Some(gb) = font.glyph_bounds(gid) {
                let translated_x_min = gb.x_min + cursor_x;
                let translated_y_min = gb.y_min;
                let translated_x_max = gb.x_max + cursor_x;
                let translated_y_max = gb.y_max;
                prop_assert!(
                    bbox.x_min - eps <= translated_x_min && translated_x_max <= bbox.x_max + eps,
                    "X 範囲外 ch = {:?}, cursor_x = {}, 翻訳 = ({}, {}), bbox = ({}, {})",
                    ch, cursor_x, translated_x_min, translated_x_max, bbox.x_min, bbox.x_max,
                );
                prop_assert!(
                    bbox.y_min - eps <= translated_y_min && translated_y_max <= bbox.y_max + eps,
                    "Y 範囲外 ch = {:?}, 翻訳 = ({}, {}), bbox = ({}, {})",
                    ch, translated_y_min, translated_y_max, bbox.y_min, bbox.y_max,
                );
            }
            cursor_x += font.glyph_advance(gid);
        }
    });
}

/// `glyph_bounds` が `Path::control_box()` を包含する : TrueType glyf ヘッダの
/// bbox は制御点を含む全点座標の min/max として書かれるため、`append_glyph_outline`
/// で生成された Path の制御点 bbox (Y down) を Y up に反転した範囲が
/// `glyph_bounds` (Y up) に含まれる。
/// size を `prop_size_linearity` と同じ `4.0..200.0` の範囲で振る。
/// Arial の ASCII 範囲では Simple Glyph のみで `i16 * scale` の単純乗算のため
/// 厳密に exact 一致するが、Compound Glyph (将来別フォント対応時) や f2dot14 行列で
/// 浮動小数点丸めが乗る経路を捕まえるための保険として許容範囲を設ける。
fn prop_glyph_bounds_contains_path_control_box() {
    let Some(face) = load_arial() else {
        return;
    };
    proptest!(|(
        ch in proptest::char::range(' ', '~'),
        size in 4.0f64..200.0,
    )| {
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
        prop_assert!(
            gb.x_min - eps <= x0 && x1 <= gb.x_max + eps,
            "X 範囲外 ch = {:?}, size = {}, control_box X = ({}, {}), glyph_bounds X = ({}, {})",
            ch, size, x0, x1, gb.x_min, gb.x_max,
        );
        prop_assert!(
            gb.y_min - eps <= y_min_up && y_max_up <= gb.y_max + eps,
            "Y 範囲外 ch = {:?}, size = {}, control_box Y up = ({}, {}), glyph_bounds Y = ({}, {})",
            ch, size, y_min_up, y_max_up, gb.y_min, gb.y_max,
        );
    });
}

/// 符号 ( Arial 限定 ) : line_gap は非負、cap_height / x_height は正値。
fn prop_font_metrics_sign() {
    let Some(face) = load_arial() else {
        return;
    };
    proptest!(|(size in 0.001f64..1000.0)| {
        let font = Font::from_face(&face, size);
        prop_assert!(
            font.line_gap() >= 0.0,
            "line_gap = {}, size = {}",
            font.line_gap(), size,
        );
        let cap_height = font.cap_height().expect("cap_height は Some のはず");
        prop_assert!(cap_height > 0.0, "cap_height = {}, size = {}", cap_height, size);
        let x_height = font.x_height().expect("x_height は Some のはず");
        prop_assert!(x_height > 0.0, "x_height = {}, size = {}", x_height, size);
    });
}

#[test]
fn single_char_advance() {
    prop_single_char_advance();
}

#[test]
fn concatenation() {
    prop_concatenation();
}

#[test]
fn size_linearity() {
    prop_size_linearity();
}

#[test]
fn non_negative() {
    prop_non_negative();
}

#[test]
fn line_gap_scale_linearity() {
    prop_line_gap_scale_linearity();
}

#[test]
fn cap_height_scale_linearity() {
    let Some(face) = load_arial() else {
        return;
    };
    prop_opt_metric_scale_linearity(&face, "cap_height", face.cap_height(), |font| {
        font.cap_height()
    });
}

#[test]
fn x_height_scale_linearity() {
    let Some(face) = load_arial() else {
        return;
    };
    prop_opt_metric_scale_linearity(&face, "x_height", face.x_height(), |font| font.x_height());
}

#[test]
fn font_metrics_sign() {
    prop_font_metrics_sign();
}

#[test]
fn glyph_bounds_ordering() {
    prop_glyph_bounds_ordering();
}

#[test]
fn glyph_bounds_scale_linearity() {
    prop_glyph_bounds_scale_linearity();
}

#[test]
fn measure_text_bounding_box_single_char() {
    prop_measure_text_bounding_box_single_char();
}

#[test]
fn measure_text_bounding_box_contains() {
    prop_measure_text_bounding_box_contains();
}

#[test]
fn glyph_bounds_contains_path_control_box() {
    prop_glyph_bounds_contains_path_control_box();
}
