# Phase 6: Agent Models & Signal Control - Context

**Gathered:** 2026-03-07
**Status:** Ready for planning

<domain>
## Phase Boundary

Every vehicle and pedestrian type operates at GPU scale with realistic behavior, signals respond to traffic demand, and agents interact with V2I infrastructure. This phase adds bus, bicycle, truck, and emergency agent types, optimizes pedestrian GPU dispatch with adaptive workgroups, implements meso-micro hybrid zones, upgrades signals from fixed-time to actuated/adaptive with V2I communication, and adds traffic sign interaction.

Requirements: AGT-01 through AGT-08, SIG-01 through SIG-05.

</domain>

<decisions>
## Implementation Decisions

### Agent Type Architecture
- Extend `VehicleType` enum to: `Motorbike`, `Car`, `Bus`, `Bicycle`, `Truck`, `Emergency`, `Pedestrian`
- Add `vehicle_type: u32` field to `GpuAgentState` for GPU-side branching on agent-type-specific physics (bus dwell, truck accel limits, emergency priority flag)
- Existing `cf_model` tag stays separate — it handles IDM vs Krauss; `vehicle_type` handles type-specific behavior
- Demand-config-driven car-following model assignment (deferred from Phase 5): each VehicleType maps to a default CarFollowingModel in demand config, overridable per-agent

### Bus Agents (AGT-01, AGT-02)
- Empirical dwell model: `5s + 0.5s/boarding + 0.67s/alighting`, capped at 60s
- Bus stops as ECS components attached to edges (position + capacity)
- GTFS import lives in velos-demand (route/schedule data, not network topology)
- Buses use IDM car-following with lower desired speed matching route schedule
- Bus stop pull-over: bus decelerates into rightmost lane position near stop, blocks that lane during dwell

### Bicycle Agents (AGT-03)
- Reuse sublane model with rightmost lateral preference and IDM at v0=15km/h
- No separate crate — bicycle params added to velos-vehicle alongside motorbike sublane
- Bicycles filter through traffic like motorbikes but with narrower width (0.6m vs 0.8m) and lower speed

### Truck Agents (AGT-07)
- Distinct dynamics: 12m length, 1.0 m/s^2 max accel, 90 km/h max speed
- IDM car-following with truck-specific params (longer safe gap s0=4m, larger time headway T=2.0s)
- Lane restriction: trucks stay in rightmost lanes on multi-lane roads

### Emergency Vehicles (AGT-08)
- Emergency type encoded in `vehicle_type` field of GpuAgentState
- Yield behavior: surrounding agents within 50m cone ahead shift to rightmost position and slow down
- Signal priority: emergency vehicle approaching intersection triggers green on its approach
- Emergency vehicles ignore red signals but decelerate through intersections

### Pedestrian Adaptive Workgroups (AGT-04)
- Social force model stays in velos-vehicle (no separate velos-pedestrian crate for now)
- Adaptive workgroup sizing on GPU: spatial hash cells sized by density (2m dense, 5m medium, 10m sparse)
- Prefix-sum compaction to skip empty cells
- Target: 3-8x speedup over uniform dispatch for sparse pedestrian areas

### Meso-Micro Hybrid (AGT-05, AGT-06)
- Implement with runtime toggle (`SimConfig::meso_enabled: bool` — default false)
- Peripheral network zones run O(1) queue model; core zones remain full micro
- Zone designation: static config file marking edges as meso vs micro (distance from core area centroid)
- 100m graduated buffer zone at meso-micro boundary
- Buffer zone: IDM params interpolated from relaxed (meso side) to normal (micro side) over 100m
- Velocity-matching insertion: agents entering micro zone match lane speed
- New `velos-meso` crate for queue model and buffer zone logic

### Actuated Signals (SIG-01)
- `ActuatedController` alongside existing `FixedTimeController` in velos-signal
- Loop detectors as point sensors on approach lanes (count agents crossing the point)
- Phase extension: green extends up to max_green if detector sees vehicles; gaps out after 3s default gap threshold
- Min green: 7s, max green: 60s (HCMC-appropriate defaults)

### Adaptive Signals (SIG-02)
- Simple demand-responsive: redistribute green time proportional to queue length on each approach
- Update timing every cycle (not real-time within cycle)
- No SCOOT/SCATS-level optimization — proportional queue-based redistribution captures the essence for SUMO replacement proof

### SPaT Broadcast (SIG-03)
- Signal Phase and Timing broadcast to agents within 200m of intersection
- Agents use SPaT for approach speed adjustment (GLOSA — Green Light Optimal Speed Advisory)
- Each signal stores current phase + time-to-next-change; agents query during GPU perception phase

### Signal Priority (SIG-04)
- Bus/emergency vehicles within 100m of intersection can request priority
- Priority extends current green (if approach is green) or shortens conflicting green (if red)
- Emergency > bus priority; at most 1 priority request per cycle to prevent starvation

### Traffic Signs (SIG-05)
- Sign types: speed limit, stop, yield, no-turn restriction, school zone
- Speed limit: agent reduces v0 to posted limit within 50m of sign
- Stop sign: full stop, wait gap_accept_time (2s default), then proceed
- Yield sign: reduce speed, stop only if conflicting traffic
- No-turn: infinite cost in pathfinding + runtime enforcement
- School zone: reduced speed limit (20 km/h) active during configured time windows
- Signs as ECS components on edges (sign_type, value, position_on_edge, time_window)
- GPU shader reads sign buffer alongside agent state
- Dual enforcement: pathfinding cost + runtime behavior

### Claude's Discretion
- GPU shader architecture for multi-agent-type branching (how to organize WGSL code)
- Prefix-sum compaction implementation details for pedestrian workgroups
- Queue model internals for meso zones (BPR-based or simple FIFO)
- Exact GTFS parsing strategy (full spec vs minimal route/stop/schedule)
- Loop detector implementation (virtual point sensor vs zone-based)
- Buffer zone IDM interpolation curve (linear vs smooth step)
- GpuAgentState struct packing for new fields (maintain 32-byte alignment or expand)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `FixedTimeController` (velos-signal/src/controller.rs): Fixed-time signals working with tick/get_phase_state/reset — actuated extends this pattern
- `VehicleType` enum (velos-core/src/components.rs, velos-vehicle/src/types.rs): Currently Motorbike/Car/Pedestrian — needs new variants in both locations
- `CarFollowingModel` enum (velos-core/src/components.rs): IDM/Krauss with GPU cf_model tag — already dispatched in GPU shader
- `GpuAgentState` (velos-core/src/components.rs): 32-byte packed struct with cf_model and rng_state — needs vehicle_type field
- `social_force` module (velos-vehicle/src/social_force.rs): Pedestrian model exists — adaptive workgroups optimize GPU dispatch
- `sublane` module (velos-vehicle/src/sublane.rs): Continuous lateral positioning — bicycles reuse this with different params
- `krauss`/`idm` modules (velos-vehicle): CPU reference implementations for validation
- `spawner`/`tod_profile`/`od_matrix` (velos-demand): Spawning infrastructure exists — extend for GTFS bus routes
- `default_idm_params`/`default_mobil_params` (velos-vehicle/src/types.rs): Parameter factory pattern — extend for truck/bus/bicycle profiles

### Established Patterns
- GPU compute dispatch via ComputeDispatcher with wave-front per-lane
- ECS SoA component layout with bytemuck Pod for GPU buffer mapping
- Fixed-point arithmetic (Q16.16 position, Q12.20 speed, Q8.8 lateral)
- CPU reference + GPU production (tick() vs tick_gpu())
- Signal plan with phases, cycle time, amber handling

### Integration Points
- `VehicleType` enum needs sync between velos-core and velos-vehicle (both have copies)
- `GpuAgentState` struct expansion requires updating all GPU buffer creation, upload, and download code
- velos-signal needs trait or enum dispatch for FixedTime vs Actuated vs Adaptive controllers
- velos-demand spawner needs GTFS route/schedule data source
- agent_update.wgsl shader needs new branches for bus dwell, emergency yield, sign interaction
- Sign data needs a new GPU buffer bound alongside agent state buffer

</code_context>

<specifics>
## Specific Ideas

- Bicycles reuse the sublane model — they're a parameter variant of motorbikes with narrower width and lower speed, not a fundamentally different model
- Meso-micro implemented with runtime toggle — if benchmarks show full-micro handles 280K comfortably, meso is disabled by default but available for future scaling
- Adaptive signals use simple proportional queue redistribution, not SCOOT/SCATS — this is a simulation engine proof, not a signal optimization platform
- Emergency yield uses 50m cone (practical for HCMC narrow urban streets)
- GTFS import lives in velos-demand (not velos-net) because it's demand/schedule data
- Dual sign enforcement (pathfinding + runtime) prevents both route selection and in-simulation violations of restrictions

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 06-agent-models-signal-control*
*Context gathered: 2026-03-07*
