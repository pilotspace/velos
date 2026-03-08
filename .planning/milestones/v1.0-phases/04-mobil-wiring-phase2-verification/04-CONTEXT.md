# Phase 4: MOBIL Wiring + Motorbike Jam Fix + Performance - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire the existing MOBIL lane-change model into the simulation loop so cars actually change lanes, fix motorbike traffic jam/clustering at intersections, optimize spatial query performance for 800+ agents, create formal VERIFICATION.md for Phases 2 and 3, and fix APP-01/APP-02 documentation status. Gap closure phase expanded with bug fix and performance work.

</domain>

<decisions>
## Implementation Decisions

### Lane-change execution
- Gradual lateral drift over 2 seconds (not instant swap)
- Reuse existing `LateralOffset` component from motorbike sublane model — cars get a temporary `LateralOffset` during lane change, drift toward target lane center, component removed when transition complete
- Lane changes only on road segments — no lane changes within intersection boxes
- `RoadPosition.lane` already exists (u8, 0-based from right) — update lane index when transition completes (at midpoint or end of drift)

### MOBIL evaluation frequency
- Claude's discretion on evaluation frequency (every tick vs periodic)
- Claude's discretion on post-lane-change cooldown (recommended 3s to prevent oscillation)

### Motorbike traffic jam fix
- Motorbikes are permanently clustering/jamming at intersections at 800+ agents (see screenshots)
- Root causes to investigate: IDM leader detection lateral_dist threshold (1.5m) causing excessive braking in dense clusters, sublane gap-seeking not finding gaps when density is high, intersection dispersal too slow after green
- Fix must ensure motorbikes flow through intersections without permanent jam
- Claude's discretion on specific fixes — likely combination of IDM parameter tuning, gap-seeking improvements, and intersection behavior adjustments

### Performance optimization
- Frame time is 52-58ms at ~900 agents — target < 33ms (30 FPS) at 1000 agents
- Primary bottleneck: `step_motorbikes_sublane()` calls `nearest_within_radius(pos, 10.0)` for each of ~800 motorbikes — in dense clusters returns 50+ neighbors per bike
- Optimization approaches (Claude's discretion): reduce spatial query radius, cap neighbor count, early-exit in neighbor processing, grid-based spatial index for better cache locality
- Must not change simulation behavior significantly — optimizations should be accuracy-preserving

### Verification approach
- Code inspection + existing unit tests as evidence — reference specific files/functions for each requirement
- Cover both Phase 2 AND Phase 3 requirements in verification (separate files: 02-VERIFICATION.md and 03-VERIFICATION.md)
- Pass/fail for each requirement with evidence trail

### Documentation fixes
- Mark APP-01 and APP-02 as Complete in REQUIREMENTS.md traceability table (egui controls and dashboard already implemented in Phase 2)
- Update ROADMAP.md checkboxes to reflect completion

### Claude's Discretion
- MOBIL evaluation frequency (every tick vs periodic timer)
- Lane-change cooldown duration and implementation
- `LaneChangeContext` population strategy (how to find leaders/followers in adjacent lanes)
- Lane-change animation curve (linear vs easing)
- Specific motorbike jam fixes (parameter tuning, algorithm changes)
- Performance optimization strategy (spatial query radius, neighbor cap, data structure changes)
- Verification document format and structure

</decisions>

<specifics>
## Specific Ideas

- The gradual lane-change drift should look smooth visually — cars sliding sideways like real traffic, not teleporting
- Reusing `LateralOffset` keeps rendering consistent: motorbikes and cars both use the same world-position calculation path
- Motorbike jam visible at the intersection in screenshots — dense green clusters with 785-816 motorbikes not flowing
- Frame time 52-58ms at 900+ agents at 4x speed — performance degrades noticeably
- Verification should be thorough enough to close the milestone audit gaps but not over-engineered for a POC

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `mobil_decision()` (`velos-vehicle/src/mobil.rs:39-61`): Fully implemented MOBIL logic with safety + incentive criteria — just needs to be called
- `MobilParams` (`velos-vehicle/src/mobil.rs:9-20`): HCMC defaults with politeness=0.3
- `LaneChangeContext` (`velos-vehicle/src/mobil.rs:22-37`): All fields defined, needs IDM evaluations to populate
- `LateralOffset` component: Used by motorbikes in sublane model — can be attached to cars during lane-change transition
- `RoadPosition.lane` (`velos-core/src/components.rs:38-39`): Lane field exists but currently unused by cars

### Performance-Critical Code
- `SpatialIndex::from_positions()` (`velos-net/src/spatial.rs:38-51`): RTree bulk_load O(n log n) — rebuilt every tick
- `nearest_within_radius()` (`velos-net/src/spatial.rs:60-68`): RTree radius query — called once per agent per step function
- `step_motorbikes_sublane()` (`sim.rs:297-460`): Main motorbike loop — 800+ iterations with 10m spatial query each
- `AgentSnapshot::collect()`: Full ECS scan every tick

### Motorbike Jam Code
- `step_motorbikes_sublane()` IDM leader detection: `lateral_dist < 1.5` threshold (sim.rs ~line 405)
- `sublane::compute_desired_lateral()`: gap-seeking algorithm
- Swarming behavior at red lights: `at_red && idm_gap > 2.0` sets gap to 2.0

### Established Patterns
- f64 CPU / f32 GPU: all physics runs in f64 on CPU
- `step_vehicles()` filters to Car type, runs IDM — MOBIL evaluation goes here
- `edge_agents` HashMap groups agents by edge for leader-finding — extend to group by lane for adjacent-lane queries
- `integrate_with_stopping_guard` for safe velocity integration

### Integration Points
- `SimWorld::step_vehicles()` (`sim.rs:220-295`): Add MOBIL evaluation after IDM acceleration calculation
- `SimWorld::step_motorbikes_sublane()` (`sim.rs:297-460`): Optimize neighbor queries, fix jam behavior
- `build_render_instances()`: Already reads `LateralOffset` for motorbikes — cars with `LateralOffset` will render correctly automatically
- `apply_vehicle_update()`: May need extension to handle lane changes (update `RoadPosition.lane`)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 04-mobil-wiring-phase2-verification*
*Context gathered: 2026-03-07 (updated with motorbike jam + performance scope)*
