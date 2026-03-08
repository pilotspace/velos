---
phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm
plan: 03
subsystem: vehicle-behavior
tags: [motorbike, sublane, gap-acceptance, intersection, red-light-creep, hcmc]

requires:
  - phase: 08-01
    provides: VehicleConfig TOML infrastructure with VehicleTypeParams and gap_acceptance_ttc

provides:
  - red_light_creep_speed() for motorbike/bicycle forward creep at red lights
  - effective_filter_gap() with speed-dependent lateral gap widening
  - intersection_gap_acceptance() with vehicle-type TTC thresholds and size intimidation
  - IntersectionState struct for per-agent intersection tracking
  - creep_max_speed and creep_distance_scale config fields in VehicleTypeParams

affects: [velos-gpu, wave-front-shader, simulation-loop]

tech-stack:
  added: []
  patterns: [size-intimidation-factor, wait-time-modifier, forced-acceptance-deadlock-prevention]

key-files:
  created:
    - crates/velos-vehicle/src/intersection.rs
    - crates/velos-vehicle/tests/intersection_tests.rs
  modified:
    - crates/velos-vehicle/src/sublane.rs
    - crates/velos-vehicle/src/config.rs
    - crates/velos-vehicle/src/lib.rs
    - crates/velos-vehicle/tests/sublane_tests.rs
    - data/hcmc/vehicle_params.toml

key-decisions:
  - "Red-light creep limited to motorbike/bicycle types with 0.3 m/s max speed cap"
  - "Speed-dependent gap widening coefficient 0.1 s/m (effective_gap = base + 0.1 * |delta_v|)"
  - "Size intimidation factors: truck/bus=1.3x, emergency=2.0x, motorbike/bicycle=0.8x, car=1.0x"
  - "Forced acceptance after 5s wait (threshold halved) for deadlock prevention"
  - "Wait-time reduction rate 10% per second (gradual first-come priority)"

patterns-established:
  - "Size intimidation pattern: approaching vehicle type modifies gap acceptance threshold"
  - "Wait-time forced acceptance: deadlock prevention via threshold decay over time"
  - "Speed-dependent gap: base gap + linear coefficient * speed difference"

requirements-completed: [TUN-04, TUN-05, TUN-06]

duration: 5min
completed: 2026-03-08
---

# Phase 08 Plan 03: HCMC Behavioral Rules Summary

**Red-light creep for motorbikes, speed-dependent weaving gaps, and intersection gap acceptance with size intimidation and deadlock prevention**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-08T03:59:56Z
- **Completed:** 2026-03-08T04:05:39Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments
- Red-light creep produces gradual forward movement (0-0.3 m/s) for motorbikes/bicycles at red lights, zero for all other vehicle types
- Speed-dependent effective filter gap scales with speed difference (0.5m base + 0.1 * delta_v), integrated into compute_desired_lateral()
- Intersection gap acceptance with vehicle-type TTC thresholds, size intimidation factors, and max-wait forced acceptance after 5s
- 37 total new tests (20 sublane + 13 intersection integration + 4 intersection unit) all passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Red-light creep + speed-dependent weaving gap** - `ebc5710` (feat)
2. **Task 2: Intersection gap acceptance with size-dependent thresholds** - `478ff5c` (feat)

## Files Created/Modified
- `crates/velos-vehicle/src/intersection.rs` - Gap acceptance with size intimidation and deadlock prevention
- `crates/velos-vehicle/src/sublane.rs` - Red-light creep speed and effective filter gap functions
- `crates/velos-vehicle/src/config.rs` - Added creep_max_speed/creep_distance_scale to VehicleTypeParams
- `crates/velos-vehicle/src/lib.rs` - Added pub mod intersection
- `crates/velos-vehicle/tests/intersection_tests.rs` - 13 tests for gap acceptance behaviors
- `crates/velos-vehicle/tests/sublane_tests.rs` - 20 tests including creep and effective gap
- `data/hcmc/vehicle_params.toml` - Creep parameters for motorbike (0.3 m/s) and bicycle (0.2 m/s)

## Decisions Made
- Red-light creep limited to motorbike/bicycle types with 0.3 m/s max speed cap and 5m ramp distance
- Speed-dependent gap uses linear coefficient 0.1 (simple, predictable, calibratable)
- Size intimidation factors chosen to reflect HCMC driver psychology: motorbikes bold around each other, cautious around trucks/buses
- Forced acceptance at 5s prevents simulation deadlock at unsignalized intersections
- Creep config fields use serde defaults for backward compatibility with existing TOML files

## Deviations from Plan
None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All HCMC behavioral rules implemented and tested
- Phase 08 complete: config infrastructure (08-01), sublane enhancements (08-02), behavioral rules (08-03)
- Ready for GPU shader integration to consume these CPU-side behavioral functions

---
*Phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm*
*Completed: 2026-03-08*
