---
phase: 10-sim-loop-integration-bus-dwell-meso-micro
plan: 01
subsystem: simulation
tags: [bus-dwell, gpu-flags, ecs, state-machine, wgsl]

# Dependency graph
requires:
  - phase: 06-agent-models-signal-control
    provides: BusState, BusDwellModel, BusStop types in velos-vehicle
  - phase: 09-sim-loop-startup-frame-pipeline
    provides: tick_gpu() 10-step pipeline, SimWorld::new_cpu_only()
provides:
  - step_bus_dwell() CPU function called every frame in tick_gpu/tick
  - BusState attached to bus entities at spawn with route-matched stop indices
  - FLAG_BUS_DWELLING propagation from CPU to GPU shader
  - GPU wave_front.wgsl dwelling guard (zero speed for dwelling buses)
affects: [10-02-meso-micro, future-gtfs-integration]

# Tech tracking
tech-stack:
  added: []
  patterns: [cpu-state-machine-gpu-flag, split-module-for-700-line-limit]

key-files:
  created:
    - crates/velos-gpu/src/sim_bus.rs
  modified:
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/sim_lifecycle.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-gpu/shaders/wave_front.wgsl
    - crates/velos-gpu/tests/integration_bus_dwell.rs

key-decisions:
  - "step_bus_dwell() made pub (not pub(crate)) for integration test access"
  - "Bus stop indices pre-computed before path_u32 ownership transfer in spawn"
  - "Stochastic passenger counts: boarding uniform(0..capacity*0.3), alighting uniform(0..3)"

patterns-established:
  - "CPU state machine + GPU flag: complex branching on CPU, simple flag read on GPU"

requirements-completed: [AGT-01]

# Metrics
duration: 4min
completed: 2026-03-08
---

# Phase 10 Plan 01: Bus Dwell Wiring Summary

**Bus dwell lifecycle wired into sim loop: CPU state machine drives begin_dwell/tick_dwell, GPU shader holds dwelling buses at zero speed via FLAG_BUS_DWELLING**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-08T07:22:40Z
- **Completed:** 2026-03-08T07:26:49Z
- **Tasks:** 1
- **Files modified:** 6

## Accomplishments
- Bus agents spawn with BusState containing route-matched stop indices
- step_bus_dwell() called every frame after vehicle physics in both tick_gpu() and tick()
- FLAG_BUS_DWELLING set in GpuAgentState.flags for dwelling buses, GPU holds at zero speed
- Dwell completion clears flag and advances to next stop index
- 6 integration tests confirm full dwell lifecycle

## Task Commits

Each task was committed atomically:

1. **Task 1 (RED): Failing bus dwell tests** - `ce2b44b` (test)
2. **Task 1 (GREEN): Bus dwell wiring implementation** - `ef528fa` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/sim_bus.rs` - step_bus_dwell() CPU dwell state machine (new file)
- `crates/velos-gpu/src/sim.rs` - bus_stops/bus_dwell_model fields, FLAG_BUS_DWELLING in GPU upload, pipeline wiring
- `crates/velos-gpu/src/sim_lifecycle.rs` - BusState::new() attached at bus spawn with route-matched stops
- `crates/velos-gpu/src/lib.rs` - mod sim_bus declaration
- `crates/velos-gpu/shaders/wave_front.wgsl` - Dwelling guard: skip IDM, hold speed=0 for FLAG_BUS_DWELLING
- `crates/velos-gpu/tests/integration_bus_dwell.rs` - 6 integration tests for dwell lifecycle

## Decisions Made
- step_bus_dwell() made `pub` instead of `pub(crate)` so integration tests (external to crate) can call it directly
- Bus stop indices pre-computed before path_u32 moves into Route component, avoiding ownership conflict
- Simple uniform RNG for passenger counts (not Poisson) since rand_distr is not a dependency; sufficient for engine proof

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed ownership conflict in spawn_single_agent**
- **Found during:** Task 1 (GREEN phase)
- **Issue:** path_u32 moved into Route before bus stop index computation tried to borrow it
- **Fix:** Pre-computed bus_stop_indices before base_components tuple creation
- **Files modified:** crates/velos-gpu/src/sim_lifecycle.rs
- **Verification:** Compilation succeeds, tests pass
- **Committed in:** ef528fa (Task 1 GREEN commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary reordering for Rust ownership semantics. No scope creep.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Bus dwell wiring complete, ready for Plan 10-02 (meso-micro hybrid zones)
- bus_stops Vec is empty by default; future GTFS loading will populate it
- All 56 lib tests + 6 integration tests green, zero regressions

---
*Phase: 10-sim-loop-integration-bus-dwell-meso-micro*
*Completed: 2026-03-08*
