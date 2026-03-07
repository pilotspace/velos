# VELOS -- GPU-Accelerated Traffic Microsimulation

## What This Is

A native macOS desktop application that simulates mixed urban traffic (motorbikes, cars, pedestrians) in real-time using GPU compute. Runs ~1.5K agents on an HCMC District 1 road network at 30+ FPS, rendered natively via wgpu on Apple Silicon (Metal backend). Built as a pure Rust application using winit for windowing and egui for the dashboard UI.

## Core Value

Motorbikes move realistically through traffic using continuous sublane positioning -- not forced into discrete lanes like Western traffic models. If everything else is rough, this must look right.

## Requirements

### Validated

- GPU compute pipeline dispatching agent updates via wgpu/Metal -- v1.0
- f64 CPU / f32 GPU arithmetic (no fixed-point for POC) -- v1.0
- hecs ECS managing agent state with GPU buffer mapping -- v1.0
- CFL numerical stability checks on simulation timestep -- v1.0
- Motorbike sublane model with continuous lateral position and filtering behavior -- v1.0
- Car IDM (Intelligent Driver Model) car-following -- v1.0
- MOBIL lane-change decision model for cars -- v1.0
- Pedestrian basic social force model (repulsion + attraction, no adaptive workgroups) -- v1.0
- OSM import of small HCMC road network into graph structure -- v1.0
- A* pathfinding on petgraph (no CCH, no rerouting) -- v1.0
- rstar R-tree spatial index for neighbor queries -- v1.0
- Traffic signal control at intersections (fixed-time) -- v1.0
- OD matrices and time-of-day demand profiles for agent spawning -- v1.0
- winit window with wgpu render surface -- v1.0
- GPU-instanced 2D rendering with styled agent shapes and direction arrows -- v1.0
- Zoom/pan camera, visible road lanes, intersection areas -- v1.0
- egui immediate-mode UI for simulation controls (start/stop/speed/reset) -- v1.0
- egui dashboard panels for real-time metrics and agent statistics -- v1.0
- Frame time and throughput benchmarks -- v1.0
- Gridlock detection at intersections -- v1.0

### Active

(Empty -- define with `/gsd:new-milestone`)

### Out of Scope

- Multi-GPU / RTX 4090 deployment -- macOS single-GPU first
- 280K agent scale -- targeting ~1K for this slice
- Full 5-district coverage -- one small HCMC area only
- Fixed-point arithmetic (Q16.16/Q12.20/Q8.8) -- deferred to scale-up phase
- Wave-front (Gauss-Seidel) dispatch -- simple parallel dispatch for POC
- CCH pathfinding -- A* on petgraph sufficient for 1K agents
- Prediction ensemble (BPR+ETS+historical) -- no travel time prediction
- Mesoscopic queue model / meso-micro hybrid -- full micro only
- Dynamic rerouting -- agents follow initial A* path
- Bicycle agents -- deferred
- Pedestrian adaptive GPU workgroups -- basic social force only
- deck.gl web visualization -- using native wgpu rendering instead
- FCD/GeoJSON/Parquet data exports -- deferred to later milestone
- Calibration / GEH validation -- no real-world data comparison yet
- Scenario DSL / batch runner -- interactive single-scenario only
- Redis pub/sub / WebSocket scaling -- in-process egui handles local control
- OAuth / authentication -- single-user desktop app
- CesiumJS 3D visualization -- 2D top-down view sufficient
- API server (gRPC/REST) -- deferred to v2

## Context

Shipped v1.0 MVP with 7,802 Rust LOC + 117 WGSL LOC across 6 crates.
Tech stack: Rust nightly (2024 edition), wgpu 28 (Metal backend), hecs ECS, petgraph, rstar, egui.
185 tests passing, 4/4 E2E flows verified, 25/25 requirements satisfied.

Known tech debt: GPU compute pipeline proven via tests but not wired into main sim loop (CPU-side ECS physics sufficient at 1.5K agents). `should_jaywalk()` tested but not wired into sim loop.

Initial visual verification confirmed motorbike filtering, swarming, dispersal, and pedestrian social force behavior look correct.

## Constraints

- **Platform**: macOS Apple Silicon (Metal GPU backend via wgpu)
- **Scale**: ~1.5K agents on HCMC District 1 road network
- **Toolchain**: Rust nightly (Edition 2024) -- needs portable_simd, async traits
- **App framework**: winit + egui (pure Rust, no webview)
- **UI**: egui immediate-mode GUI rendered via wgpu
- **Arithmetic**: f64 on CPU, f32 on GPU (no fixed-point for POC)
- **No external services**: Everything runs locally, no cloud dependencies

## Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Language | Rust nightly (2024 edition) | portable_simd, async traits |
| GPU | wgpu + WGSL shaders | Cross-platform GPU abstraction, Metal on macOS |
| ECS | hecs | Lightweight, minimal overhead for simulation entities |
| CPU parallel | rayon + tokio | rayon for compute (OSM parse, pathfinding), tokio for async IO |
| Pathfinding | A* on petgraph | Simple, sufficient for 1K agents on small network |
| Spatial index | rstar | R-tree for neighbor queries in agent interactions |
| Window | winit | Cross-platform windowing, proven with wgpu (used by Bevy) |
| UI | egui + egui-wgpu | Immediate-mode GUI rendered on same wgpu surface as simulation |
| Rendering | GPU-instanced wgpu 2D | Styled shapes, direction arrows, zoom/pan, one draw call per type |
| Sim control | In-process | Direct function calls from egui to simulation engine, zero overhead |
| Serialization | bincode (internal) + Parquet (future) | Fast checkpoints now, columnar exports later |

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| f64 CPU / f32 GPU instead of fixed-point | No emulated i64 in WGSL, no golden vectors. Determinism deferred to 280K scale | Good |
| Simple parallel dispatch instead of wave-front | Wave-front matters at 280K for convergence, not at 1K POC scale | Good |
| A* on petgraph instead of custom CCH | CCH is massive custom work. A* sufficient for small network, no rerouting | Good |
| No prediction/meso-micro | These are scale features. POC proves simulation pipeline, not optimization | Good |
| Motorbikes + cars + pedestrians (no bicycles) | Core differentiator + essential interactions. Bicycles deferred | Good |
| Styled + instanced rendering | GPU-instanced draw calls, styled shapes with direction arrows, zoom/pan | Good |
| Rendering from Phase 1 | Visual feedback from day one. Minimal window with dots, grows with features | Good |
| egui in Phase 2 | Add controls when there's real simulation to control | Good |
| winit+egui instead of Tauri+React | No webview/wgpu surface conflict, single Rust binary, proven pattern | Good |
| Nightly Rust | Need portable_simd for math performance | Good |
| ~1K agents first | Prove pipeline on Metal before scaling to 280K on RTX 4090 | Good |
| Keep ~12 crate structure | Create crates as needed, split at 700 lines | Good |
| BFS visited-set for gridlock | Simpler than Tarjan SCC, sufficient at POC scale | Good |
| Pure CPU math models (IDM/MOBIL/signal) | f64 precision, zero external deps beyond thiserror/log | Good |
| Probe-based gap scanning for sublane | 0.3m step size, obstacle-edge sweep for swarming | Good |
| Spatial query radius 6m + 20-neighbor cap | Prevents O(n^2) at density; heading filter prevents deadlocks | Good |
| Linear drift over 2s for lane changes | Simple, visually smooth, no complex interpolation needed | Good |

---
*Last updated: 2026-03-07 after v1.0 milestone*
