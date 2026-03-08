---
phase: 07-intelligence-routing-prediction
plan: 02
subsystem: cost-function-profiles
tags: [cost-function, agent-profiles, demand-spawning, route-choice]
dependency_graph:
  requires: []
  provides: [CostWeights, PROFILE_WEIGHTS, route_cost, AgentProfile, assign_profile, ProfileDistribution, EdgeAttributes]
  affects: [velos-core, velos-demand]
tech_stack:
  added: []
  patterns: [data-driven-profiles, distance-weighted-blend, flag-bit-encoding]
key_files:
  created:
    - crates/velos-core/src/cost.rs
    - crates/velos-demand/src/profile.rs
  modified:
    - crates/velos-core/src/lib.rs
    - crates/velos-demand/src/lib.rs
    - crates/velos-demand/src/spawner.rs
    - crates/velos-demand/Cargo.toml
decisions:
  - "RoadClass duplicated in cost.rs to avoid velos-core -> velos-net circular dependency"
  - "r#gen() syntax required for Rust 2024 edition (gen is reserved keyword)"
  - "Task 3 (EdgeAttributes heuristics) merged into Task 1 since same file and natural cohesion"
metrics:
  duration: 4m
  completed: "2026-03-07"
---

# Phase 7 Plan 02: Multi-Factor Cost Function & Agent Profiles Summary

6-factor weighted cost function (time/comfort/safety/fuel/signal_delay/prediction_penalty) with 8 agent profiles and distance-weighted blend routing cost

## What Was Built

### CostWeights & route_cost (crates/velos-core/src/cost.rs)
- `AgentProfile` enum: 8 variants (Commuter, Bus, Truck, Emergency, Tourist, Teen, Senior, Cyclist) with `#[repr(u8)]` for 4-bit flag encoding
- `CostWeights` struct: 6 f32 fields matching architecture doc specification
- `PROFILE_WEIGHTS` const array: 8-entry lookup table with exact values from research doc
- `route_cost()`: Distance-weighted blend -- edges within 2km cumulative use observed travel times, faraway edges blend toward predicted times; low-confidence predictions (< 0.5) incur penalty
- `encode_profile_in_flags()` / `decode_profile_from_flags()`: Bits 4-7 of GpuAgentState.flags, preserving bits 0-3
- `EdgeAttributes` struct and `default_edge_attributes()`: Road-class-based heuristic derivation for HCMC POC (no ground truth data)
- `RoadClass` enum: Local copy to avoid circular dependency with velos-net

### Profile Assignment (crates/velos-demand/src/profile.rs)
- `ProfileDistribution` struct: Configurable percentages (default 60/15/15/10 for Commuter/Tourist/Teen/Senior)
- `assign_profile()`: 1:1 mapping for Bus/Truck/Emergency/Bicycle/Pedestrian; random distribution for Car/Motorbike
- `SpawnRequest` extended with `profile: AgentProfile` field
- `Spawner` updated with `ProfileDistribution` and builder method `with_profile_distribution()`

## Test Coverage

| Module | Tests | Coverage |
|--------|-------|----------|
| cost.rs | 15 | All profile weights, route_cost variants, flag encoding, edge attributes |
| profile.rs | 9 | All 1:1 mappings, distribution stats, determinism, validation |
| Total | 24 | All plan behaviors verified |

## Deviations from Plan

### Auto-merged Tasks

**1. [Rule 3 - Blocking] Task 3 merged into Task 1**
- **Issue:** Task 3 (EdgeAttributes heuristics) targets the same file as Task 1 (cost.rs) with overlapping content
- **Fix:** Implemented `default_edge_attributes()` and its tests in Task 1's commit for natural cohesion
- **Impact:** 2 commits instead of 3; all functionality present

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Rust 2024 reserved keyword `gen`**
- **Found during:** Task 2
- **Issue:** `rng.gen()` fails to compile in Rust 2024 edition where `gen` is reserved
- **Fix:** Changed to `rng.r#gen()` raw identifier syntax
- **Files modified:** crates/velos-demand/src/profile.rs

**2. [Rule 2 - Missing] RoadClass circular dependency avoidance**
- **Found during:** Task 1
- **Issue:** velos-net owns RoadClass but velos-core cannot depend on velos-net (leaf crate)
- **Fix:** Defined local `RoadClass` enum in cost.rs; documented in module comment
- **Files modified:** crates/velos-core/src/cost.rs

## Commits

| Task | Commit | Description |
|------|--------|-------------|
| 1+3 | 9524437 | feat(07-02): add multi-factor cost function with 8 agent profiles |
| 2 | a84824c | feat(07-02): add profile assignment in demand spawner |

## Verification Results

- `cargo test -p velos-core --lib cost` -- 15/15 passed
- `cargo test -p velos-demand --lib profile` -- 9/9 passed (12 including tod_profile filter matches)
- `cargo clippy -p velos-core -p velos-demand -- -D warnings` -- clean, 0 warnings
