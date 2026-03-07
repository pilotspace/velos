//! BPR (Bureau of Public Roads) physics extrapolation model.
//!
//! Computes predicted travel times from current flow/capacity ratios:
//! `t = t_free * (1 + alpha * (V/C)^beta)`
//!
//! Standard coefficients: alpha = 0.15, beta = 4.0.

/// BPR predictor that computes travel times from volume/capacity ratios.
///
/// Uses the standard BPR function with beta=4.0 fast path (multiplication
/// instead of powf) for numerical stability and performance.
#[derive(Debug, Clone)]
pub struct BPRPredictor {
    alpha: f64,
    beta: f64,
}

impl BPRPredictor {
    /// Create a new predictor with standard BPR coefficients (alpha=0.15, beta=4.0).
    pub fn new() -> Self {
        Self {
            alpha: 0.15,
            beta: 4.0,
        }
    }

    /// Create a predictor with custom coefficients.
    pub fn with_params(alpha: f64, beta: f64) -> Self {
        Self { alpha, beta }
    }

    /// Predict travel times for all edges given current flows, capacities, and free-flow times.
    ///
    /// Formula per edge: `t = t_free * (1 + alpha * (V/C)^beta)`
    ///
    /// Negative flows are clamped to 0. Zero capacity edges return free-flow time.
    pub fn predict(
        &self,
        edge_flows: &[f32],
        edge_capacities: &[f32],
        edge_free_flow: &[f32],
    ) -> Vec<f32> {
        debug_assert_eq!(edge_flows.len(), edge_capacities.len());
        debug_assert_eq!(edge_flows.len(), edge_free_flow.len());

        edge_flows
            .iter()
            .zip(edge_capacities.iter())
            .zip(edge_free_flow.iter())
            .map(|((&flow, &cap), &t_free)| {
                if cap <= 0.0 {
                    return t_free;
                }
                let vc = (flow.max(0.0) / cap) as f64;
                let vc_pow = if (self.beta - 4.0).abs() < f64::EPSILON {
                    let vc_sq = vc * vc;
                    vc_sq * vc_sq
                } else {
                    vc.powf(self.beta)
                };
                (t_free as f64 * (1.0 + self.alpha * vc_pow)) as f32
            })
            .collect()
    }
}

impl Default for BPRPredictor {
    fn default() -> Self {
        Self::new()
    }
}
