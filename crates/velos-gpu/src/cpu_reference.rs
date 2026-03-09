//! CPU reference implementations of vehicle physics.
//!
//! Kept for GPU validation testing. NOT used in the production sim loop.
//! Production uses `SimWorld::step_vehicles_gpu()` via wave-front dispatch.

use std::collections::HashMap;

use hecs::Entity;

use velos_core::components::{
    JunctionTraversal, JustExitedJunction, Kinematics, LaneChangeState, LateralOffset, Position,
    RoadPosition, VehicleType,
};
use velos_net::SpatialIndex;
use velos_vehicle::idm::{idm_acceleration, integrate_with_stopping_guard, IdmParams};
use velos_vehicle::sublane::{self, NeighborInfo};

use crate::sim::SimWorld;
use crate::sim_snapshot::AgentSnapshot;

/// CPU step for car vehicles (IDM + MOBIL). Test/validation reference only.
pub fn step_vehicles(
    sim: &mut SimWorld,
    dt: f64,
    spatial: &SpatialIndex,
    snapshot: &AgentSnapshot,
) {
    use crate::sim_mobil::CarMobilContext;
    use std::collections::HashSet;

    let just_exited: HashSet<Entity> = sim
        .world
        .query_mut::<(Entity, &JustExitedJunction)>()
        .into_iter()
        .map(|(e, _)| e)
        .collect();

    struct CarSnap {
        entity: Entity,
        rp: RoadPosition,
        speed: f64,
        heading: f64,
        idm: IdmParams,
        pos: [f64; 2],
        has_lc: bool,
    }

    let agents: Vec<CarSnap> = sim
        .world
        .query_mut::<(
            Entity,
            &RoadPosition,
            &Kinematics,
            &IdmParams,
            &VehicleType,
            &Position,
            Option<&LaneChangeState>,
            Option<&JunctionTraversal>,
        )>()
        .into_iter()
        // Bug 6 fix: skip junction-traversing agents from edge-based physics
        .filter(|(e, _, _, _, vt, _, _, jt)| {
            **vt == VehicleType::Car && jt.is_none() && !just_exited.contains(e)
        })
        .map(|(e, rp, kin, idm, _, pos, lcs, _)| CarSnap {
            entity: e,
            rp: *rp,
            speed: kin.speed,
            heading: kin.heading,
            idm: *idm,
            pos: [pos.x, pos.y],
            has_lc: lcs.is_some(),
        })
        .collect();

    let mut edge_agents: HashMap<u32, Vec<(Entity, f64)>> = HashMap::new();
    let mut edge_lane_agents: HashMap<(u32, u8), Vec<(Entity, f64)>> = HashMap::new();
    for car in &agents {
        edge_agents
            .entry(car.rp.edge_index)
            .or_default()
            .push((car.entity, car.rp.offset_m));
        edge_lane_agents
            .entry((car.rp.edge_index, car.rp.lane))
            .or_default()
            .push((car.entity, car.rp.offset_m));
    }
    for v in edge_agents.values_mut() {
        v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    }
    for v in edge_lane_agents.values_mut() {
        v.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    }

    let speed_map: HashMap<Entity, f64> =
        agents.iter().map(|c| (c.entity, c.speed)).collect();

    struct CarUpdate {
        entity: Entity,
        v_new: f64,
        new_offset: f64,
        at_red: bool,
        start_lane_change: Option<(u8, f64)>,
    }

    let mut updates: Vec<CarUpdate> = Vec::with_capacity(agents.len());

    for car in &agents {
        let at_red = sim.check_signal_red(&car.rp);

        let (mut gap, mut delta_v) = if at_red {
            (2.0, car.speed)
        } else {
            SimWorld::find_leader_static(
                car.entity, &car.rp, &edge_agents, &speed_map, car.speed,
            )
        };

        let nearby = spatial.nearest_within_radius(car.pos, 8.0);
        for neighbor in &nearby {
            let dx = neighbor.pos[0] - car.pos[0];
            let dy = neighbor.pos[1] - car.pos[1];
            let longitudinal = dx * car.heading.cos() + dy * car.heading.sin();
            let lateral = (-dx * car.heading.sin() + dy * car.heading.cos()).abs();
            if longitudinal < 2.0 || lateral > 2.0 {
                continue;
            }
            if let Some(vt) = snapshot.vehicle_type(neighbor.id)
                && vt == VehicleType::Pedestrian
                && longitudinal < gap
            {
                let ped_speed = snapshot.speed(neighbor.id).unwrap_or(0.0);
                gap = longitudinal;
                delta_v = (car.speed - ped_speed).max(0.0);
            }
        }

        let accel_current = idm_acceleration(&car.idm, car.speed, gap, delta_v);
        let (v_new, dx) = integrate_with_stopping_guard(car.speed, accel_current, dt);

        let start_lc = if !at_red {
            let ctx = CarMobilContext {
                entity: car.entity,
                rp: car.rp,
                speed: car.speed,
                idm_params: car.idm,
                has_lane_change: car.has_lc,
            };
            sim.evaluate_mobil(&ctx, accel_current, &edge_lane_agents, &speed_map)
        } else {
            None
        };

        updates.push(CarUpdate {
            entity: car.entity,
            v_new,
            new_offset: car.rp.offset_m + dx,
            at_red,
            start_lane_change: start_lc,
        });
    }

    for upd in updates {
        if let Some((target_lane, started_at)) = upd.start_lane_change {
            sim.start_lane_change(upd.entity, target_lane, started_at);
        }
        sim.apply_vehicle_update(upd.entity, upd.v_new, upd.new_offset, upd.at_red);
    }

    sim.process_car_lane_changes(dt);

    let car_offsets: Vec<(Entity, f64, bool)> = sim
        .world
        .query_mut::<(Entity, &LateralOffset, &VehicleType, Option<&LaneChangeState>)>()
        .into_iter()
        .filter(|(_, _, vt, _)| **vt == VehicleType::Car)
        .map(|(e, lat, _, lcs)| (e, lat.lateral_offset, lcs.is_some()))
        .collect();
    for (entity, lateral, has_lc) in car_offsets {
        if !has_lc {
            sim.apply_lateral_world_offset(entity, lateral);
        }
    }
}

/// CPU step for motorbike agents with sublane lateral positioning.
/// Test/validation reference only.
pub fn step_motorbikes_sublane(
    sim: &mut SimWorld,
    dt: f64,
    spatial: &SpatialIndex,
    snapshot: &AgentSnapshot,
) {
    use petgraph::graph::EdgeIndex;
    use std::collections::HashSet;

    let just_exited: HashSet<Entity> = sim
        .world
        .query_mut::<(Entity, &JustExitedJunction)>()
        .into_iter()
        .map(|(e, _)| e)
        .collect();

    struct BikeState {
        entity: Entity,
        rp: RoadPosition,
        speed: f64,
        idm_params: IdmParams,
        lateral: f64,
        heading: f64,
        pos: [f64; 2],
    }

    let bikes: Vec<BikeState> = sim
        .world
        .query_mut::<(
            Entity,
            &RoadPosition,
            &Kinematics,
            &IdmParams,
            &LateralOffset,
            &VehicleType,
            &Position,
            Option<&JunctionTraversal>,
        )>()
        .into_iter()
        // Bug 6 fix: skip junction-traversing agents from edge-based physics
        .filter(|(e, _, _, _, _, vt, _, jt)| {
            **vt == VehicleType::Motorbike && jt.is_none() && !just_exited.contains(e)
        })
        .map(|(e, rp, kin, idm, lat, _, pos, _)| BikeState {
            entity: e,
            rp: *rp,
            speed: kin.speed,
            idm_params: *idm,
            lateral: lat.lateral_offset,
            heading: kin.heading,
            pos: [pos.x, pos.y],
        })
        .collect();

    struct BikeUpdate {
        entity: Entity,
        v_new: f64,
        new_offset: f64,
        new_lateral: f64,
        at_red: bool,
    }

    let mut updates: Vec<BikeUpdate> = Vec::with_capacity(bikes.len());

    for bike in &bikes {
        let (entity, rp, speed, idm_params, lateral, heading, agent_pos) = (
            &bike.entity,
            &bike.rp,
            &bike.speed,
            &bike.idm_params,
            &bike.lateral,
            &bike.heading,
            &bike.pos,
        );
        let at_red = sim.check_signal_red(rp);

        let edge = EdgeIndex::new(rp.edge_index as usize);
        let lane_count = sim
            .road_graph
            .inner()
            .edge_weight(edge)
            .map(|e| e.lane_count as f64)
            .unwrap_or(2.0);
        let road_width = lane_count * 3.5;

        let nearby = spatial.nearest_within_radius_capped(*agent_pos, 6.0, 20);

        let mut neighbor_infos = Vec::new();
        let mut idm_gap = 1000.0_f64;
        let mut idm_delta_v = 0.0_f64;

        for n in &nearby {
            let dx = n.pos[0] - agent_pos[0];
            let dy = n.pos[1] - agent_pos[1];
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < 0.0001 {
                continue;
            }
            let Some(n_vtype) = snapshot.vehicle_type(n.id) else {
                continue;
            };

            let n_heading = snapshot.heading(n.id).unwrap_or(0.0);
            let angle_diff = n_heading - heading;
            if angle_diff.cos() < 0.0 {
                continue;
            }

            let n_speed = snapshot.speed(n.id).unwrap_or(0.0);
            let n_half_width = AgentSnapshot::half_width_for_type(n_vtype);
            let n_lateral = snapshot.lateral_offset(n.id).unwrap_or(road_width / 2.0);

            let longitudinal = dx * heading.cos() + dy * heading.sin();

            if n_vtype != VehicleType::Pedestrian {
                neighbor_infos.push(NeighborInfo {
                    lateral_offset: n_lateral,
                    longitudinal_gap: longitudinal,
                    half_width: n_half_width,
                    speed: n_speed,
                });

                let lateral_dist = (-dx * heading.sin() + dy * heading.cos()).abs();
                if longitudinal > 0.0 && lateral_dist < 0.8 && longitudinal < idm_gap {
                    idm_gap = longitudinal;
                    idm_delta_v = *speed - n_speed;
                }
            }
        }

        if at_red && *speed < 0.5 && idm_gap > 2.0 {
            idm_gap = 2.0;
            idm_delta_v = *speed;
        }

        let desired = sublane::compute_desired_lateral(
            *lateral,
            *speed,
            road_width,
            &neighbor_infos,
            at_red,
            &sim.sublane_params,
        );
        let max_lat_speed = if at_red {
            sim.sublane_params.swarm_lateral_speed
        } else {
            sim.sublane_params.max_lateral_speed
        };
        let new_lateral = sublane::apply_lateral_drift(*lateral, desired, max_lat_speed, dt);

        let accel = idm_acceleration(idm_params, *speed, idm_gap, idm_delta_v);
        let (v_new, ddx) = integrate_with_stopping_guard(*speed, accel, dt);

        updates.push(BikeUpdate {
            entity: *entity,
            v_new,
            new_offset: rp.offset_m + ddx,
            new_lateral,
            at_red,
        });
    }

    for upd in updates {
        if let Ok(lat) = sim.world.query_one_mut::<&mut LateralOffset>(upd.entity) {
            lat.lateral_offset = upd.new_lateral;
            lat.desired_lateral = upd.new_lateral;
        }

        sim.apply_vehicle_update(upd.entity, upd.v_new, upd.new_offset, upd.at_red);
        sim.apply_lateral_world_offset(upd.entity, upd.new_lateral);
    }
}
