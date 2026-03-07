//! Adaptive weight adjustment based on prediction error.
//!
//! Shifts weight toward the model with lowest recent prediction error.
//! Minimum weight capped at 0.05 to prevent zeroing any model.

/// Minimum weight for any single model to prevent complete silencing.
const MIN_WEIGHT: f32 = 0.05;

/// Adaptive weights for the BPR/ETS/historical ensemble.
///
/// After each update cycle, weights shift toward the model with lowest
/// error using a softmax-like adjustment. Weights always sum to 1.0.
#[derive(Debug, Clone)]
pub struct AdaptiveWeights {
    /// Current BPR model weight (initial 0.40).
    pub bpr_weight: f32,
    /// Current ETS model weight (initial 0.35).
    pub ets_weight: f32,
    /// Current historical model weight (initial 0.25).
    pub hist_weight: f32,
    /// Learning rate per update cycle.
    learning_rate: f32,
}

impl AdaptiveWeights {
    /// Create with default initial weights: BPR=0.40, ETS=0.35, Historical=0.25.
    pub fn new() -> Self {
        Self {
            bpr_weight: 0.40,
            ets_weight: 0.35,
            hist_weight: 0.25,
            learning_rate: 0.05,
        }
    }

    /// Update weights based on mean absolute errors from each model.
    ///
    /// Lower error = higher weight. Uses inverse-error softmax with learning rate
    /// to smoothly shift weights. Minimum weight capped at [`MIN_WEIGHT`].
    pub fn update(&mut self, bpr_error: f32, ets_error: f32, hist_error: f32) {
        // Avoid division by zero: add small epsilon
        let eps = 1e-6_f32;
        let inv_bpr = 1.0 / (bpr_error + eps);
        let inv_ets = 1.0 / (ets_error + eps);
        let inv_hist = 1.0 / (hist_error + eps);
        let inv_sum = inv_bpr + inv_ets + inv_hist;

        // Target weights from inverse-error distribution
        let target_bpr = inv_bpr / inv_sum;
        let target_ets = inv_ets / inv_sum;
        let target_hist = inv_hist / inv_sum;

        // Smoothly move toward target
        self.bpr_weight += self.learning_rate * (target_bpr - self.bpr_weight);
        self.ets_weight += self.learning_rate * (target_ets - self.ets_weight);
        self.hist_weight += self.learning_rate * (target_hist - self.hist_weight);

        // Enforce minimum weight
        self.bpr_weight = self.bpr_weight.max(MIN_WEIGHT);
        self.ets_weight = self.ets_weight.max(MIN_WEIGHT);
        self.hist_weight = self.hist_weight.max(MIN_WEIGHT);

        // Renormalize to sum to 1.0
        let sum = self.bpr_weight + self.ets_weight + self.hist_weight;
        self.bpr_weight /= sum;
        self.ets_weight /= sum;
        self.hist_weight /= sum;
    }

    /// Blend predictions from three models using current weights.
    ///
    /// Returns weighted average: `bpr_w * bpr[i] + ets_w * ets[i] + hist_w * hist[i]`.
    pub fn blend(&self, bpr: &[f32], ets: &[f32], hist: &[f32]) -> Vec<f32> {
        debug_assert_eq!(bpr.len(), ets.len());
        debug_assert_eq!(bpr.len(), hist.len());

        bpr.iter()
            .zip(ets.iter())
            .zip(hist.iter())
            .map(|((&b, &e), &h)| self.bpr_weight * b + self.ets_weight * e + self.hist_weight * h)
            .collect()
    }
}

impl Default for AdaptiveWeights {
    fn default() -> Self {
        Self::new()
    }
}
