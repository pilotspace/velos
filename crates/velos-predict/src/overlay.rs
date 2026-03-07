//! PredictionOverlay with ArcSwap for lock-free concurrent reads.
//!
//! The overlay contains predicted travel times and confidence scores.
//! Writers swap in new overlays atomically; readers get a guard that
//! remains valid even after a subsequent swap (no data corruption).

use std::sync::Arc;

use arc_swap::{ArcSwap, Guard};

/// Snapshot of predicted travel times and confidence for all edges.
///
/// Immutable once created -- new predictions produce a new overlay
/// that is atomically swapped into the [`PredictionStore`].
#[derive(Debug, Clone)]
pub struct PredictionOverlay {
    /// Predicted travel time per edge (seconds).
    pub edge_travel_times: Vec<f32>,
    /// Confidence per edge in [0.0, 1.0].
    pub edge_confidence: Vec<f32>,
    /// Simulation time when this overlay was computed.
    pub timestamp_sim_seconds: f64,
}

/// Thread-safe store for the current prediction overlay.
///
/// Uses [`ArcSwap`] for lock-free reads: concurrent readers are never
/// blocked by a writer swapping in a new overlay. Guards from `current()`
/// hold a reference to the overlay that was active at read time.
#[derive(Debug)]
pub struct PredictionStore {
    inner: Arc<ArcSwap<PredictionOverlay>>,
}

impl PredictionStore {
    /// Create a new store initialized with free-flow travel times and full confidence.
    pub fn new(edge_count: usize, free_flow: &[f32]) -> Self {
        debug_assert_eq!(free_flow.len(), edge_count);
        let initial = PredictionOverlay {
            edge_travel_times: free_flow.to_vec(),
            edge_confidence: vec![1.0; edge_count],
            timestamp_sim_seconds: 0.0,
        };
        Self {
            inner: Arc::new(ArcSwap::from_pointee(initial)),
        }
    }

    /// Get a guard to the current overlay (lock-free read).
    ///
    /// The guard keeps the overlay alive even if a swap occurs while
    /// the guard is held.
    pub fn current(&self) -> Guard<Arc<PredictionOverlay>> {
        self.inner.load()
    }

    /// Atomically replace the current overlay with a new one.
    pub fn swap(&self, new_overlay: PredictionOverlay) {
        self.inner.store(Arc::new(new_overlay));
    }

    /// Create a cheap clone handle for sharing across threads.
    pub fn clone_handle(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}
