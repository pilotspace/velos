---
marp: true
theme: default
paginate: true
backgroundColor: #fff
color: #333
style: |
  section {
    font-family: 'Segoe UI', Arial, sans-serif;
    font-size: 26px;
  }
  h1 { color: #1e3a5f; }
  h2 { color: #2d6a9f; }
  strong { color: #c53030; }
  code { font-size: 0.8em; }
  pre { font-size: 0.75em; }
  table { font-size: 0.8em; }
  section.lead h1 { color: #fff; }
  section.lead { background: linear-gradient(135deg, #1e3a5f 0%, #2d6a9f 100%); color: #fff; }
  section.invert { background: #1e3a5f; color: #fff; }
---

<!-- _class: lead -->

# VELOS v2

## GPU-Accelerated Traffic Microsimulation
### Technical Architecture & Implementation Plan

**POC: Ho Chi Minh City, 280K Agents, 12 Months**

---

# Why Not Just Use SUMO?

| Dimension | SUMO | VELOS |
|-----------|------|-------|
| Threading | Single-threaded C++ | Multi-GPU Rust + wgpu |
| Max real-time agents | ~80K | **280K (POC), 500K+ (target)** |
| Car-following dispatch | Sequential per-lane | **GPU wave-front parallel** |
| Motorbike model | None (lane-based only) | **Sublane continuous lateral** |
| Pathfinding | A* (0.5ms/query) | **CCH (0.02ms/query)** |
| Prediction | None built-in | **BPR + ETS + historical ensemble** |
| Memory model | Heap-allocated, GC-less but cache-unfriendly | **ECS SoA, GPU buffer-mapped** |

SUMO's architecture is fundamentally single-threaded. Adding parallelism would require a rewrite — which is what VELOS is.

---

# Architecture Overview

```
                 +-----------+    +-----------+    +----------+
                 | velos-core|    | velos-gpu |    | velos-net|
                 | ECS World |<-->| Multi-GPU |<-->| RoadGraph|
                 | Scheduler |    | Buffers   |    | CCH      |
                 +-----+-----+    +-----+-----+    +----+-----+
                       |                |                |
        +--------------+---+    +-------+------+   +-----+------+
        | velos-vehicle    |    | velos-ped    |   | velos-signal|
        | IDM + Motorbike  |    | Social Force |   | Fixed/Act   |
        +---------+--------+    +------+-------+   +------+------+
                  |                    |                   |
        +---------+--------+    +------+-------+   +------+------+
        | velos-meso       |    | velos-predict|   | velos-demand|
        | Queue + Buffer   |    | Ensemble     |   | OD + ToD    |
        +------------------+    +--------------+   +-------------+

        +------------------------------------------------------+
        | velos-api: gRPC + WebSocket (Redis) + REST           |
        +------------------------------------------------------+
        | velos-viz: deck.gl + MapLibre + CesiumJS (optional)  |
        +------------------------------------------------------+
```

12 crates. Each < 700 LOC target. Clear ownership boundaries.

---

# Core Innovation 1: Wave-Front GPU Dispatch

**Problem:** v1's EVEN/ODD semi-synchronous dispatch has no convergence proof in dense traffic. Collision correction can cascade.

**Solution:** Per-lane Gauss-Seidel wave-front.

```
For each lane L (parallel across all lanes — 50K workgroups):
    Sort agents by position (leader first)
    For each agent A in lane L (sequential within lane):
        leader = previous agent (ALREADY UPDATED this step)
        A.accel = IDM(A, leader)     // zero stale data
        A.speed += A.accel * dt
        A.pos   += A.speed * dt
```

| Property | EVEN/ODD (v1) | Wave-Front (v2) |
|----------|:------------:|:--------------:|
| Stale reads | 1-step | **Zero** |
| Collision risk | Correction needed | **Impossible** |
| Convergence proof | None | **Trivial (1D Gauss-Seidel)** |
| GPU workgroups | 250K threads | **50K workgroups** |

---

# Core Innovation 2: Motorbike Sublane Model

HCMC: 80% motorbikes. Western lane-based models are wrong.

```rust
pub struct SublanePosition {
    pub longitudinal: FixedQ16_16,  // along edge
    pub lateral: FixedQ8_8,         // continuous across edge width
}
```

**Filtering logic (WGSL shader):**
```wgsl
if (leader_speed < my_speed * 0.7
    && lateral_gap > MIN_FILTER_GAP) {
    // Motorbike filters past slower vehicle
    lateral_offset += filter_direction * FILTER_RATE * dt;
}
```

**Swarm behavior:** At red signals, motorbikes fill all available space (no lane boundaries). Modeled as 2D packing with social force repulsion.

---

# Core Innovation 3: Multi-GPU Spatial Decomposition

**Problem:** Single RTX 4090 caps at ~200K agents (VRAM + compute).

**Solution:** METIS graph partitioning across 2-4 GPUs on single node.

```
HCMC Network (25K edges) → METIS 4-way partition
  GPU 0: District 1          GPU 1: District 3
  GPU 2: District 5+10       GPU 3: Binh Thanh
```

**Boundary protocol:**
1. Agent reaches partition boundary edge
2. GPU writes to `outbox_buffer` (64 bytes/agent)
3. CPU reads outbox, routes to destination GPU's `inbox_buffer`
4. Next step: agent spawns on new partition

**Cost:** ~500-1000 crossings/step x 64 bytes = 64KB via PCIe (negligible).

---

# Core Innovation 4: CCH Dynamic Routing

**Problem:** Standard Contraction Hierarchies require 30s rebuild when edge weights change. Prediction updates every 60s.

**Solution:** Customizable Contraction Hierarchies (CCH)

| Operation | Standard CH | CCH |
|-----------|:----------:|:---:|
| Initial build | 30s | 30s |
| Weight update | **30s** | **3ms** |
| Query time | 0.01ms | 0.02ms |

CCH separates **topology** (computed once) from **weights** (customized in 3ms). Prediction updates edge costs every 60 sim-seconds. CCH re-customizes in 3ms. Agents use fresh costs immediately.

**Reroute budget:** 500 agents/step x 0.02ms = 10ms sequential. With rayon (16 cores): **0.7ms**.

---

# Prediction: In-Process Rust Ensemble

**Problem:** v1 used Arrow IPC bridge to Python. Cross-process latency (6-60s stale), 300 MB/s throughput, ops burden.

**Solution:** Rust-native ensemble. No Python. No IPC.

```
PredictionEnsemble (every 60 sim-seconds, async tokio::spawn)
  Model A: BPR Physics      (weight 0.40)
           V = V_ff / (1 + a*(flow/cap)^b)
  Model B: ETS Correction   (weight 0.35)
           correction = gamma * error + (1-gamma) * prev_correction
  Model C: Historical Match  (weight 0.25)
           V = weighted_avg(same hour, same day_of_week)

  Publish via ArcSwap (atomic, lock-free, zero-copy)
```

**Cost:** 25K edges x 3 models x arithmetic = **~0.1ms**. Negligible.

ML models (LSTM, Transformer) deferred to v3 via gRPC service — cleaner than shared memory.

---

# Numerical Stability

**CFL condition for traffic:** `CFL = v_max * dt / dx_min < 1`

At dt=0.1s, v_max=33.3 m/s (120 km/h): edges must be > 3.33m.
HCMC has short edges (driveways, alleys). Solution: **adaptive sub-stepping.**

```wgsl
fn idm_update(agent_idx: u32, dt: f32) {
    let cfl = speed * dt / edge_length;
    if (cfl < 1.0) {
        single_step(agent_idx, dt);
    } else {
        let n_sub = u32(ceil(cfl));
        let sub_dt = dt / f32(n_sub);
        for (var i = 0u; i < n_sub; i++) {
            single_step(agent_idx, sub_dt);
        }
    }
}
```

**IDM safety:** `safe_pow4(x) = x*x*x*x` (avoids `pow()` undefined behavior). Zero-speed kickstart at 0.1 m/s prevents deadlock.

---

# ECS + GPU Buffer Layout

```
Buffer 0: Position[]        12 bytes x N   (edge_id, lane, offset, lateral)
Buffer 1: Kinematics[]       8 bytes x N   (speed, acceleration)
Buffer 2: LeaderIndex[]      4 bytes x N   (leader in same lane)
Buffer 3: IDMParams[]       20 bytes x N   (v0, s0, T, a, b)
Buffer 4: LaneChangeState[]  8 bytes x N   (desire, safety check result)
───────────────────────────────────────────
Total per agent:             52 bytes
280K agents:                 14.6 MB VRAM   (RTX 4090 has 24 GB)
```

**Double-buffered staging pattern:**
- Front buffer: GPU reads during dispatch
- Back buffer: CPU writes new agents
- Swap at frame boundary after fence wait
- Prevents race condition (v1 bug C4)

---

# Frame Execution Pipeline

```
Step  1: [CPU]       Partition boundary transfer        0.1ms
Step  2: [CPU/rayon] Per-lane leader sort               1.5ms
Step  3: [GPU x2]    Upload staging buffers             0.3ms
Step  4: [GPU x2]    Lane-change desire (parallel)      1.0ms
Step  5: [GPU x2]    Wave-front car-following            2.0ms
Step  6: [GPU x2]    Pedestrian social force             1.5ms
Step  7: [CPU/rayon] CCH reroutes (~500/step)           0.5ms  (parallel w/ GPU)
Step  8: [CPU/rayon] Route advance + edge transitions   0.3ms
Step  9: [GPU->CPU]  Download results                   0.3ms
Step 10: [CPU]       Prediction update (if due)         0.2ms  (async)
Step 11: [CPU]       Output + WebSocket broadcast       0.5ms
──────────────────────────────────────────────────────────────
Total:                                                  ~8.2ms
Budget:                                               100ms (10 Hz)
Headroom:                                             ~92ms (11x margin)
```

11x margin absorbs PCIe contention, memory jitter, OS scheduling, checkpoint I/O.

---

# Meso-Micro Transition

**Problem:** Vehicles teleporting from queue-based (meso) to agent-based (micro) creates phantom congestion.

**Solution:** 100m graduated buffer zone with velocity matching.

```
Meso Zone → Buffer (100m) → Micro Zone

Buffer entry: T = 2.0 * T_normal, s0 = 1.5 * s0_normal  (relaxed)
Buffer exit:  T = T_normal,       s0 = s0_normal          (normal)

Insertion check:
  if micro_queue_has_space(gap > s0 + vehicle_length):
      spawn at queue back, speed = min(v_meso, v_last_micro)
  else:
      hold in meso queue (don't force-spawn)
```

Linear parameter interpolation eliminates boundary artifacts.

---

# Checkpoint / Restart

**Problem:** 24h simulation crashes at hour 23 = total loss.

**Solution:** ECS snapshot to Parquet every 5 min sim-time.

```
Checkpoint contents:
  positions.parquet      (12B x 280K = 3.4 MB)
  kinematics.parquet     (8B x 280K  = 2.2 MB)
  routes.parquet         (variable   = ~5 MB)
  profiles.parquet       (fixed      = ~2 MB)
  meta.json              (sim_time, step, RNG state, signal states)
  ─────────────────────────────────────────
  Total:                 ~15 MB (Zstd compressed)
  Save time:             ~200ms (async, non-blocking)
  Restore time:          ~500ms (including GPU buffer rebuild)
```

Rolling window: keep latest 10 checkpoints. Auto-save on SIGTERM.

---

# Visualization: deck.gl + Redis WebSocket

**deck.gl layers:**
- ScatterplotLayer: 280K agent dots (type-colored)
- HeatmapLayer: congestion density
- IconLayer: flow direction arrows
- PathLayer: bus routes

**WebSocket scaling (v1 fix W12):**
```
Simulation → Redis pub/sub (channel per 500m tile)
                 ↓
         Stateless WS relay pods (K8s HPA)
                 ↓
         Browser clients (subscribe to visible tiles)

Per-agent: 8 bytes (x_offset, y_offset, speed, heading, type)
Per tile:  ~8 KB (1000 agents)
Per client: ~32 KB/frame (4 visible tiles) at 10 Hz = 320 KB/s
```

Scales to 100+ concurrent viewers by adding relay pods.

---

# Calibration Framework

**Target:** GEH < 5 for 85% of measured links

```
GEH = sqrt(2 * (sim - obs)^2 / (sim + obs))

GEH < 5: acceptable | GEH < 4: good | GEH < 3: excellent
```

**Automated tuning loop:**
1. Run simulation with current parameters
2. Compare simulated vs. observed counts at 40-50 locations
3. Compute GEH per link
4. If pass rate < 85%: Bayesian optimization (argmin crate) tunes:
   - OD scaling factors per zone pair
   - IDM parameters per road class
   - Signal timing offsets
5. Repeat until convergence

Validate against 20% held-out count locations.

---

# Technical Spikes: Week 1-2

Three experiments **before any committed development:**

| Spike | Question | GO if | NO-GO fallback |
|-------|----------|-------|---------------|
| **S1** | Wave-front GPU throughput sufficient? | > 40% of naive parallel | EVEN/ODD + 3-pass correction |
| **S2** | wgpu multi-GPU works for compute? | Both GPUs addressable, < 0.1ms transfer | Single-GPU, 200K agents |
| **S3** | CCH library usable? | Correct paths, < 10ms customization | Custom CCH (+3 weeks) or ALT |

**Gate G0 (Week 2):** If S1 AND S2 fail → CPU-only architecture. Caps at 100K agents. Reassess project viability.

These spikes cost 10 engineer-days. They save months of wrong-direction development.

---

# Go/No-Go Decision Gates

| Gate | Week | Question | Fail Action |
|------|------|----------|-------------|
| **G0** | 2 | GPU architecture viable? | Architecture pivot meeting |
| **G1** | 8 | Vehicles moving on HCMC map? | Extend Phase 1 (max 2 weeks) |
| **G2** | 12 | Motorbike sublane stable? | Discrete sublanes (0.5m quantized) |
| **G3** | 20 | 280K agents, p99 < 15ms? | Reduce to 200K, optimize later |
| **G4** | 32 | GEH < 5 for 70% links? | Consultant + reduce scope area |
| **G5** | 44 | 3 scenarios stable? | Drop 1 scenario, fix-only sprint |

Every gate has **explicit pass criteria, fail criteria, and a concrete fallback plan**.
No gate requires "hoping it works."

---

# 12-Month Phase Plan

```
Phase 1 (M1-3): Foundation
  E1: ECS + IDM shader + wave-front + motorbike sublane
  E2: OSM import + CCH + MOBIL lane-change
  E3: PMTiles + deck.gl + gRPC + WebSocket
  → Gate G1 (W8): first vehicles on map
  → Gate G2 (W12): motorbike behavior validated

Phase 2 (M4-6): Scale + Intelligence
  E1: Multi-GPU + pedestrian + checkpoint
  E2: Signals + buses + prediction ensemble
  E3: Redis WS + heatmaps + KPI dashboard
  E4: HCMC data collection + OD matrix + ToD profiles
  → Gate G3 (W20): 280K agents sustained

Phase 3 (M7-9): Calibration + Validation
  E1: Meso-micro buffer + gridlock detection
  E2: Scenario DSL + emissions
  E3: CesiumJS (stretch) + playback controls
  E4: Bayesian optimization + validation report
  → Gate G4 (W32): calibration feasible

Phase 4 (M10-12): Hardening + Demo
  All: Performance tuning, load testing, documentation
  E4: 3 demo scenarios + presentation
  → Gate G5 (W44): demo-ready
```

---

# Technology Stack

| Layer | Choice | Why |
|-------|--------|-----|
| Language | **Rust 1.78+** | Memory safety, zero-cost abstractions, GPU interop |
| GPU | **wgpu + WGSL** | Cross-platform WebGPU (Vulkan/Metal/DX12) |
| ECS | **hecs** | Lightweight, SoA layout maps to GPU buffers |
| CPU Parallel | **rayon** | Work-stealing, compile-time safety |
| Async | **tokio** | gRPC, WebSocket, sensor I/O |
| Pathfinding | **CCH (custom)** | Dynamic weights, 0.02ms/query |
| API | **tonic + axum** | gRPC + REST/WebSocket |
| Viz | **deck.gl + MapLibre** | GPU-accelerated, open-source |
| Tiles | **PMTiles** | Single static file, zero ops |
| Monitoring | **Prometheus + Grafana** | Industry standard |
| Calibration | **argmin** | Bayesian optimization in Rust |

Zero vendor lock-in. Zero license fees. All production-grade.

---

# The Ask

## Authorize the engineering team to begin the 12-month POC

**Immediately needed:**
- 4 engineers allocated (E1-E3 from Day 1, E4 from Month 3)
- Hardware: 1x workstation with 2x RTX 4090 (24GB each)
- HCMC DOT introduction for traffic count + signal timing data

**First visible result: Week 8** — vehicles moving on HCMC digital map.

**Final deliverable: Week 48** — calibrated 280K-agent simulation with 3 demo scenarios.

---

<!-- _class: lead -->

# Questions?

### VELOS v2 — Technical Architecture
**github.com/[org]/velos** (after authorization)

---
