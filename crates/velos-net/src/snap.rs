//! Edge segment R-tree for snapping GTFS stops to the nearest road edge.
//!
//! Builds an R-tree of polyline segments from `RoadGraph` edges, then provides
//! nearest-edge queries with perpendicular projection to compute `(edge_id, offset_m)`.
//! Used by the GTFS bus stop pipeline to convert WGS84 lat/lon stops into
//! edge-based `BusStop` positions.

use rstar::{PointDistance, RTree, RTreeObject, AABB};
use velos_vehicle::bus::BusStop;

use crate::graph::RoadGraph;
use crate::projection::EquirectangularProjection;
use velos_demand::gtfs::GtfsStop;

/// Maximum snap distance in metres. Stops beyond this radius are skipped.
/// HCMC OSM coverage is sparse in some areas; 200m accommodates stops
/// on minor roads not present in the District 1 PBF extract.
const MAX_SNAP_RADIUS_M: f64 = 200.0;

/// Merge threshold: duplicate stops on the same edge within this distance are merged.
const MERGE_THRESHOLD_M: f64 = 10.0;

/// Default passenger capacity for snapped bus stops.
const DEFAULT_CAPACITY: u16 = 40;

/// A line segment from a road edge's polyline geometry, stored in the R-tree.
///
/// Each edge polyline is decomposed into consecutive segments. The `offset_along_edge`
/// tracks cumulative distance from the edge start to this segment's start point,
/// enabling accurate `offset_m` computation after projection.
#[derive(Debug, Clone)]
pub struct EdgeSegment {
    /// Road edge index (as u32 for BusStop compatibility).
    pub edge_id: u32,
    /// Start point of this segment in local metres [x, y].
    pub segment_start: [f64; 2],
    /// End point of this segment in local metres [x, y].
    pub segment_end: [f64; 2],
    /// Cumulative distance from edge start to this segment's start point.
    pub offset_along_edge: f64,
}

impl RTreeObject for EdgeSegment {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [
                self.segment_start[0].min(self.segment_end[0]),
                self.segment_start[1].min(self.segment_end[1]),
            ],
            [
                self.segment_start[0].max(self.segment_end[0]),
                self.segment_start[1].max(self.segment_end[1]),
            ],
        )
    }
}

impl PointDistance for EdgeSegment {
    fn distance_2(&self, point: &[f64; 2]) -> f64 {
        let (_, _, dist) = project_onto_segment(*point, self.segment_start, self.segment_end);
        dist * dist
    }
}

/// Project a point onto a line segment, returning `(t_param, nearest_point, distance)`.
///
/// `t_param` is clamped to `[0, 1]`. Handles degenerate zero-length segments
/// by returning t=0 and distance to the start point.
pub fn project_onto_segment(
    point: [f64; 2],
    seg_start: [f64; 2],
    seg_end: [f64; 2],
) -> (f64, [f64; 2], f64) {
    let dx = seg_end[0] - seg_start[0];
    let dy = seg_end[1] - seg_start[1];
    let len_sq = dx * dx + dy * dy;

    if len_sq < 1e-12 {
        // Degenerate zero-length segment
        let d = ((point[0] - seg_start[0]).powi(2) + (point[1] - seg_start[1]).powi(2)).sqrt();
        return (0.0, seg_start, d);
    }

    let t = ((point[0] - seg_start[0]) * dx + (point[1] - seg_start[1]) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);

    let nearest = [seg_start[0] + t * dx, seg_start[1] + t * dy];
    let dist = ((point[0] - nearest[0]).powi(2) + (point[1] - nearest[1]).powi(2)).sqrt();

    (t, nearest, dist)
}

/// Build an R-tree of edge segments from all edges in the road graph.
///
/// Each edge's polyline geometry is decomposed into consecutive line segments.
/// Cumulative offset is tracked so that projection results can compute the
/// total distance from edge start.
pub fn build_edge_rtree(graph: &RoadGraph) -> RTree<EdgeSegment> {
    let mut segments = Vec::new();

    for edge_idx in graph.inner().edge_indices() {
        let edge = &graph.inner()[edge_idx];
        let edge_id = edge_idx.index() as u32;
        let geom = &edge.geometry;

        if geom.len() < 2 {
            continue;
        }

        let mut cumulative_offset = 0.0;
        for i in 0..geom.len() - 1 {
            let start = geom[i];
            let end = geom[i + 1];

            segments.push(EdgeSegment {
                edge_id,
                segment_start: start,
                segment_end: end,
                offset_along_edge: cumulative_offset,
            });

            let seg_len =
                ((end[0] - start[0]).powi(2) + (end[1] - start[1]).powi(2)).sqrt();
            cumulative_offset += seg_len;
        }
    }

    RTree::bulk_load(segments)
}

/// Snap a point (in local metres) to the nearest road edge.
///
/// Returns `Some((edge_id, offset_m, distance))` if a segment is within `max_radius`,
/// or `None` if no edge is close enough.
///
/// `offset_m` is the distance along the edge from its start to the projected point.
pub fn snap_to_nearest_edge(
    tree: &RTree<EdgeSegment>,
    point: [f64; 2],
    max_radius: f64,
) -> Option<(u32, f64, f64)> {
    let nearest = tree.nearest_neighbor(&point)?;

    let seg_dx = nearest.segment_end[0] - nearest.segment_start[0];
    let seg_dy = nearest.segment_end[1] - nearest.segment_start[1];
    let seg_len = (seg_dx * seg_dx + seg_dy * seg_dy).sqrt();

    let (t, _, dist) = project_onto_segment(point, nearest.segment_start, nearest.segment_end);

    if dist > max_radius {
        return None;
    }

    let offset_m = nearest.offset_along_edge + t * seg_len;
    Some((nearest.edge_id, offset_m, dist))
}

/// Convert GTFS stops to edge-based `BusStop` positions via R-tree snapping.
///
/// 1. Builds an R-tree from the road graph edges.
/// 2. Projects each `GtfsStop` lat/lon to local metres via the projection.
/// 3. Snaps to the nearest edge within `MAX_SNAP_RADIUS_M`.
/// 4. Skips stops beyond the radius with a logged warning.
/// 5. Merges duplicate stops on the same edge within 10m.
///
/// Returns `Vec<BusStop>` with default `capacity: 40`.
pub fn snap_gtfs_stops(
    stops: &[GtfsStop],
    graph: &RoadGraph,
    proj: &EquirectangularProjection,
) -> Vec<BusStop> {
    if stops.is_empty() {
        return Vec::new();
    }

    let tree = build_edge_rtree(graph);
    let mut snapped: Vec<BusStop> = Vec::new();

    for stop in stops {
        let (x, y) = proj.project(stop.lat, stop.lon);
        let point = [x, y];

        match snap_to_nearest_edge(&tree, point, MAX_SNAP_RADIUS_M) {
            Some((edge_id, offset_m, _dist)) => {
                snapped.push(BusStop {
                    edge_id,
                    offset_m,
                    capacity: DEFAULT_CAPACITY,
                    name: stop.name.clone(),
                });
            }
            None => {
                log::warn!(
                    "GTFS stop '{}' (id={}) at ({:.6}, {:.6}) is >{}m from any edge, skipping",
                    stop.name,
                    stop.stop_id,
                    stop.lat,
                    stop.lon,
                    MAX_SNAP_RADIUS_M as u32
                );
            }
        }
    }

    // Merge duplicates: stops on the same edge within MERGE_THRESHOLD_M
    merge_nearby_stops(&mut snapped);

    snapped
}

/// Merge stops on the same edge that are within `MERGE_THRESHOLD_M` of each other.
///
/// Keeps the first stop encountered, skips subsequent duplicates. This prevents
/// multiple route stops that map to essentially the same physical location from
/// creating redundant BusStop entries.
fn merge_nearby_stops(stops: &mut Vec<BusStop>) {
    if stops.len() <= 1 {
        return;
    }

    // Sort by (edge_id, offset_m) for efficient duplicate detection
    stops.sort_by(|a, b| {
        a.edge_id
            .cmp(&b.edge_id)
            .then(a.offset_m.partial_cmp(&b.offset_m).unwrap())
    });

    let mut merged = Vec::with_capacity(stops.len());
    merged.push(stops[0].clone());

    for stop in stops.iter().skip(1) {
        let last = merged.last().unwrap();
        if stop.edge_id == last.edge_id && (stop.offset_m - last.offset_m).abs() < MERGE_THRESHOLD_M
        {
            // Duplicate -- skip this stop
            log::debug!(
                "Merging duplicate stop '{}' on edge {} (offset {:.1}m vs {:.1}m)",
                stop.name,
                stop.edge_id,
                stop.offset_m,
                last.offset_m
            );
        } else {
            merged.push(stop.clone());
        }
    }

    *stops = merged;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{RoadEdge, RoadGraph, RoadNode, RoadClass};
    use petgraph::graph::DiGraph;

    /// Helper: build a simple graph with edges defined by geometry polylines.
    fn make_graph(edges: Vec<(usize, usize, Vec<[f64; 2]>)>) -> RoadGraph {
        let mut g = DiGraph::new();

        // Collect unique node indices
        let max_node = edges
            .iter()
            .flat_map(|(s, t, _)| [*s, *t])
            .max()
            .unwrap_or(0);

        // Add nodes (position at origin -- not used by snap logic)
        let nodes: Vec<_> = (0..=max_node)
            .map(|_| g.add_node(RoadNode { pos: [0.0, 0.0] }))
            .collect();

        for (src, tgt, geom) in &edges {
            let length = geom
                .windows(2)
                .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
                .sum();
            g.add_edge(
                nodes[*src],
                nodes[*tgt],
                RoadEdge {
                    length_m: length,
                    speed_limit_mps: 13.9,
                    lane_count: 2,
                    oneway: true,
                    road_class: RoadClass::Primary,
                    geometry: geom.clone(),
                    motorbike_only: false,
                    time_windows: None,
                },
            );
        }

        RoadGraph::new(g)
    }

    #[test]
    fn build_edge_rtree_nonempty() {
        let graph = make_graph(vec![
            (0, 1, vec![[0.0, 0.0], [100.0, 0.0]]),
            (1, 2, vec![[100.0, 0.0], [200.0, 0.0]]),
            (2, 3, vec![[200.0, 0.0], [200.0, 100.0]]),
        ]);
        let tree = build_edge_rtree(&graph);
        assert!(tree.size() > 0, "R-tree should have segments");
        assert_eq!(tree.size(), 3, "3 edges with 1 segment each");
    }

    #[test]
    fn snap_to_nearest_edge_basic() {
        // Edge 0: horizontal line from (0,0) to (100,0)
        let graph = make_graph(vec![(0, 1, vec![[0.0, 0.0], [100.0, 0.0]])]);
        let tree = build_edge_rtree(&graph);

        // Point 10m north of the edge at x=50
        let result = snap_to_nearest_edge(&tree, [50.0, 10.0], 50.0);
        assert!(result.is_some(), "should snap within 50m");
        let (edge_id, offset_m, distance) = result.unwrap();
        assert_eq!(edge_id, 0);
        assert!((offset_m - 50.0).abs() < 0.1, "offset should be ~50m, got {offset_m}");
        assert!((distance - 10.0).abs() < 0.1, "distance should be ~10m, got {distance}");
    }

    #[test]
    fn snap_to_nearest_edge_beyond_radius() {
        let graph = make_graph(vec![(0, 1, vec![[0.0, 0.0], [100.0, 0.0]])]);
        let tree = build_edge_rtree(&graph);

        // Point 60m north -- beyond the 50m radius passed to this call
        let result = snap_to_nearest_edge(&tree, [50.0, 60.0], 50.0);
        assert!(result.is_none(), "should return None for distance > max_radius");
    }

    #[test]
    fn snap_offset_perpendicular_projection() {
        // Edge with a multi-segment polyline: (0,0) -> (50,0) -> (100,0)
        let graph = make_graph(vec![
            (0, 1, vec![[0.0, 0.0], [50.0, 0.0], [100.0, 0.0]]),
        ]);
        let tree = build_edge_rtree(&graph);

        // Point at (75, 5) should project onto the second segment at offset 75m
        let result = snap_to_nearest_edge(&tree, [75.0, 5.0], 50.0);
        assert!(result.is_some());
        let (edge_id, offset_m, distance) = result.unwrap();
        assert_eq!(edge_id, 0);
        assert!((offset_m - 75.0).abs() < 0.5, "offset should be ~75m, got {offset_m}");
        assert!((distance - 5.0).abs() < 0.1, "distance should be ~5m, got {distance}");
    }

    #[test]
    fn snap_gtfs_stops_projects_and_snaps() {
        // Edge from (0,0) to (200,0) in local metres
        let graph = make_graph(vec![(0, 1, vec![[0.0, 0.0], [200.0, 0.0]])]);

        // Use a projection centered at (10.7756, 106.7019)
        let proj = EquirectangularProjection::new(10.7756, 106.7019);

        // Place a GTFS stop slightly north of the edge center
        // proj.project(10.7756, 106.7019) = (0, 0), so we need a small lat/lon offset
        // that maps to roughly (100, 5) in local metres
        // y = (lat - 10.7756) * 110540 => lat = 10.7756 + 5/110540
        // x = (lon - 106.7019) * cos(10.7756 deg) * 111320 => lon = 106.7019 + 100/(cos(10.7756 deg)*111320)
        let cos_lat = (10.7756_f64).to_radians().cos();
        let stop_lat = 10.7756 + 5.0 / 110_540.0;
        let stop_lon = 106.7019 + 100.0 / (cos_lat * 111_320.0);

        let stops = vec![GtfsStop {
            stop_id: "S1".to_string(),
            name: "Test Stop".to_string(),
            lat: stop_lat,
            lon: stop_lon,
        }];

        let bus_stops = snap_gtfs_stops(&stops, &graph, &proj);
        assert_eq!(bus_stops.len(), 1);
        assert_eq!(bus_stops[0].edge_id, 0);
        assert!((bus_stops[0].offset_m - 100.0).abs() < 1.0, "offset ~100m, got {}", bus_stops[0].offset_m);
        assert_eq!(bus_stops[0].name, "Test Stop");
        assert_eq!(bus_stops[0].capacity, 40);
    }

    #[test]
    fn snap_gtfs_stops_skips_far_stops() {
        let graph = make_graph(vec![(0, 1, vec![[0.0, 0.0], [100.0, 0.0]])]);
        let proj = EquirectangularProjection::new(10.7756, 106.7019);

        // Stop 500m north -- way beyond 200m snap radius
        let stop_lat = 10.7756 + 500.0 / 110_540.0;
        let stops = vec![GtfsStop {
            stop_id: "S_FAR".to_string(),
            name: "Far Stop".to_string(),
            lat: stop_lat,
            lon: 106.7019,
        }];

        let bus_stops = snap_gtfs_stops(&stops, &graph, &proj);
        assert!(bus_stops.is_empty(), "far stop should be skipped");
    }

    #[test]
    fn snap_gtfs_stops_merges_duplicates() {
        // Edge from (0,0) to (200,0)
        let graph = make_graph(vec![(0, 1, vec![[0.0, 0.0], [200.0, 0.0]])]);
        let proj = EquirectangularProjection::new(10.7756, 106.7019);
        let cos_lat = (10.7756_f64).to_radians().cos();

        // Two stops that map to very close positions on the same edge (within 10m)
        let lon1 = 106.7019 + 100.0 / (cos_lat * 111_320.0);
        let lon2 = 106.7019 + 105.0 / (cos_lat * 111_320.0); // 5m apart along edge

        let stops = vec![
            GtfsStop {
                stop_id: "S1".to_string(),
                name: "Stop A".to_string(),
                lat: 10.7756,
                lon: lon1,
            },
            GtfsStop {
                stop_id: "S2".to_string(),
                name: "Stop B".to_string(),
                lat: 10.7756,
                lon: lon2,
            },
        ];

        let bus_stops = snap_gtfs_stops(&stops, &graph, &proj);
        assert_eq!(bus_stops.len(), 1, "duplicates within 10m should merge");
    }

    #[test]
    fn snap_gtfs_stops_empty_input() {
        let graph = make_graph(vec![(0, 1, vec![[0.0, 0.0], [100.0, 0.0]])]);
        let proj = EquirectangularProjection::new(10.7756, 106.7019);

        let bus_stops = snap_gtfs_stops(&[], &graph, &proj);
        assert!(bus_stops.is_empty());
    }

    #[test]
    fn project_onto_segment_degenerate() {
        let (t, _, dist) = project_onto_segment([5.0, 5.0], [0.0, 0.0], [0.0, 0.0]);
        assert_eq!(t, 0.0);
        assert!((dist - (50.0_f64).sqrt()).abs() < 0.01);
    }
}
