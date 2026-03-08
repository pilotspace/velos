---
phase: 11-gpu-buffer-wiring-perception-emergency
plan: 02
subsystem: gpu
tags: [wgpu, compute-shader, emergency-vehicle, bitfield-flags, ecs-gpu-wiring]

requires:
  - phase: 06-agent-models-signal-control
    provides: GpuEmergencyVehicle struct, upload_emergency_vehicles(), FLAG_EMERGENCY_ACTIVE const in WGSL
  - phase: 09-pipeline-wiring
    provides: PerceptionPipeline, wave_front.wgsl with binding(5) emergency buffer and check_emergency_yield()
provides:
  - upload_emergency_vehicles() called every frame in step_vehicles_gpu()
  - FLAG_EMERGENCY_ACTIVE (bit 1) set on GpuAgentState for emergency vehicles
  - compute_agent_flags() pure function for testable flag computation
  - Integration tests verifying emergency wiring end-to-end
affects: [gpu-pipeline, agent-behavior, emergency-response]

tech-stack:
  added: []
  patterns: [pure-function flag computation for testability, ECS Position query for world-space GPU upload]

key-files:
  created:
    - crates/velos-gpu/tests/integration_emergency_wiring.rs
  modified:
    - crates/velos-gpu/src/compute.rs
    - crates/velos-gpu/src/sim.rs

key-decisions:
  - "Extracted compute_agent_flags() as public pure function for unit testability outside GPU context"
  - "Emergency vehicles use ECS Position component (world coords) not RoadPosition (edge-relative) for yield cone buffer"

patterns-established:
  - "Pure function extraction for GPU flag bitfield computation -- enables unit tests without wgpu device"
  - "Emergency vehicle collection piggybacked on existing agent iteration loop (no second pass)"

requirements-completed: [AGT-08]

duration: 6min
completed: 2026-03-08
---

# Phase 11 Plan 02: Emergency Vehicle Upload + FLAG_EMERGENCY_ACTIVE Wiring Summary

**Emergency vehicle GPU yield cone activated: upload_emergency_vehicles() called every frame with world positions, FLAG_EMERGENCY_ACTIVE bit set on GpuAgentState, emergency_count reflects actual emergency presence**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-08T07:58:56Z
- **Completed:** 2026-03-08T08:04:31Z
- **Tasks:** 1
- **Files modified:** 3

## Accomplishments
- FLAG_EMERGENCY_ACTIVE (bit 1) now set on GpuAgentState for all VehicleType::Emergency agents
- upload_emergency_vehicles() called every frame in step_vehicles_gpu() with world positions from ECS Position + Kinematics.heading
- emergency_count correctly reflects 0 when no emergency vehicles exist (no crash, shader early-exits)
- Bus dwelling flag (bit 0) works correctly alongside emergency flag (both can be set simultaneously, flags=3)
- 10 new tests: 4 unit tests for flag bitfield logic, 6 integration tests for emergency collection and wiring

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire emergency vehicle upload and FLAG_EMERGENCY_ACTIVE in step_vehicles_gpu** - `f4aa235` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/compute.rs` - Added compute_agent_flags() pure function and unit tests for flag bitfield combinations
- `crates/velos-gpu/src/sim.rs` - Modified step_vehicles_gpu() to add Position to ECS query, set FLAG_EMERGENCY_ACTIVE, collect emergency world positions, call upload_emergency_vehicles()
- `crates/velos-gpu/tests/integration_emergency_wiring.rs` - 6 integration tests: flag set, flag+dwelling, no-flag for cars, world position collection, zero count, multiple vehicles

## Decisions Made
- Extracted `compute_agent_flags()` as a public pure function in compute.rs rather than inlining the logic in sim.rs. This enables comprehensive unit testing of the flag bitfield without requiring a wgpu device or full SimWorld setup.
- Emergency vehicle world positions sourced from ECS `Position` component (world-space x, y) rather than interpolating along edge geometry from `RoadPosition`. The Position component already contains world coordinates maintained by the simulation loop.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- GPU yield cone in wave_front.wgsl now activates when emergency vehicles are present
- emergency_count > 0 causes check_emergency_yield() to execute instead of early-exiting
- FLAG_EMERGENCY_ACTIVE on GpuAgentState allows perception.wgsl to detect emergency-nearby agents
- All 65 velos-gpu lib tests + 6 integration tests + full workspace suite pass

## Self-Check: PASSED

---
*Phase: 11-gpu-buffer-wiring-perception-emergency*
*Completed: 2026-03-08*
