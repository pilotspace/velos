---
phase: 12-cpu-lane-change-sublane-wiring
plan: 02
subsystem: gpu
tags: [mobil, lane-change, sublane, prediction, tick-gpu, motorbike]

requires:
  - phase: 12-cpu-lane-change-sublane-wiring
    provides: 12-float GpuVehicleParams with creep/gap fields driven from TOML config
  - phase: 07-intelligence-routing-prediction
    provides: PredictionService with should_update/update/store API
  - phase: 04-mobil-wiring
    provides: MOBIL evaluate_mobil, start_lane_change, process_car_lane_changes
provides:
  - step_lane_changes() MOBIL evaluation + drift in tick_gpu before GPU physics
  - step_motorbikes_sublane() after GPU readback with post-GPU spatial index
  - step_prediction() after step_bus_dwell in both tick_gpu() and tick()
  - Integration tests for lane-change, sublane, and prediction pipeline
affects: []

tech-stack:
  added: []
  patterns:
    - "CPU lane-change before GPU physics: MOBIL evaluated on CPU, LaneChangeState set on ECS, GPU reads updated positions"
    - "Post-GPU sublane filtering: motorbike lateral positions adjusted after GPU readback with rebuilt spatial index"
    - "Prediction overlay refresh: PredictionService::update() called every 60 sim-seconds with edge flow data"

key-files:
  created:
    - crates/velos-gpu/tests/lane_change_integration.rs
  modified:
    - crates/velos-gpu/src/sim_mobil.rs
    - crates/velos-gpu/src/sim_reroute.rs
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/lib.rs

key-decisions:
  - "step_lane_changes runs before GPU physics (step 6.7) so MOBIL decisions and drift are applied to ECS before GPU upload"
  - "step_motorbikes_sublane runs after GPU readback (step 7.5) with rebuilt spatial index for accurate neighbor gaps"
  - "step_prediction runs after step_bus_dwell (step 8.5) -- edge flows are fresh from vehicle physics"
  - "step_prediction added to CPU tick() path for consistency -- prediction overlay refreshes in both code paths"
  - "RerouteState, reroute field, step_lane_changes, start_lane_change, step_prediction promoted to pub for integration test access"
  - "cpu_reference module promoted to pub for integration test access to step_motorbikes_sublane"

patterns-established:
  - "Full 12-step GPU pipeline: spawn -> detectors -> signals -> priority -> perception -> reroute -> meso -> lane_changes -> GPU_physics -> sublane -> bus_dwell -> prediction -> pedestrians -> cleanup"

requirements-completed: [RTE-05, RTE-07]

duration: 12min
completed: 2026-03-08
---

# Phase 12 Plan 02: MOBIL Lane-Change + Sublane + Prediction Wiring Summary

**MOBIL lane-change evaluation, motorbike sublane filtering, and PredictionService update wired into tick_gpu() pipeline with 5 integration tests**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-08T10:09:26Z
- **Completed:** 2026-03-08T10:21:39Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Created step_lane_changes() that evaluates MOBIL for all cars without active lane change, starts accepted decisions, and processes ongoing lateral drift -- runs before GPU physics dispatch
- Created step_prediction() that gathers per-edge flow/capacity data from road graph and agent positions, calls PredictionService::update() when 60 sim-seconds have elapsed
- Wired step_motorbikes_sublane after GPU readback with post-GPU spatial index rebuild for accurate neighbor gap computation
- Added step_prediction() to CPU tick() path for consistency
- Updated tick_gpu() pipeline comment block to reflect all 12 steps
- 5 integration tests covering MOBIL trigger, drift completion, single-lane skip, motorbike sublane adjustment, and prediction overlay update

## Task Commits

Each task was committed atomically:

1. **Task 1: Create step_lane_changes(), step_prediction(), wire all into tick_gpu()** - `6afbcce` (test: RED), `f7adbbe` (feat: GREEN)
2. **Task 2: Integration tests for lane-change, sublane, and prediction** - `4252e26` (test)

**Plan metadata:** (pending)

_Note: TDD tasks -- RED phase confirmed step_prediction compilation failure, GREEN phase implemented all methods and wiring._

## Files Created/Modified
- `crates/velos-gpu/src/sim_mobil.rs` - Added step_lane_changes() method combining MOBIL evaluation + drift processing for all cars
- `crates/velos-gpu/src/sim_reroute.rs` - Added step_prediction() method with edge flow gathering and PredictionService::update() call
- `crates/velos-gpu/src/sim.rs` - Wired step_lane_changes (6.7), step_motorbikes_sublane (7.5), step_prediction (8.5) into tick_gpu(); added step_prediction to tick()
- `crates/velos-gpu/src/lib.rs` - Promoted cpu_reference module to pub for integration test access
- `crates/velos-gpu/tests/lane_change_integration.rs` - 5 integration tests for MOBIL lane-change, sublane filtering, and prediction overlay

## Decisions Made
- step_lane_changes placed at step 6.7 (after meso, before GPU physics) -- MOBIL decisions must be in ECS before GPU reads positions
- step_motorbikes_sublane placed at step 7.5 (after GPU physics) -- needs updated longitudinal positions for correct neighbor gaps
- step_prediction placed at step 8.5 (after bus dwell, before pedestrians) -- edge flows are most current here
- Spatial index rebuilt post-GPU for motorbike sublane (snapshot_post + spatial_post) -- pre-GPU positions would give stale gaps
- Promoted visibility of several items (RerouteState, reroute field, cpu_reference module) from pub(crate) to pub for integration test ergonomics

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added WaitState component to integration test agents**
- **Found during:** Task 2 (integration tests)
- **Issue:** apply_vehicle_update -> update_wait_state unwraps WaitState component, which test-spawned agents lacked
- **Fix:** Added WaitState { stopped_since: -1.0, at_red_signal: false } to spawn_car and spawn_motorbike helpers
- **Files modified:** crates/velos-gpu/tests/lane_change_integration.rs
- **Verification:** All 5 integration tests pass
- **Committed in:** 4252e26 (part of task commit)

**2. [Rule 3 - Blocking] Fixed hecs query_mut destructuring pattern**
- **Found during:** Task 1 (step_prediction implementation)
- **Issue:** hecs query_mut::<(&A, &B)> yields (&A, &B) directly, not (Entity, (&A, &B))
- **Fix:** Changed destructuring from (_, (rp, kin)) to (rp, kin)
- **Files modified:** crates/velos-gpu/src/sim_reroute.rs
- **Verification:** Compilation succeeds, tests pass
- **Committed in:** f7adbbe (part of task commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes necessary for correct compilation and test execution. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 12 is now complete -- all lane-change behavior, sublane filtering, and prediction overlay refresh are wired into the production tick_gpu() pipeline
- The full 12-step pipeline runs: spawn -> detectors -> signals -> priority -> perception -> reroute -> meso -> lane_changes -> GPU_physics -> sublane -> bus_dwell -> prediction -> pedestrians -> cleanup
- All 143+ velos-gpu tests pass with zero warnings

## Self-Check: PASSED

- All 5 modified/created files exist on disk
- Commits 6afbcce, f7adbbe, 4252e26 verified in git log
- step_lane_changes, step_motorbikes_sublane, step_prediction all present in tick_gpu()
- step_prediction present in tick() (CPU path)
- All 143+ velos-gpu tests pass

---
*Phase: 12-cpu-lane-change-sublane-wiring*
*Completed: 2026-03-08*
