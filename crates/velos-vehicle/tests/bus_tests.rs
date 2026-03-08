//! Tests for the bus dwell model and bus stop/state components.

use velos_vehicle::bus::{BusDwellModel, BusState, BusStop};

// --- BusDwellModel tests ---

#[test]
fn dwell_with_boarding_and_alighting() {
    let model = BusDwellModel::default();
    // 10 boarding * 0.5 + 5 alighting * 0.67 + 5.0 fixed = 13.35
    let dwell = model.compute_dwell(10, 5);
    assert!(
        (dwell - 13.35).abs() < 1e-9,
        "expected 13.35, got {dwell}"
    );
}

#[test]
fn dwell_capped_at_max() {
    let model = BusDwellModel::default();
    // 100 * 0.5 + 100 * 0.67 + 5.0 = 122.0 -> capped at 60.0
    let dwell = model.compute_dwell(100, 100);
    assert!(
        (dwell - 60.0).abs() < 1e-9,
        "expected 60.0, got {dwell}"
    );
}

#[test]
fn dwell_zero_passengers() {
    let model = BusDwellModel::default();
    // 0 boarding, 0 alighting -> fixed 5.0s door open/close
    let dwell = model.compute_dwell(0, 0);
    assert!(
        (dwell - 5.0).abs() < 1e-9,
        "expected 5.0, got {dwell}"
    );
}

// --- BusStop tests ---

#[test]
fn bus_stop_fields() {
    let stop = BusStop {
        edge_id: 42,
        offset_m: 150.5,
        capacity: 30,
        name: "Ben Thanh".to_string(),
    };
    assert_eq!(stop.edge_id, 42);
    assert!((stop.offset_m - 150.5).abs() < 1e-9);
    assert_eq!(stop.capacity, 30);
    assert_eq!(stop.name, "Ben Thanh");
}

// --- BusState tests ---

#[test]
fn should_stop_within_threshold() {
    let stops = vec![
        BusStop {
            edge_id: 1,
            offset_m: 100.0,
            capacity: 20,
            name: "Stop A".to_string(),
        },
        BusStop {
            edge_id: 2,
            offset_m: 200.0,
            capacity: 20,
            name: "Stop B".to_string(),
        },
    ];
    let state = BusState::new(vec![0, 1], 0);
    // On same edge, within 5m
    assert!(state.should_stop(1, 97.0, &stops));
    assert!(state.should_stop(1, 103.0, &stops));
}

#[test]
fn should_stop_outside_threshold() {
    let stops = vec![BusStop {
        edge_id: 1,
        offset_m: 100.0,
        capacity: 20,
        name: "Stop A".to_string(),
    }];
    let state = BusState::new(vec![0], 0);
    // Outside 5m
    assert!(!state.should_stop(1, 90.0, &stops));
    // Wrong edge
    assert!(!state.should_stop(2, 100.0, &stops));
}

#[test]
fn should_stop_returns_false_when_all_stops_visited() {
    let stops = vec![BusStop {
        edge_id: 1,
        offset_m: 100.0,
        capacity: 20,
        name: "Stop A".to_string(),
    }];
    let mut state = BusState::new(vec![0], 0);
    let model = BusDwellModel::default();
    state.begin_dwell(&model, 5, 3);
    // Tick until done
    state.tick_dwell(60.0);
    // Now all stops visited, should_stop returns false
    assert!(!state.should_stop(1, 100.0, &stops));
}

#[test]
fn begin_dwell_sets_state() {
    let model = BusDwellModel::default();
    let mut state = BusState::new(vec![0], 0);
    state.begin_dwell(&model, 10, 5);
    assert!(state.is_dwelling());
    assert!((state.dwell_remaining() - 13.35).abs() < 1e-9);
}

#[test]
fn tick_dwell_decrements_and_completes() {
    let model = BusDwellModel::default();
    let mut state = BusState::new(vec![0], 0);
    state.begin_dwell(&model, 0, 0); // 5.0s dwell

    // Tick 3s -- not done yet
    assert!(!state.tick_dwell(3.0));
    assert!(state.is_dwelling());
    assert!((state.dwell_remaining() - 2.0).abs() < 1e-9);

    // Tick 3s more -- done (remaining was 2.0)
    assert!(state.tick_dwell(3.0));
    assert!(!state.is_dwelling());
}

#[test]
fn tick_dwell_advances_stop_index() {
    let model = BusDwellModel::default();
    let stops = vec![
        BusStop {
            edge_id: 1,
            offset_m: 100.0,
            capacity: 20,
            name: "Stop A".to_string(),
        },
        BusStop {
            edge_id: 2,
            offset_m: 200.0,
            capacity: 20,
            name: "Stop B".to_string(),
        },
    ];
    let mut state = BusState::new(vec![0, 1], 0);

    // Dwell at first stop
    state.begin_dwell(&model, 0, 0);
    state.tick_dwell(10.0); // completes dwell

    // Now should target second stop
    assert!(state.should_stop(2, 200.0, &stops));
    assert!(!state.should_stop(1, 100.0, &stops)); // first stop already visited
}

#[test]
fn route_index_stored_and_accessible() {
    let state = BusState::new(vec![0, 1], 5);
    assert_eq!(state.route_index(), 5);
}

#[test]
fn route_index_zero_default() {
    let state = BusState::new(vec![0], 0);
    assert_eq!(state.route_index(), 0);
}

#[test]
fn route_index_max_value() {
    let state = BusState::new(vec![], 255);
    assert_eq!(state.route_index(), 255);
}
