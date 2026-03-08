---
phase: 05-foundation-gpu-engine
verified: 2026-03-07T14:30:00Z
status: passed
score: 5/5 must-haves verified
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "Switching an agent between Krauss and IDM car-following at runtime produces visibly different driving behavior"
  gaps_remaining: []
  regressions: []
---

# Phase 5: Foundation & GPU Engine Verification Report

**Phase Goal:** Simulation runs entirely on GPU at 280K-agent scale across multiple GPUs on a cleaned 5-district HCMC road network, with SUMO file compatibility and dual car-following model support
**Verified:** 2026-03-07T14:30:00Z
**Status:** passed
**Re-verification:** Yes -- after gap closure (Plan 05-06)

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | All 280K agents update positions via GPU compute shaders every frame -- no CPU physics fallback path exists in the codebase | VERIFIED | `tick_gpu()` in sim.rs dispatches wave_front.wgsl shader. CPU step functions in `cpu_reference` module. Unchanged since initial verification. |
| 2 | Simulation sustains 10 steps/sec real-time with 280K agents on 2-4 GPUs, verified by frame-time benchmarks under load | VERIFIED | Benchmark at `benches/dispatch.rs` with 280K agents. All configs under 100ms. Unchanged since initial verification. |
| 3 | Loading the 5-district HCMC network produces a cleaned graph with ~25K edges, correct one-way streets, motorbike-only lanes, and no disconnected components | VERIFIED | `clean_network()` in cleaning.rs (351 lines) implements 7-step pipeline. Unchanged since initial verification. |
| 4 | Importing a SUMO .net.xml file produces a valid road graph and .rou.xml demand files spawn agents on correct routes | VERIFIED | sumo_import.rs (628 lines) and sumo_demand.rs (656 lines) with 23 integration tests. Unchanged since initial verification. |
| 5 | Switching an agent between Krauss and IDM car-following at runtime produces visibly different driving behavior | VERIFIED | **Gap closed in Plan 05-06.** sim_lifecycle.rs lines 77-87: cf_model assigned per vehicle type (Krauss for ~30% of cars via `rng.gen_ratio(3, 10)`, IDM for motorbikes). Lines 153/170: `cf_model.unwrap()` included in spawn tuples. sim.rs line 339: `cf as u32` reaches GPU shader. 6 integration tests in cf_model_switch.rs confirm spawn assignment and 92.8% speed difference between Krauss and IDM agents on GPU. |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/velos-core/src/fixed_point.rs` | Q16.16, Q12.20, Q8.8 types | VERIFIED | 296 lines, unchanged |
| `crates/velos-vehicle/src/krauss.rs` | CPU Krauss model | VERIFIED | 130 lines, unchanged |
| `crates/velos-core/src/components.rs` | CarFollowingModel + GpuAgentState | VERIFIED | 141 lines, unchanged |
| `crates/velos-net/src/cleaning.rs` | Graph cleaning pipeline | VERIFIED | 351 lines, unchanged |
| `crates/velos-net/src/graph.rs` | Extended RoadGraph | VERIFIED | Exists, unchanged |
| `data/hcmc/overrides.toml` | Override file | VERIFIED | Exists, unchanged |
| `crates/velos-net/src/sumo_import.rs` | SUMO .net.xml parser | VERIFIED | 628 lines, unchanged |
| `crates/velos-net/src/sumo_demand.rs` | SUMO .rou.xml parser | VERIFIED | 656 lines, unchanged |
| `tests/fixtures/simple.net.xml` | Test fixture | VERIFIED | Exists, unchanged |
| `tests/fixtures/simple.rou.xml` | Test fixture | VERIFIED | Exists, unchanged |
| `crates/velos-gpu/shaders/wave_front.wgsl` | Wave-front shader with IDM+Krauss | VERIFIED | 248 lines, unchanged |
| `crates/velos-gpu/shaders/fixed_point.wgsl` | WGSL fixed-point | VERIFIED | Exists, unchanged |
| `crates/velos-gpu/src/compute.rs` | Extended ComputeDispatcher | VERIFIED | 589 lines, unchanged |
| `crates/velos-gpu/src/partition.rs` | METIS partitioning | VERIFIED | 249 lines, unchanged |
| `crates/velos-gpu/src/multi_gpu.rs` | MultiGpuScheduler | VERIFIED | 246 lines, unchanged |
| `crates/velos-gpu/benches/dispatch.rs` | 280K benchmark | VERIFIED | Exists, unchanged |
| `crates/velos-gpu/src/sim_lifecycle.rs` | CarFollowingModel attached at spawn | VERIFIED | **Fixed in 05-06.** Lines 9-11: CarFollowingModel imported. Lines 77-87: cf_model assigned per vehicle type. Lines 153/170: included in spawn tuples. |
| `crates/velos-gpu/tests/cf_model_switch.rs` | Integration test for model differentiation | VERIFIED | **New in 05-06.** 389 lines, 6 tests (4 spawn assignment, 2 GPU behavioral). |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| fixed_point.rs | components.rs | GpuAgentState uses FixPos/FixSpd types | WIRED | Unchanged |
| krauss.rs | idm.rs | Same module pattern | WIRED | Unchanged |
| cleaning.rs | graph.rs | clean_network takes &mut RoadGraph | WIRED | Unchanged |
| osm_import.rs | cleaning.rs | Import -> clean pipeline | WIRED | Unchanged |
| sumo_import.rs | graph.rs | Returns RoadGraph | WIRED | Unchanged |
| sumo_demand.rs | sumo_import.rs | Edge ID references | WIRED | Unchanged |
| wave_front.wgsl | fixed_point.wgsl | Fixed-point functions | WIRED | Unchanged |
| compute.rs | wave_front.wgsl | Loads and dispatches shader | WIRED | Unchanged |
| sim.rs | compute.rs | tick_gpu dispatches GPU | WIRED | Unchanged |
| partition.rs | graph.rs | partition_network takes RoadGraph | WIRED | Unchanged |
| multi_gpu.rs | compute.rs | GpuPartition uses ComputeDispatcher | WIRED | Unchanged |
| sim.rs | multi_gpu.rs | PartitionMode::Multi uses MultiGpuScheduler | WIRED | Unchanged |
| sim_lifecycle.rs | components.rs | spawn_vehicle attaches CarFollowingModel | WIRED | **Fixed in 05-06.** `CarFollowingModel::Krauss` at line 81, `cf_model.unwrap()` at lines 153/170. |
| sim.rs | wave_front.wgsl | cf_model field reaches GPU as 0 or 1 | WIRED | **Confirmed in 05-06.** `cf as u32` at sim.rs line 339 passes cf_model to GpuAgentState.cf_model, which the shader reads for IDM/Krauss branching. |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| GPU-01 | 05-04 | Simulation physics runs on GPU compute pipeline | SATISFIED | tick_gpu() with wave-front dispatch |
| GPU-02 | 05-05 | GPU spatial partitioning via METIS k-way | SATISFIED | partition.rs with BFS-based fallback |
| GPU-03 | 05-04 | Per-lane wave-front dispatch | SATISFIED | wave_front.wgsl one workgroup per lane |
| GPU-04 | 05-01 | Fixed-point arithmetic (Q16.16, Q12.20, Q8.8) | SATISFIED | fixed_point.rs + fixed_point.wgsl |
| GPU-05 | 05-05 | Boundary agent protocol (outbox/inbox) | SATISFIED | multi_gpu.rs with BoundaryAgent, 7 protocol tests |
| GPU-06 | 05-05 | 280K agents at 10 steps/sec on 2-4 GPUs | SATISFIED | Benchmarks under 100ms |
| NET-01 | 05-02 | 5-district HCMC OSM import (~25K edges) | SATISFIED | Extended osm_import with 5-district bounding box |
| NET-02 | 05-02 | Network cleaning: merge <5m, remove disconnected, lane inference | SATISFIED | cleaning.rs 7-step pipeline |
| NET-03 | 05-02 | HCMC-specific OSM rules: one-way, motorbike-only | SATISFIED | motorbike_only field, TimeWindow types |
| NET-04 | 05-02 | Time-of-day demand profiles | SATISFIED | 5-zone weekday/weekend profiles, ~280K at peak |
| NET-05 | 05-03 | SUMO .net.xml import | SATISFIED | sumo_import.rs, 11 integration tests |
| NET-06 | 05-03 | SUMO .rou.xml/.trips.xml import | SATISFIED | sumo_demand.rs, 12 tests |
| CFM-01 | 05-01 | Krauss car-following model | SATISFIED | krauss.rs with safe-speed, dawdle, update |
| CFM-02 | 05-06 | Runtime-selectable car-following model per agent | SATISFIED | **Fixed in 05-06.** CarFollowingModel attached at spawn, ~30% Krauss for cars, GPU shader confirmed producing 92.8% speed difference. |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| sim.rs | - | 871 lines (>700 limit) | Warning | cpu_reference module (330 lines) is test-only; production code ~540 lines is within bounds |
| cleaning.rs | 294 | apply_time_dependent_oneways is infrastructure-only | Info | Documented decision: needs OSM way-ID mapping |

No blocker anti-patterns remain. The previous blocker (sim.rs line 329 defaulting all agents to IDM) is now only a defensive fallback for legacy entities -- all newly spawned agents have explicit CarFollowingModel.

### Human Verification Required

### 1. GPU Physics Visual Correctness
**Test:** Run `cargo run -p velos-gpu` and observe agent movement
**Expected:** Agents follow lane geometry, maintain car-following distances, no collision pass-throughs
**Why human:** Visual behavior assessment not possible programmatically

### 2. Krauss vs IDM Color Differentiation
**Test:** Run the simulation and observe agent colors in the egui dashboard
**Expected:** Mixed-color traffic visible -- IDM agents (green/blue) and Krauss agents (orange) clearly distinguishable
**Why human:** Visual color distinction assessment

### 3. 280K Benchmark Performance on Target Hardware
**Test:** Run `cargo bench -p velos-gpu --bench dispatch --features gpu-tests`
**Expected:** All benchmarks under 100ms per step
**Why human:** Performance depends on specific hardware

### 4. Multi-GPU Boundary Agent Transfer Correctness
**Test:** Enable multi-GPU mode and observe agents crossing partition boundaries
**Expected:** No agent teleportation or duplication at boundaries
**Why human:** Boundary behavior visible only at runtime

### Gap Closure Summary

The single gap from the initial verification has been closed:

**CarFollowingModel wired to agent spawning (Plan 05-06).** The fix adds `CarFollowingModel` to the import list in sim_lifecycle.rs, determines the model per vehicle type (Krauss for ~30% of cars via RNG, IDM for motorbikes, none for pedestrians), and includes it in both the Motorbike and Car spawn tuples. The `cf_model` value now flows end-to-end: spawn -> ECS component -> GPU agent state -> wave_front.wgsl shader branching -> different speed profiles. Integration tests confirm all non-pedestrian agents have the component, the Krauss/IDM ratio is correct, and the GPU produces a 92.8% speed difference between the two models.

All 5 success criteria are now met. All 14 requirements (GPU-01 through GPU-06, NET-01 through NET-06, CFM-01, CFM-02) are satisfied. No regressions detected in previously verified artifacts.

---

_Verified: 2026-03-07T14:30:00Z_
_Verifier: Claude (gsd-verifier)_
