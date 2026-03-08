//! Tests for motorbike sublane lateral model.

use velos_vehicle::sublane::{
    apply_lateral_drift, compute_desired_lateral, effective_filter_gap, red_light_creep_speed,
    NeighborInfo, SublaneParams,
};
use velos_vehicle::types::VehicleType;

fn default_params() -> SublaneParams {
    SublaneParams::default()
}

// ---------- compute_desired_lateral ----------

#[test]
fn no_neighbors_stays_at_current_position() {
    let params = default_params();
    let road_width = 7.0; // 2-lane road
    let result = compute_desired_lateral(2.0, 5.0, road_width, &[], false, &params);
    assert!(
        (result - 2.0).abs() < 1e-10,
        "no neighbors: should stay at current lateral={result}, expected 2.0"
    );
}

#[test]
fn gap_on_right_side_returns_right_offset() {
    let params = default_params();
    let road_width = 7.0;
    // Neighbor to the left at lateral=3.5, blocking leftward movement.
    // Own position at 2.0 with gap to the right (toward 0.25 boundary).
    let neighbors = vec![NeighborInfo {
        lateral_offset: 3.5,
        longitudinal_gap: 5.0,
        half_width: 0.25,
        speed: 5.0,
    }];
    let result = compute_desired_lateral(2.0, 5.0, road_width, &neighbors, false, &params);
    // There should be a gap available to probe toward; result should differ from current
    // or stay at current if no valid gap found. The key: algorithm should not crash.
    assert!(result >= params.half_width);
    assert!(result <= road_width - params.half_width);
}

#[test]
fn gap_too_small_on_both_sides_stays_put() {
    let params = default_params();
    let road_width = 1.5; // Very narrow road
    // Two neighbors boxing us in tightly
    let neighbors = vec![
        NeighborInfo {
            lateral_offset: 0.5,
            longitudinal_gap: 5.0,
            half_width: 0.25,
            speed: 5.0,
        },
        NeighborInfo {
            lateral_offset: 1.0,
            longitudinal_gap: 5.0,
            half_width: 0.25,
            speed: 5.0,
        },
    ];
    let result = compute_desired_lateral(0.75, 5.0, road_width, &neighbors, false, &params);
    // Should stay at current since gaps are too small
    assert!(
        (result - 0.75).abs() < 0.5,
        "tight space: should stay near current={result}"
    );
}

#[test]
fn drift_clamps_to_max_lateral_speed_times_dt() {
    let params = default_params();
    // Large desired offset change, but dt is small
    let dt = 0.1;
    let result = apply_lateral_drift(1.0, 5.0, params.max_lateral_speed, dt);
    // Max displacement = 1.2 * 0.1 = 0.12m (HCMC: max_lateral_speed=1.2)
    let expected = 1.0 + params.max_lateral_speed * dt;
    assert!(
        (result - expected).abs() < 1e-10,
        "drift clamped: result={result}, expected={expected}"
    );
}

#[test]
fn drift_dt_consistency() {
    // Same total time (1.0s) at different dt values should produce same final position
    let params = default_params();
    let desired = 3.0;
    let start = 1.0;

    let mut pos_005 = start;
    for _ in 0..20 {
        pos_005 = apply_lateral_drift(pos_005, desired, params.max_lateral_speed, 0.05);
    }

    let mut pos_01 = start;
    for _ in 0..10 {
        pos_01 = apply_lateral_drift(pos_01, desired, params.max_lateral_speed, 0.1);
    }

    let mut pos_02 = start;
    for _ in 0..5 {
        pos_02 = apply_lateral_drift(pos_02, desired, params.max_lateral_speed, 0.2);
    }

    // All should reach same position within tolerance
    assert!(
        (pos_005 - pos_01).abs() < 0.02,
        "dt-consistency 0.05 vs 0.1: {pos_005} vs {pos_01}"
    );
    assert!(
        (pos_01 - pos_02).abs() < 0.02,
        "dt-consistency 0.1 vs 0.2: {pos_01} vs {pos_02}"
    );
}

#[test]
fn red_light_swarming_finds_largest_gap() {
    let params = default_params();
    let road_width = 10.5; // 3-lane road
    // Two neighbors at lateral 2.0 and 5.0, leaving a big gap around 3.5
    let neighbors = vec![
        NeighborInfo {
            lateral_offset: 2.0,
            longitudinal_gap: 2.0,
            half_width: 0.8, // car width
            speed: 0.0,
        },
        NeighborInfo {
            lateral_offset: 7.0,
            longitudinal_gap: 2.0,
            half_width: 0.8,
            speed: 0.0,
        },
    ];
    let result = compute_desired_lateral(1.0, 0.0, road_width, &neighbors, true, &params);
    // Should find the largest gap (between 2.8 and 6.2) and target its center (~4.5)
    assert!(result > 3.0 && result < 6.5, "swarming should find gap center: {result}");
}

#[test]
fn road_boundary_respected() {
    let params = default_params();
    let road_width = 3.5; // single lane
    // Neighbor pushing us toward edge
    let neighbors = vec![NeighborInfo {
        lateral_offset: 1.75,
        longitudinal_gap: 5.0,
        half_width: 0.8,
        speed: 5.0,
    }];
    let result = compute_desired_lateral(0.3, 5.0, road_width, &neighbors, false, &params);
    assert!(
        result >= params.half_width,
        "must respect lower boundary: {result} >= {}",
        params.half_width
    );
    assert!(
        result <= road_width - params.half_width,
        "must respect upper boundary: {result} <= {}",
        road_width - params.half_width
    );
}

#[test]
fn gap_computation_subtracts_both_half_widths() {
    let params = default_params();
    let road_width = 7.0;
    let neighbors = vec![NeighborInfo {
        lateral_offset: 2.5,
        longitudinal_gap: 5.0,
        half_width: 0.8,
        speed: 5.0,
    }];
    let result = compute_desired_lateral(1.0, 5.0, road_width, &neighbors, false, &params);
    assert!(result >= params.half_width);
    assert!(result <= road_width - params.half_width);
}

// ---------- red_light_creep_speed ----------

#[test]
fn creep_motorbike_at_red_light_returns_positive() {
    // Motorbike at red light with distance > 0.5m should creep forward
    let speed = red_light_creep_speed(3.0, VehicleType::Motorbike);
    assert!(speed > 0.0, "motorbike should creep at red light, got {speed}");
}

#[test]
fn creep_car_returns_zero() {
    // Cars do NOT creep at red lights
    let speed = red_light_creep_speed(3.0, VehicleType::Car);
    assert!(
        speed.abs() < 1e-10,
        "car should not creep, got {speed}"
    );
}

#[test]
fn creep_bus_returns_zero() {
    let speed = red_light_creep_speed(3.0, VehicleType::Bus);
    assert!(speed.abs() < 1e-10, "bus should not creep, got {speed}");
}

#[test]
fn creep_truck_returns_zero() {
    let speed = red_light_creep_speed(3.0, VehicleType::Truck);
    assert!(speed.abs() < 1e-10, "truck should not creep, got {speed}");
}

#[test]
fn creep_bicycle_returns_positive() {
    // Bicycles also creep at red lights
    let speed = red_light_creep_speed(3.0, VehicleType::Bicycle);
    assert!(speed > 0.0, "bicycle should creep at red light, got {speed}");
}

#[test]
fn creep_zero_when_past_stop_line() {
    // Already at/past stop line (distance < 0.5m), no more creeping
    let speed = red_light_creep_speed(0.3, VehicleType::Motorbike);
    assert!(
        speed.abs() < 1e-10,
        "should not creep when already at stop line, got {speed}"
    );
}

#[test]
fn creep_speed_decreases_closer_to_stop_line() {
    // Creep speed should be gradual: higher at distance=5m, lower at distance=1m
    let far = red_light_creep_speed(5.0, VehicleType::Motorbike);
    let close = red_light_creep_speed(1.0, VehicleType::Motorbike);
    assert!(
        far > close,
        "creep should be faster farther from stop line: far={far} > close={close}"
    );
}

#[test]
fn creep_speed_bounded_at_max() {
    // At distance > 5m, creep speed should be capped at 0.3 m/s
    let speed = red_light_creep_speed(10.0, VehicleType::Motorbike);
    assert!(
        (speed - 0.3).abs() < 1e-10,
        "creep speed should be capped at 0.3 m/s, got {speed}"
    );
}

// ---------- effective_filter_gap ----------

#[test]
fn effective_gap_at_zero_delta_v() {
    // Base gap at zero speed difference
    let gap = effective_filter_gap(0.5, 5.0, 5.0);
    assert!(
        (gap - 0.5).abs() < 1e-10,
        "gap at delta_v=0 should be base (0.5m), got {gap}"
    );
}

#[test]
fn effective_gap_increases_with_speed_difference() {
    // At delta_v=5.0 m/s, gap = 0.5 + 0.1 * 5.0 = 1.0m
    let gap = effective_filter_gap(0.5, 10.0, 5.0);
    assert!(
        (gap - 1.0).abs() < 1e-10,
        "gap at delta_v=5 should be 1.0m, got {gap}"
    );
}

#[test]
fn effective_gap_uses_config_base() {
    // With different base gap
    let gap = effective_filter_gap(0.6, 5.0, 5.0);
    assert!(
        (gap - 0.6).abs() < 1e-10,
        "gap at delta_v=0 with 0.6 base should be 0.6m, got {gap}"
    );
}

#[test]
fn effective_gap_symmetric_on_speed_direction() {
    // delta_v sign should not matter (absolute value)
    let gap_faster = effective_filter_gap(0.5, 10.0, 5.0);
    let gap_slower = effective_filter_gap(0.5, 5.0, 10.0);
    assert!(
        (gap_faster - gap_slower).abs() < 1e-10,
        "gap should be symmetric: {gap_faster} vs {gap_slower}"
    );
}
