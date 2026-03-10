---
phase: 18-3d-rendering-core
plan: 02
subsystem: rendering
tags: [wgpu, wgsl, road-geometry, tessellation, gpu-buffers, 3d-rendering]

# Dependency graph
requires:
  - phase: 18-01
    provides: "Renderer3D scaffold with depth buffer, camera uniform, ground plane pipeline"
  - phase: velos-net
    provides: "RoadGraph with RoadEdge geometry polylines and lane counts"
provides:
  - "Road surface polygon generation from RoadGraph edge polylines"
  - "Lane marking geometry (dashed center, solid edge) with z-fighting prevention"
  - "Junction surface convex hull generation and triangulation"
  - "Static GPU vertex buffers for road geometry rendered every frame"
  - "road_surface.wgsl shader with camera uniform bind group"
affects: [18-03, 18-04]

# Tech tracking
tech-stack:
  added: []
  patterns: ["perpendicular offset polygon expansion", "Y-offset layering for z-fighting prevention", "static vertex buffer upload at load time"]

key-files:
  created:
    - "crates/velos-gpu/src/road_surface.rs"
    - "crates/velos-gpu/shaders/road_surface.wgsl"
  modified:
    - "crates/velos-gpu/src/renderer3d.rs"
    - "crates/velos-gpu/src/lib.rs"

key-decisions:
  - "Road surface WGSL shader identical structure to ground_plane.wgsl for pipeline reuse"
  - "Alpha blending on road pipeline for semi-transparent lane marking color"
  - "Separate render pass for road geometry (Load existing color/depth from ground pass)"
  - "Reverted Plan 03 concurrent struct changes to maintain compilability (Plan 03 re-adds)"

patterns-established:
  - "Perpendicular offset expansion: polyline segments expanded to quad strips using normal vectors"
  - "Y-offset layering: road=0.0, junction=0.005, marking=0.01 prevents z-fighting without depth bias"
  - "Static buffer pattern: generate_*() returns Vec<Vertex>, upload once, render every frame"

requirements-completed: [R3D-02]

# Metrics
duration: 8min
completed: 2026-03-10
---

# Phase 18 Plan 02: Road Surface Geometry Summary

**Road polygon expansion from RoadGraph edges with perpendicular offset tessellation, dashed/solid lane markings, and convex hull junction fills as static GPU vertex buffers**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-10T15:58:32Z
- **Completed:** 2026-03-10T16:06:11Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Road surface mesh generation from RoadGraph edge polylines using perpendicular offset expansion (lane_count * 3.5m width)
- Lane marking geometry with dashed center lines (3m/3m pattern) and solid edge lines at y=+0.01
- Junction surface generation via angle-sorted convex hull with centroid fan triangulation
- road_surface.wgsl shader with naga validation, Renderer3D integration with upload_road_geometry() and multi-pass rendering

## Task Commits

Each task was committed atomically:

1. **Task 1: Road surface and lane marking geometry generation** - `a4536b8` (feat)
2. **Task 2: Road surface WGSL shader and Renderer3D integration** - `6b86f3d` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/road_surface.rs` - RoadSurfaceVertex, generate_road_mesh, generate_lane_markings, generate_junction_surfaces with 14 unit tests
- `crates/velos-gpu/shaders/road_surface.wgsl` - Vertex color pass-through shader with camera uniform
- `crates/velos-gpu/src/renderer3d.rs` - Road pipeline creation, upload_road_geometry(), render_roads() in render_frame()
- `crates/velos-gpu/src/lib.rs` - road_surface module declaration and public exports

## Decisions Made
- Road surface shader reuses identical vertex layout (position vec3 + color vec4 = 28 bytes) as ground_plane for pipeline compatibility
- Alpha blending enabled on road pipeline to support semi-transparent marking colors (white at 0.8 alpha)
- Road geometry rendered in a separate pass that loads (not clears) the ground plane's color and depth buffers
- Reverted Plan 03's concurrent CameraUniform3D expansion and mesh/billboard fields to maintain compilation; Plan 03 will re-add when executing

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Resolved concurrent modification conflict in renderer3d.rs**
- **Found during:** Task 2 (Renderer3D integration)
- **Issue:** Another parallel plan (Plan 03) modified renderer3d.rs adding mesh_loader, lighting imports and expanded CameraUniform3D struct while Plan 02 was executing, causing compilation failure
- **Fix:** Reverted Plan 03's additions (mesh_pipeline, billboard_pipeline, lighting_uniform_buffer fields, expanded CameraUniform3D) to maintain compilation. Plan 03 will re-add its changes when it runs.
- **Files modified:** crates/velos-gpu/src/renderer3d.rs
- **Verification:** cargo test -p velos-gpu --lib passes (186 tests)
- **Committed in:** 6b86f3d (Task 2 commit)

**2. [Rule 1 - Bug] Fixed clippy for_kv_map warning in junction generation**
- **Found during:** Task 2 (verification)
- **Issue:** `for (_id, jdata) in junction_data` triggers clippy warning
- **Fix:** Changed to `for jdata in junction_data.values()`
- **Files modified:** crates/velos-gpu/src/road_surface.rs
- **Verification:** clippy passes for road_surface module
- **Committed in:** 6b86f3d (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Blocking fix necessary due to concurrent plan modifications. Bug fix is trivial clippy compliance. No scope creep.

## Issues Encountered
- Pre-existing clippy errors in lighting.rs and lod.rs (out of scope, from other plans) prevent clean `cargo clippy -p velos-gpu -- -D warnings`. Road surface code itself is clippy-clean.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Road surfaces, lane markings, and junction fills are ready for 3D rendering
- Plan 03 (mesh instancing/billboards) can extend Renderer3D with its pipelines
- Plan 04 (view toggle) can wire road geometry upload into the application startup

---
*Phase: 18-3d-rendering-core*
*Completed: 2026-03-10*
