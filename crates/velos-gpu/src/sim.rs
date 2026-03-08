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
    CarFollowingModel, GpuAgentState, Kinematics, LateralOffset, RoadPosition, VehicleType,
};
use velos_core::fixed_point::{FixLat, FixPos, FixSpd};
use velos_demand::{OdMatrix, Spawner, TodProfile, Zone};
use velos_net::{RoadGraph, SpatialIndex};
use velos_signal::detector::{DetectorReading, LoopDetector};
use velos_signal::priority::{PriorityLevel, PriorityRequest};
use velos_signal::SignalController;
use velos_meso::queue_model::SpatialQueue;
use velos_meso::zone_config::ZoneConfig;
use velos_vehicle::bus::{BusDwellModel, BusStop};
use velos_vehicle::config::VehicleConfig;
use velos_vehicle::social_force::SocialForceParams;
use velos_vehicle::sublane::SublaneParams;

use crate::sim_meso::MesoAgentState;

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
    /// Loaded vehicle configuration (used at startup, retained for runtime queries).
    #[allow(dead_code)]
    pub(crate) vehicle_config: VehicleConfig,
    /// Loop detectors for actuated signal controllers.
    pub(crate) loop_detectors: Vec<(NodeIndex, Vec<LoopDetector>)>,
    /// Pre-allocated GPU buffers for perception pipeline input.
    /// None in CPU-only test paths.
    pub(crate) perception_buffers: Option<PerceptionBuffers>,
    /// Bus stops on the network (empty until GTFS loaded).
    pub bus_stops: Vec<BusStop>,
    /// Empirical bus dwell time model parameters.
    pub(crate) bus_dwell_model: BusDwellModel,
    /// Whether mesoscopic zone simulation is enabled (default false).
    pub meso_enabled: bool,
    /// Zone configuration: maps edge IDs to Micro/Meso/Buffer zones.
    pub zone_config: ZoneConfig,
    /// Active SpatialQueues for meso-designated edges (keyed by edge ID).
    pub meso_queues: HashMap<u32, SpatialQueue>,
    /// Preserved agent state during meso transit (keyed by vehicle ID).
    pub meso_agent_states: HashMap<u32, MesoAgentState>,
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
            bus_stops: Vec::new(),
            bus_dwell_model: BusDwellModel::default(),
            meso_enabled: false,
            zone_config: sim_startup::load_zone_config(),
            meso_queues: HashMap::new(),
            meso_agent_states: HashMap::new(),
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
            bus_stops: Vec::new(),
            bus_dwell_model: BusDwellModel::default(),
            meso_enabled: false,
            zone_config: sim_startup::load_zone_config(),
            meso_queues: HashMap::new(),
            meso_agent_states: HashMap::new(),
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

    /// Enable mesoscopic simulation for edges designated in zone_config.
    ///
    /// Creates SpatialQueues for all edges with ZoneType::Meso.
    /// Queue capacity derived from edge length and lane count.
    pub fn enable_meso(&mut self) {
        use velos_meso::zone_config::ZoneType;

        let g = self.road_graph.inner();
        let mut queue_count = 0u32;

        for edge_idx in g.edge_indices() {
            let edge_id = edge_idx.index() as u32;
            if self.zone_config.zone_type(edge_id) == ZoneType::Meso {
                let edge = &g[edge_idx];
                let t_free = edge.length_m / edge.speed_limit_mps.max(1.0);
                let capacity = (edge.length_m / 7.0) * edge.lane_count as f64;
                self.meso_queues
                    .insert(edge_id, SpatialQueue::new(t_free, capacity.max(1.0)));
                queue_count += 1;
            }
        }

        self.meso_enabled = true;
        log::info!(
            "Meso simulation enabled: {} SpatialQueues created for meso edges",
            queue_count
        );
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
    ///
    /// Full 10-step pipeline:
    /// 1. spawn_agents       — generate new agents from OD matrix
    /// 2. update_loop_detectors — check agents crossing virtual detectors
    /// 3. step_signals        — advance signal controllers with detector readings
    /// 4. step_signal_priority — process bus/emergency priority requests
    /// 5. step_perception     — GPU perception gather + readback
    /// 6. step_reroute        — evaluate rerouting from perception results
    /// 7. step_vehicles_gpu   — GPU wave-front car-following physics
    /// 8. step_pedestrians    — CPU social force model
    /// 9. detect_gridlock     — cycle detection in stopped agents
    /// 10. remove + metrics   — cleanup finished agents, update counters
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

        // Update sim_time on dispatcher for WGSL school zone time windows.
        dispatcher.sim_time = self.sim_time as f32;

        // 1. Spawn new agents
        self.spawn_agents(dt);

        // 2. Update loop detectors (feed actuated signals)
        let detector_readings = self.update_loop_detectors();

        // 3. Advance signal controllers with detector readings
        self.step_signals_with_detectors(dt, &detector_readings);

        // 4. Process signal priority requests from buses/emergencies
        self.step_signal_priority();

        // 5. GPU perception dispatch + readback
        let perception_results = self.step_perception(device, queue, dispatcher);

        // 6. Reroute evaluation using perception results
        self.step_reroute(&perception_results);

        // 6.5. Meso queue tick + buffer zone insertion (BEFORE micro physics)
        self.step_meso(dt);

        // 7-8. Vehicle and pedestrian physics
        let snapshot = AgentSnapshot::collect(&self.world);
        let spatial = SpatialIndex::from_positions(&snapshot.ids, &snapshot.positions);

        self.step_vehicles_gpu(dt as f32, device, queue, dispatcher);
        self.step_bus_dwell(dt);
        self.step_pedestrians(dt, &spatial, &snapshot);

        // 9-10. Gridlock detection, cleanup, metrics
        self.detect_gridlock();
        self.remove_finished_agents();
        self.update_metrics();

        self.build_instances()
    }

    /// Run one simulation tick using CPU physics (fallback for tests without GPU).
    ///
    /// Same pipeline order as tick_gpu() but skips GPU perception and reroute
    /// (no GPU device available). Detector readings still feed signal controllers.
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

        let detector_readings = self.update_loop_detectors();
        self.step_signals_with_detectors(dt, &detector_readings);
        self.step_signal_priority();

        // No perception/reroute in CPU path (requires GPU device)

        // Meso queue tick + buffer zone insertion (BEFORE micro physics)
        self.step_meso(dt);

        let snapshot = AgentSnapshot::collect(&self.world);
        let spatial = SpatialIndex::from_positions(&snapshot.ids, &snapshot.positions);

        crate::cpu_reference::step_vehicles(self, dt, &spatial, &snapshot);
        crate::cpu_reference::step_motorbikes_sublane(self, dt, &spatial, &snapshot);
        self.step_bus_dwell(dt);
        self.step_pedestrians(dt, &spatial, &snapshot);

        self.detect_gridlock();
        self.remove_finished_agents();
        self.update_metrics();

        self.build_instances()
    }

    /// Advance signal controllers with detector readings from loop detectors.
    ///
    /// Each controller receives only the readings from its own intersection's
    /// detectors. Fixed-time controllers ignore readings; actuated controllers
    /// use them for gap-out decisions.
    fn step_signals_with_detectors(
        &mut self,
        dt: f64,
        detector_readings: &[(NodeIndex, Vec<DetectorReading>)],
    ) {
        for (node, ctrl) in &mut self.signal_controllers {
            let readings = detector_readings
                .iter()
                .find(|(n, _)| n == node)
                .map_or(&[][..], |(_, r)| r.as_slice());
            ctrl.tick(dt, readings);
        }
    }

    /// Check loop detectors for agent crossings.
    ///
    /// For each detector, scans agents on the same edge and checks if any
    /// agent's offset crossed the detector point this frame. Uses current
    /// ECS positions (RoadPosition.offset_m) compared against the previous
    /// frame's position stored in Kinematics.speed * dt approximation.
    fn update_loop_detectors(&self) -> Vec<(NodeIndex, Vec<DetectorReading>)> {
        let mut results = Vec::with_capacity(self.loop_detectors.len());

        for (node, detectors) in &self.loop_detectors {
            let mut readings = Vec::with_capacity(detectors.len());

            for (det_idx, detector) in detectors.iter().enumerate() {
                let mut triggered = false;

                // Scan agents on this detector's edge
                for (rp, kin) in self
                    .world
                    .query::<(&RoadPosition, &Kinematics)>()
                    .iter()
                {
                    if rp.edge_index != detector.edge_id {
                        continue;
                    }

                    // Approximate previous position: current offset minus distance
                    // traveled this frame. For forward-only detection this is
                    // sufficient (LoopDetector::check uses prev < offset <= cur).
                    let cur_pos = rp.offset_m;
                    // Use a small dt estimate; the exact dt doesn't matter much
                    // since we only need to know if the agent crossed the point.
                    // Speed * 1 tick at base dt gives a conservative estimate.
                    let prev_pos = (cur_pos - kin.speed.abs() * 0.1).max(0.0);

                    if detector.check(prev_pos, cur_pos) {
                        triggered = true;
                        break; // One trigger per detector per frame is sufficient
                    }
                }

                readings.push(DetectorReading {
                    detector_index: det_idx,
                    triggered,
                });
            }

            results.push((*node, readings));
        }

        results
    }

    /// Process signal priority requests from bus and emergency vehicles.
    ///
    /// Scans vehicles near signalized intersections (within 100m of the
    /// intersection node) and submits priority requests for bus and
    /// emergency vehicle types.
    fn step_signal_priority(&mut self) {
        // Collect priority requests (avoid borrow conflict with self)
        let mut requests: Vec<(NodeIndex, PriorityRequest)> = Vec::new();

        let g = self.road_graph.inner();

        for (entity, rp, vtype) in self
            .world
            .query::<(hecs::Entity, &RoadPosition, &VehicleType)>()
            .iter()
        {
            let level = match *vtype {
                VehicleType::Bus => PriorityLevel::Bus,
                VehicleType::Emergency => PriorityLevel::Emergency,
                _ => continue,
            };

            // Check if agent's edge connects to a signalized node
            let edge_idx = EdgeIndex::new(rp.edge_index as usize);
            let Some(endpoints) = g.edge_endpoints(edge_idx) else {
                continue;
            };
            let target_node = endpoints.1;
            let target_id = target_node.index() as u32;

            if !self.signalized_nodes.contains_key(&target_id) {
                continue;
            }

            // Check proximity: agent must be within 100m of intersection
            let edge_length = g
                .edge_weight(edge_idx)
                .map(|e| e.length_m)
                .unwrap_or(100.0);
            let distance_to_intersection = edge_length - rp.offset_m;
            if distance_to_intersection > 100.0 {
                continue;
            }

            // Determine approach index for this edge
            let incoming: Vec<_> = g
                .edges_directed(target_node, petgraph::Direction::Incoming)
                .collect();
            let approach_index = incoming
                .iter()
                .position(|e| {
                    use petgraph::visit::EdgeRef;
                    e.id() == edge_idx
                })
                .unwrap_or(0);

            requests.push((
                target_node,
                PriorityRequest {
                    approach_index,
                    level,
                    vehicle_id: entity.id(),
                },
            ));
        }

        // Submit requests to the matching signal controllers
        for (target_node, request) in &requests {
            for (ctrl_node, ctrl) in &mut self.signal_controllers {
                if ctrl_node == target_node {
                    ctrl.request_priority(request);
                    break;
                }
            }
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

        for (entity, rp, kin, vtype, lat, cf_model, bus_state) in self
            .world
            .query_mut::<(
                Entity,
                &RoadPosition,
                &Kinematics,
                &VehicleType,
                Option<&LateralOffset>,
                Option<&CarFollowingModel>,
                Option<&velos_vehicle::bus::BusState>,
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
                flags: if bus_state.map_or(false, |bs| bs.is_dwelling()) {
                    1
                } else {
                    0
                },
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

}
