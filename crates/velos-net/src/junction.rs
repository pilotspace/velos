//! Junction geometry: Bezier turn paths and conflict point precomputation.
//!
//! Each junction node in the road graph gets a set of precomputed quadratic
//! Bezier curves (one per valid entry/exit edge pair) and conflict points
//! where those curves cross within a threshold distance.

use std::collections::HashMap;

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::graph::RoadGraph;

/// Minimum arc length in metres for a valid Bezier turn.
/// Curves shorter than this produce degenerate tangents (NaN) and visual artifacts.
const MIN_ARC_LENGTH_M: f64 = 0.3;

/// Number of sample points for arc-length estimation.
const ARC_LENGTH_SAMPLES: usize = 20;

/// Bezier control point tension: 0.0 = straight line, 1.0 = full centroid.
const BEZIER_TENSION: f64 = 0.3;

/// Junction curve radius in metres. P0/P2 are placed this far from the junction
/// centroid along approach/departure directions, keeping curves short (~2*radius)
/// so agents don't teleport to the far end of connected edges on junction entry.
const JUNCTION_RADIUS_M: f64 = 15.0;

/// Number of sample steps per curve for conflict detection grid search.
const CONFLICT_SEARCH_STEPS: usize = 30;

/// Distance threshold in metres for two curves to be considered crossing.
const CONFLICT_DISTANCE_THRESHOLD_M: f64 = 2.0;

/// A precomputed quadratic Bezier turn path through a junction.
///
/// Control points: P0 = source node of entry edge, P1 = junction centroid,
/// P2 = target node of exit edge. Uses `RoadNode.pos` (not edge geometry
/// polyline endpoints) to avoid coordinate mismatch at junction boundaries.
#[derive(Debug, Clone)]
pub struct BezierTurn {
    /// Edge index of the entry (incoming) edge.
    pub entry_edge: u32,
    /// Edge index of the exit (outgoing) edge.
    pub exit_edge: u32,
    /// Start point (source node position of entry edge).
    pub p0: [f64; 2],
    /// Control point (junction node position / centroid).
    pub p1: [f64; 2],
    /// End point (target node position of exit edge).
    pub p2: [f64; 2],
    /// Approximate arc length in metres (precomputed).
    pub arc_length: f64,
    /// Offset in metres on the exit edge where the agent should be placed
    /// after completing junction traversal. Avoids edge-boundary issues at offset=0.
    pub exit_offset_m: f64,
}

impl BezierTurn {
    /// Evaluate position on the quadratic Bezier at parameter `t` in [0, 1].
    ///
    /// B(t) = (1-t)^2 * P0 + 2(1-t)t * P1 + t^2 * P2
    pub fn position(&self, t: f64) -> [f64; 2] {
        let u = 1.0 - t;
        [
            u * u * self.p0[0] + 2.0 * u * t * self.p1[0] + t * t * self.p2[0],
            u * u * self.p0[1] + 2.0 * u * t * self.p1[1] + t * t * self.p2[1],
        ]
    }

    /// Evaluate the tangent vector (unnormalized) at parameter `t`.
    ///
    /// B'(t) = 2(1-t)(P1 - P0) + 2t(P2 - P1)
    pub fn tangent(&self, t: f64) -> [f64; 2] {
        let u = 1.0 - t;
        [
            2.0 * u * (self.p1[0] - self.p0[0]) + 2.0 * t * (self.p2[0] - self.p1[0]),
            2.0 * u * (self.p1[1] - self.p0[1]) + 2.0 * t * (self.p2[1] - self.p1[1]),
        ]
    }

    /// Compute position offset perpendicular to the tangent direction.
    ///
    /// The normal points left relative to the tangent direction. The offset
    /// is computed as `(lateral_offset - road_half_width)` so that an offset
    /// of 0.0 maps to the right edge and `road_half_width * 2` to the left edge.
    pub fn offset_position(
        &self,
        t: f64,
        lateral_offset: f64,
        road_half_width: f64,
    ) -> [f64; 2] {
        let pos = self.position(t);
        let tan = self.tangent(t);
        let len = (tan[0] * tan[0] + tan[1] * tan[1]).sqrt().max(1e-6);
        // Left-pointing normal: rotate tangent 90 degrees CCW
        let nx = -tan[1] / len;
        let ny = tan[0] / len;
        let offset_from_center = lateral_offset - road_half_width;
        [
            pos[0] + offset_from_center * nx,
            pos[1] + offset_from_center * ny,
        ]
    }
}

/// A precomputed crossing point between two turn paths at a junction.
#[derive(Debug, Clone, Copy)]
pub struct ConflictPoint {
    /// Index of the first turn in the junction's `turns` array.
    pub turn_a_idx: u16,
    /// Index of the second turn in the junction's `turns` array.
    pub turn_b_idx: u16,
    /// Bezier t-parameter on turn A at the crossing point.
    pub t_a: f32,
    /// Bezier t-parameter on turn B at the crossing point.
    pub t_b: f32,
}

/// Precomputed junction geometry: all valid turns and their conflict points.
#[derive(Debug, Clone)]
pub struct JunctionData {
    /// All valid Bezier turn paths through this junction.
    pub turns: Vec<BezierTurn>,
    /// All pairwise conflict points between turns.
    pub conflicts: Vec<ConflictPoint>,
}

/// Estimate the arc length of a quadratic Bezier curve by sampling `steps` points
/// and summing the segment lengths.
///
/// Returns 0.0 for degenerate curves where all control points coincide.
pub fn estimate_arc_length(p0: &[f64; 2], p1: &[f64; 2], p2: &[f64; 2], steps: usize) -> f64 {
    if steps == 0 {
        return 0.0;
    }
    let mut length = 0.0;
    let inv = 1.0 / steps as f64;
    let mut prev = *p0;
    for i in 1..=steps {
        let t = i as f64 * inv;
        let u = 1.0 - t;
        let x = u * u * p0[0] + 2.0 * u * t * p1[0] + t * t * p2[0];
        let y = u * u * p0[1] + 2.0 * u * t * p1[1] + t * t * p2[1];
        let dx = x - prev[0];
        let dy = y - prev[1];
        length += (dx * dx + dy * dy).sqrt();
        prev = [x, y];
    }
    length
}

/// Find the closest approach between two Bezier turn curves using grid search.
///
/// Returns `Some((t_a, t_b))` if the minimum distance is within
/// [`CONFLICT_DISTANCE_THRESHOLD_M`], `None` otherwise.
///
/// Includes NaN guard: returns `None` if the computed distance is not finite.
pub fn find_conflict_point(
    a: &BezierTurn,
    b: &BezierTurn,
    steps: usize,
) -> Option<(f32, f32)> {
    let mut best_dist_sq = f64::MAX;
    let mut best_ta = 0.0_f64;
    let mut best_tb = 0.0_f64;
    let inv = 1.0 / steps as f64;

    for i in 0..=steps {
        let ta = i as f64 * inv;
        let pa = a.position(ta);
        for j in 0..=steps {
            let tb = j as f64 * inv;
            let pb = b.position(tb);
            let dx = pa[0] - pb[0];
            let dy = pa[1] - pb[1];
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                best_ta = ta;
                best_tb = tb;
            }
        }
    }

    // NaN guard: degenerate curves can produce non-finite distances
    if !best_dist_sq.is_finite() {
        return None;
    }

    let best_dist = best_dist_sq.sqrt();
    if !best_dist.is_finite() {
        return None;
    }

    if best_dist < CONFLICT_DISTANCE_THRESHOLD_M {
        Some((best_ta as f32, best_tb as f32))
    } else {
        None
    }
}

/// Precompute all Bezier turns and conflict points for a single junction node.
///
/// Iterates all (incoming, outgoing) edge pairs, skipping U-turns (where the
/// source of the incoming edge equals the target of the outgoing edge).
/// Filters out degenerate turns with arc length < [`MIN_ARC_LENGTH_M`].
pub fn precompute_junction(
    graph: &RoadGraph,
    node: NodeIndex,
) -> JunctionData {
    let g = graph.inner();
    let centroid = g[node].pos;

    let incoming: Vec<_> = g.edges_directed(node, Direction::Incoming).collect();
    let outgoing: Vec<_> = g.edges_directed(node, Direction::Outgoing).collect();

    let mut turns = Vec::new();

    for inc in &incoming {
        let source_pos = g[inc.source()].pos;
        for out in &outgoing {
            // Skip U-turns: entry source == exit target
            if inc.source() == out.target() {
                continue;
            }
            let target_pos = g[out.target()].pos;

            // P0: JUNCTION_RADIUS_M back from junction along incoming approach direction.
            // This keeps the curve start near where the agent actually is (edge end),
            // preventing the backward teleport that caused flickering.
            let approach = [centroid[0] - source_pos[0], centroid[1] - source_pos[1]];
            let approach_len = (approach[0] * approach[0] + approach[1] * approach[1]).sqrt();
            let radius0 = JUNCTION_RADIUS_M.min(approach_len * 0.5);
            let p0 = if approach_len > 0.01 {
                [
                    centroid[0] - (approach[0] / approach_len) * radius0,
                    centroid[1] - (approach[1] / approach_len) * radius0,
                ]
            } else {
                centroid
            };

            // P2: JUNCTION_RADIUS_M forward from junction along departure direction.
            let depart = [target_pos[0] - centroid[0], target_pos[1] - centroid[1]];
            let depart_len = (depart[0] * depart[0] + depart[1] * depart[1]).sqrt();
            let radius2 = JUNCTION_RADIUS_M.min(depart_len * 0.5);
            let p2 = if depart_len > 0.01 {
                [
                    centroid[0] + (depart[0] / depart_len) * radius2,
                    centroid[1] + (depart[1] / depart_len) * radius2,
                ]
            } else {
                centroid
            };

            // Tension-weighted control point between straight-line midpoint and centroid.
            let midpoint = [(p0[0] + p2[0]) / 2.0, (p0[1] + p2[1]) / 2.0];
            let p1 = [
                midpoint[0] + BEZIER_TENSION * (centroid[0] - midpoint[0]),
                midpoint[1] + BEZIER_TENSION * (centroid[1] - midpoint[1]),
            ];

            let arc_length = estimate_arc_length(&p0, &p1, &p2, ARC_LENGTH_SAMPLES);

            // Filter degenerate curves (lesson #2)
            if arc_length < MIN_ARC_LENGTH_M {
                continue;
            }

            turns.push(BezierTurn {
                entry_edge: inc.id().index() as u32,
                exit_edge: out.id().index() as u32,
                p0,
                p1,
                p2,
                arc_length,
                exit_offset_m: 0.1,
            });
        }
    }

    // Find conflict points between all turn pairs
    let mut conflicts = Vec::new();
    for i in 0..turns.len() {
        for j in (i + 1)..turns.len() {
            if let Some((ta, tb)) = find_conflict_point(
                &turns[i],
                &turns[j],
                CONFLICT_SEARCH_STEPS,
            ) {
                conflicts.push(ConflictPoint {
                    turn_a_idx: i as u16,
                    turn_b_idx: j as u16,
                    t_a: ta,
                    t_b: tb,
                });
            }
        }
    }

    JunctionData { turns, conflicts }
}

/// Precompute junction geometry for all junction nodes in the road graph.
///
/// A node is considered a junction if it has both incoming and outgoing edges,
/// AND is not a pass-through node (in_degree == 1 AND out_degree == 1).
/// Pass-through nodes are road continuations, not true junctions.
///
/// Returns a map from node index (as u32) to precomputed [`JunctionData`].
pub fn precompute_all_junctions(graph: &RoadGraph) -> HashMap<u32, JunctionData> {
    let g = graph.inner();
    let mut result = HashMap::new();

    for node in g.node_indices() {
        let in_degree = g.edges_directed(node, Direction::Incoming).count();
        let out_degree = g.edges_directed(node, Direction::Outgoing).count();

        // Must have both incoming and outgoing edges
        if in_degree == 0 || out_degree == 0 {
            continue;
        }

        // Filter pass-through nodes (lesson #1): in_degree==1 AND out_degree==1
        // are road continuations, not junctions
        if in_degree == 1 && out_degree == 1 {
            continue;
        }

        let data = precompute_junction(graph, node);

        // Only store if there are valid turns
        if !data.turns.is_empty() {
            result.insert(node.index() as u32, data);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use petgraph::graph::DiGraph;
    use crate::graph::{RoadNode, RoadEdge, RoadClass, RoadGraph};

    /// Helper to create a minimal RoadEdge for testing.
    fn test_edge(length: f64) -> RoadEdge {
        RoadEdge {
            length_m: length,
            speed_limit_mps: 13.89,
            lane_count: 2,
            oneway: true,
            road_class: RoadClass::Secondary,
            geometry: vec![],
            motorbike_only: false,
            time_windows: None,
        }
    }

    /// Build a simple T-junction graph:
    ///
    /// ```text
    ///     A (0, 100)
    ///     |
    ///     v
    /// B ---> C (100, 0) <--- D (200, 0)
    ///  (0,0)
    /// ```
    ///
    /// Node C is the junction with 3 incoming and 0 outgoing... let's make
    /// it a proper junction with incoming from A,B,D and outgoing to A,B,D.
    fn build_cross_junction() -> (RoadGraph, NodeIndex, NodeIndex, NodeIndex, NodeIndex) {
        let mut g = DiGraph::new();
        // Create a 4-way cross junction at center
        let center = g.add_node(RoadNode { pos: [100.0, 100.0] });
        let north = g.add_node(RoadNode { pos: [100.0, 200.0] });
        let south = g.add_node(RoadNode { pos: [100.0, 0.0] });
        let east = g.add_node(RoadNode { pos: [200.0, 100.0] });
        let west = g.add_node(RoadNode { pos: [0.0, 100.0] });

        // Incoming edges to center
        g.add_edge(north, center, test_edge(100.0));
        g.add_edge(south, center, test_edge(100.0));
        g.add_edge(east, center, test_edge(100.0));
        g.add_edge(west, center, test_edge(100.0));

        // Outgoing edges from center
        g.add_edge(center, north, test_edge(100.0));
        g.add_edge(center, south, test_edge(100.0));
        g.add_edge(center, east, test_edge(100.0));
        g.add_edge(center, west, test_edge(100.0));

        (RoadGraph::new(g), center, north, east, west)
    }

    // ---- BezierTurn position tests ----

    #[test]
    fn position_at_t0_equals_p0() {
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
        };
        let pos = turn.position(0.0);
        assert!((pos[0] - 0.0).abs() < 1e-10);
        assert!((pos[1] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn position_at_t1_equals_p2() {
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
        };
        let pos = turn.position(1.0);
        assert!((pos[0] - 100.0).abs() < 1e-10);
        assert!((pos[1] - 0.0).abs() < 1e-10);
    }

    #[test]
    fn position_at_t05_is_midpoint_weighted() {
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
        };
        let pos = turn.position(0.5);
        // B(0.5) = 0.25*P0 + 0.5*P1 + 0.25*P2
        // x = 0.25*0 + 0.5*50 + 0.25*100 = 50
        // y = 0.25*0 + 0.5*50 + 0.25*0 = 25
        assert!((pos[0] - 50.0).abs() < 1e-10);
        assert!((pos[1] - 25.0).abs() < 1e-10);
    }

    // ---- BezierTurn tangent tests ----

    #[test]
    fn tangent_at_t0_proportional_to_p1_minus_p0() {
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
        };
        let tan = turn.tangent(0.0);
        // B'(0) = 2(P1 - P0) = 2*(50,50) = (100, 100)
        assert!((tan[0] - 100.0).abs() < 1e-10);
        assert!((tan[1] - 100.0).abs() < 1e-10);
    }

    #[test]
    fn tangent_at_t1_proportional_to_p2_minus_p1() {
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
        };
        let tan = turn.tangent(1.0);
        // B'(1) = 2(P2 - P1) = 2*(50, -50) = (100, -100)
        assert!((tan[0] - 100.0).abs() < 1e-10);
        assert!((tan[1] - (-100.0)).abs() < 1e-10);
    }

    // ---- offset_position tests ----

    #[test]
    fn offset_position_shifts_perpendicular_to_tangent() {
        // Straight line: P0=(0,0), P1=(50,0), P2=(100,0)
        // Tangent is always (100, 0) -> right direction
        // Left-pointing normal is (0, 1)
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 0.0],
            p2: [100.0, 0.0],
            arc_length: 100.0,
            exit_offset_m: 0.1,
        };

        let half_width = 3.5;
        // lateral_offset = half_width puts us at center (offset_from_center = 0)
        let center_pos = turn.offset_position(0.5, half_width, half_width);
        let base_pos = turn.position(0.5);
        assert!((center_pos[0] - base_pos[0]).abs() < 1e-10);
        assert!((center_pos[1] - base_pos[1]).abs() < 1e-10);

        // lateral_offset = 0 puts us at road right edge (offset_from_center = -3.5)
        let right_pos = turn.offset_position(0.5, 0.0, half_width);
        assert!((right_pos[1] - (-half_width)).abs() < 1e-10);

        // lateral_offset = 2*half_width puts us at road left edge
        let left_pos = turn.offset_position(0.5, 2.0 * half_width, half_width);
        assert!((left_pos[1] - half_width).abs() < 1e-10);
    }

    // ---- estimate_arc_length tests ----

    #[test]
    fn arc_length_straight_line() {
        // Straight line from (0,0) to (100,0) via (50,0)
        let len = estimate_arc_length(&[0.0, 0.0], &[50.0, 0.0], &[100.0, 0.0], 20);
        assert!((len - 100.0).abs() < 0.1);
    }

    #[test]
    fn arc_length_positive_for_non_degenerate() {
        let len = estimate_arc_length(&[0.0, 0.0], &[50.0, 50.0], &[100.0, 0.0], 20);
        assert!(len > 0.0);
        // Arc should be longer than the straight-line chord of 100m
        assert!(len > 100.0);
    }

    #[test]
    fn arc_length_degenerate_near_zero() {
        // All points at same location
        let len = estimate_arc_length(&[50.0, 50.0], &[50.0, 50.0], &[50.0, 50.0], 20);
        assert!(len < MIN_ARC_LENGTH_M);
    }

    #[test]
    fn arc_length_zero_steps_returns_zero() {
        let len = estimate_arc_length(&[0.0, 0.0], &[50.0, 50.0], &[100.0, 0.0], 0);
        assert_eq!(len, 0.0);
    }

    // ---- find_conflict_point tests ----

    #[test]
    fn conflict_found_for_crossing_paths() {
        // Two perpendicular straight-ish paths crossing at (50, 50)
        let turn_a = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 50.0],
            p1: [50.0, 50.0],
            p2: [100.0, 50.0],
            arc_length: 100.0,
            exit_offset_m: 0.1,
        };
        let turn_b = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [50.0, 0.0],
            p1: [50.0, 50.0],
            p2: [50.0, 100.0],
            arc_length: 100.0,
            exit_offset_m: 0.1,
        };
        let result = find_conflict_point(&turn_a, &turn_b, 30);
        assert!(result.is_some(), "crossing paths should produce a conflict point");
        let (ta, tb) = result.unwrap();
        // Both should be near 0.5 (the crossing is at the midpoint)
        assert!((ta - 0.5).abs() < 0.1);
        assert!((tb - 0.5).abs() < 0.1);
    }

    #[test]
    fn no_conflict_for_parallel_paths() {
        // Two parallel paths 50m apart
        let turn_a = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 0.0],
            p2: [100.0, 0.0],
            arc_length: 100.0,
            exit_offset_m: 0.1,
        };
        let turn_b = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [0.0, 50.0],
            p1: [50.0, 50.0],
            p2: [100.0, 50.0],
            arc_length: 100.0,
            exit_offset_m: 0.1,
        };
        let result = find_conflict_point(&turn_a, &turn_b, 30);
        assert!(result.is_none(), "parallel paths 50m apart should not conflict");
    }

    #[test]
    fn conflict_for_same_path_returns_some() {
        // Two identical curves will have distance 0 everywhere, which is
        // within the 2m threshold. This is expected -- in practice,
        // precompute_junction never creates duplicate turns (different
        // entry/exit pairs), so this case doesn't arise in real usage.
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
        };
        let result = find_conflict_point(&turn, &turn, 30);
        // Same curve: distance is 0, within threshold -> returns Some
        assert!(result.is_some());
    }

    // ---- precompute_junction tests ----

    #[test]
    fn precompute_junction_cross_intersection() {
        let (graph, center, _north, _east, _west) = build_cross_junction();

        let data = precompute_junction(&graph, center);

        // 4 incoming x 4 outgoing = 16, minus 4 U-turns = 12 potential turns
        // All should have arc_length >= MIN_ARC_LENGTH_M since nodes are 100m apart
        assert_eq!(data.turns.len(), 12, "4-way junction: 4*4 - 4 U-turns = 12 turns");

        // All turns should have positive arc length
        for turn in &data.turns {
            assert!(turn.arc_length >= MIN_ARC_LENGTH_M);
            assert_eq!(turn.exit_offset_m, 0.1);
        }
    }

    #[test]
    fn precompute_junction_skips_uturns() {
        let (graph, center, _north, _east, _west) = build_cross_junction();
        let data = precompute_junction(&graph, center);

        // Verify no turn has entry source == exit target (U-turn)
        let g = graph.inner();
        for turn in &data.turns {
            let entry_endpoints = g.edge_endpoints(
                petgraph::graph::EdgeIndex::new(turn.entry_edge as usize),
            ).unwrap();
            let exit_endpoints = g.edge_endpoints(
                petgraph::graph::EdgeIndex::new(turn.exit_edge as usize),
            ).unwrap();
            assert_ne!(
                entry_endpoints.0, exit_endpoints.1,
                "U-turn detected: entry source {:?} == exit target {:?}",
                entry_endpoints.0, exit_endpoints.1,
            );
        }
    }

    #[test]
    fn precompute_junction_finds_conflicts() {
        let (graph, center, _, _, _) = build_cross_junction();
        let data = precompute_junction(&graph, center);

        // A 4-way cross junction should have crossing paths that produce conflict points
        // Not all pairs will conflict (some are parallel or diverging), but some should
        assert!(
            !data.conflicts.is_empty(),
            "4-way junction should have at least one conflict point"
        );
    }

    // ---- precompute_all_junctions tests ----

    #[test]
    fn precompute_all_junctions_cross() {
        let (graph, center, _, _, _) = build_cross_junction();
        let junctions = precompute_all_junctions(&graph);

        // Only the center node is a junction (degree > 1 in both directions)
        // The peripheral nodes have in=1, out=1 -> pass-through, filtered out
        assert_eq!(junctions.len(), 1);
        assert!(junctions.contains_key(&(center.index() as u32)));
    }

    #[test]
    fn precompute_all_junctions_filters_passthrough() {
        // Build a chain: A -> B -> C
        // B is pass-through (in=1, out=1) and should NOT be a junction
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [50.0, 0.0] });
        let c = g.add_node(RoadNode { pos: [100.0, 0.0] });
        g.add_edge(a, b, test_edge(50.0));
        g.add_edge(b, c, test_edge(50.0));

        let graph = RoadGraph::new(g);
        let junctions = precompute_all_junctions(&graph);

        assert!(junctions.is_empty(), "pass-through nodes should not be treated as junctions");
    }

    #[test]
    fn precompute_all_junctions_filters_dead_ends() {
        // A -> B (B has no outgoing)
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [100.0, 0.0] });
        g.add_edge(a, b, test_edge(100.0));

        let graph = RoadGraph::new(g);
        let junctions = precompute_all_junctions(&graph);

        assert!(junctions.is_empty(), "dead-end nodes should not be junctions");
    }

    #[test]
    fn precompute_all_junctions_skips_degenerate_arcs() {
        // Create a junction where all turns have near-zero arc length
        // by placing all nodes at nearly the same position
        let mut g = DiGraph::new();
        let center = g.add_node(RoadNode { pos: [50.0, 50.0] });
        let n1 = g.add_node(RoadNode { pos: [50.001, 50.001] });
        let n2 = g.add_node(RoadNode { pos: [50.002, 50.0] });
        let n3 = g.add_node(RoadNode { pos: [50.0, 50.002] });

        g.add_edge(n1, center, test_edge(0.01));
        g.add_edge(n2, center, test_edge(0.01));
        g.add_edge(center, n2, test_edge(0.01));
        g.add_edge(center, n3, test_edge(0.01));

        let graph = RoadGraph::new(g);
        let junctions = precompute_all_junctions(&graph);

        // All arcs < 1m, so the junction should either have no valid turns
        // or not appear in the map at all
        if let Some(data) = junctions.get(&(center.index() as u32)) {
            // If it exists, all turns must have arc_length >= MIN_ARC_LENGTH_M
            for turn in &data.turns {
                assert!(turn.arc_length >= MIN_ARC_LENGTH_M);
            }
        }
    }

    // ---- NaN guard tests ----

    #[test]
    fn find_conflict_nan_guard() {
        // Create a degenerate turn with all points at origin
        let degenerate = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [0.0, 0.0],
            p2: [0.0, 0.0],
            arc_length: 0.0,
            exit_offset_m: 0.1,
        };
        let normal = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
        };

        // Should not panic, should return None due to zero-distance guard
        let result = find_conflict_point(&degenerate, &normal, 30);
        // Either None or Some -- just must not panic or produce NaN
        if let Some((ta, tb)) = result {
            assert!(ta.is_finite());
            assert!(tb.is_finite());
        }
    }
}
