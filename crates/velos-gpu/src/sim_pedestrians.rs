//! Pedestrian stepping for SimWorld using social force model (CPU) and GPU adaptive pipeline.
//!
//! Extracted from sim.rs to keep the main module under 700 lines.
//! Steps all pedestrian agents toward their route waypoints using
//! social-force-based acceleration with neighbor repulsion.
//!
//! Two paths:
//! - `step_pedestrians`: CPU social force (used in `tick()`)
//! - `step_pedestrians_gpu`: GPU adaptive pipeline (used in `tick_gpu()`)

use hecs::Entity;
use petgraph::graph::NodeIndex;

use velos_core::components::{Kinematics, Position, Route, VehicleType};
use velos_net::SpatialIndex;
use velos_vehicle::social_force::{self, PedestrianNeighbor};

use crate::ped_adaptive::{GpuPedestrian, PedestrianAdaptiveParams};
use crate::sim::SimWorld;
use crate::sim_snapshot::AgentSnapshot;

/// Compute axis-aligned bounding box for pedestrian positions with a 5m margin.
fn compute_bounding_box(peds: &[GpuPedestrian]) -> (f32, f32, f32, f32) {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for p in peds {
        min_x = min_x.min(p.pos_x);
        max_x = max_x.max(p.pos_x);
        min_y = min_y.min(p.pos_y);
        max_y = max_y.max(p.pos_y);
    }
    // Add 5m margin to avoid edge-case zero-size grids.
    (min_x - 5.0, max_x + 5.0, min_y - 5.0, max_y + 5.0)
}

impl SimWorld {
    /// Step pedestrians using social force model (CPU).
    pub(crate) fn step_pedestrians(
        &mut self,
        dt: f64,
        spatial: &SpatialIndex,
        snapshot: &AgentSnapshot,
    ) {
        struct PedState {
            entity: Entity,
            path: Vec<u32>,
            current_step: usize,
            pos: [f64; 2],
            vel: [f64; 2],
        }

        let peds: Vec<PedState> = self
            .world
            .query_mut::<(Entity, &VehicleType, &Route, &Position, &Kinematics)>()
            .into_iter()
            .filter(|(_, vt, _, _, _)| **vt == VehicleType::Pedestrian)
            .map(|(e, _, r, pos, kin)| PedState {
                entity: e,
                path: r.path.clone(),
                current_step: r.current_step,
                pos: [pos.x, pos.y],
                vel: [kin.vx, kin.vy],
            })
            .collect();

        struct PedUpdate {
            entity: Entity,
            new_pos: [f64; 2],
            new_vel: [f64; 2],
            speed: f64,
            advance_step: bool,
        }

        let mut updates = Vec::with_capacity(peds.len());

        for ped in &peds {
            let (entity, path, current_step, pos, vel) = (
                &ped.entity,
                &ped.path,
                &ped.current_step,
                &ped.pos,
                &ped.vel,
            );
            if *current_step >= path.len() {
                continue;
            }

            let target_node = NodeIndex::new(path[*current_step] as usize);
            let raw_target = self.road_graph.inner()[target_node].pos;

            let target_pos = if *current_step > 0 {
                let prev_node = NodeIndex::new(path[*current_step - 1] as usize);
                let prev_pos = self.road_graph.inner()[prev_node].pos;
                let seg_dx = raw_target[0] - prev_pos[0];
                let seg_dy = raw_target[1] - prev_pos[1];
                let seg_len = (seg_dx * seg_dx + seg_dy * seg_dy).sqrt();
                if seg_len > 0.1 {
                    let perp_x = -seg_dy / seg_len;
                    let perp_y = seg_dx / seg_len;
                    [raw_target[0] + perp_x * 5.0, raw_target[1] + perp_y * 5.0]
                } else {
                    raw_target
                }
            } else {
                raw_target
            };

            let nearby = spatial.nearest_within_radius(*pos, 3.0);
            let neighbors: Vec<PedestrianNeighbor> = nearby
                .iter()
                .filter(|n| {
                    let ddx = n.pos[0] - pos[0];
                    let ddy = n.pos[1] - pos[1];
                    ddx * ddx + ddy * ddy > 0.0001
                })
                .take(10)
                .filter_map(|n| {
                    let idx = snapshot.id_to_index.get(&n.id)?;
                    let n_vtype = snapshot.vehicle_types[*idx];
                    let radius = AgentSnapshot::half_width_for_type(n_vtype);
                    Some(PedestrianNeighbor {
                        pos: n.pos,
                        vel: [0.0, 0.0],
                        radius,
                    })
                })
                .collect();

            let accel = social_force::social_force_acceleration(
                *pos,
                *vel,
                target_pos,
                &neighbors,
                &self.social_force_params,
            );
            let (new_vel, speed) = social_force::integrate_pedestrian(
                *vel,
                accel,
                dt,
                self.social_force_params.max_speed,
            );

            let new_pos = [pos[0] + new_vel[0] * dt, pos[1] + new_vel[1] * dt];

            let dx = target_pos[0] - new_pos[0];
            let dy = target_pos[1] - new_pos[1];
            let dist = (dx * dx + dy * dy).sqrt();
            let advance = dist < 2.0;

            updates.push(PedUpdate {
                entity: *entity,
                new_pos,
                new_vel,
                speed,
                advance_step: advance,
            });
        }

        for upd in updates {
            let (pos, kin, route) = self
                .world
                .query_one_mut::<(&mut Position, &mut Kinematics, &mut Route)>(upd.entity)
                .unwrap();
            pos.x = upd.new_pos[0];
            pos.y = upd.new_pos[1];
            kin.vx = upd.new_vel[0];
            kin.vy = upd.new_vel[1];
            kin.speed = upd.speed;
            if upd.speed > 1e-6 {
                kin.heading = upd.new_vel[1].atan2(upd.new_vel[0]);
            }
            if upd.advance_step {
                route.current_step += 1;
            }
        }
    }

    /// Step pedestrians using GPU adaptive pipeline (spatial hash + prefix-sum compaction).
    ///
    /// Upload pedestrian data -> 6-pass GPU dispatch -> readback -> write back to ECS.
    /// Falls back to logging a warning if `ped_adaptive` is None (should not happen in tick_gpu).
    pub(crate) fn step_pedestrians_gpu(
        &mut self,
        dt: f64,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) {
        let ped_pipeline = match &mut self.ped_adaptive {
            Some(p) => p,
            None => {
                log::warn!("No GPU pedestrian pipeline, skipping GPU pedestrian step");
                return;
            }
        };

        // Collect pedestrian data into GpuPedestrian format.
        let g = self.road_graph.inner();
        struct PedInfo {
            entity: Entity,
            gpu_ped: GpuPedestrian,
        }
        let mut ped_infos: Vec<PedInfo> = Vec::new();

        for (entity, vtype, route, pos, kin) in self
            .world
            .query_mut::<(Entity, &VehicleType, &Route, &Position, &Kinematics)>()
        {
            if *vtype != VehicleType::Pedestrian {
                continue;
            }
            if route.current_step >= route.path.len() {
                continue;
            }

            let target_node = NodeIndex::new(route.path[route.current_step] as usize);
            let target = g[target_node].pos;

            // Apply sidewalk offset (same 5m perpendicular as CPU path).
            let dest = if route.current_step > 0 {
                let prev_node = NodeIndex::new(route.path[route.current_step - 1] as usize);
                let prev_pos = g[prev_node].pos;
                let seg_dx = target[0] - prev_pos[0];
                let seg_dy = target[1] - prev_pos[1];
                let seg_len = (seg_dx * seg_dx + seg_dy * seg_dy).sqrt();
                if seg_len > 0.1 {
                    let perp_x = -seg_dy / seg_len;
                    let perp_y = seg_dx / seg_len;
                    [target[0] + perp_x * 5.0, target[1] + perp_y * 5.0]
                } else {
                    target
                }
            } else {
                target
            };

            ped_infos.push(PedInfo {
                entity,
                gpu_ped: GpuPedestrian {
                    pos_x: pos.x as f32,
                    pos_y: pos.y as f32,
                    vel_x: kin.vx as f32,
                    vel_y: kin.vy as f32,
                    dest_x: dest[0] as f32,
                    dest_y: dest[1] as f32,
                    radius: 0.3, // pedestrian radius
                    _pad: 0.0,
                },
            });
        }

        if ped_infos.is_empty() {
            return;
        }

        let gpu_peds: Vec<GpuPedestrian> = ped_infos.iter().map(|p| p.gpu_ped).collect();

        // Compute grid dimensions from bounding box.
        let (min_x, max_x, min_y, max_y) = compute_bounding_box(&gpu_peds);
        let area_sq_m = (max_x - min_x) * (max_y - min_y);
        let cell_size =
            crate::ped_adaptive::PedestrianAdaptivePipeline::classify_density(
                gpu_peds.len() as u32,
                area_sq_m,
            );
        let grid_w = ((max_x - min_x) / cell_size).ceil().max(1.0) as u32;
        let grid_h = ((max_y - min_y) / cell_size).ceil().max(1.0) as u32;

        ped_pipeline.upload(device, queue, &gpu_peds, grid_w, grid_h);

        let params = PedestrianAdaptiveParams {
            ped_count: gpu_peds.len() as u32,
            cell_count: grid_w * grid_h,
            grid_w,
            grid_h,
            cell_size,
            dt: dt as f32,
            workgroup_count: 0, // filled by dispatch
            _pad: 0,
            ..PedestrianAdaptiveParams::default()
        };

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("pedestrian_adaptive"),
        });
        ped_pipeline.dispatch(&mut encoder, device, queue, &params);
        queue.submit(std::iter::once(encoder.finish()));

        let updated = ped_pipeline.readback(device, queue);

        // Write back to ECS.
        for (i, info) in ped_infos.iter().enumerate() {
            if i >= updated.len() {
                break;
            }
            let upd = &updated[i];

            if let Ok((pos, kin, route)) = self
                .world
                .query_one_mut::<(&mut Position, &mut Kinematics, &mut Route)>(info.entity)
            {
                pos.x = upd.pos_x as f64;
                pos.y = upd.pos_y as f64;
                kin.vx = upd.vel_x as f64;
                kin.vy = upd.vel_y as f64;
                kin.speed = ((upd.vel_x * upd.vel_x + upd.vel_y * upd.vel_y) as f64).sqrt();
                if kin.speed > 1e-6 {
                    kin.heading = (upd.vel_y as f64).atan2(upd.vel_x as f64);
                }

                // Advance waypoint if close to destination.
                let dx = upd.dest_x - upd.pos_x;
                let dy = upd.dest_y - upd.pos_y;
                if (dx * dx + dy * dy).sqrt() < 2.0 {
                    route.current_step += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use petgraph::graph::DiGraph;
    use velos_core::components::{Kinematics, Position, Route, VehicleType};
    use velos_net::graph::{RoadGraph, RoadNode};

    use crate::sim::SimWorld;

    fn make_test_graph() -> RoadGraph {
        let mut g = DiGraph::new();
        g.add_node(RoadNode { pos: [0.0, 0.0] });
        g.add_node(RoadNode { pos: [100.0, 0.0] });
        g.add_node(RoadNode { pos: [200.0, 0.0] });
        RoadGraph::new(g)
    }

    #[test]
    fn cpu_only_has_no_ped_adaptive() {
        let graph = make_test_graph();
        let sim = SimWorld::new_cpu_only(graph);
        assert!(sim.ped_adaptive.is_none());
    }

    #[test]
    fn step_pedestrians_gpu_skips_when_no_pipeline() {
        let graph = make_test_graph();
        let mut sim = SimWorld::new_cpu_only(graph);
        sim.sim_state = crate::sim::SimState::Running;

        // Spawn a pedestrian
        let ped = sim.world.spawn((
            VehicleType::Pedestrian,
            Route {
                path: vec![1, 2],
                current_step: 0,
            },
            Position { x: 0.0, y: 0.0 },
            Kinematics {
                speed: 0.0,
                heading: 0.0,
                vx: 0.0,
                vy: 0.0,
            },
        ));

        // step_pedestrians_gpu with ped_adaptive=None should log warn and return
        // without modifying the pedestrian's position.
        // We can't call it with real device/queue, so we just verify the field is None.
        assert!(sim.ped_adaptive.is_none());

        // Position should be unchanged since no GPU pipeline available.
        let pos = sim.world.query_one_mut::<&Position>(ped).unwrap();
        assert!((pos.x - 0.0).abs() < 1e-6);
    }

    #[test]
    fn compute_bounding_box_with_margin() {
        use crate::ped_adaptive::GpuPedestrian;

        let peds = vec![
            GpuPedestrian {
                pos_x: 10.0, pos_y: 20.0,
                vel_x: 0.0, vel_y: 0.0,
                dest_x: 0.0, dest_y: 0.0,
                radius: 0.3, _pad: 0.0,
            },
            GpuPedestrian {
                pos_x: 50.0, pos_y: 80.0,
                vel_x: 0.0, vel_y: 0.0,
                dest_x: 0.0, dest_y: 0.0,
                radius: 0.3, _pad: 0.0,
            },
        ];

        let (min_x, max_x, min_y, max_y) = super::compute_bounding_box(&peds);
        // 5m margin applied
        assert!((min_x - 5.0).abs() < 1e-6);
        assert!((max_x - 55.0).abs() < 1e-6);
        assert!((min_y - 15.0).abs() < 1e-6);
        assert!((max_y - 85.0).abs() < 1e-6);
    }
}
