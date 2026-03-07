# Roadmap: VELOS

## Milestones

- Shipped **v1.0 MVP** -- Phases 1-4 (shipped 2026-03-07)
- Active **v1.1 SUMO Replacement Engine** -- Phases 5-7 (in progress)

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
- [ ] **Phase 6: Agent Models & Signal Control** - All agent types at scale (bus, bicycle, truck, emergency), pedestrian adaptive workgroups, meso-micro hybrid, actuated/adaptive signals, V2I communication, traffic signs
- [ ] **Phase 7: Intelligence, Routing & Prediction** - Agent intelligence (multi-factor cost, profiles, GPU perception/evaluation), CCH routing, prediction ensemble, staggered reroute, global knowledge routing

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
**Plans:** 7 plans
Plans:
- [x] 06-01-PLAN.md -- GpuAgentState expansion (32->40 bytes) + VehicleType extension + new agent type params
- [ ] 06-02-PLAN.md -- Bus agents with dwell model + GTFS import for HCMC bus routes
- [ ] 06-03-PLAN.md -- Actuated + adaptive signal controllers with loop detectors
- [ ] 06-04-PLAN.md -- Emergency vehicle yield behavior + GPU shader branching
- [ ] 06-05-PLAN.md -- V2I: SPaT broadcast, signal priority, traffic signs
- [ ] 06-06-PLAN.md -- Pedestrian adaptive GPU workgroups with prefix-sum compaction
- [ ] 06-07-PLAN.md -- Meso-micro hybrid: velos-meso crate with BPR queue model + buffer zones

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
**Plans**: TBD

## Progress

**Execution Order:**
Phases 5 through 7 execute sequentially. Each phase depends on the prior phase.

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. GPU Pipeline & Visual Proof | v1.0 | 2/2 | Complete | 2026-03-06 |
| 2. Road Network & Vehicle Models + egui | v1.0 | 4/4 | Complete | 2026-03-07 |
| 3. Motorbike Sublane & Pedestrians | v1.0 | 2/2 | Complete | 2026-03-07 |
| 4. MOBIL Wiring + Motorbike Jam Fix + Performance | v1.0 | 3/3 | Complete | 2026-03-07 |
| 5. Foundation & GPU Engine | v1.1 | 6/6 | Complete | 2026-03-07 |
| 6. Agent Models & Signal Control | v1.1 | 1/7 | In progress | - |
| 7. Intelligence, Routing & Prediction | v1.1 | 0/0 | Not started | - |
