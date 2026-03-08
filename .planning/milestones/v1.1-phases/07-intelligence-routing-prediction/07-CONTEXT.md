# Phase 7: Intelligence, Routing & Prediction - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Agents make intelligent route choices using predicted future conditions, reroute dynamically around congestion, and exhibit profile-driven behavior differences. This phase implements CCH pathfinding, multi-factor cost functions with 8 agent profiles, GPU perception phase, CPU-side reroute evaluation with staggered scheduling, and BPR+ETS+historical prediction ensemble with adaptive weights and ArcSwap overlay.

Requirements: INT-01, INT-02, INT-03, INT-04, INT-05, RTE-01, RTE-02, RTE-03, RTE-04, RTE-05, RTE-06, RTE-07.

</domain>

<decisions>
## Implementation Decisions

### CCH Implementation Strategy
- Pure Rust from-scratch implementation in velos-net (no C++ FFI, no RoutingKit wrapper)
- Target under 10 seconds startup on 25K-edge HCMC network (aggressive optimization)
- Optional disk cache for node ordering + shortcut topology (enabled by default, --no-cache flag to skip)
- Cache follows Phase 5 cleaned-graph caching pattern (binary serialization, invalidated on network change)
- Weight customization (3ms target) runs on background thread (rayon/tokio), not blocking sim step
- Routing queries use previous weights until ArcSwap completes the atomic swap -- matches prediction overlay pattern
- Bidirectional Dijkstra queries at 0.02ms/query target

### Reroute Triggering & Scheduling
- Conservative rerouting: agents only reroute when cost_delta exceeds 30%+ threshold (realistic driver behavior)
- Staggered evaluation: 1K agents/step, ~50s full cycle across 280K population
- Immediate triggers (blocked edges, emergency vehicles) use priority queue -- pushed to front of staggered queue but respect 1K/step budget
- No immediate bypass -- even urgent triggers go through the queue to prevent reroute spikes
- Reroute cooldown per agent to prevent oscillation (configurable, default 30s sim-time between reroutes)

### Prediction Ensemble
- BPR + ETS + historical prediction, Rust-native in-process, no Python bridge
- Adaptive weights: track prediction error per model, adjust weights toward better-performing model over time
- Initial weights: BPR=0.40, ETS=0.35, Historical=0.25 (architecture doc defaults as starting point)
- Update frequency: every 60 sim-seconds (architecture doc spec)
- PredictionOverlay uses ArcSwap for zero-copy, lock-free weight updates to CCH
- Distance-weighted blend for routing: nearby edges use current observed travel times, faraway edges use predicted times

### GPU Perception Phase
- Single compute pass: one GPU kernel reads all data sources (agent buffer, signal buffer, sign buffer, congestion map) and writes PerceptionResult per agent
- PerceptionResult content: Claude's discretion on exact fields -- start minimal, expand based on evaluation needs
- Hybrid congestion map: per-edge travel time ratio for agent's own route + grid-based heatmap (500m cells) for global area awareness
- Perception output downloaded to CPU for evaluation (not evaluated on GPU)

### Reroute Evaluation (CPU-side)
- GPU perception, CPU decision split: GPU gathers perception data, CPU runs cost comparison using CCH queries
- Natural architecture since CCH queries are CPU operations
- CPU evaluates should_reroute flag + cost_delta for staggered 1K agents per step
- Full access to route data and CCH for alternative path computation on CPU

### Agent Profiles & Cost Function
- All 8 profiles implemented: Commuter, Bus, Truck, Emergency, Tourist, Teen, Senior, Cyclist
- Shared CostWeights struct -- profiles differ only in weight values, not in logic (data-driven)
- All 6 cost factors active: time, comfort, safety, fuel, signal delay, prediction penalty (INT-01)
- Profile assignment: VehicleType default + random distribution (Car/Motorbike randomly assigned Commuter/Tourist/Teen/Senior with configurable distribution percentages)
- Bus -> Bus profile, Truck -> Truck profile, Emergency -> Emergency profile, Bicycle -> Cyclist profile (1:1 for these types)
- Profile ID stored in GpuAgentState flags field (4 bits). GPU reads weight lookup table from uniform buffer for future GPU-side cost computation
- Cost weights also accessible CPU-side for evaluation phase

### Claude's Discretion
- PerceptionResult exact field layout and byte size
- CCH node ordering algorithm choice (nested dissection, graph bisection, etc.)
- Prediction error tracking mechanism for adaptive weights
- Grid heatmap update frequency and interpolation method
- Exact reroute cooldown value tuning
- ETS model implementation details (smoothing parameters, initialization)
- Historical matcher data structure (3D array vs HashMap)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `GpuAgentState` (velos-core): 40 bytes with vehicle_type (u32) and flags (u32) -- profile_id fits in flags field
- `ArcSwap` pattern: already planned for prediction overlay -- reuse for CCH weight swaps
- `ComputeDispatcher` (velos-gpu): GPU pipeline pattern for perception kernel
- Signal buffer (binding 5), sign buffer (binding 6): existing GPU buffer bindings -- perception kernel reads these
- `WaveFrontParams`: extended to 32 bytes in Phase 6 -- may need further extension for perception params
- `spawner`/`tod_profile` (velos-demand): agent spawning infrastructure -- extend with profile assignment
- Dual enforcement pattern (Phase 6): pathfinding cost + runtime behavior -- applies to cost function design

### Established Patterns
- GPU compute dispatch via ComputeDispatcher with wave-front per-lane
- CPU reference + GPU production (tick() vs tick_gpu())
- Fixed-point arithmetic (Q16.16 position, Q12.20 speed)
- Binary serialization with postcard (Phase 5 cleaned graph) -- reuse for CCH cache
- Background async via tokio::spawn (prediction update runs async)
- Demand-config-driven assignment (Phase 5/6 car-following model assignment pattern)

### Integration Points
- velos-net: CCH implementation lives here alongside RoadGraph and spatial index
- velos-predict: new crate for prediction ensemble (BPR, ETS, historical, overlay)
- velos-demand: profile assignment at spawn time
- velos-gpu/shaders: new perception.wgsl kernel
- GpuAgentState flags field: encode profile_id (4 bits)
- Frame pipeline: perception pass added between existing car-following and route advance steps

</code_context>

<specifics>
## Specific Ideas

- CCH is pure Rust to maintain all-Rust codebase -- no C++ FFI despite existing C++ implementations
- Conservative rerouting matches real HCMC driver behavior -- motorbike riders know their routes, only switch when obviously better
- Distance-weighted prediction blend is the key differentiator: agents anticipate congestion on faraway edges while trusting current data nearby
- Adaptive prediction weights let the ensemble self-correct over long simulations without manual tuning
- Profile differences should be visible in route choices: Commuter takes fastest route, Tourist takes scenic/comfortable route through same OD pair (Success Criterion 1)

</specifics>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 07-intelligence-routing-prediction*
*Context gathered: 2026-03-07*
