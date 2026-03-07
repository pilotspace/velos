//! Tests for multi-GPU graph partitioning and boundary agent protocol.
//!
//! These tests are CPU-only: they verify partition logic and boundary
//! transfer protocol without requiring a GPU adapter.

use std::collections::HashMap;

use petgraph::graph::DiGraph;
use velos_core::components::GpuAgentState;
use velos_gpu::multi_gpu::{BoundaryAgent, GpuPartition, MultiGpuScheduler};
use velos_gpu::partition::{partition_edges, partition_network, PartitionAssignment};
use velos_net::{RoadEdge, RoadGraph, RoadNode};

/// Build a synthetic grid road network with `n` x `n` intersections.
/// Returns a RoadGraph with approximately 2*n*(n-1) directed edges.
fn make_grid_graph(n: usize) -> RoadGraph {
    let mut g = DiGraph::new();

    // Create n*n nodes in a grid pattern.
    let mut node_indices = Vec::new();
    for row in 0..n {
        for col in 0..n {
            let idx = g.add_node(RoadNode {
                pos: [col as f64 * 100.0, row as f64 * 100.0],
            });
            node_indices.push(idx);
        }
    }

    let edge_data = || RoadEdge {
        length_m: 100.0,
        speed_limit_mps: 13.89,
        lane_count: 2,
        oneway: false,
        road_class: velos_net::graph::RoadClass::Secondary,
        geometry: vec![],
        motorbike_only: false,
        time_windows: None,
    };

    // Add horizontal edges (both directions).
    for row in 0..n {
        for col in 0..(n - 1) {
            let a = node_indices[row * n + col];
            let b = node_indices[row * n + col + 1];
            g.add_edge(a, b, edge_data());
            g.add_edge(b, a, edge_data());
        }
    }

    // Add vertical edges (both directions).
    for row in 0..(n - 1) {
        for col in 0..n {
            let a = node_indices[row * n + col];
            let b = node_indices[(row + 1) * n + col];
            g.add_edge(a, b, edge_data());
            g.add_edge(b, a, edge_data());
        }
    }

    RoadGraph::new(g)
}

fn make_agent(edge_id: u32, lane: u32, position: i32, speed: i32) -> GpuAgentState {
    GpuAgentState {
        edge_id,
        lane_idx: lane,
        position,
        lateral: 0,
        speed,
        acceleration: 0,
        cf_model: 0,
        rng_state: 42,
    }
}

// ---------------------------------------------------------------------------
// Partitioning tests
// ---------------------------------------------------------------------------

#[test]
fn partition_k2_balanced() {
    let graph = make_grid_graph(8); // 8x8 grid -> 64 nodes, ~224 edges
    assert!(graph.edge_count() >= 100, "need >= 100 edges for test");

    let assignment = partition_network(&graph, 2);

    assert_eq!(assignment.partition_count, 2);

    // Check balance: each partition should have ~50% of edges (+/- 10%).
    let total = graph.edge_count();
    let p0_count = partition_edges(&assignment, 0).len();
    let p1_count = partition_edges(&assignment, 1).len();
    assert_eq!(p0_count + p1_count, total);

    let ratio = p0_count as f64 / total as f64;
    assert!(
        (0.4..=0.6).contains(&ratio),
        "partition balance out of range: {ratio:.2} (p0={p0_count}, p1={p1_count}, total={total})"
    );
}

#[test]
fn partition_k4_produces_4_partitions() {
    let graph = make_grid_graph(8);
    let assignment = partition_network(&graph, 4);

    assert_eq!(assignment.partition_count, 4);

    // All edges assigned.
    let mut total = 0;
    for pid in 0..4 {
        total += partition_edges(&assignment, pid).len();
    }
    assert_eq!(total, graph.edge_count());
}

#[test]
fn partition_is_deterministic() {
    let graph = make_grid_graph(6);
    let a1 = partition_network(&graph, 3);
    let a2 = partition_network(&graph, 3);

    assert_eq!(a1.edge_to_partition, a2.edge_to_partition);
    assert_eq!(a1.boundary_edges, a2.boundary_edges);
}

#[test]
fn partition_identifies_boundary_edges() {
    let graph = make_grid_graph(6);
    let assignment = partition_network(&graph, 2);

    // There must be boundary edges in a connected graph with k > 1.
    assert!(
        !assignment.boundary_edges.is_empty(),
        "expected boundary edges between 2 partitions"
    );

    // Each boundary edge has src_partition != dst_partition.
    for &(edge_id, src_p, dst_p) in &assignment.boundary_edges {
        assert_ne!(src_p, dst_p, "boundary edge {edge_id} has same partition on both sides");
        assert!(assignment.edge_to_partition.contains_key(&edge_id));
    }
}

// ---------------------------------------------------------------------------
// Boundary agent protocol tests
// ---------------------------------------------------------------------------

#[test]
fn boundary_agent_preserves_state() {
    let original = make_agent(10, 1, 65536, 32768);
    let boundary = BoundaryAgent {
        state: original,
        dest_partition: 1,
        dest_edge_id: 20,
    };

    // All fields preserved.
    assert_eq!(boundary.state.edge_id, 10);
    assert_eq!(boundary.state.lane_idx, 1);
    assert_eq!(boundary.state.position, 65536);
    assert_eq!(boundary.state.speed, 32768);
    assert_eq!(boundary.state.cf_model, 0);
    assert_eq!(boundary.state.rng_state, 42);
    assert_eq!(boundary.dest_partition, 1);
    assert_eq!(boundary.dest_edge_id, 20);
}

#[test]
fn outbox_inbox_transfer() {
    // Create 2 partitions manually with agents near a boundary edge.
    let graph = make_grid_graph(4);
    let assignment = partition_network(&graph, 2);

    // Find a boundary edge.
    let (boundary_edge, src_partition, dst_partition) = assignment.boundary_edges[0];

    // Create a partition with an agent on the boundary edge source side.
    let mut partition_a = GpuPartition::new(
        src_partition,
        partition_edges(&assignment, src_partition),
    );
    let agent = make_agent(boundary_edge, 0, 65536 * 99, 32768); // near end of edge
    partition_a.agent_states.push(agent);

    // Collect outbox: agent on boundary edge should produce a BoundaryAgent.
    partition_a.collect_outbox_agents(&assignment.boundary_map());

    assert_eq!(
        partition_a.outbox.len(),
        1,
        "agent on boundary edge should be in outbox"
    );
    assert_eq!(partition_a.outbox[0].dest_partition, dst_partition);

    // Route outbox to destination partition inbox.
    let mut partition_b = GpuPartition::new(
        dst_partition,
        partition_edges(&assignment, dst_partition),
    );
    let outbox_agent = partition_a.outbox.drain(..).next().unwrap();
    partition_b.inbox.push(outbox_agent);

    // Spawn inbox agents into partition B.
    partition_b.spawn_inbox_agents();

    assert_eq!(partition_b.inbox.len(), 0, "inbox should be drained");
    assert_eq!(
        partition_b.agent_states.len(),
        1,
        "agent should appear in partition B"
    );

    // Verify state preservation.
    let transferred = &partition_b.agent_states[0];
    assert_eq!(transferred.speed, agent.speed);
    assert_eq!(transferred.cf_model, agent.cf_model);
    assert_eq!(transferred.rng_state, agent.rng_state);
}

#[test]
fn multi_gpu_scheduler_step_with_2_partitions() {
    let graph = make_grid_graph(4);
    let assignment = partition_network(&graph, 2);

    let mut scheduler = MultiGpuScheduler::new(assignment);

    // Distribute 20 agents across partition edges.
    let mut agents = Vec::new();
    for pid in 0..2u32 {
        let edges = scheduler.partition_edge_ids(pid);
        for (i, &edge_id) in edges.iter().enumerate().take(10) {
            agents.push(make_agent(edge_id, 0, (i as i32 + 1) * 65536, 32768));
        }
    }
    scheduler.distribute_agents(&agents);

    let total_before = scheduler.agent_count();
    assert_eq!(total_before, 20);

    // Run a step (CPU-only protocol, no GPU dispatch).
    scheduler.step_cpu();

    // Agent count should be preserved: agents in partitions + agents in-transit (inbox).
    // Agents on boundary edges are outboxed this step and routed to inboxes.
    // They will be spawned at the start of the next step.
    let agents_in_partitions: usize = scheduler
        .partitions()
        .iter()
        .map(|p| p.agent_states.len())
        .sum();
    let agents_in_transit: usize = scheduler
        .partitions()
        .iter()
        .map(|p| p.inbox.len())
        .sum();
    let total_after = agents_in_partitions + agents_in_transit;
    assert_eq!(
        total_after, total_before,
        "agents lost during multi-GPU step (in_partitions={agents_in_partitions}, in_transit={agents_in_transit})"
    );

    // Run a second step to verify inbox agents get spawned.
    scheduler.step_cpu();
    let total_after_step2: usize = scheduler
        .partitions()
        .iter()
        .map(|p| p.agent_states.len() + p.inbox.len())
        .sum();
    assert_eq!(
        total_after_step2, total_before,
        "agents lost after second multi-GPU step"
    );
}
