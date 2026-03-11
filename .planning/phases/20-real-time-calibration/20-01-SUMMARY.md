---
phase: 20-real-time-calibration
plan: 01
subsystem: calibration
tags: [event-driven, staleness-decay, change-cap, cooldown, ema]

requires:
  - phase: 17-detection-calibration
    provides: CameraCalibrationState, compute_calibration_factors, CalibrationStore, DetectionAggregator

provides:
  - Event-driven calibration trigger (window-change detection replaces 300s timer)
  - Staleness tracking and decay_toward_baseline for cameras with stale data
  - Per-step OD factor change cap (apply_change_cap, +/-0.2)
  - MIN_OBSERVED_THRESHOLD (10) for camera participation
  - calibration_paused flag on SimWorld
  - last_processed_windows per-camera window tracking

affects: [20-02-PLAN, calibration-ui, grpc-calibration-control]

tech-stack:
  added: []
  patterns:
    - "Event-driven calibration: window start_ms comparison instead of fixed timer"
    - "Staleness decay: exponential convergence toward baseline ratio after 3+ stale windows"
    - "Change cap: delta clamped to +/-0.2 per recalibration cycle"
    - "Cooldown guard: 30s minimum between recalibrations to prevent thrashing"

key-files:
  created: []
  modified:
    - crates/velos-api/src/calibration.rs
    - crates/velos-gpu/src/sim_calibration.rs
    - crates/velos-gpu/src/sim.rs
    - crates/velos-api/src/camera.rs

key-decisions:
  - "Window-change detection as sole trigger (no fallback timer) per user decision"
  - "30s cooldown prevents thrashing from rapid window completions"
  - "Staleness decay starts at 3 consecutive unchanged windows with 0.1*(n-2) rate"
  - "Change cap of +/-0.2 per step applied after compute, before overlay swap"
  - "MIN_OBSERVED_THRESHOLD=10 added alongside existing MIN_SIMULATED_THRESHOLD=5"
  - "insert_camera() helper added to CameraRegistry for downstream test setup"

patterns-established:
  - "Event-driven calibration: compare latest_window.start_ms vs last_processed_windows"
  - "Staleness bifurcation: cameras split into new-window vs unchanged sets each cycle"

requirements-completed: [CAL-02]

duration: 15min
completed: 2026-03-11
---

# Phase 20 Plan 01: Event-Driven Calibration Trigger Summary

**Event-driven window-change calibration trigger with 5 stability safeguards: min observation (10), cooldown (30s), staleness decay (3+ windows), change cap (+/-0.2), and pause flag**

## Performance

- **Duration:** 15 min
- **Started:** 2026-03-11T06:25:40Z
- **Completed:** 2026-03-11T06:41:02Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Replaced fixed 300s calibration timer with event-driven window-change detection
- Extended CameraCalibrationState with staleness tracking (consecutive_stale_windows, last_window_start_ms)
- Implemented 5 stability safeguards: min observation threshold, cooldown, staleness decay, change cap, pause flag
- 30 total unit tests passing across both crates (22 in velos-api, 8 in velos-gpu)

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend CameraCalibrationState and add stability functions** - `14ef2b0` (feat)
2. **Task 2: Refactor step_calibration trigger and add SimWorld fields** - `f634567` (feat)

_Note: TDD tasks combined RED+GREEN into single commits due to function signature dependencies_

## Files Created/Modified
- `crates/velos-api/src/calibration.rs` - Extended CameraCalibrationState, added decay_toward_baseline(), apply_change_cap(), MIN_OBSERVED_THRESHOLD
- `crates/velos-gpu/src/sim_calibration.rs` - Refactored step_calibration to event-driven trigger with staleness tracking
- `crates/velos-gpu/src/sim.rs` - Added calibration_paused, last_processed_windows fields to SimWorld
- `crates/velos-api/src/camera.rs` - Added insert_camera() helper for test setup in downstream crates

## Decisions Made
- Window-change detection as sole trigger -- no 300s fallback timer retained per user decision
- 30s cooldown chosen over 60s to balance responsiveness vs stability
- Staleness decay formula: 0.1*(consecutive_stale_windows - 2) capped at 1.0 -- linear ramp starting at window 3
- MIN_OBSERVED_THRESHOLD check placed before MIN_SIMULATED_THRESHOLD in compute_camera_ratio
- Added insert_camera() as a public method (not cfg(test)) since velos-gpu tests need cross-crate access

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Updated existing test for min observation threshold compatibility**
- **Found during:** Task 1
- **Issue:** Test ratio_clamped_to_0_5_when_observed_much_less used observed=1, now below MIN_OBSERVED_THRESHOLD
- **Fix:** Updated test to use observed=10 (at threshold boundary), which still exercises the clamping path
- **Files modified:** crates/velos-api/src/calibration.rs
- **Verification:** Test passes with correct expected value
- **Committed in:** 14ef2b0 (Task 1 commit)

**2. [Rule 2 - Missing Critical] Added insert_camera() to CameraRegistry**
- **Found during:** Task 2
- **Issue:** CameraRegistry.cameras is private; velos-gpu tests cannot set up cameras without full spatial pipeline
- **Fix:** Added public insert_camera() method for simplified camera registration without R-tree/projection
- **Files modified:** crates/velos-api/src/camera.rs
- **Verification:** All 8 sim_calibration tests use it successfully
- **Committed in:** f634567 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 missing critical)
**Impact on plan:** Both auto-fixes necessary for test correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Event-driven calibration trigger fully functional with all 5 stability safeguards
- Ready for Plan 02 (calibration UI/metrics integration) which can read calibration_paused flag and last_processed_windows
- CalibrationStore overlay now includes change-capped factors for smoother OD adjustments

---
*Phase: 20-real-time-calibration*
*Completed: 2026-03-11*
