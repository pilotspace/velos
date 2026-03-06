//! Tests for gridlock cycle detection.

use std::collections::HashMap;
use velos_vehicle::gridlock::{GridlockDetector, detect_cycles};

#[test]
fn three_node_cycle_detected() {
    // A->B->C->A forms a cycle
    let mut graph = HashMap::new();
    graph.insert(1, 2);
    graph.insert(2, 3);
    graph.insert(3, 1);

    let cycles = detect_cycles(&graph);
    assert_eq!(cycles.len(), 1, "should find exactly one cycle");
    assert_eq!(cycles[0].len(), 3, "cycle should have 3 nodes");
    // All three nodes should be in the cycle
    assert!(cycles[0].contains(&1));
    assert!(cycles[0].contains(&2));
    assert!(cycles[0].contains(&3));
}

#[test]
fn linear_chain_no_cycle() {
    // A->B->C with no cycle (C doesn't point back)
    let mut graph = HashMap::new();
    graph.insert(1, 2);
    graph.insert(2, 3);
    // 3 has no entry -- chain ends

    let cycles = detect_cycles(&graph);
    assert!(cycles.is_empty(), "linear chain should not produce cycles");
}

#[test]
fn empty_graph_no_cycles() {
    let graph: HashMap<u32, u32> = HashMap::new();
    let cycles = detect_cycles(&graph);
    assert!(cycles.is_empty(), "empty graph should have no cycles");
}

#[test]
fn multiple_independent_cycles() {
    // Cycle 1: 1->2->1
    // Cycle 2: 10->20->30->10
    let mut graph = HashMap::new();
    graph.insert(1, 2);
    graph.insert(2, 1);
    graph.insert(10, 20);
    graph.insert(20, 30);
    graph.insert(30, 10);

    let cycles = detect_cycles(&graph);
    assert_eq!(cycles.len(), 2, "should find two independent cycles");
}

#[test]
fn two_node_cycle() {
    // A->B->A (mutual blocking)
    let mut graph = HashMap::new();
    graph.insert(5, 6);
    graph.insert(6, 5);

    let cycles = detect_cycles(&graph);
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].len(), 2);
}

#[test]
fn chain_with_tail_before_cycle() {
    // 1->2->3->4->3 (tail: 1,2 | cycle: 3,4)
    let mut graph = HashMap::new();
    graph.insert(1, 2);
    graph.insert(2, 3);
    graph.insert(3, 4);
    graph.insert(4, 3);

    let cycles = detect_cycles(&graph);
    assert_eq!(cycles.len(), 1, "should find the cycle");
    // Cycle should be [3, 4] (the circular part)
    assert_eq!(cycles[0].len(), 2);
    assert!(cycles[0].contains(&3));
    assert!(cycles[0].contains(&4));
}

#[test]
fn detector_default_timeout() {
    let det = GridlockDetector::default();
    assert!((det.timeout_secs - 300.0).abs() < f64::EPSILON);
}

#[test]
fn detector_custom_timeout() {
    let det = GridlockDetector::new(600.0);
    assert!((det.timeout_secs - 600.0).abs() < f64::EPSILON);
}
