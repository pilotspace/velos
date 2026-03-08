---
phase: 15-file-size-reduction-housekeeping
plan: 01
subsystem: gpu
tags: [refactoring, file-size, velos-gpu, module-extraction]

requires:
  - phase: none
    provides: existing sim.rs and compute.rs implementations

provides:
  - sim_signals.rs with signal-related SimWorld methods
  - sim_vehicles.rs with GPU vehicle stepping method
  - compute_wave_front.rs with wave-front upload/dispatch/readback + helpers
  - compute_tests.rs with extracted test module

affects: [velos-gpu]

tech-stack:
  added: []
  patterns: [pub(crate) field visibility for cross-module impl blocks, path-based test module extraction]

key-files:
  created:
    - crates/velos-gpu/src/sim_signals.rs
    - crates/velos-gpu/src/sim_vehicles.rs
    - crates/velos-gpu/src/compute_wave_front.rs
    - crates/velos-gpu/src/compute_tests.rs
  modified:
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/compute.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-gpu/src/sim_bus.rs
    - crates/velos-gpu/src/sim_mobil.rs

key-decisions:
  - "WaveFrontParams fields made pub(crate) for cross-module impl block access"
  - "Tests extracted to compute_tests.rs via #[path] attribute to bring compute.rs under 700 lines"
  - "Re-exports in compute.rs preserve API stability (compute_agent_flags, sort_agents_by_lane, bgl_entry)"

patterns-established:
  - "Path-based test extraction: #[cfg(test)] #[path = 'x_tests.rs'] mod tests when tests push file over 700 lines"

requirements-completed: []

duration: 13min
completed: 2026-03-08
---

# Phase 15 Plan 01: File Size Reduction Summary

**Split sim.rs (948->663 lines) and compute.rs (1119->471 lines) into focused submodules with zero behavioral changes**

## Performance

- **Duration:** 13 min
- **Started:** 2026-03-08T15:35:48Z
- **Completed:** 2026-03-08T15:49:00Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments

- sim.rs reduced from 948 to 663 lines by extracting signal methods and vehicle GPU stepping
- compute.rs reduced from 1119 to 471 lines by extracting wave-front pipeline methods, helpers, and tests
- All 98 velos-gpu tests pass with zero behavioral changes
- Clippy clean (also fixed 8 pre-existing clippy warnings as blocking issues)

## Task Commits

Each task was committed atomically:

1. **Task 1: Extract sim.rs signal and vehicle methods** - `322ca36` (refactor)
2. **Task 2: Extract compute.rs wave-front methods** - `d670ba1` (refactor)

## Files Created/Modified

- `crates/velos-gpu/src/sim_signals.rs` - Signal stepping, loop detector updates, signal priority (168 lines)
- `crates/velos-gpu/src/sim_vehicles.rs` - GPU vehicle physics step_vehicles_gpu (145 lines)
- `crates/velos-gpu/src/compute_wave_front.rs` - Wave-front upload/dispatch/readback + sort/flags/bgl helpers (299 lines)
- `crates/velos-gpu/src/compute_tests.rs` - All compute module tests extracted (296 lines)
- `crates/velos-gpu/src/sim.rs` - Reduced from 948 to 663 lines
- `crates/velos-gpu/src/compute.rs` - Reduced from 1119 to 471 lines
- `crates/velos-gpu/src/lib.rs` - Added mod declarations for new modules
- `crates/velos-gpu/src/sim_bus.rs` - Fixed pre-existing collapsible if clippy warning
- `crates/velos-gpu/src/sim_mobil.rs` - Fixed pre-existing doc continuation clippy warning

## Decisions Made

- WaveFrontParams struct and fields made pub(crate) so compute_wave_front.rs impl block can construct it
- ComputeDispatcher wave-front fields changed from private to pub(crate) for cross-file impl access
- Tests moved to separate file via `#[cfg(test)] #[path = "compute_tests.rs"] mod tests` to keep compute.rs under 700 lines
- Re-exports added in compute.rs for API compatibility (compute_agent_flags, sort_agents_by_lane, bgl_entry)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed 8 pre-existing clippy warnings**
- **Found during:** Task 1 (clippy verification step)
- **Issue:** clippy -D warnings failed due to pre-existing doc list items (6), map_or (1), collapsible if (1) across sim.rs, sim_bus.rs, sim_mobil.rs
- **Fix:** Converted numbered doc list to unordered list with blank line before continuation; used is_some_and instead of map_or; collapsed nested if
- **Files modified:** sim.rs, sim_bus.rs, sim_mobil.rs, sim_vehicles.rs
- **Verification:** cargo clippy -p velos-gpu -- -D warnings passes clean
- **Committed in:** 322ca36 (Task 1 commit)

**2. [Rule 3 - Blocking] Made WaveFrontParams and fields pub(crate)**
- **Found during:** Task 2 (cross-module impl block compilation)
- **Issue:** compute_wave_front.rs impl block could not access private struct and fields
- **Fix:** Changed WaveFrontParams and its fields to pub(crate); changed ComputeDispatcher wave-front fields to pub(crate)
- **Files modified:** compute.rs
- **Verification:** cargo test passes, cargo clippy clean
- **Committed in:** d670ba1 (Task 2 commit)

**3. [Rule 3 - Blocking] Extracted tests to separate file**
- **Found during:** Task 2 (compute.rs line count check)
- **Issue:** After wave-front method extraction, compute.rs was still 834 lines due to 366-line test module
- **Fix:** Moved test module to compute_tests.rs via #[path] attribute, renamed shadowed compute_agent_flags test helper to compute_agent_flags_test to avoid name collision
- **Files modified:** compute.rs, compute_tests.rs (new)
- **Verification:** All 98 tests pass, compute.rs at 471 lines
- **Committed in:** d670ba1 (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (3 blocking)
**Impact on plan:** All auto-fixes necessary for compilation and lint compliance. No scope creep.

## Issues Encountered

None beyond the auto-fixed deviations above.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Both sim.rs and compute.rs are well under the 700-line convention
- Module extraction pattern established for future file splits
- All workspace tests pass

---
*Phase: 15-file-size-reduction-housekeeping*
*Completed: 2026-03-08*
