//! Tests for IDM car-following model.

use velos_vehicle::idm::{IdmParams, idm_acceleration, integrate_with_stopping_guard};
use velos_vehicle::types::{VehicleType, default_idm_params};

/// Standard car IDM parameters for testing.
fn car_params() -> IdmParams {
    default_idm_params(VehicleType::Car)
}

#[test]
fn free_flow_acceleration_approaches_a_max() {
    let params = car_params();
    // Large gap, no leader effectively (1000m gap, no delta_v)
    let accel = idm_acceleration(&params, 5.0, 1000.0, 0.0);
    // With v=5 << v0=13.9, free term ~1.0, interaction ~0, so accel ~ a_max
    assert!(accel > 0.9 * params.a, "free-flow accel={accel} should approach a_max={}", params.a);
    assert!(accel <= params.a, "accel={accel} must not exceed a_max={}", params.a);
}

#[test]
fn following_close_leader_produces_braking() {
    let params = car_params();
    // Close gap (3m), same speed as leader (delta_v=0), speed=10 m/s
    let accel = idm_acceleration(&params, 10.0, 3.0, 0.0);
    assert!(accel < 0.0, "close following accel={accel} should be negative (braking)");
}

#[test]
fn approaching_stopped_leader_increases_deceleration_as_gap_shrinks() {
    let params = car_params();
    // Approaching stopped leader: delta_v > 0 (we are faster)
    let accel_far = idm_acceleration(&params, 10.0, 20.0, 10.0);
    let accel_near = idm_acceleration(&params, 10.0, 5.0, 10.0);
    assert!(
        accel_near < accel_far,
        "closer gap should produce stronger braking: near={accel_near}, far={accel_far}"
    );
}

#[test]
fn zero_speed_kickstart_no_division_issues() {
    let params = car_params();
    // v=0, large gap, no leader moving -- should produce positive accel
    let accel = idm_acceleration(&params, 0.0, 100.0, 0.0);
    assert!(accel > 0.0, "zero-speed kickstart should give positive accel={accel}");
    assert!(accel.is_finite(), "accel must be finite");
}

#[test]
fn acceleration_clamped_to_valid_range() {
    let params = car_params();
    // Test upper clamp: free flow with v ~ 0
    let accel_high = idm_acceleration(&params, 0.0, 1000.0, 0.0);
    assert!(accel_high <= params.a, "upper clamp: {accel_high} <= {}", params.a);

    // Test lower clamp: very close gap, high speed, approaching fast
    let accel_low = idm_acceleration(&params, 13.0, 0.5, 13.0);
    assert!(accel_low >= -9.0, "lower clamp: {accel_low} >= -9.0");
}

#[test]
fn stopping_guard_prevents_negative_velocity() {
    // Strong braking that would push v below zero
    let (v_new, dx) = integrate_with_stopping_guard(2.0, -8.0, 1.0);
    assert_eq!(v_new, 0.0, "velocity must be zero, not negative");
    assert!(dx >= 0.0, "displacement must be non-negative: dx={dx}");
}

#[test]
fn stopping_guard_normal_integration() {
    // Normal case: mild braking, velocity stays positive
    let (v_new, dx) = integrate_with_stopping_guard(10.0, -1.0, 0.1);
    let expected_v = 10.0 + (-1.0) * 0.1; // 9.9
    let expected_dx = 10.0 * 0.1 + 0.5 * (-1.0) * 0.01; // 0.995
    assert!((v_new - expected_v).abs() < 1e-10, "v_new={v_new}, expected={expected_v}");
    assert!((dx - expected_dx).abs() < 1e-10, "dx={dx}, expected={expected_dx}");
}

#[test]
fn stopping_guard_zero_initial_speed() {
    // Already stopped, decelerating -- should stay at zero
    let (v_new, dx) = integrate_with_stopping_guard(0.0, -5.0, 0.1);
    assert_eq!(v_new, 0.0);
    assert_eq!(dx, 0.0);
}

#[test]
fn motorbike_params_differ_from_car() {
    let car = default_idm_params(VehicleType::Car);
    let moto = default_idm_params(VehicleType::Motorbike);
    assert!(moto.v0 < car.v0, "motorbike desired speed should be lower");
    assert!(moto.s0 < car.s0, "motorbike minimum gap should be smaller");
    assert!(moto.a > car.a, "motorbike max accel should be higher (lighter)");
}
