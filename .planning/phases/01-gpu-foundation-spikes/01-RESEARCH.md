# Phase 1: GPU Pipeline & Visual Proof - Research

**Researched:** 2026-03-06
**Domain:** wgpu GPU compute + render pipeline, hecs ECS, winit windowing on macOS Metal
**Confidence:** HIGH

## Summary

Phase 1 is a greenfield technical validation: prove that wgpu compute shaders work on Apple Silicon Metal, that hecs ECS data can round-trip through GPU buffers, and that 1K agents can render as GPU-instanced 2D shapes in a winit window at 60 FPS. No simulation logic beyond position += velocity * dt.

The stack is well-established: wgpu 28.x provides compute + render on a single device, hecs 0.11.x provides a minimal ECS with columnar storage ideal for SoA GPU projection, and winit 0.30.x provides the macOS window with the new `ApplicationHandler` trait pattern. All three are actively maintained and interoperate cleanly. The main integration challenge is structuring compute dispatch and render pass within the same frame loop (same command encoder, same device/queue).

**Primary recommendation:** Use wgpu 28, hecs 0.11, winit 0.30, bytemuck for zero-copy buffer casting. Structure the frame loop as: ECS query -> write_buffer -> compute pass -> render pass -> present. Double-buffer agent data from the start. Use orthographic projection with a uniform buffer for zoom/pan camera.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- f64 on CPU, f32 on GPU -- no fixed-point types for POC
- No emulated i64 in WGSL, no golden test vectors, no Python script
- Tolerance-based comparison (not bitwise) for CPU-GPU parity: f32 precision (~7 decimal digits)
- CFL check stays (simple and useful, uses f64 on CPU)
- Simple parallel dispatch -- every agent updated independently in one compute pass
- No wave-front (Gauss-Seidel), no per-lane leader sort, no dual-leader tracking
- No PCG hash -- add when stochastic behavior needed (Phase 2/3)
- Workgroup size 256, ceil_div for non-multiple agent counts
- winit window from Phase 1 -- visual feedback from day one
- GPU-instanced 2D rendering: one instanced draw call per shape type
- Styled shapes with direction arrows (triangles for moving agents, dots for stationary)
- Zoom/pan camera controls
- Road lanes and intersection areas visible (placeholder geometry in Phase 1, real OSM in Phase 2)
- No egui in Phase 1 -- controls come in Phase 2
- Create only velos-core + velos-gpu (no empty scaffolds for other crates)
- Local-only quality gates (no CI in Phase 1)
- Nightly Rust toolchain pinned via rust-toolchain.toml
- Workspace-level dependency declarations in root Cargo.toml
- MIT license
- main + feature branches (simple trunk-based)
- ECS component structs (Position, Kinematics) defined in velos-core using f64 types
- velos-gpu owns GPU buffer layout (f32 SoA buffers), rendering pipeline, and compute pipeline
- velos-gpu exposes high-level API (ComputeDispatcher, BufferPool, Renderer) -- no raw wgpu leakage
- Per-crate error types with thiserror (#[from] wrapping)
- GPU integration tests gated by feature flag AND runtime skip (feature = "gpu-tests" + adapter check)
- Tolerance-based comparison for f32 results
- Dense index array maps GPU index -> ECS entity (rebuild on spawn/despawn)
- Double buffering from the start (front/back GPU buffers, swap per frame)
- Built-in #[bench] harness (nightly) for benchmarks
- Four benchmark metrics: GPU dispatch time, buffer readback time, full round-trip time, agents per second
- Benchmark results written to JSON baseline file (benchmarks/baseline.json)

### Claude's Discretion
- GPU buffer stride and alignment choices
- Internal module organization within each crate
- wgpu adapter selection and device configuration
- Benchmark iteration counts and warm-up strategy
- Exact instanced rendering implementation (vertex pulling vs instance buffer)
- Camera zoom/pan implementation details
- Placeholder road geometry for Phase 1 rendering

### Deferred Ideas (OUT OF SCOPE)
- Fixed-point arithmetic (Q16.16/Q12.20/Q8.8) -- v2 for 280K deterministic scale
- Wave-front (Gauss-Seidel) dispatch -- v2 for convergence at scale
- PCG deterministic hash -- add in Phase 2/3 when stochastic behavior needed
- Per-lane leader sort with dual-leader tracking -- v2 with wave-front

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| GPU-01 | GPU compute pipeline dispatches agent position/velocity updates each timestep via wgpu/Metal compute shaders using simple parallel dispatch | wgpu compute pipeline pattern (device init, shader module, compute pass, dispatch_workgroups) documented below |
| GPU-02 | f64 arithmetic on CPU, f32 in WGSL shaders. No fixed-point types for POC | f64 ECS components in velos-core, f32 SoA buffers in velos-gpu, tolerance comparison strategy |
| GPU-03 | hecs ECS stores agent state as components, projected to SoA GPU buffers each frame via queue.write_buffer() with entity-to-GPU index mapping | hecs query API + bytemuck cast_slice + dense index array pattern |
| GPU-04 | CFL numerical stability check validates dt * max_speed < cell_size before each simulation step | Trivial f64 CPU check -- no GPU involvement, returns bool |
| REN-01 | winit native macOS window hosts wgpu render surface with compute and render sharing the same device | Single device/queue serves both compute and render pipelines; ApplicationHandler pattern |
| REN-02 | GPU-instanced 2D renderer draws styled agent shapes with direction arrows | Instance buffer with VertexStepMode::Instance, per-type draw calls |
| REN-03 | Zoom/pan camera controls, visible road lanes, intersection areas marked | Orthographic projection uniform buffer, mouse/scroll event handling |
| REN-04 | One instanced draw call per vehicle type for rendering performance | Separate instance buffers per shape type, one draw_indexed per type |
| PERF-01 | Frame time benchmark measures GPU dispatch + buffer readback duration per simulation step | Nightly #[bench] harness with std::time::Instant around dispatch + readback |
| PERF-02 | Agent throughput metric tracks agents processed per second and GPU utilization percentage | agents_count / frame_time calculation; GPU utilization via timestamp queries if available |

</phase_requirements>

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| wgpu | 28.x | GPU compute + render API | Cross-platform WebGPU impl; Metal backend on macOS; single crate for both compute and render |
| hecs | 0.11.x | Minimal ECS | Columnar (SoA-like) storage, no framework overhead, query-based iteration, ideal for GPU projection |
| winit | 0.30.x | Window creation + event loop | Standard Rust windowing; ApplicationHandler trait for macOS event loop |
| bytemuck | 1.x | Zero-copy buffer casting | Pod/Zeroable derives for safe cast between Rust structs and GPU byte buffers |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| thiserror | 2.x | Error type derivation | Per-crate error enums with #[from] wrapping |
| log + env_logger | 0.4 / 0.11 | Logging | Debug output during development, wgpu validation messages |
| glam | 0.29.x | Math (vec2, mat4) | Camera projection matrix, transform math for rendering |
| pollster | 0.4.x | Async block_on | wgpu async init (request_adapter, request_device) in sync context |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| hecs | bevy_ecs | bevy_ecs is heavier, brings scheduler/system concepts not needed here |
| glam | nalgebra | nalgebra more complete but heavier; glam is wgpu-ecosystem standard |
| pollster | tokio | tokio is overkill for blocking on 2 async calls during init |
| nightly #[bench] | criterion | criterion is stable but heavier; nightly is fine since we already pin nightly for wgpu features |

**Installation (root Cargo.toml workspace dependencies):**
```toml
[workspace.dependencies]
wgpu = "28"
hecs = "0.11"
winit = "0.30"
bytemuck = { version = "1", features = ["derive"] }
thiserror = "2"
log = "0.4"
env_logger = "0.11"
glam = "0.29"
pollster = "0.4"
```

## Architecture Patterns

### Recommended Project Structure

```
velos/
  Cargo.toml              # workspace root
  rust-toolchain.toml     # nightly pinned
  crates/
    velos-core/
      src/
        lib.rs
        components.rs     # Position, Kinematics (f64)
        error.rs
      tests/
    velos-gpu/
      src/
        lib.rs
        device.rs         # GpuContext: device, queue, surface
        compute.rs        # ComputeDispatcher: pipeline, bind groups, dispatch
        buffers.rs        # BufferPool: double-buffered SoA agent buffers
        renderer.rs       # Renderer: render pipeline, instanced draw
        camera.rs         # Camera2D: orthographic projection, zoom/pan
        error.rs
      shaders/
        agent_update.wgsl # compute shader
        agent_render.wgsl # vertex + fragment shader
      tests/
        gpu_round_trip.rs
      benches/
        dispatch.rs
  benchmarks/
    baseline.json         # benchmark regression baseline
```

### Pattern 1: ECS-to-GPU Projection (SoA Write)

**What:** Query hecs World for (Position, Kinematics), convert f64 -> f32, write as separate SoA buffers to GPU.
**When to use:** Every frame before compute dispatch.

```rust
// velos-gpu/src/buffers.rs
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuPosition {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuKinematics {
    pub vx: f32,
    pub vy: f32,
    pub speed: f32,
    pub heading: f32, // radians, for rendering direction arrows
}

pub struct BufferPool {
    pos_front: wgpu::Buffer,
    pos_back: wgpu::Buffer,
    kin_front: wgpu::Buffer,
    kin_back: wgpu::Buffer,
    index_map: Vec<hecs::Entity>, // GPU index -> ECS entity
    capacity: u32,
}

impl BufferPool {
    pub fn upload_from_ecs(&mut self, world: &hecs::World, queue: &wgpu::Queue) {
        let mut positions = Vec::with_capacity(self.capacity as usize);
        let mut kinematics = Vec::with_capacity(self.capacity as usize);
        self.index_map.clear();

        for (entity, (pos, kin)) in world.query::<(&Position, &Kinematics)>().iter() {
            self.index_map.push(entity);
            positions.push(GpuPosition {
                x: pos.x as f32,
                y: pos.y as f32,
            });
            kinematics.push(GpuKinematics {
                vx: kin.vx as f32,
                vy: kin.vy as f32,
                speed: kin.speed as f32,
                heading: kin.heading as f32,
            });
        }

        queue.write_buffer(&self.pos_back, 0, bytemuck::cast_slice(&positions));
        queue.write_buffer(&self.kin_back, 0, bytemuck::cast_slice(&kinematics));
    }

    pub fn swap(&mut self) {
        std::mem::swap(&mut self.pos_front, &mut self.pos_back);
        std::mem::swap(&mut self.kin_front, &mut self.kin_back);
    }
}
```

### Pattern 2: Compute + Render in Same Frame

**What:** Single command encoder with compute pass first, then render pass. Both use same device/queue.
**When to use:** Every frame in the winit event loop.

```rust
// Frame execution order:
// 1. Upload ECS -> GPU buffers (queue.write_buffer)
// 2. Create command encoder
// 3. Begin compute pass -> dispatch agent_update.wgsl -> end compute pass
// 4. Begin render pass -> bind camera uniform -> draw instanced agents -> end render pass
// 5. Submit encoder -> present surface

fn render_frame(&mut self) -> Result<(), wgpu::SurfaceError> {
    let output = self.surface.get_current_texture()?;
    let view = output.texture.create_view(&Default::default());

    let mut encoder = self.device.create_command_encoder(&Default::default());

    // Compute pass
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&self.compute_pipeline);
        pass.set_bind_group(0, &self.compute_bind_group, &[]);
        let workgroups = (self.agent_count + 255) / 256;
        pass.dispatch_workgroups(workgroups, 1, 1);
    }

    // Render pass
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.1, g: 0.1, b: 0.15, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(0, self.shape_vertex_buffer.slice(..));
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));
        pass.draw(0..self.shape_vertex_count, 0..self.agent_count);
    }

    self.queue.submit(std::iter::once(encoder.finish()));
    output.present();
    Ok(())
}
```

### Pattern 3: winit ApplicationHandler Integration

**What:** Implement ApplicationHandler trait for the app struct with Option<State> pattern.
**When to use:** App entry point.

```rust
struct App {
    state: Option<GpuState>,
}

impl ApplicationHandler for App {
    fn can_create_surfaces(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let window = event_loop.create_window(
                WindowAttributes::default()
                    .with_title("VELOS - GPU Pipeline Proof")
                    .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
            ).unwrap();
            // pollster::block_on for async wgpu init
            self.state = Some(pollster::block_on(GpuState::new(window)));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => state.resize(size),
            WindowEvent::RedrawRequested => {
                state.update(); // ECS step + GPU upload
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        state.resize(state.window.inner_size());
                    }
                    Err(e) => log::error!("Render error: {e}"),
                }
                state.window.request_redraw(); // continuous rendering
            }
            WindowEvent::MouseWheel { delta, .. } => state.camera.zoom(delta),
            WindowEvent::CursorMoved { position, .. } => state.camera.cursor_moved(position),
            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                state.camera.mouse_input(button, btn_state);
            }
            _ => {}
        }
    }
}
```

### Pattern 4: Camera2D with Orthographic Projection

**What:** 2D orthographic camera with zoom (scroll wheel) and pan (middle-mouse drag).
**When to use:** All rendering.

```rust
pub struct Camera2D {
    pub center: glam::Vec2,    // world-space center
    pub zoom: f32,             // pixels per world-unit
    pub viewport: glam::Vec2,  // window size in pixels
    // Pan state
    is_panning: bool,
    last_cursor: glam::Vec2,
}

impl Camera2D {
    pub fn view_proj_matrix(&self) -> glam::Mat4 {
        let half_w = self.viewport.x / (2.0 * self.zoom);
        let half_h = self.viewport.y / (2.0 * self.zoom);
        glam::Mat4::orthographic_rh(
            self.center.x - half_w,
            self.center.x + half_w,
            self.center.y - half_h,
            self.center.y + half_h,
            -1.0,
            1.0,
        )
    }
}
```

The view-projection matrix is uploaded as a uniform buffer each frame via `queue.write_buffer`.

### Pattern 5: Double Buffering

**What:** Two sets of GPU buffers (front/back). GPU reads front during dispatch; CPU writes to back. Swap each frame.
**When to use:** From day one -- prevents CPU/GPU race conditions.

The compute shader reads from `pos_front`/`kin_front` and writes to `pos_back`/`kin_back`. After submit, swap front and back. The render pass reads from the freshly-written back (now front after swap) for display.

Buffer usage flags:
- Front buffers: `STORAGE | VERTEX` (read by compute, read by render as instance data)
- Back buffers: `STORAGE | COPY_DST` (written by compute, written by CPU upload)

### Anti-Patterns to Avoid

- **Single-buffering GPU data:** Causes race conditions between CPU upload and GPU read. Always double-buffer.
- **Mapping buffers for readback every frame:** Only map staging buffers when you need CPU-side results (tests, benchmarks). Normal frame loop should NOT read back -- compute writes to buffer, render reads same buffer.
- **Leaking raw wgpu types outside velos-gpu:** The rest of the codebase should never see `wgpu::Device`, `wgpu::Buffer`, etc. Wrap in high-level API.
- **AoS GPU buffers:** Use SoA (separate buffer per component type), not AoS (interleaved struct). SoA is better for GPU cache lines when shaders only need subset of fields.
- **Blocking on device.poll(Wait) in render loop:** Only use `poll(Wait)` for test readback. In render loop, use non-blocking poll or let wgpu handle it internally.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Matrix math | Custom mat4/vec2 | glam | Tested, SIMD-optimized, wgpu-ecosystem standard |
| Buffer byte casting | unsafe transmute | bytemuck::cast_slice | Safe, validates alignment and size at compile time via Pod/Zeroable |
| Async runtime for wgpu init | tokio/custom | pollster::block_on | Only 2 async calls needed (request_adapter, request_device); pollster is minimal |
| Error types | manual impl Error | thiserror derive | Less boilerplate, #[from] for wrapping |
| Window event loop | custom loop | winit ApplicationHandler | Platform-correct event handling on macOS |
| WGSL validation | manual testing | naga --validate | Catches shader errors at build/test time |

**Key insight:** For Phase 1, the compute shader is trivially simple (pos += vel * dt). The complexity is in the plumbing: buffer layout, bind groups, pipeline creation, frame synchronization. Use existing crates for everything except the actual GPU dispatch logic.

## Common Pitfalls

### Pitfall 1: wgpu Async Init in Sync Context
**What goes wrong:** `request_adapter()` and `request_device()` are async. New developers try to use tokio or create complex async runtimes.
**Why it happens:** wgpu API is designed for WebGPU compatibility where everything is async.
**How to avoid:** Use `pollster::block_on()` for the two init calls. No async runtime needed.
**Warning signs:** Seeing `tokio` in dependencies for a windowed app.

### Pitfall 2: Buffer Usage Flags Mismatch
**What goes wrong:** Creating a buffer with wrong usage flags causes wgpu validation errors at runtime.
**Why it happens:** Each buffer operation requires specific flags (STORAGE for compute, VERTEX for render, COPY_DST for write_buffer, MAP_READ for readback).
**How to avoid:** Carefully plan buffer lifecycle. Document which operations each buffer supports. For double-buffered agent data: front needs STORAGE | VERTEX, back needs STORAGE | COPY_DST. Staging buffer for test readback needs MAP_READ | COPY_DST.
**Warning signs:** wgpu validation layer errors about "buffer usage".

### Pitfall 3: Forgetting copy_buffer_to_buffer Before Readback
**What goes wrong:** Trying to map a storage buffer directly for CPU read fails.
**Why it happens:** GPU storage buffers cannot be mapped. Must copy to a staging buffer with MAP_READ usage first.
**How to avoid:** Always create a separate staging buffer for readback. Encode `copy_buffer_to_buffer` before submitting. Then `map_async` + `device.poll(Wait)` on the staging buffer.
**Warning signs:** Test hangs or panics on `map_async`.

### Pitfall 4: winit ApplicationHandler Async Trap
**What goes wrong:** Trying to make ApplicationHandler methods async for wgpu init.
**Why it happens:** winit's ApplicationHandler methods are sync, but wgpu init is async.
**How to avoid:** Use `pollster::block_on()` inside `can_create_surfaces()` or `resumed()`. Use `Option<State>` pattern -- state is None until first resume.
**Warning signs:** Lifetime errors or borrow checker fights in event loop.

### Pitfall 5: f32 Precision Comparison in Tests
**What goes wrong:** Tests fail with `assert_eq!` on f32 values that should match but differ by ULP.
**Why it happens:** f32 has ~7 decimal digits of precision. CPU f64 -> GPU f32 -> CPU readback introduces rounding.
**How to avoid:** Use tolerance-based comparison: `(a - b).abs() < epsilon` where epsilon depends on value range. For positions in 0..1000m range, epsilon = 0.01 is reasonable. For velocities 0..50 m/s, epsilon = 0.001.
**Warning signs:** Intermittent test failures on different hardware.

### Pitfall 6: Workgroup Dispatch Count Off-by-One
**What goes wrong:** Last agents in buffer don't get updated.
**Why it happens:** `dispatch_workgroups` takes workgroup count, not thread count. With workgroup_size(256) and 1000 agents, need `ceil(1000/256) = 4` workgroups (1024 threads). Shader must bounds-check `if (gid.x >= agent_count) { return; }`.
**How to avoid:** Always use `(agent_count + workgroup_size - 1) / workgroup_size` for dispatch. Always bounds-check in shader.
**Warning signs:** Last few agents frozen at initial position.

### Pitfall 7: Surface Configuration on macOS
**What goes wrong:** Surface doesn't render or panics on present.
**Why it happens:** Must configure surface after window creation with correct format and present mode.
**How to avoid:** Get preferred format via `surface.get_capabilities(&adapter).formats[0]`. Use `PresentMode::Fifo` (vsync) or `AutoVsync` for smooth 60 FPS.
**Warning signs:** Black window or "surface not configured" panic.

## Code Examples

### WGSL Compute Shader: Simple Agent Update

```wgsl
// shaders/agent_update.wgsl
// Simple parallel dispatch: each agent updated independently

struct Params {
    agent_count: u32,
    dt: f32,
}

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> pos_in: array<vec2<f32>>;
@group(0) @binding(2) var<storage, read> kin_in: array<vec4<f32>>; // vx, vy, speed, heading
@group(0) @binding(3) var<storage, read_write> pos_out: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read_write> kin_out: array<vec4<f32>>;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= params.agent_count) {
        return;
    }

    let pos = pos_in[idx];
    let kin = kin_in[idx];
    let vx = kin.x;
    let vy = kin.y;

    // Simple Euler integration: pos += vel * dt
    let new_pos = pos + vec2<f32>(vx, vy) * params.dt;

    pos_out[idx] = new_pos;
    kin_out[idx] = kin; // velocity unchanged in Phase 1
}
```

### WGSL Render Shader: Instanced 2D Shapes

```wgsl
// shaders/agent_render.wgsl

struct CameraUniform {
    view_proj: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) local_pos: vec2<f32>,  // shape vertex in local space
}

struct InstanceInput {
    @location(1) world_pos: vec2<f32>,  // agent position
    @location(2) heading: f32,          // rotation angle (radians)
    @location(3) color: vec4<f32>,      // RGBA
}

struct VertexOutput {
    @builtin(position) clip_pos: vec4<f32>,
    @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(vert: VertexInput, inst: InstanceInput) -> VertexOutput {
    // Rotate local vertex by heading
    let c = cos(inst.heading);
    let s = sin(inst.heading);
    let rotated = vec2<f32>(
        vert.local_pos.x * c - vert.local_pos.y * s,
        vert.local_pos.x * s + vert.local_pos.y * c,
    );

    let world = vec4<f32>(rotated + inst.world_pos, 0.0, 1.0);

    var out: VertexOutput;
    out.clip_pos = camera.view_proj * world;
    out.color = inst.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
```

### Instance Buffer Layout for Rendering

```rust
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AgentInstance {
    pub position: [f32; 2],
    pub heading: f32,
    pub color: [f32; 4],
}

impl AgentInstance {
    pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<AgentInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute { // position
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute { // heading
                    offset: 8,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute { // color
                    offset: 12,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}
```

### CFL Stability Check (CPU-side, f64)

```rust
// velos-core/src/lib.rs
pub fn cfl_check(dt: f64, max_speed: f64, min_cell_size: f64) -> bool {
    let cfl = max_speed * dt / min_cell_size;
    cfl < 1.0
}

// Usage before each sim step:
assert!(cfl_check(0.1, 33.3, 50.0), "CFL violation: dt too large for cell size");
```

### GPU Test: Round-Trip Verification

```rust
// velos-gpu/tests/gpu_round_trip.rs
#[cfg(feature = "gpu-tests")]
#[test]
fn test_round_trip_1k_agents_matches_cpu() {
    // Skip if no GPU adapter available
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()));
    let adapter = match adapter {
        Some(a) => a,
        None => { eprintln!("No GPU adapter, skipping"); return; }
    };

    let (device, queue) = pollster::block_on(
        adapter.request_device(&Default::default(), None)
    ).unwrap();

    // 1. Create 1K agents with known positions/velocities
    // 2. Upload to GPU buffers
    // 3. Dispatch compute shader (pos += vel * dt)
    // 4. Copy to staging buffer, map, readback
    // 5. Compare with CPU f64 reference within tolerance

    let dt = 0.1_f64;
    let epsilon = 0.01; // f32 tolerance for position

    for i in 0..1000 {
        let cpu_result_x = initial_x[i] + initial_vx[i] * dt;
        let gpu_result_x = readback_positions[i].x as f64;
        assert!(
            (cpu_result_x - gpu_result_x).abs() < epsilon,
            "Agent {i}: CPU={cpu_result_x}, GPU={gpu_result_x}"
        );
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| winit EventLoop::run closure | ApplicationHandler trait | winit 0.30 (2024) | Must implement trait; window creation in can_create_surfaces/resumed |
| wgpu::Instance::new() | wgpu::Instance::default() or ::new(InstanceDescriptor) | wgpu 0.19+ | Simpler API; backends auto-detected |
| Surface::configure with SurfaceConfiguration | Same but get_capabilities for format | wgpu 22+ | Must query capabilities, not hardcode format |
| entry_point: "main" (required) | entry_point: Some("main") or None for default | wgpu 28 | Optional if shader has single entry point |

**Deprecated/outdated:**
- `winit::event_loop::EventLoop::run(closure)` pattern -- replaced by `run_app(&mut handler)`
- `wgpu::RequestAdapterOptions::compatible_surface` -- still works but surface can be created after adapter
- Explicit `device.poll()` in render loop -- wgpu handles polling internally for presented frames; only needed for explicit readback

## Open Questions

1. **Exact wgpu 28 API for Surface creation timing**
   - What we know: Surface needs a window handle; window created in ApplicationHandler::can_create_surfaces
   - What's unclear: Whether wgpu 28 changed any surface init APIs vs 22-27
   - Recommendation: Follow learn-wgpu tutorial pattern; test on target macOS hardware early

2. **Nightly #[bench] harness JSON output**
   - What we know: Nightly bench outputs to stdout in a text format, not JSON
   - What's unclear: Whether built-in harness can write JSON or if we need a custom harness
   - Recommendation: Write a thin wrapper that runs bench, parses output, writes JSON to benchmarks/baseline.json

3. **AgentInstance buffer sharing between compute and render**
   - What we know: Compute writes positions to storage buffer; render needs positions as instance buffer
   - What's unclear: Whether same buffer can have STORAGE | VERTEX usage flags simultaneously on Metal
   - Recommendation: Test early. If not supported, use copy_buffer_to_buffer from compute output to render instance buffer (small overhead for 1K agents)

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test + nightly #[bench] |
| Config file | None needed -- Cargo.toml [features] for gpu-tests gate |
| Quick run command | `cargo test --workspace` |
| Full suite command | `cargo test --workspace --features gpu-tests && cargo bench --workspace` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GPU-01 | Compute shader dispatches and writes position data | integration | `cargo test -p velos-gpu --features gpu-tests -- test_compute_dispatch` | Wave 0 |
| GPU-02 | f64 CPU vs f32 GPU results within tolerance | integration | `cargo test -p velos-gpu --features gpu-tests -- test_f32_f64_tolerance` | Wave 0 |
| GPU-03 | hecs -> SoA GPU buffer round-trip with 1K agents | integration | `cargo test -p velos-gpu --features gpu-tests -- test_round_trip_1k` | Wave 0 |
| GPU-04 | CFL check returns correct bool for given inputs | unit | `cargo test -p velos-core -- test_cfl_check` | Wave 0 |
| REN-01 | Window opens and surface configures without panic | integration | `cargo test -p velos-gpu --features gpu-tests -- test_window_surface` | Wave 0 |
| REN-02 | Instanced draw renders without validation errors | integration | `cargo test -p velos-gpu --features gpu-tests -- test_instanced_render` | Wave 0 |
| REN-03 | Camera zoom/pan produces correct projection matrix | unit | `cargo test -p velos-gpu -- test_camera_projection` | Wave 0 |
| REN-04 | Single draw call per shape type (verified by structure) | manual-only | Code review: one draw_indexed per shape type | N/A |
| PERF-01 | Frame time < 16ms for 1K agents | bench | `cargo bench -p velos-gpu --features gpu-tests -- frame_time` | Wave 0 |
| PERF-02 | Agents/sec metric computed correctly | bench | `cargo bench -p velos-gpu --features gpu-tests -- throughput` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --workspace && cargo clippy --all-targets -- -D warnings`
- **Per wave merge:** `cargo test --workspace --features gpu-tests && cargo bench --workspace`
- **Phase gate:** Full suite green before verification

### Wave 0 Gaps
- [ ] `crates/velos-gpu/tests/gpu_round_trip.rs` -- covers GPU-01, GPU-02, GPU-03
- [ ] `crates/velos-core/src/lib.rs` (inline tests) -- covers GPU-04
- [ ] `crates/velos-gpu/tests/render_tests.rs` -- covers REN-01, REN-02
- [ ] `crates/velos-gpu/src/camera.rs` (inline tests) -- covers REN-03
- [ ] `crates/velos-gpu/benches/dispatch.rs` -- covers PERF-01, PERF-02
- [ ] `rust-toolchain.toml` -- nightly pin for #[bench]
- [ ] Feature flag `gpu-tests` in velos-gpu Cargo.toml

## Sources

### Primary (HIGH confidence)
- [wgpu 28.0.0 docs](https://docs.rs/wgpu/28.0.0/wgpu/) -- API types, buffer creation, pipeline descriptors
- [hecs 0.11.0 docs](https://docs.rs/hecs/0.11.0/hecs/) -- World, query, spawn APIs
- [winit 0.30.13 docs](https://docs.rs/winit/0.30.13/winit/) -- ApplicationHandler trait, WindowEvent
- [Learn Wgpu tutorial](https://sotrh.github.io/learn-wgpu/) -- Surface setup, uniforms, instancing patterns

### Secondary (MEDIUM confidence)
- [wgpu compute readback pattern](https://tillcode.com/rust-wgpu-compute-minimal-example-buffer-readback-and-performance-tips/) -- Storage + staging buffer pattern, dispatch, map_async
- [winit ApplicationHandler docs](https://rust-windowing.github.io/winit/winit/application/trait.ApplicationHandler.html) -- can_create_surfaces, window_event required methods
- [wgpu + winit 0.30 example](https://github.com/erer1243/wgpu-0.20-winit-0.30-web-example) -- Integration pattern reference
- [WebGPU orthographic projection](https://webgpufundamentals.org/webgpu/lessons/webgpu-orthographic-projection.html) -- Camera math concepts

### Tertiary (LOW confidence)
- Nightly #[bench] JSON output capability -- unverified, may need custom harness wrapper
- STORAGE | VERTEX buffer flag combination on Metal -- needs empirical validation

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- wgpu/hecs/winit are well-documented, versions verified on docs.rs
- Architecture: HIGH -- compute + render pattern is standard wgpu; SoA buffer layout is textbook
- Pitfalls: HIGH -- sourced from official docs, community discussions, and common wgpu issues
- Validation: MEDIUM -- GPU test gating pattern is custom but follows standard Rust feature-flag conventions
- Benchmarks: MEDIUM -- nightly #[bench] works but JSON baseline requires custom wrapper

**Research date:** 2026-03-06
**Valid until:** 2026-04-06 (stable ecosystem, 30-day validity)
