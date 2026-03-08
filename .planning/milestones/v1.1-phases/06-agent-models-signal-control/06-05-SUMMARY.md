---
phase: 06-agent-models-signal-control
plan: 05
subsystem: signal
tags: [spat, glosa, priority, traffic-signs, v2i, wgsl, ecs, gpu]

requires:
  - phase: 06-01
    provides: "GpuAgentState with vehicle_type and flags fields"
  - phase: 06-03
    provides: "SignalController trait, ActuatedController, AdaptiveController"
  - phase: 06-04
    provides: "Emergency vehicle yield cone, FLAG_YIELDING, wave_front.wgsl bindings 0-5"
provides:
  - "SpatBroadcast struct with GLOSA advisory speed computation"
  - "PriorityQueue with Emergency > Bus ordering, max 1 per cycle"
  - "SignalController::spat_data() and ::request_priority() trait methods"
  - "TrafficSign ECS component with 5 sign types"
  - "GpuSign 16-byte repr(C) struct for GPU buffer"
  - "WGSL handle_sign_interaction() for speed limit, stop, school zone"
  - "Sign buffer at binding 6 in wave_front.wgsl"
affects: [06-06, 07-pathfinding, integration-checkpoint]

tech-stack:
  added: [bytemuck (velos-signal)]
  patterns: [cpu-reference-functions-for-gpu-validation, spat-broadcast, glosa-advisory, priority-queue-per-cycle]

key-files:
  created:
    - crates/velos-signal/src/spat.rs
    - crates/velos-signal/src/priority.rs
    - crates/velos-signal/src/signs.rs
    - crates/velos-signal/tests/spat_tests.rs
    - crates/velos-signal/tests/priority_tests.rs
    - crates/velos-signal/tests/signs_tests.rs
  modified:
    - crates/velos-signal/src/lib.rs
    - crates/velos-signal/src/actuated.rs
    - crates/velos-signal/src/adaptive.rs
    - crates/velos-signal/Cargo.toml
    - crates/velos-gpu/shaders/wave_front.wgsl
    - crates/velos-gpu/src/compute.rs

key-decisions:
  - "GLOSA minimum practical speed threshold at 3.0 m/s -- below this agent stops and waits"
  - "PriorityQueue clears stale requests on reset_cycle() to prevent request buildup"
  - "GpuSign uses unsafe Pod/Zeroable impl (not derive) for explicit repr(C) control"
  - "School zone time-window enforcement on CPU; GPU always applies reduced speed when sign present in buffer"
  - "Sign buffer at binding 6 with sign_count in Params -- zero cost when no signs (early exit)"

patterns-established:
  - "V2I broadcast pattern: controller.spat_data(num_approaches) returns SpatBroadcast"
  - "Priority request pattern: queue.submit() + queue.dequeue() with max-1-per-cycle guard"
  - "Traffic sign GPU interaction: CPU filters active signs into buffer, shader applies effects"

requirements-completed: [SIG-03, SIG-04, SIG-05]

duration: 10min
completed: 2026-03-07
---

# Phase 6 Plan 5: V2I Communication & Traffic Signs Summary

**SPaT broadcast with GLOSA advisory speed, bus/emergency signal priority queue, and 5-type traffic sign system with GPU shader interaction**

## Performance

- **Duration:** 10 min
- **Started:** 2026-03-07T14:41:47Z
- **Completed:** 2026-03-07T14:51:44Z
- **Tasks:** 2
- **Files modified:** 12

## Accomplishments
- SPaT broadcast delivers current phase state + time-to-next-change; GLOSA computes optimal approach speed for green wave arrival
- Signal priority queue with Emergency > Bus ordering, max 1 request per cycle, green extension (15s) and red shortening (10s)
- TrafficSign ECS component covering 5 sign types (SpeedLimit, Stop, Yield, NoTurn, SchoolZone) with GpuSign 16-byte GPU struct
- WGSL shader handle_sign_interaction() applies speed limits within 50m, stop signs at 2m, school zone speed reduction
- 40 new unit tests across 3 test files, all passing

## Task Commits

Each task was committed atomically:

1. **Task 1: SPaT broadcast + signal priority + SignalController trait extension** - `e554788` (feat)
2. **Task 2: Traffic sign ECS component + GPU buffer + shader interaction** - `98a0904` (feat)

_Note: Both tasks used TDD (tests written first, then implementation)_

## Files Created/Modified
- `crates/velos-signal/src/spat.rs` - SpatBroadcast struct and glosa_speed() function
- `crates/velos-signal/src/priority.rs` - PriorityLevel enum, PriorityRequest, PriorityQueue
- `crates/velos-signal/src/signs.rs` - SignType, TrafficSign ECS, GpuSign Pod struct, CPU reference functions
- `crates/velos-signal/src/lib.rs` - Extended SignalController trait with spat_data() and request_priority()
- `crates/velos-signal/src/actuated.rs` - Actuated controller SPaT and priority implementations
- `crates/velos-signal/src/adaptive.rs` - Adaptive controller SPaT and priority implementations
- `crates/velos-signal/Cargo.toml` - Added bytemuck dependency
- `crates/velos-signal/tests/spat_tests.rs` - 6 tests for SPaT/GLOSA behavior
- `crates/velos-signal/tests/priority_tests.rs` - 6 tests for priority queue behavior
- `crates/velos-signal/tests/signs_tests.rs` - 17 tests for traffic sign behavior
- `crates/velos-gpu/shaders/wave_front.wgsl` - GpuSign struct, sign buffer binding 6, handle_sign_interaction()
- `crates/velos-gpu/src/compute.rs` - WaveFrontParams extended with sign_count and sim_time

## Decisions Made
- GLOSA uses 3.0 m/s as minimum practical approach speed (below this, stopping is more efficient than crawling)
- PriorityQueue resets stale requests on cycle boundary to prevent unbounded growth
- School zone time-window enforcement split: CPU decides which signs are active, GPU always applies speed reduction for signs in the buffer
- Yield and NoTurn signs handled on CPU only (Yield needs conflicting traffic info; NoTurn is pathfinding-level, Phase 7)
- WaveFrontParams padded to 32 bytes (8 u32/f32 fields) for GPU alignment

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Created velos-meso stub lib.rs**
- **Found during:** Task 1 (before tests could run)
- **Issue:** velos-meso crate had Cargo.toml but no src/lib.rs, blocking workspace compilation
- **Fix:** Created minimal `crates/velos-meso/src/lib.rs` placeholder
- **Files modified:** crates/velos-meso/src/lib.rs
- **Verification:** cargo test --workspace no longer fails on manifest loading
- **Committed in:** e554788 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Minimal -- pre-existing workspace issue, not caused by this plan's changes.

## Issues Encountered
- Pre-existing velos-net test compilation error (unrelated to this plan, `RoadEdge` not in scope in test)
- Concurrent plan 06-07 commits interleaved with this plan's commits in git history

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- V2I communication layer complete -- agents can receive signal timing for optimal approach
- Signal priority ready for bus route integration (Plan 06-02 provides BusState)
- Traffic sign GPU buffer ready for pipeline wiring at Phase 6 integration checkpoint
- NoTurn sign enforcement deferred to Phase 7 pathfinding (infinite cost on restricted edges)

---
*Phase: 06-agent-models-signal-control*
*Completed: 2026-03-07*
