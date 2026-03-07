//! Tests for LoopDetector virtual point sensor.

use velos_signal::detector::{DetectorReading, LoopDetector};

#[test]
fn agent_crossing_detector_triggers() {
    let det = LoopDetector::new(1, 50.0);
    // Agent moves from 45m to 55m, crossing the 50m sensor
    assert!(det.check(45.0, 55.0), "should trigger when agent crosses sensor");
}

#[test]
fn agent_arriving_exactly_at_detector_triggers() {
    let det = LoopDetector::new(1, 50.0);
    // Agent moves from 45m to exactly 50m
    assert!(det.check(45.0, 50.0), "should trigger when agent arrives exactly at sensor");
}

#[test]
fn agent_before_detector_does_not_trigger() {
    let det = LoopDetector::new(1, 50.0);
    // Agent moves from 40m to 45m, still before the 50m sensor
    assert!(!det.check(40.0, 45.0), "should not trigger when agent is before sensor");
}

#[test]
fn agent_already_past_detector_does_not_trigger() {
    let det = LoopDetector::new(1, 50.0);
    // Agent is already past the sensor and moves further
    assert!(!det.check(55.0, 60.0), "should not trigger when agent already past sensor");
}

#[test]
fn agent_stationary_on_detector_does_not_trigger() {
    let det = LoopDetector::new(1, 50.0);
    // Agent is stopped exactly on the detector
    assert!(!det.check(50.0, 50.0), "should not trigger when agent stationary on sensor");
}

#[test]
fn backward_movement_does_not_trigger() {
    let det = LoopDetector::new(1, 50.0);
    // Agent moves backward across the sensor point
    assert!(!det.check(55.0, 45.0), "should not trigger on backward movement");
}

#[test]
fn detector_reading_equality() {
    let r1 = DetectorReading {
        detector_index: 0,
        triggered: true,
    };
    let r2 = DetectorReading {
        detector_index: 0,
        triggered: true,
    };
    assert_eq!(r1, r2);
}

#[test]
fn detector_at_zero_offset() {
    let det = LoopDetector::new(1, 0.0);
    // Agent starting before 0 and crossing
    assert!(det.check(-1.0, 0.5), "should trigger crossing at offset 0");
    // Agent already at or past 0
    assert!(!det.check(0.0, 1.0), "should not trigger when starting at offset");
}
