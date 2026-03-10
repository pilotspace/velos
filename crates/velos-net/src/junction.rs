//! Junction geometry: Bezier turn paths and conflict point precomputation.
//!
//! Each junction node in the road graph gets a set of precomputed quadratic
//! Bezier curves (one per valid entry/exit edge pair) and conflict points
//! where those curves cross within a threshold distance.
//!
//! **Close-junction merging:** When two junction nodes are within
//! [`MERGE_DISTANCE_M`], they are merged into a single cluster with a shared
//! centroid. Turns are computed only for *peripheral* edges (edges whose other
//! endpoint is outside the cluster), eliminating the sharp Bezier discontinuities
//! that cause teleportation between closely-spaced junctions.

use std::collections::{HashMap, HashSet};

use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::Direction;

use crate::graph::RoadGraph;

/// Minimum arc length in metres for a valid Bezier turn.
/// Curves shorter than this produce degenerate tangents (NaN) and visual artifacts.
const MIN_ARC_LENGTH_M: f64 = 0.3;

/// Number of sample points for arc-length estimation.
const ARC_LENGTH_SAMPLES: usize = 10;

// BEZIER_TENSION removed — P1 is now computed so B(0.5) = centroid exactly.

/// Junction curve radius in metres. P0/P2 are placed this far from the junction
/// centroid along approach/departure directions, keeping curves short (~2*radius)
/// so agents don't teleport to the far end of connected edges on junction entry.
/// Reduced from 15m to 8m to keep Bezier curves tighter and on-road.
const JUNCTION_RADIUS_M: f64 = 8.0;

/// Maximum distance (metres) between two junction nodes for them to be merged
/// into a single cluster. Set to `2 * JUNCTION_RADIUS_M` so overlapping Bezier
/// curves are unified rather than fighting each other.
const MERGE_DISTANCE_M: f64 = 5.0;

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
    /// after completing junction traversal. Matches P2's projection on the
    /// exit edge (~departure radius) so there is no position discontinuity.
    pub exit_offset_m: f64,
    /// Bezier t-parameter where the curve passes closest to the junction
    /// centroid. Agents start here (not t=0) so their position matches the
    /// edge endpoint, eliminating the ~15m backward teleport on entry.
    pub entry_t: f64,
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

    /// Find the Bezier t-parameter closest to a target position.
    ///
    /// Grid search with `samples` steps. Used to preserve position continuity
    /// when a vehicle enters or chains into a junction — the initial t matches
    /// the vehicle's current world position rather than a fixed `entry_t`.
    pub fn find_closest_t(&self, target: [f64; 2], samples: usize) -> f64 {
        let mut best_t = 0.0;
        let mut best_dist_sq = f64::MAX;
        let inv = 1.0 / samples.max(1) as f64;
        for i in 0..=samples {
            let t = i as f64 * inv;
            let pos = self.position(t);
            let dx = pos[0] - target[0];
            let dy = pos[1] - target[1];
            let dist_sq = dx * dx + dy * dy;
            if dist_sq < best_dist_sq {
                best_dist_sq = dist_sq;
                best_t = t;
            }
        }
        best_t
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
    /// Edge indices that are *internal* to a merged cluster (both endpoints are
    /// cluster members). Agents on these edges skip normal physics and chain
    /// directly into the merged junction. Empty for single-node junctions.
    pub internal_edges: HashSet<u32>,
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

/// Minimum t-parameter distance from curve endpoints for a valid conflict.
/// Conflicts at t < this or t > 1-this are filtered out because they
/// represent shared entry/exit geometry, not true crossing points.
const CONFLICT_ENDPOINT_MARGIN: f64 = 0.08;

/// Find the closest approach between two Bezier turn curves using grid search.
///
/// Returns `Some((t_a, t_b))` if the minimum distance is within
/// [`CONFLICT_DISTANCE_THRESHOLD_M`], `None` otherwise.
///
/// Filters out conflicts at curve endpoints (t < 0.08 or t > 0.92) which
/// represent shared entry/exit geometry rather than actual crossing points.
/// Also filters conflicts between turns sharing an entry or exit edge.
///
/// Includes NaN guard: returns `None` if the computed distance is not finite.
pub fn find_conflict_point(
    a: &BezierTurn,
    b: &BezierTurn,
    steps: usize,
) -> Option<(f32, f32)> {
    // Skip turns that share entry or exit edges — they converge/diverge at
    // endpoints, not cross in the middle. False conflict points here cause
    // vehicles to yield permanently at junction start/end.
    if a.entry_edge == b.entry_edge || a.exit_edge == b.exit_edge {
        return None;
    }

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

    if best_dist >= CONFLICT_DISTANCE_THRESHOLD_M {
        return None;
    }

    // Filter endpoint conflicts: crossings at curve start/end are shared
    // geometry (common entry/exit node), not real intersection crossings.
    let valid_range = CONFLICT_ENDPOINT_MARGIN..=(1.0 - CONFLICT_ENDPOINT_MARGIN);
    if !valid_range.contains(&best_ta) || !valid_range.contains(&best_tb) {
        return None;
    }

    Some((best_ta as f32, best_tb as f32))
}

// find_closest_t is now a method on BezierTurn — see BezierTurn::find_closest_t.

/// Extract approach direction toward `centroid` from edge geometry.
/// Uses the last geometry segment (near junction) for the tangent direction,
/// falling back to straight-line source→centroid if geometry is too short.
/// Returns the unit direction vector FROM source TOWARD centroid.
fn approach_direction(geometry: &[[f64; 2]], source_pos: [f64; 2], centroid: [f64; 2]) -> [f64; 2] {
    // For incoming edges, geometry goes source→junction.
    // Use the last segment for accurate road direction near the junction.
    if geometry.len() >= 2 {
        let near = geometry[geometry.len() - 2];
        let at = geometry[geometry.len() - 1];
        let dx = at[0] - near[0];
        let dy = at[1] - near[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.01 {
            return [dx / len, dy / len];
        }
    }
    // Fallback: straight line
    let dx = centroid[0] - source_pos[0];
    let dy = centroid[1] - source_pos[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len > 0.01 { [dx / len, dy / len] } else { [1.0, 0.0] }
}

/// Extract departure direction from `centroid` using edge geometry.
/// Uses the first geometry segment (near junction) for the tangent direction.
/// Returns the unit direction vector FROM centroid TOWARD target.
fn departure_direction(geometry: &[[f64; 2]], target_pos: [f64; 2], centroid: [f64; 2]) -> [f64; 2] {
    // For outgoing edges, geometry goes junction→target.
    // Use the first segment for accurate road direction leaving the junction.
    if geometry.len() >= 2 {
        let at = geometry[0];
        let near = geometry[1];
        let dx = near[0] - at[0];
        let dy = near[1] - at[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len > 0.01 {
            return [dx / len, dy / len];
        }
    }
    // Fallback: straight line
    let dx = target_pos[0] - centroid[0];
    let dy = target_pos[1] - centroid[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len > 0.01 { [dx / len, dy / len] } else { [1.0, 0.0] }
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

            // Use edge geometry for accurate road direction near the junction.
            let approach_dir = approach_direction(&inc.weight().geometry, source_pos, centroid);
            let src_dist = ((centroid[0] - source_pos[0]).powi(2) + (centroid[1] - source_pos[1]).powi(2)).sqrt();
            let radius0 = JUNCTION_RADIUS_M.min(src_dist * 0.5);
            let p0 = [
                centroid[0] - approach_dir[0] * radius0,
                centroid[1] - approach_dir[1] * radius0,
            ];

            let depart_dir = departure_direction(&out.weight().geometry, target_pos, centroid);
            let tgt_dist = ((target_pos[0] - centroid[0]).powi(2) + (target_pos[1] - centroid[1]).powi(2)).sqrt();
            let radius2 = JUNCTION_RADIUS_M.min(tgt_dist * 0.5);
            let p2 = [
                centroid[0] + depart_dir[0] * radius2,
                centroid[1] + depart_dir[1] * radius2,
            ];

            // P1 = junction centroid. The curve passes near (not exactly through)
            // the centroid but stays within the convex hull of P0-centroid-P2,
            // keeping it on-road. The old formula P1=2*C-0.5*(P0+P2) forced
            // B(0.5)=centroid exactly but pushed P1 off-road when P0/P2 were
            // asymmetric around the centroid.
            let p1 = centroid;

            let arc_length = estimate_arc_length(&p0, &p1, &p2, ARC_LENGTH_SAMPLES);

            // Filter degenerate curves (lesson #2)
            if arc_length < MIN_ARC_LENGTH_M {
                continue;
            }

            let mut turn = BezierTurn {
                entry_edge: inc.id().index() as u32,
                exit_edge: out.id().index() as u32,
                p0,
                p1,
                p2,
                arc_length,
                exit_offset_m: radius2.max(0.1),
                entry_t: 0.0, // placeholder, computed below
            };
            turn.entry_t = turn.find_closest_t(centroid, ARC_LENGTH_SAMPLES);
            turns.push(turn);
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

    JunctionData {
        turns,
        conflicts,
        internal_edges: HashSet::new(),
    }
}

// ---------------------------------------------------------------------------
// Union-Find for clustering close junction nodes
// ---------------------------------------------------------------------------

/// Minimal Union-Find (disjoint-set) with path compression and union by rank.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        if self.rank[ra] < self.rank[rb] {
            self.parent[ra] = rb;
        } else if self.rank[ra] > self.rank[rb] {
            self.parent[rb] = ra;
        } else {
            self.parent[rb] = ra;
            self.rank[ra] += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// Merged-cluster junction precomputation
// ---------------------------------------------------------------------------

/// Precompute junction data for a merged cluster of close junction nodes.
///
/// The cluster centroid is the average position of all member nodes.
/// Only *peripheral* edges are used for turns — edges where the other endpoint
/// is **not** in the cluster. Internal edges (both endpoints in cluster) are
/// recorded in `JunctionData::internal_edges` so the traversal system can
/// chain through them without edge-based physics.
fn precompute_merged_junction(
    graph: &RoadGraph,
    cluster: &[NodeIndex],
) -> JunctionData {
    let g = graph.inner();
    let cluster_set: HashSet<NodeIndex> = cluster.iter().copied().collect();

    // Collect peripheral edges FIRST so we can compute centroid from
    // edge geometry (the actual road lines) rather than node positions.
    struct PeripheralEdge {
        edge_id: u32,
        outer_node_pos: [f64; 2],
        /// The cluster-side endpoint of this edge (the node inside the cluster).
        inner_node_pos: [f64; 2],
    }

    let mut incoming: Vec<PeripheralEdge> = Vec::new();
    let mut outgoing: Vec<PeripheralEdge> = Vec::new();
    let mut internal_edges: HashSet<u32> = HashSet::new();

    for &node in cluster {
        for edge_ref in g.edges_directed(node, Direction::Incoming) {
            let edge_id = edge_ref.id().index() as u32;
            if cluster_set.contains(&edge_ref.source()) {
                internal_edges.insert(edge_id);
            } else {
                incoming.push(PeripheralEdge {
                    edge_id,
                    outer_node_pos: g[edge_ref.source()].pos,
                    inner_node_pos: g[node].pos,
                });
            }
        }
        for edge_ref in g.edges_directed(node, Direction::Outgoing) {
            let edge_id = edge_ref.id().index() as u32;
            if cluster_set.contains(&edge_ref.target()) {
                internal_edges.insert(edge_id);
            } else {
                outgoing.push(PeripheralEdge {
                    edge_id,
                    outer_node_pos: g[edge_ref.target()].pos,
                    inner_node_pos: g[node].pos,
                });
            }
        }
    }

    // Deduplicate edges (a single edge may be seen from both endpoints in the cluster)
    let mut seen_in = HashSet::new();
    incoming.retain(|e| seen_in.insert(e.edge_id));
    let mut seen_out = HashSet::new();
    outgoing.retain(|e| seen_out.insert(e.edge_id));

    // Compute centroid from edge geometry: average the cluster-side
    // endpoints of all peripheral edges. These points sit ON the actual
    // road lines entering the intersection, so the centroid lands where
    // the roads converge — matching the visible map data.
    let centroid = {
        let mut cx = 0.0;
        let mut cy = 0.0;
        let total = (incoming.len() + outgoing.len()) as f64;
        if total > 0.0 {
            for e in incoming.iter().chain(outgoing.iter()) {
                cx += e.inner_node_pos[0];
                cy += e.inner_node_pos[1];
            }
            [cx / total, cy / total]
        } else {
            // Fallback: average of cluster node positions
            let mut fx = 0.0;
            let mut fy = 0.0;
            for &node in cluster {
                fx += g[node].pos[0];
                fy += g[node].pos[1];
            }
            let n = cluster.len() as f64;
            [fx / n, fy / n]
        }
    };

    // Build Bezier turns for all (peripheral_incoming, peripheral_outgoing) pairs
    let mut turns = Vec::new();

    for inc in &incoming {
        for out in &outgoing {
            // Skip U-turns: same outer node
            if inc.outer_node_pos == out.outer_node_pos {
                continue;
            }

            let source_pos = inc.outer_node_pos;
            let target_pos = out.outer_node_pos;

            // Use inner_node_pos (cluster-side endpoint) for approach direction —
            // more accurate than outer_node_pos for merged clusters.
            let approach = [centroid[0] - inc.inner_node_pos[0], centroid[1] - inc.inner_node_pos[1]];
            let approach_len = (approach[0] * approach[0] + approach[1] * approach[1]).sqrt();
            // If inner node is too close to centroid, fall back to outer node direction
            let (a_dir, a_dist) = if approach_len > 0.5 {
                ([approach[0] / approach_len, approach[1] / approach_len], approach_len)
            } else {
                let dx = centroid[0] - source_pos[0];
                let dy = centroid[1] - source_pos[1];
                let d = (dx * dx + dy * dy).sqrt();
                if d > 0.01 { ([dx / d, dy / d], d) } else { ([1.0, 0.0], 1.0) }
            };
            let radius0 = JUNCTION_RADIUS_M.min(a_dist * 0.5);
            let p0 = [centroid[0] - a_dir[0] * radius0, centroid[1] - a_dir[1] * radius0];

            let depart = [out.inner_node_pos[0] - centroid[0], out.inner_node_pos[1] - centroid[1]];
            let depart_len = (depart[0] * depart[0] + depart[1] * depart[1]).sqrt();
            let (d_dir, d_dist) = if depart_len > 0.5 {
                ([depart[0] / depart_len, depart[1] / depart_len], depart_len)
            } else {
                let dx = target_pos[0] - centroid[0];
                let dy = target_pos[1] - centroid[1];
                let d = (dx * dx + dy * dy).sqrt();
                if d > 0.01 { ([dx / d, dy / d], d) } else { ([1.0, 0.0], 1.0) }
            };
            let radius2 = JUNCTION_RADIUS_M.min(d_dist * 0.5);
            let p2 = [centroid[0] + d_dir[0] * radius2, centroid[1] + d_dir[1] * radius2];

            // P1 = centroid directly, keeping curve on-road.
            let p1 = centroid;

            let arc_length = estimate_arc_length(&p0, &p1, &p2, ARC_LENGTH_SAMPLES);
            // For merged clusters, require a higher minimum arc length to
            // eliminate tiny overlapping paths that cause vehicles to get stuck.
            let min_arc = if cluster.len() > 1 { 2.0 } else { MIN_ARC_LENGTH_M };
            if arc_length < min_arc {
                continue;
            }

            let mut turn = BezierTurn {
                entry_edge: inc.edge_id,
                exit_edge: out.edge_id,
                p0,
                p1,
                p2,
                arc_length,
                exit_offset_m: radius2.max(0.1),
                entry_t: 0.0,
            };
            turn.entry_t = turn.find_closest_t(centroid, ARC_LENGTH_SAMPLES);
            turns.push(turn);
        }
    }

    // Find conflict points between all turn pairs
    let mut conflicts = Vec::new();
    for i in 0..turns.len() {
        for j in (i + 1)..turns.len() {
            if let Some((ta, tb)) =
                find_conflict_point(&turns[i], &turns[j], CONFLICT_SEARCH_STEPS)
            {
                conflicts.push(ConflictPoint {
                    turn_a_idx: i as u16,
                    turn_b_idx: j as u16,
                    t_a: ta,
                    t_b: tb,
                });
            }
        }
    }

    JunctionData {
        turns,
        conflicts,
        internal_edges,
    }
}

/// Precompute junction geometry for all junction nodes in the road graph.
///
/// A node is considered a junction if it has both incoming and outgoing edges,
/// AND is not a pass-through node (in_degree == 1 AND out_degree == 1).
/// Pass-through nodes are road continuations, not true junctions.
///
/// **Close-junction merging:** Junction nodes within [`MERGE_DISTANCE_M`] of
/// each other are merged into clusters. Each cluster shares a single
/// [`JunctionData`] with a combined centroid and turns from peripheral edges
/// only, eliminating Bezier discontinuities between closely-spaced junctions.
///
/// Returns a map from node index (as u32) to precomputed [`JunctionData`].
pub fn precompute_all_junctions(graph: &RoadGraph) -> HashMap<u32, JunctionData> {
    let g = graph.inner();

    // Step 1: Identify junction nodes
    let mut junction_nodes: Vec<NodeIndex> = Vec::new();
    for node in g.node_indices() {
        let in_degree = g.edges_directed(node, Direction::Incoming).count();
        let out_degree = g.edges_directed(node, Direction::Outgoing).count();
        if in_degree == 0 || out_degree == 0 {
            continue;
        }
        if in_degree == 1 && out_degree == 1 {
            continue;
        }
        junction_nodes.push(node);
    }

    if junction_nodes.is_empty() {
        return HashMap::new();
    }

    // Step 2: Cluster close junction nodes using Union-Find.
    // Build index map: NodeIndex -> position in junction_nodes vec.
    let idx_map: HashMap<usize, usize> = junction_nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.index(), i))
        .collect();

    let mut uf = UnionFind::new(junction_nodes.len());

    // Only check edges between junction nodes (O(E) not O(N^2))
    for (i, &node) in junction_nodes.iter().enumerate() {
        let pos_a = g[node].pos;
        for edge_ref in g.edges_directed(node, Direction::Outgoing) {
            let target = edge_ref.target();
            if let Some(&j) = idx_map.get(&target.index()) {
                let pos_b = g[target].pos;
                let dx = pos_a[0] - pos_b[0];
                let dy = pos_a[1] - pos_b[1];
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < MERGE_DISTANCE_M {
                    uf.union(i, j);
                }
            }
        }
    }

    // Step 3: Group into clusters
    let mut clusters: HashMap<usize, Vec<NodeIndex>> = HashMap::new();
    for (i, &node) in junction_nodes.iter().enumerate() {
        let root = uf.find(i);
        clusters.entry(root).or_default().push(node);
    }

    // Step 4: Compute junction data per cluster
    let mut result = HashMap::new();

    for cluster in clusters.values() {
        let data = if cluster.len() == 1 {
            // Single-node junction — use original logic
            let d = precompute_junction(graph, cluster[0]);
            if d.turns.is_empty() {
                continue;
            }
            d
        } else {
            // Merged cluster — compute combined junction
            let d = precompute_merged_junction(graph, cluster);
            if d.turns.is_empty() {
                continue;
            }
            d
        };

        // Map ALL nodes in the cluster to the same JunctionData
        for &node in cluster {
            result.insert(node.index() as u32, data.clone());
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
            entry_t: 0.0,
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
            entry_t: 0.0,
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
            entry_t: 0.0,
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
            entry_t: 0.0,
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
            entry_t: 0.0,
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
            entry_t: 0.0,
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
            entry_t: 0.0,
        };
        let turn_b = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [50.0, 0.0],
            p1: [50.0, 50.0],
            p2: [50.0, 100.0],
            arc_length: 100.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
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
            entry_t: 0.0,
        };
        let turn_b = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [0.0, 50.0],
            p1: [50.0, 50.0],
            p2: [100.0, 50.0],
            arc_length: 100.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        let result = find_conflict_point(&turn_a, &turn_b, 30);
        assert!(result.is_none(), "parallel paths 50m apart should not conflict");
    }

    #[test]
    fn conflict_for_same_geometry_different_edges_filtered() {
        // Identical geometry but same entry/exit edges → filtered out by the
        // shared-edge check (not a real crossing). In practice precompute_junction
        // never produces duplicate turns.
        let turn = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        let result = find_conflict_point(&turn, &turn, 30);
        // Same entry/exit edges → filtered as non-crossing
        assert!(result.is_none());
    }

    #[test]
    fn conflict_for_crossing_paths_different_edges() {
        // Two crossing curves with distinct entry/exit edges → real conflict
        let turn_a = BezierTurn {
            entry_edge: 0,
            exit_edge: 1,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        let turn_b = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [50.0, -20.0],
            p1: [50.0, 50.0],
            p2: [50.0, 120.0],
            arc_length: 140.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };
        let result = find_conflict_point(&turn_a, &turn_b, 30);
        assert!(result.is_some(), "crossing paths with different edges should produce a conflict");
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
            // exit_offset_m matches the departure radius (~15m for 100m edges)
            assert!(turn.exit_offset_m > 0.1, "exit_offset_m should be radius, got {}", turn.exit_offset_m);
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
            entry_t: 0.0,
        };
        let normal = BezierTurn {
            entry_edge: 2,
            exit_edge: 3,
            p0: [0.0, 0.0],
            p1: [50.0, 50.0],
            p2: [100.0, 0.0],
            arc_length: 120.0,
            exit_offset_m: 0.1,
            entry_t: 0.0,
        };

        // Should not panic, should return None due to zero-distance guard
        let result = find_conflict_point(&degenerate, &normal, 30);
        // Either None or Some -- just must not panic or produce NaN
        if let Some((ta, tb)) = result {
            assert!(ta.is_finite());
            assert!(tb.is_finite());
        }
    }

    // ---- Close-junction merging tests ----

    /// Build two close junctions (4m apart, within MERGE_DISTANCE_M=5)
    /// connected by a short edge.
    ///
    /// ```text
    ///    A (0,0) --> J1 (100,0) --> J2 (104,0) --> B (220,0)
    ///                  ^                ^
    ///    C (100,100)---+   D (104,100)--+
    /// ```
    fn build_close_junction_pair() -> (RoadGraph, NodeIndex, NodeIndex) {
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let j1 = g.add_node(RoadNode { pos: [100.0, 0.0] });
        let j2 = g.add_node(RoadNode { pos: [104.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [220.0, 0.0] });
        let c = g.add_node(RoadNode { pos: [100.0, 100.0] });
        let d = g.add_node(RoadNode { pos: [104.0, 100.0] });

        // Edges making j1 and j2 junctions (in>1 or out>1)
        g.add_edge(a, j1, test_edge(100.0));
        g.add_edge(j1, j2, test_edge(4.0)); // short internal edge (<MERGE_DISTANCE_M)
        g.add_edge(j2, b, test_edge(116.0));
        g.add_edge(c, j1, test_edge(100.0));
        g.add_edge(d, j2, test_edge(100.0));
        // Outgoing from j1/j2 to make them true junctions
        g.add_edge(j1, c, test_edge(100.0));
        g.add_edge(j2, d, test_edge(100.0));

        (RoadGraph::new(g), j1, j2)
    }

    #[test]
    fn close_junctions_merged_into_cluster() {
        let (graph, j1, j2) = build_close_junction_pair();
        let junctions = precompute_all_junctions(&graph);

        // Both j1 and j2 should map to the same JunctionData
        let j1_data = junctions.get(&(j1.index() as u32));
        let j2_data = junctions.get(&(j2.index() as u32));
        assert!(j1_data.is_some(), "j1 should have junction data");
        assert!(j2_data.is_some(), "j2 should have junction data");

        // Same data (shared clone) — same number of turns
        let d1 = j1_data.unwrap();
        let d2 = j2_data.unwrap();
        assert_eq!(d1.turns.len(), d2.turns.len());
    }

    #[test]
    fn merged_cluster_has_internal_edges() {
        let (graph, j1, _j2) = build_close_junction_pair();
        let junctions = precompute_all_junctions(&graph);
        let data = junctions.get(&(j1.index() as u32)).unwrap();

        // The j1->j2 edge (20m, within MERGE_DISTANCE_M) should be internal
        assert!(
            !data.internal_edges.is_empty(),
            "merged cluster should have internal edges"
        );
    }

    #[test]
    fn merged_cluster_turns_use_peripheral_edges_only() {
        let (graph, j1, _j2) = build_close_junction_pair();
        let junctions = precompute_all_junctions(&graph);
        let data = junctions.get(&(j1.index() as u32)).unwrap();

        for turn in &data.turns {
            assert!(
                !data.internal_edges.contains(&turn.entry_edge),
                "turn entry_edge {} should not be internal",
                turn.entry_edge
            );
            assert!(
                !data.internal_edges.contains(&turn.exit_edge),
                "turn exit_edge {} should not be internal",
                turn.exit_edge
            );
        }
    }

    #[test]
    fn distant_junctions_not_merged() {
        // Two junctions 200m apart — should NOT merge
        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let j1 = g.add_node(RoadNode { pos: [100.0, 0.0] });
        let j2 = g.add_node(RoadNode { pos: [300.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [400.0, 0.0] });
        let c = g.add_node(RoadNode { pos: [100.0, 100.0] });
        let d = g.add_node(RoadNode { pos: [300.0, 100.0] });

        g.add_edge(a, j1, test_edge(100.0));
        g.add_edge(j1, j2, test_edge(200.0));
        g.add_edge(j2, b, test_edge(100.0));
        g.add_edge(c, j1, test_edge(100.0));
        g.add_edge(d, j2, test_edge(100.0));
        g.add_edge(j1, c, test_edge(100.0));
        g.add_edge(j2, d, test_edge(100.0));

        let graph = RoadGraph::new(g);
        let junctions = precompute_all_junctions(&graph);

        let d1 = junctions.get(&(j1.index() as u32)).unwrap();
        let d2 = junctions.get(&(j2.index() as u32)).unwrap();
        // Distant junctions should have separate data with no internal edges
        assert!(d1.internal_edges.is_empty());
        assert!(d2.internal_edges.is_empty());
    }

    #[test]
    fn union_find_basic() {
        let mut uf = UnionFind::new(5);
        uf.union(0, 1);
        uf.union(2, 3);
        assert_eq!(uf.find(0), uf.find(1));
        assert_ne!(uf.find(0), uf.find(2));
        uf.union(1, 3);
        assert_eq!(uf.find(0), uf.find(3));
    }
}
