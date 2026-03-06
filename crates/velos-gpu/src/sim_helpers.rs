//! Helper methods for SimWorld: signal checks, leader detection, state updates.

use std::collections::HashMap;

use hecs::Entity;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use velos_core::components::{Kinematics, Position, RoadPosition, Route, WaitState};
use velos_signal::plan::PhaseState;

use crate::sim::SimWorld;

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
            Some((Some(edge_idx), new_step)) => {
                let (route, rp) = self
                    .world
                    .query_one_mut::<(&mut Route, &mut RoadPosition)>(entity)
                    .unwrap();
                route.current_step = new_step;
                rp.edge_index = edge_idx;
                rp.offset_m = overflow;
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
}
