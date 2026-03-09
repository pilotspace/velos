# Requirements: VELOS v1.2 Digital Twin

**Defined:** 2026-03-09
**Core Value:** Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models

## v1.2 Requirements

Requirements for Digital Twin milestone. Each maps to roadmap phases.

### Camera Detection

- [ ] **CAM-01**: User can load video files as camera feed source for detection
- [ ] **CAM-02**: System detects vehicles (car, motorbike, bus, truck) and pedestrians from camera frames using YOLO + ONNX
- [ ] **CAM-03**: System counts detected objects per class crossing virtual detection lines per camera
- [ ] **CAM-04**: User can map cameras to simulation network edges/junctions via config
- [ ] **CAM-05**: User can see camera positions and FOV coverage areas overlaid on the map

### Calibration

- [ ] **CAL-01**: System adjusts simulation demand (OD spawn rates) based on observed vs simulated counts
- [ ] **CAL-02**: System continuously calibrates demand during a running simulation from streaming camera counts
- [ ] **CAL-03**: System estimates vehicle speeds from camera footage for speed-based validation

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

### Camera Enhancement

- **CAM-06**: System connects to live RTSP camera streams for real-time detection
- **CAM-07**: System fine-tunes YOLO model on HCMC-specific motorbike/vehicle data
- **CAM-08**: System re-identifies vehicles across multiple cameras for trajectory reconstruction

### 3D Enhancement

- **R3D-08**: Buildings render with cascaded shadow maps
- **R3D-09**: Buildings render with PBR materials (normal/roughness/metallic maps)
- **R3D-10**: Scene includes vegetation and street furniture (trees, streetlights)

### Detection Analytics

- **DET-01**: Detection confidence heatmap overlay shows camera coverage strength per edge
- **DET-02**: GPU-accelerated YOLO inference via Metal Performance Shaders

## Out of Scope

| Feature | Reason |
|---------|--------|
| Real-time video overlay in 3D scene | Video decode per camera per frame is expensive; detection results overlay is sufficient |
| Automatic camera calibration | Unsolved at production quality; manual config is reliable and one-time |
| Cross-camera vehicle ReID | Massive ML complexity for marginal calibration value; per-camera counts sufficient |
| Photorealistic PBR rendering | Multiple render passes + texture memory at 280K agents + 50K buildings exceeds GPU budget |
| CityGML/3DTiles from external source | No CityGML dataset exists for HCMC; OSM extrusion is available and sufficient |
| Browser-based web viewer | VELOS is a native wgpu app; no webview/CesiumJS/deck.gl in v1.2 |
| Python ML sidecar | All inference runs in-process via ort crate; no Python bridge |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| CAM-01 | — | Pending |
| CAM-02 | — | Pending |
| CAM-03 | — | Pending |
| CAM-04 | — | Pending |
| CAM-05 | — | Pending |
| CAL-01 | — | Pending |
| CAL-02 | — | Pending |
| CAL-03 | — | Pending |
| R3D-01 | — | Pending |
| R3D-02 | — | Pending |
| R3D-03 | — | Pending |
| R3D-04 | — | Pending |
| R3D-05 | — | Pending |
| R3D-06 | — | Pending |
| R3D-07 | — | Pending |

**Coverage:**
- v1.2 requirements: 15 total
- Mapped to phases: 0
- Unmapped: 15 ⚠️

---
*Requirements defined: 2026-03-09*
*Last updated: 2026-03-09 after initial definition*
