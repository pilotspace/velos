# Requirements: VELOS v1.1 SUMO Replacement Engine

**Defined:** 2026-03-07
**Revised:** 2026-03-08 (added Phase 8 tuning requirements)
**Core Value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models

## v1.1 Requirements

Prove VELOS can replace SUMO: reimplement all core SUMO features, add agent intelligence and V2I that SUMO lacks, optimize for GPU-scale (280K+ agents). No web platform or data exports this milestone.

### GPU Engine & Scale

- [x] **GPU-01**: Simulation physics runs on GPU compute pipeline (not CPU) as the primary execution path
- [x] **GPU-02**: GPU spatial partitioning via METIS k-way graph partitioning across multiple adapters
- [x] **GPU-03**: Per-lane wave-front (Gauss-Seidel) dispatch replaces simple parallel dispatch
- [x] **GPU-04**: Fixed-point arithmetic (Q16.16 position, Q12.20 speed, Q8.8 lateral) for cross-GPU determinism
- [x] **GPU-05**: Boundary agent protocol (outbox/inbox staging buffers) for multi-GPU agent transfers
- [x] **GPU-06**: System sustains 280K agents at 10 steps/sec real-time on 2-4 GPUs

### Network & Compatibility

- [x] **NET-01**: 5-district HCMC road network imported from OSM (Districts 1, 3, 5, 10, Binh Thanh, ~25K edges)
- [x] **NET-02**: Network cleaning: merge short edges <5m, remove disconnected components, lane count inference
- [x] **NET-03**: HCMC-specific OSM rules: one-way streets, U-turn points, motorbike-only lanes
- [x] **NET-04**: Time-of-day demand profiles: weekday AM/PM peak, off-peak, weekend across 5 districts
- [x] **NET-05**: SUMO .net.xml network import for compatibility with existing SUMO models
- [x] **NET-06**: SUMO .rou.xml / .trips.xml demand file import

### Car-Following Models

- [x] **CFM-01**: Krauss car-following model (SUMO default) with safe-speed and dawdle behavior
- [x] **CFM-02**: Runtime-selectable car-following model per agent type (IDM or Krauss via ECS component)

### Agent Models

- [x] **AGT-01**: Bus agents with empirical dwell time model (5s + 0.5s/boarding + 0.67s/alighting, cap 60s)
- [x] **AGT-02**: GTFS import for 130 HCMC bus routes with stop locations and schedules
- [x] **AGT-03**: Bicycle agents with sublane model (rightmost position, IDM v0=15km/h)
- [x] **AGT-04**: Pedestrian adaptive GPU workgroups with prefix-sum compaction (3-8x speedup)
- [x] **AGT-05**: Meso-micro hybrid with 100m graduated buffer zone and velocity-matching insertion
- [x] **AGT-06**: Mesoscopic queue model (O(1) per edge) for peripheral network zones
- [x] **AGT-07**: Truck agent type with distinct dynamics (12m length, 1.0 m/s2 accel, 90 km/h max)
- [x] **AGT-08**: Emergency vehicle with priority behavior and yield-to-emergency from other agents

### Agent Intelligence

- [x] **INT-01**: Multi-factor pathfinding cost function: time, comfort, safety, fuel, signal delay, prediction penalty
- [x] **INT-02**: Configurable agent profiles (Commuter, Bus, Truck, Emergency, Tourist, Teen, Senior, Cyclist) with per-profile cost weights
- [x] **INT-03**: GPU perception phase: sense leader vehicle, signal state, traffic signs, nearby agents, global congestion map
- [x] **INT-04**: GPU evaluation phase: cost comparison current route vs alternative, output should_reroute flag + cost_delta
- [x] **INT-05**: Staggered reroute evaluation (1K agents/step, ~50s full cycle) with immediate triggers for blocked edges, emergency vehicles, and prediction flags

### Routing & Prediction

- [x] **RTE-01**: CCH (Customizable Contraction Hierarchies) replaces A* for pathfinding on 25K-edge network
- [x] **RTE-02**: CCH supports 3ms dynamic weight customization without full re-contraction
- [x] **RTE-03**: Dynamic agent rerouting at 500 reroutes/step using CCH queries (0.02ms/query)
- [x] **RTE-04**: BPR + ETS + historical prediction ensemble runs in-process every 60 sim-seconds
- [x] **RTE-05**: Prediction overlay uses ArcSwap for zero-copy, lock-free weight updates to CCH
- [x] **RTE-06**: Global network knowledge routing -- real-time congestion map (edge travel times) feeds into pathfinding cost function
- [x] **RTE-07**: Prediction-informed routing -- cost function uses predicted future travel times, not just current observed

### Signal Control & V2I

- [x] **SIG-01**: Actuated signal control with loop detector-triggered phase transitions
- [x] **SIG-02**: Adaptive signal control with demand-responsive timing optimization
- [x] **SIG-03**: SPaT (Signal Phase and Timing) broadcast to agents within range for signal-aware driving
- [x] **SIG-04**: Signal priority request from buses and emergency vehicles
- [x] **SIG-05**: Traffic sign interaction: speed limits, stop/yield, no-turn restrictions, school zones affect agent speed targets and cost function

### HCMC Behavior Tuning

- [x] **TUN-01**: All ~50 vehicle behavior parameters externalized to TOML config file (data/hcmc/vehicle_params.toml) with per-vehicle-type sections
- [x] **TUN-02**: GPU/CPU parameter parity -- GPU shader reads vehicle-type parameters from uniform buffer populated from config, eliminating hardcoded WGSL constants
- [x] **TUN-03**: HCMC-calibrated parameter defaults for all vehicle types (motorbike v0=35-45 km/h, car v0=30-40 km/h, truck v0=30-40 km/h not 90 km/h)
- [x] **TUN-04**: Red-light creep behavior -- motorbikes inch past stop line during red, forming dense swarm ahead of cars
- [x] **TUN-05**: Aggressive weaving -- speed-dependent lateral filter gap (0.5m base + 0.1*delta_v) for motorbike squeeze-through
- [x] **TUN-06**: Yield-based intersection negotiation -- vehicle-type-dependent TTC gap acceptance with size intimidation and deadlock prevention

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Web Platform & API

- **API-01**: gRPC server (tonic) for simulation control and state queries
- **API-02**: REST gateway (axum) for HTTP clients
- **API-03**: WebSocket real-time streaming with spatial tiling
- **API-04**: Redis pub/sub for multi-viewer scaling

### Visualization

- **VIZ-01**: deck.gl 2D web dashboard
- **VIZ-02**: CesiumJS 3D visualization
- **VIZ-03**: PMTiles static map tiles

### Data & Calibration

- **DAT-01**: Parquet checkpoints with rolling window
- **DAT-02**: FCD, edge stats, GeoJSON data exports
- **DAT-03**: GEH/RMSE calibration with Bayesian optimization
- **DAT-04**: HBEFA emissions modeling
- **DAT-05**: Scenario DSL, batch runner, MOE comparison

### Infrastructure

- **INF-01**: Docker Compose deployment
- **INF-02**: Prometheus/Grafana monitoring

### Advanced Models

- **ADV-01**: Autonomous vehicle agent models
- **ADV-02**: Full passenger flow model (multi-commodity OD, transfers, overcrowding)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Wiedemann 99 car-following | Requires PTV-calibrated datasets unavailable for HCMC. Uncalibrated W99 worse than calibrated IDM. |
| SUMO TraCI compatibility | Synchronous single-threaded protocol incompatible with GPU-parallel execution. ~200 command surface. Native file import (.net.xml, .rou.xml) provides migration path instead. |
| Real-time sensor data fusion | Requires data partnerships, streaming pipeline. Offline historical data sufficient for engine proof. |
| ML/DL prediction (PyTorch/TF) | Python sidecar latency, ops complexity. In-process BPR+ETS+historical ensemble sufficient. |
| Web visualization / API | Deferred to v2. Focus is engine proof, not platform. egui desktop retained for dev visualization. |
| Data exports / calibration | Deferred to v2. Minimal FCD output for internal validation only. |
| Docker / monitoring | Deferred to v2. Single-binary execution sufficient for engine proof. |
| Multi-node distributed sim | 280K agents fit on single-node 2-4 GPUs. |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| GPU-01 | Phase 5 | Complete |
| GPU-02 | Phase 5 | Complete |
| GPU-03 | Phase 5 | Complete |
| GPU-04 | Phase 5 | Complete (05-01) |
| GPU-05 | Phase 5 | Complete |
| GPU-06 | Phase 5 | Complete |
| NET-01 | Phase 5 | Complete |
| NET-02 | Phase 5 | Complete |
| NET-03 | Phase 5 | Complete |
| NET-04 | Phase 5 | Complete |
| NET-05 | Phase 5 | Complete |
| NET-06 | Phase 5 | Complete |
| CFM-01 | Phase 5 | Complete (05-01) |
| CFM-02 | Phase 5 | Complete (05-01) |
| AGT-01 | Phase 6 | Complete |
| AGT-02 | Phase 6 | Complete |
| AGT-03 | Phase 6 | Complete (06-01) |
| AGT-04 | Phase 6 | Complete |
| AGT-05 | Phase 6 | Complete |
| AGT-06 | Phase 6 | Complete |
| AGT-07 | Phase 6 | Complete (06-01) |
| AGT-08 | Phase 6 | Complete |
| SIG-01 | Phase 6 | Complete |
| SIG-02 | Phase 6 | Complete |
| SIG-03 | Phase 6 | Complete |
| SIG-04 | Phase 6 | Complete |
| SIG-05 | Phase 6 | Complete |
| INT-01 | Phase 7 | Complete |
| INT-02 | Phase 7 | Complete |
| INT-03 | Phase 7 | Complete |
| INT-04 | Phase 7 | Complete |
| INT-05 | Phase 7 | Complete |
| RTE-01 | Phase 7 | Complete |
| RTE-02 | Phase 7 | Complete |
| RTE-03 | Phase 7 | Complete |
| RTE-04 | Phase 7 | Complete |
| RTE-05 | Phase 7 | Complete |
| RTE-06 | Phase 7 | Complete |
| RTE-07 | Phase 7 | Complete |
| TUN-01 | Phase 8 | Planned |
| TUN-02 | Phase 8 | Planned |
| TUN-03 | Phase 8 | Planned |
| TUN-04 | Phase 8 | Planned |
| TUN-05 | Phase 8 | Planned |
| TUN-06 | Phase 8 | Planned |

**Coverage:**
- v1.1 requirements: 45 total
- Mapped to phases: 45
- Unmapped: 0

---
*Requirements defined: 2026-03-07*
*Last updated: 2026-03-08 after Phase 8 tuning requirements added*
