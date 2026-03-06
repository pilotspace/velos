//! Tests for MOBIL lane-change decision model.

use velos_vehicle::mobil::{MobilParams, LaneChangeContext, mobil_decision};
use velos_vehicle::types::default_mobil_params;

fn default_params() -> MobilParams {
    default_mobil_params()
}

#[test]
fn safety_rejection_when_new_follower_brakes_too_hard() {
    let params = default_params();
    let ctx = LaneChangeContext {
        accel_current: 0.5,
        accel_target: 2.0,
        accel_new_follower: -5.0, // worse than safe_decel=-4.0
        accel_old_follower: 0.0,
        is_right: true,
    };
    assert!(!mobil_decision(&params, &ctx), "should reject: new follower decel too strong");
}

#[test]
fn safety_passes_at_boundary() {
    let params = default_params();
    let ctx = LaneChangeContext {
        accel_current: 0.0,
        accel_target: 2.0,
        accel_new_follower: -4.0, // exactly at safe_decel boundary
        accel_old_follower: 0.0,
        is_right: true,
    };
    // At boundary (-4.0 >= -4.0), safety passes. Incentive should decide.
    // own_advantage = 2.0, follower_disadvantage = 0.0 - (-4.0) = 4.0
    // incentive = 2.0 - 0.3*4.0 + 0.1 = 2.0 - 1.2 + 0.1 = 0.9 > 0.2
    assert!(mobil_decision(&params, &ctx), "should accept: safety passes, good incentive");
}

#[test]
fn incentive_acceptance_with_clear_advantage() {
    let params = default_params();
    let ctx = LaneChangeContext {
        accel_current: 0.0,
        accel_target: 1.5,     // big advantage
        accel_new_follower: -1.0, // safe
        accel_old_follower: -0.5,
        is_right: true,
    };
    // own_advantage = 1.5, follower_disadvantage = -0.5 - (-1.0) = 0.5
    // incentive = 1.5 - 0.3*0.5 + 0.1 = 1.5 - 0.15 + 0.1 = 1.45 > 0.2
    assert!(mobil_decision(&params, &ctx), "should accept: clear advantage");
}

#[test]
fn incentive_rejection_below_threshold() {
    let params = default_params();
    let ctx = LaneChangeContext {
        accel_current: 1.0,
        accel_target: 1.1,     // tiny advantage
        accel_new_follower: 0.0,
        accel_old_follower: 0.5,
        is_right: false,       // left = negative bias
    };
    // own_advantage = 0.1, follower_disadvantage = 0.5 - 0.0 = 0.5
    // incentive = 0.1 - 0.3*0.5 + (-0.1) = 0.1 - 0.15 - 0.1 = -0.15 < 0.2
    assert!(!mobil_decision(&params, &ctx), "should reject: below threshold");
}

#[test]
fn right_bias_makes_right_change_easier() {
    let params = default_params();
    // Same context but different lane direction
    let base = LaneChangeContext {
        accel_current: 0.5,
        accel_target: 0.7,
        accel_new_follower: 0.0,
        accel_old_follower: 0.0,
        is_right: false,
    };
    let right = LaneChangeContext {
        is_right: true,
        ..base
    };
    // Right gets +0.1, left gets -0.1: a 0.2 m/s^2 difference
    let left_result = mobil_decision(&params, &base);
    let right_result = mobil_decision(&params, &right);
    // own_advantage = 0.2, follower_disadvantage = 0.0
    // left incentive = 0.2 - 0.0 - 0.1 = 0.1 (below threshold 0.2)
    // right incentive = 0.2 - 0.0 + 0.1 = 0.3 (above threshold 0.2)
    assert!(!left_result, "left should be rejected (below threshold)");
    assert!(right_result, "right should be accepted (above threshold with bias)");
}

#[test]
fn politeness_reduces_selfish_advantage() {
    // High politeness makes lane change harder
    let selfish = MobilParams {
        politeness: 0.0,
        ..default_params()
    };
    let polite = MobilParams {
        politeness: 1.0,
        ..default_params()
    };
    let ctx = LaneChangeContext {
        accel_current: 0.0,
        accel_target: 1.0,
        accel_new_follower: -2.0,
        accel_old_follower: 0.0,
        is_right: true,
    };
    // Selfish: 1.0 - 0.0*2.0 + 0.1 = 1.1 > 0.2 -> accept
    // Polite: 1.0 - 1.0*2.0 + 0.1 = -0.9 < 0.2 -> reject
    assert!(mobil_decision(&selfish, &ctx), "selfish should accept");
    assert!(!mobil_decision(&polite, &ctx), "polite should reject");
}
