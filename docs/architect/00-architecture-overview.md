# VELOS v2 Architecture Overview

## POC Scope: Ho Chi Minh City Traffic Digital Twin

**Version:** 2.0 | **Date:** 2026-03-06 | **Author:** Tin Dang

---

## 1. Vision

VELOS v2 is a GPU-accelerated, distributed traffic microsimulation platform. This POC targets Ho Chi Minh City (HCMC) — a city of 10M+ people, 8M+ motorbikes, extreme mixed-traffic conditions, and limited signal infrastructure. HCMC's characteristics stress-test the simulator in ways Western-centric tools (SUMO, VISSIM) handle poorly: high-density motorbike swarms, informal lane discipline, and non-signalized intersections dominating the network.

## 2. POC Boundaries

### In Scope (POC)

| Feature | Target |
|---------|--------|
| **Area** | HCMC Districts 1, 3, 5, 10, Binh Thanh (~50 km road network) |
| **Agents** | Cars, motorbikes, buses, bicycles, pedestrians |
| **Agent count** | 200K motorbikes + 50K cars + 10K buses + 20K pedestrians = ~280K |
| **Car-following** | IDM only (validated, numerically stable) |
| **Pedestrian** | Social force with adaptive workgroup load balancing |
| **Routing** | Customizable Hierarchies (CCH) with dynamic edge weights |
| **Prediction** | Built-in ensemble (BPR + ETS + historical) only |
| **Signals** | Fixed-time (HCMC reality: most intersections are fixed-time or unsignalized) |
| **Visualization** | deck.gl 2D (primary) + CesiumJS 3D (secondary) |
| **Deployment** | Single-node multi-GPU (2-4 GPUs) with checkpoint/restart |
| **Calibration** | GEH + RMSE against HCMC traffic counts |
| **Demand** | Time-of-day profiles (morning/evening peak, off-peak, weekend) |

### Out of Scope (Deferred to v3)

| Feature | Reason |
|---------|--------|
| Multi-node distributed simulation | Not needed for 280K agents on 2-4 GPUs |
| W99 (Wiedemann 99) car-following | Requires PTV-calibrated datasets unavailable for HCMC |
| Cooperative intersection / V2I | HCMC has no connected vehicle infrastructure |
| ML prediction models (external) | Built-in ensemble sufficient for POC |
| CityGML 3D buildings | No CityGML dataset exists for HCMC |
| Autonomous vehicle models | Negligible AV presence in HCMC |
| SUMO TraCI compatibility | Not needed for POC |
| Multi-commodity transit passenger flow | Defer to post-POC bus route optimization phase |

## 3. Weakness Resolution Map

Every weakness from the architecture review is tracked to a specific resolution:

| # | Weakness | Severity | Resolution | Document |
|---|----------|----------|------------|----------|
| W1 | Single-GPU bottleneck | Critical | Multi-GPU spatial decomposition on single node | [01-simulation-engine.md](./01-simulation-engine.md) |
| W2 | EVEN/ODD dispatch unproven | Critical | Replace with deterministic wave-front dispatch + formal convergence proof | [01-simulation-engine.md](./01-simulation-engine.md) |
| W3 | No numerical stability verification | Critical | Formal CFL analysis + adaptive sub-stepping for high-speed edges | [01-simulation-engine.md](./01-simulation-engine.md) |
| W4 | Pedestrian model GPU-hostile | Major | Adaptive workgroup sizing + density-aware spatial hashing | [02-agent-models.md](./02-agent-models.md) |
| W5 | CH incompatible with dynamic weights | Major | Replace CH with Customizable Contraction Hierarchies (CCH) | [03-routing-prediction.md](./03-routing-prediction.md) |
| W6 | Arrow IPC latency trap | Major | Eliminate cross-process bridge; run prediction in-process via Rust-native ensemble | [03-routing-prediction.md](./03-routing-prediction.md) |
| W7 | 9-month timeline unrealistic | Major | 12-month timeline, 4 phases, HCMC-scoped deliverables | [07-timeline-risks.md](./07-timeline-risks.md) |
| W8 | No checkpoint/restart | Moderate | ECS snapshot to Parquet at configurable intervals | [06-infrastructure.md](./06-infrastructure.md) |
| W9 | Meso/micro transition discontinuities | Moderate | Velocity-matching insertion with 100m graduated buffer zone | [02-agent-models.md](./02-agent-models.md) |
| W10 | No public transport passenger model | Moderate | Simplified boarding/alighting model with empirical dwell times | [02-agent-models.md](./02-agent-models.md) |
| W11 | Self-hosted map stack ops burden | Moderate | PMTiles static hosting (single binary, no DB) + defer non-essential services | [06-infrastructure.md](./06-infrastructure.md) |
| W12 | No horizontal WebSocket scaling | Moderate | Redis pub/sub fan-out + stateless relay pods | [05-visualization-api.md](./05-visualization-api.md) |
| W13 | W99 unusable without PTV data | Minor | Drop W99 entirely from POC; IDM-only is sufficient and calibratable | [02-agent-models.md](./02-agent-models.md) |
| W14 | Deterministic simulation impossible cross-GPU | Minor | Fixed-point integer arithmetic for position/speed on GPU | [01-simulation-engine.md](./01-simulation-engine.md) |
| W15 | No time-of-day demand profiles | Minor | HCMC demand with 4 profiles: AM peak, PM peak, off-peak, weekend | [04-data-pipeline-hcmc.md](./04-data-pipeline-hcmc.md) |

## 4. Component Diagram

```
+------------------------------------------------------------------+
|                         VELOS v2 Platform                         |
+------------------------------------------------------------------+
|                                                                    |
|  +------------------+    +------------------+    +--------------+  |
|  |   velos-core     |    |   velos-gpu      |    | velos-net    |  |
|  |  ECS World       |    |  Multi-GPU Mgr   |    | Road Graph   |  |
|  |  Scheduler       |<-->|  Partition Mgr    |<-->| CCH Router   |  |
|  |  Checkpoint Mgr  |    |  Shader Registry  |    | Spatial Idx  |  |
|  |  Time Controller |    |  Buffer Pool      |    | OSM Import   |  |
|  +--------+---------+    +--------+---------+    +------+-------+  |
|           |                       |                      |         |
|  +--------v---------+    +--------v---------+    +------v-------+  |
|  |  velos-vehicle    |    | velos-pedestrian |    | velos-signal |  |
|  |  IDM Car-Follow   |    | Social Force     |    | Fixed-Time   |  |
|  |  MOBIL Lane-Chg   |    | Adaptive WG      |    | Actuated     |  |
|  |  Motorbike Model  |    | Crossing Logic   |    | Phase Ctrl   |  |
|  +-------------------+    +------------------+    +--------------+  |
|                                                                    |
|  +------------------+    +------------------+    +--------------+  |
|  |  velos-meso       |    | velos-predict    |    | velos-demand |  |
|  |  Queue Model      |    | BPR Ensemble     |    | OD Matrices  |  |
|  |  Graduated Buffer |    | ETS Correction   |    | ToD Profiles |  |
|  |  Velocity Match   |    | Historical Match |    | Sensor Calib |  |
|  +------------------+    +------------------+    +--------------+  |
|                                                                    |
|  +------------------+    +------------------+    +--------------+  |
|  |  velos-output     |    | velos-calibrate  |    | velos-scene  |  |
|  |  FCD / Parquet    |    | GEH Statistic    |    | Scenario DSL |  |
|  |  Edge Stats       |    | Bayesian Optim   |    | Batch Runner |  |
|  |  Emissions HBEFA  |    | RMSE Validation  |    | MOE Compare  |  |
|  +------------------+    +------------------+    +--------------+  |
|                                                                    |
|  +---------------------------------------------------------------+ |
|  |                     velos-api                                  | |
|  |  gRPC Server | WebSocket Relay (Redis fan-out) | REST Gateway | |
|  +---------------------------------------------------------------+ |
|                                                                    |
|  +---------------------------------------------------------------+ |
|  |                     velos-viz                                  | |
|  |  deck.gl Dashboard | CesiumJS 3D | Heatmaps | Playback Ctrl  | |
|  +---------------------------------------------------------------+ |
+------------------------------------------------------------------+
```

## 5. Data Flow

```
OSM PBF (HCMC)          Traffic Counts (HCMC DOT)     GPS Probe Data
      |                          |                          |
      v                          v                          v
  [velos-net]              [velos-demand]            [velos-calibrate]
  Road Graph +             OD Matrix +                GEH/RMSE
  CCH Index                ToD Profiles               Parameter Tuning
      |                          |                          |
      +----------+---------------+----------+---------------+
                 |                          |
                 v                          v
           [velos-core]              [velos-predict]
           ECS World +               Ensemble: BPR +
           Multi-GPU Scheduler       ETS + Historical
                 |                          |
                 v                          v
    +------------+------------+    +--------+--------+
    |            |            |    |                  |
[vehicle]  [pedestrian]  [signal]  | PredictionOverlay|
 IDM+MOBIL  Social Force  Fixed    | (ArcSwap)        |
    |            |            |    +------------------+
    +------------+------------+
                 |
                 v
          [velos-output]
           FCD + Edge Stats
           Emissions
                 |
        +--------+--------+
        |                 |
   [velos-api]      [velos-viz]
   gRPC/WS/REST     deck.gl/CesiumJS
```

## 6. Technology Stack (POC)

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Language | Rust 1.78+ | Memory safety, no GC, GPU interop |
| GPU Compute | wgpu + WGSL | Cross-platform WebGPU, multi-GPU support |
| ECS | hecs | Lightweight, SoA layout, direct GPU buffer mapping |
| CPU Parallel | rayon | Work-stealing, Send+Sync safety |
| Async Runtime | tokio | gRPC, WebSocket, sensor ingestion |
| Pathfinding | CCH (custom) | Dynamic weight updates without full re-contraction |
| Spatial Index | rstar (R-tree) | Nearest-edge queries |
| Math | glam (SIMD) | Fast vector math |
| Serialization | FlatBuffers | WebSocket binary protocol |
| Storage | Parquet (arrow-rs) | Checkpoint + output archival |
| API | tonic (gRPC) + axum (REST/WS) | Production-grade async |
| Viz (2D) | deck.gl + MapLibre | GPU-accelerated overlays, open-source |
| Viz (3D) | CesiumJS + self-hosted tiles | 3D city view |
| Map Tiles | PMTiles (static) | Zero-ops tile serving |
| Calibration | argmin | Bayesian optimization |
| Monitoring | Prometheus + Grafana | Metrics + dashboards |

## 7. Non-Functional Requirements

| NFR | Target | Measurement |
|-----|--------|-------------|
| Agent capacity | 280K (POC), extensible to 500K | Load test |
| Simulation rate | 10 steps/sec real-time (Dt=0.1s) | frame_time < 100ms |
| Faster-than-real-time | 20x for 100K agents | Batch benchmark |
| GPU VRAM | < 16GB for 280K agents (fits RTX 4090) | nvidia-smi |
| Startup time | < 10s (network load + agent spawn) | Wall clock |
| gRPC latency | < 5ms p99 | Prometheus histogram |
| WebSocket throughput | 100 concurrent viewers | Load test |
| Checkpoint interval | Configurable, default 5 min | Parquet snapshot |
| Checkpoint restore | < 30s for 280K agents | Wall clock |
| Calibration accuracy | GEH < 5 for 85%+ links | Validation report |
| Cross-GPU determinism | Bitwise identical via fixed-point | Comparison test |

## 8. Document Index

| Document | Content |
|----------|---------|
| [01-simulation-engine.md](./01-simulation-engine.md) | Core engine: multi-GPU, wave-front dispatch, numerical stability, fixed-point |
| [02-agent-models.md](./02-agent-models.md) | Vehicle, motorbike, pedestrian, bus, meso-micro transitions |
| [03-routing-prediction.md](./03-routing-prediction.md) | CCH routing, in-process prediction ensemble, demand integration |
| [04-data-pipeline-hcmc.md](./04-data-pipeline-hcmc.md) | HCMC-specific data: OSM, signals, traffic counts, GPS probes, ToD profiles |
| [05-visualization-api.md](./05-visualization-api.md) | deck.gl, CesiumJS, WebSocket scaling, gRPC API contracts |
| [06-infrastructure.md](./06-infrastructure.md) | Deployment, checkpoint/restart, simplified map stack, monitoring |
| [07-timeline-risks.md](./07-timeline-risks.md) | Full roadmap: 3 technical spikes, dependency DAG, 5 Go/No-Go gates, week-level plan, budget, v3/v4 vision |
