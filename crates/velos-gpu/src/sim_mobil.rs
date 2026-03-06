//! MOBIL lane-change wiring for car agents.
//!
//! Evaluates MOBIL lane-change decisions and processes gradual lateral drift
//! for cars executing lane changes. Extracted from sim.rs to keep files under 700 lines.

use std::collections::HashMap;

use hecs::Entity;
use petgraph::graph::EdgeIndex;

use velos_core::components::{
    LaneChangeState, LastLaneChange, LateralOffset, RoadPosition, VehicleType,
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
    pub(crate) fn start_lane_change(&mut self, entity: Entity, target_lane: u8, started_at: f64) {
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
                // Drift complete: update lane, remove LaneChangeState and LateralOffset.
                if let Ok(rp) = self.world.query_one_mut::<&mut RoadPosition>(drift.entity) {
                    rp.lane = drift.target_lane;
                }
                let _ = self
                    .world
                    .remove::<(LaneChangeState, LateralOffset)>(drift.entity);
                let _ = self.world.insert_one(
                    drift.entity,
                    LastLaneChange {
                        completed_at: sim_time,
                    },
                );
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
}
