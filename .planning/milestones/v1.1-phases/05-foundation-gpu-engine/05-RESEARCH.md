# Phase 5: Foundation & GPU Engine - Research

**Researched:** 2026-03-07
**Domain:** GPU compute engine, road network import/cleaning, SUMO compatibility, car-following models
**Confidence:** HIGH

## Summary

Phase 5 cuts over the simulation from CPU physics to GPU compute as the sole execution path, scales to 280K agents across 2-4 GPUs, expands the road network from single-district to 5-district HCMC with aggressive cleaning, adds SUMO file import compatibility (.net.xml, .rou.xml), and implements the Krauss car-following model alongside existing IDM with runtime switching.

The existing codebase provides strong foundations: `ComputeDispatcher` with double-buffered SoA pattern, `SimWorld` CPU sim loop as reference implementation, ECS components, OSM import pipeline, and IDM implementation. The primary technical risks are: (1) wgpu multi-adapter compute is untested and has limited documentation, (2) WGSL lacks native i64 so fixed-point multiplication requires manual 32-bit emulation with ~20-40% performance overhead, and (3) the Krauss model dawdle behavior requires per-agent RNG state on the GPU.

**Primary recommendation:** Decompose the work into four tracks: GPU engine cutover (wave-front dispatch + fixed-point), multi-GPU partitioning (METIS + boundary protocol), network expansion (5-district cleaning + SUMO import), and car-following models (Krauss + runtime switching). Validate GPU against CPU reference before deleting CPU physics path.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- SUMO .net.xml import covers: edges, lanes, junctions, connections, roundabouts, traffic light programs (tlLogic)
- .rou.xml import is full-featured: trips, flows, vehicles, persons, vType distributions, calibrator elements
- Unmapped SUMO attributes use best-effort mapping with logged warnings -- never silently drop attributes
- Krauss model uses SUMO-faithful defaults: sigma=0.5
- Agents are color-coded by car-following model in the egui dashboard
- Runtime model switching via per-agent ECS component tag (CarFollowingModel enum) -- GPU shader branches on tag
- No hardcoded default model per vehicle type -- demand configuration specifies which car-following model each vehicle type uses
- Aggressive network cleaning: merge short edges <5m, remove disconnected components, infer lane counts from road class, fix topology errors
- Manual override file (JSON/TOML) for correcting specific edges/junctions where OSM is wrong
- Motorbike-only lane detection: OSM tags + road class heuristic (alleys <4m wide = motorbike-only)
- Time-dependent one-way edges: support time-of-day directional changes
- Cleaned graph serialized to binary format for fast reload; re-import from OSM on demand
- Parallel CPU+GPU run during validation period, comparing aggregate metrics -- not per-agent position matching
- Behavioral equivalence is the bar: same traffic patterns, not identical floating-point values
- After validation: delete CPU physics from production sim loop, keep CPU model implementations in test modules
- Multi-GPU validated via simulated partitions on single GPU (2-4 logical partitions with own buffers, real inbox/outbox boundary agent protocol)

### Claude's Discretion
- God crate decomposition strategy (how to split velos-gpu into focused crates)
- Wave-front dispatch implementation details
- Fixed-point arithmetic precision trade-offs and @invariant fallback
- WGSL shader architecture for multi-model branching
- Binary serialization schema for cleaned graph
- Override file format specifics (JSON vs TOML)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| GPU-01 | Simulation physics runs on GPU compute pipeline as primary execution path | Wave-front dispatch architecture, WGSL shader design with IDM+Krauss branching, ComputeDispatcher extension |
| GPU-02 | GPU spatial partitioning via METIS k-way graph partitioning across multiple adapters | metis crate (0.2.2) for graph partitioning, wgpu multi-adapter enumeration via Instance::enumerate_adapters |
| GPU-03 | Per-lane wave-front (Gauss-Seidel) dispatch replaces simple parallel dispatch | Architecture doc Section 2 wave-front algorithm, lane-sorted agent processing, workgroup-per-lane mapping |
| GPU-04 | Fixed-point arithmetic (Q16.16 position, Q12.20 speed, Q8.8 lateral) for cross-GPU determinism | WGSL i32-based fixed-point with manual 64-bit emulation, @invariant fallback strategy |
| GPU-05 | Boundary agent protocol (outbox/inbox staging buffers) for multi-GPU agent transfers | GpuPartition/MultiGpuScheduler architecture from 01-simulation-engine.md, ~500-1000 agents/step transfer |
| GPU-06 | System sustains 280K agents at 10 steps/sec real-time on 2-4 GPUs | Performance budget: ~8.2ms theoretical per step, 100ms budget, 11x margin |
| NET-01 | 5-district HCMC road network imported from OSM (~25K edges) | Extend existing osm_import.rs, add Motorway/Trunk road classes, 5-district bounding box |
| NET-02 | Network cleaning: merge short edges <5m, remove disconnected components, lane count inference | Graph cleaning pipeline per architecture doc Section 2 (04-data-pipeline-hcmc.md) |
| NET-03 | HCMC-specific OSM rules: one-way streets, U-turn points, motorbike-only lanes | OSM tag parsing for motorcycle=designated, junction=roundabout, U-turn edges |
| NET-04 | Time-of-day demand profiles: weekday AM/PM peak, off-peak, weekend across 5 districts | Existing TodProfile in velos-demand, extend for 5-zone granularity |
| NET-05 | SUMO .net.xml network import | XML parsing of edges/lanes/junctions/connections/roundabouts/tlLogic elements |
| NET-06 | SUMO .rou.xml / .trips.xml demand file import | XML parsing of vehicle/trip/flow/vType/vTypeDistribution/route elements |
| CFM-01 | Krauss car-following model with safe-speed and dawdle behavior | SUMO-faithful safe speed formula + sigma dawdle, GPU-side RNG for stochastic component |
| CFM-02 | Runtime-selectable car-following model per agent type (IDM or Krauss via ECS component) | CarFollowingModel enum as ECS tag, GPU shader branching on model tag |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| wgpu | 27 | GPU compute + render | Already in workspace, cross-platform WebGPU API, Metal backend for Mac |
| hecs | 0.11 | ECS world | Already in workspace, lightweight SoA-friendly ECS |
| petgraph | 0.6 | Road graph | Already in workspace, directed graph with node/edge weights |
| bytemuck | 1 | GPU buffer casting | Already in workspace, zero-cost Pod/Zeroable for #[repr(C)] structs |
| metis | 0.2.2 | METIS k-way graph partitioning | Idiomatic Rust bindings to libmetis, vendored build, MIT/Apache-2.0 |
| quick-xml | 0.37 | SUMO XML parsing | Fast streaming XML parser, serde integration, widely used in Rust |
| rand | 0.8 | RNG for Krauss dawdle | Already in workspace |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rayon | latest | CPU-parallel leader sort, graph cleaning | Parallel per-lane sort before GPU dispatch |
| oxicode | latest | Binary serialization for cleaned graph | Successor to bincode (abandoned), binary-compatible, actively maintained |
| serde | 1 | Serialization framework | Override file parsing, graph serialization |
| toml | latest | Override file format | TOML preferred over JSON for human-edited config (comments, better readability) |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| oxicode | postcard | postcard is smaller messages but 1.5x slower; oxicode is bincode-compatible successor |
| quick-xml | roxmltree | roxmltree is DOM-based (loads full tree); quick-xml is streaming (lower memory for large .net.xml) |
| TOML override | JSON override | TOML supports comments for documenting overrides; JSON is more portable but no comments |

**Installation:**
```bash
cargo add metis quick-xml rayon serde toml
# For graph serialization, check oxicode availability; fallback to postcard
cargo add oxicode || cargo add postcard
```

## Architecture Patterns

### Recommended Crate Decomposition (Claude's Discretion)

Split the monolithic `velos-gpu` into focused crates:

```
crates/
├── velos-core/          # ECS components (extend: CarFollowingModel, FixedQ types)
├── velos-gpu/           # Device management, buffer pools, compute dispatcher
│   └── shaders/         # WGSL shaders (car_following.wgsl, wave_front.wgsl)
├── velos-net/           # Road graph, OSM import, SUMO import, graph cleaning
├── velos-vehicle/       # CPU reference: IDM, Krauss, MOBIL, sublane, social force
├── velos-demand/        # OD matrices, ToD profiles, agent spawning
├── velos-signal/        # Signal controllers
└── velos-sim/           # NEW: Orchestration crate -- SimWorld, tick loop, GPU dispatch wiring
```

Rationale: Keep `velos-gpu` as the low-level GPU abstraction. Extract the simulation orchestration (currently `sim.rs` at 695 lines) into a new `velos-sim` crate that depends on `velos-gpu`, `velos-vehicle`, `velos-net`. This separates GPU plumbing from simulation logic and keeps files under 700 lines.

### Pattern 1: Wave-Front Dispatch

**What:** Per-lane sequential processing (front-to-back) with cross-lane parallelism
**When to use:** All vehicle physics updates (IDM + Krauss car-following)

```wgsl
// Each workgroup processes one lane
// Agents sorted by position descending (leader first)
// Sequential within workgroup guarantees leader is already updated
@compute @workgroup_size(256)
fn wave_front_update(
    @builtin(workgroup_id) wg_id: vec3<u32>,
    @builtin(local_invocation_id) local_id: vec3<u32>
) {
    let lane_idx = wg_id.x;
    let lane_start = lane_offsets[lane_idx];
    let lane_count = lane_counts[lane_idx];

    // Sequential: thread 0 processes all agents in this lane
    if (local_id.x != 0u) { return; }

    for (var i = 0u; i < lane_count; i = i + 1u) {
        let agent_idx = lane_agents[lane_start + i];
        let leader_idx = select(lane_agents[lane_start + i - 1u], 0xFFFFFFFFu, i == 0u);

        // Read leader's ALREADY-UPDATED position (current step)
        let leader_speed = select(agents[leader_idx].speed, 0.0, leader_idx == 0xFFFFFFFFu);
        let leader_pos = select(agents[leader_idx].position, 99999.0, leader_idx == 0xFFFFFFFFu);

        let gap = leader_pos - agents[agent_idx].position - agents[agent_idx].length;
        let delta_v = agents[agent_idx].speed - leader_speed;

        // Branch on car-following model
        let model_tag = agents[agent_idx].cf_model;
        var accel: f32;
        if (model_tag == CF_IDM) {
            accel = idm_acceleration(agent_idx, gap, delta_v);
        } else {
            accel = krauss_safe_speed(agent_idx, gap, leader_speed);
        }

        // Update in-place (wave-front: subsequent agents read this)
        agents[agent_idx].speed = max(agents[agent_idx].speed + accel * params.dt, 0.0);
        agents[agent_idx].position += agents[agent_idx].speed * params.dt;
    }
}
```

**Performance note:** Only thread 0 per workgroup is active. With ~50K lanes and avg 5.6 agents/lane, each workgroup does ~6 sequential updates. GPU occupancy comes from 50K concurrent workgroups, not thread-level parallelism within a workgroup.

### Pattern 2: Fixed-Point Arithmetic in WGSL

**What:** Q16.16 / Q12.20 / Q8.8 integer arithmetic for cross-GPU determinism
**When to use:** All position and speed calculations in GPU shaders

```wgsl
alias FixPos = i32;   // Q16.16: 16 int bits, 16 frac bits
alias FixSpd = i32;   // Q12.20: 12 int bits, 20 frac bits

const POS_FRAC: u32 = 16u;
const SPD_FRAC: u32 = 20u;
const POS_SCALE: i32 = 65536;    // 1 << 16
const SPD_SCALE: i32 = 1048576;  // 1 << 20

// Safe multiplication avoiding i64: split into high/low 16-bit halves
fn fix_mul_q16(a: i32, b: i32) -> i32 {
    let a_sign = select(1, -1, a < 0);
    let b_sign = select(1, -1, b < 0);
    let ua = u32(abs(a));
    let ub = u32(abs(b));

    let ah = ua >> 16u;
    let al = ua & 0xFFFFu;
    let bh = ub >> 16u;
    let bl = ub & 0xFFFFu;

    // Result = (ah*bh)<<16 + ah*bl + al*bh + (al*bl)>>16
    let result = (ah * bh) << 16u
               + ah * bl
               + al * bh
               + (al * bl) >> 16u;

    return i32(result) * a_sign * b_sign;
}

fn f32_to_fixpos(v: f32) -> FixPos {
    return i32(v * f32(POS_SCALE));
}

fn fixpos_to_f32(v: FixPos) -> f32 {
    return f32(v) / f32(POS_SCALE);
}
```

**Fallback (@invariant):** If fixed-point performance penalty exceeds 40%, switch to f32 with @invariant on position outputs. This guarantees identical vertex positions across shader invocations on the SAME GPU but does NOT guarantee cross-vendor determinism. Document the cross-GPU delta per vendor.

### Pattern 3: Multi-GPU Partition with Boundary Protocol

**What:** METIS-partitioned road network across logical GPU partitions with outbox/inbox agent transfers
**When to use:** Multi-GPU scaling (GPU-02, GPU-05)

```rust
pub struct GpuPartition {
    device: wgpu::Device,
    queue: wgpu::Queue,
    agent_buffers: BufferPool,
    network_edges: Vec<u32>,  // edge IDs in this partition
    outbox: HashMap<u32, Vec<BoundaryAgent>>,  // dest_partition -> agents
    inbox: Vec<BoundaryAgent>,
}

pub struct MultiGpuScheduler {
    partitions: Vec<GpuPartition>,
    boundary_map: HashMap<u32, (u32, u32)>,  // edge_id -> (src_partition, dst_partition)
}

impl MultiGpuScheduler {
    pub fn step(&mut self, dt: f32) {
        // 1. Drain inboxes: spawn boundary agents on destination partitions
        for partition in &mut self.partitions {
            partition.spawn_inbox_agents();
        }
        // 2. Dispatch compute on all partitions (parallel)
        // 3. Collect outboxes: read agents that crossed partition boundaries
        for partition in &mut self.partitions {
            partition.collect_outbox_agents(&self.boundary_map);
        }
        // 4. Route outbox agents to correct partition inboxes
        self.route_boundary_agents();
    }
}
```

**Single-GPU validation strategy:** Create 2-4 logical partitions on one GPU, each with own buffers. Run the real boundary protocol (outbox/inbox staging, PCIe-simulated copy). This validates the protocol without requiring multiple physical GPUs.

### Pattern 4: SUMO .net.xml Import

**What:** Parse SUMO network files into VELOS road graph
**When to use:** NET-05 compatibility

```rust
pub struct SumoNetImporter;

impl SumoNetImporter {
    pub fn import(path: &Path) -> Result<RoadGraph, NetError> {
        // Parse with quick-xml streaming reader
        // Elements in order: edges (with lanes), junctions, connections, tlLogic, roundabouts
        // Map SUMO edge/lane structure to VELOS RoadEdge with geometry
        // Map tlLogic to VELOS SignalPlan
        // Log warnings for unmapped attributes (never silently drop)
    }
}
```

### Anti-Patterns to Avoid
- **CPU physics fallback:** After GPU validation passes, delete all CPU physics from the production sim loop. Keep CPU implementations only in test modules as reference oracles.
- **Per-agent GPU thread:** Don't dispatch one thread per agent for wave-front. Use one workgroup per lane with sequential processing within the workgroup.
- **Floating-point position comparison:** Never compare GPU positions with exact float equality. Use aggregate metrics (average speed, throughput) for validation.
- **Monolithic shader:** Don't put IDM, Krauss, MOBIL, sublane all in one giant shader. Use separate shader modules (car_following.wgsl, lane_change.wgsl) and compose via pipeline.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Graph partitioning | Custom bisection | `metis` crate (0.2.2) | METIS k-way is O(n) with edge-cut minimization; custom partitioning will have poor balance |
| XML parsing | Custom SUMO parser | `quick-xml` streaming parser | SUMO .net.xml can be 50MB+; DOM parsers will OOM |
| Binary serialization | Custom byte format | `oxicode` or `postcard` | Version migration, endianness, alignment are deceptively complex |
| Per-lane sorting | GPU radix sort | CPU `rayon` parallel sort | ~50K lanes with ~6 agents each; CPU sort is <1.5ms with rayon; GPU sort overhead (upload/dispatch/readback) exceeds CPU time at this scale |
| Spatial hashing (CPU) | Custom hash grid | `rstar` (already in workspace) | R-tree with bulk loading; well-tested |

**Key insight:** At 280K agents on ~50K lanes, the per-lane agent count is small (avg 5.6). CPU-side preparation (sorting, partitioning) is fast enough with rayon parallelism. Don't over-optimize by moving sort/partition to GPU.

## Common Pitfalls

### Pitfall 1: WGSL Fixed-Point Overflow in Multiplication
**What goes wrong:** Q16.16 * Q16.16 produces a Q32.32 result, but WGSL only has i32. Without careful splitting into high/low halves, intermediate products overflow.
**Why it happens:** No native i64 in WGSL. The architecture doc's `fix_mul` function uses a simplified approach that can still overflow for large values.
**How to avoid:** Always split operands into 16-bit halves, use u32 intermediates, handle sign separately. Test edge cases: max position (65535m) * max speed factor.
**Warning signs:** Positions jumping to negative or wrapping around; speeds suddenly becoming zero or enormous.

### Pitfall 2: Wave-Front Workgroup Utilization
**What goes wrong:** Only thread 0 in each 256-thread workgroup is active (the rest immediately return). This wastes 255/256 threads.
**Why it happens:** Wave-front requires sequential processing within a lane.
**How to avoid:** This is intentional and acceptable. With 50K workgroups, GPU occupancy is still high. However, set workgroup_size to the minimum required (e.g., 64 or even 32) to reduce resource waste. The architecture doc suggests 256 but smaller is fine since only thread 0 works.
**Warning signs:** GPU utilization metrics showing low occupancy -- verify with profiler that it's from workgroup_size not from insufficient workgroups.

### Pitfall 3: Krauss Dawdle Requires Per-Agent RNG State
**What goes wrong:** The Krauss model needs a random number per agent per step for the sigma dawdle. GPU shaders don't have a built-in RNG.
**Why it happens:** WGSL has no rand() function. CPU-side RNG can't be passed per-agent without a buffer.
**How to avoid:** Use a deterministic hash-based PRNG on GPU: `pcg_hash(agent_id ^ step_counter)` produces a pseudo-random u32. Convert to [0,1) float. Store minimal RNG state in the agent buffer (a u32 seed per agent, updated each step).
**Warning signs:** All agents dawdling identically (forgot per-agent seed); non-deterministic runs (forgot to seed from agent_id).

### Pitfall 4: SUMO .net.xml Internal Edges
**What goes wrong:** SUMO .net.xml contains internal edges (prefixed with `:`) that represent junction-internal connections. Importing these as regular edges creates spurious short edges and wrong topology.
**Why it happens:** SUMO's internal edge representation has no direct VELOS equivalent.
**How to avoid:** Filter internal edges during import. Map SUMO connections to VELOS junction connectivity directly. Only import external edges as VELOS RoadEdge instances.
**Warning signs:** Graph has thousands of extra edges <5m; junction connectivity is wrong.

### Pitfall 5: Multi-Adapter wgpu Compute Limitations
**What goes wrong:** wgpu's multi-adapter support is limited to adapter enumeration. No built-in cross-device buffer sharing or synchronized compute dispatch.
**Why it happens:** WebGPU spec doesn't define multi-device. wgpu exposes per-adapter Device objects but no inter-device primitives.
**How to avoid:** Each GPU partition gets its own Device/Queue. Agent transfer uses CPU staging: GPU A readback -> CPU -> GPU B upload. This is the only reliable path. Budget ~0.1-0.3ms per transfer per step (negligible for 64KB).
**Warning signs:** Attempting to share buffers across devices; looking for wgpu peer-to-peer APIs that don't exist.

### Pitfall 6: bincode Crate is Abandoned
**What goes wrong:** Using bincode for graph serialization -- the crate's latest release on crates.io contains only a compiler error.
**Why it happens:** Developer abandoned the project due to harassment (2025).
**How to avoid:** Use oxicode (binary-compatible successor) or postcard (smaller but different format). Pin an older bincode version (2.0.0-rc.3) as last resort.
**Warning signs:** Build failure with cryptic error from bincode 3.0.0.

## Code Examples

### Krauss Car-Following Model (CPU Reference)

Source: SUMO MSCFModel_Krauss.cpp + GitHub Issue #6791

```rust
/// Krauss car-following model parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KraussParams {
    /// Maximum acceleration (m/s^2).
    pub accel: f64,
    /// Maximum deceleration (m/s^2, positive value).
    pub decel: f64,
    /// Driver imperfection / dawdle parameter [0.0, 1.0].
    pub sigma: f64,
    /// Reaction time / driver tau (s). Typically 1.0.
    pub tau: f64,
    /// Maximum speed (m/s).
    pub max_speed: f64,
    /// Minimum gap at standstill (m).
    pub min_gap: f64,
}

impl KraussParams {
    /// SUMO default passenger car Krauss parameters.
    pub fn sumo_default() -> Self {
        Self {
            accel: 2.6,
            decel: 4.5,
            sigma: 0.5,
            tau: 1.0,
            max_speed: 13.89, // 50 km/h
            min_gap: 2.5,
        }
    }
}

/// Compute safe following speed (vsafe).
///
/// Ensures the follower can always stop before hitting the leader,
/// assuming the leader brakes maximally.
pub fn krauss_safe_speed(
    params: &KraussParams,
    gap: f64,          // net gap to leader (m)
    leader_speed: f64, // leader's current speed (m/s)
    own_speed: f64,    // own current speed (m/s)
) -> f64 {
    let tau = params.tau;
    let b = params.decel;

    // v_safe = v_leader + (gap - v_leader * tau) / ((v_leader + v_follower) / (2*b) + tau)
    let denominator = (leader_speed + own_speed) / (2.0 * b) + tau;
    let numerator = gap - leader_speed * tau;
    let v_safe = leader_speed + numerator / denominator;

    v_safe.max(0.0)
}

/// Apply Krauss dawdle: random deceleration proportional to sigma.
pub fn krauss_dawdle(speed: f64, params: &KraussParams, rng: &mut impl rand::Rng) -> f64 {
    let random: f64 = rng.gen(); // [0, 1)
    let dawdle_amount = if speed < params.accel {
        params.sigma * speed * random
    } else {
        params.sigma * params.accel * random
    };
    (speed - dawdle_amount).max(0.0)
}

/// Full Krauss velocity update for one timestep.
pub fn krauss_update(
    params: &KraussParams,
    own_speed: f64,
    gap: f64,
    leader_speed: f64,
    dt: f64,
    rng: &mut impl rand::Rng,
) -> (f64, f64) {
    let v_safe = krauss_safe_speed(params, gap, leader_speed, own_speed);
    let v_desired = (own_speed + params.accel * dt).min(params.max_speed);
    let v_next = v_desired.min(v_safe);
    let v_dawdled = krauss_dawdle(v_next, params, rng);
    let v_final = v_dawdled.max(0.0);
    let dx = v_final * dt;
    (v_final, dx)
}
```

### CarFollowingModel ECS Component

```rust
/// Car-following model selector for per-agent runtime switching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CarFollowingModel {
    IDM = 0,
    Krauss = 1,
}

/// GPU-side representation: u32 tag for shader branching.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuAgentState {
    pub edge_id: u32,
    pub lane_idx: u32,
    pub position: i32,       // Q16.16 fixed-point
    pub lateral: i32,        // Q8.8 fixed-point (stored in i32 for alignment)
    pub speed: i32,          // Q12.20 fixed-point
    pub acceleration: i32,   // Q8.24 fixed-point
    pub cf_model: u32,       // 0 = IDM, 1 = Krauss
    pub rng_state: u32,      // PCG hash state for Krauss dawdle
    // Total: 32 bytes per agent
}
```

### GPU Hash-Based PRNG for Krauss Dawdle

```wgsl
// PCG hash for deterministic per-agent random numbers
fn pcg_hash(input: u32) -> u32 {
    var state = input * 747796405u + 2891336453u;
    var word = ((state >> ((state >> 28u) + 4u)) ^ state) * 277803737u;
    return (word >> 22u) ^ word;
}

fn rand_float(agent_id: u32, step: u32) -> f32 {
    let hash = pcg_hash(agent_id ^ (step * 1664525u + 1013904223u));
    return f32(hash) / 4294967295.0;
}
```

### Network Cleaning Pipeline

```rust
pub fn clean_network(graph: &mut RoadGraph) -> CleaningReport {
    let mut report = CleaningReport::default();

    // 1. Remove disconnected components (keep largest SCC)
    let removed = remove_small_components(graph, min_edges: 10);
    report.disconnected_removed = removed;

    // 2. Merge short edges (<5m) into adjacent edges
    let merged = merge_short_edges(graph, min_length: 5.0);
    report.short_edges_merged = merged;

    // 3. Infer missing lane counts from road class
    infer_lane_counts(graph);

    // 4. Apply manual overrides from TOML file
    if let Some(overrides) = load_overrides("data/hcmc/overrides.toml") {
        apply_overrides(graph, &overrides);
        report.overrides_applied = overrides.len();
    }

    // 5. Detect motorbike-only lanes (alleys <4m wide)
    tag_motorbike_only_lanes(graph);

    // 6. Add time-dependent one-way edges
    apply_time_dependent_oneways(graph);

    // 7. Validate connectivity
    validate_connectivity(graph);

    report
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| CPU sim loop (v1.0) | GPU compute pipeline (v1.1) | Phase 5 | 10-100x throughput gain for 280K agents |
| Simple parallel dispatch | Per-lane wave-front (Gauss-Seidel) | Phase 5 | Zero collision risk, no correction pass needed |
| f32/f64 floating-point | Q16.16/Q12.20 fixed-point | Phase 5 | Cross-GPU determinism (at ~20-40% perf cost) |
| Single-district OSM import | 5-district with cleaning | Phase 5 | 25K edges vs ~3K edges |
| IDM only | IDM + Krauss (runtime-switchable) | Phase 5 | SUMO compatibility, behavioral comparison |
| bincode serialization | oxicode or postcard | 2025 | bincode abandoned; oxicode is binary-compatible successor |

**Deprecated/outdated:**
- bincode crate: abandoned in 2025, latest crates.io release is broken. Use oxicode (successor) or postcard.
- EVEN/ODD dispatch: replaced by wave-front in architecture v2. No collision correction pass needed.

## Open Questions

1. **wgpu Multi-Adapter Compute on Metal**
   - What we know: wgpu supports `Instance::enumerate_adapters()` to list GPUs. Each adapter produces its own Device/Queue. No cross-device buffer sharing.
   - What's unclear: Metal backend behavior with multiple GPUs on Mac (M-series have unified memory -- is "multi-GPU" even meaningful?). Real multi-GPU requires discrete GPUs (eGPU or Mac Pro).
   - Recommendation: Start with single-GPU + logical partitions (per user decision). Test multi-adapter only when hardware is available. The boundary protocol works identically whether partitions are on same or different devices.

2. **Fixed-Point Performance Penalty**
   - What we know: Architecture doc estimates 20% overhead. WGSL i64 is available via SHADER_INT64 feature but only natively (not web). Manual 32-bit emulation is heavier.
   - What's unclear: Actual overhead on Metal (Mac target). Could be 20% or 80% depending on how well Metal handles the split-multiply pattern.
   - Recommendation: Implement fixed-point first. Benchmark against f32+@invariant fallback. If penalty >40%, switch to @invariant path and document cross-GPU delta.

3. **SUMO .net.xml Geometry Fidelity**
   - What we know: SUMO lanes have explicit `shape` polylines. VELOS edges have simpler geometry (start/end with intermediate points).
   - What's unclear: How much lane-level geometry detail matters for behavioral equivalence when comparing SUMO import vs OSM import for the same area.
   - Recommendation: Import SUMO lane shapes as edge geometry. Use the midline of leftmost+rightmost lanes as edge centerline. Log warnings for complex junction shapes that can't be represented.

4. **Override File Format: TOML vs JSON**
   - What we know: TOML supports comments (good for documenting why an override exists). JSON is more widely tooled.
   - Recommendation: Use TOML. The override file is human-edited by domain experts correcting OSM errors. Comments are essential for documenting the reasoning ("edge 12345: OSM shows 2 lanes but field survey shows 3").

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + criterion 0.5 (benchmarks) |
| Config file | Cargo.toml workspace test configuration |
| Quick run command | `cargo test --workspace -q` |
| Full suite command | `cargo test --workspace --no-fail-fast && cargo bench --workspace -- --baseline main` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| GPU-01 | GPU physics is sole execution path | integration | `cargo test -p velos-sim --test gpu_physics -- --exact` | Wave 0 |
| GPU-02 | METIS partitioning produces balanced partitions | unit | `cargo test -p velos-net --test partition_tests` | Wave 0 |
| GPU-03 | Wave-front produces same results as sequential CPU | integration | `cargo test -p velos-sim --test wave_front_validation` | Wave 0 |
| GPU-04 | Fixed-point arithmetic matches f64 within tolerance | unit | `cargo test -p velos-core --test fixed_point_tests` | Wave 0 |
| GPU-05 | Boundary agents transfer correctly between partitions | integration | `cargo test -p velos-sim --test boundary_protocol_tests` | Wave 0 |
| GPU-06 | 280K agents at 10 steps/sec | benchmark | `cargo bench -p velos-gpu --bench dispatch` | Exists (extend) |
| NET-01 | 5-district import produces ~25K edges | integration | `cargo test -p velos-net --test import_5district` | Wave 0 |
| NET-02 | Cleaning removes disconnected, merges short edges | unit | `cargo test -p velos-net --test cleaning_tests` | Wave 0 |
| NET-03 | HCMC-specific rules applied (one-way, motorbike lanes) | unit | `cargo test -p velos-net --test hcmc_rules_tests` | Wave 0 |
| NET-04 | ToD profiles produce correct demand scaling | unit | `cargo test -p velos-demand --test tod_5district` | Wave 0 |
| NET-05 | SUMO .net.xml import produces valid graph | integration | `cargo test -p velos-net --test sumo_net_import` | Wave 0 |
| NET-06 | SUMO .rou.xml import spawns agents correctly | integration | `cargo test -p velos-net --test sumo_rou_import` | Wave 0 |
| CFM-01 | Krauss safe speed + dawdle matches SUMO reference | unit | `cargo test -p velos-vehicle --test krauss_tests` | Wave 0 |
| CFM-02 | Runtime model switch changes agent behavior | integration | `cargo test -p velos-sim --test cf_model_switch` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --workspace -q`
- **Per wave merge:** `cargo test --workspace --no-fail-fast`
- **Phase gate:** Full suite green + 280K benchmark meets 10 steps/sec before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-core/src/fixed_point.rs` -- Q16.16, Q12.20, Q8.8 types with tests
- [ ] `crates/velos-vehicle/src/krauss.rs` -- Krauss model CPU implementation with tests
- [ ] `crates/velos-vehicle/tests/krauss_tests.rs` -- Krauss vs SUMO reference values
- [ ] `crates/velos-net/src/sumo_import.rs` -- SUMO .net.xml + .rou.xml parser
- [ ] `crates/velos-net/tests/sumo_net_import.rs` -- test with sample .net.xml fixture
- [ ] `crates/velos-net/src/cleaning.rs` -- graph cleaning pipeline
- [ ] `crates/velos-net/tests/cleaning_tests.rs` -- cleaning unit tests
- [ ] `crates/velos-gpu/shaders/wave_front.wgsl` -- wave-front dispatch shader
- [ ] Test fixture: small SUMO .net.xml file (3-4 edges, 2 junctions) in `tests/fixtures/`
- [ ] Test fixture: small SUMO .rou.xml file matching the .net.xml fixture

## Sources

### Primary (HIGH confidence)
- Architecture doc `01-simulation-engine.md` -- wave-front dispatch, fixed-point, ECS layout, frame pipeline, multi-GPU
- Architecture doc `02-agent-models.md` -- IDM parameters, motorbike sublane, vehicle types
- Architecture doc `04-data-pipeline-hcmc.md` -- OSM import, network cleaning, demand profiles
- Existing codebase: `velos-gpu/src/compute.rs`, `sim.rs`, `buffers.rs`, `agent_update.wgsl` -- current GPU infrastructure
- Existing codebase: `velos-net/src/osm_import.rs`, `graph.rs` -- current OSM import pipeline
- Existing codebase: `velos-vehicle/src/idm.rs` -- CPU IDM reference implementation
- [SUMO Road Networks docs](https://sumo.dlr.de/docs/Networks/SUMO_Road_Networks.html) -- .net.xml element structure
- [SUMO Vehicle/Route definition](https://sumo.dlr.de/docs/Definition_of_Vehicles,_Vehicle_Types,_and_Routes.html) -- .rou.xml element structure
- [SUMO Krauss issue #6791](https://github.com/eclipse-sumo/sumo/issues/6791) -- safe speed formula verified
- [metis crate](https://crates.io/crates/metis) v0.2.2 -- METIS Rust bindings

### Secondary (MEDIUM confidence)
- [SUMO Car-Following Models docs](https://sumo.dlr.de/docs/Car-Following-Models.html) -- Krauss sigma dawdle behavior
- [wgpu adapter enumeration](https://docs.rs/wgpu/latest/wgpu/struct.Instance.html) -- multi-adapter API
- [gpuweb i64 issue #5152](https://github.com/gpuweb/gpuweb/issues/5152) -- WGSL integer type limitations
- [WGSL 64-bit BigInt](https://github.com/Gold18K/WebGPU-WGSL-64bit-BigInt) -- integer emulation techniques
- [bincode alternatives thread](https://users.rust-lang.org/t/bincode-alternatives/130325) -- oxicode as successor

### Tertiary (LOW confidence)
- [SUMO Krauss pre-print 2025](https://sumo.dlr.de/pdf/2025/pre-print-2638.pdf) -- SUMO's interpretation of Krauss model (not verified in detail)
- Fixed-point performance overhead estimate (20-80%) -- from architecture doc estimate + general knowledge, not benchmarked

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all core libraries are already in workspace or well-established crates
- Architecture: HIGH -- architecture docs are detailed and internally consistent; existing code confirms patterns
- GPU multi-adapter: MEDIUM -- wgpu multi-adapter API exists but real multi-GPU compute on Metal is untested
- Fixed-point: MEDIUM -- WGSL approach is sound but performance impact on Metal is unknown
- Krauss model: HIGH -- SUMO source code and documentation confirm the formula
- SUMO import: HIGH -- file format is well-documented with XSD schemas
- Pitfalls: HIGH -- derived from architecture docs, known WGSL limitations, and crate ecosystem state

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (30 days -- stable domain, wgpu version pinned)
