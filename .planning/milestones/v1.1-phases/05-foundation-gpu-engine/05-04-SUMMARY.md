---
phase: 05-foundation-gpu-engine
plan: 04
subsystem: gpu-compute
tags: [wave-front, wgsl, gpu-dispatch, car-following, idm, krauss, pcg-rng, fixed-point]

requires:
  - phase: 05-01
    provides: "Fixed-point types (FixPos/FixSpd/FixLat), Krauss CPU reference, CarFollowingModel enum, GpuAgentState struct"
provides:
  - "Wave-front WGSL compute shader with IDM+Krauss branching and PCG hash RNG"
  - "Fixed-point WGSL arithmetic library (Q16.16/Q12.20 multiply, cross-format speed*dt)"
  - "Extended ComputeDispatcher with wave-front pipeline, lane buffer management, readback"
  - "CPU-side lane sorting (sort_agents_by_lane) for wave-front dispatch"
  - "SimWorld::tick_gpu() as sole production vehicle physics path"
  - "Car-following model color-coding in egui dashboard (IDM=green/blue, Krauss=orange)"
affects: [05-05, 06-01, 06-02]

tech-stack:
  added: []
  patterns: [wave-front Gauss-Seidel per-lane dispatch, f32 intermediate physics with fixed-point storage, PCG hash RNG for GPU determinism]

key-files:
  created:
    - crates/velos-gpu/shaders/fixed_point.wgsl
    - crates/velos-gpu/shaders/wave_front.wgsl
    - crates/velos-gpu/tests/wave_front_validation.rs
    - crates/velos-gpu/tests/gpu_physics.rs
  modified:
    - crates/velos-gpu/src/compute.rs
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/sim_render.rs
    - crates/velos-gpu/src/app.rs

key-decisions:
  - "f32 intermediates for physics calculations, fixed-point only for position/speed storage -- avoids full fixed-point performance penalty while keeping deterministic storage"
  - "Trapezoidal integration (avg of old+new speed * dt) instead of simple Euler for smoother position updates"
  - "CPU-side lane sorting every frame (~1.5ms with rayon for 280K agents) -- acceptable overhead per RESEARCH.md"
  - "tick_gpu() as new production method, tick() preserved as CPU fallback for tests without GPU"
  - "CPU reference car-following moved to cpu_reference module (not deleted) for ongoing validation"

patterns-established:
  - "Wave-front dispatch pattern: one workgroup per lane, thread 0 only, agents processed front-to-back"
  - "GPU buffer lifecycle: upload GpuAgentState -> sort lanes -> dispatch -> readback -> write to ECS"
  - "Car-following model color-coding: IDM = cool tones (green/blue), Krauss = warm tones (orange)"

requirements-completed: [GPU-01, GPU-03]

duration: 12min
completed: 2026-03-07
---

# Phase 5 Plan 04: GPU Wave-Front Dispatch Summary

**Wave-front WGSL compute shader with IDM+Krauss car-following, PCG RNG dawdle, and GPU-only vehicle physics in SimWorld tick loop**

## Performance

- **Duration:** 12 min
- **Started:** 2026-03-07T12:31:04Z
- **Completed:** 2026-03-07T12:43:01Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- Two WGSL shaders: fixed_point.wgsl (Q16.16/Q12.20 arithmetic) and wave_front.wgsl (per-lane sequential car-following with IDM+Krauss branching)
- ComputeDispatcher extended with wave-front pipeline, lane buffer management, and GpuAgentState readback
- SimWorld::tick_gpu() replaces CPU physics as sole production path for vehicle updates
- CPU step_vehicles/step_motorbikes_sublane moved to cpu_reference module (test-only)
- Agents color-coded by car-following model in egui (IDM=green/blue, Krauss=orange)
- 11 new tests across 2 test files, all existing tests continue passing (37 total)

## Task Commits

Each task was committed atomically:

1. **Task 1: Wave-front WGSL shaders** - `9d30347` (feat)
2. **Task 2: GPU physics cutover** - `4e71bff` (feat)

## Files Created/Modified
- `crates/velos-gpu/shaders/fixed_point.wgsl` - Q16.16 multiply, f32 conversions, cross-format speed*dt
- `crates/velos-gpu/shaders/wave_front.wgsl` - Wave-front kernel with IDM/Krauss branching, PCG RNG, trapezoidal integration
- `crates/velos-gpu/src/compute.rs` - Extended with wave-front pipeline, lane buffers, sort_agents_by_lane()
- `crates/velos-gpu/src/sim.rs` - tick_gpu() for GPU dispatch, cpu_reference module for validation
- `crates/velos-gpu/src/sim_render.rs` - Color-coding by CarFollowingModel
- `crates/velos-gpu/src/app.rs` - Wired tick_gpu() with ComputeDispatcher
- `crates/velos-gpu/tests/wave_front_validation.rs` - CPU reference, lane sorting, PCG RNG tests
- `crates/velos-gpu/tests/gpu_physics.rs` - Struct sizes, discriminants, fixed-point roundtrip tests

## Decisions Made
- Used f32 intermediates for IDM/Krauss calculations on GPU, converting only final speed/position to fixed-point -- avoids 40-80% fixed-point performance penalty while keeping storage deterministic
- Trapezoidal integration (average of old and new speed) for position update instead of simple Euler -- smoother trajectory, better energy conservation
- tick_gpu() takes device/queue/dispatcher as parameters rather than storing GPU resources on SimWorld -- cleaner separation of concerns, SimWorld stays GPU-agnostic
- CPU reference functions kept in cpu_reference module (pub(crate)) rather than deleted -- enables ongoing GPU vs CPU validation testing

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Wave-front dispatch ready for 280K agents (one workgroup per lane, parallel across lanes)
- GPU physics pipeline complete: upload -> sort -> dispatch -> readback -> write ECS
- Ready for Phase 5 Plan 05 (multi-GPU partitioning, performance benchmarks)
- CPU reference available for ongoing GPU shader validation

---
*Phase: 05-foundation-gpu-engine*
*Completed: 2026-03-07*
