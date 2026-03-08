---
phase: 02-road-network-vehicle-models-egui
plan: 02
subsystem: simulation
tags: [idm, mobil, vehicle-model, signal-controller, gridlock, car-following, lane-change]

requires:
  - phase: 01-gpu-foundation-spikes
    provides: "wgpu GPU pipeline, ECS components (Position, Kinematics)"
provides:
  - "IDM car-following acceleration model (idm_acceleration, integrate_with_stopping_guard)"
  - "MOBIL lane-change decision model (mobil_decision)"
  - "VehicleType enum (Motorbike/Car/Pedestrian) with default IDM/MOBIL params"
  - "GridlockDetector with BFS cycle detection on waiting graphs"
  - "FixedTimeController with phase cycling (green/amber/red)"
  - "SignalPlan and SignalPhase definitions"
affects: [02-03, 02-04, 03-motorbike-pedestrian]

tech-stack:
  added: [thiserror, log]
  patterns: [pure-math-models, TDD-unit-tests, ballistic-stopping-guard]

key-files:
  created:
    - crates/velos-vehicle/src/idm.rs
    - crates/velos-vehicle/src/mobil.rs
    - crates/velos-vehicle/src/gridlock.rs
    - crates/velos-vehicle/src/types.rs
    - crates/velos-vehicle/src/error.rs
    - crates/velos-vehicle/src/lib.rs
    - crates/velos-signal/src/controller.rs
    - crates/velos-signal/src/plan.rs
    - crates/velos-signal/src/error.rs
    - crates/velos-signal/src/lib.rs
    - crates/velos-vehicle/tests/idm_tests.rs
    - crates/velos-vehicle/tests/mobil_tests.rs
    - crates/velos-vehicle/tests/gridlock_tests.rs
    - crates/velos-signal/tests/signal_tests.rs
  modified:
    - Cargo.toml

key-decisions:
  - "BFS visited-set for gridlock detection over Tarjan SCC -- simpler, sufficient for ~100 stopped agents"
  - "u32 agent IDs in gridlock graph for consistency with spatial index"
  - "v_eff=max(v,0.1) kickstart in IDM to avoid zero-speed division issues"
  - "gap_eff=max(gap,0.01) floor in IDM to avoid division by zero"
  - "Signal phases are approach-indexed (Vec<usize>) for flexibility"

patterns-established:
  - "Pure CPU math models: no dependencies beyond thiserror/log, f64 precision"
  - "Ballistic stopping guard: always use integrate_with_stopping_guard after idm_acceleration"
  - "Signal plan = Vec<SignalPhase> with auto-computed cycle_time"

requirements-completed: [VEH-01, VEH-02, NET-03, GRID-01]

duration: 5min
completed: 2026-03-06
---

# Phase 02 Plan 02: Vehicle Models + Signal Controller Summary

**IDM car-following with ballistic stopping guard, MOBIL lane-change with politeness=0.3, BFS gridlock detection, and fixed-time signal controller with green/amber/red phase cycling**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-06T14:32:35Z
- **Completed:** 2026-03-06T14:37:58Z
- **Tasks:** 2
- **Files modified:** 15

## Accomplishments
- IDM acceleration model with free-flow, following, and approaching-stopped-leader modes, clamped to [-9.0, a_max], with v_eff kickstart for zero-speed and gap_eff floor for division safety
- Ballistic stopping guard that prevents negative velocity: computes time-to-stop and integrates only to that point
- MOBIL lane-change decision with safety criterion (new follower decel >= -4.0) and incentive criterion (own advantage - politeness*follower disadvantage + bias > threshold)
- GridlockDetector with BFS cycle detection on HashMap<u32,u32> waiting graph -- finds circular waits, ignores linear chains
- VehicleType enum (Motorbike/Car/Pedestrian) with calibrated default IDM params per type and shared MOBIL defaults
- FixedTimeController with SignalPlan/SignalPhase: green/amber/red cycling, modular wrapping, approach-indexed phase lookup
- 34 total tests (9 IDM + 6 MOBIL + 8 gridlock + 11 signal), all passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: velos-vehicle crate (IDM + MOBIL + VehicleType + gridlock)** - `861c384` (feat)
2. **Task 2: velos-signal crate (fixed-time controller)** - `d8e23e3` (feat)

## Files Created/Modified
- `crates/velos-vehicle/src/idm.rs` - IDM car-following acceleration + ballistic stopping guard
- `crates/velos-vehicle/src/mobil.rs` - MOBIL lane-change safety + incentive decision
- `crates/velos-vehicle/src/gridlock.rs` - GridlockDetector + detect_cycles BFS
- `crates/velos-vehicle/src/types.rs` - VehicleType enum, default_idm_params, default_mobil_params
- `crates/velos-vehicle/src/error.rs` - VehicleError type
- `crates/velos-vehicle/src/lib.rs` - Module re-exports
- `crates/velos-vehicle/Cargo.toml` - Crate manifest (thiserror + log)
- `crates/velos-signal/src/controller.rs` - FixedTimeController with tick/reset/get_phase_state
- `crates/velos-signal/src/plan.rs` - SignalPlan, SignalPhase, PhaseState enum
- `crates/velos-signal/src/error.rs` - SignalError type
- `crates/velos-signal/src/lib.rs` - Module re-exports
- `crates/velos-signal/Cargo.toml` - Crate manifest (thiserror + log)
- `crates/velos-vehicle/tests/idm_tests.rs` - 9 IDM tests (free-flow, braking, stopping guard, clamp, kickstart)
- `crates/velos-vehicle/tests/mobil_tests.rs` - 6 MOBIL tests (safety, incentive, bias, politeness)
- `crates/velos-vehicle/tests/gridlock_tests.rs` - 8 gridlock tests (cycles, chains, empty, tail+cycle)
- `crates/velos-signal/tests/signal_tests.rs` - 11 signal tests (timing, wrap, reset, incremental)
- `Cargo.toml` - Added velos-vehicle and velos-signal to workspace members

## Decisions Made
- BFS visited-set for gridlock detection over Tarjan SCC: simpler, O(V+E) where V=stopped agents (~100), sufficient for POC scale
- u32 agent IDs in gridlock graph for consistency with spatial index types
- v_eff=max(v,0.1) in IDM: prevents division issues when vehicle is stopped while maintaining numerical stability
- gap_eff=max(gap,0.01) in IDM: prevents division by zero when vehicles are bumper-to-bumper
- Signal phases indexed by approach (Vec<usize>): flexible enough for any intersection geometry

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed MOBIL right_bias test floating-point boundary**
- **Found during:** Task 1 (MOBIL tests)
- **Issue:** Test used accel values that produced incentive=0.2 at threshold=0.2, which is a floating-point equality boundary (IEEE 754 representation of 0.3-0.1)
- **Fix:** Adjusted test values to produce incentive=0.1 (clearly below) and 0.3 (clearly above) to avoid fp boundary
- **Files modified:** crates/velos-vehicle/tests/mobil_tests.rs
- **Verification:** Test passes reliably
- **Committed in:** 861c384 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug fix in test)
**Impact on plan:** Test correctness fix only. No scope change.

## Issues Encountered
None - both crates compiled and passed all tests on first attempt (after the boundary fix).

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- velos-vehicle exports are ready for simulation loop integration in Plan 03/04
- velos-signal FixedTimeController ready to be attached to intersection nodes in the road graph
- Both crates are pure CPU, zero external dependencies (just thiserror+log), easy to integrate

## Self-Check: PASSED

All 14 created files verified on disk. Both commit hashes (861c384, d8e23e3) verified in git log.

---
*Phase: 02-road-network-vehicle-models-egui*
*Completed: 2026-03-06*
