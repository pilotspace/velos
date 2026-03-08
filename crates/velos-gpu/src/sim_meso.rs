//! Mesoscopic zone integration for SimWorld.
//!
//! Provides step_meso() CPU function that ticks SpatialQueues for meso-designated
//! edges and handles meso-micro zone transitions with velocity matching.
//! Follows sim_bus.rs pattern: extracted to its own module for 700-line compliance.

use petgraph::graph::EdgeIndex;

use velos_core::components::{
    CarFollowingModel, Kinematics, LateralOffset, Position, RoadPosition, Route, VehicleType,
    WaitState,
};
use velos_meso::buffer_zone::velocity_matching_speed;
use velos_meso::queue_model::MesoVehicle;
use velos_vehicle::idm::IdmParams;

use crate::sim::SimWorld;

/// Preserved agent state during mesoscopic transit.
///
/// When an agent enters a meso zone, its ECS entity is despawned but its
/// identity is preserved here. On meso exit, a new ECS entity is spawned
/// with these fields restored -- no identity loss across zone transitions.
#[derive(Debug, Clone)]
pub struct MesoAgentState {
    pub route: Route,
    pub vehicle_type: VehicleType,
    pub idm_params: IdmParams,
    pub cf_model: CarFollowingModel,
    pub lateral_offset: Option<f64>,
}

impl SimWorld {
    /// Tick mesoscopic queues and handle meso-to-micro agent transitions.
    ///
    /// Called every frame BEFORE step_vehicles_gpu in the pipeline.
    /// When meso_enabled is false, this is a no-op.
    ///
    /// Processing order:
    /// 1. try_exit() on each SpatialQueue to find agents ready to leave meso
    /// 2. For each exiting agent, spawn into micro simulation via spawn_from_meso()
    /// 3. Micro-to-meso entry handled separately in advance_to_next_edge()
    pub fn step_meso(&mut self, _dt: f64) {
        if !self.meso_enabled {
            return;
        }

        // Collect exits from all meso queues (avoid borrow conflict with self).
        let mut exits: Vec<(u32, MesoVehicle)> = Vec::new();
        for (&edge_id, queue) in &mut self.meso_queues {
            while let Some(vehicle) = queue.try_exit(self.sim_time) {
                exits.push((edge_id, vehicle));
            }
        }

        // Spawn each exiting vehicle into micro simulation.
        for (edge_id, vehicle) in exits {
            self.spawn_from_meso(edge_id, vehicle);
        }
    }

    /// Spawn an agent exiting a meso queue into the micro simulation.
    ///
    /// Reconstructs a full ECS entity from the preserved MesoAgentState.
    /// Uses velocity matching to set the insertion speed, preventing
    /// speed discontinuities at the zone boundary.
    fn spawn_from_meso(&mut self, meso_edge_id: u32, vehicle: MesoVehicle) {
        let Some(state) = self.meso_agent_states.remove(&vehicle.vehicle_id) else {
            log::warn!(
                "MesoAgentState not found for vehicle {} -- identity loss detected",
                vehicle.vehicle_id
            );
            return;
        };

        let exit_edge_id = vehicle.exit_edge;

        // Check if there is physical space on the micro edge for insertion.
        if !self.check_gap_for_insertion(exit_edge_id) {
            // Re-insert into meso queue -- will retry next frame.
            self.meso_agent_states.insert(vehicle.vehicle_id, state);
            if let Some(queue) = self.meso_queues.get_mut(&meso_edge_id) {
                queue.enter(MesoVehicle::new(
                    vehicle.vehicle_id,
                    self.sim_time,
                    vehicle.exit_edge,
                ));
            }
            return;
        }

        // Compute insertion speed via velocity matching.
        let meso_exit_speed = {
            let queue = self.meso_queues.get(&meso_edge_id);
            let edge_idx = EdgeIndex::new(meso_edge_id as usize);
            let edge_length = self
                .road_graph
                .inner()
                .edge_weight(edge_idx)
                .map(|e| e.length_m)
                .unwrap_or(100.0);
            let tt = queue.map(|q| q.travel_time()).unwrap_or(edge_length / 13.9);
            (edge_length / tt).max(0.1)
        };

        // Find last micro vehicle speed on exit edge for velocity matching.
        let last_micro_speed = self.find_last_micro_speed(exit_edge_id, meso_exit_speed);
        let insertion_speed = velocity_matching_speed(meso_exit_speed, last_micro_speed);

        // Compute world position from edge geometry.
        let exit_edge_idx = EdgeIndex::new(exit_edge_id as usize);
        let g = self.road_graph.inner();
        let (world_x, world_y, heading) = g
            .edge_weight(exit_edge_idx)
            .map(|e| {
                let start = e.geometry[0];
                let end = e.geometry[e.geometry.len() - 1];
                let heading = (end[1] - start[1]).atan2(end[0] - start[0]);
                (start[0], start[1], heading)
            })
            .unwrap_or((0.0, 0.0, 0.0));

        // Spawn ECS entity at buffer zone start (offset=0, lane=0 rightmost).
        let entity = self.world.spawn((
            Position {
                x: world_x,
                y: world_y,
            },
            Kinematics {
                vx: insertion_speed * heading.cos(),
                vy: insertion_speed * heading.sin(),
                speed: insertion_speed,
                heading,
            },
            state.vehicle_type,
            RoadPosition {
                edge_index: exit_edge_id,
                lane: 0,
                offset_m: 0.0,
            },
            state.route,
            WaitState {
                stopped_since: -1.0,
                at_red_signal: false,
            },
            state.idm_params,
            state.cf_model,
        ));

        // Add lateral offset if present (motorbikes/bicycles).
        if let Some(lat) = state.lateral_offset {
            let _ = self
                .world
                .insert_one(entity, LateralOffset {
                    lateral_offset: lat,
                    desired_lateral: lat,
                });
        }

        log::debug!(
            "Agent {} spawned from meso edge {} to micro edge {} at speed {:.1} m/s",
            vehicle.vehicle_id,
            meso_edge_id,
            exit_edge_id,
            insertion_speed
        );
    }

    /// Check if there is sufficient gap at the start of a micro edge for insertion.
    ///
    /// Returns false if the nearest agent has offset_m < 10.0m (vehicle length + buffer).
    fn check_gap_for_insertion(&self, edge_id: u32) -> bool {
        let min_gap = 10.0; // vehicle length + safety buffer
        for rp in self.world.query::<&RoadPosition>().iter() {
            if rp.edge_index == edge_id && rp.offset_m < min_gap {
                return false;
            }
        }
        true
    }

    /// Find the speed of the last (lowest offset) micro vehicle on an edge.
    ///
    /// Used for velocity matching during meso-to-micro insertion.
    /// Returns the edge's free-flow speed if no agents are present.
    fn find_last_micro_speed(&self, edge_id: u32, default_speed: f64) -> f64 {
        let mut min_offset = f64::MAX;
        let mut last_speed = default_speed;

        for (rp, kin) in self.world.query::<(&RoadPosition, &Kinematics)>().iter() {
            if rp.edge_index == edge_id && rp.offset_m < min_offset {
                min_offset = rp.offset_m;
                last_speed = kin.speed;
            }
        }

        last_speed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use velos_meso::zone_config::{ZoneConfig, ZoneType};

    #[test]
    fn meso_agent_state_preserves_identity() {
        let state = MesoAgentState {
            route: Route {
                path: vec![0, 1, 2],
                current_step: 1,
            },
            vehicle_type: VehicleType::Car,
            idm_params: IdmParams {
                v0: 13.89,
                s0: 2.0,
                t_headway: 1.6,
                a: 1.0,
                b: 2.0,
                delta: 4.0,
            },
            cf_model: CarFollowingModel::Idm,
            lateral_offset: None,
        };

        assert_eq!(state.vehicle_type, VehicleType::Car);
        assert_eq!(state.route.path.len(), 3);
        assert!((state.idm_params.v0 - 13.89).abs() < 1e-9);
    }

    #[test]
    fn zone_config_defaults_to_micro() {
        let config = ZoneConfig::new();
        assert_eq!(config.zone_type(42), ZoneType::Micro);
        assert_eq!(config.zone_type(999), ZoneType::Micro);
    }
}
