//! Customizable Contraction Hierarchies (CCH) for fast pathfinding.
//!
//! Builds weight-independent topology (node ordering + shortcut graph) from a
//! `RoadGraph`. Customization and query come in a later plan (07-03).
//!
//! The CCH construction has two phases:
//! 1. **Ordering** -- nested dissection via BFS balanced bisection
//! 2. **Contraction** -- process nodes in order, adding shortcuts

pub mod cache;
pub mod ordering;
pub mod topology;

use crate::error::NetError;
use crate::graph::RoadGraph;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Core CCH data structure holding the contraction hierarchy topology.
///
/// Forward/backward stars are in CSR format indexed by rank.
/// Weights are initialized to `f32::INFINITY` -- set during customization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CCHRouter {
    /// Maps node index -> rank (contraction order).
    pub node_order: Vec<u32>,
    /// Maps rank -> node index (inverse of node_order).
    pub rank_to_node: Vec<u32>,
    /// CSR: target ranks of upward edges (forward star).
    pub forward_head: Vec<u32>,
    /// CSR: index into forward_head per rank.
    pub forward_first_out: Vec<u32>,
    /// Forward edge weights (set during customization, initialized to INFINITY).
    pub forward_weight: Vec<f32>,
    /// CSR: source ranks of upward edges (backward star).
    pub backward_head: Vec<u32>,
    /// CSR: index into backward_head per rank.
    pub backward_first_out: Vec<u32>,
    /// Backward edge weights (set during customization, initialized to INFINITY).
    pub backward_weight: Vec<f32>,
    /// For each CCH edge: None if original, Some(middle_rank) if shortcut.
    pub shortcut_middle: Vec<Option<u32>>,
    /// Maps original edge index -> CCH edge index (forward star position).
    pub original_edge_to_cch: Vec<usize>,
    /// Number of nodes in the graph.
    pub node_count: usize,
    /// Number of edges in the original graph (for cache invalidation).
    pub edge_count: usize,
}

impl CCHRouter {
    /// Build a CCH from a `RoadGraph`.
    ///
    /// Computes nested dissection ordering, then contracts nodes to build
    /// the shortcut graph topology.
    pub fn from_graph(graph: &RoadGraph) -> Self {
        let inner = graph.inner();
        let n = inner.node_count();
        let order = ordering::compute_ordering(inner);

        topology::contract_graph(inner, &order, n, graph.edge_count())
    }

    /// Build from graph with disk cache support.
    ///
    /// Tries to load from `cache_path` first. On miss or mismatch, builds
    /// from scratch and saves to cache.
    pub fn from_graph_cached(
        graph: &RoadGraph,
        cache_path: &Path,
    ) -> Result<Self, NetError> {
        // Try loading from cache
        if cache_path.exists() {
            match cache::load_cch(cache_path) {
                Ok(router) => {
                    // Validate cache matches current graph
                    if router.node_count == graph.node_count()
                        && router.edge_count == graph.edge_count()
                    {
                        log::info!("CCH cache hit: loaded from {}", cache_path.display());
                        return Ok(router);
                    }
                    log::info!(
                        "CCH cache stale: graph changed (nodes {}->{}, edges {}->{})",
                        router.node_count,
                        graph.node_count(),
                        router.edge_count,
                        graph.edge_count()
                    );
                }
                Err(e) => {
                    log::info!("CCH cache miss: {}", e);
                }
            }
        }

        // Build from scratch
        let router = Self::from_graph(graph);

        // Save to cache
        if let Err(e) = cache::save_cch(&router, cache_path) {
            log::warn!("Failed to save CCH cache: {}", e);
        } else {
            log::info!("CCH cache saved to {}", cache_path.display());
        }

        Ok(router)
    }
}
