# Phase 10: Sim Loop Integration -- Bus Dwell & Meso-Micro Hybrid - Research

**Researched:** 2026-03-08
**Domain:** Simulation loop wiring -- bus dwell state machine + mesoscopic hybrid zone integration
**Confidence:** HIGH

## Summary

Phase 10 is a **pure integration/wiring phase**. All domain models already exist with tests: BusState (bus.rs), SpatialQueue (queue_model.rs), BufferZone (buffer_zone.rs), and ZoneConfig (zone_config.rs). The work is connecting these models into the SimWorld tick pipeline and adding the velos-meso crate as a dependency of velos-gpu.

Three integration surfaces exist: (1) bus dwell as a CPU step after GPU vehicle physics that sets FLAG_BUS_DWELLING in the GPU agent state, (2) meso zone stepping as a CPU step before GPU vehicle physics that runs SpatialQueue.enter()/try_exit(), and (3) buffer zone insertion that spawns micro agents from meso exits with velocity-matched speeds.

The GPU shader (wave_front.wgsl) already defines FLAG_BUS_DWELLING = 1u but never reads it. The shader needs a guard clause to skip physics for dwelling buses (set speed to 0, skip IDM). This is a ~5-line WGSL change following the existing FLAG_YIELDING pattern.

**Primary recommendation:** Wire in three steps -- (1) bus dwell CPU step + GPU flag handling, (2) meso zone activation with SpatialQueue, (3) buffer zone meso-micro transitions. Each is independently testable.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Bus dwell logic (begin_dwell/tick_dwell) runs on CPU, not GPU -- dwell is a state machine with branching that doesn't benefit from GPU parallelism
- Execute bus dwell step AFTER vehicle physics in tick_gpu() -- buses move via normal IDM first, then CPU checks if any bus is near a BusStop and triggers dwell
- Dwelling buses set FLAG_BUS_DWELLING in GpuAgentState.flags -- GPU physics reads this flag to hold the bus at zero speed
- Following vehicles see a dwelling bus as a stopped leader -- normal IDM deceleration handles queuing behind stopped bus
- Stochastic passenger counts per stop using existing SimWorld RNG (Poisson boarding, fractional alighting)
- No GTFS-derived demand this phase -- stochastic is sufficient for engine proof
- Meso activation controlled by SimConfig::meso_enabled (default false)
- When enabled, ZoneConfig loaded from TOML at startup (like signal_config.toml pattern from Phase 9)
- Missing zone_config.toml: all edges default to Micro, log warning
- Meso edges skip GPU dispatch entirely -- SpatialQueue.enter()/try_exit() called on CPU
- Meso step runs BEFORE micro vehicle physics
- Lane assignment for meso exit: rightmost available lane on the micro edge
- Speed set via velocity_matching_speed() from velos-meso
- BufferZone::params_at() provides smoothstep-interpolated IDM params over 100m zone
- Agents entering meso from micro: removed from micro simulation when crossing into meso edge, inserted into SpatialQueue

### Claude's Discretion
- Exact SimWorld field additions for meso state (Vec<SpatialQueue>, ZoneConfig storage)
- Bus stop detection mechanism (linear scan of stops per bus vs spatial index)
- Meso step ordering relative to other pipeline stages
- Buffer zone spawn implementation details (ECS entity creation, GPU buffer upload timing)
- Test strategy for meso-micro transitions (unit tests vs integration tests)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| AGT-01 | Bus agents with empirical dwell time model (5s + 0.5s/boarding + 0.67s/alighting, cap 60s) | BusDwellModel and BusState fully implemented in velos-vehicle/src/bus.rs; needs wiring into tick_gpu() pipeline and FLAG_BUS_DWELLING GPU handling |
| AGT-05 | Meso-micro hybrid with 100m graduated buffer zone and velocity-matching insertion | BufferZone in velos-meso/src/buffer_zone.rs ready; needs integration into SimWorld with spawn/despawn logic at zone boundaries |
| AGT-06 | Mesoscopic queue model (O(1) per edge) for peripheral network zones | SpatialQueue in velos-meso/src/queue_model.rs ready; needs activation in tick_gpu() pipeline controlled by ZoneConfig |
</phase_requirements>

## Standard Stack

### Core (already in workspace)
| Library | Version | Purpose | Status |
|---------|---------|---------|--------|
| velos-vehicle | workspace | BusState, BusDwellModel, BusStop | Implemented, tested |
| velos-meso | workspace | SpatialQueue, BufferZone, ZoneConfig | Implemented, tested, NOT yet a dependency of velos-gpu |
| velos-gpu | workspace | SimWorld, tick_gpu(), wave_front shader | Integration target |
| hecs | workspace | ECS world (entity spawn/despawn for zone transitions) | In use |
| rand | workspace | StdRng for stochastic passenger counts | In use (SimWorld.rng) |

### Supporting (no new dependencies needed)
| Library | Purpose | Notes |
|---------|---------|-------|
| toml | ZoneConfig TOML parsing | Already a dependency of velos-meso |
| serde | ZoneConfig deserialization | Already a dependency of velos-meso |

### New Dependency Required
| Change | From | To |
|--------|------|-----|
| Add `velos-meso` to velos-gpu/Cargo.toml | Not present | `velos-meso = { path = "../velos-meso" }` |

**Installation:**
```toml
# In crates/velos-gpu/Cargo.toml [dependencies]
velos-meso = { path = "../velos-meso" }
```

## Architecture Patterns

### Integration Points in tick_gpu() Pipeline

Current 10-step pipeline with insertion points for Phase 10:

```
tick_gpu():
  1. spawn_agents           (existing)
  2. update_loop_detectors  (existing)
  3. step_signals            (existing)
  4. step_signal_priority    (existing)
  5. step_perception         (existing)
  6. step_reroute            (existing)
  NEW: step_meso()           -- CPU meso queue tick, buffer zone insertion
  7. step_vehicles_gpu       (existing -- modified to skip meso edges)
  NEW: step_bus_dwell()      -- CPU bus dwell state machine, set/clear FLAGS
  8. step_pedestrians        (existing)
  9. detect_gridlock         (existing)
  10. remove + metrics       (existing)
```

### Pattern 1: CPU State Machine + GPU Flag (Established)
**What:** CPU manages complex state logic, sets a flag bit in GpuAgentState.flags that GPU physics reads to modify behavior.
**When to use:** Bus dwelling (FLAG_BUS_DWELLING = bit 0).
**Existing precedent:** Emergency yield (FLAG_YIELDING = bit 2) -- CPU detects yield cone, GPU limits speed.

```rust
// In step_bus_dwell() -- CPU side:
// 1. Query buses with BusState component
// 2. For each bus: check should_stop(), begin_dwell(), tick_dwell()
// 3. If dwelling: set flags |= FLAG_BUS_DWELLING on next GPU upload
// 4. If dwell complete: clear flags &= !FLAG_BUS_DWELLING
```

```wgsl
// In wave_front.wgsl -- GPU side (add after line ~106):
// Early return for dwelling buses -- skip IDM computation
if (agent.vehicle_type == VT_BUS && (agent.flags & FLAG_BUS_DWELLING) != 0u) {
    // Bus is dwelling at stop -- hold at zero speed
    agent.speed = 0;
    agent.acceleration = 0;
    agents[sorted_idx] = agent;
    return;  // or continue to next agent in wave-front
}
```

### Pattern 2: Config-Gated Feature Activation (Established)
**What:** Feature controlled by config flag (default off), TOML config loaded at startup with graceful degradation.
**When to use:** Meso zone activation (SimConfig::meso_enabled).
**Existing precedent:** Signal config loading in Phase 9 (signal_config.toml with safe defaults on missing file).

```rust
// In SimWorld::new() -- startup:
let zone_config = if meso_enabled {
    match velos_meso::zone_config::ZoneConfig::load_from_toml(&zone_config_path) {
        Ok(config) => config,
        Err(_) => {
            log::warn!("zone_config.toml not found, defaulting all edges to Micro");
            ZoneConfig::default()  // all edges = Micro (safe default)
        }
    }
} else {
    ZoneConfig::default()
};
```

### Pattern 3: Meso-Micro Zone Boundary Crossing
**What:** Agents transition between mesoscopic (CPU queue) and microscopic (GPU physics) simulation at zone boundaries.
**When to use:** Agents crossing between ZoneType::Meso and ZoneType::Micro edges.

```rust
// Micro-to-Meso: Agent advances to a meso edge
// 1. Detect edge transition in apply_vehicle_update() / advance_to_next_edge()
// 2. Check zone_config.zone_type(next_edge_id)
// 3. If Meso: despawn from ECS world, insert into SpatialQueue
//    MesoVehicle { vehicle_id: entity.id(), entry_time: sim_time, exit_edge: next_route_edge }

// Meso-to-Micro: SpatialQueue.try_exit() succeeds
// 1. In step_meso(), call try_exit() on each SpatialQueue
// 2. Exiting vehicle: spawn new ECS entity at buffer zone start
// 3. Lane = rightmost available (lane 0)
// 4. Speed = velocity_matching_speed(meso_exit_speed, last_micro_vehicle_speed)
// 5. Agent traverses 100m buffer with smoothstep-interpolated IDM params
```

### Recommended SimWorld Field Additions

```rust
pub struct SimWorld {
    // ... existing fields ...

    /// Bus stops indexed by edge for O(1) lookup per bus.
    pub(crate) bus_stops: Vec<BusStop>,

    /// Mesoscopic spatial queues indexed by edge ID.
    /// Only populated when meso_enabled = true.
    pub(crate) meso_queues: HashMap<u32, SpatialQueue>,

    /// Zone configuration mapping edges to Micro/Meso/Buffer.
    pub(crate) zone_config: ZoneConfig,

    /// Whether mesoscopic hybrid mode is active.
    pub(crate) meso_enabled: bool,

    /// Bus dwell model parameters.
    pub(crate) bus_dwell_model: BusDwellModel,
}
```

### Anti-Patterns to Avoid
- **GPU-side dwell logic:** Do NOT implement bus dwell state machine in WGSL. The branching, RNG, and stop-index tracking are CPU-appropriate. GPU only needs the flag.
- **Meso agents in GPU buffers:** Do NOT upload meso-zone agents to GPU buffers. They run purely on CPU via SpatialQueue. Only micro-zone agents go through GPU dispatch.
- **Synchronous zone transitions:** Do NOT spawn micro agents in the middle of GPU dispatch. Spawn them BEFORE step_vehicles_gpu() so they participate in the same frame's physics.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| BPR travel time | Custom queue model | `SpatialQueue::travel_time()` | Already handles beta=4.0 fast-path, V/C ratio |
| Bus dwell computation | Inline formula | `BusDwellModel::compute_dwell()` | Handles cap at max_dwell_s, tested |
| IDM interpolation in buffer | Manual lerp | `BufferZone::params_at()` | C1-continuous smoothstep, tested |
| Velocity matching | `min()` inline | `velocity_matching_speed()` | Documents intent, tested |
| Zone classification | Ad-hoc edge checks | `ZoneConfig::zone_type()` | HashMap lookup, TOML-configurable, centroid-based auto-designation |
| Stop proximity check | Distance calculation | `BusState::should_stop()` | Handles edge_id matching + 5m threshold + route progression |

**Key insight:** Every model function is already implemented and unit-tested. This phase is 100% integration wiring. No new model code needed.

## Common Pitfalls

### Pitfall 1: Bus Dwelling Flag Not Persisted Across Frames
**What goes wrong:** FLAG_BUS_DWELLING is set in the flags field when uploading to GPU, but the GPU readback doesn't preserve it (flags are overwritten or reset each frame).
**Why it happens:** step_vehicles_gpu() constructs GpuAgentState fresh each frame with `flags: 0`. The dwelling flag must be set before upload.
**How to avoid:** Store dwelling state in a BusState ECS component. In step_vehicles_gpu(), check BusState.is_dwelling() and set `flags |= 1` before GPU upload. The GPU output flags are irrelevant for dwelling -- it's CPU-authoritative.
**Warning signs:** Buses stop for one frame then resume, or never stop at all.

### Pitfall 2: Meso Agent Identity Loss During Zone Transition
**What goes wrong:** When an agent transitions micro-to-meso, it's despawned from ECS. When it exits meso, a new entity is spawned. The vehicle_id/route continuity is lost.
**Why it happens:** hecs Entity IDs are not reusable after despawn.
**How to avoid:** Store full agent state (Route, VehicleType, IdmParams, etc.) in MesoVehicle or a separate MesoAgentState struct. On meso exit, reconstruct the ECS entity with the preserved state. Use vehicle_id (u32) as the stable identity, not Entity.
**Warning signs:** Agents lose their route after crossing a meso zone, or agent counts don't balance.

### Pitfall 3: Double-Stepping Agents at Zone Boundaries
**What goes wrong:** An agent that transitions from micro to meso in the same frame gets both micro physics AND meso queue entry, or a meso-exiting agent gets both queue exit AND micro physics.
**Why it happens:** Pipeline ordering is wrong -- meso step and micro step both process the same agent.
**How to avoid:** Meso step runs BEFORE micro physics (per CONTEXT.md decision). Meso exits spawn agents that participate in micro physics in the SAME frame. Micro-to-meso transitions happen in advance_to_next_edge() and should immediately remove the agent from the micro population before GPU upload.
**Warning signs:** Agents appear at two positions simultaneously, or conservation-of-vehicles is violated.

### Pitfall 4: Meso Queue Starvation on Congested Micro Edge
**What goes wrong:** velocity_matching_speed() returns a reasonable speed, but the micro edge has no gap for insertion. Agents pile up in the meso queue indefinitely.
**Why it happens:** BufferZone::should_insert() checks distance and speed diff, but doesn't check physical gap availability.
**How to avoid:** Before spawning, verify there's sufficient gap at the buffer zone entry point (edge start). If no gap exists, hold the vehicle in the meso queue for one more timestep. Log a warning if held > 30s to detect systemic issues.
**Warning signs:** Meso queue lengths grow unboundedly while micro edges are gridlocked.

### Pitfall 5: BusState Component Not Attached at Spawn
**What goes wrong:** Bus agents are spawned in sim_lifecycle.rs but never get a BusState component, so step_bus_dwell() finds no buses to process.
**Why it happens:** spawn_single_agent() currently doesn't distinguish bus from other lane-based vehicles for component attachment.
**How to avoid:** In spawn_single_agent(), when vtype == Bus, also attach BusState with the bus's route stops. Requires mapping route nodes to BusStop indices.
**Warning signs:** step_bus_dwell() is called but no buses ever dwell.

## Code Examples

### Bus Dwell Step (CPU)

```rust
// Source: Pattern derived from existing step_signal_priority() in sim.rs
fn step_bus_dwell(&mut self, dt: f64) {
    let mut dwell_changes: Vec<(Entity, bool)> = Vec::new();

    for (entity, bus_state, rp) in self
        .world
        .query_mut::<(Entity, &mut BusState, &RoadPosition)>()
    {
        if bus_state.is_dwelling() {
            let completed = bus_state.tick_dwell(dt);
            if completed {
                dwell_changes.push((entity, false)); // clear FLAG
            }
        } else if bus_state.should_stop(rp.edge_index, rp.offset_m, &self.bus_stops) {
            // Stochastic passenger counts
            let capacity = self.bus_stops[bus_state.current_stop_index()]
                .capacity;
            let boarding = poisson_sample(&mut self.rng, capacity as f64 * 0.3);
            let alighting = /* fraction of onboard */ 0;
            bus_state.begin_dwell(&self.bus_dwell_model, boarding, alighting);
            dwell_changes.push((entity, true)); // set FLAG
        }
    }
    // Flag changes applied in next GPU upload (step_vehicles_gpu reads BusState)
}
```

### GPU Dwelling Guard (WGSL)

```wgsl
// Source: Pattern from FLAG_YIELDING handling in wave_front.wgsl:510
// Add early in the per-agent processing loop, before IDM computation:
if (agent.vehicle_type == VT_BUS && (agent.flags & FLAG_BUS_DWELLING) != 0u) {
    agent.speed = 0;
    agent.acceleration = 0;
    agents[sorted_idx] = agent;
    // Skip IDM -- bus is held at stop
    // Following vehicles see speed=0 via normal leader detection
    continue;  // or return depending on loop structure
}
```

### Meso Step (CPU)

```rust
// Source: Pattern derived from existing pipeline steps
fn step_meso(&mut self, dt: f64) {
    if !self.meso_enabled {
        return;
    }

    // 1. Try to exit vehicles from meso queues
    let mut exits: Vec<(u32, MesoVehicle)> = Vec::new();
    for (edge_id, queue) in &mut self.meso_queues {
        while let Some(vehicle) = queue.try_exit(self.sim_time) {
            exits.push((*edge_id, vehicle));
        }
    }

    // 2. Spawn exiting vehicles into micro zone at buffer entry
    for (meso_edge_id, vehicle) in exits {
        self.spawn_from_meso(vehicle);
    }
}
```

### Meso-to-Micro Spawn

```rust
fn spawn_from_meso(&mut self, vehicle: MesoVehicle) {
    let exit_edge = vehicle.exit_edge;
    let zone = self.zone_config.zone_type(exit_edge);

    // Determine insertion speed via velocity matching
    let last_micro_speed = self.last_vehicle_speed_on_edge(exit_edge);
    let meso_exit_speed = /* derived from queue travel time */ 10.0;
    let insert_speed = velos_meso::buffer_zone::velocity_matching_speed(
        meso_exit_speed, last_micro_speed
    );

    // Spawn ECS entity with preserved state
    self.world.spawn((
        /* Position, Kinematics, RoadPosition, Route, etc. from stored MesoAgentState */
    ));
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| All agents on GPU | Hybrid meso-micro: peripheral on CPU queue | Phase 6 (model), Phase 10 (wiring) | Reduces GPU load for peripheral zones |
| Bus dwell as GPU computation | CPU state machine + GPU flag | Phase 6 (model), Phase 10 (wiring) | Simpler, no GPU branching overhead |

**No deprecated/outdated concerns** -- all models are freshly implemented in previous phases.

## Open Questions

1. **BusState attachment at spawn time**
   - What we know: spawn_single_agent() in sim_lifecycle.rs doesn't create BusState for buses
   - What's unclear: How to map a bus's route (Vec<NodeIndex>) to BusStop indices -- need to know which stops are on the route
   - Recommendation: Linear scan of bus_stops Vec matching edge_ids in route path. With ~130 routes and ~2000 stops (HCMC), this is acceptable at spawn time.

2. **MesoAgentState preservation during zone transition**
   - What we know: MesoVehicle only stores vehicle_id, entry_time, exit_edge -- not enough to reconstruct full ECS entity
   - What's unclear: Exact struct layout for preserved agent state
   - Recommendation: Create MesoAgentState struct holding Route, VehicleType, IdmParams, CarFollowingModel. Store in a HashMap<u32, MesoAgentState> on SimWorld alongside meso_queues.

3. **Bus stop detection: linear scan vs spatial index**
   - What we know: ~10K buses, each checking ~5-20 stops per route. BusState::should_stop() does edge_id + offset comparison.
   - What's unclear: Whether linear scan is fast enough at scale
   - Recommendation: Linear scan is fine. Each bus only checks its NEXT stop (current_stop_index), so it's O(1) per bus per frame, not O(stops). Total: ~10K comparisons/frame.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + cargo test |
| Config file | Cargo.toml per-crate |
| Quick run command | `cargo test -p velos-gpu --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AGT-01 | Bus dwell: begin_dwell/tick_dwell called in sim loop, FLAG_BUS_DWELLING set, empirical model formula | integration | `cargo test -p velos-gpu --test integration_bus_dwell -x` | Wave 0 |
| AGT-01 | GPU holds dwelling bus at zero speed via FLAG_BUS_DWELLING | unit (WGSL) | `cargo test -p velos-gpu --test wave_front_validation -x` | Partial (shader exists, dwelling test needed) |
| AGT-05 | Meso-micro transition: buffer zone velocity matching, no speed discontinuity | integration | `cargo test -p velos-gpu --test integration_meso_micro -x` | Wave 0 |
| AGT-06 | SpatialQueue active for meso edges, velos-meso imported | integration | `cargo test -p velos-gpu --test integration_meso_micro -x` | Wave 0 |
| AGT-06 | ZoneConfig loaded from TOML, graceful degradation | unit | `cargo test -p velos-meso --test zone_config -x` | Exists (zone_config_tests in velos-meso) |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-gpu --lib && cargo test -p velos-meso`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before /gsd:verify-work

### Wave 0 Gaps
- [ ] `crates/velos-gpu/tests/integration_bus_dwell.rs` -- covers AGT-01 (bus dwell in sim loop)
- [ ] `crates/velos-gpu/tests/integration_meso_micro.rs` -- covers AGT-05, AGT-06 (meso-micro transitions)
- [ ] WGSL test for FLAG_BUS_DWELLING guard in `wave_front_validation.rs` -- add test case
- [ ] velos-meso dependency in velos-gpu/Cargo.toml -- prerequisite for all meso integration

## Sources

### Primary (HIGH confidence)
- `crates/velos-vehicle/src/bus.rs` -- BusState, BusDwellModel, BusStop (full implementation reviewed)
- `crates/velos-meso/src/queue_model.rs` -- SpatialQueue with BPR travel time (full implementation reviewed)
- `crates/velos-meso/src/buffer_zone.rs` -- BufferZone with smoothstep interpolation (full implementation reviewed)
- `crates/velos-meso/src/zone_config.rs` -- ZoneConfig with TOML loading and centroid auto-designation (full implementation reviewed)
- `crates/velos-gpu/src/sim.rs` -- SimWorld struct and tick_gpu() 10-step pipeline (full implementation reviewed)
- `crates/velos-gpu/src/sim_lifecycle.rs` -- spawn_single_agent() and edge transition logic (full implementation reviewed)
- `crates/velos-gpu/shaders/wave_front.wgsl` -- FLAG_BUS_DWELLING defined (bit 0), FLAG_YIELDING pattern for reference
- `docs/architect/02-agent-models.md` -- Bus dwell model spec and meso-micro transition protocol

### Secondary (MEDIUM confidence)
- None needed -- all sources are project-internal code

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, no new external deps
- Architecture: HIGH -- tick_gpu() pipeline fully reviewed, insertion points clear
- Pitfalls: HIGH -- derived from direct code review of existing patterns and data flow

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (stable -- internal codebase, no external API changes)
