---
gsd_state_version: 1.0
milestone: v1.2
milestone_name: Digital Twin
status: in_progress
stopped_at: Completed 18-02-PLAN.md
last_updated: "2026-03-10T16:06:11Z"
last_activity: 2026-03-10 -- Phase 18 Plan 02 complete (road surface polygons, lane markings, junction fills)
progress:
  total_phases: 5
  completed_phases: 2
  total_plans: 12
  completed_plans: 10
  percent: 83
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-09)

**Core value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models
**Current focus:** Phase 18 in progress -- 3D Rendering Core (OrbitCamera, Renderer3D, mesh/billboard instancing, view toggle)

## Current Position

Phase: 18 of 20 (3D Rendering Core) -- IN PROGRESS
Plan: 2 of 4 in current phase (complete)
Status: Plan 02 complete, ready for Plan 03
Last activity: 2026-03-10 -- Phase 18 Plan 02 complete (road surface polygons, lane markings, junction fills)

Progress: [█████░░░░░] 50% (Phase 18)

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [18-02]: Road surface shader reuses ground_plane vertex layout (position vec3 + color vec4 = 28 bytes)
- [18-02]: Alpha blending on road pipeline for semi-transparent lane marking colors
- [18-02]: Y-offset layering: road=0.0, junction=0.005, marking=0.01 prevents z-fighting
- [18-02]: Separate render pass for road geometry (Load existing color/depth from ground pass)
- [18-01]: Renderer3D fully independent of existing 2D Renderer (no shared state)
- [18-01]: Camera bind group layout uses VERTEX|FRAGMENT visibility for future lighting
- [18-01]: Ground plane at y=-0.01 to prevent z-fighting with road surfaces at y=0
- [18-01]: Pitch clamp enforced through orbit() method; direct field access allowed for flexibility
- [17-04]: Camera FOV rendered as semi-transparent triangle cone (alpha 0.15 fill, 0.6 outline)
- [17-04]: RegisterCamera switched to fire-and-forget (no oneshot reply) for simplicity
- [17-04]: Camera range 40m (reduced from 100m) for realistic urban CCTV
- [17-04]: Speed heatmap overlay with live Python feed for detection visualization
- [17-03]: EMA alpha=0.3 for calibration ratio smoothing; clamp [0.5, 2.0] applied after EMA
- [17-03]: Simulated count threshold <=5 skips calibration (returns previous ratio)
- [17-03]: Edge-to-zone mapping uses nearest centroid heuristic (simplified)
- [17-03]: gRPC server on std::thread with tokio runtime before winit event loop
- [17-03]: Calibration recomputes every 300 sim-seconds (5 min) matching aggregation window
- [17-02]: edges_in_fov uses AABB pre-filter + angle normalization to [-PI, PI] for wraparound correctness
- [17-02]: DetectionAggregator uses i32 keys (prost proto encoding) for vehicle class, not Rust enum
- [17-02]: DetectionServiceImpl holds Arc<RTree> and Arc<Projection> for local camera registration
- [17-02]: ReceiverStream + tokio::spawn for bidirectional streaming response pattern
- [17-01]: tonic-prost-build replaces tonic-build::compile_protos (API split in tonic 0.14)
- [17-01]: Bounded mpsc channel (256) with drain(budget) for per-frame command processing
- [17-01]: Oneshot reply channel in RegisterCamera ApiCommand for request-response pattern
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
- Protobuf contract design needed before Phase 17 implementation -- RESOLVED (17-01)

## Session Continuity

Last session: 2026-03-10T16:06:11Z
Stopped at: Completed 18-02-PLAN.md
Resume file: .planning/phases/18-3d-rendering-core/18-03-PLAN.md
