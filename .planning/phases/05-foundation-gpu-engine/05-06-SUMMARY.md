---
phase: 05-foundation-gpu-engine
plan: 06
subsystem: gpu-engine
tags: [car-following, krauss, idm, gpu, wgsl, ecs, spawning]

# Dependency graph
requires:
  - phase: 05-04
    provides: "GPU wave-front dispatch with IDM+Krauss shader branching"
  - phase: 05-01
    provides: "CarFollowingModel enum and GpuAgentState types"
provides:
  - "CarFollowingModel attached at spawn for all vehicle agents"
  - "~30% Krauss / ~70% IDM ratio for car agents"
  - "GPU behavioral differentiation verified via integration tests"
affects: [06-agent-models, 07-intelligence-routing]

# Tech tracking
tech-stack:
  added: []
  patterns: ["RNG-based cf_model assignment at spawn time"]

key-files:
  created:
    - "crates/velos-gpu/tests/cf_model_switch.rs"
  modified:
    - "crates/velos-gpu/src/sim_lifecycle.rs"

key-decisions:
  - "RNG-based 30/70 Krauss/IDM assignment for cars; demand-config-driven assignment deferred to Phase 6"
  - "Motorbikes always IDM (sublane model is IDM-based)"
  - "Pedestrians excluded from CarFollowingModel (social force, not car-following)"

patterns-established:
  - "Vehicle type determines cf_model assignment strategy at spawn"
  - "GPU tests gated behind gpu-tests feature flag"

requirements-completed: [CFM-02]

# Metrics
duration: 6min
completed: 2026-03-07
---

# Phase 5 Plan 6: CarFollowingModel Spawn Wiring Summary

**Wired CarFollowingModel into agent spawning: ~30% Krauss cars produce 92.8% lower avg speed than IDM on GPU**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-07T13:31:10Z
- **Completed:** 2026-03-07T13:37:15Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments
- Closed the single Phase 5 verification gap: agents now spawn with explicit CarFollowingModel
- GPU shader branching confirmed working: Krauss agents have 92.8% lower avg speed than IDM after 50 steps
- 6 integration tests covering spawn assignment (4) and GPU behavioral differentiation (2)

## Task Commits

Each task was committed atomically:

1. **Task 1: Attach CarFollowingModel to spawned agents** - `45ce173` (test: RED), `173bb7a` (feat: GREEN)
2. **Task 2: Verify GPU shader produces different Krauss vs IDM behavior** - `c9f99c0` (test: GPU behavioral verification)

## Files Created/Modified
- `crates/velos-gpu/src/sim_lifecycle.rs` - Added CarFollowingModel import and assignment in spawn_single_agent()
- `crates/velos-gpu/tests/cf_model_switch.rs` - 6 integration tests (4 spawn, 2 GPU behavior)

## Decisions Made
- RNG-based 30/70 Krauss/IDM assignment for cars (uses `self.rng.gen_ratio(3, 10)`); full demand-config-driven assignment deferred to Phase 6 when demand config is extended
- Motorbikes always IDM because sublane model is IDM-based
- Pedestrians excluded from CarFollowingModel entirely (they use social force, not car-following)
- Kept `Option<&CarFollowingModel>` query in sim.rs with `unwrap_or(Idm)` fallback for defensive handling of legacy entities

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Phase 5 verification gap fully closed
- CarFollowingModel is now wired end-to-end: spawn -> ECS -> GPU shader -> different behavior
- Ready for Phase 6 (Agent Models & Signals) which can extend demand config to control cf_model per vehicle type

---
*Phase: 05-foundation-gpu-engine*
*Completed: 2026-03-07*
