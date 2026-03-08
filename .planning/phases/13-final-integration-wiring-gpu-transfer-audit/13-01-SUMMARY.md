---
phase: 13-final-integration-wiring-gpu-transfer-audit
plan: 01
subsystem: gpu-engine
tags: [agent-profile, glosa, spat, ecs, gpu-flags, cost-function]

requires:
  - phase: 07-intelligence-routing
    provides: "CCH routing with multi-factor cost function and profile weights"
  - phase: 06-agent-models-signal-control
    provides: "SignalController trait with spat_data(), GLOSA speed advisory"
  - phase: 12-cpu-lane-change-sublane-wiring
    provides: "Full tick pipeline with step ordering"
provides:
  - "AgentProfile ECS component spawned on all agents from SpawnRequest.profile"
  - "GPU flags bits 4-7 carry profile ID via encode_profile_in_flags per frame"
  - "step_glosa() advisory speed system for agents approaching non-green signals"
  - "CPU/GPU parity: step_glosa in both tick_gpu() and tick()"
affects: [13-02, 13-03, rerouting, signal-consumption]

tech-stack:
  added: []
  patterns:
    - "Option<&AgentProfile> ECS query with Commuter default for backward compat"
    - "MockSignalController pattern for testing signal-dependent behavior"

key-files:
  created: []
  modified:
    - "crates/velos-gpu/src/sim_lifecycle.rs"
    - "crates/velos-gpu/src/sim.rs"
    - "crates/velos-gpu/src/compute.rs"
    - "crates/velos-gpu/src/sim_helpers.rs"
    - "crates/velos-gpu/tests/integration_emergency_wiring.rs"

key-decisions:
  - "Option<&AgentProfile> ECS query with Commuter default -- backward compatible with entities spawned before this change"
  - "step_glosa at step 4.5 (after signal priority, before perception) -- GLOSA-modified speeds captured by perception pipeline"
  - "MockSignalController for GLOSA tests -- avoids dependency on full graph topology for unit tests"

patterns-established:
  - "AgentProfile as ECS component: spawned per agent, queried via Option<&AgentProfile> with Commuter fallback"
  - "Signal-dependent test pattern: MockSignalController with configurable PhaseState and time_to_green"

requirements-completed: [INT-01, INT-02, SIG-03]

duration: 8min
completed: 2026-03-08
---

# Phase 13 Plan 01: Agent Profile Wiring + GLOSA Advisory Speed Summary

**AgentProfile ECS component wired through spawn/GPU paths with per-frame flag encoding, plus GLOSA advisory speed system consuming SPaT broadcast for approach speed reduction**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-08T13:22:21Z
- **Completed:** 2026-03-08T13:30:21Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- AgentProfile ECS component spawned on every agent (motorbike, car, bus, truck, emergency, pedestrian) matching SpawnRequest.profile
- GPU flags bits 4-7 encode profile ID via compute_agent_flags + encode_profile_in_flags every frame in step_vehicles_gpu
- step_glosa() advisory speed system: queries signalized intersections, applies GLOSA speed to agents within 200m of non-green signals
- CPU/GPU parity maintained: step_glosa called in both tick_gpu() and tick() pipelines
- 11 new unit tests: 4 spawn profile, 3 flag encoding/roundtrip, 4 GLOSA behavior

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire AgentProfile into spawn and GPU flag paths** - `ffc1cf6` (feat)
2. **Task 2: Wire GLOSA advisory speed into tick pipeline** - `d14a4a5` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/sim_lifecycle.rs` - Added AgentProfile (req.profile) to all spawn branches
- `crates/velos-gpu/src/compute.rs` - Extended compute_agent_flags with profile parameter + 3 new tests
- `crates/velos-gpu/src/sim.rs` - Added AgentProfile to step_vehicles_gpu ECS query, step_glosa to both tick pipelines
- `crates/velos-gpu/src/sim_helpers.rs` - Implemented step_glosa() with SPaT/GLOSA consumption + 4 new tests
- `crates/velos-gpu/tests/integration_emergency_wiring.rs` - Updated compute_agent_flags call site (3rd parameter)

## Decisions Made
- Used `Option<&AgentProfile>` in step_vehicles_gpu ECS query with Commuter default -- ensures backward compatibility if any pre-existing entity lacks the component
- Placed step_glosa at step 4.5 in tick pipeline (after signal priority, before perception) so GLOSA-modified speeds are captured by the perception pipeline
- Created MockSignalController for GLOSA unit tests instead of building full 4-way intersection topology

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated integration_emergency_wiring.rs call site**
- **Found during:** Task 1 (compute_agent_flags signature change)
- **Issue:** Integration test called compute_agent_flags with 2 args, now requires 3
- **Fix:** Added AgentProfile::Commuter as third argument
- **Files modified:** crates/velos-gpu/tests/integration_emergency_wiring.rs
- **Verification:** cargo test passes
- **Committed in:** ffc1cf6 (Task 1 commit)

**2. [Rule 1 - Bug] Added missing IdmParams::delta field in test helpers**
- **Found during:** Task 2 (GLOSA test compilation)
- **Issue:** IdmParams struct gained `delta` field but test helper missed it
- **Fix:** Added `delta: 4.0` to test IdmParams construction
- **Files modified:** crates/velos-gpu/src/sim_helpers.rs
- **Verification:** Tests compile and pass
- **Committed in:** d14a4a5 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both auto-fixes were necessary for compilation. No scope creep.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Profile encoding active: decode_profile_from_flags in sim_reroute.rs now returns correct profile for non-Commuter agents
- GLOSA advisory active: agents approaching red/amber signals within 200m receive speed reduction
- Ready for 13-02 (GPU transfer audit) and 13-03 (integration verification)

---
*Phase: 13-final-integration-wiring-gpu-transfer-audit*
*Completed: 2026-03-08*
