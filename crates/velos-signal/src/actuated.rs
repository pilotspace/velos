//! Actuated signal controller with gap-out logic.
//!
//! An `ActuatedController` extends green time while vehicles are detected
//! on the active phase's approaches. When no vehicle is detected for
//! `gap_threshold` seconds (the "gap-out"), the controller transitions
//! to the next phase (subject to `min_green` and `max_green` bounds).

use crate::detector::DetectorReading;
use crate::plan::{PhaseState, SignalPlan};
use crate::SignalController;

/// An actuated traffic signal controller.
///
/// Uses loop detector readings to extend green time dynamically.
/// Implements a gap-out state machine:
/// - Stays green while detectors fire on current phase approaches
/// - Gaps out after `gap_threshold` seconds of silence
/// - Respects `min_green` (won't transition before) and `max_green` (always transitions at)
#[derive(Debug, Clone)]
pub struct ActuatedController {
    /// The signal timing plan (used for phase ordering and amber durations).
    plan: SignalPlan,
    /// Index of the currently active phase.
    current_phase_idx: usize,
    /// Time the current phase has been active (seconds).
    phase_active_time: f64,
    /// Time since last detector trigger on current phase approaches (seconds).
    gap_timer: f64,
    /// Whether the current phase is in amber (transitioning).
    in_amber: bool,
    /// Elapsed time within amber period.
    amber_elapsed: f64,
    /// Minimum green time before gap-out can trigger (seconds).
    min_green: f64,
    /// Maximum green time -- forced transition regardless of detectors (seconds).
    max_green: f64,
    /// Gap-out threshold -- transition after this many seconds without detection (seconds).
    gap_threshold: f64,
    /// Total number of approaches at the intersection.
    num_approaches: usize,
}

impl ActuatedController {
    /// Create a new actuated controller with HCMC default parameters.
    ///
    /// Defaults: min_green = 7s, max_green = 60s, gap_threshold = 3s.
    pub fn new(plan: SignalPlan, num_approaches: usize) -> Self {
        Self::new_with_params(plan, num_approaches, 7.0, 60.0, 3.0)
    }

    /// Create a new actuated controller with custom parameters.
    pub fn new_with_params(
        plan: SignalPlan,
        num_approaches: usize,
        min_green: f64,
        max_green: f64,
        gap_threshold: f64,
    ) -> Self {
        Self {
            plan,
            current_phase_idx: 0,
            phase_active_time: 0.0,
            gap_timer: 0.0,
            in_amber: false,
            amber_elapsed: 0.0,
            min_green,
            max_green,
            gap_threshold,
            num_approaches,
        }
    }

    /// Check if any detector fired on the current phase's approaches.
    fn detector_on_current_phase(&self, detectors: &[DetectorReading]) -> bool {
        let current_approaches = &self.plan.phases[self.current_phase_idx].approaches;
        detectors.iter().any(|d| {
            d.triggered && current_approaches.contains(&d.detector_index)
        })
    }

    /// Advance to the next phase, starting fresh timers.
    fn advance_phase(&mut self) {
        self.current_phase_idx = (self.current_phase_idx + 1) % self.plan.phases.len();
        self.phase_active_time = 0.0;
        self.gap_timer = 0.0;
        self.in_amber = false;
        self.amber_elapsed = 0.0;
    }

    /// Get the amber duration for the current phase.
    fn current_amber_duration(&self) -> f64 {
        self.plan.phases[self.current_phase_idx].amber_duration
    }
}

impl SignalController for ActuatedController {
    fn tick(&mut self, dt: f64, detectors: &[DetectorReading]) {
        if self.plan.phases.is_empty() {
            return;
        }

        if self.in_amber {
            // In amber transition period
            self.amber_elapsed += dt;
            if self.amber_elapsed >= self.current_amber_duration() {
                // Amber complete, advance to next phase
                self.advance_phase();
            }
            return;
        }

        // In green period
        self.phase_active_time += dt;
        self.gap_timer += dt;

        // Check if any detector on current phase fired -- reset gap timer
        if self.detector_on_current_phase(detectors) {
            self.gap_timer = 0.0;
        }

        // Check transition conditions
        let should_transition = self.phase_active_time >= self.max_green
            || (self.phase_active_time >= self.min_green
                && self.gap_timer >= self.gap_threshold);

        if should_transition {
            self.in_amber = true;
            self.amber_elapsed = 0.0;
        }
    }

    fn get_phase_state(&self, approach_index: usize) -> PhaseState {
        if approach_index >= self.num_approaches {
            return PhaseState::Red;
        }

        let current_approaches = &self.plan.phases[self.current_phase_idx].approaches;
        if current_approaches.contains(&approach_index) {
            if self.in_amber {
                PhaseState::Amber
            } else {
                PhaseState::Green
            }
        } else {
            PhaseState::Red
        }
    }

    fn reset(&mut self) {
        self.current_phase_idx = 0;
        self.phase_active_time = 0.0;
        self.gap_timer = 0.0;
        self.in_amber = false;
        self.amber_elapsed = 0.0;
    }
}
