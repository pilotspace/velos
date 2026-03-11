---
phase: 18-3d-rendering-core
plan: 04
subsystem: rendering
tags: [wgpu, orbit-camera, view-toggle, egui, lod, reverse-z, trackpad]

# Dependency graph
requires:
  - phase: 18-01
    provides: "OrbitCamera, Renderer3D shell, ground plane, depth texture"
  - phase: 18-02
    provides: "Road surface polygon geometry, lane markings, junction fills"
  - phase: 18-03
    provides: "Mesh/billboard/dot LOD pipeline, lighting uniforms, WGSL shaders"
provides:
  - "End-to-end 2D/3D view toggle via [V] key and egui button"
  - "build_instances_3d() ECS query producing LodBuffers from simulation world"
  - "Orbit camera input routing (left-drag orbit, scroll zoom, middle/right-drag pan)"
  - "Render dispatch branching (TopDown2D vs Perspective3D)"
  - "Reverse-Z depth buffer for z-fighting-free large-scale rendering"
  - "Mac trackpad pan support (right-drag + horizontal scroll)"
affects: [19-3d-city-scene, velos-gpu]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Reverse-Z infinite projection for large-scale 3D scenes"
    - "Module extraction (app_input.rs, app_egui.rs) to keep app.rs under 700 lines"
    - "ViewMode enum with transition state for animated 2D/3D switching"

key-files:
  created:
    - crates/velos-gpu/src/app_input.rs
    - crates/velos-gpu/src/app_egui.rs
  modified:
    - crates/velos-gpu/src/app.rs
    - crates/velos-gpu/src/sim_render.rs
    - crates/velos-gpu/src/orbit_camera.rs
    - crates/velos-gpu/src/renderer3d.rs
    - crates/velos-gpu/src/road_surface.rs
    - crates/velos-gpu/src/lib.rs

key-decisions:
  - "Extracted app_input.rs and app_egui.rs from app.rs to stay under 700-line limit"
  - "Render dispatch renders target mode during transition (no cross-fade)"
  - "build_instances_3d maps 2D (x, y) to 3D (x, 0, y) with LOD classification from eye position"
  - "Reverse-Z depth buffer with infinite far plane eliminates z-fighting on overlapping road layers"
  - "Right-drag pan added for Mac trackpad two-finger click+drag ergonomics"
  - "Y separation between road layers increased 10x (junction 0.05, marking 0.1) for visual clarity"

patterns-established:
  - "Module extraction: app_input.rs for input handling, app_egui.rs for UI panels"
  - "ViewMode dispatch: match on view_mode for input routing and render path selection"
  - "Dual renderer: GpuState holds both Renderer (2D) and Renderer3D, dispatching by ViewMode"
  - "Reverse-Z projection: GreaterEqual compare, clear depth to 0.0, infinite far plane"

requirements-completed: [R3D-01, R3D-02, R3D-03, R3D-04, R3D-05]

duration: 45min
completed: 2026-03-11
---

# Phase 18 Plan 04: View Toggle Wiring Summary

**End-to-end 2D/3D view toggle with orbit camera, LOD render dispatch, reverse-Z depth buffer, and Mac trackpad pan support**

## Performance

- **Duration:** ~45 min (across sessions including post-checkpoint fixes)
- **Started:** 2026-03-11T03:00:00Z
- **Completed:** 2026-03-11T04:15:00Z
- **Tasks:** 3 (2 auto + 1 human-verify checkpoint)
- **Files modified:** 8

## Accomplishments

- Wired all Phase 18 components (Plans 01-03) into a working end-to-end 3D rendering pipeline
- Added build_instances_3d() that queries ECS world and classifies agents into LOD tiers based on camera distance
- Integrated ViewMode state machine with keyboard [V] and egui toggle button for 2D/3D switching
- Adopted reverse-Z depth buffer with infinite far plane, eliminating z-fighting between ground/road/marking layers
- Added Mac trackpad pan support (right-drag pan + horizontal scroll pan)
- Extracted input handling and egui panel code into separate modules to maintain 700-line file limit

## Task Commits

Each task was committed atomically:

1. **Task 1: 3D instance building and view toggle in sim_render** - `d2cfc6e` (feat)
2. **Task 2: App.rs wiring -- ViewMode state, input routing, render dispatch, egui toggle** - `6a2f196` (feat)
3. **Task 3: Visual verification + post-checkpoint fixes** - `b14aeec` (fix)

## Files Created/Modified

- `crates/velos-gpu/src/sim_render.rs` - Added build_instances_3d() with LOD classification and 2D-to-3D coordinate mapping
- `crates/velos-gpu/src/app.rs` - ViewMode state, dual renderer fields, render dispatch, right_pressed field
- `crates/velos-gpu/src/app_input.rs` - 3D orbit camera input handling with trackpad pan and horizontal scroll
- `crates/velos-gpu/src/app_egui.rs` - Extracted egui panel: 3D toggle button, view mode display
- `crates/velos-gpu/src/orbit_camera.rs` - Reverse-Z infinite projection, near plane 1.0
- `crates/velos-gpu/src/renderer3d.rs` - GreaterEqual depth compare, clear to 0.0, ground depth bias, ground Y -0.5
- `crates/velos-gpu/src/road_surface.rs` - Increased Y layer separation (junction 0.05, marking 0.1)
- `crates/velos-gpu/src/lib.rs` - Module declarations for app_input and app_egui

## Decisions Made

- Extracted app_input.rs and app_egui.rs from app.rs to stay under the 700-line file limit
- During view transition, render the target mode directly (no cross-fade blending)
- build_instances_3d uses same ECS query as 2D build_instances but maps coordinates to 3D space
- Reverse-Z depth buffer with infinite far plane for z-fighting elimination at all distances
- Right-drag pan as Mac trackpad alternative to middle-drag (two-finger click+drag)
- Y separation between road layers increased 10x for reliable depth ordering at oblique angles

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Reverse-Z depth buffer to fix z-fighting flickering**
- **Found during:** Task 3 (visual verification)
- **Issue:** Standard depth buffer caused visible z-fighting between ground plane, road surfaces, and lane markings at distance with oblique camera angles
- **Fix:** Switched to reverse-Z infinite projection (GreaterEqual compare, clear to 0.0, `perspective_infinite_reverse_rh`), near plane 0.1 to 1.0, depth bias on ground pipeline
- **Files modified:** orbit_camera.rs, renderer3d.rs
- **Verification:** Visual inspection confirmed no flickering at any zoom level
- **Committed in:** b14aeec

**2. [Rule 1 - Bug] Increased Y separation between road geometry layers**
- **Found during:** Task 3 (visual verification)
- **Issue:** Small Y offsets (junction 0.005, marking 0.01) were insufficient to prevent z-fighting at oblique angles
- **Fix:** Ground Y -0.01 to -0.5, junction Y 0.005 to 0.05, marking Y 0.01 to 0.1
- **Files modified:** renderer3d.rs, road_surface.rs
- **Verification:** Road layers render cleanly without flickering
- **Committed in:** b14aeec

**3. [Rule 2 - Missing Critical] Mac trackpad pan support**
- **Found during:** Task 3 (visual verification)
- **Issue:** Middle-drag pan was impossible on Mac trackpads (no middle button); horizontal scroll was ignored
- **Fix:** Added right-drag as pan alternative, added horizontal trackpad scroll for left/right panning, split scroll delta handling for mouse (LineDelta) vs trackpad (PixelDelta)
- **Files modified:** app_input.rs, app.rs
- **Verification:** Pan works with both mouse middle-drag and trackpad right-drag + horizontal scroll
- **Committed in:** b14aeec

---

**Total deviations:** 3 auto-fixed (2 bug fixes, 1 missing critical)
**Impact on plan:** All fixes necessary for usability on target platform (Mac Metal). No scope creep.

## Issues Encountered

None beyond the deviations documented above.

## User Setup Required

None -- no external service configuration required.

## Next Phase Readiness

- Phase 18 (3D Rendering Core) is fully complete -- all 4 plans finished
- Phase 19 (3D City Scene) can proceed: OSM building extrusions and SRTM DEM terrain will integrate into the Renderer3D pipeline
- Reverse-Z depth buffer pattern established for all future 3D rendering work
- The dual-renderer architecture cleanly separates 2D and 3D paths

## Self-Check: PASSED

- FOUND: crates/velos-gpu/src/app_input.rs
- FOUND: crates/velos-gpu/src/app_egui.rs
- FOUND: crates/velos-gpu/src/sim_render.rs
- FOUND: crates/velos-gpu/src/orbit_camera.rs
- FOUND: crates/velos-gpu/src/renderer3d.rs
- FOUND: crates/velos-gpu/src/road_surface.rs
- FOUND: .planning/phases/18-3d-rendering-core/18-04-SUMMARY.md
- FOUND: d2cfc6e (Task 1 commit)
- FOUND: 6a2f196 (Task 2 commit)
- FOUND: b14aeec (Task 3 fix commit)

---
*Phase: 18-3d-rendering-core*
*Completed: 2026-03-11*
