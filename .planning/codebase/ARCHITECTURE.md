# Architecture

**Analysis Date:** 2026-03-06

## Pattern Overview

**Overall:** GPU-accelerated ECS (Entity Component System) microsimulation with Cargo workspace monorepo (14 crates + 1 TypeScript dashboard)

**Key Characteristics:**
- Pre-development phase: no source code exists yet; architecture is fully specified in `docs/architect/`
- Single-node multi-GPU (2-4x RTX 4090) with METIS graph partitioning across GPUs
- ECS (hecs) with SoA layout for direct GPU buffer mapping via wgpu/WGSL compute shaders
- Deterministic wave-front (Gauss-Seidel) per-lane dispatch instead of EVEN/ODD parity
- Fixed-point integer arithmetic (Q16.16 position, Q12.20 speed) for cross-GPU bitwise determinism
- In-process Rust-native prediction ensemble (BPR + ETS + historical) with ArcSwap zero-lock overlay
- CCH (Customizable Contraction Hierarchies) for dynamic-weight pathfinding (3ms weight update vs 30s full CH rebuild)

## Layers

**Foundation Layer (velos-core):**
- Purpose: ECS world management, simulation scheduler, checkpoint/restore, time controller
- Location: `crates/velos-core/`
- Contains: hecs World, frame pipeline orchestration, checkpoint manager (Parquet snapshots), RNG state, gridlock detection
- Depends on: hecs, rayon
- Used by: Every other velos crate

**GPU Abstraction Layer (velos-gpu):**
- Purpose: Multi-GPU device management, buffer pools, WGSL shader registry, partition management
- Location: `crates/velos-gpu/`
- Contains: `GpuPartition` (per-GPU device/queue/buffers), `MultiGpuScheduler`, `AgentBufferPool` (double-buffered front/back with fence sync), METIS partition mapping, boundary agent inbox/outbox staging buffers
- Depends on: wgpu, velos-core
- Used by: velos-vehicle, velos-pedestrian

**Network Layer (velos-net):**
- Purpose: Road graph representation, OSM import, CCH pathfinding, spatial indexing
- Location: `crates/velos-net/`
- Contains: `RoadGraph`, `CCHRouter` (immutable node order + mutable weights via `customize()`), `NetworkImporter` (HCMC-specific OSM parsing rules), rstar R-tree spatial index
- Depends on: rstar, velos-core
- Used by: velos-vehicle, velos-signal, velos-meso, velos-predict, velos-demand

**Agent Simulation Layer:**
- Purpose: Per-agent-type physics models executed on GPU
- Location: `crates/velos-vehicle/`, `crates/velos-pedestrian/`
- Contains:
  - velos-vehicle: IDM car-following, MOBIL lane-change, motorbike sublane filtering (continuous lateral FixedQ8_8), bicycle behavior
  - velos-pedestrian: Social force model with adaptive workgroup sizing, prefix-sum compaction for non-empty spatial hash cells, density-aware cell sizing (2m/5m/10m)
- Depends on: velos-core, velos-gpu, velos-net (vehicle only)
- Used by: velos-core (frame pipeline)

**Signal Control Layer (velos-signal):**
- Purpose: Traffic signal controllers
- Location: `crates/velos-signal/`
- Contains: Fixed-time signal plans, actuated controllers, junction control types (`Signalized`, `PriorityRule`, `Uncontrolled`), default timing inference by junction leg count
- Depends on: velos-core, velos-net
- Used by: velos-core (frame pipeline)

**Mesoscopic Layer (velos-meso):**
- Purpose: Queue-based mesoscopic simulation for peripheral areas, with graduated buffer zone transition to microscopic
- Location: `crates/velos-meso/`
- Contains: Queue model, 100m graduated buffer zone with velocity-matching insertion, IDM parameter interpolation (relaxed at buffer entry, normal at exit)
- Depends on: velos-core, velos-net
- Used by: velos-core (frame pipeline)

**Prediction Layer (velos-predict):**
- Purpose: Edge travel time prediction for dynamic routing
- Location: `crates/velos-predict/`
- Contains: `PredictionEnsemble` (BPR w=0.40, ETS w=0.35, Historical w=0.25), `PredictionOverlay` with `Arc<ArcSwap>` for zero-lock atomic swap, `HistoricalMatcher` (3D array: edge x hour x day_type)
- Depends on: velos-core, velos-net, arc-swap
- Used by: velos-net (CCH weight customization every 60s)

**Demand Layer (velos-demand):**
- Purpose: Trip generation and agent spawning
- Location: `crates/velos-demand/`
- Contains: OD matrices, time-of-day profiles (weekday/weekend with 19+ time breakpoints), demand events (Tet, football), agent spawning scheduler
- Depends on: velos-core, velos-net
- Used by: velos-core (frame pipeline)

**Output Layer (velos-output):**
- Purpose: Simulation result export
- Location: `crates/velos-output/`
- Contains: FCD (floating car data), edge statistics, HBEFA emissions, export to Parquet/CSV/GeoJSON/SUMO XML
- Depends on: velos-core, arrow-rs
- Used by: velos-calibrate

**Calibration Layer (velos-calibrate):**
- Purpose: Parameter tuning against real-world traffic counts
- Location: `crates/velos-calibrate/`
- Contains: GEH statistic computation, Bayesian optimization via argmin crate, RMSE validation, calibration workflow (tune OD scaling, IDM params, signal offsets until GEH < 5 for 85%+ links)
- Depends on: velos-core, velos-output, argmin
- Used by: External calibration workflow

**API Layer (velos-api):**
- Purpose: External interface to simulation engine
- Location: `crates/velos-api/`
- Contains: tonic gRPC server (lifecycle, checkpoint, agent management, streaming, scenarios), axum REST gateway, WebSocket relay with Redis pub/sub spatial tile fan-out (500m x 500m tiles, FlatBuffers binary frames at 8 bytes/agent)
- Depends on: velos-core, tonic, axum, redis
- Used by: velos-scene, velos-viz

**Scenario Layer (velos-scene):**
- Purpose: Scenario definition and batch comparison
- Location: `crates/velos-scene/`
- Contains: Scenario DSL, batch runner, MOE (Measures of Effectiveness) comparison
- Depends on: velos-core, velos-api
- Used by: External scenario workflows

**Visualization Layer (velos-viz):**
- Purpose: Browser-based traffic visualization dashboard
- Location: `dashboard/` (TypeScript/React, separate pnpm workspace)
- Contains: deck.gl 2D dashboard (ScatterplotLayer for 280K agents, HeatmapLayer for density, PathLayer for bus routes), CesiumJS 3D (optional), MapLibre with PMTiles
- Depends on: N/A (separate from Rust workspace)
- Used by: End users via browser

## Data Flow

**Simulation Frame Pipeline (280K agents, 2 GPUs, ~8.2ms total):**

1. CPU: Partition boundary agent transfer (inbox/outbox staging buffers) - 0.1ms
2. CPU/rayon: Per-lane leader sort (parallel per GPU) - 1.5ms
3. GPU x2: Upload staging buffers - 0.3ms
4. GPU x2: Lane-change desire computation (parallel, MOBIL for cars / sublane filtering for motorbikes) - 1.0ms
5. GPU x2: Wave-front car-following IDM (sequential within lane, parallel across 50K lanes) - 2.0ms
6. GPU x2: Pedestrian social force (adaptive workgroups, prefix-sum compaction) - 1.5ms
7. CPU/rayon: CCH pathfinding (staggered ~500 reroutes/step, 0.02ms/query) - 0.5ms (parallel with GPU)
8. CPU/rayon: Route advance + edge transitions (CFL-bounded, adaptive sub-stepping for short edges) - 0.3ms
9. GPU->CPU: Download results - 0.3ms
10. CPU: Prediction ensemble update (if due, every 60 sim-seconds, async via tokio::spawn) - 0.2ms
11. CPU: Output recording + WebSocket broadcast (tile-based via Redis pub/sub) - 0.5ms

Budget: 100ms (10 steps/sec at Dt=0.1s). Headroom: ~92ms (11x margin).

**Data Ingestion Flow:**

1. OSM PBF -> `velos-net` NetworkImporter (HCMC-specific rules: one-way, motorbike lanes, U-turns) -> RoadGraph (~25K edges, ~15K junctions)
2. Traffic counts / GPS probes -> `velos-demand` OD matrix + ToD profiles -> `velos-calibrate` GEH/RMSE tuning
3. GTFS -> `velos-demand` BusRoute import (130 routes, headway + operating hours)
4. Signal timing (field survey + inference) -> `velos-signal` JunctionControl plans

**Prediction Update Flow:**

1. Every 60 sim-seconds: `PredictionEnsemble::update()` runs async on tokio
2. BPR physics + ETS error correction + historical pattern match -> weighted ensemble
3. New `PredictionOverlay` atomically swapped via `ArcSwap` (zero-lock)
4. CCH `customize()` called with new edge travel times (~3ms)
5. Subsequent agent reroute queries use updated weights immediately

**State Management:**
- ECS (hecs): All agent state as SoA component arrays
- GPU buffers: Double-buffered (`AgentBufferPool` with front/back swap + fence sync)
- Prediction: `Arc<ArcSwap<PredictionOverlay>>` for lock-free reads
- Checkpoints: Parquet snapshots of all ECS components + JSON metadata (sim_time, RNG, signal states)

## Key Abstractions

**GpuPartition:**
- Purpose: Encapsulates one GPU's slice of the simulation (device, queue, agent buffers, network subset, boundary staging)
- Examples: `GpuPartition` struct in `crates/velos-gpu/`
- Pattern: METIS k-way graph bisection assigns road segments to GPUs; boundary agents transfer via CPU-mediated inbox/outbox buffers (~64KB/step, negligible vs PCIe bandwidth)

**AgentBufferPool:**
- Purpose: Race-condition-free GPU buffer management via double buffering
- Examples: `AgentBufferPool` struct in `crates/velos-gpu/`
- Pattern: Front buffer read by GPU during dispatch, back buffer written by CPU; swap after fence wait

**CCHRouter:**
- Purpose: Fast dynamic-weight pathfinding
- Examples: `CCHRouter` in `crates/velos-net/`
- Pattern: Immutable node ordering + shortcut topology (built once at startup, ~30s), mutable edge weights updated via bottom-up `customize()` pass (~3ms). Bidirectional Dijkstra queries at ~0.02ms each.

**PredictionEnsemble:**
- Purpose: Edge travel time prediction without external Python process
- Examples: `PredictionEnsemble` in `crates/velos-predict/`
- Pattern: Three models (BPR, ETS, Historical) produce per-edge predictions; weighted mean + confidence stored in `PredictionOverlay`; atomic swap via `ArcSwap` for zero-lock consumption by routing layer

**ECS Component Layout:**
- Purpose: SoA GPU-friendly agent data
- Examples: `Position` (12 bytes), `Kinematics` (8 bytes), `IDMParams` (20 bytes), `LaneChangeState` (8 bytes), `LeaderIndex` (4 bytes) in `crates/velos-core/` and `crates/velos-vehicle/`
- Pattern: Each component type maps to a contiguous GPU buffer. Total ~52 bytes/agent. 280K agents = ~14.6 MB VRAM.

## Entry Points

**Simulation Engine:**
- Location: `crates/velos-core/` (main simulation loop)
- Triggers: gRPC `Start`/`Step`/`Resume` calls from `velos-api`
- Responsibilities: Orchestrates the 11-step frame pipeline, manages ECS world, coordinates multi-GPU dispatch

**gRPC Server:**
- Location: `crates/velos-api/` (tonic server on port 50051)
- Triggers: External clients (dashboard, CLI, notebooks)
- Responsibilities: Simulation lifecycle control, checkpoint management, agent CRUD, streaming (agent positions, edge stats), scenario management

**REST Gateway:**
- Location: `crates/velos-api/` (axum on port 8080)
- Triggers: HTTP clients (dashboards, Jupyter notebooks)
- Responsibilities: Convenience wrapper over gRPC for non-streaming use cases

**WebSocket Relay:**
- Location: `crates/velos-api/` (axum on port 8081)
- Triggers: Browser dashboard connections
- Responsibilities: Spatial-tile-based frame streaming via Redis pub/sub fan-out; FlatBuffers binary protocol at 10Hz

**Dashboard:**
- Location: `dashboard/` (deck.gl React app on port 3000)
- Triggers: Browser navigation
- Responsibilities: 2D visualization of 280K agents, heatmaps, flow arrows, signal states, KPI display, playback controls

## Error Handling

**Strategy:** Typed error enums per crate with gRPC error code mapping (13 defined error codes)

**Patterns:**
- gRPC: `VelosError` with `ErrorCode` enum (NETWORK_NOT_LOADED, SIMULATION_NOT_RUNNING, EDGE_NOT_FOUND, CAPACITY_EXCEEDED, GRIDLOCK_DETECTED, etc.) + string message + key-value details map
- Gridlock detection: Tarjan SCC on stalled-agent dependency graph; resolution via teleport, reroute, or signal override
- Numerical stability: CFL-bounded adaptive sub-stepping for short edges; edge transition guard clamps position at edge end if no capacity on next edge
- Data validation: `DataValidator` checks network connectivity, edge lengths, speed limits, demand totals, zone-to-network mapping before simulation start

## Cross-Cutting Concerns

**Logging:** `tracing` crate with structured fields (step, sim_time, agent_count, frame_time_ms, gpu_time_ms, reroute_count). `#[instrument]` attribute on key functions.

**Validation:** `DataValidator` struct validates network graph (connectivity, edge lengths, speed limits) and demand (total trips, zone mapping) before simulation. GEH statistic validates simulation output against real traffic counts.

**Authentication:** API key header for gRPC/REST (simple, POC-scope). All services on private Docker network; only ports 3000, 8080, 50051 exposed.

**Monitoring:** Prometheus metrics (`SimMetrics`: frame_time_ms histogram, agent_count gauge, gridlock_events counter, etc.) + Grafana dashboards. Alert rules for frame time p99 > 15ms, gridlock spikes, GPU VRAM > 85%.

**Determinism:** Fixed-point integer arithmetic (Q16.16 position, Q12.20 speed) in WGSL shaders with manual 64-bit emulation. `@invariant` attribute fallback if fixed-point performance is unacceptable (~20% overhead).

---

*Architecture analysis: 2026-03-06*
