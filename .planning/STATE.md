---
gsd_state_version: 1.0
milestone: v1.2
milestone_name: Digital Twin
status: executing
stopped_at: 16-04 Task 3 checkpoint (human-verify)
last_updated: "2026-03-09T12:50:00Z"
last_activity: 2026-03-09 -- Phase 16 Plan 04 Tasks 1-2 complete (visualization + overlays), awaiting Task 3 human-verify
progress:
  total_phases: 5
  completed_phases: 0
  total_plans: 4
  completed_plans: 3
  percent: 87
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-09)

**Core value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models
**Current focus:** Phase 16 -- Intersection Sublane Model

## Current Position

Phase: 16 of 20 (Intersection Sublane Model)
Plan: 4 of 4 in current phase
Status: Checkpoint (human-verify)
Last activity: 2026-03-09 -- Phase 16 Plan 04 Tasks 1-2 complete, awaiting visual verification

Progress: [████████░░] 87% (v1.2)

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [16-04]: Vehicle-type coloring replaces car-following-model coloring for clearer visual identity
- [16-04]: Guide lines as quad strips (0.5m width) for cross-GPU consistency, not native lines
- [16-04]: Instance buffer capacity 300K (was 8192) to support POC agent count
- [16-02]: Local ConflictPoint struct in velos-vehicle to avoid circular dependency with velos-net
- [16-02]: VehicleType conversion function bridges velos-core and velos-vehicle enum types
- [16-02]: Junction entry blocked when foe within 0.3 t-distance of conflict crossing point
- [16-02]: advance_to_next_edge returns bool to propagate blocked state for anti-flicker
- [16-03]: 128-tile LRU cache with GPU buffer eviction for map tile memory management
- [16-03]: Background thread decode + main thread GPU upload via mpsc channel
- [16-03]: Skip label rendering; map polygons provide sufficient spatial context
- [16-01]: Filter pass-through nodes (in=1, out=1) from junction precomputation
- [16-01]: Minimum arc length 1.0m threshold to filter degenerate Bezier curves
- [16-01]: wait_ticks field on JunctionTraversal for deadlock prevention (MAX_YIELD_TICKS=100)
- [16-01]: exit_offset_m on BezierTurn (0.1m default) for edge-boundary safety
- [v1.2 rev2]: Intersection sublane model (Phase 16) is foundation -- simulation correctness before visualization
- [v1.2]: gRPC ingestion instead of built-in YOLO -- external CV pushes detections to VELOS
- [v1.2]: Phases 17 + 18 execute in parallel after Phase 16 (architecturally independent)
- [v1.2]: New Renderer3D crate (cannot retrofit existing 2D renderer)
- [v1.2]: OSM building extrusion via earcut (no external 3D datasets for HCMC)

### Pending Todos

- **[critical] Fix performance regression at 8K agents** — 83.4ms frame time (5x over budget). Profile junction HashMap lookups, build_instances iteration, map tile thread contention, N+1 ECS queries. File: `.planning/todos/pending/2026-03-09-fix-critical-performance-regression-at-8k-agents.md`

### Blockers/Concerns

- Building count for POC area unverified (estimated 80K-120K)
- wgpu version decision needed before Phase 18 (v27 current vs v28 available)
- Protobuf contract design needed before Phase 17 implementation

## Session Continuity

Last session: 2026-03-09T12:50:00Z
Stopped at: 16-04 Task 3 checkpoint (human-verify visual verification)
Resume file: .planning/phases/16-intersection-sublane-model/16-04-SUMMARY.md
