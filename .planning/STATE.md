---
gsd_state_version: 1.0
milestone: v1.3
milestone_name: Web Dashboard
status: active
stopped_at: ""
last_updated: "2026-03-12T00:00:00.000Z"
last_activity: 2026-03-12 -- Milestone v1.3 started
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-12)

**Core value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models
**Current focus:** v1.3 Web Dashboard -- Tauri + React replacing egui, deck.gl 2D + CesiumJS 3D, self-hosted tiles

## Current Position

Phase: Not started (defining requirements)
Plan: --
Status: Defining requirements
Last activity: 2026-03-12 -- Milestone v1.3 started

Progress: [░░░░░░░░░░] 0%

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [19-01]: No instancing for buildings -- unique geometry in merged vertex/index buffer (single draw call)
- [19-01]: Outward wall normals = (dy/len, 0, -dx/len) for CCW polygon in XZ plane
- [19-01]: Only Way-type buildings for POC (multipolygon Relations deferred)
- [19-01]: Base color #D4C5A9 beige with deterministic +/-5% centroid-based variation
- [18-04]: Extracted app_input.rs and app_egui.rs from app.rs to stay under 700-line limit
- [18-04]: Render dispatch renders target mode during transition (no cross-fade)
- [18-04]: build_instances_3d maps 2D (x, y) to 3D (x, 0, y) with LOD classification from eye position
- [18-02]: Road surface shader reuses ground_plane vertex layout (position vec3 + color vec4 = 28 bytes)
- [18-02]: Alpha blending on road pipeline for semi-transparent lane marking colors
- [18-02]: Y-offset layering: road=0.0, junction=0.005, marking=0.01 prevents z-fighting
- [18-02]: Separate render pass for road geometry (Load existing color/depth from ground pass)
- [18-03]: Separate bind group layouts: camera-only (ground/road) vs camera+lighting (mesh/billboard)
- [18-03]: CameraUniform3D extended to 112 bytes (view_proj + eye_pos + camera_right + camera_up)
- [18-03]: Billboard uses 6-vertex quad from vertex_index in shader (no vertex buffer needed)
- [18-03]: LOD exact boundary classifies to cheaper tier; hysteresis only on downgrade (threshold*1.1)
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
- [Phase 18]: Reverse-Z depth buffer with infinite far plane eliminates z-fighting on overlapping road layers
- [Phase 18]: Right-drag pan for Mac trackpad ergonomics (two-finger click+drag as middle-drag alternative)
- [Phase 18]: Y layer separation increased 10x (junction 0.05, marking 0.1) for reliable depth ordering at oblique angles
- [Phase 19]: Manual SRTM .hgt parsing over external crate -- format is trivially simple
- [Phase 19]: Terrain Y clamped to -0.5 to stay below road surface at Y=0
- [Phase 19]: Disabled back-face culling for buildings to handle mixed winding from earcut triangulation
- [Phase 19]: Half-Lambert shading (dot*0.5+0.5) for softer building appearance
- [Phase 19]: Camera focus distance passed to render_frame for LOD selection
- [20-01]: Window-change detection as sole calibration trigger (no 300s fallback timer)
- [20-01]: 30s cooldown between recalibrations prevents thrashing
- [20-01]: Staleness decay starts at 3 consecutive unchanged windows with 0.1*(n-2) rate
- [20-01]: Change cap +/-0.2 per step applied after compute, before overlay swap
- [20-01]: MIN_OBSERVED_THRESHOLD=10 added alongside existing MIN_SIMULATED_THRESHOLD=5
- [20-02]: Plan 20-02 was no-op -- all egui panel enhancements already implemented in Plan 20-01
- [20-02]: Plan 02 served as end-to-end human verification gate for complete streaming calibration system

### Pending Todos

- **[critical] Fix performance regression at 8K agents** — 83.4ms frame time (5x over budget). Profile junction HashMap lookups, build_instances iteration, map tile thread contention, N+1 ECS queries. File: `.planning/todos/pending/2026-03-09-fix-critical-performance-regression-at-8k-agents.md`

### Blockers/Concerns

- Building count for POC area unverified (estimated 80K-120K)
- wgpu version decision needed before Phase 18 (v27 current vs v28 available)
- Protobuf contract design needed before Phase 17 implementation -- RESOLVED (17-01)

## Session Continuity

Last session: 2026-03-11T13:16:30Z
Stopped at: Completed 20-02-PLAN.md -- v1.2 milestone feature-complete
Resume file: .planning/phases/20-real-time-calibration/20-02-SUMMARY.md
