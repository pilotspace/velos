---
phase: 06-agent-models-signal-control
plan: 07
subsystem: simulation
tags: [meso, bpr, queue-model, buffer-zone, idm, smoothstep]

requires:
  - phase: 06-agent-models-signal-control
    provides: "IdmParams struct and idm_acceleration from velos-vehicle"
provides:
  - "SpatialQueue with BPR O(1) exit rate for mesoscopic edges"
  - "BufferZone with C1-continuous IDM interpolation over 100m"
  - "ZoneConfig for static meso/micro/buffer edge designation"
  - "velocity_matching_speed for phantom-braking prevention"
affects: [velos-core, velos-demand, velos-gpu]

tech-stack:
  added: [toml, serde]
  patterns: [bpr-queue, smoothstep-interpolation, zone-config]

key-files:
  created:
    - crates/velos-meso/Cargo.toml
    - crates/velos-meso/src/lib.rs
    - crates/velos-meso/src/queue_model.rs
    - crates/velos-meso/src/buffer_zone.rs
    - crates/velos-meso/src/zone_config.rs
    - crates/velos-meso/tests/queue_model_tests.rs
    - crates/velos-meso/tests/buffer_zone_tests.rs
  modified:
    - Cargo.toml

key-decisions:
  - "BPR beta field uses fast-path multiplication for beta=4.0, powf fallback for non-standard"
  - "ZoneConfig defaults unconfigured edges to Micro (safe default: full simulation)"
  - "BufferZone::should_insert uses static 100m and 2.0 m/s thresholds"
  - "smoothstep (3x^2-2x^3) chosen over hermite for simpler C1 continuity"

patterns-established:
  - "BPR queue pattern: SpatialQueue::new(t_free, capacity) with O(1) try_exit"
  - "Zone designation: ZoneConfig with TOML or centroid-distance auto-classification"
  - "Buffer interpolation: smoothstep-weighted lerp between relaxed and normal IDM params"

requirements-completed: [AGT-05, AGT-06]

duration: 6min
completed: 2026-03-07
---

# Phase 6 Plan 7: Mesoscopic Queue Model Summary

**BPR spatial queue with O(1) exit rate, 100m graduated buffer zone using C1-continuous smoothstep IDM interpolation, and static zone configuration for meso/micro/buffer edge designation**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-07T14:41:55Z
- **Completed:** 2026-03-07T14:47:57Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments
- BPR queue model with standard coefficients (alpha=0.15, beta=4.0) and O(1) FIFO exit
- Zone configuration supporting TOML file loading and centroid-distance auto-designation
- Buffer zone with C1-continuous smoothstep IDM interpolation preventing phantom braking
- Velocity-matching insertion ensuring smooth meso-micro boundary transitions
- 33 total tests across unit and integration test suites

## Task Commits

Each task was committed atomically:

1. **Task 1: SpatialQueue BPR model and zone configuration** - `12b137f` (feat)
2. **Task 2: Buffer zone IDM interpolation and velocity-matching insertion** - `646d3cc` (feat)

## Files Created/Modified
- `crates/velos-meso/Cargo.toml` - Crate manifest with velos-vehicle, serde, toml, thiserror deps
- `crates/velos-meso/src/lib.rs` - Module exports and MesoError enum
- `crates/velos-meso/src/queue_model.rs` - SpatialQueue with BPR travel time and FIFO exit
- `crates/velos-meso/src/buffer_zone.rs` - Smoothstep interpolation, velocity matching, BufferZone
- `crates/velos-meso/src/zone_config.rs` - ZoneConfig with TOML and centroid-distance support
- `crates/velos-meso/tests/queue_model_tests.rs` - 12 integration tests for BPR and zone config
- `crates/velos-meso/tests/buffer_zone_tests.rs` - 15 integration tests for buffer zone
- `Cargo.toml` - Added velos-meso to workspace members

## Decisions Made
- BPR beta field stored for generality but uses fast-path `vc_sq * vc_sq` multiplication when beta=4.0 (standard case), falling back to `powf` for non-standard values
- Unconfigured edges default to Micro (full simulation) -- safe default ensuring no edges silently downgrade to mesoscopic
- `BufferZone::should_insert` uses compile-time constants (100m distance, 2.0 m/s speed diff) rather than runtime config to keep the insertion logic simple and predictable
- Used smoothstep (3x^2 - 2x^3) over cubic hermite for simpler C1 continuity with zero derivatives at both boundary endpoints

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed test expectations for BPR travel time with non-zero V/C**
- **Found during:** Task 1 (TDD GREEN phase)
- **Issue:** Tests assumed single-vehicle queue has exactly t_free travel time, but BPR formula gives slightly higher value when V/C > 0
- **Fix:** Updated tests to use large capacity (10000) for single-vehicle tests, making V/C negligible
- **Files modified:** crates/velos-meso/tests/queue_model_tests.rs
- **Verification:** All 12 queue model tests pass
- **Committed in:** 12b137f (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Test expectation correction only. No scope creep.

## Issues Encountered
- Pre-existing `velos-signal` test compilation failure (missing `use bytemuck::Zeroable` import) -- unrelated to this plan, not fixed per deviation scope rules

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- velos-meso crate is functional and tested, ready for integration with velos-core simulation loop
- Meso-micro feature remains disabled by default (SimConfig::meso_enabled = false) per plan
- Integration with actual RoadGraph edge IDs requires wiring in a future plan

## Self-Check: PASSED

All 8 files verified present. Both task commits (12b137f, 646d3cc) confirmed in git history.

---
*Phase: 06-agent-models-signal-control*
*Completed: 2026-03-07*
