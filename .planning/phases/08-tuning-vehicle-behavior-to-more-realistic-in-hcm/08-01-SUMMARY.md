---
phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm
plan: 01
subsystem: vehicle-behavior
tags: [toml, serde, config, idm, krauss, mobil, sublane, hcmc-calibration]

requires:
  - phase: 06-agent-models-signal-control
    provides: VehicleType enum, IDM/Krauss/MOBIL/sublane param structs
provides:
  - VehicleConfig TOML infrastructure with serde deserialization and validation
  - HCMC-calibrated vehicle_params.toml for all 7 vehicle types
  - Config-backed factory functions (default_idm_params_from_config, from_config methods)
  - Per-vehicle-type MOBIL params (motorbike politeness 0.1 vs car 0.3)
affects: [08-02-gpu-parameter-unification, 08-03-behavioral-rules]

tech-stack:
  added: [serde, toml]
  patterns: [toml-config-loading, config-backed-factory-functions, parameter-validation]

key-files:
  created:
    - crates/velos-vehicle/src/config.rs
    - data/hcmc/vehicle_params.toml
    - crates/velos-vehicle/tests/config_tests.rs
  modified:
    - crates/velos-vehicle/Cargo.toml
    - crates/velos-vehicle/src/lib.rs
    - crates/velos-vehicle/src/error.rs
    - crates/velos-vehicle/src/types.rs
    - crates/velos-vehicle/src/krauss.rs
    - crates/velos-vehicle/src/sublane.rs
    - crates/velos-vehicle/src/mobil.rs
    - crates/velos-vehicle/tests/types_tests.rs
    - crates/velos-vehicle/tests/krauss_tests.rs
    - crates/velos-vehicle/tests/sublane_tests.rs
    - crates/velos-vehicle/tests/idm_tests.rs

key-decisions:
  - "Truck v0 changed from 25.0 m/s (90 km/h) to 9.7 m/s (35 km/h) for HCMC urban"
  - "Car v0 changed from 13.9 m/s (50 km/h) to 9.7 m/s (35 km/h) for HCMC congestion"
  - "Motorbike t_headway reduced from 1.0s to 0.8s for HCMC aggressive following"
  - "VehicleConfig::default() provides hardcoded fallback matching TOML file exactly"
  - "Pedestrian IDM params kept hardcoded (social force is primary model)"

patterns-established:
  - "TOML config loading: load_vehicle_config(path) with load_vehicle_config_from_str for tests"
  - "Config-backed factories: default_X_params() delegates to VehicleConfig::default()"
  - "from_config() on param structs: KraussParams::from_config(&VehicleTypeParams)"
  - "Config validation: VehicleConfig::validate() returns descriptive Vec<String> errors"

requirements-completed: [TUN-01, TUN-03]

duration: 5min
completed: 2026-03-08
---

# Phase 8 Plan 1: Vehicle Config Infrastructure Summary

**TOML config infrastructure with HCMC-calibrated defaults for all 7 vehicle types, replacing hardcoded literature values with realistic Ho Chi Minh City parameters**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-08T03:51:37Z
- **Completed:** 2026-03-08T03:56:56Z
- **Tasks:** 2
- **Files modified:** 14

## Accomplishments
- Created VehicleConfig module with serde TOML deserialization, validation, and conversion methods to all existing param structs (IdmParams, KraussParams, MobilParams, SublaneParams)
- HCMC-calibrated vehicle_params.toml with per-vehicle-type sections covering IDM, Krauss, MOBIL, and sublane parameters
- Migrated all factory functions to return HCMC-calibrated values (truck v0: 25.0->9.7, car v0: 13.9->9.7, bus v0: 11.1->8.3)
- Added from_config() methods on all param structs for config-driven parameter loading
- 107 tests pass across all test files (16 new config tests, 3 new types tests, updated assertions throughout)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create VehicleConfig module + TOML config file with HCMC defaults** - `a8b53cd` (feat)
2. **Task 2: Migrate factory functions to config-backed defaults + update existing tests** - `f075b82` (feat)

## Files Created/Modified
- `crates/velos-vehicle/src/config.rs` - VehicleConfig, VehicleTypeParams, PedestrianParams with TOML loading and validation
- `data/hcmc/vehicle_params.toml` - HCMC-calibrated per-vehicle-type parameter defaults
- `crates/velos-vehicle/tests/config_tests.rs` - 16 tests for parsing, ranges, validation, conversions
- `crates/velos-vehicle/src/types.rs` - Factory functions now delegate to VehicleConfig::default()
- `crates/velos-vehicle/src/krauss.rs` - sumo_default() returns HCMC car values, added from_config()
- `crates/velos-vehicle/src/sublane.rs` - Default updated to HCMC values, added from_config()
- `crates/velos-vehicle/src/mobil.rs` - Added MobilParams::from_config()
- `crates/velos-vehicle/src/error.rs` - Added ConfigLoad, ConfigParse, ConfigValidation variants
- `crates/velos-vehicle/Cargo.toml` - Added serde and toml workspace dependencies

## Decisions Made
- Truck v0 changed from 25.0 m/s (90 km/h literature) to 9.7 m/s (35 km/h HCMC urban) -- trucks crawl in city center
- Car v0 changed from 13.9 m/s (50 km/h) to 9.7 m/s (35 km/h) -- HCMC is congested
- Motorbike t_headway reduced from 1.0s to 0.8s -- motorbikes follow more aggressively in HCMC
- SublaneParams default min_filter_gap changed from 0.6m to 0.5m -- motorbikes squeeze through tighter gaps
- SublaneParams default max_lateral_speed changed from 1.0 to 1.2 m/s -- faster lateral filtering
- VehicleConfig::default() hardcoded fallback matches TOML file exactly for resilience

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed idm_tests::motorbike_params_differ_from_car assertion**
- **Found during:** Task 2
- **Issue:** Test asserted `moto.v0 < car.v0` but with HCMC calibration motorbike v0 (11.1) > car v0 (9.7) -- motorbikes are faster than cars in congested HCMC traffic
- **Fix:** Reversed assertion to `moto.v0 > car.v0` with updated comment explaining HCMC dynamics
- **Files modified:** crates/velos-vehicle/tests/idm_tests.rs
- **Committed in:** f075b82 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Assertion direction fix necessary for correctness with new HCMC parameter values. No scope creep.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- VehicleConfig module ready for GPU parameter unification (Plan 02)
- from_config() methods ready for behavioral rules integration (Plan 03)
- All existing tests updated and passing with HCMC-calibrated values

---
*Phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm*
*Completed: 2026-03-08*
