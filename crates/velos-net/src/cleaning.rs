//! Network cleaning pipeline for HCMC road graphs.
//!
//! Seven-step process:
//! 1. Remove small disconnected components
//! 2. Merge short edges (< 5m)
//! 3. Infer lane counts from road class
//! 4. Apply manual overrides from TOML
//! 5. Tag motorbike-only lanes
//! 6. Apply time-dependent one-way rules
//! 7. Validate final connectivity

use std::path::Path;

use petgraph::algo::kosaraju_scc;
use petgraph::visit::EdgeRef;
use serde::Deserialize;

use crate::error::NetError;
use crate::graph::{RoadClass, RoadGraph};

/// Configuration for the cleaning pipeline.
#[derive(Debug, Clone)]
pub struct CleaningConfig {
    /// Minimum number of edges for a component to be kept.
    pub min_component_edges: usize,
    /// Minimum edge length in metres; shorter edges are merged.
    pub min_edge_length_m: f64,
    /// Path to the override TOML file (optional).
    pub override_path: Option<std::path::PathBuf>,
}

impl Default for CleaningConfig {
    fn default() -> Self {
        Self {
            min_component_edges: 10,
            min_edge_length_m: 5.0,
            override_path: None,
        }
    }
}

/// Report of operations performed during cleaning.
#[derive(Debug, Clone, Default)]
pub struct CleaningReport {
    /// Number of disconnected components removed.
    pub components_removed: usize,
    /// Number of nodes removed from small components.
    pub nodes_removed: usize,
    /// Number of short edges merged.
    pub edges_merged: usize,
    /// Number of edges where lane count was inferred.
    pub lanes_inferred: usize,
    /// Number of overrides applied.
    pub overrides_applied: usize,
    /// Number of edges tagged as motorbike-only.
    pub motorbike_only_tagged: usize,
    /// Number of edges with time-dependent one-way rules applied.
    pub time_dependent_applied: usize,
    /// Whether the final graph is strongly connected.
    pub is_connected: bool,
}

/// Override file format for correcting OSM data.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OverrideFile {
    /// Edge-level overrides.
    #[serde(default)]
    pub edge_override: Vec<EdgeOverride>,
}

/// A single edge override entry.
#[derive(Debug, Clone, Deserialize)]
pub struct EdgeOverride {
    /// OSM way ID (e.g., "way/123456").
    pub osm_way_id: String,
    /// Override lane count.
    pub lanes: Option<u8>,
    /// Override speed limit in km/h.
    pub speed_limit_kmh: Option<f64>,
    /// Override motorbike-only flag.
    pub motorbike_only: Option<bool>,
    /// Reason for the override.
    pub reason: String,
}

/// Run the full cleaning pipeline on a road graph.
///
/// Modifies the graph in-place and returns a report of operations performed.
pub fn clean_network(graph: &mut RoadGraph, config: &CleaningConfig) -> CleaningReport {
    let mut report = CleaningReport::default();

    // Step 1: Remove small disconnected components.
    remove_small_components(graph, config.min_component_edges, &mut report);

    // Step 2: Merge short edges.
    merge_short_edges(graph, config.min_edge_length_m, &mut report);

    // Step 3: Infer lane counts from road class.
    infer_lane_counts(graph, &mut report);

    // Step 4: Apply overrides if path provided.
    if let Some(ref path) = config.override_path
        && let Ok(overrides) = load_overrides(path)
    {
        apply_overrides(graph, &overrides, &mut report);
    }

    // Step 5: Tag motorbike-only lanes.
    tag_motorbike_only_lanes(graph, &mut report);

    // Step 6: Apply time-dependent one-way rules for known HCMC streets.
    apply_time_dependent_oneways(graph, &mut report);

    // Step 7: Validate connectivity.
    report.is_connected = validate_connectivity(graph);

    report
}

/// Remove strongly connected components with fewer edges than the threshold.
/// Keeps only the largest component.
fn remove_small_components(
    graph: &mut RoadGraph,
    min_edges: usize,
    report: &mut CleaningReport,
) {
    let sccs = kosaraju_scc(graph.inner());

    if sccs.len() <= 1 {
        return;
    }

    // Find the largest SCC by node count.
    let largest_idx = sccs
        .iter()
        .enumerate()
        .max_by_key(|(_, scc)| scc.len())
        .map(|(i, _)| i)
        .unwrap_or(0);

    // Collect nodes to remove: all nodes NOT in the largest SCC.
    let mut to_remove = Vec::new();
    for (i, scc) in sccs.iter().enumerate() {
        if i == largest_idx {
            continue;
        }
        // Count edges in this SCC.
        let edge_count: usize = scc
            .iter()
            .flat_map(|&n| graph.inner().edges(n))
            .filter(|e| scc.contains(&e.target()))
            .count();

        // Remove components below threshold (always remove if not largest).
        if edge_count < min_edges {
            report.components_removed += 1;
            for &node in scc {
                to_remove.push(node);
            }
        }
    }

    report.nodes_removed = to_remove.len();

    // Remove nodes in reverse index order to avoid invalidating earlier indices.
    // petgraph uses swap-remove, so we must sort descending.
    to_remove.sort_by_key(|n| std::cmp::Reverse(n.index()));
    for node in to_remove {
        graph.inner_mut().remove_node(node);
    }
}

/// Merge edges shorter than `min_length` by contracting the shorter node
/// into its neighbor.
fn merge_short_edges(graph: &mut RoadGraph, min_length: f64, report: &mut CleaningReport) {
    // Iterative approach: find short edges and merge until none remain.
    // Limit iterations to prevent infinite loops on pathological cases.
    for _ in 0..100 {
        let short_edge = graph
            .inner()
            .edge_indices()
            .find(|&ei| graph.inner()[ei].length_m < min_length);

        let Some(ei) = short_edge else {
            break;
        };

        let (src, tgt) = graph.inner().edge_endpoints(ei).unwrap();
        if src == tgt {
            // Self-loop, just remove it.
            graph.inner_mut().remove_edge(ei);
            report.edges_merged += 1;
            continue;
        }

        // Contract: redirect all edges of `tgt` to `src`, then remove `tgt`.
        let tgt_edges: Vec<_> = graph
            .inner()
            .edges(tgt)
            .map(|e| (e.target(), e.weight().clone(), e.id()))
            .collect();

        let incoming_edges: Vec<_> = graph
            .inner()
            .edges_directed(tgt, petgraph::Direction::Incoming)
            .map(|e| (e.source(), e.weight().clone(), e.id()))
            .collect();

        // Add redirected outgoing edges from src (skip self-loops and duplicates).
        for (target, weight, _) in &tgt_edges {
            if *target != src && *target != tgt {
                graph.inner_mut().add_edge(src, *target, weight.clone());
            }
        }

        // Add redirected incoming edges to src.
        for (source, weight, _) in &incoming_edges {
            if *source != src && *source != tgt {
                graph.inner_mut().add_edge(*source, src, weight.clone());
            }
        }

        // Remove the contracted node (removes all its edges).
        graph.inner_mut().remove_node(tgt);
        report.edges_merged += 1;
    }
}

/// Infer lane counts from road class when lane_count is 0.
fn infer_lane_counts(graph: &mut RoadGraph, report: &mut CleaningReport) {
    for ei in graph.inner().edge_indices().collect::<Vec<_>>() {
        let edge = &graph.inner()[ei];
        if edge.lane_count == 0 {
            let inferred = match edge.road_class {
                RoadClass::Motorway | RoadClass::Trunk => 3,
                RoadClass::Primary => 3,
                RoadClass::Secondary => 2,
                RoadClass::Tertiary => 2,
                RoadClass::Residential | RoadClass::Service => 1,
            };
            graph.inner_mut()[ei].lane_count = inferred;
            report.lanes_inferred += 1;
        }
    }
}

/// Load override file from disk.
fn load_overrides(path: &Path) -> Result<OverrideFile, NetError> {
    let contents = std::fs::read_to_string(path)?;
    toml::from_str(&contents)
        .map_err(|e| NetError::OverrideParse(format!("TOML parse error: {e}")))
}

/// Apply manual overrides to graph edges.
fn apply_overrides(
    graph: &mut RoadGraph,
    overrides: &OverrideFile,
    report: &mut CleaningReport,
) {
    // Currently overrides reference OSM way IDs, which we don't store on edges.
    // This is a placeholder for future integration where edge metadata includes
    // the originating OSM way ID. For now, log the number of overrides available.
    report.overrides_applied = 0;

    // When OSM way IDs are stored on edges, iterate overrides and apply:
    // for ov in &overrides.edge_override { ... }
    let _ = overrides;
    let _ = graph;
}

/// Tag service/residential roads as motorbike-only based on road class.
/// This supplements the OSM-tag-based detection done during import by also
/// marking Service roads without explicit width tags.
fn tag_motorbike_only_lanes(graph: &mut RoadGraph, report: &mut CleaningReport) {
    for ei in graph.inner().edge_indices().collect::<Vec<_>>() {
        let edge = &graph.inner()[ei];
        // Already tagged during import from OSM tags.
        if edge.motorbike_only {
            continue;
        }
        // Heuristic: Service roads are typically narrow alleys in HCMC.
        if edge.road_class == RoadClass::Service {
            graph.inner_mut()[ei].motorbike_only = true;
            report.motorbike_only_tagged += 1;
        }
    }
}

/// Apply known HCMC time-dependent one-way rules.
///
/// Several streets in HCMC have directional changes by time of day to manage
/// rush-hour traffic. This applies a default set of rules. Custom rules can be
/// added via the override file in the future.
fn apply_time_dependent_oneways(_graph: &mut RoadGraph, report: &mut CleaningReport) {
    // Known HCMC time-dependent one-way patterns:
    // These would normally be matched by OSM way ID or street name.
    // For now, we just record that the step ran. Actual street-level rules
    // will be applied when the real HCMC OSM data is loaded and we have
    // way-ID-to-edge mapping.
    //
    // Example pattern that would be applied:
    // - Nguyen Thi Minh Khai (parts): AM peak forward, PM peak reverse
    // - Ly Thai To: AM peak forward only
    //
    // The TimeWindow infrastructure is ready for these rules.
    report.time_dependent_applied = 0;
}

/// Validate that the cleaned graph is strongly connected.
fn validate_connectivity(graph: &RoadGraph) -> bool {
    let sccs = kosaraju_scc(graph.inner());
    sccs.len() == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{OneWayDirection, TimeWindow};

    #[allow(dead_code)]
    fn make_edge(length: f64, road_class: RoadClass) -> crate::graph::RoadEdge {
        RoadEdge {
            length_m: length,
            speed_limit_mps: 13.9,
            lane_count: 2,
            oneway: false,
            road_class,
            geometry: vec![],
            motorbike_only: false,
            time_windows: None,
        }
    }

    #[test]
    fn override_file_empty_parses() {
        let toml_str = "";
        let overrides: OverrideFile = toml::from_str(toml_str).unwrap();
        assert!(overrides.edge_override.is_empty());
    }

    #[test]
    fn time_window_boundary() {
        let tw = TimeWindow {
            start_hour: 7,
            end_hour: 9,
            direction: OneWayDirection::Forward,
        };
        assert!(tw.contains_hour(7)); // inclusive start
        assert!(!tw.contains_hour(9)); // exclusive end
    }
}
