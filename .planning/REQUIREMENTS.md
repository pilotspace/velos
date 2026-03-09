# Requirements: VELOS v1.2 Digital Twin

**Defined:** 2026-03-09
**Core Value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models

## v1.2 Requirements

Requirements for Digital Twin milestone. Each maps to roadmap phases.

### Intersection Sublane

- [x] **ISL-01**: Vehicles maintain continuous lateral position through junction internal edges (not reset to lane center)
- [x] **ISL-02**: Motorbikes can filter and weave through intersection areas using probe-based gap scanning
- [x] **ISL-03**: Turn geometry supports sublane positioning (curved paths through intersections with lateral offset)
- [x] **ISL-04**: Conflict detection at crossing points within junctions resolves priority between sublane-positioned agents

### 2D Map Rendering

- [x] **MAP-01**: Self-hosted 2D vector map tiles from OSM render as background layer in the simulation view
- [ ] **MAP-02**: Sublane positions are visually rendered in 2D — vehicles show lateral offsets through intersections with lane marking context

### Detection Ingestion

- [ ] **DET-01**: System exposes gRPC service accepting vehicle/pedestrian detection events from external CV services
- [ ] **DET-02**: System aggregates received detections into per-class counts per camera over configurable time windows
- [ ] **DET-03**: User can register cameras with position, FOV, and network edge/junction mapping via gRPC or config
- [ ] **DET-04**: User can see camera positions and FOV coverage areas overlaid on the map
- [ ] **DET-05**: System accepts speed estimation data from external CV services per camera
- [ ] **DET-06**: Python and Rust client libraries connect to VELOS gRPC detection service for integration testing

### Calibration

- [ ] **CAL-01**: System adjusts simulation demand (OD spawn rates) based on observed vs simulated counts
- [ ] **CAL-02**: System continuously calibrates demand during a running simulation from streaming detection data

### 3D Rendering

- [ ] **R3D-01**: User can view simulation in 3D perspective with depth-correct rendering
- [ ] **R3D-02**: Roads render as 3D surface polygons with lane markings
- [ ] **R3D-03**: Agents render as 3D meshes (close), billboards (mid-range), dots (far) with GPU instancing per LOD tier
- [ ] **R3D-04**: User can toggle between 2D top-down and 3D perspective views
- [ ] **R3D-05**: Scene lighting follows simulation time-of-day (day/night cycle with directional sun + ambient)
- [ ] **R3D-06**: OSM building footprints render as extruded 3D buildings with height from building:levels tag
- [ ] **R3D-07**: Terrain renders from SRTM DEM heightmap data as ground surface mesh

## Future Requirements

Deferred to future release. Tracked but not in current roadmap.

### Built-in CV Detection

- **CV-01**: System runs YOLO inference in-process via ort crate for self-contained detection
- **CV-02**: System ingests RTSP camera streams directly for real-time detection
- **CV-03**: System fine-tunes YOLO model on HCMC-specific motorbike/vehicle data

### Detection Analytics

- **DAN-01**: Detection confidence heatmap overlay shows camera coverage strength per edge
- **DAN-02**: Cross-camera vehicle re-identification for trajectory reconstruction

### 3D Enhancement

- **R3D-08**: Buildings render with cascaded shadow maps
- **R3D-09**: Buildings render with PBR materials (normal/roughness/metallic maps)
- **R3D-10**: Scene includes vegetation and street furniture (trees, streetlights)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Built-in YOLO inference | Detection runs in external services; VELOS consumes results via gRPC |
| Video decoding / RTSP ingestion | External CV services handle camera feeds; VELOS receives structured detection data |
| Real-time video overlay in 3D scene | Detection results overlay is sufficient; video decode per camera is expensive |
| Automatic camera calibration | Unsolved at production quality; manual registration is reliable |
| Cross-camera vehicle ReID | Massive ML complexity for marginal calibration value |
| Photorealistic PBR rendering | Multiple render passes + texture memory at 280K agents + 50K buildings exceeds GPU budget |
| CityGML/3DTiles from external source | No CityGML dataset exists for HCMC; OSM extrusion is available and sufficient |
| Browser-based web viewer | VELOS is a native wgpu app; no webview/CesiumJS/deck.gl in v1.2 |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| ISL-01 | Phase 16 | Complete |
| ISL-02 | Phase 16 | Complete |
| ISL-03 | Phase 16 | Complete |
| ISL-04 | Phase 16 | Complete |
| MAP-01 | Phase 16 | Complete |
| MAP-02 | Phase 16 | Pending |
| DET-01 | Phase 17 | Pending |
| DET-02 | Phase 17 | Pending |
| DET-03 | Phase 17 | Pending |
| DET-04 | Phase 17 | Pending |
| DET-05 | Phase 17 | Pending |
| DET-06 | Phase 17 | Pending |
| CAL-01 | Phase 17 | Pending |
| CAL-02 | Phase 20 | Pending |
| R3D-01 | Phase 18 | Pending |
| R3D-02 | Phase 18 | Pending |
| R3D-03 | Phase 18 | Pending |
| R3D-04 | Phase 18 | Pending |
| R3D-05 | Phase 18 | Pending |
| R3D-06 | Phase 19 | Pending |
| R3D-07 | Phase 19 | Pending |

**Coverage:**
- v1.2 requirements: 21 total
- Mapped to phases: 21
- Unmapped: 0

---
*Requirements defined: 2026-03-09*
*Last updated: 2026-03-09 (revision 2 -- ISL requirements added, all 19 mapped to phases 16-20)*
