---
phase: 02-road-network-vehicle-models-egui
plan: 03
subsystem: demand
tags: [od-matrix, time-of-day, spawner, stochastic, rand, seeded-rng]

requires:
  - phase: 01-gpu-foundation-spikes
    provides: "workspace Cargo.toml, edition 2024, thiserror/log workspace deps"
provides:
  - "OdMatrix with District 1 POC 5-zone trip volumes (560 trips/hr)"
  - "TodProfile with HCMC weekday AM/PM peak demand scaling"
  - "Spawner with 80% motorbike / 15% car / 5% pedestrian distribution"
  - "SpawnRequest and SpawnVehicleType types for downstream integration"
affects: [02-04-integration, velos-demand]

tech-stack:
  added: [rand 0.8, WeightedIndex, StdRng]
  patterns: [seeded-rng-determinism, piecewise-linear-interpolation, bernoulli-fractional-spawning]

key-files:
  created:
    - crates/velos-demand/Cargo.toml
    - crates/velos-demand/src/lib.rs
    - crates/velos-demand/src/error.rs
    - crates/velos-demand/src/od_matrix.rs
    - crates/velos-demand/src/tod_profile.rs
    - crates/velos-demand/src/spawner.rs
    - crates/velos-demand/tests/demand_tests.rs
  modified:
    - Cargo.toml

key-decisions:
  - "SpawnVehicleType defined locally in velos-demand to avoid circular dependency with velos-vehicle"
  - "Bernoulli fractional spawning: integer part deterministic + fractional part probabilistic"
  - "gen_range(0.0..1.0) instead of Rng::gen::<f64>() due to gen being reserved keyword in Rust 2024 edition"

patterns-established:
  - "Seeded RNG pattern: StdRng::seed_from_u64(seed) for reproducible simulation runs"
  - "Factory methods for POC data: district1_poc(), hcmc_weekday()"
  - "Piecewise-linear interpolation with boundary clamping for time-series profiles"

requirements-completed: [DEM-01, DEM-02, DEM-03]

duration: 6min
completed: 2026-03-06
---

# Phase 2 Plan 3: Demand Generation Summary

**OD matrix with 5-zone District 1 POC, HCMC weekday ToD profile, and stochastic agent spawner with 80/15/5 motorbike/car/pedestrian distribution**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-06T14:32:38Z
- **Completed:** 2026-03-06T14:38:57Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- OdMatrix with HashMap-backed zone-to-zone trip storage and District 1 POC factory (9 OD pairs, 560 trips/hr)
- TodProfile with piecewise-linear interpolation and HCMC weekday profile (12 control points, AM/PM peaks at factor 1.0)
- Spawner combining OD + ToD with WeightedIndex vehicle type distribution and seeded StdRng determinism
- 27 tests passing (8 unit + 19 integration), clippy clean, workspace builds

## Task Commits

Each task was committed atomically:

1. **Task 1: OD matrix + time-of-day profiles** (TDD)
   - `5347536` test(02-03): add failing tests for OD matrix and ToD profile
   - `f883b5b` feat(02-03): implement OD matrix and time-of-day demand profiles

2. **Task 2: Agent spawner with vehicle type distribution** (TDD)
   - `7ea350a` test(02-03): add failing tests for agent spawner
   - `b5ad050` feat(02-03): implement agent spawner with 80/15/5 vehicle type distribution

## Files Created/Modified
- `crates/velos-demand/Cargo.toml` - Crate manifest with rand, thiserror, log deps
- `crates/velos-demand/src/lib.rs` - Crate root re-exporting all public types
- `crates/velos-demand/src/error.rs` - DemandError enum (InvalidTime, InvalidZone)
- `crates/velos-demand/src/od_matrix.rs` - OdMatrix + Zone enum with District 1 POC factory
- `crates/velos-demand/src/tod_profile.rs` - TodProfile with piecewise-linear interpolation
- `crates/velos-demand/src/spawner.rs` - Spawner + SpawnRequest + SpawnVehicleType
- `crates/velos-demand/tests/demand_tests.rs` - 19 integration tests
- `Cargo.toml` - Added velos-demand to workspace members, rand to workspace deps

## Decisions Made
- **SpawnVehicleType local enum:** Defined in velos-demand rather than importing from velos-vehicle to avoid circular dependency. Integration plan 02-04 will map between the two.
- **Bernoulli fractional spawning:** For expected counts < 1.0, use probability sampling rather than rounding. Prevents systematic undercounting at small timesteps.
- **gen_range vs gen:** Rust 2024 edition reserves `gen` keyword. Used `gen_range(0.0..1.0)` instead of `Rng::gen::<f64>()`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Created missing lib.rs for velos-net**
- **Found during:** Task 1 (workspace compilation)
- **Issue:** velos-net crate had source files from plan 02-01 but no lib.rs, preventing workspace build
- **Fix:** Created minimal lib.rs re-exporting modules
- **Files modified:** crates/velos-net/src/lib.rs
- **Verification:** cargo check --workspace passes
- **Committed in:** 5347536

**2. [Rule 1 - Bug] Fixed `gen` reserved keyword in Rust 2024 edition**
- **Found during:** Task 2 (spawner compilation)
- **Issue:** `Rng::gen::<f64>()` fails to compile because `gen` is reserved in edition 2024
- **Fix:** Replaced with `gen_range(0.0..1.0)`
- **Files modified:** crates/velos-demand/src/spawner.rs
- **Verification:** cargo build + all tests pass
- **Committed in:** b5ad050

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes necessary for compilation. No scope creep.

## Issues Encountered
None beyond the auto-fixed deviations above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Demand crate complete with all public types exported
- SpawnRequest/SpawnVehicleType ready for integration plan 02-04 to map to ECS entities
- OdMatrix.district1_poc() and TodProfile.hcmc_weekday() provide immediate POC data
- Plan 02-04 (integration) can wire spawner output to road graph positions

---
*Phase: 02-road-network-vehicle-models-egui*
*Completed: 2026-03-06*
