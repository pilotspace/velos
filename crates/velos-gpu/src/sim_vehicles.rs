//! GPU vehicle physics step for SimWorld.
//!
//! Extracted from sim.rs to keep files under 700 lines.
//! Contains `step_vehicles_gpu()` — the GPU wave-front dispatch
//! for car-following physics (IDM/Krauss).

use hecs::Entity;

use velos_core::components::{
    CarFollowingModel, GpuAgentState, JunctionTraversal, JustExitedJunction, LateralOffset,
    Position, RoadPosition, VehicleType,
};
use velos_core::cost::AgentProfile;
use velos_core::fixed_point::{FixLat, FixPos, FixSpd};

use crate::compute::{compute_agent_flags, sort_agents_by_lane, ComputeDispatcher, GpuEmergencyVehicle};
use crate::sim::SimWorld;
use velos_core::components::Kinematics;

impl SimWorld {
    /// GPU wave-front dispatch for vehicle physics (cars + motorbikes).
    pub(crate) fn step_vehicles_gpu(
        &mut self,
        dt: f32,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        dispatcher: &mut ComputeDispatcher,
    ) {
        let mut gpu_agents: Vec<GpuAgentState> = Vec::new();
        let mut entity_map: Vec<Entity> = Vec::new();
        let mut emergency_list: Vec<GpuEmergencyVehicle> = Vec::new();

        // Collect agents that just exited a junction this frame — skip them
        // to prevent single-frame teleport to the next junction.
        let just_exited: std::collections::HashSet<Entity> = self
            .world
            .query_mut::<(Entity, &JustExitedJunction)>()
            .into_iter()
            .map(|(e, _)| e)
            .collect();

        for (entity, rp, kin, vtype, lat, cf_model, bus_state, pos, agent_profile, jt) in self
            .world
            .query_mut::<(
                Entity,
                &RoadPosition,
                &Kinematics,
                &VehicleType,
                Option<&LateralOffset>,
                Option<&CarFollowingModel>,
                Option<&velos_vehicle::bus::BusState>,
                &Position,
                Option<&AgentProfile>,
                Option<&JunctionTraversal>,
            )>()
            .into_iter()
        {
            if *vtype == VehicleType::Pedestrian {
                continue;
            }
            // Bug 6 fix: skip junction-traversing agents from edge-based physics
            if jt.is_some() {
                continue;
            }
            // Skip agents that just exited a junction this frame to prevent
            // single-frame teleport (step_vehicles overshoots exit edge → enters next junction).
            if just_exited.contains(&entity) {
                continue;
            }

            let cf = cf_model.copied().unwrap_or(CarFollowingModel::Idm);
            let rng_seed = entity.id();
            let profile = agent_profile.copied().unwrap_or(AgentProfile::Commuter);

            let vtype_gpu = match *vtype {
                VehicleType::Motorbike => 0,
                VehicleType::Car => 1,
                VehicleType::Bus => 2,
                VehicleType::Bicycle => 3,
                VehicleType::Truck => 4,
                VehicleType::Emergency => 5,
                VehicleType::Pedestrian => 6,
            };

            let is_dwelling = bus_state.is_some_and(|bs| bs.is_dwelling());
            let is_emergency = *vtype == VehicleType::Emergency;

            gpu_agents.push(GpuAgentState {
                edge_id: rp.edge_index,
                lane_idx: rp.lane as u32,
                position: FixPos::from_f64(rp.offset_m).raw(),
                lateral: FixLat::from_f64(lat.map_or(0.0, |l| l.lateral_offset)).raw(),
                speed: FixSpd::from_f64(kin.speed).raw(),
                acceleration: 0,
                cf_model: cf as u32,
                rng_state: rng_seed,
                vehicle_type: vtype_gpu,
                flags: compute_agent_flags(is_dwelling, is_emergency, profile),
            });
            entity_map.push(entity);

            // Collect emergency vehicle world positions for yield cone buffer.
            if is_emergency {
                emergency_list.push(GpuEmergencyVehicle {
                    pos_x: pos.x as f32,
                    pos_y: pos.y as f32,
                    heading: kin.heading as f32,
                    _pad: 0.0,
                });
            }
        }

        if gpu_agents.is_empty() {
            // Still upload empty emergency list to reset count to 0.
            dispatcher.upload_emergency_vehicles(queue, &emergency_list);
            return;
        }

        // Upload emergency vehicle positions for GPU yield cone detection.
        dispatcher.upload_emergency_vehicles(queue, &emergency_list);

        let (lane_offsets, lane_counts, lane_agent_indices) = sort_agents_by_lane(&gpu_agents);

        dispatcher.upload_wave_front_data(
            device,
            queue,
            &gpu_agents,
            &lane_offsets,
            &lane_counts,
            &lane_agent_indices,
        );

        let mut encoder = device.create_command_encoder(&Default::default());
        dispatcher.dispatch_wave_front(&mut encoder, device, queue, dt);
        queue.submit(std::iter::once(encoder.finish()));

        let updated = dispatcher.readback_wave_front_agents(device, queue);

        for (i, gpu_state) in updated.iter().enumerate() {
            if i >= entity_map.len() {
                break;
            }
            let entity = entity_map[i];

            let new_offset = FixPos::from_raw(gpu_state.position).to_f64();
            let new_speed = FixSpd::from_raw(gpu_state.speed).to_f64();

            let at_red = {
                let Ok(rp) = self.world.query_one_mut::<&RoadPosition>(entity) else {
                    continue;
                };
                let rp_copy = *rp;
                self.check_signal_red(&rp_copy)
            };
            self.apply_vehicle_update(entity, new_speed, new_offset, at_red);

            if let Ok(lat) = self.world.query_one_mut::<&mut LateralOffset>(entity) {
                let new_lateral = FixLat::from_raw(gpu_state.lateral).to_f64();
                lat.lateral_offset = new_lateral;
                lat.desired_lateral = new_lateral;
                self.apply_lateral_world_offset(entity, new_lateral);
            }
        }
    }
}
