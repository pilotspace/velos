---
phase: 09-sim-loop-integration-startup-frame-pipeline
plan: 01
subsystem: gpu-engine
tags: [wgpu, signal-controller, startup, perception, vehicle-config, toml]

requires:
  - phase: 06-agent-models-signal-control
    provides: "SignalController trait, FixedTimeController, ActuatedController, AdaptiveController, LoopDetector, GpuSign"
  - phase: 07-intelligence-routing-prediction
    provides: "PerceptionPipeline, CCHRouter, PredictionService"
  - phase: 08-tuning-vehicle-behavior
    provides: "VehicleConfig TOML loading, GpuVehicleParams, HCMC-calibrated defaults"
provides:
  - "Polymorphic signal controllers (Box<dyn SignalController>) from TOML config"
  - "SignalConfig infrastructure (TOML loading with graceful fallback)"
  - "GPU param upload at startup (vehicle params binding 7, signs binding 6)"
  - "PerceptionPipeline instantiation at startup (300K max agents)"
  - "sim_startup.rs module with all startup initialization logic"
  - "cpu_reference.rs extracted module for CPU test validation"
  - "SimWorld::new(device, queue, dispatcher) with full GPU init"
  - "SimWorld::new_cpu_only() for CPU-only test paths"
  - "ComputeDispatcher::sign_buffer() accessor and upload_signs() method"
affects: [09-02-frame-pipeline, 09-03-detector-wiring]

tech-stack:
  added: [serde, toml (in velos-signal)]
  patterns: [polymorphic-signal-dispatch, startup-extraction, cpu-only-test-constructor]

key-files:
  created:
    - crates/velos-gpu/src/sim_startup.rs
    - crates/velos-gpu/src/cpu_reference.rs
    - crates/velos-signal/src/config.rs
    - data/hcmc/signal_config.toml
  modified:
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/app.rs
    - crates/velos-gpu/src/compute.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-signal/src/lib.rs
    - crates/velos-signal/Cargo.toml

key-decisions:
  - "Polymorphic signal controllers via Box<dyn SignalController> instead of enum dispatch"
  - "cpu_reference module extracted to own file (cpu_reference.rs) for sim.rs line compliance"
  - "SimWorld::new_cpu_only() constructor for test paths without GPU device"
  - "Speed limit signs auto-generated from edge speed_limit_mps at startup"
  - "PerceptionPipeline 300K max agents (covers 280K target with headroom)"

patterns-established:
  - "Startup extraction pattern: sim_startup.rs holds init functions, sim.rs calls them"
  - "CPU-only constructor pattern: new_cpu_only() skips GPU init for test paths"
  - "Signal config override pattern: TOML file with per-node controller type overrides"

requirements-completed: [SIG-01, SIG-02, SIG-05, TUN-02]

duration: 8min
completed: 2026-03-08
---

# Phase 09 Plan 01: Startup Initialization Summary

**Polymorphic signal controllers from TOML config, GPU param upload at startup, PerceptionPipeline init, sim.rs extracted to 581 lines**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-08T05:07:07Z
- **Completed:** 2026-03-08T05:15:45Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- Signal controllers now dispatch polymorphically via `Box<dyn SignalController>` -- actuated/adaptive from TOML config, fixed-time default
- GPU uniform buffer (binding 7) populated with real VehicleConfig values at startup, not zeros
- PerceptionPipeline created at startup with 300K max agent capacity
- Sign buffer populated from network speed limits at startup
- sim.rs reduced from 888 to 581 lines by extracting cpu_reference and startup logic

## Task Commits

Each task was committed atomically:

1. **Task 1: SignalConfig TOML infrastructure + sign_buffer accessor** - `3f383ab` (feat)
2. **Task 2: SimWorld startup refactor** - `5aaf216` (feat)

## Files Created/Modified
- `crates/velos-signal/src/config.rs` - SignalConfig, IntersectionConfig, load_signal_config() with TOML loading and graceful fallback
- `crates/velos-gpu/src/sim_startup.rs` - Startup init: load_vehicle_config, build_signal_controllers, build_loop_detectors, upload_network_signs
- `crates/velos-gpu/src/cpu_reference.rs` - CPU reference vehicle physics extracted from sim.rs
- `data/hcmc/signal_config.toml` - Default signal controller configuration with documented format
- `crates/velos-gpu/src/sim.rs` - Refactored: polymorphic signals, new() with device/queue, new_cpu_only()
- `crates/velos-gpu/src/app.rs` - Updated SimWorld::new() call with device/queue/dispatcher
- `crates/velos-gpu/src/compute.rs` - Added sign_buffer() accessor and upload_signs() method
- `crates/velos-gpu/src/lib.rs` - Added sim_startup and cpu_reference modules
- `crates/velos-signal/src/lib.rs` - Added config module and re-exports
- `crates/velos-signal/Cargo.toml` - Added serde and toml dependencies

## Decisions Made
- Polymorphic signal controllers via `Box<dyn SignalController>` -- trait dispatch is cleanest for 3 controller types
- cpu_reference module extracted to own file (335 lines) rather than inline module -- largest contributor to sim.rs bloat
- SimWorld::new_cpu_only() for test paths -- avoids GPU dependency in integration tests while keeping new() signature clean
- Speed limit signs auto-generated from edge.speed_limit_mps at offset 0.0 -- no manual sign placement needed for basic coverage
- PerceptionPipeline uses 300K max (7% headroom over 280K target) -- single allocation, no resize needed

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Rust 2024 edition: unsafe env var mutation in tests**
- **Found during:** Task 1 (signal config tests)
- **Issue:** `std::env::set_var()` and `std::env::remove_var()` are unsafe in Rust 2024 edition
- **Fix:** Wrapped in `unsafe {}` blocks with safety comment
- **Files modified:** crates/velos-signal/src/config.rs
- **Verification:** Tests compile and pass
- **Committed in:** 3f383ab (Task 1 commit)

**2. [Rule 3 - Blocking] Test file using old SimWorld::new(graph) signature**
- **Found during:** Task 2 (SimWorld refactor)
- **Issue:** cf_model_switch.rs test used `SimWorld::new(graph)` which no longer exists
- **Fix:** Changed to `SimWorld::new_cpu_only(graph)`
- **Files modified:** crates/velos-gpu/tests/cf_model_switch.rs
- **Verification:** Test compiles and passes
- **Committed in:** 5aaf216 (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both auto-fixes necessary for compilation. No scope creep.

## Issues Encountered
- Pre-existing clippy warning in velos-predict (manual_range_contains) -- not from this plan, logged as out-of-scope

## Next Phase Readiness
- SimWorld starts with all subsystems initialized, ready for frame pipeline wiring (Plan 09-02)
- PerceptionPipeline available for dispatch in tick_gpu frame loop
- Loop detectors ready for actuated signal feedback wiring (Plan 09-03)

---
*Phase: 09-sim-loop-integration-startup-frame-pipeline*
*Completed: 2026-03-08*
