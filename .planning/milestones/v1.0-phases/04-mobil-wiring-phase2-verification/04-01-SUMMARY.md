---
phase: 04-mobil-wiring-phase2-verification
plan: 01
subsystem: vehicle-dynamics
tags: [mobil, lane-change, idm, ecs, lateral-drift, hecs]

# Dependency graph
requires:
  - phase: 02-road-network-vehicle-models-egui
    provides: "IDM car-following, MOBIL pure function, road graph with lane_count"
  - phase: 03-motorbike-sublane-pedestrians
    provides: "LateralOffset component, apply_lateral_world_offset helper, AgentSnapshot"
provides:
  - "MOBIL lane-change wired into step_vehicles() for car agents"
  - "LaneChangeState ECS component for tracking active lane changes"
  - "Gradual 2-second lateral drift with LateralOffset"
  - "Adjacent-lane leader/follower finding helpers"
  - "LastLaneChange cooldown tracking"
affects: [04-02, 04-03, rendering, vehicle-behavior]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "ECS attach/remove pattern for transient state (LaneChangeState lifecycle)"
    - "SimWorld impl split into sim_mobil.rs for MOBIL-specific logic"
    - "Heading-based neighbor filtering for direction-aware avoidance"

key-files:
  created:
    - crates/velos-gpu/src/sim_mobil.rs
    - crates/velos-gpu/tests/mobil_wiring_tests.rs
  modified:
    - crates/velos-core/src/components.rs
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/sim_helpers.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-gpu/src/sim_lifecycle.rs
    - crates/velos-gpu/src/sim_snapshot.rs

key-decisions:
  - "Extract MOBIL wiring into sim_mobil.rs to keep sim.rs under 700 lines"
  - "Linear drift interpolation over 2 seconds (constant lateral speed per frame)"
  - "Cars spawn with LateralOffset at lane 0 center to prevent flicker"
  - "Heading-based neighbor filtering (cos < 0 skip) prevents opposing-traffic deadlock"

patterns-established:
  - "LaneChangeState attach/remove lifecycle: spawn on MOBIL accept, remove on drift completion or edge transition"
  - "LastLaneChange component for 3-second cooldown between lane changes"
  - "Direction-aware spatial query: skip neighbors with heading diff >90 degrees"

requirements-completed: [VEH-02]

# Metrics
duration: 9min
completed: 2026-03-07
---

# Phase 4 Plan 01: Wire MOBIL Lane-Change Summary

**MOBIL lane-change wired into sim loop with gradual 2s lateral drift, heading-based neighbor filtering, and car spawn LateralOffset fix**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-06T18:21:13Z
- **Completed:** 2026-03-07
- **Tasks:** 2 (1 auto + 1 human-verify)
- **Files modified:** 8

## Accomplishments
- MOBIL lane-change evaluation wired into `step_vehicles()` -- cars now evaluate left/right adjacent lanes each tick and change lanes when benefit exceeds politeness threshold (0.3)
- Gradual 2-second lateral drift using LateralOffset component with linear interpolation, visible as smooth lane-change animation
- Safety guards: no lane changes at red signals, within 5m of edge start, within 20m of edge end, on single-lane roads, or within 3s cooldown
- Fixed car flicker bug (missing LateralOffset on non-drifting cars) and motorbike head-on deadlock (opposing-direction neighbor filtering)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add LaneChangeState component and wire MOBIL evaluation into step_vehicles** - `875454d` (feat)
2. **Task 2: Visual verification of MOBIL lane-change behavior** - `3dc4712` (fix: car flicker + motorbike head-on deadlock)

## Files Created/Modified
- `crates/velos-core/src/components.rs` - Added LaneChangeState and LastLaneChange ECS components
- `crates/velos-gpu/src/sim.rs` - Wired MOBIL evaluation into step_vehicles(), car spawn with LateralOffset
- `crates/velos-gpu/src/sim_mobil.rs` - Extracted MOBIL evaluation, lane-change start, and drift processing logic
- `crates/velos-gpu/src/sim_helpers.rs` - Added find_leader_in_lane() and find_follower_in_lane() helpers, edge transition lane-change cleanup
- `crates/velos-gpu/src/sim_lifecycle.rs` - Cars spawn with LateralOffset at lane 0 center
- `crates/velos-gpu/src/sim_snapshot.rs` - Added heading tracking for direction-aware filtering
- `crates/velos-gpu/src/lib.rs` - Registered sim_mobil module
- `crates/velos-gpu/tests/mobil_wiring_tests.rs` - 7 unit tests for lane-change state, drift math, boundary checks

## Decisions Made
- Extracted MOBIL wiring into `sim_mobil.rs` to keep `sim.rs` under 700 lines (was 827 before extraction)
- Linear drift interpolation (constant lateral velocity per frame) over 2 seconds -- simpler than easing, sufficient for POC
- Cars spawn with LateralOffset at lane 0 center (1.75m) to prevent flicker between lane-centered and road-centered positions
- Heading-based neighbor filtering: skip neighbors with heading difference >90 degrees (cos < 0) to prevent opposing-direction motorbikes from causing mutual braking deadlock
- OD boost increased from 10x to 50x for faster agent ramp-up during visual verification

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Car flicker between lane positions**
- **Found during:** Task 2 (visual verification)
- **Issue:** Cars without active lane change had no LateralOffset, causing position to alternate between lane-centered and road-centered rendering each frame
- **Fix:** Cars now spawn with LateralOffset at lane 0 center; on lane-change completion, LateralOffset is kept (only LaneChangeState removed); apply_lateral_world_offset runs every tick for non-drifting cars
- **Files modified:** sim.rs, sim_lifecycle.rs, sim_mobil.rs
- **Committed in:** 3dc4712

**2. [Rule 1 - Bug] Motorbike head-on deadlock at intersections**
- **Found during:** Task 2 (visual verification)
- **Issue:** Motorbikes on opposing-direction roads were detecting each other as neighbors, causing mutual braking and permanent clustering at intersections
- **Fix:** Added heading to AgentSnapshot; skip neighbors with heading diff >90 degrees (cos < 0) in motorbike sublane loop
- **Files modified:** sim.rs, sim_snapshot.rs
- **Committed in:** 3dc4712

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Both fixes necessary for correct visual behavior. No scope creep.

## Issues Encountered
- `query_one()` vs `query_one_mut()` API difference in hecs -- `query_one()` returns `QueryOne` wrapper requiring `.get()`, while `query_one_mut()` directly returns the component reference. Fixed by using `query_one_mut()` for cooldown check.
- Clippy type_complexity warning on 7-element tuple -- resolved by introducing `CarSnap` struct.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- MOBIL lane-change is fully wired and visually verified
- Ready for Plan 04-02 (motorbike jam fix + spatial query optimization)
- All quality gates pass: build clean, 139 tests (7 new), clippy clean

---
*Phase: 04-mobil-wiring-phase2-verification*
*Completed: 2026-03-07*
