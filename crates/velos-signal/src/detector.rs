//! Virtual loop detector for traffic signal actuation.
//!
//! A `LoopDetector` represents a virtual inductive loop sensor embedded
//! in the road surface at a specific offset along an edge. It detects
//! when an agent crosses the sensor point during a simulation step.

/// A virtual loop detector (point sensor) on a road edge.
///
/// Detects when an agent's position crosses `offset_m` between two
/// successive simulation steps. This models the behavior of a real
/// inductive loop detector embedded in pavement.
#[derive(Debug, Clone)]
pub struct LoopDetector {
    /// The edge this detector is placed on.
    pub edge_id: u32,
    /// Distance from the start of the edge (metres).
    pub offset_m: f64,
}

/// A single detector reading for one simulation step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectorReading {
    /// Index of the detector that produced this reading.
    pub detector_index: usize,
    /// Whether an agent was detected (crossed the sensor point) this step.
    pub triggered: bool,
}

impl LoopDetector {
    /// Create a new loop detector on the given edge at the given offset.
    pub fn new(edge_id: u32, offset_m: f64) -> Self {
        Self { edge_id, offset_m }
    }

    /// Check whether an agent crossed this detector during a step.
    ///
    /// Returns `true` if the agent moved forward across the detector point,
    /// i.e., `prev_pos < offset_m <= cur_pos`.
    ///
    /// Backward movement (cur_pos < prev_pos) never triggers the detector,
    /// matching real-world inductive loop behavior.
    pub fn check(&self, prev_pos: f64, cur_pos: f64) -> bool {
        prev_pos < self.offset_m && cur_pos >= self.offset_m
    }
}
