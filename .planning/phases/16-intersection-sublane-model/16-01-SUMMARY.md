---
phase: 16-intersection-sublane-model
plan: 01
subsystem: network-geometry
tags: [bezier, quadratic-curve, junction, conflict-detection, ecs, petgraph]

# Dependency graph
requires: []
provides:
  - "BezierTurn struct with position/tangent/offset_position evaluation"
  - "ConflictPoint struct for precomputed curve crossing points"
  - "JunctionData aggregating turns and conflicts per junction node"
  - "precompute_junction() and precompute_all_junctions() functions"
  - "JunctionTraversal ECS component with wait_ticks deadlock guard"
  - "MAX_YIELD_TICKS constant (100) for deadlock detection"
affects: [16-02 junction traversal logic, 16-04 junction rendering, velos-vehicle junction_traversal]

# Tech tracking
tech-stack:
  added: []
  patterns: [quadratic-bezier-evaluation, grid-search-conflict-detection, pass-through-node-filtering]

key-files:
  created:
    - crates/velos-net/src/junction.rs
  modified:
    - crates/velos-net/src/lib.rs
    - crates/velos-core/src/components.rs
    - crates/velos-core/src/lib.rs

key-decisions:
  - "Filter pass-through nodes (in=1, out=1) from junction precomputation to prevent phantom guide lines"
  - "Minimum arc length threshold of 1.0m to filter degenerate Bezier curves"
  - "Grid search with 30 steps per curve and 2m distance threshold for conflict detection"
  - "Added wait_ticks field to JunctionTraversal for deadlock prevention (MAX_YIELD_TICKS=100)"
  - "Added exit_offset_m field to BezierTurn (default 0.1m) to avoid edge-boundary issues"

patterns-established:
  - "Quadratic Bezier evaluation: B(t) = (1-t)^2*P0 + 2(1-t)t*P1 + t^2*P2"
  - "Junction precomputation using RoadNode.pos for control points (not edge geometry polylines)"
  - "NaN guard pattern in find_conflict_point for degenerate curve safety"

requirements-completed: [ISL-01, ISL-03]

# Metrics
duration: 5min
completed: 2026-03-09
---

# Phase 16 Plan 01: Junction Geometry Summary

**Quadratic Bezier junction turn paths with conflict point precomputation, pass-through filtering, and JunctionTraversal ECS component with deadlock guard**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-09T12:20:51Z
- **Completed:** 2026-03-09T12:25:57Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- BezierTurn struct evaluates quadratic Bezier position, tangent, and lateral offset correctly at all t boundaries
- ConflictPoint detection via grid search finds crossing paths within 2m threshold with NaN guard
- precompute_all_junctions filters pass-through nodes and degenerate arcs, producing HashMap<u32, JunctionData>
- JunctionTraversal ECS component with wait_ticks field for deadlock prevention (MAX_YIELD_TICKS=100)
- 24 unit tests covering all geometric operations, edge cases, and component behavior

## Task Commits

Each task was committed atomically:

1. **Task 1: BezierTurn, ConflictPoint, and junction precomputation** - `f996d75` (feat)
2. **Task 2: JunctionTraversal ECS component** - `cb6512d` (feat)

## Files Created/Modified
- `crates/velos-net/src/junction.rs` - BezierTurn, ConflictPoint, JunctionData structs; estimate_arc_length, find_conflict_point, precompute_junction, precompute_all_junctions functions; 21 tests
- `crates/velos-net/src/lib.rs` - Added `pub mod junction` and re-exports for key types
- `crates/velos-core/src/components.rs` - Added JunctionTraversal component, MAX_YIELD_TICKS constant, 3 tests
- `crates/velos-core/src/lib.rs` - Added JunctionTraversal and MAX_YIELD_TICKS re-exports

## Decisions Made
- Filter pass-through nodes (in_degree==1 AND out_degree==1) from junction precomputation -- prevents phantom guide lines from road continuations being treated as junctions
- Minimum arc length check of 1.0m -- degenerate curves cause NaN tangents and visual artifacts
- Grid search conflict detection with 30 steps per curve -- O(900) per pair, sufficient accuracy for 10-30m junction curves
- Two identical curves produce a valid conflict point (distance 0 < 2m threshold) -- acceptable because precompute_junction never creates duplicate turns
- exit_offset_m defaults to 0.1m to prevent edge-boundary issues at offset=0

## Deviations from Plan

None - plan executed exactly as written. All critical lessons from failed branch were incorporated directly.

## Issues Encountered
- petgraph EdgeRef trait needed explicit import for source()/target()/id() methods on edge references -- standard Rust trait scoping, fixed immediately

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- BezierTurn and JunctionData are ready for Plan 02 (junction traversal logic in velos-vehicle)
- JunctionTraversal ECS component is ready for Plan 02 to attach/remove during simulation
- ConflictPoint data is ready for Plan 02 conflict resolution with IDM yielding
- Guide line rendering data is ready for Plan 04 (junction visualization)

## Self-Check: PASSED

- [x] crates/velos-net/src/junction.rs exists
- [x] crates/velos-core/src/components.rs exists
- [x] 16-01-SUMMARY.md exists
- [x] Commit f996d75 exists (Task 1)
- [x] Commit cb6512d exists (Task 2)
- [x] wait_ticks field present in JunctionTraversal
- [x] exit_offset_m field present in BezierTurn
- [x] Pass-through node filter present
- [x] Minimum arc length check present
- [x] NaN guard present in find_conflict_point

---
*Phase: 16-intersection-sublane-model*
*Completed: 2026-03-09*
