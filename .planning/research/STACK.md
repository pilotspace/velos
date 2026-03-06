# Stack Research

**Domain:** GPU-accelerated traffic microsimulation (macOS/Metal native desktop)
**Researched:** 2026-03-06
**Confidence:** HIGH (core stack verified via crates.io, official docs, and release notes)

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Rust nightly (Edition 2024) | nightly-2026-03-xx | Language, GPU compute, simulation engine | `portable_simd` required for fixed-point math (Q16.16/Q12.20/Q8.8) in WGSL shader host code. Edition 2024 stable since Rust 1.85.0 (Feb 2025), but `std::simd` still nightly-only. Pin a specific nightly date in `rust-toolchain.toml` for reproducibility. |
| wgpu | 28.0.0 | GPU compute + rendering via Metal backend | Pure-Rust, first-class Metal backend on Apple Silicon. Compute shaders via WGSL translated to MSL by Naga. 3-month release cadence means breaking changes are frequent -- pin the major version. wgpu 28 is current as of early 2026. |
| naga / naga-cli | 28.0.0 (bundled with wgpu) | WGSL shader validation and translation | Ships inside wgpu; version-locked. Use `naga-cli` standalone for CI shader validation (`naga --validate *.wgsl`). Validates WGSL fully, translates to SPIR-V/MSL/HLSL. |
| Tauri | 2.10.x | Native macOS desktop shell with webview | Stable v2, active development (2.10.3 released 2026-03-04). Supports multiple surfaces in one window -- critical for wgpu render surface alongside React webview dashboard. |
| hecs | 0.11.0 | Lightweight ECS for agent state | Minimal, library-not-framework design. No global state, no scheduler opinions. Perfect for simulation where you control the tick loop. 0.11.0 released Feb 2026. |
| tokio | 1.49.0 (LTS: 1.47.x) | Async runtime for Tauri, API server, IO | Required by Tauri v2 internals and axum/tonic. Use LTS 1.47.x for stability (supported until Sep 2026). Only for async IO -- simulation tick loop stays synchronous on rayon. |
| rayon | 1.11.0 | CPU data-parallelism | OSM parsing, CCH construction, spatial index building, any CPU-bound batch work. Stable, mature, zero-config work-stealing. |

### GPU and Rendering

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| wgpu (compute) | 28.0.0 | Agent position updates, car-following, sublane model | Compute shaders dispatch per-lane wave-front updates. Metal subgroup size is 32 on all Apple Silicon -- use `@workgroup_size(32, 1, 1)` or `@workgroup_size(64, 1, 1)` as baseline. Max 256 invocations per workgroup on Metal. |
| wgpu (render) | 28.0.0 | 2D top-down visualization of agents on road network | Same wgpu instance handles both compute and render passes. Render pass draws agents as instanced quads/circles over road geometry. |
| WGSL | (wgpu bundled) | Shader language | Only shader language with full Naga validation. Write compute + vertex + fragment shaders in WGSL. Avoid SPIR-V or GLSL input -- WGSL is the first-class citizen in wgpu. |

### Network and Pathfinding

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| Custom CCH implementation | N/A (in-house) | Dynamic shortest path queries | No production-quality CCH crate exists in Rust with dynamic weight customization. The `contraction_hierarchies` crate (1.0.0) is basic Dijkstra-on-CH without dynamic updates. Build custom: CCH construction via rayon parallel node contraction, weight customization via `ArcSwap` overlay. |
| rstar | 0.12.2 | R*-tree spatial index for neighbor queries | Mature georust crate. Used for agent-to-agent proximity queries (motorbike filtering, pedestrian social force). Rebuild or bulk-load each frame for ~1K agents -- cheap at this scale. |
| osmpbf | 0.3.8 | OpenStreetMap PBF file parsing | Parallel decompression, lazy decoding, simple iterator API. Best Rust OSM parser for extracting road network geometry from `.osm.pbf` files. |
| petgraph | 0.7.x | Road network graph structure | Standard Rust graph library. Use `DiGraph` for directed road network. CCH operates on top of petgraph's adjacency structure. |

### Serialization and State

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| postcard | 1.1.3 | Binary serialization for checkpoints and IPC | Replaces bincode, which is unmaintained (RUSTSEC-2025-0141). Postcard is serde-compatible, `no_std` friendly, stable wire format since 1.0, actively maintained. Slightly smaller payloads than bincode at ~1.5x slower -- irrelevant for checkpoint writes. |
| serde | 1.x | Serialization framework | Universal derive macros for all data structures. Required by postcard, used across IPC boundaries. |
| arc-swap | 1.7.1 | Atomic Arc swapping for prediction overlay | Read-optimized atomic pointer swap. Simulation reads current prediction weights while background thread computes new weights and swaps atomically. Zero-lock reads. |

### API Server (headless / external access)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| axum | 0.8.x | REST + WebSocket API | Tokio-native, tower middleware ecosystem. New path syntax `/{param}` in 0.8. Use for headless simulation control and metrics export. |
| tonic | 0.12.x | gRPC server | Protobuf-based API for programmatic simulation control. Shares tokio runtime with axum. Tonic 0.12.x is current stable branch. |
| prost | 0.13.x | Protobuf code generation | Standard Rust protobuf codegen, used by tonic. Define `.proto` files in `proto/velos/v2/`. |

### Frontend Dashboard (inside Tauri webview)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| React | 19.x | UI framework for dashboard panels | Tauri v2 webview runs any web framework. React 19 with server components not needed here -- use client-only. |
| TypeScript | 5.7.x | Type safety for dashboard code | Non-negotiable for any frontend code. |
| Vite | 6.x | Dev server and bundler | Tauri v2 officially recommends Vite. Fast HMR, native ESM. |
| @tauri-apps/api | 2.x | Tauri IPC from frontend | Type-safe invoke commands to Rust backend. Primary communication channel for simulation control (start/stop/speed/step). |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| cargo-nextest | Fast test runner | Parallel test execution, better output than `cargo test`. Use for CI and local dev. |
| cargo-watch | File watcher for auto-rebuild | `cargo watch -x 'clippy --all-targets'` during development. |
| naga-cli | WGSL shader validation in CI | `cargo install naga-cli` then `naga --validate crates/velos-gpu/shaders/*.wgsl`. Run as pre-commit check. |
| cargo-criterion | Benchmark runner | Frame time benchmarks. Tracks regressions across commits. |
| rust-toolchain.toml | Pin nightly version | Pin exact nightly date to prevent breakage from nightly churn. Example: `channel = "nightly-2026-03-01"`. |
| pnpm | Node package manager | For dashboard TypeScript code. Tauri v2 works with pnpm out of the box. |

## Installation

```bash
# Rust toolchain (nightly pinned via rust-toolchain.toml)
rustup install nightly
rustup default nightly

# Create rust-toolchain.toml in repo root:
# [toolchain]
# channel = "nightly-2026-03-01"
# components = ["rustfmt", "clippy", "rust-src"]
# targets = ["aarch64-apple-darwin"]

# Core Rust dependencies (in workspace Cargo.toml)
# wgpu = "28.0"
# hecs = "0.11"
# tokio = { version = "1.47", features = ["full"] }
# rayon = "1.11"
# rstar = "0.12"
# osmpbf = "0.3"
# petgraph = "0.7"
# postcard = { version = "1.1", features = ["alloc"] }
# serde = { version = "1", features = ["derive"] }
# arc-swap = "1.7"
# axum = "0.8"
# tonic = "0.12"
# prost = "0.13"

# Dev dependencies
# cargo-nextest, naga-cli, cargo-criterion installed via cargo install

# Tauri CLI
cargo install tauri-cli

# Frontend (in dashboard/ directory)
pnpm create tauri-app  # or manual setup
pnpm add react react-dom @tauri-apps/api
pnpm add -D typescript vite @vitejs/plugin-react
```

## Alternatives Considered

| Recommended | Alternative | When to Use Alternative |
|-------------|-------------|-------------------------|
| hecs 0.11 | bevy_ecs (standalone) | If you need built-in scheduling, change detection, and system ordering. Heavier -- pulls in Bevy's scheduler. Use hecs because VELOS owns the tick loop entirely. |
| hecs 0.11 | legion | If you need archetypal storage with better query performance at high entity counts. But legion is less maintained than hecs. Stick with hecs for ~1K agents. |
| wgpu 28 | vulkano (Vulkan-only) | Never for this project. No Metal support = no macOS. |
| wgpu 28 | metal-rs (raw Metal) | If you need Metal-specific features wgpu does not expose. Lose cross-platform portability. Only consider if wgpu compute dispatch proves insufficient for wave-front pattern. |
| Tauri v2 | Electron | Never. Chromium overhead unacceptable for a simulation app. Tauri uses native webview (WebKit on macOS). |
| Tauri v2 | winit + egui (pure native) | If the webview dashboard is abandoned in favor of pure Rust UI. Simpler GPU integration (no surface fights). Consider if Tauri+wgpu flickering proves unsolvable. |
| postcard 1.1 | rkyv 0.8 | If zero-copy deserialization matters for checkpoint loading. Requires more invasive type changes (derive `Archive`). Postcard is simpler drop-in. |
| postcard 1.1 | bitcode | Smaller output, but fewer maintainers and lower community adoption than postcard. |
| Custom CCH | rust_road_router | If upstream provides the dynamic weight API you need. Last checked: it does not expose clean dynamic customization. Custom build is the right call. |
| rayon 1.11 | std threads + crossbeam | If you need fine-grained control over thread affinity. Rayon's work-stealing is better for data-parallel workloads like OSM parsing. |
| portable_simd (nightly) | wide crate (stable) | If you want to avoid nightly Rust entirely. `wide` provides portable SIMD on stable Rust. Trade-off: less ergonomic API, manual type wrapping. Consider if nightly breakage becomes a recurring problem. |
| React 19 | Svelte 5 | Lighter bundle, but smaller ecosystem for Tauri plugins. React has more Tauri community examples. |

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| bincode | Unmaintained (RUSTSEC-2025-0141), development ceased due to project issues | postcard 1.1.3 |
| Wiedemann 99 (W99) car-following | Overengineered for this use case, 10+ calibration parameters vs IDM's 5 | IDM (Intelligent Driver Model) |
| deck.gl | Web-based, requires separate server, not needed for native desktop app | wgpu native rendering |
| Python bridge for prediction | IPC overhead, deployment complexity, GIL issues | Rust-native BPR+ETS ensemble |
| SPIR-V shaders (direct) | Naga's WGSL validation is more complete; SPIR-V requires external toolchain | WGSL shaders |
| Martin / PostGIS tile server | Overkill for static map tiles on a local desktop app | PMTiles static files |
| Redis pub/sub | No multi-client scaling needed for single-user desktop app | Tauri IPC (invoke commands + events) |
| npm / yarn | Project convention specifies pnpm | pnpm |
| Stable Rust (if portable_simd needed) | `std::simd` is nightly-only; fixed-point math performance depends on it | Rust nightly with pinned date |

## Stack Patterns by Variant

**If Tauri + wgpu surface flickering is unsolvable:**
- Drop Tauri, use winit + egui for both rendering and UI
- Lose web-based dashboard, gain simpler GPU integration
- This is the fallback if the Tauri+wgpu dual-surface approach fails in spike testing

**If portable_simd proves unnecessary (fixed-point math fast enough without SIMD):**
- Switch to stable Rust (Edition 2024, Rust 1.85+)
- Eliminates nightly churn risk entirely
- Test fixed-point throughput on stable first before committing to nightly

**If scaling beyond 1K agents on Metal reveals GPU limits:**
- Metal max workgroup invocations = 256 (vs Vulkan's typical 1024)
- May need to restructure compute dispatch to use smaller workgroups with more dispatches
- Profile early: Apple Silicon M-series has unified memory, so CPU-GPU transfer is zero-copy

**If headless mode is primary (no GUI needed):**
- Skip Tauri entirely, run simulation as CLI with axum/tonic API
- wgpu compute still works headless (no surface needed for compute-only)
- Add Tauri shell later as optional feature

## Version Compatibility

| Package A | Compatible With | Notes |
|-----------|-----------------|-------|
| wgpu 28.0 | naga 28.0 (bundled) | Versions are locked together in wgpu monorepo. Do not mix versions. |
| wgpu 28.0 | Tauri 2.10.x | Requires `raw-window-handle` 0.6 compatibility. Tauri v2 exposes raw window handle via wry. Known flickering issue (#9220) -- test in spike. |
| axum 0.8.x | tokio 1.47+ | axum 0.8 requires tokio with `rt-multi-thread` feature. |
| tonic 0.12.x | tokio 1.47+ | Shares runtime with axum. Both use tower middleware. |
| tonic 0.12.x | prost 0.13.x | tonic's codegen depends on matching prost version. |
| hecs 0.11 | serde 1.x | hecs has optional `serde` feature for world serialization. Enable for checkpoints. |
| Tauri 2.10.x | React 19 + Vite 6 | Official Tauri template supports this combination. Use `@tauri-apps/cli` 2.x. |
| postcard 1.1 | serde 1.x | postcard requires serde. Enable `alloc` feature for Vec/String support. |
| rayon 1.11 | Rust 1.80+ | Minimum rustc requirement. Nightly exceeds this. |

## Confidence Assessment

| Area | Confidence | Rationale |
|------|------------|-----------|
| wgpu + Metal compute | HIGH | wgpu Metal backend is mature, first-class. Compute shaders well-documented. Verified via crates.io (v28.0.0) and official wgpu.rs docs. |
| Tauri v2 + wgpu integration | MEDIUM | Working examples exist (FabianLars/tauri-v2-wgpu), but flickering issue (#9220) is documented. Needs spike testing. Multiple-surface support is confirmed in Tauri v2. |
| hecs for simulation ECS | HIGH | Well-suited minimal ECS. 0.11.0 recently released. No opinions on scheduling = perfect for custom sim loop. |
| Custom CCH pathfinding | MEDIUM | No off-the-shelf Rust CCH crate with dynamic weights. Custom implementation is significant engineering. Algorithm is well-documented in academic literature. |
| portable_simd on nightly | MEDIUM | Works but nightly-only with no stable timeline. Pin nightly date to mitigate. Consider `wide` crate as fallback. Evaluate during spike whether SIMD is actually needed for 1K agents. |
| postcard replacing bincode | HIGH | bincode confirmed unmaintained (RUSTSEC advisory). postcard is stable, serde-compatible, actively maintained. Drop-in replacement. |
| Frontend (React + Vite + Tauri) | HIGH | Standard Tauri v2 recommended stack. Well-documented, official templates available. |

## Sources

- [wgpu crates.io](https://crates.io/crates/wgpu) -- version 28.0.0 verified
- [wgpu docs.rs](https://docs.rs/crate/wgpu/latest) -- API docs for 28.0.0
- [hecs crates.io](https://crates.io/crates/hecs) -- version 0.11.0 verified
- [Tauri v2 releases](https://v2.tauri.app/release/) -- version 2.10.3 verified (2026-03-04)
- [Tauri v2 wgpu example](https://github.com/FabianLars/tauri-v2-wgpu) -- integration proof-of-concept
- [Tauri wgpu flickering issue #9220](https://github.com/tauri-apps/tauri/issues/9220) -- known integration challenge
- [tokio releases](https://github.com/tokio-rs/tokio/releases) -- v1.49.0 current, LTS 1.47.x
- [axum 0.8 announcement](https://tokio.rs/blog/2025-01-01-announcing-axum-0-8-0) -- breaking changes documented
- [rayon crates.io](https://crates.io/crates/rayon) -- version 1.11.0 verified
- [rstar crates.io](https://crates.io/crates/rstar) -- version 0.12.2 verified
- [osmpbf crates.io](https://crates.io/crates/osmpbf) -- version 0.3.8 verified
- [arc-swap crates.io](https://crates.io/crates/arc-swap) -- version 1.7.1 verified
- [postcard crates.io](https://crates.io/crates/postcard) -- version 1.1.3 verified
- [Rust 1.85.0 / Edition 2024 announcement](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/) -- Edition 2024 stable since Feb 2025
- [State of SIMD in Rust 2025](https://shnatsel.medium.com/the-state-of-simd-in-rust-in-2025-32c263e5f53d) -- portable_simd still nightly-only
- [tonic GitHub](https://github.com/hyperium/tonic) -- 0.12.x branch is current stable
- [naga crates.io](https://crates.io/crates/naga) -- version 28.0.0 (bundled with wgpu)
- [bincode RUSTSEC advisory](https://github.com/tursodatabase/libsql/issues/2207) -- unmaintained status confirmed
- [Apple Metal compute docs](https://developer.apple.com/documentation/metal/performing-calculations-on-a-gpu) -- workgroup limits
- [wgpu compute workgroup issue #582](https://github.com/gfx-rs/wgpu-rs/issues/582) -- workgroup sizing guidance

---
*Stack research for: VELOS GPU-accelerated traffic microsimulation (macOS/Metal)*
*Researched: 2026-03-06*
