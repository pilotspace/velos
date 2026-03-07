//! ETS (Exponential Smoothing) correction model.
//!
//! Tracks smoothed prediction error and applies correction to BPR predictions.
//! `correction[i] = gamma * error + (1 - gamma) * correction[i]`
//! `output = bpr_pred + correction`

/// Exponential smoothing corrector that tracks prediction error over time.
///
/// Each edge maintains a running correction that smoothly adapts toward
/// the actual-vs-predicted gap. Gamma controls responsiveness:
/// - Higher gamma (closer to 1.0) = faster adaptation, more noise
/// - Lower gamma (closer to 0.0) = slower adaptation, smoother
#[derive(Debug, Clone)]
pub struct ETSCorrector {
    correction: Vec<f32>,
    gamma: f32,
}

impl ETSCorrector {
    /// Create a new corrector with default gamma=0.3 for the given number of edges.
    pub fn new(edge_count: usize) -> Self {
        Self {
            correction: vec![0.0; edge_count],
            gamma: 0.3,
        }
    }

    /// Create a corrector with custom gamma.
    pub fn with_gamma(edge_count: usize, gamma: f32) -> Self {
        Self {
            correction: vec![0.0; edge_count],
            gamma,
        }
    }

    /// Predict corrected travel times.
    ///
    /// Updates the internal correction based on observed error, then returns
    /// BPR predictions adjusted by the smoothed correction.
    ///
    /// For each edge:
    /// 1. error = actual - bpr_pred
    /// 2. correction = gamma * error + (1 - gamma) * prev_correction
    /// 3. output = bpr_pred + correction
    pub fn predict(
        &mut self,
        bpr_predictions: &[f32],
        actual_travel_times: &[f32],
    ) -> Vec<f32> {
        debug_assert_eq!(bpr_predictions.len(), actual_travel_times.len());
        debug_assert_eq!(bpr_predictions.len(), self.correction.len());

        bpr_predictions
            .iter()
            .zip(actual_travel_times.iter())
            .zip(self.correction.iter_mut())
            .map(|((&bpr_pred, &actual), corr)| {
                let error = actual - bpr_pred;
                *corr = self.gamma * error + (1.0 - self.gamma) * *corr;
                bpr_pred + *corr
            })
            .collect()
    }

    /// Reset all corrections to zero.
    pub fn reset(&mut self) {
        self.correction.fill(0.0);
    }

    /// Current correction values (read-only).
    pub fn corrections(&self) -> &[f32] {
        &self.correction
    }
}
