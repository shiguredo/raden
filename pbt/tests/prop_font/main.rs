//! font モジュールの PBT 。
//!
//! Arial 不在時は `proptest!` の外側で早期 return する。
//! `prop_assume!` は proptest が低品質扱いするため避ける。

use proptest::prelude::*;
use raden::{Font, FontData, FontFace};

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
