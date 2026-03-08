//! Bus agent model: dwell time computation, bus stop component, and bus state.
//!
//! Implements an empirical dwell time model based on passenger boarding/alighting
//! counts. The formula follows transit simulation best practice:
//!
//!   dwell = fixed_dwell + per_boarding * boarding + per_alighting * alighting
//!
//! Capped at a maximum to prevent unrealistic dwell times.
//!
//! Reference: Levinson (1983) empirical bus dwell time model;
//! HCMC bus operations assume single-door boarding/alighting.

/// Parameters for the empirical bus dwell time model.
///
/// Default values: 5s fixed door open/close, 0.5s per boarding passenger,
/// 0.67s per alighting passenger, 60s maximum dwell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BusDwellModel {
    /// Fixed door open/close time (s).
    pub fixed_dwell_s: f64,
    /// Time per boarding passenger (s).
    pub per_boarding_s: f64,
    /// Time per alighting passenger (s).
    pub per_alighting_s: f64,
    /// Maximum dwell time cap (s).
    pub max_dwell_s: f64,
}

impl Default for BusDwellModel {
    fn default() -> Self {
        Self {
            fixed_dwell_s: 5.0,
            per_boarding_s: 0.5,
            per_alighting_s: 0.67,
            max_dwell_s: 60.0,
        }
    }
}

impl BusDwellModel {
    /// Compute dwell time given boarding and alighting passenger counts.
    ///
    /// Formula: `fixed_dwell + per_boarding * boarding + per_alighting * alighting`,
    /// capped at `max_dwell_s`.
    pub fn compute_dwell(&self, boarding: u32, alighting: u32) -> f64 {
        let raw = self.fixed_dwell_s
            + self.per_boarding_s * f64::from(boarding)
            + self.per_alighting_s * f64::from(alighting);
        raw.min(self.max_dwell_s)
    }
}

/// A bus stop attached to a road network edge.
///
/// ECS-attachable component representing a physical bus stop location.
/// `edge_id` and `offset_m` map the stop to a position on the road graph.
#[derive(Debug, Clone, PartialEq)]
pub struct BusStop {
    /// Road network edge this stop is on.
    pub edge_id: u32,
    /// Distance from the start of the edge (m).
    pub offset_m: f64,
    /// Maximum passenger capacity at this stop.
    pub capacity: u16,
    /// Human-readable stop name.
    pub name: String,
}

/// Proximity threshold for bus stop detection (m).
const STOP_PROXIMITY_M: f64 = 5.0;

/// Active bus journey state tracking stop progression and dwell lifecycle.
///
/// A bus traverses an ordered list of stop indices. At each stop, the bus
/// dwells for a computed duration (boarding + alighting) before resuming.
#[derive(Debug, Clone)]
pub struct BusState {
    /// Indices into an external `Vec<BusStop>` defining the route.
    stop_indices: Vec<usize>,
    /// Index into `stop_indices` for the next stop to visit.
    current_stop_index: usize,
    /// Remaining dwell time at the current stop (s).
    dwell_remaining: f64,
    /// Whether the bus is currently dwelling at a stop.
    is_dwelling: bool,
    /// Route index for color-coding in the visualization (0–255).
    route_index: u8,
}

impl BusState {
    /// Create a new bus state for a route with the given stop indices and route index.
    pub fn new(stop_indices: Vec<usize>, route_index: u8) -> Self {
        Self {
            stop_indices,
            current_stop_index: 0,
            dwell_remaining: 0.0,
            is_dwelling: false,
            route_index,
        }
    }

    /// The ordered stop indices for this bus route.
    pub fn stop_indices(&self) -> &[usize] {
        &self.stop_indices
    }

    /// Check if the bus should stop at its next scheduled stop.
    ///
    /// Returns `true` when the agent is on the same edge as the next stop
    /// and within `STOP_PROXIMITY_M` of the stop offset.
    pub fn should_stop(&self, current_edge: u32, current_offset: f64, stops: &[BusStop]) -> bool {
        if self.is_dwelling {
            return false;
        }
        let Some(&idx) = self.stop_indices.get(self.current_stop_index) else {
            return false;
        };
        let Some(stop) = stops.get(idx) else {
            return false;
        };
        stop.edge_id == current_edge
            && (current_offset - stop.offset_m).abs() <= STOP_PROXIMITY_M
    }

    /// Begin dwelling at the current stop.
    ///
    /// Computes dwell time from the model and passenger counts. The caller
    /// is responsible for generating stochastic passenger counts via RNG.
    pub fn begin_dwell(&mut self, model: &BusDwellModel, boarding: u32, alighting: u32) {
        self.dwell_remaining = model.compute_dwell(boarding, alighting);
        self.is_dwelling = true;
    }

    /// Tick the dwell timer by `dt` seconds.
    ///
    /// Returns `true` when dwell is complete. Advances `current_stop_index`
    /// and clears the dwelling flag upon completion.
    pub fn tick_dwell(&mut self, dt: f64) -> bool {
        if !self.is_dwelling {
            return false;
        }
        self.dwell_remaining -= dt;
        if self.dwell_remaining <= 0.0 {
            self.dwell_remaining = 0.0;
            self.is_dwelling = false;
            self.current_stop_index += 1;
            true
        } else {
            false
        }
    }

    /// Whether the bus is currently dwelling at a stop.
    pub fn is_dwelling(&self) -> bool {
        self.is_dwelling
    }

    /// Remaining dwell time at the current stop (s).
    pub fn dwell_remaining(&self) -> f64 {
        self.dwell_remaining
    }

    /// Index of the next stop to visit.
    pub fn current_stop_index(&self) -> usize {
        self.current_stop_index
    }

    /// Whether all stops have been visited.
    pub fn route_complete(&self) -> bool {
        self.current_stop_index >= self.stop_indices.len()
    }

    /// Route index for per-route color-coding (0–255).
    pub fn route_index(&self) -> u8 {
        self.route_index
    }
}
