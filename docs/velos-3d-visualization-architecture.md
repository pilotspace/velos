# VELOS — 3D Map Visualization Architecture
## Displaying Simulation Traffic on a 3D City Map

**Companion to:** `rebuild-sumo-architecture-plan.md` (§9 Visualization Architecture)
**Date:** March 5, 2026

---

## Table of Contents

1. [Problem Statement & Requirements](#1-problem-statement)
2. [Architecture Overview — Dual Rendering Strategy](#2-architecture-overview)
3. [Coordinate Pipeline — Simulation → World → Screen](#3-coordinate-pipeline)
4. [Option A: Native wgpu Renderer (Desktop)](#4-native-wgpu-renderer)
5. [Option B: CesiumJS Web Renderer (Browser)](#5-cesiumjs-web-renderer)
6. [Option C: deck.gl High-Density Overlay](#6-deckgl-overlay)
7. [Data Streaming Protocol — VELOS → Visualization](#7-data-streaming)
8. [Level-of-Detail (LOD) Strategy](#8-lod-strategy)
9. [Instanced Rendering — 500K Agents at 60 FPS](#9-instanced-rendering)
10. [3D City Model Pipeline — CityGML → 3D Tiles](#10-city-model-pipeline)
11. [Heatmaps, Flow Arrows & Analytical Overlays](#11-analytical-overlays)
12. [Performance Budget & Benchmarks](#12-performance-budget)
13. [Technology Decision Matrix](#13-decision-matrix)
14. [Implementation Roadmap](#14-implementation-roadmap)
15. [Open-Source References](#15-oss-references)

---

## 1. Problem Statement & Requirements {#1-problem-statement}

VELOS simulates 500K+ agents (vehicles, pedestrians, cyclists, transit) at city scale. The visualization system must render these agents on a 3D city map in real-time, supporting both high-performance desktop analysis and shareable web-based dashboards.

### Visualization Requirements

| ID | Requirement | Target |
|----|------------|--------|
| VR-1 | Render 500K agents at ≥30 FPS (desktop native) | GPU instanced rendering, zero-copy from sim buffers |
| VR-2 | Render 50K agents at ≥30 FPS (web browser) | CesiumJS/deck.gl with spatial culling |
| VR-3 | 3D city model with buildings, terrain, roads | CityGML → 3D Tiles pipeline |
| VR-4 | Real-time agent animation (position, heading, speed) | ≤33ms update latency |
| VR-5 | Multi-scale LOD (city overview → intersection close-up) | 3-level LOD: dot → billboard → 3D mesh |
| VR-6 | Analytical overlays (heatmaps, flow arrows, KPIs) | Compute shader → fragment shader pipeline |
| VR-7 | Camera controls (orbit, fly-through, follow-agent) | Configurable camera modes |
| VR-8 | Time controls (play, pause, speed, scrub) | Decouple rendering from simulation clock |
| VR-9 | Agent type differentiation (color, shape, size) | Visual encoding per agent class |
| VR-10 | Click-to-inspect (select agent → show route, stats) | Ray-picking on instanced geometry |

---

## 2. Architecture Overview — Dual Rendering Strategy {#2-architecture-overview}

VELOS provides two rendering paths, both consuming the same ECS simulation state:

```
                          VELOS Simulation Engine (Rust)
                          ┌───────────────────────────────┐
                          │  ECS World (hecs)              │
                          │  ┌──────────────────────┐     │
                          │  │ Position[]  Velocity[]│     │
                          │  │ Heading[]   AgentType│     │
                          │  │ Route[]     Speed[]  │     │
                          │  └──────────┬───────────┘     │
                          └─────────────┼─────────────────┘
                                        │
                   ┌────────────────────┼────────────────────┐
                   │                    │                    │
                   ▼                    ▼                    ▼
        ┌──────────────────┐  ┌────────────────┐  ┌────────────────┐
        │ PATH A: Native   │  │ PATH B: Cesium │  │ PATH C: deck.gl│
        │ wgpu Renderer    │  │ JS Bridge      │  │ Overlay        │
        │                  │  │                │  │                │
        │ • Desktop app    │  │ • Web browser  │  │ • Web browser  │
        │ • Zero-copy GPU  │  │ • WebSocket    │  │ • WebSocket    │
        │ • 500K @ 60 FPS  │  │ • 50K @ 30 FPS │  │ • 100K @ 60 FPS│
        │ • Full fidelity  │  │ • 3D Tiles city│  │ • GPU layers   │
        │ • Dev/analysis   │  │ • Shareable URL│  │ • High density │
        └──────────────────┘  └────────────────┘  └────────────────┘
```

**Why three paths?**

- **Path A (wgpu native)**: Maximum performance for development and analysis. Simulation GPU buffers ARE render buffers — zero CPU-to-GPU copy. Only works on the machine running the simulation.
- **Path B (CesiumJS)**: Beautiful 3D globe/city context with Google Photorealistic 3D Tiles or CityGML. Shareable via URL. Best for stakeholder presentations and dashboards.
- **Path C (deck.gl)**: When you need 100K+ agents in the browser with GPU-accelerated rendering. TripsLayer for animated trajectories. Can overlay on Mapbox/MapLibre base maps.

All three paths share a single data source: the VELOS streaming API (§7).

---

## 3. Coordinate Pipeline — Simulation → World → Screen {#3-coordinate-pipeline}

VELOS agents store positions in **edge-local coordinates** (meters along edge + lateral offset). The visualization pipeline transforms these through four stages:

```
┌──────────────────────────────────────────────────────────────────────┐
│                    COORDINATE TRANSFORM PIPELINE                      │
│                                                                       │
│  Stage 1: Edge-Local → Network-Local (meters)                        │
│  ┌─────────────────────────────────────────────────┐                 │
│  │  Input:  (edge_id, distance_along, lateral_offset)                │
│  │  Method: Binary search on EdgeGeometry.cumulative_lengths (C5 fix)│
│  │  Output: (x_meters, y_meters) in simulation coordinate space     │
│  │  Cost:   O(log S) per agent, where S = segments per edge         │
│  │                                                                    │
│  │  // Rust (velos-net/src/geometry.rs)                               │
│  │  fn edge_local_to_network(                                        │
│  │      edge: &EdgeGeometry,                                         │
│  │      distance: f32,                                               │
│  │      lateral: f32                                                 │
│  │  ) -> Vec2 {                                                      │
│  │      let seg = edge.cumulative_lengths                            │
│  │          .binary_search_by(|d| d.partial_cmp(&distance).unwrap())│
│  │          .unwrap_or_else(|i| i.saturating_sub(1));               │
│  │      let seg_start = edge.points[seg];                            │
│  │      let seg_end = edge.points[seg + 1];                         │
│  │      let seg_frac = (distance - edge.cumulative_lengths[seg])     │
│  │          / (edge.cumulative_lengths[seg+1]                        │
│  │             - edge.cumulative_lengths[seg]);                      │
│  │      let along = seg_start.lerp(seg_end, seg_frac);              │
│  │      let normal = (seg_end - seg_start).perp().normalize();      │
│  │      along + normal * lateral                                     │
│  │  }                                                                │
│  └─────────────────────────────────────────────────┘                 │
│                          │                                            │
│                          ▼                                            │
│  Stage 2: Network-Local → WGS84 (latitude, longitude, altitude)      │
│  ┌─────────────────────────────────────────────────┐                 │
│  │  Method: Affine transform using network origin + projection       │
│  │                                                                    │
│  │  // Network stores its geographic origin                          │
│  │  struct NetworkOrigin {                                           │
│  │      lat: f64,   // WGS84 latitude of (0,0)                      │
│  │      lon: f64,   // WGS84 longitude of (0,0)                     │
│  │      proj: Projection,  // UTM zone or local tangent plane       │
│  │  }                                                                │
│  │                                                                    │
│  │  fn network_to_wgs84(pos: Vec2, origin: &NetworkOrigin) -> LatLon│
│  │  {                                                                │
│  │      // Use proj crate for accurate UTM → WGS84                  │
│  │      let (lat, lon) = origin.proj.inverse(                        │
│  │          origin.easting + pos.x as f64,                           │
│  │          origin.northing + pos.y as f64                           │
│  │      );                                                           │
│  │      LatLon { lat, lon }                                          │
│  │  }                                                                │
│  └─────────────────────────────────────────────────┘                 │
│                          │                                            │
│                          ▼                                            │
│  Stage 3: WGS84 → Cartesian (ECEF or local ENU)                     │
│  ┌─────────────────────────────────────────────────┐                 │
│  │  For CesiumJS: Cesium.Cartesian3.fromDegrees(lon, lat, alt)      │
│  │  For wgpu:     ENU (East-North-Up) local tangent plane           │
│  │  For deck.gl:  [longitude, latitude, altitude] array             │
│  └─────────────────────────────────────────────────┘                 │
│                          │                                            │
│                          ▼                                            │
│  Stage 4: Cartesian → Screen (clip space → viewport)                 │
│  ┌─────────────────────────────────────────────────┐                 │
│  │  Standard Model-View-Projection pipeline                          │
│  │  wgpu:     vertex shader applies MVP matrix                       │
│  │  CesiumJS: internal camera projection                             │
│  │  deck.gl:  WebMercatorViewport projection                         │
│  └─────────────────────────────────────────────────┘                 │
└──────────────────────────────────────────────────────────────────────┘
```

### GPU-Accelerated Batch Transform

For the native renderer, coordinate transforms run on the GPU as a compute shader before the render pass:

```wgsl
// coordinate_transform.wgsl — runs once per frame before rendering
// Transforms all agent positions from edge-local to world coordinates

struct AgentSim {
    edge_id: u32,
    distance_along: f32,
    lateral_offset: f32,
    heading: f32,
    speed: f32,
    agent_type: u32,
    parity: u32,
    _pad: u32,
};

struct AgentRender {
    world_x: f32,
    world_y: f32,
    world_z: f32,
    heading: f32,
    speed: f32,
    agent_type: u32,
    color: u32,          // packed RGBA
    lod_level: u32,      // computed from camera distance
};

struct EdgeSegment {
    x0: f32, y0: f32,
    x1: f32, y1: f32,
    cumulative_start: f32,
    cumulative_end: f32,
};

@group(0) @binding(0) var<storage, read> agents_sim: array<AgentSim>;
@group(0) @binding(1) var<storage, read_write> agents_render: array<AgentRender>;
@group(0) @binding(2) var<storage, read> edge_segments: array<EdgeSegment>;
@group(0) @binding(3) var<storage, read> edge_segment_offsets: array<u32>;  // per-edge offset into segments
@group(0) @binding(4) var<uniform> camera: CameraUniforms;

@compute @workgroup_size(256)
fn transform_positions(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if (idx >= arrayLength(&agents_sim)) { return; }

    let agent = agents_sim[idx];
    let seg_start = edge_segment_offsets[agent.edge_id];
    let seg_end = edge_segment_offsets[agent.edge_id + 1u];

    // Binary search for segment containing agent's distance_along
    var lo = seg_start;
    var hi = seg_end;
    while (lo < hi) {
        let mid = (lo + hi) / 2u;
        if (edge_segments[mid].cumulative_end < agent.distance_along) {
            lo = mid + 1u;
        } else {
            hi = mid;
        }
    }

    let seg = edge_segments[lo];
    let seg_len = seg.cumulative_end - seg.cumulative_start;
    let frac = (agent.distance_along - seg.cumulative_start) / max(seg_len, 0.001);

    // Interpolate along segment
    let px = mix(seg.x0, seg.x1, frac);
    let py = mix(seg.y0, seg.y1, frac);

    // Apply lateral offset (perpendicular to segment direction)
    let dx = seg.x1 - seg.x0;
    let dy = seg.y1 - seg.y0;
    let len = sqrt(dx * dx + dy * dy);
    let nx = -dy / max(len, 0.001);  // normal x
    let ny = dx / max(len, 0.001);   // normal y

    let world_x = px + nx * agent.lateral_offset;
    let world_y = py + ny * agent.lateral_offset;

    // Camera distance for LOD
    let cam_dist = length(vec2f(world_x - camera.position.x,
                                world_y - camera.position.y));
    var lod: u32 = 0u;  // full detail
    if (cam_dist > 500.0) { lod = 1u; }   // billboard
    if (cam_dist > 2000.0) { lod = 2u; }  // dot
    if (cam_dist > 5000.0) { lod = 3u; }  // invisible (culled)

    // Agent type → color mapping
    var color: u32 = 0xFF3366FFu;  // blue = car (default)
    switch (agent.agent_type) {
        case 1u: { color = 0xFF33FF66u; }  // green = pedestrian
        case 2u: { color = 0xFFFF9933u; }  // orange = cyclist
        case 3u: { color = 0xFFFF3333u; }  // red = bus/transit
        case 4u: { color = 0xFFFFFF33u; }  // yellow = emergency
        default: {}
    }

    agents_render[idx] = AgentRender(
        world_x, world_y, 0.0,  // z from terrain later
        agent.heading, agent.speed,
        agent.agent_type, color, lod
    );
}
```

### Performance

| Agent Count | Transform Time (GPU) | Notes |
|------------|---------------------|-------|
| 100K | ~0.1 ms | Single dispatch |
| 500K | ~0.5 ms | Single dispatch on RTX 4090 |
| 1M | ~1.0 ms | Within frame budget |

---

## 4. Option A: Native wgpu Renderer (Desktop) {#4-native-wgpu-renderer}

The native renderer achieves maximum performance by sharing GPU buffers between simulation and rendering — no CPU round-trip.

### Render Pipeline Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                    NATIVE wgpu RENDER PIPELINE                      │
│                                                                     │
│  Frame N:                                                           │
│  ┌──────────────────────────────────────────────────────────┐      │
│  │ COMPUTE PASSES (simulation + transform)                   │      │
│  │                                                           │      │
│  │  Pass 0: Simulation step (EVEN agents) ─── 1.5ms         │      │
│  │  Pass 1: Simulation step (ODD agents)  ─── 1.5ms         │      │
│  │  Pass 2: Collision correction          ─── 0.3ms         │      │
│  │  Pass 3: Coordinate transform          ─── 0.5ms         │      │
│  │  Pass 4: Frustum cull + LOD assign     ─── 0.2ms         │      │
│  │  Pass 5: Indirect draw count           ─── 0.1ms         │      │
│  └──────────────────────────────────────────────────────────┘      │
│                          │                                          │
│                          ▼                                          │
│  ┌──────────────────────────────────────────────────────────┐      │
│  │ RENDER PASSES                                             │      │
│  │                                                           │      │
│  │  Pass 1: Terrain + Buildings (static, cached)             │      │
│  │          ├── CityGML → 3D Tiles → GPU mesh cache         │      │
│  │          ├── Depth pre-pass for occlusion                 │      │
│  │          └── Texture atlas for building facades    ~2ms   │      │
│  │                                                           │      │
│  │  Pass 2: Road Network (static, cached)                    │      │
│  │          ├── Road surface as textured quads               │      │
│  │          ├── Lane markings from edge geometry             │      │
│  │          └── Traffic signals as small meshes       ~1ms   │      │
│  │                                                           │      │
│  │  Pass 3: Agents — INSTANCED DRAW (dynamic)               │      │
│  │          ├── LOD 0 (<500m):  3D mesh per agent type       │      │
│  │          │   Car: 200-triangle sedan/truck/bus mesh       │      │
│  │          │   Ped: 50-triangle capsule + color             │      │
│  │          │   Cyclist: 100-triangle bike+rider             │      │
│  │          ├── LOD 1 (500m-2km): Billboard sprites          │      │
│  │          │   Camera-facing quads, 16x16 px icons          │      │
│  │          ├── LOD 2 (2km+): Colored dots                   │      │
│  │          │   Point primitives, 2-4 px radius              │      │
│  │          └── GPU indirect draw (no CPU involvement) ~3ms  │      │
│  │                                                           │      │
│  │  Pass 4: Overlays (heatmaps, flow arrows, KPIs)          │      │
│  │          ├── Compute → texture for density heatmap        │      │
│  │          ├── Instanced arrows for flow direction          │      │
│  │          └── UI overlay (ImGui via egui)           ~1ms   │      │
│  └──────────────────────────────────────────────────────────┘      │
│                                                                     │
│  TOTAL FRAME TIME: ~10ms (100 FPS) for 500K agents                 │
└────────────────────────────────────────────────────────────────────┘
```

### Key Implementation: Zero-Copy Sim-to-Render

The critical optimization is that simulation GPU buffers feed directly into the render pipeline:

```rust
// velos-gpu/src/renderer.rs

pub struct VelosRenderer {
    // Simulation buffers (owned by velos-gpu compute pipeline)
    sim_buffer_read: wgpu::Buffer,   // current frame positions
    sim_buffer_write: wgpu::Buffer,  // next frame (being computed)

    // Render buffer (output of coordinate transform compute shader)
    render_buffer: wgpu::Buffer,     // AgentRender[] for instanced draw

    // Indirect draw buffer (written by GPU cull pass)
    indirect_buffer: wgpu::Buffer,   // DrawIndirect struct

    // Static geometry (loaded once)
    terrain_mesh: GpuMesh,
    road_mesh: GpuMesh,
    vehicle_mesh_lod0: GpuMesh,      // ~200 triangles
    vehicle_billboard_lod1: GpuMesh, // 2-triangle quad
    pedestrian_mesh: GpuMesh,
    cyclist_mesh: GpuMesh,
}

impl VelosRenderer {
    pub fn render_frame(&self, encoder: &mut wgpu::CommandEncoder) {
        // STEP 1: Coordinate transform (compute)
        // Input: sim_buffer_read (from simulation)
        // Output: render_buffer (world positions + LOD + color)
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.transform_pipeline);
            pass.set_bind_group(0, &self.transform_bind_group, &[]);
            pass.dispatch_workgroups(
                (self.agent_count + 255) / 256, 1, 1
            );
        }

        // STEP 2: Frustum cull + indirect draw count (compute)
        // Input: render_buffer
        // Output: indirect_buffer.instance_count, visible_buffer
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.cull_pipeline);
            pass.set_bind_group(0, &self.cull_bind_group, &[]);
            pass.dispatch_workgroups(
                (self.agent_count + 255) / 256, 1, 1
            );
        }

        // STEP 3: Render passes
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(/* depth buffer */),
                ..Default::default()
            });

            // Pass 1: Terrain + buildings (cached static mesh)
            pass.set_pipeline(&self.terrain_pipeline);
            pass.set_bind_group(0, &self.terrain_bind_group, &[]);
            pass.draw_indexed(0..self.terrain_mesh.index_count, 0, 0..1);

            // Pass 2: Road network
            pass.set_pipeline(&self.road_pipeline);
            pass.set_bind_group(0, &self.road_bind_group, &[]);
            pass.draw_indexed(0..self.road_mesh.index_count, 0, 0..1);

            // Pass 3: Agents — GPU INDIRECT INSTANCED DRAW
            // The GPU itself determined how many visible agents to draw
            pass.set_pipeline(&self.agent_pipeline_lod0);
            pass.set_vertex_buffer(0, self.vehicle_mesh_lod0.vertex_buf.slice(..));
            pass.set_vertex_buffer(1, self.render_buffer.slice(..));  // instance data
            pass.set_index_buffer(
                self.vehicle_mesh_lod0.index_buf.slice(..),
                wgpu::IndexFormat::Uint16
            );
            // GPU-driven: indirect_buffer contains instance count from cull pass
            pass.draw_indexed_indirect(&self.indirect_buffer, 0);
        }
    }
}
```

### Vertex Shader for Instanced Agents

```wgsl
// agent_render.wgsl

struct AgentInstance {
    @location(5) world_x: f32,
    @location(6) world_y: f32,
    @location(7) world_z: f32,
    @location(8) heading: f32,
    @location(9) speed: f32,
    @location(10) agent_type: u32,
    @location(11) color: u32,
    @location(12) lod_level: u32,
};

struct VertexInput {
    @location(0) position: vec3f,   // mesh-local vertex position
    @location(1) normal: vec3f,
    @location(2) uv: vec2f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) world_normal: vec3f,
    @location(1) color: vec4f,
    @location(2) speed_factor: f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniforms;

@vertex
fn vs_main(vertex: VertexInput, instance: AgentInstance) -> VertexOutput {
    // Rotation matrix from heading angle
    let cos_h = cos(instance.heading);
    let sin_h = sin(instance.heading);

    // Rotate mesh to face heading direction
    let rotated = vec3f(
        vertex.position.x * cos_h - vertex.position.z * sin_h,
        vertex.position.y,
        vertex.position.x * sin_h + vertex.position.z * cos_h
    );

    // Scale based on agent type
    var scale = 1.0;
    switch (instance.agent_type) {
        case 0u: { scale = 4.5; }   // car: ~4.5m
        case 1u: { scale = 0.5; }   // pedestrian: ~0.5m
        case 2u: { scale = 1.8; }   // cyclist: ~1.8m
        case 3u: { scale = 12.0; }  // bus: ~12m
        case 4u: { scale = 5.0; }   // emergency: ~5m
        default: { scale = 4.0; }
    }

    let world_pos = vec3f(
        instance.world_x + rotated.x * scale,
        instance.world_z + rotated.y * scale,  // Y-up in render
        instance.world_y + rotated.z * scale
    );

    // Unpack color from u32
    let r = f32((instance.color >> 0u) & 0xFFu) / 255.0;
    let g = f32((instance.color >> 8u) & 0xFFu) / 255.0;
    let b = f32((instance.color >> 16u) & 0xFFu) / 255.0;
    let a = f32((instance.color >> 24u) & 0xFFu) / 255.0;

    // Speed-based color modulation (brake lights = red tint at low speed)
    let speed_factor = clamp(instance.speed / 15.0, 0.0, 1.0);

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4f(world_pos, 1.0);
    out.world_normal = rotated;
    out.color = vec4f(r, g, b, a);
    out.speed_factor = speed_factor;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Simple directional lighting
    let light_dir = normalize(vec3f(0.5, 1.0, 0.3));
    let ndotl = max(dot(normalize(in.world_normal), light_dir), 0.2);

    // Brake light effect: red tint when speed < 2 m/s
    var final_color = in.color.rgb * ndotl;
    if (in.speed_factor < 0.15) {
        final_color = mix(final_color, vec3f(1.0, 0.1, 0.1), 0.4);
    }

    return vec4f(final_color, in.color.a);
}
```

---

## 5. Option B: CesiumJS Web Renderer (Browser) {#5-cesiumjs-web-renderer}

CesiumJS provides the richest 3D geospatial context — globe, terrain, 3D Tiles buildings, and atmospheric effects. VELOS streams agent positions to CesiumJS via WebSocket.

### Architecture

```
┌─────────────────────┐          ┌────────────────────────────────────┐
│  VELOS Engine (Rust) │          │  Browser (CesiumJS)                │
│                      │          │                                    │
│  Simulation step     │ WebSocket│  ┌──────────────────────────────┐ │
│  ─── 33ms cycle ───→ │ ────────→│  │ 3D Tiles City Model         │ │
│                      │ FlatBuf  │  │ (CityGML / Google Photreal)  │ │
│  Spatial tiling:     │ binary   │  ├──────────────────────────────┤ │
│  256×256 grid        │          │  │ Terrain (Cesium World Terrain│ │
│  Client subscribes   │          │  │ or custom DEM tiles)         │ │
│  visible tiles only  │          │  ├──────────────────────────────┤ │
│                      │          │  │ Agent Layer:                 │ │
│  LOD per agent:      │          │  │  LOD 0: CZML Entity w/ model│ │
│  >2km: density color │          │  │  LOD 1: Billboard (sprite)  │ │
│  500m-2km: billboard │          │  │  LOD 2: Point primitive      │ │
│  <500m: 3D model     │          │  ├──────────────────────────────┤ │
│                      │          │  │ Overlay Layer:               │ │
│                      │          │  │  Heatmap (deck.gl)           │ │
│                      │          │  │  Flow arrows (Primitive)     │ │
│                      │          │  │  Signal status (billboards)  │ │
│                      │          │  └──────────────────────────────┘ │
└─────────────────────┘          └────────────────────────────────────┘
```

### CesiumJS Integration Code

```javascript
// velos-cesium-client/src/VelosViewer.js

import * as Cesium from 'cesium';

class VelosViewer {
    constructor(containerId, velosServerUrl) {
        // Initialize Cesium with 3D Tiles city model
        this.viewer = new Cesium.Viewer(containerId, {
            terrain: Cesium.Terrain.fromWorldTerrain(),
            timeline: false,
            animation: false,
            baseLayerPicker: false,
        });

        // Add Google Photorealistic 3D Tiles (or CityGML-derived tiles)
        this.addCityModel();

        // Agent entity pool (pre-allocated for performance)
        this.agentEntities = new Map();        // id → Cesium.Entity
        this.agentPrimitives = null;           // PointPrimitiveCollection for LOD 2
        this.billboardCollection = null;       // BillboardCollection for LOD 1

        // Connect to VELOS WebSocket
        this.connectToVelos(velosServerUrl);
    }

    async addCityModel() {
        // Option 1: Google Photorealistic 3D Tiles
        try {
            const tileset = await Cesium.Cesium3DTileset.fromUrl(
                `https://tile.googleapis.com/v1/3dtiles/root.json?key=${API_KEY}`,
                {
                    maximumScreenSpaceError: 8,     // quality vs performance
                    maximumMemoryUsage: 512,         // MB
                    skipLevelOfDetail: true,
                    skipScreenSpaceErrorFactor: 16,
                    dynamicScreenSpaceError: true,
                }
            );
            this.viewer.scene.primitives.add(tileset);
        } catch (e) {
            console.warn('Google 3D Tiles unavailable, falling back to CityGML');

            // Option 2: Self-hosted CityGML → 3D Tiles
            const tileset = await Cesium.Cesium3DTileset.fromUrl(
                '/tiles/city/tileset.json'
            );
            this.viewer.scene.primitives.add(tileset);
        }
    }

    connectToVelos(serverUrl) {
        this.ws = new WebSocket(serverUrl);
        this.ws.binaryType = 'arraybuffer';

        // Send viewport subscription on camera move
        this.viewer.camera.changed.addEventListener(() => {
            this.sendViewportUpdate();
        });

        // Handle incoming agent updates
        this.ws.onmessage = (event) => {
            if (event.data instanceof ArrayBuffer) {
                this.handleBinaryUpdate(event.data);
            }
        };

        this.ws.onopen = () => {
            this.sendViewportUpdate();
        };
    }

    sendViewportUpdate() {
        const rect = this.viewer.camera.computeViewRectangle();
        if (!rect) return;

        // Tell VELOS which spatial tiles we need
        const viewport = {
            type: 'viewport',
            west: Cesium.Math.toDegrees(rect.west),
            south: Cesium.Math.toDegrees(rect.south),
            east: Cesium.Math.toDegrees(rect.east),
            north: Cesium.Math.toDegrees(rect.north),
            cameraHeight: this.viewer.camera.positionCartographic.height,
        };
        this.ws.send(JSON.stringify(viewport));
    }

    handleBinaryUpdate(buffer) {
        // Decode FlatBuffers frame
        const frame = decodeFlatBuffersFrame(buffer);

        // Process agent updates by LOD level
        for (const agent of frame.agents) {
            const position = Cesium.Cartesian3.fromDegrees(
                agent.longitude,
                agent.latitude,
                agent.altitude || 0
            );

            if (agent.lod === 0) {
                // Full 3D model entity
                this.updateModelEntity(agent.id, position, agent);
            } else if (agent.lod === 1) {
                // Billboard sprite
                this.updateBillboard(agent.id, position, agent);
            } else if (agent.lod === 2) {
                // Colored dot
                this.updatePoint(agent.id, position, agent);
            }
        }

        // Remove agents no longer in viewport
        for (const removedId of frame.removed) {
            this.removeAgent(removedId);
        }
    }

    updateModelEntity(id, position, agent) {
        let entity = this.agentEntities.get(id);
        if (!entity) {
            // Model URIs per agent type
            const modelUri = {
                car: '/models/sedan.glb',
                bus: '/models/bus.glb',
                pedestrian: '/models/pedestrian.glb',
                cyclist: '/models/cyclist.glb',
                emergency: '/models/ambulance.glb',
            }[agent.type] || '/models/sedan.glb';

            entity = this.viewer.entities.add({
                id: `agent_${id}`,
                position: position,
                orientation: Cesium.Transforms.headingPitchRollQuaternion(
                    position,
                    new Cesium.HeadingPitchRoll(agent.heading, 0, 0)
                ),
                model: {
                    uri: modelUri,
                    minimumPixelSize: 24,
                    maximumScale: 20,
                    color: this.speedToColor(agent.speed, agent.type),
                    colorBlendMode: Cesium.ColorBlendMode.HIGHLIGHT,
                    silhouetteColor: Cesium.Color.WHITE,
                    silhouetteSize: 1.0,
                },
                description: `Agent ${id} | ${agent.type} | ${agent.speed.toFixed(1)} m/s`,
            });
            this.agentEntities.set(id, entity);
        } else {
            // Update existing entity (position + orientation)
            entity.position = position;
            entity.orientation = Cesium.Transforms.headingPitchRollQuaternion(
                position,
                new Cesium.HeadingPitchRoll(agent.heading, 0, 0)
            );
            entity.model.color = this.speedToColor(agent.speed, agent.type);
        }
    }

    speedToColor(speed, agentType) {
        // Color ramp: green (flowing) → yellow (slow) → red (stopped)
        const maxSpeed = agentType === 'pedestrian' ? 2.0 : 15.0;
        const ratio = Math.min(speed / maxSpeed, 1.0);

        if (ratio > 0.6) return Cesium.Color.fromCssColorString('#33cc33');
        if (ratio > 0.3) return Cesium.Color.fromCssColorString('#ffcc00');
        return Cesium.Color.fromCssColorString('#ff3333');
    }
}
```

### CesiumJS Performance Optimization

| Optimization | Impact | Implementation |
|-------------|--------|----------------|
| Entity pooling | Avoid GC churn from add/remove | Pre-allocate entity pool, recycle on remove |
| BillboardCollection | 10x faster than individual entities | Batch all LOD 1 agents into single collection |
| PointPrimitiveCollection | Handle 100K+ dots efficiently | LOD 2 agents as points, not entities |
| RequestRenderMode | Skip frames when nothing changes | `viewer.scene.requestRenderMode = true` |
| 3D Tiles caching | Avoid re-fetching static city | `maximumMemoryUsage: 512` MB cache |
| Viewport culling (server) | Reduce WebSocket bandwidth by 90% | Only send agents in camera view |

---

## 6. Option C: deck.gl High-Density Overlay {#6-deckgl-overlay}

deck.gl excels at rendering massive point/path datasets with GPU acceleration. Best for analytical views (heatmaps, flow visualization, trajectory replay).

### deck.gl Layer Stack

```javascript
// velos-deckgl-client/src/VelosDeckGL.js

import { Deck } from '@deck.gl/core';
import { MapboxOverlay } from '@deck.gl/mapbox';
import { ScatterplotLayer, IconLayer, PathLayer } from '@deck.gl/layers';
import { TripsLayer } from '@deck.gl/geo-layers';
import { HeatmapLayer, HexagonLayer } from '@deck.gl/aggregation-layers';
import maplibregl from 'maplibre-gl';

class VelosDeckGL {
    constructor(containerId, velosWsUrl) {
        // MapLibre base map with 3D terrain
        this.map = new maplibregl.Map({
            container: containerId,
            style: 'https://demotiles.maplibre.org/style.json',
            center: [11.58, 48.15],  // Munich
            zoom: 13,
            pitch: 45,
            bearing: -17,
            terrain: { source: 'terrain', exaggeration: 1.0 },
        });

        // deck.gl overlay
        this.deckOverlay = new MapboxOverlay({
            interleaved: true,  // proper depth testing with 3D buildings
            layers: [],
        });
        this.map.addControl(this.deckOverlay);

        // Data state
        this.agentData = [];      // current agent positions
        this.trajectoryData = []; // historical paths for TripsLayer
        this.heatmapData = [];    // density data

        this.connectToVelos(velosWsUrl);
    }

    updateLayers() {
        this.deckOverlay.setProps({
            layers: [
                // Layer 1: 3D buildings from vector tiles
                // (handled by MapLibre fill-extrusion layer)

                // Layer 2: Agent positions — ScatterplotLayer for speed
                new ScatterplotLayer({
                    id: 'agents-scatter',
                    data: this.agentData,
                    getPosition: d => [d.lon, d.lat, d.alt || 0],
                    getRadius: d => this.agentRadius(d.type),
                    getFillColor: d => this.speedColor(d.speed, d.type),
                    radiusMinPixels: 2,
                    radiusMaxPixels: 10,
                    pickable: true,
                    onClick: info => this.onAgentClick(info),
                    updateTriggers: {
                        getPosition: this.frameCounter,
                        getFillColor: this.frameCounter,
                    },
                }),

                // Layer 3: Animated trip trails — TripsLayer
                new TripsLayer({
                    id: 'trips',
                    data: this.trajectoryData,
                    getPath: d => d.path,           // [[lon, lat], ...]
                    getTimestamps: d => d.timestamps,
                    getColor: d => d.color || [253, 128, 93],
                    widthMinPixels: 2,
                    trailLength: 300,               // 300 sim-seconds of trail
                    currentTime: this.simTime,
                }),

                // Layer 4: Density heatmap overlay
                new HeatmapLayer({
                    id: 'heatmap',
                    data: this.heatmapData,
                    getPosition: d => [d.lon, d.lat],
                    getWeight: d => d.density,
                    radiusPixels: 30,
                    intensity: 1,
                    threshold: 0.05,
                    colorRange: [
                        [1, 152, 189],    // low density: blue
                        [73, 227, 206],
                        [216, 254, 181],
                        [254, 237, 177],
                        [254, 173, 84],
                        [209, 55, 78],    // high density: red
                    ],
                }),

                // Layer 5: Flow arrows showing traffic direction
                new PathLayer({
                    id: 'flow-arrows',
                    data: this.flowData,
                    getPath: d => d.path,
                    getColor: d => this.flowColor(d.speed),
                    getWidth: d => Math.max(d.volume / 100, 2),
                    widthMinPixels: 1,
                    widthMaxPixels: 10,
                    billboard: false,
                }),
            ],
        });
    }

    speedColor(speed, agentType) {
        // Returns RGBA array based on speed ratio
        const maxSpeed = agentType === 'pedestrian' ? 2.0 :
                         agentType === 'cyclist' ? 6.0 : 15.0;
        const ratio = Math.min(speed / maxSpeed, 1.0);

        if (ratio > 0.7) return [51, 204, 51, 200];    // green
        if (ratio > 0.4) return [255, 204, 0, 200];     // yellow
        if (ratio > 0.1) return [255, 102, 0, 200];     // orange
        return [255, 51, 51, 200];                        // red (stopped)
    }

    agentRadius(type) {
        switch (type) {
            case 'car': return 3;
            case 'bus': return 5;
            case 'pedestrian': return 1;
            case 'cyclist': return 2;
            case 'emergency': return 4;
            default: return 3;
        }
    }
}
```

### deck.gl Performance Characteristics

| Layer | Max Entities (60 FPS) | GPU Memory | Notes |
|-------|----------------------|------------|-------|
| ScatterplotLayer | ~1M points | ~32 MB | Binary data attributes |
| TripsLayer | ~50K paths | ~100 MB | Depends on path length |
| HeatmapLayer | ~500K points | ~64 MB | Aggregation on GPU |
| IconLayer | ~100K icons | ~48 MB | Texture atlas optimization |
| HexagonLayer | ~1M points | ~32 MB | GPU aggregation |

---

## 7. Data Streaming Protocol — VELOS → Visualization {#7-data-streaming}

### Protocol Design (extends §9 M9 from architecture plan)

```
┌─────────────────────────────────────────────────────────────────────┐
│                   VELOS VISUALIZATION STREAMING PROTOCOL             │
│                                                                      │
│  Transport: WebSocket (wss://) with binary FlatBuffers frames        │
│                                                                      │
│  ┌──── Client → Server Messages ─────────────────────────────────┐  │
│  │                                                                │  │
│  │  ViewportSubscription {                                        │  │
│  │    west: f64, south: f64, east: f64, north: f64,  // WGS84    │  │
│  │    camera_height: f64,     // meters above ground              │  │
│  │    max_agents: u32,        // client-side budget               │  │
│  │    lod_override: u8,       // 0=auto, 1=force_full, 2=dots    │  │
│  │    subscribe_heatmap: bool,                                    │  │
│  │    subscribe_signals: bool,                                    │  │
│  │    subscribe_events: bool,                                     │  │
│  │  }                                                             │  │
│  │                                                                │  │
│  │  AgentQuery {                                                  │  │
│  │    agent_id: u32,          // request detailed agent info      │  │
│  │  }                                                             │  │
│  │                                                                │  │
│  │  TimeControl {                                                 │  │
│  │    action: enum { Play, Pause, SetSpeed(f32), Seek(f64) }     │  │
│  │  }                                                             │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌──── Server → Client Messages ─────────────────────────────────┐  │
│  │                                                                │  │
│  │  AgentFrame (binary FlatBuffers, sent every 33ms):             │  │
│  │  {                                                             │  │
│  │    sim_time: f64,                                              │  │
│  │    frame_id: u64,                                              │  │
│  │    agents_full: [        // LOD 0: full state (~20 bytes each) │  │
│  │      { id, lat, lon, alt, heading, speed, type, route_pct }   │  │
│  │    ],                                                          │  │
│  │    agents_compact: [     // LOD 1: position only (~8 bytes)    │  │
│  │      { id, lat_u16, lon_u16, type_u8 }                        │  │
│  │    ],                                                          │  │
│  │    agents_dots: [        // LOD 2: tile-relative (~4 bytes)    │  │
│  │      { x_u16, y_u16 }   // relative to tile origin            │  │
│  │    ],                                                          │  │
│  │    removed_ids: [u32],   // agents exited viewport             │  │
│  │    delta_only: bool,     // if true, only changed agents sent  │  │
│  │  }                                                             │  │
│  │                                                                │  │
│  │  HeatmapFrame (sent every 1s):                                 │  │
│  │  {                                                             │  │
│  │    grid_resolution: u32,  // e.g., 100×100                    │  │
│  │    bounds: [west, south, east, north],                         │  │
│  │    density: [f32],        // flattened grid values              │  │
│  │    avg_speed: [f32],      // average speed per cell            │  │
│  │  }                                                             │  │
│  │                                                                │  │
│  │  SignalFrame (sent on change):                                 │  │
│  │  {                                                             │  │
│  │    signals: [{ id, phase, time_to_change, lat, lon }]         │  │
│  │  }                                                             │  │
│  │                                                                │  │
│  │  EventFrame (sent on occurrence):                              │  │
│  │  {                                                             │  │
│  │    events: [{ type, severity, lat, lon, description, time }]  │  │
│  │  }                                                             │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  Bandwidth Budget:                                                   │
│  ┌────────────────────────────────────────────────────────────┐     │
│  │  50K agents in viewport (typical city-wide view):           │     │
│  │    LOD 0 (close):    500 agents × 20 bytes = 10 KB         │     │
│  │    LOD 1 (medium): 5,000 agents × 8 bytes  = 40 KB         │     │
│  │    LOD 2 (far):   44,500 agents × 4 bytes  = 178 KB        │     │
│  │    Overhead:                                   12 KB        │     │
│  │    TOTAL per frame:                          ~240 KB        │     │
│  │    × 30 FPS:                                ~7.2 MB/s       │     │
│  │                                                              │     │
│  │  With delta compression (only moved >1m):                    │     │
│  │    ~30% of agents move per frame → ~2.2 MB/s                │     │
│  │                                                              │     │
│  │  With spatial tile culling (subscribe to visible tiles):     │     │
│  │    Typical: 8-16 tiles out of 65,536 → ~0.5 MB/s           │     │
│  └────────────────────────────────────────────────────────────┘     │
└─────────────────────────────────────────────────────────────────────┘
```

### FlatBuffers Schema

```flatbuffers
// velos_viz.fbs — FlatBuffers schema for visualization streaming

namespace velos.viz;

enum AgentType : byte { Car = 0, Pedestrian = 1, Cyclist = 2, Bus = 3, Emergency = 4 }

table AgentFull {
    id: uint32;
    latitude: float64;
    longitude: float64;
    altitude: float32;
    heading: float32;       // radians
    speed: float32;         // m/s
    agent_type: AgentType;
    route_progress: float32; // 0.0 - 1.0
}

struct AgentCompact {
    id: uint32;
    lat_offset: uint16;     // offset from tile origin, ~0.3m resolution
    lon_offset: uint16;
    type_speed: uint8;      // upper 4 bits: type, lower 4: speed bucket
}

struct AgentDot {
    x: uint16;              // tile-relative x
    y: uint16;              // tile-relative y
}

table AgentFrame {
    sim_time: float64;
    frame_id: uint64;
    agents_full: [AgentFull];
    agents_compact: [AgentCompact];
    agents_dots: [AgentDot];
    removed_ids: [uint32];
    delta_only: bool;
}

table HeatmapFrame {
    grid_width: uint32;
    grid_height: uint32;
    bounds_west: float64;
    bounds_south: float64;
    bounds_east: float64;
    bounds_north: float64;
    density: [float32];
    avg_speed: [float32];
}

table SignalState {
    signal_id: uint32;
    phase: uint8;
    seconds_to_change: float32;
    latitude: float64;
    longitude: float64;
}

table SignalFrame {
    signals: [SignalState];
}

union FramePayload { AgentFrame, HeatmapFrame, SignalFrame }

table VelosFrame {
    payload: FramePayload;
}

root_type VelosFrame;
```

### Server-Side Spatial Tiling (Rust)

```rust
// velos-api/src/viz_stream.rs

use flatbuffers::FlatBufferBuilder;
use tokio::sync::broadcast;
use warp::ws::{Message, WebSocket};

const TILE_GRID_SIZE: u32 = 256;

pub struct VizStreamServer {
    /// Spatial tile grid: city bounds divided into 256×256 tiles
    tile_grid: TileGrid,
    /// Per-client subscriptions
    clients: DashMap<ClientId, ClientSubscription>,
    /// Broadcast channel from simulation
    sim_frames: broadcast::Receiver<SimFrame>,
}

struct ClientSubscription {
    subscribed_tiles: HashSet<TileCoord>,
    camera_height: f64,
    max_agents: u32,
    ws_sender: futures::channel::mpsc::UnboundedSender<Message>,
}

impl VizStreamServer {
    /// Called every sim step (~33ms) to push frame to all clients
    pub async fn broadcast_frame(&self, frame: &SimFrame) {
        // Group agents by spatial tile using rayon
        let agents_by_tile: HashMap<TileCoord, Vec<&AgentState>> =
            self.tile_grid.assign_agents_to_tiles(&frame.agents);

        // For each connected client, build a custom frame
        // containing only their subscribed tiles
        self.clients.iter().par_bridge().for_each(|entry| {
            let client = entry.value();
            let mut builder = FlatBufferBuilder::with_capacity(64 * 1024);

            let mut full_agents = Vec::new();
            let mut compact_agents = Vec::new();
            let mut dot_agents = Vec::new();

            for tile_coord in &client.subscribed_tiles {
                if let Some(agents) = agents_by_tile.get(tile_coord) {
                    for agent in agents {
                        let cam_dist = self.tile_grid
                            .tile_center_distance(tile_coord, client.camera_height);

                        if cam_dist < 500.0 {
                            // LOD 0: full detail
                            full_agents.push(agent);
                        } else if cam_dist < 2000.0 {
                            // LOD 1: compact
                            compact_agents.push(agent);
                        } else {
                            // LOD 2: dot
                            dot_agents.push(agent);
                        }
                    }
                }
            }

            // Build FlatBuffers message
            let frame_buf = build_agent_frame(
                &mut builder,
                frame.sim_time,
                frame.frame_id,
                &full_agents,
                &compact_agents,
                &dot_agents,
            );

            // Send binary WebSocket message
            let _ = client.ws_sender.unbounded_send(
                Message::binary(frame_buf)
            );
        });
    }
}
```

---

## 8. Level-of-Detail (LOD) Strategy {#8-lod-strategy}

### Three-Tier Agent LOD

```
Camera Distance     Visual Representation     Data per Agent    Draw Method
──────────────────────────────────────────────────────────────────────────────
< 500m (close)      3D mesh model             20 bytes          Instanced mesh
                    ┌──────────┐               (full state)      ~500 agents
                    │  ╱╲      │
                    │ ╱  ╲     │  ← Car, bus, pedestrian
                    │╱    ╲    │    3D models with heading
                    │══════│   │    Speed-based color
                    └──────────┘    Click-to-inspect

500m – 2km (mid)    Billboard sprite          8 bytes           BillboardCollection
                    ┌──────┐                   (compact)         ~5,000 agents
                    │  🚗  │   ← Camera-facing icon
                    └──────┘     Color = speed
                                 Size = agent type

> 2km (far)         Colored dot               4 bytes           PointPrimitive
                    •                          (dot)             ~50,000+ agents
                    ← 2-4px radius
                      Color = speed or density

> 5km (city-wide)   Edge density coloring     0 bytes           Road mesh color
                    ═══════                    (aggregated)      ~0 individual
                    ← Road colored by          to edge level     agents rendered
                      congestion level
```

### LOD Transition Strategy

To avoid visual popping, VELOS uses **cross-fade transitions** over a 100m buffer zone:

```wgsl
// LOD transition in vertex/fragment shader
fn compute_lod_alpha(cam_distance: f32) -> LodResult {
    // Smooth transition zones
    let lod0_end = 500.0;
    let lod1_start = 450.0;   // 50m overlap for cross-fade
    let lod1_end = 2000.0;
    let lod2_start = 1900.0;  // 100m overlap

    if (cam_distance < lod1_start) {
        return LodResult { level: 0, alpha: 1.0 };
    } else if (cam_distance < lod0_end) {
        // Cross-fade zone: LOD 0 fading out, LOD 1 fading in
        let t = (cam_distance - lod1_start) / (lod0_end - lod1_start);
        return LodResult { level: 0, alpha: 1.0 - t };
    } else if (cam_distance < lod2_start) {
        return LodResult { level: 1, alpha: 1.0 };
    } else if (cam_distance < lod1_end) {
        let t = (cam_distance - lod2_start) / (lod1_end - lod2_start);
        return LodResult { level: 1, alpha: 1.0 - t };
    } else {
        return LodResult { level: 2, alpha: 1.0 };
    }
}
```

---

## 9. Instanced Rendering — 500K Agents at 60 FPS {#9-instanced-rendering}

### Why Instancing?

Without instancing, rendering 500K agents would require 500K draw calls (~500ms). With instancing, we need only 3-5 draw calls (one per LOD per agent type):

```
WITHOUT INSTANCING:                    WITH INSTANCING:
─────────────────────────────         ─────────────────────────────
for each agent:                        Upload instance buffer (1 call)
  set transform (1 API call)           for each (lod, agent_type):
  draw mesh (1 API call)                 draw_instanced (1 call)
= 500K × 2 = 1M API calls             = 5 LOD × 5 types = 25 calls
≈ 500ms                               ≈ 3ms
```

### Instance Buffer Layout

```rust
// GPU instance data — 32 bytes per agent, tightly packed
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct AgentInstanceGPU {
    world_x: f32,          // 4 bytes
    world_y: f32,          // 4 bytes
    world_z: f32,          // 4 bytes
    heading: f32,          // 4 bytes (radians)
    scale: f32,            // 4 bytes (agent-type dependent)
    color_packed: u32,     // 4 bytes (RGBA8)
    speed: f32,            // 4 bytes
    flags: u32,            // 4 bytes (selected, highlighted, etc.)
}                          // TOTAL: 32 bytes × 500K = 16 MB GPU

// Instance buffer descriptor for wgpu
let instance_layout = wgpu::VertexBufferLayout {
    array_stride: std::mem::size_of::<AgentInstanceGPU>() as u64,
    step_mode: wgpu::VertexStepMode::Instance,  // ← per instance, not per vertex
    attributes: &[
        wgpu::VertexAttribute { offset: 0,  shader_location: 5, format: Float32 },
        wgpu::VertexAttribute { offset: 4,  shader_location: 6, format: Float32 },
        wgpu::VertexAttribute { offset: 8,  shader_location: 7, format: Float32 },
        wgpu::VertexAttribute { offset: 12, shader_location: 8, format: Float32 },
        wgpu::VertexAttribute { offset: 16, shader_location: 9, format: Float32 },
        wgpu::VertexAttribute { offset: 20, shader_location: 10, format: Uint32 },
        wgpu::VertexAttribute { offset: 24, shader_location: 11, format: Float32 },
        wgpu::VertexAttribute { offset: 28, shader_location: 12, format: Uint32 },
    ],
};
```

### GPU Indirect Drawing (Zero CPU Involvement)

The GPU cull pass writes directly to an indirect draw buffer, so the CPU never needs to know how many agents are visible:

```rust
// Indirect draw buffer (written by GPU compute cull pass)
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct DrawIndexedIndirect {
    index_count: u32,      // vertices per mesh (e.g., 600 for car)
    instance_count: u32,   // ← FILLED BY GPU CULL SHADER
    first_index: u32,
    base_vertex: i32,
    first_instance: u32,
}

// GPU cull compute shader atomically increments instance_count
// for each agent that passes frustum + LOD test
```

---

## 10. 3D City Model Pipeline — CityGML → 3D Tiles {#10-city-model-pipeline}

### Pipeline Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│              CITY MODEL PIPELINE                                      │
│                                                                       │
│  INPUT SOURCES:                                                       │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐  ┌──────────────┐  │
│  │ CityGML    │  │ OSM        │  │ LiDAR      │  │ Google Photo │  │
│  │ (LOD 1-3)  │  │ Buildings  │  │ Point Cloud │  │ 3D Tiles API │  │
│  └─────┬──────┘  └─────┬──────┘  └─────┬──────┘  └──────┬───────┘  │
│        │               │               │                 │           │
│        ▼               ▼               ▼                 │           │
│  ┌──────────────────────────────────────────┐           │           │
│  │  OFFLINE PROCESSING (build-time)         │           │           │
│  │                                          │           │           │
│  │  Tool: FME, 3DCityDB, or py3dtiles      │           │           │
│  │                                          │           │           │
│  │  1. Parse CityGML / OSM / LiDAR         │           │           │
│  │  2. Classify: buildings, terrain, roads  │           │           │
│  │  3. Triangulate building footprints      │           │           │
│  │  4. Extrude to building heights          │           │           │
│  │  5. Generate multi-resolution LODs       │           │           │
│  │  6. Tile spatially (bounding volume)     │           │           │
│  │  7. Output: 3D Tiles tileset.json        │           │           │
│  │     + .b3dm (Batched 3D Model)           │           │           │
│  │     + .pnts (Point Cloud)                │           │           │
│  └─────────────────┬────────────────────────┘           │           │
│                    │                                     │           │
│                    ▼                                     ▼           │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │  RUNTIME: Load in CesiumJS / wgpu                            │   │
│  │                                                               │   │
│  │  CesiumJS: Cesium.Cesium3DTileset.fromUrl('tileset.json')    │   │
│  │  wgpu:     Parse tileset.json → load visible tiles → GPU mesh│   │
│  │                                                               │   │
│  │  Features:                                                    │   │
│  │  - Hierarchical LOD (close = detailed, far = simplified)     │   │
│  │  - Screen-space error controls quality/performance            │   │
│  │  - Tile caching (keep loaded tiles in GPU memory)             │   │
│  │  - Streaming (load new tiles as camera moves)                 │   │
│  └──────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────┘
```

### Tool Recommendations

| Tool | Input | Output | License | Notes |
|------|-------|--------|---------|-------|
| **py3dtiles** | CityGML, LiDAR (LAS) | 3D Tiles (.b3dm, .pnts) | Apache 2.0 | Python, OSS, handles large datasets |
| **FME** | CityGML, GeoJSON, SHP | 3D Tiles, Cesium Ion | Commercial | Enterprise ETL, most format support |
| **3DCityDB** | CityGML LOD 0-4 | PostgreSQL/PostGIS | Apache 2.0 | Database-driven, CityGML standard |
| **citygml-tools** | CityGML | Optimized CityGML | Apache 2.0 | Pre-processing (validation, upgrade) |
| **Cesium Ion** | CityGML, LiDAR, KML | 3D Tiles (hosted) | SaaS | Easiest pipeline, cloud-hosted tiles |
| **Google 3D Tiles** | N/A (pre-built) | Photorealistic 3D Tiles | API usage | 2500+ cities, highest visual quality |

### OSM → 3D Tiles Pipeline (Free, OSS)

For cities without CityGML data, use OSM building footprints:

```bash
# Step 1: Extract buildings from OSM
osmium extract -b 11.4,48.0,11.8,48.3 germany-latest.osm.pbf -o munich.osm.pbf
osmium tags-filter munich.osm.pbf w/building -o munich-buildings.osm.pbf

# Step 2: Convert to GeoJSON with heights
ogr2ogr -f GeoJSON buildings.geojson munich-buildings.osm.pbf multipolygons

# Step 3: Generate 3D Tiles
# Using py3dtiles (Python)
pip install py3dtiles
py3dtiles convert --srs_in 4326 --srs_out 4978 buildings.geojson --out ./tiles/

# Or using Cesium Ion REST API
curl -X POST "https://api.cesium.com/v1/assets" \
  -H "Authorization: Bearer $CESIUM_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"name": "Munich Buildings", "type": "3DTILES", "options": {"sourceType": "CITYGML"}}'
```

---

## 11. Heatmaps, Flow Arrows & Analytical Overlays {#11-analytical-overlays}

### Overlay Types

```
┌─────────────────────────────────────────────────────────────────┐
│  ANALYTICAL OVERLAY SYSTEM                                       │
│                                                                   │
│  1. DENSITY HEATMAP                                              │
│     ┌────────────────────────────────┐                           │
│     │ ░░░░░░░░░░░▒▒▒▒▒▓▓███████░░░ │  Compute shader:           │
│     │ ░░░░░░░░▒▒▒▒▓▓▓▓██████▓▓▒░░░ │  - Grid 100×100 cells     │
│     │ ░░░░░░▒▒▓▓▓███████████▓▒░░░░ │  - Count agents per cell   │
│     │ ░░░░░▒▒▓▓██████████▓▓▒░░░░░░ │  - Gaussian blur           │
│     │ ░░░░░░▒▒▓▓▓▓████▓▓▒▒░░░░░░░ │  - Color ramp lookup       │
│     └────────────────────────────────┘  - Alpha blend onto scene  │
│                                                                   │
│  2. SPEED HEATMAP (congestion map)                               │
│     Same grid, but value = average speed per cell                │
│     Color: green (free-flow) → red (congested)                   │
│                                                                   │
│  3. FLOW ARROWS                                                  │
│     ┌────────────────────────────────┐                           │
│     │    →  →  →→→  ←←  ←           │  Per-edge aggregation:     │
│     │   ↗  →  →→→→  ←←  ↙          │  - Direction from edge     │
│     │   ↑   →→→→→→  ←←←  ↓         │  - Width = volume          │
│     │   ↑    →→→→    ←←  ↓         │  - Color = speed           │
│     │   ↑     →→      ←  ↓         │  - Animated dash pattern   │
│     └────────────────────────────────┘                           │
│                                                                   │
│  4. SIGNAL STATUS OVERLAY                                        │
│     🔴 Red  🟢 Green  🟡 Yellow                                  │
│     Billboard at each intersection with countdown timer          │
│                                                                   │
│  5. EMISSIONS OVERLAY                                            │
│     CO2/NOx/PM2.5 concentration grid                             │
│     From HBEFA emissions model output (M2)                       │
│                                                                   │
│  6. PREDICTION OVERLAY                                           │
│     Show predicted congestion 15min into future                  │
│     Ghost/transparent coloring on affected roads                 │
└─────────────────────────────────────────────────────────────────┘
```

### GPU Heatmap Compute Shader

```wgsl
// heatmap_compute.wgsl — aggregates agent positions into density grid

struct HeatmapParams {
    grid_width: u32,
    grid_height: u32,
    bounds_min_x: f32,
    bounds_min_y: f32,
    bounds_max_x: f32,
    bounds_max_y: f32,
    agent_count: u32,
    mode: u32,          // 0=density, 1=avg_speed, 2=emissions
};

@group(0) @binding(0) var<storage, read> agents: array<AgentRender>;
@group(0) @binding(1) var<storage, read_write> grid_density: array<atomic<u32>>;
@group(0) @binding(2) var<storage, read_write> grid_speed_sum: array<atomic<u32>>;
@group(0) @binding(3) var<uniform> params: HeatmapParams;

@compute @workgroup_size(256)
fn accumulate(@builtin(global_invocation_id) gid: vec3u) {
    let idx = gid.x;
    if (idx >= params.agent_count) { return; }

    let agent = agents[idx];

    // Map world position to grid cell
    let nx = (agent.world_x - params.bounds_min_x)
           / (params.bounds_max_x - params.bounds_min_x);
    let ny = (agent.world_y - params.bounds_min_y)
           / (params.bounds_max_y - params.bounds_min_y);

    if (nx < 0.0 || nx >= 1.0 || ny < 0.0 || ny >= 1.0) { return; }

    let gx = u32(nx * f32(params.grid_width));
    let gy = u32(ny * f32(params.grid_height));
    let cell = gy * params.grid_width + gx;

    // Atomic increment density
    atomicAdd(&grid_density[cell], 1u);

    // Accumulate speed (as fixed-point: speed × 100)
    let speed_fp = u32(agent.speed * 100.0);
    atomicAdd(&grid_speed_sum[cell], speed_fp);
}
```

---

## 12. Performance Budget & Benchmarks {#12-performance-budget}

### Native wgpu Renderer (RTX 4090, 500K agents)

| Stage | Time (ms) | GPU Util | Notes |
|-------|----------|----------|-------|
| Simulation (EVEN+ODD) | 3.0 | Compute | Semi-sync dispatch |
| Collision correction | 0.3 | Compute | |
| Coordinate transform | 0.5 | Compute | Edge-local → world |
| Frustum cull + LOD | 0.2 | Compute | Writes indirect buffer |
| Heatmap accumulation | 0.3 | Compute | 100×100 grid |
| Terrain render | 2.0 | Render | 3D Tiles mesh, cached |
| Road network render | 1.0 | Render | Static, cached |
| Agent instanced draw | 3.0 | Render | 3 LOD levels |
| Overlay render | 1.0 | Render | Heatmap + UI |
| **TOTAL** | **~11.3** | | **~88 FPS** |

### CesiumJS Web Client (Chrome, 50K agents in viewport)

| Metric | Target | Strategy |
|--------|--------|----------|
| Agent update rate | 30 FPS | WebSocket binary frames |
| LOD 0 entities | ≤500 | 3D model entities (expensive) |
| LOD 1 billboards | ≤5,000 | BillboardCollection batch |
| LOD 2 points | ≤50,000 | PointPrimitiveCollection |
| WebSocket bandwidth | ≤2 MB/s | Delta + spatial tiling |
| Client frame time | ≤33ms | RequestRenderMode when idle |
| GPU memory | ≤1 GB | 3D Tiles cache limit |

### deck.gl Web Client (100K agents)

| Layer | Entities | Draw Time | Memory |
|-------|----------|-----------|--------|
| ScatterplotLayer | 100K | ~5ms | 8 MB |
| TripsLayer (trails) | 10K paths | ~8ms | 40 MB |
| HeatmapLayer | 100K aggregated | ~3ms | 4 MB |
| PathLayer (flow) | 5K edges | ~2ms | 2 MB |
| **TOTAL** | | **~18ms (55 FPS)** | **~54 MB** |

---

## 13. Technology Decision Matrix {#13-decision-matrix}

### When to Use Each Rendering Path

| Use Case | Recommended Path | Why |
|----------|-----------------|-----|
| Development & debugging | **A: Native wgpu** | Zero-copy, max FPS, full detail |
| Stakeholder dashboard | **B: CesiumJS** | Beautiful 3D context, shareable URL |
| Traffic operations center | **C: deck.gl** | High density, analytical layers |
| Public web portal | **B + C hybrid** | CesiumJS city + deck.gl agent overlay |
| Offline trajectory replay | **C: deck.gl TripsLayer** | GPU-animated trails |
| Print/report screenshots | **A: Native wgpu** | Highest resolution, controlled camera |
| Mobile / tablet | **B: CesiumJS** | Best mobile WebGL support |
| VR / immersive | **A: Native wgpu** | Stereo rendering, low latency |

### Library Comparison

| Criteria | wgpu (native) | CesiumJS | deck.gl | Three.js + MapLibre |
|----------|:------------:|:--------:|:-------:|:-------------------:|
| Max agents (60 FPS) | 500K+ | 5K entities / 50K points | 100K+ | 30K |
| 3D Tiles support | Custom loader | Native | Via Tile3DLayer | Plugin |
| Globe/terrain | Custom | Native | Via base map | MapLibre terrain |
| Instanced rendering | Native wgpu | BillboardCollection | GPU layers | InstancedMesh |
| Analytical overlays | Custom compute | Limited | Excellent (built-in) | Custom |
| Coordinate systems | ENU local | WGS84 native | Web Mercator | Local + adapter |
| Cross-platform | Win/Mac/Linux | Any browser | Any browser | Any browser |
| Setup complexity | High | Medium | Medium | High |
| OSS license | Apache 2.0 | Apache 2.0 | MIT | MIT |

---

## 14. Implementation Roadmap {#14-implementation-roadmap}

Aligns with VELOS 9-month roadmap. Visualization tasks primarily fall to E3 (GPU/Rendering) and E4 (API/Integration).

### Month 3-4: Foundation
- E4: WebSocket streaming server with FlatBuffers encoding
- E4: Spatial tile subscription system (256×256 grid)
- E3: Native wgpu basic renderer (terrain mesh + colored dots)
- E4: CesiumJS client skeleton with 3D Tiles city loading

### Month 5-6: Core Visualization
- E3: Instanced rendering pipeline (LOD 0/1/2) with GPU indirect draw
- E3: Coordinate transform compute shader
- E3: Frustum culling compute shader
- E4: CesiumJS full integration (entity pool, billboard batch, point collection)
- E4: deck.gl client with ScatterplotLayer + HeatmapLayer

### Month 7-8: Analytical Overlays
- E3: GPU heatmap compute shader (density, speed, emissions)
- E3: LOD cross-fade transitions
- E4: deck.gl TripsLayer integration for trajectory replay
- E4: Signal status overlay (real-time phase display)
- E4: Flow arrow layer (per-edge aggregation)

### Month 9: Polish & Performance
- E3: GPU indirect draw optimization (zero CPU involvement)
- E3: Shadow mapping for 3D models
- E4: Dashboard UI (egui for native, React for web)
- ALL: Performance tuning, load testing (100 concurrent web clients)

---

## 15. Open-Source References {#15-oss-references}

### Directly Reusable

| Project | What to Reuse | License |
|---------|--------------|---------|
| [py3dtiles](https://github.com/Oslandia/py3dtiles) | CityGML/LiDAR → 3D Tiles conversion | Apache 2.0 |
| [3DCityDB](https://github.com/3dcitydb/3dcitydb) | CityGML database + export | Apache 2.0 |
| [CesiumJS](https://github.com/CesiumGS/cesium) | Web 3D globe + 3D Tiles renderer | Apache 2.0 |
| [deck.gl](https://github.com/visgl/deck.gl) | GPU-accelerated data layers | MIT |
| [MapLibre GL JS](https://github.com/maplibre/maplibre-gl-js) | Open-source base map renderer | BSD-3 |
| [FlatBuffers](https://github.com/google/flatbuffers) | Zero-copy binary serialization | Apache 2.0 |
| [wgpu](https://github.com/gfx-rs/wgpu) | WebGPU implementation in Rust | Apache 2.0 / MIT |

### Reference Architecture (Study, Don't Fork)

| Project | What to Learn | URL |
|---------|--------------|-----|
| SUMO-Web3D | Three.js + TraCI traffic viz | github.com/sidewalklabs/sumo-web3d |
| kepler.gl | Large-scale geospatial viz (by Uber) | github.com/keplergl/kepler.gl |
| Tangram | WebGL map renderer with shaders | github.com/tangrams/tangram |
| loaders.gl | 3D Tiles / I3S / point cloud loaders | github.com/visgl/loaders.gl |
| terrain_renderer | Bevy + wgpu GPU terrain LOD | github.com/kurtkuehnert/terrain_renderer |

---

## Appendix: Quick-Start — Minimum Viable 3D Visualization

For the fastest path to "agents on a 3D map" during VELOS Month 3-4 prototype:

```
QUICK-START STACK:
─────────────────
1. CesiumJS + Cesium Ion (free tier) for 3D city context
2. WebSocket (tokio-tungstenite) from VELOS
3. JSON messages initially (switch to FlatBuffers in Month 5)
4. PointPrimitiveCollection for all agents (LOD 2 only)
5. Color = speed (green→red ramp)
6. Camera: fly to city center, 45° pitch

This gets a working demo in ~2 days of E4's time.
Upgrade path: JSON → FlatBuffers → LOD system → instanced models
```

---

*v1.0 — Companion to VELOS Architecture Plan §9*
*Cross-references: C5 (EdgeGeometry), M9 (WebSocket spatial tiling), M2 (HBEFA emissions)*
