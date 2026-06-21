use raden::{Font, FontData, FontFace};

/// macOS の Arial.ttf を使ったフォント読み込みヘルパー。
/// CI 環境では Arial がない可能性があるため、ファイルが存在しない場合はスキップする。
/// `FontFace` は内部で `Arc<Vec<u8>>` を所有するため、呼び出し側で `FontData` を
/// 保持し続ける必要はない。
fn load_arial() -> Option<FontFace> {
    let path = "/System/Library/Fonts/Supplemental/Arial.ttf";
    if !std::path::Path::new(path).exists() {
        return None;
    }
    let data = FontData::from_file(path).ok()?;
    FontFace::from_data(&data, 0).ok()
}

#[test]
fn font_face_metrics() {
    let Some(face) = load_arial() else {
        return;
    };
    // Arial の units_per_em は 2048
    assert_eq!(face.units_per_em(), 2048);
    // ascent > 0, descent < 0
    assert!(face.ascent() > 0);
    assert!(face.descent() < 0);
}

/// Arial の OS/2 v4 で cap_height / x_height が取得できることを確認する。
#[test]
fn font_face_cap_x_height() {
    let Some(face) = load_arial() else {
        return;
    };
    // Arial は OS/2 v4 で sCapHeight / sxHeight が定義済み。
    let cap_height = face
        .cap_height()
        .expect("Arial は cap_height を提供する必要がある");
    let x_height = face
        .x_height()
        .expect("Arial は x_height を提供する必要がある");
    assert!(cap_height > 0, "cap_height は正値である必要がある");
    assert!(x_height > 0, "x_height は正値である必要がある");
    assert!(
        x_height <= cap_height,
        "x_height は cap_height 以下である必要がある"
    );
}

#[test]
fn font_char_to_glyph() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);

    // 'A' (U+0041) はグリフ ID != 0 でなければならない
    let glyph_a = font.map_char_to_glyph('A');
    assert_ne!(
        glyph_a, 0,
        "'A' は 0 以外のグリフ ID にマップされる必要がある"
    );

    // スペース (U+0020) もグリフ ID != 0
    let glyph_space = font.map_char_to_glyph(' ');
    assert_ne!(
        glyph_space, 0,
        "スペースは 0 以外のグリフ ID にマップされる必要がある"
    );

    // advance width > 0
    assert!(font.glyph_advance(glyph_a) > 0.0);
    assert!(font.glyph_advance(glyph_space) > 0.0);
}

#[test]
fn font_glyph_outline_to_path() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);

    let glyph_a = font.map_char_to_glyph('A');
    let mut path = raden::Path::new();
    font.append_glyph_outline(glyph_a, 0.0, 48.0, &mut path)
        .expect("グリフアウトラインの取得に失敗しない");

    // 'A' はアウトラインを持つ (空でない)
    assert!(
        !path.is_empty(),
        "'A' グリフは空でないパスを生成する必要がある"
    );
    assert!(
        path.points().len() > 4,
        "'A' グリフは複数の点を持つ必要がある"
    );
}

#[test]
fn font_space_glyph_has_no_outline() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);

    let glyph_space = font.map_char_to_glyph(' ');
    let mut path = raden::Path::new();
    font.append_glyph_outline(glyph_space, 0.0, 48.0, &mut path)
        .expect("スペースグリフのアウトライン取得に失敗しない");

    // スペースはアウトラインなし
    assert!(path.is_empty(), "スペースグリフは空のパスである必要がある");
}

#[test]
fn font_measure_text_empty() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    // 空文字列のメトリクスは advance が 0.0 になる境界値ケース。
    let metrics = font.measure_text("");
    assert_eq!(metrics.advance, 0.0);
}

#[test]
fn font_measure_text_single_char() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    // 単一文字の measure_text は glyph_advance(map_char_to_glyph) と一致する境界値。
    let metrics = font.measure_text("A");
    let expected = font.glyph_advance(font.map_char_to_glyph('A'));
    assert_eq!(metrics.advance, expected);
}

#[test]
fn font_measure_text_newline_no_panic() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    // 制御文字を含む文字列でも panic せず計測できることを確認する。
    // 'A' と 'B' は Arial cmap に確実に含まれるため advance > 0 になる。
    let metrics = font.measure_text("A\nB");
    assert!(metrics.advance > 0.0);
}

#[test]
fn font_glyph_advance_out_of_range_is_zero() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);
    // glyph_id が num_glyphs を超えた場合の寛容フォールバック挙動を固定する。
    // 壊れた cmap が範囲外グリフ ID を返したときの保険として機能する。
    assert_eq!(font.glyph_advance(u16::MAX), 0.0);
}

#[test]
fn font_fill_text_integration() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 32.0);

    let mut img = raden::Image::new(256, 64, raden::PixelFormat::Prgb32);
    let mut runtime = raden::PipelineRuntime::new();
    let mut ctx = raden::Context::new(&mut img, &mut runtime);

    ctx.set_fill_style(raden::Rgba32::rgb(255, 255, 255));
    ctx.fill_text(10.0, 48.0, &font, "Hello");
    ctx.end();

    // 描画後、少なくとも一部のピクセルが非ゼロであること
    let has_nonzero = img
        .data()
        .chunks(4)
        .any(|px| px[0] != 0 || px[1] != 0 || px[2] != 0);
    assert!(has_nonzero, "fill_text は可視ピクセルを生成する必要がある");
}
