---
phase: 18-3d-rendering-core
verified: 2026-03-11T14:30:00Z
status: human_needed
score: 5/5 must-haves verified
re_verification:
  previous_status: human_needed
  previous_score: 5/5
  gaps_closed: []
  gaps_remaining: []
  regressions: []
human_verification:
  - test: "Run cargo run -p velos-gpu, press [V] to toggle 3D, verify ground plane, roads, agents render"
    expected: "Green ground plane, grey road surfaces with white lane markings, LOD agents visible"
    why_human: "Visual rendering correctness cannot be verified programmatically"
  - test: "Orbit camera: left-drag to rotate, scroll to zoom, middle-drag to pan"
    expected: "Smooth orbit rotation, zoom in/out, pan focus point, pitch never goes underground"
    why_human: "Interactive input behavior requires human testing"
  - test: "Let simulation run for a few sim-minutes, observe lighting tint shift"
    expected: "Subtle color temperature changes following day/night cycle"
    why_human: "Time-of-day lighting changes are visual and gradual"
  - test: "Press [V] again to return to 2D, verify 2D mode is unchanged"
    expected: "2D mode renders identically to pre-Phase-18 behavior"
    why_human: "Visual regression comparison requires human judgment"
  - test: "Toggle 2D->3D->2D, check camera preserves position"
    expected: "Same area visible after round-trip toggle"
    why_human: "Coordinate mapping correctness requires visual confirmation"
---

# Phase 18: 3D Rendering Core Verification Report

**Phase Goal:** User can view the running simulation in a 3D perspective with depth-correct rendering, LOD agents, road surfaces, and time-of-day lighting
**Verified:** 2026-03-11T14:30:00Z
**Status:** human_needed (all automated checks pass; visual verification required)
**Re-verification:** Yes -- confirms previous verification findings with updated line counts and evidence

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User sees simulation from 3D perspective with correct depth ordering | VERIFIED | OrbitCamera produces perspective view-proj matrix (12 tests), Renderer3D uses Depth32Float with DepthStencilState on all 4 pipelines (lines 226, 289, 369, 452), render_frame draws ground -> roads -> meshes -> billboards with depth attachment |
| 2 | Roads render as 3D surface polygons with visible lane markings | VERIFIED | generate_road_mesh() expands RoadGraph edges to polygons (14 tests), generate_lane_markings() creates dashed center + solid edge lines, road_surface.wgsl loaded via include_wgsl! at line 316, upload_road_geometry at renderer3d.rs:683 calls both generators |
| 3 | Agents render as 3D meshes (close), billboards (mid), dots (far) via GPU instancing | VERIFIED | LodTier enum in lod.rs (10 tests), classify_lod() with hysteresis, MeshSet loads .glb or procedural fallback (6 tests), mesh_3d.wgsl + billboard_3d.wgsl naga-validated, build_instances_3d() at sim_render.rs:561 classifies agents via classify_lod import at line 17 |
| 4 | User can toggle between 2D and 3D with single click, preserving camera position | VERIFIED | [V] key handler at app.rs:557, egui button at app_egui.rs:50, OrbitCamera::from_camera_2d() at orbit_camera.rs:177, toggle_view_mode() at app_input.rs:121, ViewTransition animates over 0.5s (5 tests) |
| 5 | Scene lighting changes with simulation time-of-day | VERIFIED | compute_lighting() in lighting.rs (6 tests), LightingUniform imported at renderer3d.rs:14, called at line 594, mesh_3d.wgsl applies diffuse+ambient, billboard_3d.wgsl applies ambient tint |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/velos-gpu/src/orbit_camera.rs` | OrbitCamera, ViewMode, ViewTransition, instance types | VERIFIED | 420 lines, 12 tests, exports: OrbitCamera, ViewMode, ViewTransition, MeshInstance3D, BillboardInstance3D |
| `crates/velos-gpu/src/renderer3d.rs` | Renderer3D with depth buffer, ground, road, mesh, billboard pipelines | VERIFIED | 942 lines (exceeds 700-line limit), 7 tests including 4 naga shader validations, all 4 shaders wired via include_wgsl! |
| `crates/velos-gpu/src/road_surface.rs` | Road polygon generation from RoadGraph | VERIFIED | 692 lines, exports: RoadSurfaceVertex, generate_road_mesh, generate_lane_markings, generate_junction_surfaces, 14 tests |
| `crates/velos-gpu/src/lighting.rs` | Time-of-day keyframes and interpolation | VERIFIED | 251 lines, exports: LightingUniform, compute_lighting (no LightingSystem wrapper -- functional without it), 6 tests |
| `crates/velos-gpu/src/lod.rs` | LOD classification with hysteresis | VERIFIED | 166 lines, exports: LodTier, classify_lod, 10 tests |
| `crates/velos-gpu/src/mesh_loader.rs` | glTF loading + procedural fallback | VERIFIED | 296 lines, exports: LoadedMesh, Vertex3D, load_glb, generate_fallback_box, MeshSet, 6 tests |
| `crates/velos-gpu/shaders/ground_plane.wgsl` | Ground plane shader | VERIFIED | 909 bytes, naga-validated in renderer3d.rs test |
| `crates/velos-gpu/shaders/road_surface.wgsl` | Road surface shader | VERIFIED | 935 bytes, naga-validated in renderer3d.rs test |
| `crates/velos-gpu/shaders/mesh_3d.wgsl` | Lit instanced mesh shader | VERIFIED | 2850 bytes, naga-validated in renderer3d.rs test |
| `crates/velos-gpu/shaders/billboard_3d.wgsl` | Camera-facing billboard shader | VERIFIED | 2762 bytes, naga-validated in renderer3d.rs test |
| `crates/velos-gpu/src/app_input.rs` | 3D input handling (orbit, zoom, pan) | VERIFIED | 197 lines, orbit/zoom/pan routing + toggle_view_mode, 5 tests |
| `crates/velos-gpu/src/app_egui.rs` | egui panel with view toggle button | VERIFIED | 170 lines, private module (mod app_egui in lib.rs), [V] toggle at line 50, used by app.rs |
| `crates/velos-gpu/src/sim_render.rs` | build_instances_3d() + LodBuffers | VERIFIED | LodBuffers at line 441, build_instances_3d at line 561, imports classify_lod from lod.rs at line 17 |
| `crates/velos-gpu/src/app.rs` | ViewMode dispatch, orbit camera routing | VERIFIED | 660 lines, ViewMode::Perspective3D branch at line 411, orbit_camera field, build_instances_3d call at line 324 |
| `crates/velos-gpu/src/lib.rs` | Module exports | VERIFIED | All new modules registered: orbit_camera, renderer3d, road_surface, lighting, lod, mesh_loader, app_input (app_egui correctly private) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| renderer3d.rs | orbit_camera.rs | view_proj_matrix() -> camera uniform | WIRED | Camera uniform buffer used by all 4 shader pipelines |
| renderer3d.rs | ground_plane.wgsl | include_wgsl! at line 236 | WIRED | Pipeline created and drawn in render_frame |
| renderer3d.rs | road_surface.wgsl | include_wgsl! at line 316 | WIRED | Pipeline created, used in render pass |
| renderer3d.rs | mesh_3d.wgsl | include_wgsl! at line 377 | WIRED | Pipeline created, instanced draw |
| renderer3d.rs | billboard_3d.wgsl | include_wgsl! at line 460 | WIRED | Pipeline created, instanced draw |
| renderer3d.rs | road_surface.rs | upload_road_geometry calls generators | WIRED | Lines 690 (generate_road_mesh), 702 (generate_lane_markings) |
| renderer3d.rs | mesh_loader.rs | MeshSet for instanced draw | WIRED | Import at line 20 |
| renderer3d.rs | lighting.rs | compute_lighting -> LightingUniform | WIRED | Import at line 14, called at line 594 |
| app.rs | renderer3d.rs | render_frame dispatch in 3D mode | WIRED | ViewMode::Perspective3D at line 411 -> renderer_3d.render_frame at line 412 |
| app.rs | orbit_camera.rs | Holds OrbitCamera, init from_camera_2d | WIRED | orbit_camera field, init at line 203, input routing via app_input |
| app.rs | sim_render.rs | build_instances_3d with eye position | WIRED | Line 324: self.sim.build_instances_3d(self.orbit_camera.eye_position()) |
| sim_render.rs | lod.rs | classify_lod in build_instances_3d | WIRED | Import at line 17, called at line 590 |
| app_input.rs | orbit_camera.rs | orbit/zoom/pan calls | WIRED | Lines 70, 75, 88, 93, 102, 106 |
| app_egui.rs | app_input.rs | toggle_view_mode on button click | WIRED | Line 51 |
| road_surface.rs | velos-net graph.rs | Reads RoadGraph for polygon gen | WIRED | generate_road_mesh/lane_markings take &RoadGraph parameter |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-----------|-------------|--------|----------|
| R3D-01 | 18-01, 18-04 | 3D perspective with depth-correct rendering | SATISFIED | OrbitCamera perspective projection + Depth32Float on all 4 pipelines + depth attachment in render pass |
| R3D-02 | 18-02 | Roads as 3D surface polygons with lane markings | SATISFIED | generate_road_mesh + generate_lane_markings + generate_junction_surfaces + road_surface.wgsl + upload_road_geometry |
| R3D-03 | 18-03, 18-04 | 3-tier LOD with GPU instancing | SATISFIED | LodTier + classify_lod + MeshInstance3D/BillboardInstance3D + instanced draw calls + build_instances_3d |
| R3D-04 | 18-01, 18-04 | Toggle between 2D and 3D views | SATISFIED | ViewMode + [V] key + egui button + from_camera_2d + ViewTransition |
| R3D-05 | 18-03 | Time-of-day lighting | SATISFIED | 4 keyframes + compute_lighting + LightingUniform GPU buffer + shader diffuse+ambient |

No orphaned requirements. R3D-06 (buildings) is correctly mapped to Phase 19 in REQUIREMENTS.md.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| renderer3d.rs | - | 942 lines (exceeds 700-line limit) | Warning | Contains 7 tests + 4 pipeline setups. Could extract pipeline builders into helpers. |
| sim_render.rs | - | 1042 lines (exceeds 700-line limit) | Warning | Pre-existing issue worsened by Phase 18 (added ~120 lines for build_instances_3d + LodBuffers). Refactor candidate. |
| lighting.rs | - | Missing LightingSystem struct (plan 03 expected it) | Info | compute_lighting() + LightingUniform sufficient. No functional impact. |
| lod.rs | - | LodBuffers in sim_render.rs not lod.rs (plan 03 expected it there) | Info | Reasonable placement -- LodBuffers handles buffer construction, not classification. |

No blocker anti-patterns. Zero TODOs, FIXMEs, placeholders, or stub implementations found across all Phase 18 files.

### Human Verification Required

### 1. Visual 3D Rendering

**Test:** Run `cargo run -p velos-gpu`, press [V] to toggle to 3D perspective
**Expected:** Green ground plane below roads, grey road surfaces with white dashed center lines and solid edge lines, junction areas in lighter grey, agents rendered at different LOD tiers (boxes up close, billboards mid-range, dots far away)
**Why human:** Visual rendering correctness (depth ordering, colors, geometry alignment) cannot be verified by unit tests

### 2. Orbit Camera Controls

**Test:** In 3D mode: left-drag to orbit, scroll to zoom, middle-drag to pan
**Expected:** Smooth orbit rotation around focus point, zoom in/out with scroll, pan moves focus point, pitch clamped above ground (5-89 degrees)
**Why human:** Interactive input handling requires human interaction with the window

### 3. Time-of-Day Lighting

**Test:** Let simulation run, observe lighting changes over sim-time
**Expected:** Gradual color temperature shifts: warm orange at dawn, bright white at noon, orange at sunset, blue-tinted at night
**Why human:** Lighting is visual and changes gradually

### 4. 2D/3D Toggle Round-Trip

**Test:** Pan/zoom in 2D, press [V] for 3D, then [V] back to 2D
**Expected:** Camera center position preserved across toggle (same area visible), smooth 0.5s transition animation
**Why human:** Coordinate mapping correctness and animation smoothness require visual confirmation

### 5. 2D Mode Regression

**Test:** Verify 2D mode is completely unchanged from before Phase 18
**Expected:** Pan, zoom, agent rendering, overlays all work identically
**Why human:** Requires comparison with known-good pre-Phase-18 behavior

### Gaps Summary

No automated gaps found. All 5 success criteria from ROADMAP.md are fully supported by verified artifacts and wiring:

1. **3D perspective with depth ordering** -- OrbitCamera perspective projection + Depth32Float attachment + depth-tested render passes across all 4 pipelines (ground, road, mesh, billboard).
2. **Road surface polygons with lane markings** -- RoadGraph -> polygon expansion via generate_road_mesh/lane_markings/junctions -> static GPU buffers -> per-frame rendering with road_surface.wgsl.
3. **3-tier LOD with GPU instancing** -- classify_lod with 10% hysteresis band -> MeshInstance3D/BillboardInstance3D buffers -> instanced draw calls via mesh_3d.wgsl and billboard_3d.wgsl.
4. **2D/3D toggle** -- ViewMode state machine + [V] key + egui button + bidirectional camera state mapping (from_camera_2d / reverse) + ViewTransition animation.
5. **Time-of-day lighting** -- 4 keyframes -> compute_lighting() -> LightingUniform GPU buffer -> shader diffuse+ambient application.

89 unit tests across Phase 18 files (orbit_camera:12, renderer3d:7, road_surface:14, lighting:6, lod:10, mesh_loader:6, app_input:5, sim_render:29). All 4 WGSL shaders naga-validated in tests.

Two files exceed the 700-line convention limit (renderer3d.rs at 942, sim_render.rs at 1042). These are code organization warnings, not functionality gaps.

---

_Verified: 2026-03-11T14:30:00Z_
_Verifier: Claude (gsd-verifier)_
