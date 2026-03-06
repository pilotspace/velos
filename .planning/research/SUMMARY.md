# Project Research Summary

**Project:** VELOS
**Domain:** GPU-accelerated traffic microsimulation (motorbike-native, Southeast Asian mixed traffic, macOS/Metal desktop)
**Researched:** 2026-03-06
**Confidence:** MEDIUM-HIGH

## Executive Summary

VELOS is a GPU-accelerated traffic microsimulation engine targeting Ho Chi Minh City's motorbike-dominant mixed traffic. The core technical bet is running agent updates via wgpu/WGSL compute shaders on Metal, achieving real-time performance at scales (280K agents) that CPU-based competitors (SUMO, VISSIM, Aimsun) cannot match. The product's primary differentiator -- continuous sublane lateral positioning for motorbikes -- fills a confirmed gap: no existing simulator handles Southeast Asian motorcycle swarm behavior natively. The recommended stack (Rust nightly + wgpu 28 + hecs + Tauri v2) is well-supported on all fronts except the Tauri+wgpu surface integration, which has a documented flickering issue and no production-quality pattern.

The recommended build approach is spike-first: validate three high-risk integrations (wgpu compute on Metal, Tauri+wgpu dual-surface rendering, fixed-point arithmetic in WGSL) before writing any simulation logic. The architecture follows an ECS-to-GPU pipeline where hecs components are projected into SoA storage buffers for compute dispatch, with a per-lane wave-front (Gauss-Seidel) pattern for car-following. This pattern is theoretically sound but has no direct WGSL precedent and requires careful workgroupBarrier handling.

The top risks are: (1) Tauri+wgpu surface conflict forcing a two-window or pure-winit fallback, (2) WGSL's lack of 64-bit integers making fixed-point multiplication fragile with overflow potential, (3) IDM numerical instabilities at discrete timesteps producing negative velocities, and (4) sublane lateral dynamics being timestep-dependent (a known SUMO bug that VELOS must avoid repeating). All four have concrete mitigation strategies and should be addressed in the first two development phases. The CCH pathfinding implementation is significant custom engineering with no off-the-shelf Rust crate available, but the algorithm is well-documented academically.

## Key Findings

### Recommended Stack

The stack centers on Rust nightly (for `portable_simd` in fixed-point host code) with wgpu 28.0 for Metal GPU compute and rendering. Tauri v2 provides the desktop shell with a React/Vite webview dashboard. The simulation is CPU-orchestrated (hecs ECS + rayon for parallelism) with GPU compute for agent state updates.

**Core technologies:**
- **Rust nightly + wgpu 28.0:** GPU compute + rendering via Metal backend. WGSL shaders for all compute. Pin nightly date in `rust-toolchain.toml`.
- **hecs 0.11:** Minimal ECS -- no scheduler opinions, perfect for a custom simulation tick loop.
- **Tauri 2.10.x:** Native macOS desktop shell. Supports wgpu surface alongside webview, but with known integration issues.
- **postcard 1.1.3:** Binary serialization for checkpoints. Replaces bincode (unmaintained, RUSTSEC-2025-0141).
- **Custom CCH:** No production Rust CCH crate exists with dynamic weight customization. Must be built in-house using petgraph + rayon.
- **rayon 1.11 + tokio 1.47 LTS:** rayon for CPU data-parallelism (OSM parsing, sorting). tokio for async IO only (Tauri, API server).

**Key stack risk:** `portable_simd` is nightly-only. If nightly churn becomes problematic, the `wide` crate on stable Rust is the fallback. Evaluate during spike whether SIMD is actually needed at 1K agent POC scale.

### Expected Features

**Must have (table stakes):**
- IDM car-following + MOBIL lane-change -- core microsimulation behavior
- OSM road network import with HCMC-specific edge splitting
- Fixed-time signal control at intersections
- OD-based demand with time-of-day profiles
- Static CCH pathfinding (dynamic weights deferred)
- Deterministic simulation via fixed-point arithmetic
- Native wgpu 2D visualization with Tauri desktop shell

**Should have (differentiators):**
- Continuous sublane motorbike model (FixedQ8_8 lateral position) -- THE primary differentiator
- GPU-accelerated compute dispatch enabling real-time 280K agents
- HCMC-calibrated default parameters (motorbike v0=40km/h, s0=1.0m, T=0.8s)
- Fixed-point cross-GPU determinism (unique in the market)

**Defer (v2+):**
- Pedestrian social force model, meso-micro hybrid, dynamic routing + prediction (add after core vehicle validation)
- Data export (FCD/Parquet/GeoJSON), emissions model, scenario DSL (post-validation)
- Multi-GPU partitioning, web dashboard (deck.gl), actuated signals (scaling features)
- W99 car-following, TraCI compatibility, activity-based demand, 3D visualization (anti-features -- do not build)

### Architecture Approach

The system follows an ECS-to-GPU pipeline: hecs manages agent state on the CPU, components are projected into SoA GPU storage buffers each frame, compute shaders update agent positions/kinematics, and results are read back to the ECS. The per-frame pipeline runs at 10 Hz with a target of <15ms p99 frame time. Crate boundaries mirror simulation subsystems with a strict dependency DAG (velos-gpu and velos-net at the bottom, velos-app at the top).

**Major components:**
1. **velos-gpu** -- wgpu device management, compute pipeline registry, buffer pools, WGSL shader hosting
2. **velos-net** -- Road graph (petgraph), OSM import (osmpbf), CCH pathfinding, rstar spatial index
3. **velos-core** -- hecs ECS world, frame scheduler, time control, entity-to-GPU index mapping
4. **velos-vehicle** -- IDM + MOBIL + motorbike sublane model (WGSL compute shaders + Rust reference)
5. **velos-app** -- Tauri binary crate, IPC command handlers, wgpu render loop

### Critical Pitfalls

1. **Tauri+wgpu surface conflict** -- Use separate windows (simulation + dashboard) for POC. Single-window overlay is unreliable. Validate in Phase 1 spike before writing simulation code.
2. **WGSL no-i64 fixed-point overflow** -- Q16.16 multiply overflows i32 intermediates. Use u32, clamp inputs, consider Q20.12, and maintain an f32+@invariant fallback. Test with exhaustive edge cases in Phase 1.
3. **IDM negative velocity cascade** -- Add ballistic stopping guard (compute exact stopping time within timestep). Enforce gap floor. Validate with approach-to-stop test scenarios before GPU port.
4. **wgpu buffer mapping deadlock** -- Establish poll discipline from day one. Call `device.poll()` after every `queue.submit()`. Use `StagingBelt` for per-frame uploads. Never skip double-buffering.
5. **Sublane timestep dependence** -- Express all lateral dynamics as rates (not per-step increments), use `sqrt(dt)` scaling for stochastic terms. Validate at multiple timesteps (dt=0.05s, 0.1s, 0.2s).
6. **CCH wrong ordering metric** -- Use topology-only nested dissection, not weighted importance. Verify with adversarial weight reversal tests against Dijkstra ground truth.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Technical Spikes and Foundation

**Rationale:** Three high-risk integrations must be validated before any simulation code is written. If any spike fails, the architecture changes fundamentally (e.g., drop Tauri for winit+egui, drop fixed-point for f32+@invariant). Discovering these failures late would waste weeks of simulation development.

**Delivers:** Proven wgpu+Metal compute pipeline, validated Tauri+wgpu rendering approach, fixed-point arithmetic library with CPU-GPU equivalence tests, wgpu buffer management pattern (double-buffer + poll + staging).

**Addresses:** GPU compute pipeline (P1), determinism (P1), visualization foundation (P1)

**Avoids:** Tauri+wgpu surface conflict (Pitfall 1), WGSL i64 overflow (Pitfall 2), wgpu polling deadlock (Pitfall 4), workgroup size misconfiguration

### Phase 2: Road Network and Core Simulation

**Rationale:** The simulation engine depends on having a real road network. OSM import + graph construction must precede agent behavior models because IDM/MOBIL operate on edges with known geometry. Building IDM in Rust first (as CPU oracle) before porting to WGSL catches numerical issues early.

**Delivers:** HCMC road network from OSM, hecs ECS world with agent spawning, IDM car-following (Rust + WGSL), MOBIL lane-change, fixed-time signals, basic OD demand.

**Addresses:** OSM import (P1), IDM (P1), MOBIL (P1), signals (P1), demand (P1), ECS world (P1)

**Avoids:** IDM negative velocity (Pitfall 3), hecs iteration order instability

### Phase 3: Motorbike Sublane Model and Routing

**Rationale:** The sublane model is the core differentiator and depends on working neighbor queries (R-tree) and validated longitudinal behavior (IDM from Phase 2). CCH pathfinding is independent enough to develop in parallel but requires the road graph from Phase 2.

**Delivers:** Continuous lateral positioning for motorbikes, gap filtering behavior, CCH with static weights, agent rerouting.

**Addresses:** Motorbike sublane model (P1, primary differentiator), CCH pathfinding (P1)

**Avoids:** Sublane timestep dependence (Pitfall 5), CCH wrong ordering (Pitfall 6)

### Phase 4: Desktop Application and Visualization

**Rationale:** The Tauri app shell wraps a working headless simulation. Building it last ensures the simulation is testable without the GUI. The wgpu render pipeline reuses the device/queue from Phase 1 spikes.

**Delivers:** Tauri desktop app with simulation controls, 2D agent rendering on road network, frame rate display, speed controls.

**Addresses:** Simulation playback controls (P1), visualization (P1)

**Avoids:** Camera/dashboard input conflicts, missing frame rate indicator

### Phase 5: Validation and Calibration

**Rationale:** Calibration requires MOE output which requires a complete simulation loop. GEH validation against HCMC traffic counts proves the model is a simulator, not an animation.

**Delivers:** MOE output (travel times, delay, queue lengths), GEH calibration against field data, pedestrian social force model, checkpoint/restart.

**Addresses:** MOE output (P2), calibration (P2), pedestrian model (P2), checkpoint (P2)

### Phase 6: Advanced Features and Scaling

**Rationale:** Dynamic routing, meso-micro hybrid, and multi-GPU are scaling features that add value only after the core model is validated.

**Delivers:** Dynamic CCH weight updates + prediction ensemble, meso-micro graduated buffer, data export (Parquet/FCD), multi-GPU partitioning.

**Addresses:** Dynamic routing (P2), meso-micro hybrid (P2), data export (P3), multi-GPU (P3)

### Phase Ordering Rationale

- **Spikes first:** Tauri+wgpu and fixed-point are binary risks -- they either work or the architecture changes. Validate before investing in simulation code.
- **Network before agents:** IDM/MOBIL operate on road edges. No road network = no agent behavior testing on real geometry.
- **IDM before sublane:** Longitudinal behavior must be correct before adding lateral dynamics. Sublane model builds on top of working car-following.
- **Headless before GUI:** The simulation must be testable via unit/integration tests without the Tauri shell. velos-app is the last crate to build.
- **Validation before scaling:** Prove correctness at 1K agents before optimizing for 280K. Multi-GPU partitioning is wasted effort if the model is wrong.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 1 (Spikes):** Tauri+wgpu integration is poorly documented. The FabianLars example is a proof of concept, not production code. May need to study raw NSView/CALayer management on macOS.
- **Phase 3 (Sublane + CCH):** No direct WGSL precedent for continuous sublane model. CCH implementation requires studying RoutingKit or InertialFlowCutter source code. HCMC-specific motorbike parameters may need literature review.

Phases with standard patterns (skip research-phase):
- **Phase 2 (Network + IDM):** OSM import via osmpbf is well-documented. IDM is extensively documented in academic literature with known test scenarios. hecs ECS usage has clear examples.
- **Phase 4 (App shell):** Tauri v2 has official templates and extensive documentation for IPC commands and React integration.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Core technologies (wgpu, hecs, rayon, tokio) verified via crates.io with specific versions. Nightly Rust risk mitigated by pinned toolchain. |
| Features | MEDIUM-HIGH | Competitor analysis well-sourced. Motorbike gap confirmed by academic literature. Feature prioritization is clear. |
| Architecture | MEDIUM | ECS-to-GPU pipeline and wave-front dispatch are sound in theory but lack direct WGSL precedent. Tauri+wgpu integration confidence is LOW. |
| Pitfalls | MEDIUM-HIGH | Pitfalls verified against official issue trackers and academic papers. Mitigation strategies are concrete. |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

- **Tauri+wgpu dual-surface on macOS:** No production example exists. Phase 1 spike is the validation point. Fallback: two-window or winit+egui.
- **Wave-front dispatch occupancy:** With avg 5.6 agents/lane and workgroup_size(256), 98% of threads are idle. At 280K scale, may need workgroup_size(1) or workgroup_size(32) with subgroup ops. Profile during Phase 2.
- **Fixed-point Q16.16 overflow boundary:** The exact safe range for multiplication intermediates needs empirical testing with realistic HCMC coordinate values (lat/lon range, speed range). Phase 1 spike deliverable.
- **HCMC motorbike calibration data:** Default parameters (v0=40km/h, s0=1.0m, T=0.8s) are from literature but need field validation. No automated calibration until Phase 5.
- **Apple Silicon Metal compute limits:** Max 256 workgroup invocations, subgroup size 32. Needs profiling with actual simulation workloads to determine optimal dispatch strategy.

## Sources

### Primary (HIGH confidence)
- wgpu 28.0.0 -- crates.io, docs.rs (API, compute pipelines, buffer management)
- WGSL W3C Specification -- barrier uniformity, type system, compute semantics
- hecs 0.11.0 -- crates.io (ECS API, archetype storage)
- Tauri v2.10.3 -- official release notes, IPC documentation
- Apple Metal compute documentation -- workgroup limits, unified memory
- IDM academic literature (SIAM 2021) -- numerical pathologies, mitigation strategies
- CCH paper (Dibbelt et al., 2014) -- ordering requirements, customization algorithm
- FHWA microsimulation guidelines -- GEH calibration standards

### Secondary (MEDIUM confidence)
- FabianLars/tauri-v2-wgpu -- proof of concept for Tauri+wgpu integration
- GPU-accelerated traffic simulation (2024 arxiv) -- 88x speedup benchmark
- SUMO sublane timestep bug (eclipse-sumo #8154) -- lateral dynamics dependence
- wgpu compute tutorials (Till Code, Hugo Daniel) -- buffer patterns, staging belt

### Tertiary (LOW confidence)
- Tauri+wgpu overlay discussion (#11944) -- community discussion, no resolution
- VISSIM motorcycle simulation for HCMC -- confirms VISSIM struggles but limited methodology detail

---
*Research completed: 2026-03-06*
*Ready for roadmap: yes*
