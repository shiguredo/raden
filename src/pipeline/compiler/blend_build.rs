use cranelift_codegen::isa::TargetFrontendConfig;
use cranelift_frontend::FunctionBuilder;

use super::blend_modes::*;
use super::porter_duff::{build_generic_compose, build_generic_compose_cov};

// =============================================================================
// ブレンドモード build 関数
// =============================================================================

pub(super) fn build_minus(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_minus_simd,
        compose_minus_scalar,
    );
}

pub(super) fn build_modulate(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_modulate_simd,
        compose_modulate_scalar,
    );
}

pub(super) fn build_multiply(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_multiply_simd,
        compose_multiply_scalar,
    );
}

pub(super) fn build_screen(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_screen_simd,
        compose_screen_scalar,
    );
}

pub(super) fn build_overlay(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_overlay_simd,
        compose_overlay_scalar,
    );
}

pub(super) fn build_darken(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_darken_simd,
        compose_darken_scalar,
    );
}

pub(super) fn build_lighten(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_lighten_simd,
        compose_lighten_scalar,
    );
}

pub(super) fn build_color_dodge(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_color_dodge_simd,
        compose_color_dodge_scalar,
    );
}

pub(super) fn build_color_burn(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_color_burn_simd,
        compose_color_burn_scalar,
    );
}

pub(super) fn build_linear_burn(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_linear_burn_simd,
        compose_linear_burn_scalar,
    );
}

pub(super) fn build_linear_light(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_linear_light_simd,
        compose_linear_light_scalar,
    );
}

pub(super) fn build_pin_light(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_pin_light_simd,
        compose_pin_light_scalar,
    );
}

pub(super) fn build_hard_light(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_hard_light_simd,
        compose_hard_light_scalar,
    );
}

pub(super) fn build_soft_light(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_soft_light_simd,
        compose_soft_light_scalar,
    );
}

pub(super) fn build_difference(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_difference_simd,
        compose_difference_scalar,
    );
}

pub(super) fn build_exclusion(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose(
        bcx,
        frontend_config,
        compose_exclusion_simd,
        compose_exclusion_scalar,
    );
}

// カバレッジ付き build 関数

pub(super) fn build_minus_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_minus_simd,
        compose_minus_scalar,
    );
}

pub(super) fn build_modulate_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_modulate_simd,
        compose_modulate_scalar,
    );
}

pub(super) fn build_multiply_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_multiply_simd,
        compose_multiply_scalar,
    );
}

pub(super) fn build_screen_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_screen_simd,
        compose_screen_scalar,
    );
}

pub(super) fn build_overlay_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_overlay_simd,
        compose_overlay_scalar,
    );
}

pub(super) fn build_darken_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_darken_simd,
        compose_darken_scalar,
    );
}

pub(super) fn build_lighten_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_lighten_simd,
        compose_lighten_scalar,
    );
}

pub(super) fn build_color_dodge_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_color_dodge_simd,
        compose_color_dodge_scalar,
    );
}

pub(super) fn build_color_burn_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_color_burn_simd,
        compose_color_burn_scalar,
    );
}

pub(super) fn build_linear_burn_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_linear_burn_simd,
        compose_linear_burn_scalar,
    );
}

pub(super) fn build_linear_light_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_linear_light_simd,
        compose_linear_light_scalar,
    );
}

pub(super) fn build_pin_light_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_pin_light_simd,
        compose_pin_light_scalar,
    );
}

pub(super) fn build_hard_light_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_hard_light_simd,
        compose_hard_light_scalar,
    );
}

pub(super) fn build_soft_light_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_soft_light_simd,
        compose_soft_light_scalar,
    );
}

pub(super) fn build_difference_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_difference_simd,
        compose_difference_scalar,
    );
}

pub(super) fn build_exclusion_cov(bcx: FunctionBuilder, frontend_config: TargetFrontendConfig) {
    build_generic_compose_cov(
        bcx,
        frontend_config,
        compose_exclusion_simd,
        compose_exclusion_scalar,
    );
}
