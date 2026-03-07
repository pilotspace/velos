//! Nested dissection node ordering via BFS balanced bisection.
//!
//! Produces a contraction order where separator nodes get the highest ranks
//! (contracted last), giving CCH its quality ordering without METIS.

use petgraph::graph::DiGraph;
use std::collections::VecDeque;

use crate::graph::{RoadEdge, RoadNode};

/// Compute a nested dissection ordering for the graph.
///
/// Returns a Vec mapping node_index -> rank. Rank 0 is contracted first,
/// rank n-1 is contracted last (separator nodes get highest ranks).
///
/// Algorithm:
/// 1. Recursively bisect using BFS from a peripheral node
/// 2. Separator nodes (with edges crossing the bisection) get highest ranks
/// 3. Recurse into each partition
/// 4. Base case: partition <= 10 nodes, order by degree ascending
pub fn compute_ordering(graph: &DiGraph<RoadNode, RoadEdge>) -> Vec<u32> {
    let n = graph.node_count();
    if n == 0 {
        return vec![];
    }

    // Build adjacency list (undirected view of the directed graph)
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];
    for edge in graph.edge_indices() {
        let (s, t) = graph.edge_endpoints(edge).unwrap();
        let si = s.index();
        let ti = t.index();
        if !adj[si].contains(&ti) {
            adj[si].push(ti);
        }
        if !adj[ti].contains(&si) {
            adj[ti].push(si);
        }
    }

    let mut order = vec![0u32; n];
    let all_nodes: Vec<usize> = (0..n).collect();
    let mut next_rank = 0u32;

    nested_dissection(&adj, &all_nodes, &mut order, &mut next_rank);

    order
}

/// Recursive nested dissection on a subset of nodes.
///
/// Assigns ranks starting from `next_rank` upward. Partition nodes get
/// low ranks, separator nodes get high ranks within this subproblem.
fn nested_dissection(
    adj: &[Vec<usize>],
    nodes: &[usize],
    order: &mut [u32],
    next_rank: &mut u32,
) {
    if nodes.is_empty() {
        return;
    }

    // Base case: small partition -- order by degree ascending
    if nodes.len() <= 10 {
        let mut sorted: Vec<usize> = nodes.to_vec();
        sorted.sort_by_key(|&n| {
            // Count neighbors within this partition only
            let node_set: std::collections::HashSet<usize> =
                nodes.iter().copied().collect();
            adj[n].iter().filter(|nb| node_set.contains(nb)).count()
        });
        for &node in &sorted {
            order[node] = *next_rank;
            *next_rank += 1;
        }
        return;
    }

    // BFS balanced bisection
    let node_set: std::collections::HashSet<usize> = nodes.iter().copied().collect();
    let (part_a, part_b, separator) = bfs_bisect(adj, nodes, &node_set);

    // Recurse into partitions (get lower ranks)
    nested_dissection(adj, &part_a, order, next_rank);
    nested_dissection(adj, &part_b, order, next_rank);

    // Separator nodes get highest ranks (contracted last)
    for &node in &separator {
        order[node] = *next_rank;
        *next_rank += 1;
    }
}

/// BFS-based balanced bisection of a node set.
///
/// 1. Find a peripheral node (farthest from arbitrary start via BFS)
/// 2. BFS from peripheral, assign first half to partition A
/// 3. Nodes with edges crossing partitions become separator
fn bfs_bisect(
    adj: &[Vec<usize>],
    nodes: &[usize],
    node_set: &std::collections::HashSet<usize>,
) -> (Vec<usize>, Vec<usize>, Vec<usize>) {
    if nodes.len() <= 1 {
        return (nodes.to_vec(), vec![], vec![]);
    }

    // Find peripheral node: BFS from first node, take the farthest
    let peripheral = find_peripheral(adj, nodes[0], node_set);

    // BFS from peripheral to get ordering
    let bfs_order = bfs_from(adj, peripheral, node_set);

    let half = bfs_order.len() / 2;
    let part_a_set: std::collections::HashSet<usize> =
        bfs_order[..half].iter().copied().collect();
    let part_b_set: std::collections::HashSet<usize> =
        bfs_order[half..].iter().copied().collect();

    // Find separator: nodes with neighbors in the other partition
    let mut separator = Vec::new();
    let mut part_a = Vec::new();
    let mut part_b = Vec::new();

    for &node in &bfs_order[..half] {
        let crosses = adj[node]
            .iter()
            .any(|nb| part_b_set.contains(nb));
        if crosses {
            separator.push(node);
        } else {
            part_a.push(node);
        }
    }

    for &node in &bfs_order[half..] {
        let crosses = adj[node]
            .iter()
            .any(|nb| part_a_set.contains(nb));
        if crosses {
            separator.push(node);
        } else {
            part_b.push(node);
        }
    }

    (part_a, part_b, separator)
}

/// Find a peripheral node by running BFS and taking the farthest reached.
fn find_peripheral(
    adj: &[Vec<usize>],
    start: usize,
    node_set: &std::collections::HashSet<usize>,
) -> usize {
    let order = bfs_from(adj, start, node_set);
    *order.last().unwrap_or(&start)
}

/// BFS from a start node, visiting only nodes in the given set.
/// Returns nodes in BFS visit order.
fn bfs_from(
    adj: &[Vec<usize>],
    start: usize,
    node_set: &std::collections::HashSet<usize>,
) -> Vec<usize> {
    let mut visited = std::collections::HashSet::new();
    let mut queue = VecDeque::new();
    let mut result = Vec::new();

    visited.insert(start);
    queue.push_back(start);

    while let Some(node) = queue.pop_front() {
        result.push(node);
        for &nb in &adj[node] {
            if node_set.contains(&nb) && visited.insert(nb) {
                queue.push_back(nb);
            }
        }
    }

    // Include any nodes not reached by BFS (disconnected components)
    for &node in node_set {
        if !visited.contains(&node) {
            result.push(node);
        }
    }

    result
}
