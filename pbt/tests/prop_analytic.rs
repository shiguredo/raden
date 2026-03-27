use proptest::prelude::*;
use raden::FillRule;
use raden::pipeline::compiler::PipelineCompiler;
use raden::raster::analytic::sweep_reference;

/// テスト用の JIT sweep 関数をコンパイルする。
///
/// PipelineCompiler はテストごとにモジュールを保持するため、
/// テストスイート全体で 1 つのインスタンスを使い回す。
fn compile_sweep(fill_rule: FillRule) -> raden::pipeline::cache::SweepFn {
    let mut compiler = PipelineCompiler::new();
    compiler.compile_sweep(fill_rule)
}

/// JIT sweep の出力がリファレンス実装と一致することを検証する strategy。
///
/// cells の各要素は area-cover パック値 (512 倍スケール) なので、
/// 実際の使用範囲 (-131072..131072) で生成する。
fn cells_strategy(max_len: usize) -> impl Strategy<Value = Vec<i32>> {
    prop::collection::vec(-131072i32..131072, 0..=max_len)
}

/// FillRule の strategy。
fn fill_rule_strategy() -> impl Strategy<Value = FillRule> {
    prop_oneof![Just(FillRule::NonZero), Just(FillRule::EvenOdd)]
}

proptest! {
    /// JIT sweep と Rust リファレンスの出力が完全に一致する。
    #[test]
    fn jit_sweep_matches_reference(
        cells in cells_strategy(256),
        fill_rule in fill_rule_strategy()
    ) {
        let sweep_fn = compile_sweep(fill_rule);
        let len = cells.len();

        // リファレンス sweep (JIT が cells を破壊する前に実行する)
        let mut ref_buf = vec![0xFFu8; len];
        if len > 0 {
            sweep_reference(&cells, &mut ref_buf, 0, len, fill_rule);
        }

        // JIT sweep (cells をゼロクリアする)
        let mut jit_cells = cells;
        let mut jit_buf = vec![0xFFu8; len]; // 0xFF で初期化して書き込み確認
        if len > 0 {
            unsafe {
                sweep_fn(jit_cells.as_mut_ptr(), jit_buf.as_mut_ptr(), len);
            }
        }

        prop_assert_eq!(&jit_buf, &ref_buf, "JIT sweep とリファレンスの出力が不一致 (fill_rule={:?})", fill_rule);

        // JIT sweep がセルをゼロクリアしたことを検証する
        if len > 0 {
            prop_assert!(jit_cells.iter().all(|&c| c == 0), "JIT sweep がセルをゼロクリアしていない");
        }
    }

    /// len=0 で JIT sweep がクラッシュしないことを検証する。
    #[test]
    fn jit_sweep_empty(fill_rule in fill_rule_strategy()) {
        let sweep_fn = compile_sweep(fill_rule);
        let mut cells: Vec<i32> = vec![];
        let mut buf: Vec<u8> = vec![];
        unsafe {
            sweep_fn(cells.as_mut_ptr(), buf.as_mut_ptr(), 0);
        }
        prop_assert!(buf.is_empty());
    }

    /// 4 の倍数でない長さでも正しく処理される。
    #[test]
    fn jit_sweep_non_multiple_of_4(
        base in cells_strategy(60),
        extra_len in 1usize..4,
        fill_rule in fill_rule_strategy()
    ) {
        let sweep_fn = compile_sweep(fill_rule);
        // base の長さを 4 の倍数にしてから extra_len を足す
        let aligned_len = (base.len() / 4) * 4;
        let total_len = aligned_len + extra_len;
        let mut cells = vec![0i32; total_len];
        for (i, &v) in base.iter().take(total_len).enumerate() {
            cells[i] = v;
        }

        let mut ref_buf = vec![0u8; total_len];
        if total_len > 0 {
            sweep_reference(&cells, &mut ref_buf, 0, total_len, fill_rule);
        }

        let mut jit_buf = vec![0u8; total_len];
        if total_len > 0 {
            unsafe {
                sweep_fn(cells.as_mut_ptr(), jit_buf.as_mut_ptr(), total_len);
            }
        }

        prop_assert_eq!(&jit_buf, &ref_buf, "余り要素の処理が不一致 (len={}, fill_rule={:?})", total_len, fill_rule);
    }

    /// 大きな累積値 (|cover >> 9| > 255) が 255 にクランプされる。
    #[test]
    fn jit_sweep_clamp_large_values(
        scale in 131072i32..262144,
        fill_rule in fill_rule_strategy()
    ) {
        let sweep_fn = compile_sweep(fill_rule);
        let mut cells = vec![scale, 0, 0, -scale];
        let mut ref_buf = vec![0u8; 4];
        sweep_reference(&cells, &mut ref_buf, 0, 4, fill_rule);

        let mut jit_buf = vec![0u8; 4];
        unsafe {
            sweep_fn(cells.as_mut_ptr(), jit_buf.as_mut_ptr(), 4);
        }

        prop_assert_eq!(&jit_buf, &ref_buf);
        // NonZero では |cover >> 9| > 255 なので必ず 255 にクランプされる。
        // EvenOdd では周期的折り返しにより 255 とは限らない。
        if fill_rule == FillRule::NonZero {
            prop_assert_eq!(jit_buf[0], 255, "NonZero: 255 にクランプされるべき");
        }
    }

    /// 負の累積値が sshr(9) + fill_rule 変換で正しく処理される。
    #[test]
    fn jit_sweep_negative_cover(
        cells in prop::collection::vec(-131072i32..0, 1..=32),
        fill_rule in fill_rule_strategy()
    ) {
        let sweep_fn = compile_sweep(fill_rule);
        let len = cells.len();
        let mut ref_buf = vec![0u8; len];
        sweep_reference(&cells, &mut ref_buf, 0, len, fill_rule);

        let mut jit_cells = cells;
        let mut jit_buf = vec![0u8; len];
        unsafe {
            sweep_fn(jit_cells.as_mut_ptr(), jit_buf.as_mut_ptr(), len);
        }

        prop_assert_eq!(&jit_buf, &ref_buf);
    }

    /// area-cover パック値の sweep が正しいカバレッジを生成する。
    ///
    /// cover * 512 - area を cells[x] に、area を cells[x+1] に書き込み、
    /// sweep で正しいカバレッジが得られることを検証する。
    #[test]
    fn jit_sweep_area_cover_pack(
        cover_val in 1i32..=256,
        area_frac in 0.0f64..=1.0,
        cell_pos in 0usize..8,
        fill_rule in fill_rule_strategy()
    ) {
        let sweep_fn = compile_sweep(fill_rule);
        let len = cell_pos + 2; // cell_pos と cell_pos+1 に書き込むため
        let mut cells = vec![0i32; len];

        let area = (area_frac * (cover_val as f64) * 512.0) as i32;
        let delta = cover_val * 512 - area;
        cells[cell_pos] = delta;
        if cell_pos + 1 < len {
            cells[cell_pos + 1] = area;
        }

        let mut ref_buf = vec![0u8; len];
        sweep_reference(&cells, &mut ref_buf, 0, len, fill_rule);

        let mut jit_buf = vec![0u8; len];
        unsafe {
            sweep_fn(cells.as_mut_ptr(), jit_buf.as_mut_ptr(), len);
        }

        prop_assert_eq!(&jit_buf, &ref_buf, "area-cover パック値の sweep が不一致 (fill_rule={:?})", fill_rule);

        // NonZero の場合のみ、cell_pos+1 の累積値は cover_val*512 → coverage = cover_val
        if fill_rule == FillRule::NonZero && cell_pos + 1 < len {
            let expected_full = (cover_val as u32).min(255) as u8;
            prop_assert_eq!(
                jit_buf[cell_pos + 1], expected_full,
                "cell_pos+1 のカバレッジが cover_val に一致するべき"
            );
        }
    }

    /// EvenOdd の周期性を検証する。
    ///
    /// 2 winding (cover >> 9 == 512) でカバレッジが 0 に戻ること、
    /// 奇数 winding (cover >> 9 == 256) でカバレッジが 255 になることを確認する。
    #[test]
    fn even_odd_periodicity(winding_count in 0u32..8) {
        let sweep_fn = compile_sweep(FillRule::EvenOdd);
        // winding_count 個分の cover を蓄積する
        // 1 winding = cover >> 9 == 256 → cells 値 = 256 * 512 = 131072
        let cell_value = 131072i32;
        let len = (winding_count as usize) + 1;
        let mut cells = vec![0i32; len];
        for c in cells.iter_mut().take(winding_count as usize) {
            *c = cell_value;
        }

        let mut ref_buf = vec![0u8; len];
        sweep_reference(&cells, &mut ref_buf, 0, len, FillRule::EvenOdd);

        let mut jit_buf = vec![0u8; len];
        unsafe {
            sweep_fn(cells.as_mut_ptr(), jit_buf.as_mut_ptr(), len);
        }

        prop_assert_eq!(&jit_buf, &ref_buf, "EvenOdd 周期性テストで JIT とリファレンスが不一致");

        // 最後の要素 (winding_count 個目の加算後) のカバレッジを検証
        if winding_count > 0 {
            let last = jit_buf[winding_count as usize - 1];
            if winding_count % 2 == 1 {
                // 奇数 winding → 内側 → 255
                prop_assert_eq!(last, 255, "奇数 winding でカバレッジが 255 になるべき (winding={})", winding_count);
            } else {
                // 偶数 winding → 外側 → 0
                prop_assert_eq!(last, 0, "偶数 winding でカバレッジが 0 になるべき (winding={})", winding_count);
            }
        }
    }
}
