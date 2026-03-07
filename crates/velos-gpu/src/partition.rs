//! Graph partitioning for multi-GPU road network distribution.
//!
//! Uses a BFS-based balanced bisection approach inspired by METIS.
//! The `metis` crate (C FFI) requires libmetis which fails to build
//! on macOS without manual installation. This pure-Rust fallback
//! provides balanced k-way partitioning via recursive bisection with
//! BFS-based node splitting.
//!
//! The algorithm:
//! 1. Convert road graph to node adjacency (undirected) view
//! 2. Recursively bisect until k partitions achieved
//! 3. BFS from seed node, assigning nodes to partition A until half reached
//! 4. Map node partitions to edge partitions (edge assigned to source node's partition)
//! 5. Identify boundary edges (source and target in different partitions)

use std::collections::{HashMap, HashSet, VecDeque};

use velos_net::RoadGraph;

/// Assignment of edges to partitions with boundary information.
#[derive(Debug, Clone)]
pub struct PartitionAssignment {
    /// Map from edge index (petgraph EdgeIndex raw) to partition ID.
    pub edge_to_partition: HashMap<u32, u32>,
    /// Boundary edges: (edge_id, source_partition, dest_partition).
    /// These are edges where the source node and target node are in
    /// different partitions.
    pub boundary_edges: Vec<(u32, u32, u32)>,
    /// Number of partitions.
    pub partition_count: u32,
}

impl PartitionAssignment {
    /// Build a boundary map: edge_id -> (src_partition, dst_partition).
    /// Only includes edges that cross partition boundaries.
    pub fn boundary_map(&self) -> HashMap<u32, (u32, u32)> {
        self.boundary_edges
            .iter()
            .map(|&(eid, sp, dp)| (eid, (sp, dp)))
            .collect()
    }
}

/// Partition the road network into `k` balanced partitions.
///
/// Uses recursive BFS bisection. Deterministic for the same graph and k.
/// Edge assignment: each edge is assigned to the partition of its source node.
/// Boundary edges are those where source and target nodes are in different partitions.
pub fn partition_network(graph: &RoadGraph, k: u32) -> PartitionAssignment {
    let g = graph.inner();
    let node_count = g.node_count();

    if node_count == 0 || k <= 1 {
        // Single partition: all edges in partition 0.
        let edge_to_partition: HashMap<u32, u32> = g
            .edge_indices()
            .map(|eidx| (eidx.index() as u32, 0))
            .collect();
        return PartitionAssignment {
            edge_to_partition,
            boundary_edges: Vec::new(),
            partition_count: 1.max(k),
        };
    }

    // Build undirected adjacency from directed graph.
    let mut adj: HashMap<u32, HashSet<u32>> = HashMap::new();
    for eidx in g.edge_indices() {
        let (src, tgt) = g.edge_endpoints(eidx).unwrap();
        let s = src.index() as u32;
        let t = tgt.index() as u32;
        adj.entry(s).or_default().insert(t);
        adj.entry(t).or_default().insert(s);
    }

    // Collect all node IDs sorted for determinism.
    let mut all_nodes: Vec<u32> = g
        .node_indices()
        .map(|n| n.index() as u32)
        .collect();
    all_nodes.sort();

    // Recursive bisection to get k partitions.
    let node_partitions = recursive_bisect(&all_nodes, &adj, k);

    // Map node partitions to edge partitions.
    let mut edge_to_partition = HashMap::new();
    let mut boundary_edges = Vec::new();

    for eidx in g.edge_indices() {
        let (src, tgt) = g.edge_endpoints(eidx).unwrap();
        let src_id = src.index() as u32;
        let tgt_id = tgt.index() as u32;
        let eid = eidx.index() as u32;

        let src_part = node_partitions[&src_id];
        let tgt_part = node_partitions[&tgt_id];

        // Edge assigned to source node's partition.
        edge_to_partition.insert(eid, src_part);

        if src_part != tgt_part {
            boundary_edges.push((eid, src_part, tgt_part));
        }
    }

    PartitionAssignment {
        edge_to_partition,
        boundary_edges,
        partition_count: k,
    }
}

/// Get all edge IDs belonging to a specific partition.
pub fn partition_edges(assignment: &PartitionAssignment, partition_id: u32) -> Vec<u32> {
    assignment
        .edge_to_partition
        .iter()
        .filter(|(_, pid)| **pid == partition_id)
        .map(|(eid, _)| *eid)
        .collect()
}

/// Recursively bisect node set into `k` partitions.
/// Returns node_id -> partition_id mapping.
fn recursive_bisect(
    nodes: &[u32],
    adj: &HashMap<u32, HashSet<u32>>,
    k: u32,
) -> HashMap<u32, u32> {
    if k <= 1 || nodes.len() <= 1 {
        return nodes.iter().map(|&n| (n, 0)).collect();
    }

    // Bisect into two groups.
    let (group_a, group_b) = bfs_bisect(nodes, adj);

    if k == 2 {
        let mut result = HashMap::new();
        for &n in &group_a {
            result.insert(n, 0);
        }
        for &n in &group_b {
            result.insert(n, 1);
        }
        return result;
    }

    // Recursive: split k among sub-groups proportional to size.
    let k_a = k.div_ceil(2);
    let k_b = k - k_a;

    let mut result = HashMap::new();

    let sub_a = recursive_bisect(&group_a, adj, k_a);
    for (&n, &p) in &sub_a {
        result.insert(n, p);
    }

    let sub_b = recursive_bisect(&group_b, adj, k_b.max(1));
    for (&n, &p) in &sub_b {
        result.insert(n, p + k_a);
    }

    result
}

/// BFS-based balanced bisection of a node set.
///
/// Starts BFS from the first node (deterministic). Assigns nodes to group A
/// until half the nodes are reached, remainder goes to group B.
fn bfs_bisect(
    nodes: &[u32],
    adj: &HashMap<u32, HashSet<u32>>,
) -> (Vec<u32>, Vec<u32>) {
    if nodes.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let node_set: HashSet<u32> = nodes.iter().copied().collect();
    let target_a = nodes.len() / 2;

    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let mut group_a = Vec::new();

    // Start from first node (sorted, so deterministic).
    queue.push_back(nodes[0]);
    visited.insert(nodes[0]);

    while let Some(node) = queue.pop_front() {
        if group_a.len() >= target_a {
            break;
        }
        group_a.push(node);

        if let Some(neighbors) = adj.get(&node) {
            let mut sorted_neighbors: Vec<u32> = neighbors
                .iter()
                .filter(|n| node_set.contains(n) && !visited.contains(n))
                .copied()
                .collect();
            sorted_neighbors.sort();
            for n in sorted_neighbors {
                if !visited.contains(&n) {
                    visited.insert(n);
                    queue.push_back(n);
                }
            }
        }
    }

    // Remaining nodes from the queue go to group A until target reached.
    while group_a.len() < target_a {
        if let Some(node) = queue.pop_front() {
            group_a.push(node);
        } else {
            break;
        }
    }

    let group_a_set: HashSet<u32> = group_a.iter().copied().collect();
    let group_b: Vec<u32> = nodes
        .iter()
        .filter(|n| !group_a_set.contains(n))
        .copied()
        .collect();

    (group_a, group_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bfs_bisect_splits_evenly() {
        let nodes: Vec<u32> = (0..10).collect();
        let mut adj = HashMap::new();
        for i in 0..9u32 {
            adj.entry(i).or_insert_with(HashSet::new).insert(i + 1);
            adj.entry(i + 1).or_insert_with(HashSet::new).insert(i);
        }

        let (a, b) = bfs_bisect(&nodes, &adj);
        assert_eq!(a.len(), 5);
        assert_eq!(b.len(), 5);
    }
}
