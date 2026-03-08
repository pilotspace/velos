---
phase: 06-agent-models-signal-control
plan: 06
subsystem: gpu
tags: [wgsl, prefix-sum, social-force, pedestrian, adaptive-workgroup, spatial-hash]

requires:
  - phase: 06-01
    provides: "VehicleType::Pedestrian enum, GpuAgentState 40-byte struct with vehicle_type field"
provides:
  - "PedestrianAdaptivePipeline: 6-dispatch GPU pipeline for density-adaptive pedestrian social force"
  - "pedestrian_adaptive.wgsl: 4-pass WGSL shader (count, prefix-sum, scatter, social force)"
  - "GpuPedestrian struct for GPU-side pedestrian state"
  - "Density classification function (2m/5m/10m cells based on density)"
affects: [velos-gpu, pedestrian-simulation]

tech-stack:
  added: []
  patterns: [multi-workgroup-prefix-sum, spatial-hash-gpu, adaptive-dispatch]

key-files:
  created:
    - "crates/velos-gpu/shaders/pedestrian_adaptive.wgsl"
    - "crates/velos-gpu/src/ped_adaptive.rs"
    - "crates/velos-gpu/tests/pedestrian_adaptive_tests.rs"
  modified:
    - "crates/velos-gpu/src/compute.rs"
    - "crates/velos-gpu/src/lib.rs"

key-decisions:
  - "Separate PedestrianAdaptivePipeline module instead of embedding in ComputeDispatcher (file size limit)"
  - "Hillis-Steele scan with reduce-then-scan for portable multi-workgroup prefix sum"
  - "Workgroup size 256 for compute passes, 64 for social force (matches architecture doc)"
  - "All cells dispatched in social force pass (empty cells early-exit via count check)"

patterns-established:
  - "GPU pipeline extraction: separate module per pipeline family when compute.rs exceeds 700 lines"
  - "Multi-workgroup prefix sum: 3 sub-dispatches (local scan, scan workgroup sums, propagate)"

requirements-completed: [AGT-04]

duration: 13min
completed: 2026-03-07
---

# Phase 6 Plan 6: Pedestrian Adaptive GPU Workgroups Summary

**6-dispatch WGSL pipeline with Hillis-Steele prefix-sum compaction for density-adaptive pedestrian social force dispatch**

## Performance

- **Duration:** 13 min
- **Started:** 2026-03-07T14:42:01Z
- **Completed:** 2026-03-07T14:55:18Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- 4-pass WGSL shader implementing count, multi-workgroup prefix-sum, scatter, and social force
- PedestrianAdaptivePipeline with upload, dispatch, readback, and density classification
- GPU social force within 5% of CPU reference for face-to-face pedestrian scenario
- 1000-pedestrian sparse dispatch completes with all outputs finite

## Task Commits

Each task was committed atomically:

1. **Task 1: Prefix-sum WGSL shader and pedestrian adaptive dispatch** - `fa2f728` (feat)
2. **Task 2: Pedestrian adaptive dispatch integration tests** - `77d3fd6` (test, co-committed with 06-05 SUMMARY)

## Files Created/Modified
- `crates/velos-gpu/shaders/pedestrian_adaptive.wgsl` - 4-pass WGSL: count_per_cell, prefix_sum (3 sub-dispatches), scatter, social_force_adaptive
- `crates/velos-gpu/src/ped_adaptive.rs` - PedestrianAdaptivePipeline with GpuPedestrian, PedestrianAdaptiveParams, upload/dispatch/readback
- `crates/velos-gpu/tests/pedestrian_adaptive_tests.rs` - 4 GPU integration tests: prefix-sum, GPU-vs-CPU, sparse scenario, density classification
- `crates/velos-gpu/src/compute.rs` - Module doc update, bgl_entry visibility change to pub(crate)
- `crates/velos-gpu/src/lib.rs` - Register ped_adaptive module, export types

## Decisions Made
- Extracted pedestrian pipeline to separate `ped_adaptive.rs` module rather than embedding in `compute.rs` (compute.rs was already 644 lines; adding ~400 lines would exceed 700-line limit)
- Used Hillis-Steele scan (O(n log n) work) over Blelloch scan for simplicity and portability across Metal/Vulkan
- Dispatching one workgroup per ALL cells in social force pass (empty cells early-exit). True indirect dispatch deferred until profiling shows benefit.
- `bgl_entry` made `pub(crate)` to share between compute.rs and ped_adaptive.rs

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed WaveFrontParams struct field additions from concurrent plan**
- **Found during:** Task 1 (compilation)
- **Issue:** A concurrent linter/plan added `sign_count`, `sim_time`, `_pad0`, `_pad1` fields to WaveFrontParams
- **Fix:** Updated constructor and dispatch to include new fields with default values
- **Files modified:** crates/velos-gpu/src/compute.rs
- **Verification:** cargo check passes
- **Committed in:** fa2f728 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary to maintain compilation with concurrent struct changes. No scope creep.

## Issues Encountered
- Pre-existing `velos-net` test compilation error (`RoadEdge` not in scope in cleaning.rs). Not caused by this plan. Logged as out-of-scope.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Pedestrian adaptive dispatch pipeline complete and tested on Metal backend
- Ready for integration into simulation loop (SimWorld) when pedestrian agent spawning is implemented
- CPU social force reference in velos-vehicle/social_force.rs remains the validation baseline

---
*Phase: 06-agent-models-signal-control*
*Completed: 2026-03-07*
