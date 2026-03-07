---
phase: 05-foundation-gpu-engine
plan: 01
subsystem: simulation-engine
tags: [fixed-point, krauss, car-following, gpu-determinism, ecs]

requires: []
provides:
  - "Q16.16 / Q12.20 / Q8.8 fixed-point types with overflow-safe multiplication"
  - "Krauss car-following model (CPU reference for GPU shader validation)"
  - "CarFollowingModel enum (Idm=0, Krauss=1) for runtime model switching"
  - "GpuAgentState 32-byte #[repr(C)] struct for GPU compute buffers"
affects: [05-02, 05-03, 05-04, 05-05]

tech-stack:
  added: [bytemuck (velos-core), rand (velos-vehicle)]
  patterns: [fixed-point newtype wrappers, half-splitting multiplication, pure-function car-following models]

key-files:
  created:
    - crates/velos-core/src/fixed_point.rs
    - crates/velos-core/tests/fixed_point_tests.rs
    - crates/velos-core/tests/components_tests.rs
    - crates/velos-vehicle/src/krauss.rs
    - crates/velos-vehicle/tests/krauss_tests.rs
  modified:
    - crates/velos-core/src/components.rs
    - crates/velos-core/src/lib.rs
    - crates/velos-core/Cargo.toml
    - crates/velos-vehicle/src/lib.rs
    - crates/velos-vehicle/Cargo.toml

key-decisions:
  - "Used i64 intermediate in fix_mul_mixed for cross-format multiplication (Q12.20 * Q16.16) -- simpler and correct; GPU shader will use half-splitting"
  - "FixPos/FixSpd/FixLat use wrapping_add/wrapping_sub to match GPU u32 wrapping semantics"
  - "CarFollowingModel enum variant named Idm (not IDM) to follow Rust naming conventions"
  - "GpuAgentState acceleration field uses Q12.20 (same as speed) for consistency"

patterns-established:
  - "Fixed-point newtype pattern: #[repr(transparent)] i32 wrapper with from_f64/to_f64/raw/from_raw API"
  - "Car-following model pattern: pure functions with params struct, f64 precision, no ECS dependency (same as idm.rs)"
  - "GPU struct pattern: #[repr(C)] + bytemuck::Pod/Zeroable with fixed-point i32 fields and u32 tags"

requirements-completed: [GPU-04, CFM-01, CFM-02]

duration: 5min
completed: 2026-03-07
---

# Phase 5 Plan 01: Fixed-Point Types & Krauss Model Summary

**Q16.16/Q12.20/Q8.8 fixed-point arithmetic with overflow-safe multiplication, SUMO-faithful Krauss car-following model, and GPU agent state struct**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-07T12:13:24Z
- **Completed:** 2026-03-07T12:18:29Z
- **Tasks:** 2
- **Files modified:** 10

## Accomplishments
- Three fixed-point types (FixPos Q16.16, FixSpd Q12.20, FixLat Q8.8) with conversions, arithmetic, and ordering
- Overflow-safe Q16.16 multiplication via 16-bit half-splitting (mirrors WGSL shader pattern)
- Cross-format multiplication (speed * dt -> displacement) for position updates
- SUMO-faithful Krauss model: safe-speed formula, stochastic dawdle, full velocity update
- CarFollowingModel enum with #[repr(u8)] and GpuAgentState 32-byte #[repr(C)] struct
- 34 total new tests across both crates, all passing with zero clippy warnings

## Task Commits

Each task was committed atomically:

1. **Task 1: Fixed-point arithmetic types** - `a286825` (feat)
2. **Task 2: Krauss car-following model + CarFollowingModel ECS component** - `3767bd1` (feat)

## Files Created/Modified
- `crates/velos-core/src/fixed_point.rs` - Q16.16/Q12.20/Q8.8 newtype wrappers with arithmetic and multiplication
- `crates/velos-core/tests/fixed_point_tests.rs` - 18 tests: roundtrip conversions, multiplication edge cases, arithmetic
- `crates/velos-core/tests/components_tests.rs` - 5 tests: enum discriminants, struct size, bytemuck cast
- `crates/velos-core/src/components.rs` - Added CarFollowingModel enum and GpuAgentState struct
- `crates/velos-core/src/lib.rs` - Added fixed_point module and re-exports
- `crates/velos-core/Cargo.toml` - Added bytemuck dependency
- `crates/velos-vehicle/src/krauss.rs` - SUMO-faithful Krauss model (safe speed, dawdle, update)
- `crates/velos-vehicle/tests/krauss_tests.rs` - 11 tests: safe speed, dawdle, update behavior
- `crates/velos-vehicle/src/lib.rs` - Added krauss module
- `crates/velos-vehicle/Cargo.toml` - Added rand dependency

## Decisions Made
- Used i64 intermediate in `fix_mul_mixed` for cross-format multiplication -- simpler and correct on CPU; GPU shader will use half-splitting technique instead
- Fixed-point types use `wrapping_add`/`wrapping_sub` to match GPU unsigned wrapping semantics
- Named enum variant `Idm` (not `IDM`) to follow Rust naming conventions for enum variants
- GpuAgentState acceleration field uses Q12.20 format (same as speed) for consistency, not Q8.24 as initially suggested in plan
- Used `r#gen()` for rand calls due to Rust 2024 edition reserving `gen` keyword

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Rust 2024 reserves `gen` keyword**
- **Found during:** Task 2 (Krauss implementation)
- **Issue:** `rng.gen()` is a compile error in edition 2024 because `gen` is a reserved keyword
- **Fix:** Changed to `rng.r#gen()` (raw identifier escape)
- **Files modified:** crates/velos-vehicle/src/krauss.rs
- **Verification:** Compilation succeeds, all tests pass
- **Committed in:** 3767bd1 (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** Trivial syntax adaptation for Rust 2024 edition. No scope creep.

## Issues Encountered
None.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Fixed-point types ready for GPU shader implementation (Plan 04: WGSL shaders)
- Krauss CPU reference ready as test oracle for GPU shader validation
- CarFollowingModel enum and GpuAgentState struct ready for GPU buffer creation (Plan 02/03)
- All prerequisite types for wave-front dispatch are in place

---
*Phase: 05-foundation-gpu-engine*
*Completed: 2026-03-07*
