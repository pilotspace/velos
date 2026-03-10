//! Demand calibration overlay using ArcSwap (mirrors PredictionStore pattern).
//!
//! Stores per-OD-pair calibration scaling factors that are atomically swapped
//! and lock-free readable by the Spawner. Calibration ratios are computed from
//! observed (detection) vs simulated agent counts, smoothed via EMA, and clamped.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::{ArcSwap, Guard};

use velos_demand::Zone;

use crate::aggregator::DetectionAggregator;
use crate::camera::CameraRegistry;

/// EMA smoothing factor: weight on the new raw ratio.
const EMA_ALPHA: f32 = 0.3;

/// Minimum simulated count threshold below which calibration is skipped.
const MIN_SIMULATED_THRESHOLD: u32 = 5;

/// Lower clamp bound for calibration ratio.
const RATIO_CLAMP_LOW: f32 = 0.5;

/// Upper clamp bound for calibration ratio.
const RATIO_CLAMP_HIGH: f32 = 2.0;

/// Immutable snapshot of calibration scaling factors per OD pair.
///
/// Created by `compute_calibration_factors` and atomically swapped
/// into a [`CalibrationStore`].
#[derive(Debug, Clone)]
pub struct CalibrationOverlay {
    /// Per OD-pair multiplicative scaling factor.
    pub factors: HashMap<(Zone, Zone), f32>,
    /// Simulation time when this overlay was computed.
    pub timestamp_sim_seconds: f64,
}

/// Thread-safe store for the current calibration overlay.
///
/// Uses [`ArcSwap`] for lock-free reads: concurrent readers are never
/// blocked by a writer swapping in a new overlay.
#[derive(Debug)]
pub struct CalibrationStore {
    inner: Arc<ArcSwap<CalibrationOverlay>>,
}

impl CalibrationStore {
    /// Create a new store with empty calibration factors.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ArcSwap::from_pointee(CalibrationOverlay {
                factors: HashMap::new(),
                timestamp_sim_seconds: 0.0,
            })),
        }
    }

    /// Get a guard to the current overlay (lock-free read).
    pub fn current(&self) -> Guard<Arc<CalibrationOverlay>> {
        self.inner.load()
    }

    /// Atomically replace the current overlay with a new one.
    pub fn swap(&self, new_overlay: CalibrationOverlay) {
        self.inner.store(Arc::new(new_overlay));
    }

    /// Create a cheap clone handle for sharing across threads.
    pub fn clone_handle(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Default for CalibrationStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-camera EMA state for calibration ratio tracking.
#[derive(Debug, Clone)]
pub struct CameraCalibrationState {
    /// Previous EMA-smoothed ratio (starts at 1.0 = no adjustment).
    pub previous_ratio: f32,
    /// Last observed count from detection aggregator.
    pub last_observed: u32,
    /// Last simulated count from ECS query.
    pub last_simulated: u32,
}

impl Default for CameraCalibrationState {
    fn default() -> Self {
        Self {
            previous_ratio: 1.0,
            last_observed: 0,
            last_simulated: 0,
        }
    }
}

/// Compute a single camera's calibration ratio with EMA smoothing and clamping.
///
/// Returns the smoothed, clamped ratio and updates the camera state.
pub fn compute_camera_ratio(
    observed: u32,
    simulated: u32,
    state: &mut CameraCalibrationState,
) -> f32 {
    state.last_observed = observed;
    state.last_simulated = simulated;

    if simulated <= MIN_SIMULATED_THRESHOLD {
        // Not enough simulated data to calibrate; keep previous ratio.
        return state.previous_ratio;
    }

    let raw_ratio = observed as f32 / simulated as f32;

    // EMA smoothing: new = alpha * raw + (1 - alpha) * previous
    let smoothed = EMA_ALPHA * raw_ratio + (1.0 - EMA_ALPHA) * state.previous_ratio;

    // Clamp after EMA
    let clamped = smoothed.clamp(RATIO_CLAMP_LOW, RATIO_CLAMP_HIGH);

    state.previous_ratio = clamped;
    clamped
}

/// Compute calibration factors for all OD pairs based on camera observations.
///
/// For each registered camera:
/// 1. Sum observed counts from the latest aggregation window (all vehicle classes).
/// 2. Sum simulated counts from `simulated_counts`.
/// 3. Compute EMA-smoothed, clamped ratio.
/// 4. Apply that ratio to all OD pairs whose origin or destination zone
///    has at least one edge covered by the camera (simplified heuristic).
///
/// When multiple cameras affect the same OD pair, the average ratio is used.
pub fn compute_calibration_factors(
    registry: &CameraRegistry,
    aggregator: &DetectionAggregator,
    simulated_counts: &HashMap<u32, u32>,
    camera_states: &mut HashMap<u32, CameraCalibrationState>,
    edge_to_zone: &HashMap<u32, Zone>,
    sim_time: f64,
) -> CalibrationOverlay {
    // Accumulator for OD pair ratios: (sum_of_ratios, count)
    let mut od_accum: HashMap<(Zone, Zone), (f32, u32)> = HashMap::new();

    for camera in registry.list() {
        let cam_id = camera.id;

        // Sum observed across all vehicle classes in latest window
        let total_observed: u32 = aggregator
            .latest_window(cam_id)
            .map(|w| w.counts.values().sum())
            .unwrap_or(0);

        // Sum simulated count for this camera
        let total_simulated = simulated_counts.get(&cam_id).copied().unwrap_or(0);

        let state = camera_states
            .entry(cam_id)
            .or_insert_with(CameraCalibrationState::default);

        let ratio = compute_camera_ratio(total_observed, total_simulated, state);

        // Map camera ratio to OD pairs via covered edges -> zones
        let mut zones_for_camera: Vec<Zone> = Vec::new();
        for &edge_id in &camera.covered_edges {
            if let Some(&zone) = edge_to_zone.get(&edge_id) {
                zones_for_camera.push(zone);
            }
        }
        zones_for_camera.sort_unstable_by_key(|z| *z as u8);
        zones_for_camera.dedup();

        // Apply ratio to all OD pairs involving any of these zones
        // (origin OR destination matches a covered zone)
        for &zone in &zones_for_camera {
            // For each zone, we affect all OD pairs where zone is origin or destination.
            // To avoid needing the full OD matrix here, we just record zone-level ratios
            // and build OD pairs from all zone combinations.
            for &other_zone in &zones_for_camera {
                let entry = od_accum.entry((zone, other_zone)).or_insert((0.0, 0));
                entry.0 += ratio;
                entry.1 += 1;
            }
        }
    }

    // Average ratios when multiple cameras affect the same OD pair
    let factors: HashMap<(Zone, Zone), f32> = od_accum
        .into_iter()
        .map(|(key, (sum, count))| (key, sum / count as f32))
        .collect();

    CalibrationOverlay {
        factors,
        timestamp_sim_seconds: sim_time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_new_starts_with_empty_factors() {
        let store = CalibrationStore::new();
        let overlay = store.current();
        assert!(overlay.factors.is_empty());
        assert_eq!(overlay.timestamp_sim_seconds, 0.0);
    }

    #[test]
    fn store_swap_replaces_overlay_atomically() {
        let store = CalibrationStore::new();

        let mut factors = HashMap::new();
        factors.insert((Zone::BenThanh, Zone::NguyenHue), 1.5);
        store.swap(CalibrationOverlay {
            factors,
            timestamp_sim_seconds: 100.0,
        });

        let overlay = store.current();
        assert_eq!(overlay.factors.len(), 1);
        assert_eq!(
            overlay.factors.get(&(Zone::BenThanh, Zone::NguyenHue)),
            Some(&1.5)
        );
        assert_eq!(overlay.timestamp_sim_seconds, 100.0);
    }

    #[test]
    fn store_clone_handle_shares_same_underlying() {
        let store = CalibrationStore::new();
        let handle = store.clone_handle();

        let mut factors = HashMap::new();
        factors.insert((Zone::District1, Zone::District3), 0.8);
        store.swap(CalibrationOverlay {
            factors,
            timestamp_sim_seconds: 50.0,
        });

        // handle should see the same swap
        let overlay = handle.current();
        assert_eq!(
            overlay.factors.get(&(Zone::District1, Zone::District3)),
            Some(&0.8)
        );
    }

    #[test]
    fn ratio_observed_100_simulated_80_gives_1_25_smoothed() {
        // raw = 100/80 = 1.25
        // EMA: 0.3 * 1.25 + 0.7 * 1.0 = 0.375 + 0.7 = 1.075
        let mut state = CameraCalibrationState::default();
        let ratio = compute_camera_ratio(100, 80, &mut state);
        let expected = 0.3 * 1.25 + 0.7 * 1.0;
        assert!(
            (ratio - expected).abs() < 0.001,
            "expected {expected}, got {ratio}"
        );
    }

    #[test]
    fn ratio_clamped_to_2_0_when_observed_much_greater() {
        // Start with previous_ratio near max to test clamping
        let mut state = CameraCalibrationState {
            previous_ratio: 1.9,
            last_observed: 0,
            last_simulated: 0,
        };
        // raw = 1000/10 = 100.0
        // EMA: 0.3 * 100.0 + 0.7 * 1.9 = 30.0 + 1.33 = 31.33 -> clamped to 2.0
        let ratio = compute_camera_ratio(1000, 10, &mut state);
        assert_eq!(ratio, 2.0);
    }

    #[test]
    fn ratio_clamped_to_0_5_when_observed_much_less() {
        let mut state = CameraCalibrationState {
            previous_ratio: 0.6,
            last_observed: 0,
            last_simulated: 0,
        };
        // raw = 1/1000 = 0.001
        // EMA: 0.3 * 0.001 + 0.7 * 0.6 = 0.0003 + 0.42 = 0.4203 -> clamped to 0.5
        let ratio = compute_camera_ratio(1, 1000, &mut state);
        assert_eq!(ratio, 0.5);
    }

    #[test]
    fn simulated_zero_defaults_ratio_to_1_0() {
        let mut state = CameraCalibrationState::default();
        let ratio = compute_camera_ratio(100, 0, &mut state);
        assert_eq!(ratio, 1.0);
    }

    #[test]
    fn simulated_below_threshold_defaults_ratio_to_previous() {
        let mut state = CameraCalibrationState {
            previous_ratio: 1.3,
            last_observed: 0,
            last_simulated: 0,
        };
        // simulated=5 is at threshold (<=5), should skip
        let ratio = compute_camera_ratio(100, 5, &mut state);
        assert_eq!(ratio, 1.3);
    }

    #[test]
    fn ema_smoothing_computed_correctly() {
        // previous = 1.0, raw = 1.5, alpha = 0.3
        // new = 0.3 * 1.5 + 0.7 * 1.0 = 0.45 + 0.70 = 1.15
        let mut state = CameraCalibrationState::default(); // previous = 1.0
        let ratio = compute_camera_ratio(150, 100, &mut state);
        let expected = 0.3 * 1.5 + 0.7 * 1.0;
        assert!(
            (ratio - expected).abs() < 0.001,
            "expected {expected}, got {ratio}"
        );
    }

    #[test]
    fn ema_applied_before_clamping() {
        // If EMA were applied AFTER clamping, we'd get a different result.
        // raw = 5.0, previous = 1.0
        // Correct (EMA first): 0.3 * 5.0 + 0.7 * 1.0 = 2.2 -> clamp to 2.0
        // Wrong (clamp first): clamp(5.0) = 2.0 -> EMA: 0.3*2.0 + 0.7*1.0 = 1.3
        let mut state = CameraCalibrationState::default();
        let ratio = compute_camera_ratio(50, 10, &mut state);
        assert_eq!(ratio, 2.0, "EMA should be applied before clamping");
    }
}
