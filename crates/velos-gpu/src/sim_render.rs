//! Rendering helpers for SimWorld: instance building, signals, road lines.

use petgraph::visit::EdgeRef;
use petgraph::Direction;

use velos_core::components::{CarFollowingModel, Kinematics, Position, VehicleType, WaitState};
use velos_signal::plan::PhaseState;
use velos_vehicle::bus::BusState;

use crate::renderer::AgentInstance;
use crate::sim::SimWorld;

/// Distinct colors for up to 8 bus routes, then wraps.
const BUS_ROUTE_COLORS: [[f32; 4]; 8] = [
    [1.0, 0.84, 0.0, 1.0],   // gold
    [0.0, 0.75, 0.4, 1.0],   // emerald
    [0.85, 0.2, 0.2, 1.0],   // crimson
    [0.2, 0.6, 1.0, 1.0],    // dodger blue
    [0.93, 0.5, 0.0, 1.0],   // tangerine
    [0.6, 0.2, 0.8, 1.0],    // purple
    [0.0, 0.8, 0.8, 1.0],    // teal
    [0.9, 0.4, 0.6, 1.0],    // rose
];

impl SimWorld {
    /// Build per-type instance arrays for rendering.
    pub fn build_instances(
        &self,
    ) -> (Vec<AgentInstance>, Vec<AgentInstance>, Vec<AgentInstance>) {
        let mut motorbikes = Vec::new();
        let mut cars = Vec::new();
        let mut pedestrians = Vec::new();

        for (pos, kin, vtype, ws, cf_model, bus_state) in self
            .world
            .query::<(
                &Position,
                &Kinematics,
                &VehicleType,
                Option<&WaitState>,
                Option<&CarFollowingModel>,
                Option<&BusState>,
            )>()
            .iter()
        {
            // Position already includes lateral offset (applied in tick).
            // Color-code by car-following model:
            //   IDM: original colors (green/blue)
            //   Krauss: orange/amber tones
            let is_krauss = cf_model == Some(&CarFollowingModel::Krauss);
            let color = match *vtype {
                VehicleType::Motorbike => {
                    let at_red = ws.map(|w| w.at_red_signal).unwrap_or(false);
                    if is_krauss {
                        if at_red {
                            [1.0, 0.7, 0.2, 1.0] // bright orange: Krauss swarming
                        } else {
                            [0.9, 0.6, 0.1, 1.0] // orange: Krauss motorbike
                        }
                    } else if at_red {
                        [0.4, 1.0, 0.5, 1.0] // brighter green: IDM swarming
                    } else {
                        [0.2, 0.8, 0.4, 1.0] // normal green: IDM motorbike
                    }
                }
                VehicleType::Car => {
                    if is_krauss {
                        [0.9, 0.5, 0.1, 1.0] // orange: Krauss car
                    } else {
                        [0.2, 0.4, 0.9, 1.0] // blue: IDM car
                    }
                }
                VehicleType::Bus => {
                    let ri = bus_state.map(|bs| bs.route_index()).unwrap_or(0);
                    BUS_ROUTE_COLORS[ri as usize % BUS_ROUTE_COLORS.len()]
                }
                VehicleType::Bicycle => [0.0, 0.9, 0.9, 1.0],     // cyan
                VehicleType::Truck => [0.6, 0.4, 0.2, 1.0],       // brown
                VehicleType::Emergency => [1.0, 0.0, 0.0, 1.0],   // red
                VehicleType::Pedestrian => [0.9, 0.9, 0.9, 1.0],
            };

            let instance = AgentInstance {
                position: [pos.x as f32, pos.y as f32],
                heading: kin.heading as f32,
                _pad: 0.0,
                color,
            };

            match *vtype {
                VehicleType::Motorbike | VehicleType::Bicycle => motorbikes.push(instance),
                VehicleType::Car | VehicleType::Bus | VehicleType::Truck | VehicleType::Emergency => cars.push(instance),
                VehicleType::Pedestrian => pedestrians.push(instance),
            }
        }

        (motorbikes, cars, pedestrians)
    }

    /// Build signal indicator instances for rendering at signalized intersections.
    pub fn build_signal_indicators(&self) -> Vec<AgentInstance> {
        let g = self.road_graph.inner();
        let mut indicators = Vec::new();

        for (ctrl_node, ctrl) in &self.signal_controllers {
            let node_pos = g[*ctrl_node].pos;
            let incoming: Vec<_> =
                g.edges_directed(*ctrl_node, Direction::Incoming).collect();

            for (approach_idx, edge_ref) in incoming.iter().enumerate() {
                let state = ctrl.get_phase_state(approach_idx);
                let color = match state {
                    PhaseState::Green => [0.0, 1.0, 0.0, 1.0],
                    PhaseState::Amber => [1.0, 0.8, 0.0, 1.0],
                    PhaseState::Red => [1.0, 0.0, 0.0, 1.0],
                };

                let source_pos = g[edge_ref.source()].pos;
                let dx = node_pos[0] - source_pos[0];
                let dy = node_pos[1] - source_pos[1];
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                let offset = 8.0;
                let ix = node_pos[0] - dx / dist * offset;
                let iy = node_pos[1] - dy / dist * offset;

                indicators.push(AgentInstance {
                    position: [ix as f32, iy as f32],
                    heading: 0.0,
                    _pad: 0.0,
                    color,
                });
            }
        }

        indicators
    }

    /// Extract road edge line segments for rendering.
    pub fn road_edge_lines(&self) -> Vec<([f32; 2], [f32; 2])> {
        let g = self.road_graph.inner();
        let mut lines = Vec::with_capacity(g.edge_count());
        for edge in g.edge_weights() {
            let geom = &edge.geometry;
            for w in geom.windows(2) {
                lines.push((
                    [w[0][0] as f32, w[0][1] as f32],
                    [w[1][0] as f32, w[1][1] as f32],
                ));
            }
        }
        lines
    }

    /// Compute bounding box center of the road network for initial camera.
    pub fn network_center(&self) -> (f32, f32) {
        let g = self.road_graph.inner();
        if g.node_count() == 0 {
            return (0.0, 0.0);
        }
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;
        for node in g.node_indices() {
            let pos = g[node].pos;
            min_x = min_x.min(pos[0]);
            max_x = max_x.max(pos[0]);
            min_y = min_y.min(pos[1]);
            max_y = max_y.max(pos[1]);
        }
        (
            ((min_x + max_x) / 2.0) as f32,
            ((min_y + max_y) / 2.0) as f32,
        )
    }
}
