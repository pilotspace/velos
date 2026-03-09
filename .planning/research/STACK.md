# Stack Research: Camera CV Detection + 3D Native Rendering

**Domain:** Camera-based vehicle/pedestrian detection pipeline and 3D wgpu city visualization for GPU-accelerated traffic microsimulation
**Researched:** 2026-03-09
**Confidence:** MEDIUM-HIGH (core crates verified via crates.io API and official repos; some integration patterns are LOW confidence due to limited Rust-specific precedent)

## Scope

This document covers ONLY the stack additions needed for two new capabilities:

1. **Camera CV Pipeline** -- RTSP camera feed ingestion, video decoding, YOLO-based vehicle/pedestrian detection, demand calibration feedback loop
2. **3D Native Rendering** -- wgpu-based 3D city visualization with OSM building extrusion, glTF vehicle models, terrain/DEM, replacing the current 2D GPU-instanced rendering

The existing v1.0 stack (Rust nightly 2024 edition, wgpu 27, hecs, petgraph, rstar, egui, winit, glam, rayon) and v1.1 additions (tonic, axum, parquet, etc.) are NOT re-researched.

---

## Part 1: Camera CV Detection Pipeline

### ML Inference Runtime

| Crate | Version | Purpose | Why Recommended |
|-------|---------|---------|-----------------|
| ort | 2.0.0-rc.12 | ONNX Runtime bindings for ML model inference | The dominant Rust ML inference crate (6.8M downloads). Wraps Microsoft ONNX Runtime which supports CoreML execution provider for macOS Apple Silicon GPU acceleration. Production-ready despite RC status -- pykeio recommends it for new projects. Supports YOLO models exported from PyTorch via ONNX. |

**Why ort over alternatives:**

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| ort (ONNX Runtime) | tract | tract is pure-Rust and CPU-only (no Metal/CoreML GPU acceleration). For YOLO at 10-30 FPS on camera feeds, CPU-only inference is too slow. tract is better for lightweight models or WASM targets. |
| ort (ONNX Runtime) | candle | candle (Hugging Face) is optimized for LLM/transformer workloads, not object detection. Metal support exists via metal-candle but the YOLO model zoo is ONNX-native, not candle-native. Would require porting model weights. |
| ort (ONNX Runtime) | tch-rs | tch-rs wraps libtorch (PyTorch C++). Massive binary (~2GB), poor macOS Metal support, complex build. ONNX Runtime is lighter and has first-class CoreML. |
| ort (ONNX Runtime) | burn | burn is a training-focused framework. Inference support exists but the ONNX import is less mature than ort. Better for training Rust-native models from scratch. |

**ort feature flags needed:**

```toml
[dependencies]
ort = { version = "2.0.0-rc.12", features = ["coreml"] }
```

The `coreml` feature enables Apple's CoreML execution provider, which delegates supported ops to the Apple Neural Engine (ANE) or Metal GPU. This is critical for achieving real-time inference on Apple Silicon without discrete GPU.

**Detection model recommendation:** YOLO11n (nano) or YOLOv8n exported to ONNX format via Ultralytics. The nano variants balance speed vs accuracy for traffic counting. Export command: `yolo export model=yolo11n.pt format=onnx`. Pre-trained COCO weights already detect vehicles (car, truck, bus, motorcycle) and persons. Fine-tune on HCMC traffic camera footage for motorbike-heavy scenes.

**Confidence:** HIGH -- ort + CoreML on macOS is well-documented, YOLO ONNX export is a standard workflow.

### Video Ingestion (RTSP + Decoding)

| Crate | Version | Purpose | Why Recommended |
|-------|---------|---------|-----------------|
| retina | 0.4.17 | Pure-Rust RTSP client for IP camera streams | Production-proven in Moonfire NVR. Handles RTSP/1.0, RTP over TCP (interleaved), H.264/H.265 depacketization. No FFmpeg dependency for the RTSP protocol layer. ~62K downloads. |
| ffmpeg-next | 8.0.0 | Video frame decoding (H.264/H.265 to raw pixels) | Safe FFmpeg wrapper (2M downloads). Needed because retina gives you NAL units, not decoded frames. FFmpeg's VideoToolbox decoder on macOS provides hardware-accelerated H.264/H.265 decoding via Apple Silicon media engine. |
| image | 0.25.9 | Frame format conversion (RGB/RGBA buffers) | Standard Rust image processing. Convert decoded frames to tensor-compatible layouts for ort inference. Already widely used in the Rust ecosystem (103M downloads). |

**Architecture: retina + ffmpeg-next (not ffmpeg-next alone)**

Using retina for RTSP and ffmpeg-next only for frame decoding is cleaner than using ffmpeg-next for everything because:
- retina handles RTSP reconnection, authentication, and stream negotiation in pure Rust with async/await (tokio-compatible)
- ffmpeg-next's RTSP client is synchronous and harder to integrate with async Rust
- Separation of concerns: network protocol vs codec decoding

**Alternative considered:**

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| retina + ffmpeg-next | GStreamer (gstreamer-rs) | GStreamer is a full media framework -- massive dependency graph, complex to build on macOS, overkill for "decode RTSP to frames." retina + ffmpeg-next is surgical. |
| retina + ffmpeg-next | video-rs 0.11 | video-rs wraps ffmpeg-next with a simpler API, but doesn't support RTSP natively. Would still need retina for camera ingestion. Adds an abstraction layer without benefit. |
| retina + ffmpeg-next | nokhwa 0.10 | nokhwa is for local webcams (USB/V4L2), not IP cameras with RTSP. Wrong tool for traffic cameras. |

**System dependency:** FFmpeg 6.x or 7.x must be installed on the build machine. On macOS: `brew install ffmpeg`. The ffmpeg-next crate links against libavcodec, libavformat, libavutil, libswscale.

**Confidence:** HIGH -- retina is production-proven in NVR systems; ffmpeg-next is the standard Rust FFmpeg binding.

### Supporting Libraries for CV Pipeline

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| ndarray | 0.16.x | Tensor manipulation for pre/post-processing | Resize, normalize, transpose frames before ort inference. NMS (non-max suppression) post-processing on detection outputs. |
| imageproc | 0.25.x | Image processing primitives | Bounding box drawing on debug frames, resize operations as alternative to ndarray. |
| tokio | 1.47+ | Async runtime for camera stream tasks | Already in v1.1 stack. Camera ingestion runs as tokio tasks, one per camera feed. |

---

## Part 2: 3D Native wgpu Rendering

### 3D Model Loading

| Crate | Version | Purpose | Why Recommended |
|-------|---------|---------|-----------------|
| gltf | 1.4.1 | glTF 2.0 / GLB model loading | The standard Rust glTF loader (5.8M downloads). Parses meshes, materials, textures, animations. Outputs vertex data ready for wgpu buffer upload. Used by Bevy, rend3, and every major Rust 3D project. |

**Why glTF format:** glTF is the "JPEG of 3D" -- industry standard, supported by Blender export, optimized for GPU upload (binary buffers match GPU vertex layout). Vehicle models (motorbike, car, bus, truck) and street furniture (traffic lights, signs) should be authored as GLB files.

**Integration with existing wgpu renderer:** The current 2D renderer uses GPU instancing with styled shapes. The 3D renderer will:
1. Parse GLB with `gltf` crate at startup
2. Upload vertex/index buffers to wgpu
3. Use instanced draw calls (one draw per vehicle type, instance buffer has transform + color)
4. This mirrors the existing 2D instancing pattern but with 3D meshes instead of quads

**Confidence:** HIGH -- gltf is the uncontested standard.

### OSM Building Extrusion (Custom Rust)

| Crate | Version | Purpose | Why Recommended |
|-------|---------|---------|-----------------|
| earcut | 0.4.5 | Polygon triangulation for building footprint caps | Rust port of Mapbox's earcut algorithm. Handles concave polygons with holes (common in building footprints). Fast, no-alloc design. Used by maplibre-rs. |
| lyon | 1.0.19 | Path tessellation for complex geometry | Alternative/complement to earcut for road surfaces, park boundaries, water bodies. Handles Bezier curves and stroke tessellation. 3.3M downloads. |
| geo | 0.29.x | Geometric operations on OSM polygons | Boolean operations, simplification, coordinate transforms. Already implied by osmpbf usage in velos-net. |

**Why custom Rust over existing tools:**

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| Custom Rust (earcut + osmpbf) | osm2world (Java) | Java sidecar process. Would need IPC, adds JVM dependency. The extrusion algorithm is straightforward: parse OSM building polygons, read `building:levels` or `height` tag, extrude walls, triangulate caps with earcut. ~500 lines of Rust. |
| Custom Rust (earcut + osmpbf) | py3dtilers (Python) | Python sidecar. Violates project principle of no Python bridge. Outputs 3D Tiles format which then needs parsing -- double conversion. |
| Custom Rust (earcut + osmpbf) | Cesium OSM Buildings | Commercial service (Cesium Ion). Requires internet, API key, 3D Tiles streaming. Not self-hosted. |

**Building extrusion algorithm (implemented in a new `velos-city` crate):**
1. Filter OSM ways/relations with `building=*` tag (already parsed by osmpbf in velos-net)
2. Project lat/lon to local meters (UTM zone 48N for HCMC)
3. Read `building:levels` tag (default 3 for HCMC), multiply by 3.0m per level for height
4. Generate wall quads by extruding each edge of the footprint polygon
5. Triangulate top/bottom caps with earcut (handles concave polygons + holes)
6. Output vertex buffer (position + normal) ready for wgpu upload
7. Batch all buildings into a single GPU buffer with indirect draw

**Confidence:** HIGH -- earcut is well-proven, the algorithm is well-understood, OSM data is already parsed.

### Terrain / DEM

| Crate | Version | Purpose | Why Recommended |
|-------|---------|---------|-----------------|
| tiff | 0.11.3 | GeoTIFF raster parsing | Low-level TIFF reader (62M downloads via image crate dependency). Read SRTM or ALOS DEM elevation data. |
| geotiff | 0.1.0 | GeoTIFF coordinate-aware reading | Adds geospatial metadata (CRS, transform) on top of tiff crate. Early but functional (11K downloads). Built specifically for DEM/elevation use cases. |

**Practical note on DEM for HCMC:** Ho Chi Minh City is extremely flat (1-10m elevation). Terrain mesh adds visual fidelity but has near-zero impact on simulation accuracy. Recommend deferring DEM integration to a polish phase.

**DEM mesh generation approach:**
1. Parse SRTM 30m GeoTIFF for HCMC region (~small file, covers Districts 1/3/5/10/BT)
2. Generate regular grid mesh (vertices at each elevation sample, 30m spacing)
3. Apply Delaunay or regular grid triangulation
4. Upload as single static wgpu vertex buffer
5. Drape road network and buildings on top via vertex shader height offset

**Alternative considered:**

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| geotiff + custom mesh | tin-terrain (C++) | C++ CLI tool, not a library. Would need subprocess call + file I/O. The mesh generation from a regular grid is trivial in Rust (~200 lines). |

**Confidence:** MEDIUM -- geotiff crate is v0.1.0 with low downloads. Fallback: use tiff crate directly and parse GeoTIFF metadata manually (well-documented format).

### 3D Rendering Infrastructure

| Crate | Version | Purpose | Why Recommended |
|-------|---------|---------|-----------------|
| glam | 0.29 | 3D math (Mat4, Vec3, Quat) | Already in workspace. Handles view/projection matrices, camera transforms, frustum culling. No change needed. |
| bytemuck | 1.x | Safe casting for GPU buffer uploads | Already in workspace. Cast Rust structs to &[u8] for wgpu buffer writes. No change needed. |
| wgpu | 27 (current) | GPU rendering backend | Already in workspace. Supports 3D render passes, depth buffers, MSAA. The existing 2D renderer already uses wgpu -- extend with depth attachment and 3D pipeline. No version change required for 3D support. |

**3D rendering additions needed (no new crates, just new shaders + pipeline config):**
- Depth buffer attachment (wgpu::TextureFormat::Depth32Float)
- 3D vertex shader with MVP matrix uniform
- Phong or PBR fragment shader for buildings/vehicles
- Shadow mapping (optional, adds significant complexity)
- Frustum culling (CPU-side with glam, or GPU compute shader)

**Confidence:** HIGH -- all required crates are already in the workspace.

### Map Tile Generation (Offline Tooling)

| Tool | Purpose | Why Recommended |
|------|---------|-----------------|
| Planetiler | Generate PMTiles vector tiles from OSM PBF | Java CLI tool, runs offline during data prep. Generates planet-scale vector tiles. Actively maintained, used by Protomaps for daily builds. Outputs PMTiles format which the project already uses (served by Nginx per 06-infrastructure.md). |

**Why Planetiler over alternatives:**

| Recommended | Alternative | Why Not |
|-------------|-------------|---------|
| Planetiler | tilemaker | tilemaker's last release was 2022, GitHub issues unmonitored. Planetiler is actively maintained and faster at scale. |
| Planetiler | Martin (Rust) | Martin is a tile *server* that generates tiles on-the-fly from PostGIS. The project uses PMTiles (static files, no database). Martin is the wrong tool for static tile generation. Already excluded in CLAUDE.md. |
| Planetiler | tippecanoe | tippecanoe converts GeoJSON to vector tiles. Planetiler reads OSM PBF directly -- no intermediate GeoJSON step. More efficient for OSM data. |

**Note:** Planetiler is an offline build tool, not a Rust dependency. It runs during data pipeline preparation: `java -jar planetiler.jar --osm-path=hcmc.osm.pbf --output=hcmc.pmtiles`. The output PMTiles file is served statically.

**Confidence:** HIGH -- Planetiler is the community standard for PMTiles generation.

---

## Installation Summary

### Rust Dependencies (Cargo.toml additions)

```toml
[workspace.dependencies]
# CV Pipeline
ort = { version = "2.0.0-rc.12", features = ["coreml"] }
retina = "0.4"
ffmpeg-next = "8"
ndarray = "0.16"
image = "0.25"

# 3D Rendering
gltf = "1.4"
earcut = "0.4"
lyon = "1.0"
geo = "0.29"

# Terrain (defer to polish phase)
# geotiff = "0.1"
# tiff = "0.11"
```

### System Dependencies (macOS)

```bash
# FFmpeg for video decoding (required by ffmpeg-next)
brew install ffmpeg

# ONNX Runtime downloads automatically via ort's download strategy
# No manual install needed -- ort fetches the correct dylib at build time

# Planetiler for tile generation (offline tool)
# Download from https://github.com/onthegomap/planetiler/releases
```

### New Crates to Create

| Crate | Dependencies | Responsibility |
|-------|-------------|----------------|
| velos-cv | ort, retina, ffmpeg-next, ndarray, image, tokio | Camera feed ingestion, YOLO inference, detection post-processing, count aggregation |
| velos-city | earcut, lyon, geo, gltf, osmpbf | OSM building extrusion, 3D mesh generation, glTF vehicle model loading, terrain mesh |

---

## Version Compatibility Matrix

| Package | Compatible With | Notes |
|---------|-----------------|-------|
| ort 2.0.0-rc.12 | ONNX Runtime 1.20+ | Auto-downloads correct ORT version. CoreML EP requires macOS 12+. |
| retina 0.4.17 | tokio 1.x | Async RTSP client, shares tokio runtime with axum/tonic from v1.1 stack. |
| ffmpeg-next 8.0.0 | FFmpeg 6.x-8.x | Links against system FFmpeg. Verify with `pkg-config --modversion libavcodec`. |
| gltf 1.4.1 | Any wgpu version | Pure parser, outputs vertex data. No GPU dependency. |
| earcut 0.4.5 | No external deps | Pure Rust, no-std compatible. |
| ort 2.0.0-rc.12 | Rust 2024 edition | Verified: ort supports modern Rust editions. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| OpenCV (opencv-rust) | Massive C++ dependency (1GB+), complex build, most features unused. Only needed if doing advanced image processing beyond detection. | ort for inference + image/ndarray for pre/post-processing |
| Python sidecar (ultralytics) | Violates project principle. IPC overhead, deployment complexity, two runtimes. | Export YOLO to ONNX once (Python), run inference in Rust (ort) forever |
| GStreamer (gstreamer-rs) | Full media framework overkill. Complex macOS build (dozens of plugins). | retina + ffmpeg-next for surgical RTSP + decode |
| rend3 | 3D renderer built on wgpu, but opinionated about scene graph, materials, lighting. Would conflict with existing custom wgpu renderer and ECS architecture. | Extend existing wgpu renderer with 3D pipeline |
| Bevy (bevy_render) | Full game engine. Cannot integrate with existing hecs ECS and custom wgpu usage. | Keep custom renderer, add 3D capabilities directly |
| three-d | Another 3D renderer. Same problem as rend3 -- owns the render loop. | Custom wgpu 3D pipeline |
| 3D Tiles (Cesium) | Commercial streaming format. Requires Cesium Ion subscription or complex self-hosting. Overkill for static city visualization. | Custom OSM building extrusion with earcut |
| tch-rs (PyTorch) | 2GB libtorch binary, poor macOS Metal support, complex linking. | ort with CoreML execution provider |

---

## Stack Patterns by Use Case

**If adding more camera feeds (>4 simultaneous):**
- Use tokio::spawn per camera with bounded channels
- Consider ffmpeg-next hardware decoder pool (VideoToolbox sessions are limited on macOS)
- ort supports batched inference -- accumulate frames from multiple cameras into one batch

**If detection accuracy is insufficient with YOLO11n:**
- Step up to YOLO11s (small) or YOLO11m (medium) -- larger but more accurate
- Fine-tune on HCMC traffic dataset (motorbike-heavy, helmet variations)
- Consider two-stage: YOLO for detection + lightweight classifier for vehicle type

**If 3D rendering performance degrades with many buildings:**
- LOD (Level of Detail): simplified geometry for distant buildings
- Frustum culling: skip buildings outside camera view (glam AABB test)
- Occlusion culling: skip buildings behind other buildings (GPU occlusion queries or Hi-Z buffer)
- Instancing: batch identical building geometries (common in residential areas)

**If geotiff crate is too immature:**
- Use tiff crate directly + manual GeoTIFF metadata parsing
- GeoTIFF is just TIFF + specific TIFF tags (ModelTiepointTag, ModelPixelScaleTag, GeoKeyDirectoryTag)
- ~100 lines of custom code to extract elevation grid from SRTM GeoTIFF

---

## Sources

- [ort crate (crates.io)](https://crates.io/crates/ort) -- v2.0.0-rc.12, 6.8M downloads, verified via crates.io API
- [ort documentation](https://ort.pyke.io/) -- CoreML execution provider setup, download strategy
- [ONNX Runtime CoreML EP](https://onnxruntime.ai/docs/execution-providers/CoreML-ExecutionProvider.html) -- macOS support, op coverage
- [retina crate (GitHub)](https://github.com/scottlamb/retina) -- v0.4.17, RTSP protocol handling, production use in Moonfire NVR
- [ffmpeg-next (crates.io)](https://crates.io/crates/ffmpeg-next) -- v8.0.0, 2M downloads, FFmpeg 6-8 compatibility
- [gltf crate (GitHub)](https://github.com/gltf-rs/gltf) -- v1.4.1, 5.8M downloads, glTF 2.0 spec compliance
- [earcut (crates.io)](https://crates.io/crates/earcut) -- v0.4.5, Mapbox earcut port, concave polygon + holes support
- [Ultralytics YOLO docs](https://docs.ultralytics.com/models/yolo11/) -- ONNX export workflow, model variants (n/s/m/l/x)
- [Planetiler (GitHub)](https://github.com/onthegomap/planetiler) -- PMTiles generation from OSM PBF
- [geotiff (crates.io)](https://crates.io/crates/geotiff) -- v0.1.0, early but purpose-built for DEM reading
- [lyon (crates.io)](https://crates.io/crates/lyon) -- v1.0.19, 3.3M downloads, path tessellation

---
*Stack research for: Camera CV detection + 3D native wgpu rendering*
*Researched: 2026-03-09*
