---
phase: 14-wire-gtfs-bus-stops-pipeline
plan: 01
subsystem: demand
tags: [rstar, rtree, gtfs, bus-stops, snapping, spawner]

requires:
  - phase: 06-agent-models-signal-control
    provides: "GTFS CSV parser (BusRoute, BusSchedule, GtfsStop), BusStop/BusState structs"
provides:
  - "Edge R-tree snapping: snap_gtfs_stops converts WGS84 lat/lon to edge_id + offset_m"
  - "BusSpawner: time-gated bus spawn generation from GTFS trip schedules"
  - "stop_id_to_index mapping pattern for decoupled snapping/spawner construction"
affects: [14-02-PLAN, velos-core-sim-world-bus-integration]

tech-stack:
  added: []
  patterns: ["R-tree segment decomposition for polyline nearest-edge queries", "Cursor-based time-gated spawner with seconds-of-day matching"]

key-files:
  created:
    - crates/velos-net/src/snap.rs
    - crates/velos-demand/src/bus_spawner.rs
  modified:
    - crates/velos-net/src/lib.rs
    - crates/velos-net/Cargo.toml
    - crates/velos-demand/src/lib.rs

key-decisions:
  - "BusSpawner accepts stop_id_to_index HashMap instead of matching by name -- decouples snapping from spawner construction"
  - "velos-net depends on velos-demand and velos-vehicle for snap_gtfs_stops high-level function -- acceptable coupling since velos-net is the spatial pipeline crate"

patterns-established:
  - "EdgeSegment R-tree pattern: decompose edge polyline into segments with cumulative offset for accurate projection"
  - "Cursor-based spawner: sorted schedules + next_trip_index for O(1) per-tick spawn generation"

requirements-completed: [AGT-02]

duration: 3min
completed: 2026-03-08
---

# Phase 14 Plan 01: GTFS Bus Stop Snapping & Spawn Generation Summary

**Edge R-tree snapping pipeline converting WGS84 GTFS stops to edge_id+offset_m, plus BusSpawner time-gating bus agent creation by trip departure schedules**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-08T15:01:11Z
- **Completed:** 2026-03-08T15:04:26Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Edge segment R-tree with PointDistance for accurate nearest-segment queries on road graph polylines
- snap_gtfs_stops end-to-end pipeline: WGS84 projection -> R-tree nearest edge -> 50m radius filter -> 10m duplicate merge
- BusSpawner with cursor-based schedule traversal for O(1) per-tick spawn generation
- 15 unit tests total (9 snap, 6 bus_spawner) -- all passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Edge segment R-tree and stop snapping in velos-net** - `e512f93` (feat)
2. **Task 2: BusSpawner time-gated spawn generation in velos-demand** - `2b61eb6` (feat)

## Files Created/Modified
- `crates/velos-net/src/snap.rs` - EdgeSegment R-tree, snap_to_nearest_edge, snap_gtfs_stops with 50m radius and 10m merge
- `crates/velos-net/src/lib.rs` - Added snap module and public re-exports
- `crates/velos-net/Cargo.toml` - Added velos-demand and velos-vehicle dependencies
- `crates/velos-demand/src/bus_spawner.rs` - BusSpawner struct with time-gated generate_bus_spawns
- `crates/velos-demand/src/lib.rs` - Added bus_spawner module and public re-exports

## Decisions Made
- BusSpawner accepts `stop_id_to_index: &HashMap<String, usize>` instead of matching stops by name. This decouples the snapping pipeline from spawner construction, making Plan 02 integration cleaner.
- velos-net now depends on velos-demand (for GtfsStop) and velos-vehicle (for BusStop). This is acceptable since neither crate depends on velos-net, avoiding circular dependencies.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- snap_gtfs_stops and BusSpawner are ready for Plan 02 to wire into SimWorld
- Plan 02 needs to build the `stop_id_to_index` map during GTFS loading and pass it to BusSpawner::new()
- Route edge paths (CCH) will be computed at spawn time in Plan 02

---
*Phase: 14-wire-gtfs-bus-stops-pipeline*
*Completed: 2026-03-08*
