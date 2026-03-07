//! Tests for the network cleaning pipeline.

use petgraph::graph::DiGraph;
use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};
use velos_net::cleaning::{clean_network, CleaningConfig};

/// Helper: create a simple edge with default values.
fn make_edge(length: f64, road_class: RoadClass, lanes: u8) -> RoadEdge {
    RoadEdge {
        length_m: length,
        speed_limit_mps: 13.9, // ~50 km/h
        lane_count: lanes,
        oneway: false,
        road_class,
        geometry: vec![],
        motorbike_only: false,
        time_windows: None,
    }
}

/// Helper: build a simple connected graph with N nodes in a chain.
fn chain_graph(n: usize, edge_length: f64) -> RoadGraph {
    let mut g = DiGraph::new();
    let nodes: Vec<_> = (0..n)
        .map(|i| {
            g.add_node(RoadNode {
                pos: [i as f64 * edge_length, 0.0],
            })
        })
        .collect();
    for i in 0..n - 1 {
        g.add_edge(
            nodes[i],
            nodes[i + 1],
            make_edge(edge_length, RoadClass::Secondary, 2),
        );
        g.add_edge(
            nodes[i + 1],
            nodes[i],
            make_edge(edge_length, RoadClass::Secondary, 2),
        );
    }
    RoadGraph::new(g)
}

/// Helper: build a graph with a main component and a small disconnected island.
fn graph_with_island() -> RoadGraph {
    let mut g = DiGraph::new();

    // Main component: 5 nodes in a cycle (strongly connected).
    let main_nodes: Vec<_> = (0..5)
        .map(|i| {
            let angle = std::f64::consts::TAU * i as f64 / 5.0;
            g.add_node(RoadNode {
                pos: [100.0 * angle.cos(), 100.0 * angle.sin()],
            })
        })
        .collect();
    for i in 0..5 {
        let j = (i + 1) % 5;
        g.add_edge(
            main_nodes[i],
            main_nodes[j],
            make_edge(50.0, RoadClass::Primary, 3),
        );
        g.add_edge(
            main_nodes[j],
            main_nodes[i],
            make_edge(50.0, RoadClass::Primary, 3),
        );
    }

    // Island component: 2 nodes, 2 edges (below min_edges threshold).
    let island_a = g.add_node(RoadNode {
        pos: [500.0, 500.0],
    });
    let island_b = g.add_node(RoadNode {
        pos: [510.0, 510.0],
    });
    g.add_edge(
        island_a,
        island_b,
        make_edge(14.0, RoadClass::Residential, 1),
    );
    g.add_edge(
        island_b,
        island_a,
        make_edge(14.0, RoadClass::Residential, 1),
    );

    RoadGraph::new(g)
}

#[test]
fn remove_small_components_keeps_largest() {
    let mut graph = graph_with_island();
    assert_eq!(graph.node_count(), 7); // 5 main + 2 island

    let config = CleaningConfig::default();
    let report = clean_network(&mut graph, &config);

    // Island (2 edges) is below min_edges=10, should be removed.
    assert_eq!(graph.node_count(), 5);
    assert!(report.components_removed > 0);
}

#[test]
fn merge_short_edges_removes_tiny_segments() {
    let mut g = DiGraph::new();

    // Create A --3m-- B --100m-- C (strongly connected cycle)
    let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let b = g.add_node(RoadNode { pos: [3.0, 0.0] });
    let c = g.add_node(RoadNode { pos: [103.0, 0.0] });

    // Forward
    g.add_edge(a, b, make_edge(3.0, RoadClass::Secondary, 2));
    g.add_edge(b, c, make_edge(100.0, RoadClass::Secondary, 2));
    g.add_edge(c, a, make_edge(103.0, RoadClass::Secondary, 2));
    // Reverse
    g.add_edge(b, a, make_edge(3.0, RoadClass::Secondary, 2));
    g.add_edge(c, b, make_edge(100.0, RoadClass::Secondary, 2));
    g.add_edge(a, c, make_edge(103.0, RoadClass::Secondary, 2));

    let mut graph = RoadGraph::new(g);
    let config = CleaningConfig::default();
    let report = clean_network(&mut graph, &config);

    // The 3m edge should be merged -- node B collapses into A or C.
    assert!(report.edges_merged > 0);
    // After merging, node count should decrease.
    assert!(graph.node_count() < 3);
}

#[test]
fn infer_lane_counts_from_road_class() {
    let mut g = DiGraph::new();
    let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
    let b = g.add_node(RoadNode { pos: [100.0, 0.0] });

    // Primary with 0 lanes -> should infer 3
    g.add_edge(a, b, make_edge(100.0, RoadClass::Primary, 0));
    g.add_edge(b, a, make_edge(100.0, RoadClass::Primary, 0));

    // Secondary with 0 lanes -> should infer 2
    let c = g.add_node(RoadNode {
        pos: [200.0, 0.0],
    });
    g.add_edge(b, c, make_edge(100.0, RoadClass::Secondary, 0));
    g.add_edge(c, b, make_edge(100.0, RoadClass::Secondary, 0));

    // Tertiary with 0 lanes -> should infer 2
    let d = g.add_node(RoadNode {
        pos: [300.0, 0.0],
    });
    g.add_edge(c, d, make_edge(100.0, RoadClass::Tertiary, 0));
    g.add_edge(d, c, make_edge(100.0, RoadClass::Tertiary, 0));

    // Residential with 0 lanes -> should infer 1
    let e = g.add_node(RoadNode {
        pos: [400.0, 0.0],
    });
    g.add_edge(d, e, make_edge(100.0, RoadClass::Residential, 0));
    g.add_edge(e, d, make_edge(100.0, RoadClass::Residential, 0));

    // Close the cycle for strong connectivity
    g.add_edge(e, a, make_edge(400.0, RoadClass::Primary, 3));
    g.add_edge(a, e, make_edge(400.0, RoadClass::Primary, 3));

    let mut graph = RoadGraph::new(g);
    let config = CleaningConfig::default();
    let report = clean_network(&mut graph, &config);

    assert!(report.lanes_inferred > 0);

    // Verify inferred lanes on remaining edges.
    for edge in graph.inner().edge_weights() {
        if edge.lane_count == 0 {
            panic!("Found edge with 0 lanes after cleaning");
        }
    }
}

#[test]
fn binary_serialization_roundtrip() {
    let graph = chain_graph(10, 50.0);
    let dir = std::env::temp_dir().join("velos_test_binary");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("test_graph.bin");

    graph.serialize_binary(&path).unwrap();
    let loaded = RoadGraph::deserialize_binary(&path).unwrap();

    assert_eq!(graph.node_count(), loaded.node_count());
    assert_eq!(graph.edge_count(), loaded.edge_count());

    // Clean up
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cleaning_report_tracks_all_operations() {
    let mut graph = graph_with_island();
    let config = CleaningConfig::default();
    let report = clean_network(&mut graph, &config);

    // Report should be non-default (at least some operations performed).
    assert!(
        report.components_removed > 0
            || report.edges_merged > 0
            || report.lanes_inferred > 0
            || report.motorbike_only_tagged > 0
    );
}
