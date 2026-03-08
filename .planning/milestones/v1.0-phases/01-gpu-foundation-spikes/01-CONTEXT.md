# Phase 1: GPU Pipeline & Visual Proof - Context

**Gathered:** 2026-03-06
**Status:** Ready for replanning

<domain>
## Phase Boundary

Validate that the wgpu/Metal compute pipeline works on Apple Silicon with f32 arithmetic: ECS-to-GPU buffer round-trip with 1K agents, simple parallel dispatch (no wave-front), and a winit window rendering GPU-instanced styled shapes with zoom/pan. Pure technical validation -- no road network, no vehicle physics, no simulation logic beyond moving dots.

</domain>

<decisions>
## Implementation Decisions

### Arithmetic strategy
- f64 on CPU, f32 on GPU -- no fixed-point types for POC
- No emulated i64 in WGSL, no golden test vectors, no Python script
- Tolerance-based comparison (not bitwise) for CPU-GPU parity: f32 precision (~7 decimal digits)
- CFL check stays (simple and useful, uses f64 on CPU)

### GPU dispatch
- Simple parallel dispatch -- every agent updated independently in one compute pass
- No wave-front (Gauss-Seidel) ordering, no per-lane leader sort, no dual-leader tracking
- No PCG hash -- add when stochastic behavior is needed (Phase 2/3)
- Workgroup size 256, ceil_div for non-multiple agent counts

### Rendering (in this phase)
- winit window from Phase 1 -- visual feedback from day one
- GPU-instanced 2D rendering: one instanced draw call per shape type
- Styled shapes with direction arrows (triangles for moving agents, dots for stationary)
- Zoom/pan camera controls
- Road lanes and intersection areas visible (placeholder geometry in Phase 1, real OSM in Phase 2)
- No egui in Phase 1 -- controls come in Phase 2

### Workspace bootstrapping
- Create only velos-core + velos-gpu (no empty scaffolds for other crates)
- Local-only quality gates (no CI in Phase 1)
- Nightly Rust toolchain pinned via rust-toolchain.toml
- Workspace-level dependency declarations in root Cargo.toml
- MIT license
- main + feature branches (simple trunk-based)
- Minimal README.md (name, description, build/test commands, link to docs/architect/)
- Default rustfmt + clippy config (no custom files)

### Crate boundaries
- ECS component structs (Position, Kinematics) defined in velos-core using f64 types
- velos-gpu owns GPU buffer layout (f32 SoA buffers), rendering pipeline, and compute pipeline
- velos-gpu exposes high-level API (ComputeDispatcher, BufferPool, Renderer) -- no raw wgpu leakage
- Per-crate error types with thiserror (#[from] wrapping)

### Test organization
- GPU integration tests live in velos-gpu crate (tests/ directory)
- GPU tests gated by feature flag AND runtime skip (feature = "gpu-tests" + wgpu::Instance adapter check)
- Tolerance-based comparison for f32 results (not bitwise equality)
- Test naming: descriptive style (test_round_trip_1k_agents_matches_cpu)

### Benchmark infrastructure
- Built-in #[bench] harness (nightly)
- Four metrics: GPU dispatch time, buffer readback time, full round-trip time, agents per second
- Results written to JSON baseline file (benchmarks/baseline.json) for regression comparison

### ECS-to-GPU round-trip
- Dense index array maps GPU index -> ECS entity (rebuild on spawn/despawn)
- Double buffering from the start (front/back GPU buffers, swap per frame)
- Round-trip spike packs Position + Kinematics components as f32 SoA

### Claude's Discretion
- GPU buffer stride and alignment choices
- Internal module organization within each crate
- wgpu adapter selection and device configuration
- Benchmark iteration counts and warm-up strategy
- Exact instanced rendering implementation (vertex pulling vs instance buffer)
- Camera zoom/pan implementation details
- Placeholder road geometry for Phase 1 rendering

</decisions>

<specifics>
## Specific Ideas

- Visual from day one: seeing 1K dots move on screen is the proof that the pipeline works, not just test assertions
- Instanced rendering pattern established in Phase 1 carries through to final product
- Zoom/pan lets you inspect individual agent movement even at 1K scale
- Go/no-go gate after ECS round-trip: if data corrupts through GPU, stop and redesign before investing in rendering

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- No source code exists yet -- greenfield project
- Architecture docs in docs/architect/ provide detailed specs (designed for production scale, POC simplifies)

### Established Patterns
- No code patterns established -- Phase 1 sets the patterns

### Integration Points
- velos-core: foundation crate, all future crates depend on it for types
- velos-gpu: all simulation crates will dispatch through its high-level API, rendering pipeline shared
- f64 CPU types will be used by every crate that touches agent state

</code_context>

<deferred>
## Deferred Ideas

- Fixed-point arithmetic (Q16.16/Q12.20/Q8.8) -- v2 for 280K deterministic scale
- Wave-front (Gauss-Seidel) dispatch -- v2 for convergence at scale
- PCG deterministic hash -- add in Phase 2/3 when stochastic behavior needed
- Per-lane leader sort with dual-leader tracking -- v2 with wave-front

</deferred>

---

*Phase: 01-gpu-pipeline-visual-proof*
*Context gathered: 2026-03-06 (revised after project simplification)*
