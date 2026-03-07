//! Multi-GPU scheduling with boundary agent protocol.
//!
//! Implements logical GPU partitions that share a single physical device
//! but use separate buffer allocations and the same boundary transfer
//! protocol that would be used across physical devices.
//!
//! Each `GpuPartition` owns its own set of agents and staging buffers
//! (outbox/inbox). The `MultiGpuScheduler` orchestrates per-partition
//! dispatch and boundary agent routing each simulation step.

use std::collections::HashMap;

use velos_core::components::GpuAgentState;

use crate::partition::{partition_edges, PartitionAssignment};

/// An agent transferring between partitions via the boundary protocol.
///
/// Contains the full agent state plus routing metadata for the
/// destination partition and edge.
#[derive(Debug, Clone, Copy)]
pub struct BoundaryAgent {
    /// Full agent state at the moment of boundary crossing.
    pub state: GpuAgentState,
    /// Target partition ID.
    pub dest_partition: u32,
    /// Target edge ID in the destination partition.
    pub dest_edge_id: u32,
}

/// A logical GPU partition with its own agent buffer and boundary staging.
///
/// In the single-GPU case (current implementation), all partitions share
/// the same `wgpu::Device` and `wgpu::Queue` but have independent buffer
/// allocations. The boundary protocol (outbox/inbox) is identical to what
/// would be used across physical devices.
pub struct GpuPartition {
    /// Partition identifier (0-based).
    pub partition_id: u32,
    /// Edge IDs belonging to this partition.
    pub edge_ids: Vec<u32>,
    /// Agent states currently in this partition.
    pub agent_states: Vec<GpuAgentState>,
    /// Agents leaving this partition (collected after dispatch).
    pub outbox: Vec<BoundaryAgent>,
    /// Agents arriving from other partitions (to be spawned before next dispatch).
    pub inbox: Vec<BoundaryAgent>,
    /// Set of edge IDs for O(1) membership checks.
    edge_set: std::collections::HashSet<u32>,
}

impl GpuPartition {
    /// Create a new partition with the given edge IDs.
    pub fn new(partition_id: u32, edge_ids: Vec<u32>) -> Self {
        let edge_set = edge_ids.iter().copied().collect();
        Self {
            partition_id,
            edge_ids,
            agent_states: Vec::new(),
            outbox: Vec::new(),
            inbox: Vec::new(),
            edge_set,
        }
    }

    /// Spawn inbox agents into this partition's agent buffer.
    ///
    /// Each incoming boundary agent gets its edge_id updated to the
    /// destination edge and is added to the local agent state array.
    /// The inbox is drained after processing.
    pub fn spawn_inbox_agents(&mut self) {
        for ba in self.inbox.drain(..) {
            let mut agent = ba.state;
            agent.edge_id = ba.dest_edge_id;
            // Reset position to start of new edge (position 0).
            agent.position = 0;
            self.agent_states.push(agent);
        }
    }

    /// Identify agents on boundary edges and move them to the outbox.
    ///
    /// An agent is considered to be crossing a boundary if its current
    /// edge_id is in the boundary map (meaning the edge connects two
    /// different partitions).
    ///
    /// `boundary_map`: edge_id -> (src_partition, dst_partition)
    pub fn collect_outbox_agents(
        &mut self,
        boundary_map: &HashMap<u32, (u32, u32)>,
    ) {
        let mut remaining = Vec::new();

        for agent in self.agent_states.drain(..) {
            if let Some(&(src_p, dst_p)) = boundary_map.get(&agent.edge_id) {
                if src_p == self.partition_id {
                    // Agent is on a boundary edge originating from this partition.
                    // Move to outbox destined for the other partition.
                    self.outbox.push(BoundaryAgent {
                        state: agent,
                        dest_partition: dst_p,
                        dest_edge_id: agent.edge_id, // Will be on the same edge but in dest partition
                    });
                } else {
                    remaining.push(agent);
                }
            } else {
                remaining.push(agent);
            }
        }

        self.agent_states = remaining;
    }

    /// Check if this partition owns the given edge.
    pub fn contains_edge(&self, edge_id: u32) -> bool {
        self.edge_set.contains(&edge_id)
    }
}

/// Orchestrates multi-partition simulation dispatch and boundary routing.
///
/// Manages a set of `GpuPartition` instances and coordinates the
/// per-step protocol:
/// 1. Drain inboxes (spawn boundary agents in destination partitions)
/// 2. Dispatch physics for each partition
/// 3. Collect outboxes (identify agents on boundary edges)
/// 4. Route outbox agents to correct partition inboxes
pub struct MultiGpuScheduler {
    /// Partitions indexed by partition ID.
    partitions: Vec<GpuPartition>,
    /// Boundary edge map: edge_id -> (src_partition, dst_partition).
    boundary_map: HashMap<u32, (u32, u32)>,
    /// The partition assignment used to create this scheduler.
    assignment: PartitionAssignment,
}

impl MultiGpuScheduler {
    /// Create a new scheduler from a partition assignment.
    pub fn new(assignment: PartitionAssignment) -> Self {
        let k = assignment.partition_count;
        let boundary_map = assignment.boundary_map();

        let mut partitions = Vec::with_capacity(k as usize);
        for pid in 0..k {
            let edges = partition_edges(&assignment, pid);
            partitions.push(GpuPartition::new(pid, edges));
        }

        Self {
            partitions,
            boundary_map,
            assignment,
        }
    }

    /// Get edge IDs for a specific partition.
    pub fn partition_edge_ids(&self, partition_id: u32) -> Vec<u32> {
        if let Some(p) = self.partitions.get(partition_id as usize) {
            p.edge_ids.clone()
        } else {
            Vec::new()
        }
    }

    /// Distribute agents across partitions based on their edge_id.
    pub fn distribute_agents(&mut self, agents: &[GpuAgentState]) {
        for agent in agents {
            if let Some(&pid) = self.assignment.edge_to_partition.get(&agent.edge_id)
                && let Some(p) = self.partitions.get_mut(pid as usize)
            {
                p.agent_states.push(*agent);
            }
        }
    }

    /// Total agent count across all partitions.
    pub fn agent_count(&self) -> usize {
        self.partitions
            .iter()
            .map(|p| p.agent_states.len())
            .sum()
    }

    /// Run one CPU-only protocol step (no GPU dispatch).
    ///
    /// Used for testing the boundary transfer protocol without requiring
    /// a GPU adapter. Performs:
    /// 1. Drain inboxes
    /// 2. (Skip GPU dispatch)
    /// 3. Collect outboxes
    /// 4. Route outbox agents to destination partition inboxes
    pub fn step_cpu(&mut self) {
        // 1. Drain inboxes: spawn boundary agents in destination partitions.
        for p in &mut self.partitions {
            p.spawn_inbox_agents();
        }

        // 2. GPU dispatch would happen here (skipped for CPU-only test).

        // 3. Collect outboxes from each partition.
        for p in &mut self.partitions {
            p.collect_outbox_agents(&self.boundary_map);
        }

        // 4. Route outbox agents to destination partition inboxes.
        self.route_outbox_agents();
    }

    /// Route all outbox agents to their destination partition inboxes.
    fn route_outbox_agents(&mut self) {
        // Collect all outbox agents first to avoid borrow conflicts.
        let mut all_outbox: Vec<BoundaryAgent> = Vec::new();
        for p in &mut self.partitions {
            all_outbox.append(&mut p.outbox);
        }

        // Route to destination inboxes.
        for ba in all_outbox {
            let dest = ba.dest_partition as usize;
            if let Some(p) = self.partitions.get_mut(dest) {
                p.inbox.push(ba);
            }
        }
    }

    /// Get immutable access to partitions.
    pub fn partitions(&self) -> &[GpuPartition] {
        &self.partitions
    }

    /// Get mutable access to partitions.
    pub fn partitions_mut(&mut self) -> &mut [GpuPartition] {
        &mut self.partitions
    }

    /// Get the boundary map.
    pub fn boundary_map(&self) -> &HashMap<u32, (u32, u32)> {
        &self.boundary_map
    }

    /// Get the partition assignment.
    pub fn assignment(&self) -> &PartitionAssignment {
        &self.assignment
    }
}
