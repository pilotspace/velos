# Roadmap: VELOS

## Overview

VELOS validates GPU-accelerated traffic microsimulation on macOS Apple Silicon through three phases: prove the GPU compute and rendering pipeline works with visual feedback (spikes), build the road network with core vehicle models and egui controls, then add the motorbike sublane differentiator alongside pedestrians.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: GPU Pipeline & Visual Proof** - Validate wgpu/Metal compute pipeline, f32 shaders, ECS-to-GPU round-trip, winit window with GPU-instanced dot/triangle renderer, benchmarks (completed 2026-03-06)
- [x] **Phase 2: Road Network & Vehicle Models + egui** - Build HCMC road graph, IDM car-following, MOBIL lane-change, signals, A* routing, demand spawning, gridlock detection, egui dashboard and controls (completed 2026-03-07)
- [x] **Phase 3: Motorbike Sublane & Pedestrians** - Continuous lateral positioning, basic social force pedestrians, mixed-traffic interactions (filtering, clustering) (completed 2026-03-07)
- [ ] **Phase 4: MOBIL Wiring + Motorbike Jam Fix + Performance** - Wire MOBIL lane-change into sim loop, fix motorbike traffic jam clustering, optimize spatial query performance at 800+ agents, create Phase 2 VERIFICATION.md, fix documentation gaps

## Phase Details

### Phase 1: GPU Pipeline & Visual Proof
**Goal**: A proven wgpu/Metal compute pipeline dispatches f32 agent updates, round-trips ECS data through GPU buffers, and renders 1K agents as styled instanced shapes in a winit window with zoom/pan
**Depends on**: Nothing (first phase)
**Requirements**: GPU-01, GPU-02, GPU-03, GPU-04, REN-01, REN-02, REN-03, REN-04, PERF-01, PERF-02
**Success Criteria** (what must be TRUE):
  1. A wgpu compute shader dispatches on Metal and writes agent position/velocity data back to CPU-readable buffers
  2. f32 arithmetic in WGSL produces results matching f64 CPU code within acceptable tolerance for simulation ranges
  3. hecs entities project into SoA GPU buffers and read back correctly after a compute dispatch round-trip with 1K agents
  4. A winit window opens on macOS displaying 1K agents as GPU-instanced styled shapes (triangles/dots) moving on screen
  5. Zoom and pan camera controls work
  6. Frame time for 1K agents < 16ms (60 FPS target)
**Plans**: 2 plans

Plans:
- [ ] 01-01-PLAN.md -- Workspace bootstrap + GPU compute pipeline + ECS round-trip (GO/NO-GO gate)
- [ ] 01-02-PLAN.md -- winit window + instanced renderer + camera (depends on 01-01)

### Phase 2: Road Network & Vehicle Models + egui
**Goal**: Cars spawn from OD matrices onto a real HCMC road network, follow IDM car-following, change lanes via MOBIL, obey traffic signals, route via A*, and gridlock detection prevents intersection deadlocks. egui provides simulation controls and dashboard.
**Depends on**: Phase 1
**Requirements**: VEH-01, VEH-02, NET-01, NET-02, NET-03, NET-04, RTE-01, DEM-01, DEM-02, DEM-03, GRID-01, APP-01, APP-02
**Success Criteria** (what must be TRUE):
  1. An OSM PBF file for a small HCMC area loads into a directed graph with lane counts, speed limits, and one-way rules, and R-tree spatial queries return correct neighbors
  2. A car agent following a leader decelerates smoothly to a stop without negative velocity, including the ballistic stopping guard edge case
  3. A car agent evaluates lane-change via MOBIL and executes when benefit exceeds politeness threshold (0.3)
  4. Traffic signals cycle through green/amber/red phases and agents stop at red lights
  5. Agents spawn from OD matrices shaped by time-of-day profiles with correct vehicle type distribution (80% motorbike, 15% car, 5% pedestrian)
  6. A* pathfinding assigns routes to spawned agents
  7. Gridlock detection identifies circular waiting (speed=0 for >300s) and resolves via configured strategy
  8. egui controls (start/stop/pause/speed/reset) invoke simulation methods and take effect immediately
  9. egui dashboard displays real-time metrics (frame time, agent count, throughput)
  10. Agents render as styled shapes on visible road lanes with direction arrows
**Plans**: 4 plans

Plans:
- [x] 02-01-PLAN.md -- velos-net crate: OSM import, projection, road graph, spatial index, A* routing
- [x] 02-02-PLAN.md -- velos-vehicle + velos-signal: IDM, MOBIL, gridlock, traffic signals
- [x] 02-03-PLAN.md -- velos-demand: OD matrix, time-of-day profiles, agent spawner
- [x] 02-04-PLAN.md -- Integration: wgpu downgrade, per-type rendering, wire subsystems, egui sidebar

### Phase 3: Motorbike Sublane & Pedestrians
**Goal**: Motorbikes move with continuous lateral positioning (the core differentiator) and pedestrians move via Helbing social force with jaywalking, with cross-type collision avoidance at intersections
**Depends on**: Phase 2
**Requirements**: VEH-03, VEH-04
**Success Criteria** (what must be TRUE):
  1. A motorbike agent filters between two car agents using continuous lateral position, with behavior consistent across different timestep sizes (dt=0.05s, 0.1s, 0.2s)
  2. Motorbikes swarm and cluster at red lights in front of cars, then disperse on green
  3. Pedestrian agents repel each other via basic social force, and jaywalking occurs at configured probability (0.3)
  4. Mixed traffic (motorbikes, cars, pedestrians) interacts correctly at intersections
**Plans**: 2 plans

Plans:
- [x] 03-01-PLAN.md -- Sublane model + social force model: pure functions with tests (LateralOffset, gap-seeking, Helbing model)
- [x] 03-02-PLAN.md -- Integration: wire models into SimWorld tick loop, spatial index, swarming color, visual verification

### Phase 4: MOBIL Wiring + Motorbike Jam Fix + Performance
**Goal**: Wire the MOBIL lane-change model into the simulation loop, fix motorbike traffic jam/clustering at intersections, optimize spatial query performance for 800+ agents, create formal Phase 2 VERIFICATION.md, and fix documentation staleness
**Depends on**: Phase 2, Phase 3
**Requirements**: VEH-02 (re-verify), APP-01 (doc fix), APP-02 (doc fix)
**Gap Closure:** Closes gaps from v1.0 milestone audit + fixes motorbike jam + performance regression
**Success Criteria** (what must be TRUE):
  1. `mobil_decision()` is called in the sim loop and cars change lanes when MOBIL benefit exceeds politeness threshold
  2. Motorbikes flow through intersections without permanent clustering/jamming at 800+ agents
  3. Frame time < 33ms (30 FPS) at 1000 agents on Metal
  4. Phase 2 VERIFICATION.md exists with pass/fail for all 13 Phase 2 requirements
  5. APP-01 and APP-02 marked Complete in REQUIREMENTS.md traceability table
  6. Nyquist validation passes for Phases 2 and 3
**Plans**: 3 plans

Plans:
- [ ] 04-01-PLAN.md -- Wire MOBIL into sim loop with gradual drift
- [ ] 04-02-PLAN.md -- Fix motorbike jam + optimize spatial query performance
- [ ] 04-03-PLAN.md -- Phase 2 verification, documentation fixes

## Progress

**Execution Order:**
Phases execute in numeric order: 1 -> 2 -> 3

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. GPU Pipeline & Visual Proof | 2/2 | Complete   | 2026-03-06 |
| 2. Road Network & Vehicle Models + egui | 4/4 | Complete   | 2026-03-07 |
| 3. Motorbike Sublane & Pedestrians | 2/2 | Complete   | 2026-03-07 |
| 4. MOBIL Wiring + Motorbike Jam Fix + Performance | 0/3 | Pending | - |
