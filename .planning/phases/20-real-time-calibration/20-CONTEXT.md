# Phase 20: Real-Time Calibration - Context

**Gathered:** 2026-03-11
**Status:** Ready for planning

<domain>
## Phase Boundary

Simulation demand continuously self-corrects from streaming detection data without requiring restart. While the simulation is running, new detection data flowing in causes demand adjustments within the current session. User can observe OD spawn rates changing in response to streaming detection counts without stopping or restarting.

Requirements: CAL-02

</domain>

<decisions>
## Implementation Decisions

### Calibration Trigger Strategy
- **Event-driven recalibration** on new aggregation window completion, replacing the fixed 300s timer
- When DetectionAggregator rolls to a new TimeWindow (detected by comparing latest window start_ms against last-processed), trigger calibration immediately
- Minimum cooldown of 30 sim-seconds between recalibrations to prevent thrashing when multiple cameras complete windows simultaneously
- If no detection data arrives, no recalibration happens (demand stays at last computed overlay) — this is inherently continuous since it reacts to data, not a clock
- The existing `step_calibration()` method in `sim_calibration.rs` is refactored: remove the fixed `CALIBRATION_INTERVAL_SECS` timer, replace with window-change detection logic
- Fallback: if detection stream is active but aggregator has no new complete windows, skip — don't recalibrate on partial windows

### Convergence & Stability Safeguards
- Keep existing EMA alpha=0.3 and clamp [0.5, 2.0] — already proven in Phase 17
- Add **minimum observation threshold per camera**: skip camera from calibration if observed count < 10 in the latest window (prevents noisy ratios from sparse data)
- Add **decay toward baseline**: if a camera has no new detection data for 3 consecutive aggregation windows, EMA-decay its ratio toward 1.0 at rate 0.1 per missed window (gradual return to uncalibrated demand)
- Add **per-step demand change cap**: the absolute change in any OD pair factor between consecutive overlays is capped at ±0.2 per recalibration — prevents sharp jumps even when EMA output changes significantly
- Track `last_window_start_ms` per camera in `CameraCalibrationState` to detect staleness

### User Visibility & Control
- Egui calibration panel (existing) enhanced with:
  - **Status indicator**: "Calibrating" (green dot) when detections flowing and ratios updating, "Idle" (gray) when no cameras registered, "Stale" (yellow) when stream stopped but ratios held
  - **Per-camera row**: camera name, observed/simulated counts, current ratio, last update timestamp, staleness indicator
  - **Global summary**: total cameras active, mean ratio across all cameras, time since last recalibration
- **Pause/resume toggle**: checkbox in calibration panel to freeze calibration overlay (stop recalculating, keep current factors) — user can examine or manually adjust simulation without calibration interference
- **No ratio history chart** — out of scope for POC; the per-camera current values are sufficient

### Session Continuity Behavior
- **Stream disconnect**: ratios freeze at last computed values — no automatic decay on disconnect (decay only triggers on camera staleness within an active stream)
- **Late camera connection**: simulation can start without any cameras; cameras registered mid-simulation immediately participate in next calibration cycle when their first aggregation window completes
- **Camera removal**: if a camera is unregistered (future capability), its ratio contributions are dropped on next recalibration — affected OD pairs fall back to remaining cameras or 1.0
- **Simulation restart**: calibration state (EMA history, staleness counters) resets — fresh start, consistent with SimWorld::new() semantics

### Claude's Discretion
- Exact egui layout and styling for enhanced calibration panel
- Whether to add a log::info on each recalibration event or keep it at debug level
- Internal data structure for tracking per-camera window freshness (could be a simple HashMap<u32, i64> for last_window_start_ms)
- Whether the minimum observation threshold (10) should be configurable via TOML or hardcoded constant

</decisions>

<specifics>
## Specific Ideas

- The key insight is that `step_calibration()` already runs every frame — Phase 20 changes its trigger from "has 300s elapsed?" to "has a new aggregation window completed?" This is a focused refactor, not a rewrite
- Decay-toward-baseline prevents phantom demand adjustments from cameras that stopped sending data
- The pause toggle is important for debugging: when tuning the simulation, you don't want calibration fighting your manual adjustments

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `velos-api/src/calibration.rs`: CalibrationStore (ArcSwap), CalibrationOverlay, CameraCalibrationState, compute_camera_ratio(), compute_calibration_factors() — all reusable, extend CameraCalibrationState with staleness tracking
- `velos-api/src/aggregator.rs`: DetectionAggregator with latest_window(), TimeWindow with start_ms — window change detection reads latest_window().start_ms
- `velos-gpu/src/sim_calibration.rs`: step_calibration() and step_api_commands() — refactor trigger logic here
- `velos-gpu/src/sim_lifecycle.rs`: step_spawning() already calls generate_spawns_calibrated() with overlay.factors — no changes needed
- `velos-gpu/src/app_egui.rs`: egui panel rendering — extend existing calibration panel

### Established Patterns
- ArcSwap lock-free overlay swap — same pattern continues, just swapped more responsively
- Mutex<CameraRegistry> + Mutex<DetectionAggregator> for shared gRPC state — same access pattern
- Per-frame drain of API commands via bounded mpsc channel — unchanged
- ECS query for simulated counts (world.query_mut::<&RoadPosition>) — same query on each recalibration

### Integration Points
- `velos-gpu/src/sim_calibration.rs`: Primary change — trigger logic in step_calibration()
- `velos-api/src/calibration.rs`: Extend CameraCalibrationState with staleness fields, add per-step change cap logic
- `velos-gpu/src/sim.rs`: Add calibration_paused: bool field, last_processed_window_ms per camera tracking
- `velos-gpu/src/app_egui.rs`: Enhanced calibration panel with status indicator and pause toggle

</code_context>

<deferred>
## Deferred Ideas

- Ratio history chart / time-series visualization — future enhancement
- Camera removal/unregister gRPC RPC — future capability
- Configurable EMA alpha / clamp bounds via egui sliders — future enhancement
- Per-vehicle-class calibration ratios (currently aggregated across all classes) — future enhancement

</deferred>

---

*Phase: 20-real-time-calibration*
*Context gathered: 2026-03-11*
