---
phase: 13-final-integration-wiring-gpu-transfer-audit
plan: 03
subsystem: gpu-engine
tags: [wgpu, mobil, dirty-flag, buffer-upload, cpu-gpu-parity, gpu-transfer]

# Dependency graph
requires:
  - phase: 13-final-integration-wiring-gpu-transfer-audit
    provides: "13-01: AgentProfile + GLOSA wiring, 13-02: GPU pedestrian pipeline"
  - phase: 12-cpu-lane-change-sublane-wiring
    provides: "step_lane_changes in tick_gpu() at step 6.7"
provides:
  - "CPU tick() step_lane_changes parity with GPU tick_gpu() pipeline"
  - "Dirty-flag gated signal buffer uploads (skip when no phase transition)"
  - "Dirty-flag gated prediction buffer uploads (skip when overlay unchanged)"
  - "No per-frame GPU transfer waste for unchanged buffers"
affects: [gpu-performance, frame-time]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Dirty-flag gated GPU buffer uploads: signal_dirty/prediction_dirty fields on SimWorld"
    - "Phase transition detection via get_phase_state(0) before/after tick()"

key-files:
  created: []
  modified:
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/sim_perception.rs
    - crates/velos-gpu/src/sim_reroute.rs

key-decisions:
  - "Checking approach 0 phase state for transition detection (all approaches transition simultaneously in signal plan)"
  - "Dirty flags initialized true to force initial upload on first frame"
  - "Restructured step_perception to call mutable dirty-flag methods before immutable borrows"

patterns-established:
  - "Dirty-flag GPU upload pattern: set flag on data change, check flag before queue.write_buffer, reset after upload"
  - "step_signals_with_detectors promoted to pub(crate) for cross-module test access"

requirements-completed: [INT-01, INT-02, SIG-03, AGT-04]

# Metrics
duration: 10min
completed: 2026-03-08
---

# Phase 13 Plan 03: CPU Tick Parity + Dirty-Flag GPU Buffer Optimization Summary

**CPU tick() MOBIL parity via step_lane_changes at step 6.7, plus dirty-flag elimination of wasteful per-frame signal and prediction GPU buffer uploads**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-08T13:33:50Z
- **Completed:** 2026-03-08T13:44:07Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- CPU tick() now calls step_lane_changes(dt) at step 6.7 between meso and vehicle physics, matching GPU tick_gpu() pipeline order
- signal_dirty flag gates update_signal_buffer: ~25K u32 writes skipped when no signal phase transitions
- prediction_dirty flag gates update_edge_travel_ratio_buffer: ~25K f32 writes skipped when overlay unchanged
- Both dirty flags initialized true for mandatory first-frame upload
- step_signals_with_detectors detects phase transitions via get_phase_state(0) comparison
- step_prediction sets prediction_dirty only when overlay actually swaps
- 6 new unit tests: 2 CPU tick parity, 4 dirty-flag behavior

## Task Commits

Each task was committed atomically:

1. **Task 1 (RED): CPU tick parity tests** - `5d040ba` (test)
2. **Task 1 (GREEN): Add step_lane_changes to CPU tick()** - `e989591` (feat)
3. **Task 2 (RED): Dirty-flag tests** - `7b0b77a` (test)
4. **Task 2 (GREEN): Dirty-flag buffer upload optimization** - `a3a521c` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/sim.rs` - Added signal_dirty/prediction_dirty fields, step_lane_changes in tick(), phase transition detection in step_signals_with_detectors, 2 CPU parity tests
- `crates/velos-gpu/src/sim_perception.rs` - Dirty-flag early returns in update_signal_buffer/update_edge_travel_ratio_buffer, restructured step_perception for borrow safety, 4 dirty-flag tests
- `crates/velos-gpu/src/sim_reroute.rs` - Set prediction_dirty=true after overlay swap in step_prediction

## Decisions Made
- Used approach 0 phase state comparison for transition detection -- all approaches transition simultaneously in the signal plan, so checking one is sufficient
- Initialized dirty flags to true (not false) so the first frame always uploads complete signal and prediction state
- Restructured step_perception to call update_signal_buffer and update_edge_travel_ratio_buffer before taking immutable borrows on perception/perc_buffers, avoiding Rust borrow checker conflicts
- Made step_signals_with_detectors pub(crate) to allow test access from sim_perception tests

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added WaitState and Route components to test cars**
- **Found during:** Task 1 (RED test compilation)
- **Issue:** Test cars spawned without WaitState/Route components, causing panic in apply_vehicle_update during tick()
- **Fix:** Added WaitState and Route to spawn_test_car helper
- **Files modified:** crates/velos-gpu/src/sim.rs
- **Verification:** Tests compile and run without panic
- **Committed in:** 5d040ba (Task 1 RED commit)

**2. [Rule 3 - Blocking] Restructured step_perception for borrow safety**
- **Found during:** Task 2 (update_signal_buffer &self -> &mut self change)
- **Issue:** Changing update_signal_buffer/update_edge_travel_ratio_buffer from &self to &mut self caused borrow conflict with immutable perception/perc_buffers borrows taken earlier in step_perception
- **Fix:** Reordered step_perception: call mutable methods first, then take immutable borrows with is_none() guard + unwrap()
- **Files modified:** crates/velos-gpu/src/sim_perception.rs
- **Verification:** Compiles cleanly, all 89 lib tests pass
- **Committed in:** a3a521c (Task 2 GREEN commit)

**3. [Rule 3 - Blocking] Made step_signals_with_detectors pub(crate)**
- **Found during:** Task 2 (dirty-flag test needs to call step_signals_with_detectors)
- **Issue:** step_signals_with_detectors was private, tests in sim_perception module could not call it
- **Fix:** Changed visibility from fn to pub(crate) fn
- **Files modified:** crates/velos-gpu/src/sim.rs
- **Verification:** Tests compile and pass
- **Committed in:** a3a521c (Task 2 GREEN commit)

---

**Total deviations:** 3 auto-fixed (3 blocking)
**Impact on plan:** All fixes necessary for compilation and test access. No scope creep.

## Issues Encountered
- cpu_reference::step_vehicles already contains MOBIL evaluation, so adding step_lane_changes before it results in MOBIL running from step_lane_changes (with step_vehicles skipping via has_lc=true check). Functionally equivalent but structurally matches GPU path.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All Phase 13 plans complete: agent profile wiring, GPU pedestrian pipeline, CPU tick parity, dirty-flag optimization
- CPU and GPU tick pipelines now have identical logical step sequences
- GPU transfer overhead eliminated for unchanged signal and prediction buffers
- Full workspace test suite passes (89 velos-gpu lib tests, all integration tests)

---
*Phase: 13-final-integration-wiring-gpu-transfer-audit*
*Completed: 2026-03-08*
