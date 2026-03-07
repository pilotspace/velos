---
gsd_state_version: 1.0
milestone: v1.1
milestone_name: SUMO Replacement Engine
status: in-progress
stopped_at: Completed 06-01-PLAN.md
last_updated: "2026-03-07T14:27:00.000Z"
last_activity: 2026-03-07 -- Completed Plan 06-01 (Buffer layout and vehicle types)
progress:
  total_phases: 3
  completed_phases: 1
  total_plans: 7
  completed_plans: 7
  percent: 38
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-07)

**Core value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models
**Current focus:** Phase 6 -- Agent Models & Signal Control

## Current Position

Phase: 6 of 7 (Agent Models & Signal Control) -- IN PROGRESS
Plan: 01 complete -- Buffer layout and vehicle types
Status: Plan 06-01 complete, ready for Plan 06-02
Last activity: 2026-03-07 -- Completed Plan 06-01 (Buffer layout and vehicle types)

Progress: [████------] 38%

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

### Pending Todos

None.

### Blockers/Concerns

- ~~cf_model differentiation gap~~ RESOLVED in 05-06: CarFollowingModel now attached at spawn; GPU shader confirmed producing 92.8% speed difference between Krauss and IDM agents.
- No Rust CCH crate exists -- Phase 7 requires custom implementation (2-3 weeks estimated)
- Meso-micro hybrid (AGT-05/AGT-06) may be unnecessary if full-micro handles 280K within 15ms frame time
- Multi-GPU boundary protocol validated with logical partitions; physical multi-adapter untested (wgpu Spike S2 still needed)

## Session Continuity

Last session: 2026-03-07T14:27:00.000Z
Stopped at: Completed 06-01-PLAN.md
Resume file: .planning/phases/06-agent-models-signal-control/06-01-SUMMARY.md
