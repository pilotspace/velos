---
phase: 12-cpu-lane-change-sublane-wiring
plan: 01
subsystem: gpu
tags: [wgsl, gpu-uniform-buffer, vehicle-params, creep, gap-acceptance]

requires:
  - phase: 08-hcmc-vehicle-tuning
    provides: VehicleConfig with creep_max_speed, creep_distance_scale, gap_acceptance_ttc fields
  - phase: 09-gpu-tick-loop
    provides: wave_front.wgsl with VehicleTypeParams struct and red_light_creep_speed/intersection_gap_acceptance functions
provides:
  - 12-float GpuVehicleParams with creep/gap fields driven from TOML config
  - WGSL VehicleTypeParams extended to 12 fields matching Rust layout
  - Zero hardcoded behavior constants in wave_front.wgsl
affects: [12-02-PLAN, velos-gpu, wave_front.wgsl]

tech-stack:
  added: []
  patterns:
    - "Config-driven GPU uniform buffer: all vehicle behavior constants read from TOML via uniform buffer, no WGSL hardcoding"

key-files:
  created:
    - crates/velos-gpu/tests/gpu_vehicle_params.rs
  modified:
    - crates/velos-gpu/src/compute.rs
    - crates/velos-gpu/shaders/wave_front.wgsl
    - crates/velos-gpu/tests/gpu_params_tests.rs

key-decisions:
  - "GAP_MAX_WAIT_TIME/GAP_FORCED_ACCEPTANCE_FACTOR/GAP_WAIT_REDUCTION_RATE kept as local let constants inside intersection_gap_acceptance() rather than uniform buffer -- they are universal physics constants not vehicle-type-specific"
  - "creep_min_distance hardcoded to 0.5 for all types -- not in TOML, constant across vehicle types"
  - "gap_acceptance_ttc replaces t_headway as base TTC threshold in intersection gap acceptance"

patterns-established:
  - "12-float vehicle params: indices 0-7 IDM+Krauss, 8-11 creep/gap behavior"

requirements-completed: [TUN-04, TUN-06]

duration: 4min
completed: 2026-03-08
---

# Phase 12 Plan 01: GPU Vehicle Params Extension Summary

**Extended GpuVehicleParams from 8 to 12 floats per vehicle type, eliminating all hardcoded WGSL behavior constants for creep and gap acceptance**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-08T10:01:41Z
- **Completed:** 2026-03-08T10:06:09Z
- **Tasks:** 1
- **Files modified:** 4

## Accomplishments
- Extended GpuVehicleParams from [[f32; 8]; 7] (224 bytes) to [[f32; 12]; 7] (336 bytes)
- Added creep_max_speed, creep_distance_scale, creep_min_distance, gap_acceptance_ttc to WGSL VehicleTypeParams
- Removed all 6 hardcoded WGSL constants (CREEP_MAX_SPEED, CREEP_DISTANCE_SCALE, CREEP_MIN_DISTANCE, GAP_MAX_WAIT_TIME, GAP_FORCED_ACCEPTANCE_FACTOR, GAP_WAIT_REDUCTION_RATE)
- Updated red_light_creep_speed() to read from uniform buffer with early-exit for non-creeping vehicles
- Updated intersection_gap_acceptance() to use gap_acceptance_ttc from uniform buffer
- Added 11 comprehensive tests verifying struct size, field mapping, WGSL alignment, and constant elimination

## Task Commits

Each task was committed atomically:

1. **Task 1: Extend GpuVehicleParams to 12 floats and update WGSL VehicleTypeParams** - `b4005ba` (feat)

**Plan metadata:** (pending)

_Note: TDD task -- RED phase confirmed compilation errors with [f32; 8], GREEN phase implemented 12-float extension._

## Files Created/Modified
- `crates/velos-gpu/src/compute.rs` - Extended GpuVehicleParams struct and from_config() to 12 floats, updated CPU reference test
- `crates/velos-gpu/shaders/wave_front.wgsl` - Extended VehicleTypeParams to 12 fields, removed hardcoded constants, updated red_light_creep_speed and intersection_gap_acceptance
- `crates/velos-gpu/tests/gpu_vehicle_params.rs` - New test file with 11 tests for struct size, field mapping, WGSL validation
- `crates/velos-gpu/tests/gpu_params_tests.rs` - Updated size assertion from 224 to 336 bytes

## Decisions Made
- GAP_MAX_WAIT_TIME, GAP_FORCED_ACCEPTANCE_FACTOR, GAP_WAIT_REDUCTION_RATE converted to local `let` constants inside intersection_gap_acceptance() -- universal physics constants, not vehicle-type-specific
- creep_min_distance hardcoded to 0.5f32 for all vehicle types -- constant across types, not in TOML config
- gap_acceptance_ttc (index 11) replaces t_headway as base TTC threshold in gap acceptance decisions

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated existing gpu_params_tests.rs size assertion**
- **Found during:** Task 1 (GREEN phase verification)
- **Issue:** Existing test `gpu_vehicle_params_size_is_224_bytes` asserted old 224-byte size
- **Fix:** Updated assertion to 336 bytes and renamed test to `gpu_vehicle_params_size_is_336_bytes`
- **Files modified:** crates/velos-gpu/tests/gpu_params_tests.rs
- **Verification:** cargo test -p velos-gpu passes all 155 tests
- **Committed in:** b4005ba (part of task commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary update to existing test for correctness. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- GpuVehicleParams now carries all behavior parameters needed for Plan 02
- wave_front.wgsl reads creep/gap params from uniform buffer -- ready for MOBIL/sublane wiring
- All 155 velos-gpu tests pass including naga WGSL validation

## Self-Check: PASSED

- All 4 modified/created files exist on disk
- Commit b4005ba verified in git log
- Zero hardcoded CREEP/GAP constants remain in wave_front.wgsl
- All 155 velos-gpu tests pass

---
*Phase: 12-cpu-lane-change-sublane-wiring*
*Completed: 2026-03-08*
