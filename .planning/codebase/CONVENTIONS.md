# Coding Conventions

**Analysis Date:** 2026-03-06

## Project Status

VELOS is in pre-development (architecture/design phase). No source code exists yet. These conventions are derived from architecture documents (`docs/architect/`), `CLAUDE.md`, and `.claude/CLAUDE.md` (author's global instructions). All conventions below are **prescriptive** -- follow them when writing code.

## Naming Patterns

**Files:**
- Rust source: `snake_case.rs` (standard Rust)
- Max 700 lines per file -- enforced by project rules
- WGSL shaders: `snake_case.wgsl` in `crates/velos-gpu/shaders/`
- Protobuf: `snake_case.proto` in `proto/velos/v2/`
- TypeScript/React: component files follow standard React conventions in `dashboard/`

**Crates:**
- Prefix all crates with `velos-` (e.g., `velos-core`, `velos-gpu`, `velos-net`)
- Each crate has a single, clear responsibility
- 14 planned crates: `velos-core`, `velos-gpu`, `velos-net`, `velos-vehicle`, `velos-pedestrian`, `velos-signal`, `velos-meso`, `velos-predict`, `velos-demand`, `velos-calibrate`, `velos-output`, `velos-api`, `velos-scene`, `velos-viz`

**Functions:**
- Rust: `snake_case` (standard Rust convention)
- Constructor-like: `new()`, factory functions: descriptive names like `hcmc_commuter_motorbike()`
- WGSL shader functions: `snake_case` (e.g., `idm_update`, `motorbike_lateral_desire`, `social_force`)

**Variables:**
- Rust: `snake_case`
- WGSL: `snake_case` for variables, `SCREAMING_SNAKE_CASE` for constants (e.g., `POS_SCALE`, `SPD_SCALE`, `TILE_SIZE`)

**Types:**
- Rust structs/enums: `PascalCase` (e.g., `GpuPartition`, `AgentType`, `GridlockStrategy`)
- WGSL type aliases: `PascalCase` (e.g., `FixPos`, `FixSpd`)
- Enum variants: `PascalCase` (e.g., `AgentType::Motorbike`, `DayType::Weekday`)

**Domain-Specific Naming:**
- Fixed-point types: `FixedQ{int}_{frac}` format (e.g., `FixedQ16_16`, `FixedQ12_20`, `FixedQ8_8`)
- IDM parameters: use standard names `v0`, `s0`, `T`, `a`, `b`
- MOBIL parameters: `politeness`, `threshold`, `safe_decel`, `right_bias`

## Code Style

**Formatting:**
- Use `rustfmt` (Rust standard formatter)
- Use `prettier` for TypeScript/React code in `dashboard/`

**Linting:**
- Rust: `cargo clippy --all-targets --all-features -- -D warnings` (treat all warnings as errors)
- WGSL: `naga --validate crates/velos-gpu/shaders/*.wgsl` for shader correctness
- TypeScript: standard ESLint for dashboard code

**Quality Gate (run before every commit):**
```bash
cargo clippy --all-targets -- -D warnings && cargo test --workspace && cargo bench --bench frame_time
```

## Struct Design

**Component structs (ECS):** Use SoA (Structure of Arrays) layout for GPU compatibility. Keep components small and focused.

```rust
// GOOD: Small, focused, GPU-friendly components
#[derive(Copy, Clone)]
pub struct Position {
    pub edge_id: u32,
    pub lane_idx: u8,
    pub offset: FixedQ16_16,
    pub lateral: FixedQ8_8,
}

#[derive(Copy, Clone)]
pub struct Kinematics {
    pub speed: FixedQ12_20,
    pub acceleration: FixedQ8_24,
}
```

**Configuration structs:** Group related parameters. Provide HCMC-specific factory functions.

```rust
pub struct IDMParams {
    pub v0: f32,   // desired speed
    pub s0: f32,   // minimum gap
    pub T: f32,    // time headway
    pub a: f32,    // max acceleration
    pub b: f32,    // comfortable deceleration
}

// Factory for HCMC-specific profiles
pub fn hcmc_commuter_motorbike() -> AgentProfile { /* ... */ }
pub fn hcmc_taxi_car() -> AgentProfile { /* ... */ }
pub fn hcmc_bus() -> AgentProfile { /* ... */ }
```

**Manager/Service structs:** Use the `Manager` suffix for stateful coordinators.

```rust
pub struct CheckpointManager { /* ... */ }
pub struct MultiGpuScheduler { /* ... */ }
pub struct GridlockDetector { /* ... */ }
pub struct RerouteScheduler { /* ... */ }
```

## Import Organization

**Order (Rust):**
1. Standard library (`std::`)
2. External crates (`wgpu::`, `hecs::`, `rayon::`, `tokio::`, `tracing::`)
3. Workspace crates (`velos_core::`, `velos_gpu::`)
4. Local module imports (`use super::`, `use crate::`)

**Path Aliases:**
- No path aliases in Rust -- use standard module paths
- TypeScript dashboard: follow standard `@/` alias conventions if configured

## Error Handling

**Strategy:** Use `Result<T, E>` throughout. Use domain-specific error types.

**gRPC errors:** Use the `VelosError` protobuf type with typed `ErrorCode` enum:
```protobuf
enum ErrorCode {
    UNKNOWN = 0;
    NETWORK_NOT_LOADED = 1;
    SIMULATION_NOT_RUNNING = 2;
    EDGE_NOT_FOUND = 3;
    AGENT_NOT_FOUND = 5;
    CAPACITY_EXCEEDED = 7;
    CHECKPOINT_CORRUPTED = 11;
    GRIDLOCK_DETECTED = 12;
}
```

**Numerical safety patterns:**
- Always clamp IDM acceleration output: `clamp(acc, -9.0, a_max)` (physical limits)
- Prevent division by zero: `max(value, 0.1)` for speeds, `max(gap, 0.1)` for gaps
- Use `safe_pow4()` instead of `pow()` in WGSL to avoid undefined behavior
- CFL-bounded time stepping: sub-step when `v_max * dt / edge_length >= 1.0`

**Edge transition guard:** Clamp position to edge length when next edge is full:
```rust
if position > edge_length {
    if next_edge.has_capacity() {
        position = overflow; // carry to next edge
    } else {
        position = edge_length; // clamp, speed = 0
    }
}
```

## Logging

**Framework:** `tracing` crate with structured fields

**Patterns:**
```rust
use tracing::{info, warn, instrument};

#[instrument(skip(world))]
pub fn simulation_step(world: &mut World, step: u64, sim_time: f64) {
    info!(
        step = step,
        sim_time = sim_time,
        agent_count = world.len(),
        frame_time_ms = elapsed_ms,
        gpu_time_ms = gpu_elapsed,
        reroute_count = reroutes,
        "Step completed"
    );
}
```

- Use `#[instrument]` on public functions. Use `skip()` for large arguments.
- Always include `step`, `sim_time` in simulation log entries.
- Use structured key-value fields, not string interpolation.

## Comments

**When to Comment:**
- Explain the "why" for non-obvious design decisions (e.g., why wave-front over EVEN/ODD)
- Reference architecture doc sections (e.g., `// See 01-simulation-engine.md Section 2`)
- Document physical constants and their sources (e.g., IDM calibration ranges)
- Document CFL conditions and numerical safety rationale

**Doc Comments:**
- Use `///` for public API documentation
- Include units in parameter docs (m/s, km/h, degrees)
- Include valid ranges where applicable

## Function Design

**Size:** Keep functions small. Max 700 lines per file implies functions should be much shorter.

**Parameters:** Use structs for related parameters (e.g., `IDMParams`, `MOBILParams`, `CostWeights`) rather than long parameter lists.

**Return Values:** Use `Option<T>` for lookups that may fail, `Result<T, E>` for operations that can error.

## Module Design

**Exports:** Each crate exposes a clean public API through `lib.rs`.

**Barrel Files:** Not applicable in Rust. Use `pub use` re-exports in `lib.rs`.

**Single Responsibility:** Each of the 14 crates owns one domain concern. Do not mix concerns across crates.

## Concurrency Patterns

**CPU parallelism:** Use `rayon` for CPU-side parallel work (e.g., CCH queries, leader sorting).

**Async runtime:** Use `tokio` for I/O-bound work (gRPC, WebSocket, sensor ingestion, checkpoint I/O).

**Lock-free updates:** Use `ArcSwap` for prediction overlay -- zero-lock, zero-copy atomic swaps.

**Double buffering:** GPU buffers use front/back pattern -- GPU reads front, CPU writes back, swap per frame.

## Protobuf Conventions

**Package:** `velos.v2`

**Location:** `proto/velos/v2/`

**Service naming:** `VelosSimulation` (single service for POC)

**Message naming:** `PascalCase` for messages, `snake_case` for fields

**REST mapping:** Wrap gRPC methods via axum REST endpoints at `/api/v1/`

## Dashboard Conventions (TypeScript/React)

**Package manager:** `pnpm` (never `npm` or `yarn`)

**Location:** `dashboard/` directory (pnpm workspace)

**Primary viz:** deck.gl + MapLibre GL JS

**Binary protocol:** FlatBuffers for WebSocket frames (8 bytes per agent)

---

*Convention analysis: 2026-03-06*
