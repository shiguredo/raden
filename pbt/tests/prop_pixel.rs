use raden::premultiply_rgba;

/// 1 テストあたりのケース数。
const CASES: usize = 256;

/// シード再現用の環境変数名。
const SEED_ENV: &str = "RADEN_PBT_SEED";

/// premultiply 後の各カラーチャネルは alpha 以下になる。
#[test]
fn channels_le_alpha() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let r = noprop::sample_u8(ctx);
        let g = noprop::sample_u8(ctx);
        let b = noprop::sample_u8(ctx);
        let a = noprop::sample_u8(ctx);

        let prgb = premultiply_rgba(r, g, b, a);
        let out_a = (prgb >> 24) & 0xFF;
        let out_r = (prgb >> 16) & 0xFF;
        let out_g = (prgb >> 8) & 0xFF;
        let out_b = prgb & 0xFF;

        assert_eq!(out_a, a as u32);
        assert!(out_r <= out_a, "r={out_r} > a={out_a}");
        assert!(out_g <= out_a, "g={out_g} > a={out_a}");
        assert!(out_b <= out_a, "b={out_b} > a={out_a}");
        Ok(())
    })?;
    Ok(())
}

/// alpha=255 で入力値がそのまま保持される。
#[test]
fn opaque_identity() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let r = noprop::sample_u8(ctx);
        let g = noprop::sample_u8(ctx);
        let b = noprop::sample_u8(ctx);

        let prgb = premultiply_rgba(r, g, b, 255);
        let out_r = ((prgb >> 16) & 0xFF) as u8;
        let out_g = ((prgb >> 8) & 0xFF) as u8;
        let out_b = (prgb & 0xFF) as u8;

        assert_eq!(out_r, r);
        assert_eq!(out_g, g);
        assert_eq!(out_b, b);
        Ok(())
    })?;
    Ok(())
}

/// alpha=0 で全チャネルがゼロになる。
#[test]
fn transparent_zero() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time(SEED_ENV)?;
    let mut runner = noprop::Runner::new(seed);
    runner.run(CASES, |ctx| {
        let r = noprop::sample_u8(ctx);
        let g = noprop::sample_u8(ctx);
        let b = noprop::sample_u8(ctx);
        let prgb = premultiply_rgba(r, g, b, 0);
        assert_eq!(prgb, 0);
        Ok(())
    })?;
    Ok(())
}
