# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 -- MVP

**Shipped:** 2026-03-07
**Phases:** 4 | **Plans:** 11 | **Timeline:** 2 days

### What Was Built
- GPU compute pipeline (wgpu/Metal) with ECS round-trip and instanced 2D rendering
- HCMC District 1 road network from OSM PBF with spatial index, A* routing, traffic signals
- IDM car-following + MOBIL lane-change with gradual drift animation
- Motorbike sublane model with continuous lateral positioning and red-light swarming
- Pedestrian social force (Helbing) with jaywalking and cross-type collision avoidance
- egui dashboard with simulation controls and real-time metrics

### What Worked
- Pure-function math models (IDM, MOBIL, social force) enabled TDD: write tests first, then implement, zero integration surprises
- Visual rendering from Phase 1 gave immediate feedback on every subsequent phase -- bugs visible instantly
- 700-line file limit forced clean module extraction (sim.rs split into 5 files) without over-engineering
- Probe-based gap scanning and obstacle-edge sweep algorithms worked first try due to clear mathematical spec
- Phase 4 gap-closure audit caught MOBIL not wired into sim loop and documentation staleness before shipping

### What Was Inefficient
- Phase 3 Plan 02 (integration) took 42min vs 5-12min for other plans -- wiring sublane + social force + spatial index into SimWorld required extensive debugging of interaction edge cases
- GPU compute pipeline proven in Phase 1 but not used in main sim loop -- CPU-side ECS sufficient at 1.5K agents, making Phase 1 GPU work partially stranded
- MOBIL was implemented as pure function in Phase 2 but not wired until Phase 4 -- could have been caught earlier with integration testing

### Patterns Established
- Pure-function models with dedicated test suites before integration wiring
- AgentSnapshot as unified query type for all spatial lookups (cross-type)
- Heading-based filtering in spatial queries to prevent head-on deadlocks
- Spatial query radius caps (6m/20 neighbors for motorbikes, 3m for pedestrians) for performance
- Linear drift interpolation for lane changes (simple, visually smooth)

### Key Lessons
1. Wire models into the sim loop in the same phase they're implemented -- don't defer integration to a later phase
2. Spatial query performance degrades non-linearly above 800 agents -- cap neighbors early, not after observing jams
3. Milestone audits before completion catch real gaps (MOBIL wiring, doc staleness) -- worth the 10-minute investment
4. egui + wgpu on the same surface works well for native Rust apps -- no webview conflicts, simpler than Tauri

### Cost Observations
- Model mix: 100% opus (all phases)
- Sessions: ~10 sessions across 2 days
- Notable: Plans averaged 5-12 minutes each (excluding Phase 3 P02 outlier at 42min)

---

## Milestone: v1.1 -- SUMO Replacement Engine

**Shipped:** 2026-03-09
**Phases:** 11 | **Plans:** 39 | **Timeline:** 3 days

### What Was Built
- GPU-first physics with per-lane wave-front dispatch at 280K-agent scale with multi-GPU partitioning
- Fixed-point arithmetic (Q16.16/Q12.20/Q8.8) for cross-GPU determinism
- All 7 vehicle types with HCMC-calibrated behavior params from TOML config
- CCH pathfinding with 8 agent profiles and BPR+ETS+historical prediction ensemble
- GPU perception + evaluation pipeline for autonomous agent decisions
- Actuated/adaptive signal control, SPaT/GLOSA, V2I communication
- HCMC-specific behaviors: red-light creep, aggressive weaving, yield-based gap acceptance
- Bus dwell lifecycle with GTFS import for 130 HCMC routes
- Meso-micro hybrid with 100m buffer zones
- SUMO .net.xml/.rou.xml import compatibility
- Full 10-step frame pipeline integration (Phases 9-15)

### What Worked
- Coarse granularity (3 feature phases + 8 integration phases) kept momentum -- feature phases built all modules, integration phases wired them
- Milestone audit (twice!) caught real gaps: unwired bus stops, oversized files, stale tracking docs -- all fixed in Phases 14-15
- TOML config externalization in Phase 8 paid off immediately -- GPU uniform buffer populated from same config, no param mismatch
- BFS balanced bisection fallback when METIS failed on macOS -- pragmatic workaround that validated the partitioning protocol
- CSV GTFS parser instead of gtfs-structures crate handled non-standard HCMC data without fighting the library

### What Was Inefficient
- Phases 9-15 were all integration/wiring phases -- core modules built in Phases 5-8 but not wired into sim loop. Same v1.0 lesson: wire models into sim loop in the same phase they're implemented
- Phase 13 SC7 (remove unused GPU buffers) was planned but research showed both were actively used -- wasted planning effort on assumptions
- Multiple dirty-flag and buffer-upload optimizations scattered across phases 11-13 could have been a single coherent pass
- Phase 15 (housekeeping) was entirely tech debt from earlier phases not maintaining file size limits

### Patterns Established
- GPU uniform buffer pattern: TOML -> Rust struct -> bytemuck -> GPU binding for any vehicle-type-specific params
- Dirty-flag GPU buffer uploads: only transfer when state changes, not every frame
- Polymorphic signal controllers via Box<dyn SignalController> trait dispatch
- sim_*.rs submodule extraction pattern for sim.rs file size compliance
- PerceptionBindings struct to group related buffer references (avoids clippy too-many-args)
- CPU reference functions kept alongside GPU for validation (cpu_reference module)

### Key Lessons
1. **Wire integration in the same phase as implementation** -- confirmed across v1.0 and v1.1. 7 of 11 phases were pure wiring/integration
2. **Audit before milestone completion is essential** -- caught GTFS→bus_stops not connected, files over 700 lines, stale docs
3. **GPU/CPU parity requires explicit design** -- uniform buffers, matching struct layouts, dirty flags. Can't be an afterthought
4. **Config-driven params > hardcoded constants** -- Phase 8 TOML externalization eliminated all GPU/CPU param mismatches
5. **BFS fallback patterns work** -- when METIS and CCH crates don't exist for Rust, custom implementations with BFS ordering are viable

### Cost Observations
- Model mix: ~80% opus, ~20% sonnet (routine integration tasks)
- Sessions: ~25 sessions across 3 days
- Notable: Integration phases (9-15) averaged faster per-plan than feature phases (5-8) due to established patterns

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Timeline | Phases | Plans | Key Change |
|-----------|----------|--------|-------|------------|
| v1.0 | 2 days | 4 | 11 | Initial process -- pure-function TDD + visual verification |
| v1.1 | 3 days | 11 | 39 | Coarse feature phases + dedicated integration phases + milestone audit |

### Cumulative Quality

| Milestone | LOC (Rust+WGSL) | Crates | Requirements |
|-----------|-----------------|--------|-------------|
| v1.0 | 7,919 | 6 | 25/25 |
| v1.1 | 33,281 | ~12 | 45/45 |

### Top Lessons (Verified Across Milestones)

1. **Wire integration in the same phase as implementation** -- v1.0 MOBIL was deferred; v1.1 had 7 integration phases. Both confirmed: deferred wiring creates compounding integration debt
2. **Milestone audits catch real gaps** -- v1.0 audit caught unwired MOBIL + stale docs; v1.1 audit (run twice) caught unwired bus stops, oversized files, stale tracking. Always audit before shipping
3. **Pure-function models with test suites first** -- TDD on math models (IDM, MOBIL, social force, CCH, BPR) had zero integration surprises in both milestones
