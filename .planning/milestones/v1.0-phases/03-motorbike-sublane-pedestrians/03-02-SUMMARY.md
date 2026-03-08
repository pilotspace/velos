---
phase: 03-motorbike-sublane-pedestrians
plan: 02
subsystem: simulation-integration
tags: [sublane-wiring, social-force-integration, spatial-index, swarming-color, cross-type-avoidance, lateral-offset]

requires:
  - phase: 03-motorbike-sublane-pedestrians
    provides: "sublane::compute_desired_lateral, social_force::social_force_acceleration, LateralOffset component"
  - phase: 02-road-network-vehicle-models-egui
    provides: "SimWorld tick loop, IDM car-following, signal controllers, SpatialIndex, RoadGraph"
provides:
  - "step_motorbikes_sublane() wired into SimWorld::tick() with sublane gap-seeking and lateral drift"
  - "step_pedestrians() replaced with Helbing social force model"
  - "Cross-type SpatialIndex built once per frame, shared across all step functions"
  - "LateralOffset spawned on motorbike entities with road_width/2 initial offset"
  - "Swarming motorbike color (brighter green at red lights)"
  - "Pedestrian sidewalk offset (5m from road centerline)"
  - "Cross-type avoidance: vehicles slow for pedestrians ahead"
affects: []

tech-stack:
  added: []
  patterns: ["per-frame AgentSnapshot for cross-type spatial queries", "split impl blocks across sim_*.rs modules for 700-line compliance", "position-proximity self-skip in spatial queries (no entity ID matching)"]

key-files:
  created:
    - crates/velos-gpu/src/sim_snapshot.rs
    - crates/velos-gpu/src/sim_helpers.rs
    - crates/velos-gpu/src/sim_lifecycle.rs
    - crates/velos-gpu/src/sim_render.rs
  modified:
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-gpu/src/renderer.rs

key-decisions:
  - "AgentSnapshot with sequential IDs and position-proximity self-skip instead of entity ID matching"
  - "Split SimWorld impl across 5 files (sim.rs + 4 extracted modules) to keep all files under 700 lines"
  - "Pedestrians walk on sidewalk (5m perpendicular offset from road centerline)"
  - "Softened vehicle-pedestrian avoidance (speed reduction, not hard stop)"

patterns-established:
  - "Per-frame AgentSnapshot: collect all agent state once, share across step functions via immutable reference"
  - "Cross-type spatial queries: SpatialIndex built from all agent types, step functions filter by VehicleType"
  - "Modular sim impl: sim.rs (tick+step logic), sim_helpers.rs (state updates), sim_lifecycle.rs (spawn/gridlock), sim_render.rs (instance building), sim_snapshot.rs (frame snapshot)"

requirements-completed: [VEH-03, VEH-04]

duration: 42min
completed: 2026-03-07
---

# Phase 3 Plan 2: Sublane & Social Force Integration Summary

**Motorbike sublane gap-seeking and pedestrian social force wired into SimWorld tick loop with cross-type spatial index, swarming color, and sidewalk offset**

## Performance

- **Duration:** 42 min
- **Started:** 2026-03-06T16:50:58Z
- **Completed:** 2026-03-07T16:52:52Z
- **Tasks:** 2 (1 auto + 1 human-verify checkpoint)
- **Files modified:** 7

## Accomplishments
- Motorbike sublane model integrated: step_motorbikes_sublane() runs per tick with compute_desired_lateral + apply_lateral_drift, world position offset perpendicular to heading
- Pedestrian social force replaces linear walk: social_force_acceleration + integrate_pedestrian with 10-neighbor spatial query
- Cross-type SpatialIndex built once per frame from all agents, enabling vehicle-pedestrian avoidance
- Swarming motorbikes at red lights render brighter green [0.4, 1.0, 0.5, 1.0] vs normal [0.2, 0.8, 0.4, 1.0]
- Pedestrians walk on sidewalk (5m offset from road centerline) for visual realism
- All sim files kept under 700 lines via extraction into 4 helper modules
- Visual verification approved: motorbike filtering, swarming, dispersal, and pedestrian social force all confirmed

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire sublane, social force, spatial index, and swarming color** - `5610c83` (feat)
2. **Fix: Swarming color blink** - `65732ad` (fix)
3. **Fix: Pedestrian sidewalk offset + avoidance tuning** - `909d0ea` (fix)
4. **Task 2: Visual verification** - approved by human

## Files Created/Modified
- `crates/velos-gpu/src/sim.rs` - tick() wiring: snapshot + spatial index + step_motorbikes_sublane + replaced step_pedestrians + step_vehicles car-only filter
- `crates/velos-gpu/src/sim_snapshot.rs` - AgentSnapshot struct: parallel vecs of agent state for spatial queries
- `crates/velos-gpu/src/sim_helpers.rs` - Extracted: check_signal_red, apply_vehicle_update, update_agent_state, apply_lateral_world_offset
- `crates/velos-gpu/src/sim_lifecycle.rs` - Extracted: spawn_agents (with LateralOffset for motorbikes), detect_gridlock, remove_finished, update_metrics
- `crates/velos-gpu/src/sim_render.rs` - Extracted: build_instances (swarming color), build_signal_indicators, road_edge_lines, network_center
- `crates/velos-gpu/src/lib.rs` - Added 4 new modules
- `crates/velos-gpu/src/renderer.rs` - Fixed clippy collapsible_if

## Decisions Made
- **AgentSnapshot with sequential IDs:** hecs immutable query does not expose Entity IDs, so snapshot uses sequential indices (0, 1, 2...) as IDs. Self-skip in spatial queries uses position proximity (dist < 0.01m) instead of entity ID comparison.
- **5-file SimWorld split:** sim.rs (587 lines) contains tick loop and step functions; helpers, lifecycle, render, snapshot extracted to keep all files under 700 lines.
- **Pedestrian sidewalk offset:** Pedestrians positioned 5m perpendicular from road centerline to avoid walking on the road surface.
- **Soft vehicle-pedestrian avoidance:** Vehicles reduce speed when pedestrians are ahead rather than hard-stopping, preventing unrealistic traffic behavior.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Swarming color blink at red lights**
- **Found during:** Task 2 visual verification
- **Issue:** Motorbike swarming color alternated between bright/normal green because `at_red_signal` was only set when `speed < 0.1` -- motorbikes approaching the stop line at speed appeared normal-colored
- **Fix:** Set `at_red_signal = at_red` regardless of speed in update_wait_state
- **Files modified:** `crates/velos-gpu/src/sim_helpers.rs`
- **Committed in:** `65732ad`

**2. [Rule 1 - Bug] Pedestrians walking on roads**
- **Found during:** Task 2 visual verification
- **Issue:** Pedestrians rendered directly on road centerlines, visually overlapping with vehicle traffic
- **Fix:** Added 5m perpendicular offset from road centerline for pedestrian spawn/walk positions; softened vehicle-pedestrian avoidance to speed reduction instead of hard stop
- **Files modified:** `crates/velos-gpu/src/sim_lifecycle.rs`, `crates/velos-gpu/src/sim.rs`
- **Committed in:** `909d0ea`

---

**Total deviations:** 2 auto-fixed (2 bugs found during visual verification)
**Impact on plan:** Both fixes improved visual realism without scope creep. Core integration logic unchanged.

## Issues Encountered

None beyond the deviations listed above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 3 complete: all VEH-03 and VEH-04 requirements satisfied
- VELOS POC milestone complete: GPU pipeline, road network, vehicle models, motorbike sublane, pedestrian social force all integrated
- Full simulation runs with HCMC District 1 road network, mixed traffic (motorbikes/cars/pedestrians), signal control, and visual verification

---
*Phase: 03-motorbike-sublane-pedestrians*
*Completed: 2026-03-07*
