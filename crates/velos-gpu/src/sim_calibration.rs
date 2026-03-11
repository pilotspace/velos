//! API command processing and calibration overlay computation for SimWorld.
//!
//! Drains gRPC commands via try_recv each frame, computes calibration
//! ratios from observed vs simulated counts, and swaps the overlay.
//! Calibration triggers on aggregation window changes (event-driven),
//! not on a fixed timer.

use std::collections::HashMap;

use velos_api::bridge::ApiCommand;
use velos_api::calibration::{
    apply_change_cap, compute_calibration_factors, decay_toward_baseline,
};
use velos_core::components::RoadPosition;
use velos_demand::Zone;
use velos_net::RoadGraph;

use crate::sim::SimWorld;

/// Build a mapping from edge ID to the nearest zone.
///
/// For each edge, find the zone whose centroid is closest to the edge midpoint.
/// Used by calibration to map camera-covered edges to OD pair zones.
pub(crate) fn build_edge_to_zone(
    graph: &RoadGraph,
    centroids: &HashMap<Zone, [f64; 2]>,
) -> HashMap<u32, Zone> {
    let g = graph.inner();
    let mut mapping = HashMap::new();

    if centroids.is_empty() {
        return mapping;
    }

    for edge_idx in g.edge_indices() {
        let edge_id = edge_idx.index() as u32;
        let (src, tgt) = g.edge_endpoints(edge_idx).unwrap();
        let sp = g[src].pos;
        let tp = g[tgt].pos;
        let mid = [(sp[0] + tp[0]) / 2.0, (sp[1] + tp[1]) / 2.0];

        let mut best_zone = None;
        let mut best_dist = f64::MAX;
        for (&zone, &pos) in centroids {
            let dx = mid[0] - pos[0];
            let dy = mid[1] - pos[1];
            let dist = dx * dx + dy * dy;
            if dist < best_dist {
                best_dist = dist;
                best_zone = Some(zone);
            }
        }
        if let Some(zone) = best_zone {
            mapping.insert(edge_id, zone);
        }
    }

    mapping
}

/// Polling interval for window-change detection in sim-seconds.
/// Avoids per-frame mutex acquisition on registry + aggregator.
/// 2 seconds is responsive enough for real-time calibration (vs old 300s).
const CALIBRATION_POLL_INTERVAL_SECS: f64 = 2.0;

/// Minimum cooldown between calibration recalibrations in sim-seconds.
/// Prevents thrashing when multiple windows complete in rapid succession.
const CALIBRATION_COOLDOWN_SECS: f64 = 30.0;

/// Minimum number of agents with RoadPosition required before calibration runs.
/// Prevents wildly inaccurate factors when the sim has just started spawning.
const MIN_AGENTS_FOR_CALIBRATION: u32 = 100;

/// Maximum API commands to process per frame to prevent frame spikes.
const MAX_COMMANDS_PER_FRAME: usize = 64;

impl SimWorld {
    /// Drain pending API commands from the gRPC bridge (non-blocking).
    ///
    /// Processes up to [`MAX_COMMANDS_PER_FRAME`] commands per frame.
    /// RegisterCamera commands are forwarded to the camera registry and
    /// replied via oneshot. DetectionBatch commands are ingested into
    /// the aggregator.
    pub(crate) fn step_api_commands(&mut self) {
        let bridge = match &mut self.api_bridge {
            Some(b) => b,
            None => return,
        };

        let commands = bridge.drain(MAX_COMMANDS_PER_FRAME);
        for cmd in commands {
            match cmd {
                ApiCommand::RegisterCamera { request } => {
                    // Camera is already registered in the shared registry by the
                    // gRPC handler. This notification lets SimWorld update any
                    // local bookkeeping (e.g., edge-to-zone mapping refresh).
                    log::info!(
                        "SimWorld notified of camera registration: '{}'",
                        request.name
                    );
                }
                ApiCommand::DetectionBatch { batch } => {
                    // Detection batches are already ingested by the gRPC handler
                    // into the shared aggregator. This is a notification that
                    // the simulation can use for bookkeeping if needed.
                    log::trace!(
                        "SimWorld received detection batch {}, {} events",
                        batch.batch_id,
                        batch.events.len()
                    );
                }
            }
        }
    }

    /// Recompute calibration factors when aggregation windows change.
    ///
    /// Event-driven trigger: detects when any camera's latest aggregation
    /// window has a new `start_ms` compared to `last_processed_windows`.
    /// Applies stability safeguards: cooldown, staleness decay, change cap.
    pub(crate) fn step_calibration(&mut self) {
        // 1. Early return if calibration is paused
        if self.calibration_paused {
            log::debug!("[calibration] paused, skipping");
            return;
        }

        // 1.5. Poll guard: only check for window changes every N sim-seconds
        // to avoid per-frame mutex acquisition on registry + aggregator.
        log::debug!(
            "[calibration] entry: sim_time={:.1}, last_poll={:.1}, diff={:.1}, threshold={}",
            self.sim_time, self.last_calibration_poll_time,
            self.sim_time - self.last_calibration_poll_time,
            CALIBRATION_POLL_INTERVAL_SECS
        );
        if self.sim_time - self.last_calibration_poll_time < CALIBRATION_POLL_INTERVAL_SECS {
            return;
        }
        self.last_calibration_poll_time = self.sim_time;
        log::debug!("[calibration] poll check passed at sim_time={:.1}", self.sim_time);

        // 2. Lock registry, get camera list, early return if empty
        let registry = self.camera_registry.lock().unwrap();
        let cameras = registry.list();
        if cameras.is_empty() {
            log::debug!("[calibration] no cameras registered, skipping");
            return;
        }
        let cam_ids: Vec<u32> = cameras.iter().map(|c| c.id).collect();
        log::debug!("[calibration] {} cameras registered: {:?}", cam_ids.len(), cam_ids);
        drop(registry); // release lock before aggregator

        // 3. Lock aggregator, check for new windows
        let aggregator = self.aggregator.lock().unwrap();
        let mut has_new_windows = false;
        let mut cameras_with_new_windows: Vec<u32> = Vec::new();
        let mut cameras_unchanged: Vec<u32> = Vec::new();

        for &cam_id in &cam_ids {
            let latest_start = aggregator
                .latest_window(cam_id)
                .map(|w| w.start_ms)
                .unwrap_or(-1);

            let last_processed = self
                .last_processed_windows
                .get(&cam_id)
                .copied()
                .unwrap_or(-1);

            log::debug!(
                "[calibration] cam_id={}: latest_start={}, last_processed={}",
                cam_id, latest_start, last_processed
            );

            if latest_start > last_processed {
                has_new_windows = true;
                cameras_with_new_windows.push(cam_id);
            } else {
                cameras_unchanged.push(cam_id);
            }
        }
        drop(aggregator); // release lock

        // 4. If no new windows found, handle staleness and return
        if !has_new_windows {
            log::debug!("[calibration] no new windows, incrementing staleness");
            for &cam_id in &cameras_unchanged {
                let state = self.calibration_states.entry(cam_id).or_default();
                state.consecutive_stale_windows += 1;
                decay_toward_baseline(state);
            }
            return;
        }

        // 5. Track staleness: increment for unchanged, reset for new
        for &cam_id in &cameras_unchanged {
            let state = self.calibration_states.entry(cam_id).or_default();
            state.consecutive_stale_windows += 1;
            decay_toward_baseline(state);
        }
        for &cam_id in &cameras_with_new_windows {
            let state = self.calibration_states.entry(cam_id).or_default();
            state.consecutive_stale_windows = 0;
        }

        // 6. Apply cooldown: new windows exist but cooldown not elapsed
        log::debug!(
            "[calibration] new windows found! sim_time={:.1}, last_cal={:.1}, cooldown={}",
            self.sim_time, self.last_calibration_time, CALIBRATION_COOLDOWN_SECS
        );
        if self.sim_time - self.last_calibration_time < CALIBRATION_COOLDOWN_SECS {
            log::debug!("[calibration] cooldown not elapsed, skipping full recalibration");
            return;
        }

        // 7. Capture current overlay factors BEFORE computing new ones (for change cap)
        let old_factors: HashMap<(Zone, Zone), f32> =
            self.calibration_store.current().factors.clone();

        // 8. Collect simulated counts: for each camera, count agents on covered edges
        let registry = self.camera_registry.lock().unwrap();
        let cameras = registry.list();

        let mut edge_to_cameras: HashMap<u32, Vec<u32>> = HashMap::new();
        for cam in &cameras {
            log::debug!(
                "Camera '{}' (id={}) covers {} edges: {:?}",
                cam.name,
                cam.id,
                cam.covered_edges.len(),
                &cam.covered_edges[..cam.covered_edges.len().min(10)]
            );
            for &edge_id in &cam.covered_edges {
                edge_to_cameras
                    .entry(edge_id)
                    .or_default()
                    .push(cam.id);
            }
        }
        drop(registry); // release lock before ECS query

        self.simulated_counts.clear();
        let mut total_agents = 0u32;
        for rp in self.world.query_mut::<&RoadPosition>().into_iter() {
            total_agents += 1;
            if let Some(cam_ids) = edge_to_cameras.get(&rp.edge_index) {
                for &cam_id in cam_ids {
                    *self.simulated_counts.entry(cam_id).or_insert(0) += 1;
                }
            }
        }
        log::debug!(
            "[calibration] {} agents, {} covered edges, sim_counts={:?}",
            total_agents, edge_to_cameras.len(), &self.simulated_counts,
        );

        // 8.5. Guard: skip calibration if too few agents (sim still warming up)
        // In detection-only mode, skip this guard to allow bootstrapping from zero.
        if !self.detection_only_spawning && total_agents < MIN_AGENTS_FOR_CALIBRATION {
            log::debug!(
                "[calibration] only {} agents (need {}), skipping",
                total_agents, MIN_AGENTS_FOR_CALIBRATION
            );
            return;
        }

        // 9. Compute calibration factors
        let registry = self.camera_registry.lock().unwrap();
        let aggregator = self.aggregator.lock().unwrap();

        let mut overlay = compute_calibration_factors(
            &registry,
            &aggregator,
            &self.simulated_counts,
            &mut self.calibration_states,
            &self.edge_to_zone,
            self.sim_time,
        );

        let camera_count = cameras_with_new_windows.len();
        drop(registry);
        drop(aggregator);

        // 10. Apply change cap before swapping
        apply_change_cap(&old_factors, &mut overlay);

        let factor_count = overlay.factors.len();
        self.calibration_store.swap(overlay);
        self.last_calibration_time = self.sim_time;

        // 11. Update last_processed_windows for all cameras that have windows
        let aggregator = self.aggregator.lock().unwrap();
        for &cam_id in &cam_ids {
            if let Some(w) = aggregator.latest_window(cam_id) {
                self.last_processed_windows.insert(cam_id, w.start_ms);
            }
        }
        drop(aggregator);

        if factor_count > 0 {
            log::info!(
                "Calibration triggered: {} cameras, {} OD factors updated",
                camera_count,
                factor_count
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use velos_api::calibration::{CalibrationOverlay, CameraCalibrationState};
    use velos_api::proto::velos::v2::{DetectionEvent, VehicleClass};
    use velos_core::components::{Kinematics, Position, VehicleType};

    /// Create a minimal SimWorld for calibration tests (CPU-only, simple graph).
    fn make_calibration_sim() -> SimWorld {
        use petgraph::graph::DiGraph;
        use velos_net::graph::{RoadClass, RoadEdge, RoadNode};

        let mut g = DiGraph::new();
        let a = g.add_node(RoadNode { pos: [0.0, 0.0] });
        let b = g.add_node(RoadNode { pos: [200.0, 0.0] });
        g.add_edge(
            a,
            b,
            RoadEdge {
                length_m: 200.0,
                speed_limit_mps: 13.9,
                lane_count: 2,
                oneway: true,
                road_class: RoadClass::Primary,
                geometry: vec![[0.0, 0.0], [200.0, 0.0]],
                motorbike_only: false,
                time_windows: None,
            },
        );
        let graph = velos_net::RoadGraph::new(g);
        SimWorld::new_cpu_only(graph)
    }

    /// Spawn `count` test agents on the given edge so calibration's MIN_AGENTS guard passes.
    fn spawn_test_agents(sim: &mut SimWorld, edge_index: u32, count: u32) {
        for i in 0..count {
            sim.world.spawn((
                Position { x: i as f64 * 2.0, y: 0.0 },
                Kinematics { vx: 1.0, vy: 0.0, speed: 1.0, heading: 0.0 },
                VehicleType::Motorbike,
                RoadPosition { edge_index, lane: 0, offset_m: i as f64 * 2.0 },
            ));
        }
    }

    /// Register a camera and add detection data so the aggregator has a window.
    fn setup_camera_with_detection(
        sim: &mut SimWorld,
        cam_name: &str,
        covered_edges: Vec<u32>,
        window_timestamp_ms: i64,
        observed_count: u32,
    ) -> u32 {
        let cam_id = {
            let mut registry = sim.camera_registry.lock().unwrap();
            registry.insert_camera(cam_name, covered_edges)
        };
        if observed_count > 0 {
            let mut aggregator = sim.aggregator.lock().unwrap();
            aggregator.ingest(
                cam_id,
                &DetectionEvent {
                    camera_id: cam_id,
                    timestamp_ms: window_timestamp_ms,
                    vehicle_class: VehicleClass::Motorbike as i32,
                    count: observed_count,
                    speed_kmh: None,
                },
            );
        }
        cam_id
    }

    #[test]
    fn step_calibration_returns_early_when_paused() {
        let mut sim = make_calibration_sim();
        sim.calibration_paused = true;
        sim.sim_time = 1000.0;

        // Register a camera with detection data
        setup_camera_with_detection(&mut sim, "cam-1", vec![0], 100_000, 50);

        sim.step_calibration();

        // Overlay should remain empty because calibration is paused
        let overlay = sim.calibration_store.current();
        assert!(
            overlay.factors.is_empty(),
            "calibration should not run when paused"
        );
    }

    #[test]
    fn step_calibration_returns_early_when_no_cameras() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 1000.0;

        sim.step_calibration();

        let overlay = sim.calibration_store.current();
        assert!(
            overlay.factors.is_empty(),
            "calibration should not run with no cameras"
        );
    }

    #[test]
    fn step_calibration_returns_early_when_no_new_windows() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 1000.0;

        // Register camera but give it NO detection data
        let cam_id = {
            let mut registry = sim.camera_registry.lock().unwrap();
            registry.insert_camera("cam-1", vec![0])
        };

        // Mark the last processed window as current (nothing new)
        sim.last_processed_windows.insert(cam_id, -1);

        sim.step_calibration();

        let overlay = sim.calibration_store.current();
        assert!(
            overlay.factors.is_empty(),
            "calibration should not run when no new windows"
        );
    }

    #[test]
    fn step_calibration_returns_early_when_cooldown_not_elapsed() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 50.0;
        sim.last_calibration_time = 40.0; // only 10s ago, cooldown is 30s

        setup_camera_with_detection(&mut sim, "cam-1", vec![0], 100_000, 50);

        sim.step_calibration();

        // Cooldown should prevent overlay swap but staleness tracking still runs
        let overlay = sim.calibration_store.current();
        assert!(
            overlay.factors.is_empty(),
            "calibration should not run within cooldown period"
        );
    }

    #[test]
    fn step_calibration_skips_when_too_few_agents() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 100.0;
        sim.last_calibration_time = 0.0;

        // Spawn only 5 agents (below MIN_AGENTS_FOR_CALIBRATION)
        spawn_test_agents(&mut sim, 0, 5);

        setup_camera_with_detection(&mut sim, "cam-1", vec![0], 100_000, 50);
        sim.edge_to_zone.insert(0, Zone::District1);

        sim.step_calibration();

        let overlay = sim.calibration_store.current();
        assert!(
            overlay.factors.is_empty(),
            "calibration should skip when too few agents"
        );
    }

    #[test]
    fn step_calibration_triggers_on_window_change() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 100.0;
        sim.last_calibration_time = 0.0; // cooldown elapsed

        // Spawn enough agents to pass MIN_AGENTS guard
        spawn_test_agents(&mut sim, 0, MIN_AGENTS_FOR_CALIBRATION);

        // Register camera with detection data and edge-to-zone mapping
        let cam_id = setup_camera_with_detection(
            &mut sim,
            "cam-1",
            vec![0],
            100_000,
            50,
        );

        // Ensure edge 0 maps to a zone
        sim.edge_to_zone.insert(0, Zone::District1);

        // Camera has a window at start_ms=90000 (floor(100000/15000)*15000 = 90000).
        // last_processed_windows is empty, so -1 < 90000 => new window detected.
        sim.step_calibration();

        let overlay = sim.calibration_store.current();
        assert!(
            !overlay.factors.is_empty(),
            "calibration should trigger when new window detected, cam_id={}",
            cam_id,
        );
    }

    #[test]
    fn step_calibration_updates_last_processed_windows() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 100.0;
        sim.last_calibration_time = 0.0;

        spawn_test_agents(&mut sim, 0, MIN_AGENTS_FOR_CALIBRATION);

        let cam_id = setup_camera_with_detection(
            &mut sim,
            "cam-1",
            vec![0],
            100_000,
            50,
        );
        sim.edge_to_zone.insert(0, Zone::District1);

        assert!(sim.last_processed_windows.is_empty());

        sim.step_calibration();

        // After calibration, last_processed_windows should be updated
        assert!(
            sim.last_processed_windows.contains_key(&cam_id),
            "last_processed_windows should be updated for camera"
        );
        // window start_ms = floor(100000/15000)*15000 = 90000
        assert_eq!(sim.last_processed_windows[&cam_id], 90_000);
    }

    #[test]
    fn decay_called_for_cameras_with_unchanged_windows() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 100.0;

        // Register camera with no detection data (will be unchanged)
        let cam_id = {
            let mut registry = sim.camera_registry.lock().unwrap();
            registry.insert_camera("stale-cam", vec![0])
        };

        // Pre-set a calibration state with ratio != 1.0
        sim.calibration_states.insert(
            cam_id,
            CameraCalibrationState {
                previous_ratio: 1.5,
                consecutive_stale_windows: 0,
                ..Default::default()
            },
        );

        // Call step_calibration multiple times to accumulate staleness.
        // Advance sim_time each iteration to pass the poll interval guard.
        for i in 0..4 {
            sim.sim_time = 100.0 + (i as f64 + 1.0) * CALIBRATION_POLL_INTERVAL_SECS;
            sim.step_calibration();
        }

        let state = sim.calibration_states.get(&cam_id).unwrap();
        assert_eq!(
            state.consecutive_stale_windows, 4,
            "consecutive_stale_windows should increment each call"
        );
        // After 4 stale windows, decay should have been applied on windows 3 and 4
        // Window 3: decay = 0.1*(3-2) = 0.1, ratio moves from 1.5 toward 1.0
        // Window 4: decay = 0.1*(4-2) = 0.2, ratio moves further toward 1.0
        assert!(
            state.previous_ratio < 1.5,
            "ratio should have decayed from 1.5 toward 1.0, got {}",
            state.previous_ratio
        );
    }

    #[test]
    fn apply_change_cap_applied_before_swap() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 100.0;
        sim.last_calibration_time = 0.0;

        spawn_test_agents(&mut sim, 0, MIN_AGENTS_FOR_CALIBRATION);

        // Pre-load an overlay with a known factor
        let mut old_factors = HashMap::new();
        old_factors.insert((Zone::District1, Zone::District1), 1.0);
        sim.calibration_store.swap(CalibrationOverlay {
            factors: old_factors,
            timestamp_sim_seconds: 0.0,
        });

        // Set up camera covering edge 0 with extreme observed count
        setup_camera_with_detection(&mut sim, "cam-1", vec![0], 100_000, 500);
        sim.edge_to_zone.insert(0, Zone::District1);

        sim.step_calibration();

        let overlay = sim.calibration_store.current();
        if let Some(&factor) = overlay.factors.get(&(Zone::District1, Zone::District1)) {
            // Factor should be capped at old + 0.2 = 1.2 maximum
            assert!(
                factor <= 1.2 + 0.001,
                "factor should be capped at 1.2 (old=1.0 + 0.2), got {}",
                factor
            );
        }
    }

    #[test]
    fn step_calibration_skips_within_poll_interval() {
        let mut sim = make_calibration_sim();
        sim.sim_time = 100.0;
        sim.last_calibration_time = 0.0;

        spawn_test_agents(&mut sim, 0, MIN_AGENTS_FOR_CALIBRATION);
        setup_camera_with_detection(&mut sim, "cam-1", vec![0], 100_000, 50);
        sim.edge_to_zone.insert(0, Zone::District1);

        // First call triggers (poll interval elapsed: 100 - 0 >= 2)
        sim.step_calibration();
        let overlay = sim.calibration_store.current();
        assert!(!overlay.factors.is_empty(), "first call should trigger");

        // Reset overlay to detect if second call triggers
        sim.calibration_store.swap(CalibrationOverlay {
            factors: HashMap::new(),
            timestamp_sim_seconds: 0.0,
        });

        // Second call at same sim_time should skip (poll guard)
        sim.step_calibration();
        let overlay = sim.calibration_store.current();
        assert!(
            overlay.factors.is_empty(),
            "second call within poll interval should skip"
        );
    }
}
