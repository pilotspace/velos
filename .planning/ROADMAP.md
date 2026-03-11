# Roadmap: VELOS

## Milestones

- ✅ **v1.0 MVP** — Phases 1-4 (shipped 2026-03-07)
- ✅ **v1.1 SUMO Replacement Engine** — Phases 5-15 (shipped 2026-03-09)
- 🚧 **v1.2 Digital Twin** — Phases 16-20 (in progress)

## Phases

<details>
<summary>v1.0 MVP (Phases 1-4) — SHIPPED 2026-03-07</summary>

- [x] Phase 1: GPU Pipeline & Visual Proof (2/2 plans) — completed 2026-03-06
- [x] Phase 2: Road Network & Vehicle Models + egui (4/4 plans) — completed 2026-03-07
- [x] Phase 3: Motorbike Sublane & Pedestrians (2/2 plans) — completed 2026-03-07
- [x] Phase 4: MOBIL Wiring + Motorbike Jam Fix + Performance (3/3 plans) — completed 2026-03-07

</details>

<details>
<summary>v1.1 SUMO Replacement Engine (Phases 5-15) — SHIPPED 2026-03-09</summary>

- [x] Phase 5: Foundation & GPU Engine (6/6 plans) — completed 2026-03-07
- [x] Phase 6: Agent Models & Signal Control (7/7 plans) — completed 2026-03-07
- [x] Phase 7: Intelligence, Routing & Prediction (6/6 plans) — completed 2026-03-07
- [x] Phase 8: Tuning Vehicle Behavior (HCM) (3/3 plans) — completed 2026-03-08
- [x] Phase 9: Sim Loop Integration — Startup & Frame Pipeline (3/3 plans) — completed 2026-03-08
- [x] Phase 10: Sim Loop Integration — Bus Dwell & Meso-Micro (2/2 plans) — completed 2026-03-08
- [x] Phase 11: GPU Buffer Wiring — Perception & Emergency (2/2 plans) — completed 2026-03-08
- [x] Phase 12: CPU Lane-Change, Prediction Loop & GPU Config (2/2 plans) — completed 2026-03-08
- [x] Phase 13: Final Integration Wiring & GPU Transfer Audit (3/3 plans) — completed 2026-03-08
- [x] Phase 14: Wire GTFS → Bus Stops Pipeline (2/2 plans) — completed 2026-03-08
- [x] Phase 15: File Size Reduction & Housekeeping (3/3 plans) — completed 2026-03-08

</details>

### v1.2 Digital Twin (In Progress)

**Milestone Goal:** Complete the digital twin loop with intersection sublane correctness, real-world camera detection ingestion for demand calibration, and 3D native city visualization with agent rendering.

- [x] **Phase 16: Intersection Sublane Model & 2D Map Tiles** - Sublane through junctions, conflict detection, 2D vector map tile background, sublane visualization (completed 2026-03-09)
- [x] **Phase 17: Detection Ingestion & Demand Calibration** - gRPC detection service with camera registration, count/speed aggregation, batch demand adjustment, and client SDKs (completed 2026-03-10)
- [x] **Phase 18: 3D Rendering Core** - Perspective camera, depth buffer, 3D roads, LOD agents, day/night lighting, 2D/3D toggle (completed 2026-03-11)
- [x] **Phase 19: 3D City Scene** - OSM building extrusions and SRTM DEM terrain rendering (completed 2026-03-11)
- [ ] **Phase 20: Real-Time Calibration** - Continuous streaming calibration without simulation restart

## Phase Details

### Phase 16: Intersection Sublane Model
**Goal**: Vehicles navigate intersections with continuous sublane positioning, enabling realistic motorbike filtering and conflict resolution at junctions
**Depends on**: Phase 15 (v1.1 complete -- foundation for v1.2)
**Requirements**: ISL-01, ISL-02, ISL-03, ISL-04, MAP-01, MAP-02
**Success Criteria** (what must be TRUE):
  1. A vehicle entering a junction retains its lateral offset throughout internal edge traversal -- no snap to lane center
  2. Motorbikes filter between larger vehicles inside intersection areas using probe-based gap scanning
  3. Turning vehicles follow curved paths with lateral offset preserved (a motorbike on the left side of the lane traces a tighter arc)
  4. Two agents on crossing paths within a junction detect the conflict and one yields based on priority rules
  5. Self-hosted 2D vector map tiles from OSM render as background layer in the simulation view
  6. Vehicles visually show lateral offsets through intersections with lane marking context in 2D rendering
**Plans**: 4 plans
Plans:
- [x] 16-01-PLAN.md — Junction geometry data model and Bezier precomputation
- [x] 16-02-PLAN.md — Junction traversal logic and frame pipeline integration
- [x] 16-03-PLAN.md — 2D vector map tile rendering pipeline
- [x] 16-04-PLAN.md — Sublane visualization, guide lines, and debug overlays

### Phase 17: Detection Ingestion & Demand Calibration
**Goal**: External CV services can push detection data into VELOS via gRPC, and the system uses those detections to adjust simulation demand
**Depends on**: Phase 16
**Requirements**: DET-01, DET-02, DET-03, DET-04, DET-05, DET-06, CAL-01
**Success Criteria** (what must be TRUE):
  1. A gRPC client can stream vehicle/pedestrian detection events to VELOS and receive acknowledgment per batch
  2. User can register cameras with position, FOV, and network edge/junction mapping, then see camera positions with FOV coverage areas overlaid on the simulation map
  3. Detection counts per class are aggregated over configurable time windows and speed estimation data is accepted per camera
  4. System adjusts OD spawn rates based on observed-vs-simulated count ratios, with demand changes reflected in agent spawn behavior
  5. Python and Rust client libraries can connect to the gRPC service and push detection events for integration testing
**Plans**: 4 plans
Plans:
- [x] 17-01-PLAN.md — Proto definition, velos-api crate scaffold, and async-sync bridge
- [x] 17-02-PLAN.md — CameraRegistry, DetectionAggregator, and gRPC DetectionService handler
- [x] 17-03-PLAN.md — CalibrationOverlay, Spawner integration, app wiring, and egui panel
- [x] 17-04-PLAN.md — Camera FOV rendering, Rust/Python client SDKs, and visual verification

### Phase 18: 3D Rendering Core
**Goal**: User can view the running simulation in a 3D perspective with depth-correct rendering, LOD agents, road surfaces, and time-of-day lighting
**Depends on**: Phase 16 (independent of Phase 17 -- can execute in parallel)
**Requirements**: R3D-01, R3D-02, R3D-03, R3D-04, R3D-05
**Success Criteria** (what must be TRUE):
  1. User sees the simulation from a 3D perspective camera with correct depth ordering (near objects occlude far objects)
  2. Roads render as 3D surface polygons with visible lane markings
  3. Agents render as 3D meshes when close, billboards at mid-range, and dots when far -- all via GPU instancing
  4. User can toggle between 2D top-down and 3D perspective with a single click, preserving camera position
  5. Scene lighting changes with simulation time-of-day (bright directional sun during day, dim ambient at night)
**Plans**: 4 plans
Plans:
- [x] 18-01-PLAN.md — OrbitCamera, ViewMode types, depth buffer, and Renderer3D scaffold
- [x] 18-02-PLAN.md — Road surface polygons, lane markings, and junction fills from RoadGraph
- [x] 18-03-PLAN.md — Lighting system, glTF mesh loader, LOD classification, and 3D agent shaders
- [x] 18-04-PLAN.md — View toggle wiring, orbit camera input, render dispatch, and visual verification

### Phase 19: 3D City Scene
**Goal**: The 3D view includes extruded buildings from OSM data and terrain from SRTM DEM, creating a recognizable HCMC cityscape
**Depends on**: Phase 18 (3D rendering pipeline proven)
**Requirements**: R3D-06, R3D-07
**Success Criteria** (what must be TRUE):
  1. OSM building footprints render as extruded 3D volumes with height derived from building:levels tag
  2. Ground surface renders from SRTM DEM heightmap data as a terrain mesh with elevation variation
  3. Buildings and terrain integrate with the existing 3D scene (correct depth, lighting, and camera interaction -- no z-fighting)
**Plans**: 3 plans
Plans:
- [ ] 19-01-PLAN.md — Building footprint extraction from OSM and extrusion geometry generation
- [ ] 19-02-PLAN.md — SRTM DEM terrain parsing and mesh generation
- [ ] 19-03-PLAN.md — Renderer3D integration, app wiring, LOD, and visual verification

### Phase 20: Real-Time Calibration
**Goal**: Simulation demand continuously self-corrects from streaming detection data without requiring restart
**Depends on**: Phase 17 (batch calibration validated before streaming)
**Requirements**: CAL-02
**Success Criteria** (what must be TRUE):
  1. While the simulation is running, new detection data flowing in causes demand adjustments within the current session
  2. User can observe OD spawn rates changing in response to streaming detection counts without stopping or restarting the simulation
**Plans**: [To be planned]

## Progress

**Execution Order:**
Phase 16 first (foundation). Phases 17 + 18 in parallel after 16. Phase 19 after 18. Phase 20 after 17.

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. GPU Pipeline & Visual Proof | v1.0 | 2/2 | Complete | 2026-03-06 |
| 2. Road Network & Vehicle Models + egui | v1.0 | 4/4 | Complete | 2026-03-07 |
| 3. Motorbike Sublane & Pedestrians | v1.0 | 2/2 | Complete | 2026-03-07 |
| 4. MOBIL Wiring + Motorbike Jam Fix + Performance | v1.0 | 3/3 | Complete | 2026-03-07 |
| 5. Foundation & GPU Engine | v1.1 | 6/6 | Complete | 2026-03-07 |
| 6. Agent Models & Signal Control | v1.1 | 7/7 | Complete | 2026-03-07 |
| 7. Intelligence, Routing & Prediction | v1.1 | 6/6 | Complete | 2026-03-07 |
| 8. Tuning Vehicle Behavior (HCM) | v1.1 | 3/3 | Complete | 2026-03-08 |
| 9. Sim Loop Integration — Startup & Frame Pipeline | v1.1 | 3/3 | Complete | 2026-03-08 |
| 10. Sim Loop Integration — Bus Dwell & Meso-Micro | v1.1 | 2/2 | Complete | 2026-03-08 |
| 11. GPU Buffer Wiring — Perception & Emergency | v1.1 | 2/2 | Complete | 2026-03-08 |
| 12. CPU Lane-Change, Prediction Loop & GPU Config | v1.1 | 2/2 | Complete | 2026-03-08 |
| 13. Final Integration Wiring & GPU Transfer Audit | v1.1 | 3/3 | Complete | 2026-03-08 |
| 14. Wire GTFS → Bus Stops Pipeline | v1.1 | 2/2 | Complete | 2026-03-08 |
| 15. File Size Reduction & Housekeeping | v1.1 | 3/3 | Complete | 2026-03-08 |
| 16. Intersection Sublane Model | v1.2 | 4/4 | Complete | 2026-03-09 |
| 17. Detection Ingestion & Demand Calibration | v1.2 | 4/4 | Complete | 2026-03-10 |
| 18. 3D Rendering Core | v1.2 | 4/4 | Complete | 2026-03-11 |
| 19. 3D City Scene | 3/3 | Complete   | 2026-03-11 | - |
| 20. Real-Time Calibration | v1.2 | 0/? | Not started | - |

---
*Roadmap created: 2026-03-09*
*Last updated: 2026-03-11 (Phase 19 plans created)*
