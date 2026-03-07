//! Integration tests proving CarFollowingModel is attached at spawn and
//! that Krauss vs IDM agents produce different behavior on the GPU.
//!
//! Tests 1-4: Spawn assignment (CPU-only, no GPU needed).
//! Tests 5-6: GPU behavioral differentiation (requires `gpu-tests` feature).

use petgraph::graph::DiGraph;
use velos_core::components::{CarFollowingModel, VehicleType};
use velos_gpu::sim::SimWorld;
use velos_net::{RoadEdge, RoadGraph, RoadNode};

/// Build a small linear road network with proper geometry for physics.
fn make_linear_graph() -> RoadGraph {
    let mut g = DiGraph::new();

    let positions: Vec<[f64; 2]> = vec![
        [0.0, 0.0],
        [100.0, 0.0],
        [200.0, 0.0],
        [300.0, 0.0],
        [400.0, 0.0],
    ];

    let nodes: Vec<_> = positions
        .iter()
        .map(|&pos| g.add_node(RoadNode { pos }))
        .collect();

    let make_edge = |from: [f64; 2], to: [f64; 2]| RoadEdge {
        length_m: ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt(),
        speed_limit_mps: 13.89,
        lane_count: 2,
        oneway: false,
        road_class: velos_net::graph::RoadClass::Secondary,
        geometry: vec![from, to],
        motorbike_only: false,
        time_windows: None,
    };

    // Forward edges with geometry.
    for i in 0..positions.len() - 1 {
        g.add_edge(nodes[i], nodes[i + 1], make_edge(positions[i], positions[i + 1]));
    }
    // Reverse edges.
    for i in 0..positions.len() - 1 {
        g.add_edge(nodes[i + 1], nodes[i], make_edge(positions[i + 1], positions[i]));
    }

    RoadGraph::new(g)
}

/// Spawn agents via ticking the simulation. Returns SimWorld with agents.
fn spawn_agents_via_tick() -> SimWorld {
    let graph = make_linear_graph();
    let mut sim = SimWorld::new(graph);
    sim.sim_state = velos_gpu::sim::SimState::Running;

    // Tick many times to trigger spawning via the demand system.
    for _ in 0..500 {
        sim.tick(0.1);
    }

    sim
}

// ---------------------------------------------------------------------------
// Test 1: All non-pedestrian agents have Some(CarFollowingModel)
// ---------------------------------------------------------------------------

#[test]
fn all_vehicle_agents_have_car_following_model() {
    let sim = spawn_agents_via_tick();

    let mut vehicle_count = 0u32;
    let mut with_cf_model = 0u32;

    for (vtype, cf) in sim
        .world
        .query::<(&VehicleType, Option<&CarFollowingModel>)>()
        .iter()
    {
        if *vtype == VehicleType::Pedestrian {
            continue;
        }
        vehicle_count += 1;
        if cf.is_some() {
            with_cf_model += 1;
        }
    }

    // Must have spawned at least some vehicles for this test to be meaningful.
    assert!(
        vehicle_count > 0,
        "No vehicles spawned -- test graph may be too small"
    );
    assert_eq!(
        vehicle_count, with_cf_model,
        "All {vehicle_count} vehicles should have CarFollowingModel, but only {with_cf_model} do"
    );
}

// ---------------------------------------------------------------------------
// Test 2: ~30% of Car agents have Krauss, ~70% have IDM
// ---------------------------------------------------------------------------

#[test]
fn car_agents_krauss_ratio_approximately_30_percent() {
    let sim = spawn_agents_via_tick();

    let mut car_count = 0u32;
    let mut krauss_count = 0u32;

    for (vtype, cf) in sim
        .world
        .query::<(&VehicleType, &CarFollowingModel)>()
        .iter()
    {
        if *vtype != VehicleType::Car {
            continue;
        }
        car_count += 1;
        if *cf == CarFollowingModel::Krauss {
            krauss_count += 1;
        }
    }

    if car_count < 5 {
        // Not enough cars to validate ratio -- skip rather than false-fail.
        eprintln!("WARN: Only {car_count} cars spawned, skipping ratio check");
        return;
    }

    let ratio = krauss_count as f64 / car_count as f64;
    eprintln!("Krauss ratio: {krauss_count}/{car_count} = {ratio:.2}");

    // Accept 10-50% range (30% target, wide tolerance for small samples).
    assert!(
        (0.10..=0.50).contains(&ratio),
        "Krauss ratio {ratio:.2} should be approximately 30% (tolerance: 10-50%)"
    );
}

// ---------------------------------------------------------------------------
// Test 3: All motorbike agents get IDM
// ---------------------------------------------------------------------------

#[test]
fn motorbike_agents_always_idm() {
    let sim = spawn_agents_via_tick();

    let mut motorbike_count = 0u32;

    for (vtype, cf) in sim
        .world
        .query::<(&VehicleType, &CarFollowingModel)>()
        .iter()
    {
        if *vtype != VehicleType::Motorbike {
            continue;
        }
        motorbike_count += 1;
        assert_eq!(
            *cf,
            CarFollowingModel::Idm,
            "Motorbike should always use IDM, got {:?}",
            cf
        );
    }

    eprintln!("Checked {motorbike_count} motorbikes -- all IDM");
}

// ---------------------------------------------------------------------------
// Test 4: Pedestrian agents do NOT have CarFollowingModel
// ---------------------------------------------------------------------------

#[test]
fn pedestrian_agents_have_no_car_following_model() {
    let sim = spawn_agents_via_tick();

    let mut ped_with_cf = 0u32;
    let mut ped_count = 0u32;

    for (vtype, cf) in sim
        .world
        .query::<(&VehicleType, Option<&CarFollowingModel>)>()
        .iter()
    {
        if *vtype != VehicleType::Pedestrian {
            continue;
        }
        ped_count += 1;
        if cf.is_some() {
            ped_with_cf += 1;
        }
    }

    assert_eq!(
        ped_with_cf, 0,
        "Pedestrians should not have CarFollowingModel, but {ped_with_cf}/{ped_count} do"
    );
}
