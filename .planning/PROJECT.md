# VELOS — GPU-Accelerated Traffic Microsimulation

## What This Is

A native macOS desktop application that simulates mixed urban traffic (motorbikes, cars, pedestrians) in real-time using GPU compute. The first slice targets ~1K agents in a small Ho Chi Minh City area, rendered natively via wgpu on Apple Silicon (Metal backend). Built as a pure Rust application using winit for windowing and egui for the dashboard UI.

## Core Value

Motorbikes move realistically through traffic using continuous sublane positioning — not forced into discrete lanes like Western traffic models. If everything else is rough, this must look right.

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] GPU compute pipeline dispatching agent updates via wgpu/Metal
- [ ] Fixed-point arithmetic (Q16.16 position, Q12.20 speed, Q8.8 lateral) for determinism
- [ ] hecs ECS managing agent state with GPU buffer mapping
- [ ] CFL numerical stability checks on simulation timestep
- [ ] Motorbike sublane model with continuous lateral position and filtering behavior
- [ ] Car IDM (Intelligent Driver Model) car-following
- [ ] MOBIL lane-change decision model for cars
- [ ] Pedestrian social force model with adaptive GPU workgroups
- [ ] OSM import of small HCMC road network into graph structure
- [ ] Custom CCH pathfinding with dynamic weight customization
- [ ] rstar R-tree spatial index for neighbor queries
- [ ] Traffic signal control at intersections (fixed-time)
- [ ] Mesoscopic queue model with graduated buffer zone transitions
- [ ] BPR+ETS+historical ensemble for travel time prediction
- [ ] OD matrices and time-of-day demand profiles for agent spawning
- [ ] winit window with wgpu render surface for simulation visualization
- [ ] Native wgpu 2D rendering showing agents moving on road network
- [ ] egui immediate-mode UI for simulation controls (start/stop/speed/reset)
- [ ] egui dashboard panels for real-time metrics and agent statistics
- [ ] Frame time and throughput benchmarks
- [ ] API server (gRPC via tonic + REST via axum) for external/headless access

### Out of Scope

- Multi-GPU / RTX 4090 deployment — macOS single-GPU first
- 280K agent scale — targeting ~1K for this slice
- Full 5-district coverage — one small HCMC area only
- deck.gl web visualization — using native wgpu rendering instead
- FCD/GeoJSON/Parquet data exports — deferred to later milestone
- Calibration / GEH validation — no real-world data comparison yet
- Scenario DSL / batch runner — interactive single-scenario only
- Redis pub/sub / WebSocket scaling — in-process egui handles local control
- OAuth / authentication — single-user desktop app
- CesiumJS 3D visualization — 2D top-down view sufficient

## Context

VELOS has extensive architecture documents in `docs/architect/` (7 documents) designed for a 2x RTX 4090 production deployment. This first slice adapts that architecture to run on a single macOS Apple Silicon machine, proving the core simulation pipeline works before scaling up.

Key differentiator: Southeast Asian mixed traffic where 80% of vehicles are motorbikes that don't follow lane discipline. The sublane model uses continuous lateral positioning (FixedQ8_8) instead of discrete lane assignment.

The codebase currently has architecture docs, presentation slides, and GSD planning tools — no Rust source code yet.

## Constraints

- **Platform**: macOS Apple Silicon (Metal GPU backend via wgpu)
- **Scale**: ~1K agents on a small HCMC road network segment
- **Toolchain**: Rust nightly (Edition 2024) — needs `portable_simd`, async traits
- **App framework**: winit + egui (pure Rust, no webview)
- **UI**: egui immediate-mode GUI rendered via wgpu
- **No external services**: Everything runs locally, no cloud dependencies

## Tech Stack

| Layer | Choice | Rationale |
|-------|--------|-----------|
| Language | Rust nightly (2024 edition) | portable_simd for fixed-point math, async traits |
| GPU | wgpu + WGSL shaders | Cross-platform GPU abstraction, Metal on macOS |
| ECS | hecs | Lightweight, minimal overhead for simulation entities |
| CPU parallel | rayon + tokio | rayon for compute (OSM parse, pathfinding), tokio for async IO |
| Pathfinding | Custom CCH | Full control over dynamic weight customization (3ms update target) |
| Spatial index | rstar | R-tree for neighbor queries in agent interactions |
| Window | winit | Cross-platform windowing, proven with wgpu (used by Bevy) |
| UI | egui + egui-wgpu | Immediate-mode GUI rendered on same wgpu surface as simulation |
| Sim control | In-process | Direct function calls from egui to simulation engine, zero overhead |
| Serialization | bincode (internal) + Parquet (future) | Fast checkpoints now, columnar exports later |
| Fixed-point | Q16.16 / Q12.20 / Q8.8 | Cross-GPU determinism, integer arithmetic in shaders |

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| winit+egui instead of Tauri+React | No webview/wgpu surface conflict, single Rust binary, proven pattern (Bevy ecosystem) | — Decided |
| Custom CCH over rust_road_router | Full control over dynamic weight API, tighter integration | — Pending |
| wgpu native render over deck.gl | Direct GPU access for simulation + rendering, no web overhead | — Pending |
| Nightly Rust | Need portable_simd for fixed-point math performance | — Pending |
| All agent types from start | Motorbike sublane model is the differentiator — can't defer it | — Pending |
| ~1K agents first | Prove pipeline on Metal before scaling to 280K on RTX 4090 | — Pending |

---
*Last updated: 2026-03-06 after initialization*
