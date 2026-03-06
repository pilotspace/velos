---
phase: 03-motorbike-sublane-pedestrians
plan: 01
subsystem: vehicle-physics
tags: [sublane, social-force, helbing, motorbike, pedestrian, lateral-movement, jaywalking]

requires:
  - phase: 02-road-network-vehicle-models-egui
    provides: "IDM/MOBIL pure-function pattern, VehicleType enum, ECS components"
provides:
  - "LateralOffset ECS component for motorbike sublane positioning"
  - "compute_desired_lateral + apply_lateral_drift sublane functions"
  - "social_force_acceleration + integrate_pedestrian + should_jaywalk functions"
  - "Rng trait for deterministic social force testing"
affects: [03-02-integration, 03-03-mixed-traffic]

tech-stack:
  added: []
  patterns: ["TDD RED-GREEN for physics models", "Rng trait injection for stochastic tests", "probe-based gap scanning for lateral movement", "obstacle-edge sweep for swarming gap search"]

key-files:
  created:
    - crates/velos-vehicle/src/sublane.rs
    - crates/velos-vehicle/src/social_force.rs
    - crates/velos-vehicle/tests/sublane_tests.rs
    - crates/velos-vehicle/tests/social_force_tests.rs
  modified:
    - crates/velos-core/src/components.rs
    - crates/velos-vehicle/src/lib.rs

key-decisions:
  - "Probe-based gap scanning at 0.3m steps for sublane lateral gap-seeking"
  - "Obstacle-edge sweep algorithm for swarming gap search (not uniform probing)"
  - "Rng trait for social force jaywalking allows deterministic testing without external crate"
  - "Anisotropic weighting uses cos(phi) of ego velocity vs neighbor direction"

patterns-established:
  - "TDD RED-GREEN-REFACTOR for physics models: stubs fail first, then implement"
  - "Rng trait injection: no external rand dependency, tests use xorshift64"
  - "Pure functions with params struct: no ECS dependency in physics models"

requirements-completed: [VEH-03, VEH-04]

duration: 5min
completed: 2026-03-06
---

# Phase 3 Plan 1: Sublane & Social Force Models Summary

**Motorbike sublane gap-seeking with dt-consistent lateral drift and Helbing social force pedestrian model with anisotropic repulsion and jaywalking**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-06T16:42:16Z
- **Completed:** 2026-03-06T16:47:34Z
- **Tasks:** 2 (4 TDD commits: 2 RED + 2 GREEN)
- **Files modified:** 6

## Accomplishments
- Motorbike sublane model: probe-based gap scanning, red-light swarming with obstacle-edge sweep, dt-consistent forward-Euler drift
- Helbing social force model: driving force, exponential repulsion, anisotropic vision cone, force/speed explosion prevention
- Jaywalking decisions: 30% red light / 10% mid-block probability with TTC gap acceptance
- LateralOffset ECS component added to velos-core
- 18 integration tests + 7 unit tests all pass with clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: LateralOffset + sublane model (RED)** - `fd28481` (test)
2. **Task 1: LateralOffset + sublane model (GREEN)** - `65e8590` (feat)
3. **Task 2: Social force model (RED)** - `9bc7b82` (test)
4. **Task 2: Social force model (GREEN)** - `a02a903` (feat)

_TDD: each task has separate RED (failing tests) and GREEN (implementation) commits._

## Files Created/Modified
- `crates/velos-core/src/components.rs` - Added LateralOffset component (lateral_offset + desired_lateral)
- `crates/velos-vehicle/src/sublane.rs` - Motorbike sublane model: SublaneParams, NeighborInfo, compute_desired_lateral, apply_lateral_drift
- `crates/velos-vehicle/src/social_force.rs` - Helbing social force: SocialForceParams, PedestrianNeighbor, social_force_acceleration, integrate_pedestrian, should_jaywalk, Rng trait
- `crates/velos-vehicle/src/lib.rs` - Added pub mod sublane and social_force
- `crates/velos-vehicle/tests/sublane_tests.rs` - 8 tests covering gap-seeking, dt-consistency, swarming, boundary clamping
- `crates/velos-vehicle/tests/social_force_tests.rs` - 10 tests covering driving force, repulsion, anisotropy, clamping, jaywalking

## Decisions Made
- **Probe-based gap scanning** at 0.3m step resolution for sublane lateral movement (fast enough for CPU, sufficient precision for 0.6m min gap)
- **Obstacle-edge sweep** for swarming gap search instead of uniform probing (exact, O(n log n) sort vs O(n * probes))
- **Rng trait** for jaywalking randomness -- no external rand crate dependency; tests use xorshift64 SimpleRng
- **Anisotropic weighting** via cos(phi) between ego velocity and neighbor direction vector -- lambda=0.5 gives 50% reduction for agents directly behind

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- sublane.rs and social_force.rs are pure functions ready for integration wiring in sim.rs
- LateralOffset component ready to be added to motorbike entities in ECS
- Integration plan (03-02) can wire step_motorbikes_sublane() and replace step_pedestrians() body
- All locked parameters match CONTEXT.md: min_filter_gap=0.6, max_lateral_speed=1.0, jaywalking red=0.3/mid-block=0.1

---
*Phase: 03-motorbike-sublane-pedestrians*
*Completed: 2026-03-06*
