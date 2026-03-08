---
phase: 06-agent-models-signal-control
plan: 04
subsystem: gpu-engine
tags: [gpu, wgsl, emergency, yield-cone, vehicle-priority, car-following]

# Dependency graph
requires:
  - phase: 06-agent-models-signal-control
    provides: "40-byte GpuAgentState with vehicle_type/flags, VT_EMERGENCY/FLAG_EMERGENCY_ACTIVE constants"
provides:
  - "Emergency yield cone CPU reference (compute_yield_cone, should_yield, yield_speed_target)"
  - "EmergencyState struct for ECS component"
  - "GPU shader emergency branching with early-exit when emergency_count==0"
  - "EmergencyVehicle buffer binding (max 16) in wave_front.wgsl"
  - "GpuEmergencyVehicle Rust struct and upload_emergency_vehicles() method"
affects: [06-agent-models-signal-control]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Emergency yield cone: 50m range, 90-degree cone (45-degree half-angle)"
    - "GPU emergency early-exit: emergency_count in params uniform, zero cost when 0"
    - "CPU reference pattern: Rust functions mirror WGSL logic for validation"
    - "EmergencyVehicle buffer at binding 5, max 16 entries"

key-files:
  created:
    - "crates/velos-vehicle/src/emergency.rs"
    - "crates/velos-vehicle/tests/emergency_tests.rs"
  modified:
    - "crates/velos-vehicle/src/lib.rs"
    - "crates/velos-gpu/shaders/wave_front.wgsl"
    - "crates/velos-gpu/src/compute.rs"

key-decisions:
  - "Yield cone uses 45-degree half-angle (90 total) at 50m range -- wide enough for realistic siren detection"
  - "Emergency vehicles decelerate to 5 m/s at intersections (lane leader only) for safety"
  - "Yielding agents capped at 1.4 m/s (5 km/h) -- matches pedestrian walking speed"
  - "Max 16 emergency vehicles in GPU buffer -- sufficient for city-scale simulation"
  - "emergency_count replaces _pad field in WaveFrontParams -- backward compatible (0 = no-op)"

patterns-established:
  - "GPU emergency buffer: binding 5 with GpuEmergencyVehicle struct (pos_x, pos_y, heading, _pad)"
  - "CPU reference functions in velos-vehicle for GPU shader validation"
  - "FLAG_YIELDING set by shader to override agent speed"

requirements-completed: [AGT-08]

# Metrics
duration: 5min
completed: 2026-03-07
---

# Phase 6 Plan 04: Emergency Vehicle Priority Summary

**Emergency vehicle yield cone detection (50m/90-degree), GPU shader branching with zero-cost early-exit, and CPU reference implementation with 11 unit tests**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-07T14:31:52Z
- **Completed:** 2026-03-07T14:36:52Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- CPU reference implementation of emergency yield cone with compute_yield_cone(), should_yield(), and yield_speed_target() -- 11 edge-case tests
- GPU shader emergency branching: early-exit when emergency_count==0, yield cone check for surrounding agents, FLAG_YIELDING speed override
- Emergency vehicles decelerate to 5 m/s at intersections (lane leader safety), yielding agents slow to 1.4 m/s
- EmergencyVehicle buffer (binding 5, max 16) with GpuEmergencyVehicle Rust struct and upload method

## Task Commits

Each task was committed atomically:

1. **Task 1: Emergency vehicle CPU reference and yield cone logic** - `ddecac2` (feat) [TDD: RED+GREEN]
2. **Task 2: GPU shader emergency branching** - `2e17f38` (feat)

## Files Created/Modified
- `crates/velos-vehicle/src/emergency.rs` - EmergencyState, YieldCone, compute_yield_cone, should_yield, yield_speed_target
- `crates/velos-vehicle/tests/emergency_tests.rs` - 11 tests for yield cone geometry edge cases
- `crates/velos-vehicle/src/lib.rs` - Added `pub mod emergency`
- `crates/velos-gpu/shaders/wave_front.wgsl` - EmergencyVehicle struct, binding 5, check_emergency_yield(), intersection decel, yield speed override
- `crates/velos-gpu/src/compute.rs` - GpuEmergencyVehicle, emergency_buffer, upload_emergency_vehicles(), emergency_count in WaveFrontParams

## Decisions Made
- Yield cone uses 45-degree half-angle at 50m range for realistic siren detection geometry
- emergency_count replaces _pad in WaveFrontParams -- backward compatible, shader early-exits when 0
- Emergency vehicles only decelerate at intersections when they are lane leader (i==0)
- Max 16 emergency vehicles -- pre-allocated buffer avoids runtime allocation

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- Pre-existing velos-net compilation error (RoadEdge import) -- not caused by this plan, documented in 06-01 summary

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Emergency yield cone CPU + GPU implementations ready for Plan 06-05 (signal priority/preemption)
- FLAG_YIELDING mechanism can be reused for other priority vehicle types
- GpuEmergencyVehicle buffer needs CPU-side population from ECS world (simulation loop integration)
- All workspace tests pass (excluding pre-existing velos-net issue)

---
*Phase: 06-agent-models-signal-control*
*Completed: 2026-03-07*
