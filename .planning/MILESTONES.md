# Milestones

## v1.1 SUMO Replacement Engine (Shipped: 2026-03-08)

**Delivered:** Full SUMO-replacement simulation engine with GPU-first physics, 7 vehicle types, intelligent routing, and HCMC-calibrated mixed traffic behavior at 280K-agent scale

**Phases completed:** 11 phases (5-15), 39 plans
**Timeline:** 3 days (2026-03-06 to 2026-03-09)
**Codebase:** 31,780 Rust LOC + 1,501 WGSL LOC | 33,281 total
**Git range:** 168 commits, feat(05-01) to feat(sim): bus line color-coding

**Key accomplishments:**
1. GPU-first simulation at scale — 280K agents on GPU compute with per-lane wave-front dispatch, multi-GPU partitioning (METIS), and fixed-point arithmetic (Q16.16/Q12.20/Q8.8) for cross-GPU determinism
2. Complete vehicle type coverage — All 7 agent types (motorbike, car, bus, truck, bicycle, emergency, pedestrian) with HCMC-calibrated behavior loaded from TOML config
3. Intelligent routing & prediction — CCH pathfinding with 8 agent profiles, BPR+ETS+historical prediction ensemble, staggered reroute evaluation, GPU perception/evaluation pipeline
4. HCMC-realistic mixed traffic — Red-light creep, aggressive weaving with speed-dependent gaps, yield-based intersection negotiation, motorbike-native sublane model
5. Full sim loop integration — 10-step frame pipeline: perception, reroute, polymorphic signals, bus dwell lifecycle, meso-micro hybrid zones, dirty-flag GPU transfer optimization
6. SUMO compatibility + GTFS — .net.xml/.rou.xml import, 130 HCMC bus routes with R-tree stop snapping and bus dwell lifecycle

**Tech debt (2 items, all low severity):**
- sublane.rs constants (CREEP_MAX_SPEED, CREEP_DISTANCE_SCALE, GAP_SPEED_COEFF) could be wired to config — values match config defaults
- Phase 13 SC7 dropped: congestion grid buffer and acceleration field confirmed actively used in GPU shaders

---

## v1.0 MVP (Shipped: 2026-03-07)

**Delivered:** GPU-accelerated traffic microsimulation POC with motorbike sublane model running on macOS Metal

**Phases completed:** 4 phases, 11 plans
**Timeline:** 2 days (2026-03-06 to 2026-03-07)
**Codebase:** 7,802 Rust LOC + 117 WGSL LOC | 185 tests passing
**Git range:** 57 commits, feat(01-01) to docs(04-03)

**Key accomplishments:**
1. wgpu/Metal GPU compute pipeline with ECS round-trip, instanced 2D rendering at 60 FPS
2. HCMC District 1 road network from OSM PBF with R-tree spatial index, A* routing, traffic signals
3. IDM car-following + MOBIL lane-change with smooth 2-second gradual drift animation
4. Motorbike sublane model with continuous lateral positioning, red-light swarming/dispersal
5. Pedestrian social force model (Helbing) with jaywalking and cross-type collision avoidance
6. egui dashboard with simulation controls (start/stop/pause/speed/reset) and real-time metrics

**Tech debt (7 items, all low severity):**
- GPU compute proven but not in main sim loop (CPU ECS sufficient at 1.5K agents)
- `should_jaywalk()` tested but not wired into sim loop
- `GridlockDetector` struct unused (free function used directly)
- No VALIDATION.md for Phase 4
- See `.planning/milestones/v1.0-MILESTONE-AUDIT.md` for full details

---

