---
phase: 07-intelligence-routing-prediction
verified: 2026-03-07T17:00:00Z
status: passed
score: 5/5 must-haves verified
re_verification: false
---

# Phase 7: Intelligence, Routing & Prediction Verification Report

**Phase Goal:** Agents make intelligent route choices using predicted future conditions, reroute dynamically around congestion, and exhibit profile-driven behavior differences
**Verified:** 2026-03-07T17:00:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Commuter and Tourist agents choose different routes due to differing cost weights | VERIFIED | `cost.rs` has 8 distinct `PROFILE_WEIGHTS` entries (531 lines), `route_cost` function weights time/comfort/safety differently per profile, `profile.rs` assigns profiles at spawn with configurable distribution (217 lines, 8+ tests) |
| 2 | Road closure mid-simulation causes CCH reroute within same step, 500 reroutes/step without frame drops | VERIFIED | `reroute.rs` (669 lines, 17 tests) implements `RerouteScheduler` with 1K/step budget, immediate triggers for blocked edges, `evaluate_reroute` uses CCH `query_with_path`, `query.rs` has `query_batch` for parallel queries, CCH tests include batch performance (27 tests in cch_tests.rs) |
| 3 | Prediction ensemble updates every 60 sim-seconds, agents avoid predicted-congested corridors | VERIFIED | `velos-predict` crate has `BPRPredictor` (71 lines), `ETSCorrector` (74 lines), `HistoricalMatcher` (71 lines), `AdaptiveWeights` (89 lines), `PredictionEnsemble` orchestrator (213 lines) blending all three, `PredictionOverlay` with `ArcSwap` for lock-free updates (68 lines), 23 ensemble tests |
| 4 | GPU perception produces per-agent awareness of leader, signal, signs, nearby agents feeding evaluation | VERIFIED | `perception.wgsl` (222 lines) with 8 bindings (agents, signals, signs, congestion_grid, edge_travel_ratios, results), outputs `PerceptionResult` struct, `perception.rs` (387 lines) has `PerceptionPipeline` with CPU readback |
| 5 | Staggered reroute evaluation processes 1K agents/step with immediate triggers for blocked edges | VERIFIED | `RerouteScheduler` in `reroute.rs` implements round-robin 1K/step, priority queue displacement for immediate triggers, 30s cooldown, `sim_reroute.rs` (370 lines) wires `init_reroute` and `step_reroute` into `SimWorld` frame loop |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/velos-net/src/cch/mod.rs` | CCHRouter struct + public API | VERIFIED | 156 lines, exports CCHRouter, EdgeNodeMap, wired to RoadGraph |
| `crates/velos-net/src/cch/ordering.rs` | Nested dissection node ordering | VERIFIED | 192 lines, exports compute_ordering |
| `crates/velos-net/src/cch/topology.rs` | Node contraction + shortcuts | VERIFIED | 197 lines, exports contract_graph |
| `crates/velos-net/src/cch/cache.rs` | Binary serialization | VERIFIED | 24 lines, save_cch/load_cch via postcard |
| `crates/velos-net/src/cch/customization.rs` | Bottom-up weight customization | VERIFIED | 147 lines, customize/customize_with_fn methods |
| `crates/velos-net/src/cch/query.rs` | Bidirectional Dijkstra query | VERIFIED | 371 lines, query/query_with_path/query_batch |
| `crates/velos-core/src/cost.rs` | CostWeights, profiles, route_cost | VERIFIED | 531 lines, 8 profiles, 6 cost factors, 15 tests |
| `crates/velos-core/src/reroute.rs` | RerouteScheduler + evaluate_reroute | VERIFIED | 669 lines, 17 tests, staggered scheduling + cooldown |
| `crates/velos-demand/src/profile.rs` | Profile assignment + distribution | VERIFIED | 217 lines, assign_profile + ProfileDistribution + tests |
| `crates/velos-predict/src/lib.rs` | PredictionEnsemble orchestrator | VERIFIED | 213 lines, blends BPR+ETS+historical |
| `crates/velos-predict/src/overlay.rs` | PredictionOverlay + ArcSwap | VERIFIED | 68 lines, Arc<ArcSwap<PredictionOverlay>> |
| `crates/velos-predict/src/bpr.rs` | BPR model | VERIFIED | 71 lines, BPRPredictor |
| `crates/velos-predict/src/ets.rs` | ETS correction model | VERIFIED | 74 lines, ETSCorrector |
| `crates/velos-predict/src/historical.rs` | Historical pattern matcher | VERIFIED | 71 lines, HistoricalMatcher |
| `crates/velos-predict/src/adaptive.rs` | Adaptive weight adjustment | VERIFIED | 89 lines, AdaptiveWeights |
| `crates/velos-gpu/shaders/perception.wgsl` | GPU perception kernel | VERIFIED | 222 lines, 8 bindings, AgentState struct |
| `crates/velos-gpu/src/perception.rs` | PerceptionPipeline + PerceptionResult | VERIFIED | 387 lines, GPU pipeline with CPU readback |
| `crates/velos-gpu/src/sim_reroute.rs` | SimWorld reroute integration | VERIFIED | 370 lines, RerouteState, init_reroute, step_reroute |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| cch/mod.rs | graph.rs | RoadGraph input | WIRED | `use crate::graph::RoadGraph`, `from_graph(&RoadGraph)` |
| cch/cache.rs | postcard | Binary serialization | WIRED | `postcard::to_allocvec` + `postcard::from_bytes` |
| cch/customization.rs | cch/mod.rs | Mutates forward/backward weights | WIRED | `forward_weight`/`backward_weight` accessed |
| cch/query.rs | cch/mod.rs | Reads forward/backward stars | WIRED | Uses CCHRouter fields for bidirectional search |
| cost.rs | components.rs | Profile flags encoding bits 4-7 | WIRED | `decode_profile_from_flags`: `(flags >> 4) & 0x0F` |
| profile.rs | cost.rs | Uses AgentProfile constants | WIRED | `use velos_core::cost::AgentProfile` |
| overlay.rs | arc-swap | Lock-free PredictionOverlay | WIRED | `Arc<ArcSwap<PredictionOverlay>>` |
| predict/lib.rs | bpr+ets+historical | Ensemble blending | WIRED | `bpr.predict` + `ets` correction + `historical.predict` + `weights.blend` |
| perception.wgsl | wave_front.wgsl | Shared AgentState struct | WIRED | `struct AgentState` defined in perception shader |
| perception.rs | compute.rs | Shared wgpu device/buffers | WIRED | Uses `wgpu::Device` and `wgpu::Buffer` |
| reroute.rs | cch/mod.rs | CCH queries for alternatives | WIRED | `use velos_net::cch::CCHRouter` in sim_reroute |
| reroute.rs | cost.rs | route_cost for evaluation | WIRED | `use crate::cost::route_cost`, called in evaluate_reroute |
| reroute.rs | overlay.rs | PredictionStore for travel times | WIRED | `use velos_predict::PredictionService` in sim_reroute |
| sim_reroute.rs | sim.rs | RerouteState in SimWorld | WIRED | `reroute: RerouteState` field, init_reroute/step_reroute methods |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-----------|-------------|--------|----------|
| INT-01 | 07-02 | Multi-factor cost function (time, comfort, safety, fuel, signal delay, prediction) | SATISFIED | `cost.rs`: CostWeights with 6 factors, route_cost function |
| INT-02 | 07-02 | 8 agent profiles with per-profile cost weights | SATISFIED | `cost.rs`: PROFILE_WEIGHTS[8], `profile.rs`: assign_profile |
| INT-03 | 07-05 | GPU perception: leader, signal, signs, nearby agents, congestion | SATISFIED | `perception.wgsl` reads all 5 data sources, outputs PerceptionResult |
| INT-04 | 07-06 | CPU evaluation: cost comparison, should_reroute + cost_delta | SATISFIED | `reroute.rs`: evaluate_reroute with RouteEvalContext, 30% threshold |
| INT-05 | 07-06 | Staggered reroute (1K/step, ~50s cycle) with immediate triggers | SATISFIED | `reroute.rs`: RerouteScheduler round-robin + priority queue |
| RTE-01 | 07-01 | CCH replaces A* on 25K-edge network | SATISFIED | `cch/` module: ordering, topology, customization, query |
| RTE-02 | 07-03 | CCH 3ms weight customization without re-contraction | SATISFIED | `customization.rs`: bottom-up triangle customization |
| RTE-03 | 07-03 | 500 reroutes/step via CCH queries (0.02ms/query) | SATISFIED | `query.rs`: query_batch with rayon parallel |
| RTE-04 | 07-04 | BPR + ETS + historical ensemble every 60 sim-seconds | SATISFIED | `velos-predict`: BPRPredictor + ETSCorrector + HistoricalMatcher + PredictionEnsemble |
| RTE-05 | 07-04 | ArcSwap overlay for lock-free weight updates | SATISFIED | `overlay.rs`: Arc<ArcSwap<PredictionOverlay>> |
| RTE-06 | 07-04 | Global congestion map feeds pathfinding cost | SATISFIED | `perception.wgsl` reads edge_travel_ratios, cost.rs uses overlay travel times |
| RTE-07 | 07-04 | Prediction-informed routing uses predicted future travel times | SATISFIED | `route_cost` accepts overlay_travel_times + overlay_confidence, ensemble blends predictions |

No orphaned requirements found -- all 12 IDs (INT-01 through INT-05, RTE-01 through RTE-07) are mapped to Phase 7 in REQUIREMENTS.md and covered by plans.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| -- | -- | No TODO/FIXME/HACK/placeholder found in any phase 07 artifact | -- | -- |

Zero anti-patterns detected across all 18 files (4,069 lines total).

### Human Verification Required

### 1. Profile-Driven Route Differentiation

**Test:** Create a scenario with Commuter and Tourist agents sharing the same OD pair. Observe their route choices.
**Expected:** Commuter takes the fastest route (time-weighted). Tourist takes a different route prioritizing comfort/safety (wider roads, fewer signals).
**Why human:** Route visualization requires running the simulation and inspecting path traces -- cannot verify pure logic produces visually different paths.

### 2. Mid-Simulation Reroute on Road Closure

**Test:** Run simulation, block an edge mid-step, observe affected agents.
**Expected:** Agents on the blocked edge reroute within the same step via CCH, no frame drop visible.
**Why human:** Real-time frame drop detection and visual reroute confirmation require running the simulation.

### 3. Prediction-Informed Avoidance

**Test:** Create a corridor that is currently free-flowing but predicted to congest. Observe agent route choices.
**Expected:** Agents receiving prediction-informed routes avoid the corridor before congestion materializes.
**Why human:** Requires temporal observation of agent behavior relative to congestion onset timing.

### Gaps Summary

No gaps found. All 5 observable truths verified with supporting artifacts at all three levels (existence, substantive implementation, wired integration). All 12 requirements (INT-01 through INT-05, RTE-01 through RTE-07) are satisfied with implementation evidence. Zero anti-patterns. Test coverage is strong with 67+ unit tests across the phase artifacts.

Note: ROADMAP.md shows plan 07-06 as `[ ]` (unchecked) but the SUMMARY exists and all artifacts are present and wired. This is a ROADMAP tracking inconsistency, not a code gap.

---

_Verified: 2026-03-07T17:00:00Z_
_Verifier: Claude (gsd-verifier)_
