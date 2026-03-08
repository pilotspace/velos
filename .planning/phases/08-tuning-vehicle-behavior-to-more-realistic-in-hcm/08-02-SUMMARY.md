---
phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm
plan: 02
subsystem: gpu-compute
tags: [wgpu, wgsl, uniform-buffer, idm, krauss, gpu-parameters, bytemuck]

requires:
  - phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm
    provides: VehicleConfig TOML infrastructure with per-vehicle-type params
  - phase: 06-agent-models-signal-control
    provides: GpuAgentState with vehicle_type field, wave_front.wgsl shader
provides:
  - GpuVehicleParams struct (224 bytes) for CPU-to-GPU parameter transfer
  - Uniform buffer binding 7 in wave-front pipeline for per-vehicle-type params
  - Parameterized IDM and Krauss functions in WGSL reading from vehicle_params buffer
  - upload_vehicle_params() method for runtime config updates
affects: [08-03-behavioral-rules]

tech-stack:
  added: []
  patterns: [gpu-uniform-buffer-parameterization, config-to-gpu-pipeline]

key-files:
  created:
    - crates/velos-gpu/tests/gpu_params_tests.rs
  modified:
    - crates/velos-gpu/src/compute.rs
    - crates/velos-gpu/shaders/wave_front.wgsl

key-decisions:
  - "KRAUSS_TAU kept as WGSL const (1.0s) -- reaction time is physics, not vehicle-type-specific"
  - "IDM_MAX_DECEL kept as WGSL const (-9.0) -- physical braking limit, not tunable"
  - "Pedestrian params mapped: v0=desired_speed, s0=personal_space, rest zeroed (social force is primary)"
  - "8 f32 per vehicle type: v0, s0, t_headway, a, b, krauss_accel, krauss_decel, krauss_sigma"

patterns-established:
  - "GPU uniform buffer pattern: Rust repr(C) Pod struct -> bytemuck -> queue.write_buffer -> WGSL uniform"
  - "GpuVehicleParams::from_config bridges VehicleConfig (f64) to GPU buffer (f32)"
  - "Vehicle-type indexing: vehicle_params[agent.vehicle_type] in WGSL for per-type behavior"

requirements-completed: [TUN-02]

duration: 6min
completed: 2026-03-08
---

# Phase 8 Plan 2: GPU Parameter Unification Summary

**Per-vehicle-type IDM/Krauss parameters via uniform buffer binding 7, eliminating GPU/CPU parameter mismatch by reading from shared VehicleConfig**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-08T03:59:50Z
- **Completed:** 2026-03-08T04:06:00Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- GpuVehicleParams struct (7 types x 8 f32 = 224 bytes) with from_config() conversion from VehicleConfig
- Uniform buffer at binding 7 wired into wave-front bind group layout and dispatch
- Removed 11 hardcoded IDM/Krauss parameter constants from wave_front.wgsl
- idm_acceleration(), krauss_safe_speed(), krauss_update() now parameterized by vehicle_type
- 5 unit tests validating struct size, per-type index mapping, and config conversion

## Task Commits

Each task was committed atomically:

1. **Task 1: Create GpuVehicleParams struct + upload to binding 7** - `e020f39` (feat, TDD)
2. **Task 2: Replace WGSL hardcoded constants with uniform buffer reads** - `d30a932` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/compute.rs` - GpuVehicleParams struct, from_config(), upload_vehicle_params(), binding 7 in BGL and bind group
- `crates/velos-gpu/shaders/wave_front.wgsl` - VehicleTypeParams struct, binding 7, parameterized IDM/Krauss functions
- `crates/velos-gpu/tests/gpu_params_tests.rs` - 5 tests for struct size, index mapping, config conversion

## Decisions Made
- KRAUSS_TAU kept as WGSL constant (1.0s) -- reaction time is a physics constant, not vehicle-type-specific
- IDM_MAX_DECEL kept as WGSL constant (-9.0 m/s^2) -- physical braking limit, not a tunable parameter
- Pedestrian row maps desired_speed to v0 and personal_space to s0; remaining fields zeroed since pedestrians use social force model
- 8 parameters per type chosen to cover both IDM and Krauss needs while staying compact (224 bytes total fits in a single uniform buffer)
- Buffer initialized to zeroes on creation; callers must call upload_vehicle_params() before first dispatch

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- GPU now reads per-vehicle-type parameters from VehicleConfig source of truth
- Changing vehicle_params.toml will change both CPU and GPU behavior
- Ready for Plan 03 (behavioral rules tuning)

---
*Phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm*
*Completed: 2026-03-08*
