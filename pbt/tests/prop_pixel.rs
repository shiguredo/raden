use proptest::prelude::*;
use raden::premultiply_rgba;

proptest! {
    /// premultiply 後の各カラーチャネルは alpha 以下になる。
    #[test]
    fn channels_le_alpha(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255, a in 0u8..=255) {
        let prgb = premultiply_rgba(r, g, b, a);
        let out_a = (prgb >> 24) & 0xFF;
        let out_r = (prgb >> 16) & 0xFF;
        let out_g = (prgb >> 8) & 0xFF;
        let out_b = prgb & 0xFF;

        prop_assert_eq!(out_a, a as u32);
        prop_assert!(out_r <= out_a, "r={out_r} > a={out_a}");
        prop_assert!(out_g <= out_a, "g={out_g} > a={out_a}");
        prop_assert!(out_b <= out_a, "b={out_b} > a={out_a}");
    }

    /// alpha=255 で入力値がそのまま保持される。
    #[test]
    fn opaque_identity(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
        let prgb = premultiply_rgba(r, g, b, 255);
        let out_r = ((prgb >> 16) & 0xFF) as u8;
        let out_g = ((prgb >> 8) & 0xFF) as u8;
        let out_b = (prgb & 0xFF) as u8;

        prop_assert_eq!(out_r, r);
        prop_assert_eq!(out_g, g);
        prop_assert_eq!(out_b, b);
    }

    /// alpha=0 で全チャネルがゼロになる。
    #[test]
    fn transparent_zero(r in 0u8..=255, g in 0u8..=255, b in 0u8..=255) {
        let prgb = premultiply_rgba(r, g, b, 0);
        prop_assert_eq!(prgb, 0);
    }
}
