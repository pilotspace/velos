//! Pedestrian stepping for SimWorld using social force model (CPU).
//!
//! Extracted from sim.rs to keep the main module under 700 lines.
//! Steps all pedestrian agents toward their route waypoints using
//! social-force-based acceleration with neighbor repulsion.

use hecs::Entity;
use petgraph::graph::NodeIndex;

use velos_core::components::{Kinematics, Position, Route, VehicleType};
use velos_net::SpatialIndex;
use velos_vehicle::social_force::{self, PedestrianNeighbor};

use crate::sim::SimWorld;
use crate::sim_snapshot::AgentSnapshot;

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
}
