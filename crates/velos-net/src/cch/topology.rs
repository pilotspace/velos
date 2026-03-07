//! Node contraction and shortcut graph construction for CCH.
//!
//! Processes nodes in rank order (lowest first), adding shortcuts between
//! higher-ranked neighbors. Produces CSR-format forward and backward stars.

use petgraph::graph::DiGraph;
use std::collections::{HashMap, HashSet};

use crate::cch::CCHRouter;
use crate::graph::{RoadEdge, RoadNode};

/// Contract the graph using the given node ordering to produce a CCH topology.
///
/// Processes nodes from lowest rank (contracted first) to highest rank.
/// For each contracted node v, checks all pairs of higher-ranked neighbors
/// (u, w) and adds a shortcut edge if needed.
///
/// Returns a `CCHRouter` with CSR-format forward/backward stars indexed by rank.
pub fn contract_graph(
    graph: &DiGraph<RoadNode, RoadEdge>,
    order: &[u32],
    node_count: usize,
    edge_count: usize,
) -> CCHRouter {
    // Build rank_to_node inverse mapping
    let mut rank_to_node = vec![0u32; node_count];
    for (node, &rank) in order.iter().enumerate() {
        if (rank as usize) < node_count {
            rank_to_node[rank as usize] = node as u32;
        }
    }

    // Build undirected adjacency for contraction
    // Each entry: (neighbor_node, is_shortcut, shortcut_middle)
    let mut adj: Vec<HashSet<usize>> = vec![HashSet::new(); node_count];
    for edge in graph.edge_indices() {
        let (s, t) = graph.edge_endpoints(edge).unwrap();
        adj[s.index()].insert(t.index());
        adj[t.index()].insert(s.index());
    }

    // Collect all CCH edges: (from_rank, to_rank, shortcut_middle)
    // where from_rank < to_rank (upward edges)
    let mut cch_edges: Vec<(u32, u32, Option<u32>)> = Vec::new();

    // Track original edge -> CCH edge mapping
    let mut original_edge_to_cch: Vec<usize> = vec![0; edge_count];

    // First, add all original edges as CCH edges (upward direction)
    let mut original_edge_set: HashMap<(u32, u32), usize> = HashMap::new();
    for edge_idx in graph.edge_indices() {
        let (s, t) = graph.edge_endpoints(edge_idx).unwrap();
        let s_rank = order[s.index()];
        let t_rank = order[t.index()];

        // Store edge going upward (lower rank -> higher rank)
        let (low_rank, high_rank) = if s_rank < t_rank {
            (s_rank, t_rank)
        } else {
            (t_rank, s_rank)
        };

        let key = (low_rank, high_rank);
        if let Some(&existing_idx) = original_edge_set.get(&key) {
            original_edge_to_cch[edge_idx.index()] = existing_idx;
        } else {
            let idx = cch_edges.len();
            cch_edges.push((low_rank, high_rank, None));
            original_edge_set.insert(key, idx);
            original_edge_to_cch[edge_idx.index()] = idx;
        }
    }

    // Contract nodes in rank order (lowest first)
    // Track existing edges to avoid duplicates
    let mut edge_exists: HashSet<(u32, u32)> = original_edge_set.keys().copied().collect();

    for rank in 0..node_count as u32 {
        let node = rank_to_node[rank as usize] as usize;

        // Find all neighbors with higher rank
        let higher_neighbors: Vec<usize> = adj[node]
            .iter()
            .filter(|&&nb| order[nb] > rank)
            .copied()
            .collect();

        // For each pair of higher-ranked neighbors, add shortcut
        for i in 0..higher_neighbors.len() {
            for j in (i + 1)..higher_neighbors.len() {
                let u = higher_neighbors[i];
                let w = higher_neighbors[j];
                let u_rank = order[u];
                let w_rank = order[w];

                let (low, high) = if u_rank < w_rank {
                    (u_rank, w_rank)
                } else {
                    (w_rank, u_rank)
                };

                if !edge_exists.contains(&(low, high)) {
                    edge_exists.insert((low, high));
                    cch_edges.push((low, high, Some(rank)));
                    // Also update adjacency so future contractions see shortcuts
                    adj[u].insert(w);
                    adj[w].insert(u);
                }
            }
        }
    }

    // Build CSR format indexed by rank
    // Forward star: edges going up from each rank
    // Backward star: edges coming down to each rank (stored as "I can reach rank X going down")
    let mut forward_adj: Vec<Vec<(u32, Option<u32>)>> = vec![vec![]; node_count];
    let mut backward_adj: Vec<Vec<(u32, Option<u32>)>> = vec![vec![]; node_count];

    for &(low_rank, high_rank, middle) in &cch_edges {
        forward_adj[low_rank as usize].push((high_rank, middle));
        backward_adj[high_rank as usize].push((low_rank, middle));
    }

    // Sort adjacency lists for deterministic CSR
    for list in forward_adj.iter_mut() {
        list.sort_by_key(|&(target, _)| target);
    }
    for list in backward_adj.iter_mut() {
        list.sort_by_key(|&(source, _)| source);
    }

    // Build forward CSR
    let mut forward_head = Vec::new();
    let mut forward_first_out = Vec::with_capacity(node_count + 1);
    let mut forward_shortcut_middle = Vec::new();

    for adj_list in forward_adj.iter() {
        forward_first_out.push(forward_head.len() as u32);
        for &(target, middle) in adj_list {
            forward_head.push(target);
            forward_shortcut_middle.push(middle);
        }
    }
    forward_first_out.push(forward_head.len() as u32);

    // Build backward CSR
    let mut backward_head = Vec::new();
    let mut backward_first_out = Vec::with_capacity(node_count + 1);
    let mut backward_shortcut_middle = Vec::new();

    for adj_list in backward_adj.iter() {
        backward_first_out.push(backward_head.len() as u32);
        for &(source, middle) in adj_list {
            backward_head.push(source);
            backward_shortcut_middle.push(middle);
        }
    }
    backward_first_out.push(backward_head.len() as u32);

    // Merge shortcut_middle (forward then backward)
    let total_edges = forward_head.len() + backward_head.len();
    let mut shortcut_middle = Vec::with_capacity(total_edges);
    shortcut_middle.extend_from_slice(&forward_shortcut_middle);
    shortcut_middle.extend_from_slice(&backward_shortcut_middle);

    let forward_len = forward_head.len();
    let backward_len = backward_head.len();

    CCHRouter {
        node_order: order.to_vec(),
        rank_to_node,
        forward_head,
        forward_first_out,
        forward_weight: vec![f32::INFINITY; forward_len],
        backward_head,
        backward_first_out,
        backward_weight: vec![f32::INFINITY; backward_len],
        shortcut_middle,
        original_edge_to_cch,
        node_count,
        edge_count,
    }
}
