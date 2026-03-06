//! A* pathfinding on the road graph.
//!
//! Delegates to `petgraph::algo::astar` with travel-time edge costs
//! and an admissible Euclidean/max-speed heuristic.

use petgraph::algo::astar;
use petgraph::graph::NodeIndex;

use crate::error::NetError;
use crate::graph::RoadGraph;

/// Find the shortest (minimum travel-time) route between two nodes.
///
/// Returns the path as a sequence of `NodeIndex` values and the total
/// travel time in seconds.
///
/// Edge cost = `length_m / speed_limit_mps` (travel time in seconds).
/// Heuristic = Euclidean distance / max_speed (admissible).
pub fn find_route(
    graph: &RoadGraph,
    from: NodeIndex,
    to: NodeIndex,
) -> Result<(Vec<NodeIndex>, f64), NetError> {
    let g = graph.inner();
    let to_pos = graph.node_position(to);

    // Compute max speed across all edges for admissible heuristic.
    let max_speed = g
        .edge_weights()
        .map(|e| e.speed_limit_mps)
        .fold(0.0_f64, f64::max)
        .max(1.0); // guard against empty graph

    let result = astar(
        g,
        from,
        |node| node == to,
        |edge| {
            let w = edge.weight();
            w.length_m / w.speed_limit_mps
        },
        |node| {
            let pos = g[node].pos;
            let dx = pos[0] - to_pos[0];
            let dy = pos[1] - to_pos[1];
            (dx * dx + dy * dy).sqrt() / max_speed
        },
    );

    match result {
        Some((cost, path)) => Ok((path, cost)),
        None => Err(NetError::NoPathFound {
            from: from.index() as u32,
            to: to.index() as u32,
        }),
    }
}
