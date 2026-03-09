# Architecture Patterns: Camera CV Detection + 3D wgpu Rendering

**Domain:** GPU-accelerated traffic microsimulation -- CV integration and native 3D rendering
**Researched:** 2026-03-09
**Confidence:** MEDIUM (verified against existing codebase; CV and 3D patterns sourced from ecosystem research)

---

## 1. Recommended Architecture

### 1.1 System Overview

Two new capability domains integrate into the existing VELOS architecture:

1. **Camera CV Pipeline** -- async camera frame decode, ONNX inference (YOLOv8/11), count aggregation, and demand calibration feedback
2. **3D Native Rendering** -- wgpu depth-buffered mesh rendering for buildings, terrain, and 3D agent models alongside existing 2D mode

Both domains are additive. Neither modifies the core simulation loop (wave-front dispatch, ECS tick, prediction). They attach at well-defined integration points.

```
                        EXISTING PIPELINE (unchanged)
                        ============================
[velos-demand] --> [velos-core ECS] --> [velos-gpu compute] --> [velos-gpu render]
      ^                                                              |
      |                                                              v
      |                                                        [2D Renderer]
      |                                                              |
  NEW: demand                                              NEW: 3D Renderer
  adjustment                                               (parallel render pass)
      ^                                                              |
      |                                                              v
[velos-cv]                                                   [velos-render3d]
  Camera decode                                              glTF mesh loading
  ONNX inference                                             Depth buffer
  Count aggregation                                          Building extrusions
  Edge-level counts                                          Terrain heightmap
```

### 1.2 New Crate Structure

| Crate | Responsibility | Dependencies |
|-------|---------------|-------------|
| `velos-cv` | Camera frame decode, ONNX inference, detection aggregation, edge-level vehicle counts | `ort`, `image`, `tokio`, `velos-net` (for edge snapping) |
| `velos-render3d` | 3D mesh pipeline, building loading, terrain, depth buffer, LOD, Camera3D | `wgpu`, `gltf`, `glam`, `bytemuck` |

**Why two new crates (not modifications to existing ones):**

- `velos-cv` is conceptually separate from simulation. It is an input source (like OSM import or GTFS), not part of the sim loop. Putting it in `velos-demand` would bloat that crate with ONNX, image processing, and async camera dependencies.
- `velos-render3d` is a rendering concern, not a GPU compute concern. `velos-gpu` already conflates compute dispatch and 2D rendering (it owns `Renderer`, `ComputeDispatcher`, and `SimWorld`). Adding 3D rendering there would push it past 700 lines per file. A separate crate enforces the render/compute boundary.

**Why NOT a `velos-tile` crate:** Tile serving is handled by PMTiles/Nginx (infrastructure, not code). No new crate needed for map tiles.

**Modifications to existing crates:**

| Crate | Change |
|-------|--------|
| `velos-gpu` | `app.rs`: Add render mode toggle (2D/3D), wire `Renderer3D` into frame loop. `camera.rs`: Keep `Camera2D` as-is; `Camera3D` lives in `velos-render3d`. |
| `velos-demand` | `spawner.rs`: Accept `CvDemandAdjustment` from `velos-cv` to scale spawn rates per edge. |
| `velos-core` | `components.rs`: No new ECS components needed. CV data is a shared resource, not per-entity. |

---

## 2. Component Boundaries

| Component | Responsibility | Communicates With |
|-----------|---------------|-------------------|
| `velos-cv::CameraManager` | Manages camera streams (RTSP/file), frame decode | `InferenceRunner` |
| `velos-cv::InferenceRunner` | Runs ONNX YOLOv8/11 models on decoded frames | `CountAggregator` |
| `velos-cv::CountAggregator` | Aggregates per-frame detections into smoothed per-edge vehicle counts | `velos-demand::Spawner` |
| `velos-cv::CameraConfig` | Camera position, FOV, edge mapping (which camera watches which edges) | All `velos-cv` components |
| `velos-render3d::Renderer3D` | Owns 3D render pipeline, depth texture, mesh buffers | `velos-gpu::GpuState` (device/queue sharing) |
| `velos-render3d::Camera3D` | Perspective camera with orbit, zoom, WASD controls | `Renderer3D` |
| `velos-render3d::BuildingLoader` | Loads glTF building meshes or OSM-extruded geometry | `Renderer3D` |
| `velos-render3d::TerrainLoader` | Loads heightmap DEM data into terrain mesh | `Renderer3D` |
| `velos-render3d::AgentMeshPool` | Per-vehicle-type 3D meshes with LOD (mesh/billboard/dot) | `Renderer3D` |

---

## 3. Data Flow

### 3.1 CV Pipeline Data Flow

```
Camera Stream (RTSP/file)
    |
    v
[CameraManager]  <-- tokio::spawn, async frame decode (ffmpeg or gstreamer bindings)
    |  Frame: (camera_id, timestamp, RGB image)
    v
[InferenceRunner]  <-- dedicated OS thread (not tokio, not rayon)
    |  Detections: Vec<Detection { class, bbox, confidence }>
    v
[CountAggregator]  <-- sliding window (30s), exponential smoothing
    |  EdgeCounts: HashMap<EdgeId, VehicleCount { motorbike, car, bus, truck }>
    v
[DemandAdjuster]  <-- compare detected counts vs simulated counts per edge
    |  Adjustment: HashMap<EdgeId, f64>  (multiplier: 1.2 = 20% more spawns)
    v
[velos-demand::Spawner]  <-- applies multiplier to spawn rate for affected edges
```

**Threading model:**

- **Camera decode:** tokio task (async I/O for RTSP streams). Uses `nokhwa` or `ffmpeg-next` crate for frame capture.
- **ONNX inference:** Dedicated OS thread via `std::thread::spawn`. The `ort::Session` is `Send` but inference is blocking (10-30ms per frame at 640x640 on CPU). Must NOT run on tokio (blocks cooperative scheduling) or rayon (starves simulation work-stealing pool).
- **Count aggregation:** Lightweight CPU work. Runs in the inference thread after each detection, or on a separate tokio task receiving via `crossbeam::channel`.
- **Demand adjustment:** Applied during the sim tick's spawn phase, already on the main simulation thread. Reads `CvEdgeCounts` via `ArcSwap` (lock-free).

**Why a dedicated thread for inference, not rayon:**

The simulation's wave-front dispatch uses rayon for CPU-side lane sorting (`sort_agents_by_lane`) and route advance. ONNX inference for a single camera at 640x640 takes 10-30ms on CPU (YOLOv8m). Running this on rayon risks starving the simulation's work-stealing pool. A dedicated thread with `crossbeam::channel` for frame handoff isolates inference latency entirely.

### 3.2 3D Rendering Data Flow

```
[SimWorld ECS]
    |  Agent positions, headings, types (same query as 2D build_instances)
    v
[AgentInstanceBuilder3D]  <-- converts ECS Position/Kinematics/VehicleType to 3D instances
    |  Instance data: position (x, y, z=0), rotation (quat from heading), scale, color
    v
[Renderer3D::render_frame]
    |
    +-- Single render pass with depth buffer:
    |     1. Terrain pipeline  (1 draw call, large index buffer)
    |     2. Building pipeline (instanced, per-LOD bucket)
    |     3. Road surface pipeline (textured quads from edge geometry)
    |     4. Agent pipeline    (instanced, 7 draw calls by vehicle type)
    |
    v
[Surface texture]  <-- same wgpu surface as 2D mode, shared device/queue
```

**Key design: single render pass with depth buffer.**

The 3D scene uses ONE render pass with a `Depth32Float` depth-stencil attachment. Buildings, terrain, road surfaces, and agents must depth-test against each other. Separate render passes would lose depth information between passes (requiring depth texture resolve/reattach, adding GPU overhead).

The existing 2D pipeline (`Renderer`) uses `depth_stencil: None`. The 2D and 3D modes are mutually exclusive, selected by a runtime toggle.

### 3.3 Cross-Thread Data Sharing: CvEdgeCounts

CV detection results cross the thread boundary via `ArcSwap`, following the same pattern as `PredictionOverlay` in `velos-predict`:

```rust
// In velos-cv
pub struct CvPipeline {
    edge_counts: Arc<ArcSwap<CvEdgeCounts>>,
}

// CV inference thread (writer)
self.edge_counts.store(Arc::new(new_counts));

// Sim tick, demand spawner (reader -- lock-free)
let counts = cv_pipeline.edge_counts.load();
```

`CvEdgeCounts` is a shared resource, NOT an ECS component. Cameras and detections are not simulation agents -- they do not need position updates, routing, or car-following. Polluting the ECS with non-agent entities complicates queries and wastes GPU buffer space.

```rust
/// Per-edge CV detection counts. Written by velos-cv, read by velos-demand.
pub struct CvEdgeCounts {
    pub counts: HashMap<u32, VehicleCounts>,
    pub last_updated: f64,  // sim_time when last refreshed
}

pub struct VehicleCounts {
    pub motorbike: u32,
    pub car: u32,
    pub bus: u32,
    pub truck: u32,
}
```

---

## 4. Integration Points

### 4.1 CV Integration: velos-demand Spawner

**Integration point:** `velos-demand::Spawner::tick()`

Currently, the spawner reads OD matrices and ToD profiles to determine spawn rates. CV integration adds a third input -- observed edge-level counts:

```rust
// In velos-demand/src/spawner.rs
pub fn tick(&mut self, sim_time: f64, cv_counts: Option<&CvEdgeCounts>) -> Vec<SpawnRequest> {
    let base_rate = self.tod_profile.rate_at(sim_time);

    for (edge_id, od_demand) in &self.od_matrix {
        let mut rate = base_rate * od_demand;

        // CV adjustment: compare detected vs simulated counts
        if let Some(cv) = cv_counts {
            if let Some(detected) = cv.counts.get(edge_id) {
                let simulated = self.current_edge_counts.get(edge_id).unwrap_or(&0);
                let ratio = detected.total() as f64 / (*simulated as f64).max(1.0);
                // Clamp to avoid runaway spawning or complete suppression
                let adj = ratio.clamp(0.5, 2.0);
                rate *= adj;
            }
        }

        // ... existing spawn logic unchanged
    }
}
```

**Why at the spawner, not the OD matrix:** The OD matrix is a static input representing spatial trip distribution. CV counts are real-time magnitude observations. Adjusting spawn rates preserves the OD spatial distribution while scaling magnitudes to match observed reality. This is standard practice in online traffic simulation calibration.

### 4.2 3D Rendering Integration: app.rs Frame Loop

**Integration point:** `velos-gpu::app.rs::GpuState::render()`

Current render flow (from codebase analysis of `app.rs`):
1. `renderer.render_frame(&mut encoder, &view)` -- 2D agents + roads (no depth buffer, clear background)
2. `egui_renderer.render()` -- UI overlay (LoadOp::Load, draws on top)

New render flow with 3D mode toggle:

```rust
// In velos-gpu/src/app.rs
#[derive(Clone, Copy, PartialEq)]
enum RenderMode {
    TwoD,
    ThreeD,
}

impl GpuState {
    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&Default::default());

        let mut encoder = self.device.create_command_encoder(&Default::default());

        match self.render_mode {
            RenderMode::TwoD => {
                self.renderer.update_camera(&self.queue, &self.camera);
                self.renderer.render_frame(&mut encoder, &view);
            }
            RenderMode::ThreeD => {
                self.renderer_3d.update_camera(&self.queue, &self.camera_3d);
                self.renderer_3d.update_agent_instances(&self.queue, &self.agent_instances_3d);
                self.renderer_3d.render_frame(&mut encoder, &view, &self.depth_view);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));

        // egui overlay (unchanged, always on top via LoadOp::Load)
        // ... existing egui code ...

        output.present();
        Ok(())
    }
}
```

**Why mode toggle, not combined rendering:** The 2D orthographic pipeline has no depth buffer, no z-coordinates, and no perspective. Overlaying 3D on 2D creates z-fighting with flat shapes. A clean toggle is simpler and avoids hybrid rendering complexity. The egui panel gets a "2D / 3D" toggle button.

### 4.3 Device and Queue Sharing

Both `Renderer` (2D) and `Renderer3D` share the same `wgpu::Device` and `wgpu::Queue`. This is safe because:

- wgpu `Device` and `Queue` are `Send + Sync`
- Only one render mode is active per frame (no surface texture contention)
- Both renderers create pipelines on the same device at init time
- The camera bind group layout (`CameraUniform { view_proj: mat4x4<f32> }`) is structurally identical between 2D and 3D -- only the matrix contents differ (orthographic vs perspective). The bind group layout can be shared.

The `Renderer3D` is initialized alongside `Renderer` in `GpuState::new()` but only executes render passes when `render_mode == ThreeD`.

---

## 5. 3D Render Pipeline Details

### 5.1 Depth Buffer

The current 2D pipeline has `depth_stencil: None` in `RenderPipelineDescriptor` (verified in `renderer.rs` line 274). The 3D pipeline adds:

```rust
// In velos-render3d/src/renderer3d.rs
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn create_depth_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth_texture_3d"),
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
```

The depth texture must be recreated on window resize (same lifecycle as surface texture). The `DepthStencilState` uses `CompareFunction::Less` (front-to-back rendering).

All 3D render pipelines (terrain, buildings, agents) include this depth-stencil config:

```rust
depth_stencil: Some(wgpu::DepthStencilState {
    format: DEPTH_FORMAT,
    depth_write_enabled: true,
    depth_compare: wgpu::CompareFunction::Less,
    stencil: wgpu::StencilState::default(),
    bias: wgpu::DepthBiasState::default(),
}),
```

### 5.2 Camera3D (Perspective)

```rust
/// Perspective camera with orbit controls for 3D city view.
pub struct Camera3D {
    pub position: Vec3,   // Eye position in world space (metres)
    pub target: Vec3,     // Look-at point
    pub up: Vec3,         // Up vector (Y-up to match existing coordinate system)
    pub fov_y: f32,       // Vertical FOV in radians (default: 45 degrees)
    pub aspect: f32,      // viewport width / height
    pub near: f32,        // Near clip (1.0m -- no need for sub-metre precision)
    pub far: f32,         // Far clip (20000.0m -- HCMC POC area is ~8km)
}

impl Camera3D {
    pub fn view_proj_matrix(&self) -> Mat4 {
        let view = Mat4::look_at_rh(self.position, self.target, self.up);
        let proj = Mat4::perspective_rh(self.fov_y, self.aspect, self.near, self.far);
        proj * view
    }
}
```

The camera uniform buffer layout is identical to the 2D camera: `struct CameraUniform { view_proj: mat4x4<f32> }`. The shader only sees a 4x4 matrix. This means the WGSL camera uniform struct and bind group layout are reusable between 2D and 3D modes.

### 5.3 Asset Pipeline: glTF Buildings and Terrain

**Building loading -- two strategies:**

**Strategy A (preferred for HCMC): OSM building extrusions.**

OSM has `building:levels` and building footprints for HCMC. Extrude 2D polygons to height = `levels * 3.0m`. This generates simple box meshes directly from existing OSM data that `velos-net` already imports. No external 3D model files needed.

```rust
pub struct BuildingMesh {
    pub vertex_buffer: wgpu::Buffer,  // [pos: vec3<f32>, normal: vec3<f32>]
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub color: [f32; 4],              // building type coloring
}

pub struct BuildingLoader;

impl BuildingLoader {
    /// Extrude OSM building footprints into 3D meshes.
    /// Reads building polygons from velos-net RoadGraph (already parsed from PBF).
    pub fn from_osm_extrusions(
        device: &wgpu::Device,
        buildings: &[OsmBuilding],
    ) -> Vec<BuildingMesh> {
        // For each building polygon:
        // 1. Triangulate the footprint (earcut algorithm)
        // 2. Extrude walls: for each edge of polygon, create 2 triangles
        // 3. Create roof: copy floor triangulation at height
        // 4. Upload vertex + index buffers
    }
}
```

**Strategy B (for landmark buildings): glTF models.**

Use the `gltf` crate (gltf-rs, v1.4+) to load individual glTF files for notable buildings (Bitexco Financial Tower, Landmark 81). Parse geometry, extract vertex positions/normals/UVs, upload to wgpu buffers.

```rust
pub fn load_gltf_mesh(device: &wgpu::Device, path: &Path) -> Vec<BuildingMesh> {
    let (document, buffers, _images) = gltf::import(path).unwrap();
    for mesh in document.meshes() {
        for primitive in mesh.primitives() {
            let reader = primitive.reader(|buffer| Some(&buffers[buffer.index()]));
            let positions: Vec<[f32; 3]> = reader.read_positions().unwrap().collect();
            let normals: Vec<[f32; 3]> = reader.read_normals().unwrap().collect();
            let indices: Vec<u32> = reader.read_indices().unwrap().into_u32().collect();
            // Upload to wgpu buffers
        }
    }
}
```

**Terrain loading:**

SRTM 30m DEM data for HCMC. Convert heightmap to a grid mesh. HCMC is flat (elevation 0-10m), so terrain is mostly cosmetic but provides a ground plane.

```rust
pub struct TerrainMesh {
    pub vertex_buffer: wgpu::Buffer,  // grid vertices with height from DEM
    pub index_buffer: wgpu::Buffer,
    pub index_count: u32,
    pub grid_size: u32,               // e.g., 256x256 for 8km at ~31m resolution
}
```

### 5.4 3D Agent Instances

3D agent rendering reuses the same ECS query as `SimWorld::build_instances()` (Position, Kinematics, VehicleType) but produces a 3D instance struct:

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AgentInstance3D {
    pub position: [f32; 3],    // x, y, z (z = 0.0 for flat HCMC, or terrain height)
    pub _pad0: f32,
    pub rotation: [f32; 4],    // quaternion from heading angle
    pub color: [f32; 4],       // same color logic as 2D (vehicle type + model coloring)
    pub scale: [f32; 3],       // vehicle-type-dependent: motorbike [2,1,1], car [4,2,1.5]
    pub _pad1: f32,
}
```

**LOD strategy** (per architecture doc `05-visualization-api.md`):

| Distance from camera | Representation | Triangles per agent |
|----------------------|---------------|---------------------|
| < 500m | Low-poly 3D mesh | ~100-200 |
| 500m - 2km | Billboard (camera-facing quad) | 2 |
| > 2km | Colored dot (like current 2D) | 2-6 |

**For initial implementation:** Use colored dots only (existing 2D shapes rendered with perspective projection and depth buffer). This validates the 3D pipeline without requiring 3D model assets. Add mesh LOD as a follow-up.

### 5.5 Render Pass Structure

Single render pass for the entire 3D scene:

```
begin_render_pass(color: Clear(sky_blue) + depth: Clear(1.0)):
    1. set_pipeline(terrain_pipeline)         -- draw terrain mesh (1 draw call)
    2. set_pipeline(building_pipeline)        -- draw buildings (instanced, 1-3 draw calls by LOD)
    3. set_pipeline(road_surface_pipeline)    -- draw road quads (1 draw call)
    4. set_pipeline(agent_3d_pipeline)        -- draw agents (up to 7 draw calls by vehicle type)
end_render_pass
```

All pipelines share the same camera bind group (group 0, binding 0). Each has its own vertex layout, shader, and pipeline state. All include `DepthStencilState` with `Depth32Float` and `CompareFunction::Less`.

---

## 6. Patterns to Follow

### Pattern 1: ArcSwap for Cross-Thread Data Sharing

**What:** Use `arc_swap::ArcSwap<T>` for data that one thread writes and another reads without locks.
**When:** CV pipeline produces detection counts; demand spawner reads them during sim tick.
**Why:** Already established in `velos-predict` for `PredictionOverlay`. Consistent pattern across the codebase.

### Pattern 2: Dedicated Thread with Channel for Blocking Work

**What:** Spawn a dedicated OS thread for ONNX inference, communicate via `crossbeam::channel`.
**When:** Any blocking computation (>5ms) that must not interfere with rayon or tokio.

```rust
let (frame_tx, frame_rx) = crossbeam::channel::bounded(2);  // backpressure at 2 frames
let (result_tx, result_rx) = crossbeam::channel::bounded(2);

std::thread::Builder::new()
    .name("cv-inference".to_string())
    .spawn(move || {
        let session = ort::Session::builder()
            .with_model_from_file("yolov8m.onnx")
            .expect("failed to load ONNX model");

        while let Ok(frame) = frame_rx.recv() {
            let detections = run_inference(&session, &frame);
            let _ = result_tx.send(detections);  // non-blocking send, drop if full
        }
    });
```

### Pattern 3: Render Mode Abstraction

**What:** Enum dispatch for 2D/3D rendering to keep `app.rs` clean.
**When:** Adding the 3D renderer alongside the existing 2D renderer.

```rust
enum RenderMode {
    TwoD,
    ThreeD,
}

// In app.rs render(), match on mode instead of complex conditionals.
// Each renderer is a separate struct with independent state.
```

Do NOT use `Box<dyn SimRenderer>` trait objects -- the two renderers have different state (Camera2D vs Camera3D, no depth buffer vs depth buffer). Enum dispatch with explicit match arms is simpler and avoids trait object overhead.

---

## 7. Anti-Patterns to Avoid

### Anti-Pattern 1: Running ONNX Inference on tokio Runtime

**What:** Calling `session.run()` inside an `async fn` on tokio.
**Why bad:** ONNX inference blocks for 10-30ms. tokio cooperative scheduling depends on tasks yielding at `.await` points. A 30ms block starves all other async tasks on that worker thread (WebSocket relay, RTSP frame reads, egui repaints).
**Instead:** Dedicated OS thread with channel communication. If async integration is needed, use `tokio::task::spawn_blocking()` but be aware it consumes a thread from tokio's blocking thread pool.

### Anti-Pattern 2: Putting CV State into ECS Components

**What:** Adding per-camera or per-detection entities to the hecs World.
**Why bad:** Cameras are not simulation agents. They do not need position updates, routing, or car-following. Adding them to the ECS pollutes agent queries (`world.query::<(&Position, &Kinematics, &VehicleType)>()` would need to filter out camera entities). The ECS indexes (sparse sets in hecs) would waste memory on non-agent archetypes.
**Instead:** CV state lives in `velos-cv` structs. Only the output (`CvEdgeCounts`) crosses the boundary via `ArcSwap`.

### Anti-Pattern 3: Shared Depth Buffer Between 2D and 3D Modes

**What:** Creating a depth buffer used by both the 2D and 3D pipelines.
**Why bad:** The current 2D pipeline intentionally has no depth buffer. All agents are at z=0, drawn in painter's order: roads first, then motorbikes, then cars, then pedestrians (via 3 sequential draw calls at lines 499-526 of `renderer.rs`). Adding a depth buffer to 2D changes draw order semantics and causes z-fighting with overlapping flat shapes.
**Instead:** Depth buffer exists only in `Renderer3D`. Mode toggle determines which renderer runs. The 2D pipeline remains unchanged.

### Anti-Pattern 4: Dual Render Pass for 3D (Opaque + Transparent)

**What:** Using two render passes -- one for opaque geometry, one for transparent (alpha-blended) geometry.
**Why bad (for now):** VELOS agents are solid-colored, buildings are opaque. There are no transparent objects in the POC. Adding a second pass for transparency doubles GPU submission overhead for zero benefit.
**Instead:** Single render pass. If transparency is needed later (glass buildings, fog effects), add the second pass then. YAGNI.

### Anti-Pattern 5: Loading Full CityGML/3DTiles for HCMC

**What:** Attempting to load 3D Tiles or CityGML datasets for building geometry.
**Why bad:** No CityGML dataset exists for HCMC (explicitly listed as out-of-scope in `00-architecture-overview.md`). 3D Tiles require a tile server and complex LOD management.
**Instead:** OSM building extrusions (data already available via `velos-net` OSM import) + a handful of glTF landmark models. Simple, data-available, sufficient for POC.

---

## 8. Build Order (Dependency-Aware)

### Phase 1: 3D Renderer Foundation

**Must come first** because 3D agent visualization requires the perspective pipeline and depth buffer. This is the most architecturally invasive change (touches `app.rs`, adds new crate, modifies frame loop).

| Step | Task | Depends On | Unblocks |
|------|------|------------|----------|
| 1.1 | Create `velos-render3d` crate: `Camera3D`, `Renderer3D` skeleton (empty scene, depth buffer, sky-blue clear color) | Existing `wgpu::Device` from `velos-gpu` | 1.2 |
| 1.2 | Add render mode toggle to `app.rs` (2D/3D switch via egui button) | 1.1 | 1.3 |
| 1.3 | Flat terrain mesh (ground plane at z=0, single color) | 1.2 | 1.4, 1.5 |
| 1.4 | Port agent instances to 3D (same colored dots but with perspective projection) | 1.3 | Phase 3 |
| 1.5 | OSM building extrusions (requires parsing `building:levels` from PBF) | 1.3 | Phase 3 |

### Phase 2: CV Pipeline Foundation

**Can run in parallel with Phase 1.** No dependency on 3D renderer. CV output feeds `velos-demand::Spawner`, which works in both 2D and 3D modes.

| Step | Task | Depends On | Unblocks |
|------|------|------------|----------|
| 2.1 | Create `velos-cv` crate: `CameraConfig` struct, file-based frame source (read video file or image directory) | Nothing | 2.2 |
| 2.2 | `InferenceRunner`: load YOLOv8m ONNX model via `ort`, run detection on single frame | 2.1 | 2.3 |
| 2.3 | `CountAggregator`: sliding window aggregation, exponential smoothing, output `CvEdgeCounts` | 2.2 | 2.4 |
| 2.4 | Wire `CvEdgeCounts` to `velos-demand::Spawner` via `ArcSwap`. Add egui panel showing per-edge adjustment factors. | 2.3 | Phase 3 |

### Phase 3: Integration and Polish

**Depends on Phase 1 + Phase 2 completion.**

| Step | Task | Depends On | Unblocks |
|------|------|------------|----------|
| 3.1 | CV detection overlay in 3D view (camera FOV cones as wireframe meshes, detection counts as 3D text labels) | Phase 1, Phase 2 | 3.2 |
| 3.2 | DEM terrain (replace flat plane with SRTM heightmap) | 1.3 | Done |
| 3.3 | LOD agent meshes (low-poly 3D vehicle models at close zoom, billboards at medium, dots at far) | 1.4 | Done |
| 3.4 | RTSP camera source (replace file-based with live RTSP stream decode) | 2.1 | Done |
| 3.5 | Calibration loop: compare CV-adjusted sim vs raw sim, report GEH improvement | 2.4 | Done |

### Rationale for This Order

1. **3D renderer before 3D agents:** Cannot render 3D agent meshes without a perspective pipeline and depth buffer. Foundation must exist first.
2. **CV pipeline independent of 3D:** CV detection produces edge-level counts that feed the spawner. This works identically with 2D rendering. No 3D dependency.
3. **Integration last:** Showing CV detections as 3D overlays requires both the CV pipeline (data) and the 3D renderer (visualization). Naturally comes after both are working independently.
4. **Terrain before buildings:** Terrain provides the ground plane. Buildings sit on terrain. Rendering buildings without terrain makes them float in void.
5. **File-based frames before RTSP:** Developing and testing ONNX inference is much easier with a video file (deterministic, reproducible) than a live RTSP stream (network latency, dropped frames, authentication). RTSP is a deployment concern, not a development concern.

---

## 9. Scalability Considerations

| Concern | 1 camera | 10 cameras | 50 cameras |
|---------|----------|------------|------------|
| Inference threads | 1 dedicated OS thread | 10 threads (acceptable on 8-core CPU) | GPU inference via CUDA/CoreML Execution Provider required |
| ONNX model memory | ~200MB (YOLOv8m, shared weights) | ~200MB (single Session, batch across cameras) | ~200MB (batch inference) |
| Frame decode | ~5% CPU | ~30% CPU | Dedicated decode hardware or lower FPS |
| Count aggregation | Trivial (HashMap insert) | Trivial | Still trivial |
| Demand adjustment | 1 HashMap lookup per tick | 10 lookups | 50 lookups (sub-microsecond) |

| Concern | 100 buildings | 10K buildings | 100K buildings |
|---------|---------------|---------------|----------------|
| GPU VRAM | ~1MB vertex data | ~100MB vertex data | Frustum culling required |
| Draw calls | 1 instanced | 1 instanced | LOD bucketing (2-3 instanced calls) |
| Load time | <100ms | ~1s | ~5s (one-time at init) |
| Frame time impact | <0.1ms | ~0.5ms | ~1ms with culling |

---

## 10. wgpu Compatibility Notes

The existing codebase uses **wgpu 27** (`workspace Cargo.toml` line 12: `wgpu = "27"`). The milestone context mentions "wgpu 28" -- both versions support all features described in this document.

Verified compatibility for 3D features:
- `TextureFormat::Depth32Float` -- fully supported on Metal, Vulkan, DX12 (all backends)
- Multiple render pipelines with distinct `DepthStencilState` -- supported
- Instanced rendering with 3D vertex layouts (`vec3<f32>` position, `vec4<f32>` quaternion) -- same mechanism as 2D
- `wgpu::PrimitiveState` with `cull_mode: Some(Face::Back)` -- standard for 3D (not used in 2D currently)
- Texture sampling for building materials -- supported via bind groups (group 1 for textures, group 0 for camera)

**If upgrading to wgpu 28:** API is backward-compatible for these features. Main wgpu 28 change is improved error reporting and `PollType` API (already used in codebase).

---

## Sources

- Existing codebase analysis (HIGH confidence):
  - `crates/velos-gpu/src/renderer.rs` -- current 2D instanced renderer, no depth buffer
  - `crates/velos-gpu/src/camera.rs` -- Camera2D orthographic projection
  - `crates/velos-gpu/src/app.rs` -- GpuState, frame loop, egui integration
  - `crates/velos-gpu/src/compute.rs` -- ComputeDispatcher, wave-front pipeline
  - `crates/velos-gpu/src/sim_render.rs` -- build_instances(), signal indicators
  - `crates/velos-core/src/components.rs` -- ECS component types, GpuAgentState
  - `crates/velos-gpu/shaders/agent_render.wgsl` -- 2D vertex shader with rotation
- Existing architecture docs (HIGH confidence):
  - `docs/architect/00-architecture-overview.md` -- component diagram, CityGML out-of-scope
  - `docs/architect/01-simulation-engine.md` -- frame pipeline, buffer layout, ECS architecture
  - `docs/architect/05-visualization-api.md` -- LOD strategy, 3D building extrusions
  - `docs/architect/04-data-pipeline-hcmc.md` -- OSM building footprints, SRTM DEM
- [Learn wgpu -- Depth Buffer](https://sotrh.github.io/learn-wgpu/beginner/tutorial8-depth/) -- depth stencil setup (MEDIUM confidence)
- [Learn wgpu -- Model Loading](https://sotrh.github.io/learn-wgpu/beginner/tutorial9-models/) -- mesh loading patterns (MEDIUM confidence)
- [gltf-rs crate](https://github.com/gltf-rs/gltf) -- glTF 2.0 loading in Rust (HIGH confidence)
- [ort crate documentation](https://ort.pyke.io/) -- ONNX Runtime Rust bindings (HIGH confidence)
- [Real-time YOLO with Rust and ORT](https://medium.com/@alfred.weirich/rust-ort-onnx-real-time-yolo-on-a-live-webcam-part-1-b6edfb50bf9b) -- multi-threaded inference pipeline (MEDIUM confidence)
- [Rust CV Ecosystem 2025](https://andrewodendaal.com/rust-computer-vision-ecosystem/) -- ecosystem overview (MEDIUM confidence)
