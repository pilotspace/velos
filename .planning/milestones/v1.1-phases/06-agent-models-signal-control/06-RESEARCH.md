# Phase 6: Agent Models & Signal Control - Research

**Researched:** 2026-03-07
**Domain:** Traffic agent types (bus/bicycle/truck/emergency), pedestrian GPU optimization, meso-micro hybrid, actuated/adaptive signals, V2I, traffic signs
**Confidence:** HIGH

## Summary

Phase 6 extends the Phase 5 GPU simulation engine with seven new agent behaviors, three signal controller types, and two infrastructure systems (SPaT broadcast, traffic signs). The existing codebase provides strong foundations: `VehicleType` enum (needs extension), `GpuAgentState` struct (needs `vehicle_type` field), `FixedTimeController` (pattern for actuated/adaptive), sublane model (bicycle reuse), social force model (pedestrian optimization), and a demand spawner (GTFS extension point).

The key technical challenges are: (1) expanding `GpuAgentState` from 32 to 40 bytes without breaking GPU alignment, (2) implementing prefix-sum compaction in WGSL for pedestrian adaptive workgroups, (3) creating the `velos-meso` crate with a spatial queue model, and (4) adding multi-agent-type branching to the wave-front WGSL shader. All challenges have well-understood solutions from traffic simulation literature and GPU compute patterns.

**Primary recommendation:** Implement in waves -- agent types first (bus/bicycle/truck/emergency extend existing patterns), then signal controllers (actuated/adaptive build on FixedTimeController), then GPU-heavy work (pedestrian prefix-sum, meso-micro), then V2I/signs.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Extend `VehicleType` enum to: Motorbike, Car, Bus, Bicycle, Truck, Emergency, Pedestrian
- Add `vehicle_type: u32` field to `GpuAgentState` for GPU-side branching
- Existing `cf_model` tag stays separate from `vehicle_type`
- Bus dwell: `5s + 0.5s/boarding + 0.67s/alighting`, capped 60s
- Bus stops as ECS components on edges
- GTFS import in velos-demand (not velos-net)
- Bicycles reuse sublane model with rightmost pref, IDM v0=15km/h, width 0.6m
- Truck: 12m length, 1.0 m/s^2 accel, 90 km/h max, rightmost lane preference
- Emergency: 50m yield cone, signal priority, ignore red with decel
- Pedestrian adaptive workgroups: spatial hash 2m/5m/10m cells, prefix-sum compaction
- Meso-micro: runtime toggle `SimConfig::meso_enabled`, static zone config, 100m buffer, IDM interpolation
- New `velos-meso` crate for queue model and buffer zone logic
- ActuatedController: loop detectors, gap-out 3s, min green 7s, max green 60s
- Adaptive: proportional queue redistribution per cycle
- SPaT broadcast to 200m range, GLOSA speed advisory
- Signal priority: emergency > bus, 100m range, max 1 request/cycle
- Traffic signs: speed limit, stop, yield, no-turn, school zone as ECS on edges
- Dual sign enforcement: pathfinding cost + runtime behavior
- Signs as GPU buffer alongside agent state

### Claude's Discretion
- GPU shader architecture for multi-agent-type branching (WGSL code organization)
- Prefix-sum compaction implementation details for pedestrian workgroups
- Queue model internals for meso zones (BPR-based or simple FIFO)
- Exact GTFS parsing strategy (full spec vs minimal route/stop/schedule)
- Loop detector implementation (virtual point sensor vs zone-based)
- Buffer zone IDM interpolation curve (linear vs smooth step)
- GpuAgentState struct packing for new fields (maintain 32-byte alignment or expand)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| AGT-01 | Bus agents with empirical dwell time model | Bus dwell formula defined, GTFS structures for route/stop data, IDM params for bus type |
| AGT-02 | GTFS import for 130 HCMC bus routes | `gtfs-structures` crate v0.47.0 parses routes/stops/stop_times; lives in velos-demand |
| AGT-03 | Bicycle agents with sublane model | Reuse existing `sublane.rs` with narrower width (0.3m half_width) and IDM v0=4.17m/s |
| AGT-04 | Pedestrian adaptive GPU workgroups | Three-pass WGSL prefix-sum: count, scan, scatter; workgroup-level Hillis-Steele scan |
| AGT-05 | Meso-micro 100m graduated buffer zone | Linear IDM param interpolation over buffer distance; velocity-matching insertion |
| AGT-06 | Mesoscopic queue model O(1)/edge | Spatial queue model: free-flow segment + queue segment per edge, BPR exit rate |
| AGT-07 | Truck agent type with distinct dynamics | IDM params: s0=4m, T=2.0s, a=1.0, v0=25m/s; rightmost lane constraint |
| AGT-08 | Emergency vehicle priority + yield | vehicle_type flag, 50m cone detection, signal priority request, yield behavior |
| SIG-01 | Actuated signal control with loop detectors | Virtual point sensors on approach lanes, gap-out timer per phase, min/max green bounds |
| SIG-02 | Adaptive signal with demand-responsive timing | Per-cycle queue-length proportional green redistribution |
| SIG-03 | SPaT broadcast to agents within range | Signal stores phase + time-to-next; agents query during perception; GLOSA speed calc |
| SIG-04 | Signal priority for buses and emergency | Priority request queue, green extension / conflicting red shortening, starvation guard |
| SIG-05 | Traffic sign interaction | Sign ECS component, GPU sign buffer, speed limit/stop/yield/no-turn/school zone behaviors |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| gtfs-structures | 0.47.0 | GTFS parsing (routes, stops, stop_times, trips) | Only maintained Rust GTFS parser; serde-based, HashMap collections |
| bytemuck | (workspace) | Pod/Zeroable derive for new GPU structs | Already used throughout for GpuAgentState, required for repr(C) GPU buffers |
| hecs | (workspace) | ECS for new components (BusStop, Sign, LoopDetector) | Already the project ECS; all new agent components attach here |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| rand | (workspace) | Stochastic dwell time, jaywalking, dawdle | Already in velos-demand spawner; extend for bus passenger counts |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| gtfs-structures | fastgtfs (flatbuffers) | fastgtfs is faster for large feeds but less maintained; gtfs-structures simpler API |
| Manual prefix-sum | GPUPrefixSums WGPU port | GPUPrefixSums is testing-only, not production-ready; manual 3-pass is simpler and sufficient |

**Installation:**
```bash
cargo add gtfs-structures --package velos-demand
```

## Architecture Patterns

### Recommended Project Structure
```
crates/
  velos-core/src/
    components.rs          # Extended VehicleType enum, GpuAgentState with vehicle_type
  velos-vehicle/src/
    types.rs               # Extended VehicleType + default params for Bus/Bicycle/Truck/Emergency
    bus.rs                 # NEW: BusDwellModel, BusStop component, bus stop pull-over logic
    emergency.rs           # NEW: yield cone detection, emergency priority flag
  velos-signal/src/
    controller.rs          # Existing FixedTimeController (unchanged)
    actuated.rs            # NEW: ActuatedController with gap-out logic
    adaptive.rs            # NEW: AdaptiveController with queue-proportional timing
    priority.rs            # NEW: PriorityRequestQueue for bus/emergency
    spat.rs                # NEW: SPaT broadcast and GLOSA computation
    signs.rs               # NEW: TrafficSign component, sign buffer, sign behaviors
    detector.rs            # NEW: LoopDetector virtual point sensor
  velos-demand/src/
    gtfs.rs                # NEW: GTFS import -> BusRoute/BusStop/BusSchedule structs
    spawner.rs             # Extended with Bus/Bicycle/Truck/Emergency spawn types
  velos-meso/              # NEW CRATE
    src/
      lib.rs               # Queue model, buffer zone, zone config
      queue_model.rs       # Spatial queue: free-flow + queue segments
      buffer_zone.rs       # 100m graduated IDM interpolation
      zone_config.rs       # Static meso/micro zone designation
  velos-gpu/shaders/
    wave_front.wgsl        # Extended with vehicle_type branching (bus dwell, emergency, signs)
    pedestrian_adaptive.wgsl  # NEW: 3-pass prefix-sum + social force with adaptive workgroups
    sign_interaction.wgsl  # NEW: Sign buffer reads + speed/behavior modification
```

### Pattern 1: Signal Controller Trait
**What:** Common trait for all signal controller types (Fixed, Actuated, Adaptive)
**When to use:** Always -- the simulation loop dispatches controllers polymorphically
**Example:**
```rust
/// Common interface for all signal controllers.
pub trait SignalController {
    /// Advance the controller by dt seconds.
    fn tick(&mut self, dt: f64, detectors: &[DetectorReading]);
    /// Get current phase state for an approach.
    fn get_phase_state(&self, approach_index: usize) -> PhaseState;
    /// Handle a priority request (bus or emergency).
    fn request_priority(&mut self, request: PriorityRequest);
    /// Get SPaT data for broadcast.
    fn spat_data(&self) -> SpatBroadcast;
    /// Reset to cycle start.
    fn reset(&mut self);
}
```

### Pattern 2: Vehicle Type Parameter Factory
**What:** Extend the existing `default_idm_params()` pattern to cover all new types
**When to use:** When spawning any new agent type
**Example:**
```rust
pub fn default_idm_params(vehicle_type: VehicleType) -> IdmParams {
    match vehicle_type {
        VehicleType::Car => IdmParams { v0: 13.9, s0: 2.0, t_headway: 1.5, a: 1.0, b: 2.0, delta: 4.0 },
        VehicleType::Motorbike => IdmParams { v0: 11.1, s0: 1.0, t_headway: 1.0, a: 2.0, b: 3.0, delta: 4.0 },
        VehicleType::Bus => IdmParams { v0: 11.1, s0: 3.0, t_headway: 1.5, a: 1.0, b: 2.5, delta: 4.0 },
        VehicleType::Bicycle => IdmParams { v0: 4.17, s0: 1.5, t_headway: 1.0, a: 1.0, b: 3.0, delta: 4.0 },
        VehicleType::Truck => IdmParams { v0: 25.0, s0: 4.0, t_headway: 2.0, a: 1.0, b: 2.5, delta: 4.0 },
        VehicleType::Emergency => IdmParams { v0: 16.7, s0: 2.0, t_headway: 1.2, a: 2.0, b: 3.5, delta: 4.0 },
        VehicleType::Pedestrian => IdmParams { v0: 1.4, s0: 0.5, t_headway: 0.5, a: 0.5, b: 1.0, delta: 4.0 },
    }
}
```

### Pattern 3: GpuAgentState Expansion (32 -> 40 bytes)
**What:** Add `vehicle_type` and `flags` fields to the packed GPU struct
**When to use:** The single most impactful change -- touches all GPU buffer code
**Example:**
```rust
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuAgentState {
    pub edge_id: u32,        // 0..4
    pub lane_idx: u32,       // 4..8
    pub position: i32,       // 8..12  Q16.16
    pub lateral: i32,        // 12..16 Q8.8 in i32
    pub speed: i32,          // 16..20 Q12.20
    pub acceleration: i32,   // 20..24 Q12.20
    pub cf_model: u32,       // 24..28 0=IDM, 1=Krauss
    pub rng_state: u32,      // 28..32
    pub vehicle_type: u32,   // 32..36 NEW: 0=Motorbike,1=Car,2=Bus,3=Bicycle,4=Truck,5=Emergency,6=Ped
    pub flags: u32,          // 36..40 NEW: bit0=at_bus_stop, bit1=emergency_active, bit2=yielding, etc.
}
// Total: 40 bytes (aligned to 8 bytes, valid for GPU storage buffers)
```

### Pattern 4: Three-Pass Prefix-Sum for Pedestrian Compaction
**What:** GPU pedestrian dispatch uses count -> scan -> scatter to compact non-empty cells
**When to use:** Pedestrian adaptive workgroup dispatch
**Example (WGSL pseudocode):**
```wgsl
// Pass 1: Count pedestrians per spatial hash cell (atomic adds)
// Pass 2: Exclusive prefix sum on cell_counts -> cell_offsets
//          Use workgroup-level Hillis-Steele scan (O(n log n) work, simple)
//          For >1 workgroup, use reduce-then-scan (2 dispatches)
// Pass 3: Scatter pedestrians into compacted array using cell_offsets
// Pass 4: Social force on compacted non-empty cells only
```

### Pattern 5: Sign Buffer as Separate GPU Binding
**What:** Traffic signs stored in a separate storage buffer, bound alongside agent state
**When to use:** Signs are read-only per-step; separate buffer avoids expanding agent state further

### Anti-Patterns to Avoid
- **Monolithic shader:** Do NOT put all agent-type logic in one massive if/else chain in `wave_front.wgsl`. Use helper functions per type and keep the main loop readable.
- **Dynamic workgroup sizing in WGSL:** WGSL requires compile-time `@workgroup_size`. Use indirect dispatch with different kernels for dense/sparse pedestrian zones, NOT dynamic workgroup sizes.
- **Meso agents on GPU:** Mesoscopic queue model is CPU-only (O(1) per edge, no physics). Do NOT upload meso agents to GPU buffers.
- **Full GTFS parse:** Do NOT parse the entire GTFS spec. Only need: routes.txt, stops.txt, stop_times.txt, trips.txt. Skip fare, shape, calendar complexity.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| GTFS CSV parsing | Custom CSV parser for transit data | `gtfs-structures` crate | Handles all GTFS quirks (optional fields, encoding, cross-references) |
| Prefix-sum correctness | Novel scan algorithm | Hillis-Steele workgroup scan + reduce-then-scan for multi-workgroup | Well-studied algorithm with known correctness; WGSL atomics are limited |
| Signal timing math | Custom cycle time calculations | Port SUMO's gap-out logic verbatim | SUMO's actuated control is battle-tested; gap-out timer is simple state machine |
| BPR travel time function | Custom speed-density relationship | Standard BPR: `t = t0 * (1 + alpha * (V/C)^beta)` with alpha=0.15, beta=4.0 | BPR is the industry standard; coefficients are well-calibrated |

**Key insight:** Agent models are parameter variations on existing patterns (IDM, sublane). The real complexity is in GPU buffer management and shader branching, not in the traffic science.

## Common Pitfalls

### Pitfall 1: GpuAgentState Size Change Cascading
**What goes wrong:** Changing GpuAgentState from 32 to 40 bytes breaks every GPU buffer creation, upload, download, and read-back site.
**Why it happens:** The struct is used in `compute.rs` (13 references), `multi_gpu.rs` (4 references), `sim.rs` (4 references), plus all test files.
**How to avoid:** Make GpuAgentState expansion the FIRST task. Grep for all `GpuAgentState` references and update every site. Run all existing GPU tests before proceeding.
**Warning signs:** GPU buffer bind group validation errors, incorrect agent count on readback.

### Pitfall 2: WGSL Prefix-Sum Atomics Limitations
**What goes wrong:** Attempting decoupled look-back prefix sum in WGSL fails because Metal backend lacks the required atomic barrier types.
**Why it happens:** WGSL atomic model follows Metal's strictly-typed atomics, which don't support cross-workgroup synchronization needed by advanced scan algorithms.
**How to avoid:** Use simple multi-dispatch approach: (1) per-workgroup reduction, (2) scan of workgroup sums, (3) per-workgroup propagation. Three dispatches instead of one, but correct and portable.
**Warning signs:** Validation errors on Metal, non-deterministic results, hanging shaders.

### Pitfall 3: Bus Dwell Blocking Wave-Front
**What goes wrong:** Bus dwelling at a stop blocks the entire lane in the wave-front shader, causing followers to pile up unrealistically.
**Why it happens:** Wave-front processes front-to-back; a stopped bus is the leader with gap=0 for all followers.
**How to avoid:** In the shader, when a bus is dwelling (`flags & BUS_DWELLING != 0`), followers should check if they can lane-change around the bus. The bus dwell timer should be managed CPU-side (not GPU) and the bus's speed forced to 0 with a "no-follow" marker.
**Warning signs:** All agents behind a dwelling bus stopping permanently.

### Pitfall 4: Meso-Micro Boundary Speed Discontinuity
**What goes wrong:** Agents entering micro zone from meso have a speed mismatch with existing micro traffic, causing phantom braking waves.
**Why it happens:** Meso exit speed (from BPR function) doesn't account for actual micro-zone conditions.
**How to avoid:** Velocity-matching insertion: query the last micro-zone vehicle's speed and use `min(meso_exit_speed, last_micro_speed)`. Plus 100m buffer with interpolated IDM params.
**Warning signs:** Sudden speed drops at zone boundaries, queue formation at meso-micro borders.

### Pitfall 5: Emergency Vehicle Yield Cone Scope
**What goes wrong:** Yield detection runs on GPU for all agents every step, even when no emergency vehicles are active.
**Why it happens:** Naive implementation checks 50m cone for every agent pair.
**How to avoid:** Use the `flags` field: only run yield detection when `emergency_count > 0` (tracked CPU-side). Early-exit in shader: if `emergency_count == 0`, skip entire yield block.
**Warning signs:** Performance regression when emergency vehicles are rare (99.9% of the time).

### Pitfall 6: VehicleType Enum Sync Between Crates
**What goes wrong:** `VehicleType` exists in both `velos-core/src/components.rs` and `velos-vehicle/src/types.rs`. Adding variants to one but not the other causes compilation errors or silent mismatches.
**Why it happens:** Historical design with duplicated enum.
**How to avoid:** Add variants to BOTH locations simultaneously. Consider making `velos-core::VehicleType` the canonical source and having `velos-vehicle` re-export it (requires adding velos-core as dependency of velos-vehicle).
**Warning signs:** Match arm exhaustiveness errors in one crate but not the other.

## Code Examples

### Bus Dwell Model (CPU-side)
```rust
/// Empirical bus dwell time model for HCMC.
pub struct BusDwellModel {
    pub fixed_dwell_s: f64,        // 5.0s door open/close
    pub per_boarding_s: f64,       // 0.5s per boarding passenger
    pub per_alighting_s: f64,      // 0.67s per alighting passenger
    pub max_dwell_s: f64,          // 60.0s cap
}

impl BusDwellModel {
    pub fn compute_dwell(&self, boarding: u32, alighting: u32) -> f64 {
        let dwell = self.fixed_dwell_s
            + self.per_boarding_s * boarding as f64
            + self.per_alighting_s * alighting as f64;
        dwell.min(self.max_dwell_s)
    }
}
```

### Actuated Signal Controller (gap-out state machine)
```rust
pub struct ActuatedController {
    plan: SignalPlan,
    elapsed: f64,
    current_phase_idx: usize,
    phase_active_time: f64,  // time current phase has been green
    gap_timer: f64,          // time since last detector activation
    min_green: f64,          // 7.0s
    max_green: f64,          // 60.0s
    gap_threshold: f64,      // 3.0s
    num_approaches: usize,
    detectors: Vec<LoopDetector>,
}

impl ActuatedController {
    pub fn tick(&mut self, dt: f64, detector_readings: &[bool]) {
        self.elapsed += dt;
        self.phase_active_time += dt;
        self.gap_timer += dt;

        // Reset gap timer if any detector on current phase's approaches fires
        let current_approaches = &self.plan.phases[self.current_phase_idx].approaches;
        for &approach in current_approaches {
            if detector_readings.get(approach).copied().unwrap_or(false) {
                self.gap_timer = 0.0;
            }
        }

        // Phase transition logic
        let should_transition = self.phase_active_time >= self.max_green
            || (self.phase_active_time >= self.min_green && self.gap_timer >= self.gap_threshold);

        if should_transition {
            self.current_phase_idx = (self.current_phase_idx + 1) % self.plan.phases.len();
            self.phase_active_time = 0.0;
            self.gap_timer = 0.0;
        }
    }
}
```

### Spatial Queue Model (mesoscopic)
```rust
/// O(1) per-step mesoscopic queue model for a single edge.
pub struct SpatialQueue {
    /// Edge free-flow travel time (seconds).
    pub t_free: f64,
    /// Edge capacity (vehicles/hour).
    pub capacity: f64,
    /// BPR alpha (0.15 standard).
    pub alpha: f64,
    /// BPR beta (4.0 standard).
    pub beta: f64,
    /// Current vehicle count on this edge.
    pub vehicle_count: u32,
    /// Queue of vehicles with entry_time (FIFO).
    pub queue: VecDeque<MesoVehicle>,
}

impl SpatialQueue {
    /// BPR travel time: t = t_free * (1 + alpha * (V/C)^beta)
    pub fn travel_time(&self) -> f64 {
        let vc_ratio = self.vehicle_count as f64 / self.capacity.max(1.0);
        self.t_free * (1.0 + self.alpha * vc_ratio.powf(self.beta))
    }

    /// Check if the first vehicle in queue should exit (O(1)).
    pub fn try_exit(&mut self, sim_time: f64) -> Option<MesoVehicle> {
        if let Some(front) = self.queue.front() {
            if sim_time - front.entry_time >= self.travel_time() {
                self.vehicle_count -= 1;
                return self.queue.pop_front();
            }
        }
        None
    }
}
```

### Wave-Front Shader Vehicle-Type Branching
```wgsl
// Recommended: helper functions per vehicle type, clean branching in main loop
fn handle_bus_dwell(agent_idx: u32, agent: ptr<function, AgentState>) -> bool {
    let flags = (*agent).flags;
    if (flags & FLAG_BUS_DWELLING) != 0u {
        // Bus is dwelling -- speed stays 0, CPU manages dwell timer
        (*agent).speed = 0;
        (*agent).acceleration = 0;
        return true;  // handled, skip normal car-following
    }
    return false;
}

fn handle_emergency_yield(agent_idx: u32, agent: ptr<function, AgentState>) {
    if emergency_count == 0u { return; }  // early exit: no emergencies active
    // Check if within 50m cone of any emergency vehicle
    // If so, set yielding flag and reduce speed
}

// In main wave-front loop:
if agent.vehicle_type == VT_BUS {
    if handle_bus_dwell(agent_idx, &agent) { continue; }
}
// Normal car-following (IDM/Krauss) applies to all types
// Then overlay sign interaction and yield behavior
```

## Discretion Recommendations

### GPU Shader Architecture for Multi-Type Branching
**Recommendation:** Use a layered approach in the wave-front shader:
1. Vehicle-type-specific pre-processing (bus dwell check, emergency flag)
2. Standard car-following (IDM/Krauss -- same for all motor vehicles)
3. Vehicle-type-specific post-processing (sign interaction, yield, speed limits)

This keeps the core physics loop clean and adds type-specific logic as pre/post hooks. Helper functions per type, NOT one giant if/else.

### Prefix-Sum Implementation
**Recommendation:** Three-dispatch reduce-then-scan approach:
- Dispatch 1: Per-workgroup prefix sum using Hillis-Steele (workgroup_size=256, within shared memory)
- Dispatch 2: Scan of per-workgroup totals (single workgroup if <65K cells)
- Dispatch 3: Add scanned totals back to each workgroup's elements

This is portable across Metal/Vulkan/DX12 and avoids WGSL atomic limitations. For VELOS's pedestrian count (~20K), a single dispatch with workgroup reduction may suffice (20K / 256 = ~78 workgroups).

### Queue Model for Meso Zones
**Recommendation:** BPR-based spatial queue (not simple FIFO). Reasons:
- BPR function `t = t_free * (1 + 0.15 * (V/C)^4)` captures congestion effects
- O(1) per edge per step (just check front of FIFO against BPR travel time)
- Well-calibrated coefficients exist for urban networks
- Simple FIFO ignores capacity constraints and produces unrealistic travel times

### GTFS Parsing Strategy
**Recommendation:** Minimal parse -- only 4 files from GTFS spec:
- `routes.txt` -> route_id, route_short_name
- `stops.txt` -> stop_id, stop_name, stop_lat, stop_lon
- `trips.txt` -> trip_id, route_id, service_id, direction_id
- `stop_times.txt` -> trip_id, stop_id, arrival_time, departure_time, stop_sequence

Use `gtfs-structures` crate which parses all these into `Gtfs` struct with HashMap lookups. Post-process into velos-internal `BusRoute` structs that map stop positions to road edge+offset.

### Loop Detector Implementation
**Recommendation:** Virtual point sensor (not zone-based). Implementation:
- Each detector is a position on an approach lane (edge_id + offset)
- Each simulation step, check if any agent crossed the detector point (position_prev < detector_pos <= position_now)
- Return `true`/`false` per detector per step -- this is all the actuated controller needs
- Point sensors are simpler and match real-world inductive loop behavior

### Buffer Zone IDM Interpolation
**Recommendation:** Smooth-step (Hermite) interpolation, not linear. Reasoning:
- Linear creates a discontinuity in the derivative at both ends of the buffer
- Smooth-step: `t = 3x^2 - 2x^3` (where x = distance_into_buffer / 100m)
- `T_effective = T_relaxed + (T_normal - T_relaxed) * smoothstep(x)`
- This produces C1-continuous parameter variation, eliminating acceleration artifacts

### GpuAgentState Packing
**Recommendation:** Expand to 40 bytes (add 2 u32 fields):
- `vehicle_type: u32` (at byte 32) -- vehicle type enum value
- `flags: u32` (at byte 36) -- bitfield for runtime state flags

40 bytes is still well-aligned (8-byte aligned, divisible by 4). The 25% size increase (32->40) adds 2.2MB for 280K agents (trivial vs 24GB VRAM). All existing buffer code needs stride update.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Fixed workgroup for all pedestrians | Density-adaptive workgroup sizing | Standard in modern GPU particle sims | 3-8x speedup for non-uniform density |
| Separate meso/micro simulators | Hybrid within single simulator | SUMO, Aimsun support since ~2015 | Seamless zone transitions |
| Fixed-time signals only | Actuated + adaptive | SUMO NEMA controller since 2022 | Realistic signal response to traffic demand |
| Manual transit route entry | GTFS standard import | GTFS adopted worldwide since 2006 | 130 HCMC bus routes available as open data |

**Deprecated/outdated:**
- Simple FIFO queue model for meso (no congestion feedback) -- use BPR-based spatial queue instead
- Uniform pedestrian grid dispatch -- use adaptive spatial hash with prefix-sum compaction

## Open Questions

1. **HCMC GTFS data availability**
   - What we know: GTFS standard exists, `gtfs-structures` can parse it
   - What's unclear: Whether HCMC bus data is available in GTFS format or needs manual conversion
   - Recommendation: Build the GTFS importer for standard format; provide a fallback CSV loader for manual route/stop data

2. **Pedestrian prefix-sum workgroup count**
   - What we know: ~20K pedestrians at 5% mode share; spatial hash cells at 2m/5m/10m
   - What's unclear: Exact non-empty cell count at runtime determines if single-workgroup scan suffices
   - Recommendation: Start with multi-dispatch reduce-then-scan; optimize to single-dispatch if profiling shows cell count < 256

3. **Emergency vehicle count in practice**
   - What we know: Emergency vehicles will be rare (<0.1% of agents)
   - What's unclear: How many simultaneous emergency vehicles to support
   - Recommendation: Store emergency vehicle indices in a small GPU buffer (max 16); broadcast count as uniform parameter for early-exit

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) |
| Config file | Cargo.toml [workspace] |
| Quick run command | `cargo test --lib -p velos-vehicle -p velos-signal -p velos-demand -p velos-meso -p velos-core` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AGT-01 | Bus dwell time computation | unit | `cargo test -p velos-vehicle --lib bus` | Wave 0 |
| AGT-02 | GTFS route/stop parsing | unit | `cargo test -p velos-demand --lib gtfs` | Wave 0 |
| AGT-03 | Bicycle sublane with narrow width | unit | `cargo test -p velos-vehicle --lib sublane -- bicycle` | Wave 0 |
| AGT-04 | Pedestrian adaptive dispatch speedup | integration | `cargo test -p velos-gpu -- pedestrian_adaptive` | Wave 0 |
| AGT-05 | Buffer zone IDM interpolation | unit | `cargo test -p velos-meso --lib buffer_zone` | Wave 0 |
| AGT-06 | Spatial queue O(1) exit check | unit | `cargo test -p velos-meso --lib queue_model` | Wave 0 |
| AGT-07 | Truck IDM params and lane constraint | unit | `cargo test -p velos-vehicle --lib types -- truck` | Wave 0 |
| AGT-08 | Emergency yield + signal priority | integration | `cargo test -p velos-signal -- emergency` | Wave 0 |
| SIG-01 | Actuated gap-out phase transition | unit | `cargo test -p velos-signal --lib actuated` | Wave 0 |
| SIG-02 | Adaptive queue-proportional timing | unit | `cargo test -p velos-signal --lib adaptive` | Wave 0 |
| SIG-03 | SPaT broadcast data generation | unit | `cargo test -p velos-signal --lib spat` | Wave 0 |
| SIG-04 | Priority request handling | unit | `cargo test -p velos-signal --lib priority` | Wave 0 |
| SIG-05 | Sign speed limit reduces agent v0 | integration | `cargo test -p velos-gpu -- sign_interaction` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib -p <affected-crate>`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-vehicle/src/bus.rs` + `crates/velos-vehicle/tests/bus_tests.rs` -- bus dwell model
- [ ] `crates/velos-demand/src/gtfs.rs` + `crates/velos-demand/tests/gtfs_tests.rs` -- GTFS parsing
- [ ] `crates/velos-meso/` -- entire new crate (lib.rs, queue_model.rs, buffer_zone.rs, zone_config.rs + tests)
- [ ] `crates/velos-signal/src/actuated.rs` + tests -- actuated controller
- [ ] `crates/velos-signal/src/adaptive.rs` + tests -- adaptive controller
- [ ] `crates/velos-signal/src/priority.rs` + tests -- priority request handling
- [ ] `crates/velos-signal/src/spat.rs` + tests -- SPaT broadcast
- [ ] `crates/velos-signal/src/signs.rs` + tests -- traffic sign component
- [ ] `crates/velos-signal/src/detector.rs` + tests -- loop detector
- [ ] `crates/velos-vehicle/src/emergency.rs` + tests -- emergency vehicle logic
- [ ] GTFS test fixture file (minimal route/stop/stop_times for 1-2 routes)

## Sources

### Primary (HIGH confidence)
- Existing codebase: `velos-core/src/components.rs`, `velos-vehicle/src/types.rs`, `velos-signal/src/controller.rs`, `velos-gpu/shaders/wave_front.wgsl` -- all directly inspected
- Architecture doc: `docs/architect/02-agent-models.md` -- agent models, IDM params, pedestrian model, meso-micro transition
- Architecture doc: `docs/architect/01-simulation-engine.md` -- GPU dispatch, fixed-point, ECS layout

### Secondary (MEDIUM confidence)
- [gtfs-structures v0.47.0 API](https://docs.rs/gtfs-structures/latest/gtfs_structures/struct.Gtfs.html) -- verified fields: stops, routes, trips with HashMap collections
- [SUMO Traffic Lights documentation](https://sumo.dlr.de/docs/Simulation/Traffic_Lights.html) -- actuated control gap-out algorithm, NEMA controller
- [Raph Levien: Prefix sum on portable compute shaders](https://raphlinus.github.io/gpu/2021/11/17/prefix-sum-portable.html) -- confirmed WGSL limitations for advanced prefix-sum
- [GPUPrefixSums WGPU port](https://github.com/b0nes164/GPUPrefixSums) -- reduce-then-scan approach available but testing-only
- [DTALite mesoscopic queue model](https://www.tandfonline.com/doi/full/10.1080/23311916.2014.961345) -- BPR-based spatial queue model reference
- [FHWA Traffic Detector Handbook](https://www.fhwa.dot.gov/publications/research/operations/its/06108/04.cfm) -- loop detector gap-out logic

### Tertiary (LOW confidence)
- HCMC GTFS data availability -- no verified source found; may need manual route/stop CSV as fallback

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- gtfs-structures is the only maintained Rust GTFS parser; all other libs already in workspace
- Architecture: HIGH -- all patterns extend existing code (VehicleType, FixedTimeController, sublane, social force); GpuAgentState expansion is mechanical
- Pitfalls: HIGH -- verified by inspecting actual code references (21 GpuAgentState refs, WGSL atomic limitations confirmed by Raph Levien)
- Meso model: MEDIUM -- BPR spatial queue is well-documented in literature but no Rust implementation exists; must implement from scratch
- Prefix-sum: MEDIUM -- simple reduce-then-scan is portable but performance characteristics on Metal unverified

**Research date:** 2026-03-07
**Valid until:** 2026-04-07 (stable domain, 30 days)
