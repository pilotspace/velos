//! Rendering helpers for SimWorld: instance building, signals, road lines.

use petgraph::visit::EdgeRef;
use petgraph::Direction;

use velos_core::components::{
    CarFollowingModel, JunctionTraversal, Kinematics, Position, VehicleType, WaitState,
};
use velos_signal::plan::PhaseState;
use velos_vehicle::bus::BusState;

use crate::renderer::AgentInstance;
use crate::sim::SimWorld;

/// Return the standard display color for a given vehicle type.
///
/// Per CONTEXT.md locked decisions:
///   Motorbike=orange, Car=blue, Bus=green (route override), Truck=red,
///   Emergency=white, Bicycle=yellow, Pedestrian=light grey.
pub fn vehicle_type_color(vtype: VehicleType) -> [f32; 4] {
    match vtype {
        VehicleType::Motorbike => [1.0, 0.6, 0.0, 1.0],
        VehicleType::Car => [0.2, 0.4, 1.0, 1.0],
        VehicleType::Bus => [0.2, 0.8, 0.2, 1.0],
        VehicleType::Truck => [0.9, 0.2, 0.2, 1.0],
        VehicleType::Emergency => [1.0, 1.0, 1.0, 1.0],
        VehicleType::Bicycle => [0.9, 0.9, 0.2, 1.0],
        VehicleType::Pedestrian => [0.9, 0.9, 0.9, 1.0],
    }
}

/// Compute heading from a Bezier tangent vector. Returns atan2(dy, dx).
///
/// Falls back to `fallback` if the tangent produces a non-finite heading.
pub fn heading_from_tangent(tangent: [f64; 2], fallback: f32) -> f32 {
    let heading = tangent[1].atan2(tangent[0]);
    if heading.is_finite() {
        heading as f32
    } else {
        fallback
    }
}

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
    ///
    /// Vehicle-type coloring (ISL-02 locked decisions):
    ///   Motorbike: orange, Car: blue, Bus: green, Truck: red,
    ///   Emergency: white, Bicycle: yellow, Pedestrian: light grey.
    ///
    /// Agents in junctions use Bezier tangent heading (B'(t) = 2(1-t)(P1-P0) + 2t(P2-P1))
    /// instead of kinematic heading, so they visually rotate to follow curves.
    pub fn build_instances(
        &self,
    ) -> (Vec<AgentInstance>, Vec<AgentInstance>, Vec<AgentInstance>) {
        let mut motorbikes = Vec::new();
        let mut cars = Vec::new();
        let mut pedestrians = Vec::new();

        for (pos, kin, vtype, _ws, _cf_model, bus_state, jt) in self
            .world
            .query::<(
                &Position,
                &Kinematics,
                &VehicleType,
                Option<&WaitState>,
                Option<&CarFollowingModel>,
                Option<&BusState>,
                Option<&JunctionTraversal>,
            )>()
            .iter()
        {
            // Bug 7 fix: skip agents with NaN/Inf positions
            if !pos.x.is_finite() || !pos.y.is_finite() {
                log::warn!("Skipping agent with non-finite position: ({}, {})", pos.x, pos.y);
                continue;
            }

            // Vehicle-type coloring per CONTEXT.md locked decisions.
            // Buses use per-route color; all others use standard vehicle_type_color.
            let color = if *vtype == VehicleType::Bus {
                let ri = bus_state.map(|bs| bs.route_index()).unwrap_or(0);
                BUS_ROUTE_COLORS[ri as usize % BUS_ROUTE_COLORS.len()]
            } else {
                vehicle_type_color(*vtype)
            };

            // Bezier tangent heading for junction agents.
            let heading = if let Some(jt) = jt {
                self.junction_heading(jt)
            } else {
                kin.heading as f32
            };

            let instance = AgentInstance {
                position: [pos.x as f32, pos.y as f32],
                heading,
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

    /// Compute heading from Bezier tangent for an agent traversing a junction.
    ///
    /// Falls back to 0.0 if the junction data is missing or the tangent is degenerate.
    fn junction_heading(&self, jt: &JunctionTraversal) -> f32 {
        if let Some(jd) = self.junction_data.get(&jt.junction_node)
            && let Some(turn) = jd.turns.get(jt.turn_index as usize)
        {
            let tan = turn.tangent(jt.t);
            return heading_from_tangent(tan, 0.0);
        }
        0.0
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_4;

    #[test]
    fn vehicle_type_color_motorbike_orange() {
        let c = vehicle_type_color(VehicleType::Motorbike);
        assert_eq!(c, [1.0, 0.6, 0.0, 1.0]);
    }

    #[test]
    fn vehicle_type_color_car_blue() {
        let c = vehicle_type_color(VehicleType::Car);
        assert_eq!(c, [0.2, 0.4, 1.0, 1.0]);
    }

    #[test]
    fn vehicle_type_color_bus_green() {
        let c = vehicle_type_color(VehicleType::Bus);
        assert_eq!(c, [0.2, 0.8, 0.2, 1.0]);
    }

    #[test]
    fn vehicle_type_color_truck_red() {
        let c = vehicle_type_color(VehicleType::Truck);
        assert_eq!(c, [0.9, 0.2, 0.2, 1.0]);
    }

    #[test]
    fn vehicle_type_color_emergency_white() {
        let c = vehicle_type_color(VehicleType::Emergency);
        assert_eq!(c, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn vehicle_type_color_bicycle_yellow() {
        let c = vehicle_type_color(VehicleType::Bicycle);
        assert_eq!(c, [0.9, 0.9, 0.2, 1.0]);
    }

    #[test]
    fn vehicle_type_color_pedestrian_grey() {
        let c = vehicle_type_color(VehicleType::Pedestrian);
        assert_eq!(c, [0.9, 0.9, 0.9, 1.0]);
    }

    #[test]
    fn heading_from_tangent_east() {
        // Tangent pointing east: atan2(0, 1) = 0
        let h = heading_from_tangent([1.0, 0.0], -999.0);
        assert!((h - 0.0).abs() < 1e-6);
    }

    #[test]
    fn heading_from_tangent_northeast() {
        // Tangent pointing NE: atan2(1, 1) = pi/4
        let h = heading_from_tangent([1.0, 1.0], -999.0);
        assert!((h - FRAC_PI_4 as f32).abs() < 1e-5);
    }

    #[test]
    fn heading_from_tangent_degenerate_uses_fallback() {
        // Zero tangent: atan2(0, 0) = 0 on most platforms, but let's test NaN explicitly
        let h = heading_from_tangent([f64::NAN, 0.0], 42.0);
        assert_eq!(h, 42.0);
    }
}
