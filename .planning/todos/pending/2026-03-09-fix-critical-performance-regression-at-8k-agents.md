---
created: "2026-03-09T12:55:22.601Z"
title: Fix critical performance regression at 8K agents
area: gpu
priority: critical
files:
  - crates/velos-gpu/src/sim_junction.rs
  - crates/velos-gpu/src/sim_helpers.rs
  - crates/velos-gpu/src/sim_render.rs
  - crates/velos-gpu/src/map_tiles.rs
  - crates/velos-gpu/src/sim.rs
---

## Problem

At only 8,302 agents (3% of 280K target), frame time is 83.4ms — over 5x the 16ms budget for 60fps. Resource usage: 57% CPU, 63.2% GPU on Apple Metal, 7 threads, 434 idle wake-ups.

Visual observation: only 4 agents visible on screen despite 8K+ count, suggesting most agents are off-screen or rendering pipeline has a culling/visibility issue.

Key metrics from Activity Monitor:
- Process: velos-gpu
- CPU: 57.0%
- GPU: 63.2%
- Threads: 7
- Frame: 83.4ms
- Agents: 8,302 (Motorbike: 4951, Car: 1720, Bus: 278, Bicycle: 406, Truck: 412, Emergency: 68, Pedestrian: 467)

## Solution

Profile and fix these suspected bottlenecks (in priority order):

1. **Junction traversal HashMap lookups** — `step_junction_traversal` does `HashMap<u32, JunctionData>` lookups per agent per frame. With 8K agents across many junctions, this is O(n) hash lookups. Consider pre-grouping agents by junction node.

2. **build_instances full iteration** — `build_instances` in sim_render.rs iterates ALL agents to build GPU instance buffers every frame. At 8K this should be fast, but check for N+1 ECS queries inside the loop.

3. **Map tile decode thread contention** — MapTileRenderer background thread may be contending with simulation thread. Check mpsc channel polling frequency and tile decode CPU cost.

4. **N+1 ECS queries in sim_helpers** — `apply_vehicle_update` and `advance_to_next_edge` may do multiple separate `query_one_mut` calls per agent. Batch queries where possible.

5. **Excessive junction precomputation** — If pass-through node filtering isn't working correctly, we may have 10x more "junctions" than real intersections, inflating conflict detection cost.

6. **GPU dispatch inefficiency** — 63.2% GPU at 8K agents suggests shaders may be overdrawing or map tile geometry is excessive (check triangle count from earcut).

Target: <16ms frame time at 8K agents, <33ms at 50K agents.
