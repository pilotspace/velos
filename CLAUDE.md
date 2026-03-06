# Project Overview

VELOS is a GPU-accelerated traffic microsimulation platform targeting Ho Chi Minh City (HCMC). It simulates 280K agents (motorbikes, cars, buses, pedestrians) in real-time using Rust + wgpu GPU compute. The project is currently in the **architecture/design phase** — no source code exists yet.

Key differentiator: motorbike-native sublane model for Southeast Asian mixed traffic, where 80% of vehicles are motorbikes that don't follow lane discipline.

## Project Status

- **Phase:** Pre-development (architecture documents complete, awaiting engineering greenlight)
- **Target stack:** Rust workspace with ~12 crates, wgpu/WGSL GPU compute, hecs ECS, rayon CPU parallelism, tonic gRPC, axum REST/WebSocket, deck.gl visualization
- **POC scope:** HCMC Districts 1, 3, 5, 10, Binh Thanh — 280K agents on Mac Metal

## Architecture Documents

All architecture lives in `docs/architect/`. Read these before making any implementation decisions:

| Document | Purpose |
|----------|---------|
| `00-architecture-overview.md` | **Start here.** POC scope, weakness resolution map, component diagram, tech stack, NFRs |
| `01-simulation-engine.md` | Core engine: multi-GPU wave-front dispatch, numerical stability (CFL), fixed-point arithmetic, ECS layout, frame pipeline |
| `02-agent-models.md` | Vehicle types (motorbike sublane, car IDM, bus dwell), pedestrian social force with adaptive GPU workgroups, meso-micro transition |
| `03-routing-prediction.md` | CCH dynamic pathfinding, in-process BPR+ETS+historical prediction ensemble, reroute scheduling |
| `04-data-pipeline-hcmc.md` | HCMC-specific: OSM import rules, signal timing inference, demand profiles (ToD), calibration data sources |
| `05-visualization-api.md` | deck.gl layers, Redis pub/sub WebSocket scaling, gRPC/REST API contracts (protobuf), dashboard layout |
| `06-infrastructure.md` | Docker Compose deployment, ECS checkpoint to Parquet, PMTiles (zero-ops map tiles), monitoring (Prometheus/Grafana) |
| `07-timeline-risks.md` | Full roadmap: 3 technical spikes (Week 1-2), dependency DAG, 5 Go/No-Go gates, week-level plan, risk register |

Legacy architecture docs (v1, superseded by `docs/architect/`) are in `docs/` root. They provide context but the `docs/architect/` versions are authoritative.

Presentation slide decks (Marp format) are in `docs/architect/slides/`.

## Planned Crate Structure

When development begins, the Cargo workspace will have these crates:

- `velos-core` — ECS world (hecs), scheduler, checkpoint manager, time controller
- `velos-gpu` — wgpu device management, multi-GPU partitioning, buffer pools, WGSL shader registry
- `velos-net` — Road graph, OSM import, CCH pathfinding, spatial index (rstar)
- `velos-vehicle` — IDM car-following, MOBIL lane-change, motorbike sublane filtering
- `velos-pedestrian` — Social force model, adaptive workgroup spatial hashing
- `velos-signal` — Fixed-time and actuated signal controllers
- `velos-meso` — Mesoscopic queue model, graduated buffer zone transitions
- `velos-predict` — BPR + ETS + historical ensemble, ArcSwap overlay
- `velos-demand` — OD matrices, time-of-day profiles, agent spawning
- `velos-calibrate` — GEH statistic, Bayesian optimization (argmin), validation
- `velos-output` — FCD, edge stats, emissions (HBEFA), Parquet/CSV/GeoJSON export
- `velos-api` — tonic gRPC server, axum REST/WebSocket, Redis pub/sub relay
- `velos-scene` — Scenario DSL, batch runner, MOE comparison
- `velos-viz` — deck.gl dashboard (TypeScript/React), CesiumJS 3D (optional)

### Key Technical Decisions

- **GPU dispatch:** Per-lane wave-front (Gauss-Seidel), NOT EVEN/ODD. See `01-simulation-engine.md` Section 2.
- **Pathfinding:** CCH with dynamic weight customization (3ms update), NOT standard CH. See `03-routing-prediction.md` Section 1.
- **Prediction:** Rust-native in-process ensemble. NO Python bridge, NO Arrow IPC. See `03-routing-prediction.md` Section 2.
- **Car-following:** IDM only. W99 (Wiedemann) intentionally excluded. See `02-agent-models.md` Section 2.
- **Map tiles:** PMTiles static files served by Nginx. No Martin, no 3DCityDB, no Nominatim. See `06-infrastructure.md` Section 3.
- **Fixed-point:** Position uses Q16.16, speed uses Q12.20 for cross-GPU determinism. See `01-simulation-engine.md` Section 4.
- **Motorbike model:** Continuous lateral position (FixedQ8_8), NOT discrete lane-based. See `02-agent-models.md` Section 1.

### File Conventions

- Max 700 lines per file
- Each crate should have a clear single responsibility
- WGSL shaders live in `crates/velos-gpu/shaders/`
- Protobuf definitions in `proto/velos/v2/`
- HCMC-specific data configs in `data/hcmc/`
- Dashboard TypeScript code in `dashboard/` (pnpm workspace)
