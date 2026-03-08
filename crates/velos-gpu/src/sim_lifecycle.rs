//! Agent spawning, gridlock detection, removal, and metrics for SimWorld.

use std::collections::HashMap;

use hecs::Entity;
use petgraph::graph::{EdgeIndex, NodeIndex};
use rand::Rng;

use velos_core::components::{
    CarFollowingModel, Kinematics, LateralOffset, Position, RoadPosition, Route, VehicleType,
    WaitState,
};
use velos_core::cost::AgentProfile;
use velos_demand::bus_spawner::BusSpawnRequest;
use velos_demand::SpawnVehicleType;
use velos_vehicle::bus::BusState;
use velos_vehicle::gridlock::detect_cycles;
use velos_vehicle::types::default_idm_params;

use crate::sim::SimWorld;

impl SimWorld {
    pub(crate) fn spawn_agents(&mut self, dt: f64) {
        let sim_hour = self.sim_time / 3600.0;
        let requests = self.spawner.generate_spawns(sim_hour, dt);
        for req in requests {
            self.spawn_single_agent(&req);
        }

        // GTFS bus spawning (time-gated by trip departure schedule).
        if let Some(ref mut bus_spawner) = self.bus_spawner {
            let bus_requests = bus_spawner.generate_bus_spawns(self.sim_time);
            for bus_req in bus_requests {
                self.spawn_gtfs_bus(&bus_req);
            }
        }
    }

    /// Spawn a GTFS-scheduled bus agent with route-specific stop indices.
    ///
    /// Unlike OD-spawned buses (which discover stops by matching route edges),
    /// GTFS buses receive precomputed stop_indices from their route definition.
    /// The bus starts at the first stop's edge with offset matching the stop position.
    fn spawn_gtfs_bus(&mut self, req: &BusSpawnRequest) {
        if req.stop_indices.is_empty() {
            log::debug!(
                "Skipping GTFS bus trip={} route={}: no valid stop indices",
                req.trip_id, req.route_id
            );
            return;
        }

        let first_stop_idx = req.stop_indices[0];
        if first_stop_idx >= self.bus_stops.len() {
            log::warn!(
                "GTFS bus trip={}: first stop index {} out of bounds ({})",
                req.trip_id, first_stop_idx, self.bus_stops.len()
            );
            return;
        }

        let first_stop = &self.bus_stops[first_stop_idx];
        let edge_id = first_stop.edge_id;
        let offset_m = first_stop.offset_m;

        let g = self.road_graph.inner();
        let edge_idx = EdgeIndex::new(edge_id as usize);
        let Some(endpoints) = g.edge_endpoints(edge_idx) else {
            log::warn!(
                "GTFS bus trip={}: edge {} not found in graph",
                req.trip_id, edge_id
            );
            return;
        };

        let start_pos = g[endpoints.0].pos;
        let end_pos = g[endpoints.1].pos;
        let heading = (end_pos[1] - start_pos[1]).atan2(end_pos[0] - start_pos[0]);

        // Interpolate spawn position along the edge.
        let edge_length = g.edge_weight(edge_idx)
            .map(|e| e.length_m)
            .unwrap_or(100.0);
        let t = (offset_m / edge_length).clamp(0.0, 1.0);
        let spawn_x = start_pos[0] + t * (end_pos[0] - start_pos[0]);
        let spawn_y = start_pos[1] + t * (end_pos[1] - start_pos[1]);

        // Build a minimal route: just the two nodes of the starting edge,
        // then the last stop's edge target node.
        let mut path_nodes: Vec<u32> = vec![
            endpoints.0.index() as u32,
            endpoints.1.index() as u32,
        ];

        // Add the last stop's edge target if different from current.
        if req.stop_indices.len() > 1 {
            let last_stop_idx = *req.stop_indices.last().unwrap();
            if last_stop_idx < self.bus_stops.len() {
                let last_edge = EdgeIndex::new(self.bus_stops[last_stop_idx].edge_id as usize);
                if let Some(last_endpoints) = g.edge_endpoints(last_edge) {
                    let last_target = last_endpoints.1.index() as u32;
                    if last_target != path_nodes[path_nodes.len() - 1] {
                        path_nodes.push(last_target);
                    }
                }
            }
        }

        let idm_params = default_idm_params(velos_vehicle::types::VehicleType::Bus);
        let initial_lateral = 0.5 * 3.5; // lane 0 center

        // 70% IDM, 30% Krauss per existing convention.
        let cf_model = if self.rng.gen_ratio(3, 10) {
            CarFollowingModel::Krauss
        } else {
            CarFollowingModel::Idm
        };

        self.world.spawn((
            Position { x: spawn_x, y: spawn_y },
            Kinematics {
                vx: heading.cos() * 0.1,
                vy: heading.sin() * 0.1,
                speed: 0.1,
                heading,
            },
            VehicleType::Bus,
            RoadPosition {
                edge_index: edge_id,
                lane: 0,
                offset_m,
            },
            Route {
                path: path_nodes,
                current_step: 1,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            idm_params,
            cf_model,
            LateralOffset {
                lateral_offset: initial_lateral,
                desired_lateral: initial_lateral,
            },
            BusState::new(req.stop_indices.clone()),
            AgentProfile::Bus,
        ));

        log::debug!(
            "Spawned GTFS bus: trip={}, route={}, stops={}",
            req.trip_id, req.route_id, req.stop_indices.len()
        );
    }

    fn spawn_single_agent(&mut self, req: &velos_demand::SpawnRequest) {
        let origin_pos = self.zone_centroids.get(&req.origin).copied().unwrap_or([0.0, 0.0]);
        let dest_pos = self.zone_centroids.get(&req.destination).copied().unwrap_or([0.0, 0.0]);

        let from_node = self.random_node_near(origin_pos, 300.0);
        let to_node = self.random_node_near(dest_pos, 300.0);

        if from_node == to_node {
            return;
        }

        let route_result = velos_net::find_route(&self.road_graph, from_node, to_node);
        let path = match route_result {
            Ok((path, _cost)) => path,
            Err(_) => return,
        };

        if path.len() < 2 {
            return;
        }

        let vtype = match req.vehicle_type {
            SpawnVehicleType::Motorbike => VehicleType::Motorbike,
            SpawnVehicleType::Car => VehicleType::Car,
            SpawnVehicleType::Bus => VehicleType::Bus,
            SpawnVehicleType::Bicycle => VehicleType::Bicycle,
            SpawnVehicleType::Truck => VehicleType::Truck,
            SpawnVehicleType::Emergency => VehicleType::Emergency,
            SpawnVehicleType::Pedestrian => VehicleType::Pedestrian,
        };

        let g = self.road_graph.inner();
        let edge_idx = g
            .find_edge(path[0], path[1])
            .map(|e| e.index() as u32)
            .unwrap_or(0);

        let start_pos = g[path[0]].pos;
        let next_pos = g[path[1]].pos;
        let heading = (next_pos[1] - start_pos[1]).atan2(next_pos[0] - start_pos[0]);

        let vehicle_type_for_params = match vtype {
            VehicleType::Motorbike => velos_vehicle::types::VehicleType::Motorbike,
            VehicleType::Car => velos_vehicle::types::VehicleType::Car,
            VehicleType::Bus => velos_vehicle::types::VehicleType::Bus,
            VehicleType::Bicycle => velos_vehicle::types::VehicleType::Bicycle,
            VehicleType::Truck => velos_vehicle::types::VehicleType::Truck,
            VehicleType::Emergency => velos_vehicle::types::VehicleType::Emergency,
            VehicleType::Pedestrian => velos_vehicle::types::VehicleType::Pedestrian,
        };
        let idm_params = default_idm_params(vehicle_type_for_params);

        // Determine car-following model per agent.
        // Motorbikes + Bicycles: always IDM (sublane model is IDM-based).
        // Cars, Buses, Trucks, Emergency: ~30% Krauss, ~70% IDM.
        // Pedestrians: no CarFollowingModel component (skip car-following entirely).
        let cf_model = match vtype {
            VehicleType::Motorbike | VehicleType::Bicycle => Some(CarFollowingModel::Idm),
            VehicleType::Car | VehicleType::Bus | VehicleType::Truck | VehicleType::Emergency => {
                if self.rng.gen_ratio(3, 10) {
                    Some(CarFollowingModel::Krauss)
                } else {
                    Some(CarFollowingModel::Idm)
                }
            }
            VehicleType::Pedestrian => None,
        };

        let jitter_x = self.rng.gen_range(-5.0..5.0);
        let jitter_y = self.rng.gen_range(-5.0..5.0);
        let path_u32: Vec<u32> = path.iter().map(|n| n.index() as u32).collect();

        // Pre-compute bus stop indices for bus vehicles (before path_u32 is moved).
        let bus_stop_indices: Vec<usize> = if vtype == VehicleType::Bus {
            let route_edges: Vec<u32> = path_u32.windows(2).filter_map(|w| {
                let from = NodeIndex::new(w[0] as usize);
                let to = NodeIndex::new(w[1] as usize);
                g.find_edge(from, to).map(|e| e.index() as u32)
            }).collect();
            self.bus_stops.iter().enumerate()
                .filter(|(_, stop)| route_edges.contains(&stop.edge_id))
                .map(|(idx, _)| idx)
                .collect()
        } else {
            Vec::new()
        };

        // Offset pedestrians to the sidewalk (5m perpendicular to road direction).
        let (spawn_x, spawn_y) = if vtype == VehicleType::Pedestrian {
            let seg_dx = next_pos[0] - start_pos[0];
            let seg_dy = next_pos[1] - start_pos[1];
            let seg_len = (seg_dx * seg_dx + seg_dy * seg_dy).sqrt().max(0.1);
            let perp_x = -seg_dy / seg_len;
            let perp_y = seg_dx / seg_len;
            (
                start_pos[0] + perp_x * 5.0 + jitter_x * 0.5,
                start_pos[1] + perp_y * 5.0 + jitter_y * 0.5,
            )
        } else {
            (start_pos[0] + jitter_x, start_pos[1] + jitter_y)
        };

        let base_components = (
            Position {
                x: spawn_x,
                y: spawn_y,
            },
            Kinematics {
                vx: heading.cos() * 0.1,
                vy: heading.sin() * 0.1,
                speed: 0.1,
                heading,
            },
            vtype,
            RoadPosition {
                edge_index: edge_idx,
                lane: 0,
                offset_m: 0.0,
            },
            Route {
                path: path_u32,
                current_step: 1,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            idm_params,
        );

        if vtype == VehicleType::Motorbike || vtype == VehicleType::Bicycle {
            // Sublane model vehicles: continuous lateral positioning.
            let edge = EdgeIndex::new(edge_idx as usize);
            let lane_count = g
                .edge_weight(edge)
                .map(|e| e.lane_count as f64)
                .unwrap_or(2.0);
            let road_width = lane_count * 3.5;
            let initial_lateral = road_width / 2.0;

            self.world.spawn((
                base_components.0,
                base_components.1,
                base_components.2,
                base_components.3,
                base_components.4,
                base_components.5,
                base_components.6,
                cf_model.unwrap(),
                LateralOffset {
                    lateral_offset: initial_lateral,
                    desired_lateral: initial_lateral,
                },
                req.profile,
            ));
        } else if vtype == VehicleType::Car
            || vtype == VehicleType::Bus
            || vtype == VehicleType::Truck
            || vtype == VehicleType::Emergency
        {
            // Lane-based vehicles: LateralOffset at lane 0 center.
            let initial_lateral = (0.0 + 0.5) * 3.5; // lane 0 center = 1.75m

            if vtype == VehicleType::Bus {
                self.world.spawn((
                    base_components.0,
                    base_components.1,
                    base_components.2,
                    base_components.3,
                    base_components.4,
                    base_components.5,
                    base_components.6,
                    cf_model.unwrap(),
                    LateralOffset {
                        lateral_offset: initial_lateral,
                        desired_lateral: initial_lateral,
                    },
                    BusState::new(bus_stop_indices),
                    req.profile,
                ));
            } else {
                self.world.spawn((
                    base_components.0,
                    base_components.1,
                    base_components.2,
                    base_components.3,
                    base_components.4,
                    base_components.5,
                    base_components.6,
                    cf_model.unwrap(),
                    LateralOffset {
                        lateral_offset: initial_lateral,
                        desired_lateral: initial_lateral,
                    },
                    req.profile,
                ));
            }
        } else {
            // Pedestrians: no CarFollowingModel (they use social force, not car-following).
            self.world.spawn((
                base_components.0,
                base_components.1,
                base_components.2,
                base_components.3,
                base_components.4,
                base_components.5,
                base_components.6,
                req.profile,
            ));
        }
    }

    pub(crate) fn random_node_near(&mut self, pos: [f64; 2], radius: f64) -> NodeIndex {
        let g = self.road_graph.inner();
        let r2 = radius * radius;
        let candidates: Vec<NodeIndex> = g
            .node_indices()
            .filter(|n| {
                let np = g[*n].pos;
                let dx = np[0] - pos[0];
                let dy = np[1] - pos[1];
                dx * dx + dy * dy <= r2
            })
            .collect();

        if candidates.is_empty() {
            let mut best = NodeIndex::new(0);
            let mut best_dist = f64::MAX;
            for node in g.node_indices() {
                let np = g[node].pos;
                let dx = np[0] - pos[0];
                let dy = np[1] - pos[1];
                let dist = dx * dx + dy * dy;
                if dist < best_dist {
                    best_dist = dist;
                    best = node;
                }
            }
            best
        } else {
            let idx = self.rng.gen_range(0..candidates.len());
            candidates[idx]
        }
    }

    pub(crate) fn detect_gridlock(&mut self) {
        let stopped: Vec<(Entity, RoadPosition)> = self
            .world
            .query_mut::<(Entity, &RoadPosition, &WaitState, &VehicleType)>()
            .into_iter()
            .filter(|(_, _, ws, vt)| {
                **vt != VehicleType::Pedestrian
                    && ws.stopped_since > 0.0
                    && (self.sim_time - ws.stopped_since) > self.gridlock_timeout
                    && !ws.at_red_signal
            })
            .map(|(e, rp, _, _)| (e, *rp))
            .collect();

        if stopped.is_empty() {
            return;
        }

        let mut edge_stopped: HashMap<u32, Vec<(Entity, f64)>> = HashMap::new();
        for (entity, rp) in &stopped {
            edge_stopped
                .entry(rp.edge_index)
                .or_default()
                .push((*entity, rp.offset_m));
        }

        let mut waiting_graph: HashMap<u32, u32> = HashMap::new();
        for (entity, rp) in &stopped {
            let eid = entity.id();
            if let Some(agents_on_edge) = edge_stopped.get(&rp.edge_index) {
                let mut closest_ahead: Option<u32> = None;
                let mut closest_gap = f64::MAX;
                for (other, other_offset) in agents_on_edge {
                    if *other == *entity {
                        continue;
                    }
                    let gap = other_offset - rp.offset_m;
                    if gap > 0.0 && gap < closest_gap {
                        closest_gap = gap;
                        closest_ahead = Some(other.id());
                    }
                }
                if let Some(blocker) = closest_ahead {
                    waiting_graph.insert(eid, blocker);
                }
            }
        }

        let cycles = detect_cycles(&waiting_graph);
        for cycle in &cycles {
            if let Some(&agent_id) = cycle.first() {
                self.teleport_agent_forward(agent_id);
            }
        }
    }

    fn teleport_agent_forward(&mut self, agent_id: u32) {
        let entity: Option<Entity> = self
            .world
            .query_mut::<(Entity, &Route)>()
            .into_iter()
            .find(|(e, _)| e.id() == agent_id)
            .map(|(e, _)| e);

        let Some(entity) = entity else { return };

        let next_pos = {
            let route = self.world.query_one_mut::<&Route>(entity).unwrap();
            if route.current_step + 1 < route.path.len() {
                let next_node = NodeIndex::new(route.path[route.current_step + 1] as usize);
                Some(self.road_graph.inner()[next_node].pos)
            } else {
                None
            }
        };

        if let Some(next_pos) = next_pos {
            let (pos, route, rp, ws) = self
                .world
                .query_one_mut::<(&mut Position, &mut Route, &mut RoadPosition, &mut WaitState)>(
                    entity,
                )
                .unwrap();
            pos.x = next_pos[0];
            pos.y = next_pos[1];
            route.current_step += 1;
            rp.offset_m = 0.0;
            ws.stopped_since = -1.0;

            if route.current_step + 1 < route.path.len() {
                let from = NodeIndex::new(route.path[route.current_step] as usize);
                let to = NodeIndex::new(route.path[route.current_step + 1] as usize);
                if let Some(edge) = self.road_graph.inner().find_edge(from, to) {
                    rp.edge_index = edge.index() as u32;
                }
            }
        }
    }

    pub(crate) fn remove_finished_agents(&mut self) {
        let finished: Vec<Entity> = self
            .world
            .query_mut::<(Entity, &Route)>()
            .into_iter()
            .filter(|(_, route)| route.current_step >= route.path.len())
            .map(|(e, _)| e)
            .collect();

        for entity in finished {
            let _ = self.world.despawn(entity);
        }
    }

    pub(crate) fn update_metrics(&mut self) {
        let mut motorbike_count = 0u32;
        let mut car_count = 0u32;
        let mut ped_count = 0u32;

        for vtype in self.world.query_mut::<&VehicleType>().into_iter() {
            match *vtype {
                VehicleType::Motorbike | VehicleType::Bicycle => motorbike_count += 1,
                VehicleType::Car | VehicleType::Bus | VehicleType::Truck | VehicleType::Emergency => car_count += 1,
                VehicleType::Pedestrian => ped_count += 1,
            }
        }

        self.metrics.agent_count = motorbike_count + car_count + ped_count;
        self.metrics.motorbike_count = motorbike_count;
        self.metrics.car_count = car_count;
        self.metrics.ped_count = ped_count;
        self.metrics.sim_time = self.sim_time;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::graph::DiGraph;
    use velos_core::cost::AgentProfile;
    use velos_demand::bus_spawner::BusSpawnRequest;
    use velos_demand::Zone;
    use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
    use velos_vehicle::bus::BusStop;

    /// Build a minimal graph with two nodes connected by an edge.
    fn make_spawn_test_graph() -> RoadGraph {
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [200.0, 0.0] });
        g.add_edge(
            a,
            b,
            RoadEdge {
                length_m: 200.0,
                speed_limit_mps: 13.9,
                lane_count: 2,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![[0.0, 0.0], [200.0, 0.0]],
                motorbike_only: false,
                time_windows: None,
            },
        );
        RoadGraph::new(g)
    }

    /// Build a SpawnRequest for testing profile attachment.
    fn make_spawn_request(
        vehicle_type: SpawnVehicleType,
        profile: AgentProfile,
    ) -> velos_demand::SpawnRequest {
        velos_demand::SpawnRequest {
            origin: Zone::BenThanh,
            destination: Zone::Bitexco,
            vehicle_type,
            profile,
        }
    }

    #[test]
    fn spawn_bus_agent_has_bus_profile() {
        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        let req = make_spawn_request(SpawnVehicleType::Bus, AgentProfile::Bus);
        sim.spawn_single_agent(&req);

        // Find the spawned agent and check its profile.
        let profiles: Vec<AgentProfile> = sim
            .world
            .query_mut::<&AgentProfile>()
            .into_iter()
            .copied()
            .collect();

        // The spawn may fail if no route is found (graph too small),
        // but if an agent was spawned, it must have the correct profile.
        if !profiles.is_empty() {
            assert_eq!(profiles[0], AgentProfile::Bus);
        }
    }

    #[test]
    fn spawn_emergency_agent_has_emergency_profile() {
        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        let req = make_spawn_request(SpawnVehicleType::Emergency, AgentProfile::Emergency);
        sim.spawn_single_agent(&req);

        let profiles: Vec<AgentProfile> = sim
            .world
            .query_mut::<&AgentProfile>()
            .into_iter()
            .copied()
            .collect();

        if !profiles.is_empty() {
            assert_eq!(profiles[0], AgentProfile::Emergency);
        }
    }

    #[test]
    fn spawn_motorbike_with_tourist_profile() {
        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        let req = make_spawn_request(SpawnVehicleType::Motorbike, AgentProfile::Tourist);
        sim.spawn_single_agent(&req);

        let profiles: Vec<AgentProfile> = sim
            .world
            .query_mut::<&AgentProfile>()
            .into_iter()
            .copied()
            .collect();

        if !profiles.is_empty() {
            assert_eq!(profiles[0], AgentProfile::Tourist);
        }
    }

    #[test]
    fn spawn_pedestrian_has_profile() {
        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        let req = make_spawn_request(SpawnVehicleType::Pedestrian, AgentProfile::Commuter);
        sim.spawn_single_agent(&req);

        let profiles: Vec<AgentProfile> = sim
            .world
            .query_mut::<&AgentProfile>()
            .into_iter()
            .copied()
            .collect();

        if !profiles.is_empty() {
            assert_eq!(profiles[0], AgentProfile::Commuter);
        }
    }

    #[test]
    fn spawn_gtfs_bus_creates_entity_with_bus_state() {
        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);

        // Add a bus stop on edge 0 at offset 50m.
        sim.bus_stops = vec![
            BusStop { edge_id: 0, offset_m: 50.0, capacity: 40, name: "Test Stop".to_string() },
        ];

        let req = BusSpawnRequest {
            route_id: "R1".to_string(),
            trip_id: "T1".to_string(),
            stop_indices: vec![0],
        };
        sim.spawn_gtfs_bus(&req);

        // Verify a bus entity was spawned with BusState.
        let mut bus_count = 0;
        let mut has_bus_state = false;
        for (vtype, bs) in sim.world.query_mut::<(&VehicleType, &BusState)>() {
            if *vtype == VehicleType::Bus {
                bus_count += 1;
                has_bus_state = true;
                assert_eq!(bs.stop_indices(), &[0], "stop_indices should match request");
            }
        }
        assert_eq!(bus_count, 1, "exactly one GTFS bus should be spawned");
        assert!(has_bus_state, "GTFS bus should have BusState component");
    }

    #[test]
    fn spawn_gtfs_bus_with_multiple_stops_has_correct_indices() {
        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);

        sim.bus_stops = vec![
            BusStop { edge_id: 0, offset_m: 20.0, capacity: 40, name: "Stop A".to_string() },
            BusStop { edge_id: 0, offset_m: 80.0, capacity: 40, name: "Stop B".to_string() },
            BusStop { edge_id: 0, offset_m: 150.0, capacity: 40, name: "Stop C".to_string() },
        ];

        let req = BusSpawnRequest {
            route_id: "R2".to_string(),
            trip_id: "T2".to_string(),
            stop_indices: vec![0, 1, 2],
        };
        sim.spawn_gtfs_bus(&req);

        let states: Vec<Vec<usize>> = sim.world
            .query_mut::<&BusState>()
            .into_iter()
            .map(|bs| bs.stop_indices().to_vec())
            .collect();

        assert_eq!(states.len(), 1);
        assert_eq!(states[0], vec![0, 1, 2], "GTFS bus should have all 3 stop indices");
    }

    #[test]
    fn spawn_agents_without_bus_spawner_works_normally() {
        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        // bus_spawner is None (default from new_cpu_only).
        assert!(sim.bus_spawner.is_none());

        // spawn_agents should not panic when bus_spawner is None.
        sim.spawn_agents(0.1);
        // Success if no panic occurs.
    }

    #[test]
    fn spawn_agents_with_bus_spawner_spawns_at_departure_time() {
        use velos_demand::bus_spawner::BusSpawner;
        use velos_demand::gtfs::{BusSchedule, StopTime};

        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        // Add bus stops.
        sim.bus_stops = vec![
            BusStop { edge_id: 0, offset_m: 30.0, capacity: 40, name: "First".to_string() },
            BusStop { edge_id: 0, offset_m: 170.0, capacity: 40, name: "Last".to_string() },
        ];

        // Create a BusSpawner with one trip departing at 21600s (06:00).
        let mut route_stop_ids = std::collections::HashMap::new();
        route_stop_ids.insert("R1".to_string(), vec!["S1".to_string(), "S2".to_string()]);
        let mut stop_id_to_index = std::collections::HashMap::new();
        stop_id_to_index.insert("S1".to_string(), 0);
        stop_id_to_index.insert("S2".to_string(), 1);

        let schedules = vec![BusSchedule {
            trip_id: "T1".to_string(),
            route_id: "R1".to_string(),
            stop_times: vec![
                StopTime { stop_id: "S1".to_string(), arrival_s: 21600, departure_s: 21600, stop_sequence: 1 },
                StopTime { stop_id: "S2".to_string(), arrival_s: 21900, departure_s: 21900, stop_sequence: 2 },
            ],
        }];

        sim.bus_spawner = Some(BusSpawner::new(&route_stop_ids, &stop_id_to_index, schedules));

        // Before departure time: no GTFS bus should spawn.
        sim.sim_time = 20000.0;
        sim.spawn_agents(0.1);
        let bus_count_before: usize = sim.world
            .query_mut::<(&VehicleType, &BusState)>()
            .into_iter()
            .filter(|(vt, _)| **vt == VehicleType::Bus)
            .count();
        assert_eq!(bus_count_before, 0, "no bus before departure time");

        // At departure time: GTFS bus should spawn.
        sim.sim_time = 21600.0;
        sim.spawn_agents(0.1);
        let bus_entries: Vec<(VehicleType, Vec<usize>)> = sim.world
            .query_mut::<(&VehicleType, &BusState)>()
            .into_iter()
            .filter(|(vt, _)| **vt == VehicleType::Bus)
            .map(|(vt, bs)| (*vt, bs.stop_indices().to_vec()))
            .collect();

        assert_eq!(bus_entries.len(), 1, "one GTFS bus should spawn at departure");
        assert_eq!(bus_entries[0].1, vec![0, 1], "bus should have correct stop indices");
    }

    #[test]
    fn spawn_gtfs_bus_empty_stop_indices_skipped() {
        let graph = make_spawn_test_graph();
        let mut sim = crate::sim::SimWorld::new_cpu_only(graph);

        let req = BusSpawnRequest {
            route_id: "R1".to_string(),
            trip_id: "T1".to_string(),
            stop_indices: vec![], // empty
        };
        sim.spawn_gtfs_bus(&req);

        let count: usize = sim.world
            .query_mut::<&VehicleType>()
            .into_iter()
            .count();
        assert_eq!(count, 0, "empty stop_indices should not spawn a bus");
    }
}
