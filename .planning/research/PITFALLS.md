# Pitfalls Research

**Domain:** GPU-accelerated traffic microsimulation (wgpu/Metal, WGSL, Tauri v2, fixed-point, ECS-GPU sync, CCH routing, motorbike sublane)
**Researched:** 2026-03-06
**Confidence:** MEDIUM-HIGH (verified against official docs, issue trackers, and SUMO/IDM literature)

## Critical Pitfalls

### Pitfall 1: Tauri v2 + wgpu Surface Conflict (Flickering / No Render)

**What goes wrong:**
The Tauri webview and wgpu rendering context compete for the same native window surface. On macOS, this manifests as flickering or the wgpu surface simply not appearing visually. The webview's compositing layer and wgpu's Metal surface fight for draw ownership.

**Why it happens:**
Tauri uses the platform's native webview (WKWebView on macOS) which has its own compositing pipeline. When you create a wgpu surface on the same window, both try to own the CALayer hierarchy. This is a fundamental architectural conflict documented in [tauri-apps/tauri#9220](https://github.com/tauri-apps/tauri/issues/9220) and multiple discussions. The issue was closed as "not planned" on Linux/GTK; macOS has similar but less severe compositing issues.

**How to avoid:**
Three viable approaches, in order of recommendation:
1. **Separate windows:** Use a dedicated native window for wgpu rendering (no webview on it), and a separate Tauri webview window for the dashboard UI. Communicate between them via Tauri IPC. This is the most reliable path.
2. **Headless render + canvas blit:** Render wgpu to an offscreen texture, read pixels back, and send the frame buffer to the webview's `<canvas>` via IPC. Works but adds latency (1-3ms per frame for readback + transfer).
3. **Child window overlay:** Create a transparent child window with the wgpu surface layered beneath or above the webview. Requires platform-specific window management code. Reference: [FabianLars/tauri-v2-wgpu](https://github.com/FabianLars/tauri-v2-wgpu).

**Warning signs:**
- wgpu surface creates successfully but nothing renders visually
- Intermittent flickering when the webview repaints
- Render works in standalone wgpu test but fails inside Tauri

**Phase to address:**
Phase 1 (Technical Spike). This must be validated in the first week. If the chosen approach fails, the entire app architecture changes. Build a minimal Tauri + wgpu + Metal proof-of-concept that renders a triangle before writing any simulation code.

---

### Pitfall 2: WGSL Has No 64-bit Integers -- Fixed-Point Multiplication Overflows

**What goes wrong:**
The architecture specifies Q16.16 fixed-point for position and Q12.20 for speed. Multiplying two Q16.16 values produces a 64-bit intermediate. WGSL only has `i32` and `u32`. The `fix_mul` function in the architecture doc uses a split-multiply approach (`ah * bl + al * bh`), but this is fragile: intermediate products can overflow i32 when both operands exceed ~181 (sqrt(2^15)) in their integer parts.

**Why it happens:**
WGSL deliberately excludes 64-bit integer types ([gpuweb/gpuweb#5152](https://github.com/gpuweb/gpuweb/issues/5152)). The spec's AbstractInt has 64-bit precision but only at compile-time (const-expressions), not runtime. Every fixed-point multiply must be manually emulated with i32 arithmetic, and the carry/overflow handling is easy to get wrong.

**How to avoid:**
1. **Use unsigned arithmetic (u32) for position:** Positions are always non-negative. This doubles the safe range to ~256 (sqrt(2^16)) in integer parts before intermediate overflow.
2. **Clamp inputs before multiplication:** Before every `fix_mul`, clamp operands to known safe ranges. For Q16.16 position (max 65km), the integer part reaches 65535 -- this WILL overflow in multiplication. You must decompose into more than two partial products or use a different representation.
3. **Consider Q20.12 instead of Q16.16 for position:** 12 fractional bits give ~0.25mm resolution (sufficient for traffic), and the narrower fractional part reduces overflow risk in intermediate products.
4. **Exhaustive edge-case test suite:** Test fixed-point multiply with (0,0), (max,max), (max,1), negative values, and values near overflow boundaries. Run on CPU first with overflow detection (`checked_mul` in Rust) before porting to WGSL.
5. **Fallback plan:** If fixed-point proves too error-prone, use `f32` with the `@invariant` annotation on output variables. Accept "statistical equivalence" across GPUs (~1mm drift over 24h). The architecture doc already acknowledges this fallback.

**Warning signs:**
- Agents teleporting to position 0 or position max (overflow wraparound)
- Speed values suddenly becoming enormous or negative
- Simulation diverges after a few hundred steps but works for short runs
- Results differ between Rust CPU reference and WGSL GPU implementation

**Phase to address:**
Phase 1 (Technical Spike). Implement fixed-point arithmetic library in both Rust (reference) and WGSL, with a comparison test harness that validates bitwise equivalence across thousands of random inputs.

---

### Pitfall 3: IDM Produces Negative Velocities and Division-by-Zero

**What goes wrong:**
The Intelligent Driver Model has well-documented numerical pathologies (see [Limitations and Improvements of the IDM, SIAM 2021](https://epubs.siam.org/doi/10.1137/21M1406477)):
- When a vehicle approaches a stopped leader, the ballistic Euler update overshoots zero velocity, producing negative speed.
- When the actual gap `s` drops below the minimum gap `s0`, unchanged IDM produces negative acceleration even for stopped vehicles, pushing speed further negative.
- The `s*` (desired gap) term divides by `2*sqrt(a*b)` -- if `a` or `b` are misconfigured to zero, this explodes.
- The `v/v0` ratio in the free-road term uses `pow(v/v0, 4)` -- negative `v` passed to `pow` produces NaN or undefined behavior in WGSL.

**Why it happens:**
IDM was designed as a continuous ODE, but simulation engines use discrete Euler integration. At dt=0.1s, a vehicle decelerating at 9 m/s^2 loses 0.9 m/s per step. A vehicle at 0.5 m/s will reach -0.4 m/s in one step. The architecture doc includes `max(v, 0.1)` and `clamp(acc, -9.0, a_max)` guards, but these are insufficient -- they prevent zero-division but not the negative-velocity cascade.

**How to avoid:**
1. **Ballistic stopping guard:** If `v + a*dt < 0`, compute the exact stopping time `t_stop = -v/a`, advance `t_stop`, set `v = 0`, and remain stationary for the rest of `dt`. This is the standard fix from the IDM literature.
2. **Gap floor:** After every position update, enforce `s >= s0`. If the gap is violated, clamp position to maintain minimum gap rather than allowing the IDM to produce corrective negative acceleration.
3. **Safe pow4:** Use `x*x * x*x` instead of `pow(x, 4.0)`. The architecture doc already does this -- good.
4. **Parameter validation at spawn:** Reject agents with `a <= 0`, `b <= 0`, `v0 <= 0`, `s0 < 0`, `T < 0`. These should be compile-time assertions on profile construction.
5. **Regularization function:** For `v` near zero, multiply the interaction term by `h(v)` where `h(0) = 0` and `h(v) = 1` for `v > epsilon`. This prevents the "stuck behind stopped leader" deadlock more cleanly than the `max(v, 0.1)` hack.

**Warning signs:**
- Agents moving backwards on edges
- NaN or Inf values in acceleration/speed buffers (check with a validation compute pass)
- Vehicles permanently stopped at v=0.1 m/s (the `max(v, 0.1)` floor creating phantom crawling)
- Oscillatory stop-start behavior at traffic signals

**Phase to address:**
Phase 2 (Core Simulation). Implement IDM with all guards in Rust first, validate against known IDM test scenarios (approach-to-stop, free-flow, emergency-braking), then port to WGSL. The Rust implementation serves as the oracle for GPU validation.

---

### Pitfall 4: wgpu Buffer Mapping Deadlock (Forgetting device.poll)

**What goes wrong:**
On native platforms (not browser), `buffer.slice(..).map_async()` registers a callback but does NOT drive GPU work. Without calling `device.poll()`, the callback never fires and the program hangs indefinitely. Additionally, submitting compute dispatches without periodic polling causes unbounded GPU memory growth (staging buffers accumulate) leading to device disconnection ([gfx-rs/wgpu#3806](https://github.com/gfx-rs/wgpu/issues/3806)).

**Why it happens:**
wgpu's async model is designed for both browser (where the event loop drives polling automatically) and native (where the application must drive it). On native, there is no implicit event loop. Every `queue.submit()` allocates command buffers that are only freed when `device.poll()` reclaims them.

**How to avoid:**
1. **Establish a poll discipline:** After every `queue.submit()`, either:
   - Call `device.poll(Maintain::Wait)` if you need the result immediately (blocking).
   - Call `device.poll(Maintain::WaitForSubmissionIndex(idx))` if you track submission indices (preferred for frame pipelining).
   - Call `device.poll(Maintain::Poll)` non-blockingly in a dedicated polling thread.
2. **Limit in-flight submissions:** Keep at most 2-3 frames in flight. Use submission indices as fences. The architecture doc's `AgentBufferPool::begin_frame()` correctly uses `WaitForSubmissionIndex` -- maintain this pattern everywhere.
3. **Never create pipelines in hot loops:** Create `ComputePipeline` objects once during initialization. Pipeline creation involves shader compilation and is expensive.
4. **Use StagingBelt for frequent uploads:** `queue.write_buffer()` allocates a new staging buffer each call. For per-frame uploads, use `wgpu::util::StagingBelt` which pools and reuses staging buffers.

**Warning signs:**
- Program hangs after first GPU readback attempt
- GPU memory usage grows linearly over time (check with `system_profiler SPDisplaysDataType` on macOS)
- `device.poll()` calls taking progressively longer
- "Device lost" or "Out of memory" errors after running for minutes

**Phase to address:**
Phase 1 (Technical Spike). Establish the GPU buffer management pattern in the wgpu spike. The double-buffer + poll-fence pattern must be proven before building the simulation pipeline on top of it.

---

### Pitfall 5: Sublane Lateral Dynamics Depend on Timestep Size

**What goes wrong:**
SUMO's sublane model has a documented bug where lateral movement behavior (controlled by `lcSigma`) depends on simulation step length ([eclipse-sumo/sumo#8154](https://github.com/eclipse-sumo/sumo/issues/8154)). At dt=0.1s, lateral oscillation nearly vanishes; at dt=0.5s, it works as expected. VELOS uses dt=0.1s. If the lateral speed computation is not properly normalized for the timestep, motorbikes will appear to be "on rails" laterally -- destroying the core differentiator.

**Why it happens:**
Lateral speed calculations that work correctly at one timestep fail at others because:
- Lateral acceleration/jerk terms are applied per-step without scaling by `dt`
- Random lateral perturbation (the "wobble" factor) is applied as a fixed displacement per step rather than a rate
- The lateral alignment force that centers vehicles in lanes dominates the random perturbation at small timesteps

**How to avoid:**
1. **Express all lateral dynamics as rates, not per-step increments:** Lateral speed = `lateral_acceleration * dt`. Random wobble = `wobble_amplitude * sqrt(dt) * random()` (use `sqrt(dt)` for stochastic terms, per Brownian motion scaling).
2. **Decouple decision interval from simulation step:** Use an "action step" for lateral decisions (e.g., every 0.5s) independent of the physics timestep (0.1s). Between action steps, execute the lateral movement plan smoothly.
3. **Validate at multiple timesteps:** Run the same scenario at dt=0.05s, 0.1s, 0.2s, 0.5s. Measure aggregate lateral movement distribution. It should be statistically similar across timesteps.

**Warning signs:**
- Motorbikes moving in perfectly straight lines (no lateral wobble)
- Filtering behavior works in unit tests but not in full simulation (different dt)
- Lateral position distribution is a spike at lane center instead of a spread

**Phase to address:**
Phase 2-3 (Motorbike Model). This is the project's core differentiator. Build the sublane model with explicit timestep-normalized dynamics from day one. Create a dedicated visual test: render 50 motorbikes on a straight road and visually verify lateral wobble and filtering behavior.

---

### Pitfall 6: CCH Node Ordering Computed with Wrong Metric

**What goes wrong:**
CCH separates topology ordering from edge weights. The node ordering must be computed using **metric-independent nested dissection** (graph partitioning based on topology alone). If the ordering accidentally incorporates edge weights (e.g., using weighted degree or travel-time-based importance), the customization phase produces incorrect shortest paths when weights change. Queries silently return suboptimal routes -- no crash, no error, just wrong answers.

**Why it happens:**
Standard CH implementations (like `fast_paths`) compute node ordering using edge weights as the importance metric. When adapting CH code to CCH, it is natural to reuse the ordering code, inadvertently baking in the initial weight metric. The CCH paper ([Dibbelt et al., 2014](https://arxiv.org/pdf/1402.0402)) is explicit about this requirement, but implementation tutorials are scarce.

**How to avoid:**
1. **Use nested dissection for ordering:** Partition the graph recursively using METIS or KaHIP (topology only, ignoring weights). Node ordering = order nodes appear in the separator tree.
2. **Verify with adversarial weight changes:** After building the CCH, customize with free-flow weights and verify shortest paths against Dijkstra. Then customize with REVERSED weights (invert all edges to max_weight - weight). If paths are still correct, the ordering is weight-independent.
3. **Reference implementation:** Study InertialFlowCutter or the RoutingKit CCH implementation as reference. These are the canonical open-source CCH implementations.

**Warning signs:**
- Routes are optimal for initial weights but suboptimal after weight customization
- Customization takes longer than expected (dense shortcut graph from bad ordering)
- Shortcut count >> 3x original edge count (should be ~2-3x for good ordering)

**Phase to address:**
Phase 3 (Routing). The CCH implementation is a self-contained module. Build it with correctness tests (CCH vs. Dijkstra on 1000 random queries) before integrating with the simulation. Test with at least 3 different weight configurations.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Use `f32` instead of fixed-point in WGSL | 5x faster development, no overflow bugs | Non-deterministic across GPU vendors (~1mm drift/24h) | Acceptable for single-GPU macOS POC. Tag with `// TODO: fixed-point` for multi-GPU phase |
| Skip double-buffering (single GPU buffer) | Simpler buffer management | Race conditions when CPU writes while GPU reads; intermittent corruption | Never -- even for 1K agents. The cost of debugging GPU race conditions far exceeds the cost of double-buffering |
| Hardcode IDM parameters instead of per-agent profiles | Faster initial development | Cannot represent mixed traffic (motorbikes vs. cars have different dynamics) | Never -- HCMC mixed traffic is the core requirement |
| Use A* instead of CCH for pathfinding | No preprocessing step, simpler code | 25x slower queries; at 500 reroutes/step, A* costs 250ms vs. CCH's 0.7ms | Acceptable for Phase 1 spike with <50 agents. Replace by Phase 3 |
| Single-threaded CPU-side processing | Avoid rayon complexity | Cannot scale beyond ~100 agents for per-frame CPU work (sorting, routing) | Phase 1 only. Add rayon parallelism before scaling to 1K agents |

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| hecs ECS + wgpu buffers | Iterating hecs components directly into GPU buffer, assuming contiguous memory | hecs does NOT guarantee contiguous iteration order across frames (archetype order can change when entities are added/removed). Maintain a separate, stable index mapping: `agent_id -> gpu_buffer_index`. Copy from hecs to a staging array in stable order, then upload |
| Tauri IPC + simulation thread | Calling Tauri IPC (which is async/main-thread) from the simulation thread, causing deadlocks | Run simulation on a dedicated thread with `std::sync::mpsc` or `crossbeam::channel`. Tauri commands read from the channel. Never block the simulation loop on IPC |
| tokio + rayon | Using `tokio::spawn` for CPU-bound simulation work, starving the async runtime | Use `rayon::spawn` or `tokio::task::spawn_blocking` for CPU-bound work (sorting, pathfinding). Reserve tokio for IO only (API server, file writes) |
| wgpu + Metal on macOS | Assuming `wgpu::Instance::new(Backends::PRIMARY)` selects Metal | Explicitly request `Backends::METAL` on macOS. `PRIMARY` may select a different backend depending on wgpu version. Verify with `adapter.get_info().backend` |
| OSM import + road graph | Assuming OSM ways map 1:1 to simulation edges | OSM ways must be split at every intersection. A single OSM way through 5 intersections becomes 4 edges. Use `osm4routing` or similar splitting logic. HCMC alleys have many tiny edges (<5m) that need special handling for CFL stability |

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Recreating compute pipelines per frame | Frame time grows, GPU driver memory leak | Create all `ComputePipeline` objects at startup. Reuse across frames. Only recreate if shader source changes | Immediately -- even at 1K agents |
| `queue.write_buffer` every frame for agent data | Allocation pressure, GC stalls in Metal driver | Use `StagingBelt` or pre-allocated mapped buffers. Write to staging, copy to GPU buffer via `CommandEncoder::copy_buffer_to_buffer` | ~100 agents with multiple buffers per frame |
| Dispatching workgroups for empty lanes | GPU occupancy waste: 50K workgroups dispatched but only 5K have agents | Use indirect dispatch: a prefix-sum compute pass compacts non-empty lanes, then `dispatch_workgroups_indirect` processes only active lanes | ~10K agents when lanes are sparse (off-peak simulation) |
| Per-agent neighbor search without spatial index | O(N^2) neighbor lookup for lateral gap computation in sublane model | Use R-tree (rstar) or spatial hash on CPU, or GPU spatial hash with prefix-sum compaction | ~200 agents without spatial index; O(N^2) is 40K comparisons |
| Sorting agents per-lane on CPU every frame | CPU becomes bottleneck for GPU-bound simulation | Use GPU radix sort or bitonic sort for per-lane ordering. Or maintain sorted order incrementally (insertion sort when agents change lanes, which is rare per step) | ~5K agents with 50K lanes (1.5ms budget consumed by sorting alone) |
| Workgroup size 256 on Apple Silicon | Shader compilation succeeds but dispatch fails or performs poorly | Apple Silicon maximum total workgroup invocations is 256 (not per-dimension). Verify `@workgroup_size(256, 1, 1)` is fine but `@workgroup_size(256, 256, 1)` is not. Query device limits at startup | Immediately on Apple Silicon if workgroup size is wrong |

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| Simulation runs but agents are invisible (Z-fighting or wrong coordinate space) | User sees empty road network, assumes simulation is broken | Add a debug overlay that renders agent positions as colored dots independent of the main render pipeline. First thing to build after the wgpu spike |
| No frame rate indicator | User cannot tell if simulation is running in real-time or lagging | Display simulation time, wall clock time, and their ratio. Show frame time histogram. Make this a permanent UI element, not a debug toggle |
| Simulation speed control has no visual feedback | User clicks "2x speed" but cannot tell if it took effect | Show current speed multiplier prominently. Flash the indicator when changed. Show "PAUSED" overlay when paused |
| Camera controls fight with dashboard panels | Dragging to pan the map accidentally interacts with dashboard elements | Use separate input zones: map viewport captures mouse only inside its bounds. Dashboard panels are in a non-overlapping region (sidebar or bottom panel) |

## "Looks Done But Isn't" Checklist

- [ ] **GPU compute pipeline:** Often missing proper error handling on `device.create_compute_pipeline()` -- shader compilation errors are silent by default. Register an `uncaptured_error_handler` on the device.
- [ ] **Agent spawning:** Often missing edge capacity check -- spawning agents on full edges creates instant collisions. Verify edge has gap >= vehicle length before spawning.
- [ ] **Traffic signals:** Often missing the all-red clearance phase between conflicting green phases. Without it, agents from perpendicular approaches collide in the intersection box.
- [ ] **Route computation:** Often missing the "no route found" case. When CCH returns `None`, the agent needs a fallback (despawn, wait and retry, follow shortest partial path). Unhandled `None` causes a panic in release builds or silent agent removal.
- [ ] **Fixed-point conversion:** Often missing the asymmetry between `to_fixed` and `from_fixed` for negative values. Truncation vs. rounding produces off-by-one errors that accumulate over thousands of steps.
- [ ] **Lane-change safety criterion:** Often missing the check for the NEW follower in the target lane. MOBIL requires checking that the follower in the target lane can brake safely. Without it, lane changes cause rear-end collisions.
- [ ] **CFL sub-stepping:** Often missing the update to `leader_position` within sub-steps. If the leader also moves during sub-steps (it does, in wave-front dispatch), using stale leader position causes gap violations.
- [ ] **Checkpoint/restore:** Often missing buffer re-upload after restore. ECS state loads from Parquet but GPU buffers still contain stale data from before the checkpoint.

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Tauri+wgpu surface conflict | MEDIUM | Switch to separate-window architecture. Requires refactoring IPC but not simulation code. ~2-3 days |
| Fixed-point overflow in WGSL | HIGH | Fall back to f32 with `@invariant`. Rewrite all fixed-point WGSL functions. Accept non-determinism for single-GPU POC. ~1 week |
| IDM negative velocity cascade | LOW | Add ballistic stopping guard (10 lines of code). Does not require architectural change. ~2 hours |
| CCH wrong ordering | HIGH | Rewrite ordering algorithm to use nested dissection. All shortcuts must be recomputed. Query code is unaffected. ~3-5 days |
| Sublane timestep dependence | MEDIUM | Refactor lateral dynamics to rate-based. Requires changing the shader and re-tuning parameters. ~3 days |
| GPU buffer race condition | LOW-MEDIUM | Implement double-buffering. Straightforward if buffer layout is already SoA. ~1 day |
| Polling deadlock | LOW | Add `device.poll(Maintain::Wait)` after map_async. One-line fix but hard to diagnose without knowing the pattern. ~1 hour once identified |

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Tauri+wgpu surface conflict | Phase 1: Technical Spike | Render a colored triangle in wgpu surface visible alongside Tauri webview dashboard |
| WGSL no-i64 fixed-point overflow | Phase 1: Technical Spike | CPU-WGSL comparison test: 10K random fixed-point multiplications, zero divergence |
| IDM negative velocity | Phase 2: Core Simulation | Unit test: vehicle approaching stopped leader from 50km/h, verify v >= 0 at all steps |
| wgpu polling deadlock | Phase 1: Technical Spike | Stress test: 1000 compute dispatches with buffer readback, no hangs, stable memory |
| Sublane timestep dependence | Phase 2-3: Motorbike Model | Run identical scenario at dt=0.05s and dt=0.1s, compare lateral position distributions (KS test p > 0.05) |
| CCH wrong ordering | Phase 3: Routing | CCH vs. Dijkstra comparison on 1000 random queries with 3 different weight configurations, all optimal |
| hecs iteration order instability | Phase 2: Core Simulation | Test: add/remove entities between frames, verify GPU buffer indices remain consistent |
| Workgroup size on Apple Silicon | Phase 1: Technical Spike | Query `device.limits().max_compute_workgroup_size_x` at startup, assert <= 256 total invocations used |

## Sources

- [Tauri + wgpu flickering bug (GitHub #9220)](https://github.com/tauri-apps/tauri/issues/9220)
- [FabianLars tauri-v2-wgpu reference](https://github.com/FabianLars/tauri-v2-wgpu)
- [WGSL 64-bit integer types discussion (gpuweb #5152)](https://github.com/gpuweb/gpuweb/issues/5152)
- [wgpu compute without polling causes memory growth (gfx-rs #3806)](https://github.com/gfx-rs/wgpu/issues/3806)
- [SUMO sublane step-length dependence (eclipse-sumo #8154)](https://github.com/eclipse-sumo/sumo/issues/8154)
- [IDM limitations and improvements (SIAM 2021)](https://epubs.siam.org/doi/10.1137/21M1406477)
- [IDM variants reference (traffic-simulation.de)](https://traffic-simulation.de/info/info_IDM.html)
- [CCH paper (Dibbelt et al., 2014)](https://arxiv.org/pdf/1402.0402)
- [WGSL barrier scope limitations (gpuweb #3935)](https://github.com/gpuweb/gpuweb/discussions/3935)
- [wgpu Limits documentation](https://docs.rs/wgpu/latest/wgpu/struct.Limits.html)
- [WebGPU shader limits (Hugo Daniel)](https://hugodaniel.com/posts/webgpu-shader-limits/)
- [wgpu buffer mapping and polling guide (Till Code)](https://tillcode.com/rust-wgpu-compute-minimal-example-buffer-readback-and-performance-tips/)
- [Floating Point Determinism (Gaffer On Games)](https://gafferongames.com/post/floating_point_determinism/)

---
*Pitfalls research for: VELOS GPU-accelerated traffic microsimulation*
*Researched: 2026-03-06*
