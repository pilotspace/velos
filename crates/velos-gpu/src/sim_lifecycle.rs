//! Agent spawning, gridlock detection, removal, and metrics for SimWorld.

use std::collections::HashMap;

use hecs::Entity;
use petgraph::graph::{EdgeIndex, NodeIndex};
use rand::Rng;

use velos_core::components::{
    CarFollowingModel, Kinematics, LateralOffset, Position, RoadPosition, Route, VehicleType,
    WaitState,
};
use velos_demand::SpawnVehicleType;
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
            VehicleType::Pedestrian => velos_vehicle::types::VehicleType::Pedestrian,
        };
        let idm_params = default_idm_params(vehicle_type_for_params);

        // Determine car-following model per agent.
        // Motorbikes: always IDM (sublane model is IDM-based).
        // Cars: ~30% Krauss, ~70% IDM (RNG-based; full demand-config-driven
        // assignment will be wired when demand config is extended in Phase 6).
        // Pedestrians: no CarFollowingModel component (skip car-following entirely).
        let cf_model = match vtype {
            VehicleType::Motorbike => Some(CarFollowingModel::Idm),
            VehicleType::Car => {
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

        if vtype == VehicleType::Motorbike {
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
            ));
        } else if vtype == VehicleType::Car {
            // Cars get LateralOffset at lane 0 center so they render at correct lane position.
            let initial_lateral = (0.0 + 0.5) * 3.5; // lane 0 center = 1.75m
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
            ));
        } else {
            // Pedestrians: no CarFollowingModel (they use social force, not car-following).
            self.world.spawn(base_components);
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
                VehicleType::Motorbike => motorbike_count += 1,
                VehicleType::Car => car_count += 1,
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
