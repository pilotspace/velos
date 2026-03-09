---
phase: 16-intersection-sublane-model
plan: 02
subsystem: junction-traversal
tags: [bezier, junction, frame-pipeline, conflict-detection, idm-yielding, anti-flicker]

# Dependency graph
requires:
  - "BezierTurn struct with position/tangent/offset_position evaluation (Plan 01)"
  - "ConflictPoint struct for precomputed curve crossing points (Plan 01)"
  - "JunctionData aggregating turns and conflicts per junction node (Plan 01)"
  - "JunctionTraversal ECS component with wait_ticks deadlock guard (Plan 01)"
provides:
  - "advance_on_bezier() for uniform-speed Bezier t-advancement"
  - "check_conflicts() for crossing turn conflict detection with priority ordering"
  - "yield_deceleration() for IDM-based deceleration toward virtual leader"
  - "step_junction_traversal() frame pipeline step at position 6.8"
  - "Junction-aware advance_to_next_edge returning blocked status"
  - "junction_data HashMap<u32, JunctionData> on SimWorld"
  - "Junction traversal state in AgentSnapshot for rendering"
affects: [16-04 junction rendering, sim_render NaN guard, cpu_reference junction skip]

# Tech tracking
tech-stack:
  added: []
  patterns: [virtual-leader-idm-yielding, approach-phase-gap-acceptance, bezier-t-proximity-conflict, deadlock-forced-crawl]

key-files:
  created:
    - crates/velos-vehicle/src/junction_traversal.rs
    - crates/velos-gpu/src/sim_junction.rs
  modified:
    - crates/velos-vehicle/src/lib.rs
    - crates/velos-gpu/src/sim_helpers.rs
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/sim_snapshot.rs
    - crates/velos-gpu/src/sim_vehicles.rs
    - crates/velos-gpu/src/sim_render.rs
    - crates/velos-gpu/src/cpu_reference.rs
    - crates/velos-gpu/src/lib.rs

key-decisions:
  - "Local ConflictPoint struct in velos-vehicle to avoid circular dependency with velos-net"
  - "VehicleType conversion function (to_veh_vtype) bridges velos-core and velos-vehicle enum types"
  - "Junction entry blocked when foe agent within 0.3 t-distance of conflict crossing point"
  - "advance_to_next_edge returns bool (blocked) to enable caller-side anti-flicker logic"

patterns-established:
  - "Virtual leader IDM yielding: treat conflict crossing point as stationary obstacle"
  - "Approach-phase gap acceptance: check foe proximity before attaching JunctionTraversal"
  - "Immediate Bezier position on junction entry to prevent stale-frame rendering"

requirements-completed: [ISL-01, ISL-02, ISL-04]

# Metrics
duration: 11min
completed: 2026-03-09
---

# Phase 16 Plan 02: Junction Traversal Logic Summary

**Junction traversal frame pipeline integration with Bezier curve advancement, conflict detection, IDM yielding, and all 7 anti-flicker bug fixes from failed branch**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-09T12:30:50Z
- **Completed:** 2026-03-09T12:41:45Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- advance_on_bezier advances t proportionally to speed/arc_length with clamping at 1.0
- check_conflicts detects crossing turn conflicts using t-proximity and resolves priority by distance-to-crossing with vehicle type tie-breaking
- yield_deceleration produces IDM-based deceleration toward virtual leader at conflict crossing point
- step_junction_traversal integrates at frame pipeline step 6.8 with full conflict detection loop
- advance_to_next_edge intercepts junction entry, attaches JunctionTraversal, sets Bezier(t=0) position immediately
- All 7 anti-flicker bug fixes from failed branch incorporated:
  - Bug 1: Position set to Bezier(t=0) immediately on junction entry
  - Bug 2: update_agent_state skipped for junction-traversing agents
  - Bug 3: Speed zeroed and offset clamped when gap acceptance blocks
  - Bug 4: Junction exit directly places on exit edge (no re-entry loop)
  - Bug 5: wait_ticks deadlock prevention with forced crawl after 100 ticks
  - Bug 6: Junction-traversing agents skipped in GPU, CPU car, and CPU motorbike physics
  - Bug 7: NaN/Inf position guard in build_instances
- 23 unit tests (17 in velos-vehicle, 6 in velos-gpu) covering all functions and edge cases

## Task Commits

Each task was committed atomically:

1. **Task 1: Junction traversal pure logic in velos-vehicle** - `b96608a` (feat)
2. **Task 2: Frame pipeline integration with anti-flicker fixes** - `f959517` (feat)

## Files Created/Modified
- `crates/velos-vehicle/src/junction_traversal.rs` - advance_on_bezier, check_conflicts, yield_deceleration, size_factor, ConflictPoint; 17 tests
- `crates/velos-vehicle/src/lib.rs` - Added `pub mod junction_traversal`
- `crates/velos-gpu/src/sim_junction.rs` - step_junction_traversal with conflict loop, deadlock guard, exit handling; 6 tests
- `crates/velos-gpu/src/sim_helpers.rs` - Junction-aware advance_to_next_edge returning bool, junction_entry_blocked, apply_vehicle_update with Bug 2 fix
- `crates/velos-gpu/src/sim.rs` - junction_data field on SimWorld, precompute_all_junctions initialization, step_junction_traversal wired at 6.8
- `crates/velos-gpu/src/sim_snapshot.rs` - junction_traversals field in AgentSnapshot
- `crates/velos-gpu/src/sim_vehicles.rs` - JunctionTraversal skip in GPU physics (Bug 6)
- `crates/velos-gpu/src/cpu_reference.rs` - JunctionTraversal skip in CPU car and motorbike physics (Bug 6)
- `crates/velos-gpu/src/sim_render.rs` - NaN/Inf position guard (Bug 7)
- `crates/velos-gpu/src/lib.rs` - Added `mod sim_junction`

## Decisions Made
- Local ConflictPoint struct in velos-vehicle mirrors velos-net version to avoid circular dependency (velos-net -> velos-vehicle already exists)
- VehicleType conversion function bridges the two separate enum definitions in velos-core and velos-vehicle
- Junction entry gap acceptance uses 0.3 t-distance threshold for foe proximity check
- advance_to_next_edge signature changed from `() -> ()` to `() -> bool` to propagate blocked state for anti-flicker logic

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Circular dependency velos-net <-> velos-vehicle**
- **Found during:** Task 1
- **Issue:** Plan specified `use velos_net::junction::ConflictPoint` but velos-net already depends on velos-vehicle
- **Fix:** Created local `ConflictPoint` struct in junction_traversal.rs mirroring the velos-net version
- **Files modified:** crates/velos-vehicle/src/junction_traversal.rs

**2. [Rule 3 - Blocking] VehicleType enum mismatch between crates**
- **Found during:** Task 2
- **Issue:** velos-core::VehicleType and velos-vehicle::types::VehicleType are identical but distinct types
- **Fix:** Added `to_veh_vtype()` conversion function in sim_junction.rs
- **Files modified:** crates/velos-gpu/src/sim_junction.rs

## Issues Encountered
- hecs query patterns differ between `query::<Q>().iter()` (returns components only) and `query_mut::<(Entity, Q)>()` (Entity as explicit type) -- resolved by matching existing codebase patterns

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Junction traversal is fully wired into both GPU and CPU frame pipelines
- AgentSnapshot includes junction traversal state for Plan 04 rendering
- ConflictPoint and priority resolution ready for Plan 04 debug overlay visualization
- Bezier tangent-based heading ready for Plan 04 agent shape rotation on curves

## Self-Check: PASSED

- [x] crates/velos-vehicle/src/junction_traversal.rs exists
- [x] crates/velos-gpu/src/sim_junction.rs exists
- [x] 16-02-SUMMARY.md exists
- [x] Commit b96608a exists (Task 1)
- [x] Commit f959517 exists (Task 2)
- [x] advance_to_next_edge returns bool
- [x] Position set to Bezier(t=0) on junction entry (Bug 1)
- [x] update_agent_state skipped for junction agents (Bug 2)
- [x] Speed zeroed on gap acceptance block (Bug 3)
- [x] Junction exit bypasses advance_to_next_edge (Bug 4)
- [x] wait_ticks deadlock prevention with forced crawl (Bug 5)
- [x] Junction agents skipped in GPU and CPU physics (Bug 6)
- [x] NaN/Inf guard in build_instances (Bug 7)

---
*Phase: 16-intersection-sublane-model*
*Completed: 2026-03-09*
