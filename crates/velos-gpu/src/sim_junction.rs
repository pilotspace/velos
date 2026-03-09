//! Junction traversal frame pipeline step.
//!
//! Step 6.8: Advances agents currently traversing junctions along their
//! Bezier curves with conflict detection and IDM-based yielding.
//! Runs after lane changes (6.7), before GPU vehicle physics (7.0).

use hecs::Entity;
use petgraph::graph::EdgeIndex;

use velos_core::components::{
    JunctionTraversal, Kinematics, Position, RoadPosition, Route,
    VehicleType as CoreVehicleType, MAX_YIELD_TICKS,
};
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

            // Advance t
            let (new_t, finished) = advance_on_bezier(a.t, a.speed, turn.arc_length, dt);

            // Determine effective speed after conflict check
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

            // Check conflicts
            if let Some(agents_here) = junction_agents.get(&a.junction_node)
                && let Some(conflict_result) = check_conflicts(
                    a.turn_index,
                    a.t, // use pre-advance t for priority determination
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
                // Bug 4 fix: Junction exit -- directly place on exit edge.
                // Do NOT call advance_to_next_edge (it would re-enter the junction).
                let exit_info = {
                    let Ok(jt) = self
                        .world
                        .query_one_mut::<&JunctionTraversal>(upd.entity)
                    else {
                        continue;
                    };
                    let junction_node = jt.junction_node;
                    let turn_index = jt.turn_index;
                    let Some(jd) = self.junction_data.get(&junction_node) else {
                        continue;
                    };
                    let Some(turn) = jd.turns.get(turn_index as usize) else {
                        continue;
                    };
                    (turn.exit_edge, turn.exit_offset_m)
                };

                let (exit_edge_id, exit_offset) = exit_info;

                // Remove JunctionTraversal component
                let _ = self.world.remove_one::<JunctionTraversal>(upd.entity);

                // Advance route step (the junction node was the current target)
                if let Ok(route) = self.world.query_one_mut::<&mut Route>(upd.entity) {
                    route.current_step += 1;
                }

                // Place agent on exit edge
                if let Ok(rp) = self.world.query_one_mut::<&mut RoadPosition>(upd.entity) {
                    rp.edge_index = exit_edge_id;
                    rp.offset_m = exit_offset;
                    rp.lane = 0;
                }

                // Update world position from the new edge
                self.update_agent_state(upd.entity, upd.effective_speed);
                self.update_wait_state(upd.entity, upd.effective_speed, false);
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
        let (mut sim, jn, _) = make_sim_with_junction();

        // Agent with speed 0 and wait_ticks at threshold
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
                junction_node: jn,
                turn_index: 0,
                t: 0.5,
                lateral_offset: 3.5,
                speed: 0.0,
                wait_ticks: MAX_YIELD_TICKS, // at threshold
            },
        ));

        sim.step_junction_traversal(0.1);

        let jt = sim
            .world
            .query_one_mut::<&JunctionTraversal>(agent)
            .unwrap();
        assert!(
            jt.speed >= MIN_CRAWL_SPEED,
            "should force crawl speed after deadlock timeout"
        );
    }

    #[test]
    fn step_junction_traversal_no_agents_is_noop() {
        let (mut sim, _, _) = make_sim_with_junction();
        // No agents -- should not panic
        sim.step_junction_traversal(0.1);
    }
}
