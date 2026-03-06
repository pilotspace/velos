# Feature Research

**Domain:** GPU-accelerated traffic microsimulation (motorbike-native, Southeast Asian mixed traffic)
**Researched:** 2026-03-06
**Confidence:** MEDIUM-HIGH (competitor features well-documented; SE Asian motorbike gap is confirmed by literature)

## Feature Landscape

### Table Stakes (Users Expect These)

Features that any traffic microsimulation tool must have. Missing these means the product is not a credible simulator.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Car-following model (IDM) | Core of microsimulation -- every vehicle needs longitudinal behavior | MEDIUM | VELOS uses IDM only, which is correct for POC. SUMO/VISSIM both offer IDM. W99 intentionally excluded (no PTV calibration data for HCMC). |
| Lane-changing model (MOBIL) | Vehicles must change lanes realistically | MEDIUM | Standard MOBIL with HCMC-tuned politeness (0.3, aggressive). SUMO uses LC2013/SL2015, VISSIM uses proprietary model. |
| Traffic signal control (fixed-time) | Intersections must have signals | LOW | HCMC reality: most intersections are fixed-time or unsignalized. Actuated signals deferred to later. |
| Network import (OSM) | Users expect to load real road networks | MEDIUM | All competitors support OSM. SUMO has netconvert, Aimsun has importers. VELOS needs HCMC-specific edge cases (alleys, one-ways). |
| Origin-Destination demand | Must define where trips start/end | MEDIUM | OD matrices with time-of-day profiles (AM peak, PM peak, off-peak, weekend). Standard in all competitors. |
| Dynamic routing / pathfinding | Agents must find routes and respond to congestion | HIGH | VELOS uses CCH with dynamic weight updates. SUMO uses Dijkstra/A*/CH. Aimsun has DTA. This is table stakes but VELOS's CCH approach is technically superior. |
| Simulation playback controls | Start, stop, pause, speed up, step-through | LOW | Every simulator has this. Tauri IPC handles it for VELOS desktop app. |
| Measures of Effectiveness (MOE) output | Travel time, delay, queue length, throughput, LOS | MEDIUM | FHWA guidelines define standard MOEs. Users compare simulation output against field data. Must output at minimum: link travel times, intersection delay, queue lengths. |
| Calibration against field data | GEH statistic, RMSE against traffic counts | MEDIUM | GEH < 5 for 85%+ links is the industry standard (FHWA). Without calibration, the model is an animation, not a simulation. |
| Deterministic simulation | Same inputs must produce same outputs | MEDIUM | Required for reproducibility and debugging. VELOS achieves this via fixed-point arithmetic (Q16.16/Q12.20). SUMO is deterministic by default. |
| Multi-modal agents | Cars, motorcycles, buses, pedestrians, bicycles | HIGH | All major competitors support multiple vehicle types. VELOS targets 4 vehicle types + pedestrians. |
| Pedestrian simulation | Social force or similar model | MEDIUM | SUMO has pedestrian model, VISSIM has PTV Viswalk, Aimsun has pedestrian module. VELOS uses social force with adaptive GPU workgroups. |
| Data export (FCD, statistics) | Floating Car Data, link/turn statistics, CSV/Parquet | MEDIUM | Standard output format. SUMO exports FCD XML, Aimsun exports CSV. VELOS plans Parquet (columnar, efficient). Deferred to post-MVP but must exist eventually. |
| Visualization / animation | 2D view of vehicles moving on network | MEDIUM | Every simulator has this. VELOS uses native wgpu rendering (desktop) with future deck.gl (web). |

### Differentiators (Competitive Advantage)

Features that set VELOS apart from SUMO, VISSIM, Aimsun, and MATSim.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Continuous sublane motorbike model** | SUMO's sublane model discretizes lanes into sublanes (e.g., 0.8m resolution). VISSIM requires External Driver Model hacks for motorcycle filtering. VELOS uses truly continuous lateral positioning (FixedQ8_8) -- motorbikes occupy any lateral position, not discrete sublane slots. This is the single most important differentiator. | HIGH | No competitor handles motorbike swarm behavior (gap filtering, red-light clustering) natively. SUMO's SL2015 model is the closest but still grid-based. |
| **GPU-accelerated compute** | SUMO, VISSIM, Aimsun are all CPU-based. VELOS runs agent updates on GPU via wgpu/WGSL compute shaders. Recent research (2024) shows GPU simulators achieving 88x speedup over CPU equivalents. | HIGH | Enables real-time simulation of 280K agents. CPU-based SUMO struggles above 50K agents in real-time. This unlocks interactive "what-if" analysis at city scale. |
| **Motorbike-pedestrian interaction** | HCMC pedestrians navigate through slow-moving motorbike streams. Jaywalking probability 0.3 (vs near-zero in Western models). No competitor models this interaction natively. | MEDIUM | Builds on social force model + sublane model. Critical for HCMC realism where sidewalk/road boundary is fluid. |
| **HCMC-calibrated defaults** | Pre-tuned IDM parameters for HCMC traffic (motorbike v0=40km/h, s0=1.0m, T=0.8s). Competitors ship with European/US defaults that produce garbage for SE Asian cities. | LOW | Low implementation cost, high user value. Eliminates the #1 pain point researchers report when using SUMO/VISSIM for SE Asian traffic. |
| **Meso-micro hybrid with graduated buffer** | Aimsun has meso-micro hybrid but transition causes artificial waves. VELOS's 100m buffer zone with velocity interpolation and IDM parameter relaxation eliminates phantom congestion at zone boundaries. | HIGH | Enables large networks (meso for far zones, micro for study area) without boundary artifacts. |
| **Fixed-point cross-GPU determinism** | Q16.16 positions, Q12.20 speeds ensure bitwise-identical results across different GPUs. No competitor offers cross-hardware deterministic simulation. | MEDIUM | Unique selling point for reproducible research. Academic users care deeply about this. |
| **In-process prediction ensemble** | BPR + ETS + historical prediction runs in-process (Rust-native). No Python bridge, no Arrow IPC overhead. SUMO's prediction requires external tools. VISSIM has no built-in prediction. | MEDIUM | Enables real-time rerouting response. Prediction updates in <3ms vs 100ms+ for cross-process approaches. |
| **Real-time interactive what-if** | GPU speed enables changing signal timing, adding road closures, modifying demand in real-time and seeing results immediately. CPU simulators require batch runs. | MEDIUM | Transforms workflow from "run overnight, analyze tomorrow" to "try it now, see it now." |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem valuable but create problems for VELOS specifically.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Wiedemann 99 car-following** | "Industry standard" in VISSIM, researchers ask for it | W99 has 10 calibration parameters (CC0-CC9) requiring PTV-calibrated datasets that don't exist for HCMC. Including it creates false expectations -- users select it, get uncalibrated garbage, blame VELOS. | IDM with 5 physically interpretable parameters, calibratable from basic traffic counts via Bayesian optimization. |
| **SUMO TraCI compatibility** | "I want to use my existing SUMO scripts" | Maintaining API compatibility with a moving target (TraCI evolves with SUMO releases) is a massive ongoing burden. TraCI's TCP socket architecture conflicts with VELOS's GPU-first design. | Native gRPC + REST API with clear contracts. Provide a thin Python client library. Migration guide for common TraCI patterns. |
| **Full activity-based demand (MATSim-style)** | "I want agents with daily activity chains" | MATSim's co-evolutionary approach requires hundreds of iterations to converge. This conflicts with VELOS's real-time interactive model. Activity-based demand is a different product category (strategic planning vs operational microsimulation). | OD matrices with time-of-day profiles for microsimulation. If strategic modeling needed, use MATSim for demand generation and feed OD matrices into VELOS. |
| **3D visualization (CesiumJS/Unreal)** | "I want photorealistic 3D rendering" | 3D rendering consumes GPU resources that should be used for simulation compute. Photorealistic rendering is a separate engineering discipline. No CityGML dataset exists for HCMC anyway. | 2D top-down wgpu rendering for desktop. deck.gl for web dashboard (future). Focus GPU budget on simulation, not rendering. |
| **Connected/Autonomous Vehicle (CAV) models** | "Every new simulator should support CAVs" | HCMC has negligible AV presence. Building CAV models diverts resources from the motorbike model, which is the actual differentiator. | Defer to v3+. When HCMC has CAVs, add them. Don't build features for a market that doesn't exist in the target city. |
| **Multi-node distributed simulation** | "What about million-agent scenarios?" | 280K agents fit on a single node with 2-4 GPUs. Distributed simulation introduces clock synchronization, network partitioning, and ghost zone complexity that would delay the POC by months. | Single-node multi-GPU first. Prove the simulation model works before distributing it. |
| **Plugin/extension system** | "Let users write custom models" | Plugin APIs create backward compatibility obligations. Every internal refactor must preserve plugin contracts. Premature API stabilization prevents necessary architectural changes during POC. | Provide source code (it's a research tool). Users fork and modify. Stabilize APIs after v2 architecture settles. |
| **Real-time sensor data ingestion** | "Feed live GPS/loop detector data" | Real-time ingestion requires streaming infrastructure (Kafka, etc.), data quality handling, and clock synchronization -- all orthogonal to the core simulation engine. | Batch import of historical sensor data for calibration. Real-time digital twin is a v3+ goal after the simulation model is validated. |

## Feature Dependencies

```
Road Network (OSM Import)
    +-- Spatial Index (R-tree)
    |       +-- Neighbor Queries (car-following, lane-change, motorbike filtering)
    +-- Pathfinding (CCH)
    |       +-- Dynamic Routing
    |               +-- Prediction Ensemble (BPR+ETS)
    +-- Signal Control (Fixed-time)

ECS World (hecs)
    +-- Agent Spawn (OD + ToD)
    +-- GPU Buffer Mapping
            +-- Car-Following (IDM) [GPU compute]
            +-- Lane-Change (MOBIL) [GPU compute]
            +-- Motorbike Sublane Model [GPU compute]
            +-- Pedestrian Social Force [GPU compute]

Motorbike Sublane Model
    +-- requires: Continuous Lateral Position (Q8.8)
    +-- requires: Neighbor Queries (lateral gap detection)
    +-- enhances: Motorbike-Pedestrian Interaction

Calibration (GEH/RMSE)
    +-- requires: MOE Output (travel times, counts)
    +-- requires: Field Data Import
    +-- requires: Bayesian Optimization (argmin)

Meso-Micro Hybrid
    +-- requires: Micro model (IDM+MOBIL) working first
    +-- requires: Meso queue model
    +-- requires: Buffer zone velocity interpolation

Visualization
    +-- requires: Agent position data (from ECS)
    +-- independent of: Simulation correctness (can render wrong results)
```

### Dependency Notes

- **Motorbike sublane requires spatial index:** Lateral gap detection for filtering needs fast neighbor queries. R-tree must work before sublane model can be tested.
- **Calibration requires MOE output:** Cannot validate the model without measurable outputs. Build output first, then calibration.
- **Meso-micro requires working micro:** The micro model (IDM+MOBIL+sublane) must be correct before adding meso zones, otherwise you're interpolating toward a broken model.
- **Prediction enhances routing but is not required for MVP:** Static shortest-path routing works for initial validation. Prediction adds realism for congested scenarios.

## MVP Definition

### Launch With (v1)

Minimum viable product -- enough to demonstrate the motorbike sublane model works on real HCMC road geometry.

- [x] GPU compute pipeline (wgpu/WGSL) dispatching agent updates -- proves the architecture
- [x] Road network from OSM (small HCMC area) -- real geometry, not toy networks
- [x] IDM car-following for all vehicle types -- longitudinal behavior
- [x] MOBIL lane-change for cars -- lateral behavior (discrete lanes)
- [x] Motorbike sublane model with continuous lateral position -- THE differentiator
- [x] Fixed-time signal control at intersections -- minimum intersection behavior
- [x] OD-based demand with time-of-day profiles -- agents need origins/destinations
- [x] Static shortest-path routing (CCH without dynamic weights) -- agents need routes
- [x] Native wgpu 2D visualization -- see it working
- [x] Tauri desktop shell with start/stop/speed controls -- interactive use
- [x] Deterministic simulation via fixed-point arithmetic -- reproducibility

### Add After Validation (v1.x)

Features to add once the core simulation loop is validated against visual inspection and basic traffic counts.

- [ ] Pedestrian social force model -- adds realism but not required for vehicle validation
- [ ] Dynamic CCH weight updates + prediction ensemble -- enables congestion response
- [ ] Bus dwell time model -- adds public transport realism
- [ ] GEH calibration against HCMC traffic counts -- formal validation
- [ ] MOE output (link travel times, intersection delay, queue lengths) -- quantitative analysis
- [ ] Meso-micro hybrid with graduated buffer -- enables larger study areas
- [ ] Checkpoint/restart (Parquet snapshots) -- long simulation runs

### Future Consideration (v2+)

Features to defer until the simulation model is validated and the architecture is stable.

- [ ] FCD/Parquet/GeoJSON data export -- needs stable output format
- [ ] Emissions model (HBEFA) -- bolt-on after vehicle dynamics are correct
- [ ] Scenario DSL / batch runner -- needed for systematic analysis, not exploration
- [ ] Web dashboard (deck.gl + REST API) -- multi-user/remote access
- [ ] Actuated signal control -- adaptive signals for what-if analysis
- [ ] Multi-GPU partitioning -- scaling to 280K agents
- [ ] Bayesian parameter optimization (argmin) -- automated calibration

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Motorbike sublane model | HIGH | HIGH | P1 |
| GPU compute pipeline | HIGH | HIGH | P1 |
| IDM car-following | HIGH | MEDIUM | P1 |
| OSM road network import | HIGH | MEDIUM | P1 |
| MOBIL lane-change | MEDIUM | MEDIUM | P1 |
| Fixed-time signals | MEDIUM | LOW | P1 |
| OD demand + ToD profiles | MEDIUM | MEDIUM | P1 |
| CCH pathfinding (static) | MEDIUM | HIGH | P1 |
| Native wgpu visualization | MEDIUM | MEDIUM | P1 |
| Fixed-point determinism | MEDIUM | MEDIUM | P1 |
| Pedestrian social force | MEDIUM | MEDIUM | P2 |
| Dynamic routing + prediction | HIGH | HIGH | P2 |
| GEH calibration | HIGH | MEDIUM | P2 |
| MOE output | HIGH | MEDIUM | P2 |
| Meso-micro hybrid | MEDIUM | HIGH | P2 |
| Bus dwell model | LOW | LOW | P2 |
| Checkpoint/restart | MEDIUM | MEDIUM | P2 |
| Data export (FCD/Parquet) | MEDIUM | MEDIUM | P3 |
| Emissions (HBEFA) | LOW | MEDIUM | P3 |
| Scenario DSL | MEDIUM | HIGH | P3 |
| Web dashboard (deck.gl) | MEDIUM | HIGH | P3 |
| Multi-GPU partitioning | HIGH | HIGH | P3 |

**Priority key:**
- P1: Must have for launch -- proves the motorbike-native GPU-accelerated concept
- P2: Should have -- enables formal validation and larger study areas
- P3: Nice to have -- scaling, export, and multi-user features

## Competitor Feature Analysis

| Feature | SUMO | VISSIM | Aimsun | MATSim | VELOS |
|---------|------|--------|--------|--------|-------|
| **Car-following** | Krauss (default), IDM | Wiedemann 74/99 | Gipps, IDM | Queue-based (not micro) | IDM only (calibratable for HCMC) |
| **Lane-change** | LC2013, SL2015 | Proprietary | Proprietary | N/A (meso) | MOBIL (cars), sublane filtering (motorbikes) |
| **Motorbike model** | Sublane (discretized, 0.8m resolution) | External Driver Model hack required | No native support | N/A | Continuous lateral position (Q8.8), native filtering+swarm |
| **GPU acceleration** | No (CPU only) | No (CPU only) | No (CPU only) | No (CPU only, Java) | Yes (wgpu/WGSL compute shaders) |
| **Pedestrians** | Built-in (basic) | PTV Viswalk (separate license) | Built-in module | Via contrib | Social force with adaptive GPU workgroups |
| **Meso-micro hybrid** | SUMO-meso (separate mode) | No | Yes (integrated) | Yes (primary mode) | Graduated buffer zone (100m velocity interpolation) |
| **Routing** | Dijkstra, A*, CH | Proprietary DTA | Dynamic Traffic Assignment | Iterative DTA | CCH with dynamic weight customization |
| **Signal control** | Fixed, actuated, NEMA, TraCI | Fixed, actuated, adaptive, VAP | Fixed, actuated, adaptive | External | Fixed-time (POC), actuated (later) |
| **Calibration** | Manual + external tools | Manual + Optima | Built-in calibration module | Cadyts contrib | GEH + Bayesian optimization (planned) |
| **External API** | TraCI (TCP socket) | COM interface | Python/C++ SDK | Java API | gRPC + REST (planned) |
| **Determinism** | Yes (default) | Stochastic (seed-based) | Stochastic (seed-based) | Yes | Yes (fixed-point arithmetic, cross-GPU) |
| **Scale (real-time)** | ~50K agents | ~20-50K agents | ~100K agents (meso) | ~1M agents (meso, not real-time) | Target: 280K agents (GPU) |
| **License** | Open source (EPL-2.0) | Commercial ($$$) | Commercial ($$$) | Open source (AGPL) | Open source (planned) |
| **SE Asian traffic** | Possible with sublane hacks | Possible with EDM hacks | Poor native support | Not designed for micro | Native first-class support |

### Key Competitive Gaps VELOS Fills

1. **No competitor has native motorbike-dominant mixed traffic support.** SUMO's sublane is the closest but requires discretization and manual tuning. VISSIM requires External Driver Model development. Aimsun has no motorcycle model. Researchers working on SE Asian traffic consistently report struggling with all existing tools.

2. **No competitor uses GPU compute.** All are CPU-bound, limiting real-time scale to 20-100K agents. VELOS targets 280K agents in real-time via GPU compute shaders.

3. **No competitor offers cross-GPU deterministic simulation.** Fixed-point arithmetic is unique to VELOS and valuable for reproducible research.

## Sources

- [SUMO Documentation - Sublane Model](https://sumo.dlr.de/docs/Simulation/SublaneModel.html) -- SUMO's discretized sublane approach
- [SUMO at a Glance](https://sumo.dlr.de/docs/SUMO_at_a_Glance.html) -- SUMO feature overview
- [PTV VISSIM Product Page](https://www.ptvgroup.com/en-us/products/ptv-vissim) -- VISSIM capabilities
- [Aimsun Next Editions](https://www.aimsun.com/editions/) -- Aimsun feature tiers
- [MATSim.org](https://www.matsim.org/) -- MATSim capabilities
- [VISSIM Motorcycle Simulation for HCMC](https://www.researchgate.net/publication/353345030_Application_of_VISSIM_microsimulation_model_for_the_motorcycle_traffic_in_Ho_Chi_Minh_City) -- confirms VISSIM struggles with HCMC motorbikes
- [GPU-accelerated Traffic Simulation (2024)](https://arxiv.org/html/2406.10661v1) -- 88x speedup with GPU microsimulation
- [FHWA Traffic Microsimulation Guidelines](https://ops.fhwa.dot.gov/trafficanalysistools/tat_vol3/sect5.htm) -- calibration/validation standards, GEH criteria
- [SUMO TraCI Documentation](https://sumo.dlr.de/docs/TraCI.html) -- TraCI API features
- [VISSIM motorcycle in Chiang Mai](https://www.mdpi.com/2412-3811/10/4/97) -- External Driver Model for motorcycle filtering

---
*Feature research for: GPU-accelerated traffic microsimulation (motorbike-native, Southeast Asian mixed traffic)*
*Researched: 2026-03-06*
