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
use petgraph::visit::EdgeRef;
use petgraph::Direction;
use rand::rngs::StdRng;
use rand::SeedableRng;

use velos_core::components::{
    CarFollowingModel, GpuAgentState, Kinematics, LateralOffset, Position, RoadPosition, Route,
    VehicleType,
};
use velos_core::fixed_point::{FixLat, FixPos, FixSpd};
use velos_demand::{OdMatrix, Spawner, TodProfile, Zone};
use velos_net::{RoadGraph, SpatialIndex};
use velos_signal::controller::FixedTimeController;
use velos_signal::plan::{SignalPhase, SignalPlan};
use velos_vehicle::social_force::{self, PedestrianNeighbor, SocialForceParams};
use velos_vehicle::sublane::SublaneParams;

use crate::compute::{sort_agents_by_lane, ComputeDispatcher};
use crate::renderer::AgentInstance;
use crate::sim_snapshot::AgentSnapshot;

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
    pub signal_controllers: Vec<(NodeIndex, FixedTimeController)>,
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

    pub fn new(road_graph: RoadGraph) -> Self {
        let zone_centroids = zone_centroids_from_graph(&road_graph);
        let spawner = Spawner::new(Self::boosted_od(), TodProfile::hcmc_weekday(), 42);

        let mut signal_controllers = Vec::new();
        let mut signalized_nodes = HashMap::new();
        let g = road_graph.inner();
        for node_idx in g.node_indices() {
            let in_degree = g
                .edges_directed(node_idx, Direction::Incoming)
                .count();
            if in_degree >= 4 {
                let approaches: Vec<usize> = (0..in_degree).collect();
                let half = in_degree / 2;
                let phase_a = SignalPhase {
                    green_duration: 30.0,
                    amber_duration: 3.0,
                    approaches: approaches[..half].to_vec(),
                };
                let phase_b = SignalPhase {
                    green_duration: 30.0,
                    amber_duration: 3.0,
                    approaches: approaches[half..].to_vec(),
                };
                let plan = SignalPlan::new(vec![phase_a, phase_b]);
                let controller = FixedTimeController::new(plan, in_degree);
                signal_controllers.push((node_idx, controller));

                let edges: Vec<EdgeIndex> = g
                    .edges_directed(node_idx, Direction::Incoming)
                    .map(|e| e.id())
                    .collect();
                signalized_nodes.insert(node_idx.index() as u32, edges);
            }
        }

        log::info!(
            "Simulation initialized: {} signal controllers",
            signal_controllers.len()
        );

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
        }
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
        for (_, ctrl) in &mut self.signal_controllers {
            ctrl.reset();
        }
    }

    /// Run one simulation tick using GPU wave-front dispatch for vehicle physics.
    /// Returns per-type instance arrays for rendering.
    ///
    /// Vehicle car-following (IDM + Krauss) runs on GPU.
    /// Pedestrian social force runs on CPU.
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

        // GPU wave-front dispatch for all vehicles (cars + motorbikes).
        self.step_vehicles_gpu(dt as f32, device, queue, dispatcher);

        // Pedestrians still on CPU (social force model).
        self.step_pedestrians(dt, &spatial, &snapshot);

        self.detect_gridlock();
        self.remove_finished_agents();
        self.update_metrics();

        self.build_instances()
    }

    /// Run one simulation tick using CPU physics (fallback for tests without GPU).
    /// Returns per-type instance arrays for rendering.
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

        cpu_reference::step_vehicles(self, dt, &spatial, &snapshot);
        cpu_reference::step_motorbikes_sublane(self, dt, &spatial, &snapshot);
        self.step_pedestrians(dt, &spatial, &snapshot);

        self.detect_gridlock();
        self.remove_finished_agents();
        self.update_metrics();

        self.build_instances()
    }

    fn step_signals(&mut self, dt: f64) {
        for (_, ctrl) in &mut self.signal_controllers {
            ctrl.tick(dt);
        }
    }

    /// GPU wave-front dispatch for vehicle physics (cars + motorbikes).
    ///
    /// 1. Collect ECS agent state -> GpuAgentState
    /// 2. Sort by lane (CPU)
    /// 3. Upload + dispatch (GPU)
    /// 4. Readback + write back to ECS
    fn step_vehicles_gpu(
        &mut self,
        dt: f32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dispatcher: &mut ComputeDispatcher,
    ) {
        // Collect vehicle agent states from ECS.
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
            // Only vehicles (cars + motorbikes), not pedestrians.
            if *vtype == VehicleType::Pedestrian {
                continue;
            }

            let cf = cf_model.copied().unwrap_or(CarFollowingModel::Idm);
            let rng_seed = entity.id();

            gpu_agents.push(GpuAgentState {
                edge_id: rp.edge_index,
                lane_idx: rp.lane as u32,
                position: FixPos::from_f64(rp.offset_m).raw(),
                lateral: FixLat::from_f64(lat.map_or(0.0, |l| l.lateral_offset)).raw(),
                speed: FixSpd::from_f64(kin.speed).raw(),
                acceleration: 0,
                cf_model: cf as u32,
                rng_state: rng_seed,
            });
            entity_map.push(entity);
        }

        if gpu_agents.is_empty() {
            return;
        }

        // Sort by lane for wave-front dispatch.
        let (lane_offsets, lane_counts, lane_agent_indices) = sort_agents_by_lane(&gpu_agents);

        // Upload to GPU.
        dispatcher.upload_wave_front_data(
            device,
            queue,
            &gpu_agents,
            &lane_offsets,
            &lane_counts,
            &lane_agent_indices,
        );

        // Dispatch wave-front shader.
        let mut encoder = device.create_command_encoder(&Default::default());
        dispatcher.dispatch_wave_front(&mut encoder, device, queue, dt);
        queue.submit(std::iter::once(encoder.finish()));

        // Readback updated agent states.
        let updated = dispatcher.readback_wave_front_agents(device, queue);

        // Write back to ECS.
        for (i, gpu_state) in updated.iter().enumerate() {
            if i >= entity_map.len() {
                break;
            }
            let entity = entity_map[i];

            let new_offset = FixPos::from_raw(gpu_state.position).to_f64();
            let new_speed = FixSpd::from_raw(gpu_state.speed).to_f64();

            // Apply via the standard vehicle update path (handles edge transitions).
            let at_red = {
                let Ok(rp) = self.world.query_one_mut::<&RoadPosition>(entity) else {
                    continue;
                };
                let rp_copy = *rp;
                self.check_signal_red(&rp_copy)
            };
            self.apply_vehicle_update(entity, new_speed, new_offset, at_red);

            // For motorbikes, update lateral offset.
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

/// CPU reference implementations of vehicle physics.
///
/// Kept for GPU validation testing. NOT used in the production sim loop.
/// Production uses `SimWorld::step_vehicles_gpu()` via wave-front dispatch.
pub(crate) mod cpu_reference {
    use std::collections::HashMap;

    use hecs::Entity;

    use velos_core::components::{
        Kinematics, LaneChangeState, LateralOffset, Position, RoadPosition, VehicleType,
    };
    use velos_net::SpatialIndex;
    use velos_vehicle::idm::{idm_acceleration, integrate_with_stopping_guard, IdmParams};
    use velos_vehicle::sublane::{self, NeighborInfo};

    use crate::sim::SimWorld;
    use crate::sim_snapshot::AgentSnapshot;

    /// CPU step for car vehicles (IDM + MOBIL). Test/validation reference only.
    pub fn step_vehicles(
        sim: &mut SimWorld,
        dt: f64,
        spatial: &SpatialIndex,
        snapshot: &AgentSnapshot,
    ) {
        use crate::sim_mobil::CarMobilContext;

        struct CarSnap {
            entity: Entity,
            rp: RoadPosition,
            speed: f64,
            heading: f64,
            idm: IdmParams,
            pos: [f64; 2],
            has_lc: bool,
        }

        let agents: Vec<CarSnap> = sim
            .world
            .query_mut::<(
                Entity,
                &RoadPosition,
                &Kinematics,
                &IdmParams,
                &VehicleType,
                &Position,
                Option<&LaneChangeState>,
            )>()
            .into_iter()
            .filter(|(_, _, _, _, vt, _, _)| **vt == VehicleType::Car)
            .map(|(e, rp, kin, idm, _, pos, lcs)| CarSnap {
                entity: e,
                rp: *rp,
                speed: kin.speed,
                heading: kin.heading,
                idm: *idm,
                pos: [pos.x, pos.y],
                has_lc: lcs.is_some(),
            })
            .collect();

        let mut edge_agents: HashMap<u32, Vec<(Entity, f64)>> = HashMap::new();
        let mut edge_lane_agents: HashMap<(u32, u8), Vec<(Entity, f64)>> = HashMap::new();
        for car in &agents {
            edge_agents
                .entry(car.rp.edge_index)
                .or_default()
                .push((car.entity, car.rp.offset_m));
            edge_lane_agents
                .entry((car.rp.edge_index, car.rp.lane))
                .or_default()
                .push((car.entity, car.rp.offset_m));
        }
        for v in edge_agents.values_mut() {
            v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        }
        for v in edge_lane_agents.values_mut() {
            v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        }

        let speed_map: HashMap<Entity, f64> =
            agents.iter().map(|c| (c.entity, c.speed)).collect();

        struct CarUpdate {
            entity: Entity,
            v_new: f64,
            new_offset: f64,
            at_red: bool,
            start_lane_change: Option<(u8, f64)>,
        }

        let mut updates: Vec<CarUpdate> = Vec::with_capacity(agents.len());

        for car in &agents {
            let at_red = sim.check_signal_red(&car.rp);

            let (mut gap, mut delta_v) = if at_red {
                (2.0, car.speed)
            } else {
                SimWorld::find_leader_static(
                    car.entity, &car.rp, &edge_agents, &speed_map, car.speed,
                )
            };

            let nearby = spatial.nearest_within_radius(car.pos, 8.0);
            for neighbor in &nearby {
                let dx = neighbor.pos[0] - car.pos[0];
                let dy = neighbor.pos[1] - car.pos[1];
                let longitudinal = dx * car.heading.cos() + dy * car.heading.sin();
                let lateral = (-dx * car.heading.sin() + dy * car.heading.cos()).abs();
                if longitudinal < 2.0 || lateral > 2.0 {
                    continue;
                }
                if let Some(vt) = snapshot.vehicle_type(neighbor.id)
                    && vt == VehicleType::Pedestrian
                    && longitudinal < gap
                {
                    let ped_speed = snapshot.speed(neighbor.id).unwrap_or(0.0);
                    gap = longitudinal;
                    delta_v = (car.speed - ped_speed).max(0.0);
                }
            }

            let accel_current = idm_acceleration(&car.idm, car.speed, gap, delta_v);
            let (v_new, dx) = integrate_with_stopping_guard(car.speed, accel_current, dt);

            let start_lc = if !at_red {
                let ctx = CarMobilContext {
                    entity: car.entity,
                    rp: car.rp,
                    speed: car.speed,
                    idm_params: car.idm,
                    has_lane_change: car.has_lc,
                };
                sim.evaluate_mobil(&ctx, accel_current, &edge_lane_agents, &speed_map)
            } else {
                None
            };

            updates.push(CarUpdate {
                entity: car.entity,
                v_new,
                new_offset: car.rp.offset_m + dx,
                at_red,
                start_lane_change: start_lc,
            });
        }

        for upd in updates {
            if let Some((target_lane, started_at)) = upd.start_lane_change {
                sim.start_lane_change(upd.entity, target_lane, started_at);
            }
            sim.apply_vehicle_update(upd.entity, upd.v_new, upd.new_offset, upd.at_red);
        }

        sim.process_car_lane_changes(dt);

        let car_offsets: Vec<(Entity, f64, bool)> = sim
            .world
            .query_mut::<(Entity, &LateralOffset, &VehicleType, Option<&LaneChangeState>)>()
            .into_iter()
            .filter(|(_, _, vt, _)| **vt == VehicleType::Car)
            .map(|(e, lat, _, lcs)| (e, lat.lateral_offset, lcs.is_some()))
            .collect();
        for (entity, lateral, has_lc) in car_offsets {
            if !has_lc {
                sim.apply_lateral_world_offset(entity, lateral);
            }
        }
    }

    /// CPU step for motorbike agents with sublane lateral positioning.
    /// Test/validation reference only.
    pub fn step_motorbikes_sublane(
        sim: &mut SimWorld,
        dt: f64,
        spatial: &SpatialIndex,
        snapshot: &AgentSnapshot,
    ) {
        use petgraph::graph::EdgeIndex;

        struct BikeState {
            entity: Entity,
            rp: RoadPosition,
            speed: f64,
            idm_params: IdmParams,
            lateral: f64,
            heading: f64,
            pos: [f64; 2],
        }

        let bikes: Vec<BikeState> = sim
            .world
            .query_mut::<(
                Entity,
                &RoadPosition,
                &Kinematics,
                &IdmParams,
                &LateralOffset,
                &VehicleType,
                &Position,
            )>()
            .into_iter()
            .filter(|(_, _, _, _, _, vt, _)| **vt == VehicleType::Motorbike)
            .map(|(e, rp, kin, idm, lat, _, pos)| BikeState {
                entity: e,
                rp: *rp,
                speed: kin.speed,
                idm_params: *idm,
                lateral: lat.lateral_offset,
                heading: kin.heading,
                pos: [pos.x, pos.y],
            })
            .collect();

        struct BikeUpdate {
            entity: Entity,
            v_new: f64,
            new_offset: f64,
            new_lateral: f64,
            at_red: bool,
        }

        let mut updates: Vec<BikeUpdate> = Vec::with_capacity(bikes.len());

        for bike in &bikes {
            let (entity, rp, speed, idm_params, lateral, heading, agent_pos) = (
                &bike.entity,
                &bike.rp,
                &bike.speed,
                &bike.idm_params,
                &bike.lateral,
                &bike.heading,
                &bike.pos,
            );
            let at_red = sim.check_signal_red(rp);

            let edge = EdgeIndex::new(rp.edge_index as usize);
            let lane_count = sim
                .road_graph
                .inner()
                .edge_weight(edge)
                .map(|e| e.lane_count as f64)
                .unwrap_or(2.0);
            let road_width = lane_count * 3.5;

            let nearby = spatial.nearest_within_radius_capped(*agent_pos, 6.0, 20);

            let mut neighbor_infos = Vec::new();
            let mut idm_gap = 1000.0_f64;
            let mut idm_delta_v = 0.0_f64;

            for n in &nearby {
                let dx = n.pos[0] - agent_pos[0];
                let dy = n.pos[1] - agent_pos[1];
                let dist_sq = dx * dx + dy * dy;
                if dist_sq < 0.0001 {
                    continue;
                }
                let Some(n_vtype) = snapshot.vehicle_type(n.id) else {
                    continue;
                };

                let n_heading = snapshot.heading(n.id).unwrap_or(0.0);
                let angle_diff = n_heading - heading;
                if angle_diff.cos() < 0.0 {
                    continue;
                }

                let n_speed = snapshot.speed(n.id).unwrap_or(0.0);
                let n_half_width = AgentSnapshot::half_width_for_type(n_vtype);
                let n_lateral = snapshot.lateral_offset(n.id).unwrap_or(road_width / 2.0);

                let longitudinal = dx * heading.cos() + dy * heading.sin();

                if n_vtype != VehicleType::Pedestrian {
                    neighbor_infos.push(NeighborInfo {
                        lateral_offset: n_lateral,
                        longitudinal_gap: longitudinal,
                        half_width: n_half_width,
                        speed: n_speed,
                    });

                    let lateral_dist = (-dx * heading.sin() + dy * heading.cos()).abs();
                    if longitudinal > 0.0 && lateral_dist < 0.8 && longitudinal < idm_gap {
                        idm_gap = longitudinal;
                        idm_delta_v = *speed - n_speed;
                    }
                }
            }

            if at_red && *speed < 0.5 && idm_gap > 2.0 {
                idm_gap = 2.0;
                idm_delta_v = *speed;
            }

            let desired = sublane::compute_desired_lateral(
                *lateral,
                *speed,
                road_width,
                &neighbor_infos,
                at_red,
                &sim.sublane_params,
            );
            let max_lat_speed = if at_red {
                sim.sublane_params.swarm_lateral_speed
            } else {
                sim.sublane_params.max_lateral_speed
            };
            let new_lateral = sublane::apply_lateral_drift(*lateral, desired, max_lat_speed, dt);

            let accel = idm_acceleration(idm_params, *speed, idm_gap, idm_delta_v);
            let (v_new, ddx) = integrate_with_stopping_guard(*speed, accel, dt);

            updates.push(BikeUpdate {
                entity: *entity,
                v_new,
                new_offset: rp.offset_m + ddx,
                new_lateral,
                at_red,
            });
        }

        for upd in updates {
            if let Ok(lat) = sim.world.query_one_mut::<&mut LateralOffset>(upd.entity) {
                lat.lateral_offset = upd.new_lateral;
                lat.desired_lateral = upd.new_lateral;
            }

            sim.apply_vehicle_update(upd.entity, upd.v_new, upd.new_offset, upd.at_red);
            sim.apply_lateral_world_offset(upd.entity, upd.new_lateral);
        }
    }
}
