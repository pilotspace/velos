---
phase: 06-agent-models-signal-control
plan: 01
subsystem: gpu-engine
tags: [gpu, ecs, bytemuck, wgsl, idm, vehicle-types, fixed-point]

# Dependency graph
requires:
  - phase: 05-foundation-gpu-engine
    provides: "32-byte GpuAgentState, 3-variant VehicleType, wave-front shader pipeline"
provides:
  - "40-byte GpuAgentState with vehicle_type and flags fields"
  - "7-variant VehicleType enum (Motorbike, Car, Bus, Bicycle, Truck, Emergency, Pedestrian)"
  - "Calibrated IDM parameters for all 7 vehicle types"
  - "WGSL vehicle type constants (VT_MOTORBIKE..VT_PEDESTRIAN) and flag constants"
  - "SpawnVehicleType extended with Bus, Bicycle, Truck, Emergency"
  - "Vehicle-type-aware half_width_for_type and color rendering"
affects: [06-agent-models-signal-control, 07-intelligence-routing]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "VehicleType GPU mapping: enum variant order = GPU u32 value (0-6)"
    - "Flags bitfield pattern: bit0=bus_dwelling, bit1=emergency_active, bit2=yielding"
    - "Sublane model vehicles: Motorbike + Bicycle (continuous lateral positioning)"
    - "Lane-based vehicles: Car + Bus + Truck + Emergency"

key-files:
  created:
    - "crates/velos-vehicle/tests/types_tests.rs"
  modified:
    - "crates/velos-core/src/components.rs"
    - "crates/velos-core/tests/components_tests.rs"
    - "crates/velos-vehicle/src/types.rs"
    - "crates/velos-gpu/shaders/wave_front.wgsl"
    - "crates/velos-gpu/src/compute.rs"
    - "crates/velos-gpu/src/sim.rs"
    - "crates/velos-gpu/src/sim_lifecycle.rs"
    - "crates/velos-gpu/src/sim_render.rs"
    - "crates/velos-gpu/src/sim_snapshot.rs"
    - "crates/velos-gpu/tests/wave_front_validation.rs"
    - "crates/velos-gpu/tests/gpu_physics.rs"
    - "crates/velos-gpu/tests/cf_model_switch.rs"
    - "crates/velos-gpu/tests/boundary_protocol_tests.rs"
    - "crates/velos-gpu/benches/dispatch.rs"
    - "crates/velos-demand/src/spawner.rs"
    - "crates/velos-demand/tests/demand_tests.rs"

key-decisions:
  - "Bicycle uses sublane model (same as Motorbike) with 0.3m half-width and IDM v0=4.17 m/s"
  - "Bus/Truck/Emergency use lane-based model (same as Car) with 30/70 Krauss/IDM split"
  - "VehicleType enum order is GPU mapping order: Motorbike=0..Pedestrian=6"
  - "Flags bitfield reserves 3 bits for future bus/emergency/yielding behavior"

patterns-established:
  - "GPU vehicle_type field: u32 matching VehicleType enum discriminant order"
  - "New vehicle types categorized as sublane (Bicycle) or lane-based (Bus/Truck/Emergency)"
  - "half_width_for_type provides collision widths: Bus=1.3m, Truck=1.2m, Emergency=1.0m"

requirements-completed: [AGT-03, AGT-07]

# Metrics
duration: 8min
completed: 2026-03-07
---

# Phase 6 Plan 01: Buffer Layout & Vehicle Types Summary

**40-byte GpuAgentState with vehicle_type/flags fields, 7-variant VehicleType enum, and calibrated IDM parameters for Bus/Bicycle/Truck/Emergency**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-07T14:18:52Z
- **Completed:** 2026-03-07T14:26:39Z
- **Tasks:** 2
- **Files modified:** 17

## Accomplishments
- Expanded GpuAgentState from 32 to 40 bytes with vehicle_type (u32 at offset 32) and flags (u32 at offset 36) fields
- Extended VehicleType from 3 to 7 variants in both velos-core and velos-vehicle (Bus, Bicycle, Truck, Emergency)
- Added calibrated IDM parameters for all 4 new vehicle types with literature-based values
- Updated WGSL shader struct, all GPU buffer code, spawner, renderer, and all test files to match new layout
- All workspace tests pass (excluding pre-existing velos-net test failure), zero clippy warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Expand GpuAgentState to 40 bytes and extend VehicleType enum** - `9cd7684` (feat) [TDD: RED+GREEN]
2. **Task 2: Update all GPU buffer code and shaders for 40-byte GpuAgentState** - `84be5dd` (feat)

## Files Created/Modified
- `crates/velos-core/src/components.rs` - Extended GpuAgentState (40 bytes) and VehicleType (7 variants)
- `crates/velos-core/tests/components_tests.rs` - Tests for 40-byte layout, field offsets, bytemuck roundtrips
- `crates/velos-vehicle/src/types.rs` - Extended VehicleType + IDM params for Bus/Bicycle/Truck/Emergency
- `crates/velos-vehicle/tests/types_tests.rs` - IDM parameter validation for all 7 vehicle types
- `crates/velos-gpu/shaders/wave_front.wgsl` - AgentState struct + VT_*/FLAG_* constants
- `crates/velos-gpu/src/compute.rs` - Updated GpuAgentState literals in sort tests
- `crates/velos-gpu/src/sim.rs` - vehicle_type GPU field mapping in step_vehicles_gpu
- `crates/velos-gpu/src/sim_lifecycle.rs` - Extended spawn logic for new vehicle types
- `crates/velos-gpu/src/sim_render.rs` - Color coding for Bus/Bicycle/Truck/Emergency
- `crates/velos-gpu/src/sim_snapshot.rs` - half_width_for_type with realistic widths
- `crates/velos-gpu/tests/*.rs` - All test files updated with vehicle_type/flags fields
- `crates/velos-gpu/benches/dispatch.rs` - Benchmark agent literals updated
- `crates/velos-demand/src/spawner.rs` - SpawnVehicleType extended with 4 new variants
- `crates/velos-demand/tests/demand_tests.rs` - Updated match arm for new variants

## Decisions Made
- Bicycle uses sublane model (same as Motorbike) with 0.3m half-width and IDM v0=4.17 m/s (15 km/h)
- Bus/Truck/Emergency use lane-based model (same as Car) with 30/70 Krauss/IDM split
- VehicleType enum order matches GPU u32 mapping: Motorbike=0, Car=1, Bus=2, Bicycle=3, Truck=4, Emergency=5, Pedestrian=6
- Flags bitfield reserves 3 bits for future bus/emergency/yielding behavior (no branching yet)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed non-exhaustive match in demand_tests.rs**
- **Found during:** Task 2
- **Issue:** velos-demand test had exhaustive match on SpawnVehicleType with only 3 arms; adding 4 new variants broke compilation
- **Fix:** Updated match to group new variants with existing categories (Bicycle with Motorbike, Bus/Truck/Emergency with Car)
- **Files modified:** crates/velos-demand/tests/demand_tests.rs
- **Verification:** cargo test -p velos-demand passes
- **Committed in:** 84be5dd (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Necessary fix for non-exhaustive pattern match. No scope creep.

## Issues Encountered
- Pre-existing compilation error in velos-net test (cleaning.rs:322 missing RoadEdge import) -- not caused by this plan's changes, logged to deferred items

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- GpuAgentState 40-byte layout is the foundation for all Phase 6 plans
- WGSL vehicle type constants ready for shader branching in Plan 06-02+ (bus dwell, emergency yield)
- SpawnVehicleType extended but spawn distribution unchanged (still 80/15/5) -- distribution changes deferred to later plans
- All existing GPU tests remain green with the new struct layout

---
*Phase: 06-agent-models-signal-control*
*Completed: 2026-03-07*
