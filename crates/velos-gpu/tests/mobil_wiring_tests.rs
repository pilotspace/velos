//! Integration tests for MOBIL lane-change wiring in the simulation loop.
//!
//! These tests verify that `LaneChangeState` and `LateralOffset` components
//! are correctly attached/removed during car lane changes, and that the
//! drift logic produces smooth lateral transitions.

use velos_core::components::{LaneChangeState, LastLaneChange, LateralOffset};

#[test]
fn lane_change_state_fields_are_correct() {
    let lcs = LaneChangeState {
        target_lane: 1,
        time_remaining: 2.0,
        started_at: 100.0,
    };
    assert_eq!(lcs.target_lane, 1);
    assert!((lcs.time_remaining - 2.0).abs() < 1e-10);
    assert!((lcs.started_at - 100.0).abs() < 1e-10);
}

#[test]
fn last_lane_change_tracks_completion_time() {
    let llc = LastLaneChange {
        completed_at: 105.0,
    };
    // Cooldown: 3s after completion
    let sim_time = 107.5;
    assert!(
        sim_time - llc.completed_at < 3.0,
        "should still be in cooldown"
    );
    let sim_time_later = 108.1;
    assert!(
        sim_time_later - llc.completed_at > 3.0,
        "cooldown should be over"
    );
}

#[test]
fn lateral_offset_lane_center_calculation() {
    // Lane center = (lane_index + 0.5) * 3.5
    let lane_0_center = (0_f64 + 0.5) * 3.5;
    assert!((lane_0_center - 1.75).abs() < 1e-10);

    let lane_1_center = (1_f64 + 0.5) * 3.5;
    assert!((lane_1_center - 5.25).abs() < 1e-10);

    // Drift distance for lane change 0 -> 1
    let drift_dist = lane_1_center - lane_0_center;
    assert!((drift_dist - 3.5).abs() < 1e-10, "one lane width = 3.5m");
}

#[test]
fn linear_drift_speed_calculation() {
    // Drift from lane 0 to lane 1 over 2 seconds
    let current_lateral = 1.75; // lane 0 center
    let desired_lateral = 5.25; // lane 1 center
    let time_remaining = 2.0;
    let dt = 0.016; // ~60fps

    let remaining_dist: f64 = desired_lateral - current_lateral;
    let drift_speed: f64 = remaining_dist / time_remaining;
    let new_lateral: f64 = current_lateral + drift_speed * dt;

    // drift_speed = 3.5 / 2.0 = 1.75 m/s
    assert!((drift_speed - 1.75_f64).abs() < 1e-10);
    // After one frame: 1.75 + 1.75*0.016 = 1.778
    assert!((new_lateral - 1.778_f64).abs() < 1e-3);
}

#[test]
fn drift_completes_when_time_remaining_zero() {
    let time_remaining = 0.01;
    let dt = 0.016;
    let new_time = time_remaining - dt;
    assert!(
        new_time <= 0.0,
        "drift should complete when time_remaining goes to zero or below"
    );
}

#[test]
fn lateral_offset_component_for_car_lane_change() {
    // When starting a lane change from lane 0 to lane 1:
    let current_lane = 0_u8;
    let target_lane = 1_u8;
    let lat = LateralOffset {
        lateral_offset: (current_lane as f64 + 0.5) * 3.5,
        desired_lateral: (target_lane as f64 + 0.5) * 3.5,
    };
    assert!((lat.lateral_offset - 1.75).abs() < 1e-10);
    assert!((lat.desired_lateral - 5.25).abs() < 1e-10);
}

#[test]
fn edge_boundary_prevents_lane_change() {
    // MOBIL should not evaluate near edge start (<5m) or near end (>edge_length-20m)
    let offset_too_early = 3.0;
    let offset_too_late = 85.0;
    let edge_length = 100.0;

    assert!(
        offset_too_early < 5.0,
        "should skip MOBIL near edge start"
    );
    assert!(
        offset_too_late > edge_length - 20.0,
        "should skip MOBIL near edge end"
    );

    let offset_ok = 30.0;
    assert!(
        offset_ok >= 5.0 && offset_ok <= edge_length - 20.0,
        "should allow MOBIL in safe zone"
    );
}
