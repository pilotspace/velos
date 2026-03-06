# VELOS — Rust Parallel Processing, Frameworks & Reusable Open-Source Components
## How Rust Handles Parallel Agent Steps + What We Can Reuse

**Companion to:** `rebuild-sumo-architecture-plan.md`
**Date:** March 5, 2026

---

## 1. How Rust Handles Parallel Agent Steps

Rust offers three levels of parallelism for processing 500K agent steps, each suited to different parts of VELOS's pipeline:

### Level 1: Data Parallelism with Rayon (CPU — edge-level and batch operations)

Rayon is the de-facto standard for CPU data parallelism in Rust. It converts sequential iterators into parallel ones via work-stealing thread pool:

```rust
use rayon::prelude::*;

// SEQUENTIAL (SUMO-style):
for edge in network.edges.iter() {
    let agents = get_agents_on_edge(edge.id);
    sort_and_assign_leaders(&mut agents);
}

// PARALLEL (VELOS with rayon):
network.edges.par_iter().for_each(|edge| {
    let mut agents = get_agents_on_edge(edge.id);
    sort_and_assign_leaders(&mut agents);
});
// Automatic work-stealing across all CPU cores
// Speedup: near-linear up to core count (16x on 16-core)
```

**Where VELOS uses rayon:**

| Operation | Sequential Cost | Rayon Parallel Cost | Speedup |
|-----------|----------------|--------------------:|--------:|
| Per-lane leader index sort | ~40ms | ~2.5ms (16-core) | 16x |
| CH pathfinding (1000 queries) | ~10ms | ~0.7ms | 14x |
| Route advancement (edge transitions) | ~5ms | ~0.4ms | 12x |
| Gridlock cycle detection | ~8ms | ~0.6ms | 13x |
| Coordinate transform (local→world) | ~15ms | ~1ms | 15x |

**Key Rust guarantee:** rayon's `par_iter()` enforces `Send + Sync` at compile time — data races are impossible. This is Rust's killer advantage over C++/CUDA where parallel bugs are runtime crashes.

```rust
// Reroute batch: 1000 agents in parallel using Contraction Hierarchies
let rerouted: Vec<(EntityId, Vec<EdgeId>)> = reroute_batch
    .par_iter()
    .filter_map(|agent_id| {
        let agent = world.get::<AgentIntelligence>(*agent_id)?;
        let origin = world.get::<Position>(*agent_id)?.current_edge;
        let dest = agent.profile.destination;

        // CH query: ~0.01ms each, run 1000 in parallel = ~0.7ms total
        let new_route = ch_graph.calc_path(origin, dest)?;
        Some((*agent_id, new_route))
    })
    .collect();
```

### Level 2: GPU Compute with wgpu/WGSL (GPU — per-agent arithmetic)

The heavy per-agent computation (car-following, social force, cost evaluation) runs on GPU via wgpu compute shaders:

```
                    CPU (rayon)                     GPU (wgpu compute)
              ┌──────────────────────┐        ┌──────────────────────────┐
              │ • Leader index sort   │        │ • IDM/Krauss car-follow  │
              │ • Route advancement   │        │ • Social force (ped)     │
              │ • Signal controller   │        │ • Position integration   │
              │ • CH pathfinding      │   →    │ • Perception state       │
              │ • Gridlock detection  │  GPU   │ • Cost evaluation screen │
              │ • Meso queue model    │  buf   │ • Collision correction   │
              │ • Event processing    │ upload │ • Spatial hash build     │
              └──────────────────────┘        └──────────────────────────┘
              Parallel via rayon                Parallel via GPU threads
              ~16 cores, work-stealing         ~16,384 threads (RTX 4090)
```

**VELOS's semi-synchronous EVEN/ODD dispatch:**

```rust
// velos-gpu/src/dispatch.rs

pub fn dispatch_car_following(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &wgpu::ComputePipeline,
    agent_count: u32,
) {
    let workgroup_size = 256;
    let workgroups = (agent_count / 2 + workgroup_size - 1) / workgroup_size;

    // DISPATCH 1: Update EVEN agents (read ODD = fresh data)
    set_parity_uniform(queue, 0); // current_parity = EVEN
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }

    // DISPATCH 2: Update ODD agents (read EVEN = just updated)
    set_parity_uniform(queue, 1); // current_parity = ODD
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(workgroups, 1, 1);
    }

    // DISPATCH 3: Collision correction pass
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&collision_pipeline);
        pass.set_bind_group(0, &collision_bind_group, &[]);
        pass.dispatch_workgroups(
            (agent_count + workgroup_size - 1) / workgroup_size, 1, 1
        );
    }

    queue.submit(Some(encoder.finish()));
}
```

### Level 3: Async Concurrency with Tokio (I/O — API, prediction, sensors)

Non-blocking async for I/O-bound operations:

```rust
// Prediction thread: runs independently, publishes via ArcSwap
tokio::spawn(async move {
    let mut interval = tokio::time::interval(prediction_interval);
    loop {
        interval.tick().await;

        // Snapshot current state (lock-free read from ArcSwap)
        let state = sim_state.load();

        // Run ensemble prediction (CPU-bound but on separate thread)
        let overlay = tokio::task::spawn_blocking(move || {
            ensemble.predict_global(&state, &horizons)
        }).await.unwrap();

        // Atomic publish — simulation sees it on next step
        prediction_state.publish_overlay(overlay);
    }
});

// gRPC streaming, WebSocket, MQTT — all async
tokio::spawn(grpc_server.serve(addr));
tokio::spawn(websocket_server.run(ws_addr));
tokio::spawn(mqtt_subscriber.connect_and_listen(mqtt_config));
```

### How the Three Levels Compose in One Frame

```
Frame N (target: 12ms)
├── [CPU/rayon]  Leader sort per-lane             2.5ms  ← rayon par_iter over edges
├── [GPU/wgpu]   Upload dirty buffers             0.5ms  ← staging buffer drain
├── [GPU/wgpu]   Perception + prediction overlay  0.3ms  ← 500K agents, 256 threads/group
├── [GPU/wgpu]   IDM EVEN dispatch                1.5ms  ← 250K agents
├── [GPU/wgpu]   IDM ODD dispatch                 1.5ms  ← 250K agents
├── [GPU/wgpu]   Collision correction + cost eval 0.8ms  ← 500K agents
├── [CPU/rayon]  CH reroute batch (1000 agents)   0.7ms  ← rayon par_iter, runs during GPU
├── [CPU/rayon]  Route advance + edge transitions 0.5ms  ← rayon par_iter over agents
├── [CPU]        Gridlock check (if interval)     0.1ms  ← every 60s only
├── [GPU→CPU]    Download results                 0.5ms
├── [CPU]        Output recording + streaming     1.0ms
└── [async/tokio] Prediction thread (background)  —      ← non-blocking, separate thread
                                                  ≈9.9ms total
```

---

## 2. Best Frameworks for VELOS — Decision Matrix

### ECS Libraries Comparison

| Criterion | **hecs** | **bevy_ecs** | **specs** | **legion** |
|-----------|---------|------------|---------|----------|
| **Parallel systems** | Manual (rayon) | Built-in scheduler | Built-in Dispatcher | Built-in scheduler |
| **GPU buffer mapping** | Trivial (SoA) | Possible but complex | Possible | Possible |
| **Overhead** | Minimal (~50ns/query) | Medium (~100ns) | Medium | Low (~60ns) |
| **Framework coupling** | None | Brings Bevy runtime | Minimal | Minimal |
| **Maintenance** | Active, stable | Very active | Maintenance mode | Low activity |
| **`#[repr(C)]` support** | Natural | Needs work | Natural | Natural |

**Recommendation: hecs** — VELOS needs raw component arrays that map directly to GPU buffers. hecs gives us SoA layout with zero framework overhead. We build our own scheduler with rayon, which gives us full control over the EVEN/ODD dispatch and staging buffer timing.

### GPU Compute Library

| Criterion | **wgpu** | **rust-gpu** | **CUDA (via cudarc)** |
|-----------|---------|------------|---------------------|
| **Cross-platform** | Vulkan+Metal+DX12+WebGPU | SPIR-V (Vulkan only) | NVIDIA only |
| **Shader language** | WGSL (safe, simple) | Rust → SPIR-V | CUDA C++ |
| **Compute performance** | 85-95% of native Vulkan | 90-95% of native | 100% (native) |
| **WebGPU browser** | Yes (WASM target) | No | No |
| **Maturity** | Production-ready | Beta (improving fast) | Gold standard |

**Recommendation: wgpu** with WGSL shaders. Cross-platform is critical for city deployments (not all have NVIDIA GPUs). rust-gpu is a compelling future option — write shaders in Rust, share types between CPU and GPU code.

**Future hybrid strategy:**
```
Phase 1 (v0.1): wgpu + WGSL shaders (works everywhere)
Phase 2 (v0.2): Add rust-gpu backend (share Rust types CPU↔GPU)
Phase 3 (v0.3): Optional CUDA backend via cudarc for max NVIDIA perf
```

---

## 3. Open-Source Components VELOS Can Reuse

### Tier A: Direct Dependencies (Integrate Immediately)

| Crate/Project | What It Gives Us | Lines Saved | Risk |
|---------------|-----------------|-------------|------|
| **[fast_paths](https://github.com/easbar/fast_paths)** | Contraction Hierarchies pathfinding. 0.01ms per query vs 0.5ms for A*. Road-network optimized. | ~3,000 | Low — stable, MIT licensed |
| **[wgpu](https://github.com/gfx-rs/wgpu)** | WebGPU compute + render. Cross-platform GPU abstraction. | ~10,000 | Low — Mozilla-backed, production-ready |
| **[hecs](https://github.com/Ralith/hecs)** | Lightweight ECS. SoA layout, perfect for GPU buffer mapping. | ~2,000 | Low — stable, widely used |
| **[rayon](https://github.com/rayon-rs/rayon)** | Data-parallel CPU iteration. Work-stealing thread pool. | ~1,500 | Negligible — gold standard |
| **[arc-swap](https://crates.io/crates/arc-swap)** | Lock-free atomic pointer swap. For prediction overlay publish. | ~500 | Negligible — proven pattern |
| **[tonic](https://github.com/hyperium/tonic)** | gRPC server/client. Async, production-grade. | ~3,000 | Low — maintained by Hyperium |
| **[arrow-rs](https://github.com/apache/arrow-rs)** | Apache Arrow for zero-copy data exchange with Python ML. | ~2,000 | Low — Apache foundation |
| **[rstar](https://crates.io/crates/rstar)** | R-tree spatial index. For nearest-edge queries. | ~800 | Low — stable |
| **[glam](https://crates.io/crates/glam)** | SIMD-optimized Vec2/Vec3/Mat4 math. | ~500 | Negligible |
| **[linfa](https://github.com/rust-ml/linfa)** | Rust ML toolkit. Built-in ETS, clustering for ensemble prediction. | ~1,500 | Medium — still maturing |
| **[argmin](https://github.com/argmin-rs/argmin)** | Optimization framework. For Bayesian calibration parameter tuning. | ~1,000 | Low — well-tested |

**Estimated savings: ~26,000 lines of code** that we don't write from scratch.

### Tier B: Reference Architectures (Study & Adapt Patterns)

| Project | What We Learn | How to Use |
|---------|--------------|------------|
| **[MOSS](https://github.com/tsinghua-fib-lab/moss-benchmark)** | GPU traffic simulation architecture. CUDA-based, 100x faster than CityFlow for 2M vehicles. Parallel car-following + lane-change on GPU. | Study their GPU data layout, buffer partitioning, and kernel dispatch strategy. Adapt for wgpu/WGSL instead of CUDA. Their sensing index (spatial hash for leader detection) is directly applicable. |
| **[LPSim](https://arxiv.org/html/2406.08496v1)** | Multi-GPU graph partitioning for distributed traffic sim. 2.82M trips in 6.28 min on single V100 GPU. | Study their network partitioning strategy for future multi-GPU VELOS scaling. Their edge-level parallelism approach validates our per-edge rayon pattern. |
| **[CityFlowER](https://github.com/cityflow-project/CityFlowER)** | Embedded ML models inside traffic sim. Efficient RL training loop. | Study their ML integration architecture. Their Python binding pattern via pybind11 maps to our PyO3 approach. |
| **[krABMaga](https://github.com/krABMaga/krABMaga)** | Rust agent-based modeling framework. Parallel agent scheduling, distributed execution via MPI, Bevy-based visualization. | Study their parallel agent scheduler. Their model exploration (parameter sweeping, genetic optimization) could accelerate our calibration framework. MIT licensed. |
| **[rust-agent-based-models](https://github.com/facorread/rust-agent-based-models)** | Patterns for reliable ABMs in Rust. ECS-style agent composition. | Study their agent lifecycle patterns and ECS integration approach. Small but well-designed. |

### Tier C: Reusable Components (Partial Extraction)

| Component | Source | What to Extract | Effort |
|-----------|--------|----------------|--------|
| **Social Force Model** | MOSS (Python/CUDA) | Force computation kernel, calibrated Helbing parameters (A=2000N, B=0.08m). Port to WGSL. | 2 days |
| **BPR Delay Function** | SUMO/CityFlow | Standard BPR implementation with validated α=0.15, β=4.0. Trivial to implement in Rust. | 0.5 days |
| **NEMA Signal Controller** | SUMO (C++) | Dual-ring NEMA logic. Complex but well-documented. Port to Rust. | 1 week |
| **OSM Network Parser** | osmpbfreader + SUMO netconvert | osmpbfreader handles PBF parsing. Study SUMO's netconvert for turn restriction inference. | Already using crate |
| **GEH Statistic** | SUMO/transport engineering | `GEH = sqrt(2*(sim-obs)²/(sim+obs))`. Trivial formula. | 0.5 days |
| **Spatial Hash Grid (GPU)** | MOSS / Bevy boids | GPU radix sort + prefix sum for cell assignment. Well-known pattern. | 3 days |

---

## 4. The "Build vs. Reuse" Map for VELOS

```
VELOS SYSTEM                    BUILD FROM SCRATCH    REUSE/ADAPT
──────────────────────────────  ────────────────────  ──────────────────────
ECS World + Components          Build (custom layout)  hecs (foundation)
GPU Compute Pipeline            Build (custom dispatch) wgpu (abstraction)
IDM/Krauss Car-Following        Build (WGSL shader)   MOSS patterns (study)
Social Force Pedestrian         Build (WGSL shader)   MOSS/Helbing params
EVEN/ODD Semi-Sync Dispatch     Build (novel design)   —
Staging Buffer Ring             Build                  —
Per-Lane Leader Index           Build                  —
Collision Correction Pass       Build                  —
Contraction Hierarchies         REUSE                  fast_paths crate ✓
A* Pathfinding                  REUSE                  fast_paths (CH mode)
Multi-Factor Cost Function      Build (domain logic)   —
Agent Profiles/Brain            Build (domain logic)   krABMaga patterns
Prediction Ensemble (BPR+ETS)   Build                  linfa (ETS), BPR formula
ArcSwap Overlay                 REUSE                  arc-swap crate ✓
ML Bridge (Arrow IPC)           REUSE                  arrow-rs + PyO3 ✓
gRPC API Server                 REUSE                  tonic crate ✓
WebSocket (spatial tiling)      Build (protocol)       tokio-tungstenite
CesiumJS Bridge                 Build (integration)    —
OSM Import                      REUSE                  osmpbfreader crate ✓
SUMO .net.xml Import            Build (parser)         quick-xml crate
Signal Controllers              Build (NEMA logic)     SUMO patterns (study)
Calibration (GEH + Bayesian)    Build                  argmin crate ✓
Scenario Management             Build                  —
Emissions (HBEFA)               Build (lookup table)   SUMO HBEFA data
Gridlock Detection              Build                  —
Meso Queue Model                Build                  —
Meso↔Micro Transition           Build (C7 buffer zone) —
Coordinate Transform            Build                  glam (math) ✓
Spatial Index                   REUSE                  rstar crate ✓
Visualization (native)          Build                  wgpu (render pipeline)
Docker/K8s                      REUSE                  Standard tooling

BUILD: ~65%    REUSE: ~35%
```

---

## 5. Recommended Cargo.toml Dependencies

```toml
[workspace.dependencies]
# Core
hecs = "0.10"              # ECS framework
glam = { version = "0.29", features = ["serde"] }  # SIMD math
rayon = "1.10"             # CPU data parallelism
tokio = { version = "1", features = ["full"] }     # Async runtime

# GPU
wgpu = "24"                # WebGPU compute + render

# Networking & API
tonic = "0.12"             # gRPC
prost = "0.13"             # Protobuf codegen
tokio-tungstenite = "0.24" # WebSocket
rumqttc = "0.24"           # MQTT sensor ingestion

# Data & ML Bridge
arrow = "53"               # Apache Arrow (zero-copy Python bridge)
parquet = "53"             # Parquet output
pyo3 = "0.22"              # Python bindings
linfa = "0.7"              # Built-in ML (ETS, clustering)

# Pathfinding & Spatial
fast_paths = "1.0"         # Contraction Hierarchies
rstar = "0.12"             # R-tree spatial index
osmpbfreader = "0.17"      # OSM PBF import

# Concurrency
arc-swap = "1.7"           # Lock-free prediction overlay swap
crossbeam = "0.8"          # Lock-free ring buffer (staging)

# Optimization & Calibration
argmin = "0.10"            # Bayesian optimization for calibration

# Serialization
serde = { version = "1", features = ["derive"] }
quick-xml = "0.37"         # SUMO XML import
flatbuffers = "24"         # Binary WebSocket protocol
```

---

## 6. Key Insight from MOSS — What We Should Steal

MOSS (Tsinghua University, 2024) achieved **100x speedup over CityFlow** by moving the entire agent update loop to CUDA. Their architecture validates several VELOS design decisions:

```
MOSS Architecture (CUDA)              VELOS Architecture (wgpu)
───────────────────────               ─────────────────────────
• Parallel car-following on GPU       ✓ Same — EVEN/ODD dispatch
• Spatial hash for leader sensing     ✓ Same — GPU radix sort
• Double-buffered agent state         ✓ Same — but with staging buffer
• SIMD-friendly data layout           ✓ Same — ECS SoA via hecs
• 2M+ vehicles real-time              ✓ Target: 500K real-time

MOSS Limitations We Fix:
• CUDA-only (NVIDIA locked)           wgpu: Vulkan+Metal+DX12+WebGPU
• No agent intelligence               Full cost function + rerouting
• No prediction pipeline              Hierarchical ensemble + ML
• No V2I communication                Full SPaT, priority, cooperative
• No calibration framework            GEH + Bayesian optimization
• No scenario management              Batch compare with MOEs
• C++/Python codebase                 Pure Rust (memory-safe)
```

**What to port from MOSS to VELOS:**

1. **GPU spatial hash kernel** — their cell assignment + radix sort approach is proven for O(N×K) neighbor queries. Adapt from CUDA to WGSL.
2. **Buffer layout** — their per-agent struct packing is optimized for GPU cache lines (128 bytes). Validate our 60-byte struct alignment.
3. **Sensing index** — their approach to finding leaders on GPU without CPU sorting is worth studying for future optimization (currently we sort on CPU with rayon).

---

## 7. Summary: VELOS's Parallelism Stack

```
┌─────────────────────────────────────────────────────────────┐
│                    VELOS PARALLELISM STACK                    │
│                                                              │
│  LAYER 4: Async I/O          tokio                          │
│  ─────────────────────────────────────────────              │
│  gRPC, WebSocket, MQTT, prediction thread                   │
│  Non-blocking, event-driven                                  │
│                                                              │
│  LAYER 3: GPU Compute        wgpu + WGSL                    │
│  ─────────────────────────────────────────────              │
│  IDM/Krauss, social force, perception, cost eval            │
│  500K agents × 256 threads/workgroup                        │
│  Semi-synchronous EVEN/ODD dispatch                         │
│                                                              │
│  LAYER 2: CPU Data Parallel  rayon                          │
│  ─────────────────────────────────────────────              │
│  Leader sort, CH pathfinding, route advance, gridlock       │
│  Work-stealing across 16 cores                              │
│                                                              │
│  LAYER 1: Lock-Free Shared   arc-swap + crossbeam           │
│  ─────────────────────────────────────────────              │
│  Prediction overlay (ArcSwap), staging buffer (ring)        │
│  Zero-contention between sim thread and prediction thread   │
│                                                              │
│  FOUNDATION: Memory Safety   Rust ownership + borrow checker│
│  ─────────────────────────────────────────────              │
│  Compile-time guarantee: no data races, no dangling pointers│
│  This is why we chose Rust over C++ for a 500K-agent sim    │
└─────────────────────────────────────────────────────────────┘
```

---

*Document version: 1.0 | VELOS Rust Parallel Frameworks & OSS Analysis | March 2026*
