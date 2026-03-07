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

// ===========================================================================
// GPU behavioral differentiation tests (require gpu-tests feature + GPU)
// ===========================================================================

#[cfg(feature = "gpu-tests")]
mod gpu_behavior {
    use velos_core::components::{CarFollowingModel, GpuAgentState};
    use velos_core::fixed_point::{FixPos, FixSpd};
    use velos_gpu::compute::{sort_agents_by_lane, ComputeDispatcher};
    use velos_gpu::device::GpuContext;

    /// Run N GPU wave-front dispatch steps for a set of agents.
    /// Returns the updated agent states after all steps.
    fn run_gpu_steps(
        agents: &[GpuAgentState],
        steps: u32,
        dt: f32,
    ) -> Option<Vec<GpuAgentState>> {
        let ctx = GpuContext::new_headless()?;
        let mut dispatcher = ComputeDispatcher::new(&ctx.device);

        let mut current_agents = agents.to_vec();

        for _ in 0..steps {
            let (offsets, counts, indices) = sort_agents_by_lane(&current_agents);
            dispatcher.upload_wave_front_data(
                &ctx.device,
                &ctx.queue,
                &current_agents,
                &offsets,
                &counts,
                &indices,
            );

            let mut encoder = ctx.device.create_command_encoder(&Default::default());
            dispatcher.dispatch_wave_front(&mut encoder, &ctx.device, &ctx.queue, dt);
            ctx.queue.submit(std::iter::once(encoder.finish()));

            current_agents = dispatcher.readback_wave_front_agents(&ctx.device, &ctx.queue);
        }

        Some(current_agents)
    }

    // -----------------------------------------------------------------------
    // Test 5: After 50 steps, Krauss agents have lower avg speed than IDM
    // -----------------------------------------------------------------------

    #[test]
    fn krauss_agents_have_lower_avg_speed_than_idm() {
        // Create 20 agents on the same lane: 10 IDM, 10 Krauss.
        // Spread out with generous gaps so they can accelerate freely.
        let mut agents = Vec::new();
        for i in 0..20u32 {
            let cf_model = if i < 10 {
                CarFollowingModel::Idm as u32
            } else {
                CarFollowingModel::Krauss as u32
            };
            agents.push(GpuAgentState {
                edge_id: 0,
                lane_idx: 0,
                // Leaders at highest positions, 50m apart so gaps are generous.
                position: FixPos::from_f64((19 - i) as f64 * 50.0 + 10.0).raw(),
                lateral: 0,
                speed: FixSpd::from_f64(8.0).raw(),
                acceleration: 0,
                cf_model,
                rng_state: i * 7 + 42, // Distinct RNG seeds.
            });
        }

        let result = run_gpu_steps(&agents, 50, 0.1);
        let Some(updated) = result else {
            eprintln!("SKIP: No GPU adapter available");
            return;
        };

        // Compute average speed for IDM vs Krauss agents.
        let mut idm_speed_sum = 0.0f64;
        let mut krauss_speed_sum = 0.0f64;
        let mut idm_count = 0u32;
        let mut krauss_count = 0u32;

        for (i, agent) in updated.iter().enumerate() {
            let speed = FixSpd::from_raw(agent.speed).to_f64();
            if agents[i].cf_model == CarFollowingModel::Idm as u32 {
                idm_speed_sum += speed;
                idm_count += 1;
            } else {
                krauss_speed_sum += speed;
                krauss_count += 1;
            }
        }

        let idm_avg = idm_speed_sum / idm_count as f64;
        let krauss_avg = krauss_speed_sum / krauss_count as f64;

        eprintln!("IDM avg speed: {idm_avg:.4} m/s ({idm_count} agents)");
        eprintln!("Krauss avg speed: {krauss_avg:.4} m/s ({krauss_count} agents)");

        // Krauss dawdle (sigma=0.5) should make average speed lower than IDM.
        // Require at least 5% difference to confirm shader branching works.
        let diff_pct = (idm_avg - krauss_avg) / idm_avg * 100.0;
        eprintln!("Speed difference: {diff_pct:.1}% (IDM faster)");

        assert!(
            krauss_avg < idm_avg,
            "Krauss avg ({krauss_avg:.4}) should be lower than IDM avg ({idm_avg:.4})"
        );
        assert!(
            diff_pct >= 5.0,
            "Speed difference {diff_pct:.1}% should be >= 5% to confirm shader branching"
        );
    }

    // -----------------------------------------------------------------------
    // Test 6: Two identical agents with different cf_model produce different
    //         speeds after 10 steps.
    // -----------------------------------------------------------------------

    #[test]
    fn identical_agents_different_cf_model_diverge() {
        // Two agents: same position, speed, gap -- only cf_model differs.
        // Agent 0 = IDM leader (far ahead, no interaction).
        // Agent 1 = IDM follower.
        // Agent 2 = Krauss follower at same position as agent 1.
        let agents = vec![
            // Leader (IDM, far ahead).
            GpuAgentState {
                edge_id: 0,
                lane_idx: 0,
                position: FixPos::from_f64(500.0).raw(),
                lateral: 0,
                speed: FixSpd::from_f64(10.0).raw(),
                acceleration: 0,
                cf_model: CarFollowingModel::Idm as u32,
                rng_state: 100,
            },
            // Follower A (IDM).
            GpuAgentState {
                edge_id: 0,
                lane_idx: 0,
                position: FixPos::from_f64(400.0).raw(),
                lateral: 0,
                speed: FixSpd::from_f64(10.0).raw(),
                acceleration: 0,
                cf_model: CarFollowingModel::Idm as u32,
                rng_state: 200,
            },
            // Follower B (Krauss) -- on a different lane to isolate from IDM follower.
            GpuAgentState {
                edge_id: 0,
                lane_idx: 1,
                position: FixPos::from_f64(400.0).raw(),
                lateral: 0,
                speed: FixSpd::from_f64(10.0).raw(),
                acceleration: 0,
                cf_model: CarFollowingModel::Krauss as u32,
                rng_state: 200,
            },
        ];

        let result = run_gpu_steps(&agents, 10, 0.1);
        let Some(updated) = result else {
            eprintln!("SKIP: No GPU adapter available");
            return;
        };

        let idm_follower_speed = FixSpd::from_raw(updated[1].speed).to_f64();
        let krauss_follower_speed = FixSpd::from_raw(updated[2].speed).to_f64();

        eprintln!("IDM follower speed after 10 steps: {idm_follower_speed:.4} m/s");
        eprintln!("Krauss follower speed after 10 steps: {krauss_follower_speed:.4} m/s");

        // The two followers should have different speeds due to different models.
        let speed_diff = (idm_follower_speed - krauss_follower_speed).abs();
        eprintln!("Speed difference: {speed_diff:.4} m/s");

        assert!(
            speed_diff > 0.01,
            "IDM ({idm_follower_speed:.4}) and Krauss ({krauss_follower_speed:.4}) \
             should produce different speeds (diff={speed_diff:.6})"
        );
    }
}
