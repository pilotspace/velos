//! Rendering helpers for SimWorld: instance building, signals, road lines.

use std::collections::HashMap;

use petgraph::visit::EdgeRef;
use petgraph::Direction;

use velos_core::components::{
    CarFollowingModel, JunctionTraversal, Kinematics, Position, VehicleType, WaitState,
};
use velos_signal::plan::PhaseState;
use velos_vehicle::bus::BusState;

use velos_api::Camera;
use velos_net::EquirectangularProjection;

use crate::lod::{classify_lod, LodTier};
use crate::orbit_camera::{BillboardInstance3D, MeshInstance3D};
use crate::renderer::{AgentInstance, GuideLineVertex};
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

/// Number of arc segments for FOV cone rendering.
const FOV_CONE_ARC_SEGMENTS: usize = 12;

/// Build camera overlay vertices: a diamond icon at camera position and
/// a semi-transparent FOV cone polygon for each registered camera.
///
/// Called from the application render loop when cameras are registered.
///
/// Returns `GuideLineVertex` data suitable for `Renderer::update_camera_overlay`.
/// The cone is rendered as a triangle fan from the camera position to an arc
/// at `range_m` distance, spanning `heading +/- fov/2`.
///
/// The `projection` converts camera lat/lon to local metres matching the
/// road network coordinate system.
pub fn build_camera_overlay_vertices(
    cameras: &[&Camera],
    projection: &EquirectangularProjection,
    show_cameras: bool,
) -> Vec<GuideLineVertex> {
    if !show_cameras || cameras.is_empty() {
        return Vec::new();
    }

    let mut vertices = Vec::new();

    // Camera icon color: bright yellow, opaque.
    let icon_color = [1.0_f32, 0.9, 0.0, 1.0];
    // FOV cone fill: semi-transparent cyan (visible but not overwhelming).
    let cone_fill_color = [0.0_f32, 0.8, 1.0, 0.30];
    // FOV cone outline: bright cyan.
    let cone_outline_color = [0.0_f32, 0.9, 1.0, 0.8];
    // Outline width in metres.
    let outline_width = 2.0_f32;

    for cam in cameras {
        let (cx, cy) = projection.project(cam.lat, cam.lon);
        let cx = cx as f32;
        let cy = cy as f32;

        // --- Camera icon: diamond (4 triangles forming a square rotated 45deg) ---
        let icon_size = 3.0_f32;
        let base = GuideLineVertex {
            position: [0.0, 0.0],
            color: icon_color,
            line_dist: 0.0,
            _pad: 0.0,
        };
        // Diamond vertices: top, right, bottom, left
        let top = [cx, cy + icon_size];
        let right = [cx + icon_size, cy];
        let bottom = [cx, cy - icon_size];
        let left = [cx - icon_size, cy];
        // 4 triangles: center-based fan
        let center = [cx, cy];
        for &(a, b) in &[(top, right), (right, bottom), (bottom, left), (left, top)] {
            let mut v0 = base;
            v0.position = center;
            let mut v1 = base;
            v1.position = a;
            let mut v2 = base;
            v2.position = b;
            vertices.push(v0);
            vertices.push(v1);
            vertices.push(v2);
        }

        // --- FOV cone: triangle fan from camera position to arc at range_m ---
        // Convert heading from degrees (0=north, clockwise) to radians (math convention).
        // Math convention: 0=east, CCW. North in heading = PI/2 in math.
        // heading_math = PI/2 - heading_deg_to_rad
        let heading_rad =
            std::f32::consts::FRAC_PI_2 - cam.heading_deg.to_radians();
        let half_fov_rad = (cam.fov_deg / 2.0).to_radians();
        let range = cam.range_m;

        let start_angle = heading_rad - half_fov_rad;
        let end_angle = heading_rad + half_fov_rad;
        let angle_step = (end_angle - start_angle) / FOV_CONE_ARC_SEGMENTS as f32;

        // Cone fill triangles (triangle fan)
        let cone_base = GuideLineVertex {
            position: [0.0, 0.0],
            color: cone_fill_color,
            line_dist: 0.0,
            _pad: 0.0,
        };

        for i in 0..FOV_CONE_ARC_SEGMENTS {
            let a0 = start_angle + i as f32 * angle_step;
            let a1 = start_angle + (i + 1) as f32 * angle_step;

            let p0 = [cx + range * a0.cos(), cy + range * a0.sin()];
            let p1 = [cx + range * a1.cos(), cy + range * a1.sin()];

            let mut v_center = cone_base;
            v_center.position = [cx, cy];
            let mut v0 = cone_base;
            v0.position = p0;
            let mut v1 = cone_base;
            v1.position = p1;
            vertices.push(v_center);
            vertices.push(v0);
            vertices.push(v1);
        }

        // Cone outline: left edge, right edge, and arc
        let outline_base = GuideLineVertex {
            position: [0.0, 0.0],
            color: cone_outline_color,
            line_dist: 0.0,
            _pad: 0.0,
        };
        let hw = outline_width / 2.0;

        // Left edge: from camera to left FOV boundary
        {
            let dx = start_angle.cos();
            let dy = start_angle.sin();
            // Normal perpendicular to the edge direction
            let nx = -dy;
            let ny = dx;
            let p0 = [cx, cy];
            let p1 = [cx + range * dx, cy + range * dy];
            let mut v = [outline_base; 6];
            v[0].position = [p0[0] + nx * hw, p0[1] + ny * hw];
            v[1].position = [p0[0] - nx * hw, p0[1] - ny * hw];
            v[2].position = [p1[0] + nx * hw, p1[1] + ny * hw];
            v[3].position = [p0[0] - nx * hw, p0[1] - ny * hw];
            v[4].position = [p1[0] - nx * hw, p1[1] - ny * hw];
            v[5].position = [p1[0] + nx * hw, p1[1] + ny * hw];
            vertices.extend_from_slice(&v);
        }

        // Right edge: from camera to right FOV boundary
        {
            let dx = end_angle.cos();
            let dy = end_angle.sin();
            let nx = -dy;
            let ny = dx;
            let p0 = [cx, cy];
            let p1 = [cx + range * dx, cy + range * dy];
            let mut v = [outline_base; 6];
            v[0].position = [p0[0] + nx * hw, p0[1] + ny * hw];
            v[1].position = [p0[0] - nx * hw, p0[1] - ny * hw];
            v[2].position = [p1[0] + nx * hw, p1[1] + ny * hw];
            v[3].position = [p0[0] - nx * hw, p0[1] - ny * hw];
            v[4].position = [p1[0] - nx * hw, p1[1] - ny * hw];
            v[5].position = [p1[0] + nx * hw, p1[1] + ny * hw];
            vertices.extend_from_slice(&v);
        }

        // Arc outline: quad strip along the arc
        for i in 0..FOV_CONE_ARC_SEGMENTS {
            let a0 = start_angle + i as f32 * angle_step;
            let a1 = start_angle + (i + 1) as f32 * angle_step;

            let p0 = [cx + range * a0.cos(), cy + range * a0.sin()];
            let p1 = [cx + range * a1.cos(), cy + range * a1.sin()];

            // Normal points outward from center
            let n0 = [a0.cos(), a0.sin()];
            let n1 = [a1.cos(), a1.sin()];

            let mut v = [outline_base; 6];
            v[0].position = [p0[0] - n0[0] * hw, p0[1] - n0[1] * hw];
            v[1].position = [p0[0] + n0[0] * hw, p0[1] + n0[1] * hw];
            v[2].position = [p1[0] - n1[0] * hw, p1[1] - n1[1] * hw];
            v[3].position = [p0[0] + n0[0] * hw, p0[1] + n0[1] * hw];
            v[4].position = [p1[0] + n1[0] * hw, p1[1] + n1[1] * hw];
            v[5].position = [p1[0] - n1[0] * hw, p1[1] - n1[1] * hw];
            vertices.extend_from_slice(&v);
        }
    }

    vertices
}

/// Map a speed value (km/h) to an RGBA color using a green-yellow-red gradient.
///
/// Thresholds: 0-10 red, 10-25 yellow, 25-40 green, 40+ bright green.
/// Interpolates between colors for smooth transitions.
fn speed_to_color(speed_kmh: f32) -> [f32; 4] {
    const RED: [f32; 4] = [1.0, 0.2, 0.1, 0.7];
    const YELLOW: [f32; 4] = [1.0, 0.8, 0.0, 0.7];
    const GREEN: [f32; 4] = [0.2, 0.9, 0.2, 0.7];
    const BRIGHT_GREEN: [f32; 4] = [0.0, 1.0, 0.4, 0.7];

    fn lerp_color(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
        [
            a[0] + (b[0] - a[0]) * t,
            a[1] + (b[1] - a[1]) * t,
            a[2] + (b[2] - a[2]) * t,
            a[3] + (b[3] - a[3]) * t,
        ]
    }

    if speed_kmh <= 10.0 {
        RED
    } else if speed_kmh <= 25.0 {
        let t = (speed_kmh - 10.0) / 15.0;
        lerp_color(RED, YELLOW, t)
    } else if speed_kmh <= 40.0 {
        let t = (speed_kmh - 25.0) / 15.0;
        lerp_color(YELLOW, GREEN, t)
    } else {
        BRIGHT_GREEN
    }
}

/// Half-width of speed overlay quad strip in metres.
const SPEED_OVERLAY_HALF_WIDTH: f32 = 1.5;

/// Build speed-colored quad strips for edges covered by cameras.
///
/// For each camera, gets covered edges and latest mean speed from the aggregator.
/// Colors edges using green-yellow-red gradient based on speed.
/// Only renders edges that have actual detection data.
pub fn build_speed_overlay_vertices(
    cameras: &[&velos_api::Camera],
    aggregator: &velos_api::DetectionAggregator,
    graph: &velos_net::RoadGraph,
    projection: &velos_net::EquirectangularProjection,
    show: bool,
) -> Vec<GuideLineVertex> {
    if !show || cameras.is_empty() {
        return Vec::new();
    }

    let g = graph.inner();
    let mut vertices = Vec::new();

    for cam in cameras {
        // Get average speed across all vehicle classes from latest window
        let window = match aggregator.latest_window(cam.id) {
            Some(w) => w,
            None => continue,
        };

        // Compute mean speed across all vehicle classes
        let mut total_speed_sum = 0.0_f32;
        let mut total_count = 0_u32;
        for &(sum, count) in window.speed_samples.values() {
            total_speed_sum += sum;
            total_count += count;
        }
        if total_count == 0 {
            continue; // No speed data
        }
        let mean_speed = total_speed_sum / total_count as f32;
        let color = speed_to_color(mean_speed);

        // Always render a speed indicator circle at camera position
        let (cx, cy) = projection.project(cam.lat, cam.lon);
        let cx = cx as f32;
        let cy = cy as f32;
        let radius = 8.0_f32; // metres
        let segments = 16;
        for i in 0..segments {
            let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
            let base = GuideLineVertex {
                position: [cx, cy],
                color,
                line_dist: 0.0,
                _pad: 0.0,
            };
            let mut v1 = base;
            v1.position = [cx + radius * a0.cos(), cy + radius * a0.sin()];
            let mut v2 = base;
            v2.position = [cx + radius * a1.cos(), cy + radius * a1.sin()];
            vertices.push(base);
            vertices.push(v1);
            vertices.push(v2);
        }

        // Also add a speed label ring (brighter outline)
        let mut outline_color = color;
        outline_color[3] = 1.0; // full opacity outline
        let outer_r = radius + 2.0;
        for i in 0..segments {
            let a0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;
            let inner0 = [cx + radius * a0.cos(), cy + radius * a0.sin()];
            let outer0 = [cx + outer_r * a0.cos(), cy + outer_r * a0.sin()];
            let inner1 = [cx + radius * a1.cos(), cy + radius * a1.sin()];
            let outer1 = [cx + outer_r * a1.cos(), cy + outer_r * a1.sin()];
            let mk = |pos: [f32; 2]| GuideLineVertex {
                position: pos,
                color: outline_color,
                line_dist: 0.0,
                _pad: 0.0,
            };
            vertices.push(mk(inner0));
            vertices.push(mk(outer0));
            vertices.push(mk(inner1));
            vertices.push(mk(inner1));
            vertices.push(mk(outer0));
            vertices.push(mk(outer1));
        }

        // Render each covered edge
        for &edge_id in &cam.covered_edges {
            let edge_idx = petgraph::graph::EdgeIndex::new(edge_id as usize);
            let edge = match g.edge_weight(edge_idx) {
                Some(e) => e,
                None => continue,
            };

            let geom = &edge.geometry;
            if geom.len() < 2 {
                continue;
            }

            // Build quad strip along edge geometry
            let hw = SPEED_OVERLAY_HALF_WIDTH;
            let mut cumulative_dist = 0.0_f32;

            for w in geom.windows(2) {
                let p0 = [w[0][0] as f32, w[0][1] as f32];
                let p1 = [w[1][0] as f32, w[1][1] as f32];

                let dx = p1[0] - p0[0];
                let dy = p1[1] - p0[1];
                let seg_len = (dx * dx + dy * dy).sqrt();
                if seg_len < 1e-6 {
                    continue;
                }

                // Normal perpendicular to segment
                let nx = -dy / seg_len;
                let ny = dx / seg_len;

                let dist0 = cumulative_dist;
                cumulative_dist += seg_len;
                let dist1 = cumulative_dist;

                // Two triangles forming a quad
                let v00 = GuideLineVertex {
                    position: [p0[0] + nx * hw, p0[1] + ny * hw],
                    color,
                    line_dist: dist0,
                    _pad: 0.0,
                };
                let v01 = GuideLineVertex {
                    position: [p0[0] - nx * hw, p0[1] - ny * hw],
                    color,
                    line_dist: dist0,
                    _pad: 0.0,
                };
                let v10 = GuideLineVertex {
                    position: [p1[0] + nx * hw, p1[1] + ny * hw],
                    color,
                    line_dist: dist1,
                    _pad: 0.0,
                };
                let v11 = GuideLineVertex {
                    position: [p1[0] - nx * hw, p1[1] - ny * hw],
                    color,
                    line_dist: dist1,
                    _pad: 0.0,
                };

                vertices.push(v00);
                vertices.push(v01);
                vertices.push(v10);
                vertices.push(v01);
                vertices.push(v11);
                vertices.push(v10);
            }
        }
    }

    vertices
}

/// Classified agent instances for 3D LOD rendering.
///
/// Contains per-vehicle-type instance buffers for each LOD tier.
pub struct LodBuffers {
    /// Mesh-tier instances (nearest agents, < 50m).
    pub mesh_instances: HashMap<VehicleType, Vec<MeshInstance3D>>,
    /// Billboard-tier instances (mid-range, 50-200m).
    pub billboard_instances: HashMap<VehicleType, Vec<BillboardInstance3D>>,
    /// Dot-tier agent count (far range, > 200m) -- rendered via existing 2D pipeline.
    pub dot_count: u32,
}

impl LodBuffers {
    /// Create empty LOD buffers.
    pub fn empty() -> Self {
        Self {
            mesh_instances: HashMap::new(),
            billboard_instances: HashMap::new(),
            dot_count: 0,
        }
    }

    /// Total number of mesh-tier instances across all vehicle types.
    pub fn total_mesh_count(&self) -> u32 {
        self.mesh_instances.values().map(|v| v.len() as u32).sum()
    }

    /// Total number of billboard-tier instances across all vehicle types.
    pub fn total_billboard_count(&self) -> u32 {
        self.billboard_instances
            .values()
            .map(|v| v.len() as u32)
            .sum()
    }
}

/// Vehicle dimensions (width, length, height) in metres for 3D rendering.
fn vehicle_dimensions(vtype: VehicleType) -> (f32, f32, f32) {
    match vtype {
        VehicleType::Motorbike => (0.8, 2.0, 1.2),
        VehicleType::Car => (1.8, 4.5, 1.5),
        VehicleType::Bus => (2.5, 12.0, 3.5),
        VehicleType::Truck => (2.5, 8.0, 3.0),
        VehicleType::Emergency => (1.8, 4.5, 1.8),
        VehicleType::Bicycle => (0.6, 1.8, 1.0),
        VehicleType::Pedestrian => (0.5, 0.5, 1.7),
    }
}

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

    /// Build 3D LOD-classified instance arrays for perspective rendering.
    ///
    /// Maps 2D positions (x, y) to 3D world (x, 0, y) and classifies agents
    /// into mesh/billboard/dot tiers based on distance from `eye`.
    ///
    /// Does NOT modify existing `build_instances()` -- that remains for 2D mode.
    pub fn build_instances_3d(&self, eye: glam::Vec3) -> LodBuffers {
        let mut mesh_instances: HashMap<VehicleType, Vec<MeshInstance3D>> = HashMap::new();
        let mut billboard_instances: HashMap<VehicleType, Vec<BillboardInstance3D>> =
            HashMap::new();
        let mut dot_count: u32 = 0;

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
            // Skip agents with NaN/Inf positions
            if !pos.x.is_finite() || !pos.y.is_finite() {
                continue;
            }

            // Map 2D (x, y) to 3D (x, 0, y) -- Y-up coordinate convention
            let world_pos_3d = glam::Vec3::new(pos.x as f32, 0.0, pos.y as f32);
            let distance = world_pos_3d.distance(eye);

            // Classify LOD tier (no hysteresis for now -- stateless per-frame)
            let tier = classify_lod(distance, None);

            // Vehicle-type coloring (same logic as 2D)
            let color = if *vtype == VehicleType::Bus {
                let ri = bus_state.map(|bs| bs.route_index()).unwrap_or(0);
                BUS_ROUTE_COLORS[ri as usize % BUS_ROUTE_COLORS.len()]
            } else {
                vehicle_type_color(*vtype)
            };

            // Bezier tangent heading for junction agents
            let heading = if let Some(jt) = jt {
                self.junction_heading(jt)
            } else {
                kin.heading as f32
            };

            match tier {
                LodTier::Mesh => {
                    mesh_instances
                        .entry(*vtype)
                        .or_default()
                        .push(MeshInstance3D {
                            world_pos: world_pos_3d.into(),
                            heading,
                            color,
                        });
                }
                LodTier::Billboard => {
                    let (w, _l, h) = vehicle_dimensions(*vtype);
                    billboard_instances
                        .entry(*vtype)
                        .or_default()
                        .push(BillboardInstance3D {
                            world_pos: world_pos_3d.into(),
                            size: [w, h],
                            color,
                            _pad: 0.0,
                        });
                }
                LodTier::Dot => {
                    dot_count += 1;
                }
            }
        }

        LodBuffers {
            mesh_instances,
            billboard_instances,
            dot_count,
        }
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
    use velos_net::EquirectangularProjection;

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

    #[test]
    fn camera_overlay_empty_when_disabled() {
        let cam = Camera {
            id: 1,
            lat: 10.7756,
            lon: 106.7019,
            heading_deg: 90.0,
            fov_deg: 60.0,
            range_m: 50.0,
            name: "test".to_string(),
            covered_edges: vec![],
        };
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let verts = build_camera_overlay_vertices(&[&cam], &proj, false);
        assert!(verts.is_empty(), "disabled overlay should produce no vertices");
    }

    #[test]
    fn camera_overlay_empty_when_no_cameras() {
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let verts = build_camera_overlay_vertices(&[], &proj, true);
        assert!(verts.is_empty(), "no cameras should produce no vertices");
    }

    #[test]
    fn camera_overlay_produces_vertices_for_one_camera() {
        let cam = Camera {
            id: 1,
            lat: 10.7756,
            lon: 106.7019,
            heading_deg: 0.0,
            fov_deg: 60.0,
            range_m: 100.0,
            name: "cam-1".to_string(),
            covered_edges: vec![],
        };
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let verts = build_camera_overlay_vertices(&[&cam], &proj, true);

        // Icon: 4 triangles * 3 verts = 12
        // Cone fill: FOV_CONE_ARC_SEGMENTS * 3 verts = 36
        // Left edge outline: 6 verts
        // Right edge outline: 6 verts
        // Arc outline: FOV_CONE_ARC_SEGMENTS * 6 verts = 72
        // Total: 12 + 36 + 6 + 6 + 72 = 132
        let expected = 12 + FOV_CONE_ARC_SEGMENTS * 3 + 6 + 6 + FOV_CONE_ARC_SEGMENTS * 6;
        assert_eq!(verts.len(), expected, "vertex count mismatch for single camera");
    }

    #[test]
    fn camera_overlay_scales_with_camera_count() {
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let cam1 = Camera {
            id: 1, lat: 10.775, lon: 106.700,
            heading_deg: 0.0, fov_deg: 90.0, range_m: 50.0,
            name: "a".into(), covered_edges: vec![],
        };
        let cam2 = Camera {
            id: 2, lat: 10.776, lon: 106.701,
            heading_deg: 180.0, fov_deg: 45.0, range_m: 30.0,
            name: "b".into(), covered_edges: vec![],
        };
        let v1 = build_camera_overlay_vertices(&[&cam1], &proj, true);
        let v2 = build_camera_overlay_vertices(&[&cam1, &cam2], &proj, true);
        assert_eq!(v2.len(), v1.len() * 2, "two cameras should produce 2x vertices");
    }

    // --- Speed overlay tests ---

    #[test]
    fn speed_to_color_red_at_zero() {
        let c = speed_to_color(0.0);
        assert_eq!(c, [1.0, 0.2, 0.1, 0.7]);
    }

    #[test]
    fn speed_to_color_red_at_10() {
        let c = speed_to_color(10.0);
        assert_eq!(c, [1.0, 0.2, 0.1, 0.7]);
    }

    #[test]
    fn speed_to_color_bright_green_above_40() {
        let c = speed_to_color(50.0);
        assert_eq!(c, [0.0, 1.0, 0.4, 0.7]);
    }

    #[test]
    fn speed_to_color_interpolates_between_red_and_yellow() {
        // Midpoint between red and yellow at 17.5 km/h
        let c = speed_to_color(17.5);
        // t = (17.5 - 10) / 15 = 0.5
        assert!((c[0] - 1.0).abs() < 0.01); // r stays 1.0
        assert!((c[1] - 0.5).abs() < 0.01); // g lerps 0.2 -> 0.8 at t=0.5
    }

    #[test]
    fn speed_overlay_empty_when_disabled() {
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let agg = velos_api::DetectionAggregator::default();
        let graph = velos_net::RoadGraph::new(petgraph::graph::DiGraph::new());
        let cam = Camera {
            id: 1, lat: 10.775, lon: 106.700,
            heading_deg: 0.0, fov_deg: 60.0, range_m: 50.0,
            name: "c".into(), covered_edges: vec![],
        };
        let verts = build_speed_overlay_vertices(&[&cam], &agg, &graph, &proj, false);
        assert!(verts.is_empty());
    }

    #[test]
    fn speed_overlay_empty_when_no_cameras() {
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let agg = velos_api::DetectionAggregator::default();
        let graph = velos_net::RoadGraph::new(petgraph::graph::DiGraph::new());
        let verts = build_speed_overlay_vertices(&[], &agg, &graph, &proj, true);
        assert!(verts.is_empty());
    }

    #[test]
    fn speed_overlay_empty_when_no_speed_data() {
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let agg = velos_api::DetectionAggregator::default();
        let graph = velos_net::RoadGraph::new(petgraph::graph::DiGraph::new());
        let cam = Camera {
            id: 1, lat: 10.775, lon: 106.700,
            heading_deg: 0.0, fov_deg: 60.0, range_m: 50.0,
            name: "c".into(), covered_edges: vec![0],
        };
        let verts = build_speed_overlay_vertices(&[&cam], &agg, &graph, &proj, true);
        assert!(verts.is_empty(), "no aggregator data should produce no vertices");
    }

    #[test]
    fn speed_overlay_produces_vertices_for_edge_with_data() {
        use velos_api::proto::velos::v2::DetectionEvent;
        use velos_net::graph::{RoadEdge, RoadNode, RoadClass};
        let proj = EquirectangularProjection::new(10.7756, 106.7019);

        // Build a simple graph with one edge
        let mut g = petgraph::graph::DiGraph::new();
        let n0 = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let n1 = g.add_node(RoadNode { pos: [100.0, 0.0] });
        g.add_edge(n0, n1, RoadEdge {
            length_m: 100.0,
            speed_limit_mps: 13.89,
            lane_count: 2,
            oneway: true,
            road_class: RoadClass::Primary,
            geometry: vec![[0.0, 0.0], [50.0, 0.0], [100.0, 0.0]],
            motorbike_only: false,
            time_windows: None,
        });
        let graph = velos_net::RoadGraph::new(g);

        // Create aggregator with speed data
        let mut agg = velos_api::DetectionAggregator::new(300_000, 3_600_000);
        agg.ingest(1, &DetectionEvent {
            camera_id: 1,
            timestamp_ms: 100_000,
            vehicle_class: 1,
            count: 5,
            speed_kmh: Some(35.0),
        });

        let cam = Camera {
            id: 1, lat: 10.775, lon: 106.700,
            heading_deg: 0.0, fov_deg: 60.0, range_m: 50.0,
            name: "c".into(), covered_edges: vec![0], // edge index 0
        };

        let verts = build_speed_overlay_vertices(&[&cam], &agg, &graph, &proj, true);
        // Circle indicator: 16 segments * 3 verts + 16 segments * 6 verts (outline) = 48 + 96 = 144
        // Plus edge quads: 2 segments * 6 verts = 12
        // Total = 156
        assert!(!verts.is_empty(), "should produce vertices for camera with speed data");
        assert!(verts.len() > 12, "should include circle indicator + edge quads");

        // Color should be between yellow and green (35 km/h)
        let color = verts[0].color;
        assert!(color[1] > 0.5, "green channel should be significant at 35 km/h");
    }

    // --- build_instances_3d tests ---

    #[test]
    fn lod_buffers_empty_has_zero_counts() {
        let buffers = LodBuffers::empty();
        assert_eq!(buffers.total_mesh_count(), 0);
        assert_eq!(buffers.total_billboard_count(), 0);
        assert_eq!(buffers.dot_count, 0);
    }

    #[test]
    fn vehicle_dimensions_motorbike() {
        let (w, l, h) = vehicle_dimensions(VehicleType::Motorbike);
        assert!((w - 0.8).abs() < 0.01);
        assert!((l - 2.0).abs() < 0.01);
        assert!((h - 1.2).abs() < 0.01);
    }

    #[test]
    fn vehicle_dimensions_bus_large() {
        let (w, l, h) = vehicle_dimensions(VehicleType::Bus);
        assert!(l > 10.0, "Bus should be > 10m long");
        assert!(h > 3.0, "Bus should be > 3m tall");
        assert!(w > 2.0, "Bus should be > 2m wide");
    }

    #[test]
    fn coordinate_mapping_2d_to_3d() {
        // 2D (100.0, 200.0) should map to 3D (100.0, 0.0, 200.0)
        let pos_2d = (100.0_f32, 200.0_f32);
        let pos_3d = glam::Vec3::new(pos_2d.0, 0.0, pos_2d.1);
        assert_eq!(pos_3d.x, 100.0);
        assert_eq!(pos_3d.y, 0.0);
        assert_eq!(pos_3d.z, 200.0);
    }

    #[test]
    fn lod_classification_mesh_tier_at_30m() {
        use crate::lod::classify_lod;
        let tier = classify_lod(30.0, None);
        assert_eq!(tier, crate::lod::LodTier::Mesh);
    }

    #[test]
    fn lod_classification_billboard_tier_at_100m() {
        use crate::lod::classify_lod;
        let tier = classify_lod(100.0, None);
        assert_eq!(tier, crate::lod::LodTier::Billboard);
    }

    #[test]
    fn lod_classification_dot_tier_at_300m() {
        use crate::lod::classify_lod;
        let tier = classify_lod(300.0, None);
        assert_eq!(tier, crate::lod::LodTier::Dot);
    }
}
