---
phase: 18-3d-rendering-core
verified: 2026-03-11T11:00:00Z
status: human_needed
score: 5/5 must-haves verified
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
---

# Phase 18: 3D Rendering Core Verification Report

**Phase Goal:** User can view the running simulation in a 3D perspective with depth-correct rendering, LOD agents, road surfaces, and time-of-day lighting
**Verified:** 2026-03-11
**Status:** human_needed (all automated checks pass; visual verification required)
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | User sees simulation from 3D perspective with correct depth ordering | VERIFIED | OrbitCamera produces perspective view-proj matrix (tested), Renderer3D uses Depth32Float attachment with Less compare, render_frame draws ground -> roads -> meshes -> billboards with depth |
| 2 | Roads render as 3D surface polygons with visible lane markings | VERIFIED | generate_road_mesh() expands RoadGraph edges to polygons (lane_count * 3.5m width), generate_lane_markings() creates dashed center (3m/3m) + solid edge lines at y=0.01, road_surface.wgsl naga-validated, Renderer3D.render_roads() draws surfaces then junctions then markings |
| 3 | Agents render as 3D meshes (close), billboards (mid), dots (far) via GPU instancing | VERIFIED | LodTier enum (Mesh/Billboard/Dot), classify_lod() with hysteresis at 50m/200m thresholds, MeshSet loads .glb or procedural fallback boxes, mesh_3d.wgsl + billboard_3d.wgsl naga-validated, Renderer3D draws indexed instanced meshes + 6-vertex instanced billboards, build_instances_3d() classifies ECS agents by distance |
| 4 | User can toggle between 2D and 3D with single click, preserving camera position | VERIFIED | [V] key handler at app.rs:554 calls toggle_view_mode(), egui button at app_egui.rs:50, from_camera_2d() maps 2D center to 3D focus, reverse mapping at app_input.rs:99-100, ViewTransition animates over 0.5s |
| 5 | Scene lighting changes with simulation time-of-day | VERIFIED | 4 keyframes (night/dawn/noon/sunset), compute_lighting() lerps between them, LightingUniform (48 bytes) written to GPU each frame, mesh_3d.wgsl applies diffuse+ambient shading, billboard_3d.wgsl applies ambient tint |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/velos-gpu/src/orbit_camera.rs` | OrbitCamera, ViewMode, ViewTransition, instance types | VERIFIED | 417 lines, 12 unit tests, all exports present |
| `crates/velos-gpu/src/renderer3d.rs` | Renderer3D with depth buffer, ground, road, mesh, billboard pipelines | VERIFIED | 929 lines (exceeds 700-line limit), 8 tests including 4 naga shader validations |
| `crates/velos-gpu/src/road_surface.rs` | Road polygon generation from RoadGraph | VERIFIED | 692 lines, generates road mesh + lane markings + junction surfaces, 14 unit tests |
| `crates/velos-gpu/src/lighting.rs` | Time-of-day keyframes and interpolation | VERIFIED | 251 lines, 4 keyframes, smooth interpolation, 6 unit tests |
| `crates/velos-gpu/src/lod.rs` | LOD classification with hysteresis | VERIFIED | 166 lines, classify_lod with 10% hysteresis band, 10 unit tests |
| `crates/velos-gpu/src/mesh_loader.rs` | glTF loading + procedural fallback | VERIFIED | 296 lines, load_glb + generate_fallback_box + MeshSet, 6 unit tests |
| `crates/velos-gpu/shaders/ground_plane.wgsl` | Ground plane shader | VERIFIED | 909 bytes, naga-validated |
| `crates/velos-gpu/shaders/road_surface.wgsl` | Road surface shader | VERIFIED | 935 bytes, naga-validated |
| `crates/velos-gpu/shaders/mesh_3d.wgsl` | Lit instanced mesh shader | VERIFIED | 2850 bytes, diffuse + ambient, naga-validated |
| `crates/velos-gpu/shaders/billboard_3d.wgsl` | Camera-facing billboard shader | VERIFIED | 2762 bytes, camera_right/camera_up expansion, naga-validated |
| `crates/velos-gpu/src/app_input.rs` | 3D input handling (orbit, zoom, pan) | VERIFIED | 164 lines, 5 unit tests |
| `crates/velos-gpu/src/app_egui.rs` | egui panel with view toggle button | VERIFIED | 170 lines, [V] toggle button at line 50 |
| `crates/velos-gpu/src/sim_render.rs` | build_instances_3d() with LOD classification | VERIFIED | LodBuffers struct + build_instances_3d() added, tested |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| renderer3d.rs | orbit_camera.rs | OrbitCamera.view_proj_matrix() -> camera uniform | WIRED | update_camera() at line 555 writes view_proj to GPU buffer |
| renderer3d.rs | ground_plane.wgsl | Pipeline created from shader source | WIRED | include_wgsl!("../shaders/ground_plane.wgsl") at line 235 |
| renderer3d.rs | road_surface.rs | Holds road vertex buffers and draws them | WIRED | upload_road_geometry() calls generate_road_mesh/markings/junctions, render_roads() draws them |
| renderer3d.rs | mesh_loader.rs | Holds loaded mesh vertex/index buffers | WIRED | MeshSet::load_all() at line 518, render_meshes() draws per-vtype |
| renderer3d.rs | lighting.rs | Updates lighting uniform from sim clock | WIRED | update_lighting() calls compute_lighting() at line 582 |
| lod.rs | orbit_camera.rs | Uses LOD threshold constants | WIRED | Imports LOD_MESH_THRESHOLD, LOD_BILLBOARD_THRESHOLD, HYSTERESIS_FACTOR |
| app.rs | renderer3d.rs | Dispatches to Renderer3D.render_frame() in 3D mode | WIRED | Match on ViewMode::Perspective3D at line 408 |
| app.rs | orbit_camera.rs | Holds OrbitCamera, routes input | WIRED | orbit_camera field in GpuState, handle_3d_input at line 593 |
| sim_render.rs | lod.rs | build_instances_3d calls classify_lod | WIRED | LodBuffers::classify via classify_lod in build_instances_3d |
| app.rs | road geometry | upload_road_geometry at init | WIRED | Line 192: renderer_3d.upload_road_geometry() |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-----------|-------------|--------|----------|
| R3D-01 | 18-01, 18-04 | 3D perspective with depth-correct rendering | SATISFIED | OrbitCamera perspective projection + Depth32Float depth buffer + render pass with depth testing |
| R3D-02 | 18-02 | Roads as 3D surface polygons with lane markings | SATISFIED | generate_road_mesh + generate_lane_markings + road_surface.wgsl + Renderer3D.render_roads() |
| R3D-03 | 18-03, 18-04 | 3-tier LOD with GPU instancing | SATISFIED | LodTier enum + classify_lod + mesh_3d.wgsl instanced draw + billboard_3d.wgsl instanced draw + build_instances_3d() |
| R3D-04 | 18-01, 18-04 | Toggle between 2D and 3D views | SATISFIED | ViewMode enum + [V] key + egui button + from_camera_2d() + reverse mapping |
| R3D-05 | 18-03 | Time-of-day lighting | SATISFIED | 4 keyframes + compute_lighting() + LightingUniform + mesh shader diffuse+ambient |

No orphaned requirements. All 5 R3D requirements mapped to Phase 18 in REQUIREMENTS.md traceability are covered.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| renderer3d.rs | - | 929 lines (exceeds 700-line limit) | Warning | File convention violation from CLAUDE.md. Contains tests (110 lines) + large pipeline setup. Could extract pipeline creation into helpers. |
| assets/models/.gitkeep | - | Placeholder file for model directory | Info | Expected -- .glb files are external assets, fallback boxes used at runtime |

### Human Verification Required

### 1. Visual 3D Rendering

**Test:** Run `cargo run -p velos-gpu`, press [V] to toggle to 3D perspective
**Expected:** Green ground plane below roads, grey road surfaces with white dashed center lines and solid edge lines, junction areas in lighter grey, agents rendered at different LOD tiers (boxes up close, billboards mid-range, dots far away)
**Why human:** Visual rendering correctness (depth ordering, colors, geometry) cannot be verified by unit tests

### 2. Orbit Camera Controls

**Test:** In 3D mode: left-drag to orbit, scroll to zoom, middle-drag to pan
**Expected:** Smooth orbit rotation around focus point, zoom in/out with scroll, pan moves focus point, pitch never goes below ~5 degrees (underground)
**Why human:** Interactive input handling requires human interaction with the window

### 3. Time-of-Day Lighting

**Test:** Let simulation run, observe lighting changes over sim-time
**Expected:** Gradual color temperature shifts: warm orange at dawn, bright white at noon, orange at sunset, blue-tinted at night
**Why human:** Lighting is visual and changes gradually -- cannot be captured by unit tests

### 4. 2D/3D Toggle Preservation

**Test:** Pan/zoom in 2D, press [V] for 3D, then [V] back to 2D
**Expected:** Camera center position preserved across toggle (same area visible)
**Why human:** Coordinate mapping correctness requires visual confirmation that the same area is shown

### 5. 2D Mode Regression

**Test:** Verify 2D mode is completely unchanged from before Phase 18
**Expected:** Pan, zoom, agent rendering, overlays all work identically
**Why human:** Requires comparison with known-good pre-Phase-18 behavior

### Gaps Summary

No automated gaps found. All 5 success criteria are supported by verified artifacts and wiring:

1. **3D perspective with depth ordering** -- OrbitCamera + Depth32Float attachment + depth-tested render passes
2. **Road surface polygons with lane markings** -- RoadGraph -> polygon expansion -> static GPU buffers -> per-frame rendering
3. **3-tier LOD with GPU instancing** -- classify_lod with hysteresis -> MeshInstance3D/BillboardInstance3D buffers -> instanced draw calls
4. **2D/3D toggle** -- ViewMode state machine + [V] key + egui button + bidirectional camera state mapping
5. **Time-of-day lighting** -- 4 keyframes -> compute_lighting -> LightingUniform -> shader diffuse+ambient

One minor convention violation: renderer3d.rs at 929 lines exceeds the 700-line file limit. This is a code organization concern, not a functionality gap.

**Test results:** 64 tests pass, clippy clean, all 4 WGSL shaders naga-validated.

---

_Verified: 2026-03-11_
_Verifier: Claude (gsd-verifier)_
