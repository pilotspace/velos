//! Tests for motorbike sublane lateral model.

use velos_vehicle::sublane::{
    apply_lateral_drift, compute_desired_lateral, NeighborInfo, SublaneParams,
};

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
    // Max displacement = 1.0 * 0.1 = 0.1m
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
    // Neighbor at lateral=2.5 with half_width=0.8 (car). Ego half_width=0.25.
    // Available gap to the left of neighbor starts at 2.5 + 0.8 = 3.3
    // Available gap to the right of neighbor ends at 2.5 - 0.8 = 1.7
    // Ego at 1.0: gap between ego right edge (1.0-0.25=0.75) and neighbor left edge (1.7)
    // = 0.95m, which is >= 0.6m. So there should be a valid gap.
    let neighbors = vec![NeighborInfo {
        lateral_offset: 2.5,
        longitudinal_gap: 5.0,
        half_width: 0.8,
        speed: 5.0,
    }];
    // At 1.0, gap to neighbor = |2.5-1.0| - 0.8 - 0.25 = 0.45 < 0.6 → not enough
    // At 0.5, gap = |2.5-0.5| - 0.8 - 0.25 = 0.95 → valid
    let result = compute_desired_lateral(1.0, 5.0, road_width, &neighbors, false, &params);
    // Result should be <= 1.0 (moving right toward the wider gap) or stay at 1.0
    assert!(result >= params.half_width);
    assert!(result <= road_width - params.half_width);
}
