# VELOS v2 Simulation Engine

## Resolves: W1 (Single-GPU), W2 (EVEN/ODD Dispatch), W3 (Numerical Stability), W14 (Cross-GPU Determinism)

---

## 1. Multi-GPU Spatial Decomposition (W1)

### Problem

The v1 architecture assumes a single GPU. At 500K agents on an RTX 4090 (24GB VRAM), buffer capacity is exhausted. HCMC POC targets 280K agents — manageable on one GPU, but with no growth path.

### Solution: Single-Node Multi-GPU with Graph Partitioning

For POC, we target 2-4 GPUs on a single node (e.g., 2x RTX 4090 or 1x A100 80GB). This avoids the complexity of cross-node networking while doubling/quadrupling capacity.

**Partitioning Strategy: METIS Graph Bisection**

```
HCMC Road Network (~25K edges, ~15K junctions)
          |
    METIS k-way partition (k = num_gpus)
          |
    +-----+-----+-----+
    | P0  | P1  | P2  | P3   (4 partitions)
    | D1  | D3  | D5  | BTh  (Districts mapped to GPUs)
    +-----+-----+-----+
```

Each GPU owns a partition of the road network and all agents currently on that partition's edges.

**Boundary Agent Protocol:**

When an agent crosses from partition P0 to P1:

1. At step N, agent reaches terminal edge of P0
2. P0 writes agent to `outbox_buffer[P0→P1]` (GPU-side staging)
3. After step N completes, CPU reads all outbox buffers (one PCIe transfer per GPU)
4. CPU routes agents to correct destination GPU's `inbox_buffer`
5. At step N+1, P1 spawns agent from inbox at boundary edge entry

**Cost analysis:**
- Boundary edges for HCMC 4-way partition: ~200-400 edges (1.5% of network)
- Agents crossing per step at 0.1s Dt: ~500-1000 (0.3% of total)
- Data per crossing agent: 64 bytes (position, speed, route, profile)
- Transfer volume: ~64KB per step — negligible vs. PCIe 4.0 bandwidth (32 GB/s)

**Implementation:**

```rust
pub struct GpuPartition {
    device: wgpu::Device,
    queue: wgpu::Queue,
    agent_buffers: AgentBufferPool,
    network_subset: NetworkSubset,  // edges + junctions in this partition
    outbox: Vec<StagingBuffer>,     // one per neighbor partition
    inbox: StagingBuffer,
}

pub struct MultiGpuScheduler {
    partitions: Vec<GpuPartition>,
    boundary_map: BoundaryMap,  // edge_id -> (src_partition, dst_partition)
    transfer_stats: TransferMetrics,
}
```

**Why not multi-node?** For 280K agents, single-node multi-GPU is sufficient and avoids:
- Network latency for agent handoff (PCIe: 0.001ms vs. Ethernet: 0.1ms)
- Distributed state synchronization complexity
- Clock synchronization issues

Multi-node is a v3 concern when scaling to 2M+ agents (full HCMC metro area).

---

## 2. Deterministic Wave-Front Dispatch (W2)

### Problem

The v1 EVEN/ODD semi-synchronous dispatch has no formal convergence guarantee. In dense traffic (>100 vehicles/km/lane), the collision correction pass can cascade — fixing one agent's gap violates another's. No proof exists that a single correction pass converges.

### Solution: Per-Lane Wave-Front (Gauss-Seidel) Dispatch

Instead of splitting by parity (EVEN/ODD), process agents **per-lane in headway order** (front to back). Each agent reads its leader's **already-updated** position from the current step — zero stale data, zero collision possibility.

**Algorithm:**

```
For each lane L (parallel across lanes):
    Sort agents on L by position (descending — leader first)
    For each agent A in lane L (sequential within lane):
        leader = previous agent in sorted order (already updated)
        A.acceleration = IDM(A.speed, A.position, leader.speed, leader.position)
        A.speed += A.acceleration * dt
        A.position += A.speed * dt
```

**GPU Mapping:**

- Each lane maps to one GPU workgroup (256 threads)
- Within a workgroup, agents are processed sequentially (wave-front)
- Across workgroups (different lanes), processing is fully parallel
- HCMC network: ~50K lane-segments → 50K workgroups → full GPU occupancy

**Why this works:**

| Property | EVEN/ODD (v1) | Wave-Front (v2) |
|----------|---------------|-----------------|
| Stale data | 1-step stale for same-parity leaders | Zero — leader always current |
| Collision risk | Requires correction pass | Impossible by construction |
| Convergence proof | None | Trivial: Gauss-Seidel on 1D chain |
| Parallelism | 250K threads | 50K workgroups (sufficient for GPU saturation) |
| Dense traffic | Correction cascade risk | No correction needed |

**Performance Budget:**

```
Workgroups: 50K lanes
Avg agents/lane: 5.6 (280K / 50K)
Max agents/lane: ~30 (dense arterial)
Sequential within lane: 30 * 0.001ms = 0.03ms (trivial)
GPU dispatch: 50K workgroups @ 256 threads = single dispatch
Total: ~2ms for 280K agents (vs. 3ms for EVEN+ODD+correction in v1)
```

**Lane-Change Interaction:**

Lane changes require reading adjacent-lane state. Handled by a two-phase approach:
1. **Phase 1 (parallel):** Compute lane-change desire for all agents using previous-step neighbor data
2. **Phase 2 (wave-front):** Execute accepted lane changes + car-following in headway order

This is safe because lane-change acceptance is evaluated conservatively (MOBIL safety criterion uses previous-step gaps, which are always larger than or equal to current gaps).

---

## 3. Numerical Stability (W3)

### Problem

At Dt=0.1s with highway speeds (120 km/h = 33.3 m/s), a ballistic update can overshoot a 50m edge by 3.3m per step. The v1 architecture doesn't address this formally.

### Solution: CFL-Bounded Time Stepping + Edge Transition Guard

**CFL Condition for Traffic:**

For numerical stability, the Courant-Friedrichs-Lewy number must satisfy:

```
CFL = v_max * dt / dx_min < 1
```

Where:
- `v_max` = maximum vehicle speed (33.3 m/s for 120 km/h)
- `dt` = time step (0.1s)
- `dx_min` = minimum edge length

```
CFL = 33.3 * 0.1 / dx_min = 3.33 / dx_min
```

For CFL < 1: `dx_min > 3.33m`. HCMC has some very short edges (driveways, alley entrances). Solution:

**Adaptive Sub-Stepping for Short Edges:**

```rust
fn update_agent(agent: &mut Agent, dt: f64, edge_length: f64) {
    let cfl = agent.speed * dt / edge_length;
    if cfl < 1.0 {
        // Normal single-step update
        integrate(agent, dt);
    } else {
        // Sub-step: divide dt until CFL < 1
        let n_sub = (cfl.ceil()) as u32;
        let sub_dt = dt / n_sub as f64;
        for _ in 0..n_sub {
            integrate(agent, sub_dt);
        }
    }
}
```

In WGSL shader:

```wgsl
fn idm_update(agent_idx: u32, dt: f32) {
    let edge_len = edges[agents[agent_idx].edge_id].length;
    let v = agents[agent_idx].speed;
    let cfl = v * dt / edge_len;

    if (cfl < 1.0) {
        single_step(agent_idx, dt);
    } else {
        let n_sub = u32(ceil(cfl));
        let sub_dt = dt / f32(n_sub);
        for (var i: u32 = 0u; i < n_sub; i = i + 1u) {
            single_step(agent_idx, sub_dt);
        }
    }
}
```

**Edge Transition Guard:**

After position update, if `position > edge_length`:

```
overflow = position - edge_length
next_edge = route[current_route_index + 1]
if next_edge exists AND has_capacity(next_edge):
    position = overflow  // carry over to next edge
    edge_id = next_edge
    route_index += 1
else:
    position = edge_length  // clamp at edge end (waiting)
    speed = 0  // stopped at junction
```

This prevents teleportation and handles edge boundaries cleanly.

**IDM Safe Implementation:**

```wgsl
fn safe_pow4(x: f32) -> f32 {
    let x2 = x * x;
    return x2 * x2;  // avoid pow() undefined behavior
}

fn idm_acceleration(v: f32, s: f32, delta_v: f32, v0: f32, s0: f32, T: f32, a_max: f32, b: f32) -> f32 {
    // Zero-speed kickstart: prevent deadlock when v=0 and leader far ahead
    let v_eff = max(v, 0.1);  // minimum 0.1 m/s to prevent division by zero

    let s_star = s0 + v_eff * T + (v_eff * delta_v) / (2.0 * sqrt(a_max * b));
    let s_star_clamped = max(s_star, s0);  // s* >= s0 always

    let free_road = 1.0 - safe_pow4(v / max(v0, 0.1));
    let interaction = -safe_pow4(s_star_clamped / max(s, 0.1));  // prevent div by zero on gap

    let acc = a_max * (free_road + interaction);
    return clamp(acc, -9.0, a_max);  // physical limits: max 9 m/s^2 deceleration
}
```

---

## 4. Fixed-Point Arithmetic for Cross-GPU Determinism (W14)

### Problem

IEEE 754 floating-point results vary across GPU vendors (AMD vs NVIDIA). The v1 architecture acknowledges this but offers no solution.

### Solution: Fixed-Point Integer Arithmetic for Position and Speed

Use 32-bit fixed-point for position (Q16.16) and speed (Q12.20):

```
Position Q16.16:
  - Integer part: 16 bits → range [0, 65535] meters (65km, sufficient for any edge)
  - Fractional part: 16 bits → resolution 0.000015m (~0.015mm)

Speed Q12.20:
  - Integer part: 12 bits → range [0, 4095] (interpreted as 0-409.5 m/s, ~1474 km/h)
  - Fractional part: 20 bits → resolution ~0.001 mm/s
```

**WGSL Implementation:**

```wgsl
// Fixed-point types as i32
alias FixPos = i32;   // Q16.16
alias FixSpd = i32;   // Q12.20

const POS_SCALE: i32 = 65536;    // 1 << 16
const SPD_SCALE: i32 = 1048576;  // 1 << 20

fn fix_mul(a: i32, b: i32, scale: i32) -> i32 {
    // 64-bit intermediate to prevent overflow
    // WGSL doesn't have i64, so use two i32s
    let ah = a >> 16;
    let al = a & 0xFFFF;
    let bh = b >> 16;
    let bl = b & 0xFFFF;
    let mid = ah * bl + al * bh;
    let result = ah * bh * scale + mid + (al * bl) / scale;
    return result;
}

fn idm_fixed(v: FixSpd, s: FixPos, delta_v: FixSpd, params: IDMParams) -> FixSpd {
    // All arithmetic in fixed-point — deterministic across GPUs
    // ...
}
```

**Tradeoff:**
- Deterministic: yes, bitwise identical across AMD/NVIDIA/Intel
- Performance cost: ~20% slower than float32 due to manual 64-bit emulation in WGSL
- Acceptable for POC where correctness > throughput

**Fallback:** If fixed-point performance is unacceptable, use float32 with `@invariant` WGSL attribute and accept "statistical equivalence" (positions within 1mm after 24h simulation). Document the delta per GPU vendor in validation report.

---

## 5. ECS Architecture

### Component Layout (SoA for GPU)

```rust
// Position on road network (GPU-resident)
#[derive(Copy, Clone)]
pub struct Position {
    pub edge_id: u32,
    pub lane_idx: u8,
    pub offset: FixedQ16_16,     // distance from edge start
    pub lateral: FixedQ8_8,      // lateral position within lane
}

// Kinematics (GPU-resident)
#[derive(Copy, Clone)]
pub struct Kinematics {
    pub speed: FixedQ12_20,
    pub acceleration: FixedQ8_24,
}

// Route (CPU-managed, GPU-readable)
pub struct Route {
    pub edges: SmallVec<[u32; 16]>,  // edge sequence
    pub current_idx: u16,
    pub destination: u32,            // destination junction
}

// Agent profile (CPU-managed, GPU-readable)
pub struct AgentProfile {
    pub agent_type: AgentType,       // Car, Motorbike, Bus, Bicycle, Pedestrian
    pub idm_params: IDMParams,       // desired_speed, min_gap, time_headway, max_accel, comfortable_decel
    pub cost_weights: CostWeights,   // time, comfort, safety, fuel
    pub reroute_timer: u16,          // steps until next reroute evaluation
}

#[derive(Copy, Clone)]
pub enum AgentType {
    Car,
    Motorbike,
    Bus,
    Bicycle,
    Pedestrian,
}
```

### GPU Buffer Layout

Each component type maps to a contiguous GPU buffer:

```
Buffer 0: Position[]       — 12 bytes × N agents
Buffer 1: Kinematics[]     — 8 bytes × N agents
Buffer 2: LeaderIndex[]    — 4 bytes × N agents (index of leader in same lane)
Buffer 3: IDMParams[]      — 20 bytes × N agents
Buffer 4: LaneChangeState[]— 8 bytes × N agents
```

Total VRAM per agent: ~52 bytes
280K agents: ~14.6 MB (trivial — leaves >23 GB free on RTX 4090)

### Staging Buffer Pattern (Race Condition Fix)

```rust
pub struct AgentBufferPool {
    // Double-buffered: GPU reads from front, CPU writes to back
    front: wgpu::Buffer,  // GPU reads during dispatch
    back: wgpu::Buffer,   // CPU writes new agents / GPU writes results

    // Sync fence
    frame_fence: wgpu::SubmissionIndex,
}

impl AgentBufferPool {
    pub fn begin_frame(&mut self) {
        // Wait for previous GPU dispatch to finish reading front buffer
        self.device.poll(wgpu::Maintain::WaitForSubmissionIndex(self.frame_fence));
        // Swap front ↔ back
        std::mem::swap(&mut self.front, &mut self.back);
    }

    pub fn spawn_agents(&mut self, agents: &[NewAgent]) {
        // Write to back buffer (not being read by GPU)
        self.queue.write_buffer(&self.back, offset, bytemuck::cast_slice(agents));
    }
}
```

---

## 6. Frame Execution Pipeline (280K Agents, 2 GPUs)

```
Step 1: [CPU]        Partition boundary agent transfer           0.1ms
Step 2: [CPU/rayon]  Per-lane leader sort (parallel per GPU)     1.5ms
Step 3: [GPU×2]      Upload staging buffers                      0.3ms
Step 4: [GPU×2]      Lane-change desire computation (parallel)   1.0ms
Step 5: [GPU×2]      Wave-front car-following (per-lane)         2.0ms
Step 6: [GPU×2]      Pedestrian social force (adaptive WG)       1.5ms
Step 7: [CPU/rayon]  CCH pathfinding (staggered, ~500/step)      0.5ms  (parallel with GPU)
Step 8: [CPU/rayon]  Route advance + edge transitions            0.3ms
Step 9: [GPU→CPU]    Download results                            0.3ms
Step 10: [CPU]       Prediction ensemble update (if due)         0.2ms  (async)
Step 11: [CPU]       Output recording + WebSocket broadcast      0.5ms
─────────────────────────────────────────────────────────────────
Total:                                                          ~8.2ms
Budget:                                                         100ms (10 steps/sec)
Headroom:                                                       ~92ms (11x margin)
```

The 11x margin is intentional. Real-world performance will be worse than theoretical due to:
- PCIe contention with 2 GPUs
- Memory allocation jitter
- OS scheduling
- Checkpoint I/O

Target: sustain 10 steps/sec with <15ms p99 frame time.

---

## 7. Gridlock Detection

```rust
pub struct GridlockDetector {
    pub stall_threshold: Duration,  // default: 30s sim-time
    pub check_interval: u32,       // default: every 100 steps
}

impl GridlockDetector {
    pub fn detect(&self, world: &World) -> Vec<GridlockCluster> {
        // 1. Find all agents with speed < 0.1 m/s for > stall_threshold
        let stalled: Vec<EntityId> = world.query::<(&Kinematics, &StallTimer)>()
            .filter(|(k, t)| k.speed.to_f32() < 0.1 && t.elapsed > self.stall_threshold)
            .collect();

        // 2. Build dependency graph: A waits for B (B is A's leader)
        let mut dep_graph = DiGraph::new();
        for agent in &stalled {
            if let Some(leader) = get_leader(agent) {
                if stalled.contains(&leader) {
                    dep_graph.add_edge(agent, leader);
                }
            }
        }

        // 3. Find strongly connected components (cycles = gridlock)
        tarjan_scc(&dep_graph)
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|scc| GridlockCluster { agents: scc })
            .collect()
    }

    pub fn resolve(&self, cluster: &GridlockCluster, strategy: GridlockStrategy) {
        match strategy {
            // Teleport: remove worst-positioned agent, respawn at origin
            GridlockStrategy::Teleport => { /* ... */ }
            // Reroute: force alternative route for random subset
            GridlockStrategy::Reroute => { /* ... */ }
            // Signal override: force green for blocked approach
            GridlockStrategy::SignalOverride => { /* ... */ }
        }
    }
}
```
