//! Road graph representation backed by petgraph DiGraph.
//!
//! Stores the directed road network with nodes at intersections (or way endpoints)
//! and edges representing road segments with lane counts, speed limits, and geometry.
//! Supports motorbike-only lanes and time-dependent one-way directions for HCMC.

use std::path::Path;

use petgraph::graph::{DiGraph, EdgeIndex, NodeIndex};
use serde::{Deserialize, Serialize};

use crate::error::NetError;

/// Classification of road segments, matching OSM `highway` tag values
/// that are imported for the HCMC simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RoadClass {
    Motorway,
    Trunk,
    Primary,
    Secondary,
    Tertiary,
    Residential,
    Service,
}

/// Direction constraint for time-dependent one-way edges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OneWayDirection {
    /// Traffic flows from source to target only.
    Forward,
    /// Traffic flows from target to source only.
    Reverse,
    /// Traffic flows in both directions.
    Both,
}

/// A time window specifying when a directional constraint applies.
///
/// Used for HCMC streets that change one-way direction by time of day
/// (e.g., morning rush inbound, evening rush outbound).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Start hour (inclusive), 0-23.
    pub start_hour: u8,
    /// End hour (exclusive), 1-24.
    pub end_hour: u8,
    /// Direction of traffic during this window.
    pub direction: OneWayDirection,
}

impl TimeWindow {
    /// Check whether a given hour falls within this time window.
    pub fn contains_hour(&self, hour: u8) -> bool {
        hour >= self.start_hour && hour < self.end_hour
    }
}

/// A node in the road graph, representing an intersection or way endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadNode {
    /// Position in local metres [x_east, y_north].
    pub pos: [f64; 2],
}

/// A directed edge in the road graph, representing one direction of a road segment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadEdge {
    /// Length of the edge in metres (Euclidean along geometry).
    pub length_m: f64,
    /// Speed limit in metres per second.
    pub speed_limit_mps: f64,
    /// Number of lanes in this direction.
    pub lane_count: u8,
    /// Whether the original OSM way was marked oneway.
    pub oneway: bool,
    /// Road classification.
    pub road_class: RoadClass,
    /// Polyline geometry in local metres, including start and end points.
    pub geometry: Vec<[f64; 2]>,
    /// Whether this edge is restricted to motorbikes only (alleys < 4m wide).
    pub motorbike_only: bool,
    /// Time-dependent one-way windows. `None` means the edge's `oneway` field
    /// applies at all times. `Some(windows)` overrides direction by time of day.
    pub time_windows: Option<Vec<TimeWindow>>,
}

/// Serializable representation of the road graph for binary persistence.
#[derive(Serialize, Deserialize)]
struct SerializableGraph {
    nodes: Vec<(u32, RoadNode)>,
    edges: Vec<(u32, u32, RoadEdge)>,
}

/// Wrapper around `petgraph::graph::DiGraph<RoadNode, RoadEdge>` providing
/// convenient accessors for the road network.
pub struct RoadGraph {
    inner: DiGraph<RoadNode, RoadEdge>,
}

impl RoadGraph {
    /// Create a new `RoadGraph` from an existing `DiGraph`.
    pub fn new(graph: DiGraph<RoadNode, RoadEdge>) -> Self {
        Self { inner: graph }
    }

    /// Number of nodes (intersections) in the graph.
    pub fn node_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Number of directed edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.inner.edge_count()
    }

    /// Borrow the underlying `DiGraph`.
    pub fn inner(&self) -> &DiGraph<RoadNode, RoadEdge> {
        &self.inner
    }

    /// Mutably borrow the underlying `DiGraph`.
    pub fn inner_mut(&mut self) -> &mut DiGraph<RoadNode, RoadEdge> {
        &mut self.inner
    }

    /// Get the position of a node in local metres.
    ///
    /// # Panics
    /// Panics if the node index is out of bounds.
    pub fn node_position(&self, idx: NodeIndex) -> [f64; 2] {
        self.inner[idx].pos
    }

    /// Get all edge indices in the graph.
    pub fn edge_indices(&self) -> impl Iterator<Item = EdgeIndex> + '_ {
        self.inner.edge_indices()
    }

    /// Serialize the graph to a binary file using postcard.
    pub fn serialize_binary(&self, path: &Path) -> Result<(), NetError> {
        let serializable = SerializableGraph {
            nodes: self
                .inner
                .node_indices()
                .map(|idx| (idx.index() as u32, self.inner[idx].clone()))
                .collect(),
            edges: self
                .inner
                .edge_indices()
                .map(|idx| {
                    let (src, tgt) = self.inner.edge_endpoints(idx).unwrap();
                    (
                        src.index() as u32,
                        tgt.index() as u32,
                        self.inner[idx].clone(),
                    )
                })
                .collect(),
        };

        let bytes = postcard::to_allocvec(&serializable)
            .map_err(|e| NetError::Serialization(format!("postcard serialize: {e}")))?;

        std::fs::write(path, bytes)?;
        Ok(())
    }

    /// Deserialize a graph from a binary file.
    pub fn deserialize_binary(path: &Path) -> Result<Self, NetError> {
        let bytes = std::fs::read(path)?;
        let data: SerializableGraph = postcard::from_bytes(&bytes)
            .map_err(|e| NetError::Serialization(format!("postcard deserialize: {e}")))?;

        let mut graph = DiGraph::new();

        // Add nodes in order (indices must match).
        let mut node_map = std::collections::HashMap::new();
        for (idx, node) in &data.nodes {
            let ni = graph.add_node(node.clone());
            node_map.insert(*idx, ni);
        }

        // Add edges.
        for (src, tgt, edge) in data.edges {
            if let (Some(&s), Some(&t)) = (node_map.get(&src), node_map.get(&tgt)) {
                graph.add_edge(s, t, edge);
            }
        }

        Ok(Self { inner: graph })
    }
}

impl std::fmt::Debug for RoadGraph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoadGraph")
            .field("nodes", &self.node_count())
            .field("edges", &self.edge_count())
            .finish()
    }
}
