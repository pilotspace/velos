//! Tests for the velos-predict prediction ensemble.

use velos_predict::bpr::BPRPredictor;
use velos_predict::ets::ETSCorrector;

// ── BPR tests ──────────────────────────────────────────────────────

#[test]
fn bpr_zero_flow_returns_free_flow() {
    let bpr = BPRPredictor::new();
    let flows = [0.0_f32];
    let capacities = [100.0_f32];
    let free_flow = [60.0_f32];
    let result = bpr.predict(&flows, &capacities, &free_flow);
    assert!((result[0] - 60.0).abs() < 1e-6, "expected 60.0, got {}", result[0]);
}

#[test]
fn bpr_flow_equals_capacity() {
    // V/C = 1.0 -> t = t_free * (1 + 0.15 * 1^4) = t_free * 1.15
    let bpr = BPRPredictor::new();
    let flows = [100.0_f32];
    let capacities = [100.0_f32];
    let free_flow = [60.0_f32];
    let result = bpr.predict(&flows, &capacities, &free_flow);
    let expected = 60.0 * 1.15;
    assert!((result[0] - expected).abs() < 1e-4, "expected {expected}, got {}", result[0]);
}

#[test]
fn bpr_flow_double_capacity() {
    // V/C = 2.0 -> t = t_free * (1 + 0.15 * 16) = t_free * 3.4
    let bpr = BPRPredictor::new();
    let flows = [200.0_f32];
    let capacities = [100.0_f32];
    let free_flow = [60.0_f32];
    let result = bpr.predict(&flows, &capacities, &free_flow);
    let expected = 60.0 * 3.4;
    assert!((result[0] - expected).abs() < 1e-3, "expected {expected}, got {}", result[0]);
}

#[test]
fn bpr_negative_flow_clamps_to_zero() {
    let bpr = BPRPredictor::new();
    let flows = [-50.0_f32];
    let capacities = [100.0_f32];
    let free_flow = [60.0_f32];
    let result = bpr.predict(&flows, &capacities, &free_flow);
    // Negative flow clamped to 0 -> returns free-flow
    assert!((result[0] - 60.0).abs() < 1e-6, "expected 60.0, got {}", result[0]);
}

// ── ETS tests ──────────────────────────────────────────────────────

#[test]
fn ets_zero_correction_passes_through() {
    let mut ets = ETSCorrector::new(1);
    // First call with actual == bpr_pred -> correction stays 0 -> output == bpr_pred
    let bpr_preds = [60.0_f32];
    let actuals = [60.0_f32];
    let result = ets.predict(&bpr_preds, &actuals);
    assert!((result[0] - 60.0).abs() < 1e-6, "expected 60.0, got {}", result[0]);
}

#[test]
fn ets_positive_error_increases_correction() {
    let mut ets = ETSCorrector::new(1);
    // Actual > predicted: error is positive, correction should increase
    let bpr_preds = [60.0_f32];
    let actuals = [70.0_f32]; // error = 10

    let r1 = ets.predict(&bpr_preds, &actuals);
    // correction = 0.3 * 10 + 0.7 * 0 = 3.0, output = 60 + 3 = 63
    assert!(r1[0] > 60.0, "correction should increase prediction");

    let r2 = ets.predict(&bpr_preds, &actuals);
    // correction = 0.3 * 10 + 0.7 * 3.0 = 5.1, output = 60 + 5.1 = 65.1
    assert!(r2[0] > r1[0], "correction should continue increasing with consistent error");
}

#[test]
fn ets_converges_toward_actual() {
    let mut ets = ETSCorrector::new(1);
    let bpr_preds = [60.0_f32];
    let actuals = [70.0_f32];

    let mut prev = 60.0_f32;
    for _ in 0..50 {
        let result = ets.predict(&bpr_preds, &actuals);
        assert!(result[0] >= prev || (result[0] - prev).abs() < 0.01);
        prev = result[0];
    }
    // After many iterations, should be close to actual
    assert!((prev - 70.0).abs() < 0.5, "expected convergence to ~70, got {prev}");
}

#[test]
fn ets_gamma_formula() {
    // gamma=0.3: correction = 0.3*error + 0.7*prev_correction
    let mut ets = ETSCorrector::new(1);
    let bpr_preds = [60.0_f32];
    let actuals = [70.0_f32]; // error = 10

    // Step 1: correction = 0.3 * 10 + 0.7 * 0 = 3.0
    let r1 = ets.predict(&bpr_preds, &actuals);
    assert!((r1[0] - 63.0).abs() < 1e-4, "expected 63.0, got {}", r1[0]);

    // Step 2: correction = 0.3 * 10 + 0.7 * 3.0 = 5.1
    let r2 = ets.predict(&bpr_preds, &actuals);
    assert!((r2[0] - 65.1).abs() < 1e-4, "expected 65.1, got {}", r2[0]);
}
