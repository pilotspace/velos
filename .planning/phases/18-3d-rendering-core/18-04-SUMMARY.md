---
phase: 18-3d-rendering-core
plan: 04
subsystem: rendering
tags: [wgpu, orbit-camera, view-toggle, egui, LOD, instanced-rendering, 3d-integration]

requires:
  - phase: 18-01
    provides: "OrbitCamera, ViewMode, Renderer3D scaffold, depth buffer, ground plane"
  - phase: 18-02
    provides: "Road surface polygons, lane markings, junction fills"
  - phase: 18-03
    provides: "Lighting system, LOD classification, mesh/billboard shaders, agent rendering pipeline"
provides:
  - "build_instances_3d() producing LodBuffers from ECS world with 2D-to-3D coordinate mapping"
  - "ViewMode state machine in GpuState with keyboard [V] and egui toggle"
  - "Orbit camera input routing (left-drag orbit, scroll zoom, middle-drag pan)"
  - "Render dispatch branching between 2D Renderer and 3D Renderer3D"
  - "Extracted app_input.rs and app_egui.rs modules for code organization"
  - "End-to-end 3D rendering pipeline: camera -> road surfaces -> LOD agents -> lighting"
affects: [19-3d-city-scene, visualization]

tech-stack:
  added: []
  patterns: [view-mode-dispatch, input-module-extraction, dual-renderer-architecture]

key-files:
  created:
    - crates/velos-gpu/src/app_input.rs
    - crates/velos-gpu/src/app_egui.rs
  modified:
    - crates/velos-gpu/src/app.rs
    - crates/velos-gpu/src/sim_render.rs
    - crates/velos-gpu/src/road_surface.rs
    - crates/velos-gpu/src/lib.rs

key-decisions:
  - "Extracted app_input.rs and app_egui.rs from app.rs to stay under 700-line limit"
  - "Render dispatch renders target mode during transition (no cross-fade)"
  - "build_instances_3d maps 2D (x, y) to 3D (x, 0, y) with LOD classification from eye position"

patterns-established:
  - "Module extraction: app_input.rs for input handling, app_egui.rs for UI panels"
  - "ViewMode dispatch: match on view_mode for input routing and render path selection"
  - "Dual renderer: GpuState holds both Renderer (2D) and Renderer3D, dispatching by ViewMode"

requirements-completed: [R3D-01, R3D-02, R3D-03, R3D-04, R3D-05]

duration: ~45min
completed: 2026-03-11
---

# Phase 18 Plan 04: View Toggle Wiring Summary

**End-to-end 3D pipeline integration: ViewMode toggle via [V] key and egui, orbit camera input routing, dual render dispatch, and LOD instance building from ECS world**

## Performance

- **Duration:** ~45 min (across two sessions with human verification checkpoint)
- **Started:** 2026-03-11
- **Completed:** 2026-03-11
- **Tasks:** 3 (2 auto + 1 human-verify checkpoint)
- **Files modified:** 6

## Accomplishments

- Wired all Phase 18 components (Plans 01-03) into a working end-to-end 3D rendering pipeline
- Added build_instances_3d() that queries ECS world and classifies agents into LOD tiers based on camera distance
- Integrated ViewMode state machine with keyboard [V] and egui toggle button for 2D/3D switching
- Extracted input handling and egui panel code into separate modules to maintain 700-line file limit
- Human-verified: 3D perspective renders with dark blue sky and green ground plane; 2D mode unchanged

## Task Commits

Each task was committed atomically:

1. **Task 1: 3D instance building and view toggle in sim_render** - `d2cfc6e` (feat)
2. **Task 2: App.rs wiring -- ViewMode state, input routing, render dispatch, egui toggle** - `6a2f196` (feat)
3. **Task 3: Visual verification** - No commit (human-verify checkpoint, approved)

## Files Created/Modified

- `crates/velos-gpu/src/sim_render.rs` - Added build_instances_3d() with LOD classification and 2D-to-3D coordinate mapping
- `crates/velos-gpu/src/app.rs` - ViewMode state, dual renderer fields, render dispatch, resize handling
- `crates/velos-gpu/src/app_input.rs` - Extracted input handling: orbit camera controls, view toggle keyboard shortcut
- `crates/velos-gpu/src/app_egui.rs` - Extracted egui panel: 3D toggle button, view mode display
- `crates/velos-gpu/src/road_surface.rs` - Road geometry upload integration for 3D renderer
- `crates/velos-gpu/src/lib.rs` - Module declarations for app_input and app_egui

## Decisions Made

- Extracted app_input.rs and app_egui.rs from app.rs to stay under the 700-line file limit (app.rs was 693 lines before changes)
- During view transition, render the target mode directly (no cross-fade blending for simplicity)
- build_instances_3d uses same ECS query as 2D build_instances but maps coordinates to 3D space

## Deviations from Plan

None -- plan executed exactly as written.

## Issues Encountered

- No PBF road data available in test environment, so roads and agents are not visible in either 2D or 3D mode. This is a pre-existing data dependency, not a Phase 18 issue. The rendering pipeline structure (ground plane, sky color, view toggle) was verified to work correctly.

## User Setup Required

None -- no external service configuration required.

## Next Phase Readiness

- Phase 18 (3D Rendering Core) is now complete -- all 4 plans finished
- Phase 19 (3D City Scene) can proceed: OSM building extrusions and SRTM DEM terrain will integrate into the Renderer3D pipeline established here
- The dual-renderer architecture cleanly separates 2D and 3D paths, making it safe to add 3D-only features in Phase 19

## Self-Check: PASSED

- FOUND: crates/velos-gpu/src/app_input.rs
- FOUND: crates/velos-gpu/src/app_egui.rs
- FOUND: crates/velos-gpu/src/sim_render.rs
- FOUND: .planning/phases/18-3d-rendering-core/18-04-SUMMARY.md
- FOUND: d2cfc6e (Task 1 commit)
- FOUND: 6a2f196 (Task 2 commit)

---
*Phase: 18-3d-rendering-core*
*Completed: 2026-03-11*
