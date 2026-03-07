//! Zone configuration for designating road edges as meso, micro, or buffer zones.
//!
//! Edges in the network are statically designated into zones:
//! - **Micro**: Full microscopic simulation (GPU-accelerated IDM/MOBIL)
//! - **Meso**: Mesoscopic BPR queue model (CPU-only, O(1) per edge)
//! - **Buffer**: 100m graduated transition zone between meso and micro

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::MesoError;

/// Zone type for a road edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ZoneType {
    /// Full microscopic simulation on GPU.
    Micro,
    /// Mesoscopic BPR queue model on CPU.
    Meso,
    /// Graduated buffer zone for meso-micro transitions.
    Buffer,
}

/// Static zone designation mapping edge IDs to zone types.
///
/// Unconfigured edges default to [`ZoneType::Micro`] (full simulation).
#[derive(Debug, Clone)]
pub struct ZoneConfig {
    zones: HashMap<u32, ZoneType>,
}

impl ZoneConfig {
    /// Create an empty zone config (all edges default to Micro).
    pub fn new() -> Self {
        Self {
            zones: HashMap::new(),
        }
    }

    /// Set the zone type for a specific edge.
    pub fn set_zone(&mut self, edge_id: u32, zone: ZoneType) {
        self.zones.insert(edge_id, zone);
    }

    /// Get the zone type for an edge. Returns `Micro` if not configured.
    pub fn zone_type(&self, edge_id: u32) -> ZoneType {
        self.zones.get(&edge_id).copied().unwrap_or(ZoneType::Micro)
    }

    /// Load zone configuration from a TOML file.
    pub fn load_from_toml(path: &Path) -> Result<Self, MesoError> {
        let content = std::fs::read_to_string(path)?;
        Self::load_from_toml_str(&content)
    }

    /// Load zone configuration from a TOML string.
    pub fn load_from_toml_str(toml_str: &str) -> Result<Self, MesoError> {
        let doc: ZoneConfigDoc =
            toml::from_str(toml_str).map_err(|e| MesoError::ZoneConfigParse(e.to_string()))?;

        let mut config = Self::new();
        for entry in doc.zones {
            let zone = match entry.zone.as_str() {
                "meso" => ZoneType::Meso,
                "micro" => ZoneType::Micro,
                "buffer" => ZoneType::Buffer,
                other => {
                    return Err(MesoError::ZoneConfigParse(format!(
                        "unknown zone type: {other}"
                    )));
                }
            };
            config.set_zone(entry.edge_id, zone);
        }
        Ok(config)
    }

    /// Auto-designate zones based on distance from a centroid.
    ///
    /// Edges within `micro_radius` are Micro, edges within `micro_radius + buffer_width`
    /// are Buffer, and everything beyond is Meso.
    ///
    /// # Arguments
    /// * `edge_positions` - Iterator of (edge_id, x, y) tuples for edge midpoints
    /// * `center_x` - X coordinate of core area centroid
    /// * `center_y` - Y coordinate of core area centroid
    /// * `micro_radius` - Radius of the micro simulation zone (meters)
    /// * `buffer_width` - Width of the buffer transition zone (meters)
    pub fn from_centroid_distance(
        edge_positions: impl IntoIterator<Item = (u32, f64, f64)>,
        center_x: f64,
        center_y: f64,
        micro_radius: f64,
        buffer_width: f64,
    ) -> Self {
        let mut config = Self::new();
        let buffer_outer = micro_radius + buffer_width;

        for (edge_id, x, y) in edge_positions {
            let dx = x - center_x;
            let dy = y - center_y;
            let dist = (dx * dx + dy * dy).sqrt();

            let zone = if dist <= micro_radius {
                ZoneType::Micro
            } else if dist <= buffer_outer {
                ZoneType::Buffer
            } else {
                ZoneType::Meso
            };
            config.set_zone(edge_id, zone);
        }
        config
    }

    /// Number of edges with explicit zone assignments.
    pub fn len(&self) -> usize {
        self.zones.len()
    }

    /// Whether no edges have explicit zone assignments.
    pub fn is_empty(&self) -> bool {
        self.zones.is_empty()
    }
}

impl Default for ZoneConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// TOML document structure for zone configuration.
#[derive(Deserialize)]
struct ZoneConfigDoc {
    zones: Vec<ZoneEntry>,
}

/// Single zone entry in the TOML configuration.
#[derive(Deserialize)]
struct ZoneEntry {
    edge_id: u32,
    zone: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centroid_distance_classification() {
        let edges = vec![
            (1, 0.0, 0.0),   // at center -> Micro
            (2, 50.0, 0.0),  // 50m -> Micro (within 100m radius)
            (3, 110.0, 0.0), // 110m -> Buffer (within 100+50=150m)
            (4, 200.0, 0.0), // 200m -> Meso (beyond 150m)
        ];
        let config = ZoneConfig::from_centroid_distance(edges, 0.0, 0.0, 100.0, 50.0);

        assert_eq!(config.zone_type(1), ZoneType::Micro);
        assert_eq!(config.zone_type(2), ZoneType::Micro);
        assert_eq!(config.zone_type(3), ZoneType::Buffer);
        assert_eq!(config.zone_type(4), ZoneType::Meso);
    }

    #[test]
    fn invalid_zone_type_returns_error() {
        let toml_str = r#"
[[zones]]
edge_id = 1
zone = "invalid"
"#;
        let result = ZoneConfig::load_from_toml_str(toml_str);
        assert!(result.is_err());
    }
}
