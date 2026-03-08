//! Helper methods for SimWorld: signal checks, leader detection, state updates.

use std::collections::HashMap;

use hecs::Entity;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use velos_core::components::{
    CarFollowingModel, Kinematics, LaneChangeState, LateralOffset, Position, RoadPosition, Route,
    VehicleType, WaitState,
};
use velos_meso::queue_model::MesoVehicle;
use velos_meso::zone_config::ZoneType;
use velos_signal::plan::PhaseState;
use velos_vehicle::idm::IdmParams;

use crate::sim::SimWorld;
use crate::sim_meso::MesoAgentState;

impl SimWorld {
    pub(crate) fn check_signal_red(&self, rp: &RoadPosition) -> bool {
        let edge_idx = EdgeIndex::new(rp.edge_index as usize);
        let g = self.road_graph.inner();

        let Some(edge_endpoints) = g.edge_endpoints(edge_idx) else {
            return false;
        };
        let target_node = edge_endpoints.1;

        let edge_length = g
            .edge_weight(edge_idx)
            .map(|e| e.length_m)
            .unwrap_or(100.0);

        if rp.offset_m < edge_length - 15.0 {
            return false;
        }

        let target_node_u32 = target_node.index() as u32;
        if !self.signalized_nodes.contains_key(&target_node_u32) {
            return false;
        }

        for (ctrl_node, ctrl) in &self.signal_controllers {
            if *ctrl_node == target_node {
                let incoming: Vec<_> =
                    g.edges_directed(target_node, Direction::Incoming).collect();
                for (approach_idx, edge_ref) in incoming.iter().enumerate() {
                    if edge_ref.id() == edge_idx {
                        return ctrl.get_phase_state(approach_idx) == PhaseState::Red;
                    }
                }
                return false;
            }
        }
        false
    }

    pub(crate) fn find_leader_static(
        entity: Entity,
        rp: &RoadPosition,
        edge_agents: &HashMap<u32, Vec<(Entity, f64)>>,
        speed_map: &HashMap<Entity, f64>,
        own_speed: f64,
    ) -> (f64, f64) {
        let Some(agents_on_edge) = edge_agents.get(&rp.edge_index) else {
            return (1000.0, 0.0);
        };

        let own_offset = rp.offset_m;
        let mut closest_gap = 1000.0_f64;
        let mut closest_delta_v = 0.0_f64;

        for (other_entity, other_offset) in agents_on_edge {
            if *other_entity == entity {
                continue;
            }
            let gap = other_offset - own_offset;
            if gap > 0.0 && gap < closest_gap {
                let leader_speed = speed_map.get(other_entity).copied().unwrap_or(0.0);
                closest_gap = gap;
                closest_delta_v = own_speed - leader_speed;
            }
        }

        (closest_gap, closest_delta_v)
    }

    /// Shared update logic: apply new offset, handle edge transitions, update state.
    pub(crate) fn apply_vehicle_update(
        &mut self,
        entity: Entity,
        v_new: f64,
        new_offset: f64,
        at_red: bool,
    ) {
        let edge_idx_val = {
            let Ok(rp) = self.world.query_one_mut::<&RoadPosition>(entity) else {
                return;
            };
            rp.edge_index
        };

        let edge_idx = EdgeIndex::new(edge_idx_val as usize);
        let edge_length = self
            .road_graph
            .inner()
            .edge_weight(edge_idx)
            .map(|e| e.length_m)
            .unwrap_or(100.0);

        if new_offset >= edge_length {
            if at_red {
                let rp = self.world.query_one_mut::<&mut RoadPosition>(entity).unwrap();
                rp.offset_m = edge_length - 0.1;
                self.update_agent_state(entity, 0.0);
                self.update_wait_state(entity, 0.0, true);
            } else {
                self.advance_to_next_edge(entity, new_offset - edge_length);
                self.update_agent_state(entity, v_new);
                self.update_wait_state(entity, v_new, false);
            }
        } else {
            let rp = self.world.query_one_mut::<&mut RoadPosition>(entity).unwrap();
            rp.offset_m = new_offset;
            self.update_agent_state(entity, v_new);
            self.update_wait_state(entity, v_new, at_red);
        }
    }

    pub(crate) fn advance_to_next_edge(&mut self, entity: Entity, overflow: f64) {
        let next_info = {
            let (route, _rp) = self
                .world
                .query_one_mut::<(&Route, &RoadPosition)>(entity)
                .unwrap();
            if route.current_step + 1 >= route.path.len() {
                None
            } else {
                let from = NodeIndex::new(route.path[route.current_step] as usize);
                let to = NodeIndex::new(route.path[route.current_step + 1] as usize);
                let edge = self
                    .road_graph
                    .inner()
                    .find_edge(from, to)
                    .map(|e| e.index() as u32);
                Some((edge, route.current_step + 1))
            }
        };

        match next_info {
            Some((Some(next_edge_id), new_step)) => {
                // Check for micro-to-meso transition.
                if self.meso_enabled
                    && self.zone_config.zone_type(next_edge_id) == ZoneType::Meso
                {
                    self.enter_meso_zone(entity, next_edge_id, new_step);
                    return;
                }

                let (route, rp) = self
                    .world
                    .query_one_mut::<(&mut Route, &mut RoadPosition)>(entity)
                    .unwrap();
                route.current_step = new_step;
                rp.edge_index = next_edge_id;
                rp.offset_m = overflow;
                rp.lane = 0; // reset lane on new edge
                // Cancel any in-progress car lane change on edge transition.
                // Only remove LaneChangeState (not LateralOffset -- motorbikes keep theirs).
                let _ = self.world.remove::<(LaneChangeState,)>(entity);
            }
            _ => {
                let route = self.world.query_one_mut::<&mut Route>(entity).unwrap();
                route.current_step = route.path.len();
            }
        }
    }

    pub(crate) fn update_agent_state(&mut self, entity: Entity, new_speed: f64) {
        let edge_info = {
            let rp = self.world.query_one_mut::<&RoadPosition>(entity).unwrap();
            let edge_idx = EdgeIndex::new(rp.edge_index as usize);
            let g = self.road_graph.inner();
            g.edge_weight(edge_idx).map(|e| {
                let geom = &e.geometry;
                let frac = (rp.offset_m / e.length_m).clamp(0.0, 1.0);
                let start = geom[0];
                let end = geom[geom.len() - 1];
                let x = start[0] + (end[0] - start[0]) * frac;
                let y = start[1] + (end[1] - start[1]) * frac;
                let heading = (end[1] - start[1]).atan2(end[0] - start[0]);
                (x, y, heading)
            })
        };

        if let Some((x, y, heading)) = edge_info {
            let (pos, kin) = self
                .world
                .query_one_mut::<(&mut Position, &mut Kinematics)>(entity)
                .unwrap();
            pos.x = x;
            pos.y = y;
            kin.speed = new_speed;
            kin.heading = heading;
            kin.vx = new_speed * heading.cos();
            kin.vy = new_speed * heading.sin();
        }
    }

    /// Offset world position perpendicular to heading by lateral offset.
    pub(crate) fn apply_lateral_world_offset(&mut self, entity: Entity, lateral_offset: f64) {
        let heading = {
            let kin = self.world.query_one_mut::<&Kinematics>(entity).unwrap();
            kin.heading
        };
        let edge_info = {
            let rp = self.world.query_one_mut::<&RoadPosition>(entity).unwrap();
            let edge_idx = EdgeIndex::new(rp.edge_index as usize);
            self.road_graph
                .inner()
                .edge_weight(edge_idx)
                .map(|e| e.lane_count as f64 * 3.5 / 2.0)
        };
        let road_half_width = edge_info.unwrap_or(3.5);
        let offset_from_center = lateral_offset - road_half_width;

        let perp_x = -heading.sin();
        let perp_y = heading.cos();

        let pos = self.world.query_one_mut::<&mut Position>(entity).unwrap();
        pos.x += offset_from_center * perp_x;
        pos.y += offset_from_center * perp_y;
    }

    /// Find the nearest leader (ahead) in a specific lane on a specific edge.
    /// Returns (gap_m, leader_speed).
    pub(crate) fn find_leader_in_lane(
        own_offset: f64,
        edge_index: u32,
        lane: u8,
        edge_lane_agents: &HashMap<(u32, u8), Vec<(Entity, f64)>>,
        speed_map: &HashMap<Entity, f64>,
    ) -> (f64, f64) {
        let Some(agents) = edge_lane_agents.get(&(edge_index, lane)) else {
            return (1000.0, 0.0);
        };
        let mut closest_gap = 1000.0_f64;
        let mut leader_speed = 0.0_f64;
        for (entity, offset) in agents {
            let gap = offset - own_offset;
            if gap > 0.0 && gap < closest_gap {
                closest_gap = gap;
                leader_speed = speed_map.get(entity).copied().unwrap_or(0.0);
            }
        }
        (closest_gap, leader_speed)
    }

    /// Find the nearest follower (behind) in a specific lane on a specific edge.
    /// Returns (gap_m, follower_speed).
    pub(crate) fn find_follower_in_lane(
        own_offset: f64,
        edge_index: u32,
        lane: u8,
        edge_lane_agents: &HashMap<(u32, u8), Vec<(Entity, f64)>>,
        speed_map: &HashMap<Entity, f64>,
    ) -> (f64, f64) {
        let Some(agents) = edge_lane_agents.get(&(edge_index, lane)) else {
            return (1000.0, 0.0);
        };
        let mut closest_gap = 1000.0_f64;
        let mut follower_speed = 0.0_f64;
        for (entity, offset) in agents {
            let gap = own_offset - offset;
            if gap > 0.0 && gap < closest_gap {
                closest_gap = gap;
                follower_speed = speed_map.get(entity).copied().unwrap_or(0.0);
            }
        }
        (closest_gap, follower_speed)
    }

    pub(crate) fn update_wait_state(&mut self, entity: Entity, speed: f64, at_red: bool) {
        let ws = self.world.query_one_mut::<&mut WaitState>(entity).unwrap();
        ws.at_red_signal = at_red;
        if speed < 0.1 {
            if ws.stopped_since < 0.0 {
                ws.stopped_since = self.sim_time;
            }
        } else {
            ws.stopped_since = -1.0;
        }
    }

    /// Transfer an agent from micro simulation into a meso SpatialQueue.
    ///
    /// Preserves agent identity (Route, VehicleType, IdmParams, etc.) in
    /// MesoAgentState for reconstruction on meso exit. The ECS entity is
    /// despawned by marking the route as complete (triggers remove_finished_agents).
    fn enter_meso_zone(&mut self, entity: Entity, meso_edge_id: u32, step_at_meso: usize) {
        // Extract agent state before despawning.
        let agent_state = {
            let Ok((route, vtype, idm, cf_model, lat)) = self.world.query_one_mut::<(
                &Route,
                &VehicleType,
                &IdmParams,
                Option<&CarFollowingModel>,
                Option<&LateralOffset>,
            )>(entity)
            else {
                return;
            };

            let mut preserved_route = route.clone();
            preserved_route.current_step = step_at_meso;

            MesoAgentState {
                route: preserved_route,
                vehicle_type: *vtype,
                idm_params: *idm,
                cf_model: cf_model.copied().unwrap_or(CarFollowingModel::Idm),
                lateral_offset: lat.map(|l| l.lateral_offset),
            }
        };

        let vehicle_id = entity.id();

        // Determine exit edge (next edge after meso edge in route).
        let exit_edge = {
            let route = &agent_state.route;
            if route.current_step + 1 < route.path.len() {
                let from = NodeIndex::new(route.path[route.current_step] as usize);
                let to = NodeIndex::new(route.path[route.current_step + 1] as usize);
                self.road_graph
                    .inner()
                    .find_edge(from, to)
                    .map(|e| e.index() as u32)
                    .unwrap_or(0)
            } else {
                0 // Last edge in route; will be handled on meso exit
            }
        };

        // Insert into SpatialQueue.
        if let Some(queue) = self.meso_queues.get_mut(&meso_edge_id) {
            let meso_vehicle = MesoVehicle::new(vehicle_id, self.sim_time, exit_edge);
            queue.enter(meso_vehicle);
        }

        // Preserve state for reconstruction on meso exit.
        self.meso_agent_states.insert(vehicle_id, agent_state);

        // Mark route as complete to trigger despawn in remove_finished_agents().
        if let Ok(route) = self.world.query_one_mut::<&mut Route>(entity) {
            route.current_step = route.path.len();
        }

        log::debug!(
            "Agent {} entering meso zone on edge {}",
            vehicle_id,
            meso_edge_id
        );
    }
}
