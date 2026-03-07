//! velos-predict: BPR + ETS + historical prediction ensemble with ArcSwap overlay.
//!
//! Provides predicted future travel times that feed into the cost function
//! for prediction-informed routing. Updates every 60 sim-seconds without
//! blocking simulation via lock-free ArcSwap reads.

pub mod adaptive;
pub mod bpr;
pub mod ets;
pub mod historical;
pub mod overlay;

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

/// Input data for a prediction update cycle.
#[derive(Debug)]
pub struct PredictionInput<'a> {
    /// Current flow per edge.
    pub flows: &'a [f32],
    /// Capacity per edge.
    pub capacities: &'a [f32],
    /// Free-flow travel time per edge (seconds).
    pub free_flow: &'a [f32],
    /// Most recent actual travel time per edge (seconds).
    pub actual: &'a [f32],
    /// Current hour of day (0..23).
    pub hour: u8,
    /// Day type: 0=weekday, 1=saturday, 2=sunday, 3=holiday.
    pub day_type: u8,
}

/// High-level prediction service that owns the ensemble and overlay store.
///
/// Call [`PredictionService::update`] every 60 sim-seconds to recompute
/// predictions and atomically swap the overlay. Readers access the store
/// via [`PredictionService::store`] for lock-free reads.
#[derive(Debug)]
pub struct PredictionService {
    ensemble: PredictionEnsemble,
    store: overlay::PredictionStore,
    update_interval_sim_seconds: f64,
    last_update_sim_seconds: f64,
}

impl PredictionService {
    /// Create a new prediction service for the given edges.
    pub fn new(edge_count: usize, free_flow: &[f32]) -> Self {
        Self {
            ensemble: PredictionEnsemble::new(edge_count),
            store: overlay::PredictionStore::new(edge_count, free_flow),
            update_interval_sim_seconds: 60.0,
            last_update_sim_seconds: 0.0,
        }
    }

    /// Check whether it is time to recompute predictions.
    pub fn should_update(&self, sim_time: f64) -> bool {
        sim_time - self.last_update_sim_seconds >= self.update_interval_sim_seconds
    }

    /// Recompute predictions and atomically swap the overlay.
    ///
    /// Should be called when [`should_update`](Self::should_update) returns true.
    pub fn update(&mut self, input: &PredictionInput<'_>, sim_time: f64) {
        let (travel_times, confidence) = self.ensemble.compute(
            input.flows,
            input.capacities,
            input.free_flow,
            input.actual,
            input.hour,
            input.day_type,
        );

        self.store.swap(overlay::PredictionOverlay {
            edge_travel_times: travel_times,
            edge_confidence: confidence,
            timestamp_sim_seconds: sim_time,
        });

        self.last_update_sim_seconds = sim_time;
    }

    /// Access the prediction store for lock-free reads.
    pub fn store(&self) -> &overlay::PredictionStore {
        &self.store
    }

    /// Access the ensemble (e.g., to record historical observations).
    pub fn ensemble_mut(&mut self) -> &mut PredictionEnsemble {
        &mut self.ensemble
    }
}
