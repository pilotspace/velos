# Roadmap: VELOS

## Milestones

- Shipped **v1.0 MVP** -- Phases 1-4 (shipped 2026-03-07)
- Active **v1.1 SUMO Replacement Engine** -- Phases 5-11 (in progress)

## Phases

<details>
<summary>Shipped v1.0 MVP (Phases 1-4) -- SHIPPED 2026-03-07</summary>

- [x] Phase 1: GPU Pipeline & Visual Proof (2/2 plans) -- completed 2026-03-06
- [x] Phase 2: Road Network & Vehicle Models + egui (4/4 plans) -- completed 2026-03-07
- [x] Phase 3: Motorbike Sublane & Pedestrians (2/2 plans) -- completed 2026-03-07
- [x] Phase 4: MOBIL Wiring + Motorbike Jam Fix + Performance (3/3 plans) -- completed 2026-03-07

</details>

### v1.1 SUMO Replacement Engine

- [ ] **Phase 5: Foundation & GPU Engine** - God Crate decomposition, GPU physics cutover, multi-GPU wave-front dispatch, fixed-point arithmetic, 5-district HCMC network, SUMO file imports, Krauss car-following model (gap closure in progress)
- [x] **Phase 6: Agent Models & Signal Control** - All agent types at scale (bus, bicycle, truck, emergency), pedestrian adaptive workgroups, meso-micro hybrid, actuated/adaptive signals, V2I communication, traffic signs (completed 2026-03-07)
- [x] **Phase 7: Intelligence, Routing & Prediction** - Agent intelligence (multi-factor cost, profiles, GPU perception/evaluation), CCH routing, prediction ensemble, staggered reroute, global knowledge routing (completed 2026-03-07)
- [x] **Phase 8: Tuning Vehicle Behavior (HCM)** - Vehicle behavior externalized to TOML config, GPU/CPU parameter parity, HCMC-specific behavioral rules (completed 2026-03-08)
- [ ] **Phase 9: Sim Loop Integration — Startup & Frame Pipeline** - Wire all Phase 6-8 modules into sim.rs tick_gpu() and app.rs startup: perception, reroute, polymorphic signals, sign upload, vehicle params, HCMC behaviors
- [x] **Phase 10: Sim Loop Integration — Bus Dwell & Meso-Micro Hybrid** - Wire bus dwell lifecycle and velos-meso crate into sim loop for peripheral zone transitions (completed 2026-03-08)
- [ ] **Phase 11: GPU Buffer Wiring — Perception & Emergency** - Wire perception result buffer to wave_front.wgsl binding(8) and emergency vehicle upload to sim loop tick (gap closure)

## Phase Details

### Phase 5: Foundation & GPU Engine
**Goal**: Simulation runs entirely on GPU at 280K-agent scale across multiple GPUs on a cleaned 5-district HCMC road network, with SUMO file compatibility and dual car-following model support
**Depends on**: Phase 4 (v1.0 complete)
**Requirements**: GPU-01, GPU-02, GPU-03, GPU-04, GPU-05, GPU-06, NET-01, NET-02, NET-03, NET-04, NET-05, NET-06, CFM-01, CFM-02
**Success Criteria** (what must be TRUE):
  1. All 280K agents update positions via GPU compute shaders every frame -- no CPU physics fallback path exists in the codebase
  2. Simulation sustains 10 steps/sec real-time with 280K agents on 2-4 GPUs, verified by frame-time benchmarks under load
  3. Loading the 5-district HCMC network produces a cleaned graph with ~25K edges, correct one-way streets, motorbike-only lanes, and no disconnected components
  4. Importing a SUMO .net.xml file produces the same road graph as OSM import for an equivalent area, and .rou.xml demand files spawn agents on correct routes
  5. Switching an agent between Krauss and IDM car-following at runtime produces visibly different driving behavior (Krauss dawdles, IDM maintains desired speed)
**Plans:** 6 plans (5 complete, 1 gap closure)
Plans:
- [x] 05-01-PLAN.md -- Fixed-point types + Krauss car-following + CarFollowingModel ECS component
- [x] 05-02-PLAN.md -- 5-district HCMC network import + cleaning pipeline + ToD demand profiles
- [x] 05-03-PLAN.md -- SUMO .net.xml and .rou.xml file import compatibility
- [x] 05-04-PLAN.md -- GPU wave-front dispatch + physics cutover (IDM+Krauss shader)
- [x] 05-05-PLAN.md -- Multi-GPU partitioning + boundary protocol + 280K benchmark
- [x] 05-06-PLAN.md -- Gap closure: wire CarFollowingModel into agent spawning + verify GPU behavior differentiation

### Phase 6: Agent Models & Signal Control
**Goal**: Every vehicle and pedestrian type operates at GPU scale with realistic behavior, signals respond to traffic demand, and agents interact with V2I infrastructure
**Depends on**: Phase 5
**Requirements**: AGT-01, AGT-02, AGT-03, AGT-04, AGT-05, AGT-06, AGT-07, AGT-08, SIG-01, SIG-02, SIG-03, SIG-04, SIG-05
**Success Criteria** (what must be TRUE):
  1. A GTFS-loaded bus route shows buses stopping at designated locations, dwelling realistically (visible passenger boarding delay), and resuming on schedule
  2. Emergency vehicles trigger yield behavior from surrounding agents and receive signal priority at intersections
  3. Pedestrian simulation at varying densities shows GPU workgroup adaptation (sparse areas use fewer threads, dense areas use more) with measurable speedup over uniform dispatch
  4. Agents approaching a speed limit sign visibly reduce to the posted speed, and agents at a no-turn restriction do not attempt the restricted maneuver
  5. Peripheral network zones run mesoscopic queue model (O(1) per edge) while core zones remain microscopic, with agents transitioning smoothly through 100m buffer zones without speed discontinuities
**Plans:** 7/7 plans complete
Plans:
- [x] 06-01-PLAN.md -- GpuAgentState expansion (32->40 bytes) + VehicleType extension + new agent type params
- [x] 06-02-PLAN.md -- Bus agents with dwell model + GTFS import for HCMC bus routes
- [x] 06-03-PLAN.md -- Actuated + adaptive signal controllers with loop detectors
- [x] 06-04-PLAN.md -- Emergency vehicle yield behavior + GPU shader branching
- [x] 06-05-PLAN.md -- V2I: SPaT broadcast, signal priority, traffic signs
- [x] 06-06-PLAN.md -- Pedestrian adaptive GPU workgroups with prefix-sum compaction
- [x] 06-07-PLAN.md -- Meso-micro hybrid: velos-meso crate with BPR queue model + buffer zones

### Phase 7: Intelligence, Routing & Prediction
**Goal**: Agents make intelligent route choices using predicted future conditions, reroute dynamically around congestion, and exhibit profile-driven behavior differences
**Depends on**: Phase 6
**Requirements**: INT-01, INT-02, INT-03, INT-04, INT-05, RTE-01, RTE-02, RTE-03, RTE-04, RTE-05, RTE-06, RTE-07
**Success Criteria** (what must be TRUE):
  1. A Commuter agent and a Tourist agent given the same origin-destination choose different routes due to differing cost weights (time vs comfort), visible in their path choices
  2. Creating a road closure mid-simulation causes affected agents to reroute via CCH within the same step, with 500 reroutes/step completing without frame drops
  3. The prediction ensemble updates edge weights every 60 sim-seconds, and agents receiving prediction-informed routes avoid corridors that are currently free-flowing but predicted to congest
  4. GPU perception phase produces per-agent awareness of leader vehicle, signal state, traffic signs, and nearby agents, feeding the evaluation phase that outputs should_reroute decisions
  5. Staggered reroute evaluation processes 1K agents/step across the full population, with immediate triggers firing for blocked edges and emergency vehicles
**Plans:** 6/6 plans complete
Plans:
- [x] 07-01-PLAN.md -- CCH core: node ordering + contraction + binary disk cache
- [x] 07-02-PLAN.md -- Multi-factor cost function + 8 agent profiles + demand wiring
- [x] 07-03-PLAN.md -- CCH weight customization + bidirectional Dijkstra query + rayon parallel queries
- [x] 07-04-PLAN.md -- Prediction ensemble (velos-predict crate): BPR + ETS + historical + ArcSwap overlay
- [x] 07-05-PLAN.md -- GPU perception kernel (perception.wgsl + PerceptionPipeline)
- [x] 07-06-PLAN.md -- Reroute scheduler + CPU evaluation + simulation integration

### Phase 8: Tuning Vehicle Behavior to More Realistic in HCM
**Goal**: All vehicle behavior parameters are externalized to config, GPU/CPU parameter mismatch is eliminated, and HCMC-specific behavioral rules (red-light creep, aggressive weaving, yield-based intersection negotiation) produce visually realistic mixed-traffic patterns
**Depends on**: Phase 7
**Requirements**: TUN-01, TUN-02, TUN-03, TUN-04, TUN-05, TUN-06
**Success Criteria** (what must be TRUE):
  1. All ~50 vehicle behavior parameters load from data/hcmc/vehicle_params.toml -- no hardcoded IDM/Krauss constants remain in the GPU shader or CPU factory functions
  2. GPU and CPU produce identical IDM/Krauss acceleration for the same vehicle type and same config values (parameter mismatch eliminated)
  3. Truck v0 is 30-40 km/h (HCMC urban), not 90 km/h (highway); car v0 is 30-40 km/h, not 50 km/h
  4. Motorbikes inch forward past the stop line during red lights, forming a dense swarm that launches first on green
  5. Motorbikes squeeze through 0.5m lateral gaps at low speed differences, with gap threshold widening at higher speed differences
  6. Vehicles at unsignalized intersections negotiate via gap acceptance with vehicle-type-dependent TTC thresholds and no deadlock
**Plans:** 3/3 plans complete
Plans:
- [x] 08-01-PLAN.md -- VehicleConfig TOML infrastructure + HCMC-calibrated defaults + factory migration
- [x] 08-02-PLAN.md -- GPU parameter unification: GpuVehicleParams uniform buffer + WGSL shader migration
- [x] 08-03-PLAN.md -- HCMC behavioral rules: red-light creep, aggressive weaving, intersection gap acceptance

### Phase 9: Sim Loop Integration — Startup & Frame Pipeline
**Goal**: All Phase 6-8 modules are wired into sim.rs::tick_gpu() and app.rs::GpuState::new() — the simulation runs the full pipeline (perception, reroute, polymorphic signals, sign interaction, vehicle params, HCMC behaviors) not just Phase 5 physics
**Depends on**: Phase 8
**Requirements**: SIG-01, SIG-02, SIG-03, SIG-04, SIG-05, INT-03, INT-04, INT-05, RTE-03, RTE-07, TUN-02, TUN-04, TUN-06
**Gap Closure:** Closes M-1 through M-6, M-9 from v1.1 audit
**Success Criteria** (what must be TRUE):
  1. upload_vehicle_params() is called at startup — GPU uniform buffer at binding 7 contains correct per-type parameters, not zeros
  2. init_reroute() is called at startup — CCH router, prediction overlay, and reroute scheduler are initialized and non-None
  3. PerceptionPipeline is instantiated in GpuState and dispatched every frame in tick_gpu() — perception_results buffer is populated
  4. step_reroute() is called every frame after perception — agents with should_reroute flag receive new CCH routes
  5. SignalController dispatch uses the trait polymorphically — actuated/adaptive controllers are instantiated based on intersection config, not hardcoded FixedTimeController
  6. sign_buffer is populated with sign data at startup via upload_signs() — handle_sign_interaction processes real sign data
  7. red_light_creep_speed() and intersection_gap_acceptance() are called from the GPU simulation path for motorbike agents
**Plans:** 2/3 plans executed
Plans:
- [ ] 09-01-PLAN.md — Startup initialization: vehicle config, polymorphic signals, sign upload, PerceptionPipeline
- [ ] 09-02-PLAN.md — WGSL shader: perception_results binding + HCMC behaviors (red-light creep, gap acceptance)
- [ ] 09-03-PLAN.md — Frame pipeline: perception dispatch, reroute, loop detectors, signal priority

### Phase 10: Sim Loop Integration — Bus Dwell & Meso-Micro Hybrid
**Goal**: Bus agents stop at designated stops with realistic dwell times, and peripheral network zones run mesoscopic queue model with smooth micro-meso transitions through buffer zones
**Depends on**: Phase 9
**Requirements**: AGT-01, AGT-05, AGT-06
**Gap Closure:** Closes M-7, M-8 from v1.1 audit
**Success Criteria** (what must be TRUE):
  1. begin_dwell() and tick_dwell() are called in the sim loop — buses stop at BusStop locations, FLAG_BUS_DWELLING is set, and dwell time follows the empirical model (5s + 0.5s/boarding + 0.67s/alighting)
  2. velos-meso is a dependency of velos-core or velos-gpu — the crate is imported and its queue model is active for peripheral zone edges
  3. Agents crossing from meso to micro zones pass through the 100m buffer zone with velocity-matching insertion — no speed discontinuities at zone boundaries
**Plans:** 2/2 plans complete
Plans:
- [ ] 10-01-PLAN.md — Bus dwell wiring: BusState spawn, step_bus_dwell CPU step, GPU FLAG_BUS_DWELLING guard
- [ ] 10-02-PLAN.md — Meso-micro hybrid wiring: velos-meso dependency, zone config, step_meso, zone transitions

### Phase 11: GPU Buffer Wiring — Perception & Emergency
**Goal**: Perception results reach GPU behavior functions (red-light creep, gap acceptance) via binding(8), and emergency vehicle yield cones activate via GPU upload — closing the last integration gaps in the sim loop
**Depends on**: Phase 9
**Requirements**: (GPU-path correctness for TUN-04, TUN-06, INT-03, AGT-08)
**Gap Closure:** Closes integration gaps from v1.1 audit (perception binding, emergency upload)
**Success Criteria** (what must be TRUE):
  1. set_perception_result_buffer() is called in SimWorld::new() — binding(8) contains perception results, not zeros
  2. red_light_creep_speed() reads actual signal_state from perception buffer — creep activates on red, not on green
  3. intersection_gap_acceptance() reads actual leader_speed and wait_time from perception buffer
  4. upload_emergency_vehicles() is called every frame in tick_gpu() — emergency_count > 0 when emergency vehicles exist
  5. GPU yield cone activates for agents near emergency vehicles (not early-exiting due to zero count)
**Plans:** 2 plans
Plans:
- [ ] 11-01-PLAN.md -- Perception buffer wiring: shared result buffer for binding(8)
- [ ] 11-02-PLAN.md -- Emergency vehicle upload + FLAG_EMERGENCY_ACTIVE in step_vehicles_gpu

## Progress

**Execution Order:**
Phases 5 through 8 execute sequentially. Each phase depends on the prior phase.

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. GPU Pipeline & Visual Proof | v1.0 | 2/2 | Complete | 2026-03-06 |
| 2. Road Network & Vehicle Models + egui | v1.0 | 4/4 | Complete | 2026-03-07 |
| 3. Motorbike Sublane & Pedestrians | v1.0 | 2/2 | Complete | 2026-03-07 |
| 4. MOBIL Wiring + Motorbike Jam Fix + Performance | v1.0 | 3/3 | Complete | 2026-03-07 |
| 5. Foundation & GPU Engine | v1.1 | 6/6 | Complete | 2026-03-07 |
| 6. Agent Models & Signal Control | v1.1 | 7/7 | Complete | 2026-03-07 |
| 7. Intelligence, Routing & Prediction | v1.1 | 6/6 | Complete | 2026-03-07 |
| 8. Tuning Vehicle Behavior (HCM) | v1.1 | 3/3 | Complete | 2026-03-08 |
| 9. Sim Loop Integration — Startup & Frame Pipeline | 2/3 | In Progress|  | - |
| 10. Sim Loop Integration — Bus Dwell & Meso-Micro | 2/2 | Complete    | 2026-03-08 | - |
| 11. GPU Buffer Wiring — Perception & Emergency | v1.1 | 0/2 | Planned | - |
