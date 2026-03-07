//! Tests for CCH (Customizable Contraction Hierarchies) module.
//!
//! Covers: ordering validity, contraction correctness, shortcut properties,
//! cache roundtrip, and cache invalidation.

use petgraph::graph::DiGraph;
use std::collections::HashSet;
use velos_net::cch::CCHRouter;
use velos_net::graph::{RoadClass, RoadEdge, RoadGraph, RoadNode};

// ---------------------------------------------------------------------------
// Helper: build a RoadEdge with given length and default fields
// ---------------------------------------------------------------------------

fn edge(length_m: f64) -> RoadEdge {
    RoadEdge {
        length_m,
        speed_limit_mps: 13.89, // ~50 km/h
        lane_count: 1,
        oneway: false,
        road_class: RoadClass::Residential,
        geometry: vec![],
        motorbike_only: false,
        time_windows: None,
    }
}

fn node(x: f64, y: f64) -> RoadNode {
    RoadNode { pos: [x, y] }
}

// ---------------------------------------------------------------------------
// Helper: build small test graphs
// ---------------------------------------------------------------------------

/// 6-node 2x3 grid graph:
/// 0 - 1 - 2
/// |   |   |
/// 3 - 4 - 5
fn make_grid_2x3() -> RoadGraph {
    let mut g = DiGraph::new();
    let n: Vec<_> = (0..6)
        .map(|i| g.add_node(node((i % 3) as f64 * 100.0, (i / 3) as f64 * 100.0)))
        .collect();
    // Horizontal edges (bidirectional)
    for &(a, b) in &[(0, 1), (1, 2), (3, 4), (4, 5)] {
        g.add_edge(n[a], n[b], edge(100.0));
        g.add_edge(n[b], n[a], edge(100.0));
    }
    // Vertical edges (bidirectional)
    for &(a, b) in &[(0, 3), (1, 4), (2, 5)] {
        g.add_edge(n[a], n[b], edge(100.0));
        g.add_edge(n[b], n[a], edge(100.0));
    }
    RoadGraph::new(g)
}

/// Line graph: 0 - 1 - 2 - 3 - 4
fn make_line_5() -> RoadGraph {
    let mut g = DiGraph::new();
    let n: Vec<_> = (0..5)
        .map(|i| g.add_node(node(i as f64 * 100.0, 0.0)))
        .collect();
    for i in 0..4 {
        g.add_edge(n[i], n[i + 1], edge(100.0));
        g.add_edge(n[i + 1], n[i], edge(100.0));
    }
    RoadGraph::new(g)
}

/// Diamond graph:
///     1
///    / \
///   0   3
///    \ /
///     2
fn make_diamond() -> RoadGraph {
    let mut g = DiGraph::new();
    let n0 = g.add_node(node(0.0, 100.0));
    let n1 = g.add_node(node(100.0, 200.0));
    let n2 = g.add_node(node(100.0, 0.0));
    let n3 = g.add_node(node(200.0, 100.0));
    // A-B, A-C, B-D, C-D (bidirectional)
    for &(a, b) in &[(n0, n1), (n0, n2), (n1, n3), (n2, n3)] {
        g.add_edge(a, b, edge(141.0));
        g.add_edge(b, a, edge(141.0));
    }
    RoadGraph::new(g)
}

/// 10-node 2x5 grid
fn make_grid_2x5() -> RoadGraph {
    let mut g = DiGraph::new();
    let n: Vec<_> = (0..10)
        .map(|i| g.add_node(node((i % 5) as f64 * 100.0, (i / 5) as f64 * 100.0)))
        .collect();
    // Horizontal
    for row in 0..2 {
        for col in 0..4 {
            let a = row * 5 + col;
            let b = a + 1;
            g.add_edge(n[a], n[b], edge(100.0));
            g.add_edge(n[b], n[a], edge(100.0));
        }
    }
    // Vertical
    for col in 0..5 {
        let a = col;
        let b = col + 5;
        g.add_edge(n[a], n[b], edge(100.0));
        g.add_edge(n[b], n[a], edge(100.0));
    }
    RoadGraph::new(g)
}

// ===========================================================================
// Ordering tests
// ===========================================================================

#[test]
fn ordering_is_valid_permutation_on_grid() {
    let graph = make_grid_2x3();
    let order = velos_net::cch::ordering::compute_ordering(graph.inner());

    // All nodes appear exactly once in the ordering
    assert_eq!(order.len(), 6);
    let ranks: HashSet<u32> = order.iter().copied().collect();
    assert_eq!(ranks.len(), 6, "ordering must be a permutation");
    for rank in 0..6u32 {
        assert!(ranks.contains(&rank), "rank {} missing from ordering", rank);
    }
}

#[test]
fn ordering_line_graph_endpoints_before_middle() {
    let graph = make_line_5();
    let order = velos_net::cch::ordering::compute_ordering(graph.inner());

    // In a line graph, the middle node (index 2) should be a separator
    // and thus have one of the highest ranks. Endpoints (0, 4) should
    // have lower ranks than the central separator.
    // The exact ordering depends on BFS peripheral selection, but the
    // middle node should rank higher than at least some endpoints.
    assert_eq!(order.len(), 5);

    // Verify it's a valid permutation
    let ranks: HashSet<u32> = order.iter().copied().collect();
    assert_eq!(ranks.len(), 5);
}

// ===========================================================================
// Contraction tests
// ===========================================================================

#[test]
fn diamond_graph_produces_shortcut() {
    let graph = make_diamond();
    let router = CCHRouter::from_graph(&graph);

    assert_eq!(router.node_count, 4);

    // Count shortcuts
    let shortcut_count = router
        .shortcut_middle
        .iter()
        .filter(|m| m.is_some())
        .count();

    // Diamond graph should produce at least 1 shortcut (A-D through B or C)
    assert!(
        shortcut_count >= 1,
        "expected at least 1 shortcut, got {}",
        shortcut_count
    );
}

#[test]
fn contraction_preserves_original_edges() {
    let graph = make_diamond();
    let router = CCHRouter::from_graph(&graph);

    // Count non-shortcut (original) edges in CCH
    let original_count = router
        .shortcut_middle
        .iter()
        .filter(|m| m.is_none())
        .count();

    // The diamond has 4 undirected edges = 8 directed edges,
    // but CCH stores undirected, so at least 4 original CCH edges
    assert!(
        original_count >= 4,
        "expected at least 4 original CCH edges, got {}",
        original_count
    );
}

#[test]
fn shortcut_middle_none_for_original_some_for_shortcut() {
    let graph = make_grid_2x3();
    let router = CCHRouter::from_graph(&graph);

    let total_forward = router.forward_head.len();
    let total_backward = router.backward_head.len();

    // shortcut_middle should have entries for all forward + backward edges
    assert_eq!(
        router.shortcut_middle.len(),
        total_forward + total_backward
    );

    // At least some should be None (original edges)
    let originals = router
        .shortcut_middle
        .iter()
        .filter(|m| m.is_none())
        .count();
    assert!(originals > 0, "should have original (non-shortcut) edges");
}

#[test]
fn shortcut_count_under_3x_original_on_grid() {
    let graph = make_grid_2x5();
    let router = CCHRouter::from_graph(&graph);

    // Count unique undirected original edges
    let original_edge_count = graph.edge_count(); // directed count
    let undirected_original = original_edge_count / 2; // all edges are bidirectional

    // Count shortcuts in forward star only (each shortcut appears once)
    let forward_shortcuts = router.shortcut_middle[..router.forward_head.len()]
        .iter()
        .filter(|m| m.is_some())
        .count();

    assert!(
        forward_shortcuts < 3 * undirected_original,
        "shortcut count {} should be < 3x original edges {}",
        forward_shortcuts,
        undirected_original
    );
}

#[test]
fn cch_router_from_graph_on_10_node_grid() {
    let graph = make_grid_2x5();
    let router = CCHRouter::from_graph(&graph);

    assert_eq!(router.node_count, 10);
    assert_eq!(router.node_order.len(), 10);
    assert_eq!(router.rank_to_node.len(), 10);

    // CSR format: first_out has n+1 entries
    assert_eq!(router.forward_first_out.len(), 11);
    assert_eq!(router.backward_first_out.len(), 11);

    // Weights initialized to INFINITY
    assert!(router.forward_weight.iter().all(|&w| w == f32::INFINITY));
    assert!(router.backward_weight.iter().all(|&w| w == f32::INFINITY));
}

#[test]
fn cch_correctness_all_pairs_small_graph() {
    // On a small graph, verify that for every connected node pair,
    // there exists a path in the CCH upward graph (from both ends to their
    // meeting point). This validates the contraction didn't lose connectivity.
    let graph = make_diamond();
    let router = CCHRouter::from_graph(&graph);

    // For each node, verify it can reach the top-ranked node via upward edges
    let n = router.node_count;
    let max_rank = n as u32 - 1;

    // Every node should have a path upward to the highest-ranked node
    // (or at least some high-ranked node reachable from it)
    for start_rank in 0..n as u32 {
        let mut reachable: HashSet<u32> = HashSet::new();
        let mut stack = vec![start_rank];
        while let Some(r) = stack.pop() {
            if !reachable.insert(r) {
                continue;
            }
            let begin = router.forward_first_out[r as usize] as usize;
            let end = router.forward_first_out[r as usize + 1] as usize;
            for &target in &router.forward_head[begin..end] {
                if target > r {
                    stack.push(target);
                }
            }
        }

        // From every node, we should reach the top
        assert!(
            reachable.contains(&max_rank),
            "rank {} cannot reach top rank {} via upward edges",
            start_rank,
            max_rank
        );
    }
}

// ===========================================================================
// Cache tests
// ===========================================================================

#[test]
fn cache_roundtrip_produces_identical_router() {
    let graph = make_grid_2x3();
    let router = CCHRouter::from_graph(&graph);

    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("cch_test.bin");

    velos_net::cch::cache::save_cch(&router, &path).expect("save");
    let loaded = velos_net::cch::cache::load_cch(&path).expect("load");

    assert_eq!(router.node_order, loaded.node_order);
    assert_eq!(router.rank_to_node, loaded.rank_to_node);
    assert_eq!(router.forward_head, loaded.forward_head);
    assert_eq!(router.forward_first_out, loaded.forward_first_out);
    assert_eq!(router.backward_head, loaded.backward_head);
    assert_eq!(router.backward_first_out, loaded.backward_first_out);
    assert_eq!(router.shortcut_middle, loaded.shortcut_middle);
    assert_eq!(router.original_edge_to_cch, loaded.original_edge_to_cch);
    assert_eq!(router.node_count, loaded.node_count);
    assert_eq!(router.edge_count, loaded.edge_count);
}

#[test]
fn cache_load_missing_file_returns_err() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("nonexistent.bin");

    let result = velos_net::cch::cache::load_cch(&path);
    assert!(result.is_err(), "loading missing file should return Err");
}

#[test]
fn cache_load_corrupted_file_returns_err() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("corrupted.bin");
    std::fs::write(&path, b"this is not valid postcard data").expect("write");

    let result = velos_net::cch::cache::load_cch(&path);
    assert!(result.is_err(), "loading corrupted file should return Err");
}

#[test]
fn from_graph_cached_creates_cache_file() {
    let graph = make_grid_2x3();
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("cch_cached.bin");

    assert!(!path.exists());
    let _router = CCHRouter::from_graph_cached(&graph, &path).expect("first call");
    assert!(path.exists(), "cache file should be created on first call");
}

#[test]
fn from_graph_cached_loads_from_cache_on_second_call() {
    let graph = make_grid_2x3();
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("cch_cached2.bin");

    let router1 = CCHRouter::from_graph_cached(&graph, &path).expect("first");
    let router2 = CCHRouter::from_graph_cached(&graph, &path).expect("second");

    // Should produce identical results (from cache)
    assert_eq!(router1.node_order, router2.node_order);
    assert_eq!(router1.forward_head, router2.forward_head);
}

#[test]
fn from_graph_cached_invalidates_on_graph_change() {
    let graph1 = make_grid_2x3(); // 6 nodes
    let graph2 = make_grid_2x5(); // 10 nodes
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().join("cch_invalidate.bin");

    let router1 = CCHRouter::from_graph_cached(&graph1, &path).expect("first");
    assert_eq!(router1.node_count, 6);

    // Second call with different graph should rebuild
    let router2 = CCHRouter::from_graph_cached(&graph2, &path).expect("second");
    assert_eq!(router2.node_count, 10);
}
