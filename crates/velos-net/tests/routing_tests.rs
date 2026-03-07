//! Tests for A* routing on the road graph.

use petgraph::graph::DiGraph;
use velos_net::{find_route, NetError, RoadClass, RoadEdge, RoadGraph, RoadNode};

/// Build a diamond-shaped test graph:
///
/// ```text
///        1 (100, 100)
///       / \
///      /   \
///     0     3
///      \   /
///       \ /
///        2 (100, -100)
/// ```
///
/// Node 0 at (0,0), Node 1 at (100,100), Node 2 at (100,-100), Node 3 at (200,0).
/// Top path: 0->1->3, length = ~141 + ~141 = ~282m
/// Bottom path: 0->2->3, length = ~141 + ~141 = ~282m (same distance, but different speed)
fn build_diamond_graph() -> RoadGraph {
    let mut g = DiGraph::<RoadNode, RoadEdge>::new();
    let n0 = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let n1 = g.add_node(RoadNode { pos: [100.0, 100.0] });
    let n2 = g.add_node(RoadNode { pos: [100.0, -100.0] });
    let n3 = g.add_node(RoadNode { pos: [200.0, 0.0] });

    let make_edge = |a: [f64; 2], b: [f64; 2], speed: f64| {
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let length_m = (dx * dx + dy * dy).sqrt();
        RoadEdge {
            length_m,
            speed_limit_mps: speed,
            lane_count: 2,
            oneway: true,
            road_class: RoadClass::Primary,
            geometry: vec![a, b],
            motorbike_only: false,
            time_windows: None,
        }
    };

    // Top path: faster (50 km/h = 13.89 m/s)
    g.add_edge(n0, n1, make_edge([0.0, 0.0], [100.0, 100.0], 50.0 / 3.6));
    g.add_edge(n1, n3, make_edge([100.0, 100.0], [200.0, 0.0], 50.0 / 3.6));

    // Bottom path: slower (20 km/h = 5.56 m/s)
    g.add_edge(n0, n2, make_edge([0.0, 0.0], [100.0, -100.0], 20.0 / 3.6));
    g.add_edge(n2, n3, make_edge([100.0, -100.0], [200.0, 0.0], 20.0 / 3.6));

    RoadGraph::new(g)
}

#[test]
fn finds_shortest_path_on_diamond() {
    let graph = build_diamond_graph();
    let n0 = petgraph::graph::NodeIndex::new(0);
    let n3 = petgraph::graph::NodeIndex::new(3);

    let (path, cost) = find_route(&graph, n0, n3).expect("route should exist");

    // Should take the top (faster) path: 0 -> 1 -> 3
    assert_eq!(path.len(), 3);
    assert_eq!(path[0].index(), 0);
    assert_eq!(path[1].index(), 1);
    assert_eq!(path[2].index(), 3);

    // Cost = 2 * (141.42 / 13.89) ~= 20.36s
    assert!(cost > 15.0 && cost < 25.0, "cost ~20.36s, got {cost}");
}

#[test]
fn disconnected_nodes_return_no_path() {
    let mut g = DiGraph::<RoadNode, RoadEdge>::new();
    let n0 = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let n1 = g.add_node(RoadNode { pos: [100.0, 0.0] });
    // No edges -- nodes are disconnected.
    let graph = RoadGraph::new(g);

    let result = find_route(&graph, n0, n1);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err(), NetError::NoPathFound { .. }));
}

#[test]
fn path_cost_is_travel_time() {
    // Simple two-node graph: 100m at 10 m/s = 10s travel time
    let mut g = DiGraph::<RoadNode, RoadEdge>::new();
    let n0 = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let n1 = g.add_node(RoadNode { pos: [100.0, 0.0] });
    g.add_edge(
        n0,
        n1,
        RoadEdge {
            length_m: 100.0,
            speed_limit_mps: 10.0,
            lane_count: 1,
            oneway: true,
            road_class: RoadClass::Residential,
            geometry: vec![[0.0, 0.0], [100.0, 0.0]],
            motorbike_only: false,
            time_windows: None,
        },
    );
    let graph = RoadGraph::new(g);

    let (path, cost) = find_route(&graph, n0, n1).expect("route should exist");
    assert_eq!(path.len(), 2);
    assert!((cost - 10.0).abs() < 0.01, "cost should be 10.0s, got {cost}");
}

#[test]
fn same_node_returns_zero_cost_path() {
    let mut g = DiGraph::<RoadNode, RoadEdge>::new();
    let n0 = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let graph = RoadGraph::new(g);

    let (path, cost) = find_route(&graph, n0, n0).expect("route to self should work");
    assert_eq!(path.len(), 1);
    assert!((cost).abs() < f64::EPSILON);
}
