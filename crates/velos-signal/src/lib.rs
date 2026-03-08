//! Traffic signal controllers for VELOS microsimulation.
//!
//! This crate provides:
//! - **SignalPlan** and **SignalPhase** for defining signal timing
//! - **SignalController** trait unifying all controller types
//! - **FixedTimeController** for cycling through green/amber/red phases
//! - **ActuatedController** for detector-based gap-out control
//! - **AdaptiveController** for queue-proportional green redistribution
//! - **LoopDetector** virtual point sensor for vehicle detection
//! - **SpatBroadcast** for V2I signal phase/timing broadcast
//! - **PriorityQueue** for bus/emergency signal priority requests
//! - **TrafficSign** and **GpuSign** for traffic sign interaction

pub mod actuated;
pub mod adaptive;
pub mod config;
pub mod controller;
pub mod detector;
pub mod error;
pub mod plan;
pub mod priority;
pub mod signs;
pub mod spat;

pub use config::{load_signal_config, IntersectionConfig, SignalConfig};
use detector::DetectorReading;
use plan::PhaseState;
use priority::PriorityRequest;
use spat::SpatBroadcast;

/// Unified trait for all signal controller types.
///
/// Controllers advance simulation time via `tick`, report current phase
/// state for each approach via `get_phase_state`, and can be reset.
/// Extended with SPaT broadcast and priority request support.
pub trait SignalController {
    /// Advance the controller by `dt` seconds, incorporating detector readings.
    ///
    /// Fixed-time controllers ignore detector readings. Actuated controllers
    /// use them for gap-out decisions.
    fn tick(&mut self, dt: f64, detectors: &[DetectorReading]);

    /// Get the current phase state for the given approach index.
    fn get_phase_state(&self, approach_index: usize) -> PhaseState;

    /// Reset the controller to the start of the cycle.
    fn reset(&mut self);

    /// Get SPaT broadcast data for all approaches.
    ///
    /// Default implementation returns current phase states with zero timing.
    /// Controllers with timing awareness should override this.
    fn spat_data(&self, num_approaches: usize) -> SpatBroadcast {
        let approach_states = (0..num_approaches)
            .map(|i| self.get_phase_state(i))
            .collect();
        SpatBroadcast {
            approach_states,
            time_to_next_change: 0.0,
            cycle_time: 0.0,
        }
    }

    /// Handle a signal priority request from a bus or emergency vehicle.
    ///
    /// Default implementation is a no-op. Actuated and adaptive controllers
    /// override this to extend green or shorten conflicting red.
    fn request_priority(&mut self, _request: &PriorityRequest) {
        // No-op by default (fixed-time controllers cannot respond to priority)
    }
}

/// Implement `SignalController` for `FixedTimeController`.
///
/// The fixed-time controller ignores detector readings -- it cycles
/// through phases based solely on elapsed time.
impl SignalController for controller::FixedTimeController {
    fn tick(&mut self, dt: f64, _detectors: &[DetectorReading]) {
        self.tick(dt);
    }

    fn get_phase_state(&self, approach_index: usize) -> PhaseState {
        self.get_phase_state(approach_index)
    }

    fn reset(&mut self) {
        self.reset();
    }
}
