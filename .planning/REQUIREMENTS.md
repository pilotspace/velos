# Requirements: VELOS v1.1 Digital Twin Platform

**Defined:** 2026-03-07
**Core Value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models

## v1.1 Requirements

Requirements for the full v2 architecture implementation. Each maps to roadmap phases.

### GPU Engine & Scale

- [ ] **GPU-01**: Simulation physics runs on GPU compute pipeline (not CPU) as the primary execution path
- [ ] **GPU-02**: GPU spatial partitioning via METIS k-way graph partitioning across multiple adapters
- [ ] **GPU-03**: Per-lane wave-front (Gauss-Seidel) dispatch replaces simple parallel dispatch
- [ ] **GPU-04**: Fixed-point arithmetic (Q16.16 position, Q12.20 speed, Q8.8 lateral) for cross-GPU determinism
- [ ] **GPU-05**: Boundary agent protocol (outbox/inbox staging buffers) for multi-GPU agent transfers
- [ ] **GPU-06**: System sustains 280K agents at 10 steps/sec real-time on 2-4 GPUs

### Network & Data

- [ ] **NET-01**: 5-district HCMC road network imported from OSM (Districts 1, 3, 5, 10, Binh Thanh, ~25K edges)
- [ ] **NET-02**: Network cleaning: merge short edges <5m, remove disconnected components, lane count inference
- [ ] **NET-03**: HCMC-specific OSM rules: one-way streets, U-turn points, motorbike-only lanes
- [ ] **NET-04**: Time-of-day demand profiles: weekday AM/PM peak, off-peak, weekend across 5 districts

### Routing & Prediction

- [ ] **RTE-01**: CCH (Customizable Contraction Hierarchies) replaces A* for pathfinding on 25K-edge network
- [ ] **RTE-02**: CCH supports 3ms dynamic weight customization without full re-contraction
- [ ] **RTE-03**: Dynamic agent rerouting at 500 reroutes/step using CCH queries (0.02ms/query)
- [ ] **RTE-04**: BPR + ETS + historical prediction ensemble runs in-process every 60 sim-seconds
- [ ] **RTE-05**: Prediction overlay uses ArcSwap for zero-copy, lock-free weight updates to CCH

### Agent Models

- [ ] **AGT-01**: Bus agents with empirical dwell time model (5s + 0.5s/boarding + 0.67s/alighting, cap 60s)
- [ ] **AGT-02**: GTFS import for 130 HCMC bus routes with stop locations and schedules
- [ ] **AGT-03**: Bicycle agents with sublane model (rightmost position, IDM v0=15km/h)
- [ ] **AGT-04**: Pedestrian adaptive GPU workgroups with prefix-sum compaction (3-8x speedup)
- [ ] **AGT-05**: Meso-micro hybrid with 100m graduated buffer zone and velocity-matching insertion
- [ ] **AGT-06**: Mesoscopic queue model (O(1) per edge) for peripheral network zones

### Web Platform & API

- [ ] **API-01**: gRPC server (tonic) with ~20 RPC methods for simulation control, state queries, scenario management
- [ ] **API-02**: REST gateway (axum) for HTTP clients with OpenAPI spec
- [ ] **API-03**: WebSocket real-time streaming at 10Hz with spatial tiling (500m cells) for viewport subscription
- [ ] **API-04**: FlatBuffers binary protocol for WebSocket agent position data (8 bytes/agent)
- [ ] **API-05**: Redis pub/sub fan-out for multi-viewer WebSocket scaling (100+ concurrent)

### Visualization

- [ ] **VIZ-01**: deck.gl 2D dashboard: ScatterplotLayer (vehicles), HeatmapLayer (density), PathLayer (routes), IconLayer (signals)
- [ ] **VIZ-02**: deck.gl renders 280K agents at 60 FPS using server-side binary attribute packing
- [ ] **VIZ-03**: CesiumJS 3D visualization with OSM building extrusions and terrain
- [ ] **VIZ-04**: PMTiles static map tiles served for base map layer (MapLibre GL JS)
- [ ] **VIZ-05**: React/TypeScript dashboard with simulation controls, metrics panels, layer toggles

### Data & Calibration

- [ ] **DAT-01**: ECS state checkpoint to Parquet (280K agents ~15MB compressed, rolling 10 checkpoints)
- [ ] **DAT-02**: Checkpoint restore in <30s for 280K agents
- [ ] **DAT-03**: FCD (Floating Car Data) export compatible with SUMO FCD format
- [ ] **DAT-04**: Edge statistics export (flow, density, speed per edge per interval) in Parquet/CSV
- [ ] **DAT-05**: GeoJSON export for GIS tools (QGIS, Mapbox)

### Calibration & Analysis

- [ ] **CAL-01**: GEH statistic implementation with target GEH < 5 for 85%+ links
- [ ] **CAL-02**: Bayesian optimization (argmin) for parameter tuning: OD scaling, IDM params, signal offsets
- [ ] **CAL-03**: HBEFA 5.1 emissions modeling: per-agent per-step CO2, NOx, PM by vehicle type and speed
- [ ] **SCN-01**: Scenario DSL (TOML/YAML): network mutations, demand variations, parameter overrides
- [ ] **SCN-02**: Batch runner with parallel execution across different seeds
- [ ] **SCN-03**: MOE comparison tables: throughput, mean delay, travel time index, queue length, LOS distribution

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Infrastructure

- **INF-01**: Docker Compose deployment (7 services: sim, api, viz, redis, tiles, prometheus, grafana)
- **INF-02**: Prometheus metric exposition with pre-built Grafana dashboards
- **INF-03**: Automated health checks and crash recovery

### Scale & Distribution

- **SCL-01**: Multi-node distributed simulation for 2M+ agents
- **SCL-02**: Horizontal WebSocket scaling beyond single Redis node

### Advanced Models

- **ADV-01**: Actuated signal control (demand-responsive)
- **ADV-02**: Autonomous vehicle agent models
- **ADV-03**: Full passenger flow model (multi-commodity OD, transfers, overcrowding)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Wiedemann 99 car-following | Requires PTV-calibrated datasets unavailable for HCMC. Uncalibrated W99 worse than calibrated IDM. |
| SUMO TraCI compatibility | Synchronous single-threaded protocol incompatible with GPU-parallel execution. ~200 command surface. |
| Real-time sensor data fusion | Requires data partnerships, streaming pipeline (Kafka), massive scope for marginal POC value. |
| ML/DL prediction (PyTorch/TF) | Python sidecar latency, ops complexity, no HCMC training data. In-process ensemble sufficient. |
| CityGML 3D buildings | No CityGML dataset exists for HCMC. OSM building extrusions provide 80% visual impact. |
| Plugin/extension system | Premature API stabilization prevents necessary architectural changes. Fork and modify instead. |
| Multi-node distributed sim | 280K agents fit on single-node 2-4 GPUs. Multi-node adds latency, sync complexity. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| (Updated by roadmapper) | | |

**Coverage:**
- v1.1 requirements: 40 total
- Mapped to phases: 0
- Unmapped: 40

---
*Requirements defined: 2026-03-07*
*Last updated: 2026-03-07 after initial definition*
