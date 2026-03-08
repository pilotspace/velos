---
phase: 01-gpu-foundation-spikes
plan: 01
subsystem: gpu
tags: [rust, wgpu, hecs, metal, wgsl, compute-shader, ecs]

# Dependency graph
requires: []
provides:
  - Cargo workspace with velos-core + velos-gpu crates
  - velos_core::Position and Kinematics ECS components (f64 CPU-side)
  - velos_core::cfl_check numerical stability check
  - GpuContext::new_headless() Metal adapter initialization
  - BufferPool double-buffered SoA GPU buffers with ECS upload
  - ComputeDispatcher with WGSL agent_update shader
  - GPU round-trip tests (test_compute_dispatch, test_f32_f64_tolerance, test_round_trip_1k) -- all PASS on Metal
  - Nightly benchmarks (frame_time, throughput)
affects: [02-road-vehicles-egui, 03-motorbike-pedestrian]

# Tech tracking
tech-stack:
  added: [wgpu 28, hecs 0.11, bytemuck 1, thiserror 2, pollster 0.4, glam 0.29, env_logger 0.11]
  patterns:
    - Double-buffered SoA GPU buffers (pos_front/back, kin_front/back) -- swap after dispatch
    - GpuContext wraps Device + Queue + Adapter as single resource
    - GPU integration tests gated by --features gpu-tests feature flag
    - Tests skip gracefully when no GPU adapter available (no panic)

key-files:
  created:
    - Cargo.toml
    - rust-toolchain.toml
    - crates/velos-core/src/cfl.rs
    - crates/velos-core/src/components.rs
    - crates/velos-gpu/src/device.rs
    - crates/velos-gpu/src/buffers.rs
    - crates/velos-gpu/src/compute.rs
    - crates/velos-gpu/shaders/agent_update.wgsl
    - crates/velos-gpu/tests/gpu_round_trip.rs
    - crates/velos-gpu/benches/dispatch.rs
  modified: []

key-decisions:
  - "wgpu 28 API uses PollType::wait_indefinitely() instead of Maintain::Wait -- updated all poll calls"
  - "wgpu 28 PipelineLayoutDescriptor uses immediate_size instead of push_constant_ranges -- replaced throughout"
  - "wgpu 28 request_adapter returns Result not Option -- use .ok()? for graceful None on no GPU"
  - "hecs 0.11 Entity in query is flat tuple (Entity, &Pos, &Kin) not nested -- query pattern corrected"
  - "All BufferPool buffers use STORAGE | COPY_SRC | COPY_DST to support upload + copy + readback"
  - "GO for Plan 02: all 3 GPU integration tests pass on Metal (test_compute_dispatch, test_f32_f64_tolerance, test_round_trip_1k)"

patterns-established:
  - "GPU test gating: #![cfg(feature = \"gpu-tests\")] in test file, --features velos-gpu/gpu-tests at CLI"
  - "GPU graceful skip: match GpuContext::new_headless() { None => { eprintln!(\"SKIP\"); return; } }"
  - "Buffer swap loop: upload_from_ecs -> copy_back_to_front -> dispatch -> submit -> poll -> swap -> readback"

requirements-completed: [GPU-01, GPU-02, GPU-03, GPU-04, PERF-01, PERF-02]

# Metrics
duration: 9min
completed: 2026-03-06
---

# Phase 1 Plan 01: GPU Foundation Spikes Summary

**wgpu 28 Metal compute pipeline with hecs ECS round-trip: 1K agents dispatched and read back with f32/f64 tolerance verified -- GO for Plan 02**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-06T08:22:22Z
- **Completed:** 2026-03-06T08:31:07Z
- **Tasks:** 2
- **Files modified:** 18

## Accomplishments
- Cargo workspace bootstrapped with nightly-2025-12-01 toolchain, hecs + wgpu 28 dependencies
- velos-core crate: Position/Kinematics f64 components, cfl_check with 6 unit tests (all pass)
- velos-gpu crate: GpuContext headless init, double-buffered SoA BufferPool, ComputeDispatcher with WGSL
- All 3 GPU integration tests PASS on Metal: compute_dispatch, f32_f64_tolerance, round_trip_1k
- Benchmarks compile under nightly `#[bench]` harness (frame_time, throughput)

## Task Commits

Each task was committed atomically:

1. **Task 1: Workspace bootstrap + velos-core (components, CFL, error)** - `fae20bb` (feat)
2. **Task 2: velos-gpu device + buffer pool + compute dispatcher + WGSL shader** - `5b8755c` (feat)

## Files Created/Modified

| File | Lines | Purpose |
|------|-------|---------|
| `Cargo.toml` | 20 | Workspace root with members and workspace deps |
| `rust-toolchain.toml` | 3 | Pins nightly-2025-12-01 |
| `README.md` | 21 | Build and test instructions |
| `crates/velos-core/Cargo.toml` | 9 | velos-core crate manifest |
| `crates/velos-core/src/lib.rs` | 9 | Public re-exports |
| `crates/velos-core/src/components.rs` | 24 | Position + Kinematics f64 ECS components |
| `crates/velos-core/src/cfl.rs` | 58 | cfl_check + 6 unit tests |
| `crates/velos-core/src/error.rs` | 11 | CoreError::CflViolation |
| `crates/velos-gpu/Cargo.toml` | 21 | velos-gpu manifest with gpu-tests feature |
| `crates/velos-gpu/src/lib.rs` | 12 | Public re-exports |
| `crates/velos-gpu/src/device.rs` | 43 | GpuContext::new_headless() |
| `crates/velos-gpu/src/buffers.rs` | 141 | BufferPool + GpuPosition/GpuKinematics |
| `crates/velos-gpu/src/compute.rs` | 215 | ComputeDispatcher + readback_positions |
| `crates/velos-gpu/src/error.rs` | 16 | GpuError variants |
| `crates/velos-gpu/shaders/agent_update.wgsl` | 36 | WGSL Euler integration shader |
| `crates/velos-gpu/tests/gpu_round_trip.rs` | 232 | 3 GPU integration tests |
| `crates/velos-gpu/benches/dispatch.rs` | 112 | frame_time + throughput benchmarks |
| `benchmarks/baseline.json` | 5 | Empty baseline for future bench tracking |

## Decisions Made
- wgpu 28 changed several APIs from the plan's spec: `Maintain::Wait` -> `PollType::wait_indefinitely()`, `push_constant_ranges` -> `immediate_size`, `request_adapter` returns `Result` not `Option`. All updated inline during implementation.
- BufferPool buffers given `COPY_SRC | COPY_DST` in addition to `STORAGE` to support the upload-copy-dispatch-readback test pattern.
- Clippy `manual_div_ceil` lint: replaced `(count + size - 1) / size` with `count.div_ceil(size)`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] wgpu 28 API: request_adapter returns Result not Option**
- **Found during:** Task 2 (first build attempt)
- **Issue:** Plan's device.rs used `?` operator expecting `Option<Adapter>` but wgpu 28 returns `Result<Adapter, RequestAdapterError>`
- **Fix:** Added `.ok()?` to convert Result to Option for graceful None return
- **Files modified:** `crates/velos-gpu/src/device.rs`
- **Verification:** Build succeeds, GpuContext::new_headless() returns Some on Metal
- **Committed in:** 5b8755c

**2. [Rule 1 - Bug] wgpu 28 API: Maintain::Wait removed, replaced by PollType**
- **Found during:** Task 2 (first build attempt)
- **Issue:** `wgpu::Maintain::Wait` no longer exists in wgpu 28; polling API changed to `PollType`
- **Fix:** Replaced all `device.poll(wgpu::Maintain::Wait)` with `let _ = device.poll(wgpu::PollType::wait_indefinitely())`
- **Files modified:** `crates/velos-gpu/src/compute.rs`, `crates/velos-gpu/tests/gpu_round_trip.rs`, `crates/velos-gpu/benches/dispatch.rs`
- **Verification:** GPU tests pass, polling works correctly on Metal
- **Committed in:** 5b8755c

**3. [Rule 1 - Bug] wgpu 28 API: PipelineLayoutDescriptor field renamed**
- **Found during:** Task 2 (first build attempt)
- **Issue:** `push_constant_ranges` field renamed to `immediate_size: u32` in wgpu 28
- **Fix:** Replaced `push_constant_ranges: &[]` with `immediate_size: 0`
- **Files modified:** `crates/velos-gpu/src/compute.rs`
- **Verification:** Pipeline creation succeeds
- **Committed in:** 5b8755c

**4. [Rule 1 - Bug] wgpu 28 DeviceDescriptor missing required fields**
- **Found during:** Task 2 (first build attempt)
- **Issue:** `DeviceDescriptor` now has `experimental_features` and `trace` fields not in plan's template
- **Fix:** Used `..Default::default()` for the missing fields
- **Files modified:** `crates/velos-gpu/src/device.rs`
- **Verification:** Device creation succeeds on Metal
- **Committed in:** 5b8755c

**5. [Rule 1 - Bug] hecs 0.11 query tuple is flat, not nested**
- **Found during:** Task 2 (first build attempt)
- **Issue:** Plan used `for (entity, (pos, kin))` pattern but hecs yields flat tuple `(Entity, &Position, &Kinematics)`
- **Fix:** Changed destructuring to `for (entity, pos, kin)` and updated import to include `hecs::Entity`
- **Files modified:** `crates/velos-gpu/src/buffers.rs`
- **Verification:** ECS upload correctly maps entities to GPU slots
- **Committed in:** 5b8755c

**6. [Rule 1 - Bug] Clippy: manual_div_ceil lint**
- **Found during:** Task 2 (clippy run)
- **Issue:** `(pool.agent_count + WORKGROUP_SIZE - 1) / WORKGROUP_SIZE` triggers clippy::manual_div_ceil
- **Fix:** Replaced with `pool.agent_count.div_ceil(WORKGROUP_SIZE)`
- **Files modified:** `crates/velos-gpu/src/compute.rs`
- **Verification:** `cargo clippy --all-targets -- -D warnings` exits 0
- **Committed in:** 5b8755c

---

**Total deviations:** 6 auto-fixed (all Rule 1 - API/bug fixes for wgpu 28 + hecs 0.11 differences from plan spec)
**Impact on plan:** All fixes were necessary to match actual library versions. No scope creep. Architecture unchanged.

## Issues Encountered
- velos-gpu crate must exist before `cargo test -p velos-core` works (workspace member reference). Created full GPU implementation in one pass rather than stub-then-implement.

## User Setup Required
None - no external service configuration required. Metal GPU available on this machine.

## Next Phase Readiness

**GO for Plan 02.** All GPU-01 through GPU-04 and PERF-01/PERF-02 requirements satisfied:
- GPU-01: Compute shader dispatches and writes position data back (PASS)
- GPU-02: f32 GPU matches f64 CPU within 1e-4 tolerance (PASS)
- GPU-03: 1000 hecs entities round-trip with positions within 0.01 tolerance (PASS)
- GPU-04: CFL check returns correct boolean for all 6 test cases (PASS)
- PERF-01/PERF-02: Benchmarks compile and run under nightly harness

Plan 02 can proceed to implement road graph, vehicle rendering, and egui controls.

---
*Phase: 01-gpu-foundation-spikes*
*Completed: 2026-03-06*
