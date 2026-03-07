//! Tests for Krauss car-following model.
//!
//! The Krauss model is the SUMO-default car-following model. It computes a
//! safe following speed and applies stochastic dawdle deceleration.
//! These tests validate SUMO-faithful behavior.

use rand::rngs::StdRng;
use rand::SeedableRng;
use velos_vehicle::krauss::{KraussParams, krauss_dawdle, krauss_safe_speed, krauss_update};

// ---------------------------------------------------------------------------
// KraussParams defaults
// ---------------------------------------------------------------------------

#[test]
fn sumo_default_params() {
    let p = KraussParams::sumo_default();
    assert!((p.sigma - 0.5).abs() < f64::EPSILON);
    assert!((p.accel - 2.6).abs() < f64::EPSILON);
    assert!((p.decel - 4.5).abs() < f64::EPSILON);
    assert!((p.tau - 1.0).abs() < f64::EPSILON);
    assert!((p.max_speed - 13.89).abs() < f64::EPSILON);
    assert!((p.min_gap - 2.5).abs() < f64::EPSILON);
}

// ---------------------------------------------------------------------------
// krauss_safe_speed tests
// ---------------------------------------------------------------------------

#[test]
fn safe_speed_large_gap_returns_high_speed() {
    let p = KraussParams::sumo_default();
    let v_safe = krauss_safe_speed(&p, 1000.0, 13.89, 13.89);
    // Large gap: v_safe should be well above max_speed (not constrained here)
    assert!(
        v_safe > p.max_speed,
        "large gap v_safe={v_safe} should exceed max_speed={}",
        p.max_speed
    );
}

#[test]
fn safe_speed_zero_gap_returns_zero() {
    let p = KraussParams::sumo_default();
    let v_safe = krauss_safe_speed(&p, 0.0, 0.0, 10.0);
    assert_eq!(v_safe, 0.0, "zero gap with stopped leader -> v_safe=0");
}

#[test]
fn safe_speed_moderate_gap() {
    let p = KraussParams::sumo_default();
    let v_safe = krauss_safe_speed(&p, 20.0, 10.0, 12.0);
    assert!(
        v_safe > 0.0 && v_safe < 100.0,
        "moderate gap v_safe={v_safe} should be between 0 and 100"
    );
}

#[test]
fn safe_speed_non_negative() {
    let p = KraussParams::sumo_default();
    // Very small gap, leader much slower -- v_safe should be clamped to 0
    let v_safe = krauss_safe_speed(&p, 0.5, 0.0, 20.0);
    assert!(v_safe >= 0.0, "v_safe={v_safe} must be non-negative");
}

// ---------------------------------------------------------------------------
// krauss_dawdle tests
// ---------------------------------------------------------------------------

#[test]
fn dawdle_reduces_speed() {
    let p = KraussParams::sumo_default();
    let mut rng = StdRng::seed_from_u64(42);
    let original_speed = 10.0;

    // Run 100 trials: dawdle should never increase speed
    for _ in 0..100 {
        let dawdled = krauss_dawdle(original_speed, &p, &mut rng);
        assert!(
            dawdled <= original_speed,
            "dawdle should not increase speed: {dawdled} > {original_speed}"
        );
        assert!(dawdled >= 0.0, "dawdled speed must be non-negative");
    }
}

#[test]
fn dawdle_sigma_zero_returns_unchanged() {
    let mut p = KraussParams::sumo_default();
    p.sigma = 0.0;
    let mut rng = StdRng::seed_from_u64(42);
    let speed = 10.0;
    let dawdled = krauss_dawdle(speed, &p, &mut rng);
    assert!(
        (dawdled - speed).abs() < f64::EPSILON,
        "sigma=0 should return speed unchanged: got {dawdled}"
    );
}

#[test]
fn dawdle_has_variability() {
    let p = KraussParams::sumo_default();
    let mut rng = StdRng::seed_from_u64(42);
    let speed = 10.0;

    // Collect 50 dawdle results; they should not all be identical
    let results: Vec<f64> = (0..50).map(|_| krauss_dawdle(speed, &p, &mut rng)).collect();
    let all_same = results.windows(2).all(|w| (w[0] - w[1]).abs() < f64::EPSILON);
    assert!(!all_same, "dawdle with sigma=0.5 should produce variable results");
}

// ---------------------------------------------------------------------------
// krauss_update tests
// ---------------------------------------------------------------------------

#[test]
fn update_produces_non_negative_velocity() {
    let p = KraussParams::sumo_default();
    let mut rng = StdRng::seed_from_u64(42);
    let dt = 0.1;

    // Various scenarios
    let cases = [
        (10.0, 50.0, 13.89),  // normal following
        (13.89, 5.0, 0.0),    // approaching stopped leader
        (0.0, 100.0, 13.89),  // starting from stop
        (5.0, 0.5, 0.0),      // very close to stopped leader
    ];

    for (own_speed, gap, leader_speed) in cases {
        let (v, dx) = krauss_update(&p, own_speed, gap, leader_speed, dt, &mut rng);
        assert!(
            v >= 0.0,
            "velocity must be non-negative: v={v} for own={own_speed}, gap={gap}, leader={leader_speed}"
        );
        assert!(
            dx >= 0.0,
            "displacement must be non-negative: dx={dx}"
        );
    }
}

#[test]
fn update_free_flow_accelerates() {
    let p = KraussParams::sumo_default();
    let mut rng = StdRng::seed_from_u64(42);
    let dt = 0.1;

    // Starting from moderate speed with large gap -- should accelerate
    let own_speed = 5.0;
    let (v, _dx) = krauss_update(&p, own_speed, 1000.0, 13.89, dt, &mut rng);
    // v_desired = min(5.0 + 2.6*0.1, 13.89) = min(5.26, 13.89) = 5.26
    // v_safe is very large (>max_speed), so v_next = 5.26
    // After dawdle: v >= 5.26 - sigma*accel = 5.26 - 1.3 = 3.96
    assert!(v >= 3.96, "free-flow should maintain/increase speed: v={v}");
}

#[test]
fn update_respects_max_speed() {
    let p = KraussParams::sumo_default();
    let mut rng = StdRng::seed_from_u64(42);
    let dt = 1.0;

    // Already at max speed with huge gap
    let (v, _dx) = krauss_update(&p, p.max_speed, 10000.0, p.max_speed, dt, &mut rng);
    // v_desired = min(13.89 + 2.6*1.0, 13.89) = 13.89
    // After dawdle: v <= 13.89
    assert!(
        v <= p.max_speed,
        "velocity should not exceed max_speed: v={v}, max={}",
        p.max_speed
    );
}
