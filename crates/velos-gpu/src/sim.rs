//! Simulation state and tick logic for wiring subsystems together.
//!
//! Extracted from app.rs to keep files under 700 lines.
//! Owns the ECS world, road graph, spawner, signal controllers,
//! gridlock detector, and all per-frame simulation stepping.
//!
//! Vehicle physics (car-following) runs on GPU via wave-front dispatch.
//! Pedestrian physics (social force) runs on CPU.
//! CPU car-following is kept in `cpu_reference` module for test validation only.

use std::collections::HashMap;

use hecs::{Entity, World};
use petgraph::graph::{EdgeIndex, NodeIndex};
use rand::rngs::StdRng;
use rand::SeedableRng;

use velos_core::components::{
    CarFollowingModel, GpuAgentState, Kinematics, LateralOffset, Position, RoadPosition, Route,
    VehicleType,
};
use velos_core::fixed_point::{FixLat, FixPos, FixSpd};
use velos_demand::{OdMatrix, Spawner, TodProfile, Zone};
use velos_net::{RoadGraph, SpatialIndex};
use velos_signal::detector::LoopDetector;
use velos_signal::SignalController;
use velos_vehicle::config::VehicleConfig;
use velos_vehicle::social_force::{self, PedestrianNeighbor, SocialForceParams};
use velos_vehicle::sublane::SublaneParams;

use crate::compute::{sort_agents_by_lane, ComputeDispatcher};
use crate::multi_gpu::MultiGpuScheduler;
use crate::partition::partition_network;
use crate::perception::PerceptionPipeline;
use crate::renderer::AgentInstance;
use crate::sim_perception::PerceptionBuffers;
use crate::sim_reroute::RerouteState;
use crate::sim_snapshot::AgentSnapshot;
use crate::sim_startup;

/// Partition mode for multi-GPU support.
pub enum PartitionMode {
    /// Single GPU: all agents dispatched via one ComputeDispatcher.
    Single,
    /// Multiple logical partitions (same physical GPU, separate buffers).
    Multi(MultiGpuScheduler),
}

/// Simulation run state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimState {
    Stopped,
    Running,
    Paused,
}

impl SimState {
    pub fn is_running(self) -> bool {
        self == SimState::Running
    }
}

/// Live simulation metrics.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimMetrics {
    pub frame_time_ms: f64,
    pub agent_count: u32,
    pub motorbike_count: u32,
    pub car_count: u32,
    pub ped_count: u32,
    pub sim_time: f64,
}

/// Zone centroid positions derived from road network bounding box.
fn zone_centroids_from_graph(graph: &RoadGraph) -> HashMap<Zone, [f64; 2]> {
    let g = graph.inner();
    let mut min_x = f64::MAX;
    let mut max_x = f64::MIN;
    let mut min_y = f64::MAX;
    let mut max_y = f64::MIN;
    for node in g.node_indices() {
        let p = g[node].pos;
        min_x = min_x.min(p[0]);
        max_x = max_x.max(p[0]);
        min_y = min_y.min(p[1]);
        max_y = max_y.max(p[1]);
    }
    let cx = (min_x + max_x) / 2.0;
    let cy = (min_y + max_y) / 2.0;
    let w = (max_x - min_x) * 0.3;
    let h = (max_y - min_y) * 0.3;

    let mut m = HashMap::new();
    m.insert(Zone::BenThanh, [cx, cy]);
    m.insert(Zone::NguyenHue, [cx + w, cy + h]);
    m.insert(Zone::Bitexco, [cx + w, cy - h]);
    m.insert(Zone::BuiVien, [cx - w, cy - h]);
    m.insert(Zone::Waterfront, [cx - w, cy + h]);
    m
}

/// Holds all simulation subsystems.
pub struct SimWorld {
    pub world: World,
    pub road_graph: RoadGraph,
    pub spawner: Spawner,
    pub signal_controllers: Vec<(NodeIndex, Box<dyn SignalController>)>,
    pub gridlock_timeout: f64,
    pub sim_time: f64,
    pub sim_state: SimState,
    pub speed_mult: f32,
    pub metrics: SimMetrics,
    pub(crate) rng: StdRng,
    pub(crate) signalized_nodes: HashMap<u32, Vec<EdgeIndex>>,
    pub(crate) zone_centroids: HashMap<Zone, [f64; 2]>,
    pub(crate) sublane_params: SublaneParams,
    pub(crate) social_force_params: SocialForceParams,
    /// Partition mode: Single (default) or Multi for logical multi-GPU partitions.
    pub partition_mode: PartitionMode,
    /// Reroute evaluation subsystem state.
    pub(crate) reroute: RerouteState,
    /// GPU perception pipeline. None in CPU-only test paths.
    pub(crate) perception: Option<PerceptionPipeline>,
    /// Loaded vehicle configuration.
    #[allow(dead_code)]
    pub(crate) vehicle_config: VehicleConfig,
    /// Loop detectors for actuated signal controllers.
    #[allow(dead_code)]
    pub(crate) loop_detectors: Vec<(NodeIndex, Vec<LoopDetector>)>,
    /// Pre-allocated GPU buffers for perception pipeline input.
    /// None in CPU-only test paths.
    pub(crate) perception_buffers: Option<PerceptionBuffers>,
}

impl SimWorld {
    const MORNING_RUSH_SECS: f64 = 7.0 * 3600.0;

    fn boosted_od() -> OdMatrix {
        let mut od = OdMatrix::district1_poc();
        let pairs: Vec<_> = od.zone_pairs().collect();
        for (from, to, count) in pairs {
            od.set_trips(from, to, count * 50);
        }
        od
    }

    /// Create a fully initialized SimWorld with GPU subsystems.
    ///
    /// Loads vehicle config, uploads GPU params, builds polymorphic signal
    /// controllers, creates PerceptionPipeline, uploads network signs.
    pub fn new(
        road_graph: RoadGraph,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dispatcher: &mut ComputeDispatcher,
    ) -> Self {
        let zone_centroids = zone_centroids_from_graph(&road_graph);
        let spawner = Spawner::new(Self::boosted_od(), TodProfile::hcmc_weekday(), 42);

        // Load vehicle config and upload to GPU.
        let vehicle_config = sim_startup::load_vehicle_config();
        sim_startup::upload_vehicle_params(&vehicle_config, dispatcher, queue);

        // Build signal controllers from TOML config.
        let signal_config = velos_signal::config::load_signal_config();
        let (signal_controllers, signalized_nodes) =
            sim_startup::build_signal_controllers(&road_graph, &signal_config);

        // Build loop detectors for actuated intersections.
        let loop_detectors =
            sim_startup::build_loop_detectors(&road_graph, &signal_config, &signalized_nodes);

        // Upload network signs to GPU.
        sim_startup::upload_network_signs(&road_graph, dispatcher, queue);

        // Create perception pipeline (300K max covers 280K target).
        let perception = PerceptionPipeline::new(device, 300_000);

        // Pre-allocate perception auxiliary buffers.
        let edge_count = road_graph.edge_count() as u32;
        let perception_buffers = PerceptionBuffers::new(device, edge_count);

        let mut sim = Self {
            world: World::new(),
            road_graph,
            spawner,
            signal_controllers,
            gridlock_timeout: 300.0,
            sim_time: Self::MORNING_RUSH_SECS,
            sim_state: SimState::Stopped,
            speed_mult: 2.0,
            metrics: SimMetrics::default(),
            rng: StdRng::seed_from_u64(123),
            signalized_nodes,
            zone_centroids,
            sublane_params: SublaneParams::default(),
            social_force_params: SocialForceParams::default(),
            partition_mode: PartitionMode::Single,
            reroute: RerouteState::new(),
            perception: Some(perception),
            vehicle_config,
            loop_detectors,
            perception_buffers: Some(perception_buffers),
        };

        // Initialize reroute subsystem (builds CCH, prediction service).
        sim.init_reroute();

        log::info!(
            "SimWorld initialized: {} signal controllers, perception pipeline ready",
            sim.signal_controllers.len()
        );

        sim
    }

    /// Create a SimWorld for CPU-only tests (no GPU device required).
    ///
    /// Signal controllers use fixed-time defaults. No GPU param upload,
    /// no PerceptionPipeline, no sign buffer upload.
    pub fn new_cpu_only(road_graph: RoadGraph) -> Self {
        let zone_centroids = zone_centroids_from_graph(&road_graph);
        let spawner = Spawner::new(Self::boosted_od(), TodProfile::hcmc_weekday(), 42);
        let vehicle_config = sim_startup::load_vehicle_config();

        let signal_config = velos_signal::config::load_signal_config();
        let (signal_controllers, signalized_nodes) =
            sim_startup::build_signal_controllers(&road_graph, &signal_config);
        let loop_detectors =
            sim_startup::build_loop_detectors(&road_graph, &signal_config, &signalized_nodes);

        Self {
            world: World::new(),
            road_graph,
            spawner,
            signal_controllers,
            gridlock_timeout: 300.0,
            sim_time: Self::MORNING_RUSH_SECS,
            sim_state: SimState::Stopped,
            speed_mult: 2.0,
            metrics: SimMetrics::default(),
            rng: StdRng::seed_from_u64(123),
            signalized_nodes,
            zone_centroids,
            sublane_params: SublaneParams::default(),
            social_force_params: SocialForceParams::default(),
            partition_mode: PartitionMode::Single,
            reroute: RerouteState::new(),
            perception: None,
            vehicle_config,
            loop_detectors,
            perception_buffers: None,
        }
    }

    /// Enable multi-GPU mode by partitioning the road graph into `k` logical partitions.
    pub fn enable_multi_gpu(&mut self, k: u32) {
        let assignment = partition_network(&self.road_graph, k);
        log::info!(
            "Multi-GPU enabled: {} partitions, {} boundary edges",
            assignment.partition_count,
            assignment.boundary_edges.len()
        );
        self.partition_mode = PartitionMode::Multi(MultiGpuScheduler::new(assignment));
    }

    pub fn reset(&mut self) {
        self.world.clear();
        self.sim_time = Self::MORNING_RUSH_SECS;
        self.sim_state = SimState::Stopped;
        self.metrics = SimMetrics::default();
        self.rng = StdRng::seed_from_u64(123);
        self.spawner = Spawner::new(Self::boosted_od(), TodProfile::hcmc_weekday(), 42);
        self.sublane_params = SublaneParams::default();
        self.social_force_params = SocialForceParams::default();
        self.partition_mode = PartitionMode::Single;
        self.reroute = RerouteState::new();
        // perception_buffers kept (GPU buffers are reusable)
        for (_, ctrl) in &mut self.signal_controllers {
            ctrl.reset();
        }
    }

    /// Run one simulation tick using GPU wave-front dispatch for vehicle physics.
    pub fn tick_gpu(
        &mut self,
        base_dt: f64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dispatcher: &mut ComputeDispatcher,
    ) -> (Vec<AgentInstance>, Vec<AgentInstance>, Vec<AgentInstance>) {
        if !self.sim_state.is_running() {
            return self.build_instances();
        }

        let dt = base_dt * self.speed_mult as f64;
        self.sim_time += dt;

        self.spawn_agents(dt);
        self.step_signals(dt);

        let snapshot = AgentSnapshot::collect(&self.world);
        let spatial = SpatialIndex::from_positions(&snapshot.ids, &snapshot.positions);

        self.step_vehicles_gpu(dt as f32, device, queue, dispatcher);
        self.step_pedestrians(dt, &spatial, &snapshot);

        self.detect_gridlock();
        self.remove_finished_agents();
        self.update_metrics();

        self.build_instances()
    }

    /// Run one simulation tick using CPU physics (fallback for tests without GPU).
    pub fn tick(
        &mut self,
        base_dt: f64,
    ) -> (Vec<AgentInstance>, Vec<AgentInstance>, Vec<AgentInstance>) {
        if !self.sim_state.is_running() {
            return self.build_instances();
        }

        let dt = base_dt * self.speed_mult as f64;
        self.sim_time += dt;

        self.spawn_agents(dt);
        self.step_signals(dt);

        let snapshot = AgentSnapshot::collect(&self.world);
        let spatial = SpatialIndex::from_positions(&snapshot.ids, &snapshot.positions);

        crate::cpu_reference::step_vehicles(self, dt, &spatial, &snapshot);
        crate::cpu_reference::step_motorbikes_sublane(self, dt, &spatial, &snapshot);
        self.step_pedestrians(dt, &spatial, &snapshot);

        self.detect_gridlock();
        self.remove_finished_agents();
        self.update_metrics();

        self.build_instances()
    }

    fn step_signals(&mut self, dt: f64) {
        for (_, ctrl) in &mut self.signal_controllers {
            ctrl.tick(dt, &[]);
        }
    }

    /// GPU wave-front dispatch for vehicle physics (cars + motorbikes).
    fn step_vehicles_gpu(
        &mut self,
        dt: f32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dispatcher: &mut ComputeDispatcher,
    ) {
        let mut gpu_agents: Vec<GpuAgentState> = Vec::new();
        let mut entity_map: Vec<Entity> = Vec::new();

        for (entity, rp, kin, vtype, lat, cf_model) in self
            .world
            .query_mut::<(
                Entity,
                &RoadPosition,
                &Kinematics,
                &VehicleType,
                Option<&LateralOffset>,
                Option<&CarFollowingModel>,
            )>()
            .into_iter()
        {
            if *vtype == VehicleType::Pedestrian {
                continue;
            }

            let cf = cf_model.copied().unwrap_or(CarFollowingModel::Idm);
            let rng_seed = entity.id();

            let vtype_gpu = match *vtype {
                VehicleType::Motorbike => 0,
                VehicleType::Car => 1,
                VehicleType::Bus => 2,
                VehicleType::Bicycle => 3,
                VehicleType::Truck => 4,
                VehicleType::Emergency => 5,
                VehicleType::Pedestrian => 6,
            };

            gpu_agents.push(GpuAgentState {
                edge_id: rp.edge_index,
                lane_idx: rp.lane as u32,
                position: FixPos::from_f64(rp.offset_m).raw(),
                lateral: FixLat::from_f64(lat.map_or(0.0, |l| l.lateral_offset)).raw(),
                speed: FixSpd::from_f64(kin.speed).raw(),
                acceleration: 0,
                cf_model: cf as u32,
                rng_state: rng_seed,
                vehicle_type: vtype_gpu,
                flags: 0,
            });
            entity_map.push(entity);
        }

        if gpu_agents.is_empty() {
            return;
        }

        let (lane_offsets, lane_counts, lane_agent_indices) = sort_agents_by_lane(&gpu_agents);

        dispatcher.upload_wave_front_data(
            device,
            queue,
            &gpu_agents,
            &lane_offsets,
            &lane_counts,
            &lane_agent_indices,
        );

        let mut encoder = device.create_command_encoder(&Default::default());
        dispatcher.dispatch_wave_front(&mut encoder, device, queue, dt);
        queue.submit(std::iter::once(encoder.finish()));

        let updated = dispatcher.readback_wave_front_agents(device, queue);

        for (i, gpu_state) in updated.iter().enumerate() {
            if i >= entity_map.len() {
                break;
            }
            let entity = entity_map[i];

            let new_offset = FixPos::from_raw(gpu_state.position).to_f64();
            let new_speed = FixSpd::from_raw(gpu_state.speed).to_f64();

            let at_red = {
                let Ok(rp) = self.world.query_one_mut::<&RoadPosition>(entity) else {
                    continue;
                };
                let rp_copy = *rp;
                self.check_signal_red(&rp_copy)
            };
            self.apply_vehicle_update(entity, new_speed, new_offset, at_red);

            if let Ok(lat) = self.world.query_one_mut::<&mut LateralOffset>(entity) {
                let new_lateral = FixLat::from_raw(gpu_state.lateral).to_f64();
                lat.lateral_offset = new_lateral;
                lat.desired_lateral = new_lateral;
                self.apply_lateral_world_offset(entity, new_lateral);
            }
        }
    }

    /// Step pedestrians using social force model (CPU).
    fn step_pedestrians(&mut self, dt: f64, spatial: &SpatialIndex, snapshot: &AgentSnapshot) {
        struct PedState {
            entity: Entity,
            path: Vec<u32>,
            current_step: usize,
            pos: [f64; 2],
            vel: [f64; 2],
        }

        let peds: Vec<PedState> = self
            .world
            .query_mut::<(Entity, &VehicleType, &Route, &Position, &Kinematics)>()
            .into_iter()
            .filter(|(_, vt, _, _, _)| **vt == VehicleType::Pedestrian)
            .map(|(e, _, r, pos, kin)| PedState {
                entity: e,
                path: r.path.clone(),
                current_step: r.current_step,
                pos: [pos.x, pos.y],
                vel: [kin.vx, kin.vy],
            })
            .collect();

        struct PedUpdate {
            entity: Entity,
            new_pos: [f64; 2],
            new_vel: [f64; 2],
            speed: f64,
            advance_step: bool,
        }

        let mut updates = Vec::with_capacity(peds.len());

        for ped in &peds {
            let (entity, path, current_step, pos, vel) = (
                &ped.entity,
                &ped.path,
                &ped.current_step,
                &ped.pos,
                &ped.vel,
            );
            if *current_step >= path.len() {
                continue;
            }

            let target_node = NodeIndex::new(path[*current_step] as usize);
            let raw_target = self.road_graph.inner()[target_node].pos;

            let target_pos = if *current_step > 0 {
                let prev_node = NodeIndex::new(path[*current_step - 1] as usize);
                let prev_pos = self.road_graph.inner()[prev_node].pos;
                let seg_dx = raw_target[0] - prev_pos[0];
                let seg_dy = raw_target[1] - prev_pos[1];
                let seg_len = (seg_dx * seg_dx + seg_dy * seg_dy).sqrt();
                if seg_len > 0.1 {
                    let perp_x = -seg_dy / seg_len;
                    let perp_y = seg_dx / seg_len;
                    [raw_target[0] + perp_x * 5.0, raw_target[1] + perp_y * 5.0]
                } else {
                    raw_target
                }
            } else {
                raw_target
            };

            let nearby = spatial.nearest_within_radius(*pos, 3.0);
            let neighbors: Vec<PedestrianNeighbor> = nearby
                .iter()
                .filter(|n| {
                    let ddx = n.pos[0] - pos[0];
                    let ddy = n.pos[1] - pos[1];
                    ddx * ddx + ddy * ddy > 0.0001
                })
                .take(10)
                .filter_map(|n| {
                    let idx = snapshot.id_to_index.get(&n.id)?;
                    let n_vtype = snapshot.vehicle_types[*idx];
                    let radius = AgentSnapshot::half_width_for_type(n_vtype);
                    Some(PedestrianNeighbor {
                        pos: n.pos,
                        vel: [0.0, 0.0],
                        radius,
                    })
                })
                .collect();

            let accel = social_force::social_force_acceleration(
                *pos,
                *vel,
                target_pos,
                &neighbors,
                &self.social_force_params,
            );
            let (new_vel, speed) = social_force::integrate_pedestrian(
                *vel,
                accel,
                dt,
                self.social_force_params.max_speed,
            );

            let new_pos = [pos[0] + new_vel[0] * dt, pos[1] + new_vel[1] * dt];

            let dx = target_pos[0] - new_pos[0];
            let dy = target_pos[1] - new_pos[1];
            let dist = (dx * dx + dy * dy).sqrt();
            let advance = dist < 2.0;

            updates.push(PedUpdate {
                entity: *entity,
                new_pos,
                new_vel,
                speed,
                advance_step: advance,
            });
        }

        for upd in updates {
            let (pos, kin, route) = self
                .world
                .query_one_mut::<(&mut Position, &mut Kinematics, &mut Route)>(upd.entity)
                .unwrap();
            pos.x = upd.new_pos[0];
            pos.y = upd.new_pos[1];
            kin.vx = upd.new_vel[0];
            kin.vy = upd.new_vel[1];
            kin.speed = upd.speed;
            if upd.speed > 1e-6 {
                kin.heading = upd.new_vel[1].atan2(upd.new_vel[0]);
            }
            if upd.advance_step {
                route.current_step += 1;
            }
        }
    }
}
