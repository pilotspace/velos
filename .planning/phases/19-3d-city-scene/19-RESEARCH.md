# Phase 19: 3D City Scene - Research

**Researched:** 2026-03-11
**Domain:** OSM building extrusion, SRTM DEM terrain mesh, wgpu static geometry rendering
**Confidence:** HIGH

## Summary

Phase 19 adds two static geometry layers to the existing Phase 18 3D renderer: extruded OSM buildings and SRTM DEM terrain mesh. Both follow the established pattern from `road_surface.rs` -- generate geometry at load time, upload as static GPU vertex/index buffers, render with existing camera and lighting pipelines.

The building pipeline extracts `building=*` ways from the same `.osm.pbf` file used for road import, triangulates footprints with the existing `earcutr` dependency, extrudes walls, and renders with the `mesh_3d.wgsl` lighting pipeline (diffuse+ambient). The terrain pipeline parses SRTM 1-arc-second `.hgt` binary files (3601x3601 grid of big-endian i16 elevations), builds a regular triangle strip mesh, and renders with the `ground_plane.wgsl` camera-only pipeline. Both integrate into `Renderer3D` as new pipelines alongside existing ground/road/mesh/billboard pipelines.

**Primary recommendation:** Follow the `road_surface.rs` static-buffer pattern exactly -- new vertex structs with bytemuck Pod/Zeroable, generate-at-load-time functions, upload to GPU once, render every frame with existing bind groups.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Uniform light beige/cream (#D4C5A9) base color, +/-5% per-building brightness variation to avoid monotony
- Flat roofs only -- matches HCMC concrete building reality
- Walls lit by existing diffuse+ambient pipeline; wall normals perpendicular to face, roof normal = up
- Opaque solid geometry, no transparency
- 2-tier LOD by distance from camera focus point: Close (<500m) full extrusion, Far (>500m) flat footprint, Very far (>1500m) culled
- LOD distance measured from orbit camera focus point (not per-building)
- Height: `height` tag -> direct, `building:levels` -> levels x 3.5m, default 10.5m
- Pre-exported `.osm.pbf` for 5-district bounding box in `data/hcmc/`
- All `building=*` tagged ways/relations included -- no size filter
- New `building_import.rs` module in velos-net alongside `osm_import.rs`, reusing OSM parsing infrastructure
- Output: `Vec<BuildingFootprint>` (polygon coords + computed height), consumed by velos-gpu at load time
- Footprint polygon triangulated using `earcutr` (already a velos-gpu dependency)
- SRTM 30m resolution (1 arc-second)
- Pre-downloaded `.hgt` file(s) for HCMC tiles (N10E106, N10E107) in `data/hcmc/`
- Regular grid mesh triangulated as triangle strips; elevation applied as Y displacement
- Single muted green color (#3a5a3a) matching existing ground plane
- Terrain replaces flat ground plane in 3D mode; falls back if DEM unavailable
- Roads and buildings render ON TOP of terrain -- terrain clamped below road level
- ~40-60K buildings, ~700K-840K triangles, ~107K terrain vertices
- All geometry static: generated once, uploaded as static vertex/index buffers
- Single draw call per geometry type
- Render order: Terrain -> Roads -> Buildings -> Agents

### Claude's Discretion
- Exact building color variation algorithm (random seed per building vs hash of polygon centroid)
- Terrain mesh edge stitching and boundary handling
- Building footprint simplification tolerance (if any, for performance)
- Exact LOD distance thresholds (500m/1500m are starting points)
- Whether to merge small adjacent buildings into single draw batches
- SRTM void fill strategy (interpolation for missing DEM samples)
- Parser crate choice for `.hgt` binary format (manual parsing vs crate)

### Deferred Ideas (OUT OF SCOPE)
- Cascaded shadow maps for buildings (R3D-08)
- PBR materials on buildings (R3D-09)
- Vegetation and street furniture (R3D-10)
- Building textures (windows, facades)
- Roof type variation from OSM roof:shape tag
- Indoor building visibility / transparency at close range
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| R3D-06 | OSM building footprints render as extruded 3D buildings with height from building:levels tag | Building import from osmpbf (same crate used for road import), earcutr triangulation (already a dep), extrusion geometry generation, mesh_3d.wgsl lit pipeline with per-face normals |
| R3D-07 | Terrain renders from SRTM DEM heightmap data as ground surface mesh | SRTM .hgt binary parsing (3601x3601 big-endian i16), regular grid mesh generation, ground_plane.wgsl camera-only pipeline, terrain replaces flat ground plane |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| osmpbf | workspace | OSM PBF parsing for building extraction | Already used in `osm_import.rs` for road import; same two-pass Element::Way pattern |
| earcutr | 0.5 | Polygon triangulation for building footprints | Already a velos-gpu dependency from Phase 16 map tiles; proven earcut algorithm handles concave polygons |
| bytemuck | workspace | Pod/Zeroable vertex structs for GPU buffers | Standard across all existing vertex types (GroundPlaneVertex, RoadSurfaceVertex, Vertex3D) |
| wgpu | workspace | GPU buffer creation and render pipelines | Existing renderer infrastructure |
| glam | workspace | Vector math for normal calculation | Already used throughout renderer3d.rs |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| (none needed) | - | SRTM .hgt parsing | Manual parsing preferred -- format is trivially simple (raw i16 big-endian grid), no crate dependency warranted |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Manual .hgt parsing | srtm_reader crate | .hgt is 25 bytes of parsing code (read + byteswap); adding a crate dependency is overkill |
| earcutr | earcut 0.4 | earcut is a newer rewrite but earcutr is already a dependency; switching adds risk for no benefit |
| Per-building draw calls | Batched single buffer | Single buffer with all buildings is correct; per-building draw calls would kill performance at 50K buildings |

**Installation:**
No new dependencies needed. All required crates are already in the workspace.

## Architecture Patterns

### Recommended Project Structure
```
crates/velos-net/src/
    building_import.rs    # NEW: OSM building footprint extraction
    osm_import.rs         # EXISTING: road import (template for building_import)
    lib.rs                # ADD: pub mod building_import + pub use exports

crates/velos-gpu/src/
    renderer3d.rs         # MODIFY: add building + terrain pipelines, buffers, render calls
    building_geometry.rs  # NEW: building extrusion geometry generation
    terrain.rs            # NEW: SRTM DEM parsing + terrain mesh generation
    sim_startup.rs        # MODIFY: load building + terrain data alongside road geometry

crates/velos-gpu/shaders/
    building_3d.wgsl      # NEW: lit building shader (reuses mesh_3d.wgsl structure)
    terrain.wgsl          # NEW: terrain mesh shader (reuses ground_plane.wgsl structure)

data/hcmc/
    N10E106.hgt           # SRTM DEM tile (pre-downloaded)
    N10E107.hgt           # SRTM DEM tile (pre-downloaded, may be needed for eastern districts)
```

### Pattern 1: Building Import (velos-net)
**What:** Two-pass OSM PBF reading to extract building footprints with height data
**When to use:** At startup, reusing the same .osm.pbf file as road import
**Example:**
```rust
// Source: existing osm_import.rs pattern
/// A building footprint with polygon coordinates and computed height.
#[derive(Debug, Clone)]
pub struct BuildingFootprint {
    /// Polygon exterior ring in projected metres [x, y].
    pub polygon: Vec<[f64; 2]>,
    /// Building height in metres (from tags or default).
    pub height_m: f64,
}

pub fn import_buildings(
    pbf_path: &Path,
    proj: &EquirectangularProjection,
) -> Result<Vec<BuildingFootprint>, NetError> {
    // Pass 1: collect node coordinates (same as osm_import)
    // Pass 2: extract building=* ways, resolve node refs to coords,
    //         compute height from height/building:levels/default tags
}
```

### Pattern 2: Building Extrusion Geometry (velos-gpu)
**What:** Convert BuildingFootprint polygons into 3D extruded geometry with normals
**When to use:** At load time, after building import, before GPU upload
**Example:**
```rust
// Source: road_surface.rs pattern adapted for lit geometry
/// Building vertex: position + normal for lit rendering.
/// 24 bytes: position (12) + normal (12). Color passed via uniform or per-building instance.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct BuildingVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}
// Note: color variation can be embedded in vertex color field instead,
// following the RoadSurfaceVertex pattern (position + color = 28 bytes)
// OR use position + normal (24 bytes) + per-building color via vertex attribute.
// Decision: position (12) + normal (12) + color (16) = 40 bytes per vertex.
// This avoids needing instancing for static buildings.

/// Generate extruded building geometry from footprints.
/// Returns (vertices, indices) for a single indexed draw call.
pub fn generate_building_geometry(
    buildings: &[BuildingFootprint],
    lod_distance: f32,       // camera focus distance for LOD
    focus_point: [f32; 2],   // camera focus XZ
) -> (Vec<BuildingVertex>, Vec<u32>) {
    // For each building:
    // 1. Triangulate roof with earcutr (normal = [0, 1, 0])
    // 2. Generate wall quads between consecutive polygon vertices
    //    (normal = perpendicular to wall face, pointing outward)
    // 3. Apply height as Y displacement for roof vertices
    // 4. Base vertices at Y=0 (ground level)
}
```

### Pattern 3: SRTM Terrain Mesh
**What:** Parse .hgt binary and build a regular grid triangle mesh
**When to use:** At load time, once
**Example:**
```rust
// SRTM .hgt format: 3601*3601 big-endian i16 values for 1-arc-second
const SRTM1_SAMPLES: usize = 3601;
const SRTM_VOID: i16 = -32768;

pub fn parse_hgt(path: &Path) -> Result<Vec<Vec<i16>>, io::Error> {
    let data = std::fs::read(path)?;
    // File size determines resolution: 3601*3601*2 = SRTM1, 1201*1201*2 = SRTM3
    let samples = match data.len() {
        25_934_402 => 3601, // SRTM1
        2_884_802 => 1201,  // SRTM3
        _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "unknown HGT size")),
    };
    // Parse big-endian i16 grid
    let mut grid = vec![vec![0i16; samples]; samples];
    for row in 0..samples {
        for col in 0..samples {
            let offset = (row * samples + col) * 2;
            grid[row][col] = i16::from_be_bytes([data[offset], data[offset + 1]]);
        }
    }
    Ok(grid)
}
```

### Pattern 4: Static Geometry Upload (established pattern)
**What:** Generate vertices once at startup, upload as immutable GPU buffer
**When to use:** Buildings and terrain (same as road_surface.rs)
**Example:**
```rust
// Source: renderer3d.rs upload_road_geometry pattern
pub fn upload_building_geometry(&mut self, device: &wgpu::Device, ...) {
    let (vertices, indices) = generate_building_geometry(&buildings, ...);
    self.building_vertex_buffer = Some(device.create_buffer_init(...));
    self.building_index_buffer = Some(device.create_buffer_init(...));
    self.building_index_count = indices.len() as u32;
}
```

### Anti-Patterns to Avoid
- **Per-building draw calls:** 50K+ buildings each as separate draw = GPU state change disaster. Use single indexed draw call with all buildings in one buffer.
- **Dynamic LOD per-frame:** Regenerating building geometry per-frame for LOD is wasteful. Pre-generate two buffers (full detail + flat footprints) at load time, select based on camera distance.
- **Using instancing for buildings:** Buildings are not identical meshes with different transforms (unlike agents). Each building has unique geometry. Use a single merged vertex/index buffer, not instancing.
- **Terrain as triangle list:** Use indexed triangle strip or indexed triangles from a grid -- never generate 6 vertices per quad (wastes 3x memory vs indexed).

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Polygon triangulation | Custom ear-clipping | `earcutr::earcut()` | Handles concave polygons, collinear points, degenerate cases; already a dependency |
| OSM PBF parsing | Custom protobuf decoder | `osmpbf::ElementReader` | Handles compression, dense nodes, parallel decoding; already used for roads |
| Coordinate projection | Custom lat/lon math | `EquirectangularProjection` from velos-net | Already proven for HCMC, reuse exact same projection for building/terrain consistency |
| Normal calculation | Manual cross-product | `glam::Vec3::cross()` | Already available in workspace, handles edge cases |

**Key insight:** This phase adds zero new dependencies. Every library needed is already in the workspace from Phases 16-18.

## Common Pitfalls

### Pitfall 1: Building Polygon Winding Order
**What goes wrong:** earcutr produces triangles with inconsistent winding if input polygon has mixed CW/CCW ordering, causing backface-culled faces to disappear.
**Why it happens:** OSM ways have no guaranteed winding order for building polygons.
**How to avoid:** After extracting polygon coordinates, compute signed area. If negative (clockwise), reverse the polygon to ensure CCW winding. earcutr expects CCW input.
**Warning signs:** Buildings with missing walls or roofs when viewed from certain angles.

### Pitfall 2: Wall Normal Direction
**What goes wrong:** Wall normals point inward instead of outward, making buildings appear as dark silhouettes (lit from inside).
**Why it happens:** Normal computed from cross-product of wall edge vectors without considering outward direction.
**How to avoid:** For a wall between polygon vertices V[i] and V[i+1] with height H: the outward normal is the 2D perpendicular of the edge direction, rotated 90 degrees to the LEFT of the polygon traversal direction (assuming CCW winding). Use `cross(wall_up, wall_edge)` where wall_up = (0,1,0) and wall_edge = V[i+1] - V[i].
**Warning signs:** Buildings darker on sun-facing side, brighter on shadow side.

### Pitfall 3: Z-Fighting Between Terrain and Roads
**What goes wrong:** Flickering artifacts where roads sit on terrain surface at exactly the same depth.
**Why it happens:** Terrain mesh elevation and road Y=0.0 may coincide, especially in flat areas of HCMC.
**How to avoid:** The decision specifies "terrain clamped below road level." Implement by setting terrain Y to `min(elevation, -0.5)` to stay below the existing GROUND_Y = -0.5 constant. Roads remain at Y=0.0 with Y-offset layering (junction 0.05, marking 0.1). Use the existing depth bias on terrain pipeline (same as ground_plane_pipeline bias: constant=-2, slope_scale=-2.0).
**Warning signs:** Shimmer/flickering on flat road areas when camera moves.

### Pitfall 4: SRTM Void Values
**What goes wrong:** -32768 (void) values in SRTM data create deep holes in terrain or crash on i16::MAX displacement.
**Why it happens:** SRTM has data voids in areas with radar shadow (steep terrain, water bodies).
**How to avoid:** Replace void values with bilinear interpolation from nearest valid neighbors. HCMC is flat and well-surveyed, so voids should be rare. Fallback: clamp void to 0m (sea level).
**Warning signs:** Terrain spikes downward to -32768m at random points.

### Pitfall 5: Building Footprint at Wrong Y Position
**What goes wrong:** Building base floats above or sinks below terrain because base Y is hardcoded to 0.0 while terrain has elevation.
**Why it happens:** Buildings need their base to sit on terrain, but terrain is clamped to Y <= -0.5.
**How to avoid:** Since terrain is clamped below road level (-0.5) and buildings render in the building pass (after terrain+roads), building base at Y=0.0 is correct -- they sit on the road surface level. This matches the real-world situation where building ground floors are at street level.
**Warning signs:** Buildings appearing to hover above terrain at the edges of the road network.

### Pitfall 6: OSM Multipolygon Relations for Buildings
**What goes wrong:** Only `Way`-type buildings are extracted, missing building relations (multipolygon with inner/outer rings for courtyards).
**Why it happens:** `Element::Relation` is ignored in the building import pass.
**How to avoid:** For POC scope, extracting only Way-type buildings is acceptable (covers ~95% of HCMC buildings). Multipolygon relations (buildings with holes/courtyards) can be deferred. Document this limitation.
**Warning signs:** Some large buildings (malls, government buildings) missing from the scene.

### Pitfall 7: Renderer3D File Size Exceeding 700 Lines
**What goes wrong:** Adding building + terrain pipelines, buffers, upload methods, and render calls to renderer3d.rs pushes it well past the 700-line limit.
**Why it happens:** renderer3d.rs is already 943 lines (already over limit).
**How to avoid:** Extract building and terrain rendering into separate modules (`building_geometry.rs`, `terrain.rs`) that expose pipeline creation + render functions. Renderer3D holds the buffers and calls into these modules. Consider also extracting existing ground/road rendering.
**Warning signs:** File exceeds 700 lines during implementation.

## Code Examples

### earcutr Triangulation for Building Roof
```rust
// Source: existing map_tiles.rs line 222 usage pattern
fn triangulate_roof(polygon: &[[f64; 2]]) -> Vec<u32> {
    // earcutr expects flat coordinate array: [x0, y0, x1, y1, ...]
    let coords: Vec<f64> = polygon.iter().flat_map(|p| [p[0], p[1]]).collect();
    // No holes for building roofs (simple polygon)
    earcutr::earcut(&coords, &[], 2).unwrap_or_default()
}
```

### Wall Quad Generation
```rust
fn generate_wall_quad(
    v0: [f64; 2], v1: [f64; 2], height: f32,
    vertices: &mut Vec<BuildingVertex>, indices: &mut Vec<u32>,
) {
    let base_idx = vertices.len() as u32;
    // Wall edge direction
    let dx = (v1[0] - v0[0]) as f32;
    let dz = (v1[1] - v0[1]) as f32;
    // Outward normal (perpendicular, rotated left for CCW polygon)
    let len = (dx * dx + dz * dz).sqrt();
    let normal = if len > 1e-6 { [-dz / len, 0.0, dx / len] } else { [1.0, 0.0, 0.0] };

    // Four corners: bottom-left, bottom-right, top-right, top-left
    // 2D (x, y) -> 3D (x, Y, y)
    vertices.push(BuildingVertex { position: [v0[0] as f32, 0.0, v0[1] as f32], normal });
    vertices.push(BuildingVertex { position: [v1[0] as f32, 0.0, v1[1] as f32], normal });
    vertices.push(BuildingVertex { position: [v1[0] as f32, height, v1[1] as f32], normal });
    vertices.push(BuildingVertex { position: [v0[0] as f32, height, v0[1] as f32], normal });

    // Two triangles (CCW winding when viewed from outside)
    indices.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]);
    indices.extend_from_slice(&[base_idx, base_idx + 2, base_idx + 3]);
}
```

### SRTM Grid to Terrain Mesh
```rust
fn srtm_grid_to_mesh(
    grid: &[Vec<i16>],
    proj: &EquirectangularProjection,
    tile_lat: f64, tile_lon: f64,
    samples: usize,
) -> (Vec<TerrainVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let step = 1.0 / (samples - 1) as f64; // degrees per sample

    for row in 0..samples {
        for col in 0..samples {
            let lat = tile_lat + 1.0 - row as f64 * step; // North to south
            let lon = tile_lon + col as f64 * step;
            let (x, y_proj) = proj.project(lat, lon);
            let elevation = grid[row][col];
            let y = if elevation == -32768 { 0.0 } else { elevation as f32 };
            // Clamp below road level
            let y = y.min(-0.5);
            vertices.push(TerrainVertex {
                position: [x as f32, y, y_proj as f32],
                color: GROUND_COLOR,
            });
        }
    }

    // Generate triangle indices for grid
    for row in 0..(samples - 1) {
        for col in 0..(samples - 1) {
            let tl = (row * samples + col) as u32;
            let tr = tl + 1;
            let bl = ((row + 1) * samples + col) as u32;
            let br = bl + 1;
            indices.extend_from_slice(&[tl, bl, tr]); // Triangle 1
            indices.extend_from_slice(&[tr, bl, br]); // Triangle 2
        }
    }

    (vertices, indices)
}
```

### Building Height from OSM Tags
```rust
fn compute_building_height(tags: &[(&str, &str)]) -> f64 {
    for &(key, value) in tags {
        match key {
            "height" => {
                if let Ok(h) = value.trim_end_matches(" m").trim_end_matches('m')
                    .trim().parse::<f64>() {
                    return h;
                }
            }
            "building:levels" => {
                if let Ok(levels) = value.parse::<f64>() {
                    return levels * 3.5;
                }
            }
            _ => {}
        }
    }
    10.5 // Default: 3 floors * 3.5m
}
```

### Color Variation via Polygon Centroid Hash
```rust
// Deterministic per-building color variation from centroid position
fn building_color_with_variation(centroid_x: f64, centroid_y: f64) -> [f32; 4] {
    // Base color: #D4C5A9 = (0.831, 0.773, 0.663)
    let base = [0.831f32, 0.773, 0.663];
    // Hash centroid to get deterministic [-0.05, +0.05] variation
    let hash = ((centroid_x * 1000.0 + centroid_y * 7919.0).sin() * 43758.5453).fract();
    let variation = (hash as f32 - 0.5) * 0.1; // +/- 5%
    [
        (base[0] + variation).clamp(0.0, 1.0),
        (base[1] + variation).clamp(0.0, 1.0),
        (base[2] + variation).clamp(0.0, 1.0),
        1.0,
    ]
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Per-building draw calls | Single merged vertex/index buffer | Always best practice | 50K buildings require batching; GPU state changes dominate otherwise |
| 3D tiles from CityGML | OSM extrusion at load time | N/A (no CityGML for HCMC) | No external 3D dataset exists; OSM extrusion is the only viable approach |
| Flat ground plane | SRTM DEM terrain mesh | Phase 19 | Replaces the 6-vertex flat quad with elevation-aware terrain |

**Deprecated/outdated:**
- The flat ground plane (GROUND_Y=-0.5, 6 vertices) is replaced by terrain mesh in 3D mode but retained as fallback when DEM data is unavailable.

## Open Questions

1. **Terrain grid subsampling**
   - What we know: Full SRTM1 grid is 3601x3601 = ~13M samples per tile. The 5-district area spans roughly 400x267 cells at 30m resolution.
   - What's unclear: Whether to load the full tile and clip to bounding box, or subsample. Loading full tile uses ~25MB RAM temporarily.
   - Recommendation: Load full tile, clip to bounding box of interest (bounding box from road graph extent), discard rest. 25MB temporary allocation is trivial.

2. **Building LOD buffer strategy**
   - What we know: LOD decision says full extrusion <500m, flat footprint >500m, culled >1500m from focus.
   - What's unclear: Whether to pre-generate two static buffers (full + flat) or regenerate on LOD change.
   - Recommendation: Pre-generate both at load time. LOD changes are infrequent (only when camera moves significantly). Swap buffer reference in render call based on camera focus distance. Two static buffers (~50MB + ~10MB) is simpler and faster than per-frame regeneration.

3. **SRTM tile coverage**
   - What we know: HCMC Districts 1, 3, 5, 10, Binh Thanh center around 10.775N, 106.7E. Tile N10E106 covers lat 10-11, lon 106-107 which includes all 5 districts.
   - What's unclear: Whether eastern districts extend past lon 107, requiring N10E107 tile.
   - Recommendation: Start with N10E106 only. The POC bounding box (12km x 8km centered on District 1) fits entirely within this tile. Add N10E107 support later if needed.

4. **renderer3d.rs is already 943 lines (over 700-line limit)**
   - What we know: Adding two more pipelines will push it further over.
   - What's unclear: How to split without breaking the existing render pass structure.
   - Recommendation: Extract building and terrain into separate modules. The renderer holds buffers and calls module functions for pipeline creation and rendering. Also extract existing ground plane code into its own module to bring renderer3d.rs under 700 lines.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) + naga WGSL validation |
| Config file | Cargo.toml [dev-dependencies] |
| Quick run command | `cargo test -p velos-net --lib building_import && cargo test -p velos-gpu --lib building_geometry terrain` |
| Full suite command | `cargo test -p velos-net -p velos-gpu` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| R3D-06 | Building footprint extraction from OSM | unit | `cargo test -p velos-net --lib building_import` | Wave 0 |
| R3D-06 | Building height computation from tags | unit | `cargo test -p velos-net --lib building_import::tests::test_height` | Wave 0 |
| R3D-06 | Building extrusion geometry generation | unit | `cargo test -p velos-gpu --lib building_geometry::tests` | Wave 0 |
| R3D-06 | Building WGSL shader validation | unit | `cargo test -p velos-gpu --lib renderer3d::tests::test_building_3d_wgsl` | Wave 0 |
| R3D-06 | Building vertex struct size/alignment | unit | `cargo test -p velos-gpu --lib building_geometry::tests::test_vertex_size` | Wave 0 |
| R3D-07 | SRTM .hgt file parsing | unit | `cargo test -p velos-gpu --lib terrain::tests::test_parse_hgt` | Wave 0 |
| R3D-07 | Terrain mesh generation from grid | unit | `cargo test -p velos-gpu --lib terrain::tests::test_grid_to_mesh` | Wave 0 |
| R3D-07 | Terrain WGSL shader validation | unit | `cargo test -p velos-gpu --lib renderer3d::tests::test_terrain_wgsl` | Wave 0 |
| R3D-07 | SRTM void value handling | unit | `cargo test -p velos-gpu --lib terrain::tests::test_void_fill` | Wave 0 |
| R3D-06+07 | Integration: render dispatch includes buildings+terrain | smoke/manual | Visual verification in app | Manual |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-net --lib building_import && cargo test -p velos-gpu --lib building_geometry terrain`
- **Per wave merge:** `cargo test -p velos-net -p velos-gpu`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-net/src/building_import.rs` -- building extraction module with tests
- [ ] `crates/velos-gpu/src/building_geometry.rs` -- extrusion geometry with tests
- [ ] `crates/velos-gpu/src/terrain.rs` -- SRTM parsing + mesh generation with tests
- [ ] `crates/velos-gpu/shaders/building_3d.wgsl` -- building shader + naga validation test
- [ ] `crates/velos-gpu/shaders/terrain.wgsl` -- terrain shader + naga validation test (may reuse ground_plane.wgsl)

## Sources

### Primary (HIGH confidence)
- Existing codebase: `crates/velos-gpu/src/renderer3d.rs` -- 943 lines, establishes all rendering patterns
- Existing codebase: `crates/velos-gpu/src/road_surface.rs` -- static geometry generation template
- Existing codebase: `crates/velos-net/src/osm_import.rs` -- OSM PBF parsing pattern with osmpbf crate
- Existing codebase: `crates/velos-net/src/projection.rs` -- EquirectangularProjection for coordinate conversion
- Existing codebase: `crates/velos-gpu/src/map_tiles.rs:222` -- earcutr::earcut usage pattern
- Existing codebase: `crates/velos-gpu/shaders/mesh_3d.wgsl` -- lit shader with normals (building shader template)
- Existing codebase: `crates/velos-gpu/shaders/ground_plane.wgsl` -- unlit camera-only shader (terrain shader template)

### Secondary (MEDIUM confidence)
- [SRTM HGT format](https://surferhelp.goldensoftware.com/subsys/HGT_NASA_SRTM_Data_File_Description.htm) -- 3601x3601 grid, big-endian i16, filename encodes SW corner coordinates
- [earcutr crate](https://crates.io/crates/earcutr) -- v0.5, `earcut(&flat_coords, &hole_indices, 2)` returns triangle indices
- [osmpbf crate](https://github.com/b-r-u/osmpbf) -- ElementReader with Element::Way for building extraction

### Tertiary (LOW confidence)
- Building count estimate (40-60K) from CONTEXT.md -- needs validation against actual OSM data for the 5-district bounding box

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, no new dependencies
- Architecture: HIGH -- follows exact patterns from road_surface.rs and renderer3d.rs
- Pitfalls: HIGH -- identified from direct code inspection of existing renderer patterns
- Building count: MEDIUM -- estimate from CONTEXT.md, actual count depends on OSM data density
- SRTM coverage: HIGH -- HCMC coordinates well within N10E106 tile

**Research date:** 2026-03-11
**Valid until:** 2026-04-11 (stable -- no external library changes expected)
