# VELOS -- GPU-Accelerated Traffic Microsimulation

## What This Is

A GPU-first traffic microsimulation engine for Ho Chi Minh City that simulates 280K agents (motorbikes, cars, buses, trucks, bicycles, emergency vehicles, pedestrians) in real-time using Rust + wgpu compute shaders. Features motorbike-native sublane model, CCH intelligent routing with prediction-informed rerouting, HCMC-calibrated mixed traffic behavior (red-light creep, aggressive weaving, gap acceptance), SUMO file compatibility, and real-world camera-based demand calibration. Packaged as a Tauri desktop application with web-based dashboard (React + TypeScript) supporting 2D analytics (deck.gl + MapLibre) and 3D city visualization (CesiumJS), backed by a fully self-hosted open-data tile pipeline.

## Core Value

Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models. If everything else is rough, this must look right.

## Requirements

### Validated

- GPU compute pipeline dispatching agent updates via wgpu/Metal -- v1.0
- Motorbike sublane model with continuous lateral position and filtering behavior -- v1.0
- Car IDM car-following + MOBIL lane-change -- v1.0
- Pedestrian basic social force model -- v1.0
- OSM import, A* pathfinding, rstar R-tree spatial index -- v1.0
- Traffic signal control (fixed-time), OD matrices, demand profiles -- v1.0
- egui dashboard with simulation controls and real-time metrics -- v1.0
- GPU-instanced 2D rendering with styled shapes, zoom/pan -- v1.0
- GPU-first physics with per-lane wave-front dispatch at 280K-agent scale -- v1.1
- Multi-GPU partitioning (METIS) with boundary agent protocol -- v1.1
- Fixed-point arithmetic (Q16.16/Q12.20/Q8.8) for cross-GPU determinism -- v1.1
- Krauss car-following model (SUMO default) + runtime-selectable per agent -- v1.1
- 5-district HCMC road network (Districts 1, 3, 5, 10, Binh Thanh, ~25K edges) -- v1.1
- SUMO .net.xml network import + .rou.xml demand import -- v1.1
- All 7 vehicle types: motorbike, car, bus (GTFS), truck, bicycle, emergency, pedestrian -- v1.1
- Bus dwell lifecycle with empirical model + GTFS 130 HCMC routes -- v1.1
- Pedestrian adaptive GPU workgroups with prefix-sum compaction -- v1.1
- Emergency vehicle yield + signal priority -- v1.1
- Meso-micro hybrid with 100m buffer zones and velocity-matching -- v1.1
- CCH pathfinding with dynamic weight customization (3ms update) -- v1.1
- 8 agent profiles (Commuter, Bus, Truck, Emergency, Tourist, Teen, Senior, Cyclist) with multi-factor cost -- v1.1
- GPU perception + evaluation phases for autonomous agent decisions -- v1.1
- BPR+ETS+historical prediction ensemble, staggered reroute (1K/step) -- v1.1
- Actuated + adaptive signal control, SPaT/GLOSA broadcast, V2I -- v1.1
- Traffic sign interaction (speed limits, stop/yield, no-turn, school zones) -- v1.1
- HCMC behavior tuning: red-light creep, aggressive weaving, yield-based gap acceptance -- v1.1
- All vehicle params externalized to TOML config with GPU/CPU parity -- v1.1
- gRPC detection ingestion with camera registration and streaming counts -- v1.2
- Spatial mapping of camera FOV to simulation network edges -- v1.2
- Real-time demand calibration from detection aggregation with EMA smoothing -- v1.2
- Intersection sublane model with Bezier turn paths and junction conflict resolution -- v1.2
- Self-hosted PMTiles map tile rendering in egui -- v1.2
- 3D renderer (wgpu native): buildings from OSM earcut extrusion, SRTM terrain, road surfaces -- v1.2
- 3D agent rendering with mesh/billboard LOD and orbit camera -- v1.2
- Streaming calibration with window-change detection and staleness decay -- v1.2

### Active

## Current Milestone: v1.3 Web Dashboard

**Goal:** Replace egui native renderer with a Tauri-based web dashboard featuring 2D analytics (deck.gl + MapLibre) and 3D city visualization (CesiumJS), backed by a fully self-hosted open-data tile pipeline served via Martin and Docker Compose.

**Target features:**
- Tauri app shell wrapping Rust simulation backend with WebView frontend
- Tauri IPC commands for simulation control (play/pause/step/speed/reset)
- WebSocket streaming for high-frequency agent position data (10 Hz)
- 2D analytics view: deck.gl ScatterplotLayer (280K agents), HeatmapLayer, flow arrows on MapLibre base map
- 3D city view: CesiumJS with self-hosted 3D Tiles buildings, quantized-mesh terrain, agent overlay
- Tab/toggle button to switch between 2D and 3D views
- Dashboard panels: KPIs (agent count, avg speed, LOS, frame time), calibration status, speed/demand charts
- Self-hosted tile pipeline: Martin (Rust) vector tile server, SRTM terrain, OSM building extrusion → 3D Tiles
- Full Docker Compose infrastructure (Martin, PostGIS, Nginx, TileServer GL)
- HCMC 5-district tile data generation with reusable add-city pipeline
- Remove egui and native wgpu renderer entirely

### Out of Scope

- Wiedemann 99 car-following -- requires PTV-calibrated datasets unavailable for HCMC
- SUMO TraCI compatibility -- synchronous single-threaded protocol incompatible with GPU-parallel execution
- Real-time sensor data fusion -- requires data partnerships; offline historical data sufficient
- ML/DL prediction (PyTorch/TF) -- Python sidecar latency; in-process BPR+ETS+historical sufficient
- Multi-node distributed sim -- 280K agents fit on single-node 2-4 GPUs

## Context

Shipped v1.1 SUMO Replacement Engine (31,780 Rust LOC + 1,501 WGSL LOC, 168 commits, 45/45 requirements).
Shipped v1.2 Digital Twin (20 phases, 17 plans): gRPC camera detection ingestion, intersection sublane model, PMTiles map tiles, full 3D native renderer (buildings, terrain, roads, agent LOD), streaming calibration.
Current codebase: Rust nightly (2024 edition), wgpu 28 (Metal backend), hecs ECS, petgraph, rstar, egui, tonic gRPC.

v1.3 architectural shift: egui + native wgpu rendering → Tauri + web dashboard. Simulation engine stays in Rust, visualization moves to browser tech stack inside Tauri WebView. Communication via Tauri IPC (commands) + WebSocket (agent streaming).

Known tech debt: sublane.rs CREEP/GAP constants could be wired to config (values already match defaults). Multi-GPU boundary protocol validated with logical partitions; physical multi-adapter untested. Critical performance regression at 8K agents (83.4ms frame time).

## Constraints

- **Platform**: macOS Apple Silicon (Metal GPU backend via wgpu)
- **Scale**: 280K agents on 5-district HCMC road network (~25K edges)
- **Toolchain**: Rust nightly (Edition 2024) -- needs portable_simd, async traits
- **App framework**: Tauri 2 (Rust backend + WebView frontend, replacing winit+egui)
- **Arithmetic**: Fixed-point (Q16.16/Q12.20/Q8.8) for GPU; f64 on CPU reference paths
- **No external services**: Everything runs locally, no cloud dependencies

## Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Language | Rust nightly (2024 edition) | portable_simd, async traits |
| GPU | wgpu + WGSL shaders | Cross-platform GPU abstraction, Metal on macOS |
| ECS | hecs | Lightweight, minimal overhead for simulation entities |
| CPU parallel | rayon + tokio | rayon for compute, tokio for async IO |
| Pathfinding | CCH (custom) | 3ms dynamic weight updates, 0.02ms/query on 25K edges |
| Prediction | BPR+ETS+historical ensemble | In-process, ArcSwap lock-free overlay |
| Spatial index | rstar | R-tree for neighbor queries in agent interactions |
| App shell | Tauri 2 | Rust backend + WebView, single binary distribution |
| Frontend | React + TypeScript | Dashboard UI inside Tauri WebView |
| 2D viz | deck.gl + MapLibre GL JS | GPU-accelerated 280K agent scatter, heatmaps, flow arrows |
| 3D viz | CesiumJS (self-hosted, no Ion) | 3D Tiles buildings, quantized-mesh terrain, agent overlay |
| Tile server | Martin (Rust) | Dynamic vector tiles from PostGIS |
| Sim control | Tauri IPC + WebSocket | IPC for commands, WebSocket for 10 Hz agent streaming |
| Config | TOML (vehicle_params.toml) | Per-vehicle-type behavior parameters |
| Serialization | postcard (graph), bincode (internal) | Fast compact serialization |

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| f64 CPU / f32 GPU for v1.0, fixed-point for v1.1 | POC first, determinism at scale | Good |
| Per-lane wave-front (Gauss-Seidel) dispatch | Convergence at 280K scale, GPU-friendly | Good |
| Custom CCH instead of library | No Rust CCH crate exists; BFS bisection ordering | Good |
| In-process BPR+ETS+historical prediction | No Python bridge latency, ArcSwap lock-free | Good |
| Krauss + IDM dual car-following | SUMO compatibility (Krauss) + academic standard (IDM) | Good |
| METIS k-way partitioning (BFS fallback) | Balanced GPU load; libmetis fails on macOS | Good |
| GPU perception + evaluation pipeline | Autonomous agent decisions at GPU scale | Good |
| Polymorphic signal controllers (trait dispatch) | Fixed/actuated/adaptive via Box\<dyn\> | Good |
| TOML vehicle config with GPU uniform buffer | Externalized params, GPU/CPU parity | Good |
| CSV GTFS parser (not gtfs-structures) | Handles non-standard HCMC data, lighter deps | Good |
| winit+egui instead of Tauri+React | No webview conflict, single Rust binary | Good |
| Nightly Rust | Need portable_simd for math performance | Good |
| ~12 crate workspace with 700-line limit | Clean separation, manageable files | Good |
| Motorbike sublane: probe-based gap scanning | 0.3m step, obstacle-edge sweep for swarming | Good |
| gRPC ingestion (not built-in YOLO) | External CV pushes detections to VELOS | Good |
| New Renderer3D (not retrofit 2D) | Separate render pipeline, independent of existing 2D | Good |
| OSM building extrusion via earcut | No external 3D datasets for HCMC | Good |
| Tauri + React replacing winit+egui | Web tech for visualization, Rust for simulation; richer UI | -- Pending |
| Tauri IPC + WebSocket (not pure IPC) | IPC for low-freq commands, WebSocket for high-freq streaming | -- Pending |
| CesiumJS for 3D (not MapLibre 3D) | Full 3D globe, 3D Tiles support, terrain; matches docs | -- Pending |
| Martin for tile serving (not PMTiles) | Dynamic tiles from PostGIS, flexible for dev | -- Pending |
| Remove egui entirely (not keep as fallback) | Single rendering path, avoid maintaining two UIs | -- Pending |
| Docker Compose full stack | Martin + PostGIS + Nginx + TileServer GL for self-hosted tiles | -- Pending |

---
*Last updated: 2026-03-12 after v1.3 milestone start*
