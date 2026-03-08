# Phase 5: Foundation & GPU Engine - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Simulation runs entirely on GPU at 280K-agent scale across multiple GPUs on a cleaned 5-district HCMC road network, with SUMO file compatibility and dual car-following model support (Krauss + IDM). God crate decomposition, GPU physics cutover, multi-GPU wave-front dispatch, fixed-point arithmetic.

Requirements: GPU-01 through GPU-06, NET-01 through NET-06, CFM-01, CFM-02.

</domain>

<decisions>
## Implementation Decisions

### SUMO File Compatibility
- Primary goal is migration path -- users bring existing SUMO models and get a working simulation
- .net.xml import covers: edges, lanes, junctions, connections, roundabouts, traffic light programs (tlLogic)
- .rou.xml import is full-featured: trips, flows, vehicles, persons, vType distributions, calibrator elements
- Unmapped SUMO attributes use best-effort mapping to VELOS equivalents with logged warnings -- never silently drop attributes

### Krauss Car-Following Model
- Use SUMO-faithful defaults: sigma=0.5 (moderate random deceleration, phantom jams, hesitant green-light response)
- Agents are color-coded by car-following model in the egui dashboard (distinct hue for Krauss vs IDM)
- Runtime model switching via per-agent ECS component tag (CarFollowingModel enum) -- GPU shader branches on tag
- No hardcoded default model per vehicle type -- demand configuration specifies which car-following model each vehicle type uses (maximum flexibility)

### HCMC Network Cleaning
- Aggressive cleaning: merge short edges <5m, remove disconnected components, infer lane counts from road class, fix topology errors
- Manual override file (JSON/TOML) for correcting specific edges/junctions where OSM is wrong
- Motorbike-only lane detection: OSM tags + road class heuristic (alleys <4m wide = motorbike-only, narrow service/residential roads)
- Time-dependent one-way edges: support time-of-day directional changes in the graph (edges have time windows per direction)
- Cleaned graph serialized to binary format (bincode) for fast reload; re-import from OSM on demand

### GPU Cutover Validation
- Parallel CPU+GPU run during validation period, comparing aggregate metrics (average speed, throughput, congestion patterns) -- not per-agent position matching
- Behavioral equivalence is the bar: same traffic patterns, not identical floating-point values
- After validation: delete CPU physics from production sim loop, keep CPU model implementations in test modules as reference for validating future GPU shader changes
- Multi-GPU validated via simulated partitions on single GPU (2-4 logical partitions with own buffers, real inbox/outbox boundary agent protocol)

### Claude's Discretion
- God crate decomposition strategy (how to split velos-gpu into focused crates)
- Wave-front dispatch implementation details
- Fixed-point arithmetic precision trade-offs and @invariant fallback
- WGSL shader architecture for multi-model branching
- Bincode serialization schema for cleaned graph
- Override file format specifics (JSON vs TOML)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ComputeDispatcher` (velos-gpu/src/compute.rs): GPU pipeline + dispatch proven but not wired to sim loop -- cutover starts here
- `SimWorld` (velos-gpu/src/sim.rs): CPU sim loop with step_vehicles, step_motorbikes_sublane, step_pedestrians -- reference implementation for GPU validation
- ECS components (velos-core/src/components.rs): Position, Kinematics, RoadPosition, Route, LateralOffset, WaitState, LaneChangeState -- GPU buffer layout basis
- `osm_import` (velos-net/src/osm_import.rs): District 1 OSM import -- extend for 5-district with cleaning pipeline
- `agent_update.wgsl`: Existing GPU physics shader -- extend with Krauss model branch
- IDM model (velos-vehicle/src/idm.rs): CPU IDM implementation -- test reference for GPU IDM shader

### Established Patterns
- hecs ECS with SoA component layout for GPU buffer mapping
- wgpu compute pipeline with bind group layout (ComputeDispatcher pattern)
- GPU-instanced rendering with styled agent shapes (agent_render.wgsl)
- f64 CPU / f32 GPU arithmetic (no fixed-point yet -- Phase 5 adds Q16.16/Q12.20/Q8.8)

### Integration Points
- SimWorld::tick() must be rewired from CPU step_* methods to GPU dispatch
- VehicleType enum needs CarFollowingModel component added
- Road graph needs expansion from single-district to multi-district with cleaning pipeline
- New SUMO import module needed alongside existing OSM import in velos-net

</code_context>

<specifics>
## Specific Ideas

- SUMO import should feel like a migration tool -- existing SUMO users bring their files and VELOS "just works" with warnings for unsupported features
- Krauss visual distinctness matters for demonstrations -- color coding makes model comparison immediately visible
- Time-dependent one-way streets are important for HCMC realism (many streets reverse direction during peak hours)
- Override file enables community corrections to OSM data without waiting for upstream OSM edits

</specifics>

<deferred>
## Deferred Ideas

None -- discussion stayed within phase scope

</deferred>

---

*Phase: 05-foundation-gpu-engine*
*Context gathered: 2026-03-07*
