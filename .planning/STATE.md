---
gsd_state_version: 1.0
milestone: v1.1
milestone_name: SUMO Replacement Engine
status: completed
stopped_at: Completed 06-07-PLAN.md
last_updated: "2026-03-07T14:49:55.713Z"
last_activity: 2026-03-07 -- Completed Plan 06-07 (Mesoscopic queue model)
progress:
  total_phases: 3
  completed_phases: 1
  total_plans: 13
  completed_plans: 11
  percent: 85
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models
**Current focus:** Phase 6 -- Agent Models & Signal Control

## Current Position

Phase: 6 of 7 (Agent Models & Signal Control) -- IN PROGRESS
Plan: 07 complete -- Mesoscopic queue model
Status: Plan 06-07 complete, phase 6 complete
Last activity: 2026-03-07 -- Completed Plan 06-07 (Mesoscopic queue model)

Progress: [█████████░] 85%

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

### Pending Todos

None.

### Blockers/Concerns

- ~~cf_model differentiation gap~~ RESOLVED in 05-06: CarFollowingModel now attached at spawn; GPU shader confirmed producing 92.8% speed difference between Krauss and IDM agents.
- No Rust CCH crate exists -- Phase 7 requires custom implementation (2-3 weeks estimated)
- Meso-micro hybrid (AGT-05/AGT-06) may be unnecessary if full-micro handles 280K within 15ms frame time
- Multi-GPU boundary protocol validated with logical partitions; physical multi-adapter untested (wgpu Spike S2 still needed)

## Session Continuity

Last session: 2026-03-07T14:49:49.865Z
Stopped at: Completed 06-07-PLAN.md
Resume file: None
