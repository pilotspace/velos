//! velos-net: Road graph, OSM import, spatial index, and routing for VELOS.
//!
//! This crate provides the foundational road network that all vehicle simulation
//! depends on. Agents need edges to drive on, spatial queries for neighbor
//! detection, and routing for path assignment.

pub mod cleaning;
pub mod error;
pub mod graph;
pub mod osm_import;
pub mod projection;
pub mod routing;
pub mod spatial;
#[allow(dead_code, clippy::collapsible_if)]
pub mod sumo_import;

pub use cleaning::{clean_network, CleaningConfig, CleaningReport, OverrideFile};
pub use error::NetError;
pub use graph::{OneWayDirection, RoadClass, RoadEdge, RoadGraph, RoadNode, TimeWindow};
pub use osm_import::import_osm;
pub use projection::EquirectangularProjection;
pub use routing::find_route;
pub use spatial::{AgentPoint, SpatialIndex};
