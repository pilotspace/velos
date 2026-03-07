use velos_meso::buffer_zone::{interpolate_idm_params, smoothstep, velocity_matching_speed, BufferZone};
use velos_vehicle::idm::IdmParams;

// ── Smoothstep function tests ──────────────────────────────────────

#[test]
fn smoothstep_at_zero() {
    assert!((smoothstep(0.0) - 0.0).abs() < 1e-12);
}

#[test]
fn smoothstep_at_half() {
    // 3*(0.5)^2 - 2*(0.5)^3 = 3*0.25 - 2*0.125 = 0.75 - 0.25 = 0.5
    assert!((smoothstep(0.5) - 0.5).abs() < 1e-12);
}

#[test]
fn smoothstep_at_one() {
    // 3*(1.0)^2 - 2*(1.0)^3 = 3 - 2 = 1.0
    assert!((smoothstep(1.0) - 1.0).abs() < 1e-12);
}

#[test]
fn smoothstep_clamped_below_zero() {
    assert!((smoothstep(-0.5) - 0.0).abs() < 1e-12);
}

#[test]
fn smoothstep_clamped_above_one() {
    assert!((smoothstep(1.5) - 1.0).abs() < 1e-12);
}

// ── C1 continuity verification ─────────────────────────────────────

#[test]
fn smoothstep_derivative_near_zero_at_boundaries() {
    // Derivative of 3x^2 - 2x^3 is 6x - 6x^2 = 6x(1-x)
    // At x=0: derivative = 0, at x=1: derivative = 0
    // Verify with finite differences
    let eps = 1e-6;
    let deriv_at_0 = (smoothstep(eps) - smoothstep(0.0)) / eps;
    let deriv_at_1 = (smoothstep(1.0) - smoothstep(1.0 - eps)) / eps;

    assert!(
        deriv_at_0.abs() < 1e-3,
        "derivative at 0 should be ~0, got {deriv_at_0}"
    );
    assert!(
        deriv_at_1.abs() < 1e-3,
        "derivative at 1 should be ~0, got {deriv_at_1}"
    );
}

// ── IDM parameter interpolation tests ──────────────────────────────

fn relaxed_params() -> IdmParams {
    IdmParams {
        v0: 13.89,
        s0: 2.0,
        t_headway: 3.0,
        a: 0.5,
        b: 1.5,
        delta: 4.0,
    }
}

fn normal_params() -> IdmParams {
    IdmParams {
        v0: 13.89,
        s0: 2.0,
        t_headway: 1.6,
        a: 1.0,
        b: 2.0,
        delta: 4.0,
    }
}

#[test]
fn interpolation_at_meso_boundary_returns_relaxed() {
    // distance = 0m (meso boundary) -> smoothstep(0) = 0 -> fully relaxed
    let result = interpolate_idm_params(&relaxed_params(), &normal_params(), 0.0, 100.0);
    assert!((result.t_headway - 3.0).abs() < 1e-9, "t_headway should be relaxed (3.0)");
    assert!((result.a - 0.5).abs() < 1e-9, "a should be relaxed (0.5)");
    assert!((result.b - 1.5).abs() < 1e-9, "b should be relaxed (1.5)");
}

#[test]
fn interpolation_at_micro_boundary_returns_normal() {
    // distance = 100m (micro boundary) -> smoothstep(1) = 1 -> fully normal
    let result = interpolate_idm_params(&relaxed_params(), &normal_params(), 100.0, 100.0);
    assert!((result.t_headway - 1.6).abs() < 1e-9, "t_headway should be normal (1.6)");
    assert!((result.a - 1.0).abs() < 1e-9, "a should be normal (1.0)");
    assert!((result.b - 2.0).abs() < 1e-9, "b should be normal (2.0)");
}

#[test]
fn interpolation_at_midpoint_is_between() {
    // distance = 50m -> smoothstep(0.5) = 0.5 -> midpoint
    let result = interpolate_idm_params(&relaxed_params(), &normal_params(), 50.0, 100.0);
    // t_headway: 3.0 + (1.6 - 3.0) * 0.5 = 3.0 - 0.7 = 2.3
    assert!((result.t_headway - 2.3).abs() < 1e-9, "t_headway should be 2.3 at midpoint");
    // a: 0.5 + (1.0 - 0.5) * 0.5 = 0.75
    assert!((result.a - 0.75).abs() < 1e-9, "a should be 0.75 at midpoint");
    // b: 1.5 + (2.0 - 1.5) * 0.5 = 1.75
    assert!((result.b - 1.75).abs() < 1e-9, "b should be 1.75 at midpoint");
}

#[test]
fn interpolation_preserves_shared_params() {
    // v0, s0, delta should be the same (same in both relaxed and normal)
    let result = interpolate_idm_params(&relaxed_params(), &normal_params(), 50.0, 100.0);
    assert!((result.v0 - 13.89).abs() < 1e-9);
    assert!((result.s0 - 2.0).abs() < 1e-9);
    assert!((result.delta - 4.0).abs() < 1e-9);
}

// ── Velocity matching tests ────────────────────────────────────────

#[test]
fn velocity_matching_takes_minimum() {
    assert!((velocity_matching_speed(15.0, 12.0) - 12.0).abs() < 1e-9);
    assert!((velocity_matching_speed(10.0, 14.0) - 10.0).abs() < 1e-9);
    assert!((velocity_matching_speed(12.0, 12.0) - 12.0).abs() < 1e-9);
}

// ── BufferZone should_insert tests ─────────────────────────────────

#[test]
fn should_insert_when_past_buffer_and_speed_matched() {
    assert!(BufferZone::should_insert(100.0, 1.5));
}

#[test]
fn should_not_insert_when_within_buffer() {
    assert!(!BufferZone::should_insert(50.0, 1.0));
}

#[test]
fn should_not_insert_when_speed_mismatch() {
    assert!(!BufferZone::should_insert(100.0, 3.0));
}

#[test]
fn should_insert_at_exact_boundary_conditions() {
    // At exactly buffer_length and exactly speed threshold
    assert!(BufferZone::should_insert(100.0, 2.0));
}
