# VELOS v2 Roadmap

## 12-Month POC + Strategic Vision

**Version:** 2.0 | **Date:** 2026-03-06 | **Author:** Tin Dang

---

## 1. Strategic Vision: Three Horizons

```
Year 1 (This Roadmap)          Year 2                      Year 3
v2 POC                         v3 Production               v4 Platform
─────────────────────          ─────────────────────       ─────────────────────
HCMC 5 Districts               Full HCMC Metro Area        Multi-City
280K agents                    2M agents                   10M agents
2 GPUs, single node            8-16 GPUs, multi-node       Cloud auto-scale
deck.gl 2D                     CesiumJS 3D + deck.gl       SaaS Dashboard
Built-in prediction            ML prediction (gRPC)        Real-time Digital Twin
GEH-calibrated                 Live sensor integration     TMC Integration
3 demo scenarios               Operational scenarios       Policy decision tool
4 engineers                    8 engineers                 12 engineers
~$200K budget                  ~$500K budget               ~$800K budget
```

This document covers **Year 1 (v2 POC)** in detail, with a strategic outline for Years 2-3.

---

## 2. Team Composition

| Role | Person | Skills | Start | Allocation |
|------|--------|--------|-------|------------|
| **E1: Engine Lead** | TBD | Rust, GPU compute (wgpu/WGSL), ECS, parallelism | Month 1 | Full-time (12 months) |
| **E2: Network & Routing** | TBD | Rust, graph algorithms, OSM, pathfinding, spatial indexing | Month 1 | Full-time (12 months) |
| **E3: API & Visualization** | TBD | TypeScript, React, deck.gl, Rust, gRPC, WebSocket, Redis | Month 1 | Full-time (12 months) |
| **E4: Calibration & Data** | TBD | Traffic engineering, statistics, Python (data analysis), Rust basics | Month 3 | Full-time (10 months) |

**Why 4, not 5?** The v1 plan had E5 joining at Month 3 to own agent intelligence, prediction, V2I, and scenario management — 40% of the system. This was a staffing bottleneck disguised as a plan. In v2, we distribute those responsibilities across E1/E2 and eliminate V2I/ML from POC scope entirely.

**Why E4 starts Month 3?** The simulation engine and network must exist before calibration work starts. E4's first 2 months would be idle waiting. Instead, E2 begins HCMC data collection in Month 2, and E4 inherits that work on arrival.

---

## 3. Technical Spikes (Week 1-2)

Three experiments that **must** run before any committed development. Results determine architecture decisions.

### Spike S1: Wave-Front GPU Dispatch Benchmark

**Owner:** E1 | **Duration:** 3 days | **Decision deadline:** Day 10

**Hypothesis:** Per-lane wave-front (Gauss-Seidel) dispatch on GPU achieves sufficient throughput despite sequential-within-lane processing.

**Experiment:**
1. Create standalone WGSL compute shader (no VELOS code yet)
2. Synthetic data: 50K agents on 10K lanes (avg 5 agents/lane, max 30)
3. Implement wave-front: each workgroup processes one lane sequentially
4. Implement comparison: naive parallel dispatch (all agents simultaneously)
5. Measure wall-clock time per dispatch on RTX 4090

**GO criteria:** Wave-front achieves > 40% of naive parallel throughput.
At 50K agents, naive parallel ≈ 0.3ms. Wave-front GO if < 0.75ms.

**NO-GO fallback:** Revert to EVEN/ODD dispatch with iterative correction:
- Pass 1: EVEN agents (parallel)
- Pass 2: ODD agents (parallel)
- Pass 3: Collision correction (parallel, check all gaps)
- Pass 4: Re-correction if Pass 3 created new violations (max 2 iterations)
- Accept residual error ≤ 5cm (documented, monitored at runtime)

**Why this spike matters:** Wave-front is architecturally clean (zero stale reads, zero collisions) but has unknown GPU utilization characteristics. If occupancy is too low, the entire engine design changes.

### Spike S2: wgpu Multi-GPU Feasibility

**Owner:** E1 | **Duration:** 1 day | **Decision deadline:** Day 10

**Experiment:**
1. Enumerate all GPU adapters via `wgpu::Instance::enumerate_adapters()`
2. Create a compute pipeline on each adapter
3. Dispatch a trivial shader (increment array) on each GPU
4. Transfer 64KB buffer: GPU0 → CPU → GPU1, measure latency
5. Transfer 1MB buffer, measure throughput

**GO criteria:**
- Both GPUs addressable from single process: YES/NO
- 64KB transfer latency < 0.1ms
- 1MB transfer throughput > 10 GB/s

**NO-GO fallback:** Single-GPU architecture. Reduce POC agent count to 200K. Still viable for HCMC demonstration — just less impressive at peak.

**Why this spike matters:** wgpu's multi-adapter support is not well-documented for compute workloads. If it doesn't work, we avoid 4 weeks of wasted multi-GPU development.

### Spike S3: CCH Library Evaluation

**Owner:** E2 | **Duration:** 2 days | **Decision deadline:** Day 10

**Experiment:**
1. Evaluate `rust_road_router` (KIT Karlsruhe's CCH in Rust) vs. building from scratch
2. Import HCMC OSM extract as test network (~25K edges)
3. Build CCH ordering + shortcuts
4. Run 1000 random queries, verify correctness against Dijkstra
5. Customize weights (simulate prediction update), measure customization time
6. Test with `fast_paths` crate as additional comparison

**GO criteria:**
- Library produces correct shortest paths (100% match vs. Dijkstra)
- CCH customization (weight update) completes in < 10ms for 25K edges
- Query time < 0.05ms per query

**NO-GO fallback:**
- If no usable CCH library: implement basic CCH from Bauer et al. (2010) paper. Add 3 weeks to Phase 1.
- If CCH infeasible entirely: use A* with landmarks (ALT algorithm). Slower (0.2ms/query) but well-understood. Limits reroutes to 50/step instead of 500/step.

---

## 4. Dependency Graph (Critical Path)

```
WEEK  1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16 17 18 19 20 21 22 23 24
      │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │
E1    ├S1+S2┤              │        │              │              │
      │  ├──ECS──┤         │        │              │              │
      │     ├────IDM GPU Shader────┤│              │              │
      │        ├───Staging Buffer──┤│              │              │
      │              ├──Wave-front Dispatch──┤     │              │
      │                    ├──Edge Transitions──┤  │              │
      │                          ├──G1──┤        │              │
      │                          │ ├──Motorbike Sublane────┤     │
      │                          │        ├──G2──┤  │            │
      │                          │              ├──Multi-GPU Partition───┤
      │                          │                    ├──G3──┤   │
      │                          │                          ├──Pedestrian SocialForce──┤
      │                          │                                ├──Checkpoint────┤
      │                          │                                              │
E2    ├──S3──┤                   │                                              │
      ├───OSM Import + Graph─────┤                                              │
      │     ├────CCH Build───────┤                                              │
      │              ├──Route Assignment────┤                                   │
      │                    ├───MOBIL Lane-Change───┤                             │
      │                                ├──Signal Controller──┤                  │
      │                                      ├──Bus/GTFS────┤                   │
      │                                            ├──Prediction Ensemble──┤    │
      │                                                  ├──Scenario DSL───────┤│
      │                                                        ├──Emissions────┤│
      │                                                                        │
E3    ├──PMTiles──┤                                                            │
      │  ├──deck.gl Base Map───┤                                               │
      │        ├──gRPC Skeleton────┤                                           │
      │              ├──WebSocket Binary──┤                                    │
      │                    ├──Vehicle Dots on Map──┤                            │
      │                          ├──G1──┤                                      │
      │                                ├──Heatmaps + Flow──┤                   │
      │                                      ├──KPI Dashboard──┤               │
      │                                            ├──Redis WS Scale───┤       │
      │                                                  ├──Playback Ctrl──┤   │
      │                                                        ├──REST API─┤   │
      │                                                                        │
E4                                ├──HCMC Data Collection──────┤               │
      (not yet started)           ├──Gravity Model OD──────────┤               │
                                        ├──ToD Profiles────┤                   │
                                              ├──GEH Framework─┤              │
                                                    │          │               │
      │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │
WEEK  1  2  3  4  5  6  7  8  9  10 11 12 13 14 15 16 17 18 19 20 21 22 23 24
```

```
WEEK  25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48
      │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │
E1    ├──Meso-Micro Graduated Buffer──┤                                      │
      │     ├──Gridlock Detection─────┤                                      │
      │           ├──Fixed-Point Arith (optional)──┤                         │
      │                          ├──Performance Profiling──────────┤         │
      │                                      ├──Optimization Hotspots──┤     │
      │                                                  ├──Bug Fixes──────┤ │
      │                                                                      │
E2    ├──Scenario Comparison (MOE)────┤                                      │
      │     ├──Emissions HBEFA────────┤                                      │
      │              ├──Export Formats (Parquet/GeoJSON)──┤                   │
      │                                ├──Integration Tests──────────┤       │
      │                                            ├──Documentation──────┤   │
      │                                                                      │
E3    ├──CesiumJS 3D (stretch)────────────────────┤                          │
      │              ├──Load Testing (100 viewers)────────┤                  │
      │                          ├──Dashboard Polish──────────────┤          │
      │                                      ├──Deployment Guide─────┤       │
      │                                                  ├──Demo Prep────┤   │
      │                                                                      │
E4    ├──Bayesian Parameter Optimization──────────────┤                      │
      │     ├──Calibration Iteration Loop─────────────────────┤              │
      │              ├──G4──┤                                  │              │
      │                    ├──Validation vs. Held-Out Counts──────────┤      │
      │                                      ├──Validation Report────┤       │
      │                                            ├──Demo Scenarios──────┤  │
      │                                                  ├──G5──┤            │
      │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │  │
WEEK  25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48
```

### Critical Path (Longest Chain)

```
S1 Spike → ECS → IDM Shader → Wave-front Dispatch → Edge Transitions
→ [G1] → Motorbike Sublane → [G2] → Multi-GPU → [G3]
→ Meso-Micro Buffer → Calibration Loop → [G4]
→ Demo Scenarios → [G5] → POC Complete

Total critical path: ~44 weeks (4 weeks buffer to Week 48)
```

The **bottleneck owner is E1** through Week 20. If E1 is blocked or behind schedule, the entire project slips. E2/E3/E4 work feeds into E1's output but doesn't block E1 until calibration (Week 28+).

---

## 5. Go/No-Go Decision Gates

### Gate G0: GPU Architecture Viability (Week 2)

**Inputs:** S1 (wave-front benchmark), S2 (multi-GPU feasibility)
**Decision maker:** E1 + Technical Lead

| S1 Result | S2 Result | Decision |
|-----------|-----------|----------|
| GO | GO | Proceed as planned: wave-front dispatch, multi-GPU |
| GO | NO-GO | Single GPU, wave-front. Reduce POC to 200K agents |
| NO-GO | GO | EVEN/ODD dispatch, multi-GPU. Add collision monitoring |
| NO-GO | NO-GO | **PROJECT PIVOT:** CPU-only with rayon. Cap at 100K agents. Re-scope POC to District 1 only. Reassess whether VELOS adds value over SUMO |

**If pivot to CPU-only:** The entire architecture simplifies dramatically. Drop velos-gpu crate. Use rayon parallel iterators for IDM. Achievable but unimpressive — loses the competitive advantage over SUMO.

### Gate G1: First Vehicles Moving (Week 8)

**Criteria:**
- [ ] 10K vehicles traverse routes on HCMC network graph
- [ ] Vehicles visible on deck.gl as moving dots
- [ ] No crashes or NaN positions after 1000 simulation steps
- [ ] Frame time < 5ms for 10K agents (extrapolates to ~15ms for 280K)

**PASS:** Proceed to Phase 2.
**FAIL (fixable in 2 weeks):** Extend Phase 1. Identify specific blocker.
**FAIL (fundamental):** Architecture reassessment. Call external Rust/GPU consultant.

### Gate G2: Motorbike Behavior Validated (Week 12)

**Criteria:**
- [ ] 50K motorbikes + 10K cars running simultaneously
- [ ] Motorbike filtering through car gaps visually observable
- [ ] Swarm formation at red lights visually observable
- [ ] Zero lateral collision crashes in 10,000-step stress test
- [ ] Average motorbike speed within ±20% of HCMC observations (25-35 km/h on arterials)

**PASS:** Motorbike sublane model works. Proceed.
**CONDITIONAL PASS:** Filtering works but occasional lateral collision artifacts.
- Mitigation: Add lateral safety gap check. Cap filtering attempts per step.
**FAIL:** Sublane model fundamentally unstable.
- Fallback: Discrete sublanes at 0.5m resolution. Motorbikes occupy 1 sublane, cars occupy 5-7 sublanes. Less realistic but stable.

### Gate G3: 280K Agents Sustained (Week 20)

**Criteria:**
- [ ] 280K agents (200K motorbikes, 50K cars, 10K buses, 20K pedestrians)
- [ ] On 2 GPUs with spatial partitioning
- [ ] frame_time p99 < 15ms sustained for 10,000 steps
- [ ] No memory leaks (VRAM stable over 10,000 steps)
- [ ] Boundary transfer artifacts < 0.1% of boundary-crossing agents

**PASS:** Scale target achieved. Proceed to calibration.
**PARTIAL PASS:** 200K-250K agents stable. 280K causes occasional frame spikes.
- Decision: Accept 250K as POC target (still impressive). Optimize in Phase 4.
**FAIL:** < 200K agents stable.
- Reassess GPU utilization. Profile. Consider single-GPU 200K without partitioning overhead.

### Gate G4: Calibration Feasible (Week 32)

**Criteria:**
- [ ] GEH < 5 for at least **70%** of calibration links (note: NOT 85% yet)
- [ ] Speed RMSE < 10 km/h on arterial roads
- [ ] Bayesian optimization converging (loss decreasing over iterations)
- [ ] At least 40 traffic count locations available for calibration

**PASS:** Calibration on track. Continue iterating toward 85%.
**CONDITIONAL PASS:** GEH < 5 for 50-70%. Data quality issues identified.
- Action: Commission additional field survey (10-20 intersections). Extend calibration sprint by 4 weeks. May reduce final target to 80%.
**FAIL:** GEH < 5 for < 50%. Fundamental demand model issues.
- Action: Re-examine OD matrix. Consider smaller scope area (District 1 only, ~5K edges). Engage traffic engineering consultant.

### Gate G5: Demo-Ready (Week 44)

**Criteria:**
- [ ] 3 demo scenarios run to completion without crash
- [ ] Each scenario produces meaningful, explainable KPI differences
- [ ] Dashboard renders correctly at 1920×1080
- [ ] Stakeholder-facing documentation exists

**PASS:** Proceed to final polish and demo prep.
**CONDITIONAL PASS:** 2/3 scenarios work. 1 has non-critical bugs.
- Action: Drop weakest scenario. Present 2 scenarios + future roadmap.
**FAIL:** < 2 scenarios working.
- Action: Feature freeze. All engineers on bug fixing for 4 weeks. Push demo to Week 52 (13 months).

---

## 6. Phase Plan (Detailed)

### Phase 1: Foundation (Weeks 1-12, Months 1-3)

**Exit criteria:** 50K vehicles + 50K motorbikes on HCMC network, visible in deck.gl

#### Week 1-2: Spikes + Scaffolding

| Day | E1 | E2 | E3 |
|-----|----|----|-----|
| 1-2 | S1: WGSL wave-front benchmark | S3: CCH library evaluation | Cargo workspace setup |
| 3 | S2: wgpu multi-GPU test | S3 continued | PMTiles: download HCMC OSM, run tilemaker |
| 4-5 | **G0 decision** + ECS component design | OSM PBF parser skeleton | deck.gl project scaffold + MapLibre |
| 6-10 | `velos-core`: hecs world, Position, Kinematics | `velos-net`: graph, Edge, Junction | deck.gl: render PMTiles base map |

**Deliverables:**
- G0 decision documented
- Cargo workspace with velos-core, velos-net, velos-gpu skeleton crates
- HCMC base map rendering in browser

#### Week 3-6: Core Physics Engine

| Week | E1 | E2 | E3 |
|------|----|----|-----|
| 3 | IDM shader in WGSL (single lane, no network) | OSM → RoadGraph converter (HCMC) | gRPC proto definitions |
| 4 | Wave-front dispatch integration with ECS | Network cleaning + short-edge merging | gRPC server skeleton (tonic) |
| 5 | Staging buffer + double-buffer pattern | CCH build for HCMC network | WebSocket binary frame format |
| 6 | Edge transition logic (position overflow → next edge) | Route assignment (CCH query → edge list) | deck.gl: render agent dots from WebSocket |

**Deliverables:**
- Agents moving along edges with IDM car-following
- Routes computed via CCH
- First integration: deck.gl shows moving dots

#### Week 7-8: First Integration + G1

| Week | E1 | E2 | E3 |
|------|----|----|-----|
| 7 | Junction logic (right-of-way, signal stub) | Route advance + lane selection | WebSocket spatial tiling |
| 8 | **G1 gate validation**: 10K vehicles end-to-end | Multi-lane leader sorting | **G1 gate validation**: dots visible in deck.gl |

**Deliverables:**
- **G1 PASS/FAIL decision**
- 10K vehicles traversing HCMC network, visible in browser

#### Week 9-12: Lane Change + Motorbike

| Week | E1 | E2 | E3 |
|------|----|----|-----|
| 9 | Motorbike sublane: lateral position component | MOBIL lane-change model | Heatmap layer (density) |
| 10 | Motorbike filtering shader (lateral gap check) | MOBIL + IDM integration | Speed-color road overlay |
| 11 | Motorbike swarm behavior at signals | Lane-change + route coordination | Agent type color coding |
| 12 | **G2 gate validation**: 50K motorbikes stress test | CCH weight customization (prep for prediction) | Basic KPI display (agent count, avg speed) |

**Deliverables:**
- **G2 PASS/FAIL decision**
- 100K agents (50K motorbikes + 50K cars) with lane changes and filtering

---

### Phase 2: Scale + Intelligence (Weeks 13-24, Months 4-6)

**Exit criteria:** 280K agents on 2 GPUs with prediction, pedestrians, buses, checkpoint

| Week | E1 | E2 | E3 | E4 (joins Week 9*) |
|------|----|----|-----|-----|
| 13 | METIS graph partition | Fixed-time signal controller | Redis pub/sub for WebSocket | HCMC data: traffic count collection |
| 14 | Multi-GPU buffer management | Signal phases from HCMC data | WebSocket relay pod (stateless) | HCMC data: signal timing survey |
| 15 | Boundary agent transfer protocol | Bus model: route + dwell time | Multiple viewer stress test | Gravity model OD matrix |
| 16 | Multi-GPU integration testing | GTFS import (HCMC bus routes) | Flow arrow layer | OD matrix refinement |
| 17 | Pedestrian spatial hash (GPU) | Prediction: BPR model | heatmap + flow analytics | Time-of-day demand profiles |
| 18 | Pedestrian adaptive workgroups | Prediction: ETS correction | KPI dashboard: charts | Demand validation vs counts |
| 19 | Pedestrian social force shader | Prediction: historical matcher | Playback: pause/resume/speed | SUMO baseline model (comparison) |
| 20 | **G3: 280K agents stress test** | Prediction: ArcSwap ensemble | **G3: 280K visualization test** | Initial GEH comparison |
| 21 | Checkpoint: ECS → Parquet save | Agent reroute scheduling | Checkpoint: UI save/load buttons | Calibration tooling |
| 22 | Checkpoint: Parquet → ECS restore | CCH dynamic weight integration | REST API convenience layer | GEH report generation |
| 23 | Meso queue model | Prediction → CCH weight flow | Export: Parquet + CSV | Sensitivity analysis tooling |
| 24 | Meso-micro graduated buffer | Scenario DSL design | Export: GeoJSON | Parameter range documentation |

*E4 onboarding begins Week 9 (data collection). Full-time coding from Week 13.

**Deliverables:**
- **G3 PASS/FAIL decision**
- 280K agents, 2 GPUs, prediction, pedestrians, buses
- Checkpoint save/restore working
- Basic calibration tooling operational

---

### Phase 3: Calibration + Validation (Weeks 25-36, Months 7-9)

**Exit criteria:** GEH < 5 for 85% of links. Scenario comparison operational.

| Week | E1 | E2 | E3 | E4 |
|------|----|----|-----|-----|
| 25 | Meso-micro velocity matching | Scenario batch runner | CesiumJS 3D (stretch) | Bayesian optimization: first pass |
| 26 | Gridlock detection (Tarjan SCC) | Scenario comparison: MOE calc | CesiumJS: building extrusions | Bayesian: parameter sensitivity |
| 27 | Gridlock resolution strategies | Emissions: HBEFA integration | CesiumJS: agent rendering | Calibration iteration 1 |
| 28 | Fixed-point arithmetic (optional) | Scenario: statistical significance | Load testing: 50 viewers | Calibration iteration 2 |
| 29 | GPU profiling pass | Export: SUMO FCD XML | Load testing: 100 viewers | Calibration iteration 3 |
| 30 | Optimization: shader hotspots | Integration tests: network + signal | Dashboard: scenario compare view | Calibration iteration 4 |
| 31 | Optimization: memory allocation | Integration tests: prediction + routing | Dashboard: emissions overlay | Calibration iteration 5 |
| 32 | Performance regression CI gate | Benchmark suite | Error handling + edge cases | **G4: GEH at 70%?** |
| 33 | Bug fixes from calibration | Bug fixes from calibration | Bug fixes from visualization | Validation: held-out counts |
| 34 | Bug fixes continued | Documentation: architecture | Documentation: API reference | Validation: speed profiles |
| 35 | Code review + refactor | Documentation: deployment | Documentation: user guide | Validation report draft |
| 36 | Code review + refactor | Documentation: scenarios | Dashboard: final layout | Validation report final |

**Deliverables:**
- **G4 PASS/FAIL decision**
- Calibrated model (GEH < 5 for 85% links)
- Validation report published
- Scenario comparison working

---

### Phase 4: Hardening + Demo (Weeks 37-48, Months 10-12)

**Exit criteria:** 3 demo scenarios, complete documentation, stakeholder presentation

| Week | E1 | E2 | E3 | E4 |
|------|----|----|-----|-----|
| 37-38 | Performance: sustained 10Hz for 24h sim | Integration test suite | Dashboard polish | Demo Scenario 1: AM rush hour |
| 39-40 | Memory leak hunting | Regression benchmarks in CI | Responsive layout | Demo Scenario 2: road closure |
| 41-42 | Edge case: demand overflow | WGSL shader validation (naga) | Animation smoothness | Demo Scenario 3: signal retime |
| 43-44 | Edge case: network discontinuities | **G5: demo stability check** | **G5: visual check** | **G5: scenario results check** |
| 45-46 | Bug fixing sprint | Documentation: final pass | Demo slide deck / video | Calibration: final adjustments |
| 47-48 | Release candidate | Release candidate | Release candidate | Presentation preparation |

**Deliverables:**
- **G5 PASS/FAIL decision**
- 3 demo scenarios with MOE comparison
- Complete documentation suite
- Docker Compose deployment tested on fresh server
- Stakeholder presentation

---

## 7. Demo Scenarios

### Scenario 1: Morning Rush Hour (Baseline)

```
Sim window:   06:00 → 09:00 (3 hours)
Agent ramp:   50K → 280K → 200K
Key metric:   Average network speed, total vehicle-hours of delay
Key corridors: Vo Van Kiet, Cach Mang Thang Tam, Dien Bien Phu, Nguyen Van Linh
Output:        LOS grades per corridor, speed heatmap animation
```

### Scenario 2: Event Road Closure

```
Modification: Close Nguyen Hue (walking street) + 3 surrounding blocks
              for a public event (7:00 → 9:00 AM)
Question:     How does traffic redistribute?
              Which corridors see > 20% speed reduction?
Compare:      Side-by-side MOE delta vs. Scenario 1
Output:        Choropleth of speed change, reroute flow visualization
```

### Scenario 3: Signal Retiming Intervention

```
Modification: Optimize signal timing on Vo Van Kiet corridor (6 intersections)
              Increase main-direction green by 15s, reduce side streets by 15s
Question:     Does corridor travel time improve?
              What is the side-street delay increase?
Compare:       Travel time distributions (before/after), queue length comparison
Output:        Time-series chart of corridor speed, intersection-level queue data
```

---

## 8. Risk Register

### Critical Risks (Project-Threatening)

| ID | Risk | P | I | Trigger | Mitigation | Fallback | Owner |
|----|------|---|---|---------|------------|----------|-------|
| R1 | Wave-front dispatch GPU occupancy too low | M | H | S1 benchmark result < 40% of parallel | Spike S1 in Week 1 | EVEN/ODD + 3-pass correction | E1 |
| R2 | wgpu multi-adapter broken for compute | M | H | S2 test fails | Spike S2 in Week 1 | Single-GPU, 200K agents | E1 |
| R3 | HCMC traffic data insufficient (<30 count locations) | H | H | E4 data collection report (Week 16) | Start collection early (E2, Week 9). GPS probe partnership | Google Maps qualitative speed data. Reduce calibration scope to District 1 | E4 |
| R4 | Calibration doesn't converge (GEH >5 for >50% links at Week 32) | M | H | G4 gate failure | Engage traffic engineering consultant. Re-examine OD matrix | Reduce scope to District 1. Present as "demonstrator" not "calibrated model" | E4 |

### Major Risks (Phase-Disrupting)

| ID | Risk | P | I | Trigger | Mitigation | Fallback | Owner |
|----|------|---|---|---------|------------|----------|-------|
| R5 | Motorbike sublane model crashes under density | M | M | G2 gate (Week 12) | Lateral safety check, density cap | Discrete sublanes (0.5m resolution) | E1 |
| R6 | Multi-GPU boundary artifacts (speed discontinuity) | M | M | G3 gate (Week 20) | 50m overlap zone at partition boundaries | Single-GPU 200K agents | E1 |
| R7 | CCH implementation produces incorrect paths | L | H | CCH validation test (Week 5) | Validate 1000 pairs vs. Dijkstra | Use A* with landmarks (ALT). Slower but correct | E2 |
| R8 | HCMC signal timing data unavailable (DOT unresponsive) | H | M | E4 report (Week 14) | Default timing from junction geometry | Field survey top 30 intersections ($3K budget) | E4 |
| R9 | E4 delayed start (Month 4 instead of 3) | M | M | HR/recruitment delay | E2 begins data collection in Month 2. Full handoff doc prepared | Accept 1-month calibration delay. Compress Phase 3 | E2 |

### Minor Risks (Inconvenient)

| ID | Risk | P | I | Mitigation | Owner |
|----|------|---|---|-----------|-------|
| R10 | deck.gl performance with 280K points on low-end client | M | L | Server-side heatmap aggregation mode | E3 |
| R11 | Fixed-point WGSL arithmetic >50% overhead | M | L | Accept float32 + document cross-GPU delta. Statistical equivalence | E1 |
| R12 | Parquet checkpoint >1s for 280K agents | L | L | Async write (tokio::spawn_blocking) | E1 |
| R13 | Redis pub/sub memory pressure at 10Hz × 256 tiles | L | L | Increase tile size from 500m to 1km. Reduce frame rate to 5Hz | E3 |

---

## 9. Quality Gates

### Continuous Integration (Every PR)

```bash
# Mandatory — PR cannot merge if any fail
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --no-fail-fast
cargo bench --bench frame_time -- --baseline main  # no regression > 10%
naga --validate crates/velos-gpu/shaders/*.wgsl     # shader correctness
```

### Weekly Performance Tracking

```
Every Friday: run standard benchmark suite, publish to Grafana
- bench_frame_10k:   target < 2ms
- bench_frame_100k:  target < 10ms
- bench_frame_280k:  target < 15ms (after Week 20)
- bench_cch_1000:    target < 1ms
- bench_checkpoint:  target < 500ms
Track trend lines. Alert if 3 consecutive weeks show regression.
```

### Monthly Milestone Gates

| Gate | Week | Criteria | Fail Action |
|------|------|----------|-------------|
| G0 | 2 | GPU architecture viable (spikes pass) | Architecture pivot meeting |
| G1 | 8 | Vehicles moving on map | Extend Phase 1, max 2 weeks |
| G2 | 12 | Motorbike behavior validated | Simplify to discrete sublanes |
| G3 | 20 | 280K agents sustained | Reduce to 200K, optimize later |
| G4 | 32 | GEH < 5 for 70% links | Consultant, reduce scope area |
| G5 | 44 | 3 demo scenarios stable | Drop 1 scenario, fix-only sprint |

---

## 10. Budget Estimate

### Personnel (Vietnam market rates)

| Role | Monthly Rate | Duration | Total |
|------|-------------|----------|-------|
| E1: Engine Lead (Sr Rust/GPU) | $4,000 | 12 months | $48,000 |
| E2: Network/Routing (Sr Rust) | $3,500 | 12 months | $42,000 |
| E3: API/Viz (Sr Full-Stack) | $3,000 | 12 months | $36,000 |
| E4: Calibration (Traffic Eng.) | $3,000 | 10 months | $30,000 |
| **Subtotal Personnel** | | | **$156,000** |

### Infrastructure

| Item | Cost | Duration | Total |
|------|------|----------|-------|
| Dev workstation (2× RTX 4090) | $4,200 | One-time | $4,200 |
| Cloud GPU CI/CD (Lambda Labs) | $400/mo | 12 months | $4,800 |
| Cloud staging server | $300/mo | 6 months | $1,800 |
| **Subtotal Infra** | | | **$10,800** |

### Data & External

| Item | Cost |
|------|------|
| GPS probe data (Grab partnership or purchase) | $0 - $15,000 |
| Field survey (signal timing, counts) | $3,000 - $5,000 |
| Traffic engineering consultant (if G4 fails) | $5,000 - $10,000 |
| **Subtotal Data** | **$8,000 - $30,000** |

### Total

| Scenario | Total |
|----------|-------|
| **Optimistic** (partnerships, no consultant) | **$175,000** |
| **Expected** | **$195,000** |
| **Pessimistic** (data purchase, consultant needed) | **$230,000** |

---

## 11. Beyond POC: v3 + v4 Vision

### v3 — Production HCMC Digital Twin (Year 2)

| Capability | Details |
|-----------|---------|
| **Full HCMC metro** | All 24 districts + Thu Duc City. 80K edges, 2M agents |
| **Multi-node** | 2-4 nodes, 8-16 GPUs. gRPC-based distributed simulation |
| **Real-time sensors** | Loop detector + camera count ingestion via MQTT |
| **ML prediction** | LSTM/Transformer models served via gRPC (not in-process) |
| **Actuated signals** | Vehicle-actuated + adaptive signal controllers |
| **Transit passengers** | Multi-commodity passenger flow with boarding/alighting |
| **CesiumJS 3D** | Full 3D with OSM building extrusions, photorealistic terrain |
| **SaaS API** | Multi-tenant, API key management, usage metering |
| **TMC integration** | HCMC Traffic Management Center data feed |

**Prerequisites from POC:**
- Validated simulation engine (calibrated, stable)
- Proven GPU dispatch architecture (wave-front or EVEN/ODD)
- Working visualization and API layer
- Team experienced with Rust/GPU/traffic sim stack

### v4 — Multi-City Platform (Year 3)

| Capability | Details |
|-----------|---------|
| **Multi-city** | Hanoi, Da Nang, Can Tho. City-specific calibration profiles |
| **Cloud auto-scale** | K8s GPU node pools, on-demand scenario workers |
| **10M agents** | National-scale simulation for highway corridor planning |
| **Real-time digital twin** | Live sensor → simulation → prediction → intervention loop |
| **AV simulation** | Autonomous vehicles in mixed traffic (V2I, cooperative) |
| **Emissions optimization** | City-wide routing to minimize aggregate CO2 |
| **Academic dataset** | Publish HCMC traffic dataset for research community |
| **Marketplace** | Plugin marketplace for car-following models, signal controllers |

---

## 12. Definition of Done (POC — Week 48)

- [ ] 280K agents simulate HCMC Districts 1/3/5/10/Binh Thanh at 10 steps/sec
- [ ] Motorbike sublane filtering visually matches HCMC traffic patterns
- [ ] GEH < 5 for 85%+ calibration links against HCMC traffic counts
- [ ] 3 demo scenarios (baseline, road closure, signal retiming) run and compare
- [ ] deck.gl dashboard: real-time KPIs, heatmaps, flow arrows, speed overlays
- [ ] Checkpoint save/restore (crash at hour 23 → restore at hour 22:55)
- [ ] 100 concurrent dashboard viewers sustained
- [ ] Prediction ensemble updating CCH weights every 60 sim-seconds
- [ ] API documentation (gRPC + REST) published
- [ ] Deployment guide (Docker Compose) tested on fresh server
- [ ] Calibration report with GEH/RMSE tables
- [ ] All CI gates passing (clippy, tests, benchmarks, shader validation)
- [ ] Zero critical bugs in issue tracker
