# Project Research Summary

**Project:** VELOS v1.2 -- Camera CV Detection + 3D Native Rendering
**Domain:** GPU-accelerated traffic microsimulation digital twin (Rust/wgpu)
**Researched:** 2026-03-09
**Confidence:** MEDIUM

## Executive Summary

VELOS v1.2 adds two major capabilities to the existing 280K-agent traffic microsimulation: a camera-based computer vision pipeline for real-time demand calibration, and a native 3D wgpu rendering engine replacing the current 2D instanced renderer. These two feature tracks are architecturally independent -- the CV pipeline feeds the demand spawner regardless of render mode, and the 3D renderer consumes the same ECS data as the 2D renderer. This independence is the key architectural advantage: both tracks can be developed in parallel with integration deferred to a final phase.

The recommended approach uses `ort` (ONNX Runtime) with CoreML execution provider for YOLO inference on Apple Neural Engine, keeping the GPU free for simulation compute and rendering. The 3D renderer should be built as a new `velos-render3d` crate with its own depth buffer, perspective camera, and render pipeline -- not retrofitted into the existing 2D `Renderer`. Building geometry comes from OSM footprint extrusion (earcut algorithm), not external 3D datasets which do not exist for HCMC. Agent LOD is mandatory from day one: 3D meshes only within 500m, billboards at mid-range, dots beyond 2km.

The primary risks are GPU resource contention (simulation compute + 3D rendering + potential ML inference all competing on Apple Silicon's single GPU), coordinate system confusion across five coordinate spaces, and threading complexity from mixing rayon, tokio, and dedicated decode threads. All three risks have clear mitigation strategies documented in the research, but all three have HIGH recovery cost if not addressed upfront in the architecture phase.

## Key Findings

### Recommended Stack

Two new crates (`velos-cv`, `velos-render3d`) with targeted dependencies. The stack avoids heavyweight frameworks (OpenCV, GStreamer, game engines) in favor of surgical crate-per-concern selection.

**Core technologies:**
- **ort 2.0.0-rc.12** (CoreML feature): ONNX Runtime for YOLO inference -- ANE acceleration on Apple Silicon, 5ms latency for YOLOv8n, no GPU contention
- **retina 0.4 + ffmpeg-next 8**: RTSP camera ingestion (async, tokio-compatible) + hardware video decode (VideoToolbox) -- separated protocol from codec concerns
- **gltf 1.4**: glTF 2.0 model loading for vehicle meshes and landmark buildings -- the uncontested standard (5.8M downloads)
- **earcut 0.4**: Polygon triangulation for OSM building footprint extrusion -- Mapbox algorithm port, handles concave polygons with holes
- **ndarray 0.16**: Tensor manipulation for YOLO pre/post-processing (resize, normalize, NMS)

**Critical version requirement:** FFmpeg 6.x+ system dependency for `ffmpeg-next` crate linking. Install via `brew install ffmpeg`.

**What NOT to use:** OpenCV (1GB+ C++ dependency), Python sidecar (violates single-binary principle), GStreamer (overkill), rend3/Bevy/three-d (conflict with custom wgpu renderer), 3D Tiles/CityGML (no HCMC data exists).

### Expected Features

**Must have (table stakes) -- v1.2 launch:**
- Vehicle/pedestrian detection (YOLOv8/11 + ort) with per-class counting
- Camera-to-network spatial mapping (config-driven edge association)
- Demand adjustment from observed counts (OD scaling factors)
- 3D perspective camera + depth buffer (foundational renderer upgrade)
- 3D road surface polygons with lane markings
- 3D agent LOD rendering (mesh/billboard/dot tiers)
- Day/night ambient lighting following simulation time-of-day
- 2D/3D view toggle (same data, same egui controls, one-click switch)
- Camera FOV overlay on map

**Should have (differentiators) -- v1.2 enhancement:**
- Real-time demand calibration loop (streaming, not batch)
- 3D OSM building extrusions (~50K buildings)
- Terrain from SRTM DEM
- Motorbike-specific YOLO fine-tuning for HCMC traffic
- Speed estimation from camera (requires camera calibration)
- Detection confidence heatmap

**Defer (v2+):**
- Live RTSP camera feeds (validate with file-based first)
- GPU-accelerated ML inference (CPU/ANE sufficient for 1-5 cameras)
- Shadows, PBR materials (HIGH cost, LOW analytical value)
- Cross-camera vehicle re-identification

### Architecture Approach

Two new crates integrate at well-defined boundary points without modifying the core simulation loop. `velos-cv` produces `CvEdgeCounts` published via `ArcSwap` (matching the existing `PredictionOverlay` pattern), consumed by `velos-demand::Spawner`. `velos-render3d` owns the 3D render pipeline and shares `wgpu::Device`/`Queue` with the existing 2D renderer, toggled by an enum dispatch in `app.rs`.

**Major components:**
1. **velos-cv::CameraManager** -- async camera stream decode (tokio tasks), frame buffering
2. **velos-cv::InferenceRunner** -- dedicated OS thread for ONNX inference (isolated from rayon/tokio), crossbeam channel communication
3. **velos-cv::CountAggregator** -- sliding window smoothing, edge-level count publication via ArcSwap
4. **velos-render3d::Renderer3D** -- depth buffer, perspective camera, single render pass (terrain -> buildings -> roads -> agents)
5. **velos-render3d::BuildingLoader** -- OSM footprint extrusion via earcut, batched into single GPU buffer
6. **velos-render3d::AgentMeshPool** -- per-vehicle-type glTF meshes with 3-tier LOD

### Critical Pitfalls

1. **Single wgpu queue serialization** -- Compute, render, and ML inference all serialize through one Metal queue. Mitigate by routing ML to ANE (not GPU), and time-slicing compute/render within the frame budget. Do not add a second queue (wgpu does not support multiple queues per device).

2. **GPU memory exhaustion from building meshes** -- 100K individual buffers = 10s creation time + heap fragmentation. Mitigate by batching all building geometry into a single vertex/index buffer atlas from day one. Never create per-building buffers.

3. **Coordinate system confusion across 5 spaces** -- Edge-local, world-space (UTM), NDC, WGS84, 3D world. Mitigate by defining a canonical `coords.rs` module with UTM Zone 48N as the single source of truth, with tested conversion functions, before writing any 3D or CV code.

4. **Video decode blocking the simulation loop** -- Synchronous ffmpeg calls on the main thread add 4-12ms per frame with multiple cameras. Mitigate by running decode on dedicated `std::thread` workers with bounded `crossbeam::channel`, reading latest frame non-blockingly.

5. **Threading deadlocks from rayon/tokio/std::thread mixing** -- Three runtime models create cross-boundary contention. Mitigate by enforcing strict boundaries (rayon=physics, tokio=I/O, std::thread=decode+inference) and bridging exclusively via channels, never shared mutexes.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Foundation and Spikes
**Rationale:** Both the ML inference stack and 3D rendering pipeline have unknowns that must be validated before building features on top. Coordinate system design affects every subsequent phase and has HIGH recovery cost if done wrong.
**Delivers:** Validated ML inference on ANE, skeleton 3D render pipeline (empty scene with depth buffer + perspective camera), canonical coordinate system, threading model documentation.
**Addresses:** Camera feed ingestion (file-based), 3D depth buffer + perspective camera (skeleton).
**Avoids:** Pitfalls 1 (GPU contention -- validate ANE), 4 (coordinate confusion -- define upfront), 12 (threading deadlocks -- design boundaries upfront).

### Phase 2: CV Detection Pipeline
**Rationale:** The CV pipeline is self-contained and independent of 3D rendering. File-based frame ingestion enables deterministic development and testing. Detection + counting + demand adjustment is the core value proposition of v1.2.
**Delivers:** Working YOLO detection from video files, per-class counting, camera-to-network mapping, demand adjustment from observed counts.
**Uses:** ort, ndarray, image, crossbeam, ArcSwap.
**Implements:** velos-cv crate (CameraManager, InferenceRunner, CountAggregator), spawner integration.
**Avoids:** Pitfalls 5 (decode blocking -- dedicated threads), 7 (timing mismatch -- aggregate windowing), 2 (GPU contention -- CPU/ANE inference).

### Phase 3: 3D Rendering Core
**Rationale:** Depends on Phase 1 foundation (depth buffer, camera, coordinates). Can run in parallel with Phase 2 since the renderer is independent of CV. Buildings are the highest-effort 3D feature and should come after the pipeline is proven with simpler geometry.
**Delivers:** 3D road surfaces, 3D agents with LOD, flat terrain ground plane, day/night lighting, 2D/3D view toggle.
**Uses:** gltf, glam, bytemuck, existing wgpu.
**Implements:** velos-render3d crate (Renderer3D, Camera3D, AgentMeshPool).
**Avoids:** Pitfalls 6 (depth buffer breaks 2D -- separate Renderer3D), 8 (NDC depth mismatch -- OPENGL_TO_WGPU_MATRIX), 10 (280K 3D models -- LOD from day one), 15 (egui conflicts -- egui as final pass without depth).

### Phase 4: 3D City Scene
**Rationale:** Building extrusion is the highest-effort 3D feature (50K+ buildings, geometry processing, LOD). It should come after the 3D pipeline is proven with agents and roads. Terrain provides the ground surface for buildings to sit on.
**Delivers:** OSM building extrusions, terrain heightmap (or flat ground plane with water surfaces), camera FOV overlay in 3D.
**Uses:** earcut, geo, osmpbf data from velos-net.
**Implements:** BuildingLoader, TerrainLoader.
**Avoids:** Pitfall 3 (building mesh OOM -- batched atlas + LOD + frustum culling from day one).

### Phase 5: Integration and Enhancement
**Rationale:** Combines CV and 3D capabilities. Requires both pipelines to be stable. Adds features that enhance but are not core to either track.
**Delivers:** CV detection overlay in 3D view, real-time calibration loop, detection confidence heatmap, motorbike-specific fine-tuning (if training data available).
**Avoids:** Pitfall 13 (HCMC motorbike accuracy -- fine-tune with local data).

### Phase 6: Production Hardening
**Rationale:** RTSP live streams, stream reliability, and operational concerns are deployment problems, not development problems. Solve after file-based pipeline is validated.
**Delivers:** Live RTSP camera integration, stream health monitoring, reconnection handling.
**Avoids:** Pitfall 11 (RTSP reliability -- per-stream supervisor with health metrics).

### Phase Ordering Rationale

- **Foundation first:** Coordinate system and threading model have HIGH recovery cost if retrofitted. 2-3 weeks to fix vs. 2-3 days to design upfront.
- **CV and 3D in parallel (Phases 2+3):** Zero architectural dependency between detection pipeline and 3D renderer. Different developers can work simultaneously.
- **Buildings after pipeline proven (Phase 4):** 50K buildings is a significant GPU load. Rendering pipeline must be stable before adding this geometry.
- **Integration last (Phase 5):** Showing CV results in 3D view requires both subsystems working. Natural final phase.
- **RTSP deferred (Phase 6):** File-based input enables deterministic testing. Live streams add operational complexity that distracts from core feature development.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1 (ML Spike):** Run a 2-day spike to validate ANE inference with ort + CoreML EP. Measure ANE utilization via Instruments while simulation compute is active. No existing precedent for this specific combination.
- **Phase 2 (CV Pipeline):** Camera-to-network spatial mapping and demand adjustment algorithms need research into traffic engineering calibration methods (FHWA guidelines, GEH statistic).
- **Phase 4 (Buildings):** GPU-side building extrusion via compute shader is an alternative to CPU-side earcut. Needs benchmarking to determine which approach is faster for 50K+ buildings.

Phases with standard patterns (skip research-phase):
- **Phase 3 (3D Rendering Core):** Depth buffer, perspective camera, instanced mesh rendering, LOD -- all well-documented wgpu patterns with extensive tutorials (learn-wgpu).
- **Phase 6 (RTSP):** retina crate is production-proven in Moonfire NVR. Standard reconnection/health patterns.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM-HIGH | Core crates verified via crates.io API. ort + CoreML integration pattern has limited Rust-specific precedent. geotiff crate is v0.1.0 (LOW confidence, but fallback exists). |
| Features | MEDIUM | Feature landscape well-mapped against competitors. Camera CV for traffic is established domain. Real-time calibration loop is active research with no open-source implementation at this scale. |
| Architecture | MEDIUM | Verified against existing codebase. Two-crate addition is clean. ArcSwap pattern proven in existing velos-predict. 3D rendering patterns sourced from tutorials, not production traffic sim precedent. |
| Pitfalls | MEDIUM-HIGH | Verified against wgpu v28 release notes, Apple Metal docs, ONNX Runtime CoreML docs, YOLO benchmarks. Threading pitfalls are the highest-risk area with hardest recovery. |

**Overall confidence:** MEDIUM

### Gaps to Address

- **ANE inference validation:** No confirmed benchmark of ort + CoreML EP + YOLOv8n on ANE while wgpu compute is active on the same Apple Silicon chip. Must be validated in Phase 1 spike.
- **Building count for POC area:** Estimated 80K-120K buildings but not verified against actual OSM data for Districts 1/3/5/10/Binh Thanh. Run an OSM query to get actual counts before budgeting GPU memory.
- **HCMC motorbike detection accuracy:** No benchmark of COCO-trained YOLO on HCMC traffic footage. Expected to underperform. Gap closes in Phase 5 with fine-tuning, but initial accuracy is unknown.
- **Camera calibration workflow:** Manual camera registration (lat/lon/heading/FOV) is the plan, but no UX design exists for this workflow. Needs design during Phase 2 planning.
- **wgpu version:** Research references both wgpu 27 (current workspace) and wgpu 28 (mentioned in milestone context). Decide before Phase 3 whether to upgrade. Both versions support all required 3D features.

## Sources

### Primary (HIGH confidence)
- Existing VELOS codebase (31K LOC) -- renderer.rs, camera.rs, app.rs, compute.rs, components.rs
- VELOS architecture docs (docs/architect/00-07) -- authoritative design decisions
- [ort crate](https://crates.io/crates/ort) -- v2.0.0-rc.12, 6.8M downloads
- [gltf crate](https://github.com/gltf-rs/gltf) -- v1.4.1, 5.8M downloads
- [retina crate](https://github.com/scottlamb/retina) -- production-proven in Moonfire NVR
- [ONNX Runtime CoreML EP](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html) -- ANE support docs
- [wgpu v28 release notes](https://github.com/gfx-rs/wgpu/releases/tag/v28.0.0) -- mesh shader support on Metal
- [Apple Metal command queue docs](https://developer.apple.com/documentation/metal/mtlcommandqueue)

### Secondary (MEDIUM confidence)
- [Learn wgpu tutorials](https://sotrh.github.io/learn-wgpu/) -- depth buffer, model loading
- [Vision-based vehicle counting (IEEE 2020)](https://ieeexplore.ieee.org/document/9130874/)
- [Real-time OD calibration (MIT)](https://dspace.mit.edu/bitstream/handle/1721.1/129009/1227096827-MIT.pdf)
- [Digital twin intersection monitoring (MDPI 2025)](https://www.mdpi.com/2412-3811/10/8/204)
- [YOLOv8 macOS Metal benchmarks](https://blog.roboflow.com/putting-the-new-m4-macs-to-the-test/)
- [Rust CV Ecosystem 2025](https://andrewodendaal.com/rust-computer-vision-ecosystem/)

### Tertiary (LOW confidence)
- [geotiff crate](https://crates.io/crates/geotiff) -- v0.1.0, 11K downloads, needs validation
- GPU-side building extrusion via compute shader -- theoretical, no Rust traffic sim precedent

---
*Research completed: 2026-03-09*
*Ready for roadmap: yes*
