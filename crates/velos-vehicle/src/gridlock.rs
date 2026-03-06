//! Gridlock detection via cycle-finding in a waiting graph.
//!
//! Agents that are stopped behind each other form a "waiting graph"
//! (directed: A -> B means A is blocked by B). A cycle in this graph
//! indicates gridlock that cannot resolve without intervention.

use std::collections::{HashMap, HashSet};

/// Detects gridlock cycles in agent waiting graphs.
#[derive(Debug, Clone)]
pub struct GridlockDetector {
    /// How long an agent must be stopped before considered for gridlock (seconds).
    pub timeout_secs: f64,
}

impl GridlockDetector {
    /// Create a new detector with the given timeout threshold.
    pub fn new(timeout_secs: f64) -> Self {
        Self { timeout_secs }
    }
}

impl Default for GridlockDetector {
    fn default() -> Self {
        Self::new(300.0)
    }
}

/// Detect all cycles in a waiting graph.
///
/// The waiting graph maps each blocked agent to the agent blocking it:
/// `agent_id -> blocker_id`. A cycle (A->B->C->A) means mutual deadlock.
///
/// # Arguments
/// * `waiting_graph` - map from blocked agent ID to blocker agent ID
///
/// # Returns
/// A vector of cycles, where each cycle is a vector of agent IDs
/// in the order they form the circular wait chain.
pub fn detect_cycles(waiting_graph: &HashMap<u32, u32>) -> Vec<Vec<u32>> {
    let mut visited = HashSet::new();
    let mut cycles = Vec::new();

    for &start in waiting_graph.keys() {
        if visited.contains(&start) {
            continue;
        }

        let mut path = Vec::new();
        let mut path_set = HashSet::new();
        let mut current = start;

        loop {
            if path_set.contains(&current) {
                // Found a cycle -- extract it
                let cycle_start = path.iter().position(|&id| id == current).unwrap();
                cycles.push(path[cycle_start..].to_vec());
                break;
            }
            if visited.contains(&current) {
                break;
            }
            path.push(current);
            path_set.insert(current);
            match waiting_graph.get(&current) {
                Some(&next) => current = next,
                None => break,
            }
        }

        for &node in &path {
            visited.insert(node);
        }
    }

    cycles
}
