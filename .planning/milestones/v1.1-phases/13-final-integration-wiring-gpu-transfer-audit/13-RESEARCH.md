# Phase 13: Final Integration Wiring & GPU Transfer Audit - Research

**Researched:** 2026-03-08
**Domain:** Integration wiring, GPU buffer optimization, CPU/GPU tick parity
**Confidence:** HIGH

## Summary

Phase 13 closes the final 4 unsatisfied v1.1 requirements (INT-01, INT-02, SIG-03, AGT-04) by wiring existing tested code into production paths. All building blocks are already implemented and tested in isolation -- the gap is purely wiring and optimization. No new algorithms, data structures, or external libraries are needed.

The work falls into three categories: (1) agent profile and cost function wiring into spawn and reroute paths, (2) GLOSA/SPaT advisory speed consumption by agent driving behavior, (3) GPU pedestrian pipeline activation replacing CPU fallback, plus cleanup (CPU tick parity, dirty-flag buffer uploads, dead code removal).

**Primary recommendation:** This phase is pure integration wiring -- connect existing tested modules, add dirty flags, remove dead code. No new crates or dependencies needed.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| INT-01 | Multi-factor pathfinding cost function: time, comfort, safety, fuel, signal delay, prediction penalty | Cost function exists in `velos-core/src/cost.rs` with `route_cost()`, `PROFILE_WEIGHTS`, `EdgeAttributes`. Already consumed by `sim_reroute.rs`. Gap: spawn path doesn't encode profile in flags, so reroute always decodes Commuter (profile bits are 0000). |
| INT-02 | Configurable agent profiles (Commuter, Bus, Truck, Emergency, Tourist, Teen, Senior, Cyclist) with per-profile cost weights | `AgentProfile` enum, `PROFILE_WEIGHTS` table, `encode_profile_in_flags()`/`decode_profile_from_flags()` all exist. `SpawnRequest` already carries `profile` field from `assign_profile()`. Gap: `spawn_single_agent()` never calls `encode_profile_in_flags()` -- all agents get flags=0 (Commuter). |
| SIG-03 | SPaT broadcast to agents within range for signal-aware driving | `SpatBroadcast`, `glosa_speed()`, `broadcast_range_m()` exist in `velos-signal/src/spat.rs`. Gap: No code in the tick pipeline consumes SPaT data to modify agent speed targets. |
| AGT-04 | Pedestrian adaptive GPU workgroups with prefix-sum compaction (3-8x speedup) | `PedestrianAdaptivePipeline` with full 6-dispatch pipeline exists in `velos-gpu/src/ped_adaptive.rs`. Gap: `step_pedestrians()` in `sim_pedestrians.rs` uses CPU social force exclusively -- never dispatches the GPU pipeline. |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| velos-core | workspace | AgentProfile, CostWeights, encode/decode flags | Already implemented |
| velos-signal | workspace | SpatBroadcast, glosa_speed, broadcast_range_m | Already implemented |
| velos-gpu | workspace | PedestrianAdaptivePipeline, ComputeDispatcher | Already implemented |
| velos-demand | workspace | SpawnRequest.profile, assign_profile | Already implemented |

### Supporting
No new dependencies required. All building blocks exist.

## Architecture Patterns

### Recommended Project Structure
No new files needed. Changes are to existing files:
```
crates/velos-gpu/src/
  sim_lifecycle.rs     # Wire profile into spawn flags
  sim.rs               # Add step_glosa(), fix CPU tick() parity
  sim_pedestrians.rs   # Replace CPU path with GPU dispatch option
  sim_perception.rs    # Add dirty flags to signal/edge buffers
  compute.rs           # Remove unused acceleration field
  sim_helpers.rs       # GLOSA speed integration (new method or existing)
crates/velos-core/src/
  components.rs        # Remove acceleration from GpuAgentState if unused
```

### Pattern 1: Profile Flag Encoding at Spawn
**What:** Encode agent profile bits into GpuAgentState.flags at spawn time.
**When to use:** When creating a new agent entity in spawn_single_agent().
**Example:**
```rust
// In spawn_single_agent(), after computing flags:
use velos_core::cost::{encode_profile_in_flags, AgentProfile};

// Map SpawnRequest.profile to the profile enum (already same type)
let flags = compute_agent_flags(is_dwelling, is_emergency);
let flags = encode_profile_in_flags(flags, req.profile);
```
**Impact:** The reroute path in sim_reroute.rs already calls `decode_profile_from_flags(flags)` and uses `PROFILE_WEIGHTS[profile as usize]` -- so once flags carry the profile, cost-weighted rerouting activates automatically.

### Pattern 2: GLOSA Advisory Speed in Tick Pipeline
**What:** After signal controllers tick, compute SPaT broadcast and GLOSA speed for nearby agents, adjust their speed targets.
**When to use:** Between step_signals and step_vehicles in the tick pipeline.
**Example:**
```rust
// New method: step_glosa()
fn step_glosa(&mut self) {
    for (node, ctrl) in &self.signal_controllers {
        let spat = ctrl.spat_broadcast(); // Need to add this trait method
        let incoming_edges = /* edges approaching node */;
        for (entity, rp, kin, vtype) in /* agents on incoming_edges within 200m */ {
            let distance = edge_length - rp.offset_m;
            if distance > broadcast_range_m() { continue; }
            let advisory = glosa_speed(distance, spat.time_to_next_change, kin.v_max);
            if advisory > 0.0 {
                // Adjust desired speed -- could set a GlosaAdvisory component
                // or directly cap kin.speed target
            }
        }
    }
}
```
**Decision needed:** How GLOSA modifies behavior. Options:
1. Temporary ECS component `GlosaAdvisory { speed: f64 }` read by GPU/CPU car-following
2. Direct speed clamping in the tick loop (simpler, less clean)
3. Modify existing perception pipeline to include GLOSA (already has signal_distance)

**Recommendation:** Option 2 (direct speed reduction) is simplest for v1.1. The perception pipeline already provides signal state and distance to the GPU shader. GLOSA is essentially "use that signal info to compute an approach speed on CPU and apply it before GPU dispatch." A simple ECS component or direct integration is sufficient.

### Pattern 3: GPU Pedestrian Dispatch Activation
**What:** Replace CPU social force with PedestrianAdaptivePipeline dispatch.
**When to use:** In step_pedestrians() when GPU device is available.
**Example:**
```rust
// In step_pedestrians(), check if GPU pipeline available
pub fn step_pedestrians_gpu(
    &mut self, dt: f64,
    device: &wgpu::Device, queue: &wgpu::Queue,
) {
    // Collect pedestrians into GpuPedestrian array
    let gpu_peds: Vec<GpuPedestrian> = /* map from ECS */;
    let (grid_w, grid_h) = compute_grid_dims(&gpu_peds);

    self.ped_adaptive.upload(device, queue, &gpu_peds, grid_w, grid_h);
    let params = PedestrianAdaptiveParams { dt: dt as f32, ..defaults };

    let mut encoder = device.create_command_encoder(&Default::default());
    self.ped_adaptive.dispatch(&mut encoder, device, queue, &params);
    queue.submit(std::iter::once(encoder.finish()));

    let updated = self.ped_adaptive.readback(device, queue);
    // Write back to ECS
}
```
**Key detail:** PedestrianAdaptivePipeline needs to be owned by SimWorld (currently not stored there). Add field `ped_adaptive: Option<PedestrianAdaptivePipeline>`.

### Pattern 4: Dirty-Flag Buffer Upload
**What:** Track whether buffer content has changed, skip GPU upload when unchanged.
**When to use:** For signal_buffer (changes only on phase transitions) and edge_travel_ratio_buffer (changes only on prediction updates).
**Example:**
```rust
// Add to SimWorld or PerceptionBuffers:
signal_dirty: bool,
prediction_dirty: bool,

// In update_signal_buffer():
fn update_signal_buffer(&mut self, queue: &wgpu::Queue) {
    if !self.signal_dirty { return; }
    // ... existing upload logic ...
    self.signal_dirty = false;
}

// Set dirty when signals change phase:
// In step_signals_with_detectors(), check if any phase changed
```

### Anti-Patterns to Avoid
- **Adding new GPU bindings for GLOSA:** The perception pipeline already provides signal_state and signal_distance. Use existing data rather than adding new buffer bindings.
- **Keeping CPU fallback as default:** The PedestrianAdaptivePipeline should be the production path in tick_gpu(). CPU social force remains only in tick() (CPU fallback).
- **Removing acceleration field from GPU struct without updating WGSL:** GpuAgentState.acceleration is referenced in wave_front.wgsl even if always 0. Coordinate removal across Rust struct and WGSL shader.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Profile assignment | Custom assignment logic | `assign_profile()` in velos-demand | Already distributes profiles per vehicle type |
| GLOSA speed | Custom approach speed calc | `glosa_speed()` in velos-signal | Handles edge cases (too slow, too fast, already green) |
| Cost-weighted routing | Custom cost function | `route_cost()` in velos-core | 6-factor cost with distance-weighted prediction blend |
| GPU pedestrian social force | CPU N^2 loop | `PedestrianAdaptivePipeline` | 6-dispatch prefix-sum compaction, 3-8x speedup |

**Key insight:** Every building block for Phase 13 already exists as tested code. The work is exclusively wiring and optimization -- no new algorithms.

## Common Pitfalls

### Pitfall 1: GpuAgentState Flags Bit Collision
**What goes wrong:** Profile bits (4-7) could overwrite or be overwritten by compute_agent_flags().
**Why it happens:** compute_agent_flags() only sets bits 0-1 (bus_dwelling, emergency). But if someone adds bit 2-3 flags later without knowing about profile encoding, bits collide.
**How to avoid:** Always use encode_profile_in_flags() which preserves bits 0-3. Document the full flag layout:
- bit 0: FLAG_BUS_DWELLING
- bit 1: FLAG_EMERGENCY_ACTIVE
- bit 2: yielding (reserved)
- bit 3: reserved
- bits 4-7: AgentProfile (0-7)
**Warning signs:** Profile always decodes as Commuter (0) even for non-default agents.

### Pitfall 2: CPU Tick Parity Gap
**What goes wrong:** CPU tick() path doesn't call step_lane_changes(dt), so CPU tests don't exercise MOBIL lane changes.
**Why it happens:** step_lane_changes was added in Phase 12 to tick_gpu() but wasn't added to tick().
**How to avoid:** Add step_lane_changes(dt) between step_meso() and step_vehicles in the CPU tick() path.
**Warning signs:** CPU-only integration tests pass but lane change behavior differs from GPU path.

### Pitfall 3: Pedestrian GPU Pipeline Needs SimWorld Storage
**What goes wrong:** PedestrianAdaptivePipeline exists in ped_adaptive.rs but is not stored on SimWorld.
**Why it happens:** It was designed as a standalone pipeline, never wired into the sim loop.
**How to avoid:** Add `ped_adaptive: Option<PedestrianAdaptivePipeline>` to SimWorld, initialize in SimWorld::new().
**Warning signs:** step_pedestrians still runs CPU-only even in tick_gpu() path.

### Pitfall 4: Signal Dirty Flag Must Track Phase Transitions
**What goes wrong:** Setting signal_dirty = true every frame defeats the optimization.
**Why it happens:** The simplistic approach flags dirty in step_signals_with_detectors() which runs every frame.
**How to avoid:** Compare signal phase before/after tick. Only mark dirty when a phase actually changes. Fixed-time controllers transition at known intervals; actuated controllers transition on gap-out.
**Warning signs:** GPU profiling shows signal buffer upload still happening every frame.

### Pitfall 5: GLOSA Requires SignalController Trait Extension
**What goes wrong:** Cannot get SPaT data from signal controllers.
**Why it happens:** The `SignalController` trait doesn't have a `spat_broadcast()` method. `SpatBroadcast` is a struct but no controller produces it.
**How to avoid:** Either add `spat_broadcast()` to the trait or compute SPaT externally from `get_phase_state()` + controller timing info. The simpler approach: compute GLOSA directly from the signal state already uploaded to the perception buffer (signal_states[edge_id] gives phase, time_to_change can be derived from controller).
**Warning signs:** Have to access private controller fields to get timing.

### Pitfall 6: Removing GpuAgentState.acceleration Requires WGSL Sync
**What goes wrong:** Rust struct and WGSL struct get out of sync, causing buffer layout corruption.
**Why it happens:** GpuAgentState is #[repr(C)] and matched byte-for-byte with WGSL struct. Removing a field shifts all subsequent field offsets.
**How to avoid:** Remove from both Rust struct AND WGSL struct simultaneously. Run naga validation tests. Verify sizeof matches (currently 40 bytes, would become 36 bytes -- check GPU alignment requirements).
**Warning signs:** GPU agents have garbage data after the change.

## Code Examples

### Wiring Profile into Spawn Flags
```rust
// In sim_lifecycle.rs spawn_single_agent():
// After line: flags: compute_agent_flags(is_dwelling, is_emergency),
// Change to:
use velos_core::cost::encode_profile_in_flags;

let base_flags = compute_agent_flags(is_dwelling, is_emergency);
let flags = encode_profile_in_flags(base_flags, req.profile);
```

### CPU Tick Parity Fix
```rust
// In sim.rs tick(), between step_meso(dt) and step_vehicles:
// Add:
self.step_lane_changes(dt);
```

### GLOSA Integration (Simplified Approach)
```rust
// New method on SimWorld, called between step_signals and step_perception:
fn step_glosa(&mut self) {
    use velos_signal::spat::{broadcast_range_m, glosa_speed};

    let range = broadcast_range_m();
    let g = self.road_graph.inner();

    // Collect GLOSA advisories
    struct GlosaUpdate { entity: Entity, advisory_speed: f64 }
    let mut updates = Vec::new();

    for (node, ctrl) in &self.signal_controllers {
        let incoming: Vec<_> = g.edges_directed(*node, petgraph::Direction::Incoming).collect();

        for (approach_idx, edge_ref) in incoming.iter().enumerate() {
            let phase_state = ctrl.get_phase_state(approach_idx);
            if phase_state == velos_signal::plan::PhaseState::Green {
                continue; // Already green, no advisory needed
            }

            let time_to_green = ctrl.time_to_next_green(approach_idx);
            let edge_id = edge_ref.id().index() as u32;
            let edge_length = g[edge_ref.id()].length_m;

            // Find agents on this edge within broadcast range
            for (entity, rp, kin) in self.world.query_mut::<(Entity, &RoadPosition, &Kinematics)>() {
                if rp.edge_index != edge_id { continue; }
                let distance = edge_length - rp.offset_m;
                if distance > range { continue; }

                let v_max = kin.speed.max(13.89); // Use current or default max
                let advisory = glosa_speed(distance, time_to_green, v_max);
                if advisory > 0.0 && advisory < kin.speed {
                    updates.push(GlosaUpdate { entity, advisory_speed: advisory });
                }
            }
        }
    }

    // Apply advisories
    for upd in &updates {
        if let Ok(kin) = self.world.query_one_mut::<&mut Kinematics>(upd.entity) {
            kin.speed = kin.speed.min(upd.advisory_speed);
        }
    }
}
```
**Note:** This requires `time_to_next_green(approach_idx)` on SignalController trait. Alternative: use the remaining time from phase timing directly.

### Dirty Flag for Signal Buffer
```rust
// In SimWorld, add fields:
pub(crate) signal_dirty: bool,
pub(crate) prediction_dirty: bool,

// In step_signals_with_detectors(), after ctrl.tick():
let old_state = ctrl.get_phase_state(0);
ctrl.tick(dt, readings);
let new_state = ctrl.get_phase_state(0);
if old_state != new_state {
    self.signal_dirty = true;
}

// In update_signal_buffer():
if !self.signal_dirty { return; }
// ... existing upload ...
self.signal_dirty = false;
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| CPU pedestrian social force | GPU adaptive workgroups | Phase 6 (implemented) | 3-8x speedup, not yet activated |
| All agents Commuter profile | Per-agent profile encoding | Phase 7 (cost.rs), Phase 9 (demand) | Diverse route choice, not yet wired |
| SPaT data unused | GLOSA advisory speed | Phase 6 (spat.rs) | Signal-aware driving, not yet consumed |
| Every-frame buffer upload | Dirty-flag conditional upload | Not yet | Reduces GPU transfer overhead |

## Open Questions

1. **GLOSA: How to get time_to_next_green from SignalController?**
   - What we know: SignalController trait has `get_phase_state(approach)` and `tick()`. SpatBroadcast has `time_to_next_change`.
   - What's unclear: Whether SignalController exposes remaining phase time. Fixed-time controllers have deterministic timing; actuated controllers have variable timing.
   - Recommendation: Add `fn time_to_next_change(&self) -> f64` to SignalController trait. All three implementations (fixed, actuated, adaptive) can compute this from their internal state. Alternatively, derive from phase duration for fixed-time and use a best-estimate for actuated.

2. **GpuAgentState.acceleration removal: alignment implications**
   - What we know: Current size is 40 bytes (10 x u32). Removing acceleration makes it 36 bytes.
   - What's unclear: Whether 36 bytes causes alignment issues on some GPU backends (Metal prefers 16-byte aligned, WebGPU spec requires 4-byte alignment for storage buffers).
   - Recommendation: Either remove and pad to keep 40 bytes (rename _pad), or keep the field but document it as unused. The cleanup is cosmetic -- if alignment risk exists, defer removal.

3. **Congestion grid buffer: is it actually unused?**
   - What we know: `congestion_grid_buffer` in PerceptionBuffers is created and uploaded to perception pipeline. The perception shader reads it for `congestion_area` output.
   - What's unclear: The success criteria says "remove unused congestion grid buffer" but it IS used in perception. Need to verify whether the data written to it is meaningful (currently zeroed).
   - Recommendation: Verify whether congestion grid data is ever populated with real values. If always zero, remove the buffer and hardcode congestion_area=0.0 in perception shader.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Cargo.toml workspace |
| Quick run command | `cargo test -p velos-gpu --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| INT-01 | route_cost uses profile weights in reroute | unit | `cargo test -p velos-core -- cost::tests` | Existing tests verify cost function; need integration test for reroute with non-Commuter |
| INT-02 | spawn_single_agent encodes profile in flags | unit | `cargo test -p velos-gpu -- sim_lifecycle` | Needs new test |
| SIG-03 | GLOSA advisory speed modifies agent behavior | unit | `cargo test -p velos-gpu -- sim::tests::glosa` | Needs new test |
| AGT-04 | GPU pedestrian dispatch replaces CPU | integration | `cargo test -p velos-gpu -- sim_pedestrians` | Needs new test |
| N/A | CPU tick calls step_lane_changes | unit | `cargo test -p velos-gpu -- sim::tests::cpu_tick_parity` | Needs new test |
| N/A | Signal buffer uses dirty flag | unit | `cargo test -p velos-gpu -- sim_perception::tests::signal_dirty` | Needs new test |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-gpu --lib && cargo test -p velos-core --lib`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] Test for profile encoding in spawn path (sim_lifecycle.rs)
- [ ] Test for GLOSA speed integration (sim.rs or new sim_glosa.rs)
- [ ] Test for GPU pedestrian dispatch activation (sim_pedestrians.rs)
- [ ] Test for CPU tick parity with step_lane_changes (sim.rs)
- [ ] Test for dirty-flag signal buffer optimization (sim_perception.rs)

## Sources

### Primary (HIGH confidence)
- Direct codebase inspection: sim.rs, sim_lifecycle.rs, sim_reroute.rs, sim_perception.rs, sim_pedestrians.rs, sim_mobil.rs
- Direct codebase inspection: cost.rs (AgentProfile, CostWeights, encode/decode), spat.rs (glosa_speed, SpatBroadcast)
- Direct codebase inspection: ped_adaptive.rs (PedestrianAdaptivePipeline, GpuPedestrian)
- Direct codebase inspection: compute.rs (ComputeDispatcher, compute_agent_flags)
- Direct codebase inspection: spawner.rs (SpawnRequest.profile, assign_profile)

### Secondary (MEDIUM confidence)
- Architecture docs: 02-agent-models.md (pedestrian adaptive workgroups spec)
- Architecture docs: 06-05 decisions (GLOSA minimum practical speed 3.0 m/s)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all code exists, no new dependencies
- Architecture: HIGH - wiring patterns are straightforward, follow existing code conventions
- Pitfalls: HIGH - identified from direct code inspection of actual gap locations
- GLOSA integration: MEDIUM - SignalController trait may need extension, exact approach TBD

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (stable -- internal codebase, no external dependency changes)
