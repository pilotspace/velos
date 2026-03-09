//! Junction traversal frame pipeline step.
//!
//! Step 6.8: Advances agents currently traversing junctions along their
//! Bezier curves with conflict detection and IDM-based yielding.
//! Runs after lane changes (6.7), before GPU vehicle physics (7.0).

use hecs::Entity;
use petgraph::graph::{EdgeIndex, NodeIndex};

use velos_core::components::{
    JunctionTraversal, JustExitedJunction, Kinematics, Position, RoadPosition, Route,
    VehicleType as CoreVehicleType, MAX_YIELD_TICKS,
};
use velos_net::junction::BezierTurn;
use velos_vehicle::idm::IdmParams;
use velos_vehicle::junction_traversal::{
    self, advance_on_bezier, check_conflicts, yield_deceleration, ConflictPoint, MIN_CRAWL_SPEED,
};
use velos_vehicle::types::VehicleType as VehVehicleType;

/// Convert from velos-core VehicleType to velos-vehicle VehicleType.
/// Both enums are identical but defined in separate crates.
fn to_veh_vtype(vt: CoreVehicleType) -> VehVehicleType {
    match vt {
        CoreVehicleType::Motorbike => VehVehicleType::Motorbike,
        CoreVehicleType::Car => VehVehicleType::Car,
        CoreVehicleType::Bus => VehVehicleType::Bus,
        CoreVehicleType::Bicycle => VehVehicleType::Bicycle,
        CoreVehicleType::Truck => VehVehicleType::Truck,
        CoreVehicleType::Emergency => VehVehicleType::Emergency,
        CoreVehicleType::Pedestrian => VehVehicleType::Pedestrian,
    }
}

use crate::sim::SimWorld;

impl SimWorld {
    /// Step 6.8: Advance agents currently traversing junctions.
    ///
    /// For each agent with a JunctionTraversal component:
    /// 1. Advance t along the Bezier curve
    /// 2. Check for conflicts with other agents in the same junction
    /// 3. Apply IDM yielding deceleration if needed
    /// 4. Update Position and Kinematics from Bezier evaluation
    /// 5. Handle junction exit when t >= 1.0 (Bug 4 fix: direct edge placement)
    /// 6. Track wait_ticks for deadlock prevention (Bug 5 fix)
    pub(crate) fn step_junction_traversal(&mut self, dt: f64) {
        // Phase 0: Clear JustExitedJunction markers from previous frame so
        // those agents can be processed by step_vehicles_gpu this frame.
        let exited: Vec<Entity> = self
            .world
            .query_mut::<(Entity, &JustExitedJunction)>()
            .into_iter()
            .map(|(e, _)| e)
            .collect();
        for e in exited {
            let _ = self.world.remove_one::<JustExitedJunction>(e);
        }

        // Phase 1: Collect all junction-traversing agents grouped by junction node
        // to enable conflict detection within each junction.
        struct AgentState {
            entity: Entity,
            junction_node: u32,
            turn_index: u16,
            t: f64,
            lateral_offset: f64,
            speed: f64,
            wait_ticks: u16,
            vtype: CoreVehicleType,
            idm: IdmParams,
        }

        let mut agents: Vec<AgentState> = Vec::new();

        for (entity, jt, vtype, idm) in self
            .world
            .query_mut::<(Entity, &JunctionTraversal, &CoreVehicleType, &IdmParams)>()
            .into_iter()
        {
            agents.push(AgentState {
                entity,
                junction_node: jt.junction_node,
                turn_index: jt.turn_index,
                t: jt.t,
                lateral_offset: jt.lateral_offset,
                speed: jt.speed,
                wait_ticks: jt.wait_ticks,
                vtype: *vtype,
                idm: *idm,
            });
        }

        if agents.is_empty() {
            return;
        }

        // Phase 2: Build per-junction agent lists for conflict detection
        // Uses velos-vehicle VehicleType for check_conflicts compatibility
        let mut junction_agents: std::collections::HashMap<u32, Vec<(u16, f64, VehVehicleType)>> =
            std::collections::HashMap::new();
        for a in &agents {
            junction_agents
                .entry(a.junction_node)
                .or_default()
                .push((a.turn_index, a.t, to_veh_vtype(a.vtype)));
        }

        // Phase 3: Compute updates
        struct JunctionUpdate {
            entity: Entity,
            new_t: f64,
            position: [f64; 2],
            heading: f64,
            effective_speed: f64,
            finished: bool,
            new_wait_ticks: u16,
            /// Distance in metres the agent overshot past Bezier end.
            /// Used for smooth chaining into next junction segment.
            overflow_m: f64,
        }
        let mut updates: Vec<JunctionUpdate> = Vec::new();

        for a in &agents {
            let Some(junction_data) = self.junction_data.get(&a.junction_node) else {
                continue;
            };
            let Some(turn) = junction_data.turns.get(a.turn_index as usize) else {
                continue;
            };

            // --- Determine effective speed BEFORE advancing t ---
            // This prevents the one-frame position jump when a conflict is first detected.
            let mut effective_speed = a.speed;
            let mut new_wait_ticks = a.wait_ticks;

            // Convert velos_net ConflictPoints to velos_vehicle ConflictPoints
            let local_conflicts: Vec<ConflictPoint> = junction_data
                .conflicts
                .iter()
                .map(|cp| ConflictPoint {
                    turn_a_idx: cp.turn_a_idx,
                    turn_b_idx: cp.turn_b_idx,
                    t_a: cp.t_a,
                    t_b: cp.t_b,
                })
                .collect();

            // Check conflicts FIRST using current t (before advance)
            let has_conflict = if let Some(agents_here) = junction_agents.get(&a.junction_node)
                && let Some(conflict_result) = check_conflicts(
                    a.turn_index,
                    a.t,
                    to_veh_vtype(a.vtype),
                    agents_here,
                    &local_conflicts,
                    turn.arc_length,
                    junction_traversal::DEFAULT_T_PROXIMITY,
                )
            {
                let decel = yield_deceleration(
                    a.speed,
                    conflict_result.virtual_leader_gap,
                    conflict_result.virtual_leader_speed,
                    &a.idm,
                );
                effective_speed = (a.speed + decel * dt).max(0.0);
                true
            } else {
                false
            };

            // Free-flow acceleration recovery: when no conflict, accelerate back
            // toward desired speed using IDM free-flow term. Without this, an agent
            // that yielded (speed=0) stays at 0 forever after the conflict clears.
            if !has_conflict && effective_speed < a.idm.v0 {
                let free_accel = a.idm.a * (1.0 - (effective_speed / a.idm.v0).powf(a.idm.delta));
                effective_speed = (effective_speed + free_accel * dt).min(a.idm.v0);
            }

            // Bug 5 fix: deadlock prevention via wait_ticks
            if effective_speed < 0.1 {
                new_wait_ticks = new_wait_ticks.saturating_add(1);
                if new_wait_ticks >= MAX_YIELD_TICKS {
                    effective_speed = effective_speed.max(MIN_CRAWL_SPEED);
                }
            } else {
                new_wait_ticks = 0;
            }

            // Advance t using effective_speed (post-conflict), NOT original speed.
            // This prevents position jumps when conflict detection triggers.
            let (new_t, finished, overflow_m) =
                advance_on_bezier(a.t, effective_speed, turn.arc_length, dt);

            // Compute world position and heading from Bezier curve
            let entry_edge_idx = EdgeIndex::new(turn.entry_edge as usize);
            let road_half_width = self
                .road_graph
                .inner()
                .edge_weight(entry_edge_idx)
                .map(|e| e.lane_count as f64 * 3.5 / 2.0)
                .unwrap_or(3.5);

            let pos = turn.offset_position(new_t, a.lateral_offset, road_half_width);
            let tan = turn.tangent(new_t);
            let heading = tan[1].atan2(tan[0]);

            updates.push(JunctionUpdate {
                entity: a.entity,
                new_t,
                position: pos,
                heading,
                effective_speed,
                finished,
                new_wait_ticks,
                overflow_m,
            });
        }

        // Phase 4: Apply updates
        for upd in updates {
            // Update Position and Kinematics
            if let Ok((pos, kin)) = self
                .world
                .query_one_mut::<(&mut Position, &mut Kinematics)>(upd.entity)
            {
                pos.x = upd.position[0];
                pos.y = upd.position[1];
                kin.heading = upd.heading;
                kin.speed = upd.effective_speed;
                kin.vx = upd.effective_speed * upd.heading.cos();
                kin.vy = upd.effective_speed * upd.heading.sin();
            }

            if upd.finished {
                // Junction exit with multi-segment chaining.
                // Instead of placing on exit edge (which causes teleport when
                // the next node is also a junction), try to chain directly
                // into the next junction up to MAX_CHAIN_DEPTH times.
                // Overflow distance is carried forward for smooth pre-advancement.
                self.handle_junction_exit(upd.entity, upd.effective_speed, upd.overflow_m);
            } else {
                // Update JunctionTraversal component with new t and wait_ticks
                if let Ok(jt) = self
                    .world
                    .query_one_mut::<&mut JunctionTraversal>(upd.entity)
                {
                    jt.t = upd.new_t;
                    jt.speed = upd.effective_speed;
                    jt.wait_ticks = upd.new_wait_ticks;
                }

                // Update wait state for the agent
                self.update_wait_state(upd.entity, upd.effective_speed, false);
            }
        }
    }

    /// Maximum number of consecutive junctions to chain through in one step.
    /// Prevents infinite loops on degenerate graphs.
    const MAX_CHAIN_DEPTH: usize = 3;

    /// Short edge threshold in metres. Edges shorter than this between two
    /// junctions are traversed instantly (chained) to avoid visible teleporting.
    /// Keep low (≤5m) so agents visually traverse normal-length edges between
    /// junctions instead of skipping them — higher values cause teleportation
    /// because overflow_m + edge_length overshoots the next Bezier arc.
    const SHORT_EDGE_THRESHOLD_M: f64 = 5.0;

    /// Maximum t-parameter advancement beyond entry_t when chaining into the
    /// next junction. Prevents agents from appearing near the exit of a junction
    /// they just entered (visual teleportation through the entire curve).
    const MAX_CHAIN_T_ADVANCE: f64 = 0.15;

    /// Handle junction exit with smooth multi-segment chaining.
    ///
    /// When an agent finishes a junction Bezier (t >= 1.0), instead of placing
    /// it on the exit edge and waiting for the next frame to potentially re-enter
    /// another junction (which causes visible teleporting), this method chains
    /// through up to MAX_CHAIN_DEPTH consecutive junctions in one step.
    ///
    /// **Smooth overflow carry-through:** The `overflow_m` parameter is the
    /// distance (metres) the agent travelled past the previous Bezier end. This
    /// distance plus the intermediate edge length is used to pre-advance the
    /// next Bezier's t-parameter, so the agent's position is physically correct
    /// rather than jumping to t=0. This preserves speed and prevents the
    /// "teleport from outer-point to starter-point" flickering.
    fn handle_junction_exit(&mut self, entity: Entity, speed: f64, mut overflow_m: f64) {
        for _chain_depth in 0..Self::MAX_CHAIN_DEPTH {
            // Read current junction traversal info
            let chain_info = {
                let Ok(jt) = self.world.query_one_mut::<&JunctionTraversal>(entity) else {
                    return;
                };
                let junction_node = jt.junction_node;
                let turn_index = jt.turn_index;
                let lateral_offset = jt.lateral_offset;
                let Some(jd) = self.junction_data.get(&junction_node) else {
                    break;
                };
                let Some(turn) = jd.turns.get(turn_index as usize) else {
                    break;
                };
                (turn.exit_edge, turn.exit_offset_m, lateral_offset)
            };

            let (exit_edge_id, exit_offset, lateral_offset) = chain_info;

            // Advance route step for the junction we just exited
            if let Ok(route) = self.world.query_one_mut::<&mut Route>(entity) {
                route.current_step += 1;
            }

            // Check if we can chain to the next junction:
            // 1. Exit edge must be short (<30m)
            // 2. Next node must have junction data
            // 3. There must be a matching turn (exit_edge -> next_next_edge)
            let next_junction =
                self.find_next_junction_chain(entity, exit_edge_id, &mut overflow_m);

            match next_junction {
                Some((next_jn, next_turn_idx, next_turn, next_road_half_width)) => {
                    // Smooth pre-advancement: start at entry_t (where the curve
                    // passes through the junction centroid) plus accumulated overflow.
                    // Cap advancement beyond entry_t to prevent teleporting through
                    // the entire junction when overflow + edge_length >> arc_length.
                    let next_arc = next_turn.arc_length.max(1.0);
                    let t_advance = (overflow_m / next_arc).min(Self::MAX_CHAIN_T_ADVANCE);
                    let initial_t =
                        (next_turn.entry_t + t_advance).min(0.99);
                    let next_overflow = if overflow_m > next_arc {
                        overflow_m - next_arc
                    } else {
                        0.0
                    };
                    overflow_m = next_overflow;

                    // Chain: update JunctionTraversal in-place to next junction
                    if let Ok(jt) = self.world.query_one_mut::<&mut JunctionTraversal>(entity) {
                        jt.junction_node = next_jn;
                        jt.turn_index = next_turn_idx;
                        jt.t = initial_t;
                        // Keep lateral_offset and speed from previous junction
                        jt.wait_ticks = 0;
                    }

                    // Advance route step again (skip the short intermediate edge)
                    if let Ok(route) = self.world.query_one_mut::<&mut Route>(entity) {
                        route.current_step += 1;
                    }

                    // Set position from Bezier at the pre-advanced t
                    let pos =
                        next_turn.offset_position(initial_t, lateral_offset, next_road_half_width);
                    let tan = next_turn.tangent(initial_t);
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

                    // Update RoadPosition to reflect the intermediate edge
                    // (so other systems see the correct edge context)
                    if let Ok(rp) = self.world.query_one_mut::<&mut RoadPosition>(entity) {
                        rp.edge_index = exit_edge_id;
                        rp.offset_m = exit_offset;
                        rp.lane = 0;
                    }

                    // If overflow consumed the entire next Bezier too, continue chaining
                    if initial_t >= 0.99 && next_overflow > 0.0 {
                        continue;
                    }

                    self.update_wait_state(entity, speed, false);
                    return;
                }
                None => {
                    // No chaining possible — normal exit to edge.
                    // Apply remaining overflow as additional offset on the exit edge.
                    // Mark agent so step_vehicles_gpu skips it this frame,
                    // preventing single-frame teleport to the next junction.
                    let _ = self.world.remove_one::<JunctionTraversal>(entity);
                    let _ = self.world.insert_one(entity, JustExitedJunction);

                    if let Ok(rp) = self.world.query_one_mut::<&mut RoadPosition>(entity) {
                        rp.edge_index = exit_edge_id;
                        rp.offset_m = exit_offset + overflow_m;
                        rp.lane = 0;
                    }

                    self.update_agent_state(entity, speed);
                    self.update_wait_state(entity, speed, false);
                    return;
                }
            }
        }

        // Exhausted chain depth — force normal exit with remaining overflow.
        // Also mark so step_vehicles_gpu skips this frame.
        let _ = self.world.insert_one(entity, JustExitedJunction);
        let exit_info = {
            let Ok(jt) = self.world.query_one_mut::<&JunctionTraversal>(entity) else {
                return;
            };
            let Some(jd) = self.junction_data.get(&jt.junction_node) else {
                return;
            };
            let Some(turn) = jd.turns.get(jt.turn_index as usize) else {
                return;
            };
            (turn.exit_edge, turn.exit_offset_m)
        };
        let _ = self.world.remove_one::<JunctionTraversal>(entity);
        if let Ok(rp) = self.world.query_one_mut::<&mut RoadPosition>(entity) {
            rp.edge_index = exit_info.0;
            rp.offset_m = exit_info.1 + overflow_m;
            rp.lane = 0;
        }
        self.update_agent_state(entity, speed);
        self.update_wait_state(entity, speed, false);
    }

    /// Check if the exit edge is short and the next node has a junction to chain into.
    ///
    /// If chaining is possible, adds the intermediate edge length to `overflow_m`
    /// so the caller can pre-advance the next Bezier (smooth transition).
    ///
    /// Returns `Some((next_junction_node, turn_index, BezierTurn, road_half_width))`
    /// if chaining is possible, `None` otherwise.
    fn find_next_junction_chain(
        &mut self,
        entity: Entity,
        exit_edge_id: u32,
        overflow_m: &mut f64,
    ) -> Option<(u32, u16, BezierTurn, f64)> {
        // Check exit edge length
        let exit_edge_idx = EdgeIndex::new(exit_edge_id as usize);
        let g = self.road_graph.inner();
        let edge_weight = g.edge_weight(exit_edge_idx)?;
        if edge_weight.length_m > Self::SHORT_EDGE_THRESHOLD_M {
            return None;
        }

        // NOTE: edge_length is added to overflow_m only AFTER confirming
        // the chain succeeds (see bottom of function). Adding it here
        // would corrupt overflow when the function returns None later.
        let edge_length = edge_weight.length_m;

        // Get next-next node from route
        let route = self.world.query_one_mut::<&Route>(entity).ok()?;
        // After the route step advance, current_step points to the next node.
        // We need current_step + 1 (the node after the short edge).
        if route.current_step + 2 >= route.path.len() {
            return None;
        }
        let next_node_u32 = route.path[route.current_step + 1];

        // Check if next node has junction data
        let next_jd = self.junction_data.get(&next_node_u32)?;

        // Find the next-next edge (from next_node to the node after)
        let next_next_node = NodeIndex::new(route.path[route.current_step + 2] as usize);
        let next_next_edge = g
            .find_edge(NodeIndex::new(next_node_u32 as usize), next_next_node)?
            .index() as u32;

        // Find matching turn: exit_edge -> next_next_edge
        let (turn_idx, turn) = next_jd
            .turns
            .iter()
            .enumerate()
            .find(|(_, t)| t.entry_edge == exit_edge_id && t.exit_edge == next_next_edge)?;

        let road_half_width = edge_weight.lane_count as f64 * 3.5 / 2.0;

        // Chain confirmed — now safe to add intermediate edge distance
        *overflow_m += edge_length;

        Some((next_node_u32, turn_idx as u16, turn.clone(), road_half_width))
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
    use velos_net::junction::{BezierTurn, JunctionData};
    use velos_vehicle::idm::IdmParams;

    fn test_edge(length: f64) -> RoadEdge {
        RoadEdge {
            length_m: length,
            speed_limit_mps: 13.89,
            lane_count: 2,
            oneway: true,
            road_class: RoadClass::Secondary,
            geometry: vec![[0.0, 0.0], [length, 0.0]],
            motorbike_only: false,
            time_windows: None,
        }
    }

    /// Build a T-junction: A->C (edge0), C->B (edge1), D->C (edge2), C->D (edge3)
    /// Junction at C (node 2).
    fn make_junction_graph() -> (RoadGraph, u32) {
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode {
            pos: [200.0, 0.0],
        });
        let c = g.add_node(RoadNode {
            pos: [100.0, 0.0],
        }); // junction
        let d = g.add_node(RoadNode {
            pos: [100.0, 100.0],
        });

        g.add_edge(a, c, test_edge(100.0)); // edge 0
        g.add_edge(c, b, test_edge(100.0)); // edge 1
        g.add_edge(d, c, test_edge(100.0)); // edge 2
        g.add_edge(c, d, test_edge(100.0)); // edge 3

        let graph = RoadGraph::new(g);
        let junction_node = c.index() as u32;
        (graph, junction_node)
    }

    fn make_sim_with_junction() -> (SimWorld, u32, BezierTurn) {
        let (graph, junction_node) = make_junction_graph();
        let mut sim = SimWorld::new_cpu_only(graph);

        // Create junction data for node C
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [100.0, 0.0],
            p2: [200.0, 0.0],
            arc_length: 200.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };

        let jd = JunctionData {
            turns: vec![turn.clone()],
            conflicts: vec![],
        };

        sim.junction_data.insert(junction_node, jd);
        (sim, junction_node, turn)
    }

    fn spawn_junction_agent(
        sim: &mut SimWorld,
        junction_node: u32,
        turn_index: u16,
        t: f64,
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
            RoadPosition {
                edge_index: 0,
                lane: 0,
                offset_m: 99.0,
            },
            Route {
                path: vec![0, 2, 1], // A -> C -> B
                current_step: 0,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            VehicleType::Car,
            IdmParams {
                v0: 13.89,
                s0: 2.0,
                t_headway: 1.5,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
            },
            JunctionTraversal {
                junction_node,
                turn_index,
                t,
                lateral_offset: 3.5,
                speed,
                wait_ticks: 0,
            },
        ))
    }

    #[test]
    fn step_junction_traversal_advances_t() {
        let (mut sim, jn, _turn) = make_sim_with_junction();
        sim.sim_state = crate::sim::SimState::Running;

        let agent = spawn_junction_agent(&mut sim, jn, 0, 0.0, 10.0);

        sim.step_junction_traversal(0.1);

        let jt = sim
            .world
            .query_one_mut::<&JunctionTraversal>(agent)
            .unwrap();
        // dt_param = 0.1 * 10.0 / 200.0 = 0.005
        assert!(jt.t > 0.0, "t should advance");
        assert!((jt.t - 0.005).abs() < 0.001);
    }

    #[test]
    fn step_junction_traversal_updates_position() {
        let (mut sim, jn, _) = make_sim_with_junction();

        let agent = spawn_junction_agent(&mut sim, jn, 0, 0.5, 10.0);

        sim.step_junction_traversal(0.1);

        let pos = sim
            .world
            .query_one_mut::<&Position>(agent)
            .unwrap();
        // Position should be somewhere along the Bezier curve, not at origin
        assert!(
            pos.x != 0.0 || pos.y != 0.0,
            "position should be updated from Bezier"
        );
    }

    #[test]
    fn step_junction_traversal_exit_removes_component() {
        let (mut sim, jn, _) = make_sim_with_junction();

        // Agent near end of curve
        let agent = spawn_junction_agent(&mut sim, jn, 0, 0.99, 100.0);

        sim.step_junction_traversal(0.1);

        // JunctionTraversal should be removed
        let has_jt = sim
            .world
            .query_one_mut::<&JunctionTraversal>(agent)
            .is_ok();
        assert!(!has_jt, "JunctionTraversal should be removed after exit");

        // Agent should be on exit edge (edge 1)
        let rp = sim
            .world
            .query_one_mut::<&RoadPosition>(agent)
            .unwrap();
        assert_eq!(rp.edge_index, 1, "should be on exit edge");
        // offset_m = exit_offset (0.1) + overflow from overshooting the Bezier
        // speed=100, arc=200, dt=0.1 => dt_param=0.05, raw_t=1.04 => overflow=(0.04)*200=8.0
        assert!(rp.offset_m >= 0.1, "should be at or past exit_offset_m, got {}", rp.offset_m);
    }

    #[test]
    fn step_junction_traversal_exit_advances_route() {
        let (mut sim, jn, _) = make_sim_with_junction();

        let agent = spawn_junction_agent(&mut sim, jn, 0, 0.99, 100.0);

        sim.step_junction_traversal(0.1);

        let route = sim
            .world
            .query_one_mut::<&Route>(agent)
            .unwrap();
        assert_eq!(route.current_step, 1, "route step should advance");
    }

    #[test]
    fn step_junction_traversal_deadlock_forced_crawl() {
        let (graph, junction_node) = make_junction_graph();
        let mut sim = SimWorld::new_cpu_only(graph);

        // Create junction with TWO crossing turns and a conflict between them
        let turn_0 = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [100.0, 0.0],
            p2: [200.0, 0.0],
            arc_length: 200.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        let turn_1 = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [100.0, 100.0],
            p1: [100.0, 0.0],
            p2: [100.0, -100.0],
            arc_length: 200.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        let conflict = velos_net::junction::ConflictPoint {
            turn_a_idx: 0,
            turn_b_idx: 1,
            t_a: 0.5,
            t_b: 0.5,
        };
        let jd = JunctionData {
            turns: vec![turn_0, turn_1],
            conflicts: vec![conflict],
        };
        sim.junction_data.insert(junction_node, jd);

        // Agent A on turn 0 at t=0.5 (exactly at conflict), speed=0, wait_ticks at threshold
        let agent = sim.world.spawn((
            Position { x: 0.0, y: 0.0 },
            Kinematics {
                vx: 0.0,
                vy: 0.0,
                speed: 0.0,
                heading: 0.0,
            },
            RoadPosition {
                edge_index: 0,
                lane: 0,
                offset_m: 99.0,
            },
            Route {
                path: vec![0, 2, 1],
                current_step: 0,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            VehicleType::Car,
            IdmParams {
                v0: 13.89,
                s0: 2.0,
                t_headway: 1.5,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
            },
            JunctionTraversal {
                junction_node,
                turn_index: 0,
                t: 0.35, // dist to conflict (0.5) = 0.15, farther than foe → must yield
                lateral_offset: 3.5,
                speed: 0.0,
                wait_ticks: MAX_YIELD_TICKS, // at threshold
            },
        ));

        // Agent B on turn 1 at t=0.48 (closer to conflict → has priority)
        // This creates a real conflict that keeps agent A yielding
        let _foe = sim.world.spawn((
            Position { x: 100.0, y: 50.0 },
            Kinematics {
                vx: 0.0,
                vy: -10.0,
                speed: 10.0,
                heading: -std::f64::consts::FRAC_PI_2,
            },
            RoadPosition {
                edge_index: 2,
                lane: 0,
                offset_m: 50.0,
            },
            Route {
                path: vec![3, 2, 1],
                current_step: 0,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            VehicleType::Car,
            IdmParams {
                v0: 13.89,
                s0: 2.0,
                t_headway: 1.5,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
            },
            JunctionTraversal {
                junction_node,
                turn_index: 1,
                t: 0.48, // closer to conflict at 0.5 → has priority over agent A
                lateral_offset: 3.5,
                speed: 10.0,
                wait_ticks: 0,
            },
        ));

        sim.step_junction_traversal(0.1);

        let jt = sim
            .world
            .query_one_mut::<&JunctionTraversal>(agent)
            .unwrap();
        assert!(
            jt.speed >= MIN_CRAWL_SPEED,
            "should force crawl speed after deadlock timeout, got {}",
            jt.speed,
        );
    }

    #[test]
    fn step_junction_traversal_free_flow_recovery() {
        // Agent with speed=0 but no conflict should recover via free-flow acceleration
        let (mut sim, jn, _turn) = make_sim_with_junction();

        let agent = spawn_junction_agent(&mut sim, jn, 0, 0.3, 0.0);

        sim.step_junction_traversal(0.1);

        let jt = sim
            .world
            .query_one_mut::<&JunctionTraversal>(agent)
            .unwrap();
        assert!(
            jt.speed > 0.0,
            "agent with no conflict should accelerate from 0, got {}",
            jt.speed,
        );
    }

    #[test]
    fn step_junction_traversal_no_agents_is_noop() {
        let (mut sim, _, _) = make_sim_with_junction();
        // No agents -- should not panic
        sim.step_junction_traversal(0.1);
    }

    /// Build two consecutive junctions connected by a short edge (<5m).
    ///
    /// Graph: A --edge0(100m)--> J1 --edge1(4m)--> J2 --edge2(100m)--> B
    /// Junctions at J1 (node 2) and J2 (node 3).
    /// Turn at J1: edge0 -> edge1, Turn at J2: edge1 -> edge2.
    fn make_chained_junction_graph() -> (SimWorld, u32, u32) {
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [210.0, 0.0] });
        let j1 = g.add_node(RoadNode { pos: [100.0, 0.0] }); // junction 1
        let j2 = g.add_node(RoadNode { pos: [104.0, 0.0] }); // junction 2
        // Extra arms to make j1/j2 qualify as junctions (need in>1 or out>1)
        let c = g.add_node(RoadNode { pos: [100.0, 100.0] });
        let d = g.add_node(RoadNode { pos: [104.0, 100.0] });

        let _e0 = g.add_edge(a, j1, test_edge(100.0));  // edge 0
        let _e1 = g.add_edge(j1, j2, test_edge(4.0));   // edge 1 (short!)
        let _e2 = g.add_edge(j2, b, test_edge(100.0));   // edge 2
        let _e3 = g.add_edge(c, j1, test_edge(100.0));   // edge 3 (extra arm)
        let _e4 = g.add_edge(j1, c, test_edge(100.0));   // edge 4
        let _e5 = g.add_edge(d, j2, test_edge(100.0));   // edge 5 (extra arm)
        let _e6 = g.add_edge(j2, d, test_edge(100.0));   // edge 6

        let graph = RoadGraph::new(g);
        let j1_id = j1.index() as u32;
        let j2_id = j2.index() as u32;

        let mut sim = SimWorld::new_cpu_only(graph);

        // Junction 1: turn from edge0 -> edge1
        let turn_j1 = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [90.0, 0.0],
            p1: [100.0, 0.0],
            p2: [102.0, 0.0],
            arc_length: 12.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        // Extra turn to make j1 a real junction
        let turn_j1_extra = BezierTurn {
            entry_edge: 3,
            exit_edge: 4,
            p0: [100.0, 90.0],
            p1: [100.0, 50.0],
            p2: [100.0, 10.0],
            arc_length: 80.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        sim.junction_data.insert(j1_id, JunctionData {
            turns: vec![turn_j1, turn_j1_extra],
            conflicts: vec![],
        });

        // Junction 2: turn from edge1 -> edge2
        let turn_j2 = BezierTurn {
            entry_edge: 1,
            exit_edge: 2,
            p0: [102.0, 0.0],
            p1: [104.0, 0.0],
            p2: [114.0, 0.0],
            arc_length: 12.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        let turn_j2_extra = BezierTurn {
            entry_edge: 5,
            exit_edge: 6,
            p0: [104.0, 90.0],
            p1: [104.0, 50.0],
            p2: [104.0, 10.0],
            arc_length: 80.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        sim.junction_data.insert(j2_id, JunctionData {
            turns: vec![turn_j2, turn_j2_extra],
            conflicts: vec![],
        });

        (sim, j1_id, j2_id)
    }

    #[test]
    fn smooth_chain_preserves_speed_and_pre_advances_t() {
        let (mut sim, j1_id, j2_id) = make_chained_junction_graph();

        // Agent at t=0.95 on junction 1, speed=10 m/s
        // Route: A(0) -> J1(2) -> J2(3) -> B(1)
        let agent = sim.world.spawn((
            Position { x: 99.0, y: 0.0 },
            Kinematics {
                vx: 10.0,
                vy: 0.0,
                speed: 10.0,
                heading: 0.0,
            },
            RoadPosition {
                edge_index: 0,
                lane: 0,
                offset_m: 99.0,
            },
            Route {
                path: vec![0, 2, 3, 1], // A -> J1 -> J2 -> B
                current_step: 0,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            VehicleType::Car,
            IdmParams {
                v0: 13.89,
                s0: 2.0,
                t_headway: 1.5,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
            },
            JunctionTraversal {
                junction_node: j1_id,
                turn_index: 0,
                t: 0.95,
                lateral_offset: 3.5,
                speed: 10.0,
                wait_ticks: 0,
            },
        ));

        // Step with dt=0.1, speed=10, arc=12:
        // dt_param = 0.1*10/12 = 0.0833
        // raw_t = 0.95 + 0.0833 = 1.0333 -> finished
        // overflow_m = 0.0333 * 12 = 0.4m
        // Chain: edge1 length = 4m -> accumulated overflow = 0.4 + 4 = 4.4m
        // Next Bezier arc = 12m -> t_advance = min(4.4/12, 0.15) = 0.15 (capped)
        // initial_t = entry_t(0.0) + 0.15 = 0.15
        sim.step_junction_traversal(0.1);

        // Agent should now be on junction 2, with t capped near entry
        let jt = sim
            .world
            .query_one_mut::<&JunctionTraversal>(agent)
            .unwrap();
        assert_eq!(jt.junction_node, j2_id, "should have chained to junction 2");
        assert!(
            jt.t > 0.0 && jt.t <= 0.20,
            "t should be capped near entry_t to prevent teleportation, got {}",
            jt.t,
        );
        assert!(
            (jt.speed - 10.0).abs() < 0.01,
            "speed should be preserved through chain, got {}",
            jt.speed,
        );
    }

    #[test]
    fn smooth_chain_overflow_applied_to_exit_edge() {
        let (mut sim, j1_id, _j2_id) = make_chained_junction_graph();

        // Make junction 1 exit onto a LONG edge (100m) so chaining won't happen.
        // Replace junction 1 data with a turn that exits onto edge 2 (100m, too long to chain).
        let turn_no_chain = BezierTurn {
            entry_edge: 0,
            exit_edge: 2, // edge 2 is 100m, won't chain
            p0: [90.0, 0.0],
            p1: [100.0, 0.0],
            p2: [120.0, 0.0],
            arc_length: 30.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        sim.junction_data.insert(j1_id, JunctionData {
            turns: vec![turn_no_chain],
            conflicts: vec![],
        });

        // Agent near Bezier end, high speed to create overflow
        let agent = sim.world.spawn((
            Position { x: 99.0, y: 0.0 },
            Kinematics {
                vx: 20.0,
                vy: 0.0,
                speed: 20.0,
                heading: 0.0,
            },
            RoadPosition {
                edge_index: 0,
                lane: 0,
                offset_m: 99.0,
            },
            Route {
                path: vec![0, 2, 1], // A -> J1 -> B
                current_step: 0,
            },
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            VehicleType::Car,
            IdmParams {
                v0: 13.89,
                s0: 2.0,
                t_headway: 1.5,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
            },
            JunctionTraversal {
                junction_node: j1_id,
                turn_index: 0,
                t: 0.95,
                lateral_offset: 3.5,
                speed: 20.0,
                wait_ticks: 0,
            },
        ));

        // dt=0.1, speed=20, arc=30: dt_param=0.0667, raw_t=1.0167
        // overflow = 0.0167 * 30 = 0.5m
        // No chain (exit edge 100m > 5m threshold)
        // offset_m should be exit_offset (0.1) + overflow (0.5) = 0.6
        sim.step_junction_traversal(0.1);

        let has_jt = sim
            .world
            .query_one_mut::<&JunctionTraversal>(agent)
            .is_ok();
        assert!(!has_jt, "should exit junction (no chain)");

        let rp = sim
            .world
            .query_one_mut::<&RoadPosition>(agent)
            .unwrap();
        assert!(
            rp.offset_m > 0.1,
            "overflow should be added to exit offset, got {}",
            rp.offset_m,
        );
    }

    #[test]
    fn chain_t_cap_prevents_teleport_through_junction() {
        // Regression test: with the old 30m threshold and uncapped t-advance,
        // an agent chaining through a 20m edge into a 15m-arc junction would
        // get initial_t ≈ 0.99, effectively teleporting through the entire
        // second junction. The MAX_CHAIN_T_ADVANCE cap (0.15) ensures the
        // agent enters near entry_t and visually traverses the junction.
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [125.0, 0.0] });
        let j1 = g.add_node(RoadNode { pos: [100.0, 0.0] });
        let j2 = g.add_node(RoadNode { pos: [103.0, 0.0] }); // 3m intermediate edge
        let c = g.add_node(RoadNode { pos: [100.0, 50.0] });
        let d = g.add_node(RoadNode { pos: [103.0, 50.0] });

        let _e0 = g.add_edge(a, j1, test_edge(100.0));
        let _e1 = g.add_edge(j1, j2, test_edge(3.0)); // short edge triggers chaining
        let _e2 = g.add_edge(j2, b, test_edge(22.0));
        let _e3 = g.add_edge(c, j1, test_edge(50.0));
        let _e4 = g.add_edge(j1, c, test_edge(50.0));
        let _e5 = g.add_edge(d, j2, test_edge(50.0));
        let _e6 = g.add_edge(j2, d, test_edge(50.0));

        let graph = RoadGraph::new(g);
        let j1_id = j1.index() as u32;
        let j2_id = j2.index() as u32;
        let mut sim = SimWorld::new_cpu_only(graph);

        let turn_j1 = BezierTurn {
            entry_edge: 0, exit_edge: 1,
            p0: [90.0, 0.0], p1: [100.0, 0.0], p2: [101.5, 0.0],
            arc_length: 11.5, exit_offset_m: 0.1, entry_t: 0.4,
        };
        let turn_j1_arm = BezierTurn {
            entry_edge: 3, exit_edge: 4,
            p0: [100.0, 45.0], p1: [100.0, 25.0], p2: [100.0, 5.0],
            arc_length: 40.0, exit_offset_m: 0.1, entry_t: 0.0,
        };
        sim.junction_data.insert(j1_id, JunctionData {
            turns: vec![turn_j1, turn_j1_arm], conflicts: vec![],
        });

        let turn_j2 = BezierTurn {
            entry_edge: 1, exit_edge: 2,
            p0: [101.5, 0.0], p1: [103.0, 0.0], p2: [114.0, 0.0],
            arc_length: 12.5, exit_offset_m: 0.1, entry_t: 0.3,
        };
        let turn_j2_arm = BezierTurn {
            entry_edge: 5, exit_edge: 6,
            p0: [103.0, 45.0], p1: [103.0, 25.0], p2: [103.0, 5.0],
            arc_length: 40.0, exit_offset_m: 0.1, entry_t: 0.0,
        };
        sim.junction_data.insert(j2_id, JunctionData {
            turns: vec![turn_j2, turn_j2_arm], conflicts: vec![],
        });

        // Agent at high speed near end of junction 1
        let agent = sim.world.spawn((
            Position { x: 99.0, y: 0.0 },
            Kinematics { vx: 15.0, vy: 0.0, speed: 15.0, heading: 0.0 },
            RoadPosition { edge_index: 0, lane: 0, offset_m: 99.0 },
            Route { path: vec![0, 2, 3, 1], current_step: 0 },
            WaitState { stopped_since: -1.0, at_red_signal: false },
            VehicleType::Car,
            IdmParams { v0: 13.89, s0: 2.0, t_headway: 1.5, a: 1.0, b: 2.0, delta: 4.0 },
            JunctionTraversal {
                junction_node: j1_id, turn_index: 0, t: 0.95,
                lateral_offset: 3.5, speed: 15.0, wait_ticks: 0,
            },
        ));

        sim.step_junction_traversal(0.1);

        let jt = sim.world.query_one_mut::<&JunctionTraversal>(agent).unwrap();
        assert_eq!(jt.junction_node, j2_id, "should chain to junction 2");
        // With uncapped logic: overflow ≈ 0.79 + 3 = 3.79m, t = 0.3 + 3.79/12.5 = 0.603
        // With cap: t = 0.3 + min(3.79/12.5, 0.15) = 0.3 + 0.15 = 0.45
        assert!(
            jt.t <= 0.50,
            "t should be capped to prevent teleporting through junction, got {}",
            jt.t,
        );
        assert!(
            jt.t >= 0.30,
            "t should be at least entry_t (0.3), got {}",
            jt.t,
        );
    }
}
