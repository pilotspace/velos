//! Motorbike sublane lateral movement model.
//!
//! Implements continuous lateral positioning for motorbikes in HCMC mixed traffic.
//! Motorbikes seek lateral gaps >= min_filter_gap and drift toward them at bounded speed.
//! At red lights, swarming behavior fills the full road width.

/// Parameters for the motorbike sublane model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SublaneParams {
    /// Minimum lateral gap required to filter through (metres).
    pub min_filter_gap: f64,
    /// Maximum lateral drift speed (m/s).
    pub max_lateral_speed: f64,
    /// Half-width of the ego motorbike body (metres).
    pub half_width: f64,
    /// Lateral drift speed during red-light swarming (m/s).
    pub swarm_lateral_speed: f64,
}

impl Default for SublaneParams {
    fn default() -> Self {
        Self {
            min_filter_gap: 0.6,
            max_lateral_speed: 1.0,
            half_width: 0.25,
            swarm_lateral_speed: 0.8,
        }
    }
}

/// Information about a neighboring agent for lateral gap computation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NeighborInfo {
    /// Lateral offset of neighbor from road right edge (metres).
    pub lateral_offset: f64,
    /// Longitudinal gap to this neighbor (metres, positive = ahead).
    pub longitudinal_gap: f64,
    /// Half-width of the neighbor body (metres).
    pub half_width: f64,
    /// Longitudinal speed of the neighbor (m/s).
    pub speed: f64,
}

/// Compute desired lateral target position for a motorbike.
///
/// Stub -- returns 0.0 to fail tests.
pub fn compute_desired_lateral(
    _own_lateral: f64,
    _own_speed: f64,
    _road_width: f64,
    _neighbors: &[NeighborInfo],
    _at_red_light: bool,
    _params: &SublaneParams,
) -> f64 {
    0.0 // Stub -- will fail all tests
}

/// Apply lateral drift toward desired position, clamped by max speed.
///
/// Stub -- returns 0.0 to fail tests.
pub fn apply_lateral_drift(
    _current: f64,
    _desired: f64,
    _max_speed: f64,
    _dt: f64,
) -> f64 {
    0.0 // Stub -- will fail all tests
}
