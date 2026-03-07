//! Error types for the velos-net crate.

use thiserror::Error;

/// Errors that can occur in the road network subsystem.
#[derive(Debug, Error)]
pub enum NetError {
    /// I/O error reading files.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Error parsing OSM PBF data.
    #[error("OSM parse error: {0}")]
    OsmParse(String),

    /// Error parsing XML (SUMO .net.xml / .rou.xml).
    #[error("XML parse error: {0}")]
    XmlParse(String),

    /// No path found between two nodes.
    #[error("no path found from {from} to {to}")]
    NoPathFound { from: u32, to: u32 },

    /// Serialization/deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Override file parse error.
    #[error("override parse error: {0}")]
    OverrideParse(String),
}
