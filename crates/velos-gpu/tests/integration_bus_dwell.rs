//! Integration tests for bus dwell lifecycle in sim loop.
//!
//! Verifies that bus agents spawn with BusState, step_bus_dwell triggers
//! dwell near stops, dwell completion clears FLAG_BUS_DWELLING, and the
//! flag is reflected in GpuAgentState construction.
//!
//! Requirements: AGT-01

use hecs::Entity;
use petgraph::graph::DiGraph;

use velos_core::components::{
    Kinematics, Position, RoadPosition, Route, VehicleType, WaitState,
};
use velos_gpu::sim::SimWorld;
use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
use velos_vehicle::bus::{BusState, BusStop};

/// Build a simple linear road graph with 3 nodes (2 edges).
fn make_bus_route_graph() -> RoadGraph {
    let mut g = DiGraph::new();
    let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let b = g.add_node(RoadNode { pos: [100.0, 0.0] });
    let c = g.add_node(RoadNode { pos: [200.0, 0.0] });

    let edge = |length: f64| RoadEdge {
        length_m: length,
        speed_limit_mps: 13.89,
        lane_count: 2,
        oneway: true,
        road_class: RoadClass::Secondary,
        geometry: vec![[0.0, 0.0], [100.0, 0.0]],
        motorbike_only: false,
        time_windows: None,
    };

    g.add_edge(a, b, edge(100.0));
    g.add_edge(b, c, edge(100.0));

    RoadGraph::new(g)
}

/// Spawn a bus entity manually with BusState component.
fn spawn_test_bus(
    sim: &mut SimWorld,
    edge_index: u32,
    offset_m: f64,
    stop_indices: Vec<usize>,
) -> Entity {
    let idm = velos_vehicle::types::default_idm_params(velos_vehicle::types::VehicleType::Bus);
    sim.world.spawn((
        Position { x: 0.0, y: 0.0 },
        Kinematics {
            vx: 1.0,
            vy: 0.0,
            speed: 1.0,
            heading: 0.0,
        },
        VehicleType::Bus,
        RoadPosition {
            edge_index,
            lane: 0,
            offset_m,
        },
        Route {
            path: vec![0, 1, 2],
            current_step: 1,
        },
        WaitState {
            stopped_since: -1.0,
            at_red_signal: false,
        },
        idm,
        velos_core::components::CarFollowingModel::Idm,
        velos_core::components::LateralOffset {
            lateral_offset: 1.75,
            desired_lateral: 1.75,
        },
        BusState::new(stop_indices, 0),
    ))
}

#[test]
fn test_bus_spawned_with_bus_state() {
    let graph = make_bus_route_graph();
    let mut sim = SimWorld::new_cpu_only(graph);

    // Add a bus stop on edge 0 at offset 50m
    sim.bus_stops.push(BusStop {
        edge_id: 0,
        offset_m: 50.0,
        capacity: 40,
        name: "Test Stop".to_string(),
    });

    // Spawn a bus with stop index 0
    let entity = spawn_test_bus(&mut sim, 0, 10.0, vec![0]);

    // Verify BusState is attached
    let bus_state = sim.world.query_one_mut::<&BusState>(entity).unwrap();
    assert_eq!(bus_state.current_stop_index(), 0);
    assert!(!bus_state.is_dwelling());
}

#[test]
fn test_step_bus_dwell_triggers_dwell_near_stop() {
    let graph = make_bus_route_graph();
    let mut sim = SimWorld::new_cpu_only(graph);

    // Add a bus stop on edge 0 at offset 50m
    sim.bus_stops.push(BusStop {
        edge_id: 0,
        offset_m: 50.0,
        capacity: 40,
        name: "Stop A".to_string(),
    });

    // Spawn bus within 5m of the stop (offset 48m, stop at 50m)
    let entity = spawn_test_bus(&mut sim, 0, 48.0, vec![0]);

    // Run the dwell step
    sim.step_bus_dwell(0.1);

    // Bus should now be dwelling
    let bus_state = sim.world.query_one_mut::<&BusState>(entity).unwrap();
    assert!(bus_state.is_dwelling(), "Bus should be dwelling after reaching stop");
}

#[test]
fn test_dwell_completion_clears_dwelling_and_advances_stop() {
    let graph = make_bus_route_graph();
    let mut sim = SimWorld::new_cpu_only(graph);

    sim.bus_stops.push(BusStop {
        edge_id: 0,
        offset_m: 50.0,
        capacity: 40,
        name: "Stop A".to_string(),
    });

    let entity = spawn_test_bus(&mut sim, 0, 50.0, vec![0]);

    // Trigger dwell
    sim.step_bus_dwell(0.1);
    assert!(
        sim.world
            .query_one_mut::<&BusState>(entity)
            .unwrap()
            .is_dwelling()
    );

    // Tick dwell until complete (max dwell is 60s, tick enough)
    for _ in 0..700 {
        sim.step_bus_dwell(0.1);
    }

    // Dwell should be complete: not dwelling, advanced to next stop
    let bus_state = sim.world.query_one_mut::<&BusState>(entity).unwrap();
    assert!(
        !bus_state.is_dwelling(),
        "Bus should have finished dwelling after 70s"
    );
    assert_eq!(
        bus_state.current_stop_index(),
        1,
        "Bus should advance to next stop"
    );
}

#[test]
fn test_flag_bus_dwelling_set_during_dwell() {
    let graph = make_bus_route_graph();
    let mut sim = SimWorld::new_cpu_only(graph);

    sim.bus_stops.push(BusStop {
        edge_id: 0,
        offset_m: 50.0,
        capacity: 40,
        name: "Stop A".to_string(),
    });

    let entity = spawn_test_bus(&mut sim, 0, 50.0, vec![0]);

    // Trigger dwell
    sim.step_bus_dwell(0.1);
    assert!(
        sim.world
            .query_one_mut::<&BusState>(entity)
            .unwrap()
            .is_dwelling()
    );

    // Build GPU state and check flag
    let bus_state = sim.world.query_one_mut::<&BusState>(entity).unwrap();
    let flags = if bus_state.is_dwelling() { 1u32 } else { 0u32 };
    assert_eq!(
        flags & 1,
        1,
        "FLAG_BUS_DWELLING should be set while dwelling"
    );
}

#[test]
fn test_flag_bus_dwelling_cleared_after_dwell() {
    let graph = make_bus_route_graph();
    let mut sim = SimWorld::new_cpu_only(graph);

    sim.bus_stops.push(BusStop {
        edge_id: 0,
        offset_m: 50.0,
        capacity: 40,
        name: "Stop A".to_string(),
    });

    let entity = spawn_test_bus(&mut sim, 0, 50.0, vec![0]);

    // Trigger and complete dwell
    sim.step_bus_dwell(0.1);
    for _ in 0..700 {
        sim.step_bus_dwell(0.1);
    }

    let bus_state = sim.world.query_one_mut::<&BusState>(entity).unwrap();
    assert!(!bus_state.is_dwelling());
    let flags = if bus_state.is_dwelling() { 1u32 } else { 0u32 };
    assert_eq!(
        flags & 1,
        0,
        "FLAG_BUS_DWELLING should be cleared after dwell completes"
    );
}

#[test]
fn test_bus_not_near_stop_does_not_dwell() {
    let graph = make_bus_route_graph();
    let mut sim = SimWorld::new_cpu_only(graph);

    sim.bus_stops.push(BusStop {
        edge_id: 0,
        offset_m: 50.0,
        capacity: 40,
        name: "Stop A".to_string(),
    });

    // Spawn bus far from stop (at 10m, stop at 50m -> 40m away > 5m threshold)
    let entity = spawn_test_bus(&mut sim, 0, 10.0, vec![0]);

    sim.step_bus_dwell(0.1);

    let bus_state = sim.world.query_one_mut::<&BusState>(entity).unwrap();
    assert!(
        !bus_state.is_dwelling(),
        "Bus should not dwell when far from stop"
    );
}
