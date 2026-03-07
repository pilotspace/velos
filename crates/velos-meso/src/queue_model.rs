//! BPR-based spatial queue model for mesoscopic simulation.
//!
//! Each road edge in the mesoscopic zone is represented as a [`SpatialQueue`] with
//! BPR (Bureau of Public Roads) travel time function:
//!
//! ```text
//! t = t_free * (1 + alpha * (V/C)^beta)
//! ```
//!
//! where `alpha = 0.15`, `beta = 4.0` are standard BPR coefficients.
//! Vehicle exit is O(1) per edge per timestep via FIFO queue.

use std::collections::VecDeque;

/// A vehicle tracked in the mesoscopic simulation.
///
/// Meso vehicles are CPU-only -- they are never uploaded to GPU buffers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MesoVehicle {
    /// Unique vehicle identifier (matches the ECS entity / GPU agent ID).
    pub vehicle_id: u32,
    /// Simulation time when this vehicle entered the queue (seconds).
    pub entry_time: f64,
    /// Edge ID the vehicle will move to after exiting this queue.
    pub exit_edge: u32,
}

impl MesoVehicle {
    /// Create a new meso vehicle.
    pub fn new(vehicle_id: u32, entry_time: f64, exit_edge: u32) -> Self {
        Self {
            vehicle_id,
            entry_time,
            exit_edge,
        }
    }
}

/// BPR-based spatial queue representing one road edge in the mesoscopic zone.
///
/// Travel time follows the standard BPR function:
/// `t = t_free * (1 + alpha * (V/C)^beta)`
///
/// Vehicles enter at the back and exit from the front (FIFO).
/// The exit check is O(1) -- only the front vehicle is inspected.
#[derive(Debug)]
pub struct SpatialQueue {
    /// Free-flow travel time for this edge (seconds).
    t_free: f64,
    /// Edge capacity (maximum number of vehicles at free-flow).
    capacity: f64,
    /// BPR alpha coefficient (default 0.15).
    alpha: f64,
    /// BPR beta exponent (default 4.0).
    beta: f64,
    /// FIFO queue of vehicles currently on this edge.
    queue: VecDeque<MesoVehicle>,
}

impl SpatialQueue {
    /// Create a new spatial queue with standard BPR coefficients.
    ///
    /// # Arguments
    /// * `t_free` - Free-flow travel time in seconds
    /// * `capacity` - Edge capacity (max vehicles at free-flow)
    pub fn new(t_free: f64, capacity: f64) -> Self {
        Self {
            t_free,
            capacity,
            alpha: 0.15,
            beta: 4.0,
            queue: VecDeque::new(),
        }
    }

    /// Compute BPR travel time based on current volume-to-capacity ratio.
    ///
    /// Formula: `t = t_free * (1 + alpha * (V/C)^beta)`
    ///
    /// When the queue is empty, returns `t_free` (free-flow).
    pub fn travel_time(&self) -> f64 {
        let vc = self.queue.len() as f64 / self.capacity;
        // Standard BPR: beta=4.0, use multiplication for numerical stability
        let vc_pow = if (self.beta - 4.0).abs() < f64::EPSILON {
            let vc_sq = vc * vc;
            vc_sq * vc_sq
        } else {
            vc.powf(self.beta)
        };
        self.t_free * (1.0 + self.alpha * vc_pow)
    }

    /// Add a vehicle to the back of the queue.
    pub fn enter(&mut self, vehicle: MesoVehicle) {
        self.queue.push_back(vehicle);
    }

    /// Try to exit the front vehicle from the queue (O(1)).
    ///
    /// Returns `Some(vehicle)` if the front vehicle has been in the queue
    /// long enough (sim_time - entry_time >= travel_time). Otherwise `None`.
    ///
    /// The travel time used for the exit check is computed at exit time,
    /// so congestion changes affect when vehicles can leave.
    pub fn try_exit(&mut self, sim_time: f64) -> Option<MesoVehicle> {
        let front = self.queue.front()?;
        let tt = self.travel_time();
        if sim_time - front.entry_time >= tt {
            self.queue.pop_front()
        } else {
            None
        }
    }

    /// Number of vehicles currently in the queue.
    pub fn vehicle_count(&self) -> u32 {
        self.queue.len() as u32
    }

    /// Current volume-to-capacity ratio.
    pub fn vc_ratio(&self) -> f64 {
        self.queue.len() as f64 / self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bpr_partial_fill() {
        let mut q = SpatialQueue::new(100.0, 10.0);
        // V/C = 0.5 -> t = 100 * (1 + 0.15 * 0.5^4) = 100 * (1 + 0.15 * 0.0625) = 100.9375
        for i in 0..5 {
            q.enter(MesoVehicle::new(i, 0.0, 0));
        }
        let expected = 100.0 * (1.0 + 0.15 * 0.0625);
        assert!((q.travel_time() - expected).abs() < 1e-9);
    }

    #[test]
    fn vc_ratio_reflects_occupancy() {
        let mut q = SpatialQueue::new(10.0, 20.0);
        q.enter(MesoVehicle::new(1, 0.0, 0));
        assert!((q.vc_ratio() - 0.05).abs() < 1e-9);
    }
}
