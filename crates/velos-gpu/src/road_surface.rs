//! Road surface polygon generation from RoadGraph edges.
//!
//! Generates 3D road surface polygons, lane markings, and junction surfaces
//! from the road network graph. Geometry is generated once at load time and
//! uploaded as static GPU vertex buffers.

use bytemuck::{Pod, Zeroable};
use std::collections::HashMap;

use velos_net::RoadGraph;

// --- Constants ---

/// Standard lane width in metres.
pub const LANE_WIDTH: f64 = 3.5;

/// Dark grey road surface color (#404040).
pub const ROAD_COLOR: [f32; 4] = [0.251, 0.251, 0.251, 1.0];

/// Slightly lighter grey for junction surfaces (#505050).
pub const JUNCTION_COLOR: [f32; 4] = [0.314, 0.314, 0.314, 1.0];

/// White lane marking color with slight transparency.
pub const MARKING_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.8];

/// Lane marking width in metres.
pub const MARKING_WIDTH: f64 = 0.15;

/// Dash length for center lane markings in metres.
pub const DASH_LENGTH: f64 = 3.0;

/// Gap length between dashes for center lane markings in metres.
pub const GAP_LENGTH: f64 = 3.0;

/// Road surface Y position (ground level).
pub const ROAD_Y: f32 = 0.0;

/// Lane marking Y position (slightly above road to prevent z-fighting).
pub const MARKING_Y: f32 = 0.01;

/// Junction surface Y position (between road and markings).
pub const JUNCTION_Y: f32 = 0.005;

// --- Vertex type ---

/// Road surface vertex: position (vec3) + color (vec4).
/// Layout matches ground_plane.wgsl VertexInput for pipeline reuse.
/// Total size: 28 bytes (3*4 + 4*4).
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct RoadSurfaceVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

/// Data for a junction: approach/exit points from connected edges.
#[derive(Debug, Clone)]
pub struct JunctionData {
    /// Points around the junction boundary (edge endpoints with offsets).
    pub boundary_points: Vec<[f64; 2]>,
}

// --- Geometry generation ---

/// Compute perpendicular offset vectors for a polyline segment.
/// Returns (left_offset, right_offset) as [f64; 2] for the given half_width.
fn perpendicular_offsets(p0: [f64; 2], p1: [f64; 2], half_width: f64) -> ([f64; 2], [f64; 2]) {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-10 {
        return ([0.0, 0.0], [0.0, 0.0]);
    }
    // Perpendicular: rotate direction 90 degrees
    let nx = -dy / len * half_width;
    let ny = dx / len * half_width;
    ([nx, ny], [-nx, -ny])
}

/// Generate road surface mesh from all edges in the graph.
///
/// Each edge polyline is expanded into a polygon strip using perpendicular
/// offsets based on lane count * lane width. 2D coordinates (x, y) map
/// to 3D as (x, ROAD_Y, y).
pub fn generate_road_mesh(graph: &RoadGraph) -> Vec<RoadSurfaceVertex> {
    let mut vertices = Vec::new();
    let g = graph.inner();

    for edge_idx in g.edge_indices() {
        let edge = &g[edge_idx];
        let geom = &edge.geometry;
        if geom.len() < 2 {
            continue;
        }

        let half_width = edge.lane_count as f64 * LANE_WIDTH / 2.0;

        for i in 0..geom.len() - 1 {
            let p0 = geom[i];
            let p1 = geom[i + 1];

            let (left0, right0) = perpendicular_offsets(p0, p1, half_width);
            let (left1, right1) = perpendicular_offsets(p0, p1, half_width);

            // Four corners of the quad segment
            let bl = [
                (p0[0] + left0[0]) as f32,
                ROAD_Y,
                (p0[1] + left0[1]) as f32,
            ];
            let br = [
                (p0[0] + right0[0]) as f32,
                ROAD_Y,
                (p0[1] + right0[1]) as f32,
            ];
            let tl = [
                (p1[0] + left1[0]) as f32,
                ROAD_Y,
                (p1[1] + left1[1]) as f32,
            ];
            let tr = [
                (p1[0] + right1[0]) as f32,
                ROAD_Y,
                (p1[1] + right1[1]) as f32,
            ];

            // Triangle 1: bl, br, tr
            vertices.push(RoadSurfaceVertex { position: bl, color: ROAD_COLOR });
            vertices.push(RoadSurfaceVertex { position: br, color: ROAD_COLOR });
            vertices.push(RoadSurfaceVertex { position: tr, color: ROAD_COLOR });

            // Triangle 2: bl, tr, tl
            vertices.push(RoadSurfaceVertex { position: bl, color: ROAD_COLOR });
            vertices.push(RoadSurfaceVertex { position: tr, color: ROAD_COLOR });
            vertices.push(RoadSurfaceVertex { position: tl, color: ROAD_COLOR });
        }
    }

    vertices
}

/// Generate a marking strip (thin quad) along a polyline at a lateral offset.
///
/// If `dashed` is true, produces 3m dash + 3m gap pattern.
/// If `dashed` is false, produces a continuous solid line.
fn generate_marking_strip(
    geometry: &[[f64; 2]],
    lateral_offset: f64,
    dashed: bool,
    vertices: &mut Vec<RoadSurfaceVertex>,
) {
    if geometry.len() < 2 {
        return;
    }

    let half_mark = MARKING_WIDTH / 2.0;
    let cycle_length = DASH_LENGTH + GAP_LENGTH;

    // Walk along the polyline accumulating distance for dash pattern
    let mut accumulated_dist = 0.0;

    for i in 0..geometry.len() - 1 {
        let p0 = geometry[i];
        let p1 = geometry[i + 1];

        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let seg_len = (dx * dx + dy * dy).sqrt();
        if seg_len < 1e-10 {
            continue;
        }

        // Direction and perpendicular
        let dir_x = dx / seg_len;
        let dir_y = dy / seg_len;
        let perp_x = -dir_y;
        let perp_y = dir_x;

        // Offset the center line of the marking by lateral_offset
        let cx0 = p0[0] + perp_x * lateral_offset;
        let cy0 = p0[1] + perp_y * lateral_offset;
        let cx1 = p1[0] + perp_x * lateral_offset;
        let cy1 = p1[1] + perp_y * lateral_offset;

        if !dashed {
            // Solid line: one quad for the entire segment
            let l0 = [
                (cx0 + perp_x * half_mark) as f32,
                MARKING_Y,
                (cy0 + perp_y * half_mark) as f32,
            ];
            let r0 = [
                (cx0 - perp_x * half_mark) as f32,
                MARKING_Y,
                (cy0 - perp_y * half_mark) as f32,
            ];
            let l1 = [
                (cx1 + perp_x * half_mark) as f32,
                MARKING_Y,
                (cy1 + perp_y * half_mark) as f32,
            ];
            let r1 = [
                (cx1 - perp_x * half_mark) as f32,
                MARKING_Y,
                (cy1 - perp_y * half_mark) as f32,
            ];

            vertices.push(RoadSurfaceVertex { position: l0, color: MARKING_COLOR });
            vertices.push(RoadSurfaceVertex { position: r0, color: MARKING_COLOR });
            vertices.push(RoadSurfaceVertex { position: r1, color: MARKING_COLOR });

            vertices.push(RoadSurfaceVertex { position: l0, color: MARKING_COLOR });
            vertices.push(RoadSurfaceVertex { position: r1, color: MARKING_COLOR });
            vertices.push(RoadSurfaceVertex { position: l1, color: MARKING_COLOR });
        } else {
            // Dashed line: walk along segment emitting dash quads
            let mut t = 0.0;
            while t < seg_len {
                let cycle_pos = (accumulated_dist + t) % cycle_length;
                if cycle_pos >= DASH_LENGTH {
                    // In a gap -- advance to end of gap
                    let gap_remaining = cycle_length - cycle_pos;
                    t += gap_remaining;
                    continue;
                }

                // In a dash -- determine how much dash remains
                let dash_remaining = DASH_LENGTH - cycle_pos;
                let seg_remaining = seg_len - t;
                let dash_len = dash_remaining.min(seg_remaining);

                // Start and end of this dash segment
                let frac_start = t / seg_len;
                let frac_end = (t + dash_len) / seg_len;

                let sx = cx0 + (cx1 - cx0) * frac_start;
                let sy = cy0 + (cy1 - cy0) * frac_start;
                let ex = cx0 + (cx1 - cx0) * frac_end;
                let ey = cy0 + (cy1 - cy0) * frac_end;

                let l0 = [
                    (sx + perp_x * half_mark) as f32,
                    MARKING_Y,
                    (sy + perp_y * half_mark) as f32,
                ];
                let r0 = [
                    (sx - perp_x * half_mark) as f32,
                    MARKING_Y,
                    (sy - perp_y * half_mark) as f32,
                ];
                let l1 = [
                    (ex + perp_x * half_mark) as f32,
                    MARKING_Y,
                    (ey + perp_y * half_mark) as f32,
                ];
                let r1 = [
                    (ex - perp_x * half_mark) as f32,
                    MARKING_Y,
                    (ey - perp_y * half_mark) as f32,
                ];

                vertices.push(RoadSurfaceVertex { position: l0, color: MARKING_COLOR });
                vertices.push(RoadSurfaceVertex { position: r0, color: MARKING_COLOR });
                vertices.push(RoadSurfaceVertex { position: r1, color: MARKING_COLOR });

                vertices.push(RoadSurfaceVertex { position: l0, color: MARKING_COLOR });
                vertices.push(RoadSurfaceVertex { position: r1, color: MARKING_COLOR });
                vertices.push(RoadSurfaceVertex { position: l1, color: MARKING_COLOR });

                t += dash_len;
            }

            accumulated_dist += seg_len;
        }
    }
}

/// Generate lane marking geometry for all edges in the graph.
///
/// For each edge:
/// - Solid edge lines at +/- half_width from center
/// - Dashed center lines at lane boundaries (for edges with >= 2 lanes)
pub fn generate_lane_markings(graph: &RoadGraph) -> Vec<RoadSurfaceVertex> {
    let mut vertices = Vec::new();
    let g = graph.inner();

    for edge_idx in g.edge_indices() {
        let edge = &g[edge_idx];
        let geom = &edge.geometry;
        if geom.len() < 2 {
            continue;
        }

        let half_width = edge.lane_count as f64 * LANE_WIDTH / 2.0;

        // Solid edge lines (left and right)
        generate_marking_strip(geom, half_width, false, &mut vertices);
        generate_marking_strip(geom, -half_width, false, &mut vertices);

        // Dashed center lines at lane boundaries (only for multi-lane edges)
        if edge.lane_count >= 2 {
            for lane_idx in 1..edge.lane_count {
                let offset = lane_idx as f64 * LANE_WIDTH - half_width;
                generate_marking_strip(geom, offset, true, &mut vertices);
            }
        }
    }

    vertices
}

/// Generate junction surface fills from junction boundary data.
///
/// Each junction is a convex hull of boundary points, triangulated as a fan
/// from the centroid.
pub fn generate_junction_surfaces(
    junction_data: &HashMap<u32, JunctionData>,
) -> Vec<RoadSurfaceVertex> {
    let mut vertices = Vec::new();

    for jdata in junction_data.values() {
        let pts = &jdata.boundary_points;
        if pts.len() < 3 {
            continue;
        }

        // Compute centroid
        let n = pts.len() as f64;
        let cx: f64 = pts.iter().map(|p| p[0]).sum::<f64>() / n;
        let cy: f64 = pts.iter().map(|p| p[1]).sum::<f64>() / n;

        // Sort points by angle around centroid (Graham scan style)
        let mut sorted_pts: Vec<[f64; 2]> = pts.clone();
        sorted_pts.sort_by(|a, b| {
            let angle_a = (a[1] - cy).atan2(a[0] - cx);
            let angle_b = (b[1] - cy).atan2(b[0] - cx);
            angle_a.partial_cmp(&angle_b).unwrap_or(std::cmp::Ordering::Equal)
        });

        // Triangulate as fan from centroid
        let centroid = [cx as f32, JUNCTION_Y, cy as f32];

        for i in 0..sorted_pts.len() {
            let p0 = sorted_pts[i];
            let p1 = sorted_pts[(i + 1) % sorted_pts.len()];

            let v0 = [p0[0] as f32, JUNCTION_Y, p0[1] as f32];
            let v1 = [p1[0] as f32, JUNCTION_Y, p1[1] as f32];

            vertices.push(RoadSurfaceVertex { position: centroid, color: JUNCTION_COLOR });
            vertices.push(RoadSurfaceVertex { position: v0, color: JUNCTION_COLOR });
            vertices.push(RoadSurfaceVertex { position: v1, color: JUNCTION_COLOR });
        }
    }

    vertices
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::graph::DiGraph;
    use velos_net::{RoadEdge, RoadGraph, RoadNode, RoadClass};

    /// Helper: create a RoadGraph with a single straight edge.
    fn single_edge_graph(lane_count: u8, p0: [f64; 2], p1: [f64; 2]) -> RoadGraph {
        let mut g = DiGraph::new();
        let n0 = g.add_node(RoadNode { pos: p0 });
        let n1 = g.add_node(RoadNode { pos: p1 });

        let dx = p1[0] - p0[0];
        let dy = p1[1] - p0[1];
        let length = (dx * dx + dy * dy).sqrt();

        g.add_edge(
            n0,
            n1,
            RoadEdge {
                length_m: length,
                speed_limit_mps: 11.1,
                lane_count,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![p0, p1],
                motorbike_only: false,
                time_windows: None,
            },
        );
        RoadGraph::new(g)
    }

    #[test]
    fn test_road_surface_vertex_size() {
        // 3 floats (position) + 4 floats (color) = 7 * 4 = 28 bytes
        assert_eq!(
            std::mem::size_of::<RoadSurfaceVertex>(),
            28,
            "RoadSurfaceVertex should be 28 bytes"
        );
    }

    #[test]
    fn test_generate_road_mesh_straight_2lane() {
        let graph = single_edge_graph(2, [0.0, 0.0], [100.0, 0.0]);
        let verts = generate_road_mesh(&graph);

        // 1 segment -> 2 triangles -> 6 vertices
        assert_eq!(verts.len(), 6, "2-point edge should produce 6 vertices (2 triangles)");

        // Check Y coordinate is ROAD_Y (0.0)
        for v in &verts {
            assert!(
                (v.position[1] - ROAD_Y).abs() < 1e-6,
                "Road vertex Y should be ROAD_Y={ROAD_Y}, got {}",
                v.position[1]
            );
        }
    }

    #[test]
    fn test_road_mesh_correct_width() {
        // 2 lanes * 3.5m = 7m total width
        let graph = single_edge_graph(2, [0.0, 0.0], [100.0, 0.0]);
        let verts = generate_road_mesh(&graph);

        // For a horizontal edge, perpendicular offset is in Z direction
        // Find min/max Z to verify width
        let z_values: Vec<f32> = verts.iter().map(|v| v.position[2]).collect();
        let z_min = z_values.iter().cloned().fold(f32::INFINITY, f32::min);
        let z_max = z_values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let width = z_max - z_min;

        assert!(
            (width - 7.0).abs() < 0.01,
            "Road width should be 7.0m (2 lanes * 3.5m), got {width}"
        );
    }

    #[test]
    fn test_road_vertices_3d_coordinate_mapping() {
        // 2D (x, y) -> 3D (x, 0, y)
        let graph = single_edge_graph(1, [10.0, 20.0], [50.0, 20.0]);
        let verts = generate_road_mesh(&graph);

        // X values should be around 10-50 (the 2D x range)
        for v in &verts {
            assert!(v.position[0] >= 9.0 && v.position[0] <= 51.0,
                "X should be in range of 2D x coords");
        }
        // Z values should be around 20 (the 2D y value) +/- half lane width
        for v in &verts {
            assert!(v.position[2] >= 18.0 && v.position[2] <= 22.0,
                "Z should be around 2D y value +/- half width");
        }
    }

    #[test]
    fn test_lane_markings_2lane_edge() {
        let graph = single_edge_graph(2, [0.0, 0.0], [100.0, 0.0]);
        let verts = generate_lane_markings(&graph);

        // Should have: 2 solid edge lines + 1 dashed center line
        // Each solid line = 6 vertices (1 segment, 2 tris)
        // Dashed center: 100m / 6m cycle = 16 full cycles + partial
        // 16 dashes * 6 verts + remainder dash = lots of verts
        assert!(!verts.is_empty(), "Should produce marking vertices");

        // All markings at MARKING_Y
        for v in &verts {
            assert!(
                (v.position[1] - MARKING_Y).abs() < 1e-6,
                "Marking vertex Y should be MARKING_Y={MARKING_Y}, got {}",
                v.position[1]
            );
        }
    }

    #[test]
    fn test_center_line_dash_pattern() {
        // Use a short edge (12m) to easily verify dash pattern:
        // 12m / 6m cycle = 2 full cycles -> 2 dashes of 3m each
        let graph = single_edge_graph(2, [0.0, 0.0], [12.0, 0.0]);
        let verts = generate_lane_markings(&graph);

        // Separate edge lines (solid) from center lines (dashed)
        // Edge lines are at Z = +/- 3.5 (half_width for 2 lanes)
        // Center line is at Z = 0.0
        let center_verts: Vec<&RoadSurfaceVertex> = verts.iter().filter(|v| {
            v.position[2].abs() < 1.0 // Center line near Z=0
        }).collect();

        // 2 dashes * 6 vertices each = 12 vertices for center marking
        assert_eq!(
            center_verts.len(),
            12,
            "12m edge should have 2 center dashes * 6 verts = 12, got {}",
            center_verts.len()
        );
    }

    #[test]
    fn test_edge_lines_are_solid() {
        // For a 12m edge, solid edge lines should be 1 quad each = 6 verts
        let graph = single_edge_graph(2, [0.0, 0.0], [12.0, 0.0]);
        let verts = generate_lane_markings(&graph);

        // Edge lines are at Z = +/- 3.5 (half_width = 2 * 3.5 / 2 = 3.5)
        let edge_verts: Vec<&RoadSurfaceVertex> = verts.iter().filter(|v| {
            v.position[2].abs() > 3.0 // Edge lines far from center
        }).collect();

        // 2 solid edge lines * 6 verts each = 12
        assert_eq!(
            edge_verts.len(),
            12,
            "Should have 2 solid edge lines * 6 verts = 12, got {}",
            edge_verts.len()
        );
    }

    #[test]
    fn test_marking_width() {
        // Single-lane edge, solid edge lines at +/- 1.75m
        let graph = single_edge_graph(1, [0.0, 0.0], [10.0, 0.0]);
        let verts = generate_lane_markings(&graph);

        // For horizontal edge, markings at Z = +/- 1.75
        // Each marking strip should have width ~0.15m in Z
        let left_edge_verts: Vec<&RoadSurfaceVertex> = verts.iter().filter(|v| {
            v.position[2] > 1.5
        }).collect();

        if !left_edge_verts.is_empty() {
            let z_vals: Vec<f32> = left_edge_verts.iter().map(|v| v.position[2]).collect();
            let z_min = z_vals.iter().cloned().fold(f32::INFINITY, f32::min);
            let z_max = z_vals.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            let mark_width = z_max - z_min;
            assert!(
                (mark_width - MARKING_WIDTH as f32).abs() < 0.02,
                "Marking width should be ~{MARKING_WIDTH}m, got {mark_width}"
            );
        }
    }

    #[test]
    fn test_single_lane_no_center_dashes() {
        let graph = single_edge_graph(1, [0.0, 0.0], [100.0, 0.0]);
        let verts = generate_lane_markings(&graph);

        // Single lane: only 2 edge lines, no center dashes
        // Each edge line on a 1-segment polyline = 6 verts
        assert_eq!(verts.len(), 12, "Single lane should have only 2 edge lines = 12 verts");
    }

    #[test]
    fn test_junction_surfaces_4_points() {
        let mut junctions = HashMap::new();
        junctions.insert(
            1,
            JunctionData {
                boundary_points: vec![
                    [0.0, 5.0],
                    [5.0, 0.0],
                    [0.0, -5.0],
                    [-5.0, 0.0],
                ],
            },
        );

        let verts = generate_junction_surfaces(&junctions);

        // 4 boundary points -> 4 triangles (fan from centroid)
        assert_eq!(
            verts.len(),
            12,
            "4 boundary points should produce 4 triangles = 12 vertices, got {}",
            verts.len()
        );

        // All at JUNCTION_Y
        for v in &verts {
            assert!(
                (v.position[1] - JUNCTION_Y).abs() < 1e-6,
                "Junction vertex Y should be JUNCTION_Y={JUNCTION_Y}, got {}",
                v.position[1]
            );
        }
    }

    #[test]
    fn test_junction_fewer_than_3_points_skipped() {
        let mut junctions = HashMap::new();
        junctions.insert(
            1,
            JunctionData {
                boundary_points: vec![[0.0, 0.0], [1.0, 0.0]],
            },
        );

        let verts = generate_junction_surfaces(&junctions);
        assert!(verts.is_empty(), "Junction with < 3 points should produce no vertices");
    }

    #[test]
    fn test_empty_graph_produces_no_vertices() {
        let g = DiGraph::new();
        let graph = RoadGraph::new(g);

        assert!(generate_road_mesh(&graph).is_empty());
        assert!(generate_lane_markings(&graph).is_empty());
    }

    #[test]
    fn test_edge_with_single_point_skipped() {
        let mut g = DiGraph::new();
        let n0 = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let n1 = g.add_node(RoadNode { pos: [10.0, 0.0] });
        g.add_edge(
            n0,
            n1,
            RoadEdge {
                length_m: 10.0,
                speed_limit_mps: 11.1,
                lane_count: 2,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![[0.0, 0.0]], // Only 1 point
                motorbike_only: false,
                time_windows: None,
            },
        );
        let graph = RoadGraph::new(g);

        assert!(generate_road_mesh(&graph).is_empty());
        assert!(generate_lane_markings(&graph).is_empty());
    }

    #[test]
    fn test_multi_segment_polyline() {
        // 3-point polyline -> 2 segments -> 4 triangles -> 12 vertices
        let mut g = DiGraph::new();
        let n0 = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let n1 = g.add_node(RoadNode { pos: [100.0, 50.0] });
        g.add_edge(
            n0,
            n1,
            RoadEdge {
                length_m: 120.0,
                speed_limit_mps: 11.1,
                lane_count: 1,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![[0.0, 0.0], [50.0, 0.0], [100.0, 50.0]],
                motorbike_only: false,
                time_windows: None,
            },
        );
        let graph = RoadGraph::new(g);
        let verts = generate_road_mesh(&graph);
        assert_eq!(verts.len(), 12, "3-point polyline should produce 12 vertices (2 segments * 6)");
    }
}
