---
phase: 09-sim-loop-integration-startup-frame-pipeline
plan: 02
subsystem: gpu
tags: [wgsl, wgpu, perception, hcmc-behavior, red-light-creep, gap-acceptance]

requires:
  - phase: 07-intelligence-routing-prediction
    provides: "PerceptionPipeline with result_buffer() for GPU-side perception data"
  - phase: 08-tuning-vehicle-behavior
    provides: "CPU reference implementations of red_light_creep_speed and intersection_gap_acceptance"
provides:
  - "PerceptionResult struct and @binding(8) in wave_front.wgsl for GPU perception reads"
  - "red_light_creep_speed() WGSL function matching CPU sublane.rs reference"
  - "intersection_gap_acceptance() WGSL function matching CPU intersection.rs reference"
  - "ComputeDispatcher bind group with 9 entries (bindings 0-8)"
  - "set_perception_result_buffer() API for PerceptionPipeline wiring"
affects: [09-sim-loop-integration-startup-frame-pipeline, velos-gpu]

tech-stack:
  added: [naga (dev-dependency for WGSL validation)]
  patterns: [perception-driven GPU behaviors, CPU-GPU parity testing via include_str]

key-files:
  created: []
  modified:
    - crates/velos-gpu/shaders/wave_front.wgsl
    - crates/velos-gpu/src/compute.rs
    - crates/velos-gpu/Cargo.toml

key-decisions:
  - "PerceptionResult WGSL struct field 'flags' renamed to 'perc_flags' to avoid WGSL keyword conflict with AgentState.flags"
  - "Placeholder perception_result_buffer pre-allocated for 300K agents (9.6 MB zeroed) — avoids Option complexity in bind group"
  - "Gap acceptance uses VT_CAR as default other_type since perception data lacks leader vehicle type — full type-aware logic runs on CPU"
  - "naga dev-dependency added for compile-time WGSL parse validation in unit tests"

patterns-established:
  - "CPU reference parity tests: mirror WGSL functions as Rust fn, test identical behavior"
  - "Shader structure tests via include_str! for compile-time content verification"

requirements-completed: [TUN-04, TUN-06]

duration: 5min
completed: 2026-03-08
---

# Phase 09 Plan 02: GPU Perception-Driven HCMC Behaviors Summary

**Perception results buffer at binding(8) with red-light creep and intersection gap acceptance WGSL functions enabling HCMC driving behaviors entirely on GPU**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-08T05:07:07Z
- **Completed:** 2026-03-08T05:12:29Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Added PerceptionResult struct and @binding(8) storage buffer to wave_front.wgsl for GPU-side perception reads
- Implemented red_light_creep_speed() WGSL function: motorbike/bicycle-only, 0.3 m/s max with 5m ramp distance
- Implemented intersection_gap_acceptance() WGSL function with size intimidation factors and forced acceptance after 5s wait
- Integrated both behaviors into wave_front_update main loop reading perception_results per agent
- Updated ComputeDispatcher with 9-entry bind group, placeholder buffer, and set_perception_result_buffer() API
- Added 17 unit tests covering shader structure, naga validation, and CPU-GPU behavior parity

## Task Commits

Each task was committed atomically:

1. **Task 1: Add perception_results binding to wave_front.wgsl + ComputeDispatcher bind group** - `593f18d` (feat)
2. **Task 2: Implement HCMC behavior functions in WGSL** - `c44cf52` (feat)

## Files Created/Modified
- `crates/velos-gpu/shaders/wave_front.wgsl` - PerceptionResult struct, binding(8), HCMC behavior functions, main loop integration
- `crates/velos-gpu/src/compute.rs` - perception_result_buffer field, bind group layout entry 8, set/get methods, 17 new tests
- `crates/velos-gpu/Cargo.toml` - naga dev-dependency for WGSL parse validation

## Decisions Made
- PerceptionResult WGSL field named `perc_flags` (not `flags`) to avoid collision with AgentState.flags in same shader scope
- Pre-allocated 300K-agent zeroed perception buffer avoids Option complexity in bind group creation
- Gap acceptance defaults to VT_CAR for unknown leader types (neutral size_factor=1.0) — CPU handles full type-aware logic
- naga added as dev-dependency for automated WGSL parse validation in CI

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] PerceptionResult field name mismatch**
- **Found during:** Task 1
- **Issue:** Plan specified `leader_distance` and `sign_value` but actual Rust PerceptionResult uses `leader_gap` and `sign_speed_limit`
- **Fix:** Used actual Rust struct field names in WGSL to match bytemuck layout
- **Files modified:** crates/velos-gpu/shaders/wave_front.wgsl
- **Verification:** Field offsets match 32-byte Rust struct layout

**2. [Rule 1 - Bug] WGSL flags field name collision**
- **Found during:** Task 2
- **Issue:** PerceptionResult.flags would shadow AgentState.flags in shader scope
- **Fix:** Renamed to `perc_flags` in WGSL struct
- **Files modified:** crates/velos-gpu/shaders/wave_front.wgsl
- **Verification:** naga parse validation passes

---

**Total deviations:** 2 auto-fixed (2 bugs)
**Impact on plan:** Both fixes necessary for correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Perception buffer binding ready for wiring with PerceptionPipeline in Plan 09-03
- HCMC behaviors activate automatically when perception data is populated
- All existing GPU tests pass (70 total across velos-gpu)

---
*Phase: 09-sim-loop-integration-startup-frame-pipeline*
*Completed: 2026-03-08*
