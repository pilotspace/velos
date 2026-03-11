# Phase 19: 3D City Scene - Context

**Gathered:** 2026-03-11
**Status:** Ready for planning

<domain>
## Phase Boundary

The 3D view includes extruded buildings from OSM data and terrain from SRTM DEM, creating a recognizable HCMC cityscape. Buildings render as extruded 3D volumes with height from building:levels tag. Terrain renders from SRTM DEM heightmap as a ground surface mesh with elevation variation. Both integrate with the existing Phase 18 3D renderer (depth, lighting, camera).

Requirements: R3D-06, R3D-07

</domain>

<decisions>
## Implementation Decisions

### Building Appearance
- Uniform light beige/cream (#D4C5A9) base color, ±5% per-building brightness variation to avoid monotony
- Flat roofs only — matches HCMC concrete building reality
- Walls lit by existing diffuse+ambient pipeline; wall normals perpendicular to face, roof normal = up
- Opaque solid geometry, no transparency
- 2-tier LOD by distance from camera focus point:
  - Close (<500m): full extruded geometry (walls + roof)
  - Far (>500m): flat footprint polygon only (no extrusion)
  - Very far (>1500m): culled entirely
- LOD distance measured from orbit camera focus point (not per-building) to avoid per-frame 50K distance checks

### Building Height Estimation
- If `height` tag present → use directly
- If `building:levels` tag present → levels × 3.5m per floor
- If neither → default 10.5m (3 floors × 3.5m, typical HCMC low-rise)

### OSM Data Scope
- Pre-exported `.osm.pbf` file for the 5-district bounding box placed in `data/hcmc/`
- All `building=*` tagged ways/relations included — no size filter (dense small buildings create HCMC urban fabric)
- New `building_import.rs` module in velos-net alongside existing `osm_import.rs`, reusing same OSM parsing infrastructure
- Output: `Vec<BuildingFootprint>` (polygon coords + computed height), consumed by velos-gpu at load time
- Footprint polygon triangulated using `earcutr` (already a velos-gpu dependency from road surface work)

### Terrain Approach
- SRTM 30m resolution (1 arc-second) — 90m too coarse for 12km × 8km area
- Pre-downloaded `.hgt` file(s) for HCMC tiles (N10E106, N10E107) in `data/hcmc/`
- Regular grid mesh triangulated as triangle strips; elevation applied as Y displacement
- Single muted green color (#3a5a3a) matching existing ground plane — no texture, no elevation coloring
- Terrain replaces the flat ground plane from Phase 18 in 3D mode; falls back to flat ground plane if DEM data unavailable
- Roads and buildings render ON TOP of terrain — terrain clamped below road level to prevent poke-through

### Performance Budget
- ~40-60K buildings estimated in 5 districts → ~700K-840K triangles (14 tri/building average)
- ~107K terrain vertices (400×267 grid at 30m resolution)
- All geometry static: generated once at load time, uploaded as static vertex/index buffers
- Single draw call per geometry type (buildings, terrain) — no per-frame updates
- ~50MB GPU memory for buildings, ~5MB for terrain
- 1-3 seconds added to startup for triangulation + parsing
- Render order: Terrain → Roads → Buildings → Agents (depth test handles overlap)

### Claude's Discretion
- Exact building color variation algorithm (random seed per building vs hash of polygon centroid)
- Terrain mesh edge stitching and boundary handling
- Building footprint simplification tolerance (if any, for performance)
- Exact LOD distance thresholds (500m/1500m are starting points, tune based on visual quality)
- Whether to merge small adjacent buildings into single draw batches
- SRTM void fill strategy (interpolation for missing DEM samples)
- Parser crate choice for `.hgt` binary format (manual parsing vs crate)

</decisions>

<specifics>
## Specific Ideas

- Buildings should create the HCMC urban fabric feel — dense, low-rise (3-4 floors) with occasional taller buildings in District 1 CBD
- The same `.osm.pbf` file used for road import can potentially be reused for building extraction (avoid double-download)
- Terrain elevation provides subtle ground variation — HCMC is mostly flat (0-15m elevation) but even small variation makes the 3D scene feel grounded vs a flat plane
- Phase 18's `road_surface.rs` is the template for building geometry generation: same pattern of generate-at-load-time → static GPU buffers

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `velos-gpu/src/renderer3d.rs`: Main 3D renderer — buildings and terrain add new pipelines here
- `velos-gpu/src/road_surface.rs`: Road polygon generation pattern (triangulation, static buffers) — template for building geometry
- `earcutr` crate: Already a dependency for polygon triangulation in road surfaces — reuse for building footprints
- `velos-gpu/src/lighting.rs`: Diffuse+ambient lighting pipeline — buildings use same LightingUniform
- `velos-gpu/src/orbit_camera.rs`: OrbitCamera provides eye_position for LOD distance calculation
- `velos-gpu/shaders/ground_plane.wgsl`: Ground plane shader — terrain mesh can reuse similar vertex layout (position + color)
- `velos-gpu/shaders/mesh_3d.wgsl`: Lit mesh shader — buildings can reuse for wall/roof faces with normals

### Established Patterns
- Static geometry at load time: road_surface.rs generates vertices once → uploads to GPU as static buffers
- Vertex struct with bytemuck Pod/Zeroable: RoadSurfaceVertex pattern → BuildingVertex follows same
- Separate render pass ordering: ground → road → 3D content (terrain slots into ground layer, buildings after roads)
- CameraUniform3D bind group: shared across all 3D pipelines — buildings/terrain bind same camera

### Integration Points
- `renderer3d.rs`: Add building pipeline and terrain pipeline alongside existing ground/road/mesh/billboard pipelines
- `velos-net`: New building_import.rs module exports BuildingFootprint data consumed by renderer
- `sim_startup.rs`: Building and terrain data loading happens alongside road surface generation
- `data/hcmc/`: SRTM .hgt files and OSM .pbf for buildings placed here

</code_context>

<deferred>
## Deferred Ideas

- Cascaded shadow maps for buildings (R3D-08) — future milestone
- PBR materials on buildings (R3D-09) — future milestone
- Vegetation and street furniture (R3D-10) — future milestone
- Building textures (windows, facades) — future enhancement
- Roof type variation (hip, gable) from OSM roof:shape tag — future enhancement
- Indoor building visibility / transparency at close range — future enhancement

</deferred>

---

*Phase: 19-3d-city-scene*
*Context gathered: 2026-03-11*
