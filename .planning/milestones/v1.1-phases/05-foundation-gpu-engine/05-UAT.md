---
status: testing
phase: 05-foundation-gpu-engine
source: [05-01-SUMMARY.md, 05-02-SUMMARY.md, 05-03-SUMMARY.md, 05-04-SUMMARY.md, 05-05-SUMMARY.md, 05-06-SUMMARY.md]
started: 2026-03-07T14:00:00Z
updated: 2026-03-07T14:00:00Z
---

## Current Test

number: 1
name: Cold Start Smoke Test
expected: |
  Run `cargo test --workspace`. All crates compile without errors and all tests pass (zero failures).
  Run `cargo clippy --workspace`. No errors (warnings acceptable).
awaiting: user response

## Tests

### 1. Cold Start Smoke Test
expected: Run `cargo test --workspace`. All crates compile without errors and all tests pass. Run `cargo clippy --workspace`. No errors.
result: [pending]

### 2. Fixed-Point Arithmetic Correctness
expected: Run `cargo test -p velos-core fixed_point`. All 18 fixed-point tests pass — roundtrip conversions (f64 -> fixed -> f64), multiplication edge cases, and Q16.16/Q12.20/Q8.8 arithmetic are correct.
result: [pending]

### 3. Krauss Car-Following Model
expected: Run `cargo test -p velos-vehicle krauss`. All 11 Krauss tests pass — safe speed calculation, stochastic dawdle, and velocity update produce SUMO-faithful results.
result: [pending]

### 4. SUMO Network Import
expected: Run `cargo test -p velos-net sumo_net_import`. The SUMO .net.xml importer parses the simple.net.xml fixture, producing a valid RoadGraph with edges, junctions, connections, and signal plans. All 11 tests pass.
result: [pending]

### 5. SUMO Demand Import
expected: Run `cargo test -p velos-net sumo_rou_import`. The SUMO .rou.xml importer parses vehicles, trips, flows, vTypes, persons, and calibrators from the fixture. All 12 tests pass.
result: [pending]

### 6. Network Cleaning Pipeline
expected: Run `cargo test -p velos-net cleaning`. The 7-step cleaning pipeline (remove disconnected, merge short, infer lanes, overrides, motorbike-only, time-dependent one-ways, validate) processes a graph correctly. All cleaning tests pass.
result: [pending]

### 7. GPU Wave-Front Shader Validation
expected: Run `cargo test -p velos-gpu wave_front_validation`. CPU reference matches expected IDM/Krauss behavior, lane sorting produces front-to-back order, PCG RNG is deterministic. All tests pass.
result: [pending]

### 8. Multi-GPU Partitioning & Boundary Protocol
expected: Run `cargo test -p velos-gpu boundary_protocol`. BFS-based graph partitioning produces balanced partitions, boundary agents transfer correctly between partitions via outbox/inbox protocol. All 7 tests pass.
result: [pending]

### 9. CarFollowingModel Spawn Wiring
expected: Run `cargo test -p velos-gpu cf_model_switch`. Cars spawn with ~30% Krauss / ~70% IDM ratio. Motorbikes always get IDM. GPU shader produces different behavior for Krauss vs IDM agents (Krauss ~92% lower avg speed). All 6 tests pass.
result: [pending]

### 10. 280K Agent Benchmark
expected: Run `cargo bench -p velos-gpu --bench dispatch`. Criterion benchmarks for lane_sort, single_gpu, multi_gpu_2, and multi_gpu_4 all complete. Frame times are under 100ms for 280K agents.
result: [pending]

### 11. Visual Simulation (egui App)
expected: Run `cargo run -p velos-gpu`. The egui window opens showing agents moving on a road network. Agents are color-coded by car-following model: IDM agents in green/blue tones, Krauss agents in orange. Agents move and don't freeze.
result: [pending]

### 12. 5-District Demand Profiles
expected: Run `cargo test -p velos-demand tod_5district`. Weekday and weekend ToD profiles for 5 HCMC districts produce correct shapes (D1 CBD sharp peaks, D5 Cholon early market, D10 broad residential). OD matrix produces ~280K trips at peak. All 12 tests pass.
result: [pending]

## Summary

total: 12
passed: 0
issues: 0
pending: 12
skipped: 0

## Gaps

[none yet]
