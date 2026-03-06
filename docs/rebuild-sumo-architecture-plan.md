# Project VELOS — Next-Generation Urban Mobility Simulation Engine
## Comprehensive Architecture & Build Plan

**Codename:** VELOS (Vehicle & Entity Large-scale Open Simulator)
**Stack:** Rust + wgpu (WebGPU) + ECS Architecture
**Team:** 5 Engineers, 9-Month Horizon (revised from 6-month after architecture review)
**Date:** March 5, 2026

---

## Table of Contents

1. [Why Rebuild — SUMO's Architectural Debt](#1-why-rebuild)
2. [Requirements & Acceptance Criteria](#2-requirements)
3. [Core Architecture — ECS + GPU Compute](#3-core-architecture)
4. [Simulation Models — Deep Dive](#4-simulation-models)
5. [Crate Structure & Module Map](#5-crate-structure)
6. [Data Model & Storage](#6-data-model)
7. [API Contracts](#7-api-contracts)
8. [GPU Compute Pipeline](#8-gpu-compute)
9. [Visualization Architecture](#9-visualization)
10. [Data Ingestion Pipeline](#10-data-ingestion)
11. [9-Month Roadmap](#11-roadmap)
12. [Risk Register & Trade-offs](#12-risks)
13. [Benchmark Targets](#13-benchmarks)
14. [Open Questions](#14-open-questions)
15. **[Agent Intelligence & Prediction →](./velos-agent-intelligence-and-prediction.md)** *(companion document)*

---

## 1. Why Rebuild — SUMO's Architectural Debt {#1-why-rebuild}

| SUMO Bottleneck | Root Cause | VELOS Solution |
|-----------------|-----------|----------------|
| Single-threaded core | Sequential vehicle update loop with causal dependency on leader vehicle | ECS archetype-based batch processing + GPU compute shaders for car-following |
| ~80K vehicle real-time ceiling | CPU-bound per-vehicle computation, no SIMD | GPU parallel: 1M+ vehicles on modern GPU (RTX 4090: 16,384 CUDA cores) |
| Weak pedestrian model | 1D stripe model bolted onto vehicle sim | Native 2D social-force model as first-class ECS component, GPU-accelerated |
| No 3D visualization | Text-based 2D GUI, separate rendering stack | wgpu-native rendering pipeline, CesiumJS WebSocket bridge |
| TraCI socket overhead | TCP round-trip per query per vehicle | Shared-memory ECS world + zero-copy gRPC streaming |
| Monolithic C++ codebase | 20+ years of accumulated coupling | Rust workspace with isolated crates, trait-based polymorphism |
| No ML integration | No prediction pipeline, batch-only | Arrow IPC for zero-copy data exchange with Python ML ecosystem |
| No agent intelligence | Pre-computed routes, no real-time rerouting based on conditions | Cities:Skylines-inspired multi-factor cost function + global knowledge routing |
| No prediction pipeline | Cannot forecast future congestion from current state | Built-in ensemble (BPR + ETS + historical) + pluggable ML per step |
| No V2I communication | Signals are obstacles, not collaborators | Full SPaT, signal priority, cooperative intersection management |
| Static route assignment | Agents follow fixed routes regardless of congestion | Per-agent reroute evaluation with prediction-informed cost function |

> **Companion Document:** The Agent Intelligence, Prediction Pipeline, and V2I systems are
> detailed in **[velos-agent-intelligence-and-prediction.md](./velos-agent-intelligence-and-prediction.md)**

---

## 2. Requirements & Acceptance Criteria {#2-requirements}

### Functional Requirements

| ID | Requirement | Target |
|----|------------|--------|
| FR-1 | Multi-modal agent simulation (vehicle, pedestrian, cyclist, transit) | ≥4 agent types with distinct behavioral models |
| FR-2 | Car-following models (Krauss, IDM, W99-equivalent) | Configurable per vehicle type |
| FR-3 | Lane-change model with sublane resolution | Continuous lateral movement (not discrete lanes) |
| FR-4 | Social-force pedestrian model (Helbing 1995) | 2D continuous space, density-dependent speed |
| FR-5 | Traffic signal control (fixed, actuated, adaptive) | Phase-based controller with detector triggers |
| FR-6 | Network import (OSM, SUMO .net.xml) | Full road network with turn restrictions, speed limits |
| FR-7 | Demand import (OD matrix, route files, real-time counts) | SUMO .rou.xml compatibility + live sensor feeds |
| FR-8 | Time-series output (FCD, edge data, detector data) | Arrow/Parquet format + SUMO-compatible XML |
| FR-9 | What-if scenario API (block road, change signal, add demand) | Runtime mutation without restart |
| FR-10 | 3D + 2D visualization | wgpu native + CesiumJS WebSocket bridge |
| FR-11 | Agent intelligence with multi-factor pathfinding cost (time, comfort, safety, fuel, signal, prediction) | Cities:Skylines-style per-agent decision with configurable agent profiles |
| FR-12 | Global network knowledge routing — agents reroute based on real-time + predicted congestion | Staggered reroute: each agent re-evaluates every ~50 sim-seconds |
| FR-13 | Hierarchical prediction pipeline (global → group → individual) | Built-in ensemble + pluggable external ML via Arrow IPC |
| FR-14 | Async prediction: ML model runs each step without blocking simulation | Lock-free ArcSwap overlay, dual-loop (fast sim + slow prediction) |
| FR-15 | V2I: SPaT broadcast, signal priority request, cooperative intersection, variable speed, congestion pricing | Full Vehicle-to-Infrastructure communication stack |
| FR-16 | Agent profiles: Commuter, Bus, Truck, Emergency, Tourist, Teen, Senior, Cyclist, Autonomous, Custom | Configurable cost function weights per agent class |
| FR-17 | Traffic sign interaction: speed limits, stop/yield, no-turn, school zones, dynamic signs | Signs affect agent cost function and speed targets |
| FR-18 | Prediction model registry with hot-swap and A/B testing | Register, activate, deactivate models at runtime; compare accuracy |

### Non-Functional Requirements

| ID | Requirement | Target |
|----|------------|--------|
| NFR-1 | Real-time simulation at city scale | ≥500K vehicles at 1x real-time on single GPU node |
| NFR-2 | Faster-than-real-time for planning | ≥50x speed for 100K vehicles (offline mode) |
| NFR-3 | Startup time | Network load + first step < 5 seconds for 50K-edge network |
| NFR-4 | Memory footprint | < 8 GB GPU VRAM for 500K agents |
| NFR-5 | API latency | gRPC streaming < 1ms per frame for 10K subscribed agents |
| NFR-6 | Determinism | Identical inputs → identical outputs (bitwise reproducible) |
| NFR-7 | Hot-reload | Add/remove agents, change signal plans without stopping sim |

---

## 3. Core Architecture — ECS + GPU Compute {#3-core-architecture}

### Why ECS (Entity Component System)?

Traffic simulation maps perfectly to ECS because every agent (vehicle, pedestrian, cyclist) is an **entity** with a set of **components** (position, velocity, route, behavior params), and **systems** process all entities with matching components in batch. This enables:

- **Cache-friendly memory layout** — components stored in contiguous arrays (SoA), not scattered objects (AoS)
- **Trivial GPU upload** — component arrays map directly to GPU buffers
- **Parallel system execution** — systems that touch different components run concurrently
- **Dynamic composition** — add/remove components at runtime (vehicle gains "emergency" component → different behavior)

### High-Level Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        VELOS RUNTIME                                │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                     ECS WORLD (hecs / custom)                 │  │
│  │                                                               │  │
│  │  Entities:  [E0] [E1] [E2] ... [E500000]                     │  │
│  │                                                               │  │
│  │  Component Tables (SoA layout):                               │  │
│  │  ┌──────────┬───────────┬──────────┬─────────┬──────────┐    │  │
│  │  │ Position │ Velocity  │ Route    │ CFModel │ LCState  │    │  │
│  │  │ (f32×2)  │ (f32×2)   │ (edge[]) │ (enum)  │ (f32×4)  │    │  │
│  │  ├──────────┼───────────┼──────────┼─────────┼──────────┤    │  │
│  │  │ 0.0, 1.2 │ 13.9, 0.0│ [e1,e5]  │ IDM     │ 0,0,0,0  │    │  │
│  │  │ 0.5, 1.2 │ 11.1, 0.0│ [e2,e8]  │ Krauss  │ 1,0.3,.. │    │  │
│  │  │ ...      │ ...       │ ...      │ ...     │ ...      │    │  │
│  │  └──────────┴───────────┴──────────┴─────────┴──────────┘    │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                      │
│                    ┌─────────┴─────────┐                            │
│                    ▼                   ▼                            │
│  ┌──────────────────────┐  ┌──────────────────────┐                │
│  │   CPU SYSTEMS         │  │   GPU SYSTEMS         │               │
│  │                       │  │   (wgpu compute)      │               │
│  │  • Route planning     │  │                       │               │
│  │  • Signal control     │  │  • Car-following      │               │
│  │  • Agent spawn/remove │  │  • Position update    │               │
│  │  • Collision detect   │  │  • Pedestrian force   │               │
│  │  • Event handling     │  │  • Lane-change eval   │               │
│  │  • Output recording   │  │  • Sensor aggregation │               │
│  └──────────────────────┘  └──────────────────────┘                │
│                    │                   │                            │
│                    └─────────┬─────────┘                            │
│                              ▼                                      │
│  ┌───────────────────────────────────────────────────────────────┐  │
│  │                    FRAME SCHEDULER                             │  │
│  │                                                               │  │
│  │  tick() pipeline per simulation step (Δt):                    │  │
│  │                                                               │  │
│  │  1. [CPU] Ingest external events (sensor data, API commands)  │  │
│  │  2. [CPU] Signal controller update (phase transitions)        │  │
│  │  3. [CPU] Agent insertion/removal (demand management)         │  │
│  │  4. [GPU] Upload dirty component buffers → GPU                │  │
│  │  5. [GPU] Dispatch car-following compute shader               │  │
│  │  6. [GPU] Dispatch pedestrian social-force shader             │  │
│  │  7. [GPU] Dispatch position-update shader                     │  │
│  │  8. [GPU] Download updated positions → CPU                    │  │
│  │  9. [CPU] Route advancement (edge transitions)                │  │
│  │  10.[CPU] Output recording + streaming                        │  │
│  │                                                               │  │
│  │  Target: steps 4-8 < 5ms for 500K agents                     │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                              │                                      │
│              ┌───────────────┼───────────────┐                      │
│              ▼               ▼               ▼                      │
│  ┌────────────────┐ ┌──────────────┐ ┌──────────────────┐          │
│  │ gRPC Server     │ │ Arrow IPC    │ │ wgpu Renderer    │          │
│  │ (control + sub) │ │ (ML bridge)  │ │ (3D/2D native)   │          │
│  └────────────────┘ └──────────────┘ └──────────────────┘          │
│              │               │               │                      │
└──────────────┼───────────────┼───────────────┼──────────────────────┘
               ▼               ▼               ▼
        External clients   Python/ML       CesiumJS/Browser
        (dashboard, API)   (prediction)    (3D city view)
```

### The Parallelization Breakthrough (Semi-Synchronous EVEN/ODD)

SUMO can't parallelize because vehicle N depends on vehicle N-1 (its leader). VELOS solves this with a **semi-synchronous EVEN/ODD GPU update** pattern that guarantees collision-free simulation while retaining massive parallelism:

```
SUMO's SEQUENTIAL APPROACH:
  for vehicle in sorted_by_position:          ← must be sequential
      v_safe = car_following(vehicle, leader)  ← leader must be updated first
      vehicle.speed = v_safe
      vehicle.position += v_safe * dt

VELOS's SEMI-SYNCHRONOUS GPU APPROACH:

  WHY NOT PURE STALE-READ:
  A naive "read last frame, write this frame" approach causes cumulative gap error.
  In congestion (gap < 5m), both leader and follower read stale positions → both
  accelerate → gap shrinks → after ~10 steps, gap goes NEGATIVE = collision.
  This violates the fundamental safety guarantee that Krauss/IDM must provide.

  SOLUTION: Split vehicles into EVEN and ODD sets (by index)

  STEP 1: Update EVEN vehicles (GPU dispatch, 250K threads)
  ┌─────────────────────────────────────────────────────┐
  │  Each EVEN thread reads its leader's position:        │
  │  - If leader is ODD → read from FRESH buffer (just   │
  │    written by ODD in previous sub-step) ✓             │
  │  - If leader is EVEN → read from PREVIOUS frame       │
  │    (max 1 step stale, same-set only — bounded error)  │
  │                                                       │
  │  Thread[i]: gap = leader_pos - my_pos - leader_len    │
  │             v_safe = idm(gap, my_speed, leader_speed) │
  │             buffer_WRITE[i] = updated state            │
  └─────────────────────────────────────────────────────┘

  STEP 2: Update ODD vehicles (GPU dispatch, 250K threads)
  ┌─────────────────────────────────────────────────────┐
  │  Each ODD thread reads its leader's position:         │
  │  - If leader is EVEN → read from FRESH buffer ✓       │
  │  - If leader is ODD → read from PREVIOUS frame        │
  │                                                       │
  │  Thread[i]: same IDM computation as above              │
  │             buffer_WRITE[i] = updated state            │
  └─────────────────────────────────────────────────────┘

  STEP 3: Collision correction pass (GPU, lightweight)
  ┌─────────────────────────────────────────────────────┐
  │  For each vehicle i:                                  │
  │    if gap[i] < min_gap[i]:                            │
  │      speed[i] = max(0, leader_speed - 0.5)            │
  │      pos[i] = leader_pos - leader_length - min_gap    │
  │  Cost: ~0.3ms (simple per-agent check, no branching)  │
  └─────────────────────────────────────────────────────┘

  STEP 4: Swap buffers
  ┌─────────────────────────────────────────────────────┐
  │  READ buffer ← WRITE buffer (double-buffering)       │
  └─────────────────────────────────────────────────────┘

  WHY THIS WORKS:
  - EVEN/ODD split: each vehicle's leader is fresh ~50% of the time
  - Remaining stale-read error is bounded to 1 sub-step (not cumulative)
  - Collision correction pass catches any residual violations
  - 2 dispatches of 250K instead of 1 of 500K → ~1.5x overhead vs naive
  - Still 100x+ faster than SUMO's sequential approach
  - Collision-free guarantee maintained (critical for urban planning use)
```

---

## 4. Simulation Models — Deep Dive {#4-simulation-models}

### 4.1 Car-Following: IDM (Default) + Krauss (Compat)

**GPU Compute Shader (WGSL):**

```wgsl
// idm_car_following.wgsl — runs on GPU for ALL vehicles simultaneously
// Dispatched in EVEN/ODD semi-synchronous pattern (see §3)

struct Agent {
    pos_x: f32,        // longitudinal position on edge
    pos_y: f32,        // lateral position (sublane)
    speed: f32,        // current speed (m/s)
    accel: f32,        // current acceleration output
    edge_id: u32,      // which road edge
    lane_idx: u32,     // which lane on edge (for per-lane leader)
    leader_idx: u32,   // index of leader agent IN SAME LANE (u32::MAX = no leader)
    parity: u32,       // 0 = EVEN, 1 = ODD (for semi-sync dispatch)
    // IDM parameters
    v0: f32,           // desired speed
    T: f32,            // safe time headway
    a_max: f32,        // max acceleration
    b_comfort: f32,    // comfortable deceleration
    s0: f32,           // minimum gap
    length: f32,       // vehicle length
    sigma: f32,        // driver imperfection [0,1]
};

@group(0) @binding(0) var<storage, read> agents_read: array<Agent>;
@group(0) @binding(1) var<storage, read_write> agents_write: array<Agent>;
@group(0) @binding(2) var<uniform> params: SimParams;  // dt, step_count, current_parity

// Safe x^4 without pow() — avoids NaN on pow(0.0, 4.0) on some GPU drivers
fn safe_pow4(x: f32) -> f32 {
    let x2 = x * x;
    return x2 * x2;
}

// Deterministic per-agent noise (replaces non-existent GPU rand())
fn agent_noise(agent_id: u32, step: u32, sigma: f32) -> f32 {
    var state = agent_id * 747796405u + step * 2891336453u + 1u;
    state = ((state >> 16u) ^ state) * 0x45d9f3bu;
    state = ((state >> 16u) ^ state) * 0x45d9f3bu;
    state = (state >> 16u) ^ state;
    let uniform_01 = f32(state) / 4294967295.0;
    return sigma * uniform_01;  // uniform [0, sigma]
}

@compute @workgroup_size(256)
fn idm_update(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= arrayLength(&agents_read)) { return; }

    let me = agents_read[idx];

    // Semi-synchronous: only process agents matching current_parity
    if (me.parity != params.current_parity) { return; }

    var new_accel: f32;

    if (me.leader_idx == 0xFFFFFFFFu) {
        // Free-flow: accelerate toward desired speed
        let v_ratio = me.speed / max(me.v0, 0.1);
        new_accel = me.a_max * (1.0 - safe_pow4(v_ratio));
    } else {
        // Car-following: IDM equation (Treiber 2013 variant with zero-speed fix)
        let leader = agents_read[me.leader_idx];
        let gap = max(leader.pos_x - me.pos_x - leader.length, 0.01);
        let delta_v = me.speed - leader.speed;

        // Desired gap
        let interaction_term = (me.speed * delta_v) / (2.0 * sqrt(me.a_max * me.b_comfort));
        let s_star = me.s0 + max(0.0, me.speed * me.T + interaction_term);

        // IDM acceleration with safe pow functions
        let v_ratio = me.speed / max(me.v0, 0.1);
        let gap_ratio = s_star / max(gap, 0.1);
        new_accel = me.a_max * (1.0 - safe_pow4(v_ratio) - gap_ratio * gap_ratio);
    }

    // Zero-speed kickstart: prevent IDM deadlock at v=0 (known literature issue)
    if (me.speed < 0.1 && me.leader_idx != 0xFFFFFFFFu) {
        let leader = agents_read[me.leader_idx];
        let gap = leader.pos_x - me.pos_x - leader.length;
        if (gap > me.s0 + 1.0) {
            new_accel = max(new_accel, 0.5);  // gentle start (0.5 m/s²)
        }
    }

    // Apply driver imperfection noise (deterministic per agent per step)
    let noise = agent_noise(idx, params.step_count, me.sigma);
    new_accel = new_accel - noise;

    // Clamp acceleration
    new_accel = clamp(new_accel, -me.b_comfort * 2.0, me.a_max);

    // Ballistic position update
    let new_speed = max(0.0, me.speed + new_accel * params.dt);
    let new_pos_x = me.pos_x + 0.5 * (me.speed + new_speed) * params.dt;

    // Write results
    agents_write[idx].speed = new_speed;
    agents_write[idx].accel = new_accel;
    agents_write[idx].pos_x = new_pos_x;
}
```

**Performance estimate:**
- 256 threads per workgroup × 64 workgroups = 16,384 parallel agents per dispatch
- RTX 4090: ~128 concurrent workgroups = **~500K agents in single dispatch**
- Compute time: **< 1ms for 500K agents** (arithmetic-bound, not memory-bound)

### 4.2 Pedestrian: Social Force Model (Helbing)

```
SOCIAL FORCE MODEL (2D, GPU-accelerated):

For each pedestrian i:

  F_total = F_desired + Σ F_repulsion(j) + F_boundary + F_attraction

  Where:

  F_desired = m × (v0 × ê_destination - v_current) / τ
              ↑ drives pedestrian toward their goal

  F_repulsion(j) = A × exp((r_ij - d_ij) / B) × n̂_ij + body_force + friction
                   ↑ pushes away from other pedestrians
                   ↑ A=2000N, B=0.08m (Helbing 1995 calibrated values)

  F_boundary = A_wall × exp((r_i - d_wall) / B_wall) × n̂_wall
               ↑ pushes away from walls/obstacles

  F_attraction = group_cohesion + point_of_interest
                 ↑ optional: friends walk together, interesting shop windows

GPU CHALLENGE: O(N²) pairwise force computation
SOLUTION: Spatial hash grid — only compute forces within interaction radius

  ┌────┬────┬────┬────┐
  │ 01 │ 02 │ 03 │ 04 │   Grid cell size = interaction radius (2m)
  ├────┼────┼────┼────┤
  │ 05 │ 06 │ 07 │ 08 │   Each pedestrian checks only 9 cells
  ├────┼────┼────┼────┤   (own cell + 8 neighbors)
  │ 09 │ 10 │ 11 │ 12 │
  ├────┼────┼────┼────┤   Complexity: O(N × K) where K ≈ 10-20
  │ 13 │ 14 │ 15 │ 16 │   (average neighbors in radius)
  └────┴────┴────┴────┘

GPU Implementation:
  Pass 1: Assign pedestrians to grid cells (parallel prefix sum)
  Pass 2: Sort by cell ID (GPU radix sort)
  Pass 3: For each pedestrian, scan neighboring cells, compute forces
  Pass 4: Integrate: v += F/m × dt, pos += v × dt
```

### 4.3 Lane-Change: MOBIL + Sublane

```
MOBIL (Minimize Overall Braking Induced by Lane change):

For each vehicle, evaluate lane change to left/right:

  Incentive criterion (should I change?):
    ã_new_follower - a_new_follower + p × (ã_me - a_me + ã_old_follower - a_old_follower) > Δa_th

    Where:
    ã = acceleration AFTER hypothetical lane change
    a = acceleration BEFORE lane change
    p = politeness factor (0 = selfish, 1 = altruistic)
    Δa_th = threshold (hysteresis to prevent oscillation)

  Safety criterion (can I change?):
    ã_new_follower ≥ -b_safe    (new follower won't have to brake dangerously)

SUBLANE RESOLUTION:
  Instead of discrete lanes, lateral position is continuous:

  Lane width = 3.2m
  Sublane resolution = 0.1m
  → 32 possible lateral positions per lane

  Lateral speed = 1.0 m/s (typical lane change over ~3.2 seconds)

  This enables:
  ✅ Motorcycle filtering between cars
  ✅ Bicycle overtaking within lane
  ✅ Emergency vehicle passing
  ✅ Gradual lane change animation (not teleporting)
```

### 4.4 Multi-Resolution Simulation

```
KEY INNOVATION: Switch between micro and meso PER EDGE at runtime

┌─────────────────────────────────────────────────────┐
│                 CITY-WIDE NETWORK                    │
│                                                      │
│   Meso Zone (queue-based)     Micro Zone (agent-based)
│   ~90% of edges               ~10% of edges         │
│   ┌─────────────────┐        ┌─────────────────┐   │
│   │  Edge = queue    │        │  Edge = agents   │   │
│   │  ─────────→      │   ←→   │  🚗 🚗  🚗 🚗   │   │
│   │  flow, density   │  border │  individual pos  │   │
│   │  travel time     │  sync  │  speed, accel    │   │
│   │                  │        │  lane, lateral   │   │
│   │  O(1) per edge   │        │  O(N) per agent  │   │
│   └─────────────────┘        └─────────────────┘   │
│                                                      │
│   MESO → MICRO transition (collision-safe):          │
│   1. Check if receiving micro-edge has a queue        │
│   2. If queue exists:                                 │
│      → Spawn at BACK of queue with speed = 0          │
│   3. If no queue:                                     │
│      → Spawn at edge start with speed =               │
│        min(meso_exit_speed, edge_speed_limit)         │
│   4. If signal is red: spawn with speed = 0           │
│   5. Instantiate full ECS components                  │
│                                                      │
│   MICRO → MESO transition (queue-safe):              │
│   1. Vehicle MUST reach END of micro-edge first       │
│   2. Record actual exit speed + timestamp              │
│   3. Add to meso queue with remaining_travel_time     │
│   4. If vehicle stuck in micro queue: do NOT transition│
│      (wait until vehicle naturally exits the edge)    │
│   5. Destroy agent entity only after full exit        │
│                                                      │
│   BUFFER ZONE (50m):                                  │
│   The last 50m of a micro zone stays micro even if    │
│   zone resolution switches. Prevents instant          │
│   materialization/dematerialization artifacts at       │
│   the boundary. Vehicles in buffer zone complete      │
│   their micro traversal before transitioning.         │
└─────────────────────────────────────────────────────┘

Runtime switching:
  API: velos.set_zone_resolution("intersection_42", Resolution::Micro)
  → All edges within radius switch to micro
  → Agents materialize from queue state
  → Switch back when analysis complete
```

---

## 5. Crate Structure & Module Map {#5-crate-structure}

```
velos/
├── Cargo.toml                          # Workspace root
├── crates/
│   ├── velos-core/                     # ECS world, entity management, time
│   │   ├── src/
│   │   │   ├── world.rs                # ECS world wrapper (hecs or custom)
│   │   │   ├── components.rs           # All component types
│   │   │   ├── scheduler.rs            # System execution order & dependencies
│   │   │   ├── time.rs                 # Simulation clock, step control
│   │   │   ├── events.rs              # Event bus (spawn, remove, signal change)
│   │   │   └── gridlock.rs            # Gridlock detection + resolution (C3)
│   │   └── Cargo.toml
│   │
│   ├── velos-network/                  # Road network graph + spatial index
│   │   ├── src/
│   │   │   ├── graph.rs                # Directed graph: nodes, edges, connections
│   │   │   ├── edge.rs                 # Edge: lanes, speed limit, length, shape
│   │   │   ├── junction.rs             # Junction: priority, connections, traffic lights
│   │   │   ├── spatial.rs              # R-tree spatial index for nearest-edge queries
│   │   │   ├── geometry.rs             # EdgeGeometry, local→world transform (C5)
│   │   │   ├── import_osm.rs           # OSM → network conversion
│   │   │   ├── import_sumo.rs          # SUMO .net.xml → network conversion
│   │   │   └── pathfinding.rs          # Dijkstra/A* + Contraction Hierarchies (M4)
│   │   └── Cargo.toml
│   │
│   ├── velos-vehicle/                  # Vehicle simulation systems
│   │   ├── src/
│   │   │   ├── car_following/
│   │   │   │   ├── mod.rs              # CarFollowingModel trait
│   │   │   │   ├── idm.rs              # Intelligent Driver Model (default)
│   │   │   │   ├── krauss.rs           # Krauss safe-speed model (SUMO compat)
│   │   │   │   └── w99.rs              # Wiedemann 99 (PTV compat)
│   │   │   ├── lane_change/
│   │   │   │   ├── mod.rs              # LaneChangeModel trait
│   │   │   │   ├── mobil.rs            # MOBIL incentive-based
│   │   │   │   └── sublane.rs          # Continuous lateral resolution
│   │   │   ├── insertion.rs            # Vehicle spawning + departure logic
│   │   │   └── routing.rs             # Dynamic rerouting during simulation
│   │   └── Cargo.toml
│   │
│   ├── velos-pedestrian/               # Pedestrian simulation systems
│   │   ├── src/
│   │   │   ├── social_force.rs         # Helbing social force model
│   │   │   ├── spatial_hash.rs         # 2D grid for neighbor queries
│   │   │   ├── navigation.rs           # Pedestrian pathfinding on walkable areas
│   │   │   ├── crossing.rs             # Pedestrian-vehicle interaction at crossings
│   │   │   └── group.rs               # Group behavior (families, crowds)
│   │   └── Cargo.toml
│   │
│   ├── velos-signal/                   # Traffic signal controllers
│   │   ├── src/
│   │   │   ├── fixed.rs                # Fixed-time phase controller
│   │   │   ├── actuated.rs             # Detector-actuated controller
│   │   │   ├── adaptive.rs             # Adaptive (RL-ready interface)
│   │   │   └── nema.rs                # NEMA dual-ring controller
│   │   └── Cargo.toml
│   │
│   ├── velos-meso/                     # Mesoscopic simulation (queue-based)
│   │   ├── src/
│   │   │   ├── queue.rs                # Edge-level queue model
│   │   │   ├── travel_time.rs          # BPR / conical delay functions
│   │   │   └── transition.rs           # Meso ↔ micro agent materialization
│   │   └── Cargo.toml
│   │
│   ├── velos-gpu/                      # GPU compute pipeline
│   │   ├── src/
│   │   │   ├── device.rs               # wgpu device/queue initialization
│   │   │   ├── buffers.rs              # ECS component → GPU buffer mapping
│   │   │   ├── pipelines.rs            # Compute pipeline management
│   │   │   └── dispatch.rs            # Workgroup dispatch + synchronization
│   │   ├── shaders/
│   │   │   ├── idm_car_following.wgsl  # IDM compute shader
│   │   │   ├── krauss_car_following.wgsl
│   │   │   ├── position_update.wgsl    # Ballistic position integration
│   │   │   ├── social_force.wgsl       # Pedestrian force computation
│   │   │   ├── spatial_hash.wgsl       # GPU spatial hash construction
│   │   │   └── lane_change.wgsl       # MOBIL evaluation shader
│   │   └── Cargo.toml
│   │
│   ├── velos-agent/                    # NEW — Agent intelligence & decisions
│   │   ├── src/
│   │   │   ├── brain.rs                # Per-step decision loop orchestration
│   │   │   ├── perception.rs           # GPU perception system
│   │   │   ├── cost_function.rs        # Multi-factor pathfinding cost (CS2-inspired)
│   │   │   ├── reroute_scheduler.rs    # Staggered reroute batching
│   │   │   ├── profile.rs              # Agent profiles (Commuter, Bus, Emergency...)
│   │   │   └── memory.rs              # Route memory & learning
│   │   └── Cargo.toml
│   │
│   ├── velos-predict/                  # NEW — Hierarchical prediction pipeline
│   │   ├── src/
│   │   │   ├── overlay.rs              # PredictionOverlay + lock-free ArcSwap
│   │   │   ├── provider.rs             # PredictionProvider trait (pluggable ML)
│   │   │   ├── ensemble.rs             # Built-in ensemble (BPR + ETS + historical)
│   │   │   ├── bpr.rs                  # BPR delay function extrapolation
│   │   │   ├── ets.rs                  # Exponential smoothing correction
│   │   │   ├── historical.rs           # Historical pattern matcher
│   │   │   ├── scheduler.rs            # Async dual-loop prediction scheduling
│   │   │   ├── registry.rs             # Model registry + hot-swap + A/B testing
│   │   │   └── arrow_bridge.rs        # Arrow IPC for external ML models
│   │   ├── shaders/
│   │   │   ├── prediction_upload.wgsl  # Upload overlay to GPU
│   │   │   └── cost_evaluation.wgsl   # GPU agent cost evaluation
│   │   └── Cargo.toml
│   │
│   ├── velos-v2i/                      # NEW — Vehicle-to-Infrastructure
│   │   ├── src/
│   │   │   ├── spat.rs                 # Signal Phase & Timing broadcast
│   │   │   ├── priority.rs             # Signal priority request/grant
│   │   │   ├── cooperative.rs          # Cooperative intersection (slot-based)
│   │   │   ├── signs.rs                # Traffic sign interaction
│   │   │   ├── variable_speed.rs       # Dynamic speed limit zones
│   │   │   └── congestion_pricing.rs  # Pricing zone cost injection
│   │   └── Cargo.toml
│   │
│   ├── velos-demand/                   # Demand generation from counter data
│   │   ├── src/
│   │   │   ├── od_matrix.rs            # OD matrix loading + trip generation
│   │   │   ├── route_sampler.rs        # LP-based route selection from counts
│   │   │   ├── calibrator.rs           # Runtime flow calibration
│   │   │   ├── import_sumo.rs          # SUMO .rou.xml import
│   │   │   └── sensor_bridge.rs       # Real-time sensor → demand mapping
│   │   └── Cargo.toml
│   │
│   ├── velos-output/                   # Output & recording
│   │   ├── src/
│   │   │   ├── fcd.rs                  # Floating Car Data (all agent positions)
│   │   │   ├── edge_data.rs            # Per-edge aggregated statistics
│   │   │   ├── detector.rs             # Virtual detector measurements
│   │   │   ├── emissions.rs            # HBEFA-based emissions model (M2)
│   │   │   ├── parquet.rs              # Apache Parquet writer
│   │   │   └── sumo_xml.rs            # SUMO-compatible XML output
│   │   └── Cargo.toml
│   │
│   ├── velos-calibration/              # NEW — Calibration & validation (M5)
│   │   ├── src/
│   │   │   ├── geh.rs                  # GEH statistic computation
│   │   │   ├── optimizer.rs            # Bayesian optimization (argmin crate)
│   │   │   ├── validation.rs           # Validation report generator
│   │   │   └── warmup.rs             # Warm-up period handling (m6)
│   │   └── Cargo.toml
│   │
│   ├── velos-scenario/                 # NEW — Scenario management (M10)
│   │   ├── src/
│   │   │   ├── scenario.rs             # Scenario definition + modification DSL
│   │   │   ├── batch.rs                # Batch execution orchestrator
│   │   │   ├── comparison.rs           # MOE comparison matrix
│   │   │   └── export.rs             # Result export (CSV, GeoJSON)
│   │   └── Cargo.toml
│   │
│   ├── velos-api/                      # External API server
│   │   ├── src/
│   │   │   ├── grpc_server.rs          # gRPC control + subscription service
│   │   │   ├── websocket.rs            # WebSocket for visualization streaming
│   │   │   ├── traci_compat.rs         # SUMO TraCI protocol compatibility layer
│   │   │   └── arrow_ipc.rs           # Arrow IPC for ML pipeline bridge
│   │   ├── proto/
│   │   │   ├── velos.proto             # Core API definitions
│   │   │   └── stream.proto           # Streaming subscription definitions
│   │   └── Cargo.toml
│   │
│   ├── velos-render/                   # Native wgpu visualization
│   │   ├── src/
│   │   │   ├── camera.rs               # 3D/2D camera control
│   │   │   ├── terrain.rs              # CityGML / 3D Tiles terrain rendering
│   │   │   ├── agents.rs               # Instanced rendering for vehicles/peds
│   │   │   ├── network_layer.rs        # Road network overlay
│   │   │   └── heatmap.rs             # Density/speed heatmap overlay
│   │   └── Cargo.toml
│   │
│   └── velos-cli/                      # Command-line interface
│       ├── src/
│       │   └── main.rs                 # CLI entry point: run, import, convert
│       └── Cargo.toml
│
├── tools/                              # Python tooling
│   ├── py-velos/                       # Python bindings (PyO3)
│   │   ├── src/lib.rs                  # PyO3 module definition
│   │   └── python/
│   │       └── velos/
│   │           ├── __init__.py
│   │           ├── simulation.py       # Python simulation control
│   │           ├── demand.py           # Demand generation helpers
│   │           └── ml_bridge.py       # Arrow IPC bridge for ML models
│   ├── osm_import.py                   # OSM preprocessing
│   └── visualize.py                   # Quick matplotlib visualization
│
└── examples/
    ├── hello_intersection/             # Single intersection demo
    ├── city_grid/                      # 10×10 grid network
    └── osm_district/                  # Real OSM district import
```

### Dependency Graph

```
velos-cli
  └── velos-api
        ├── velos-core
        │     └── (hecs, glam)
        ├── velos-agent          ← NEW: agent intelligence
        │     └── velos-core, velos-network, velos-predict, velos-v2i
        ├── velos-predict        ← NEW: prediction pipeline
        │     └── velos-core, velos-network, (arc-swap, arrow)
        ├── velos-v2i            ← NEW: vehicle-to-infrastructure
        │     └── velos-core, velos-network, velos-signal
        ├── velos-vehicle
        │     └── velos-core, velos-network
        ├── velos-pedestrian
        │     └── velos-core, velos-network
        ├── velos-signal
        │     └── velos-core, velos-network
        ├── velos-meso
        │     └── velos-core, velos-network
        ├── velos-gpu
        │     └── velos-core, (wgpu)
        ├── velos-demand
        │     └── velos-core, velos-network
        ├── velos-output
        │     └── velos-core, (arrow, parquet)
        └── velos-render
              └── velos-core, velos-gpu, (wgpu, winit)
```

### Key Rust Crate Dependencies

| Crate | Purpose | Why |
|-------|---------|-----|
| `wgpu` | GPU compute + rendering | WebGPU standard, cross-platform (Vulkan/Metal/DX12) |
| `hecs` | ECS framework | Lightweight, no macro magic, easy GPU buffer mapping |
| `glam` | Math (Vec2, Vec3, Mat4) | SIMD-optimized, no_std compatible |
| `tonic` | gRPC server | Async, production-grade Rust gRPC |
| `arrow` | Apache Arrow | Zero-copy data exchange with Python/ML |
| `rstar` | R-tree spatial index | Nearest-neighbor on road network |
| `osmpbfreader` | OSM PBF parsing | Fast .osm.pbf import |
| `quick-xml` | XML parsing | SUMO .net.xml / .rou.xml import |
| `pyo3` | Python bindings | PyO3 for `import velos` from Python |
| `tokio` | Async runtime | API server, WebSocket, streaming |
| `rayon` | CPU parallelism | Parallel iteration for CPU-bound systems |
| `arc-swap` | Lock-free atomic pointer | Prediction overlay swap (zero-latency read) |
| `rumqttc` | MQTT client | Real-time sensor data ingestion |
| `linfa` | Rust ML toolkit | Built-in ETS/statistical models (no Python dependency) |
| `fast_paths` | Contraction Hierarchies | 0.01ms/query vs 0.5ms for A* (M4) |
| `argmin` | Optimization framework | Bayesian parameter tuning for calibration (M5) |

---

## 6. Data Model & Storage {#6-data-model}

### ECS Component Types

```rust
// velos-core/src/components.rs

// === Spatial Components (GPU-resident) ===

#[repr(C)]              // C layout for GPU buffer compatibility
pub struct Position {
    pub x: f32,          // longitudinal position on edge (meters from start)
    pub y: f32,          // lateral position within lane (meters from center)
}

#[repr(C)]
pub struct Velocity {
    pub speed: f32,      // longitudinal speed (m/s)
    pub lateral: f32,    // lateral speed during lane change (m/s)
}

#[repr(C)]
pub struct Acceleration {
    pub longitudinal: f32,
    pub lateral: f32,
}

// === Identity Components (CPU-only) ===

pub struct AgentType(pub AgentKind);

pub enum AgentKind {
    PassengerCar,
    Truck,
    Bus,
    Motorcycle,
    Bicycle,
    Pedestrian,
    EmergencyVehicle,
    Tram,                // rail-based public transport
    Autonomous,          // self-driving vehicle
}

// === Public Transport Components (M1) ===

pub struct PublicTransportRoute {
    pub line_id: String,
    pub stops: Vec<StopInfo>,
    pub timetable: Vec<ScheduledArrival>,
    pub loop_route: bool,           // returns to start after last stop
}

pub struct StopInfo {
    pub stop_id: StopId,
    pub edge_id: EdgeId,
    pub position: f32,               // meters along edge
    pub dwell_model: DwellModel,
}

pub enum DwellModel {
    Fixed(f32),                                          // fixed seconds
    PassengerBased { board_rate: f32, alight_rate: f32 },// seconds per passenger
    Empirical(Vec<(f32, f32)>),                          // time-of-day → dwell seconds
}

pub struct ScheduledArrival {
    pub stop_index: usize,
    pub scheduled_time: f64,         // sim seconds
    pub headway_recovery: bool,      // try to recover schedule if late
}

pub struct VehicleParams {
    pub length: f32,        // meters
    pub width: f32,
    pub max_speed: f32,     // m/s
    pub max_accel: f32,     // m/s²
    pub max_decel: f32,     // m/s²
    pub min_gap: f32,       // meters
    pub tau: f32,           // reaction time (seconds)
    pub sigma: f32,         // driver imperfection [0,1] (uses deterministic PCG hash on GPU)
    pub cf_model: CarFollowingModelType,
    pub emission_class: EmissionClass,    // for emissions output (M2)
}

// === Emissions Model (M2) — HBEFA-based instantaneous emissions ===

pub enum EmissionClass {
    Euro6Diesel,
    Euro6Petrol,
    EV,
    Hybrid,
    HGV,
    BusDiesel,
    BusEV,
    Motorcycle,
    Bicycle,    // zero emissions
}

/// Instantaneous emission rates per agent per step
/// Aggregated per-edge for output (CO2 g/km, NOx mg/km, PM2.5 μg/km)
pub struct EmissionRates {
    pub co2_g_per_s: f32,
    pub nox_mg_per_s: f32,
    pub pm25_ug_per_s: f32,
    pub fuel_ml_per_s: f32,
}

// See velos-output/src/emissions.rs for HBEFA lookup: f(speed, accel, emission_class)

// === Routing Components (CPU, variable-size) ===

/// Route storage uses a flat arena allocator (m4) to avoid per-agent Vec heap allocation.
/// 500K agents × Vec overhead (24 bytes + heap) causes memory fragmentation.
/// Arena: flat route table with offset/length per agent → contiguous memory.
pub struct Route {
    pub route_offset: u32,       // index into global RouteArena
    pub route_length: u16,       // number of edges in route
    pub current_index: u16,      // current position in route
    pub depart_time: f64,        // simulation seconds
}

/// Global arena for all agent routes (contiguous Vec<EdgeId>)
/// Routes are appended when agents are created, rewritten on reroute.
/// Compacted periodically when fragmentation exceeds threshold.
pub struct RouteArena {
    pub edges: Vec<EdgeId>,      // flat array of all route edges
    pub free_list: Vec<(u32, u16)>, // (offset, length) of freed slots
}

// === Relationship Components (GPU-resident) ===

#[repr(C)]
pub struct LeaderRef {
    pub leader_entity: u32,     // entity index of leading vehicle (u32::MAX = none)
    pub gap: f32,               // current gap to leader (meters)
}

// === Pedestrian-Specific Components ===

#[repr(C)]
pub struct PedestrianForce {
    pub fx: f32,               // total force X
    pub fy: f32,               // total force Y
    pub desired_speed: f32,    // preferred walking speed
    pub destination_x: f32,    // next waypoint X
    pub destination_y: f32,    // next waypoint Y
}

// === Mesoscopic Components ===

pub struct MesoQueue {
    pub edge_id: EdgeId,
    pub vehicles: VecDeque<MesoVehicle>,
    pub free_flow_time: f64,   // seconds
    pub current_density: f32,  // vehicles/km
}

// === Gridlock Detection Components (C3) ===

pub struct GridlockState {
    pub stopped_since: Option<f64>,       // sim time when speed first hit 0
    pub gridlock_flag: bool,               // true if part of detected cycle
}

/// Gridlock detection system (runs every 60 sim-seconds, O(N)):
///   1. Per-step lightweight check:
///      If vehicle.speed == 0 for > T_threshold (default 300s) → mark "potentially gridlocked"
///   2. Periodic cycle detection:
///      Build waiting-for graph: Vehicle A → Vehicle B → ... → Vehicle A
///      If cycle detected → GRIDLOCK confirmed
///   3. Resolution strategies (configurable via GridlockPolicy):
///      a) Teleport: remove vehicle, place at next edge (SUMO-style --time-to-teleport)
///      b) Reroute: force gridlocked vehicles onto alternative routes
///      c) Signal override: force green for gridlocked approach
///      d) Log + alert: notify API subscribers, record for analysis
pub enum GridlockPolicy {
    Teleport { timeout_seconds: f64 },
    Reroute,
    SignalOverride,
    LogOnly,
}
```

### Network Data Model

```rust
// velos-network/src/graph.rs

pub struct Network {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub connections: Vec<Connection>,     // edge-to-edge turn connections
    pub junctions: Vec<Junction>,
    pub spatial_index: RTree<EdgeId>,     // for nearest-edge queries
    pub traffic_handedness: TrafficHandedness,  // left/right-hand traffic
}

pub enum TrafficHandedness {
    RightHand,  // US, Europe, China (default)
    LeftHand,   // UK, Japan, Australia, India
}

pub struct Edge {
    pub id: EdgeId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub lanes: Vec<Lane>,
    pub length: f32,                       // meters
    pub speed_limit: f32,                  // m/s (f32::INFINITY if no limit)
    pub shape: Vec<[f32; 2]>,             // polyline geometry (world coordinates)
    pub geometry: EdgeGeometry,            // pre-computed for fast local→world transform
    pub edge_type: EdgeType,              // road, sidewalk, cycleway, rail
    pub resolution: SimResolution,        // Micro or Meso (runtime switchable)
}

/// Pre-computed geometry for O(log S) edge-local → world coordinate transform
/// Built once at network load time, invalidated on edge geometry change
pub struct EdgeGeometry {
    pub cumulative_distances: Vec<f32>,   // [0, d01, d01+d12, ...] for binary search
    pub directions: Vec<glam::Vec2>,       // unit direction per shape segment
    pub normals: Vec<glam::Vec2>,          // perpendicular per segment (for lateral offset)
}

impl EdgeGeometry {
    /// Convert edge-local (pos_x along edge, pos_y lateral offset) → world (x, y)
    /// Uses binary search on cumulative_distances for O(log S) per agent
    pub fn local_to_world(&self, pos_x: f32, pos_y: f32, shape: &[[f32; 2]]) -> glam::Vec2 {
        let seg = self.cumulative_distances.partition_point(|d| *d < pos_x).saturating_sub(1);
        let seg = seg.min(self.directions.len() - 1);
        let local_offset = pos_x - self.cumulative_distances[seg];
        let base = glam::Vec2::from(shape[seg]);
        let along = self.directions[seg] * local_offset;
        let lateral = self.normals[seg] * pos_y;
        base + along + lateral
    }

    /// GPU OPTION: upload EdgeGeometry as GPU storage buffer,
    /// compute world coords in a dedicated transform shader
    /// → avoids CPU bottleneck for 500K agents per frame
}

pub struct Lane {
    pub width: f32,                        // meters (default 3.2m)
    pub allowed: Vec<AgentKind>,          // which agent types can use this lane
    pub speed_limit: Option<f32>,         // lane-specific override (None = use edge limit)
}

pub enum SimResolution {
    Micro,   // individual agent tracking
    Meso,    // queue-based aggregation
}

/// All internal units are SI (meters, m/s, seconds, radians).
/// Imperial unit conversion happens at import/export boundaries only.
/// See: velos-network/src/import_osm.rs::convert_mph_to_ms()
```

---

## 7. API Contracts {#7-api-contracts}

### gRPC Service Definition

```protobuf
// proto/velos.proto

syntax = "proto3";
package velos;

service VelosSimulation {
    // Simulation control
    rpc Start(StartRequest) returns (StartResponse);
    rpc Step(StepRequest) returns (StepResponse);          // advance N steps
    rpc Pause(Empty) returns (Empty);
    rpc Reset(Empty) returns (Empty);

    // Agent management (all return typed responses with error handling)
    rpc AddVehicle(AddVehicleRequest) returns (AddVehicleResponse);
    rpc AddPedestrian(AddPedestrianRequest) returns (AddPedestrianResponse);
    rpc RemoveAgent(AgentId) returns (VelosResponse);
    rpc RerouteAgent(RerouteRequest) returns (VelosResponse);

    // Network mutation (what-if scenarios)
    rpc BlockEdge(BlockEdgeRequest) returns (VelosResponse);
    rpc SetSignalPhase(SignalPhaseRequest) returns (VelosResponse);
    rpc SetZoneResolution(ZoneResolutionRequest) returns (VelosResponse);

    // Scenario management (M10)
    rpc CreateScenario(CreateScenarioRequest) returns (ScenarioId);
    rpc RunScenarioBatch(BatchRequest) returns (stream BatchProgress);
    rpc CompareScenarios(CompareRequest) returns (ComparisonResult);

    // Subscriptions (streaming)
    rpc SubscribeAgentPositions(SubscribeRequest) returns (stream AgentFrame);
    rpc SubscribeEdgeStats(SubscribeRequest) returns (stream EdgeStatsFrame);
    rpc SubscribeDetectors(SubscribeRequest) returns (stream DetectorFrame);

    // Demand
    rpc InjectDemandFromCounts(CountData) returns (DemandSummary);
    rpc SetCalibrator(CalibratorConfig) returns (Empty);

    // Query
    rpc GetAgentState(AgentId) returns (AgentState);
    rpc GetEdgeState(EdgeId) returns (EdgeState);
    rpc GetSimulationStats(Empty) returns (SimStats);
}

// === Error Types (M8) ===

message VelosError {
    ErrorCode code = 1;
    string message = 2;
    map<string, string> metadata = 3;
}

enum ErrorCode {
    UNKNOWN = 0;
    EDGE_NOT_FOUND = 1;
    JUNCTION_NOT_FOUND = 2;
    AGENT_NOT_FOUND = 3;
    INVALID_ROUTE = 4;
    SIMULATION_NOT_RUNNING = 5;
    NETWORK_NOT_LOADED = 6;
    CAPACITY_EXCEEDED = 7;
    INVALID_SIGNAL_PHASE = 8;
    PREDICTION_MODEL_ERROR = 9;
    SCENARIO_NOT_FOUND = 10;
    GRIDLOCK_DETECTED = 11;
}

message AddVehicleResponse {
    oneof result {
        AgentId agent_id = 1;
        VelosError error = 2;
    }
}

message AddPedestrianResponse {
    oneof result {
        AgentId agent_id = 1;
        VelosError error = 2;
    }
}

message VelosResponse {
    oneof result {
        bool success = 1;
        VelosError error = 2;
    }
}

// === Streaming Messages ===

message AgentFrame {
    double sim_time = 1;
    // For <50K agents in viewport: use repeated AgentPosition
    repeated AgentPosition agents = 2;
    // For bulk streaming (M5/m5): use raw bytes with FlatBuffers encoding
    bytes agents_binary = 3;  // FlatBuffers-encoded for 500K+ agents
}

message AgentPosition {
    uint32 id = 1;
    float x = 2;          // world coordinate X (transformed from edge-local via EdgeGeometry)
    float y = 3;          // world coordinate Y
    float speed = 4;
    float heading = 5;    // radians
    AgentKind kind = 6;
}
```

### Python Bridge (PyO3)

```python
# Usage from Python ML pipeline

import velos
import pyarrow as pa

# Connect to running simulation
sim = velos.connect("localhost:50051")

# Get current state as Arrow table (zero-copy)
agents: pa.Table = sim.get_agent_positions_arrow()
# columns: [id, x, y, speed, heading, kind, edge_id]
# rows: 500,000 (zero-copy from Rust via Arrow IPC)

# Feed ML prediction back
predicted_od = my_lstm_model.predict(agents)
sim.inject_demand_from_od(predicted_od, interval=3600)

# Subscribe to streaming updates
for frame in sim.subscribe_agents(bbox=[x1, y1, x2, y2]):
    update_cesium(frame)  # push to CesiumJS via WebSocket
```

---

## 8. GPU Compute Pipeline {#8-gpu-compute}

### Frame Execution Timeline (Updated with Semi-Sync + Staging)

```
         0ms     2ms     4ms     6ms     8ms    10ms    12ms
CPU:     ├───────┼───────┼───────┼───────┼───────┼───────┤
         │Ingest │Drain  │Leader │Reroute│ Route │Output │
         │events │staging│index  │batch  │advance│record │
         │Queue  │buffer │per-   │(1000  │Edge   │Stream │
         │to     │Append │lane   │agents)│transit│to API │
         │staging│to ECS │sort   │CH A*  │Gridlk │       │
         │SPaT + │(0.2ms)│(2.5ms)│w/pred │check  │       │
         │V2I    │       │       │(0.7ms)│(60s)  │       │

GPU:     ├───────┼───────┼───────┼───────┼───────┼───────┤
                 │Upload │Percept│CF EVEN│CF ODD │Collis │
                 │dirty  │+pred  │dispch │dispch │correct│
                 │ranges │overlay│(1.5ms)│(1.5ms)│+cost  │
                 │(0.5ms)│(0.3ms)│+SF    │       │+downld│
                 │       │       │       │       │(0.8ms)│

ASYNC:   ├─────────────────────────────────────────────────
         │ Prediction thread: ensemble model running
         │ (publishes new overlay via ArcSwap every ~600 steps)
         │ ML bridge: Arrow IPC exchange with Python (if active)
         │ Calibration thread: GEH check every 300s (if active)

Total frame time target: < 12ms (including semi-sync overhead)
  - GPU: upload(0.5) + percept(0.3) + CF_EVEN(1.5) + CF_ODD(1.5) +
         collision+cost+download(0.8) = ~4.6ms
  - CPU: staging(0.2) + leader(2.5) + reroute(0.7) + output(1.0) = ~4.4ms
  - Overlap: CPU reroute runs during GPU CF dispatches → ~9ms effective
At Δt = 0.1s: ~11x faster than real-time for 500K agents
```

### Buffer Management Strategy (Staging + Double-Buffer)

```
TRIPLE-BUFFER ARCHITECTURE: READ + WRITE + STAGING

Buffer A (READ):    [agent₀ agent₁ ... agent₅₀₀₀₀₀]  ← shaders read from this
Buffer B (WRITE):   [agent₀ agent₁ ... agent₅₀₀₀₀₀]  ← shaders write to this
Buffer S (STAGING): [new agents queued by CPU]         ← CPU writes here asynchronously

RACE CONDITION PREVENTION:
  Problem: If CPU spawns agent at index N while GPU is uploading buffers,
  GPU reads partially-initialized agent → pos=0, speed=0, leader=garbage.

  Solution: Staging buffer pattern
  ┌─────────────────────────────────────────────────────────────────┐
  │ CPU thread:                                                      │
  │   spawn_agent(params) → push to STAGING ring buffer (lock-free)  │
  │                                                                  │
  │ At sync point (BEFORE GPU upload, step 4 in frame scheduler):    │
  │   1. Drain STAGING buffer                                        │
  │   2. Append fully-initialized agents to main ECS arrays          │
  │   3. Update GPU buffer capacity if needed (resize + re-bind)     │
  │   4. Mark new index range as "dirty" for upload                  │
  │   5. Upload dirty ranges to GPU                                  │
  │                                                                  │
  │ Cost: ~0.2ms latency per frame (amortized)                       │
  │ Alternative: double-buffer ECS arrays (more memory, zero latency)│
  └─────────────────────────────────────────────────────────────────┘

Memory layout per agent (GPU):
  ┌─────────┬─────────┬─────────┬─────────┬─────────┬─────────┬─────────┬─────────┐
  │ pos_x   │ pos_y   │ speed   │ accel   │ edge_id │ lane_idx│ leader  │ parity  │
  │ f32     │ f32     │ f32     │ f32     │ u32     │ u32     │ u32     │ u32     │
  └─────────┴─────────┴─────────┴─────────┴─────────┴─────────┴─────────┴─────────┘
  + IDM params: v0, T, a_max, b_comfort, s0, length, sigma (28 bytes)
  = 60 bytes per agent

  500K agents × 60 bytes = 30 MB per buffer
  × 2 (double buffer) = 60 MB
  + staging buffer       = ~1 MB (ring, 10K capacity)
  + pedestrian buffers   = ~10 MB (50K peds)
  ─────────────────────────────────────────
  Total GPU:             ~71 MB
  Well within 24 GB VRAM budget (RTX 4090)
```

### Leader Index Computation (Per-Lane + Lane-Change Aware)

```
THE CRITICAL PRE-STEP: determining who follows whom

This MUST run on CPU (topology-dependent). Leader assignment is PER-LANE,
not per-edge, to correctly handle multi-lane roads and lane-changing agents.

for each edge in network:
    // STEP 1: Per-lane leader assignment
    for each lane in edge.lanes:
        agents_in_lane = get_agents_in_lane(edge.id, lane.index)
        sort_by_position(agents_in_lane)  // front to back

        for i in 1..agents_in_lane.len():
            leader_buffer[agents_in_lane[i]] = agents_in_lane[i-1]

        // First vehicle in lane: check downstream edge for leader
        leader_buffer[agents_in_lane[0]] = check_downstream_edge_lane(lane.index)

    // STEP 2: Lane-changing agents (sublane position straddles two lanes)
    // These agents have TWO potential leaders: current lane + target lane
    for agent in lane_changing_agents(edge):
        leader_current = find_leader_in_lane(agent.current_lane, agent.pos)
        leader_target  = find_leader_in_lane(agent.target_lane, agent.pos)

        // Follow the CLOSER leader (more conservative = safer)
        leader_buffer[agent] = closer_leader(leader_current, leader_target)

        // Also check that new FOLLOWER in target lane can brake safely
        follower_target = find_follower_in_lane(agent.target_lane, agent.pos)
        if follower_target.exists:
            // Update follower's leader to include lane-changing agent
            leader_buffer[follower_target] = agent  // if agent is closer

    // STEP 3: Assign parity (EVEN/ODD) for semi-synchronous GPU dispatch
    for agent in agents_on_edge:
        agent.parity = agent.index % 2

Cost: O(N log N) sorting, parallelized with rayon across edges
Estimated: ~2.5ms for 500K agents on 16-core CPU
```

---

## 9. Visualization Architecture {#9-visualization}

```
DUAL RENDERING STRATEGY:

┌─────────────────────────────────────┐
│     Option A: Native wgpu Renderer   │
│     (desktop, high-performance)      │
│                                      │
│  ┌───────────────────────────────┐  │
│  │  wgpu render pipeline          │  │
│  │                                │  │
│  │  Pass 1: Terrain / buildings   │  │  ← CityGML → 3D Tiles → GPU mesh
│  │          (static, cached)      │  │
│  │                                │  │
│  │  Pass 2: Road network          │  │  ← line rendering from edge shapes
│  │          (static, cached)      │  │
│  │                                │  │
│  │  Pass 3: Agents (instanced)    │  │  ← 500K instances from ECS buffers
│  │          vehicle meshes        │  │     DIRECT GPU-TO-GPU (no CPU copy!)
│  │          pedestrian sprites    │  │
│  │                                │  │
│  │  Pass 4: Overlays              │  │  ← heatmaps, flow arrows, KPIs
│  │          (compute → fragment)  │  │
│  └───────────────────────────────┘  │
│                                      │
│  Advantage: simulation buffers ARE   │
│  render buffers — zero copy!         │
└─────────────────────────────────────┘

┌─────────────────────────────────────┐
│     Option B: CesiumJS Bridge        │
│     (web-based, shareable)           │
│                                      │
│  VELOS ──WebSocket──→ Browser        │
│                                      │
│  Protocol:                           │
│  {                                   │
│    "time": 1234.5,                   │
│    "agents": [                       │
│      {"id":0,"x":48.15,"y":11.58,   │
│       "speed":13.9,"kind":"car"},    │
│      ...                             │
│    ]                                 │
│  }                                   │
│                                      │
│  Optimizations (M9 — all required):  │
│  1. Viewport culling: only send      │
│     agents within camera frustum     │
│     → reduces to ~5K-50K typically   │
│  2. Level-of-detail (LOD):           │
│     > 2km: edge density colors only  │
│     500m-2km: pos + kind (8 bytes)   │
│     < 500m: full state (20 bytes)    │
│  3. Delta compression:               │
│     Only send agents moved > 1m      │
│     Stationary agents sent once      │
│  4. Binary FlatBuffers protocol:     │
│     ~4 bytes/agent (u16 x,y in tile) │
│     vs 20 bytes for MessagePack      │
│  5. Spatial tiling:                  │
│     City → 256×256 grid tiles        │
│     Client subscribes to visible     │
│     tiles only → server processes    │
│     only subscribed tiles            │
│  Result: ~500KB/frame at 30 FPS     │
│                                      │
│  CesiumJS renders:                   │
│  - 3D Tiles city model (CityGML)     │
│  - Agent billboards/models           │
│  - deck.gl heatmap overlay           │
└─────────────────────────────────────┘
```

---

## 10. Data Ingestion Pipeline {#10-data-ingestion}

```
┌───────────────────────────────────────────────────────┐
│                DATA INGESTION LAYER                    │
│                                                        │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐            │
│  │ CCTV     │  │ Radar    │  │ Loop     │            │
│  │ Counter  │  │ Sensor   │  │ Detector │            │
│  │ (MQTT)   │  │ (MQTT)   │  │ (MQTT)   │            │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘            │
│       └──────────────┼──────────────┘                  │
│                      ▼                                 │
│  ┌────────────────────────────────────┐               │
│  │  velos-demand/sensor_bridge.rs     │               │
│  │                                    │               │
│  │  1. MQTT subscriber (rumqttc)      │               │
│  │  2. Message normalization:         │               │
│  │     { edge_id, count, interval,    │               │
│  │       agent_type, timestamp }      │               │
│  │  3. Rolling window aggregation     │               │
│  │     (tumbling 5-min windows)       │               │
│  │  4. Route Sampler (LP solver):     │               │
│  │     counts → optimal route set     │               │
│  │  5. Calibrator updates:            │               │
│  │     adjust live vehicle counts     │               │
│  └────────────────────────────────────┘               │
│                      │                                 │
│          ┌───────────┼───────────┐                     │
│          ▼           ▼           ▼                     │
│   Add vehicles  Remove excess  Update signal           │
│   to match      vehicles       timing from             │
│   sensor count  if over-count  detector data           │
└───────────────────────────────────────────────────────┘
```

---

## 11. 9-Month Roadmap (5 Engineers) {#11-roadmap}

> **Revised from 6-month after architecture review (M11).** The expanded scope — agent
> intelligence, hierarchical prediction, V2I, calibration framework, scenario management,
> public transport, emissions — doubles the engineering surface area. 9 months with
> 5 engineers provides adequate buffer for the complexity involved.

### Team Allocation

| Role | Person | Focus |
|------|--------|-------|
| **E1: Simulation Lead** | Senior Rust + HPC | Core ECS, car-following models, GPU compute shaders, semi-sync pipeline |
| **E2: Network & Demand** | Mid-Senior Rust + GIS | Network import (OSM/SUMO), pathfinding (CH), demand generation, calibration |
| **E3: GPU & Rendering** | Mid-Senior Rust + wgpu | GPU pipeline, buffer management, staging buffer, native renderer |
| **E4: API & Integration** | Mid Rust + Python | gRPC server (with error types), Python bindings, CesiumJS bridge (spatial tiling) |
| **E5: Intelligence & Prediction** | Mid-Senior Rust + ML | Agent intelligence, prediction pipeline, V2I, scenario management |

### Month 1: Foundation — "Hello Intersection"

```
Week 1-2:
  E1: Scaffold workspace. Implement velos-core (ECS world, components, scheduler)
      + gridlock detection module (GridlockState component, GridlockPolicy enum)
  E2: Implement velos-network (graph model, SUMO .net.xml import, EdgeGeometry)
      + unit system documentation (all internal SI, conversion at boundaries)
  E3: Set up wgpu device, create compute pipeline skeleton
      + staging buffer ring (lock-free CPU→GPU agent queue)
  E4: Set up tonic gRPC server skeleton, proto definitions with VelosError types

Week 3-4:
  E1: Implement IDM car-following (CPU version first), position update system
      + safe_pow4, zero-speed kickstart, deterministic PCG noise per agent
  E2: Implement Dijkstra/A* pathfinding with BASIC cost function (time-only)
      + RouteArena (flat arena allocator, no per-agent Vec heap allocation)
      + AgentProfile component scaffolding (weights struct, default profiles)
  E3: Implement triple-buffered GPU storage (READ + WRITE + STAGING), upload/download
  E4: Implement basic CLI (load network, load routes, run N steps, print stats)

DELIVERABLE: Single intersection with 50 vehicles following IDM model
             + agents using basic A* with time-based cost function
             Output: CSV of positions per step, validate against SUMO output
MILESTONE: Car-following model matches SUMO within 5% on reference scenario
           Zero collisions verified (gap ≥ min_gap for all agents at all steps)
```

### Month 2: GPU + Semi-Sync — "Safe Parallelism at Scale"

```
Week 5-6:
  E1: Port IDM to WGSL compute shader (with safe_pow4 + zero-speed fix)
      + implement EVEN/ODD semi-synchronous dispatch (C1)
      + collision correction pass shader
  E2: Implement OSM import (osmpbfreader → network graph + EdgeGeometry build)
      + Contraction Hierarchies integration (fast_paths crate, M4)
  E3: Implement per-lane leader-index computation (C2), GPU dispatch pipeline
      + lane-change aware leader assignment for straddling agents
  E4: Implement AgentFrame streaming via gRPC subscription
      + VelosError handling in all RPC methods

Week 7-8:
  E1: Implement Krauss model (GPU, with deterministic noise), MOBIL lane-change (CPU)
  E2: Implement route sampler (LP-based demand from edge counts)
      + multi-factor cost function (time, comfort, safety, fuel, signal)
  E3: Benchmark: 10K → 100K → 500K vehicles, measure frame time
      + validate EVEN/ODD overhead vs pure parallel (should be ≤1.5x)
  E4: Implement WebSocket bridge for CesiumJS (FlatBuffers + spatial tiling, M9)

DELIVERABLE: GPU-accelerated simulation of 100K vehicles on real OSM network
             Semi-synchronous EVEN/ODD dispatch proven collision-free
             10x faster than real-time on RTX 3080
MILESTONE: Benchmark report: GPU advantage over SUMO + collision safety proof
           CH pathfinding: 1000 queries < 1ms with rayon parallelism
```

### Month 3: Agent Intelligence — "Smart Agents"

```
Week 9-10:
  E1: Implement sublane lane-change model (GPU)
  E2: Implement staggered reroute scheduler (1000 agents/step batched CH queries)
      + agent profile system (Commuter, Bus, Emergency, Tourist, etc.)
  E3: Implement perception_state.wgsl + cost_evaluation.wgsl GPU shaders
      (GPU cost eval is SCREENING FILTER only — single should_reroute bool, M3)
  E5: (joins) Implement velos-agent crate: brain.rs, cost_function.rs, profile.rs
      + multi-factor cost function (SINGLE source of truth, CPU-side, M3)
      + SPaT broadcast system (signal phase & timing to agents)

Week 11-12:
  E1: Implement traffic signal controllers (fixed + actuated + priority-aware)
  E2: Implement vehicle-pedestrian interaction at crossings
      + traffic sign interaction (speed limit, stop, yield, school zone)
  E3: Implement social-force pedestrian model (GPU, spatial hash)
      + anisotropic counter-flow factor (Helbing's directional weighting, m7)
  E4: Implement Python bindings (PyO3), Arrow IPC bridge
  E5: Signal priority request/grant for Bus/Emergency agents
      + agent memory system (route memory, learning from past reroutes)

DELIVERABLE: Intelligent agents with multi-factor rerouting on real network
             Agents reroute around blocked roads using prediction-informed cost
MILESTONE: Demo showing agents dynamically rerouting around blocked road
           Bus agents receiving signal priority at 3+ intersections
```

### Month 4: Pedestrians + Meso + Prediction — "People + Foresight"

```
Week 13-14:
  E1: Performance optimization pass (profiling, bottleneck elimination)
  E2: Implement calibrator system (runtime flow matching)
  E3: Optimize pedestrian spatial hash on GPU (radix sort)
  E4: Arrow IPC bridge for external ML models (PredictionProvider trait)
  E5: Implement meso queue model + meso→micro transition (C7 buffer zone fix)
      + BPR + ETS ensemble prediction engine (built-in, no ML dependency)
      + PredictionOverlay with ArcSwap lock-free swap

Week 15-16:
  E1: Implement public transport model (M1): stops, dwell time, timetable
  E2: Implement left-hand traffic support (m8): configurable handedness
  E3: Implement heatmap overlay (GPU compute → render)
  E4: Implement multi-resolution zone switching API
  E5: Async dual-loop architecture (fast sim + slow prediction threads)
      + route-level prediction pre-computation on CPU (M7)

DELIVERABLE: Mixed vehicle + pedestrian + bus simulation on city district
             Meso mode for 90% of network, micro for key intersections
             Built-in prediction overlay operational
MILESTONE: 500K vehicles + 50K pedestrians at real-time on single GPU
           Prediction overlay visible in output (predicted vs actual travel times)
           Meso→micro transition: zero phantom jams (C7 verified)
```

### Month 5: V2I + ML Bridge — "Intelligence Meets Reality"

```
Week 17-18:
  E1: Implement W99 car-following model (PTV Vissim compatibility)
  E2: MQTT sensor bridge (rumqttc), rolling window aggregation
  E3: Native wgpu renderer (instanced vehicles on terrain mesh)
  E4: CesiumJS integration demo (3D Tiles city + live agent overlay)
      + prediction overlay visualization (heatmap of predicted congestion)
  E5: Cooperative intersection management (slot-based crossing)
      + variable speed limit zones, congestion pricing zones

Week 19-20:
  E1: Implement emissions model (M2): HBEFA lookup, per-edge CO2/NOx/PM2.5
  E2: Implement SUMO TraCI compatibility layer (partial — key commands)
  E3: Implement agent decision visualization (why did agent reroute?)
  E4: Implement detector output (virtual loop detectors, edge stats)
      + gRPC VelosPrediction + VelosV2I + VelosAgent services
  E5: Model registry + hot-swap at runtime
      + hierarchical prediction: group-level (per zone, per agent class)
      + historical pattern matcher (Model C of ensemble)

DELIVERABLE: Real sensor data → prediction → smart agent reroute → CesiumJS 3D
             Full V2I: SPaT + priority + cooperative intersection + variable speed
MILESTONE: End-to-end pipeline: MQTT sensor → ensemble prediction → agent reroute
           Emissions output per edge per interval (CO2, NOx, PM2.5)
```

### Month 6: Calibration + Scenarios — "Trust the Numbers"

```
Week 21-22:
  E1: Implement warm-up period handling (m6): suppress stats during warm-up
  E2: Implement velos-calibration crate (M5): GEH statistic, Bayesian optimization
      + validation report output (GEH distribution, RMSE, scatter plot)
  E3: GPU error recovery (device lost, buffer overflow, shader compilation failure)
  E4: Implement velos-scenario crate (M10): scenario DSL, batch execution
      + MOE comparison matrix (delay, throughput, emissions per scenario)
  E5: Implement individual-tier prediction (emergency, fleet agents)
      + example Python LSTM prediction model with Arrow IPC

Week 23-24:
  E2: Automated parameter tuning pipeline (argmin Bayesian optimization)
      + validation against reference datasets (GEH < 5 for ≥85% detectors)
  E3: Implement speed_limit edge handling (f32::INFINITY for no-limit, m3)
  E4: Batch simulation orchestration (run N scenarios in parallel)
      + result aggregation API with export (CSV, GeoJSON)
  E5: A/B testing framework for comparing prediction models
      + ensemble auto-weight-tuning (shift weights toward best model)

DELIVERABLE: Calibration framework with automated GEH optimization
             Scenario management: define, run, compare multiple what-if scenarios
MILESTONE: Calibration passes HCM standard (GEH < 5 for ≥85% detectors)
           Scenario batch: 4 scenarios run in parallel with comparison output
```

### Month 7: Hardening — "Don't Crash, Don't Mislead"

```
Week 25-26:
  E1: Determinism verification (bitwise reproducible across runs)
      + verify PCG hash noise produces identical results on different GPUs
  E2: Edge case handling (network disconnections, invalid routes, zero-length edges)
      + demand overflow handling (cap at network capacity, queue excess)
  E3: Memory profiling (GPU + CPU), leak detection, arena compaction
  E4: Load testing (1000 concurrent gRPC subscribers, 100 WebSocket spatial-tile clients)
  E5: V2I edge cases (priority deadlock, competing emergency vehicles)
      + prediction accuracy metrics (MAPE, GEH per edge)

Week 27-28:
  ALL: Integration testing suite
       - Reference scenario vs SUMO output (regression tests)
       - Performance regression tests (fail if frame time > 12ms threshold)
       - Collision safety regression (gap ≥ min_gap verified every step)
       - Memory leak detection (GPU + CPU, 24-hour soak test)
       - Fuzzing of network import (malformed OSM/SUMO files)
       - Meso→micro transition fuzz (random zone switches under load)
       - Prediction accuracy regression (MAPE must stay < 15% on reference)
       - Agent rerouting validation (agents must improve travel time vs fixed-route)
       - V2I priority test (buses arrive ≥10% faster with priority enabled)
       - Calibration regression (GEH must pass on reference after re-calibration)
       - Gridlock detection test (inject known gridlock → detect + resolve)

DELIVERABLE: CI pipeline with automated testing, performance dashboards
MILESTONE: Zero crashes on 24-hour continuous simulation run
           Prediction MAPE < 15% at 5-min horizon after 30-min warmup
```

### Month 8: Polish + Documentation — "Make It Usable"

```
Week 29-30:
  E1: Documentation: architecture guide, model reference, API docs
      + agent intelligence guide (how to create custom profiles)
  E2: Example scenarios: single intersection, city grid, real city district
      + "smart corridor" scenario: V2I + prediction + rerouting demo
  E3: Docker container with GPU support (NVIDIA Container Toolkit)
  E4: Dashboard: Grafana integration for simulation KPIs
      + prediction accuracy dashboard (live MAPE, model comparison)
      + agent decision dashboard (reroute reasons, cost breakdowns)
  E5: Kubernetes Helm chart for cloud deployment
      + prediction model development guide
      + example Python LSTM prediction model (ready to train + deploy)

Week 31-32:
  E1: Calibration tutorial: step-by-step guide for new cities
  E2: Scenario management tutorial: create, run, compare what-if scenarios
  E3: Performance tuning guide (GPU selection, meso/micro ratio, batch sizes)
  E4: API reference documentation (gRPC, WebSocket, Python, Arrow IPC)
  E5: V2I deployment guide (SPaT integration with NTCIP controllers)

DELIVERABLE: Complete documentation suite, Docker/K8s deployment artifacts
MILESTONE: New engineer can set up, run, and calibrate a city in < 1 day
```

### Month 9: Demo + Ship — "City-Wide Showcase"

```
Week 33-34:
  ALL: City-wide demo scenario preparation
       - Import target city from OSM (full network + CityGML buildings)
       - Load historical sensor data (1 year of detector counts)
       - Calibrate against historical data (GEH < 5 target)
       - Define 4 what-if scenarios:
         Scenario A: Baseline (current conditions)
         Scenario B: Add bus lane on Main Street
         Scenario C: Bus lane + signal priority + prediction-informed routing
         Scenario D: Convert corridor to bike boulevard + congestion pricing

Week 35-36:
  ALL: Final demo + release
       - Run 24-hour simulation with prediction-informed smart agents
       - CesiumJS 3D + deck.gl 2D visualization
       - Show: agent rerouting around predicted congestion
       - Show: bus getting signal priority, reducing travel time 10%+
       - Show: prediction overlay (green/yellow/red) on city map
       - Show: ML model hot-swap (switch ensemble → LSTM live)
       - Show: emissions heatmap per scenario (CO2, NOx)
       - Show: scenario comparison dashboard (4 scenarios side-by-side)
       - Show: calibration report (GEH distribution, RMSE)
       - Stakeholder demo preparation + dry run

DELIVERABLE: Production-ready v0.1.0 with full city-wide demo
MILESTONE: City-wide simulation running real-time with:
           - 500K+ smart agents rerouting based on predictions
           - V2I signal priority operational across city
           - Built-in prediction with < 15% MAPE
           - Pluggable ML demonstrated with example LSTM
           - Calibrated against real data (GEH < 5 for ≥85% detectors)
           - 4 what-if scenarios with MOE comparison
           - Emissions output per scenario
```

### Gantt Summary (9 Months)

```
Month:    1           2           3           4           5           6           7           8           9
         ┌───────────┬───────────┬───────────┬───────────┬───────────┬───────────┬───────────┬───────────┬───────────┐
E1 (Lead)│ ECS+IDM   │ GPU semi- │ Sublane+  │ Perf opt  │ W99+Emis- │ Warmup+   │ Determin  │ Docs+     │ City-wide │
         │ foundation│ sync+CF   │ Signal    │ PubTrans  │ sions M2  │ speed lim │ +testing  │ tutorials │ demo      │
         │ gridlock  │ collision │ priority  │ M1 model  │           │ m3        │ 24h soak  │           │ release   │
         ├───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┤
E2 (Net) │ Network   │ OSM+CH   │ Reroute+  │ Calibratr │ MQTT+TraCI│ GEH calib │ Edge case │ Scenarios │ Calibrate │
         │ +Geometry │ pathfind  │ profiles  │ LH traff  │ sensor    │ Bayesian  │ demand    │ tutorial  │ real city │
         │ RouteAren │ M4        │ signs     │ m8        │ compat    │ M5        │ overflow  │           │           │
         ├───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┤
E3 (GPU) │ wgpu      │ Leader   │ Percept+  │ Ped GPU   │ Renderer  │ GPU error │ Mem prof  │ Docker    │ Perf      │
         │ staging   │ per-lane  │ Social    │ radix     │ heatmap   │ recovery  │ arena GC  │ K8s       │ tuning    │
         │ buffer    │ benchmark │ Force GPU │ sort      │ +decision │ m3 speed  │ 24h GPU   │           │ guide     │
         ├───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┤
E4 (API) │ gRPC+     │ WebSocket │ PyO3     │ Arrow IPC │ CesiumJS  │ Scenario  │ Load test │ API docs  │ Dashboard │
         │ errors    │ FlatBuf   │ Arrow    │ zone API  │ +pred viz │ batch M10 │ 1000 sub  │ Grafana   │ demo      │
         │ M8        │ tiles M9  │          │           │ +V2I API  │ MOE export│           │ dashbrd   │           │
         ├───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┼───────────┤
E5       │           │           │ Agent    │ Meso+BPR  │ Coop intx │ Individ   │ V2I edge  │ ML guide  │ Scenario  │
(Month3+)│     —     │     —     │ brain+   │ +ETS pred │ VarSpeed  │ pred+AB   │ cases     │ LSTM      │ compare   │
         │           │           │ V2I SPaT │ +ArcSwap  │ +historic │ autoweight│ pred MAPE │ example   │ 4 scenar  │
         └───────────┴───────────┴───────────┴───────────┴───────────┴───────────┴───────────┴───────────┴───────────┘
```

---

## 12. Risk Register & Trade-offs {#12-risks}

### Technical Risks

| # | Risk | Likelihood | Impact | Mitigation |
|---|------|-----------|--------|------------|
| R1 | **Semi-synchronous GPU update residual error** — EVEN/ODD split eliminates most stale-read drift but same-parity leaders still read 1-step-old data | Low | Medium | Collision correction pass runs after both EVEN+ODD dispatches (~0.3ms). Validates gap ≥ min_gap for all agents. Residual error bounded to 0.015m (1.5cm). Validated against SUMO reference output. |
| R2 | **wgpu compute shader limitations** — WGSL lacks features needed for complex models (no recursion, limited control flow) | Medium | Medium | Keep complex logic (routing, signal control) on CPU. Only port arithmetic-heavy, per-agent computation to GPU. Fallback: use `rust-gpu` to write shaders in Rust → SPIR-V. |
| R3 | **9-month timeline still tight for full scope** — expanded scope (agent intelligence, prediction, V2I, calibration, scenario mgmt) is substantial | Medium | Medium | Timeline extended from 6→9 months after architecture review. PoC milestone at Month 2 (100K agents). Calibration + scenario management added in Month 7-8. If GPU path stalls, fall back to CPU multi-threaded (rayon) as intermediate. Month 9 buffer for slip. |
| R4 | **OSM import quality** — real-world OSM data has errors (missing turn restrictions, wrong lane counts, disconnected edges) | High | Medium | Build validation pass that detects disconnected components, impossible turns, zero-length edges. Manual override API for corrections. Budget 1 week of data cleaning per city. |
| R5 | **Pedestrian social-force on GPU is numerically unstable** — force magnitudes explode at very close distances | Medium | Medium | Clamp minimum distance to body radius. Use Verlet integration instead of Euler for stability. Add maximum force limit per step. |
| R6 | **SUMO compatibility gap** — users expect exact SUMO output compatibility, but models differ subtly | Medium | Low | Don't promise bit-exact SUMO compatibility. Target "behavioral equivalence" (GEH < 5 on validation scenarios). Document all model differences. |
| R7 | **Prediction-induced oscillation** — agents all reroute to same "predicted clear" road, causing new congestion there (Braess's paradox) | High | High | Add stochastic perturbation to cost function per agent. Limit reroute rate per edge (cap % of agents that can switch to same alternative). Implement damping factor: agents trust prediction less if recent reroutes failed. |
| R8 | **Agent rerouting CPU bottleneck** — 1000 CH queries per step on 100K-edge graph | Low | Medium | Contraction Hierarchies (fast_paths crate) reduce query from 0.5ms→0.01ms. 1000 queries parallelized with rayon on 16 cores = ~0.7ms. CH graph re-contracted periodically (~30s) if edges blocked. Route-level prediction pre-computed on CPU (M7): single f32 route_predicted_cost uploaded to GPU. |
| R9 | **ML prediction model latency** — external Python model via Arrow IPC takes > 200ms, causing stale predictions | Medium | Medium | Built-in ensemble always runs as fallback. ML predictions are advisory overlay, not blocking. Monitor staleness and auto-fallback if ML model is too slow. |
| R10 | **V2I complexity scope creep** — cooperative intersection management is an entire research domain; could consume all Month 4 capacity | High | Medium | Implement basic SPaT + priority first (Week 9-10). Cooperative intersection is stretch goal for Month 4. Mark as "experimental" in v0.1.0. |

### Architectural Trade-offs

| Decision | Option A | Option B | Chosen | Rationale |
|----------|----------|----------|--------|-----------|
| ECS library | **hecs** (lightweight) | **bevy_ecs** (full framework) | hecs | Bevy brings rendering+audio+input we don't need. hecs gives raw ECS without framework coupling. Easier GPU buffer mapping. |
| GPU API | **wgpu** (WebGPU) | **CUDA** (NVIDIA only) | wgpu | wgpu runs on AMD, Intel, Apple Silicon, AND NVIDIA. CUDA is faster but locks to NVIDIA. For a product that cities deploy, vendor neutrality matters. |
| Network format | **Custom binary** | **SUMO .net.xml compat** | Both | Binary for production speed (mmap-able). SUMO XML import for ecosystem compatibility. Convert on first load, cache binary. |
| Car-following default | **Krauss** (SUMO compat) | **IDM** (smoother) | IDM | IDM produces smoother acceleration profiles, better for visualization. Krauss available as option for SUMO validation. |
| Language | **Rust** | **C++ (CUDA)** | Rust | Memory safety eliminates entire class of simulation bugs (dangling pointers, data races). wgpu is Rust-native. PyO3 gives excellent Python bridge. |
| Pedestrian model | **Striping** (SUMO compat) | **Social Force** (physics) | Social Force | Building this from scratch — no reason to replicate SUMO's weakest model. Social force is the industry standard (Helbing 1995, validated). |

---

## 13. Benchmark Targets {#13-benchmarks}

### Performance Comparison Goals

```
Scenario: 10×10 grid network, 30-minute simulation, Δt=0.1s

                    SUMO        CityFlow      VELOS (target)
                    (single     (multi-       (GPU +
                    thread)     thread, 16c)   multi-res)
─────────────────   ─────────   ──────────    ──────────────
10K vehicles        12s         0.8s          0.1s
50K vehicles        180s        8s            0.3s
100K vehicles       720s        25s           0.8s
500K vehicles       crash/OOM   180s          3s
1M vehicles         —           crash         8s

Real-time ratio @ 100K vehicles:
  SUMO:     0.25x (4x SLOWER than real-time)
  CityFlow: 1.2x  (barely real-time)
  VELOS:    37x   (faster than real-time)
```

### Memory Budget (Updated with Agent Intelligence + Prediction)

```
Component                     Per Agent    500K Agents
────────────────────────────  ──────────   ───────────
GPU agent buffer (60B struct) 60 bytes     30 MB       ← Updated: includes lane_idx, parity, sigma
GPU perception state          32 bytes     16 MB
GPU agent weights             32 bytes     16 MB
GPU cost evaluation           16 bytes     8 MB
GPU collision correction buf  8 bytes      4 MB        ← NEW (C1 fix)
GPU pedestrian force buffer   20 bytes     10 MB (50K peds)
GPU prediction overlay        16 bytes     1.6 MB (per 100K edges)
GPU edge geometry (for coord) —            ~5 MB (100K edges)  ← NEW (C5 fix)
CPU route storage (arena)     ~40 bytes    20 MB       ← Reduced: RouteArena vs Vec (m4)
CPU agent intelligence        ~128 bytes   64 MB
CPU gridlock state            ~12 bytes    6 MB        ← NEW (C3 fix)
CPU public transport routes   —            ~10 MB      ← NEW (M1)
CPU ECS overhead              ~64 bytes    32 MB
CPU staging ring buffer       —            ~1 MB       ← NEW (C4 fix)
Network graph + EdgeGeometry  —            ~60 MB (100K edges + geometry)
Spatial indices               —            ~30 MB
CH overlay graph              —            ~15 MB      ← NEW (M4)
Prediction history buffer     —            ~20 MB
Calibration state             —            ~5 MB       ← NEW (M5)
─────────────────────────────────────────────────────
Total                                      ~345 MB RAM + ~91 MB VRAM
```

---

## 14. Open Questions {#14-open-questions}

- **Determinism vs. GPU parallelism:** GPU floating-point operations are not always deterministic across hardware. Do we need bitwise reproducibility (use fixed-point on GPU) or is statistical equivalence sufficient?

- **SUMO ecosystem tools:** How much of SUMO's toolchain do we need to replicate? (netedit network editor, duarouter, activitygen, od2trips) — each is months of work. Prioritize which ones.

- **Indoor simulation scope:** Social force model works for open pedestrian areas. For building interiors with rooms, corridors, elevators — do we need a full navigation mesh system in v0.1, or can this be Phase 2?

- **Licensing strategy:** Open-source (Apache 2.0) to build community and compete with SUMO? Or proprietary core with open API? This affects whether universities and cities adopt it.

- **Cloud GPU pricing:** For customers without local GPUs — what's the cloud deployment cost? A single A100 GPU instance runs ~$3/hour. Is this acceptable for 24/7 real-time simulation?

- **Validation methodology:** Which reference scenarios do we use to validate against SUMO? Recommend: Cologne TAPAS scenario (public, well-documented, 700K trips).

- **TraCI compatibility depth:** Full TraCI protocol is hundreds of commands. Which subset is critical for existing SUMO users to migrate? Recommend: vehicle control, simulation step, edge queries, detector data (covers ~80% of usage).

- **Agent reroute frequency:** How often should agents re-evaluate routes? Every 50 sim-seconds (current design) may be too slow for fast-changing conditions or too fast for CPU budget. Need empirical testing.

- **Prediction model training data:** Does the customer have historical time-series data (edge volumes, travel times) for training ML models? If not, the built-in ensemble will be the only option initially.

- **V2I deployment reality:** How many intersections in the target city have DSRC/C-V2X infrastructure? If few/none, V2I simulation provides planning value but cannot be validated against real V2I data.

- **Braess's paradox mitigation:** How aggressively should agents reroute? Over-optimization leads to system instability (everyone avoids the same road). Need to define the "selfishness vs. system optimum" policy — user equilibrium or system optimum?

- **Prediction-informed routing ethics:** If prediction model is wrong and agents make worse decisions than no-prediction baseline, who is responsible? Need fallback policy and accuracy monitoring with auto-disable threshold.

---

## Appendix A: Key Reference Material

- [GPU-Accelerated Large-Scale Simulator for Transportation](https://arxiv.org/html/2406.10661v2) — CUDA-based traffic sim achieving 1M vehicles
- [LPSim: Multi-GPU Parallel Traffic Simulation](https://arxiv.org/html/2406.08496v1) — graph partitioning for distributed GPU sim
- [QarSUMO: Parallel Congestion-Optimized SUMO](https://arxiv.org/pdf/2010.03289) — 10x SUMO speedup via vehicle grouping
- [CityFlow: Multi-Agent RL Traffic Environment](https://cityflow.readthedocs.io/en/latest/) — multi-threaded reference architecture
- [Helbing Social Force Model (1995)](https://doi.org/10.1103/PhysRevE.51.4282) — pedestrian dynamics foundation
- [MOBIL Lane-Change Model](https://traffic-simulation.de/MOBIL.html) — incentive-based lane change
- [wgpu Compute Shader Examples](https://github.com/scttfrdmn/webgpu-compute-exploration) — GPU boids, SPH, molecular dynamics
- [bevy_gpu_compute](https://github.com/Sheldonfrith/bevy_gpu_compute) — ECS + GPU compute integration in Rust
- [hecs ECS Library](https://github.com/Ralith/hecs) — lightweight Rust ECS
- [Rust-GPU: GPU Shaders in Rust](https://github.com/EmbarkStudios/rust-gpu) — alternative to WGSL

---

*Document version: 2.0 (post-architecture-review) | VELOS Architecture Plan | March 2026*
*Changes from v1.0: Applied C1-C7 critical fixes, M1-M11 major fixes, m1-m8 minor fixes.*
*Timeline extended from 6→9 months. See velos-architecture-review.md for full issue log.*
