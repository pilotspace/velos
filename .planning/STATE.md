---
gsd_state_version: 1.0
milestone: v1.1
milestone_name: SUMO Replacement Engine
status: in-progress
stopped_at: Completed 14-02-PLAN.md
last_updated: "2026-03-08T15:15:03Z"
last_activity: 2026-03-08 -- Completed Plan 14-02 (SimWorld GTFS Integration)
progress:
  total_phases: 13
  completed_phases: 9
  total_plans: 36
  completed_plans: 36
  percent: 100
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models
**Current focus:** Phase 14 -- Wire GTFS Bus Stops Pipeline

## Current Position

Phase: 14 of 15 (Wire GTFS Bus Stops Pipeline)
Plan: 02 of 02 complete -- 02 (SimWorld GTFS Integration)
Status: Phase 14 complete
Last activity: 2026-03-08 -- Completed Plan 14-02 (SimWorld GTFS Integration)

Progress: [██████████] 100%

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [v1.1 Pivot]: Milestone renamed from "Digital Twin Platform" to "SUMO Replacement Engine" -- no web platform, no data exports, no calibration, no Docker/monitoring
- [v1.1 Roadmap]: Coarse granularity -- 3 phases (5-7) covering 39 requirements across GPU engine, agents/signals, and intelligence/routing
- [v1.1 Roadmap]: Phases are strictly sequential (5 -> 6 -> 7) -- intelligence/routing needs agent models and signals to exist first
- [v1.1 Roadmap]: egui desktop app retained for dev visualization -- no web dashboard this milestone
- [05-01]: Fixed-point types use wrapping arithmetic to match GPU u32 wrapping semantics
- [05-01]: CarFollowingModel enum variant named Idm (not IDM) per Rust naming conventions
- [05-01]: GpuAgentState acceleration uses Q12.20 (same as speed) for consistency
- [05-03]: Streaming XML (quick-xml) for memory-efficient SUMO network parsing
- [05-03]: SUMO amber phases merged into preceding green phase for velos-signal compatibility
- [05-03]: RoadClass extended with Motorway, Trunk, Service for SUMO edge type coverage
- [05-02]: postcard for binary graph serialization (compact, serde-native)
- [05-02]: Service roads heuristically tagged motorbike-only (HCMC alleys)
- [05-02]: Base OD matrix ~140K/hr scales to ~280K via ToD peak factor ~2.0x
- [05-04]: f32 intermediates for GPU physics, fixed-point only for position/speed storage
- [05-04]: tick_gpu() as production method, tick() as CPU fallback for tests
- [05-04]: CPU reference functions kept in cpu_reference module for ongoing GPU validation
- [05-05]: BFS-based balanced bisection fallback for METIS (libmetis vendored build fails on macOS)
- [05-05]: Logical partitions on single GPU validate boundary protocol without multi-adapter
- [05-05]: PartitionMode enum (Single/Multi) preserves backward compatibility on SimWorld
- [05-06]: RNG-based 30/70 Krauss/IDM assignment for cars; demand-config-driven assignment deferred to Phase 6
- [05-06]: Motorbikes always IDM (sublane model is IDM-based); pedestrians excluded from CarFollowingModel
- [06-01]: GpuAgentState expanded to 40 bytes with vehicle_type (u32) and flags (u32) fields
- [06-01]: VehicleType extended to 7 variants: Motorbike, Car, Bus, Bicycle, Truck, Emergency, Pedestrian
- [06-01]: Bicycle uses sublane model (like Motorbike); Bus/Truck/Emergency use lane-based (like Car)
- [06-01]: VehicleType enum order = GPU u32 mapping (0=Motorbike..6=Pedestrian)
- [06-02]: CSV-native GTFS parser instead of gtfs-structures crate -- avoids heavy dependency, handles non-standard HCMC data
- [06-02]: Bus stop proximity threshold of 5m for should_stop detection
- [06-02]: Passenger counts caller-provided (stochastic via RNG), not generated inside BusState
- [06-03]: SignalController trait takes &[DetectorReading] in tick() -- fixed-time ignores, actuated consumes
- [06-03]: ActuatedController uses explicit amber state machine for precise gap-out control
- [06-03]: AdaptiveController redistributes green only at cycle boundaries, not mid-cycle
- [06-03]: LoopDetector uses strict prev < offset <= cur for forward-only crossing detection
- [06-04]: Emergency yield cone: 50m range, 90-degree cone (45-degree half-angle) for siren detection
- [06-04]: emergency_count replaces _pad in WaveFrontParams -- GPU shader early-exits when 0
- [06-04]: EmergencyVehicle buffer at binding 5, max 16 entries, pre-allocated
- [06-07]: BPR beta fast-path multiplication for beta=4.0, powf fallback for non-standard values
- [06-07]: ZoneConfig defaults unconfigured edges to Micro (safe default: full simulation)
- [06-07]: BufferZone::should_insert uses static thresholds (100m distance, 2.0 m/s speed diff)
- [06-07]: smoothstep (3x^2-2x^3) for C1-continuous buffer zone IDM interpolation
- [Phase 06]: BPR beta fast-path multiplication for beta=4.0, powf fallback for non-standard
- [Phase 06]: [06-05]: GLOSA minimum practical speed 3.0 m/s -- below this agent stops and waits
- [Phase 06]: [06-05]: School zone time-window enforcement on CPU; GPU always applies reduced speed for signs in buffer
- [Phase 06]: [06-05]: Sign buffer at binding 6, WaveFrontParams extended to 32 bytes with sign_count and sim_time
- [06-06]: Separate PedestrianAdaptivePipeline module (ped_adaptive.rs) for file size compliance
- [06-06]: Hillis-Steele reduce-then-scan for portable multi-workgroup prefix sum on Metal/Vulkan
- [06-06]: Workgroup size 256 for compute passes, 64 for social force (per architecture doc)
- [06-06]: bgl_entry made pub(crate) for cross-module pipeline sharing
- [Phase 06]: [06-06]: Separate PedestrianAdaptivePipeline module for file size compliance
- [Phase 06]: [06-06]: Hillis-Steele reduce-then-scan for portable multi-workgroup prefix sum
- [07-01]: Undirected adjacency view of directed graph for CCH ordering and contraction
- [07-01]: BFS balanced bisection with peripheral node start (reuses Phase 5 METIS fallback pattern)
- [07-01]: CSR format with separate forward/backward stars indexed by rank for CCH
- [07-01]: Cache invalidation based on node_count + edge_count comparison
- [07-02]: RoadClass duplicated in cost.rs to avoid velos-core -> velos-net circular dependency
- [07-02]: r#gen() syntax for Rust 2024 edition (gen is reserved keyword)
- [07-02]: Task 3 (EdgeAttributes heuristics) merged into Task 1 since same file and natural cohesion
- [07-04]: PredictionInput struct to avoid clippy too-many-arguments (8 params -> struct)
- [07-04]: Inverse-error softmax for adaptive weights with min floor 0.05
- [07-04]: Historical matcher flat Vec with 96 slots/edge (24h * 4 day_types) for O(1) lookup
- [07-04]: Confidence = 1 - (range/mean) normalized inter-model disagreement
- [07-03]: Fixed topology.rs original_edge_to_cch mapping (CSR sort invalidated pre-sort indices)
- [07-03]: Binary search for O(log d) edge lookup in triangle enumeration inner loop
- [07-03]: Symmetric weight model: forward_weight == backward_weight for both search directions
- [07-03]: Both forward and backward Dijkstra searches use forward star (both go upward in hierarchy)
- [07-05]: PerceptionBindings struct groups 6 buffer refs to satisfy clippy too-many-arguments
- [07-05]: Linear agent scan for leader detection (acceptable for 1-20 agents per edge)
- [07-05]: Signal state indexed by edge_id (simplified one-signal-per-edge model)
- [07-05]: Separate bind group layout from wave_front to avoid binding conflicts
- [07-06]: PerceptionSnapshot in velos-core avoids circular dependency on velos-gpu PerceptionResult
- [07-06]: RouteEvalContext struct decouples evaluate_reroute from CCH/ECS for pure-logic testability
- [07-06]: EdgeNodeMap separate from CCHRouter (CCH topology is graph-independent)
- [07-06]: sim_reroute.rs follows existing SimWorld split pattern for file size compliance
- [08-01]: Truck v0 changed from 25.0 m/s (90 km/h) to 9.7 m/s (35 km/h) for HCMC urban
- [08-01]: Car v0 changed from 13.9 m/s (50 km/h) to 9.7 m/s (35 km/h) for HCMC congestion
- [08-01]: Motorbike t_headway reduced from 1.0s to 0.8s for aggressive HCMC following
- [08-01]: VehicleConfig::default() hardcoded fallback matches TOML file for resilience
- [08-01]: SublaneParams default min_filter_gap 0.6->0.5m, max_lateral_speed 1.0->1.2 m/s
- [08-03]: Red-light creep limited to motorbike/bicycle with 0.3 m/s max, 5m ramp distance
- [08-03]: Speed-dependent gap widening: effective_gap = base + 0.1 * |delta_v|
- [08-03]: Size intimidation factors: truck/bus=1.3x, emergency=2.0x, motorbike/bicycle=0.8x
- [08-03]: Forced acceptance after 5s wait (threshold halved) prevents intersection deadlock
- [Phase 08]: [08-02]: KRAUSS_TAU kept as WGSL const (1.0s) -- reaction time is physics, not vehicle-type-specific
- [Phase 08]: [08-02]: 8 f32 per vehicle type in GPU uniform buffer: v0, s0, t_headway, a, b, krauss_accel, krauss_decel, krauss_sigma
- [09-01]: Polymorphic signal controllers via Box<dyn SignalController> -- trait dispatch for fixed/actuated/adaptive
- [09-01]: SimWorld::new() takes device/queue/dispatcher -- GPU init at construction, not deferred
- [09-01]: SimWorld::new_cpu_only() for test paths without GPU device
- [09-01]: Speed limit signs auto-generated from edge speed_limit_mps at startup
- [09-01]: PerceptionPipeline 300K max agents (7% headroom over 280K target)
- [09-02]: PerceptionResult WGSL field named perc_flags (not flags) to avoid collision with AgentState.flags
- [09-02]: Placeholder perception_result_buffer pre-allocated 300K agents (9.6 MB zeroed) -- avoids Option complexity
- [09-02]: Gap acceptance defaults VT_CAR for unknown leader type -- neutral size_factor, full type-aware logic on CPU
- [09-02]: naga dev-dependency for automated WGSL parse validation in unit tests
- [09-03]: Loop detector prev_pos approximation uses speed * 0.1s backward estimate (avoids HashMap storage)
- [09-03]: Signal priority proximity threshold 100m from intersection on incoming edge
- [09-03]: PerceptionBuffers pre-allocated at startup with zeroed congestion grid (20x20, 500m cells)
- [09-03]: step_pedestrians extracted to sim_pedestrians.rs for sim.rs 700-line compliance
- [10-01]: Bus dwell runs on CPU (state machine), GPU reads FLAG_BUS_DWELLING to hold speed=0
- [10-01]: step_bus_dwell() inserted after vehicle physics, before pedestrian physics in pipeline
- [10-01]: Stochastic passenger counts (uniform RNG) sufficient for engine proof; GTFS deferred
- [10-02]: MesoAgentState preserves Route, VehicleType, IdmParams, CarFollowingModel, LateralOffset across zone transitions
- [10-02]: Gap check 10m threshold prevents micro edge insertion when congested; blocked vehicles re-enqueue
- [10-02]: step_meso() runs BEFORE step_vehicles_gpu (meso agents ready for buffer zone insertion before micro physics)
- [Phase 11]: [11-01]: ComputeDispatcher owns shared perception buffer; PerceptionPipeline receives references via PerceptionBindings and readback_results parameter
- [Phase 11]: [11-02]: compute_agent_flags() extracted as public pure function for testable flag bitfield computation
- [12-01]: GpuVehicleParams extended from 8 to 12 floats per vehicle type (224->336 bytes): +creep_max_speed, creep_distance_scale, creep_min_distance, gap_acceptance_ttc
- [12-02]: step_lane_changes at step 6.7 (before GPU physics) -- MOBIL decisions in ECS before GPU upload
- [12-02]: step_motorbikes_sublane at step 7.5 (after GPU readback) -- needs updated positions for neighbor gaps
- [12-02]: step_prediction at step 8.5 (after bus dwell) -- edge flows fresh from vehicle physics
- [12-02]: Spatial index rebuilt post-GPU for sublane (snapshot_post + spatial_post) -- avoids stale gaps
- [12-01]: GAP_MAX_WAIT_TIME/GAP_FORCED_ACCEPTANCE_FACTOR/GAP_WAIT_REDUCTION_RATE kept as local lets (universal physics, not vehicle-specific)
- [12-01]: gap_acceptance_ttc replaces t_headway as base TTC threshold in WGSL intersection gap acceptance
- [13-01]: Option<&AgentProfile> ECS query with Commuter default -- backward compatible with entities spawned before this change
- [13-01]: step_glosa at step 4.5 (after signal priority, before perception) -- GLOSA-modified speeds captured by perception pipeline
- [13-01]: MockSignalController for GLOSA unit tests -- avoids dependency on full graph topology
- [13-02]: classify_density used for adaptive cell sizing (2m/5m/10m) instead of hardcoded 3m in GPU pedestrian dispatch
- [13-02]: 5m bounding box margin prevents degenerate zero-size grids when pedestrians are co-located
- [13-02]: CPU tick() step_pedestrians unchanged -- GPU path only in tick_gpu()
- [13-03]: CPU tick() step_lane_changes added at step 6.7 for MOBIL parity with GPU tick_gpu() pipeline
- [13-03]: Dirty-flag gated GPU buffer uploads: signal_dirty/prediction_dirty fields on SimWorld
- [13-03]: Phase transition detection via get_phase_state(0) comparison before/after ctrl.tick()
- [13-03]: Dirty flags initialized true to force initial upload on first frame
- [14-01]: BusSpawner accepts stop_id_to_index HashMap -- decouples snapping from spawner construction
- [14-01]: velos-net depends on velos-demand and velos-vehicle for snap_gtfs_stops high-level function
- [14-02]: Name-based stop_id_to_index mapping -- BusStop.name preserved from GtfsStop.name by snap_gtfs_stops
- [14-02]: GTFS loading after init_reroute() in SimWorld::new() -- ensures CCH available for future inter-stop routing

### Pending Todos

None.

### Roadmap Evolution

- Phase 8 added: tuning vehicle behavior to more realistic in HCM
- Phase 12 added: CPU Lane-Change & Sublane Wiring — MOBIL overtaking and motorbike lateral filtering in GPU tick loop
- Phase 13 added: Final Integration Wiring & GPU Transfer Audit — gap closure for remaining unwired subsystems

### Blockers/Concerns

- ~~cf_model differentiation gap~~ RESOLVED in 05-06: CarFollowingModel now attached at spawn; GPU shader confirmed producing 92.8% speed difference between Krauss and IDM agents.
- ~~No Rust CCH crate exists~~ RESOLVED in 07-01: Custom CCH implementation with nested dissection ordering and node contraction
- Meso-micro hybrid (AGT-05/AGT-06) may be unnecessary if full-micro handles 280K within 15ms frame time
- Multi-GPU boundary protocol validated with logical partitions; physical multi-adapter untested (wgpu Spike S2 still needed)

## Session Continuity

Last session: 2026-03-08T15:15:03Z
Stopped at: Completed 14-02-PLAN.md
Resume file: .planning/phases/14-wire-gtfs-bus-stops-pipeline/14-02-SUMMARY.md
