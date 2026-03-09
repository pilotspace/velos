//! Junction traversal frame pipeline step.
//!
//! Step 6.8: Advances agents currently traversing junctions along their
//! Bezier curves with conflict detection and IDM-based yielding.
//! Runs after lane changes (6.7), before GPU vehicle physics (7.0).

use hecs::Entity;
use petgraph::graph::{EdgeIndex, NodeIndex};

use velos_core::components::{
    JunctionTraversal, Kinematics, Position, RoadPosition, Route,
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
            let (new_t, finished) = advance_on_bezier(a.t, effective_speed, turn.arc_length, dt);

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
                self.handle_junction_exit(upd.entity, upd.effective_speed);
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
    const SHORT_EDGE_THRESHOLD_M: f64 = 2.0;

    /// Handle junction exit with multi-segment chaining.
    ///
    /// When an agent finishes a junction Bezier (t >= 1.0), instead of placing
    /// it on the exit edge and waiting for the next frame to potentially re-enter
    /// another junction (which causes visible teleporting), this method chains
    /// through up to MAX_CHAIN_DEPTH consecutive junctions in one step.
    ///
    /// For each chain step:
    /// 1. Read current junction's exit edge and advance route
    /// 2. Check if the exit edge is short AND the next node is also a junction
    /// 3. If yes: update JunctionTraversal in-place to the next junction's Bezier
    /// 4. If no: exit to edge normally
    fn handle_junction_exit(&mut self, entity: Entity, speed: f64) {
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
            // 1. Exit edge must be short
            // 2. Next node must have junction data
            // 3. There must be a matching turn (exit_edge -> next_next_edge)
            let next_junction = self.find_next_junction_chain(entity, exit_edge_id);

            match next_junction {
                Some((next_jn, next_turn_idx, next_turn, next_road_half_width)) => {
                    // Chain: update JunctionTraversal in-place to next junction
                    if let Ok(jt) = self.world.query_one_mut::<&mut JunctionTraversal>(entity) {
                        jt.junction_node = next_jn;
                        jt.turn_index = next_turn_idx;
                        jt.t = 0.0;
                        // Keep lateral_offset and speed from previous junction
                        jt.wait_ticks = 0;
                    }

                    // Advance route step again (skip the short intermediate edge)
                    if let Ok(route) = self.world.query_one_mut::<&mut Route>(entity) {
                        route.current_step += 1;
                    }

                    // Set position to Bezier(t=0) of the new junction
                    let pos = next_turn.offset_position(0.0, lateral_offset, next_road_half_width);
                    let tan = next_turn.tangent(0.0);
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

                    // Continue loop to check if THIS junction also chains
                    // (agent.t is 0.0, so it won't be "finished" — loop will break
                    //  at the JunctionTraversal read since t=0 won't trigger exit)
                    // Actually we just set t=0, so we're done chaining for this frame.
                    // The agent will advance through this new junction normally next frame.
                    self.update_wait_state(entity, speed, false);
                    return;
                }
                None => {
                    // No chaining possible — normal exit to edge
                    let _ = self.world.remove_one::<JunctionTraversal>(entity);

                    if let Ok(rp) = self.world.query_one_mut::<&mut RoadPosition>(entity) {
                        rp.edge_index = exit_edge_id;
                        rp.offset_m = exit_offset;
                        rp.lane = 0;
                    }

                    self.update_agent_state(entity, speed);
                    self.update_wait_state(entity, speed, false);
                    return;
                }
            }
        }

        // Exhausted chain depth — force normal exit
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
            rp.offset_m = exit_info.1;
            rp.lane = 0;
        }
        self.update_agent_state(entity, speed);
        self.update_wait_state(entity, speed, false);
    }

    /// Check if the exit edge is short and the next node has a junction to chain into.
    ///
    /// Returns `Some((next_junction_node, turn_index, &BezierTurn, road_half_width))`
    /// if chaining is possible, `None` otherwise.
    fn find_next_junction_chain(
        &mut self,
        entity: Entity,
        exit_edge_id: u32,
    ) -> Option<(u32, u16, BezierTurn, f64)> {
        // Check exit edge length
        let exit_edge_idx = EdgeIndex::new(exit_edge_id as usize);
        let g = self.road_graph.inner();
        let edge_weight = g.edge_weight(exit_edge_idx)?;
        if edge_weight.length_m > Self::SHORT_EDGE_THRESHOLD_M {
            return None;
        }

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
        assert!((rp.offset_m - 0.1).abs() < 0.01, "should be at exit_offset_m");
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
        };
        let turn_1 = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [100.0, 100.0],
            p1: [100.0, 0.0],
            p2: [100.0, -100.0],
            arc_length: 200.0,
            exit_offset_m: 0.1,
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
}
