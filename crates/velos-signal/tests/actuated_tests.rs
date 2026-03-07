//! Tests for ActuatedController with gap-out logic.

use velos_signal::actuated::ActuatedController;
use velos_signal::detector::DetectorReading;
use velos_signal::plan::{PhaseState, SignalPhase, SignalPlan};
use velos_signal::SignalController;

/// Create a standard 2-phase plan for testing actuated control.
///
/// Phase 0: NS approaches (0,1) green 30s + amber 3s
/// Phase 1: EW approaches (2,3) green 25s + amber 3s
fn two_phase_plan() -> SignalPlan {
    SignalPlan::new(vec![
        SignalPhase {
            green_duration: 30.0,
            amber_duration: 3.0,
            approaches: vec![0, 1],
        },
        SignalPhase {
            green_duration: 25.0,
            amber_duration: 3.0,
            approaches: vec![2, 3],
        },
    ])
}

/// No detectors firing -- gap timer should count up.
fn no_detectors() -> Vec<DetectorReading> {
    vec![]
}

/// Detector firing on approach 0 (NS phase).
fn ns_detector_firing() -> Vec<DetectorReading> {
    vec![DetectorReading {
        detector_index: 0,
        triggered: true,
    }]
}

/// Detector firing on approach 2 (EW phase).
fn ew_detector_firing() -> Vec<DetectorReading> {
    vec![DetectorReading {
        detector_index: 2,
        triggered: true,
    }]
}

#[test]
fn min_green_holds_even_with_no_detections() {
    // Gap-out threshold = 3s, min_green = 7s.
    // Even with no detectors, should stay green for at least 7s.
    let plan = two_phase_plan();
    let mut ctrl = ActuatedController::new_with_params(plan, 4, 7.0, 60.0, 3.0);

    // Tick 5s with no detectors -- gap timer exceeds 3s but min green (7s) holds
    ctrl.tick(5.0, &no_detectors());
    assert_eq!(
        ctrl.get_phase_state(0),
        PhaseState::Green,
        "NS should still be green at 5s (min green = 7s)"
    );
}

#[test]
fn gap_out_triggers_transition_after_min_green() {
    let plan = two_phase_plan();
    let mut ctrl = ActuatedController::new_with_params(plan, 4, 7.0, 60.0, 3.0);

    // Tick 8s with no detectors -- past min_green (7s) and gap_timer (8s) > threshold (3s)
    // Should transition to amber on NS, then to EW phase
    ctrl.tick(8.0, &no_detectors());

    // After gap-out at 7s (when min_green met AND gap >= 3s), phase transitions.
    // At 8s we should be in amber for NS (1s into amber) or already transitioned.
    // The gap-out fires when phase_active_time >= min_green AND gap_timer >= gap_threshold.
    // So at tick 8.0 with no detectors, gap_timer = 8.0 >= 3.0, phase_active_time = 8.0 >= 7.0.
    // After transition, NS goes to amber/red and EW starts.
    assert_ne!(
        ctrl.get_phase_state(0),
        PhaseState::Green,
        "NS should no longer be green after gap-out"
    );
}

#[test]
fn detector_resets_gap_timer_extends_green() {
    let plan = two_phase_plan();
    let mut ctrl = ActuatedController::new_with_params(plan, 4, 7.0, 60.0, 3.0);

    // Tick 6s without detectors, then fire detector, then tick 2 more seconds
    ctrl.tick(6.0, &no_detectors());
    // Now at 6s, gap_timer = 6s. Fire detector to reset it.
    ctrl.tick(1.0, &ns_detector_firing());
    // gap_timer reset. Now at 7s total, gap_timer = 0 (just reset). min_green met.
    // Tick 2 more seconds -- gap_timer = 2s < 3s threshold, should still be green
    ctrl.tick(2.0, &no_detectors());
    assert_eq!(
        ctrl.get_phase_state(0),
        PhaseState::Green,
        "NS should still be green at 9s because detector reset gap timer at 7s"
    );
}

#[test]
fn max_green_forces_transition_despite_detectors() {
    let plan = two_phase_plan();
    let mut ctrl = ActuatedController::new_with_params(plan, 4, 7.0, 60.0, 3.0);

    // Keep firing detectors every second for 61 seconds
    for _ in 0..61 {
        ctrl.tick(1.0, &ns_detector_firing());
    }

    // At 61s, max_green (60s) should have forced transition
    assert_ne!(
        ctrl.get_phase_state(0),
        PhaseState::Green,
        "NS should not be green after 61s -- max_green is 60s"
    );
}

#[test]
fn after_transition_next_phase_starts_fresh() {
    let plan = two_phase_plan();
    let mut ctrl = ActuatedController::new_with_params(plan, 4, 7.0, 60.0, 3.0);

    // Force gap-out on NS: tick 11s with no detectors (min_green=7 met, gap>=3)
    ctrl.tick(11.0, &no_detectors());

    // NS should have transitioned. Now EW should be active.
    // The exact timing depends on amber duration, but EW approach should
    // eventually become green.
    // Tick through amber (3s)
    ctrl.tick(3.0, &no_detectors());

    assert_eq!(
        ctrl.get_phase_state(2),
        PhaseState::Green,
        "EW should be green after NS gap-out + amber"
    );
}

#[test]
fn implements_signal_controller_trait() {
    let plan = two_phase_plan();
    let mut ctrl = ActuatedController::new(plan, 4);

    // Use trait methods
    let _state = ctrl.get_phase_state(0);
    ctrl.tick(1.0, &no_detectors());
    ctrl.reset();
    assert_eq!(ctrl.get_phase_state(0), PhaseState::Green, "green after reset");
}

#[test]
fn detector_on_wrong_phase_does_not_prevent_gap_out() {
    let plan = two_phase_plan();
    let mut ctrl = ActuatedController::new_with_params(plan, 4, 7.0, 60.0, 3.0);

    // Fire EW detector while NS phase is active -- should not affect NS gap timer
    ctrl.tick(8.0, &ew_detector_firing());

    // NS should have gapped out (8s > min_green 7s, gap_timer 8s > 3s threshold)
    // EW detector doesn't reset NS gap timer
    assert_ne!(
        ctrl.get_phase_state(0),
        PhaseState::Green,
        "NS should gap out even when EW detector fires"
    );
}

#[test]
fn fixed_time_controller_implements_signal_controller() {
    use velos_signal::controller::FixedTimeController;

    let plan = two_phase_plan();
    let mut ctrl = FixedTimeController::new(plan, 4);

    // Use trait methods from SignalController
    let state = SignalController::get_phase_state(&ctrl, 0);
    assert_eq!(state, PhaseState::Green);

    SignalController::tick(&mut ctrl, 1.0, &no_detectors());
    SignalController::reset(&mut ctrl);
    assert_eq!(SignalController::get_phase_state(&ctrl, 0), PhaseState::Green);
}
