# Codebase Concerns

**Analysis Date:** 2026-03-06

## Project Status Context

VELOS is in **pre-development/architecture phase** -- no source code exists yet. All concerns below are derived from architecture documents in `docs/architect/` and the architecture review in `docs/velos-architecture-review.md`. These concerns represent risks, gaps, and design weaknesses that must be addressed during implementation.

---

## Tech Debt

**No Code Yet -- Architecture-Level Design Debt:**
- Issue: The project has 8 architecture documents (`docs/architect/00-07`) plus 7 legacy v1 documents in `docs/` root. The legacy docs are superseded but not removed or clearly marked as such, creating confusion about which designs are authoritative.
- Files: `docs/rebuild-sumo-architecture-plan.md`, `docs/velos-agent-intelligence-and-prediction.md`, `docs/velos-architecture-review.md`, `docs/velos-3d-visualization-architecture.md`, `docs/VELOS-API-Contract-Reference.md`, `docs/VELOS-Deployment-Infrastructure-Guide.md`, `docs/velos-rust-parallel-frameworks-and-reusable-oss.md`, `docs/velos-self-hosted-open-data-tile-map.md`
- Impact: Developers may reference outdated designs (e.g., v1 EVEN/ODD dispatch, Arrow IPC Python bridge, W99 car-following) and build the wrong thing.
- Fix approach: Move legacy docs to `docs/legacy/` or add prominent "SUPERSEDED" banners. The `CLAUDE.md` already notes `docs/architect/` is authoritative, but the legacy files remain at the same level.

**Fixed-Point Arithmetic Marked as Optional:**
- Issue: Cross-GPU determinism via Q16.16/Q12.20 fixed-point is described in `docs/architect/01-simulation-engine.md` Section 4 but explicitly labeled as "optional" in the timeline (`docs/architect/07-timeline-risks.md` Week 28). The 20% performance overhead is the stated reason.
- Files: `docs/architect/01-simulation-engine.md` (Section 4), `docs/architect/07-timeline-risks.md` (Week 28)
- Impact: If deferred, cross-GPU determinism (W14) remains unresolved. The fallback ("statistical equivalence within 1mm") is acceptable for POC but undermines reproducibility claims.
- Fix approach: Implement fixed-point as a compile-time feature flag. Run benchmarks during Spike S1 to measure actual overhead. Decide at G0 whether to commit.

**WGSL Lacks 64-bit Integer Support:**
- Issue: The fixed-point multiplication in `docs/architect/01-simulation-engine.md` Section 4 requires 64-bit intermediate values, but WGSL has no `i64` type. The proposed workaround uses manual hi/lo i32 splitting, which is error-prone and untested.
- Files: `docs/architect/01-simulation-engine.md` (Section 4, `fix_mul` function)
- Impact: The `fix_mul` implementation may have overflow bugs at boundary values. Silent arithmetic errors in the core physics loop would produce incorrect simulation results.
- Fix approach: Write exhaustive unit tests for fixed-point arithmetic before integrating into shaders. Consider using `f32` with `@invariant` as the primary path and fixed-point as a validation mode.

---

## Known Bugs

No code exists yet -- no bugs to report. The architecture review (`docs/velos-architecture-review.md`) identified 14 critical and 11 major issues in the v1 design. The v2 architecture documents (`docs/architect/`) resolve all 15 tracked weaknesses (W1-W15). However, the following v1 review issues have only partial resolution:

**Coordinate Transformation System Not Fully Specified:**
- Symptoms: The v1 review (C5) identified that edge-local coordinates (offset along edge) need transformation to world coordinates (lat/lon) for visualization. The v2 docs mention FlatBuffers with `x_offset`/`y_offset` in `docs/architect/05-visualization-api.md` Section 2, but the GPU-side or CPU-side transformation pipeline is not designed.
- Files: `docs/architect/05-visualization-api.md` (Section 2, TileFrame format), `docs/velos-architecture-review.md` (C5)
- Trigger: First integration of deck.gl with simulation output (Week 6-7 per timeline).
- Workaround: Compute edge-local to lat/lon on CPU during output recording. Acceptable for 280K agents at 1 Hz output rate, but 10 Hz WebSocket will require GPU-side transformation.

**Social Force Model Missing Anisotropic Factor:**
- Symptoms: The v1 review (m7) noted the pedestrian social force model omits the anisotropic factor (pedestrians react more to oncoming people). The v2 pedestrian model in `docs/architect/02-agent-models.md` Section 3 still does not include this.
- Files: `docs/architect/02-agent-models.md` (Section 3, PedestrianParams)
- Trigger: Pedestrian crosswalk scenarios with counter-flow (common at HCMC intersections).
- Workaround: Accept reduced realism for POC. Add anisotropic factor as an enhancement task.

---

## Security Considerations

**API Authentication Is Minimal:**
- Risk: The POC uses API key header for gRPC/REST authentication. No rate limiting, no RBAC, no audit logging. If the API is exposed beyond localhost, any client with the key has full simulation control (pause, reset, mutate network).
- Files: `docs/architect/06-infrastructure.md` (Section 7)
- Current mitigation: All services on private Docker network. Only ports 3000, 8080, 50051 exposed.
- Recommendations: For POC, restrict API to localhost/VPN. Before any external exposure, add: (1) per-endpoint rate limiting, (2) read-only vs. admin API key separation, (3) audit log for mutation operations (BlockEdge, SetSignalTiming, Reset).

**Redis Has No Authentication:**
- Risk: The Docker Compose config in `docs/architect/06-infrastructure.md` Section 1 runs Redis without a password. If port 6379 is exposed (it is in the compose file), anyone can read simulation frame data or inject messages.
- Files: `docs/architect/06-infrastructure.md` (Section 1, Docker Compose)
- Current mitigation: Docker network isolation (services communicate internally).
- Recommendations: Add `requirepass` to Redis config. Remove port 6379 from host-level exposure (only needed for inter-container communication). Use Redis ACLs if multi-tenant scenarios arise.

**GPS Probe Data Privacy:**
- Risk: If Grab/Be GPS probe data is obtained for OD matrix calibration, raw trip data could contain PII (individual trip trajectories). The architecture states "raw probes are never stored" but the aggregation pipeline is not implemented.
- Files: `docs/architect/04-data-pipeline-hcmc.md` (Section 4), `docs/architect/06-infrastructure.md` (Section 7)
- Current mitigation: Architecture doc states OD matrix aggregation at import time.
- Recommendations: Implement aggregation as a separate offline tool. Ensure raw probe files are never copied to the simulation server. Add data handling policy document.

---

## Performance Bottlenecks

**Wave-Front Dispatch GPU Occupancy (Unproven):**
- Problem: The wave-front (Gauss-Seidel) dispatch processes agents sequentially within each lane. With average 5.6 agents/lane but up to 30 agents/lane on dense arterials, the longest lane dictates workgroup completion time. This creates GPU thread divergence.
- Files: `docs/architect/01-simulation-engine.md` (Section 2), `docs/architect/07-timeline-risks.md` (Risk R1, Spike S1)
- Cause: GPU workgroups with 256 threads but only 5-30 useful work items per workgroup waste most threads. Dense lanes (30 agents) run 6x longer than average lanes (5 agents), creating tail latency.
- Improvement path: Spike S1 (Week 1-2) will benchmark this. GO criteria: >40% throughput vs naive parallel. Fallback: EVEN/ODD dispatch with iterative correction (3-pass).

**Per-Lane Leader Sorting on CPU:**
- Problem: The frame pipeline (`docs/architect/01-simulation-engine.md` Section 6) allocates 1.5ms to per-lane leader sorting on CPU via rayon. At 280K agents across 50K lanes, this is a parallel sort-by-key operation. The 1.5ms estimate assumes uniform distribution.
- Files: `docs/architect/01-simulation-engine.md` (Section 6, Step 2)
- Cause: Non-uniform lane occupancy (some lanes have 30 agents, most have 1-5). Rayon work-stealing helps but cannot eliminate the sort cost entirely.
- Improvement path: Consider GPU radix sort (single dispatch) instead of CPU sort. Alternatively, maintain sorted order incrementally (insertion sort on mostly-sorted data) since agents rarely change relative order within a lane between steps.

**CCH Customization Every 60s:**
- Problem: CCH weight customization takes ~3ms for 25K edges. This runs every 60 sim-seconds (600 steps). At 10 steps/sec, this is a 3ms spike every 60 real-time seconds.
- Files: `docs/architect/03-routing-prediction.md` (Section 1)
- Cause: Bottom-up shortcut weight recomputation is inherently sequential by level.
- Improvement path: Run customization on a background thread via `tokio::spawn_blocking`. The CCH query uses the old weights until customization completes. ArcSwap pattern (already designed for prediction overlay) works here too.

---

## Fragile Areas

**Motorbike Sublane Filtering Model:**
- Files: `docs/architect/02-agent-models.md` (Section 1, MotorbikeFilter), `docs/architect/07-timeline-risks.md` (Risk R5, Gate G2)
- Why fragile: The sublane model uses continuous lateral positioning (FixedQ8_8) instead of discrete lanes. Lateral gap computation requires checking all nearby agents in adjacent sublane positions, not just the leader. At high densities (200K motorbikes), lateral collision detection is O(N*K) where K is the number of nearby agents.
- Safe modification: Any changes to the lateral gap threshold (`min_gap_lateral: 0.8m`) or filtering speed limit (`max_filter_speed: 20 km/h`) must be validated with a 10,000-step stress test at full agent count. The G2 gate (Week 12) exists specifically for this.
- Test coverage: No tests exist yet. When implemented, create: (1) unit tests for lateral gap computation, (2) stress test with 200K motorbikes for 10K steps checking zero lateral collisions, (3) visual validation of swarm formation at signals.

**Meso-Micro Graduated Buffer Zone:**
- Files: `docs/architect/02-agent-models.md` (Section 4, MesoMicroTransition)
- Why fragile: The 100m buffer zone with linear IDM parameter interpolation is sensitive to the interpolation function. If the relaxation factor (`T = 2.0 * T_normal` at entry) is too aggressive, vehicles enter micro zones too fast. If too conservative, artificial congestion forms at zone boundaries.
- Safe modification: Change interpolation factors only with before/after comparison of zone-boundary traffic flow. The `max_queue_wait: 30s` force-insert mechanism is a safety valve but could itself cause artifacts if triggered frequently.
- Test coverage: Test with scenarios that stress zone boundaries: (1) full micro zone with zero capacity, (2) empty micro zone with high-speed meso exit, (3) oscillating demand at zone boundary.

**Multi-GPU Boundary Agent Transfer:**
- Files: `docs/architect/01-simulation-engine.md` (Section 1, Boundary Agent Protocol)
- Why fragile: Agent handoff between GPU partitions involves CPU-mediated buffer transfers. If the CPU read of outbox buffers (Step 3 in protocol) takes longer than expected due to PCIe contention, agents can be "in flight" for more than one step, causing them to temporarily disappear from both partitions.
- Safe modification: The boundary map (`BoundaryMap`) must be consistent with the METIS partition. Any network changes (edge blocking) that alter partition boundaries require re-partitioning. Do not change partition boundaries at runtime.
- Test coverage: Test boundary crossing with: (1) single agent crossing at various speeds, (2) 1000 agents crossing simultaneously, (3) agent crossing while partition is at full capacity.

---

## Scaling Limits

**Single-Node Multi-GPU Cap:**
- Current capacity: 280K agents on 2x RTX 4090 (24GB VRAM each), ~14.6 MB VRAM per GPU.
- Limit: Theoretical max ~500K agents on 2 GPUs (VRAM limited). Multi-node distributed simulation is explicitly out of scope.
- Scaling path: v3 targets multi-node (gRPC-based distributed simulation) for 2M agents. Single-node scaling to 4 GPUs (4x RTX 4090) could reach ~750K agents as an intermediate step.

**WebSocket Viewer Limit:**
- Current capacity: 100 concurrent dashboard viewers via Redis pub/sub fan-out.
- Limit: Each relay pod handles ~50 connections. At 10Hz frame rate with 256 tiles at 8KB each, Redis memory is ~60MB (trivial). The bottleneck is relay pod fan-out computation.
- Scaling path: Add more stateless relay pods (K8s HPA on connection count). For >500 viewers, consider server-side rendering of heatmaps instead of sending individual agent positions.

**Calibration Data Coverage:**
- Current capacity: ~50 traffic count locations from HCMC DOT automated counters.
- Limit: GEH < 5 for 85% of links requires sufficient spatial coverage. With only 50 count locations across 25K edges, the calibration is sparse. Risk R3 (High probability, High impact) in `docs/architect/07-timeline-risks.md`.
- Scaling path: GPS probe data partnership (Grab/Be) would provide millions of data points. Manual field survey can add 10-20 locations at $3-5K cost.

---

## Dependencies at Risk

**CCH Library Availability:**
- Risk: No production-ready CCH crate exists in the Rust ecosystem. Spike S3 (`docs/architect/07-timeline-risks.md`) evaluates `rust_road_router` (KIT Karlsruhe academic code) and `fast_paths` (standard CH only, not CCH). If neither works, custom implementation adds 3 weeks.
- Impact: Routing is on the critical path. CCH delay pushes everything from Week 5 onward.
- Migration plan: Fallback to A* with landmarks (ALT algorithm) at 0.2ms/query (10x slower than CCH). This limits reroutes to 50/step instead of 500/step, degrading prediction responsiveness.

**wgpu Multi-Adapter Compute Support:**
- Risk: wgpu's multi-adapter API for compute workloads is not well-documented or widely tested. Spike S2 tests this in Week 1. Risk R2 (Medium probability, High impact).
- Impact: If multi-GPU fails, the project falls back to single-GPU with 200K agents. This is viable for POC but reduces the impressiveness of the 280K target.
- Migration plan: Single-GPU with agent count reduction. No architectural change needed -- the `MultiGpuScheduler` degrades to a single `GpuPartition`.

**hecs ECS Library:**
- Risk: hecs is lightweight and suitable for SoA layout, but it lacks built-in serialization for checkpoint/restore. The checkpoint system (`docs/architect/06-infrastructure.md` Section 2) requires manual component extraction and Parquet serialization.
- Impact: Checkpoint code will be boilerplate-heavy and fragile across component schema changes.
- Migration plan: If hecs proves too limiting, `bevy_ecs` or `specs` offer more features but are heavier. Evaluate during Phase 1 if checkpoint ergonomics become a problem.

**HCMC Signal Timing Data:**
- Risk: Risk R8 (High probability, Medium impact). HCMC DOT may be unresponsive. Most intersections have undocumented fixed-time plans. Only ~30% of junctions have known signal timing.
- Impact: 40% of junctions use "unsignalized" priority rules, 30% use inferred default timing. Calibration accuracy depends heavily on signal timing quality.
- Migration plan: Field survey of top 30 intersections ($3K budget). Default timing from junction geometry (cycle length based on leg count). Google Street View for approximate phase timing.

---

## Missing Critical Features

**No Warm-Up Period Handling:**
- Problem: Traffic simulations require 15-30 minutes of sim-time warm-up before measurements are valid. The architecture has no `warm_up_duration` config and no mechanism to suppress statistics during warm-up.
- Blocks: All calibration and validation results will be biased if measurements include the warm-up transient.
- Files: `docs/velos-architecture-review.md` (m6)
- Fix: Add `warm_up_duration: Duration` to simulation config. Suppress GEH/RMSE collection, output recording, and KPI reporting during warm-up. Display warm-up progress on dashboard.

**No Demand Overflow Handling:**
- Problem: If the OD matrix generates more agents than the network can absorb (all origin edges full), there is no queuing or rate-limiting mechanism for pending departures.
- Blocks: Peak-hour demand spikes could overwhelm edge capacity, causing agent spawn failures or infinite spawn retries.
- Files: Not addressed in any architecture document.
- Fix: Implement a departure queue with configurable max backlog. Agents waiting to depart are held in a CPU-side queue and spawned when origin edge has capacity. Report queue depth as a metric.

**No Network Disconnection Handling at Runtime:**
- Problem: If an edge is blocked via the API (`BlockEdge`) and this disconnects parts of the network, agents with routes through the blocked edge have no route. The architecture does not specify behavior for stranded agents.
- Blocks: Scenario 2 (road closure demo) could crash or hang if route invalidation is not handled.
- Files: `docs/architect/05-visualization-api.md` (Section 3, BlockEdge RPC)
- Fix: When an edge is blocked, immediately trigger reroute for all agents whose route includes that edge. If no alternative route exists, despawn the agent and log it. Report "stranded agent" count.

---

## Test Coverage Gaps

**No Test Infrastructure Exists:**
- What's not tested: Everything -- the project has zero source code and zero tests.
- Files: No test files exist.
- Risk: The entire test strategy is undefined beyond CI commands in `CLAUDE.md` (`cargo test --workspace`, `cargo clippy`, `naga --validate`).
- Priority: High. When implementation begins, establish test infrastructure in Week 1-2 alongside Spikes S1/S2/S3. Define: (1) unit test patterns per crate, (2) integration test harness for multi-crate scenarios, (3) GPU shader validation via naga, (4) benchmark framework for performance regression CI.

**Spike Validation Tests Must Be Preserved:**
- What's not tested: The three spikes (S1: wave-front benchmark, S2: multi-GPU feasibility, S3: CCH evaluation) will produce standalone test code that validates architectural assumptions. This code must be preserved as regression tests, not discarded after the spike.
- Files: `docs/architect/07-timeline-risks.md` (Section 3, Spikes S1-S3)
- Risk: If spike code is thrown away, the assumptions it validates cannot be re-verified after refactoring.
- Priority: High. Convert spike experiments into `tests/` integration tests with documented GO/NO-GO thresholds as assertions.

**Calibration Validation Requires Held-Out Data:**
- What's not tested: The calibration workflow (`docs/architect/04-data-pipeline-hcmc.md` Section 5) specifies 80/20 train/validation split of traffic counts, but the validation framework is not designed. No automation exists for computing GEH on held-out counts.
- Files: `docs/architect/04-data-pipeline-hcmc.md` (Section 5)
- Risk: Calibration could overfit to training counts without cross-validation catching it. The GEH < 5 for 85% claim would be unsubstantiated.
- Priority: Medium. E4 owns this (starting Week 13). Design the validation harness before first calibration iteration (Week 27).

---

## Critical Path Risks

**E1 Is a Single Point of Failure:**
- The critical path runs through E1 (Engine Lead) for the first 20 weeks. If E1 is blocked, sick, or underperforms, the entire project slips. E2/E3/E4 can work independently but their deliverables are useless without a working simulation engine.
- Files: `docs/architect/07-timeline-risks.md` (Section 4, Critical Path)
- Mitigation: E2 should be able to pick up basic ECS and shader work if needed. Cross-train E2 on wgpu basics during Week 1-2.

**All Team Positions Are TBD:**
- Every engineering role (E1-E4) is marked TBD in the team composition table.
- Files: `docs/architect/07-timeline-risks.md` (Section 2)
- Impact: The 12-month timeline assumes Month 1 start with E1/E2/E3 hired and onboarded. Any recruitment delay compresses the schedule.

---

*Concerns audit: 2026-03-06*
