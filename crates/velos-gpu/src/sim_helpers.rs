//! Helper methods for SimWorld: signal checks, leader detection, state updates.

use std::collections::HashMap;

use hecs::Entity;
use petgraph::graph::{EdgeIndex, NodeIndex};
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use velos_core::components::{
    CarFollowingModel, JunctionTraversal, Kinematics, LaneChangeState, LateralOffset, Position,
    RoadPosition, Route, VehicleType, WaitState,
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
    ///
    /// Bug 2 fix: After advance_to_next_edge, if the agent entered a junction
    /// (has JunctionTraversal), do NOT call update_agent_state (it overwrites
    /// the Bezier position with stale edge coordinates).
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
                let blocked = self.advance_to_next_edge(entity, new_offset - edge_length);
                if blocked {
                    // Bug 3: agent blocked at junction entry -- speed already zeroed
                    self.update_agent_state(entity, 0.0);
                    self.update_wait_state(entity, 0.0, false);
                } else if self
                    .world
                    .query_one_mut::<&JunctionTraversal>(entity)
                    .is_ok()
                {
                    // Bug 2 fix: Agent entered junction -- Position already set to Bezier(t=0)
                    // Do NOT call update_agent_state (it overwrites with stale edge coords)
                    self.update_wait_state(entity, v_new, false);
                } else {
                    self.update_agent_state(entity, v_new);
                    self.update_wait_state(entity, v_new, false);
                }
            }
        } else {
            let rp = self.world.query_one_mut::<&mut RoadPosition>(entity).unwrap();
            rp.offset_m = new_offset;
            self.update_agent_state(entity, v_new);
            self.update_wait_state(entity, v_new, at_red);
        }
    }

    /// Advance an agent to the next edge in its route, or enter junction traversal.
    ///
    /// Returns `true` if the agent is blocked (gap acceptance failed at junction entry).
    /// When blocked, the agent's speed is zeroed and offset clamped to edge end (Bug 3 fix).
    ///
    /// When entering a junction, attaches `JunctionTraversal` and immediately sets
    /// the agent's Position to Bezier(t=0) to prevent one-frame stale position (Bug 1 fix).
    pub(crate) fn advance_to_next_edge(&mut self, entity: Entity, overflow: f64) -> bool {
        let next_info = {
            let (route, _rp) = self
                .world
                .query_one_mut::<(&Route, &RoadPosition)>(entity)
                .unwrap();
            if route.current_step + 2 >= route.path.len() {
                None // No more edges — route complete
            } else {
                // NEXT edge: from the node we're arriving at to the one after.
                // path[step] → path[step+1] is the CURRENT edge (already traversed).
                // path[step+1] → path[step+2] is the NEXT edge we're advancing to.
                let junction_node = NodeIndex::new(route.path[route.current_step + 1] as usize);
                let next_node = NodeIndex::new(route.path[route.current_step + 2] as usize);
                let next_edge = self
                    .road_graph
                    .inner()
                    .find_edge(junction_node, next_node)
                    .map(|e| e.index() as u32);
                // The target node is the one the agent is arriving at (potential junction)
                let target_node_u32 = route.path[route.current_step + 1];
                Some((next_edge, route.current_step + 1, target_node_u32))
            }
        };

        match next_info {
            Some((Some(next_edge_id), new_step, target_node_u32)) => {
                // Check for micro-to-meso transition.
                if self.meso_enabled
                    && self.zone_config.zone_type(next_edge_id) == ZoneType::Meso
                {
                    self.enter_meso_zone(entity, next_edge_id, new_step);
                    return false;
                }

                // Check for junction traversal intercept
                if let Some(junction_data) = self.junction_data.get(&target_node_u32) {
                    // Clone internal_edges upfront to avoid borrow conflict
                    // with &mut self in resolve_peripheral_exit_with.
                    let internal_edges = junction_data.internal_edges.clone();

                    // Get current edge to find the matching turn
                    let current_edge_id = {
                        let rp = self.world.query_one_mut::<&RoadPosition>(entity).unwrap();
                        rp.edge_index
                    };

                    // For merged clusters, `next_edge_id` may be an internal edge.
                    // Walk forward through internal edges to find the peripheral
                    // exit edge that the merged junction's turns reference.
                    let (effective_exit_edge, extra_steps) =
                        self.resolve_peripheral_exit_with(
                            entity,
                            next_edge_id,
                            &internal_edges,
                            new_step,
                        );

                    // Re-borrow junction data after mutable self usage
                    let junction_data = self.junction_data.get(&target_node_u32).unwrap();

                    // Find the BezierTurn matching (current_edge -> peripheral_exit)
                    let turn_match = junction_data
                        .turns
                        .iter()
                        .enumerate()
                        .find(|(_, t)| {
                            t.entry_edge == current_edge_id && t.exit_edge == effective_exit_edge
                        });

                    if let Some((turn_index, turn)) = turn_match {
                        // Approach-phase gap acceptance: check if foe agents in junction
                        // would block entry. Uses simplified TTC check.
                        let entry_blocked = self.junction_entry_blocked(
                            entity,
                            target_node_u32,
                            turn_index as u16,
                            junction_data,
                        );

                        if entry_blocked {
                            // Bug 3 fix: zero speed and clamp offset
                            let edge_length = {
                                let rp = self.world.query_one_mut::<&RoadPosition>(entity).unwrap();
                                let edge_idx = EdgeIndex::new(rp.edge_index as usize);
                                self.road_graph
                                    .inner()
                                    .edge_weight(edge_idx)
                                    .map(|e| e.length_m)
                                    .unwrap_or(100.0)
                            };

                            let (rp, kin) = self
                                .world
                                .query_one_mut::<(&mut RoadPosition, &mut Kinematics)>(entity)
                                .unwrap();
                            rp.offset_m = edge_length - 0.1;
                            kin.speed = 0.0;
                            kin.vx = 0.0;
                            kin.vy = 0.0;
                            return true; // blocked
                        }

                        // Enter junction traversal
                        let lateral_offset = self
                            .world
                            .query_one_mut::<&LateralOffset>(entity)
                            .map(|lo| lo.lateral_offset)
                            .unwrap_or(0.0);

                        let speed = self
                            .world
                            .query_one_mut::<&Kinematics>(entity)
                            .map(|k| k.speed)
                            .unwrap_or(0.0);

                        // Compute initial t: start at entry_t (where curve passes
                        // through junction centroid) plus overflow from edge end.
                        // Cap advancement beyond entry_t to prevent teleporting
                        // through the junction when overflow is large.
                        let t_advance = (overflow / turn.arc_length.max(1.0)).min(0.15);
                        let initial_t = (turn.entry_t + t_advance).min(0.99);

                        // Attach JunctionTraversal component.
                        // For merged clusters, advance the route past internal
                        // edges so the agent's route step points to the
                        // peripheral exit node (not an internal cluster node).
                        let _ = self.world.insert_one(
                            entity,
                            JunctionTraversal {
                                junction_node: target_node_u32,
                                turn_index: turn_index as u16,
                                t: initial_t,
                                lateral_offset,
                                speed,
                                wait_ticks: 0,
                            },
                        );
                        if extra_steps > 0 {
                            if let Ok(route) =
                                self.world.query_one_mut::<&mut Route>(entity)
                            {
                                route.current_step += extra_steps;
                            }
                        }

                        // Immediately set Position to Bezier(initial_t) so there
                        // is no one-frame stale position at edge-end coordinates.
                        let entry_edge_idx = EdgeIndex::new(turn.entry_edge as usize);
                        let road_half_width = self
                            .road_graph
                            .inner()
                            .edge_weight(entry_edge_idx)
                            .map(|e| e.lane_count as f64 * 3.5 / 2.0)
                            .unwrap_or(3.5);

                        let pos =
                            turn.offset_position(initial_t, lateral_offset, road_half_width);
                        let tan = turn.tangent(initial_t);
                        let heading = tan[1].atan2(tan[0]);

                        if let Ok((position, kin)) = self
                            .world
                            .query_one_mut::<(&mut Position, &mut Kinematics)>(entity)
                        {
                            position.x = pos[0];
                            position.y = pos[1];
                            kin.heading = heading;
                            kin.vx = speed * heading.cos();
                            kin.vy = speed * heading.sin();
                        }

                        // Cancel any in-progress lane change
                        let _ = self.world.remove::<(LaneChangeState,)>(entity);

                        return false; // entered junction successfully
                    }
                }

                // No junction data or no matching turn -- proceed with instant transition
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
                false
            }
            _ => {
                let route = self.world.query_one_mut::<&mut Route>(entity).unwrap();
                route.current_step = route.path.len();
                false
            }
        }
    }

    /// Walk forward through internal edges of a merged cluster to find the
    /// first peripheral exit edge.
    ///
    /// Returns `(peripheral_exit_edge_id, extra_route_steps)` where
    /// `extra_route_steps` is the number of internal edges skipped (0 if
    /// `candidate_edge` is already peripheral).
    /// Walk forward through internal edges of a merged cluster to find the
    /// first peripheral exit edge.
    ///
    /// Returns `(peripheral_exit_edge_id, extra_route_steps)` where
    /// `extra_route_steps` is the number of internal edges skipped (0 if
    /// `candidate_edge` is already peripheral).
    ///
    /// Takes `internal_edges` by reference (borrowed from junction data before
    /// calling) to avoid borrow conflicts with `&mut self`.
    fn resolve_peripheral_exit_with(
        &mut self,
        entity: Entity,
        candidate_edge: u32,
        internal_edges: &std::collections::HashSet<u32>,
        base_step: usize,
    ) -> (u32, usize) {
        if !internal_edges.contains(&candidate_edge) {
            return (candidate_edge, 0);
        }

        // Read the route path to walk forward
        let route_path: Vec<u32> = match self.world.query_one_mut::<&Route>(entity) {
            Ok(r) => r.path.clone(),
            Err(_) => return (candidate_edge, 0),
        };

        let g = self.road_graph.inner();
        let mut edge_id = candidate_edge;
        let mut steps = 0;
        let max_walk = 5; // safety limit
        let mut walk_step = base_step;

        while steps < max_walk {
            if walk_step + 2 >= route_path.len() {
                break;
            }
            let from = NodeIndex::new(route_path[walk_step + 1] as usize);
            let to = NodeIndex::new(route_path[walk_step + 2] as usize);
            let next_edge = match g.find_edge(from, to) {
                Some(e) => e.index() as u32,
                None => break,
            };

            steps += 1;
            walk_step += 1;
            edge_id = next_edge;

            if !internal_edges.contains(&next_edge) {
                return (next_edge, steps);
            }
        }

        (edge_id, steps)
    }

    /// Check if entering a junction is blocked by foe agents already inside.
    ///
    /// Simplified approach-phase check: if any foe agent is in the same junction
    /// on a crossing turn and near the conflict point, block entry.
    fn junction_entry_blocked(
        &self,
        _entity: Entity,
        junction_node: u32,
        own_turn_idx: u16,
        junction_data: &velos_net::junction::JunctionData,
    ) -> bool {
        // Check each conflict involving our turn
        for cp in &junction_data.conflicts {
            let foe_turn_idx = if cp.turn_a_idx == own_turn_idx {
                cp.turn_b_idx
            } else if cp.turn_b_idx == own_turn_idx {
                cp.turn_a_idx
            } else {
                continue;
            };

            let foe_cross_t = if cp.turn_a_idx == own_turn_idx {
                cp.t_b as f64
            } else {
                cp.t_a as f64
            };

            // Check if any foe agent is in the junction on the crossing turn
            // and near the conflict point (approach-phase gap acceptance)
            for (jt,) in self.world.query::<(&JunctionTraversal,)>().iter() {
                if jt.junction_node != junction_node || jt.turn_index != foe_turn_idx {
                    continue;
                }
                // Foe is on the crossing turn -- check if near conflict point
                let foe_dist_to_cross = (jt.t - foe_cross_t).abs();
                if foe_dist_to_cross < 0.3 {
                    // Foe is near the conflict point -- block entry
                    return true;
                }
            }
        }

        false
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
    ///
    /// Defense-in-depth: skip junction-traversing agents whose positions are
    /// Bezier-computed. Calling this on them overwrites correct junction
    /// positions with stale edge-based coordinates, causing flickering.
    pub(crate) fn apply_lateral_world_offset(&mut self, entity: Entity, lateral_offset: f64) {
        // Skip junction-traversing agents — their position is Bezier-computed
        if self
            .world
            .query_one_mut::<&JunctionTraversal>(entity)
            .is_ok()
        {
            return;
        }
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

    /// Apply GLOSA advisory speed reduction to agents approaching non-green signals.
    ///
    /// For each signalized intersection, queries agents on incoming edges within
    /// broadcast range (200m). If the signal is red/amber, computes the optimal
    /// approach speed via `glosa_speed()`. Speeds below 3.0 m/s are ignored
    /// (agent will stop and wait instead).
    ///
    /// Called between step_signal_priority (step 4) and step_perception (step 5)
    /// in both tick_gpu() and tick() pipelines.
    pub(crate) fn step_glosa(&mut self) {
        use velos_signal::spat::{broadcast_range_m, glosa_speed};

        let range = broadcast_range_m();
        let g = self.road_graph.inner();

        // Collect advisories to avoid borrow conflict with self.world
        struct GlosaAdvisory {
            entity: Entity,
            speed: f64,
        }
        let mut advisories: Vec<GlosaAdvisory> = Vec::new();

        for (node, ctrl) in &self.signal_controllers {
            let node_id = node.index() as u32;
            let incoming_edges = match self.signalized_nodes.get(&node_id) {
                Some(edges) => edges,
                None => continue,
            };

            let num_approaches = incoming_edges.len();
            let spat = ctrl.spat_data(num_approaches);

            for (approach_idx, edge_idx) in incoming_edges.iter().enumerate() {
                let phase = spat
                    .approach_states
                    .get(approach_idx)
                    .copied()
                    .unwrap_or(PhaseState::Green);
                if phase == PhaseState::Green {
                    continue;
                }

                let edge_length = g
                    .edge_weight(*edge_idx)
                    .map(|e| e.length_m)
                    .unwrap_or(100.0);

                for (entity, rp, kin) in self
                    .world
                    .query::<(hecs::Entity, &RoadPosition, &Kinematics)>()
                    .iter()
                {
                    if rp.edge_index != edge_idx.index() as u32 {
                        continue;
                    }
                    let distance = edge_length - rp.offset_m;
                    if distance <= 0.0 || distance > range {
                        continue;
                    }

                    let v_max = kin.speed.max(13.89); // current or 50 km/h default
                    let advisory = glosa_speed(distance, spat.time_to_next_change, v_max);
                    if advisory >= 3.0 && advisory < kin.speed {
                        advisories.push(GlosaAdvisory {
                            entity,
                            speed: advisory,
                        });
                    }
                }
            }
        }

        // Apply advisories
        for adv in &advisories {
            if let Ok(kin) = self.world.query_one_mut::<&mut Kinematics>(adv.entity) {
                kin.speed = kin.speed.min(adv.speed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::graph::DiGraph;
    use velos_core::components::{
        Kinematics, Position, RoadPosition, Route, VehicleType, WaitState,
    };
    use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
    use velos_signal::plan::PhaseState;
    use velos_signal::SignalController;
    use velos_vehicle::idm::IdmParams;

    /// A mock signal controller that returns a fixed phase state for all approaches.
    struct MockSignalController {
        phase: PhaseState,
        time_to_green: f64,
        cycle_time: f64,
    }

    impl MockSignalController {
        fn red(time_to_green: f64) -> Self {
            Self {
                phase: PhaseState::Red,
                time_to_green,
                cycle_time: 60.0,
            }
        }

        fn green() -> Self {
            Self {
                phase: PhaseState::Green,
                time_to_green: 0.0,
                cycle_time: 60.0,
            }
        }
    }

    impl SignalController for MockSignalController {
        fn tick(&mut self, _dt: f64, _detector_readings: &[velos_signal::detector::DetectorReading]) {}

        fn get_phase_state(&self, _approach_index: usize) -> PhaseState {
            self.phase
        }

        fn reset(&mut self) {}

        fn spat_data(&self, num_approaches: usize) -> velos_signal::spat::SpatBroadcast {
            velos_signal::spat::SpatBroadcast {
                approach_states: vec![self.phase; num_approaches],
                time_to_next_change: self.time_to_green,
                cycle_time: self.cycle_time,
            }
        }
    }

    /// Build a graph: A --edge0--> B --edge1--> C <--edge2-- D <--edge3-- E
    /// plus F-->C, G-->C to give node C 4 incoming edges (signalized).
    fn make_glosa_test_graph() -> (RoadGraph, NodeIndex, Vec<EdgeIndex>) {
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [300.0, 0.0] });
        let c = g.add_node(RoadNode { pos: [600.0, 0.0] }); // signalized intersection
        let d = g.add_node(RoadNode { pos: [600.0, 300.0] });
        let e = g.add_node(RoadNode { pos: [600.0, 600.0] });
        let f = g.add_node(RoadNode { pos: [900.0, 0.0] });
        let gg = g.add_node(RoadNode { pos: [300.0, 300.0] });

        let make_edge = |len: f64| RoadEdge {
            length_m: len,
            speed_limit_mps: 13.9,
            lane_count: 2,
            oneway: true,
            road_class: RoadClass::Primary,
            geometry: vec![[0.0, 0.0], [len, 0.0]],
            motorbike_only: false,
            time_windows: None,
        };

        let _e0 = g.add_edge(a, b, make_edge(300.0));
        let e1 = g.add_edge(b, c, make_edge(300.0)); // incoming to C
        let e2 = g.add_edge(d, c, make_edge(300.0)); // incoming to C
        let e3 = g.add_edge(e, d, make_edge(300.0));
        let e4 = g.add_edge(f, c, make_edge(300.0)); // incoming to C
        let e5 = g.add_edge(gg, c, make_edge(300.0)); // incoming to C (4th)

        let _ = e3; // suppress unused

        let incoming = vec![e1, e2, e4, e5];
        let road_graph = RoadGraph::new(g);
        (road_graph, c, incoming)
    }

    fn spawn_test_agent(
        sim: &mut SimWorld,
        edge_index: u32,
        offset_m: f64,
        speed: f64,
    ) -> Entity {
        sim.world.spawn((
            Position { x: 0.0, y: 0.0 },
            Kinematics {
                vx: speed,
                vy: 0.0,
                speed,
                heading: 0.0,
            },
            VehicleType::Car,
            RoadPosition {
                edge_index,
                lane: 0,
                offset_m,
            },
            Route {
                path: vec![0, 1],
                current_step: 0,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            IdmParams {
                v0: 13.89,
                s0: 2.0,
                t_headway: 1.5,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
            },
        ))
    }

    #[test]
    fn step_glosa_reduces_speed_for_agent_near_red_signal() {
        let (graph, signal_node, incoming) = make_glosa_test_graph();
        let mut sim = SimWorld::new_cpu_only(graph);

        // Replace signal controllers with our mock red controller
        let signalized_edges: Vec<EdgeIndex> = incoming.clone();
        sim.signal_controllers = vec![(
            signal_node,
            Box::new(MockSignalController::red(10.0)), // 10s to green
        )];
        sim.signalized_nodes
            .insert(signal_node.index() as u32, signalized_edges);

        // Place agent 150m from signal on the first incoming edge
        let edge_idx = incoming[0].index() as u32;
        let agent = spawn_test_agent(&mut sim, edge_idx, 150.0, 13.89); // 150m offset on 300m edge = 150m from signal

        sim.step_glosa();

        let kin = sim.world.query_one_mut::<&Kinematics>(agent).unwrap();
        // glosa_speed(150.0, 10.0, 13.89) = 150/10 = 15.0 > 13.89 => 0.0 (cannot make it)
        // Actually 15.0 > 13.89 so returns 0.0, agent won't be affected (advisory < 3.0)
        // Let me adjust: agent at offset 200 => distance = 100m, advisory = 100/10 = 10.0
        // That's < 13.89 and >= 3.0, so should reduce speed
        assert!((kin.speed - 13.89).abs() < 0.01, "Agent at 150m offset: advisory 15 m/s > v_max, no change");

        // Now test with adjusted position: 200m offset => 100m from signal
        let agent2 = spawn_test_agent(&mut sim, edge_idx, 200.0, 13.89);
        sim.step_glosa();
        let kin2 = sim.world.query_one_mut::<&Kinematics>(agent2).unwrap();
        // glosa_speed(100.0, 10.0, 13.89) = 10.0 m/s, which is < 13.89 and >= 3.0
        assert!(
            (kin2.speed - 10.0).abs() < 0.01,
            "Agent at 200m offset (100m from signal): speed should be reduced to 10.0, got {}",
            kin2.speed
        );
    }

    #[test]
    fn step_glosa_no_change_at_green_signal() {
        let (graph, signal_node, incoming) = make_glosa_test_graph();
        let mut sim = SimWorld::new_cpu_only(graph);

        let signalized_edges: Vec<EdgeIndex> = incoming.clone();
        sim.signal_controllers = vec![(
            signal_node,
            Box::new(MockSignalController::green()),
        )];
        sim.signalized_nodes
            .insert(signal_node.index() as u32, signalized_edges);

        let edge_idx = incoming[0].index() as u32;
        let agent = spawn_test_agent(&mut sim, edge_idx, 200.0, 13.89);

        sim.step_glosa();

        let kin = sim.world.query_one_mut::<&Kinematics>(agent).unwrap();
        assert!(
            (kin.speed - 13.89).abs() < 0.01,
            "Green signal: speed should not change, got {}",
            kin.speed
        );
    }

    #[test]
    fn step_glosa_no_change_beyond_broadcast_range() {
        let (graph, signal_node, incoming) = make_glosa_test_graph();
        let mut sim = SimWorld::new_cpu_only(graph);

        let signalized_edges: Vec<EdgeIndex> = incoming.clone();
        sim.signal_controllers = vec![(
            signal_node,
            Box::new(MockSignalController::red(10.0)),
        )];
        sim.signalized_nodes
            .insert(signal_node.index() as u32, signalized_edges);

        // Agent at offset 50 on a 300m edge => distance = 250m > 200m broadcast range
        let edge_idx = incoming[0].index() as u32;
        let agent = spawn_test_agent(&mut sim, edge_idx, 50.0, 13.89);

        sim.step_glosa();

        let kin = sim.world.query_one_mut::<&Kinematics>(agent).unwrap();
        assert!(
            (kin.speed - 13.89).abs() < 0.01,
            "Beyond 200m range: speed should not change, got {}",
            kin.speed
        );
    }

    #[test]
    fn step_glosa_below_minimum_speed_ignored() {
        let (graph, signal_node, incoming) = make_glosa_test_graph();
        let mut sim = SimWorld::new_cpu_only(graph);

        let signalized_edges: Vec<EdgeIndex> = incoming.clone();
        // time_to_green = 100s, so at 100m distance: advisory = 100/100 = 1.0 m/s < 3.0 threshold
        sim.signal_controllers = vec![(
            signal_node,
            Box::new(MockSignalController::red(100.0)),
        )];
        sim.signalized_nodes
            .insert(signal_node.index() as u32, signalized_edges);

        let edge_idx = incoming[0].index() as u32;
        let agent = spawn_test_agent(&mut sim, edge_idx, 200.0, 13.89); // 100m from signal

        sim.step_glosa();

        let kin = sim.world.query_one_mut::<&Kinematics>(agent).unwrap();
        // glosa_speed(100.0, 100.0, 13.89) = 1.0 m/s < 3.0, returns 0.0 => no advisory applied
        assert!(
            (kin.speed - 13.89).abs() < 0.01,
            "Advisory below 3.0 m/s should be ignored, got {}",
            kin.speed
        );
    }
}
