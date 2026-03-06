# Architecture Research

**Domain:** GPU-accelerated traffic microsimulation (desktop, macOS/Metal)
**Researched:** 2026-03-06
**Confidence:** MEDIUM — wgpu compute patterns are well-documented; Tauri+wgpu integration is immature with known issues; wave-front dispatch in WGSL has no direct precedent and requires careful design.

## Standard Architecture

### System Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Tauri v2 App Shell                            │
│  ┌────────────────────────┐  ┌─────────────────────────────────────┐ │
│  │   wgpu Render Surface  │  │  WebView (React+Vite Dashboard)     │ │
│  │   Metal backend        │  │  Controls, metrics, charts          │ │
│  │   Agent rendering      │  │  Communicates via Tauri IPC         │ │
│  └───────────┬────────────┘  └──────────────┬──────────────────────┘ │
│              │ GPU commands                  │ IPC events            │
├──────────────┴──────────────────────────────┬┴───────────────────────┤
│                     Simulation Core (Rust)                           │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               │
│  │  velos-core   │  │  velos-gpu   │  │  velos-net   │               │
│  │  ECS (hecs)   │  │  wgpu Device │  │  Road Graph  │               │
│  │  Scheduler    │◄─┤  Compute     │◄─┤  CCH Router  │               │
│  │  Time Control │  │  Pipelines   │  │  R-tree      │               │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘               │
│         │                 │                  │                       │
│  ┌──────▼───────┐  ┌──────▼───────┐  ┌──────▼───────┐               │
│  │ velos-vehicle │  │velos-pedest. │  │ velos-signal │               │
│  │ IDM+MOBIL     │  │ Social Force │  │ Fixed-Time   │               │
│  │ Motorbike Sub │  │ Adaptive WG  │  │ Phase Ctrl   │               │
│  └──────────────┘  └──────────────┘  └──────────────┘               │
│                                                                      │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               │
│  │  velos-meso   │  │velos-predict │  │ velos-demand │               │
│  │  Queue Model  │  │ BPR+ETS     │  │ OD Matrices  │               │
│  │  Buffer Zone  │  │ Ensemble    │  │ ToD Profiles │               │
│  └──────────────┘  └──────────────┘  └──────────────┘               │
├──────────────────────────────────────────────────────────────────────┤
│                     GPU Compute Layer (wgpu/Metal)                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐               │
│  │ IDM Pipeline  │  │ LaneChange   │  │ Social Force │               │
│  │ (wave-front)  │  │ Pipeline     │  │ Pipeline     │               │
│  └──────────────┘  └──────────────┘  └──────────────┘               │
│  ┌──────────────────────────────────────────────────────┐           │
│  │ Storage Buffers: Position[] | Kinematics[] | Params[]│           │
│  └──────────────────────────────────────────────────────┘           │
└──────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Typical Implementation |
|-----------|----------------|------------------------|
| **velos-core** | ECS world (hecs), frame scheduler, time control, checkpoint | Owns the `World`, orchestrates per-frame pipeline steps, manages simulation clock |
| **velos-gpu** | wgpu device/queue, compute pipeline registry, buffer pool, staging transfers | Creates `Device`, `Queue`, `ComputePipeline` objects; manages GPU memory lifecycle |
| **velos-net** | Road network graph, OSM import, CCH pathfinding, rstar spatial index | Builds directed graph from OSM, constructs CCH for dynamic routing |
| **velos-vehicle** | IDM car-following, MOBIL lane-change, motorbike sublane filtering | Defines WGSL shaders + CPU fallback for agent movement models |
| **velos-pedestrian** | Social force model, adaptive workgroup spatial hashing | Prefix-sum compaction + density-aware dispatch |
| **velos-signal** | Traffic signal controllers (fixed-time, actuated) | Phase/cycle logic, signal state buffer updates |
| **velos-meso** | Mesoscopic queue model, graduated buffer zone transitions | BPR link travel times, meso-micro velocity matching |
| **velos-predict** | Travel time prediction ensemble (BPR+ETS+historical) | ArcSwap overlay for lock-free reads during simulation |
| **velos-demand** | OD matrices, time-of-day demand profiles, agent spawning | Reads demand config, spawns agents at origins over time |
| **velos-viz** | React+TypeScript dashboard in Tauri webview | Charts, controls, simulation state display via IPC |

## Recommended Project Structure

```
velos/
├── crates/
│   ├── velos-core/          # ECS world, scheduler, checkpoint
│   │   ├── src/
│   │   │   ├── lib.rs       # Public API
│   │   │   ├── world.rs     # hecs World wrapper + component registration
│   │   │   ├── scheduler.rs # Frame pipeline orchestration
│   │   │   ├── time.rs      # Simulation clock, dt, speed control
│   │   │   └── checkpoint.rs # ECS snapshot to/from bincode
│   │   └── Cargo.toml
│   ├── velos-gpu/           # wgpu device, pipelines, buffers
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── device.rs    # wgpu Instance/Adapter/Device/Queue setup
│   │   │   ├── pipeline.rs  # ComputePipeline creation + caching
│   │   │   ├── buffers.rs   # BufferPool, staging, double-buffering
│   │   │   └── sync.rs      # Frame fences, submission tracking
│   │   ├── shaders/
│   │   │   ├── idm.wgsl     # IDM car-following compute shader
│   │   │   ├── lane_change.wgsl  # MOBIL + motorbike filtering
│   │   │   ├── social_force.wgsl # Pedestrian social force
│   │   │   ├── prefix_sum.wgsl   # Utility: parallel prefix sum
│   │   │   └── fixed_point.wgsl  # Fixed-point arithmetic helpers
│   │   └── Cargo.toml
│   ├── velos-net/           # Road graph, routing, spatial index
│   ├── velos-vehicle/       # Agent movement models
│   ├── velos-pedestrian/    # Pedestrian-specific model
│   ├── velos-signal/        # Traffic signal control
│   ├── velos-meso/          # Mesoscopic queue model
│   ├── velos-predict/       # Travel time prediction
│   ├── velos-demand/        # Demand generation
│   └── velos-app/           # Tauri app binary crate
│       ├── src/
│       │   ├── main.rs      # Tauri entry point
│       │   ├── commands.rs  # Tauri IPC command handlers
│       │   ├── render.rs    # wgpu render loop for visualization
│       │   └── state.rs     # App state shared across IPC + render
│       └── Cargo.toml
├── dashboard/               # React+TypeScript frontend
│   ├── src/
│   │   ├── App.tsx
│   │   ├── components/      # Dashboard panels
│   │   └── hooks/           # Tauri IPC hooks
│   ├── package.json
│   └── vite.config.ts
├── data/
│   └── hcmc/                # HCMC-specific configs, OSM extracts
├── proto/                   # Protobuf definitions (future gRPC)
└── Cargo.toml               # Workspace root
```

### Structure Rationale

- **crates/:** Each crate has a single responsibility aligned with a simulation subsystem. This enables independent testing, compilation caching, and clear dependency direction.
- **velos-gpu/shaders/:** WGSL shaders co-located with the GPU crate. Validated by naga at build time.
- **velos-app/:** Separate binary crate for the Tauri application, depending on all library crates. Keeps simulation logic testable without the app shell.
- **dashboard/:** Separate pnpm workspace for the React frontend. Tauri builds it via Vite.

## Architectural Patterns

### Pattern 1: ECS-to-GPU Buffer Mapping (SoA Projection)

**What:** Project hecs component arrays into contiguous GPU storage buffers for compute dispatch. Each ECS component type maps to one GPU buffer. The CPU-side hecs world is the source of truth; GPU buffers are populated per-frame from ECS queries.

**When to use:** Every simulation frame where GPU compute needs agent data.

**Trade-offs:**
- Pro: SoA layout matches GPU memory coalescing requirements (consecutive threads read consecutive memory)
- Pro: hecs stores components in archetype tables that are already semi-contiguous
- Con: Must maintain index mapping between hecs `Entity` IDs and GPU buffer indices
- Con: Agent spawn/despawn requires compaction or free-list management in GPU buffers

**Implementation approach:**

```rust
/// Maps hecs entities to GPU buffer slots
pub struct GpuIndexMap {
    entity_to_gpu: HashMap<Entity, u32>,
    gpu_to_entity: Vec<Option<Entity>>,
    free_list: Vec<u32>,
    count: u32,
}

/// Projects ECS components into a GPU storage buffer
pub fn upload_positions(
    world: &World,
    index_map: &GpuIndexMap,
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
) {
    // Collect positions in GPU-index order
    let mut data = vec![GpuPosition::zeroed(); index_map.count as usize];
    for (entity, pos) in world.query::<&Position>().iter() {
        if let Some(gpu_idx) = index_map.entity_to_gpu.get(&entity) {
            data[*gpu_idx as usize] = pos.to_gpu();
        }
    }
    queue.write_buffer(buffer, 0, bytemuck::cast_slice(&data));
}
```

**Key constraint:** Structs written to GPU buffers must be `#[repr(C)]` and implement `bytemuck::Pod + bytemuck::Zeroable` for safe casting to `&[u8]`.

### Pattern 2: Compute Pipeline Registry

**What:** Pre-create and cache all `ComputePipeline` objects at startup. Pipelines are expensive to create (shader compilation), but using the same `PipelineLayout` across related pipelines avoids rebinding resources when switching between them.

**When to use:** Application startup. Never create pipelines during the simulation loop.

**Trade-offs:**
- Pro: Zero pipeline creation cost during simulation
- Pro: Shared bind group layouts reduce descriptor set switches
- Con: All shaders must be known at startup (no dynamic shader generation)

**Implementation approach:**

```rust
pub struct PipelineRegistry {
    idm_pipeline: wgpu::ComputePipeline,
    lane_change_pipeline: wgpu::ComputePipeline,
    social_force_pipeline: wgpu::ComputePipeline,
    prefix_sum_pipeline: wgpu::ComputePipeline,
    // Shared layout for agent data access
    agent_bind_group_layout: wgpu::BindGroupLayout,
}

impl PipelineRegistry {
    pub fn new(device: &wgpu::Device) -> Self {
        let agent_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("agent_data"),
            entries: &[
                // binding 0: Position[] storage buffer
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // binding 1: Kinematics[], binding 2: Params[], etc.
            ],
        });
        // Create pipelines sharing this layout...
        todo!()
    }
}
```

### Pattern 3: Wave-Front Dispatch via workgroupBarrier

**What:** Process agents within a lane sequentially (front-to-back) while processing different lanes in parallel. This is the Gauss-Seidel pattern: each agent reads its leader's already-updated state, eliminating stale data and collision risk.

**When to use:** IDM car-following compute dispatch. Each workgroup = one lane.

**Trade-offs:**
- Pro: Zero stale data, zero collision correction needed, provably convergent
- Pro: 50K+ lanes provide sufficient workgroup parallelism for GPU saturation
- Con: Sequential within-lane processing underutilizes threads within a workgroup (avg 5.6 agents/lane for 280K agents, only ~6 of 256 threads active per workgroup)
- Con: workgroupBarrier must be called uniformly by ALL threads in the workgroup (WGSL spec requirement), so inactive threads must still participate in the barrier loop

**Critical WGSL constraint:** `workgroupBarrier()` must be executed by all invocations in the workgroup uniformly. You cannot have conditional barriers where only some threads call it. The sequential loop pattern must have all threads enter the loop and call the barrier, with only the "active" thread doing real work in each iteration.

**Implementation approach:**

```wgsl
@compute @workgroup_size(256)
fn idm_wave_front(
    @builtin(local_invocation_id) lid: vec3<u32>,
    @builtin(workgroup_id) wgid: vec3<u32>,
) {
    let lane_id = wgid.x;
    let lane_start = lane_offsets[lane_id];
    let lane_count = lane_counts[lane_id];

    // Sequential wave-front: process agents front-to-back
    // ALL threads participate in the loop for barrier uniformity
    for (var i: u32 = 0u; i < lane_count; i = i + 1u) {
        // Only thread 0 does work (sequential within lane)
        if (lid.x == 0u) {
            let agent_idx = lane_start + i;
            let leader_idx = select(agent_idx - 1u, 0xFFFFFFFFu, i == 0u);
            idm_update(agent_idx, leader_idx);
        }
        workgroupBarrier();  // ALL threads hit this uniformly
    }
}
```

**Note on occupancy:** With only thread 0 active per workgroup, GPU compute units are heavily underutilized within each workgroup. The parallelism comes from dispatching ~50K workgroups (one per lane). For the ~1K agent POC scale, this is acceptable. At 280K scale, consider an alternative: sort agents into per-lane arrays on CPU, then dispatch with workgroup_size(1) and rely on wave-level parallelism across workgroups instead. This avoids wasting 255 threads per workgroup.

### Pattern 4: Double-Buffered GPU Transfers

**What:** Maintain two copies of agent data buffers. The GPU reads from the "front" buffer during compute dispatch while the CPU writes new/updated data to the "back" buffer. Buffers swap each frame after the GPU finishes.

**When to use:** Every frame boundary where CPU needs to inject new agents or read back results.

**Trade-offs:**
- Pro: Eliminates CPU-GPU synchronization stalls during the frame
- Pro: Staging buffer pattern is well-supported by wgpu
- Con: 2x memory usage for agent buffers (52 bytes/agent * 2 * N, negligible at 1K-280K scale)
- Con: Results are one frame delayed (acceptable at 10 Hz sim rate)

**Key wgpu constraint:** While a buffer is mapped for CPU access, no GPU commands may access it. The staging buffer pattern (separate MAP_READ buffer + queue.write_buffer for uploads) avoids this entirely.

### Pattern 5: Fixed-Point Arithmetic in WGSL

**What:** Use `i32` integer arithmetic to represent position (Q16.16) and speed (Q12.20) in GPU shaders. This guarantees bitwise-identical results across GPU vendors (AMD, NVIDIA, Intel, Apple Silicon).

**When to use:** All position and speed calculations in compute shaders.

**Trade-offs:**
- Pro: Cross-GPU determinism guaranteed (integer ops are deterministic everywhere)
- Pro: Enables checkpoint validation — resume on different hardware, same results
- Con: ~20% slower than float32 due to manual overflow handling
- Con: WGSL lacks native i64, so 64-bit intermediate products require manual splitting into high/low 32-bit halves
- Con: More complex shader code, harder to debug

**Critical WGSL limitation:** WGSL has no native 64-bit integer type. Multiplying two Q16.16 values produces a 64-bit intermediate. The existing architecture doc shows a split approach (ah*bh, ah*bl, al*bh, al*bl) but this must be tested carefully for overflow with realistic traffic values. The `SHADER_INT64` feature exists in wgpu but is not universally supported and may not be available on Metal.

**Fallback strategy:** If fixed-point performance is unacceptable or the i64 emulation is too error-prone, use float32 with the `@invariant` WGSL attribute on outputs. Accept "statistical equivalence" (positions within 1mm after 24h sim time) rather than bitwise determinism.

## Data Flow

### Per-Frame Simulation Pipeline

```
[Frame Start]
    │
    ▼
[CPU] ECS Query: collect agent positions, kinematics, params
    │
    ▼
[CPU→GPU] queue.write_buffer(): upload Position[], Kinematics[], Params[]
    │
    ▼
[GPU Compute Pass 1] Lane-change desire (parallel across all agents)
    │  reads: Position[], Kinematics[] (previous step)
    │  writes: LaneChangeDecision[]
    │
    ▼
[GPU Compute Pass 2] Wave-front IDM + lane-change execution (per-lane)
    │  reads: Position[], Kinematics[], LaneChangeDecision[], IDMParams[]
    │  writes: Position[], Kinematics[] (updated in-place)
    │
    ▼
[GPU Compute Pass 3] Pedestrian social force (adaptive workgroups)
    │  Phase A: count_per_cell (atomic histogram)
    │  Phase B: prefix_sum (exclusive scan)
    │  Phase C: scatter (compact into cell arrays)
    │  Phase D: social_force (force computation per occupied cell)
    │
    ▼
[GPU→CPU] Staging buffer map_async + readback
    │
    ▼
[CPU] Apply results back to ECS World
    │  - Update Position, Kinematics components
    │  - Edge transition: advance route index if position > edge_length
    │  - Signal state: update phase timers
    │
    ▼
[CPU/rayon] Pathfinding: reroute ~50 agents per frame (staggered)
    │
    ▼
[CPU] Demand: spawn/despawn agents per OD matrix + ToD profile
    │
    ▼
[CPU→Tauri IPC] Push frame data to dashboard (positions for rendering, metrics)
    │
    ▼
[Frame End]
```

### Data Ownership and Flow Direction

```
                     Owns             Reads              Writes
velos-demand    ──→  OD matrices  ──→  velos-core     (spawn agents)
velos-net       ──→  Road graph   ──→  velos-vehicle  (edge topology)
                                  ──→  velos-gpu      (lane buffers)
velos-core      ──→  ECS World    ──→  velos-gpu      (agent buffers)
                                  ◄──  velos-gpu      (updated state)
velos-gpu       ──→  Device/Queue ──→  WGSL shaders   (dispatch)
velos-predict   ──→  Overlay      ──→  velos-net      (edge weights)
velos-signal    ──→  Phase state  ──→  velos-vehicle  (stop/go)
velos-app       ──→  Tauri window ──→  dashboard      (IPC events)
                                  ◄──  dashboard      (user commands)
```

### Key Data Flows

1. **Agent state cycle (hot path):** ECS → GPU buffers → compute dispatch → staging readback → ECS. This runs every frame (10 Hz). Must complete in <100ms. Target: <15ms p99.

2. **Pathfinding (warm path):** velos-predict updates edge weights via ArcSwap. velos-net reads weights during CCH customization. Agents query routes through velos-core scheduler. ~50 reroutes per frame, staggered to avoid CPU spikes.

3. **Demand injection (cold path):** velos-demand reads OD matrices and ToD curves. Spawns agents into ECS at configured rates. Runs once per simulation second (every 10 frames at 10 Hz).

4. **Dashboard updates:** velos-app serializes frame summary (agent count, avg speed, frame time) and pushes via Tauri IPC `emit()`. Dashboard subscribes to events. Rendering of agent positions happens natively via wgpu, not through the webview.

## Tauri + wgpu Integration Architecture

### The Challenge

Tauri v2 provides a native window with an embedded webview. Rendering wgpu content in the same window as the webview is not straightforward:

- On Linux (GTK), the webview and wgpu fight for the same X11 surface, causing flickering. This issue was closed as "not planned" by the Tauri team.
- On macOS, the webview (WKWebView) is an NSView child of the window's contentView. wgpu also needs access to the contentView to create a Metal surface. Coordination requires main-thread management.

### Recommended Approach: Dual-Surface Architecture

**Confidence: LOW** — this area is actively evolving and no production-quality pattern exists.

Two viable approaches for macOS:

**Option A: wgpu renders to the window surface, webview overlays on top (transparent)**

```
┌─────────────────────────────────────┐
│  NSWindow                           │
│  ┌───────────────────────────────┐  │
│  │  WKWebView (transparent bg)  │  │  ← z-order: front
│  │  Dashboard controls + metrics │  │
│  └───────────────────────────────┘  │
│  ┌───────────────────────────────┐  │
│  │  CAMetalLayer (wgpu surface)  │  │  ← z-order: back
│  │  Agent rendering              │  │
│  └───────────────────────────────┘  │
└─────────────────────────────────────┘
```

- Pro: Single window, clean UX
- Con: Mouse event routing is complex (clicks on transparent webview area must pass through to wgpu)
- Con: Requires Tauri plugin or platform-specific code to manage NSView z-ordering

**Option B: Two separate windows (simulation + dashboard)**

```
┌──────────────────┐  ┌──────────────────┐
│  Simulation Win   │  │  Dashboard Win    │
│  wgpu full render │  │  WebView only     │
│  Agent positions  │  │  Controls+metrics │
│  Road network     │  │  Tauri IPC        │
└──────────────────┘  └──────────────────┘
```

- Pro: No surface conflicts, simpler implementation
- Pro: Each window manages its own rendering independently
- Con: Less polished UX (two windows)
- Con: Window synchronization (resize, move) adds complexity

**Recommendation for POC:** Start with Option B (two windows). It avoids the surface conflict issues entirely. The simulation window uses wgpu natively (via winit or raw_window_handle from Tauri). The dashboard window is a standard Tauri webview. IPC connects them. Revisit Option A once the Tauri ecosystem matures this pattern.

**Alternative to evaluate during spike:** Skip Tauri entirely for the simulation window. Use winit for the wgpu window and Tauri only for the dashboard. The winit window is a peer process or runs in the same Rust binary but on a separate thread. This gives full control over the GPU render loop.

## Scaling Considerations

| Scale | Architecture Adjustments |
|-------|--------------------------|
| ~1K agents (POC) | Single GPU, single compute pass, CPU-side sorting is fine. Frame time <<15ms. No optimization needed. |
| ~50K agents | GPU sorting becomes worthwhile (bitonic sort on GPU). Leader index computation moves to GPU. |
| ~280K agents | Per-lane wave-front may underutilize GPU — consider workgroup_size(1) with 50K dispatches. Double-buffering matters. Staging belt for uploads. |
| ~1M+ agents | Multi-GPU partition required. METIS graph bisection. Boundary agent protocol. PCIe transfer budget becomes relevant. |

### Scaling Priorities

1. **First bottleneck: CPU→GPU transfer.** At 280K agents, uploading ~14MB per frame via `queue.write_buffer()` is fine (Metal can handle GB/s). But if the upload blocks the main thread, use `StagingBelt` for async writes.

2. **Second bottleneck: Per-lane sorting.** Sorting 280K agents into ~50K lanes every frame on CPU takes ~1.5ms with rayon. At 1M+ agents, move sorting to a GPU compute pass (radix sort by lane_id + position).

3. **Third bottleneck: Wave-front occupancy.** With avg 5.6 agents/lane and workgroup_size(256), 98% of threads are idle per workgroup. The 50K workgroups provide inter-workgroup parallelism, but this is inefficient. At scale, flatten to workgroup_size(1) or workgroup_size(32) and use subgroup operations.

## Anti-Patterns

### Anti-Pattern 1: AoS (Array-of-Structs) GPU Buffers

**What people do:** Upload a single buffer of `AgentData { position, speed, params, route, ... }` structs.
**Why it's wrong:** GPU threads in a wavefront access the same field of consecutive agents. AoS means these reads are strided (e.g., reading `speed` at offsets 0, 64, 128, ...) which defeats memory coalescing. Bandwidth drops 4-8x.
**Do this instead:** SoA — separate buffers for `Position[]`, `Kinematics[]`, `Params[]`. Consecutive threads read consecutive memory addresses.

### Anti-Pattern 2: Creating Pipelines Per Frame

**What people do:** Call `device.create_compute_pipeline()` inside the simulation loop.
**Why it's wrong:** Pipeline creation involves shader compilation. On Metal, this can take 10-50ms. Even cached, the creation path has allocation overhead.
**Do this instead:** Create all pipelines at startup in a `PipelineRegistry`. Reuse the same pipeline objects every frame.

### Anti-Pattern 3: Synchronous Buffer Readback

**What people do:** Call `buffer.slice(..).map_async()` then immediately `device.poll(Maintain::Wait)` to block until the GPU finishes.
**Why it's wrong:** Stalls the CPU while the GPU completes. Wastes the overlap opportunity where CPU can do pathfinding, demand, or IPC while GPU is computing.
**Do this instead:** Submit GPU work, then do CPU work (pathfinding, demand spawning), then poll for GPU completion. Overlap CPU and GPU work within each frame.

### Anti-Pattern 4: Float32 for Determinism-Critical Values

**What people do:** Use `f32` for position and speed, assume results will be identical across hardware.
**Why it's wrong:** IEEE 754 only guarantees results within rounding tolerance. Different GPU vendors may use fused multiply-add (FMA) or different rounding modes. Results diverge after thousands of steps.
**Do this instead:** Use fixed-point i32 arithmetic for position (Q16.16) and speed (Q12.20). Or accept non-determinism and validate via statistical equivalence (positions within tolerance).

### Anti-Pattern 5: Conditional workgroupBarrier

**What people do:** Put `workgroupBarrier()` inside an `if` block so only active threads call it.
**Why it's wrong:** WGSL spec requires barriers to be called uniformly by all invocations in the workgroup. Conditional barriers are undefined behavior and will produce incorrect results or GPU hangs on some drivers.
**Do this instead:** All threads enter the loop and hit the barrier. Only the designated thread does actual computation within the loop body.

## Integration Points

### Internal Boundaries

| Boundary | Communication | Notes |
|----------|---------------|-------|
| velos-core ↔ velos-gpu | Direct function calls. Core passes `&World` to GPU upload functions, GPU returns updated component data. | Same process, same thread (main sim thread). No serialization. |
| velos-core ↔ velos-net | Direct function calls. Core asks net for routes, net reads edge weights from predict overlay. | CCH customization runs on rayon thread pool, results returned via channel. |
| velos-app ↔ velos-core | Tauri IPC commands map to `SimController` trait methods (start, stop, set_speed, reset). | Commands are async (tokio). Simulation runs on a dedicated thread, controlled via `mpsc::channel`. |
| velos-app ↔ dashboard | Tauri `emit()` for sim→dashboard events. Tauri `invoke()` for dashboard→sim commands. | Serialization: serde_json for small payloads. Consider bincode for position arrays if JSON is too slow. |
| velos-gpu ↔ WGSL shaders | Bind group layouts define the contract. Shader reads/writes storage buffers. | Buffer layout must match between Rust structs (`#[repr(C)]`, bytemuck) and WGSL struct definitions. Any mismatch = silent corruption. |
| velos-predict ↔ velos-net | ArcSwap<PredictionOverlay>. Predict publishes new overlay atomically. Net reads current overlay during route queries. | Lock-free. No mutex. Predict runs on background thread. |

### Dependency Graph (Build Order)

```
Level 0 (no deps):     velos-gpu, velos-net
Level 1:               velos-core (depends on velos-gpu, velos-net)
Level 2:               velos-vehicle, velos-pedestrian, velos-signal (depend on velos-core, velos-gpu)
Level 2:               velos-predict, velos-demand (depend on velos-net)
Level 3:               velos-meso (depends on velos-vehicle, velos-net)
Level 4:               velos-app (depends on everything)
```

### Suggested Build Order for Development

1. **velos-gpu** — Get wgpu device creation, buffer management, and a trivial compute shader running on Metal. This is the foundation spike. If wgpu+Metal has issues, discover them now.
2. **velos-net** — OSM import, road graph construction, basic R-tree. Can be developed in parallel with velos-gpu since they share no dependencies.
3. **velos-core** — ECS world with hecs, basic scheduler that uploads agent data to GPU and reads it back. Integration point between gpu and net.
4. **velos-vehicle** — IDM shader in WGSL with fixed-point arithmetic. The core simulation model. Requires velos-gpu (pipelines) and velos-core (ECS).
5. **velos-signal** — Fixed-time signal logic. Simple state machine. Enables intersection behavior.
6. **velos-pedestrian** — Social force with adaptive workgroups. More complex GPU dispatch pattern. Build after vehicle model is proven.
7. **velos-demand** — Agent spawning from OD matrices. Requires velos-net (graph) and velos-core (ECS).
8. **velos-predict** — Ensemble prediction. Can be deferred — simulation works without dynamic rerouting.
9. **velos-meso** — Mesoscopic model. Only needed when simulating large networks with mixed fidelity.
10. **velos-app** — Tauri shell. Build last because simulation must be testable headless first. The Tauri integration can wrap a working simulation engine.

## Sources

- [wgpu ComputePipeline docs](https://docs.rs/wgpu/latest/wgpu/struct.ComputePipeline.html) — HIGH confidence
- [wgpu Buffer docs](https://docs.rs/wgpu/latest/wgpu/struct.Buffer.html) — HIGH confidence
- [WGSL Specification (W3C)](https://www.w3.org/TR/WGSL/) — HIGH confidence
- [Learn Wgpu tutorial](https://sotrh.github.io/learn-wgpu/beginner/tutorial4-buffer/) — HIGH confidence
- [Rust wgpu Compute: Buffer Readback and Performance Tips](https://tillcode.com/rust-wgpu-compute-minimal-example-buffer-readback-and-performance-tips/) — MEDIUM confidence
- [High Performance GPGPU with Rust and wgpu](https://dev.to/jaysmito101/high-performance-gpgpu-with-rust-and-wgpu-4l9i) — MEDIUM confidence
- [FabianLars/tauri-v2-wgpu](https://github.com/FabianLars/tauri-v2-wgpu) — MEDIUM confidence (proof of concept, not production)
- [Tauri v2 wgpu flickering issue #9220](https://github.com/tauri-apps/tauri/issues/9220) — HIGH confidence (confirmed bug, closed as not planned for Linux; macOS status unclear)
- [Tauri discussion: render wgpu as webview overlay #11944](https://github.com/tauri-apps/tauri/discussions/11944) — MEDIUM confidence (community discussion, no resolution)
- [WebGPU Compute Shader Basics](https://webgpufundamentals.org/webgpu/lessons/webgpu-compute-shaders.html) — HIGH confidence
- [workgroupBarrier uniformity requirement](https://webgpu.rocks/wgsl/functions/synchronization-atomic/) — HIGH confidence
- [wgpu StagingBelt docs](https://wgpu.rs/doc/wgpu/util/struct.StagingBelt.html) — HIGH confidence
- VELOS architecture docs `docs/architect/00-07` — project-internal, authoritative

---
*Architecture research for: GPU-accelerated traffic microsimulation (VELOS)*
*Researched: 2026-03-06*
