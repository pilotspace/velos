//! Traffic signal controllers for VELOS microsimulation.
//!
//! This crate provides:
//! - **SignalPlan** and **SignalPhase** for defining signal timing
//! - **SignalController** trait unifying all controller types
//! - **FixedTimeController** for cycling through green/amber/red phases
//! - **ActuatedController** for detector-based gap-out control
//! - **AdaptiveController** for queue-proportional green redistribution
//! - **LoopDetector** virtual point sensor for vehicle detection

pub mod actuated;
pub mod controller;
pub mod detector;
pub mod error;
pub mod plan;

use detector::DetectorReading;
use plan::PhaseState;

/// Unified trait for all signal controller types.
///
/// Controllers advance simulation time via `tick`, report current phase
/// state for each approach via `get_phase_state`, and can be reset.
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
