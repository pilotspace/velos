---
phase: 16-intersection-sublane-model
plan: 04
subsystem: gpu-rendering
tags: [wgpu, egui, bezier, guide-lines, vehicle-coloring, debug-overlay, wgsl-shader]

# Dependency graph
requires:
  - "BezierTurn with position/tangent evaluation (Plan 01)"
  - "JunctionTraversal ECS component and junction_data on SimWorld (Plan 02)"
  - "MapTileRenderer background rendering integration (Plan 03)"
provides:
  - "Vehicle-type coloring (motorbike=orange, car=blue, bus=per-route, truck=red)"
  - "Bezier tangent heading for junction-traversing agents"
  - "Dashed guide line rendering through junctions (toggleable)"
  - "Conflict crossing point debug overlay as red dots (toggleable)"
  - "egui toggle checkboxes for guide lines and debug overlay"
  - "Instance buffer capacity increased to 300K for POC scale"
affects: [visualization, debug-workflow, renderer]

# Tech tracking
tech-stack:
  added: []
  patterns: [quad-strip-guide-lines, discard-based-dash-pattern, bezier-tangent-heading]

key-files:
  created:
    - crates/velos-gpu/shaders/guide_line.wgsl
  modified:
    - crates/velos-gpu/src/sim_render.rs
    - crates/velos-gpu/src/renderer.rs
    - crates/velos-gpu/src/app.rs

key-decisions:
  - "Vehicle-type coloring replaces car-following-model coloring for clearer visual identity"
  - "Guide lines rendered as quad strips (0.5m width) not native GPU lines for cross-GPU consistency"
  - "Dash pattern via fragment discard (3m dash, 2m gap, 5m period) in WGSL shader"
  - "Debug overlay conflict dots rendered as 2m red quads at crossing point midpoints"
  - "Instance buffer capacity increased from 8192 to 300K to support POC agent count"
  - "Extracted vehicle_type_color() and heading_from_tangent() as testable free functions"

patterns-established:
  - "Quad strip generation from Bezier curves: 20 samples, perpendicular normal offset"
  - "Fragment discard dash pattern: fract(dist / period) > duty_cycle"
  - "Overlay toggle pattern: egui checkbox -> bool flag -> render_frame conditional draw"

requirements-completed: [ISL-02, MAP-02]

# Metrics
duration: 5min
completed: 2026-03-09
---

# Phase 16 Plan 04: Sublane Visualization Summary

**Vehicle-type coloring, Bezier tangent heading, dashed guide line shader with quad strips, conflict debug overlay, and egui toggle controls for junction visualization**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-09T12:44:46Z
- **Completed:** 2026-03-09T12:50:00Z
- **Tasks:** 2 of 3 (Task 3 is human-verify checkpoint)
- **Files modified:** 4

## Accomplishments
- Vehicle-type coloring matches spec: motorbike=orange, car=blue, bus=per-route, truck=red, emergency=white, bicycle=yellow, pedestrian=grey
- Bezier tangent heading for junction agents: heading computed from B'(t) = 2(1-t)(P1-P0) + 2t(P2-P1) via atan2
- Dashed guide line WGSL shader with discard-based dash pattern (3m/2m/5m period)
- Guide line quad strips generated from all junction Bezier turns (20 samples, 0.5m width)
- Conflict debug overlay renders red dots at crossing point midpoints
- egui "Show Guide Lines" and "Show Conflict Debug" checkboxes in Debug Overlays section
- Instance buffer capacity increased from 8192 to 300K for POC scale
- 10 unit tests for vehicle_type_color and heading_from_tangent functions

## Task Commits

Each task was committed atomically:

1. **Task 1: Vehicle-type coloring and Bezier tangent heading** - `52d8b6a` (feat)
2. **Task 2: Dashed guide lines and conflict debug overlay** - `1b634e9` (feat)

## Files Created/Modified
- `crates/velos-gpu/shaders/guide_line.wgsl` - Dashed line vertex/fragment shader with camera uniform and discard pattern
- `crates/velos-gpu/src/sim_render.rs` - Vehicle-type coloring, Bezier tangent heading, extracted vehicle_type_color() and heading_from_tangent() with 10 tests
- `crates/velos-gpu/src/renderer.rs` - Guide line pipeline, update_guide_lines(), update_debug_overlay(), render_frame overlay flags, instance capacity 300K
- `crates/velos-gpu/src/app.rs` - Guide line/debug overlay initialization, egui toggle checkboxes, updated vehicle legend colors

## Decisions Made
- Replaced car-following-model-based coloring with vehicle-type coloring for clearer visual identity at intersections
- Guide lines as quad strips (not native GPU lines) for consistent width across GPU vendors
- WGSL discard-based dash pattern avoids geometry complexity of actual dashed lines
- Debug overlay reuses guide_line_pipeline (same vertex format) with solid red color and line_dist=0
- Extracted pure functions for testability: vehicle_type_color and heading_from_tangent are standalone

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed clippy collapsible-if warnings**
- **Found during:** Task 1 and Task 2 (clippy quality gate)
- **Issue:** Nested if-let blocks should be collapsed per Rust 2024 edition
- **Fix:** Combined nested if-let into single condition with `&&` chains
- **Files modified:** crates/velos-gpu/src/sim_render.rs, crates/velos-gpu/src/renderer.rs
- **Verification:** cargo clippy -D warnings passes clean
- **Committed in:** 52d8b6a, 1b634e9

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Style fix only. No scope creep.

## Issues Encountered
None - plan executed smoothly.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Task 3 (human-verify checkpoint) remains: visual verification of complete intersection sublane model
- All rendering infrastructure is in place: vehicle colors, tangent heading, guide lines, debug overlay
- Map tiles from Plan 03 provide background context
- Junction traversal from Plan 02 provides Bezier curve navigation

## Self-Check: PASSED

- [x] crates/velos-gpu/shaders/guide_line.wgsl exists
- [x] crates/velos-gpu/src/sim_render.rs exists (modified)
- [x] crates/velos-gpu/src/renderer.rs exists (modified)
- [x] crates/velos-gpu/src/app.rs exists (modified)
- [x] 16-04-SUMMARY.md exists
- [x] Commit 52d8b6a exists (Task 1)
- [x] Commit 1b634e9 exists (Task 2)
- [x] Instance buffer capacity = 300K
- [x] Vehicle-type colors: motorbike=orange, car=blue, truck=red
- [x] Bezier tangent heading in junction_heading()
- [x] Guide line WGSL shader with discard dash pattern
- [x] egui checkboxes for guide lines and debug overlay
- [x] NaN/Inf guard preserved in build_instances (Bug 7)

---
*Phase: 16-intersection-sublane-model*
*Completed: 2026-03-09*
