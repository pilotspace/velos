# Technology Stack

**Analysis Date:** 2026-03-06

**Project Status:** Pre-development / architecture phase. No source code exists yet. All details below are derived from architecture documents in `docs/architect/`.

## Languages

**Primary:**
- Rust 1.78+ - Simulation engine, all backend crates (~12 planned crates)
- WGSL - GPU compute shaders for vehicle physics, pedestrian models (`crates/velos-gpu/shaders/`)

**Secondary:**
- TypeScript/React - Visualization dashboard (`dashboard/`)
- Protocol Buffers (proto3) - gRPC API definitions (`proto/velos/v2/`)

## Runtime

**Environment:**
- Rust stable (1.78+) with tokio async runtime for API/WebSocket
- Node.js (version TBD) for dashboard build
- NVIDIA GPU runtime (CUDA drivers required for wgpu on NVIDIA)

**Package Manager:**
- Cargo (Rust workspace) - primary
- pnpm (TypeScript dashboard) - per project conventions in `CLAUDE.md`
- Lockfile: Not yet created (pre-development)

## Frameworks

**Core:**
- wgpu - WebGPU-based GPU compute, multi-GPU adapter management
- hecs - Lightweight ECS (Entity Component System) with SoA layout for GPU buffer mapping
- rayon - CPU work-stealing parallelism for pathfinding, sorting, boundary transfers
- tokio - Async runtime for gRPC server, WebSocket relay, sensor ingestion

**API:**
- tonic - gRPC server (simulation control, streaming agent positions)
- axum - REST/WebSocket convenience API layer

**Visualization:**
- deck.gl - GPU-accelerated 2D map overlays (primary visualization)
- MapLibre GL JS - Open-source base map rendering with PMTiles
- CesiumJS - 3D city view (optional/secondary, for stakeholder demos)

**Testing:**
- Rust built-in `#[test]` + `cargo test`
- naga - WGSL shader validation (CI gate)
- cargo bench - Performance regression benchmarks

**Build/Dev:**
- Cargo workspace (12+ member crates)
- Docker Compose 3.9 - multi-container deployment
- NVIDIA Container Toolkit - GPU passthrough in Docker

## Key Dependencies

**Critical (core simulation):**
- `wgpu` - GPU compute dispatch, multi-adapter support, buffer management
- `hecs` - ECS world, component queries, entity spawning
- `rayon` - Parallel iterators for CCH queries, lane sorting
- `glam` (SIMD) - Fast vector math operations

**Pathfinding & Spatial:**
- CCH (custom implementation or `rust_road_router` from KIT Karlsruhe) - Customizable Contraction Hierarchies with 3ms weight customization
- `rstar` - R-tree spatial index for nearest-edge queries
- METIS (via FFI or Rust port) - Graph partitioning for multi-GPU spatial decomposition

**Serialization & Storage:**
- `flatbuffers` - WebSocket binary protocol for agent position frames (8 bytes/agent)
- `arrow-rs` / Parquet - Checkpoint snapshots, simulation output archival (Zstd L3 compression)
- `serde` + `serde_json` - Configuration, checkpoint metadata
- `quick-xml` - SUMO FCD XML export compatibility

**Infrastructure:**
- `tracing` - Structured logging with span instrumentation
- `chrono` - Timestamp handling for checkpoints
- `arc-swap` - Lock-free atomic pointer swap for prediction overlay updates
- `prometheus` (Rust client) - Metrics export (frame time, agent count, gridlock events)

**Data Pipeline:**
- `osmium` (CLI tool) - OSM PBF extract and clipping
- `tilemaker` (CLI tool) - OSM to MBTiles conversion
- `pmtiles` (CLI tool) - MBTiles to PMTiles conversion
- `gdal` / `rio` (CLI tools) - Terrain tile preparation from SRTM DEM

**Calibration:**
- `argmin` - Bayesian optimization for parameter tuning (IDM params, OD scaling)
- `ndarray` - Multi-dimensional arrays for historical speed patterns

**Dashboard (TypeScript):**
- `@deck.gl/*` - ScatterplotLayer, HeatmapLayer, PathLayer, IconLayer, ColumnLayer
- `maplibre-gl` - Base map renderer
- `pmtiles` - Client-side PMTiles protocol for static tile loading
- `cesium` (optional) - 3D globe rendering

## Configuration

**Environment:**
- Docker Compose environment variables for service discovery:
  - `NVIDIA_VISIBLE_DEVICES=all` (GPU access)
  - `VELOS_SIM_ADDR=velos-sim:50051` (gRPC endpoint)
  - `REDIS_URL=redis://redis:6379` (pub/sub relay)
  - `API_URL`, `WS_URL` (dashboard connections)
- No `.env` files detected in repository

**Build:**
- `Cargo.toml` workspace manifest (planned, not yet created)
- `tsconfig.json` for dashboard (planned)
- `config/prometheus.yml` - Prometheus scrape targets
- `config/grafana/` - Dashboard provisioning and JSON definitions
- `config/tilemaker.json` - OSM to vector tile conversion rules

**Data Directories (runtime):**
- `/data/network/` - Cleaned OSM road graph
- `/data/demand/` - OD matrices + time-of-day profiles
- `/data/tiles/` - PMTiles (vector + terrain), served by Nginx
- `/data/checkpoints/` - Parquet snapshots (rolling window of 10)
- `/data/output/` - Simulation results (FCD, edge stats, emissions)

## Platform Requirements

**Development:**
- 2x RTX 4090 24GB (or equivalent NVIDIA GPU with wgpu compute support)
- AMD EPYC 7543 16-core or Ryzen 9 7950X CPU
- 64 GB DDR5 RAM
- 1 TB NVMe SSD
- Rust toolchain 1.78+, Docker, NVIDIA drivers + Container Toolkit

**Production (POC):**
- Single server with 2-4 GPUs (same as dev, or cloud equivalent)
- Docker Compose deployment (not Kubernetes for POC)
- Cloud alternatives: AWS g5.12xlarge (4x A10G), GCP a2-highgpu-2g (2x A100), Lambda Labs gpu_2x_a100 (2x A100 at $2.20/hr)

**CI/CD (planned):**
- GPU-enabled CI runner for shader validation and benchmarks
- Lambda Labs cloud GPU ($400/mo budget for CI/CD)
- Quality gates per PR: `cargo clippy`, `cargo test`, `cargo bench`, `naga --validate`

## Cargo Workspace Structure (Planned)

```
velos/
├── Cargo.toml                    # Workspace manifest
├── crates/
│   ├── velos-core/               # ECS world, scheduler, checkpoint, time controller
│   ├── velos-gpu/                # wgpu device mgmt, multi-GPU, buffer pools, shader registry
│   │   └── shaders/*.wgsl        # WGSL compute shaders
│   ├── velos-net/                # Road graph, OSM import, CCH pathfinding, rstar spatial index
│   ├── velos-vehicle/            # IDM car-following, MOBIL lane-change, motorbike sublane
│   ├── velos-pedestrian/         # Social force model, adaptive workgroup spatial hashing
│   ├── velos-signal/             # Fixed-time and actuated signal controllers
│   ├── velos-meso/               # Mesoscopic queue model, graduated buffer transitions
│   ├── velos-predict/            # BPR + ETS + historical ensemble, ArcSwap overlay
│   ├── velos-demand/             # OD matrices, time-of-day profiles, agent spawning
│   ├── velos-calibrate/          # GEH statistic, Bayesian optimization (argmin)
│   ├── velos-output/             # FCD, edge stats, emissions (HBEFA), Parquet/CSV/GeoJSON
│   ├── velos-api/                # tonic gRPC, axum REST/WebSocket, Redis pub/sub relay
│   └── velos-scene/              # Scenario DSL, batch runner, MOE comparison
├── proto/velos/v2/               # Protobuf definitions
├── dashboard/                    # TypeScript/React deck.gl app (pnpm workspace)
├── docker/                       # Dockerfiles (simulation, api, dashboard)
├── config/                       # Prometheus, Grafana, tilemaker, Nginx configs
└── data/                         # Runtime data (network, demand, tiles, checkpoints)
```

## Key Technical Decisions

| Decision | Choice | Rationale | Reference |
|----------|--------|-----------|-----------|
| GPU dispatch | Per-lane wave-front (Gauss-Seidel) | Zero stale reads, zero collision risk | `docs/architect/01-simulation-engine.md` Section 2 |
| Pathfinding | CCH (not standard CH) | 3ms weight customization vs 30s rebuild | `docs/architect/03-routing-prediction.md` Section 1 |
| Prediction | Rust-native in-process | No Python bridge, no Arrow IPC latency | `docs/architect/03-routing-prediction.md` Section 2 |
| Car-following | IDM only (no W99) | Calibratable without PTV datasets | `docs/architect/02-agent-models.md` |
| Map tiles | PMTiles static files via Nginx | Zero additional services | `docs/architect/06-infrastructure.md` Section 3 |
| Fixed-point | Q16.16 position, Q12.20 speed | Cross-GPU bitwise determinism | `docs/architect/01-simulation-engine.md` Section 4 |
| Motorbike model | Continuous lateral (FixedQ8_8) | Not discrete lane-based | `docs/architect/02-agent-models.md` Section 1 |
| Deployment | Docker Compose (not K8s) | Sufficient for POC, simpler ops | `docs/architect/06-infrastructure.md` Section 1 |

---

*Stack analysis: 2026-03-06*
