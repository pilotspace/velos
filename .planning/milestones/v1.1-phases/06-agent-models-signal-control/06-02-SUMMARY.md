---
phase: 06-agent-models-signal-control
plan: 02
subsystem: vehicle-models
tags: [bus, dwell-time, gtfs, transit, ecs]

requires:
  - phase: 06-01
    provides: "GpuAgentState with vehicle_type and flags fields, VehicleType enum with Bus variant"
provides:
  - "BusDwellModel with empirical dwell formula (5s + 0.5s/board + 0.67s/alight, capped 60s)"
  - "BusStop ECS component with edge_id/offset for road network attachment"
  - "BusState for stop progression and dwell lifecycle management"
  - "GTFS CSV import pipeline (load_gtfs_csv) producing BusRoute/BusSchedule structs"
  - "Test fixture with 2 HCMC bus routes and 5 stops"
affects: [06-03, 06-05, bus-spawning, signal-control]

tech-stack:
  added: []
  patterns: [empirical-dwell-model, csv-gtfs-import, stop-proximity-detection]

key-files:
  created:
    - crates/velos-vehicle/src/bus.rs
    - crates/velos-demand/src/gtfs.rs
    - crates/velos-vehicle/tests/bus_tests.rs
    - crates/velos-demand/tests/gtfs_tests.rs
    - data/gtfs/test_fixture/routes.txt
    - data/gtfs/test_fixture/stops.txt
    - data/gtfs/test_fixture/trips.txt
    - data/gtfs/test_fixture/stop_times.txt
  modified:
    - crates/velos-vehicle/src/lib.rs
    - crates/velos-demand/src/lib.rs
    - crates/velos-demand/src/error.rs
    - .gitignore

key-decisions:
  - "CSV-native GTFS parser instead of gtfs-structures crate -- avoids heavy dependency, handles non-standard HCMC data"
  - "Stop proximity threshold of 5m for bus stop detection -- balances accuracy with GPS/positioning noise"
  - "Passenger counts are caller-provided (stochastic via RNG) not internally generated -- clean separation of concerns"

patterns-established:
  - "Empirical dwell model: fixed_time + per_boarding * count + per_alighting * count, capped"
  - "GTFS CSV parsing: read_required -> parse_csv -> typed struct conversion with warning-on-malformed"
  - "Stop progression: ordered indices into external BusStop vec, auto-advance on dwell complete"

requirements-completed: [AGT-01, AGT-02]

duration: 8min
completed: 2026-03-07
---

# Phase 6 Plan 02: Bus Dwell Model and GTFS Import Summary

**Empirical bus dwell model (Levinson formula) with BusStop/BusState ECS components and CSV-based GTFS import for HCMC routes**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-07T14:31:47Z
- **Completed:** 2026-03-07T14:40:00Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments
- BusDwellModel computing dwell as 5s + 0.5s/boarding + 0.67s/alighting, capped at 60s
- BusStop component mapping stops to road network edges via edge_id + offset_m
- BusState tracking stop progression with should_stop proximity detection and dwell tick lifecycle
- GTFS CSV import parsing routes.txt, stops.txt, trips.txt, stop_times.txt into typed BusRoute/BusSchedule
- DemandError extended with Io, Parse, MissingFile variants for GTFS error handling
- Test fixture with 2 HCMC bus routes (Ben Thanh - Cho Lon, Nguyen Hue - Binh Thanh) and 5 stops

## Task Commits

Each task was committed atomically:

1. **Task 1: Bus dwell model and BusStop ECS component** - `7af88b8` (feat)
2. **Task 2: GTFS import for HCMC bus routes** - `4a903ec` (feat)

_Both tasks followed TDD: RED (failing tests) -> GREEN (implementation) -> verify_

## Files Created/Modified
- `crates/velos-vehicle/src/bus.rs` - BusDwellModel, BusStop, BusState with dwell computation and stop lifecycle
- `crates/velos-vehicle/src/lib.rs` - Added `pub mod bus`
- `crates/velos-vehicle/tests/bus_tests.rs` - 10 tests for dwell formula, stop detection, state lifecycle
- `crates/velos-demand/src/gtfs.rs` - GTFS CSV parser with GtfsStop, BusRoute, BusSchedule, StopTime
- `crates/velos-demand/src/lib.rs` - Added `pub mod gtfs` and re-exports
- `crates/velos-demand/src/error.rs` - Added Io, Parse, MissingFile error variants
- `crates/velos-demand/tests/gtfs_tests.rs` - 6 tests for CSV parsing, coordinates, ordering
- `data/gtfs/test_fixture/` - Minimal GTFS fixture (routes, stops, trips, stop_times)
- `.gitignore` - Added data/gtfs/ exception for test fixtures

## Decisions Made
- Used CSV-native parser instead of `gtfs-structures` crate -- HCMC GTFS data may be non-standard, and avoiding the dependency keeps the build lean (KISS)
- Stop proximity threshold set to 5m -- tight enough for accuracy, wide enough for positioning noise
- Passenger counts provided by caller, not generated inside BusState -- clean separation between stochastic demand and deterministic state machine
- Stop ordering derived from first trip's stop_times per route -- standard GTFS practice

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] .gitignore excluded data/gtfs/ test fixtures**
- **Found during:** Task 2 (committing test fixture files)
- **Issue:** `data/*` gitignore pattern blocked tracking of GTFS test fixture files
- **Fix:** Added `!data/gtfs/` and `!data/gtfs/test_fixture/` exceptions to .gitignore
- **Files modified:** .gitignore
- **Verification:** git add succeeds, files tracked
- **Committed in:** 4a903ec (Task 2 commit)

**2. [Rule 1 - Bug] Clippy type_complexity warning on parse_csv return type**
- **Found during:** Task 2 (clippy verification)
- **Issue:** `Option<(Vec<String>, Vec<HashMap<String, String>>)>` flagged as too complex
- **Fix:** Extracted `type CsvData` alias
- **Files modified:** crates/velos-demand/src/gtfs.rs
- **Verification:** `cargo clippy -p velos-demand -- -D warnings` passes
- **Committed in:** 4a903ec (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes necessary for correctness. No scope creep.

## Issues Encountered
- Pre-existing compilation error in velos-net (cleaning.rs:322 `RoadEdge` not in scope) causes workspace-wide `cargo test` to fail. Not related to this plan's changes. Individual crate tests (velos-vehicle, velos-demand) all pass.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Bus dwell model ready for GPU shader integration (FLAG_BUS_DWELLING in GpuAgentState.flags)
- GTFS import ready for bus spawning pipeline integration
- BusStop ready for ECS attachment to road network edges
- Pending: bus spawner integration and schedule-aware speed adjustment (future plans)

---
## Self-Check: PASSED

All 8 created files verified present. Both task commits (7af88b8, 4a903ec) verified in git log.

---
*Phase: 06-agent-models-signal-control*
*Completed: 2026-03-07*
