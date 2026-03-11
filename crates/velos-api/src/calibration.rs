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

/// Minimum observed count per camera to participate in calibration.
/// Cameras with fewer observations are skipped (returns previous ratio).
pub const MIN_OBSERVED_THRESHOLD: u32 = 10;

/// Maximum per-step change in OD factor between consecutive calibration overlays.
pub const MAX_FACTOR_CHANGE_PER_STEP: f32 = 0.2;

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
    /// Number of consecutive calibration cycles where this camera had no new window.
    pub consecutive_stale_windows: u32,
    /// Start timestamp (ms) of the last processed aggregation window for this camera.
    /// -1 indicates no window has been processed yet (late-connecting camera).
    pub last_window_start_ms: i64,
}

impl Default for CameraCalibrationState {
    fn default() -> Self {
        Self {
            previous_ratio: 1.0,
            last_observed: 0,
            last_simulated: 0,
            consecutive_stale_windows: 0,
            last_window_start_ms: -1,
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

    if observed < MIN_OBSERVED_THRESHOLD {
        // Not enough observed data to calibrate; keep previous ratio.
        return state.previous_ratio;
    }

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

/// Decay a stale camera's ratio toward 1.0 (baseline = no adjustment).
///
/// Only activates when the camera has been stale for 3 or more consecutive
/// calibration windows. The decay rate increases with each additional stale
/// window, moving the ratio progressively toward 1.0 until it converges.
pub fn decay_toward_baseline(state: &mut CameraCalibrationState) {
    if state.consecutive_stale_windows < 3 {
        return;
    }

    let decay = (0.1 * (state.consecutive_stale_windows - 2) as f32).min(1.0);
    // Move toward 1.0 regardless of direction
    state.previous_ratio += (1.0 - state.previous_ratio) * decay;
}

/// Cap per-step OD factor changes to prevent large jumps between calibration
/// cycles.
///
/// For each factor in the new overlay, if an old factor exists for that OD
/// pair, the delta is clamped to `[-MAX_FACTOR_CHANGE_PER_STEP, +MAX_FACTOR_CHANGE_PER_STEP]`.
/// First-time factors (no old value) are left uncapped.
pub fn apply_change_cap(
    old_factors: &HashMap<(Zone, Zone), f32>,
    new_overlay: &mut CalibrationOverlay,
) {
    for (key, new_factor) in new_overlay.factors.iter_mut() {
        if let Some(&old_factor) = old_factors.get(key) {
            let delta = *new_factor - old_factor;
            let clamped_delta =
                delta.clamp(-MAX_FACTOR_CHANGE_PER_STEP, MAX_FACTOR_CHANGE_PER_STEP);
            *new_factor = old_factor + clamped_delta;
        }
        // If no old_factor exists, leave uncapped (first calibration)
    }
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
            .or_default();

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
            ..Default::default()
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
            ..Default::default()
        };
        // raw = 10/1000 = 0.01
        // EMA: 0.3 * 0.01 + 0.7 * 0.6 = 0.003 + 0.42 = 0.423 -> clamped to 0.5
        let ratio = compute_camera_ratio(10, 1000, &mut state);
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
            ..Default::default()
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

    // --- Task 1: New stability safeguard tests ---

    #[test]
    fn default_state_has_staleness_fields() {
        let state = CameraCalibrationState::default();
        assert_eq!(state.consecutive_stale_windows, 0);
        assert_eq!(state.last_window_start_ms, -1);
    }

    #[test]
    fn min_observation_threshold_skips_low_observed() {
        // observed < MIN_OBSERVED_THRESHOLD (10) should return previous_ratio
        let mut state = CameraCalibrationState {
            previous_ratio: 1.4,
            ..Default::default()
        };
        let ratio = compute_camera_ratio(9, 100, &mut state);
        assert_eq!(ratio, 1.4, "observed < 10 should skip calibration");
    }

    #[test]
    fn min_observation_threshold_allows_at_threshold() {
        // observed == 10 should NOT skip
        let mut state = CameraCalibrationState::default();
        let ratio = compute_camera_ratio(10, 100, &mut state);
        // raw = 10/100 = 0.1, EMA: 0.3*0.1 + 0.7*1.0 = 0.73 -> clamp to 0.73
        let expected = 0.3 * 0.1 + 0.7 * 1.0;
        assert!(
            (ratio - expected).abs() < 0.001,
            "observed == 10 should calibrate: expected {expected}, got {ratio}"
        );
    }

    #[test]
    fn decay_toward_baseline_no_action_below_3_windows() {
        let mut state = CameraCalibrationState {
            previous_ratio: 1.5,
            consecutive_stale_windows: 2,
            ..Default::default()
        };
        decay_toward_baseline(&mut state);
        assert_eq!(state.previous_ratio, 1.5, "should not decay with < 3 stale windows");
    }

    #[test]
    fn decay_toward_baseline_at_3_windows() {
        // consecutive_stale_windows = 3 -> decay = 0.1 * (3 - 2) = 0.1
        // ratio = 1.5, move toward 1.0: 1.5 + (1.0 - 1.5) * 0.1 = 1.5 - 0.05 = 1.45
        let mut state = CameraCalibrationState {
            previous_ratio: 1.5,
            consecutive_stale_windows: 3,
            ..Default::default()
        };
        decay_toward_baseline(&mut state);
        assert!(
            (state.previous_ratio - 1.45).abs() < 0.001,
            "expected 1.45, got {}",
            state.previous_ratio
        );
    }

    #[test]
    fn decay_toward_baseline_ratio_below_1_decays_up() {
        // ratio = 0.7, consecutive_stale_windows = 4 -> decay = 0.1 * (4-2) = 0.2
        // 0.7 + (1.0 - 0.7) * 0.2 = 0.7 + 0.06 = 0.76
        let mut state = CameraCalibrationState {
            previous_ratio: 0.7,
            consecutive_stale_windows: 4,
            ..Default::default()
        };
        decay_toward_baseline(&mut state);
        assert!(
            (state.previous_ratio - 0.76).abs() < 0.001,
            "expected 0.76, got {}",
            state.previous_ratio
        );
    }

    #[test]
    fn decay_does_not_overshoot_1_0() {
        // ratio = 1.05, consecutive_stale_windows = 15 -> decay = 0.1 * 13 = 1.3 -> capped to 1.0
        // 1.05 + (1.0 - 1.05) * 1.0 = 1.05 - 0.05 = 1.0
        let mut state = CameraCalibrationState {
            previous_ratio: 1.05,
            consecutive_stale_windows: 15,
            ..Default::default()
        };
        decay_toward_baseline(&mut state);
        assert!(
            (state.previous_ratio - 1.0).abs() < 0.001,
            "decay should not overshoot 1.0, got {}",
            state.previous_ratio
        );
    }

    #[test]
    fn apply_change_cap_limits_positive_delta() {
        let mut old_factors = HashMap::new();
        old_factors.insert((Zone::District1, Zone::District3), 1.0);

        let mut overlay = CalibrationOverlay {
            factors: HashMap::new(),
            timestamp_sim_seconds: 100.0,
        };
        overlay.factors.insert((Zone::District1, Zone::District3), 1.5);

        apply_change_cap(&old_factors, &mut overlay);

        let capped = overlay.factors[&(Zone::District1, Zone::District3)];
        assert!(
            (capped - 1.2).abs() < 0.001,
            "delta +0.5 should be capped to +0.2: expected 1.2, got {capped}"
        );
    }

    #[test]
    fn apply_change_cap_limits_negative_delta() {
        let mut old_factors = HashMap::new();
        old_factors.insert((Zone::District1, Zone::District3), 1.5);

        let mut overlay = CalibrationOverlay {
            factors: HashMap::new(),
            timestamp_sim_seconds: 100.0,
        };
        overlay.factors.insert((Zone::District1, Zone::District3), 1.0);

        apply_change_cap(&old_factors, &mut overlay);

        let capped = overlay.factors[&(Zone::District1, Zone::District3)];
        assert!(
            (capped - 1.3).abs() < 0.001,
            "delta -0.5 should be capped to -0.2: expected 1.3, got {capped}"
        );
    }

    #[test]
    fn apply_change_cap_allows_uncapped_first_time() {
        let old_factors = HashMap::new(); // no previous factors

        let mut overlay = CalibrationOverlay {
            factors: HashMap::new(),
            timestamp_sim_seconds: 100.0,
        };
        overlay.factors.insert((Zone::District1, Zone::District3), 2.0);

        apply_change_cap(&old_factors, &mut overlay);

        let factor = overlay.factors[&(Zone::District1, Zone::District3)];
        assert_eq!(factor, 2.0, "first-time factor should be uncapped");
    }

    #[test]
    fn apply_change_cap_allows_within_limit() {
        let mut old_factors = HashMap::new();
        old_factors.insert((Zone::District1, Zone::District3), 1.0);

        let mut overlay = CalibrationOverlay {
            factors: HashMap::new(),
            timestamp_sim_seconds: 100.0,
        };
        overlay.factors.insert((Zone::District1, Zone::District3), 1.15);

        apply_change_cap(&old_factors, &mut overlay);

        let factor = overlay.factors[&(Zone::District1, Zone::District3)];
        assert!(
            (factor - 1.15).abs() < 0.001,
            "delta within limit should be unchanged: expected 1.15, got {factor}"
        );
    }

    #[test]
    fn late_camera_default_state_participates() {
        // A camera that just connected has default state (previous_ratio=1.0).
        // With sufficient observed and simulated data, it should calibrate normally.
        let mut state = CameraCalibrationState::default();
        assert_eq!(state.consecutive_stale_windows, 0);
        assert_eq!(state.last_window_start_ms, -1);

        let ratio = compute_camera_ratio(80, 100, &mut state);
        // raw = 0.8, EMA: 0.3*0.8 + 0.7*1.0 = 0.24 + 0.70 = 0.94
        let expected = 0.3 * 0.8 + 0.7 * 1.0;
        assert!(
            (ratio - expected).abs() < 0.001,
            "late camera should calibrate: expected {expected}, got {ratio}"
        );
    }
}
