//! velos-predict: BPR + ETS + historical prediction ensemble with ArcSwap overlay.
//!
//! Provides predicted future travel times that feed into the cost function
//! for prediction-informed routing. Updates every 60 sim-seconds without
//! blocking simulation via lock-free ArcSwap reads.

pub mod bpr;
pub mod ets;
