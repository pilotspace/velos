---
phase: 14-wire-gtfs-bus-stops-pipeline
plan: 02
subsystem: gpu-sim
tags: [gtfs, bus-stops, sim-startup, bus-spawner, lifecycle, integration]

requires:
  - phase: 14-wire-gtfs-bus-stops-pipeline
    plan: 01
    provides: "snap_gtfs_stops R-tree snapping, BusSpawner time-gated spawning"
provides:
  - "GTFS bus stops populated at SimWorld startup via load_gtfs_bus_stops"
  - "BusSpawner wired into spawn_agents for time-gated GTFS bus agent creation"
  - "E2E bus dwell lifecycle: GTFS load -> bus_stops populated -> bus arrives -> dwell triggers"
affects: [sim-world-bus-pipeline, bus-dwell-lifecycle]

tech-stack:
  added: []
  patterns: ["Graceful GTFS degradation with env-var-configured path", "Name-based stop_id_to_index mapping for merged stops"]

key-files:
  created: []
  modified:
    - crates/velos-gpu/src/sim_startup.rs
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/sim_lifecycle.rs
    - crates/velos-vehicle/src/bus.rs

key-decisions:
  - "Name-based stop_id_to_index mapping (BusStop.name matches GtfsStop.name) -- simpler than re-projecting each stop"
  - "GTFS buses spawn at first stop position interpolated along edge -- avoids complex multi-stop route computation at spawn time"
  - "BusState::stop_indices() public accessor added for test introspection and future analytics"

patterns-established:
  - "Env-var-gated optional subsystem loading with graceful degradation (empty result + log::info)"
  - "GTFS bus entities use precomputed stop_indices from BusSpawnRequest, not edge-matching heuristic"

requirements-completed: [AGT-01, AGT-02]

duration: 7min
completed: 2026-03-08
---

# Phase 14 Plan 02: SimWorld GTFS Integration Summary

**Wire GTFS stop snapping and BusSpawner into SimWorld startup and tick lifecycle, activating the full bus dwell pipeline end-to-end**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-08T15:07:22Z
- **Completed:** 2026-03-08T15:15:03Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- load_gtfs_bus_stops() in sim_startup.rs: env-var-gated GTFS CSV loading, R-tree snapping, BusSpawner construction
- SimWorld.bus_spawner field wired into struct initialization and new()/new_cpu_only()
- GTFS loading positioned after init_reroute() for CCH availability
- spawn_gtfs_bus() in sim_lifecycle.rs: GTFS bus entity creation with route-specific BusState
- BusSpawner integrated into spawn_agents() alongside existing OD spawner
- BusState::stop_indices() public accessor added
- 10 new tests (5 sim_startup + 5 sim_lifecycle), all passing
- Full workspace tests: 0 failures

## Task Commits

Each task was committed atomically:

1. **Task 1: GTFS loading function in sim_startup.rs and SimWorld wiring** - `8b43233` (feat)
2. **Task 2: Wire BusSpawner into spawn_agents and validate E2E dwell activation** - `9e9ede1` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/sim_startup.rs` - load_gtfs_bus_stops(), build_stop_id_mapping() + 5 tests
- `crates/velos-gpu/src/sim.rs` - bus_spawner: Option<BusSpawner> field, GTFS loading in new()
- `crates/velos-gpu/src/sim_lifecycle.rs` - spawn_gtfs_bus(), BusSpawner integration in spawn_agents() + 5 tests
- `crates/velos-vehicle/src/bus.rs` - BusState::stop_indices() public accessor

## Decisions Made
- Name-based stop_id_to_index mapping instead of re-projection: GtfsStop.name is preserved in BusStop.name by snap_gtfs_stops, making name matching reliable for GTFS datasets where stop names are unique
- GTFS buses spawn at first stop position (interpolated along edge geometry), with minimal route path containing start/end edge nodes
- BusState::stop_indices() accessor added as public method for test access and future analytics

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added BusState::stop_indices() public accessor**
- **Found during:** Task 2 (test writing)
- **Issue:** BusState.stop_indices was private, preventing test assertions on spawned bus stop_indices
- **Fix:** Added `pub fn stop_indices(&self) -> &[usize]` to BusState
- **Files modified:** crates/velos-vehicle/src/bus.rs
- **Commit:** 9e9ede1

**2. [Rule 3 - Blocking] Used unsafe blocks for env var mutation in tests**
- **Found during:** Task 1 (test compilation)
- **Issue:** Rust 2024 edition requires unsafe blocks for std::env::set_var/remove_var
- **Fix:** Wrapped env var mutations in unsafe blocks with SAFETY comments
- **Files modified:** crates/velos-gpu/src/sim_startup.rs
- **Commit:** 8b43233

## Issues Encountered
None

## User Setup Required
None - GTFS loading is opt-in via VELOS_GTFS_PATH env var (defaults to data/gtfs, gracefully degrades if absent).

## Next Phase Readiness
- Full bus dwell lifecycle is now active end-to-end: GTFS load -> bus_stops populated -> bus arrives at stop -> should_stop() triggers -> begin_dwell()
- Phase 14 complete -- all GTFS bus stop pipeline gaps closed
- Phase 15 can proceed with remaining gap closure items

---
*Phase: 14-wire-gtfs-bus-stops-pipeline*
*Completed: 2026-03-08*
