# Feature Research

**Domain:** Camera CV integration + 3D wgpu native rendering for GPU-accelerated traffic microsimulation digital twin
**Researched:** 2026-03-09
**Confidence:** MEDIUM (camera CV pipeline is well-established domain; wgpu 3D rendering requires custom implementation with limited traffic-sim-specific precedent; demand calibration from camera counts uses standard methods but integration is novel)

## Context

v1.1 shipped a complete SUMO-replacement simulation engine: 280K agents, 7 vehicle types, CCH routing, BPR+ETS prediction, 2D GPU-instanced rendering (triangles/rectangles/dots), egui dashboard, SUMO import. All running natively on macOS Metal via wgpu.

v1.2 adds two major capabilities:
1. **Camera-based detection** -- ingest traffic camera feeds, detect/classify vehicles and pedestrians, use counts to calibrate simulation demand in real-time
2. **3D native rendering** -- replace 2D flat shapes with 3D city scene (buildings, terrain, vehicle meshes) rendered directly in wgpu, still on the same Metal surface

Both features complete the "digital twin" loop: real-world observation feeds simulation, and simulation renders a 3D replica of the real city.

## Feature Landscape

### Table Stakes (Users Expect These)

Features that any credible traffic digital twin with camera integration and 3D visualization must have. Missing these means the milestone is incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Vehicle detection (car, motorbike, bus, truck)** | Core purpose of camera integration. Must detect the same vehicle classes the simulation models. HCMC is 80% motorbikes -- must detect motorbikes reliably, not just cars. | MEDIUM | YOLOv8/v11 with ONNX Runtime via `ort` crate. Export Ultralytics model to ONNX, run inference in Rust. Pre-trained COCO has car/truck/bus; needs fine-tuning or custom dataset for motorbike vs bicycle distinction in HCMC context. Depends on: camera feed ingestion. |
| **Pedestrian detection** | Simulation models 20K pedestrians. Camera pipeline must count pedestrians at crosswalks/intersections for demand validation. | LOW | Same YOLO model detects "person" class natively. No additional model needed. Depends on: vehicle detection pipeline (shared model). |
| **Per-class vehicle counting** | Without classified counts, camera data cannot calibrate per-type demand (motorbike OD vs car OD). Every traffic monitoring system provides counts by vehicle type. | LOW | Track detections across frames (simple centroid tracker or ByteTrack). Count crossings at a virtual line per camera. Aggregate by class. Depends on: detection pipeline. |
| **Camera-to-network spatial mapping** | Detection counts must map to specific simulation edges/intersections. Without this, counts are just numbers with no spatial meaning. | MEDIUM | Manual configuration: each camera gets associated edge_id(s) or junction_id. Camera FOV polygon mapped to simulation network via lat/lon registration. Config-driven, not automatic. Depends on: network graph with geo-coordinates. |
| **Demand adjustment from counts** | The entire point of camera integration -- observed counts correct simulated demand. Standard approach in traffic engineering (FHWA calibration guidelines). | MEDIUM | Compare observed vs simulated counts per edge. Compute scaling factors per OD pair using gradient-free optimization (extend existing argmin/Bayesian calibration). Update spawn rates. Depends on: counting pipeline, existing velos-calibrate crate. |
| **3D depth buffer + perspective camera** | Current renderer is 2D orthographic (no depth). 3D requires perspective projection and depth testing. This is the foundational upgrade for all 3D content. | MEDIUM | Add depth texture (Depth32Float), perspective camera with position/target/up, depth_stencil_attachment on render pass. Extends existing `Renderer` and `Camera2D` to `Camera3D`. Depends on: existing wgpu renderer. |
| **3D building extrusions from OSM** | Buildings are the dominant visual element of a city scene. Users expect to see the city, not just roads and dots. OSM building footprints with height data cover HCMC adequately. | HIGH | Parse OSM building polygons, triangulate footprints (earcut algorithm), extrude to height (from `building:levels` tag, default 3m/floor). Generate vertex/index buffers. Instanced or batched draw. ~50K buildings in POC area. Depends on: 3D pipeline, OSM data. |
| **3D road surface rendering** | Roads must look like roads, not lines. Render road polygons with lane markings at close zoom. | MEDIUM | Extrude road edges to width (from lane count * lane_width). Generate quads with UV for lane marking texture. Road surface slightly above terrain. Depends on: 3D pipeline, network graph geometry. |
| **3D agent rendering with LOD** | Users expect to see vehicles as recognizable shapes when zoomed in. At city scale, individual meshes are wasteful. LOD strategy is standard practice. | HIGH | Three LOD tiers: (1) Close (<200m): low-poly glTF meshes per vehicle type (~200-500 triangles), (2) Mid (200-1000m): textured billboards/quads facing camera, (3) Far (>1000m): colored dots (current 2D approach). GPU instanced per tier. Depends on: 3D pipeline, glTF loader, camera distance calculation. |
| **Day/night lighting** | Time-of-day simulation runs 24h cycles. Users expect visual correspondence -- dark at night, bright at day. Without this, the 3D scene looks static and fake. | MEDIUM | Directional sun light with time-based azimuth/elevation. Ambient + diffuse lighting in fragment shader. Night: dark ambient + point lights at intersections/streetlights. No shadows for v1.2 (HIGH complexity, LOW value). Depends on: 3D pipeline with normals. |
| **Camera FOV overlay on map** | When viewing camera data, users need to see where each camera points. Standard in traffic management systems. | LOW | Render camera position as icon + FOV cone/polygon as semi-transparent overlay on map. egui panel lists cameras with status. Depends on: camera config with position/direction/FOV angle. |

### Differentiators (Competitive Advantage)

Features that set VELOS apart from existing traffic digital twins. These are where the project competes.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Real-time demand calibration loop** | Most traffic sims calibrate offline with historical counts. VELOS can close the loop: camera counts update demand every N minutes during a running simulation. This is active research (MIT 2020, Technion 2025) with no open-source implementation at this scale. | HIGH | Pipeline: camera -> detection -> counting -> compare with sim edge flows -> compute OD adjustment factors -> update velos-demand spawner rates -> agents spawn at corrected rates. Must handle: noisy counts, partial camera coverage, count-to-OD underdetermination. Depends on: all camera pipeline table stakes + existing calibration crate. |
| **Motorbike-specific detection fine-tuning** | Generic YOLO models lump motorcycles together. HCMC needs: motorbike vs electric scooter vs bicycle distinction. No commercial traffic CV product handles SE Asian motorbike-dominant traffic well. | MEDIUM | Fine-tune YOLOv8 on HCMC traffic camera footage with custom classes: motorbike, electric_scooter, bicycle, car, bus, truck, pedestrian. 500-1000 labeled images sufficient for transfer learning. Export to ONNX. Depends on: base detection pipeline, labeled training data. |
| **Speed estimation from camera** | Beyond counting -- estimate vehicle speeds from camera footage to validate simulation speed distributions per edge. Enables speed-based calibration, not just volume-based. | HIGH | Requires camera calibration (intrinsic + extrinsic), homography to road plane, track vehicle positions across frames, compute distance/time. Literature shows 5-10% accuracy achievable with proper calibration. Depends on: detection + tracking pipeline, camera calibration. |
| **Native wgpu 3D (no browser/webview)** | Competitors use CesiumJS/deck.gl (browser), Unity/Unreal (game engines), or desktop Qt. VELOS renders 3D directly in the same wgpu context as compute shaders -- zero data transfer overhead, single binary, no webview process. | HIGH | Full custom 3D rendering pipeline in wgpu: perspective camera, depth buffer, mesh rendering, instancing, lighting. More work than using a game engine, but eliminates the GPU context switch penalty and keeps the single-binary advantage. Depends on: existing wgpu infrastructure. |
| **Seamless 2D/3D view toggle** | Switch between 2D top-down analytics view (current) and 3D perspective view with one click. Same data, same frame, same egui controls. No page reload, no separate application. | MEDIUM | Both Camera2D and Camera3D share the same instance buffer data. Toggle swaps camera projection and enables/disables depth buffer + building/terrain passes. Smooth animated transition possible. Depends on: 3D pipeline coexisting with 2D pipeline. |
| **Detection confidence heatmap overlay** | Overlay detection confidence on the simulation map -- show where camera coverage is strong vs weak. Helps identify where demand calibration is reliable vs uncertain. | LOW | Aggregate detection confidence scores per edge. Render as color-coded edge overlay (green = high confidence coverage, red = no camera). Depends on: camera-to-network mapping, detection results. |
| **Terrain from DEM** | HCMC is mostly flat but has elevation variation near rivers and canals. Terrain adds realism and enables flood simulation visualization in future. | MEDIUM | SRTM 30m DEM -> heightmap texture -> GPU vertex displacement on terrain grid mesh. terra crate provides prior art for wgpu terrain. Grid resolution: 30m cells, ~270x270 grid for POC area. Depends on: 3D pipeline, DEM data download. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Real-time video overlay in 3D scene** | "Show live camera feeds as texture planes in the 3D world." | Video decoding + texture upload per frame per camera is expensive. Multiple cameras = multiple decode streams. Synchronizing video time with sim time adds complexity. Marginal value over just showing detection results. | Show detection bounding boxes as overlay on camera panel (2D egui panel). Show aggregated counts on the 3D map. |
| **Automatic camera calibration** | "The system should auto-detect camera parameters from the video feed." | Reliable automatic extrinsic calibration from arbitrary traffic cameras is unsolved at production quality. Requires vanishing point detection, known road geometry, and still fails with occlusion/weather. Research-grade, not production-grade. | Manual camera registration: user enters lat/lon/heading/FOV per camera. Config file. One-time setup per camera. |
| **Object re-identification across cameras** | "Track the same vehicle across multiple cameras for full trajectory reconstruction." | Multi-camera ReID requires appearance feature extraction, cross-camera association, camera-to-camera transition time modeling. Massive ML complexity for marginal calibration value. Single-camera counts are sufficient for demand calibration. | Per-camera independent counting. Aggregate counts per edge. No cross-camera identity linking needed for demand calibration. |
| **Photorealistic rendering (PBR materials, shadows, reflections)** | "The 3D city should look like Google Earth." | PBR rendering requires: normal maps, roughness maps, metallic maps, environment maps, shadow maps (cascaded shadow maps for outdoor), screen-space reflections. Each adds a render pass and texture memory. At 280K agents + 50K buildings, GPU budget is tight. | Flat-shaded buildings with ambient + diffuse lighting. Colored by building type. Night lights as emissive. 80% visual impact at 20% GPU cost. |
| **CityGML/3DTiles building models** | "Use detailed architectural models for accurate building representation." | No CityGML dataset exists for HCMC. 3DTiles requires tile server infrastructure. Both add ops complexity. OSM building footprints with height extrusion are available and sufficient. | OSM building extrusion. Heights from `building:levels` tag (default 3 floors = 9m). Roof shapes flat (sufficient for HCMC). |
| **Full RTSP camera protocol support** | "Connect to any IP camera via RTSP stream." | RTSP requires ffmpeg/GStreamer bindings, codec negotiation, reconnection handling, network timeout management. Cross-platform native RTSP in Rust is immature. | Accept pre-decoded video frames: (1) read from local video file, (2) receive frames via HTTP endpoint from a lightweight ffmpeg sidecar, (3) direct USB webcam via v4l2/AVFoundation. Start with file-based, add live later. |
| **GPU-accelerated inference** | "Run YOLO on GPU for faster detection." | On macOS Metal, ONNX Runtime GPU support via CoreML is limited for YOLO models. Metal Performance Shaders backend exists but has compatibility issues. The simulation already uses the GPU heavily -- sharing GPU between sim compute and ML inference creates resource contention. | CPU inference via `ort` crate with ONNX Runtime. YOLOv8n processes 640x480 at ~30ms on M1/M2 CPU. Sufficient for 1-5 cameras at 5-10 FPS detection rate. GPU stays dedicated to simulation + rendering. |
| **Vegetation and street furniture** | "Add trees, benches, streetlights for realism." | Thousands of instanced objects that don't affect simulation. Pure visual cost with no analytical value. Trees occlude vehicles in 3D view, making the visualization worse for analysis. | Streetlights only (needed for night lighting). No trees, no benches. Keep the view clean for traffic analysis. |

## Feature Dependencies

```
[Camera Feed Ingestion]
    |-- enables --> [Vehicle/Pedestrian Detection (YOLO + ort)]
                       |-- enables --> [Per-Class Counting + Tracking]
                                          |-- requires --> [Camera-to-Network Spatial Mapping]
                                          |-- enables --> [Demand Adjustment from Counts]
                                                             |-- requires --> [Existing velos-calibrate]
                                                             |-- requires --> [Existing velos-demand spawner]
                                                             |-- enables --> [Real-Time Calibration Loop]

[Speed Estimation from Camera]
    |-- requires --> [Detection + Tracking]
    |-- requires --> [Camera Calibration (manual extrinsics)]
    |-- enhances --> [Demand Calibration (speed-based validation)]

[3D Depth Buffer + Perspective Camera]
    |-- requires --> [Existing wgpu Renderer]
    |-- enables --> [3D Building Extrusions]
    |-- enables --> [3D Road Surfaces]
    |-- enables --> [3D Agent LOD Rendering]
    |-- enables --> [Terrain Rendering]
    |-- enables --> [Day/Night Lighting]

[3D Building Extrusions]
    |-- requires --> [3D Pipeline (depth + perspective)]
    |-- requires --> [OSM Building Data (existing import pipeline)]
    |-- requires --> [Polygon Triangulation (earcut)]

[3D Agent LOD Rendering]
    |-- requires --> [3D Pipeline]
    |-- requires --> [glTF Mesh Loading (gltf crate)]
    |-- requires --> [Camera Distance Calculation]
    |-- enhances --> [Existing instanced 2D renderer (replaces at close zoom)]

[Terrain from DEM]
    |-- requires --> [3D Pipeline]
    |-- requires --> [SRTM DEM Data]
    |-- enhances --> [Building placement (buildings sit on terrain)]

[Day/Night Lighting]
    |-- requires --> [3D Pipeline with vertex normals]
    |-- requires --> [Simulation time-of-day]
    |-- enhances --> [Visual realism of 3D scene]

[Camera FOV Overlay]
    |-- requires --> [Camera spatial config (lat/lon/heading/FOV)]
    |-- independent of --> [3D pipeline (works in 2D too)]

[2D/3D View Toggle]
    |-- requires --> [Both Camera2D and Camera3D implemented]
    |-- requires --> [3D pipeline operational]

[Motorbike-Specific Fine-Tuning]
    |-- requires --> [Base detection pipeline working]
    |-- requires --> [Labeled HCMC training data]
    |-- enhances --> [Detection accuracy for HCMC motorbikes]

[Detection Confidence Heatmap]
    |-- requires --> [Camera-to-network mapping]
    |-- requires --> [Detection results with confidence scores]
```

### Dependency Notes

- **Camera pipeline is self-contained:** Detection, counting, and demand adjustment form a pipeline independent of 3D rendering. These two feature tracks (camera + 3D) can be built in parallel.
- **3D pipeline is a renderer rewrite, not an extension:** The current renderer has no depth buffer, no perspective projection, no normals, no lighting. Moving to 3D means building a new render pipeline alongside the existing 2D one, then toggling between them.
- **Detection does not need 3D:** Camera integration works entirely on 2D data (video frames in, counts out). It feeds the simulation engine, not the renderer.
- **Buildings are the highest-effort 3D feature:** Parsing OSM polygons, triangulating, extruding, and rendering 50K buildings requires significant geometry processing. This should be the last 3D feature built after the pipeline (depth/camera/lighting) is proven.
- **Agent LOD is the highest-impact 3D feature:** Users notice vehicles first. Getting 3D vehicle meshes rendering with proper LOD gives the biggest visual upgrade per effort invested.
- **Terrain is optional but improves building placement:** Without terrain, buildings float or clip through the ground plane. With terrain, buildings sit naturally on the surface. Build terrain before buildings for proper placement.

## MVP Definition

### Launch With (v1.2 Core)

Minimum viable camera integration + 3D scene -- demonstrates the digital twin loop and 3D city view.

- [ ] **Camera feed ingestion (file-based)** -- Load video files or image sequences. No RTSP yet.
- [ ] **Vehicle/pedestrian detection (YOLOv8 + ort)** -- Detect car, motorbike, bus, truck, pedestrian from camera frames.
- [ ] **Per-class counting with simple tracker** -- Count vehicles crossing virtual detection lines per camera.
- [ ] **Camera-to-network mapping (config-driven)** -- Associate each camera with edge_id(s) via TOML config.
- [ ] **Demand adjustment from counts** -- Compare observed vs simulated counts, compute OD scaling factors, update spawn rates.
- [ ] **3D perspective camera + depth buffer** -- Upgrade renderer to support 3D viewing with depth testing.
- [ ] **3D road surface polygons** -- Render roads as textured polygons with lane markings.
- [ ] **3D agent LOD (mesh + billboard + dot)** -- Vehicle meshes close up, billboards mid-range, dots far away.
- [ ] **Day/night ambient lighting** -- Basic directional light following simulation time-of-day.
- [ ] **2D/3D view toggle** -- Switch between existing 2D top-down and new 3D perspective.
- [ ] **Camera FOV overlay** -- Show camera positions and coverage areas on map.

### Add After Core Validated (v1.2 Enhancement)

Features to add once detection pipeline and 3D rendering are proven.

- [ ] **3D building extrusions from OSM** -- Add when 3D pipeline is stable and performing well. 50K buildings is a significant GPU load.
- [ ] **Terrain from SRTM DEM** -- Add when buildings are rendering to provide proper ground surface.
- [ ] **Real-time calibration loop** -- Add when offline demand adjustment is validated against known counts.
- [ ] **Motorbike-specific YOLO fine-tuning** -- Add when HCMC training data is available and base pipeline is proven.
- [ ] **Speed estimation from camera** -- Add when detection + tracking is reliable and camera calibration data exists.
- [ ] **Detection confidence heatmap** -- Add when multiple cameras are integrated and coverage gaps are visible.

### Future Consideration (v2+)

- [ ] **Live camera feeds (RTSP/HTTP)** -- Defer until file-based pipeline is validated. Requires ffmpeg sidecar.
- [ ] **GPU-accelerated inference** -- Defer until CPU inference is proven insufficient. macOS Metal ONNX support is immature.
- [ ] **Shadows (cascaded shadow maps)** -- Defer. HIGH complexity, LOW analytical value. Visual polish only.
- [ ] **PBR materials** -- Defer. Requires material textures and multiple render passes. Flat shading is sufficient.
- [ ] **Cross-camera vehicle ReID** -- Defer. Massive ML complexity for marginal calibration value.

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Vehicle/pedestrian detection (YOLO + ort) | HIGH | MEDIUM | P1 |
| Per-class counting + tracking | HIGH | LOW | P1 |
| Camera-to-network spatial mapping | HIGH | LOW | P1 |
| Demand adjustment from counts | HIGH | MEDIUM | P1 |
| Camera feed ingestion (file-based) | HIGH | LOW | P1 |
| 3D perspective camera + depth buffer | HIGH | MEDIUM | P1 |
| 3D road surface polygons | HIGH | MEDIUM | P1 |
| 3D agent LOD rendering | HIGH | HIGH | P1 |
| Day/night lighting | MEDIUM | MEDIUM | P1 |
| 2D/3D view toggle | MEDIUM | LOW | P1 |
| Camera FOV overlay | MEDIUM | LOW | P1 |
| 3D building extrusions | HIGH | HIGH | P2 |
| Terrain from DEM | MEDIUM | MEDIUM | P2 |
| Real-time calibration loop | HIGH | HIGH | P2 |
| Motorbike YOLO fine-tuning | MEDIUM | MEDIUM | P2 |
| Speed estimation from camera | MEDIUM | HIGH | P2 |
| Detection confidence heatmap | LOW | LOW | P2 |
| Live camera feeds (RTSP) | MEDIUM | MEDIUM | P3 |
| Shadows | LOW | HIGH | P3 |
| PBR materials | LOW | HIGH | P3 |

**Priority key:**
- P1: Must have for v1.2 launch -- camera detection feeding demand + 3D city visualization
- P2: Should have -- enhances detection accuracy, visual quality, and calibration sophistication
- P3: Nice to have -- visual polish and operational features for production deployment

## Competitor Feature Analysis

| Feature | SUMO/SUMO-GUI | VISSIM | PTV Visum | VELOS v1.2 |
|---------|---------------|--------|-----------|-------------|
| **Camera CV integration** | None built-in | None built-in | External (PTV Optima) | Native YOLO pipeline in Rust |
| **Detection classes** | N/A | N/A | Car/truck/bus | Car/motorbike/bus/truck/bicycle/pedestrian |
| **Demand calibration from counts** | External tools (OD estimation) | External (PTV) | Built-in (matrix estimation) | Built-in (extending velos-calibrate) |
| **Real-time calibration** | No | No | PTV Optima (commercial) | In-process, streaming |
| **3D rendering** | SUMO-GUI (basic 3D, FPS drops >10K agents) | 3D with OSG/Unreal (commercial) | 2D only | Native wgpu 3D (single binary) |
| **Building rendering** | Basic block extrusions | Detailed 3D models | No | OSM building extrusions |
| **Agent LOD** | Fixed 3D models (no LOD) | 3-level LOD | N/A | 3-tier: mesh/billboard/dot |
| **Lighting** | Basic OpenGL lighting | Full lighting | N/A | Day/night directional + ambient |
| **Scale at 60FPS** | ~5K agents in 3D | ~20K agents in 3D | N/A (2D) | Target: 280K agents (LOD) |
| **Platform** | Desktop (C++) | Desktop (C++) | Desktop (C++) | Native macOS (Rust, single binary) |

**Key competitive positions for v1.2:**
1. **Only traffic sim with integrated camera CV pipeline.** All competitors require external tools/partnerships for camera detection.
2. **Native GPU 3D rendering at 280K agent scale.** SUMO-GUI struggles above 10K in 3D. VISSIM needs commercial Unreal plugin.
3. **Motorbike-specific detection.** No commercial traffic CV product handles SE Asian motorbike-dominant traffic well.
4. **Single binary digital twin.** Simulation + detection + 3D rendering in one Rust binary. No browser, no game engine, no Python.

## Sources

- [Vision-based vehicle counting, speed estimation pipeline (IEEE 2020)](https://ieeexplore.ieee.org/document/9130874/) - HIGH confidence
- [Digital twins for vision-based vehicle speed detection (2024)](https://arxiv.org/abs/2407.08380) - HIGH confidence
- [Digital twin intersection real-time monitoring (MDPI 2025)](https://www.mdpi.com/2412-3811/10/8/204) - HIGH confidence
- [Camera FOV spatial accuracy with 3D surface model (TRR 2025)](https://journals.sagepub.com/doi/10.1177/03611981251398747) - HIGH confidence
- [Camera perspective to BEV transformation (2024)](https://arxiv.org/html/2408.05577v2) - MEDIUM confidence
- [OD matrix estimation from video recordings (ScienceDirect)](https://www.sciencedirect.com/science/article/pii/S1877705817301169) - HIGH confidence
- [Real-time OD calibration (MIT DSpace)](https://dspace.mit.edu/bitstream/handle/1721.1/129009/1227096827-MIT.pdf) - HIGH confidence
- [OD calibration with segment counts (arXiv 2025)](https://arxiv.org/html/2502.19528) - MEDIUM confidence
- [ort crate - ONNX Runtime for Rust](https://ort.pyke.io/) - HIGH confidence
- [YOLOv8 ONNX Rust implementation](https://github.com/AndreyGermanov/yolov8_onnx_rust) - HIGH confidence
- [gltf-rs crate for Rust](https://github.com/gltf-rs/gltf) - HIGH confidence
- [Learn wgpu - depth buffer tutorial](https://sotrh.github.io/learn-wgpu/beginner/tutorial8-depth/) - HIGH confidence
- [Learn wgpu - model loading tutorial](https://sotrh.github.io/learn-wgpu/beginner/tutorial9-models/) - HIGH confidence
- [terra - wgpu terrain rendering in Rust](https://github.com/fintelia/terra) - MEDIUM confidence
- [OSM2World - OSM to 3D model export](https://osm2world.org/) - HIGH confidence
- [OSM 3D building data](https://wiki.openstreetmap.org/wiki/3D) - HIGH confidence
- [Reshadable Impostors with LOD (CGF 2025)](https://onlinelibrary.wiley.com/doi/10.1111/cgf.70183) - HIGH confidence
- [GPU crowd rendering with impostors](https://www.researchgate.net/publication/220979001_Impostors_and_pseudo-instancing_for_GPU_crowd_rendering) - HIGH confidence
- [rend3 - wgpu 3D renderer](https://docs.rs/rend3) - MEDIUM confidence
- [wgpu official documentation](https://wgpu.rs/) - HIGH confidence
- VELOS architecture docs (`docs/architect/00-07`) and existing codebase - HIGH confidence

---
*Feature research for: Camera CV integration + 3D wgpu native rendering for traffic digital twin*
*Researched: 2026-03-09*
