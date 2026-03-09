# Domain Pitfalls: Camera CV Detection + 3D Rendering for VELOS

**Domain:** Adding camera-based vehicle/pedestrian detection and 3D native wgpu rendering to an existing GPU-accelerated Rust traffic microsimulation
**Researched:** 2026-03-09
**Confidence:** MEDIUM-HIGH (verified against wgpu v28 release notes, Apple Metal documentation, ONNX Runtime CoreML docs, YOLO benchmark papers, existing VELOS codebase analysis)

**Context:** VELOS is a 31K LOC Rust workspace running wgpu 28 on macOS Metal. It currently uses a single `wgpu::Device` and `wgpu::Queue` for both compute (wave-front car-following, perception, pedestrian social force) and 2D instanced rendering (agent shapes + road lines). The system renders via `egui-wgpu` integration with a `winit` event loop. Adding camera CV and 3D rendering means three GPU-heavy subsystems competing on a single Apple Silicon GPU with unified memory.

---

## Critical Pitfalls

### Pitfall 1: Single wgpu Queue Serializes Compute, Render, and ML Inference

**What goes wrong:**
VELOS currently uses one `wgpu::Device` and one `wgpu::Queue` (see `app.rs:49-74`). All compute dispatches (wave-front physics, perception, pedestrian adaptive) and all render passes (agent shapes, road lines, egui UI) are submitted sequentially through the same queue. Adding 3D rendering (buildings, terrain, depth buffer, shadow maps) and ML inference (YOLO detection via CoreML/ONNX Runtime) to this same queue creates a serial pipeline where each workload must complete before the next begins. On Apple Silicon, Metal supports only 2x concurrency across the entire GPU -- a single command queue cannot exploit even this limited parallelism.

The frame pipeline becomes: `[compute physics ~2ms] -> [3D render buildings ~4ms] -> [render agents ~1ms] -> [egui ~1ms] -> [ML inference via separate framework]`. Total: 8ms+ per frame, all serialized. The 11x headroom from the original architecture (8ms compute in 100ms budget) evaporates because 3D rendering was not in the original budget.

**Why it happens:**
wgpu's API encourages a single-queue pattern. The `request_device()` call returns one `Queue`. Creating multiple queues requires multiple `Device` instances from the same adapter, which wgpu does not support (WebGPU spec limitation -- one device per adapter per call). Developers assume "just add more passes to the encoder" is free, not realizing Metal serializes work within a single command queue. The existing `GpuState` struct bundles one `device` and one `queue` -- there is no mechanism to split work across Metal command queues.

**Consequences:**
- Frame time exceeds 16ms (60 FPS) when 3D rendering is active with 280K agents
- Simulation timestep stalls while 3D scene renders, causing timing jitter
- ML inference (if done via Metal/wgpu compute) blocks both simulation and rendering
- Cannot overlap compute and render even though Metal hardware supports it

**Prevention:**
1. **Separate ML inference from wgpu entirely.** Use ONNX Runtime (`ort` crate) with CoreML Execution Provider, which runs on the Neural Engine (ANE) and a separate Metal command queue internally. This avoids wgpu queue contention for ML workloads. The ANE is a dedicated accelerator that does not compete with GPU compute/render units.
2. **Use wgpu's `Device::create_command_encoder()` strategically.** Submit compute and render command buffers in separate `queue.submit()` calls within the same frame. While this does not create true parallelism on a single Metal queue, it allows Metal's scheduler to interleave work at command buffer boundaries.
3. **Budget 3D rendering as a separate frame cadence.** Render 3D buildings at 30 FPS while running simulation compute at 10 Hz. The 3D scene is mostly static (buildings don't move); only camera changes require re-rendering. Use a dirty flag to skip 3D render passes when the camera hasn't moved.
4. **Profile with Metal System Trace (Instruments.app).** Check GPU timeline for serial execution. Verify compute and render actually overlap at command buffer boundaries.

**Warning signs:**
- `frame_time_ms` in `SimMetrics` jumps from ~8ms to ~15ms+ when 3D rendering is enabled
- GPU utilization shows spikes (100% briefly, then idle, then 100% again) rather than sustained load
- Simulation `sim_time` advances in jerky increments rather than smooth 0.1s steps
- Enabling/disabling 3D rendering causes large frame time swing (>5ms delta)

**Phase to address:**
Phase 1 (Architecture). Design the frame pipeline with explicit compute/render/inference budgets before writing any 3D rendering code. The current `GpuState` struct needs restructuring to separate compute submission from render submission.

---

### Pitfall 2: YOLO Inference via Metal Compute Steals GPU from Simulation

**What goes wrong:**
Developers attempt to run YOLO inference directly via wgpu compute shaders or Metal Performance Shaders (MPS) to avoid adding an external dependency. This puts ML inference on the exact same GPU execution units as the traffic simulation compute. On Apple Silicon M1/M2/M3, there is a single GPU with limited concurrent kernel capacity. A YOLOv8n inference pass takes 17-21ms via MPS on M1 Max. During those 17-21ms, simulation compute dispatches are blocked or severely throttled. At 10 Hz simulation (100ms budget), a single YOLO inference consumes 17-21% of the entire frame budget.

With multiple camera streams (4-8 cameras typical for intersection monitoring), inference costs compound: 4 cameras x 17ms = 68ms, leaving only 32ms for the actual simulation + rendering.

**Why it happens:**
Apple Silicon's unified memory makes it tempting to "just run everything on GPU." The `candle-coreml` crate and `ort` with CoreML both offer Metal-accelerated inference. But "Metal-accelerated" means "uses the same GPU compute units as your simulation." Without explicit scheduling, the OS Metal scheduler interleaves ML and simulation work, causing both to slow down.

**Consequences:**
- Simulation frame rate drops below real-time (10 Hz) when inference is active
- Detection latency becomes unpredictable (17ms to 100ms+ depending on simulation load)
- Frame time variance increases, causing timing jitter in agent physics
- System appears to work in testing (1 camera, 10K agents) but fails at production scale (4 cameras, 280K agents)

**Prevention:**
1. **Route YOLO inference through the Apple Neural Engine (ANE), not GPU.** Export YOLO to CoreML format with ANE optimization. The ANE is a dedicated 16-core accelerator on M1+ chips that runs independently of GPU compute units. CoreML automatically selects ANE when the model supports it. YOLOv8n on ANE achieves ~5ms latency vs. ~17ms on GPU, and frees the GPU entirely for simulation.
2. **Use `ort` crate with CoreML Execution Provider.** Configure with `CoreMLExecutionProviderOptions` and let CoreML handle device selection (ANE > GPU > CPU fallback). Do not force GPU execution.
3. **Cap inference rate independently of simulation rate.** Camera detection at 5 FPS is sufficient for demand calibration (vehicles don't teleport between frames). Run inference on a separate `tokio::spawn_blocking` task at a fixed cadence, never in the simulation loop.
4. **Use YOLOv8n or YOLO11n (nano variants).** On M1 Pro ANE, YOLOv8n achieves 92+ FPS. Full YOLOv8m/l models are 3-5x slower and offer marginal accuracy improvement for vehicle counting (not fine-grained classification).

**Warning signs:**
- `ort::Session` created with GPU execution provider when ANE is available
- Inference latency >10ms per frame on Apple Silicon (indicates GPU, not ANE)
- `SimMetrics::frame_time_ms` increases proportionally with number of active cameras
- GPU utilization stays at 100% continuously (both sim and ML competing) rather than showing distinct compute/render phases

**Phase to address:**
Phase 1 (ML Spike). Run a 2-day spike: export YOLOv8n to CoreML, run via `ort` with CoreML EP, measure ANE utilization (via Instruments > Neural Engine) while simulation compute is active. Verify zero GPU contention.

---

### Pitfall 3: 3D Building Mesh Loading Exhausts Metal GPU Memory Budget

**What goes wrong:**
HCMC Districts 1, 3, 5, 10, and Binh Thanh contain an estimated 80,000-120,000 buildings. Naive loading of extruded building footprints (from OSM data) as 3D meshes produces 5-15M triangles. At ~40 bytes per vertex (position + normal + UV), this requires 200-600 MB of GPU buffer space. Add a terrain mesh, road surface mesh, and texture atlases, and total GPU memory for 3D rendering reaches 500MB-1.5GB.

The existing simulation already uses: position buffers (280K x 12B = 3.3MB), kinematics (280K x 8B = 2.2MB), IDM params (280K x 20B = 5.6MB), plus staging/readback buffers (~50MB total). The simulation's GPU memory is modest (~60MB), but Apple Silicon's unified memory is shared with the OS, apps, and the YOLO model (~30-50MB for YOLOv8n CoreML). On an M1 Pro with 16GB unified memory, the Metal GPU heap is capped at ~75% (12GB). After OS and app overhead, approximately 8-9GB is available. Loading 1.5GB of 3D geometry is feasible but leaves less headroom than expected.

The real problem is not total memory but **allocation patterns**. Creating thousands of individual `wgpu::Buffer` objects (one per building) causes Metal heap fragmentation and driver overhead. wgpu's buffer creation has non-trivial CPU cost (~0.1ms per buffer), and 100K individual buffers = 10 seconds of buffer creation.

**Why it happens:**
Developers treat each OSM building as an independent mesh object, creating a separate vertex buffer and index buffer per building. This is the natural approach when loading GeoJSON/OSM data building-by-building. It works for 100 buildings in a demo but collapses at city scale. The wgpu buffer creation overhead is hidden behind the `create_buffer` API, which appears cheap but involves Metal heap allocation.

**Consequences:**
- Application startup takes 10-30 seconds just for buffer creation (not counting mesh generation)
- Draw call count exceeds 100K per frame (one per building), overwhelming the Metal command encoder
- Metal heap fragmentation causes allocation failures for new simulation buffers mid-run
- Memory pressure triggers macOS jetsam (process kill) on lower-memory configs (8GB M1)

**Prevention:**
1. **Batch all building geometry into a single vertex buffer + single index buffer.** Use a geometry atlas: pack all building vertices contiguously, store per-building offset/count in a separate metadata buffer. Draw with indirect draw calls or a single `draw_indexed()` with instance ID-based lookup.
2. **Generate building geometry on GPU.** Upload only building footprint polygons (2D outlines + height) as a storage buffer. Use a compute shader or mesh shader (wgpu v28 supports mesh shaders on Metal) to extrude footprints to 3D on-the-fly. This reduces CPU-side data from ~600MB to ~20MB (footprints only).
3. **Implement LOD (Level of Detail).** Buildings beyond 500m from camera: flat colored rectangles (4 vertices). Buildings 100-500m: extruded boxes (24 vertices). Buildings <100m: full detail (if available). This reduces visible triangle count from millions to 50K-200K.
4. **Use frustum culling aggressively.** A typical viewport at street level shows 200-1000 buildings, not 100K. Implement CPU-side AABB frustum culling (or GPU-side via compute) to skip invisible buildings entirely.
5. **Memory budget check at startup.** Query `adapter.limits()` for `max_buffer_size` and the Metal heap size. If available GPU memory < required 3D geometry, reduce LOD or coverage area dynamically.

**Warning signs:**
- Buffer creation time >1 second during 3D scene loading
- Draw call count >10K per frame (check with Metal GPU profiler)
- `wgpu::Device::create_buffer` returning errors or panicking with "out of memory"
- macOS "Your system has run out of application memory" warning
- Simulation frame time degrades after loading 3D scene (not during loading, but during runtime)

**Phase to address:**
Phase 2 (3D Rendering). Start with LOD + batched geometry from day one. Never create per-building buffers. The existing `Renderer` struct's pattern of pre-creating vertex buffers (see `TRIANGLE_VERTICES`, `RECTANGLE_VERTICES` in `renderer.rs`) is the right pattern -- extend it with a batched building geometry buffer.

---

### Pitfall 4: Coordinate System Confusion Between Simulation, Geo, and Render Spaces

**What goes wrong:**
VELOS currently operates in three coordinate systems that are about to become five:

**Existing:**
1. **Edge-local** (meters along an edge + lateral offset) -- used by ECS `Position` component
2. **World-space** (projected meters, likely UTM or local Mercator) -- used by `AgentInstance.position` in renderer
3. **NDC/clip space** (wgpu's [-1,1] x [-1,1] x [0,1]) -- used by camera's `view_proj` matrix

**Adding:**
4. **WGS84 geographic** (lat/lon degrees) -- used by camera feeds with geo-referenced positions, OSM building footprints, RTSP camera GPS metadata
5. **3D world space** (x/y/z meters with elevation) -- needed for 3D building rendering and terrain

Converting between these systems introduces bugs at every boundary. The most pernicious: wgpu uses a **different NDC** than OpenGL. wgpu's depth range is [0, 1] (not [-1, 1]). wgpu's Y-axis in NDC points up, but texture coordinates have Y pointing down. The existing 2D renderer avoids this by using a simple orthographic projection, but 3D rendering with perspective projection, depth buffers, and terrain elevation exposes every NDC difference.

Additionally, the existing `Camera2D` in `camera.rs` produces a `view_proj_matrix()` using `glam::Mat4`. Switching to 3D perspective requires a fundamentally different camera model (position, look-at, up, FOV, near/far planes). If the 3D camera uses a different projection convention than the 2D orthographic (e.g., different handedness), agents will render at wrong positions in the 3D view.

**Why it happens:**
Each subsystem chooses the coordinate system most natural for its domain. Edge-local is natural for car-following (1D position along a lane). WGS84 is natural for geo-data (OSM, camera GPS). NDC is defined by the GPU API. Without a single authoritative coordinate transform pipeline, each developer writes their own conversion, introducing subtle sign flips, axis swaps, or off-by-one-hemisphere errors. The classic bug: lat/lon swapped (y before x vs x before y), producing a mirror-image city.

**Consequences:**
- Agents appear at wrong positions in 3D view (common: z-fighting with terrain, or agents floating above roads)
- Building footprints shifted by hundreds of meters (UTM zone mismatch or WGS84 datum confusion)
- Camera detection bounding boxes don't align with simulation positions (detection says "vehicle at lat,lon" but simulation has no agent at that projected position)
- Depth buffer artifacts: agents rendered behind buildings when they should be in front (depth range mismatch)

**Prevention:**
1. **Define a single canonical world coordinate system.** Use UTM Zone 48N (EPSG:32648) centered on HCMC. All data enters the system through a conversion from its native CRS to UTM 48N meters. All rendering uses UTM coordinates, with the camera projection converting to NDC. Document this in a `coords.rs` module with explicit conversion functions.
2. **Use `glam::Mat4::perspective_rh` (right-handed) for 3D.** wgpu uses a right-handed coordinate system with Y-up, Z into the screen, depth [0,1]. The OpenGL-to-wgpu correction matrix (`OPENGL_TO_WGPU_MATRIX` from learn-wgpu) must be applied if using any math library that assumes OpenGL conventions.
3. **Build coordinate transform tests from day one.** Test: convert a known HCMC intersection (e.g., Ben Thanh Market: 10.7725N, 106.6981E) from WGS84 -> UTM -> world-space -> clip-space -> screen pixels. Verify the pixel position matches the expected screen location. Run this test on every commit.
4. **Camera detection geo-referencing needs a calibration pipeline.** A camera's RTSP stream has pixel coordinates, not world coordinates. Converting detected bounding boxes to simulation positions requires camera intrinsics (focal length, distortion) + extrinsics (position, orientation). This is a full camera calibration problem, not just a coordinate transform.

**Warning signs:**
- Buildings and agents rendering in different positions that are offset by a constant amount
- Detected vehicles from camera CV appearing in wrong simulation edges
- Z-fighting (flickering) between terrain surface and road network
- 3D scene appears mirrored or upside-down when first implemented
- `Camera2D` and new `Camera3D` produce different world positions for the same screen pixel

**Phase to address:**
Phase 1 (Foundation). Define the coordinate pipeline before any 3D or CV code. The `coords.rs` module with tested conversion functions must exist before Phase 2 begins.

---

### Pitfall 5: Video Decode Pipeline Stalls Simulation Tick on Main Thread

**What goes wrong:**
RTSP camera streams are decoded on the CPU main thread or a `tokio` async task, but the decoded frames must be uploaded to GPU memory for ML inference. The decode -> upload -> infer pipeline introduces multiple blocking points:

1. **RTSP network I/O**: TCP/UDP reads are async but unpredictable (network jitter, packet loss, retransmits). A dropped packet causes ffmpeg to wait for retransmission (TCP) or produce a corrupt frame (UDP).
2. **Video decode**: Even with VideoToolbox hardware acceleration, decoded frames land in a `CVPixelBuffer` (macOS) or system memory. Getting pixels into a format suitable for YOLO input (typically RGB 640x640 float32) requires color space conversion (YUV -> RGB) and resize.
3. **Frame upload**: The decoded/preprocessed frame must reach the ML inference engine. If using wgpu compute for inference (pitfall 2), this means a `queue.write_buffer()` call that can stall if the GPU is busy.

The critical mistake: calling `ffmpeg` decode synchronously in the simulation tick loop. Even with VideoToolbox, a single 1080p H.264 frame decode takes 1-3ms. With 4 cameras, that's 4-12ms added to every simulation frame, consuming half the 8ms compute budget.

**Why it happens:**
Camera integration code is often prototyped with synchronous blocking calls ("just get it working"). The `retina` RTSP crate for Rust is async (tokio-based), but converting RTP/H.264 NAL units to decoded frames requires ffmpeg (via `ffmpeg-next` crate), which is synchronous. Bridging async RTSP -> sync ffmpeg -> async upload creates thread-blocking points that are invisible until profiled.

**Consequences:**
- Simulation frame time becomes dependent on camera stream health (network issues = sim stalls)
- Dropped RTSP connections cause the sim loop to hang waiting for reconnection
- VideoToolbox decode failures (corrupt stream, unsupported codec) crash the entire application
- Memory pressure from buffering undecoded RTSP frames (each 1080p I-frame: ~200KB)

**Prevention:**
1. **Run video decode on dedicated `std::thread` workers, not tokio or the sim loop.** One thread per camera stream. Decode produces frames into a bounded `crossbeam::channel` (capacity: 2-3 frames). The simulation tick reads the latest frame non-blockingly (`try_recv`). If no new frame is available, use the previous frame (detection on stale frames is better than stalling simulation).
2. **Use VideoToolbox via ffmpeg for hardware decode.** Configure `ffmpeg-next` with `-hwaccel videotoolbox` equivalent (`set_hwaccel("videotoolbox")`). This offloads H.264/H.265 decode to Apple's dedicated media engine, which is separate from both GPU and ANE. Decode latency drops from 3-5ms (CPU) to <1ms (media engine).
3. **Decouple detection rate from decode rate.** Decode at stream rate (25-30 FPS) but run detection at 5 FPS. Buffer the latest decoded frame; the detection thread grabs it when ready. Do not queue frames for detection -- always use the freshest frame.
4. **Handle stream failures gracefully.** Wrap each camera stream in a supervisor that reconnects with exponential backoff. A dead camera must never block or crash the simulation. Log the failure, mark detection data as stale, continue running.

**Warning signs:**
- `sim_time` advances in bursts (multiple 0.1s steps, then a pause) correlated with camera frame arrivals
- `frame_time_ms` has bimodal distribution (fast frames without decode, slow frames with decode)
- Memory usage grows continuously (unbounded frame buffer)
- Application hangs when a camera stream disconnects
- CPU core at 100% on one core (ffmpeg decode blocking a tokio worker)

**Phase to address:**
Phase 2 (Camera Integration). The video decode pipeline architecture must be designed before connecting any RTSP streams. Build the bounded-channel, dedicated-thread pattern first, test with a local video file, then add RTSP.

---

### Pitfall 6: Adding Depth Buffer and 3D Render Passes Breaks Existing 2D Pipeline

**What goes wrong:**
The existing `Renderer` in `renderer.rs` uses a 2D pipeline with **no depth buffer** (`depth_stencil: None` at line 237 and 285, `depth_stencil_attachment: None` at line 478). Agents are drawn in order: road lines first, then motorbikes, then cars, then pedestrians. This painter's algorithm works because everything is flat.

Adding 3D buildings requires a depth buffer. But the existing render pipeline was created without depth-stencil state. You cannot mix pipelines with and without depth testing in the same render pass without explicitly handling the depth attachment. The common mistake: developers add a depth buffer for 3D buildings but forget to update the existing agent pipeline to write/test depth. Result: agents always render on top of buildings (or always behind buildings), regardless of actual position.

Even worse: the current `Camera2D` uses an orthographic projection where Z is always 0 (see `agent_render.wgsl:38` -- `vec4<f32>(rotated + inst.world_pos, 0.0, 1.0)`). All agents have Z=0, so a depth buffer test with buildings at Z=10 (roof height) will either always pass or always fail, depending on the depth function.

**Why it happens:**
Adding 3D to an existing 2D renderer seems like an incremental change ("just add depth and a perspective camera"). But the 2D pipeline's assumptions are deeply embedded: no depth state in pipeline creation, Z hardcoded to 0 in the vertex shader, orthographic projection, no face culling. Each of these must change simultaneously, and forgetting any one produces incorrect rendering.

**Consequences:**
- Agents invisible (rendered behind terrain at Z=0 while terrain is at Z=0 too -- depth fighting)
- Buildings transparent (depth write disabled on building pipeline but enabled on agent pipeline)
- Performance regression from unnecessary depth testing on 2D elements
- Z-fighting flickering at road/terrain boundary

**Prevention:**
1. **Redesign the render pipeline as a multi-pass system from scratch.** Do not retrofit depth into the existing `Renderer`. Create a new `Renderer3D` that owns the full pipeline:
   - Pass 1: Terrain + buildings (opaque, depth write ON, depth test ON)
   - Pass 2: Road network on terrain surface (depth test ON, depth write OFF, polygon offset to prevent z-fighting)
   - Pass 3: Agents as 3D objects or billboards (depth test ON, depth write ON)
   - Pass 4: Transparent overlays, egui (depth test OFF)
2. **Assign meaningful Z values to agents.** Agents should be positioned at road surface elevation + a small offset (0.1-0.5m). This requires the road network to have elevation data (from terrain DEM) or a flat assumption (all roads at Z=0, buildings extruded upward).
3. **Keep the 2D renderer as a fallback.** The existing `Renderer` works. Do not delete it. Make 3D rendering toggleable (user presses a key to switch between 2D top-down and 3D perspective). This preserves a known-good rendering path while developing 3D.
4. **Create a shared depth texture.** The depth texture must persist across all render passes in a frame. Create it once per frame (or reuse with clear), attach to all passes that need it.

**Warning signs:**
- First 3D render attempt shows buildings but no agents (or agents but no buildings)
- Flickering at terrain/road boundary (z-fighting from equal Z values)
- Agent shapes look wrong in 3D (2D triangles viewed at an angle are paper-thin)
- Render pass validation errors from wgpu about mismatched depth-stencil state
- Performance drops >2x when depth testing is enabled (unexpected overdraw)

**Phase to address:**
Phase 2 (3D Rendering). Design the multi-pass pipeline before writing shaders. The `Renderer3D` should be a new struct, not a modification of the existing `Renderer`.

---

### Pitfall 7: Async Camera Inference Results Arrive at Wrong Simulation Time

**What goes wrong:**
Camera detection runs asynchronously: a camera frame captured at wall-clock T0 is decoded, preprocessed, and run through YOLO inference. The detection result (vehicle counts, positions) arrives at wall-clock T0 + 50-200ms (decode + inference + postprocessing latency). Meanwhile, the simulation has advanced 1-2 timesteps. The detection result describes traffic conditions at T0, but the simulation is now at T0 + 0.1-0.2s.

For demand calibration, this lag is acceptable (aggregate counts over minutes). For real-time simulation adjustment (e.g., "detected 5 cars entering intersection X, spawn them in sim"), the lag causes agents to spawn 1-2 timesteps late, creating a growing divergence between camera reality and simulation state. Over time, this accumulates: the simulation's traffic density at camera-visible intersections drifts from reality.

**Why it happens:**
Developers treat detection results as "current state" when they are actually "stale observation." The async pipeline hides the latency -- a detection callback fires, the handler reads the result, and assumes it's fresh. There is no timestamp correlation between the camera frame and the simulation clock.

**Consequences:**
- Agent spawning at camera-observed intersections is consistently late
- Calibration feedback loop oscillates (overcompensates because detection sees old state)
- Detection-driven signal adjustment responds to traffic conditions that have already changed
- Impossible to reproduce/validate: the timing offset varies with system load

**Prevention:**
1. **Timestamp every camera frame with the simulation clock, not wall clock.** When a camera frame is captured, record the current `sim_time`. When the detection result arrives, tag it with the originating `sim_time`. The simulation can then decide whether the result is still relevant (e.g., discard if >0.5s stale).
2. **Use detection for aggregate calibration, not real-time spawning.** Camera detections feed a rolling average of vehicle counts per intersection over 5-60 second windows. The demand calibration system adjusts OD matrix weights based on these aggregates, not individual detections. This is robust to latency.
3. **Never block the sim loop waiting for detection results.** Use `ArcSwap` (already used in `velos-predict` for prediction overlays) to atomically publish detection results. The simulation reads the latest available result without blocking.
4. **Build a latency histogram.** Track `detection_latency = sim_time_at_result_arrival - sim_time_at_frame_capture`. Alert if p95 exceeds 0.5s simulation time. This metric surfaces degradation before it affects calibration quality.

**Warning signs:**
- Calibration GEH metric oscillates rather than converging
- Agent counts at camera intersections consistently lag behind camera-observed counts
- Detection result processing shows in `frame_time_ms` spikes (blocking the sim loop)
- `ArcSwap` not used -- detection results processed synchronously in sim tick

**Phase to address:**
Phase 2 (Camera-Sim Integration). Design the timestamp correlation and `ArcSwap` publication pattern before connecting detection to the simulation. Test with simulated detection delays (inject artificial 50-200ms latency) to validate the aggregate calibration approach is robust.

---

## Moderate Pitfalls

### Pitfall 8: wgpu NDC Depth Range and Projection Matrix Mismatch

**What goes wrong:**
wgpu uses a depth range of [0, 1] (Metal and DX12 convention), not [-1, 1] (OpenGL convention). Most math libraries (including `glam`) produce projection matrices with OpenGL conventions by default. Using `glam::Mat4::perspective_rh` without the depth range correction produces a projection matrix that maps depth to [-1, 1]. Objects at the near plane get depth -1, which is outside wgpu's [0, 1] range, and are clipped.

The existing `Camera2D` works because orthographic 2D rendering maps everything to Z=0, which falls within any depth range. A 3D perspective camera immediately exposes this mismatch.

**Prevention:**
1. Use `glam::Mat4::perspective_rh` and apply the OpenGL-to-wgpu correction matrix:
   ```rust
   pub const OPENGL_TO_WGPU_MATRIX: glam::Mat4 = glam::Mat4::from_cols_array(&[
       1.0, 0.0, 0.0, 0.0,
       0.0, 1.0, 0.0, 0.0,
       0.0, 0.0, 0.5, 0.5,
       0.0, 0.0, 0.0, 1.0,
   ]);
   ```
   Apply as: `let proj = OPENGL_TO_WGPU_MATRIX * glam::Mat4::perspective_rh(fov, aspect, near, far);`
2. Or use `glam::Mat4::perspective_rh_wgpu()` if available in the glam version used.
3. Write a test: project a point at the near plane and verify its NDC Z is 0.0, not -1.0.

**Phase to address:** Phase 2 (3D Rendering). First task when implementing `Camera3D`.

---

### Pitfall 9: CoreML Model File Deployment and Cold Start Latency

**What goes wrong:**
CoreML models (`.mlpackage` or `.mlmodelc`) need compilation on first use. The first inference call triggers model compilation that can take 5-30 seconds. This happens every time the application starts unless the compiled model is cached. Additionally, CoreML models are macOS-specific -- they cannot be used on Linux (where the production deployment may eventually run).

**Prevention:**
1. **Pre-compile CoreML models at build time.** Use `coremltools` (Python) to compile `.mlpackage` to `.mlmodelc` during the build process. Ship the compiled model.
2. **Use ONNX as the portable format.** Keep `.onnx` as the canonical model format. Use `ort` with CoreML EP on macOS (auto-compiles to CoreML internally) and `ort` with CPU/CUDA EP on Linux. The same Rust code works on both platforms.
3. **Warm up the model at startup.** Run one dummy inference during `SimWorld` initialization to trigger compilation. Log the warmup time. Accept the 5-30s startup cost once.
4. **Cache compiled models.** CoreML caches compiled models in `~/Library/Caches/`. Ensure the cache directory is persistent across runs.

**Phase to address:** Phase 1 (ML Spike). Validate CoreML cold start during the YOLO spike. Measure startup time and document it.

---

### Pitfall 10: 3D Agent Models Replace 2D Instancing, Killing Performance

**What goes wrong:**
The current renderer uses 2D instanced rendering: 3-6 vertices per agent shape, one draw call per agent type. This is extremely efficient -- 280K agents = 280K instances = 3 draw calls. Replacing 2D shapes with 3D vehicle models (even low-poly: 100-500 triangles per vehicle) increases vertex count from ~1.7M to 28-140M triangles. This is a 16-82x increase in rasterization work.

Even more damaging: 3D models require per-model vertex buffers with different vertex layouts (position + normal + UV vs. the current position-only layout). Different vehicle types need different meshes (motorbike mesh, car mesh, bus mesh, truck mesh). Each mesh type requires a separate draw call with a different vertex buffer binding.

**Prevention:**
1. **Keep 2D instancing for distant agents.** Switch to 3D models only for agents within 200m of the camera. At typical 3D viewing angles, 90%+ of 280K agents are too far to see detail. Use the 2D triangle/rectangle/dot shapes for distant agents (existing code, zero cost to keep).
2. **Use impostor/billboard rendering for mid-distance agents.** A textured quad facing the camera (4 vertices) with a pre-rendered vehicle sprite is cheaper than a 3D mesh and looks acceptable at 50-200m distance.
3. **Implement GPU-driven LOD selection.** A compute shader determines which agents get 3D models vs. billboards vs. dots based on screen-space size. Output to an indirect draw buffer. This avoids CPU-side LOD decisions for 280K agents.
4. **Budget: max 50K 3D model triangles per frame.** This means at most 100-500 detailed 3D vehicles visible at once. The rest are billboards or dots.

**Phase to address:** Phase 3 (3D Agent Models). Start with LOD from day one. Never attempt to render 280K 3D models simultaneously.

---

### Pitfall 11: RTSP Stream Reliability on macOS Without Container/Service Infrastructure

**What goes wrong:**
RTSP camera connections are inherently fragile: cameras go offline, networks partition, streams switch codecs mid-session, TCP connections time out. On Linux, camera management is typically handled by a dedicated service (e.g., a GStreamer pipeline managed by systemd). On macOS (the development/deployment target), there is no equivalent service infrastructure. The VELOS binary must handle all stream lifecycle management directly.

Common failures: camera firmware reboots mid-stream (30-60s outage), WiFi cameras switch from 5GHz to 2.4GHz (causing IP change and stream URL invalidation), multiple RTSP streams exhaust macOS's file descriptor limit (default: 256 per process), and H.265 streams from newer cameras are decoded by VideoToolbox only on M1+ (older Intel Macs fail silently).

**Prevention:**
1. **Raise file descriptor limit.** Add `ulimit -n 4096` to launch scripts, or call `setrlimit` programmatically at startup. Each RTSP stream uses 2-4 file descriptors (TCP socket + RTP/RTCP).
2. **Implement per-stream health monitoring.** Track: last frame received timestamp, decode error count, reconnection count. Expose via the existing Prometheus metrics. Auto-disable streams that fail >3 consecutive reconnection attempts.
3. **Use the `retina` crate for RTSP.** It is purpose-built for IP camera integration in Rust, handles RTP depacketization, and supports both TCP and UDP interleaved transport. Prefer TCP transport for reliability over UDP (accept slightly higher latency).
4. **Test with stream interruption.** Simulate camera failures by killing the RTSP server mid-stream. Verify the simulation continues without impact. This must be an automated test, not a manual check.

**Phase to address:** Phase 2 (Camera Integration). Build the stream supervisor with health monitoring before connecting to real cameras.

---

### Pitfall 12: Mixing rayon, tokio, and Dedicated Decode Threads Causes Deadlocks

**What goes wrong:**
VELOS already uses rayon (CPU-bound simulation work) and tokio (async I/O for gRPC/WebSocket). Adding camera decode introduces a third threading model: dedicated `std::thread` workers for ffmpeg decode (which is synchronous and cannot run on tokio). Additionally, `ort` (ONNX Runtime) inference may internally spawn threads. The resulting thread pool interactions create deadlock risk:

- rayon worker calls `tokio::runtime::block_on()` -> deadlocks tokio runtime
- tokio task calls `ffmpeg_next::decode()` -> blocks async runtime, starving other tasks
- Dedicated decode thread calls `wgpu::Queue::write_buffer()` -> may block on GPU fence, holding a lock that the sim thread needs
- `ort` inference thread calls a callback that tries to acquire a mutex held by the sim thread

**Prevention:**
1. **Enforce strict threading boundaries.** Document and enforce in code review:
   - rayon: simulation physics, sorting, pathfinding (CPU-bound, no I/O)
   - tokio: RTSP network I/O, gRPC, WebSocket, file I/O (async, never CPU-bound)
   - std::thread: ffmpeg video decode, `ort` inference (synchronous blocking operations)
   - wgpu queue submission: main thread only (or a single designated render thread)
2. **Bridge with channels, not shared mutexes.** Use `crossbeam::channel` between decode threads and the sim thread. Use `tokio::sync::mpsc` between tokio tasks and std::threads. Never share a `Mutex` across rayon/tokio/std::thread boundaries.
3. **Do not call `wgpu::Queue::write_buffer()` from decode threads.** Decoded frames go into a channel; the main thread uploads to GPU as part of the frame pipeline. Only one thread should ever touch `wgpu::Queue`.
4. **The existing `sim.rs` already documents the rayon/tokio split.** Extend this pattern to the new threading model. Add a `camera_threads.rs` module that owns the decode thread pool.

**Phase to address:** Phase 1 (Architecture). Define the threading model before writing any camera or 3D code. Document in the architecture spec which operations happen on which thread type.

---

## Minor Pitfalls

### Pitfall 13: YOLOv8 Detects Vehicles in Images But Not HCMC Motorbikes

**What goes wrong:**
Standard YOLO models (COCO-trained) have a "motorcycle" class but are trained primarily on Western-style motorcycles (large cruisers, sport bikes). HCMC motorbikes are predominantly Honda Wave/Dream/Vision scooters that look visually different: smaller, riders wearing conical hats or full-face helmets, 2-3 riders per bike, cargo strapped to the back. Detection accuracy for HCMC motorbikes will be lower than the model's published mAP.

**Prevention:**
1. Fine-tune YOLOv8n on HCMC traffic footage. Collect 500-1000 annotated frames from target camera locations. Use Ultralytics training with the COCO-pretrained base. Fine-tuning takes 2-4 hours on M1 GPU.
2. For initial integration, accept lower accuracy. Vehicle counting (not classification) is the primary use case for demand calibration. Even 70% recall produces useful aggregate counts when averaged over 60-second windows.
3. Validate detection accuracy per vehicle type. Report precision/recall separately for motorbikes, cars, buses, pedestrians. If motorbike recall <60%, the demand model will systematically undercount the dominant vehicle type.

**Phase to address:** Phase 3 (Calibration). After the detection pipeline is working, fine-tune the model with HCMC-specific data.

---

### Pitfall 14: 3D Terrain Elevation Data Unavailable for HCMC

**What goes wrong:**
3D rendering assumes terrain elevation data (DEM - Digital Elevation Model) to create a ground surface for buildings to sit on. HCMC is extremely flat (average elevation 2m above sea level), so developers assume "just use Z=0 everywhere." But the Saigon River and canal network create elevation changes of 5-10m that matter for visual quality. Free DEM data (SRTM, ASTER GDEM) has 30m resolution -- too coarse for city streets. High-resolution LiDAR DEM data for HCMC is not publicly available.

**Prevention:**
1. **Start with flat terrain (Z=0 everywhere).** This is visually acceptable for HCMC because the city is genuinely flat. Do not block 3D rendering on terrain data availability.
2. **Add canal/river water surfaces as flat polygons at Z=-2m.** This provides sufficient visual differentiation without real DEM data.
3. **If elevation matters later, use OpenStreetMap contour data** (available from OpenTopography) or FABDEM (forest and building removed DEM, 30m resolution). For HCMC's flat terrain, even 30m resolution DEM is adequate for visualization.

**Phase to address:** Phase 3 (Polish). Flat terrain first, elevation later if needed.

---

### Pitfall 15: egui Integration Conflicts with 3D Render Pipeline

**What goes wrong:**
The current app uses `egui-wgpu` for UI rendering, which manages its own render pass and texture resources. Adding a 3D render pipeline with depth buffers, multiple render passes, and custom texture formats can conflict with `egui-wgpu`'s expectations. Specifically, `egui-wgpu::Renderer` expects to render to a surface texture without depth attachment. If the 3D pipeline changes the surface texture format or adds a depth attachment that persists across passes, egui rendering may fail with format mismatch errors.

**Prevention:**
1. **Render egui as the final pass, always without depth.** Clear the depth attachment before the egui pass, or use a separate render pass descriptor without depth.
2. **Do not change the surface texture format.** The 3D pipeline should use the same surface format (from `surface.get_capabilities()`) as the 2D pipeline. Use a separate depth texture, not a combined depth-stencil surface format.
3. **Consider migrating from egui to a 3D-native UI library** if the UI needs to exist in 3D space (e.g., labels floating above intersections). But for dashboard controls, egui is fine -- just keep it as a 2D overlay pass.

**Phase to address:** Phase 2 (3D Rendering). Test egui rendering after adding 3D passes to verify no format conflicts.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| ML Spike (Phase 1) | YOLO runs on GPU instead of ANE, stealing compute from sim | Force CoreML EP in `ort`, verify ANE usage via Instruments |
| Coordinate System (Phase 1) | No canonical CRS defined, each subsystem uses different coords | Create `coords.rs` with UTM 48N as canonical, test with known lat/lon points |
| Camera Decode (Phase 2) | Synchronous ffmpeg decode blocks sim loop | Dedicated std::thread workers with bounded crossbeam channels |
| 3D Buildings (Phase 2) | Per-building buffers, 100K draw calls | Batched geometry atlas, LOD, frustum culling from day one |
| Depth Buffer (Phase 2) | Existing 2D pipeline breaks when depth is added | New `Renderer3D` struct, keep `Renderer` as fallback |
| Detection-Sim Sync (Phase 2) | Detection results applied to wrong simulation timestep | Timestamp correlation, ArcSwap publication, aggregate windowing |
| Thread Model (Phase 2) | rayon/tokio/std::thread deadlocks from cross-boundary calls | Strict threading boundaries, channel-only bridging |
| Agent 3D Models (Phase 3) | 280K 3D models = 140M triangles = GPU death | LOD: 3D models <200m, billboards 200-500m, dots >500m |
| HCMC Detection Accuracy (Phase 3) | COCO-trained YOLO misses HCMC scooters | Fine-tune on 500+ HCMC annotated frames |
| Stream Reliability (Phase 2) | Camera disconnect crashes simulation | Per-stream supervisor, bounded channels, graceful degradation |

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Single queue serialization | MEDIUM | Refactor to separate compute/render submission windows. Move ML to ANE via CoreML. ~1 week |
| YOLO on GPU not ANE | LOW | Reconfigure `ort` session to use CoreML EP. Model re-export if needed. ~1-2 days |
| Building mesh OOM | MEDIUM | Rewrite mesh loading to batched atlas. Implement LOD. ~1 week |
| Coordinate system confusion | HIGH | Retrofit coordinate transforms across entire codebase. Every subsystem affected. ~2-3 weeks if not designed upfront |
| Video decode blocking sim | MEDIUM | Extract decode to dedicated threads. Add bounded channels. ~3-5 days |
| Depth buffer breaks 2D | LOW | Create separate `Renderer3D`, keep original `Renderer`. ~2-3 days |
| Async timing mismatch | MEDIUM | Add timestamp correlation, switch to aggregate windowed calibration. ~1 week |
| 280K 3D models | MEDIUM | Implement LOD system, GPU-driven selection. ~1-2 weeks |
| Threading deadlocks | HIGH | Debug deadlocks is notoriously difficult. Prevention via architecture is 10x cheaper than debugging. ~1-3 weeks if deadlocks manifest in production |

## "Looks Done But Isn't" Checklist

- [ ] **ML inference on ANE:** Session runs, produces results, but actually executing on GPU. Check with Instruments > Neural Engine trace. If ANE utilization is 0%, inference is on GPU.
- [ ] **VideoToolbox decode "working":** ffmpeg reports videotoolbox active, but is falling back to software decode for some frames (B-frames, certain profiles). Check decode time consistency -- software fallback frames take 5-10x longer.
- [ ] **3D buildings "rendering":** Buildings visible but frustum culling not implemented. Drawing 100K invisible buildings behind the camera. Check draw call count in Metal profiler vs visible building count.
- [ ] **Camera calibration "done":** Intrinsics calibrated with checkerboard, but extrinsics (camera-to-world transform) estimated by eye. Detected positions will be systematically offset from simulation positions.
- [ ] **Coordinate transforms "tested":** Tested with one point, but longitude/latitude order varies between libraries (GeoJSON: [lon, lat], most other formats: [lat, lon]). Test with points in all four quadrants of the UTM zone.
- [ ] **Depth buffer "working":** Depth test passes, but depth precision is insufficient for the scene scale. At 50km scene extent with near=0.1, far=100000, 24-bit depth buffer has <1m precision at far plane. Use logarithmic depth or tighter near/far planes.
- [ ] **RTSP reconnection "handled":** Reconnects after clean disconnect, but not after network timeout (60s TCP keepalive). Test by unplugging ethernet/disabling WiFi mid-stream, not by stopping the RTSP server cleanly.
- [ ] **LOD "implemented":** LOD transitions visible as popping (sudden geometry change). Need smooth transition: cross-fade or dithered LOD blending over 2-3 frames.

## Sources

- [wgpu v28.0.0 Release -- Mesh Shaders support on Metal](https://github.com/gfx-rs/wgpu/releases/tag/v28.0.0)
- [Apple Metal Command Queue documentation -- queue concurrency model](https://developer.apple.com/documentation/metal/mtlcommandqueue)
- [Metal Compute on MacBook Pro -- GPU scheduling](https://developer.apple.com/videos/play/tech-talks/10580/)
- [Apple Silicon GPU: 2x concurrency across entire GPU](https://github.com/philipturner/metal-benchmarks)
- [ONNX Runtime CoreML Execution Provider -- ANE support](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html)
- [ort crate -- ONNX Runtime Rust bindings with CoreML](https://crates.io/crates/ort/1.13.2)
- [Rust ORT ONNX Real-Time YOLO on Webcam (2025)](https://medium.com/@alfred.weirich/rust-ort-onnx-real-time-yolo-on-a-live-webcam-part-2-d74efc01bae0)
- [candle-coreml -- Rust CoreML bridge](https://crates.io/crates/candle-coreml)
- [YOLOv8 macOS Metal benchmarks -- M4 Pro: 92.6 FPS YOLOv8n](https://blog.roboflow.com/putting-the-new-m4-macs-to-the-test/)
- [YOLOv8 Metal vs CUDA benchmark paper](https://www.researchgate.net/publication/394305460_Benchmarking_YOLOv8-Tiny_for_Real-Time_Object_Detection_on_macOS_A_Comparison_of_Metal_and_CUDA_Performance)
- [YOLOv8 MPS backend: 16-21ms per frame with MPS, 68-71ms without](https://n-ahamed36.medium.com/running-yolov8-on-apple-silicon-with-mps-backend-a-simplified-guide-84b1d382f79c)
- [Apple Silicon unified memory: Metal caps GPU memory at ~75% of unified RAM](https://scalastic.io/en/apple-silicon-vs-nvidia-cuda-ai-2025/)
- [Metal 4 -- embed ML inference in render pipeline](https://medium.com/@shivashanker7337/apples-metal-4-the-graphics-api-revolution-nobody-saw-coming-a2e272be4d57)
- [VideoToolbox hardware decode -- 4x faster than software](https://www.martin-riedl.de/2020/12/06/using-hardware-acceleration-on-macos-with-ffmpeg/)
- [ez-ffmpeg Rust crate -- VideoToolbox hwaccel support](https://docs.rs/ez-ffmpeg/latest/ez_ffmpeg/core/hwaccel/index.html)
- [retina crate -- Rust RTSP client for IP cameras](https://lib.rs/crates/retina)
- [wgpu NDC coordinates: Y-up, depth [0,1]](https://wgpu-py.readthedocs.io/en/stable/guide.html)
- [Learn Wgpu -- OPENGL_TO_WGPU_MATRIX correction](https://sotrh.github.io/learn-wgpu/beginner/tutorial6-uniforms/)
- [Mesh shaders for LOD and culling in Metal](https://metalbyexample.com/mesh-shaders/)

---
*Pitfalls research for: VELOS -- Adding Camera CV Detection + 3D wgpu Rendering to Existing Traffic Microsimulation*
*Researched: 2026-03-09*
