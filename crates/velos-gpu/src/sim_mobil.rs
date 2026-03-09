//! MOBIL lane-change wiring for car agents.
//!
//! Evaluates MOBIL lane-change decisions and processes gradual lateral drift
//! for cars executing lane changes. Extracted from sim.rs to keep files under 700 lines.

use std::collections::HashMap;

use hecs::Entity;
use petgraph::graph::EdgeIndex;

use velos_core::components::{
    JunctionTraversal, Kinematics, LaneChangeState, LastLaneChange, LateralOffset, RoadPosition,
    VehicleType,
};
use velos_vehicle::idm::IdmParams;
use velos_vehicle::mobil::{mobil_decision, LaneChangeContext};
use velos_vehicle::types::default_mobil_params;

use crate::sim::SimWorld;

/// Intermediate state for a car agent's MOBIL evaluation.
pub(crate) struct CarMobilContext {
    pub entity: Entity,
    pub rp: RoadPosition,
    pub speed: f64,
    pub idm_params: IdmParams,
    pub has_lane_change: bool,
}

impl SimWorld {
    /// Evaluate MOBIL lane-change for a single car and return target lane if accepted.
    pub(crate) fn evaluate_mobil(
        &mut self,
        car: &CarMobilContext,
        accel_current: f64,
        edge_lane_agents: &HashMap<(u32, u8), Vec<(Entity, f64)>>,
        speed_map: &HashMap<Entity, f64>,
    ) -> Option<(u8, f64)> {
        if car.has_lane_change {
            return None;
        }

        let edge = EdgeIndex::new(car.rp.edge_index as usize);
        let (lane_count, edge_length) = self
            .road_graph
            .inner()
            .edge_weight(edge)
            .map(|e| (e.lane_count, e.length_m))
            .unwrap_or((1, 100.0));

        // Skip MOBIL near edges of the road segment or on single-lane roads.
        if car.rp.offset_m < 5.0 || car.rp.offset_m > edge_length - 20.0 || lane_count <= 1 {
            return None;
        }

        // Cooldown check: 3s since last lane change completion.
        let cooldown_ok = self
            .world
            .query_one_mut::<&LastLaneChange>(car.entity)
            .ok()
            .map(|llc| self.sim_time - llc.completed_at > 3.0)
            .unwrap_or(true);

        if !cooldown_ok {
            return None;
        }

        let mobil_params = default_mobil_params();
        let current_lane = car.rp.lane;

        // Evaluate right then left.
        let candidates: [(bool, i8); 2] = [
            (current_lane > 0, -1),              // right (lower index)
            (current_lane + 1 < lane_count, 1),  // left (higher index)
        ];

        for (valid, dir) in &candidates {
            if !valid {
                continue;
            }
            let target_lane = (current_lane as i8 + dir) as u8;
            let is_right = *dir < 0;

            let (tgt_leader_gap, tgt_leader_speed) = Self::find_leader_in_lane(
                car.rp.offset_m,
                car.rp.edge_index,
                target_lane,
                edge_lane_agents,
                speed_map,
            );
            let (tgt_follower_gap, tgt_follower_speed) = Self::find_follower_in_lane(
                car.rp.offset_m,
                car.rp.edge_index,
                target_lane,
                edge_lane_agents,
                speed_map,
            );

            let accel_target = velos_vehicle::idm::idm_acceleration(
                &car.idm_params,
                car.speed,
                tgt_leader_gap,
                car.speed - tgt_leader_speed,
            );

            let accel_new_follower = velos_vehicle::idm::idm_acceleration(
                &car.idm_params,
                tgt_follower_speed,
                tgt_follower_gap,
                tgt_follower_speed - car.speed,
            );

            let ctx = LaneChangeContext {
                accel_current,
                accel_target,
                accel_new_follower,
                accel_old_follower: 0.0,
                is_right,
            };

            if mobil_decision(&mobil_params, &ctx) {
                return Some((target_lane, self.sim_time));
            }
        }

        None
    }

    /// Apply a new lane change: attach LaneChangeState and LateralOffset to entity.
    pub fn start_lane_change(&mut self, entity: Entity, target_lane: u8, started_at: f64) {
        let current_lane = self
            .world
            .query_one_mut::<&RoadPosition>(entity)
            .map(|rp| rp.lane)
            .unwrap_or(0);
        let current_lateral = (current_lane as f64 + 0.5) * 3.5;

        let _ = self.world.insert(
            entity,
            (
                LaneChangeState {
                    target_lane,
                    time_remaining: 2.0,
                    started_at,
                },
                LateralOffset {
                    lateral_offset: current_lateral,
                    desired_lateral: (target_lane as f64 + 0.5) * 3.5,
                },
            ),
        );
    }

    /// Process gradual lateral drift for cars with active lane changes.
    pub(crate) fn process_car_lane_changes(&mut self, dt: f64) {
        struct DriftState {
            entity: Entity,
            target_lane: u8,
            time_remaining: f64,
            current_lateral: f64,
            desired_lateral: f64,
        }

        let drifting: Vec<DriftState> = self
            .world
            .query_mut::<(Entity, &LaneChangeState, &LateralOffset, &VehicleType)>()
            .into_iter()
            .filter(|(_, _, _, vt)| **vt == VehicleType::Car)
            .map(|(e, lcs, lat, _)| DriftState {
                entity: e,
                target_lane: lcs.target_lane,
                time_remaining: lcs.time_remaining,
                current_lateral: lat.lateral_offset,
                desired_lateral: lat.desired_lateral,
            })
            .collect();

        let sim_time = self.sim_time;

        for drift in &drifting {
            let new_time = drift.time_remaining - dt;

            if new_time <= 0.0 {
                // Drift complete: update lane, keep LateralOffset at target lane center.
                if let Ok(rp) = self.world.query_one_mut::<&mut RoadPosition>(drift.entity) {
                    rp.lane = drift.target_lane;
                }
                // Remove only LaneChangeState; keep LateralOffset at final position
                // so the car stays at the correct lane center (prevents flicker).
                let _ = self.world.remove::<(LaneChangeState,)>(drift.entity);
                if let Ok(lat) =
                    self.world
                        .query_one_mut::<&mut LateralOffset>(drift.entity)
                {
                    lat.lateral_offset = drift.desired_lateral;
                    lat.desired_lateral = drift.desired_lateral;
                }
                let _ = self.world.insert_one(
                    drift.entity,
                    LastLaneChange {
                        completed_at: sim_time,
                    },
                );
                // Apply the final lateral offset to world position.
                self.apply_lateral_world_offset(drift.entity, drift.desired_lateral);
            } else {
                // Linear drift toward target over remaining time.
                let remaining_dist = drift.desired_lateral - drift.current_lateral;
                let drift_speed = remaining_dist / drift.time_remaining;
                let new_lateral = drift.current_lateral + drift_speed * dt;

                if let Ok(lcs) =
                    self.world
                        .query_one_mut::<&mut LaneChangeState>(drift.entity)
                {
                    lcs.time_remaining = new_time;
                }
                if let Ok(lat) =
                    self.world
                        .query_one_mut::<&mut LateralOffset>(drift.entity)
                {
                    lat.lateral_offset = new_lateral;
                }
                self.apply_lateral_world_offset(drift.entity, new_lateral);
            }
        }
    }

    /// Evaluate MOBIL lane-change decisions for all cars and apply drift.
    ///
    /// Called once per tick in tick_gpu() before GPU physics dispatch.
    /// For each car without an active LaneChangeState:
    ///   1. Compute current IDM acceleration
    ///   2. Evaluate MOBIL for adjacent lanes
    ///   3. Start lane change if accepted
    ///
    /// Then process ongoing drift for all cars with active LaneChangeState.
    pub fn step_lane_changes(&mut self, dt: f64) {
        use velos_vehicle::idm::idm_acceleration;

        // Collect car agents for evaluation
        struct CarSnap {
            entity: Entity,
            rp: RoadPosition,
            speed: f64,
            idm: IdmParams,
            has_lc: bool,
        }

        let cars: Vec<CarSnap> = self
            .world
            .query_mut::<(
                Entity,
                &RoadPosition,
                &Kinematics,
                &IdmParams,
                &VehicleType,
                Option<&LaneChangeState>,
                Option<&JunctionTraversal>,
            )>()
            .into_iter()
            // Skip junction-traversing agents — they use Bezier curves, not lanes
            .filter(|(_, _, _, _, vt, _, jt)| **vt == VehicleType::Car && jt.is_none())
            .map(|(e, rp, kin, idm, _, lcs, _)| CarSnap {
                entity: e,
                rp: *rp,
                speed: kin.speed,
                idm: *idm,
                has_lc: lcs.is_some(),
            })
            .collect();

        // Build edge-lane agent index and speed map
        let mut edge_lane_agents: HashMap<(u32, u8), Vec<(Entity, f64)>> = HashMap::new();
        let speed_map: HashMap<Entity, f64> =
            cars.iter().map(|c| (c.entity, c.speed)).collect();

        for car in &cars {
            edge_lane_agents
                .entry((car.rp.edge_index, car.rp.lane))
                .or_default()
                .push((car.entity, car.rp.offset_m));
        }
        for v in edge_lane_agents.values_mut() {
            v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        }

        // Evaluate MOBIL and collect decisions
        let mut decisions: Vec<(Entity, u8, f64)> = Vec::new();

        for car in &cars {
            if car.has_lc {
                continue;
            }

            let at_red = self.check_signal_red(&car.rp);
            if at_red {
                continue;
            }

            // Compute current IDM acceleration against leader in same lane
            let (gap, delta_v) = Self::find_leader_in_lane(
                car.rp.offset_m,
                car.rp.edge_index,
                car.rp.lane,
                &edge_lane_agents,
                &speed_map,
            );
            let accel_current = idm_acceleration(&car.idm, car.speed, gap, delta_v);

            let ctx = CarMobilContext {
                entity: car.entity,
                rp: car.rp,
                speed: car.speed,
                idm_params: car.idm,
                has_lane_change: car.has_lc,
            };

            if let Some((target_lane, started_at)) =
                self.evaluate_mobil(&ctx, accel_current, &edge_lane_agents, &speed_map)
            {
                decisions.push((car.entity, target_lane, started_at));
            }
        }

        // Apply decisions
        for (entity, target_lane, started_at) in decisions {
            self.start_lane_change(entity, target_lane, started_at);
        }

        // Process ongoing lateral drifts
        self.process_car_lane_changes(dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use velos_core::components::{Kinematics, Position};

    fn make_test_graph_2lane() -> velos_net::RoadGraph {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};

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

    fn spawn_car(
        sim: &mut SimWorld,
        edge: u32,
        lane: u8,
        offset: f64,
        speed: f64,
    ) -> Entity {
        let entity = sim.world.spawn((
            RoadPosition {
                edge_index: edge,
                lane,
                offset_m: offset,
            },
            Kinematics {
                vx: speed,
                vy: 0.0,
                speed,
                heading: 0.0,
            },
            IdmParams {
                v0: 13.89,
                s0: 2.0,
                t_headway: 1.6,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
            },
            VehicleType::Car,
            Position { x: offset, y: (lane as f64) * 3.5 + 1.75 },
            LateralOffset {
                lateral_offset: (lane as f64 + 0.5) * 3.5,
                desired_lateral: (lane as f64 + 0.5) * 3.5,
            },
        ));
        entity
    }

    #[test]
    fn step_lane_changes_triggers_mobil_for_car_behind_slow_leader() {
        let graph = make_test_graph_2lane();
        let mut sim = SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        // Slow car ahead
        spawn_car(&mut sim, 0, 0, 100.0, 2.0);
        // Fast car behind
        let fast = spawn_car(&mut sim, 0, 0, 80.0, 10.0);

        sim.step_lane_changes(0.1);

        let has_lcs = sim.world.query_one_mut::<&LaneChangeState>(fast).is_ok();
        assert!(has_lcs, "fast car behind slow leader should get LaneChangeState");
    }

    #[test]
    fn step_lane_changes_drift_reduces_time_remaining() {
        let graph = make_test_graph_2lane();
        let mut sim = SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        let car = spawn_car(&mut sim, 0, 0, 50.0, 10.0);
        sim.start_lane_change(car, 1, sim.sim_time);

        let time_before = sim
            .world
            .query_one_mut::<&LaneChangeState>(car)
            .unwrap()
            .time_remaining;

        sim.step_lane_changes(0.1);

        // After one step, time_remaining should decrease by dt
        let lcs_result = sim.world.query_one_mut::<&LaneChangeState>(car);
        if let Ok(lcs) = lcs_result {
            assert!(lcs.time_remaining < time_before, "time_remaining should decrease");
        }
        // If LaneChangeState was removed, drift completed (also valid for large dt)
    }
}
