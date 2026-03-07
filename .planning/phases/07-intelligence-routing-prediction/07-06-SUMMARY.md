---
phase: 07-intelligence-routing-prediction
plan: 06
subsystem: routing
tags: [reroute, cch, scheduling, cooldown, perception, cost-function]

# Dependency graph
requires:
  - phase: 07-02
    provides: "route_cost function and CostWeights/EdgeAttributes types"
  - phase: 07-03
    provides: "CCH customization and bidirectional Dijkstra query"
  - phase: 07-04
    provides: "PredictionService and PredictionOverlay with ArcSwap"
  - phase: 07-05
    provides: "GPU PerceptionResult with route_blocked and emergency flags"
provides:
  - "RerouteScheduler: staggered 1K agents/step round-robin evaluation"
  - "evaluate_reroute: cost-delta comparison with 30% threshold"
  - "PerceptionSnapshot: CPU-side flag decoding for reroute triggers"
  - "EdgeNodeMap: O(1) edge-to-node lookup for CCH queries"
  - "sim_reroute integration: wired into SimWorld frame loop"
affects: [phase-08, visualization, calibration]

# Tech tracking
tech-stack:
  added: []
  patterns: [staggered-scheduling, cooldown-tracking, priority-queue-displacement]

key-files:
  created:
    - "crates/velos-core/src/reroute.rs"
    - "crates/velos-gpu/src/sim_reroute.rs"
  modified:
    - "crates/velos-core/src/lib.rs"
    - "crates/velos-gpu/src/sim.rs"
    - "crates/velos-gpu/src/lib.rs"
    - "crates/velos-gpu/Cargo.toml"
    - "crates/velos-net/src/cch/mod.rs"

key-decisions:
  - "PerceptionSnapshot struct in velos-core avoids circular dependency on velos-gpu PerceptionResult"
  - "RouteEvalContext struct decouples evaluate_reroute from CCH/ECS for pure-logic testability"
  - "EdgeNodeMap separate from CCHRouter since CCH topology is graph-independent"
  - "sim_reroute.rs module follows existing SimWorld split pattern (sim_helpers, sim_mobil, etc.)"

patterns-established:
  - "Staggered scheduling: immediate priority queue displaces round-robin budget, never exceeds it"
  - "CPU-side reroute as pure function: RouteEvalContext decouples from ECS and CCH for unit testing"

requirements-completed: [INT-04, INT-05]

# Metrics
duration: 7min
completed: 2026-03-07
---

# Phase 7 Plan 6: Reroute Evaluation Summary

**CPU-side reroute scheduler with 1K agents/step staggered evaluation, 30% cost-delta threshold, and CCH-based alternative route computation wired into SimWorld frame loop**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-07T16:26:05Z
- **Completed:** 2026-03-07T16:33:23Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- RerouteScheduler with round-robin scheduling (1K/step), immediate trigger priority queue, and 30s per-agent cooldown
- evaluate_reroute pure function comparing current route cost vs CCH alternative with configurable 30% threshold
- Full SimWorld integration: init_reroute builds CCH with disk cache, step_reroute processes batches per frame
- 20 comprehensive unit tests across scheduler, evaluation, and integration logic

## Task Commits

Each task was committed atomically:

1. **Task 1: RerouteScheduler with staggered evaluation and cooldown** - `5d82bd4` (feat)
2. **Task 2: Wire reroute into simulation step** - `ea6bff1` (feat)

_Note: Task 1 used TDD pattern with tests and implementation in single commit._

## Files Created/Modified
- `crates/velos-core/src/reroute.rs` - RerouteScheduler, evaluate_reroute, PerceptionSnapshot, RouteEvalContext (17 tests)
- `crates/velos-core/src/lib.rs` - Added reroute module and public re-exports
- `crates/velos-gpu/src/sim_reroute.rs` - RerouteState, init_reroute, step_reroute, EdgeNodeMap usage (3 tests)
- `crates/velos-gpu/src/sim.rs` - Added RerouteState field to SimWorld
- `crates/velos-gpu/src/lib.rs` - Registered sim_reroute module
- `crates/velos-gpu/Cargo.toml` - Added velos-predict dependency
- `crates/velos-net/src/cch/mod.rs` - Added EdgeNodeMap for edge-to-node lookup

## Decisions Made
- **PerceptionSnapshot in velos-core**: Defined a lightweight copy of PerceptionResult in velos-core to avoid circular dependency (velos-core cannot depend on velos-gpu). Only carries fields relevant to reroute decisions.
- **RouteEvalContext struct**: Decouples evaluate_reroute from CCH router and ECS world, enabling pure-logic unit testing with mock data.
- **EdgeNodeMap separate from CCHRouter**: CCH topology is weight-independent and graph-structure-independent after construction. Edge endpoint mapping requires graph access, so it's a separate structure built at init time.
- **sim_reroute.rs as separate module**: Follows existing SimWorld split pattern (sim_helpers.rs, sim_mobil.rs, sim_lifecycle.rs) to keep individual files under 700 lines.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- Pre-existing test compilation error in `crates/velos-net/src/cleaning.rs` (missing `RoadEdge` import) -- out of scope, not caused by this plan's changes.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Intelligence loop complete: perception (GPU) -> reroute evaluation (CPU) -> route update (ECS)
- CCH router initialized with disk cache at startup, customized with free-flow weights
- Prediction service overlay consumed by cost function for prediction-informed rerouting
- Full population cycles through reroute evaluation in ~50s at 1K/step with 280K agents

---
*Phase: 07-intelligence-routing-prediction*
*Completed: 2026-03-07*
