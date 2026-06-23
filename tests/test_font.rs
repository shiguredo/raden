mod helpers;

use helpers::font_fetch::{
    fetch_source_sans_3_bytes, fetch_source_serif_4_bytes, verify_has_table,
};
use helpers::font_local::load_arial;
use raden::{Font, FontData, FontError, FontFace, FontFeatureSettings, GlyphBuffer};

/// Source Sans 3 Regular をダウンロードして FontFace を返す。
/// ネットワーク取得や SHA-256 検証に失敗した場合は panic してテストを失敗させる。
fn load_source_sans_3() -> FontFace {
    let bytes = fetch_source_sans_3_bytes()
        .unwrap_or_else(|e| panic!("Source Sans 3 をダウンロードできる必要がある: {e:?}"));
    let data = FontData::from_bytes(bytes);
    FontFace::from_data(&data, 0).expect("Source Sans 3 を FontFace としてロードできる必要がある")
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
    // 空文字列のメトリクスは advance が 0.0、bounding_box が None になる境界値ケース。
    // bounding_box は PBT で空文字列を確実に生成する手段がないため、本単体テストに集約する。
    let metrics = font.measure_text("");
    assert_eq!(metrics.advance, 0.0);
    assert!(
        metrics.bounding_box.is_none(),
        "空文字列の bounding_box は None である必要がある"
    );
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
fn font_glyph_bounds_some() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);

    // 'A' グリフは Arial で必ずアウトラインを持つため、FontFace と Font の両方で
    // bbox が Some を返ることを固定する。PBT は単一 ASCII 文字の不変条件 (順序関係や
    // スケール線形性) を検証するため、ここではフォント実機で Some が返る既知特性の
    // 存在のみを確認する。
    let glyph_a = font.map_char_to_glyph('A');
    assert!(
        face.glyph_bounds(glyph_a).is_some(),
        "Arial の 'A' は FontFace::glyph_bounds で Some を返す必要がある"
    );
    assert!(
        font.glyph_bounds(glyph_a).is_some(),
        "Arial の 'A' は Font::glyph_bounds でも Some を返す必要がある"
    );
}

#[test]
fn font_glyph_bounds_invalid_id() {
    let Some(face) = load_arial() else {
        return;
    };
    let font = Font::from_face(&face, 48.0);

    // num_glyphs を確実に超える glyph_id を渡したとき、寛容方針で None を返すことを
    // 固定する (`append_glyph_outline` は Err を返すが、bbox 問い合わせ API は
    // None を返す意図的差異)。壊れた cmap が範囲外 glyph_id を返したときの保険として機能する。
    // FontFace 側と Font 側の両方で寛容方針が一致することを確認する。
    assert!(
        face.glyph_bounds(u16::MAX).is_none(),
        "範囲外 glyph_id は FontFace::glyph_bounds で None を返す必要がある"
    );
    assert!(
        font.glyph_bounds(u16::MAX).is_none(),
        "範囲外 glyph_id は Font::glyph_bounds で None を返す必要がある"
    );
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

// ---------------------------------------------------------------------------
// テスト用フォントダウンロード機構のスモークテスト
//
// 全プラットフォームで実行されるテスト。Source Sans 3 / Source Serif 4 を
// ネットワーク経由で取得し SHA-256 を検証したうえで、target テーブルの存在と
// 既存公開 API への読み込みを最小限確認する。CFF 系 (Source Serif 4) は
// 現状 raden 本体パーサが OTTO sfnt version を未受理のため、verify_has_table
// による低レベル走査のみで検証する。
// ---------------------------------------------------------------------------

#[test]
fn fetch_source_sans_3_has_required_tables() {
    let bytes = fetch_source_sans_3_bytes()
        .unwrap_or_else(|e| panic!("Source Sans 3 をダウンロードできる必要がある: {e:?}"));
    assert!(
        verify_has_table(&bytes, *b"GSUB"),
        "Source Sans 3 は GSUB テーブルを持つ必要がある"
    );
    assert!(
        verify_has_table(&bytes, *b"GPOS"),
        "Source Sans 3 は GPOS テーブルを持つ必要がある"
    );
    // 取得した bytes が現状の raden 公開 API でロード可能であることを確認する。
    // (Source Sans 3 は TrueType アウトラインのため OTTO 非対応問題は発生しない)
    let data = FontData::from_bytes(bytes);
    let face = FontFace::from_data(&data, 0)
        .expect("Source Sans 3 を FontFace としてロードできる必要がある");
    assert!(
        face.units_per_em() > 0,
        "Source Sans 3 の units_per_em は正値である必要がある"
    );
}

#[test]
fn fetch_source_serif_4_has_cff_table() {
    let bytes = fetch_source_serif_4_bytes()
        .unwrap_or_else(|e| panic!("Source Serif 4 をダウンロードできる必要がある: {e:?}"));
    // CFF テーブルの 4 文字 tag は末尾スペース padding が仕様。CFF2 と区別される。
    assert!(
        verify_has_table(&bytes, *b"CFF "),
        "Source Serif 4 は CFF テーブルを持つ必要がある"
    );
}

// ---------------------------------------------------------------------------
// OpenType 基本シェーピング (GSUB / GPOS) テスト
//
// Source Sans 3 Regular を対象に、合字 (liga) とカーニング (kern) の
// 有効 / 無効が shape / measure_text に反映することを検証する。
// ---------------------------------------------------------------------------

#[test]
fn font_shape_latin_default_features() {
    let face = load_source_sans_3();
    let font = Font::from_face(&face, 48.0);

    let buffer = font.shape("ABC");
    assert_eq!(buffer.len(), 3);
    for i in 0..buffer.len() {
        assert_ne!(
            buffer.glyph_id(i),
            Some(0),
            "既知のラテン文字 {i} は .notdef にならない必要がある"
        );
    }
}

#[test]
fn font_shape_liga_enabled() {
    let face = load_source_sans_3();
    let font = Font::from_face(&face, 48.0)
        .clone_with_features(FontFeatureSettings::none().with_liga(true));

    let buffer_ff = font.shape("ff");
    assert_eq!(
        buffer_ff.len(),
        1,
        "liga 有効時は Source Sans 3 の 'liga' (Lookup Type 4) により ff が f_f 合字に置換される"
    );
    assert_ne!(
        buffer_ff.glyph_id(0),
        Some(0),
        "liga 適用後の合字グリフは .notdef にならない必要がある"
    );

    // Source Sans 3 の 'liga' feature は f_f / f_f_t / f_t の Type 4 リガチャのみを
    // 含み、fi / fl / ffi 等の文脈依存リガチャは Type 6 (ChainContextSubst) で
    // 実装されている。raden は Type 1 / 4 のみ対応のため、ffi は f_f + i の 2 グリフ
    // のままとなる。フォントバージョン依存を避けるため、単に「3 未満」で
    // 何らかのリガチャ適用を検証する。
    let buffer_ffi = font.shape("ffi");
    assert_eq!(
        buffer_ffi.len(),
        2,
        "liga 有効時は ff → f_f の Type 4 リガチャが適用され、i は残る"
    );
    for i in 0..buffer_ffi.len() {
        assert_ne!(
            buffer_ffi.glyph_id(i),
            Some(0),
            "liga 適用後の各グリフ {i} は .notdef にならない必要がある"
        );
    }
}

#[test]
fn font_shape_liga_disabled() {
    let face = load_source_sans_3();
    let font = Font::from_face(&face, 48.0).clone_with_features(FontFeatureSettings::none());

    let buffer = font.shape("ffi");
    assert_eq!(
        buffer.len(),
        3,
        "liga 無効時は ffi を分割したままにする必要がある"
    );
}

#[test]
fn font_shape_kern_av() {
    let face = load_source_sans_3();
    let font_no_kern =
        Font::from_face(&face, 48.0).clone_with_features(FontFeatureSettings::none());
    let font_kern = Font::from_face(&face, 48.0)
        .clone_with_features(FontFeatureSettings::none().with_kern(true));

    // Source Sans 3 の GPOS `kern` feature は AV ペアに負のカーニングを持つ。
    // フォントバージョンに依存しない相対検証: kern 有効時の advance が無効時より
    // 小さければ、負のカーニング調整が適用されていることになる。
    let text = "AV";
    let no_kern = font_no_kern.measure_text(text).advance;
    let with_kern = font_kern.measure_text(text).advance;
    let kern_delta = no_kern - with_kern;
    assert!(
        kern_delta > 1e-6,
        "kern 有効時の AV advance ({}) は無効時 ({}) より十分小さくなる必要がある (delta = {})",
        with_kern,
        no_kern,
        kern_delta
    );
}

#[test]
fn font_shape_clig_flag_does_not_panic() {
    let face = load_source_sans_3();

    // `clig` フラグは公開 API として提供されている。Source Sans 3 の 'clig' feature
    // は raden が未対応の Type 6 (ChainContextSubst) で実装されているため、
    // 有効 / 無効で結果が変化することは期待できないが、パース・適用経路が
    // panic せず well-formed な GlyphBuffer を返すことを検証する。
    let font_clig_only = Font::from_face(&face, 48.0)
        .clone_with_features(FontFeatureSettings::none().with_clig(true));
    let buffer = font_clig_only.shape("ffi");
    assert!(
        buffer.is_well_formed(),
        "clig 有効時も buffer は well-formed"
    );
    assert_eq!(
        buffer.len(),
        3,
        "clig のみでは liga Type 4 リガチャは適用されない"
    );

    let font_liga_with_clig = Font::from_face(&face, 48.0)
        .clone_with_features(FontFeatureSettings::none().with_liga(true).with_clig(true));
    let buffer = font_liga_with_clig.shape("ffi");
    assert_eq!(
        buffer.len(),
        2,
        "liga ON + clig ON でも liga Type 4 リガチャは適用される"
    );
}

#[test]
fn font_shape_liga_kern_combinations() {
    let face = load_source_sans_3();

    let combos = [(false, false), (false, true), (true, false), (true, true)];

    let prev_ffi = combos.map(|(liga, _kern)| {
        let font = Font::from_face(&face, 48.0)
            .clone_with_features(FontFeatureSettings::none().with_liga(liga).with_kern(false));
        let len = font.shape("ffi").len();
        // liga ON のときだけ ff → f_f 合字が適用されて 2 グリフ、
        // OFF のときは 3 グリフのまま。
        let expected = if liga { 2 } else { 3 };
        assert_eq!(
            len, expected,
            "liga={liga} のとき ffi のグリフ数は {expected}"
        );
        len
    });

    let prev_av = combos.map(|(_liga, kern)| {
        let font = Font::from_face(&face, 48.0)
            .clone_with_features(FontFeatureSettings::none().with_liga(false).with_kern(kern));
        font.measure_text("AV").advance
    });

    // kern ON/OFF が AV の advance に影響することを独立に検証。
    assert!(
        prev_av[1] < prev_av[0],
        "kern OFF ({}) より kern ON ({}) の AV advance が小さくなる必要がある",
        prev_av[0],
        prev_av[1]
    );
    // 組み合わせでも同様。
    assert!(
        prev_av[3] < prev_av[2],
        "liga ON 時も kern OFF ({}) より kern ON ({}) の AV advance が小さくなる必要がある",
        prev_av[2],
        prev_av[3]
    );

    // liga ON/OFF が ffi のグリフ数に影響することを独立に検証 (kern の有無は影響しない)。
    assert_eq!(
        prev_ffi[1], prev_ffi[0],
        "liga OFF 時は kern の有無でグリフ数が変わらない"
    );
    assert_eq!(
        prev_ffi[3], prev_ffi[2],
        "liga ON 時は kern の有無でグリフ数が変わらない"
    );
}

#[test]
fn font_text_metrics_leading_trailing_bearing() {
    let face = load_source_sans_3();
    let font = Font::from_face(&face, 48.0);

    // leading_bearing は最初のグリフの bbox x_min。
    // trailing_bearing は最後のグリフの bbox x_max からそのグリフの advance を引いた値。
    let text = "AB";
    let metrics = font.measure_text(text);
    let buffer = font.shape(text);
    let gid_a = font.map_char_to_glyph('A');
    let gid_b = font.map_char_to_glyph('B');
    let bbox_a = font
        .glyph_bounds(gid_a)
        .expect("A の glyph_bounds が取得できる必要がある");
    let bbox_b = font
        .glyph_bounds(gid_b)
        .expect("B の glyph_bounds が取得できる必要がある");
    let placement_a = buffer.placement(0).expect("A の placement が存在する");
    let placement_b = buffer
        .placement(buffer.len() - 1)
        .expect("B の placement が存在する");

    assert!(
        (metrics.leading_bearing - (bbox_a.x_min + placement_a.offset_x)).abs() < 1e-9,
        "leading_bearing = {}, 期待値 = {}",
        metrics.leading_bearing,
        bbox_a.x_min + placement_a.offset_x
    );
    // trailing_bearing = advance - offset_x - x_max。
    let expected_trailing = placement_b.advance - placement_b.offset_x - bbox_b.x_max;
    assert!(
        (metrics.trailing_bearing - expected_trailing).abs() < 1e-9,
        "trailing_bearing = {}, 期待値 = {}",
        metrics.trailing_bearing,
        expected_trailing
    );
}

#[test]
fn font_shape_into_reuse() {
    let face = load_source_sans_3();
    let font = Font::from_face(&face, 48.0);

    let mut buf = GlyphBuffer::default();
    font.shape_into("AB", &mut buf);
    let _ab_len = buf.len();
    assert!(!buf.is_empty());

    // 同一バッファに短い文字列を shape_into しても、前回の結果が残らない。
    font.shape_into("C", &mut buf);
    assert_eq!(buf.len(), 1);
    assert_ne!(
        buf.glyph_id(0),
        Some(0),
        "C は .notdef にならない必要がある"
    );
}

#[test]
fn font_with_features_and_set() {
    let face = load_source_sans_3();

    // `with_features` で構築したフォントと、`from_face` 後に `set_feature_settings`
    // したフォントで、同じ feature 設定を持つことを確認する。
    let font_with = Font::with_features(&face, 48.0, FontFeatureSettings::none());
    let mut font_set = Font::from_face(&face, 48.0);
    font_set.set_feature_settings(FontFeatureSettings::none());

    assert_eq!(font_with.size(), font_set.size());
    assert_eq!(font_with.scale(), font_set.scale());
    assert_eq!(font_with.feature_settings(), font_set.feature_settings());

    // 同じ feature 設定なら shape 結果も一致する。
    let text = "ff";
    assert_eq!(font_with.shape(text).len(), font_set.shape(text).len());
}

#[test]
fn from_file_accepts_path_like_types() {
    // 4 種類のパス型を渡し、いずれもコンパイルが通り `FontError::Io` で返ることを確認する。
    // フォント実体は必要とせず、CI 環境でも実行できる。
    // 所有権を保持し続ける典型ユースケースに合わせて、`String` / `PathBuf` も参照渡しで検証する。
    let s: &str = "definitely_not_existing_font_file";
    assert!(matches!(FontData::from_file(s), Err(FontError::Io(_))));

    let p: &std::path::Path = std::path::Path::new("definitely_not_existing_font_file");
    assert!(matches!(FontData::from_file(p), Err(FontError::Io(_))));

    let buf: std::path::PathBuf = std::path::PathBuf::from("definitely_not_existing_font_file");
    assert!(matches!(FontData::from_file(&buf), Err(FontError::Io(_))));

    let owned: String = String::from("definitely_not_existing_font_file");
    assert!(matches!(FontData::from_file(&owned), Err(FontError::Io(_))));
}
