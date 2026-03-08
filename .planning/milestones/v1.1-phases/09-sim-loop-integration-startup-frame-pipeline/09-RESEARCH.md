# Phase 9: Sim Loop Integration -- Startup & Frame Pipeline - Research

**Researched:** 2026-03-08
**Domain:** Simulation loop wiring, GPU pipeline integration, Rust trait dispatch
**Confidence:** HIGH

## Summary

Phase 9 is a pure integration/wiring phase. All modules exist (PerceptionPipeline, RerouteState, SignalController trait, ActuatedController, AdaptiveController, upload_vehicle_params, upload_signs, red_light_creep_speed, intersection_gap_acceptance). The work is connecting them into SimWorld::new() for startup and tick_gpu() for per-frame execution.

The codebase has clear patterns for this integration: SimWorld owns simulation state, GpuState owns GPU resources + SimWorld, tick_gpu() orchestrates frame pipeline. The current pipeline is `spawn -> signals -> GPU vehicles -> pedestrians -> cleanup`. Target pipeline is `spawn -> signals -> perception -> reroute -> GPU vehicles -> pedestrians -> gridlock -> cleanup`.

**Primary recommendation:** Work in two passes -- (1) startup initialization in SimWorld::new() and GpuState::new(), (2) frame pipeline expansion in tick_gpu(). Signal controller polymorphism and WGSL shader modifications for HCMC behaviors can be done in parallel within these passes.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- All new subsystems initialize in SimWorld::new(), not GpuState::new() -- keeps simulation logic contained in sim layer
- Vehicle params loaded from TOML config with hardcoded default path `data/hcmc/vehicle_params.toml`, overridable via `VELOS_VEHICLE_CONFIG` env var
- If vehicle_params.toml is missing, use VehicleConfig::default() hardcoded HCMC-calibrated values and log warning -- no crash
- init_reroute() blocks startup until CCH is built (<10s on 25K edges) -- sim starts with rerouting ready
- PerceptionPipeline is mandatory -- if GPU can't create it, that's a hard startup failure
- upload_vehicle_params() called at startup to populate GPU uniform buffer at binding 7
- upload_signs() called at startup to populate sign_buffer from network sign data
- Full pipeline order: spawn -> signals -> perception -> reroute -> GPU vehicles -> CPU pedestrians -> gridlock -> cleanup
- Signal controllers tick BEFORE perception (keep current tick_gpu() ordering) -- perception sees fresh signal states
- Perception GPU pass runs BEFORE vehicle physics -- agents perceive current state, physics uses perception to inform behavior
- step_reroute() runs AFTER perception, BEFORE physics -- agents reroute based on fresh perception, physics uses new routes
- HCMC behaviors (red_light_creep_speed, intersection_gap_acceptance) called from WGSL shader reading perception_results -- no CPU readback needed
- Config file `data/hcmc/signal_config.toml` maps intersection IDs to controller types (fixed/actuated/adaptive)
- Unmapped intersections default to FixedTimeController -- safe, predictable default
- If signal_config.toml is missing, all intersections fall back to fixed-time with log warning
- Detector readings for actuated signals come from separate LoopDetector counting mechanism, not from perception pipeline
- SignalController trait dispatches polymorphically -- ActuatedController and AdaptiveController instantiated based on config

### Claude's Discretion
- CCH failure handling strategy (Option<RerouteState> vs hard fail)
- Exact SimWorld::new() initialization sequence within the constraints above
- How to pass device/queue from GpuState to SimWorld for GPU resource creation
- Loop detector update mechanism (how detector readings feed into actuated controller tick)
- Sign buffer population strategy (batch upload at startup vs incremental)
- Perception readback strategy for step_reroute() (sync vs async readback)

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SIG-01 | Actuated signal control with loop detector-triggered phase transitions | ActuatedController exists in velos-signal/src/actuated.rs; needs instantiation from config + detector wiring in tick_gpu() |
| SIG-02 | Adaptive signal control with demand-responsive timing optimization | AdaptiveController exists in velos-signal/src/adaptive.rs; needs instantiation from config + queue length feeding |
| SIG-03 | SPaT broadcast to agents within range for signal-aware driving | SignalController::spat_data() trait method exists; needs per-frame call and feed into perception |
| SIG-04 | Signal priority request from buses and emergency vehicles | SignalController::request_priority() trait method exists; needs detection of bus/emergency at intersection |
| SIG-05 | Traffic sign interaction: speed limits, stop/yield, no-turn, school zones | GpuSign, handle_sign_interaction() in wave_front.wgsl exist; needs upload_signs() call at startup |
| INT-03 | GPU perception phase: sense leader, signal, signs, nearby agents, congestion | PerceptionPipeline fully implemented; needs instantiation in SimWorld and dispatch in tick_gpu() |
| INT-04 | GPU evaluation phase: cost comparison, should_reroute flag | evaluate_reroute() in velos-core/src/reroute.rs exists; wired via step_reroute() |
| INT-05 | Staggered reroute evaluation (1K agents/step, ~50s cycle) | RerouteScheduler with batch_size=1000 exists; needs step_reroute() call in tick_gpu() |
| RTE-03 | Dynamic agent rerouting at 500 reroutes/step using CCH | CCHRouter::query_with_path() exists; wired via step_reroute() after init_reroute() |
| RTE-07 | Prediction-informed routing -- cost uses predicted future travel times | PredictionService and overlay integration exist in sim_reroute.rs; needs init at startup |
| TUN-02 | GPU/CPU parameter parity -- GPU reads params from uniform buffer, not hardcoded | upload_vehicle_params() exists on ComputeDispatcher; needs call at startup |
| TUN-04 | Red-light creep behavior for motorbikes | red_light_creep_speed() CPU reference exists; needs WGSL implementation reading perception_results |
| TUN-06 | Yield-based intersection negotiation with gap acceptance | intersection_gap_acceptance() CPU reference exists; needs WGSL implementation reading perception_results |

</phase_requirements>

## Standard Stack

### Core (already in workspace)
| Library | Purpose | Already Used |
|---------|---------|--------------|
| wgpu | GPU compute dispatch | Yes -- ComputeDispatcher, PerceptionPipeline |
| hecs | ECS world management | Yes -- SimWorld::world |
| bytemuck | Zero-copy GPU buffer casting | Yes -- GpuAgentState, PerceptionResult |
| toml | TOML config file parsing | Yes -- VehicleConfig deserialization (Phase 8) |
| serde | Config deserialization | Yes -- VehicleConfig, VehicleTypeParams |
| log | Structured logging | Yes -- throughout |

### Supporting
| Library | Purpose | When to Use |
|---------|---------|-------------|
| petgraph | Graph traversal for detector/signal setup | Already used in SimWorld::new() |
| velos-signal | All signal controllers + detector + signs | Already dependency of velos-gpu |
| velos-vehicle | Config, sublane, intersection modules | Already dependency of velos-gpu |
| velos-predict | PredictionService for overlay | Already used in sim_reroute.rs |
| velos-net | CCHRouter, EdgeNodeMap | Already used in sim_reroute.rs |

No new dependencies needed. This is a wiring phase.

## Architecture Patterns

### Current SimWorld::new() Flow (to be extended)
```
SimWorld::new(road_graph)
  1. zone_centroids_from_graph()
  2. Spawner::new()
  3. Build signal_controllers (all FixedTimeController)
  4. Build signalized_nodes map
  5. RerouteState::new() (empty, no CCH yet)
```

### Target SimWorld::new() Flow
```
SimWorld::new(road_graph, device, queue)   // NEW: needs GPU refs for PerceptionPipeline
  1. zone_centroids_from_graph()
  2. Spawner::new()
  3. Load VehicleConfig from TOML (with fallback)
  4. upload_vehicle_params() to GPU uniform buffer
  5. Build signal_controllers (polymorphic, from signal_config.toml)
  6. Build signalized_nodes map
  7. Build LoopDetectors for actuated intersections
  8. init_reroute() -- blocks until CCH built
  9. Instantiate PerceptionPipeline
  10. Collect signs from network, upload_signs()
```

### Current tick_gpu() Flow
```
tick_gpu(base_dt, device, queue, dispatcher)
  1. spawn_agents(dt)
  2. step_signals(dt)                         // FixedTimeController only
  3. AgentSnapshot::collect()
  4. step_vehicles_gpu(dt, device, queue, dispatcher)
  5. step_pedestrians(dt, spatial, snapshot)
  6. detect_gridlock()
  7. remove_finished_agents()
  8. update_metrics()
```

### Target tick_gpu() Flow
```
tick_gpu(base_dt, device, queue, dispatcher)
  1. spawn_agents(dt)
  2. update_loop_detectors()                  // NEW: scan agents for detector crossings
  3. step_signals(dt, &detector_readings)     // CHANGED: polymorphic + detector input
  4. step_perception(device, queue, dispatcher) // NEW: GPU perception dispatch + readback
  5. step_reroute(&perception_results)         // NEW: reroute evaluation batch
  6. step_vehicles_gpu(dt, device, queue, dispatcher)  // perception_results available for WGSL
  7. step_pedestrians(dt, spatial, snapshot)
  8. detect_gridlock()
  9. remove_finished_agents()
  10. update_metrics()
```

### Pattern: Device/Queue Passing to SimWorld

**Recommended approach:** SimWorld::new() receives `&wgpu::Device` and `&wgpu::Queue` as parameters. GpuState::new() creates device/queue first, then passes refs to SimWorld::new(). PerceptionPipeline is stored in SimWorld (not GpuState) to keep sim logic together.

```rust
pub struct SimWorld {
    // existing fields...
    pub(crate) perception: PerceptionPipeline,  // NEW
    pub(crate) vehicle_config: VehicleConfig,   // NEW
    pub(crate) loop_detectors: Vec<(NodeIndex, Vec<LoopDetector>)>,  // NEW
}

impl SimWorld {
    pub fn new(road_graph: RoadGraph, device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        // ...
    }
}
```

In GpuState::new():
```rust
let compute_dispatcher = ComputeDispatcher::new(&device);
let sim = SimWorld::new(road_graph, &device, &queue);
// upload_vehicle_params already called inside SimWorld::new
```

### Pattern: Polymorphic Signal Controllers

Current: `Vec<(NodeIndex, FixedTimeController)>`
Target: `Vec<(NodeIndex, Box<dyn SignalController>)>`

```rust
pub signal_controllers: Vec<(NodeIndex, Box<dyn SignalController>)>,
```

step_signals changes from:
```rust
fn step_signals(&mut self, dt: f64) {
    for (_, ctrl) in &mut self.signal_controllers {
        ctrl.tick(dt);
    }
}
```
to:
```rust
fn step_signals(&mut self, dt: f64, detector_readings: &[(NodeIndex, Vec<DetectorReading>)]) {
    for (node, ctrl) in &mut self.signal_controllers {
        let readings = detector_readings
            .iter()
            .find(|(n, _)| n == node)
            .map_or(&[][..], |(_, r)| r.as_slice());
        ctrl.tick(dt, readings);
    }
}
```

### Pattern: Signal Config TOML

```toml
# data/hcmc/signal_config.toml
[[intersection]]
node_id = 42
controller = "actuated"
min_green = 7.0
max_green = 60.0
gap_threshold = 3.0

[[intersection]]
node_id = 99
controller = "adaptive"
min_green = 7.0
```

### Pattern: PerceptionPipeline in Frame Loop

```rust
fn step_perception(
    &self,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    dispatcher: &ComputeDispatcher,
) -> Vec<PerceptionResult> {
    let agent_buffer = match dispatcher.agent_buffer() {
        Some(b) => b,
        None => return Vec::new(),
    };
    let lane_agents_buffer = match dispatcher.lane_agents_buffer() {
        Some(b) => b,
        None => return Vec::new(),
    };

    let bindings = PerceptionBindings {
        agent_buffer,
        lane_agents_buffer,
        signal_buffer: &self.signal_buffer,      // need to create/maintain
        sign_buffer: &self.sign_buffer,           // from ComputeDispatcher
        congestion_grid_buffer: &self.congestion_grid_buffer,
        edge_travel_ratio_buffer: &self.edge_travel_ratio_buffer,
    };

    let bind_group = self.perception.create_bind_group(device, &bindings);
    let params = PerceptionParams {
        agent_count: dispatcher.wave_front_agent_count,
        grid_width: 20,
        grid_height: 20,
        grid_cell_size: 500.0,
    };

    let mut encoder = device.create_command_encoder(&Default::default());
    self.perception.dispatch(&mut encoder, queue, &bind_group, &params);
    queue.submit(std::iter::once(encoder.finish()));

    self.perception.readback_results(device, queue, params.agent_count)
}
```

### Anti-Patterns to Avoid
- **Creating PerceptionPipeline per frame:** Allocate once in SimWorld::new(), reuse. Buffer re-creation is expensive.
- **CPU readback for HCMC behaviors:** red_light_creep and gap_acceptance should read perception_results in WGSL. Do NOT readback to CPU and re-upload.
- **Coupling detectors to perception:** Detectors are physical sensors for signal actuation. Perception is agent awareness. Different systems, different data flows.
- **Blocking on perception readback when not needed:** Only readback for step_reroute(). HCMC behaviors read GPU-side directly.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Signal controller dispatch | Manual if/else chain | `Box<dyn SignalController>` trait objects | Trait already defined, all impls exist |
| Config file loading | Custom parser | toml + serde Deserialize | Already used in Phase 8 for VehicleConfig |
| GPU buffer management | Custom buffer pool | Existing wgpu::Buffer + queue.write_buffer | ComputeDispatcher pattern already established |
| Perception bind group | Rebuild layout each frame | PerceptionPipeline::create_bind_group | Method already exists, reuse layout |

## Common Pitfalls

### Pitfall 1: Borrow Checker in tick_gpu()
**What goes wrong:** tick_gpu() borrows `&mut self` for SimWorld, but also needs immutable refs to perception, road_graph, etc.
**Why it happens:** Rust's borrow checker doesn't understand field-level borrows through `&mut self`.
**How to avoid:** Extract data into local variables before the mutable borrow. Use the `collect-then-mutate` pattern already established in step_vehicles_gpu() and step_reroute().
**Warning signs:** "cannot borrow `self` as immutable because it is also borrowed as mutable"

### Pitfall 2: Empty GPU Buffers
**What goes wrong:** wgpu requires buffers to have non-zero size. Empty sign buffer or zero-agent perception dispatch crashes.
**Why it happens:** Zero signs in network, or no agents spawned yet.
**How to avoid:** Always pre-allocate minimum sizes. Sign buffer already uses `(256 * 16) as u64`. Agent count check `if gpu_agents.is_empty() { return; }` already exists. Add same guard for perception dispatch.
**Warning signs:** "Buffer size must be greater than 0"

### Pitfall 3: Signal Controller Type Change Breaks Existing Code
**What goes wrong:** Changing `signal_controllers: Vec<(NodeIndex, FixedTimeController)>` to `Vec<(NodeIndex, Box<dyn SignalController>)>` breaks existing `ctrl.tick(dt)` calls since the trait signature is `tick(dt, &[DetectorReading])`.
**Why it happens:** FixedTimeController has `tick(dt)` as its own method AND implements `SignalController::tick(dt, &[DetectorReading])`. Once boxed as trait object, only the trait method is available.
**How to avoid:** After changing the type, update all call sites to use the trait method signature. Pass empty `&[]` for fixed-time controllers (they ignore it).

### Pitfall 4: Perception Buffer Dependencies
**What goes wrong:** PerceptionPipeline::create_bind_group requires signal_buffer, sign_buffer, congestion_grid_buffer, edge_travel_ratio_buffer -- these must exist before perception can dispatch.
**Why it happens:** These buffers are created in different subsystems.
**How to avoid:** Create placeholder GPU buffers at startup for all perception inputs. Populate with real data when available. Zero-filled buffers are safe (perception returns neutral results).
**Warning signs:** None at runtime if not caught -- perception just returns garbage.

### Pitfall 5: Perception Readback Timing
**What goes wrong:** readback_results() does a synchronous GPU->CPU copy with device.poll(wait). If called every frame, this stalls the GPU pipeline.
**Why it happens:** Perception readback is needed for step_reroute(), which runs on CPU.
**How to avoid:** Only readback when step_reroute needs data (every frame is fine for 1K-agent batches). The cost is O(agent_count * 32 bytes), acceptable for 280K agents = ~8.5MB. Consider async readback as optimization later.
**Warning signs:** Frame time spike from GPU stall.

### Pitfall 6: WGSL Shader Access to Perception Results
**What goes wrong:** HCMC behaviors (creep, gap acceptance) need perception_results in wave_front.wgsl, but perception uses a SEPARATE bind group layout from wave_front.
**Why it happens:** Phase 7 deliberately separated bind group layouts to avoid conflicts.
**How to avoid:** Two options: (1) Add perception_results as an additional binding to wave_front's bind group layout, or (2) Run perception BEFORE wave_front and pass results via a shared storage buffer. Option 1 is cleaner -- add a binding(8) or binding(9) for perception results read-only in wave_front.
**Warning signs:** Bind group layout mismatch errors.

### Pitfall 7: File Size Limit (700 lines)
**What goes wrong:** sim.rs is already 888 lines. Adding perception and reroute wiring will exceed 700 easily.
**Why it happens:** sim.rs contains all tick logic plus CPU reference code.
**How to avoid:** Extract new methods into separate files following the sim_reroute.rs pattern. Create sim_perception.rs and sim_startup.rs. Keep sim.rs under 700 by moving cpu_reference to its own file if needed.
**Warning signs:** File exceeding 700 lines at any point.

## Code Examples

### Example 1: Signal Config Deserialization
```rust
// Source: project pattern from VehicleConfig in velos-vehicle/src/config.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SignalConfig {
    #[serde(default)]
    pub intersection: Vec<IntersectionConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IntersectionConfig {
    pub node_id: u32,
    pub controller: String,  // "fixed", "actuated", "adaptive"
    #[serde(default = "default_min_green")]
    pub min_green: f64,
    #[serde(default = "default_max_green")]
    pub max_green: f64,
    #[serde(default = "default_gap_threshold")]
    pub gap_threshold: f64,
}

fn default_min_green() -> f64 { 7.0 }
fn default_max_green() -> f64 { 60.0 }
fn default_gap_threshold() -> f64 { 3.0 }

pub fn load_signal_config() -> SignalConfig {
    let path = std::env::var("VELOS_SIGNAL_CONFIG")
        .unwrap_or_else(|_| "data/hcmc/signal_config.toml".to_string());
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            log::warn!("Failed to parse signal config: {e}, using all fixed-time");
            SignalConfig { intersection: Vec::new() }
        }),
        Err(_) => {
            log::warn!("Signal config not found at {path}, using all fixed-time");
            SignalConfig { intersection: Vec::new() }
        }
    }
}
```

### Example 2: Polymorphic Controller Construction
```rust
// Source: project pattern from SimWorld::new() in sim.rs
fn build_signal_controller(
    plan: SignalPlan,
    num_approaches: usize,
    config: Option<&IntersectionConfig>,
) -> Box<dyn SignalController> {
    match config.map(|c| c.controller.as_str()) {
        Some("actuated") => {
            let cfg = config.unwrap();
            Box::new(ActuatedController::new_with_params(
                plan, num_approaches,
                cfg.min_green, cfg.max_green, cfg.gap_threshold,
            ))
        }
        Some("adaptive") => Box::new(AdaptiveController::new(plan, num_approaches)),
        _ => Box::new(FixedTimeController::new(plan, num_approaches)),
    }
}
```

### Example 3: Loop Detector Update
```rust
// Source: pattern from velos-signal/src/detector.rs
fn update_loop_detectors(
    &self,
    prev_positions: &HashMap<Entity, f64>,
) -> Vec<(NodeIndex, Vec<DetectorReading>)> {
    let mut readings = Vec::new();
    for (node, detectors) in &self.loop_detectors {
        let mut node_readings: Vec<DetectorReading> = detectors
            .iter()
            .enumerate()
            .map(|(i, det)| {
                let triggered = /* scan agents on det.edge_id for crossings */;
                DetectorReading { detector_index: i, triggered }
            })
            .collect();
        readings.push((*node, node_readings));
    }
    readings
}
```

### Example 4: WGSL Red-Light Creep
```wgsl
// Source: CPU reference in velos-vehicle/src/sublane.rs, adapted for WGSL
const CREEP_MAX_SPEED: f32 = 0.3;
const CREEP_DISTANCE_SCALE: f32 = 5.0;
const CREEP_MIN_DISTANCE: f32 = 0.5;

fn red_light_creep_speed(distance_to_stop: f32, vehicle_type: u32) -> f32 {
    // Only motorbikes and bicycles creep
    if vehicle_type != VT_MOTORBIKE && vehicle_type != VT_BICYCLE {
        return 0.0;
    }
    if distance_to_stop < CREEP_MIN_DISTANCE {
        return 0.0;
    }
    let ramp = min(distance_to_stop / CREEP_DISTANCE_SCALE, 1.0);
    return CREEP_MAX_SPEED * ramp;
}
```

### Example 5: WGSL Gap Acceptance
```wgsl
// Source: CPU reference in velos-vehicle/src/intersection.rs, adapted for WGSL
const MAX_WAIT_TIME: f32 = 5.0;
const FORCED_ACCEPTANCE_FACTOR: f32 = 0.5;
const WAIT_REDUCTION_RATE: f32 = 0.1;

fn size_factor(approaching_type: u32) -> f32 {
    switch approaching_type {
        case VT_TRUCK, VT_BUS: { return 1.3; }
        case VT_EMERGENCY: { return 2.0; }
        case VT_MOTORBIKE, VT_BICYCLE: { return 0.8; }
        case VT_PEDESTRIAN: { return 0.5; }
        default: { return 1.0; }
    }
}

fn intersection_gap_acceptance(
    other_type: u32, ttc: f32, ttc_threshold: f32, wait_time: f32,
) -> bool {
    let sf = size_factor(other_type);
    var wait_mod: f32;
    if wait_time >= MAX_WAIT_TIME {
        wait_mod = FORCED_ACCEPTANCE_FACTOR;
    } else {
        wait_mod = 1.0 - WAIT_REDUCTION_RATE * min(wait_time, MAX_WAIT_TIME);
    }
    let effective = ttc_threshold * sf * wait_mod;
    return ttc > effective;
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| All FixedTimeController | Polymorphic SignalController trait | Phase 6 (06-03) | Trait exists but not used polymorphically yet |
| Empty RerouteState | init_reroute() + step_reroute() | Phase 7 (07-06) | Methods exist but not called |
| No perception | PerceptionPipeline full impl | Phase 7 (07-05) | Pipeline exists but not dispatched |
| Hardcoded WGSL constants | vehicle_params uniform buffer | Phase 8 (08-02) | upload_vehicle_params() exists but not called at startup |
| No sign interaction | handle_sign_interaction() in WGSL | Phase 6 (06-05) | Shader reads signs buffer but buffer not populated at startup |

## Open Questions

1. **Perception result buffer as wave_front input**
   - What we know: Perception writes to its own result_buffer. HCMC behaviors in wave_front.wgsl need to read it.
   - What's unclear: wave_front currently has 8 bindings (0-7). Adding perception_results requires binding 8 or a second bind group.
   - Recommendation: Add binding 8 (read-only storage) to wave_front bind group layout for perception_results. This means WaveFrontParams must be updated and bind group recreation. Verify wgpu max bindings per group (default limit is 16, so binding 8 is safe).

2. **Signal buffer for perception**
   - What we know: PerceptionPipeline needs a signal_buffer input. wave_front.wgsl doesn't currently have a signal state buffer.
   - What's unclear: Where signal states get uploaded to GPU for perception to read.
   - Recommendation: Create a signal_state_buffer in ComputeDispatcher (or SimWorld) that stores per-edge signal state (u32: 0=green, 1=amber, 2=red, 3=none). Updated each frame after step_signals(). This is input to perception AND can feed into wave_front for direct signal awareness.

3. **Congestion grid and edge travel ratio buffers**
   - What we know: PerceptionPipeline needs congestion_grid_buffer and edge_travel_ratio_buffer.
   - What's unclear: Where these get computed and stored.
   - Recommendation: Create placeholder zero-filled buffers at startup. Populate edge_travel_ratio from PredictionService overlay (already has edge_travel_times). Congestion grid can be computed from agent density (simple aggregation) or left as zeros initially.

4. **CCH failure strategy**
   - What we know: init_reroute() already handles CCH build failure by setting cch_router to None. step_reroute() early-returns if None.
   - Recommendation: Keep Option<CCHRouter> (already implemented). Log error at startup, sim runs without rerouting. This matches the graceful degradation pattern and avoids a hard startup failure for non-critical functionality.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in Rust test framework) |
| Config file | Cargo.toml workspace test settings |
| Quick run command | `cargo test --workspace -q` |
| Full suite command | `cargo test --workspace --no-fail-fast` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SIG-01 | Actuated controller with detector input | unit | `cargo test -p velos-signal -- actuated -q` | YES |
| SIG-02 | Adaptive controller with queue lengths | unit | `cargo test -p velos-signal -- adaptive -q` | YES |
| SIG-03 | SPaT broadcast data generation | unit | `cargo test -p velos-signal -- spat -q` | YES |
| SIG-04 | Priority request handling | unit | `cargo test -p velos-signal -- priority -q` | YES |
| SIG-05 | Sign interaction in GPU shader | unit | `cargo test -p velos-gpu -- sign -q` | Partial (signs test exists, integration test needed) |
| INT-03 | Perception pipeline dispatch | unit | `cargo test -p velos-gpu -- perception -q` | YES (size/alignment tests) |
| INT-04 | Reroute evaluation logic | unit | `cargo test -p velos-core -- reroute -q` | YES |
| INT-05 | Staggered reroute scheduling | unit | `cargo test -p velos-core -- reroute -q` | YES |
| RTE-03 | Dynamic rerouting via CCH | unit | `cargo test -p velos-gpu -- reroute -q` | YES |
| RTE-07 | Prediction overlay in routing cost | unit | `cargo test -p velos-predict -q` | YES |
| TUN-02 | GPU vehicle params upload | unit | `cargo test -p velos-gpu -- vehicle_params -q` | YES (struct tests) |
| TUN-04 | Red-light creep speed | unit | `cargo test -p velos-vehicle -- creep -q` | YES |
| TUN-06 | Intersection gap acceptance | unit | `cargo test -p velos-vehicle -- intersection -q` | YES |

### Sampling Rate
- **Per task commit:** `cargo clippy --all-targets --all-features -- -D warnings && cargo test --workspace -q`
- **Per wave merge:** `cargo test --workspace --no-fail-fast`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-gpu/tests/integration_startup.rs` -- tests SimWorld::new() initializes all subsystems
- [ ] `crates/velos-gpu/tests/integration_frame_pipeline.rs` -- tests tick_gpu() calls perception + reroute in correct order
- [ ] `crates/velos-gpu/src/sim_perception.rs` -- new module for perception wiring (extracted from sim.rs)
- [ ] `crates/velos-gpu/src/sim_startup.rs` -- new module for startup initialization (extracted from sim.rs)
- [ ] `data/hcmc/signal_config.toml` -- signal controller configuration file
- [ ] WGSL shader validation: `naga --validate crates/velos-gpu/shaders/wave_front.wgsl` after adding HCMC behavior functions

## Sources

### Primary (HIGH confidence)
- Direct codebase inspection: sim.rs, app.rs, sim_reroute.rs, perception.rs, compute.rs, wave_front.wgsl
- velos-signal crate: lib.rs (trait definition), actuated.rs, adaptive.rs, detector.rs, signs.rs
- velos-vehicle crate: sublane.rs (red_light_creep_speed), intersection.rs (intersection_gap_acceptance), config.rs

### Secondary (MEDIUM confidence)
- wgpu binding limits: default max_storage_buffers_per_shader_stage is 8 in wgpu Limits::default(). Binding 8 may exceed this. Need to check and potentially use Limits::downlevel_defaults() or request higher limit.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, no new deps
- Architecture: HIGH -- all integration points inspected in source code, patterns well-established
- Pitfalls: HIGH -- identified from actual code review (borrow checker, file size, buffer deps)
- WGSL integration: MEDIUM -- binding limit concern for perception_results in wave_front needs verification

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (stable, internal integration phase)
