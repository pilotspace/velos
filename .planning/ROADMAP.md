# Roadmap: VELOS

## Overview

VELOS validates GPU-accelerated traffic microsimulation on macOS Apple Silicon through five phases: prove the GPU compute pipeline works (spikes), build the road network with core vehicle models, add the motorbike sublane differentiator alongside pedestrians and bicycles, layer on routing/prediction/meso-micro hybrid, then wrap everything in a native desktop application using winit + egui.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: GPU Foundation & Spikes** - Validate wgpu/Metal compute pipeline, fixed-point WGSL arithmetic, wave-front dispatch, and ECS-to-GPU round-trip
- [ ] **Phase 2: Road Network & Core Vehicle Models** - Build HCMC road graph, IDM car-following, MOBIL lane-change, signals, demand spawning, gridlock detection
- [ ] **Phase 3: Motorbike Sublane & Pedestrian & Bicycle** - Continuous lateral positioning, social force pedestrians, bicycle agents -- the core differentiator
- [ ] **Phase 4: Routing & Prediction & Meso-Micro** - CCH pathfinding, dynamic rerouting, prediction ensemble, mesoscopic hybrid zones
- [ ] **Phase 5: Desktop Application** - winit window, wgpu 2D rendering, egui dashboard, simulation controls via in-process calls

## Phase Details

### Phase 1: GPU Foundation & Spikes
**Goal**: A proven wgpu/Metal compute pipeline dispatches wave-front updates with fixed-point arithmetic, round-trips ECS data through GPU buffers, and produces measurable performance baselines
**Depends on**: Nothing (first phase)
**Requirements**: GPU-01, GPU-02, GPU-03, GPU-04, GPU-05, GPU-06, PERF-01, PERF-02
**Success Criteria** (what must be TRUE):
  1. A wgpu compute shader dispatches on Metal and writes agent position/velocity data back to CPU-readable buffers
  2. Fixed-point multiply (Q16.16) produces bitwise-identical results between CPU Rust code and WGSL shader for edge-case inputs (overflow boundaries, negative values, zero)
  3. hecs entities project into SoA GPU buffers and read back correctly after a compute dispatch round-trip
  4. Per-lane leader index computation correctly sorts agents by position within each lane, with dual-leader tracking during lane-change transitions
  5. PCG hash seeded with (agent_id, step) produces uniform noise in WGSL without using rand(), and results are deterministic across runs
**Plans**: 3 plans

Plans:
- [ ] 01-01-PLAN.md -- Workspace bootstrap, fixed-point types (Q16.16/Q12.20/Q8.8), golden vectors, wgpu device/buffer/dispatcher foundation, WGSL fixed-point parity
- [ ] 01-02-PLAN.md -- ECS-to-GPU round-trip with 1K agents (GO/NO-GO gate)
- [ ] 01-03-PLAN.md -- Wave-front dispatch with leader sort, PCG hash, integrated benchmark

### Phase 2: Road Network & Core Vehicle Models
**Goal**: Cars spawn from OD matrices onto a real HCMC road network, follow IDM car-following, change lanes via MOBIL, obey traffic signals, and gridlock detection prevents intersection deadlocks
**Depends on**: Phase 1
**Requirements**: NET-01, NET-02, NET-03, NET-04, VEH-01, VEH-02, DEM-01, DEM-02, DEM-03, GRID-01
**Success Criteria** (what must be TRUE):
  1. An OSM PBF file for a small HCMC area loads into a directed graph with lane counts, speed limits, and one-way rules, and R-tree spatial queries return correct neighbors
  2. A car agent following a leader decelerates smoothly to a stop without negative velocity, including the ballistic stopping guard edge case
  3. A car agent evaluates lane-change via MOBIL and executes when benefit exceeds politeness threshold (0.3)
  4. Traffic signals cycle through green/amber/red phases and agents stop at red lights
  5. Agents spawn from OD matrices shaped by time-of-day profiles with correct vehicle type distribution (80% motorbike, 15% car, 5% bus)
  6. Gridlock detection identifies circular waiting (speed=0 for >300s) and resolves via configured strategy (teleport/reroute/signal override)
**Plans**: TBD

Plans:
- [ ] 02-01: TBD
- [ ] 02-02: TBD
- [ ] 02-03: TBD

### Phase 3: Motorbike Sublane & Pedestrian & Bicycle
**Goal**: Motorbikes move with continuous lateral positioning (the core differentiator), pedestrians move via social force with jaywalking, and bicycles occupy rightmost sublane positions
**Depends on**: Phase 2
**Requirements**: VEH-03, VEH-04, VEH-05
**Success Criteria** (what must be TRUE):
  1. A motorbike agent filters between two car agents using continuous lateral position (FixedQ8_8), with behavior consistent across different timestep sizes (dt=0.05s, 0.1s, 0.2s)
  2. Motorbikes swarm and cluster at red lights in front of cars, then disperse on green
  3. Pedestrian agents repel each other via social force with density-adaptive GPU workgroups, and jaywalking occurs at configured probability (0.3)
  4. Bicycle agents maintain rightmost sublane position at v0=15km/h without filtering behavior
**Plans**: TBD

Plans:
- [ ] 03-01: TBD
- [ ] 03-02: TBD

### Phase 4: Routing & Prediction & Meso-Micro
**Goal**: Agents route via CCH pathfinding with dynamic weight updates driven by a prediction ensemble, and distant areas use mesoscopic queue simulation with smooth transitions
**Depends on**: Phase 3
**Requirements**: RTE-01, RTE-02, RTE-03, MESO-01, MESO-02
**Success Criteria** (what must be TRUE):
  1. CCH shortest-path queries return correct routes verified against Dijkstra ground truth, with weight customization completing within 3ms target
  2. Agents reroute when congestion changes travel times, using prediction ensemble (BPR + ETS + historical) for future travel time estimates
  3. Mesoscopic queue model simulates distant links, and agents transition smoothly through the 100m graduated buffer zone without phantom wave artifacts
**Plans**: TBD

Plans:
- [ ] 04-01: TBD
- [ ] 04-02: TBD
- [ ] 04-03: TBD

### Phase 5: Desktop Application
**Goal**: A native macOS winit+egui application displays the running simulation with 2D agent visualization via wgpu and provides interactive controls via egui panels
**Depends on**: Phase 4
**Requirements**: APP-01, APP-02, APP-03, APP-04
**Success Criteria** (what must be TRUE):
  1. A winit window opens on macOS with a wgpu render surface displaying the road network and moving agents
  2. The 2D top-down view shows motorbikes, cars, pedestrians, and bicycles as distinct colored shapes moving along roads in real-time
  3. Start, stop, pause, speed adjustment, and reset commands from the egui UI invoke simulation engine methods directly and take effect immediately
  4. egui dashboard panels display real-time simulation metrics (frame time, agent count, throughput) and provide control widgets
**Plans**: TBD

Plans:
- [ ] 05-01: TBD
- [ ] 05-02: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3 -> 4 -> 5

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. GPU Foundation & Spikes | 0/3 | Planning complete | - |
| 2. Road Network & Core Vehicle Models | 0/3 | Not started | - |
| 3. Motorbike Sublane & Pedestrian & Bicycle | 0/2 | Not started | - |
| 4. Routing & Prediction & Meso-Micro | 0/3 | Not started | - |
| 5. Desktop Application | 0/2 | Not started | - |
