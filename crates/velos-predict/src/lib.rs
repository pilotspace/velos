//! velos-predict: BPR + ETS + historical prediction ensemble with ArcSwap overlay.
//!
//! Provides predicted future travel times that feed into the cost function
//! for prediction-informed routing. Updates every 60 sim-seconds without
//! blocking simulation via lock-free ArcSwap reads.

pub mod adaptive;
pub mod bpr;
pub mod ets;
pub mod historical;

use adaptive::AdaptiveWeights;
use bpr::BPRPredictor;
use ets::ETSCorrector;
use historical::HistoricalMatcher;

/// Prediction ensemble that blends BPR, ETS, and historical models.
///
/// Each update cycle:
/// 1. BPR computes physics-based travel times from flow/capacity
/// 2. ETS applies smoothed error correction to BPR predictions
/// 3. Historical provides time-of-day pattern match
/// 4. Adaptive weights blend the three predictions
/// 5. Confidence is derived from inter-model disagreement
#[derive(Debug)]
pub struct PredictionEnsemble {
    bpr: BPRPredictor,
    ets: ETSCorrector,
    historical: HistoricalMatcher,
    weights: AdaptiveWeights,
    edge_count: usize,
}

impl PredictionEnsemble {
    /// Create a new ensemble for the given number of edges.
    pub fn new(edge_count: usize) -> Self {
        Self {
            bpr: BPRPredictor::new(),
            ets: ETSCorrector::new(edge_count),
            historical: HistoricalMatcher::new(edge_count),
            weights: AdaptiveWeights::new(),
            edge_count,
        }
    }

    /// Access the historical matcher for recording observations.
    pub fn historical_mut(&mut self) -> &mut HistoricalMatcher {
        &mut self.historical
    }

    /// Access current adaptive weights.
    pub fn weights(&self) -> &AdaptiveWeights {
        &self.weights
    }

    /// Compute blended predictions and per-edge confidence scores.
    ///
    /// Returns `(predicted_travel_times, confidence)` where confidence
    /// is `1.0 - normalized_disagreement` across the three models.
    pub fn compute(
        &mut self,
        flows: &[f32],
        capacities: &[f32],
        free_flow: &[f32],
        actual: &[f32],
        hour: u8,
        day_type: u8,
    ) -> (Vec<f32>, Vec<f32>) {
        debug_assert_eq!(flows.len(), self.edge_count);
        debug_assert_eq!(capacities.len(), self.edge_count);
        debug_assert_eq!(free_flow.len(), self.edge_count);
        debug_assert_eq!(actual.len(), self.edge_count);

        // Step 1: BPR physics prediction
        let bpr_preds = self.bpr.predict(flows, capacities, free_flow);

        // Step 2: ETS correction
        let ets_preds = self.ets.predict(&bpr_preds, actual);

        // Step 3: Historical pattern match
        let hist_preds = self.historical.predict(hour, day_type, free_flow);

        // Step 4: Update adaptive weights based on prediction errors
        let bpr_error = mean_abs_error(&bpr_preds, actual);
        let ets_error = mean_abs_error(&ets_preds, actual);
        let hist_error = mean_abs_error(&hist_preds, actual);
        self.weights.update(bpr_error, ets_error, hist_error);

        // Step 5: Blend predictions
        let blended = self.weights.blend(&bpr_preds, &ets_preds, &hist_preds);

        // Step 6: Compute confidence from inter-model disagreement
        let confidence = compute_confidence(&bpr_preds, &ets_preds, &hist_preds);

        (blended, confidence)
    }
}

/// Mean absolute error between predictions and actuals.
fn mean_abs_error(predictions: &[f32], actuals: &[f32]) -> f32 {
    if predictions.is_empty() {
        return 0.0;
    }
    let sum: f32 = predictions
        .iter()
        .zip(actuals.iter())
        .map(|(&p, &a)| (p - a).abs())
        .sum();
    sum / predictions.len() as f32
}

/// Per-edge confidence: 1.0 - normalized disagreement.
///
/// Disagreement = range of the three predictions relative to their mean.
/// High agreement = high confidence.
fn compute_confidence(bpr: &[f32], ets: &[f32], hist: &[f32]) -> Vec<f32> {
    bpr.iter()
        .zip(ets.iter())
        .zip(hist.iter())
        .map(|((&b, &e), &h)| {
            let mean = (b + e + h) / 3.0;
            if mean <= 0.0 {
                return 1.0;
            }
            let max = b.max(e).max(h);
            let min = b.min(e).min(h);
            let range = max - min;
            // Normalize by mean; cap at 1.0 disagreement
            let disagreement = (range / mean).min(1.0);
            1.0 - disagreement
        })
        .collect()
}
