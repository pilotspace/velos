---
phase: 16-intersection-sublane-model
plan: 03
subsystem: gpu-rendering
tags: [pmtiles, mvt, earcut, wgpu, map-tiles, triangulation, coordinate-reprojection]

# Dependency graph
requires:
  - phase: none
    provides: standalone map tile rendering (parallel to junction geometry work)
provides:
  - MapTileRenderer with full PMTiles->MVT->earcut->wgpu pipeline
  - TileVertex type for map tile GPU geometry
  - Background thread tile decode with main-thread GPU buffer creation
  - Visible tile calculation from Camera2D viewport
  - map_tile.wgsl shader for colored polygon rendering
affects: [16-04-integration, visualization, renderer]

# Tech tracking
tech-stack:
  added: [pmtiles2, mvt-reader, earcutr, flate2, lru, geo-types]
  patterns: [background-thread-decode, main-thread-gpu-upload, lru-tile-cache, coordinate-reprojection-pipeline]

key-files:
  created:
    - crates/velos-gpu/src/map_tiles.rs
    - crates/velos-gpu/shaders/map_tile.wgsl
  modified:
    - crates/velos-gpu/src/renderer.rs
    - crates/velos-gpu/src/lib.rs
    - crates/velos-gpu/Cargo.toml

key-decisions:
  - "128-tile LRU cache with GPU buffer eviction for memory management"
  - "Alpha blending for map tile layer to allow translucent features"
  - "Map tiles clear the screen; agent pass uses LoadOp::Load to render on top"
  - "Camera zoom to tile zoom: <0.5->z14, 0.5-2.0->z15, >=2.0->z16"
  - "Skip label rendering; map polygons provide sufficient spatial context"

patterns-established:
  - "Background thread decode + main thread GPU upload via mpsc channel"
  - "TileContext struct to bundle tile coordinate parameters (clippy compliance)"
  - "Graceful PMTiles absence: Option path, inert renderer when file missing"

requirements-completed: [MAP-01]

# Metrics
duration: 7min
completed: 2026-03-09
---

# Phase 16 Plan 03: Map Tile Rendering Summary

**PMTiles->MVT->earcut triangulation->wgpu render pipeline with background decode thread, LRU cache, and coordinate reprojection from Web Mercator to VELOS local metres**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-09T12:20:59Z
- **Completed:** 2026-03-09T12:27:36Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Full map tile rendering pipeline: PMTiles file read, MVT protobuf decode, earcut polygon triangulation, wgpu vertex buffer upload and render
- Background thread handles all file I/O and decode work; main thread creates GPU buffers (wgpu thread safety)
- Coordinate reprojection: MVT tile-local coords -> Web Mercator lon/lat -> VELOS local metres via EquirectangularProjection
- LRU cache (128 tiles) with automatic eviction including GPU buffer cleanup
- Renderer integration: map tiles render as background layer before road lines and agents
- 9 unit tests covering coordinate reprojection, triangulation, visible tile calculation, zoom mapping, gzip decompression

## Task Commits

Each task was committed atomically:

1. **Task 1: Add dependencies and WGSL shader** - `cdf71e5` (chore) - pre-existing commit
2. **Task 2: MapTileRenderer implementation** - `33ff1c6` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/map_tiles.rs` - Full MapTileRenderer: PMTiles reader, MVT decoder, earcut triangulation, LRU cache, wgpu render pipeline, background decode thread
- `crates/velos-gpu/shaders/map_tile.wgsl` - Colored polygon vertex/fragment shader sharing camera uniform with agent shader
- `crates/velos-gpu/src/renderer.rs` - Added map_tiles field, init/set/update methods, render integration before agents
- `crates/velos-gpu/src/lib.rs` - Added map_tiles module and public exports
- `crates/velos-gpu/Cargo.toml` - Added geo-types dependency (required for MVT geometry types)

## Decisions Made
- Used alpha blending (ALPHA_BLENDING) for map tile pipeline to support translucent features
- Map tile render pass clears the screen; agent render pass uses LoadOp::Load when tiles are active
- Exposed camera_bind_group_layout from Renderer so MapTileRenderer shares the same camera uniform
- Layer color mapping: building=dark grey, water=dark blue, road=grey, park=dark green, landuse=subtle dark
- Line features rendered as thin quads (1m width) rather than native GPU lines for consistent width

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Added geo-types dependency**
- **Found during:** Task 2 (MapTileRenderer implementation)
- **Issue:** mvt-reader returns geo_types::Geometry but geo-types was not a direct dependency of velos-gpu
- **Fix:** Added `geo-types = "0.7"` to Cargo.toml
- **Files modified:** crates/velos-gpu/Cargo.toml
- **Verification:** cargo check passes
- **Committed in:** 33ff1c6

**2. [Rule 1 - Bug] Fixed clippy too-many-arguments warning**
- **Found during:** Task 2 (clippy quality gate)
- **Issue:** process_polygon and process_linestring had 8 arguments (clippy limit is 7)
- **Fix:** Introduced TileContext struct to bundle tile coordinate parameters
- **Files modified:** crates/velos-gpu/src/map_tiles.rs
- **Verification:** cargo clippy -D warnings passes clean
- **Committed in:** 33ff1c6

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both auto-fixes necessary for compilation and code quality. No scope creep.

## Issues Encountered
None - plan executed smoothly.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- MapTileRenderer ready for integration with simulation app
- Requires a .pmtiles file at runtime (gracefully disabled if absent)
- Plan 04 can wire map tiles into VelosApp startup

---
*Phase: 16-intersection-sublane-model*
*Completed: 2026-03-09*
