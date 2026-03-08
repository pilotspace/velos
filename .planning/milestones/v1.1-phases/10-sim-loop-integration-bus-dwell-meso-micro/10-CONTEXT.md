# Phase 10: Sim Loop Integration — Bus Dwell & Meso-Micro Hybrid - Context

**Gathered:** 2026-03-08
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire bus dwell mechanics and meso-micro hybrid zones into the running simulation loop. After this phase:
1. Bus agents stop at designated BusStop locations during tick_gpu(), accumulate dwell time via the empirical model (5s + 0.5s/boarding + 0.67s/alighting), and resume driving when dwell completes.
2. Peripheral network edges configured as meso zones run the O(1) SpatialQueue model instead of full microscopic physics.
3. Agents crossing meso-micro boundaries pass through the 100m graduated buffer zone with velocity-matching insertion — no speed discontinuities.

Requirements: AGT-01, AGT-05, AGT-06
Gap Closure: M-7 (bus dwell not called in sim loop), M-8 (velos-meso not imported/active)

</domain>

<decisions>
## Implementation Decisions

### Bus Dwell Pipeline Placement
- Bus dwell logic (begin_dwell/tick_dwell) runs on CPU, not GPU — dwell is a state machine with branching (check proximity, manage stop index, count passengers) that doesn't benefit from GPU parallelism
- Execute bus dwell step AFTER vehicle physics in tick_gpu() — buses move via normal IDM first, then CPU checks if any bus is near a BusStop and triggers dwell
- Dwelling buses set FLAG_BUS_DWELLING in GpuAgentState.flags — GPU physics reads this flag to hold the bus at zero speed (no IDM computation needed while dwelling)
- Following vehicles see a dwelling bus as a stopped leader — normal IDM deceleration handles queuing behind stopped bus

### Passenger Count Sourcing
- Stochastic passenger counts per stop using existing SimWorld RNG
- Boarding: Poisson-distributed with mean proportional to stop capacity (BusStop.capacity)
- Alighting: fraction of onboard passengers (e.g., 20% per stop) with minimum 0
- No GTFS-derived demand this phase — stochastic is sufficient for engine proof

### Meso Zone Activation Strategy
- Meso activation controlled by SimConfig::meso_enabled (default false, per Phase 6 decision)
- When enabled, ZoneConfig loaded from TOML at startup (like signal_config.toml pattern from Phase 9)
- Missing zone_config.toml: all edges default to Micro, log warning — safe default per Phase 9 graceful degradation pattern
- Meso edges skip GPU dispatch entirely — SpatialQueue.enter()/try_exit() called on CPU during tick_gpu()
- Meso step runs BEFORE micro vehicle physics — meso agents ready for buffer zone insertion before micro physics executes

### Buffer Zone Insertion Mechanics
- Agents exiting meso queue at buffer zone entry get spawned into micro simulation at the buffer zone start position
- Lane assignment: rightmost available lane on the micro edge (simple, avoids complex lane selection)
- Speed set via velocity_matching_speed() from velos-meso — matches current lane speed to avoid discontinuity
- BufferZone::params_at() provides smoothstep-interpolated IDM params over the 100m zone — agents gradually tighten following behavior
- Agents entering meso from micro: removed from micro simulation when they cross into a meso-designated edge, inserted into SpatialQueue

### Claude's Discretion
- Exact SimWorld field additions for meso state (Vec<SpatialQueue>, ZoneConfig storage)
- Bus stop detection mechanism (linear scan of stops per bus vs spatial index)
- Meso step ordering relative to other pipeline stages
- Buffer zone spawn implementation details (ECS entity creation, GPU buffer upload timing)
- Test strategy for meso-micro transitions (unit tests vs integration tests)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `BusState` (velos-vehicle/src/bus.rs): Full state machine with should_stop(), begin_dwell(), tick_dwell(), is_dwelling() — ready to wire
- `BusDwellModel` (velos-vehicle/src/bus.rs): compute_dwell(boarding, alighting) with empirical formula — ready to use
- `BusStop` (velos-vehicle/src/bus.rs): edge_id, offset_m, capacity, name — ECS component ready
- `SpatialQueue` (velos-meso/src/queue_model.rs): BPR-based queue with enter(), try_exit(), travel_time() — ready to wire
- `BufferZone` (velos-meso/src/buffer_zone.rs): smoothstep interpolation, should_insert(), velocity_matching_speed() — ready to wire
- `ZoneConfig` (velos-meso/src/zone_config.rs): load_from_toml(), from_centroid_distance(), zone_type() lookup — ready to use
- `SimWorld` (velos-gpu/src/sim.rs): tick_gpu() orchestrates full pipeline — integration point for bus dwell and meso steps

### Established Patterns
- CPU state machine + GPU flag pattern: CPU manages complex logic, sets flags in GpuAgentState for GPU to read (like signal priority, emergency yield)
- Config loading: hardcoded TOML path with env override and safe defaults (vehicle_params.toml, signal_config.toml)
- Pipeline ordering: spawn → signals → perception → reroute → GPU vehicles → CPU pedestrians → gridlock → cleanup
- Graceful degradation: missing config = safe defaults + log warning, never crash

### Integration Points
- `SimWorld::tick_gpu()`: Needs bus dwell step after vehicle physics, meso step before vehicle physics
- `SimWorld::new()`: Needs ZoneConfig loading, SpatialQueue initialization for meso edges, BusStop loading
- `GpuAgentState.flags`: FLAG_BUS_DWELLING bit needed for GPU to skip physics on dwelling buses
- `velos-meso` crate: Currently standalone — needs to become dependency of velos-gpu (or velos-core)
- `Cargo.toml`: velos-gpu needs velos-meso dependency added

</code_context>

<specifics>
## Specific Ideas

- Bus dwell is fundamentally a CPU state machine — GPU just needs a "this bus is stopped" flag, no complex GPU branching for dwell logic
- Meso-micro is the last gap closure item — keep implementation minimal and correct, not over-engineered
- Stochastic passengers are sufficient for engine proof — deterministic GTFS-derived demand is a v2 concern
- The existing BusState, SpatialQueue, and BufferZone modules are fully implemented with tests — this phase is pure integration/wiring, not model development

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 10-sim-loop-integration-bus-dwell-meso-micro*
*Context gathered: 2026-03-08*
