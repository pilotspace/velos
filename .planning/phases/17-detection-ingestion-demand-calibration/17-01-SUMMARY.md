---
phase: 17-detection-ingestion-demand-calibration
plan: 01
subsystem: api
tags: [grpc, tonic, prost, protobuf, tokio, channels]

requires:
  - phase: none
    provides: first plan in phase, no prior dependencies

provides:
  - proto/velos/v2/detection.proto with DetectionService gRPC contract
  - velos-api crate scaffold with tonic-prost codegen
  - ApiError enum with tonic::Status mapping
  - ApiBridge async-sync channel with ApiCommand variants
  - Stub modules for detection, camera, aggregator, calibration

affects: [17-02-PLAN, 17-03-PLAN, 17-04-PLAN]

tech-stack:
  added: [tonic 0.14, prost 0.14, tonic-prost 0.14, tonic-prost-build 0.14, tokio 1]
  patterns: [tonic-prost-build proto compilation, mpsc bridge with oneshot reply, ApiError to tonic::Status mapping]

key-files:
  created:
    - proto/velos/v2/detection.proto
    - crates/velos-api/Cargo.toml
    - crates/velos-api/build.rs
    - crates/velos-api/src/lib.rs
    - crates/velos-api/src/error.rs
    - crates/velos-api/src/bridge.rs
  modified:
    - Cargo.toml

key-decisions:
  - "tonic-prost-build replaces tonic-build::compile_protos (API split in tonic 0.14)"
  - "Bounded mpsc channel (256 capacity) with drain(budget) for per-frame command processing"
  - "Oneshot reply channel embedded in RegisterCamera command for request-response pattern"

patterns-established:
  - "Proto compilation: tonic_prost_build::configure().compile_protos() in build.rs"
  - "Bridge pattern: ApiCommand enum with oneshot reply for request-response, fire-and-forget for batches"
  - "Error mapping: thiserror ApiError -> tonic::Status via From impl"

requirements-completed: [DET-01]

duration: 4min
completed: 2026-03-10
---

# Phase 17 Plan 01: API Scaffold Summary

**Protobuf DetectionService contract with tonic-prost codegen, ApiError types, and bounded mpsc bridge for gRPC-to-SimWorld communication**

## Performance

- **Duration:** 4 min
- **Started:** 2026-03-10T08:11:08Z
- **Completed:** 2026-03-10T08:15:36Z
- **Tasks:** 2
- **Files modified:** 11

## Accomplishments
- Complete protobuf contract with DetectionService (StreamDetections, RegisterCamera, ListCameras RPCs), VehicleClass enum, and all message types
- velos-api crate compiling with tonic-prost-build proto codegen and re-exported generated types
- ApiError with 4 variants mapping to tonic::Status codes (NotFound, InvalidArgument, Unavailable)
- ApiBridge with bounded mpsc channel, oneshot reply for RegisterCamera, drain() with budget
- 9 unit tests all passing, clippy clean

## Task Commits

Each task was committed atomically:

1. **Task 1: Create protobuf definition and velos-api crate scaffold** - `5336d7e` (feat)
2. **Task 2: Implement error types and async-sync bridge** - `bb3236d` (feat)

## Files Created/Modified
- `proto/velos/v2/detection.proto` - gRPC service contract with all messages and enums
- `crates/velos-api/Cargo.toml` - Crate manifest with tonic, prost, tokio dependencies
- `crates/velos-api/build.rs` - tonic-prost-build proto compilation
- `crates/velos-api/src/lib.rs` - Module declarations and proto type re-exports
- `crates/velos-api/src/error.rs` - ApiError enum with tonic::Status conversion (4 tests)
- `crates/velos-api/src/bridge.rs` - ApiBridge and ApiCommand types (5 tests)
- `crates/velos-api/src/detection.rs` - Stub module for Plan 02
- `crates/velos-api/src/camera.rs` - Stub module for Plan 02
- `crates/velos-api/src/aggregator.rs` - Stub module for Plan 03
- `crates/velos-api/src/calibration.rs` - Stub module for Plan 03
- `Cargo.toml` - Added velos-api to workspace, tonic/prost/tokio to workspace deps

## Decisions Made
- Used `tonic-prost-build` (separate crate) instead of `tonic-build::compile_protos` which was removed in tonic 0.14 API split
- Bounded mpsc channel with capacity 256 and drain(budget) method to cap per-frame command processing in simulation loop
- Oneshot reply channel embedded in RegisterCamera ApiCommand for synchronous request-response over async bridge
- Added `tonic-prost` runtime crate as dependency (required by generated code for ProstCodec)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] tonic-build API changed in 0.14**
- **Found during:** Task 1 (proto compilation)
- **Issue:** `tonic_build::compile_protos()` no longer exists in tonic-build 0.14; prost integration moved to `tonic-prost-build` crate
- **Fix:** Added `tonic-prost-build` and `tonic-prost` to workspace deps, updated build.rs to use `tonic_prost_build::configure().compile_protos()`
- **Files modified:** Cargo.toml, crates/velos-api/Cargo.toml, crates/velos-api/build.rs
- **Verification:** `cargo check -p velos-api` passes
- **Committed in:** 5336d7e (Task 1 commit)

**2. [Rule 3 - Blocking] protoc not installed**
- **Found during:** Task 1 (proto compilation)
- **Issue:** prost-build requires `protoc` binary; not present on system
- **Fix:** Installed protobuf via `brew install protobuf`
- **Files modified:** None (system tool)
- **Verification:** `cargo check -p velos-api` passes
- **Committed in:** 5336d7e (Task 1 commit)

---

**Total deviations:** 2 auto-fixed (2 blocking)
**Impact on plan:** Both fixes necessary for proto compilation. No scope creep.

## Issues Encountered
None beyond the deviations documented above.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Proto types generated and re-exported, ready for DetectionService trait implementation (Plan 02)
- ApiBridge and ApiCommand types ready for gRPC handler integration (Plan 02)
- Error types ready for use in gRPC service error handling (Plan 02)
- Stub modules in place for camera, detection, aggregator, calibration (Plans 02-03)

---
*Phase: 17-detection-ingestion-demand-calibration*
*Completed: 2026-03-10*
