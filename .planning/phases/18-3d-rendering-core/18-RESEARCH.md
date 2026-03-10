# Phase 18: 3D Rendering Core - Research

**Researched:** 2026-03-10
**Domain:** wgpu 3D rendering — perspective camera, depth buffer, instanced LOD, glTF loading, road surface geometry, time-of-day lighting
**Confidence:** HIGH

## Summary

Phase 18 adds a 3D perspective renderer alongside the existing 2D orthographic renderer. The existing codebase (velos-gpu) already uses wgpu 27, bytemuck, glam, egui, and GPU instancing for 2D agents. The 3D renderer requires: (1) a depth buffer and perspective projection, (2) an orbit camera with pitch/yaw/distance, (3) road surface polygons generated from velos-net edge geometry + earcutr triangulation (already a dependency), (4) 3-tier LOD agent rendering with glTF mesh loading for close range, (5) time-of-day lighting via a per-frame uniform buffer. All of these are well-supported by existing wgpu 27 APIs and the project's current dependency set. The main new dependency is the `gltf` crate for loading .glb model assets.

The architecture decision (from CONTEXT.md and STATE.md) is that a new Renderer3D coexists alongside the existing Renderer -- they share SimSnapshot and ECS world but have separate render pipelines. The 2D renderer stays untouched. The 3D renderer gets its own depth texture, pipelines with DepthStencilState, and a new OrbitCamera struct producing a perspective view-projection matrix.

**Primary recommendation:** Build Renderer3D as a new module within `velos-gpu` (not a separate crate), following the same patterns as the existing Renderer but with depth buffer, 3D vertex formats, and lighting uniforms.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Orbit camera: rotate around focus point on ground, left-drag to orbit, scroll to zoom, middle-drag to pan focus point
- Pitch clamp: 5 deg to 89 deg (never underground)
- Default 3D pitch: 45 deg on first toggle (classic city overview angle)
- Animated transition (~0.5s lerp) between 2D orthographic and 3D perspective when toggling
- State mapping: 2D center to 3D orbit focus, 2D zoom to 3D orbit distance, 3D pitch defaults to 45 deg on first switch
- Toggle via keyboard [V] key or egui toolbar button
- Both renderers coexist: Camera2D + Renderer (existing 2D) and OrbitCamera + Renderer3D (new 3D crate)
- Both share SimSnapshot and ECS world -- only the render pipeline swaps
- 3D mode enables depth buffer; 2D mode stays as-is (no depth buffer)
- 3-tier LOD: mesh (<50m), billboard (50-200m), dot (>200m) with hysteresis band +10%
- Close-range meshes: .glb files from CC0/MIT sources (Kenney, Quaternius)
- Models stored in assets/models/ directory
- Mid-range billboards: camera-facing colored quads, no textures
- Far-range dots: same as current 2D dot rendering
- All tiers GPU-instanced per vehicle type
- Instant LOD pop with hysteresis (no cross-fade)
- Roads as flat grey surface polygons at y=0, lane markings as white dashed/solid lines
- Junction surfaces: filled convex hull of approach endpoints
- No road textures -- flat colored geometry only
- Simple ground plane: large flat quad in muted green below road surface
- Road geometry generated from velos-net road graph at load time
- Time-of-day lighting: sun direction + ambient color tied 1:1 to simulation clock
- Basic diffuse + ambient shading: lit_color = ambient * color + diffuse * max(dot(N, L), 0) * color
- Single uniform buffer update per frame for lighting

### Claude's Discretion
- Exact orbit camera smoothing/inertia parameters
- Specific .glb model sources and vertex counts per vehicle type
- wgpu depth buffer format (Depth32Float vs Depth24PlusStencil8)
- Road polygon triangulation approach (triangle strips vs earcut)
- Ground plane extent and color tuning
- Lighting preset exact RGB values and interpolation curve
- LOD distance thresholds tuning (50m/200m are starting points)
- Billboard quad sizing per vehicle type

### Deferred Ideas (OUT OF SCOPE)
- R3D-06 (OSM building extrusion) -- Phase 19
- R3D-07 (SRTM terrain heightmap) -- Phase 19
- Shadow maps (R3D-08) -- future milestone
- PBR materials (R3D-09) -- future milestone
- Skybox / sky dome -- future enhancement
- Fly camera mode -- future enhancement
- Cross-fade LOD transitions -- future enhancement
- Independent time-of-day slider -- future enhancement
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| R3D-01 | User can view simulation in 3D perspective with depth-correct rendering | Perspective projection via glam Mat4::perspective_rh + wgpu Depth32Float depth buffer + DepthStencilState on all 3D pipelines |
| R3D-02 | Roads render as 3D surface polygons with lane markings | Road edge geometry from velos-net RoadEdge.geometry + lane_count; earcutr (already a dependency) for polygon triangulation; lane markings as offset quad strips |
| R3D-03 | Agents render as 3D meshes (close), billboards (mid), dots (far) with GPU instancing per LOD tier | gltf crate for .glb loading; 3 separate instanced draw calls per vehicle type per LOD tier; LOD distance from camera computed CPU-side per frame |
| R3D-04 | User can toggle between 2D top-down and 3D perspective views | Both renderers coexist; app.rs branches render dispatch; [V] key + egui button; 0.5s lerp transition |
| R3D-05 | Scene lighting follows simulation time-of-day | Per-frame lighting uniform buffer (sun_direction, sun_color, ambient_color, ambient_intensity); keyframe interpolation from sim clock |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| wgpu | 27.0.1 | GPU rendering | Already in workspace; provides depth buffer, render pipelines, instancing |
| glam | 0.29.3 | Math (Mat4, Vec3, Quat) | Already in workspace; perspective_rh, look_at_rh, lerp |
| bytemuck | 1.25.0 | GPU struct serialization | Already in workspace; Pod/Zeroable for vertex/uniform types |
| gltf | 1.4 | Load .glb 3D models | De facto Rust glTF loader; CC0 vehicle models in .glb format |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| earcutr | 0.5 | Polygon triangulation | Already a dependency; road surface and junction polygon triangulation |
| egui | 0.33.3 | UI controls | Already in workspace; 2D/3D toggle button, lighting controls |
| winit | 0.30.13 | Window/input | Already in workspace; keyboard [V] for toggle, orbit camera mouse input |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| gltf | easy-gltf | Higher-level API but less control; gltf is the standard choice |
| earcutr for roads | Triangle strips | Strips work for straight segments but earcutr handles junctions better; earcutr already a dep |
| Depth32Float | Depth24PlusStencil8 | Stencil unused in this phase; Depth32Float is simpler and sufficient |

**Installation:**
```bash
cargo add gltf --manifest-path crates/velos-gpu/Cargo.toml
```

**Recommendation (Claude's Discretion):** Use `Depth32Float` for the depth buffer format. No stencil operations are needed in this phase, and Depth32Float provides maximum precision for the large world-space distances in the HCMC simulation.

## Architecture Patterns

### Recommended Module Structure
```
crates/velos-gpu/
  src/
    renderer3d.rs          # Renderer3D struct (parallel to renderer.rs)
    orbit_camera.rs        # OrbitCamera struct (parallel to camera.rs)
    mesh_loader.rs         # glTF/glb loading into GPU vertex/index buffers
    road_surface.rs        # Road polygon + lane marking generation from RoadGraph
    lighting.rs            # Time-of-day keyframes and uniform buffer
    app.rs                 # Modified: view mode enum, render dispatch branching
    sim_render.rs          # Modified: 3D instance building with LOD classification
  shaders/
    mesh_3d.wgsl           # Lit 3D mesh shader (close-range LOD)
    billboard_3d.wgsl      # Camera-facing billboard shader (mid-range LOD)
    road_surface.wgsl      # Road surface + lane marking shader
    ground_plane.wgsl      # Ground plane shader (or reuse road_surface)
```

### Pattern 1: Dual Renderer Architecture
**What:** GpuState holds both Renderer (2D) and Renderer3D (3D). A `ViewMode` enum (TopDown2D | Perspective3D) determines which renderer's render() is called each frame.
**When to use:** Always -- this is the locked decision.
**Example:**
```rust
enum ViewMode {
    TopDown2D,
    Perspective3D,
}

struct GpuState {
    renderer: Renderer,           // existing 2D
    renderer_3d: Renderer3D,      // new 3D
    camera_2d: Camera2D,          // existing
    orbit_camera: OrbitCamera,    // new
    view_mode: ViewMode,
    // Transition state for animated switch
    transition_progress: Option<f32>, // 0.0..1.0 over 0.5s
}
```

### Pattern 2: OrbitCamera
**What:** Orbit camera stores focus point, distance, yaw, pitch. Produces perspective view-projection matrix via glam.
**When to use:** All 3D rendering.
**Example:**
```rust
pub struct OrbitCamera {
    pub focus: glam::Vec3,    // ground point to orbit around
    pub distance: f32,        // distance from focus
    pub yaw: f32,             // horizontal angle (radians)
    pub pitch: f32,           // vertical angle, clamped [5deg, 89deg]
    pub fov_y: f32,           // field of view (radians), e.g. 45deg
    pub near: f32,            // near plane, e.g. 0.1
    pub far: f32,             // far plane, e.g. 10000.0
    pub viewport: glam::Vec2, // window size in pixels
}

impl OrbitCamera {
    pub fn eye_position(&self) -> glam::Vec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.cos();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.sin();
        self.focus + glam::Vec3::new(x, y, z)
    }

    pub fn view_proj_matrix(&self) -> glam::Mat4 {
        let eye = self.eye_position();
        let view = glam::Mat4::look_at_rh(eye, self.focus, glam::Vec3::Y);
        let aspect = self.viewport.x / self.viewport.y;
        let proj = glam::Mat4::perspective_rh(self.fov_y, aspect, self.near, self.far);
        proj * view
    }
}
```

### Pattern 3: 3D Instance Data with LOD
**What:** CPU-side LOD classification each frame. For each agent, compute distance to camera eye, classify into mesh/billboard/dot tier, build separate instance buffers per (vehicle_type, lod_tier).
**When to use:** R3D-03 agent rendering.
**Example:**
```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MeshInstance3D {
    pub world_pos: [f32; 3],    // x, y=0, z (ground plane)
    pub heading: f32,
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BillboardInstance3D {
    pub world_pos: [f32; 3],
    pub size: [f32; 2],        // width, height in world units
    pub color: [f32; 4],
    pub _pad: [f32; 2],
}

// LOD classification with hysteresis
const LOD_MESH_THRESHOLD: f32 = 50.0;
const LOD_BILLBOARD_THRESHOLD: f32 = 200.0;
const HYSTERESIS_FACTOR: f32 = 1.1; // downgrade at threshold * 1.1
```

### Pattern 4: Depth Buffer Setup
**What:** Create a depth texture matching window size, attach to render pass, set DepthStencilState on all 3D pipelines.
**Example:**
```rust
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth_texture"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&Default::default())
}

// In RenderPipelineDescriptor:
depth_stencil: Some(wgpu::DepthStencilState {
    format: DEPTH_FORMAT,
    depth_write_enabled: true,
    depth_compare: wgpu::CompareFunction::Less,
    stencil: wgpu::StencilState::default(),
    bias: wgpu::DepthBiasState::default(),
}),

// In render pass:
depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
    view: &depth_texture_view,
    depth_ops: Some(wgpu::Operations {
        load: wgpu::LoadOp::Clear(1.0),
        store: wgpu::StoreOp::Store,
    }),
    stencil_ops: None,
}),
```

### Pattern 5: Time-of-Day Lighting Uniform
**What:** Single uniform buffer updated per frame with sun direction and ambient color. Shader applies basic diffuse + ambient.
**Example:**
```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct LightingUniform {
    pub sun_direction: [f32; 3],
    pub _pad0: f32,
    pub sun_color: [f32; 3],
    pub _pad1: f32,
    pub ambient_color: [f32; 3],
    pub ambient_intensity: f32,
}

// WGSL shader:
// lit_color = ambient_color * ambient_intensity * base_color
//           + sun_color * max(dot(normal, sun_direction), 0.0) * base_color
```

### Pattern 6: Road Surface Generation from RoadGraph
**What:** At load time, iterate RoadGraph edges, expand each edge polyline to a polygon based on lane_count * lane_width, triangulate with earcutr, upload as static vertex buffer.
**When to use:** R3D-02 road rendering.
**Example:**
```rust
fn generate_road_polygons(graph: &RoadGraph, lane_width: f64) -> Vec<RoadSurfaceVertex> {
    let mut vertices = Vec::new();
    for edge_idx in graph.edge_indices() {
        let edge = graph.edge(edge_idx);
        let half_width = (edge.lane_count as f64 * lane_width) / 2.0;
        // For each segment in edge.geometry polyline:
        //   compute perpendicular offset
        //   create quad (two triangles) for the road surface
        // Use earcutr for junction convex hulls
    }
    vertices
}
```

### Anti-Patterns to Avoid
- **Modifying existing Renderer**: The 2D renderer works. Do NOT add depth buffer or 3D features to it. Build Renderer3D separately.
- **Per-agent draw calls**: Use GPU instancing (per-type, per-LOD-tier buffers). Never one draw call per agent.
- **Recomputing road geometry per frame**: Road surfaces are static. Generate once at load, upload to GPU, render every frame from static buffers.
- **Y-up vs Y-down confusion**: The existing 2D renderer uses a flat coordinate system. The 3D renderer uses Y-up (glam convention). Position mapping: 2D (x, y) -> 3D (x, 0, y) where Y=0 is ground plane.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Polygon triangulation | Custom ear-clipping | earcutr (already a dep) | Edge cases with concave polygons, holes; earcutr handles all |
| 3D model loading | Custom .obj parser | gltf crate | glTF is standard, handles buffers/accessors/materials correctly |
| Matrix math | Manual 4x4 ops | glam (already a dep) | perspective_rh, look_at_rh, lerp, slerp all built-in |
| Convex hull | Manual algorithm | geo crate or simple sort-by-angle | Junction hulls are small (4-8 points) but correctness matters |
| Billboard rotation | Manual billboard math | WGSL shader with camera-facing computation | GPU computes view-space alignment per instance efficiently |

**Key insight:** The existing codebase already has the hard parts (wgpu device management, instancing patterns, egui integration, SimSnapshot). The 3D renderer is a parallel pipeline using the same data, not a replacement.

## Common Pitfalls

### Pitfall 1: Coordinate System Mismatch
**What goes wrong:** 2D renderer uses (x, y) flat coordinates. 3D renderer needs (x, y, z) with Y-up convention.
**Why it happens:** velos-net stores positions as [f64; 2] in local metres. SimSnapshot.positions are [f64; 2].
**How to avoid:** Define a clear mapping: `world_3d = (pos[0], 0.0, pos[1])`. Road surfaces at y=0, ground plane at y=-0.01. Document this in Renderer3D.
**Warning signs:** Objects appearing at wrong height, roads floating, agents underground.

### Pitfall 2: Depth Buffer Not Recreated on Resize
**What goes wrong:** After window resize, depth texture is wrong size, causing validation errors or visual artifacts.
**Why it happens:** Depth texture must match surface texture dimensions exactly.
**How to avoid:** In resize(), recreate depth texture with new dimensions. Same pattern as surface reconfigure.
**Warning signs:** Crash on resize, visual corruption after resize.

### Pitfall 3: Z-Fighting Between Road Surface and Lane Markings
**What goes wrong:** Lane markings flicker because they are at the same depth as road surface.
**Why it happens:** Coplanar geometry at y=0.
**How to avoid:** CONTEXT.md specifies markings at y=+0.01 offset. Use depth bias or small Y offset in vertex data. The Y offset approach is simpler and reliable.
**Warning signs:** Flickering white lines, markings appearing/disappearing as camera moves.

### Pitfall 4: LOD Pop at Exact Threshold
**What goes wrong:** Agents rapidly switch LOD tiers when near the threshold distance, causing visual flickering.
**Why it happens:** Agent at distance 50m oscillates between 49.9 and 50.1 each frame.
**How to avoid:** Hysteresis band as specified in CONTEXT.md: upgrade at threshold, downgrade at threshold * 1.1 (55m for mesh->billboard, 220m for billboard->dot). Track previous LOD tier per agent.
**Warning signs:** Agents visibly switching between mesh and billboard rapidly.

### Pitfall 5: glTF Vertex Data Alignment
**What goes wrong:** Crash or visual corruption when uploading glTF mesh data to wgpu buffers.
**Why it happens:** glTF vertex data may have different stride/alignment than what wgpu expects.
**How to avoid:** Read positions/normals/indices from gltf accessors, repack into own #[repr(C)] Pod structs, then upload. Never pass raw glTF buffer data directly to wgpu.
**Warning signs:** Validation errors, garbled meshes, wrong vertex counts.

### Pitfall 6: Transition Animation Timing
**What goes wrong:** Lerp between 2D and 3D cameras produces invalid intermediate states (e.g., zero-length view direction).
**Why it happens:** Interpolating between orthographic and perspective matrices directly does not produce valid intermediate projections.
**How to avoid:** Interpolate camera *parameters* (position, target, up), not matrices. Blend fov_y from near-zero (orthographic approximation) to 45-deg perspective. Or simply crossfade the two rendered images.
**Warning signs:** Geometry distortion during transition, objects disappearing mid-transition.

## Code Examples

### glTF Model Loading (verified pattern from gltf crate docs)
```rust
// Source: https://docs.rs/gltf / https://github.com/gltf-rs/gltf
use gltf;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex3D {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

pub struct LoadedMesh {
    pub vertices: Vec<Vertex3D>,
    pub indices: Vec<u32>,
}

pub fn load_glb(path: &std::path::Path) -> Result<LoadedMesh, Box<dyn std::error::Error>> {
    let (document, buffers, _images) = gltf::import(path)?;
    let mesh = document.meshes().next().ok_or("no mesh")?;
    let primitive = mesh.primitives().next().ok_or("no primitive")?;

    let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));

    let positions: Vec<[f32; 3]> = reader.read_positions()
        .ok_or("no positions")?.collect();
    let normals: Vec<[f32; 3]> = reader.read_normals()
        .ok_or("no normals")?.collect();
    let indices: Vec<u32> = reader.read_indices()
        .ok_or("no indices")?.into_u32().collect();

    let vertices = positions.iter().zip(normals.iter())
        .map(|(p, n)| Vertex3D { position: *p, normal: *n })
        .collect();

    Ok(LoadedMesh { vertices, indices })
}
```

### Road Surface Polygon from Edge Geometry
```rust
fn edge_to_road_polygon(geometry: &[[f64; 2]], half_width: f64) -> Vec<[f32; 3]> {
    let mut left_side = Vec::new();
    let mut right_side = Vec::new();

    for i in 0..geometry.len() {
        // Compute tangent direction
        let (dx, dy) = if i + 1 < geometry.len() {
            (geometry[i+1][0] - geometry[i][0], geometry[i+1][1] - geometry[i][1])
        } else {
            (geometry[i][0] - geometry[i-1][0], geometry[i][1] - geometry[i-1][1])
        };
        let len = (dx*dx + dy*dy).sqrt().max(1e-6);
        let nx = -dy / len; // perpendicular
        let ny = dx / len;

        let p = geometry[i];
        // 3D coords: (x, 0.0, y) -- Y-up convention
        left_side.push([
            (p[0] + nx * half_width) as f32,
            0.0,
            (p[1] + ny * half_width) as f32,
        ]);
        right_side.push([
            (p[0] - nx * half_width) as f32,
            0.0,
            (p[1] - ny * half_width) as f32,
        ]);
    }

    // Generate triangle strip as triangle list
    let mut triangles = Vec::new();
    for i in 0..left_side.len() - 1 {
        // Triangle 1
        triangles.push(left_side[i]);
        triangles.push(right_side[i]);
        triangles.push(left_side[i + 1]);
        // Triangle 2
        triangles.push(right_side[i]);
        triangles.push(right_side[i + 1]);
        triangles.push(left_side[i + 1]);
    }
    triangles
}
```

### 3D Lit Mesh WGSL Shader
```wgsl
// mesh_3d.wgsl -- Lit 3D mesh with diffuse + ambient shading

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

struct LightingUniform {
    sun_direction: vec3<f32>,
    _pad0: f32,
    sun_color: vec3<f32>,
    _pad1: f32,
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> lighting: LightingUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

struct InstanceInput {
    @location(2) world_pos: vec3<f32>,
    @location(3) heading: f32,
    @location(4) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    let c = cos(inst.heading);
    let s = sin(inst.heading);
    // Rotate around Y axis
    let rotated = vec3<f32>(
        vert.position.x * c - vert.position.z * s,
        vert.position.y,
        vert.position.x * s + vert.position.z * c,
    );
    let world = vec4<f32>(rotated + inst.world_pos, 1.0);

    let rot_normal = vec3<f32>(
        vert.normal.x * c - vert.normal.z * s,
        vert.normal.y,
        vert.normal.x * s + vert.normal.z * c,
    );

    var out: VertexOutput;
    out.clip_pos = camera.view_proj * world;
    out.world_normal = rot_normal;
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let diffuse = max(dot(n, normalize(lighting.sun_direction)), 0.0);
    let lit = lighting.ambient_color * lighting.ambient_intensity * in.color.rgb
            + lighting.sun_color * diffuse * in.color.rgb;
    return vec4<f32>(lit, in.color.a);
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| wgpu 0.x depth texture | wgpu 27 depth texture (same API, stable) | wgpu 0.19+ | Minor naming changes; current code pattern is stable |
| glam 0.24 | glam 0.29 | 2024 | perspective_rh unchanged; Vec3/Mat4 API stable |
| gltf 0.x | gltf 1.4.1 | 2025 | Stable reader API; import() function unchanged |

**Deprecated/outdated:**
- wgpu 28 is available but the project is on wgpu 27 (per STATE.md blocker). Stay on wgpu 27 -- no 3D-relevant API differences.

## Open Questions

1. **Specific .glb model sources**
   - What we know: Kenney Car Kit (CC0, glTF format) has sedan, van, ambulance. Quaternius has LowPoly Cars (CC0, glTF). Both provide cars but motorbike models may be harder to find.
   - What's unclear: Exact .glb files for motorbike, bus, truck, pedestrian. Vertex counts per model.
   - Recommendation: Start with Kenney Car Kit for cars. Search Quaternius/OpenGameArt for motorbike. If no suitable motorbike .glb found, generate a simple programmatic mesh (narrow triangle prism ~100 vertices). Use simple capsule for pedestrians.

2. **Junction convex hull computation**
   - What we know: Need to fill junction areas with slightly lighter grey polygon. CONTEXT specifies "convex hull of approach endpoints."
   - What's unclear: Whether existing junction data from velos-net provides enough endpoint info, or if additional computation is needed.
   - Recommendation: Use JunctionData.turns entry/exit points as hull input. Compute convex hull with a simple Graham scan (small point count, 4-12 points per junction).

3. **Performance with 280K agents in 3D**
   - What we know: 2D already has performance regression at 8K agents (pending todo). 3D adds more draw calls (3 LOD tiers x N vehicle types).
   - What's unclear: Whether LOD culling will offset the extra pipeline cost.
   - Recommendation: Most agents will be dots (cheapest tier). Only agents within 200m get billboard, within 50m get mesh. At typical camera altitude, <1000 agents will be mesh tier. This should be manageable.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + naga validation (already in dev-deps) |
| Config file | Cargo.toml [dev-dependencies] naga = "27" |
| Quick run command | `cargo test -p velos-gpu --lib` |
| Full suite command | `cargo test -p velos-gpu` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| R3D-01 | OrbitCamera produces valid perspective matrix | unit | `cargo test -p velos-gpu orbit_camera -x` | Wave 0 |
| R3D-01 | Depth texture creation at various sizes | unit | `cargo test -p velos-gpu depth_texture -x` | Wave 0 |
| R3D-02 | Road polygon generation from edge geometry | unit | `cargo test -p velos-gpu road_surface -x` | Wave 0 |
| R3D-02 | Lane marking offset and dash pattern | unit | `cargo test -p velos-gpu lane_marking -x` | Wave 0 |
| R3D-03 | LOD distance classification with hysteresis | unit | `cargo test -p velos-gpu lod_classify -x` | Wave 0 |
| R3D-03 | glTF mesh loading produces valid vertex data | unit | `cargo test -p velos-gpu mesh_loader -x` | Wave 0 |
| R3D-03 | 3D instance struct size matches GPU stride | unit | `cargo test -p velos-gpu instance_3d_size -x` | Wave 0 |
| R3D-04 | View mode toggle preserves camera state | unit | `cargo test -p velos-gpu view_toggle -x` | Wave 0 |
| R3D-05 | Lighting keyframe interpolation at time boundaries | unit | `cargo test -p velos-gpu lighting_keyframe -x` | Wave 0 |
| R3D-05 | LightingUniform struct alignment for GPU | unit | `cargo test -p velos-gpu lighting_uniform_size -x` | Wave 0 |
| R3D-01 | WGSL shader validation (mesh_3d, billboard_3d, road_surface) | unit | `cargo test -p velos-gpu --test render_tests` | Wave 0 |
| R3D-01-05 | Full 3D render pipeline visual verification | manual-only | Run app, toggle to 3D, verify visually | N/A |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-gpu --lib -x`
- **Per wave merge:** `cargo test -p velos-gpu`
- **Phase gate:** Full suite green before /gsd:verify-work

### Wave 0 Gaps
- [ ] `crates/velos-gpu/src/orbit_camera.rs` -- OrbitCamera tests (matrix validity, pitch clamp, state mapping)
- [ ] `crates/velos-gpu/src/road_surface.rs` -- Road polygon generation tests
- [ ] `crates/velos-gpu/src/mesh_loader.rs` -- glTF loading tests (need test .glb fixture)
- [ ] `crates/velos-gpu/src/lighting.rs` -- Keyframe interpolation tests
- [ ] `crates/velos-gpu/src/renderer3d.rs` -- Instance struct size tests, LOD classification tests
- [ ] `assets/models/test_cube.glb` -- Minimal test fixture for mesh_loader tests
- [ ] WGSL validation tests for new shaders in existing `render_tests.rs`

## Sources

### Primary (HIGH confidence)
- wgpu 27 API -- project Cargo.toml and existing renderer.rs code patterns
- glam 0.29 API -- project dependency, Mat4::perspective_rh / look_at_rh
- bytemuck -- project dependency, Pod/Zeroable derive patterns
- earcutr 0.5 -- already used in map_tiles.rs for polygon triangulation
- velos-net RoadEdge struct -- `crates/velos-net/src/graph.rs` (geometry, lane_count fields)

### Secondary (MEDIUM confidence)
- [Learn wgpu - Depth Buffer](https://sotrh.github.io/learn-wgpu/beginner/tutorial8-depth/) -- Depth32Float pattern, DepthStencilState, RenderPassDepthStencilAttachment
- [wgpu DepthStencilState docs](https://wgpu.rs/doc/wgpu/struct.DepthStencilState.html) -- Official API reference
- [gltf crate](https://docs.rs/gltf) -- Reader API for positions/normals/indices from .glb
- [Kenney Car Kit](https://kenney-assets.itch.io/car-kit) -- CC0 vehicle .glb models
- [Quaternius vehicles](https://quaternius.com/) -- CC0 low-poly vehicle models

### Tertiary (LOW confidence)
- Specific vertex counts for Kenney/Quaternius models -- not verified, need to download and inspect

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all core libraries already in workspace except gltf (well-established crate)
- Architecture: HIGH -- dual renderer pattern clearly defined in CONTEXT.md, existing code patterns understood
- Pitfalls: HIGH -- common wgpu 3D pitfalls well-documented, coordinate system mapping straightforward
- glTF model sourcing: MEDIUM -- CC0 sources exist but specific model suitability (especially motorbike) needs validation

**Research date:** 2026-03-10
**Valid until:** 2026-04-10 (stable domain, no fast-moving dependencies)
