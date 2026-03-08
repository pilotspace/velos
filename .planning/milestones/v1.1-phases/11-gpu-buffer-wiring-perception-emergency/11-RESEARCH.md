# Phase 11: GPU Buffer Wiring -- Perception & Emergency - Research

**Researched:** 2026-03-08
**Domain:** GPU buffer wiring, WGSL shader integration, Rust-GPU data path
**Confidence:** HIGH

## Summary

Phase 11 closes the last integration gaps in the simulation loop. The WGSL shaders (`wave_front.wgsl`, `perception.wgsl`) already contain all the behavior code -- red-light creep, gap acceptance, emergency yield cone -- but the Rust host code never connects the perception pipeline's output buffer to the wave-front shader's input, and never uploads emergency vehicle positions each frame. The result: binding(8) contains zeroed data (all perception fields are 0.0), and `emergency_count` is always 0 in `WaveFrontParams`, causing the yield cone to early-exit every frame.

This is strictly a wiring phase: no new algorithms, no new shaders, no new data structures. The work is connecting existing pieces in `SimWorld::new()` and `SimWorld::tick_gpu()`.

**Primary recommendation:** Wire 3 call sites: (1) `set_perception_result_buffer()` in `SimWorld::new()`, (2) `upload_emergency_vehicles()` in `step_vehicles_gpu()`, (3) set `FLAG_EMERGENCY_ACTIVE` on emergency vehicles when building GPU agent state.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| TUN-04 | Red-light creep behavior for motorbikes | WGSL `red_light_creep_speed()` exists and reads `perc.signal_state` from binding(8). Currently reads zeroes because perception buffer is placeholder. Wiring `set_perception_result_buffer()` fixes this. |
| TUN-06 | Yield-based intersection gap acceptance | WGSL `intersection_gap_acceptance()` reads `perc.leader_speed`, `perc.leader_gap`, `perc.signal_state` from binding(8). Same zeroed-buffer issue as TUN-04. Same fix. |
| INT-03 | GPU perception phase | PerceptionPipeline exists and runs (`step_perception()`), but its `result_buffer` is never shared with ComputeDispatcher. The perception shader writes to its own buffer; wave_front reads from a separate zeroed placeholder. |
| AGT-08 | Emergency vehicle priority and yield behavior | `check_emergency_yield()` in WGSL works correctly but `emergency_count` is always 0 (early-exit). `upload_emergency_vehicles()` exists but is never called. Also, `FLAG_EMERGENCY_ACTIVE` (bit 1) is never set on GPU agent state -- flags only sets bit 0 for bus dwelling. |
</phase_requirements>

## Architecture Patterns

### Current Data Flow (Broken)

```
PerceptionPipeline::new() -> creates result_buffer (STORAGE | COPY_SRC)
ComputeDispatcher::new() -> creates perception_result_buffer (STORAGE | COPY_DST, zeroed)

step_perception() -> dispatches perception.wgsl -> writes to PerceptionPipeline.result_buffer
dispatch_wave_front() -> reads from ComputeDispatcher.perception_result_buffer (ZEROED!)

Result: Two separate buffers. Perception writes to one, wave_front reads zeroes from other.
```

### Required Data Flow (Fixed)

```
SimWorld::new():
  1. Create PerceptionPipeline (already done, line 193)
  2. Call dispatcher.set_perception_result_buffer(perception.result_buffer().clone())  <-- NEW
     This replaces the zeroed placeholder with the real buffer.

tick_gpu() -> step_vehicles_gpu():
  3. Before upload_wave_front_data(), collect emergency vehicles and call
     dispatcher.upload_emergency_vehicles(queue, &emergency_list)  <-- NEW
  4. When building GpuAgentState, set flags bit 1 for Emergency vehicles  <-- NEW
```

### Buffer Binding Map (wave_front.wgsl)

| Binding | Buffer | Source | Status |
|---------|--------|--------|--------|
| 0 | WaveFrontParams (uniform) | ComputeDispatcher | Working |
| 1 | agents (read_write) | ComputeDispatcher.upload_wave_front_data | Working |
| 2 | lane_offsets (read) | ComputeDispatcher.upload_wave_front_data | Working |
| 3 | lane_counts (read) | ComputeDispatcher.upload_wave_front_data | Working |
| 4 | lane_agents (read) | ComputeDispatcher.upload_wave_front_data | Working |
| 5 | emergency_vehicles (read) | upload_emergency_vehicles | **NEVER CALLED** |
| 6 | signs (read) | upload_signs | Working |
| 7 | vehicle_params (uniform) | upload_vehicle_params | Working |
| 8 | perception_results (read) | set_perception_result_buffer | **NEVER CALLED** |

### Three Integration Gaps

**Gap 1: Perception buffer not shared (binding 8)**

`ComputeDispatcher` has `set_perception_result_buffer()` (compute.rs:425) which replaces its internal zeroed placeholder buffer with an external buffer. `PerceptionPipeline` has `result_buffer()` (perception.rs:290-292) which returns a reference to its output buffer. These two are never connected.

The fix: In `SimWorld::new()`, after creating the PerceptionPipeline (line 193), the result buffer must be shared. However, `set_perception_result_buffer()` takes ownership (`wgpu::Buffer`, not `&wgpu::Buffer`), so the PerceptionPipeline cannot simply give away its buffer. Two options:
- **Option A (recommended):** Have PerceptionPipeline accept the buffer from ComputeDispatcher at construction instead of creating its own. Both use the same buffer.
- **Option B:** Create the perception result buffer once in ComputeDispatcher and pass it to PerceptionPipeline during `create_bind_group()` (it already accepts external buffers via `PerceptionBindings`).

Wait -- looking more carefully at the code: `PerceptionPipeline.result_buffer` has `STORAGE | COPY_SRC` usage, but `ComputeDispatcher.perception_result_buffer` has `STORAGE | COPY_DST`. They need **different usage flags** because:
- Perception pipeline writes to it (STORAGE read_write) and copies from it for staging readback (COPY_SRC)
- Wave-front pipeline reads from it (STORAGE read)

Both `STORAGE | COPY_SRC` covers both needs. The ComputeDispatcher's placeholder has `STORAGE | COPY_DST` which is wrong for the perception pipeline's writes (it uses `read_write` storage, not `COPY_DST`). The real PerceptionPipeline buffer (`STORAGE | COPY_SRC`) is the correct one to share.

**Actual fix:** Create one buffer with `STORAGE | COPY_SRC` usage (PerceptionPipeline's buffer). Pass it to ComputeDispatcher via `set_perception_result_buffer()`. But `set_perception_result_buffer` takes `wgpu::Buffer` (owned). The PerceptionPipeline cannot give away its buffer without breaking itself.

**Real solution:** Create the result buffer externally (in SimWorld::new), give a clone/reference to both. But `wgpu::Buffer` is not `Clone`. The actual pattern should be: create the buffer once, pass ownership to one component, and share a reference. Looking at the existing code, the PerceptionPipeline's `create_bind_group()` already takes external buffers for everything except the result buffer (binding 7). The result buffer is internal.

**Simplest fix:** Change `PerceptionPipeline::new()` to accept an external result buffer, OR pass the ComputeDispatcher's perception_result_buffer into the PerceptionPipeline. Since the ComputeDispatcher buffer has wrong usage flags (`COPY_DST` instead of `COPY_SRC`), we should create the buffer with combined flags: `STORAGE | COPY_SRC | COPY_DST`.

Actually, the cleanest approach: create the perception result buffer in `SimWorld::new()` with the correct combined usage flags (`STORAGE | COPY_SRC | COPY_DST`), give it to ComputeDispatcher via `set_perception_result_buffer()`, and pass it to PerceptionPipeline's bind group and readback as a reference. The PerceptionPipeline already separates buffer creation from usage in its bind group methods.

Let me re-examine: PerceptionPipeline's `dispatch()` writes to `self.result_buffer` via shader (storage read_write binding 7), and `readback_results()` copies from `self.result_buffer` to staging. So the result buffer needs `STORAGE | COPY_SRC`. ComputeDispatcher's wave_front just reads it as storage (read-only binding 8), which requires `STORAGE`. So `STORAGE | COPY_SRC` is sufficient for both.

The fix:
1. Change `ComputeDispatcher::perception_result_buffer` to have `STORAGE | COPY_SRC` usage (matching PerceptionPipeline's buffer)
2. In `SimWorld::new()`, after creating PerceptionPipeline, take its result buffer and give it to ComputeDispatcher
3. PerceptionPipeline needs to be modified to use the shared buffer (or expose its buffer for transfer)

**Recommended approach:** Modify PerceptionPipeline to take an optional external result buffer, OR restructure so the buffer is created once and shared. The simplest: PerceptionPipeline already has `result_buffer()` returning `&wgpu::Buffer`. Add a method to take ownership of the buffer out, or better yet, make PerceptionPipeline accept the buffer at creation time.

**Gap 2: Emergency vehicles never uploaded (binding 5)**

`upload_emergency_vehicles()` exists in ComputeDispatcher but is never called. In `step_vehicles_gpu()`, emergency vehicles with `VehicleType::Emergency` are included in the `gpu_agents` vec but:
- `FLAG_EMERGENCY_ACTIVE` (bit 1) is never set in the flags field
- Their positions are never collected and uploaded to the emergency buffer

The fix in `step_vehicles_gpu()`:
1. While building `gpu_agents`, set `flags |= 2` (FLAG_EMERGENCY_ACTIVE) for Emergency vehicles
2. Collect emergency vehicle positions into `Vec<GpuEmergencyVehicle>`
3. Call `dispatcher.upload_emergency_vehicles(queue, &emergency_list)` before dispatch

**Gap 3: Emergency vehicle heading computation**

`GpuEmergencyVehicle` requires `pos_x`, `pos_y`, `heading`. The GPU yield cone uses Euclidean (x, y) coordinates. Currently, agent positions are stored as `(edge_id, lane_idx, position_along_edge)` -- NOT Euclidean. The emergency vehicle buffer needs world-space (x, y, heading).

This requires converting from road-relative to world coordinates. The road graph has geometry (polyline) per edge. The conversion: interpolate along edge geometry at `offset_m / length_m` fraction to get (x, y), and compute heading from geometry tangent.

This conversion already exists implicitly in `build_instances()` which produces render positions. Look at how it converts positions for rendering -- the same approach works for emergency vehicle positions.

### Anti-Patterns to Avoid

- **Creating new GPU buffers per frame:** The emergency buffer is pre-allocated (16 entries). Just write to it each frame.
- **Sharing `wgpu::Buffer` by cloning:** `wgpu::Buffer` is not `Clone`. Use a single owner and share references via bind group construction.
- **Modifying shader code:** The WGSL shaders are complete and working. This phase changes only Rust host code.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Buffer sharing | Custom reference counting | Create buffer once, pass to both pipelines | wgpu::Buffer is not Clone; single ownership with bind group refs |
| Coordinate conversion | Custom geometry interpolation | Existing edge geometry interpolation from render path | Already implemented for instance building |
| Emergency position tracking | Per-frame full scan | Filter during existing gpu_agents iteration | Already iterating all vehicles in step_vehicles_gpu |

## Common Pitfalls

### Pitfall 1: Buffer Usage Flag Mismatch
**What goes wrong:** Creating a buffer with `STORAGE | COPY_DST` then trying to use it as `COPY_SRC` for readback (or vice versa).
**Why it happens:** Two different components created their own buffers with different usage flags.
**How to avoid:** Create one buffer with all needed usage flags (`STORAGE | COPY_SRC`) and share it.
**Warning signs:** wgpu validation error about buffer usage.

### Pitfall 2: Zero emergency_count Early-Exit
**What goes wrong:** `emergency_count` in WaveFrontParams stays 0, so `check_emergency_yield()` returns immediately for every agent every frame.
**Why it happens:** `upload_emergency_vehicles()` sets `self.emergency_count` but is never called.
**How to avoid:** Call `upload_emergency_vehicles()` every frame in `step_vehicles_gpu()`, even with empty list (which correctly sets count to 0 when no emergency vehicles exist).

### Pitfall 3: Missing FLAG_EMERGENCY_ACTIVE
**What goes wrong:** Even if emergency_count > 0 and emergency buffer is populated, the perception shader's emergency-nearby detection (perception.wgsl line 197-209) checks `other.flags & FLAG_EMERGENCY_ACTIVE`, which is never set.
**Why it happens:** `step_vehicles_gpu()` only sets flag bit 0 (bus dwelling), never bit 1 (emergency active).
**How to avoid:** When building GpuAgentState for emergency vehicles, OR in bit 1: `flags |= 2`.

### Pitfall 4: Heading Computation for Emergency Vehicles
**What goes wrong:** Emergency vehicles get heading=0.0, causing the yield cone to point in wrong direction.
**Why it happens:** Road-relative positions don't directly encode heading.
**How to avoid:** Compute heading from edge geometry tangent at the agent's offset position.

### Pitfall 5: PerceptionPipeline Readback After Buffer Swap
**What goes wrong:** If `PerceptionPipeline.result_buffer` is replaced (ownership transferred), `readback_results()` tries to copy from a buffer it no longer owns.
**How to avoid:** Either keep the result buffer in PerceptionPipeline and use the same buffer reference in ComputeDispatcher's bind group, OR restructure ownership carefully. The key insight: `wgpu::BindGroupEntry` takes a `BufferBinding` which is a reference, not ownership. So the buffer can live in one place and be referenced by both bind groups.

## Code Examples

### Fix 1: Wire perception result buffer in SimWorld::new()

```rust
// In SimWorld::new(), after creating perception pipeline (current line 193):
let perception = PerceptionPipeline::new(device, 300_000);

// NEW: Share perception result buffer with wave-front dispatcher.
// PerceptionPipeline's result_buffer has STORAGE | COPY_SRC usage,
// which satisfies both perception write (storage read_write) and
// wave_front read (storage read).
//
// Approach: PerceptionPipeline exposes buffer ownership transfer,
// or we refactor to create buffer externally.
```

### Fix 2: Upload emergency vehicles each frame

```rust
// In step_vehicles_gpu(), while building gpu_agents:
let mut emergency_list: Vec<GpuEmergencyVehicle> = Vec::new();

// In the agent loop, after pushing to gpu_agents:
if *vtype == VehicleType::Emergency {
    // Set FLAG_EMERGENCY_ACTIVE (bit 1)
    // flags |= 2;  // already computed above

    // Collect position for yield cone buffer
    if let Some(world_pos) = self.agent_world_position(entity) {
        emergency_list.push(GpuEmergencyVehicle {
            pos_x: world_pos.0 as f32,
            pos_y: world_pos.1 as f32,
            heading: world_pos.2 as f32, // radians
            _pad: 0.0,
        });
    }
}

// After building gpu_agents, before dispatch:
dispatcher.upload_emergency_vehicles(queue, &emergency_list);
```

### Fix 3: Set FLAG_EMERGENCY_ACTIVE on GPU agent state

```rust
// Current code (sim.rs line 648-652):
flags: if bus_state.map_or(false, |bs| bs.is_dwelling()) {
    1 // FLAG_BUS_DWELLING
} else {
    0
},

// Fixed code:
flags: {
    let mut f = 0u32;
    if bus_state.map_or(false, |bs| bs.is_dwelling()) {
        f |= 1; // FLAG_BUS_DWELLING
    }
    if *vtype == VehicleType::Emergency {
        f |= 2; // FLAG_EMERGENCY_ACTIVE
    }
    f
},
```

## State of the Art

| Component | Current State | After Phase 11 |
|-----------|--------------|----------------|
| binding(8) perception_results | Zeroed placeholder | Real perception data |
| emergency_count | Always 0 | Actual count of emergency vehicles |
| FLAG_EMERGENCY_ACTIVE | Never set | Set for all emergency vehicles |
| red_light_creep | Never activates (signal_state=0 from zeroes) | Activates on red signals |
| gap_acceptance | No effect (leader_gap=0 from zeroes) | Uses real leader data |
| yield_cone | Early-exits every frame | Activates near emergency vehicles |

## Open Questions

1. **Buffer ownership pattern for shared perception result buffer**
   - What we know: `wgpu::Buffer` is not `Clone`. PerceptionPipeline owns the result buffer. ComputeDispatcher needs the same buffer for binding(8).
   - What's unclear: Best ownership pattern -- should PerceptionPipeline give up ownership, or should the buffer be created externally?
   - Recommendation: Create the buffer in `SimWorld::new()` with `STORAGE | COPY_SRC` flags. Pass owned buffer to ComputeDispatcher via `set_perception_result_buffer()`. Pass reference to PerceptionPipeline via a new `set_result_buffer()` method or by modifying constructor. The PerceptionPipeline's `create_bind_group()` already constructs bind groups from external refs, so refactoring the result buffer to be external follows the same pattern.

2. **Emergency vehicle world-space position computation**
   - What we know: Render instances already compute world positions from road-relative positions. Edge geometry polylines exist in `RoadGraph`.
   - What's unclear: Whether there's an existing utility function for edge-position-to-world-coordinate conversion.
   - Recommendation: Check `sim_render.rs` or `sim_helpers.rs` for existing position interpolation. If none exists, implement a simple `edge_position_to_world(graph, edge_id, offset_m) -> (f64, f64, f64)` that interpolates along edge geometry.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + cargo test |
| Config file | Cargo.toml (workspace) |
| Quick run command | `cargo test -p velos-gpu --lib -- --test-threads=1` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TUN-04 | red_light_creep reads real signal_state from perception buffer | unit | `cargo test -p velos-gpu --lib compute::tests::wave_front_shader_uses_perception_in_main` | Existing (shader string check) |
| TUN-06 | gap_acceptance reads real leader data from perception buffer | unit | `cargo test -p velos-gpu --lib compute::tests::gap_acceptance` | Existing (CPU reference tests) |
| INT-03 | Perception result buffer wired to wave-front | integration | `cargo test -p velos-gpu --test integration_perception_wiring` | Wave 0 |
| AGT-08 | Emergency vehicles uploaded + yield cone activates | integration | `cargo test -p velos-gpu --test integration_emergency_wiring` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-gpu --lib`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-gpu/tests/integration_perception_wiring.rs` -- verify set_perception_result_buffer called, perception data flows to wave_front (CPU-only path verifying the wiring logic, not GPU execution)
- [ ] `crates/velos-gpu/tests/integration_emergency_wiring.rs` -- verify upload_emergency_vehicles called, FLAG_EMERGENCY_ACTIVE set, emergency_count > 0
- [ ] Unit test in `compute.rs`: verify FLAG_EMERGENCY_ACTIVE is set for Emergency VehicleType in GpuAgentState construction
- [ ] Unit test: verify emergency vehicle world position computation from edge geometry

## Sources

### Primary (HIGH confidence)
- `crates/velos-gpu/src/compute.rs` -- ComputeDispatcher: buffer creation, upload methods, set_perception_result_buffer(), upload_emergency_vehicles()
- `crates/velos-gpu/shaders/wave_front.wgsl` -- WGSL shader: binding(8) perception_results, check_emergency_yield(), red_light_creep_speed(), intersection_gap_acceptance()
- `crates/velos-gpu/shaders/perception.wgsl` -- perception gather kernel: writes PerceptionResult to binding(7) result buffer
- `crates/velos-gpu/src/sim.rs` -- SimWorld: tick_gpu() pipeline, step_vehicles_gpu(), new() constructor
- `crates/velos-gpu/src/sim_perception.rs` -- step_perception(): dispatches perception pipeline, manages auxiliary buffers
- `crates/velos-gpu/src/perception.rs` -- PerceptionPipeline: result_buffer(), create_bind_group(), readback_results()

### Secondary (MEDIUM confidence)
- `crates/velos-gpu/src/sim_startup.rs` -- startup initialization patterns (follows same wiring style needed for Phase 11)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new libraries, all code exists
- Architecture: HIGH -- gaps are clearly identified from source code reading
- Pitfalls: HIGH -- pitfalls derived from actual buffer usage flags and ownership patterns in existing code

**Research date:** 2026-03-08
**Valid until:** No expiry -- based on project source code, not external libraries
