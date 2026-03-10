# Phase 18: 3D Rendering Core - Context

**Gathered:** 2026-03-10
**Status:** Ready for planning

<domain>
## Phase Boundary

User can view the running simulation in a 3D perspective with depth-correct rendering, LOD agents, road surfaces, and time-of-day lighting. Toggle between existing 2D top-down and new 3D perspective. This phase does NOT add buildings (Phase 19), terrain heightmaps (Phase 19), shadows, PBR materials, or skybox.

Requirements: R3D-01, R3D-02, R3D-03, R3D-04, R3D-05

</domain>

<decisions>
## Implementation Decisions

### Camera & View Toggle
- Orbit camera: rotate around focus point on ground, left-drag to orbit, scroll to zoom, middle-drag to pan focus point
- Pitch clamp: 5°–89° (never underground)
- Default 3D pitch: 45° on first toggle (classic city overview angle)
- Animated transition (~0.5s lerp) between 2D orthographic and 3D perspective when toggling
- State mapping: 2D center → 3D orbit focus, 2D zoom → 3D orbit distance, 3D pitch defaults to 45° on first switch
- Toggle via keyboard [V] key or egui toolbar button
- Both renderers coexist: Camera2D + Renderer (existing 2D) and OrbitCamera + Renderer3D (new 3D crate)
- Both share SimSnapshot and ECS world — only the render pipeline swaps
- 3D mode enables depth buffer; 2D mode stays as-is (no depth buffer)

### Agent LOD Strategy
- 3-tier LOD: mesh (<50m from camera), billboard (50–200m), dot (>200m)
- Close-range meshes: proper 3D models loaded from free .glb files (CC0/MIT-licensed, e.g., Kenney, Quaternius)
  - Motorbike: narrow low-poly model (~100–500 vertices)
  - Car: standard low-poly sedan model
  - Bus: elongated low-poly bus model
  - Truck: elongated low-poly truck model
  - Pedestrian: simple humanoid or capsule
  - Requires `gltf` crate for loading .glb assets at startup
  - Models stored in `assets/models/` directory
- Mid-range billboards: camera-facing colored quads, sized by vehicle type, no textures (flat vehicle-type color)
- Far-range dots: single colored points (same as current 2D dot rendering)
- All tiers GPU-instanced per vehicle type
- Instant LOD pop (no cross-fade) with hysteresis band: upgrade at threshold, downgrade at threshold + 10% (prevents flicker)
- Color by vehicle type: motorbike = orange, car = blue, bus = green, truck = red, pedestrian = cyan (consistent with 2D)

### Road Surface Rendering
- Roads as flat grey surface polygons at ground level (y=0)
- Grey asphalt color (#404040), width derived from OSM lane count in velos-net road graph
- Lane markings: white dashed center lines (3m line, 3m gap) + solid white edge lines
- Marking width: 0.15m, offset y=+0.01m above road surface to prevent z-fighting
- Lane marking color: white (#FFFFFF, alpha 0.8)
- Junction surfaces: filled convex hull of approach endpoints, slightly lighter grey (#505050)
- Bezier guide lines from Phase 16 render on top of junction surfaces (existing toggleable overlay)
- No road textures — flat colored geometry only
- Simple ground plane: large flat quad in muted green (#3a5a3a) below road surface (y=-0.01), extends to horizon
- Road geometry generated from velos-net road graph at load time

### Time-of-Day Lighting
- Subtle tint shift: sun direction + ambient color change with simulation time, no shadows, no skybox
- Tied 1:1 to simulation clock (SimWorld.elapsed_seconds() maps to sun angle + ambient color)
- At accelerated sim speed, day/night cycles proportionally faster
- Presets: dawn (warm orange, low intensity) → noon (bright white, high intensity) → sunset (deep orange, medium) → night (cool blue, low intensity)
- Interpolation: smooth lerp between preset keyframes
- Basic diffuse + ambient shading on 3D vehicle meshes: lit_color = ambient * color + diffuse * max(dot(N, L), 0) * color
- Road surfaces: ambient tint only (flat geometry, normal = up)
- Uniforms per frame: sun_direction (vec3), sun_color (vec3), ambient_color (vec3), ambient_intensity (f32)
- Single uniform buffer update per frame — negligible GPU cost

### Claude's Discretion
- Exact orbit camera smoothing/inertia parameters
- Specific .glb model sources and vertex counts per vehicle type
- wgpu depth buffer format (Depth32Float vs Depth24PlusStencil8)
- Road polygon triangulation approach (triangle strips vs earcut)
- Ground plane extent and color tuning
- Lighting preset exact RGB values and interpolation curve
- LOD distance thresholds tuning (50m/200m are starting points)
- Billboard quad sizing per vehicle type

</decisions>

<specifics>
## Specific Ideas

- New Renderer3D crate (decided in v1.2 rev2) — cannot retrofit existing 2D renderer. Both renderers coexist side by side.
- .glb models give proper visual identity to HCMC vehicles — motorbikes especially benefit from recognizable 3D silhouettes vs abstract boxes
- Phase 19 adds buildings and terrain on top of this 3D foundation — road surfaces and ground plane serve as the base layer
- wgpu version decision noted in STATE.md (v27 current vs v28) — researcher should investigate compatibility

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `velos-gpu/src/camera.rs`: Camera2D with orthographic projection — stays for 2D mode, OrbitCamera will be new struct in Renderer3D
- `velos-gpu/src/renderer.rs`: Renderer with AgentInstance (2D position [f32;2], heading f32) — stays for 2D mode
- `velos-gpu/src/sim_snapshot.rs`: SimSnapshot shared between renderers — 3D renderer reads same data, projects to 3D
- `velos-gpu/src/map_tiles.rs`: MapTileRenderer for 2D — NOT reused in 3D (3D generates road geometry from graph)
- `velos-gpu/shaders/agent_render.wgsl`: 2D agent shader — new 3D shader needed with depth + lighting
- `velos-net` road graph: edge geometry, lane counts, junction data — source for 3D road polygon generation

### Established Patterns
- GPU instancing via per-type vertex buffers (AgentInstance struct) — extend pattern for 3D with LOD-per-tier instance buffers
- egui toggles for overlays (guide lines, camera FOV, calibration panel) — add 2D/3D toggle button
- Uniform buffer for camera matrix — extend to include lighting uniforms
- bytemuck Pod/Zeroable for GPU-uploadable structs — same for 3D vertex/instance types

### Integration Points
- `velos-gpu/src/app.rs`: render() dispatches to Renderer — add branching: if 3D mode, dispatch to Renderer3D
- `velos-gpu/src/sim_render.rs`: builds SimSnapshot → feeds both renderers
- egui UI: new toggle button and 3D camera controls
- Cargo workspace: new `velos-renderer3d` crate or module within `velos-gpu` (researcher to evaluate)

</code_context>

<deferred>
## Deferred Ideas

- R3D-06 (OSM building extrusion) — Phase 19
- R3D-07 (SRTM terrain heightmap) — Phase 19
- Shadow maps (R3D-08) — future milestone
- PBR materials (R3D-09) — future milestone
- Skybox / sky dome — future enhancement
- Fly camera mode for street-level exploration — future enhancement
- Cross-fade LOD transitions — future if instant pop looks bad
- Independent time-of-day slider for presentations — future enhancement

</deferred>

---

*Phase: 18-3d-rendering-core*
*Context gathered: 2026-03-10*
