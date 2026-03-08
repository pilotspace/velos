# Phase 7: Intelligence, Routing & Prediction - Research

**Researched:** 2026-03-07
**Domain:** Pathfinding (CCH), prediction ensemble (BPR/ETS/historical), GPU perception, agent profiles, reroute scheduling
**Confidence:** HIGH

## Summary

Phase 7 implements the intelligence layer: agents choose routes based on multi-factor cost functions, dynamically reroute around congestion, and receive prediction-informed routing weights. The core technical challenge is building a Customizable Contraction Hierarchy (CCH) from scratch in pure Rust -- no existing Rust crate provides CCH (only standard CH via `fast_paths`). The CCH separates topology (computed once at startup) from weights (customized in ~3ms when predictions update). The prediction ensemble (BPR + ETS + historical) is straightforward arithmetic over 25K edges. The GPU perception kernel is a single compute pass reading existing buffers and writing a PerceptionResult per agent. The reroute scheduler is CPU-side, processing 1K agents/step using CCH queries.

The phase spans 5 crates: velos-net (CCH), velos-predict (new crate for prediction ensemble), velos-demand (profile assignment), velos-gpu (perception shader), and velos-core (profile encoding in GpuAgentState flags). Key new workspace dependencies needed: `arc-swap`, `tokio` (for async prediction updates), `rayon` (for parallel CCH queries).

**Primary recommendation:** Build CCH bottom-up in velos-net following the Dibbelt et al. three-phase design (ordering, customization, query). Use nested dissection with BFS-based balanced bisection for node ordering (reusing the pattern from Phase 5's METIS fallback). Start with basic customization (triangle enumeration), add perfect customization only if query times exceed 0.02ms target.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Pure Rust from-scratch CCH implementation in velos-net (no C++ FFI, no RoutingKit wrapper)
- Target under 10 seconds startup on 25K-edge HCMC network (aggressive optimization)
- Optional disk cache for node ordering + shortcut topology (enabled by default, --no-cache flag to skip)
- Cache follows Phase 5 cleaned-graph caching pattern (binary serialization, invalidated on network change)
- Weight customization (3ms target) runs on background thread (rayon/tokio), not blocking sim step
- Routing queries use previous weights until ArcSwap completes the atomic swap -- matches prediction overlay pattern
- Bidirectional Dijkstra queries at 0.02ms/query target
- Conservative rerouting: agents only reroute when cost_delta exceeds 30%+ threshold (realistic driver behavior)
- Staggered evaluation: 1K agents/step, ~50s full cycle across 280K population
- Immediate triggers (blocked edges, emergency vehicles) use priority queue -- pushed to front of staggered queue but respect 1K/step budget
- No immediate bypass -- even urgent triggers go through the queue to prevent reroute spikes
- Reroute cooldown per agent to prevent oscillation (configurable, default 30s sim-time between reroutes)
- BPR + ETS + historical prediction, Rust-native in-process, no Python bridge
- Adaptive weights: track prediction error per model, adjust weights toward better-performing model over time
- Initial weights: BPR=0.40, ETS=0.35, Historical=0.25
- Update frequency: every 60 sim-seconds
- PredictionOverlay uses ArcSwap for zero-copy, lock-free weight updates to CCH
- Distance-weighted blend for routing: nearby edges use current observed travel times, faraway edges use predicted times
- Single GPU perception compute pass: one kernel reads agent buffer, signal buffer, sign buffer, congestion map
- PerceptionResult output downloaded to CPU for evaluation (not evaluated on GPU)
- GPU perception, CPU decision split: GPU gathers perception data, CPU runs cost comparison using CCH queries
- All 8 profiles implemented: Commuter, Bus, Truck, Emergency, Tourist, Teen, Senior, Cyclist
- Shared CostWeights struct -- profiles differ only in weight values, not in logic (data-driven)
- All 6 cost factors active: time, comfort, safety, fuel, signal delay, prediction penalty
- Profile assignment: VehicleType default + random distribution
- Bus -> Bus, Truck -> Truck, Emergency -> Emergency, Bicycle -> Cyclist (1:1)
- Car/Motorbike randomly assigned Commuter/Tourist/Teen/Senior with configurable distribution percentages
- Profile ID stored in GpuAgentState flags field (4 bits)
- Cost weights also accessible CPU-side for evaluation phase

### Claude's Discretion
- PerceptionResult exact field layout and byte size
- CCH node ordering algorithm choice (nested dissection, graph bisection, etc.)
- Prediction error tracking mechanism for adaptive weights
- Grid heatmap update frequency and interpolation method
- Exact reroute cooldown value tuning
- ETS model implementation details (smoothing parameters, initialization)
- Historical matcher data structure (3D array vs HashMap)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INT-01 | Multi-factor pathfinding cost function: time, comfort, safety, fuel, signal delay, prediction penalty | CostWeights struct with 6 factors; cost function pattern documented in Code Examples |
| INT-02 | Configurable agent profiles (8 types) with per-profile cost weights | Data-driven CostWeights lookup table; profile_id encoded in 4 bits of GpuAgentState.flags |
| INT-03 | GPU perception phase: sense leader, signal, signs, nearby agents, congestion map | Single perception.wgsl compute pass; PerceptionResult struct design in Architecture Patterns |
| INT-04 | GPU evaluation phase: cost comparison, should_reroute flag + cost_delta | CPU-side evaluation (decision locked); GPU perception + CPU CCH query hybrid |
| INT-05 | Staggered reroute evaluation (1K agents/step) with immediate triggers | RerouteScheduler with ring-buffer index + priority VecDeque; cooldown tracking |
| RTE-01 | CCH replaces A* for pathfinding on 25K-edge network | Pure Rust CCH in velos-net; three-phase algorithm (ordering, customization, query) |
| RTE-02 | CCH supports 3ms dynamic weight customization | Bottom-up triangle enumeration; background thread + ArcSwap atomic swap |
| RTE-03 | Dynamic agent rerouting at 500 reroutes/step using CCH (0.02ms/query) | Bidirectional Dijkstra on upper CCH graph; rayon parallel queries |
| RTE-04 | BPR + ETS + historical prediction ensemble every 60 sim-seconds | New velos-predict crate; tokio::spawn async update; 25K edges x 3 models ~0.1ms |
| RTE-05 | PredictionOverlay uses ArcSwap for zero-copy, lock-free weight updates | arc-swap crate; Arc<ArcSwap<PredictionOverlay>> pattern |
| RTE-06 | Global network knowledge: real-time congestion map feeds pathfinding | Per-edge travel time ratio + 500m grid heatmap; perception kernel reads congestion buffer |
| RTE-07 | Prediction-informed routing: cost function uses predicted future travel times | Distance-weighted blend: nearby = observed, faraway = predicted; confidence penalty |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| arc-swap | 1.x | Lock-free atomic swap for prediction overlay | 143M+ downloads; read-optimized for config-reload pattern; zero-lock reads |
| rayon | 1.x | Parallel CCH queries + customization | De facto Rust parallel iterator; used in CCH query parallelization |
| tokio | 1.x | Async prediction ensemble updates | Background async task spawning; prediction runs every 60s non-blocking |
| petgraph | 0.6 (existing) | Graph data structures for CCH construction | Already in workspace; DiGraph used for RoadGraph |
| postcard | 1.x (existing) | Binary serialization for CCH cache | Already in workspace; Phase 5 pattern for graph caching |
| bytemuck | 1.x (existing) | GPU struct serialization for PerceptionResult | Already in workspace; Pod/Zeroable derives |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| serde | 1.x (existing) | Serialization for CCH cache structs | CCH ordering + topology persistence |
| rand | 0.8 (existing) | Profile random assignment distribution | Car/Motorbike profile distribution |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Custom CCH | fast_paths crate | fast_paths is standard CH only -- no weight customization support. MUST build custom. |
| Custom ETS | augurs-ets 0.10 | augurs-ets adds nalgebra + heavy deps for what is essentially `level = alpha * observed + (1-alpha) * level`. Hand-roll is 50 lines. |
| tokio for async | std::thread::spawn | tokio needed only for spawn_blocking; std::thread works too but tokio integrates with future async API needs |

### Not Using (Explicitly Rejected by Architecture)
| Library | Reason |
|---------|--------|
| RoutingKit (C++ FFI) | All-Rust codebase policy; no C++ FFI |
| augurs-ets | Heavy dependency chain; ETS is simple enough to implement in ~50 lines |
| ndarray | Historical matcher can use a flat Vec<f32> indexed by `[edge * 24 * 4 + hour * 4 + day_type]` instead |

**Installation:**
```bash
# Add to workspace Cargo.toml [workspace.dependencies]
# arc-swap = "1"
# rayon = "1"
# tokio = { version = "1", features = ["rt", "macros"] }
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
  velos-net/src/
    cch/
      mod.rs           # CCHRouter public API
      ordering.rs      # Nested dissection node ordering
      topology.rs      # Shortcut graph construction (contraction)
      customization.rs # Weight customization (triangle enumeration)
      query.rs         # Bidirectional Dijkstra on upper graph
      cache.rs         # Binary serialization for CCH ordering+topology
    routing.rs         # Existing A* (kept as fallback, deprecated)
  velos-predict/src/
    lib.rs             # PredictionEnsemble orchestrator
    bpr.rs             # BPR physics extrapolation
    ets.rs             # Exponential smoothing correction
    historical.rs      # Historical pattern matcher
    overlay.rs         # PredictionOverlay + ArcSwap wrapper
    adaptive.rs        # Weight adaptation based on prediction error
  velos-gpu/shaders/
    perception.wgsl    # GPU perception kernel
  velos-gpu/src/
    perception.rs      # PerceptionPipeline (Rust-side binding)
  velos-demand/src/
    profile.rs         # AgentProfile definitions + assignment logic
  velos-core/src/
    cost.rs            # CostWeights + route_cost function
    reroute.rs         # RerouteScheduler + staggered evaluation
```

### Pattern 1: CCH Three-Phase Architecture
**What:** Separate topology-dependent preprocessing from weight-dependent customization
**When to use:** Always -- this is the core CCH design

**Phase 1 -- Ordering (startup, ~10s for 25K edges):**
1. Compute nested dissection order using recursive balanced bisection
2. Contract nodes in order: for each node v, add shortcut edges between all pairs of v's neighbors if no witness path exists through lower-ranked nodes
3. Store: node_order (Vec<u32>), shortcut_graph (adjacency lists with shortcut decomposition)

**Phase 2 -- Customization (~3ms per update):**
1. For each edge in the CCH (bottom-up by rank):
   - If original edge: weight = current travel time
   - If shortcut: weight = min over all lower triangles that form this shortcut
2. Process upward edges and downward edges separately (forward/backward)
3. The "triangle" is: shortcut (u,v) can be decomposed into (u,w) + (w,v) where rank(w) < min(rank(u), rank(v))

**Phase 3 -- Query (~0.02ms per query):**
1. Bidirectional Dijkstra: forward search from source exploring only upward edges, backward search from target exploring only upward edges
2. Meet in the middle at the highest-ranked node on the shortest path
3. Unpack path by recursively expanding shortcuts

```rust
// Core CCH API
pub struct CCHRouter {
    node_order: Vec<u32>,           // node -> rank
    rank_to_node: Vec<u32>,        // rank -> node (inverse)
    // Forward star: upward edges from each node (by rank)
    forward_head: Vec<u32>,
    forward_first_out: Vec<u32>,
    forward_weight: Vec<f32>,
    // Backward star: upward edges to each node (by rank)
    backward_head: Vec<u32>,
    backward_first_out: Vec<u32>,
    backward_weight: Vec<f32>,
    // Shortcut decomposition for path unpacking
    shortcut_middle: Vec<Option<u32>>, // None = original edge
    // Original edge mapping
    original_edge_to_cch: Vec<usize>,
}
```

### Pattern 2: ArcSwap Prediction Overlay
**What:** Lock-free atomic swap of prediction data consumed by CCH queries
**When to use:** Every 60 sim-seconds when prediction ensemble produces new weights

```rust
use arc_swap::ArcSwap;
use std::sync::Arc;

pub struct PredictionOverlay {
    pub edge_travel_times: Vec<f32>,
    pub edge_confidence: Vec<f32>,
    pub timestamp_sim_seconds: f64,
}

pub struct PredictionService {
    overlay: Arc<ArcSwap<PredictionOverlay>>,
    ensemble: PredictionEnsemble,
}

impl PredictionService {
    /// Non-blocking read of current predictions (for routing queries)
    pub fn current_overlay(&self) -> arc_swap::Guard<Arc<PredictionOverlay>> {
        self.overlay.load()
    }

    /// Async update -- runs on tokio::spawn, swaps atomically when done
    pub async fn update(&self, snapshot: SimSnapshot) {
        let new_overlay = self.ensemble.compute(&snapshot);
        self.overlay.store(Arc::new(new_overlay));
    }
}
```

### Pattern 3: GPU Perception + CPU Decision Split
**What:** GPU reads all data sources in single pass; CPU evaluates reroute decisions
**When to use:** Every simulation step for the 1K agents scheduled for evaluation

```rust
// GPU side: PerceptionResult per agent (downloaded to CPU)
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PerceptionResult {
    pub leader_speed: f32,          // leader vehicle speed (m/s), 0 if none
    pub leader_gap: f32,            // gap to leader (m), 9999.0 if none
    pub signal_state: u32,          // 0=green, 1=amber, 2=red, 3=none
    pub signal_distance: f32,       // distance to next signal (m)
    pub congestion_own_route: f32,  // travel time ratio on own route edges (1.0 = free flow)
    pub congestion_area: f32,       // grid heatmap value at agent position (0-1)
    pub sign_speed_limit: f32,      // active speed limit (m/s), 0 if none
    pub flags: u32,                 // bit0=route_blocked, bit1=emergency_nearby
}
// Total: 32 bytes per agent
```

### Pattern 4: Data-Driven Agent Profiles
**What:** All profiles share the same CostWeights struct; behavior differences come from weight values only
**When to use:** Profile definition, assignment at spawn, cost evaluation

```rust
pub struct CostWeights {
    pub time: f32,              // travel time weight
    pub comfort: f32,           // ride comfort (turns, surface quality)
    pub safety: f32,            // safety score (accident history, lighting)
    pub fuel: f32,              // fuel/energy consumption
    pub signal_delay: f32,      // expected signal delay penalty
    pub prediction_penalty: f32, // penalty for low-confidence predicted edges
}

// Profile ID constants (stored in 4 bits of GpuAgentState.flags)
pub const PROFILE_COMMUTER: u8 = 0;
pub const PROFILE_BUS: u8 = 1;
pub const PROFILE_TRUCK: u8 = 2;
pub const PROFILE_EMERGENCY: u8 = 3;
pub const PROFILE_TOURIST: u8 = 4;
pub const PROFILE_TEEN: u8 = 5;
pub const PROFILE_SENIOR: u8 = 6;
pub const PROFILE_CYCLIST: u8 = 7;

pub const PROFILE_WEIGHTS: [CostWeights; 8] = [
    // Commuter: time-focused, moderate fuel concern
    CostWeights { time: 0.40, comfort: 0.05, safety: 0.10, fuel: 0.20, signal_delay: 0.15, prediction_penalty: 0.10 },
    // Bus: schedule adherence (time), safety paramount
    CostWeights { time: 0.35, comfort: 0.05, safety: 0.25, fuel: 0.15, signal_delay: 0.10, prediction_penalty: 0.10 },
    // Truck: fuel-heavy (weight = cost), avoids tight roads
    CostWeights { time: 0.20, comfort: 0.10, safety: 0.15, fuel: 0.35, signal_delay: 0.10, prediction_penalty: 0.10 },
    // Emergency: pure time, ignore comfort/fuel
    CostWeights { time: 0.80, comfort: 0.00, safety: 0.05, fuel: 0.00, signal_delay: 0.10, prediction_penalty: 0.05 },
    // Tourist: comfort and safety, not rushed
    CostWeights { time: 0.10, comfort: 0.35, safety: 0.30, fuel: 0.05, signal_delay: 0.05, prediction_penalty: 0.15 },
    // Teen: time-focused, low safety concern (risky behavior)
    CostWeights { time: 0.45, comfort: 0.10, safety: 0.05, fuel: 0.15, signal_delay: 0.15, prediction_penalty: 0.10 },
    // Senior: safety and comfort, slow and steady
    CostWeights { time: 0.10, comfort: 0.25, safety: 0.40, fuel: 0.10, signal_delay: 0.05, prediction_penalty: 0.10 },
    // Cyclist: safety paramount, comfort matters, fuel irrelevant
    CostWeights { time: 0.15, comfort: 0.20, safety: 0.45, fuel: 0.00, signal_delay: 0.10, prediction_penalty: 0.10 },
];
```

### Anti-Patterns to Avoid
- **Blocking CCH customization on sim thread:** Weight customization MUST run on background thread. Sim reads previous weights via ArcSwap until new weights are ready.
- **Evaluating all agents for reroute every step:** Only 1K/step. Full-population evaluation would cost 280K * 0.02ms = 5.6s per step.
- **GPU-side route computation:** CCH queries are CPU operations (graph traversal with priority queues). GPU excels at per-agent parallel perception, not sequential graph search.
- **Per-agent ETS model:** ETS runs per-edge (25K), not per-agent (280K). Agents consume the shared overlay.
- **Storing full routes on GPU:** Routes are CPU-managed; GPU only knows current edge_id and next edge. CCH queries and route storage stay on CPU.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Atomic config swap | Mutex<Arc<T>> with read/write locks | arc-swap crate | Mutex contention under 280K agent reads; arc-swap is wait-free for readers |
| Parallel CCH queries | Manual thread pool with channels | rayon parallel iterators | rayon handles work-stealing and core affinity; `.par_iter().map(|a| cch.query(a.src, a.dst))` |
| Binary heap for Dijkstra | Custom heap implementation | std::collections::BinaryHeap | Standard library, well-optimized, sufficient for 0.02ms queries |
| Graph bisection | Custom recursive partitioner | BFS-based balanced bisection (existing pattern from Phase 5) | Already proven in codebase for METIS fallback |

**Key insight:** The CCH algorithm itself must be hand-rolled (no Rust crate exists), but the building blocks (BinaryHeap, rayon, ArcSwap, postcard serialization) should all use established crates.

## Common Pitfalls

### Pitfall 1: CCH Shortcut Explosion on Dense Graphs
**What goes wrong:** Node ordering quality directly affects shortcut count. Bad ordering creates O(n^2) shortcuts, making customization and queries slow.
**Why it happens:** Contracting high-degree nodes early creates shortcuts between all their neighbors.
**How to avoid:** Use nested dissection ordering (contract separator nodes LAST, not first). For HCMC's 25K edges, a balanced bisection with depth 15-20 produces good orderings.
**Warning signs:** If shortcut count exceeds 3x original edge count, ordering is suboptimal. Target: 25K original edges should yield 40-60K total CCH edges.

### Pitfall 2: ArcSwap Store During Hot Loop
**What goes wrong:** Calling `overlay.store()` from the prediction thread while 1K routing queries are running causes a brief stall.
**Why it happens:** ArcSwap store is not lock-free (readers in critical section block writers). If many readers are active simultaneously, writer waits.
**How to avoid:** Time prediction updates to land between step boundaries when no queries are active. Since prediction runs every 60s and queries run every 0.1s step, schedule the swap during the gap between dispatch_wave_front and route evaluation.
**Warning signs:** Occasional frame time spikes correlated with prediction update timing.

### Pitfall 3: Reroute Oscillation
**What goes wrong:** Agent A reroutes to path P2, reducing congestion on P1. Agent B then reroutes back to P1 since it's now clear. Both keep switching.
**Why it happens:** Without cooldown, agents react to instantaneous conditions.
**How to avoid:** 30s cooldown between reroutes per agent. 30% cost_delta threshold (don't reroute for marginal gains). Prediction-informed routing dampens oscillation because agents see FUTURE congestion on the alternative too.
**Warning signs:** High reroute rate (>10% of population per cycle) combined with no improvement in network travel times.

### Pitfall 4: ETS Divergence on Sudden Changes
**What goes wrong:** ETS smoothing is too slow to react to sudden events (road closure, accident). Predictions lag reality by multiple cycles.
**Why it happens:** High smoothing parameter (alpha close to 0) means slow adaptation.
**How to avoid:** Use two ETS trackers: a "fast" one (alpha=0.7) for short-term and a "slow" one (alpha=0.2) for trend. Blend based on prediction error magnitude. When error spikes, weight fast tracker higher.
**Warning signs:** Prediction error per edge exceeding 2x for multiple consecutive update cycles.

### Pitfall 5: GPU Perception Buffer Binding Conflicts
**What goes wrong:** perception.wgsl needs to read agent buffer, signal buffer, AND sign buffer. Existing wave_front.wgsl already uses bindings 0-6.
**Why it happens:** Single bind group with 8+ bindings approaches WebGPU limits on some backends.
**How to avoid:** Use a SEPARATE bind group for perception (different pipeline). Perception runs as its own compute pass AFTER wave_front, with its own bind group layout. This also allows perception to read the UPDATED agent positions from the wave-front pass.
**Warning signs:** Compilation errors or runtime validation failures on Metal backend with too many bindings.

### Pitfall 6: Profile Bits Overwriting Existing Flags
**What goes wrong:** Writing profile_id to GpuAgentState.flags[bits 3-6] corrupts existing bit0=bus_dwelling, bit1=emergency_active, bit2=yielding.
**Why it happens:** Bit masking error in profile encoding.
**How to avoid:** Profile occupies bits 4-7 (upper nibble of lower byte): `flags = (flags & 0x0F) | (profile_id << 4)`. Read: `(flags >> 4) & 0x0F`. Existing flags use bits 0-2, so bits 3-7 are available.
**Warning signs:** Bus agents losing dwelling state or emergency vehicles losing active siren state after profile assignment.

## Code Examples

### CCH Weight Customization (Core Algorithm)
```rust
// Bottom-up triangle enumeration for CCH customization
// For each shortcut edge, its weight = minimum over all triangles that form it
pub fn customize(&mut self, original_weights: &[f32]) {
    // 1. Initialize CCH edge weights from original graph
    for (orig_idx, &cch_idx) in self.original_edge_to_cch.iter().enumerate() {
        self.forward_weight[cch_idx] = original_weights[orig_idx];
        self.backward_weight[cch_idx] = original_weights[orig_idx];
    }

    // 2. Process nodes bottom-up by rank (low rank first)
    for rank in 0..self.node_count {
        let node = self.rank_to_node[rank];
        // For each pair of (downward edge into node, upward edge out of node),
        // update the shortcut weight if this triangle provides a shorter path
        let down_start = self.backward_first_out[node] as usize;
        let down_end = self.backward_first_out[node + 1] as usize;
        let up_start = self.forward_first_out[node] as usize;
        let up_end = self.forward_first_out[node + 1] as usize;

        for d in down_start..down_end {
            for u in up_start..up_end {
                let shortcut_weight = self.backward_weight[d] + self.forward_weight[u];
                // Find the direct edge between backward_head[d] and forward_head[u]
                // and update if this path through 'node' is shorter
                if let Some(edge_idx) = self.find_edge(
                    self.backward_head[d], self.forward_head[u]
                ) {
                    self.forward_weight[edge_idx] =
                        self.forward_weight[edge_idx].min(shortcut_weight);
                }
            }
        }
    }
}
```

### BPR Prediction Model
```rust
// Reuses BPR logic from velos-meso SpatialQueue but predicts FUTURE travel times
pub struct BPRPredictor {
    alpha: f64,  // 0.15 standard
    beta: f64,   // 4.0 standard
}

impl BPRPredictor {
    pub fn predict(&self, edge_flows: &[f32], edge_capacities: &[f32], edge_free_flow: &[f32]) -> Vec<f32> {
        edge_flows.iter()
            .zip(edge_capacities.iter())
            .zip(edge_free_flow.iter())
            .map(|((&flow, &cap), &t_free)| {
                let vc = (flow as f64) / (cap as f64).max(1.0);
                let vc4 = vc * vc * vc * vc; // beta=4.0 fast path
                (t_free as f64 * (1.0 + self.alpha * vc4)) as f32
            })
            .collect()
    }
}
```

### Simple ETS Correction (Hand-Rolled, ~30 lines)
```rust
pub struct ETSCorrector {
    /// Smoothed correction per edge
    correction: Vec<f32>,
    /// Smoothing parameter (0.3 = moderate reactivity)
    gamma: f32,
}

impl ETSCorrector {
    pub fn new(edge_count: usize) -> Self {
        Self {
            correction: vec![0.0; edge_count],
            gamma: 0.3,
        }
    }

    pub fn predict(&mut self, bpr_predictions: &[f32], actual_travel_times: &[f32]) -> Vec<f32> {
        bpr_predictions.iter()
            .zip(actual_travel_times.iter())
            .zip(self.correction.iter_mut())
            .map(|((&pred, &actual), corr)| {
                let error = actual - pred;
                *corr = self.gamma * error + (1.0 - self.gamma) * *corr;
                pred + *corr
            })
            .collect()
    }
}
```

### Route Cost Function
```rust
pub fn route_cost(
    edges: &[u32],
    overlay: &PredictionOverlay,
    weights: &CostWeights,
    edge_attrs: &EdgeAttributes,  // safety, comfort, distance, signal count
    agent_distance_from_start: f32,
) -> f32 {
    let mut total = 0.0;
    let mut cumulative_distance = 0.0;

    for &edge_id in edges {
        let idx = edge_id as usize;
        let attrs = &edge_attrs[idx];
        let predicted_tt = overlay.edge_travel_times[idx];
        let confidence = overlay.edge_confidence[idx];

        // Distance-weighted blend: nearby = observed, faraway = predicted
        let blend = (cumulative_distance / 2000.0).min(1.0); // full prediction at 2km
        let observed_tt = attrs.current_travel_time;
        let tt = observed_tt * (1.0 - blend) + predicted_tt * blend;

        total += weights.time * tt
              + weights.comfort * attrs.comfort_penalty
              + weights.safety * attrs.safety_score
              + weights.fuel * attrs.distance_m * attrs.fuel_rate
              + weights.signal_delay * attrs.signal_delay;

        // Low confidence -> add prediction penalty
        if confidence < 0.5 {
            total += weights.prediction_penalty * tt * (1.0 - confidence);
        }

        cumulative_distance += attrs.distance_m;
    }
    total
}
```

### Perception WGSL Kernel
```wgsl
// perception.wgsl -- gather per-agent awareness data
// Runs AFTER wave_front_update so agent positions are current

struct PerceptionParams {
    agent_count: u32,
    grid_width: u32,    // congestion grid dimensions
    grid_height: u32,
    grid_cell_size: f32, // 500m
}

struct PerceptionResult {
    leader_speed: f32,
    leader_gap: f32,
    signal_state: u32,
    signal_distance: f32,
    congestion_own_route: f32,
    congestion_area: f32,
    sign_speed_limit: f32,
    flags: u32,
}

@group(0) @binding(0) var<uniform> params: PerceptionParams;
@group(0) @binding(1) var<storage, read> agents: array<AgentState>;
@group(0) @binding(2) var<storage, read> lane_agents: array<u32>;
@group(0) @binding(3) var<storage, read> signals: array<SignalState>;
@group(0) @binding(4) var<storage, read> signs: array<GpuSign>;
@group(0) @binding(5) var<storage, read> congestion_grid: array<f32>;
@group(0) @binding(6) var<storage, read> edge_travel_ratios: array<f32>;
@group(0) @binding(7) var<storage, read_write> results: array<PerceptionResult>;

@compute @workgroup_size(256)
fn perception_gather(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= params.agent_count) { return; }

    var result: PerceptionResult;
    let agent = agents[idx];

    // Leader detection: find closest agent ahead on same edge+lane
    // ... (scan lane_agents for same edge_id, lane_idx, higher position)

    // Signal state: find signal controlling agent's current edge
    // ... (binary search or linear scan of signals buffer)

    // Congestion: read grid cell at agent's position
    // ... (convert position to grid coordinates)

    // Edge travel ratio: read for agent's current edge
    result.congestion_own_route = edge_travel_ratios[agent.edge_id];

    results[idx] = result;
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| A* pathfinding (routing.rs) | CCH with customizable weights | Phase 7 | 25x faster queries; supports dynamic weights without rebuild |
| No prediction | BPR+ETS+historical ensemble | Phase 7 | Agents anticipate future congestion, not just react |
| Single cost factor (time) | 6-factor weighted cost function | Phase 7 | Profile-driven route choice differentiation |
| No rerouting | Staggered reroute with 30% threshold | Phase 7 | Dynamic congestion avoidance without oscillation |

**Deprecated/outdated:**
- `velos-net/src/routing.rs` (A* routing): Keep as fallback for testing, but CCH is the production path
- Direct `petgraph::algo::astar` calls: Replace with `CCHRouter::query()`

## Open Questions

1. **CCH Node Ordering Quality vs Speed**
   - What we know: Nested dissection with BFS-balanced bisection works (proven in Phase 5 METIS fallback). FlowCutter produces better orderings but is slower.
   - What's unclear: Whether BFS bisection quality is sufficient for 0.02ms query target on 25K edges, or if more sophisticated ordering (InertialFlow) is needed.
   - Recommendation: Start with BFS bisection. Measure query times. If >0.05ms, invest in InertialFlow (uses node coordinates for initial partition, which we have from RoadNode.pos).

2. **Edge Attributes for Cost Factors**
   - What we know: Time = edge travel time. Distance = edge length_m (existing). Safety/comfort scores need some source.
   - What's unclear: How to derive safety and comfort scores for HCMC edges. No ground truth data exists.
   - Recommendation: Derive from road_class heuristically: Motorway/Trunk = low comfort + high safety; Residential = high comfort + medium safety; Service = low safety + medium comfort. Configurable via TOML.

3. **Congestion Grid Update Frequency**
   - What we know: Grid cells are 500m. Updated from edge travel time ratios.
   - What's unclear: Should the grid update every step (costly for 280K agents) or batch at intervals?
   - Recommendation: Update every 10 steps (1 sim-second). Grid is coarse awareness, not precise; staleness of 1s is acceptable.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Cargo.toml per crate |
| Quick run command | `cargo test -p velos-net --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INT-01 | 6-factor cost function produces different costs for different weight configs | unit | `cargo test -p velos-core --lib cost` | No -- Wave 0 |
| INT-02 | 8 profiles produce distinct route choices for same OD pair | integration | `cargo test -p velos-net --test profile_routing` | No -- Wave 0 |
| INT-03 | Perception kernel outputs valid PerceptionResult | unit (GPU) | `cargo test -p velos-gpu --lib perception` | No -- Wave 0 |
| INT-04 | CPU evaluation produces should_reroute flag based on cost_delta | unit | `cargo test -p velos-core --lib reroute` | No -- Wave 0 |
| INT-05 | Staggered scheduler processes exactly 1K agents/step; immediate triggers front-loaded | unit | `cargo test -p velos-core --lib reroute` | No -- Wave 0 |
| RTE-01 | CCH query returns same shortest path as A* on test graph | unit | `cargo test -p velos-net --lib cch` | No -- Wave 0 |
| RTE-02 | CCH customization completes in <10ms on 25K-edge graph | bench/unit | `cargo test -p velos-net --lib cch::customization` | No -- Wave 0 |
| RTE-03 | 500 CCH queries complete in <2ms with rayon | bench/unit | `cargo test -p velos-net --lib cch::query` | No -- Wave 0 |
| RTE-04 | Prediction ensemble produces valid travel times for all edges | unit | `cargo test -p velos-predict --lib` | No -- Wave 0 (crate doesn't exist) |
| RTE-05 | ArcSwap overlay swap is observed by concurrent readers | unit | `cargo test -p velos-predict --lib overlay` | No -- Wave 0 |
| RTE-06 | Congestion map reflects current edge travel times | unit | `cargo test -p velos-predict --lib` | No -- Wave 0 |
| RTE-07 | Prediction-informed cost differs from observed-only cost on congested edges | unit | `cargo test -p velos-core --lib cost` | No -- Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p {affected_crate} --lib`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-predict/` -- new crate needs scaffolding (Cargo.toml, lib.rs, tests/)
- [ ] `crates/velos-net/src/cch/` -- new module directory
- [ ] `crates/velos-net/tests/cch_tests.rs` -- CCH correctness vs A* baseline
- [ ] `crates/velos-core/src/cost.rs` -- CostWeights + route_cost tests
- [ ] `crates/velos-core/src/reroute.rs` -- RerouteScheduler tests
- [ ] `crates/velos-gpu/shaders/perception.wgsl` -- perception kernel
- [ ] `crates/velos-predict/tests/ensemble_tests.rs` -- prediction ensemble tests
- [ ] Workspace Cargo.toml: add arc-swap, rayon, tokio dependencies

## Sources

### Primary (HIGH confidence)
- Architecture doc `docs/architect/03-routing-prediction.md` -- CCH design, prediction ensemble, reroute strategy, cost function
- Architecture doc `docs/architect/02-agent-models.md` -- agent profiles, CostWeights struct
- Architecture doc `docs/architect/01-simulation-engine.md` -- frame pipeline, GPU buffer layout, fixed-point
- Existing codebase: `velos-core/src/components.rs` (GpuAgentState 40-byte layout, flags field)
- Existing codebase: `velos-net/src/routing.rs` (A* baseline to replace)
- Existing codebase: `velos-gpu/src/compute.rs` (ComputeDispatcher pattern, buffer bindings)
- Existing codebase: `velos-meso/src/queue_model.rs` (BPR travel time function, reusable)

### Secondary (MEDIUM confidence)
- [RoutingKit CCH documentation](https://github.com/RoutingKit/RoutingKit/blob/master/doc/CustomizableContractionHierarchy.md) -- CCH API design, three-phase pattern, customization variants
- [Dibbelt et al. "Customizable Contraction Hierarchies"](https://arxiv.org/abs/1402.0402) -- Original CCH paper, nested dissection ordering, triangle-based customization
- [CCH Survey (2025)](https://arxiv.org/abs/2502.10519) -- Confirms CCH competitive with CH and CRP
- [Nested dissection orders for CCH](https://www.mdpi.com/1999-4893/12/9/196) -- FlowCutter + InertialFlow for node ordering
- [arc-swap docs](https://docs.rs/arc-swap) -- API patterns for lock-free atomic swap, Cache utility
- [augurs-ets](https://docs.rs/augurs-ets) -- Evaluated and rejected; too heavy for simple ETS

### Tertiary (LOW confidence)
- ETS smoothing parameter choices (gamma=0.3) -- based on general time series literature, not HCMC-specific validation. Needs tuning.
- Profile weight values -- heuristic, not calibrated against real driver behavior data. Will need calibration phase.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- arc-swap, rayon, tokio are well-established; CCH algorithm well-documented in literature
- Architecture: HIGH -- architecture docs provide detailed design; existing codebase patterns (ComputeDispatcher, postcard cache) are proven
- Pitfalls: MEDIUM -- CCH ordering quality and ArcSwap timing are real concerns; profile weight values need calibration
- CCH implementation: MEDIUM -- algorithm is well-understood from literature but pure Rust implementation is novel; no existing Rust CCH to reference

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable domain; CCH algorithm is mature)
