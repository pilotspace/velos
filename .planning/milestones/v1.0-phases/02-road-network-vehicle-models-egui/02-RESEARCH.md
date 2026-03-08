# Phase 2: Road Network & Vehicle Models + egui - Research

**Researched:** 2026-03-06
**Domain:** OSM road graph import, IDM/MOBIL vehicle models, egui-wgpu integration, demand/signals/gridlock
**Confidence:** MEDIUM (egui-wgpu/wgpu version mismatch is the main risk)

## Summary

Phase 2 adds four new crates (velos-net, velos-vehicle, velos-demand, velos-signal) and integrates egui for simulation controls. The core technical domains are: (1) parsing OSM PBF into a petgraph directed graph with rstar spatial indexing, (2) IDM car-following and MOBIL lane-change on CPU in f64, (3) OD matrix demand spawning with time-of-day profiles, (4) fixed-time traffic signals, (5) gridlock cycle detection, and (6) egui sidebar rendering on the same wgpu surface.

The most significant risk is **wgpu version compatibility**: the project uses wgpu 28, but egui-wgpu 0.33.3 (latest stable) requires wgpu ^27.0.1. This must be resolved before any egui work begins -- either by downgrading the project to wgpu 27, using a git dependency on egui's main branch if it has updated, or implementing egui rendering manually against the egui-wgpu Renderer API with version pinning.

**Primary recommendation:** Downgrade workspace wgpu from 28 to 27 to match egui-wgpu 0.33.3 compatibility. The wgpu 27->28 API differences are minor for this project's usage, and fighting version mismatches wastes time vs. building simulation features. Alternatively, check if egui-wgpu git main has wgpu 28 support at implementation time.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **HCMC road area**: District 1 core (Ben Thanh area), via Overpass API bounding box query, import highway=primary+secondary+tertiary+residential, project to local metres with equirectangular projection centered on District 1 centroid
- **Phase 2 agent types**: Motorbikes use IDM as placeholder (drive like cars, rightmost lane, no sublane). Pedestrians walk straight to destination (linear interpolation). Both spawned at DEM-03 ratios (80%/15%/5%). Visual: cars=blue rectangles, motorbikes=green triangles, pedestrians=white dots
- **egui layout**: Left sidebar (~240px fixed), simulation view fills remaining screen. Controls: Start/Pause/Reset + speed slider (0.1x-4x). Metrics: frame time, agent count by type, agents/sec
- **Crate additions**: velos-net, velos-vehicle, velos-demand, velos-signal (one per subsystem). Simulation tick stays in velos-gpu/app.rs GpuState::update()
- **Gridlock detection**: Claude's discretion on algorithm (simple visited-set BFS or Tarjan SCC)

### Claude's Discretion
- Gridlock detection algorithm choice
- Internal data structures for road graph, OD matrices, signal plans

### Deferred Ideas (OUT OF SCOPE)
- Scenario selector dropdown in egui
- Queue length metrics per intersection in egui
- Step-by-step single-frame advance button
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| VEH-01 | IDM car-following with ballistic stopping guard | IDM formula documented below; use f64 CPU-side, clamp accel to [-9.0, a_max], v_eff = max(v, 0.1) kickstart |
| VEH-02 | MOBIL lane-change with politeness=0.3 | MOBIL incentive + safety criteria documented; petgraph graph gives lane adjacency |
| NET-01 | OSM PBF import into directed graph with lanes/speed/oneway | osmpbf 0.3.8 + petgraph DiGraph; tag parsing for highway, lanes, maxspeed, oneway |
| NET-02 | rstar R-tree spatial index for neighbor queries | rstar crate with bulk_load; store agent positions for O(log n) nearest queries |
| NET-03 | Fixed-time traffic signals with green/amber/red phases | Custom signal controller struct; cycle time + phase splits from arch doc defaults |
| NET-04 | Edge-local to world coordinate transform | Equirectangular projection at load time; edge positions stored in metres matching Camera2D |
| RTE-01 | A* pathfinding on petgraph | petgraph::algo::astar with Euclidean heuristic on DiGraph |
| DEM-01 | OD matrix loader | Simple zone-to-zone trip table; hardcoded for District 1 POC |
| DEM-02 | Time-of-day profiles | Piecewise-linear factor curve from arch doc (AM/PM peaks at 1.0) |
| DEM-03 | Agent spawner with 80/15/5 vehicle type distribution | VehicleType enum + weighted random selection per spawn |
| GRID-01 | Gridlock detection: speed=0 >300s, resolve via teleport/reroute/signal override | Simple BFS cycle detection on waiting graph; recommend teleport for POC |
| APP-01 | egui controls: start/stop/pause/speed/reset | egui-wgpu + egui-winit integration on same wgpu surface; SimState enum |
| APP-02 | egui dashboard: frame time, agent count, throughput | egui::SidePanel::left with labels updated each frame from SimMetrics struct |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| osmpbf | 0.3.8 | Parse OSM PBF files into nodes/ways/relations | Only maintained parallel PBF reader for Rust; rayon-backed par_map_reduce |
| petgraph | latest (0.6.x) | Directed road graph (DiGraph) + A* pathfinding | De facto Rust graph library; built-in astar(), topological sort, SCC |
| rstar | latest (0.12.x) | R*-tree spatial index for agent neighbor queries | georust ecosystem standard; bulk_load, nearest_neighbor, locate_in_envelope |
| egui | 0.33.3 | Immediate-mode GUI for controls + dashboard | Rust-native, wgpu-compatible, minimal boilerplate |
| egui-wgpu | 0.33.3 | Render egui on existing wgpu surface | Official egui crate for wgpu integration |
| egui-winit | 0.33.3 | Bridge winit events to egui input | Official egui crate for winit event handling |
| rand | 0.8.x | Random number generation for demand spawning | Standard Rust RNG; WeightedIndex for vehicle type distribution |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| geo | latest | Equirectangular projection, Haversine distance | Converting lat/lon to local metres at OSM import |
| ordered-float | latest | Float keys in BTreeMap for signal timing | When you need Ord on f64 values |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| osmpbf | osmpbfreader | osmpbfreader stores all objects in memory (HashMap); osmpbf streams and is faster for large files |
| petgraph DiGraph | Custom adjacency list | petgraph has built-in A*, SCC, serialization; custom would be premature optimization |
| rstar | kiddo (k-d tree) | rstar handles 2D rectangles (road segments), not just points; better for envelope queries |

**Installation:**
```toml
# Workspace Cargo.toml additions
[workspace.dependencies]
osmpbf = "0.3"
petgraph = "0.6"
rstar = "0.12"
egui = "0.33"
egui-wgpu = "0.33"
egui-winit = "0.33"
rand = "0.8"
geo = "0.28"
wgpu = "27"  # DOWNGRADE from 28 for egui-wgpu compatibility
```

## Architecture Patterns

### Recommended Crate Structure
```
crates/
├── velos-net/
│   ├── src/
│   │   ├── lib.rs           # Re-exports
│   │   ├── error.rs         # NetError type
│   │   ├── graph.rs         # RoadGraph (petgraph DiGraph wrapper)
│   │   ├── osm_import.rs    # OSM PBF -> RoadGraph conversion
│   │   ├── projection.rs    # Lat/lon -> local metres (equirectangular)
│   │   └── spatial.rs       # R-tree wrapper for agent queries
│   └── tests/
│       ├── import_tests.rs  # OSM parsing tests
│       └── spatial_tests.rs # R-tree query tests
├── velos-vehicle/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── error.rs
│   │   ├── idm.rs           # IDM car-following model
│   │   ├── mobil.rs         # MOBIL lane-change model
│   │   └── types.rs         # VehicleType enum, IDMParams, MOBILParams
│   └── tests/
│       ├── idm_tests.rs     # Deceleration, stopping, free-flow tests
│       └── mobil_tests.rs   # Lane-change incentive/safety tests
├── velos-demand/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── od_matrix.rs     # OD matrix data structure + loader
│   │   ├── tod_profile.rs   # Time-of-day demand scaling
│   │   └── spawner.rs       # Agent spawner (OD + ToD + vehicle type)
│   └── tests/
│       └── spawner_tests.rs
├── velos-signal/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── controller.rs    # FixedTimeController + phase cycling
│   │   └── plan.rs          # SignalPlan, Phase, PhaseState
│   └── tests/
│       └── signal_tests.rs
```

### Pattern 1: IDM Car-Following (CPU f64)
**What:** Intelligent Driver Model computes acceleration from gap to leader, own speed, leader speed.
**When to use:** Every simulation tick for every car/motorbike agent.
**Example:**
```rust
// Source: traffic-simulation.de/info/info_IDM.html + arch doc 02-agent-models.md
pub struct IdmParams {
    pub v0: f64,   // desired speed (m/s)
    pub s0: f64,   // minimum gap (m)
    pub t_headway: f64,  // desired time headway (s)
    pub a: f64,    // max acceleration (m/s^2)
    pub b: f64,    // comfortable deceleration (m/s^2)
    pub delta: f64, // acceleration exponent (typically 4.0)
}

pub fn idm_acceleration(params: &IdmParams, v: f64, gap: f64, delta_v: f64) -> f64 {
    // Free-road acceleration term
    let v_ratio = v / params.v0;
    // safe_pow4: avoid pow() for numerical stability
    let free_term = 1.0 - v_ratio * v_ratio * v_ratio * v_ratio;

    // Desired dynamical gap s*
    let v_eff = v.max(0.1); // zero-speed kickstart
    let s_star = params.s0
        + v_eff * params.t_headway
        + (v * delta_v) / (2.0 * (params.a * params.b).sqrt());

    // Interaction (braking) term
    let gap_eff = gap.max(0.01); // avoid division by zero
    let interaction = (s_star / gap_eff) * (s_star / gap_eff);

    // IDM acceleration
    let accel = params.a * (free_term - interaction);

    // Clamp to physical limits
    accel.clamp(-9.0, params.a)
}

/// Ballistic stopping guard: prevent negative velocity after integration.
pub fn integrate_with_stopping_guard(v: f64, accel: f64, dt: f64) -> (f64, f64) {
    let v_new = v + accel * dt;
    if v_new < 0.0 {
        // Vehicle would go negative -- stop it at zero
        // Time to stop: t_stop = -v / accel
        let t_stop = (-v / accel).min(dt);
        let dx = v * t_stop + 0.5 * accel * t_stop * t_stop;
        (0.0, dx.max(0.0))
    } else {
        let dx = v * dt + 0.5 * accel * dt * dt;
        (v_new, dx)
    }
}
```

### Pattern 2: MOBIL Lane-Change Decision
**What:** Evaluates whether a lane change is beneficial and safe.
**When to use:** Periodically (every 0.5-1.0s) for car agents on multi-lane roads.
**Example:**
```rust
// Source: mtreiber.de/publications/MOBIL_TRB.pdf + arch doc 02-agent-models.md
pub struct MobilParams {
    pub politeness: f64,   // p = 0.3 for HCMC
    pub threshold: f64,    // delta_a_thr = 0.2 m/s^2
    pub safe_decel: f64,   // b_safe = -4.0 m/s^2
    pub right_bias: f64,   // a_bias = 0.1 m/s^2
}

pub struct LaneChangeContext {
    pub accel_current: f64,      // my IDM accel with current leader
    pub accel_target: f64,       // my IDM accel with target lane leader
    pub accel_new_follower: f64, // new follower's IDM accel if I change
    pub accel_old_follower: f64, // old follower's current IDM accel
    pub is_right: bool,          // true if changing to right lane
}

pub fn mobil_decision(params: &MobilParams, ctx: &LaneChangeContext) -> bool {
    // Safety criterion: new follower must not brake harder than safe_decel
    if ctx.accel_new_follower < params.safe_decel {
        return false;
    }

    // Incentive criterion
    let own_advantage = ctx.accel_target - ctx.accel_current;
    let follower_disadvantage = ctx.accel_old_follower - ctx.accel_new_follower;
    let bias = if ctx.is_right { params.right_bias } else { -params.right_bias };

    let incentive = own_advantage
        - params.politeness * follower_disadvantage
        + bias;

    incentive > params.threshold
}
```

### Pattern 3: OSM Import Pipeline
**What:** Stream OSM PBF, extract highway ways, build petgraph DiGraph.
**When to use:** Once at simulation startup.
**Example:**
```rust
// Source: docs.rs/osmpbf/0.3.8 + arch doc 04-data-pipeline-hcmc.md
use osmpbf::{ElementReader, Element};
use petgraph::graph::DiGraph;
use std::collections::HashMap;

pub struct RoadEdge {
    pub length_m: f64,
    pub speed_limit_mps: f64,
    pub lane_count: u8,
    pub oneway: bool,
    pub road_class: RoadClass,
    pub geometry: Vec<[f64; 2]>,  // polyline in local metres
}

pub struct RoadNode {
    pub pos: [f64; 2],  // local metres
}

pub fn import_osm(pbf_path: &str, center_lat: f64, center_lon: f64)
    -> Result<DiGraph<RoadNode, RoadEdge>, ImportError>
{
    let reader = ElementReader::from_path(pbf_path)?;

    // Pass 1: collect node lat/lon
    let mut node_coords: HashMap<i64, (f64, f64)> = HashMap::new();
    // Pass 2: collect highway ways with tags
    // Pass 3: build graph edges

    // Project to local metres using equirectangular approximation:
    // x = (lon - center_lon) * cos(center_lat) * 111_320.0
    // y = (lat - center_lat) * 110_540.0
    // (metres per degree at HCMC latitude ~10.77)

    todo!()
}
```

### Pattern 4: egui Integration with Existing wgpu Renderer
**What:** Render egui sidebar after the simulation render pass on the same surface.
**When to use:** Every frame, after agent rendering.
**Example:**
```rust
// Source: docs.rs/egui-wgpu/0.33.3 + hasenbanck/egui_example
// Integration pattern:
// 1. Create egui_winit::State + egui_wgpu::Renderer in GpuState::new()
// 2. In window_event: pass events to egui_winit::State
// 3. In update/render:
//    a. Run egui context -> build UI
//    b. Render simulation (existing render pass)
//    c. Render egui output (second render pass on same surface)

// In GpuState struct:
//   egui_state: egui_winit::State,
//   egui_renderer: egui_wgpu::Renderer,

// In render():
fn render_egui(&mut self) {
    let raw_input = self.egui_state.take_egui_input(&self.window);
    let ctx = self.egui_state.egui_ctx().clone();
    let full_output = ctx.run(raw_input, |ctx| {
        egui::SidePanel::left("controls").exact_width(240.0).show(ctx, |ui| {
            ui.heading("VELOS");
            if ui.button("Start").clicked() { /* set sim running */ }
            if ui.button("Pause").clicked() { /* pause sim */ }
            if ui.button("Reset").clicked() { /* reset sim */ }
            ui.add(egui::Slider::new(&mut self.speed_mult, 0.1..=4.0).text("Speed"));
            ui.separator();
            ui.label(format!("Frame: {:.1}ms", self.metrics.frame_time_ms));
            ui.label(format!("Agents: {}", self.metrics.agent_count));
            ui.label(format!("Throughput: {:.0}/s", self.metrics.agents_per_sec));
        });
    });

    // Handle egui output (textures, shapes) via egui_wgpu::Renderer
    // Render as second render pass after simulation render pass
}
```

### Anti-Patterns to Avoid
- **Running IDM on GPU in Phase 2**: IDM needs leader lookup which requires sorted-by-lane arrays. Keep it CPU-side in f64 for correctness; GPU optimization is Phase 3+.
- **Storing full OSM data**: Only keep the directed graph + edge metadata. Don't store raw OSM nodes/ways/relations in memory after import.
- **Per-agent R-tree insertion every frame**: Rebuild the R-tree via `bulk_load` once per frame, not incremental insert/remove. Bulk load is O(n log n) vs O(n * log n) for n individual inserts.
- **Blocking Overpass API at startup**: Download the OSM PBF extract once and commit to `data/hcmc/`. Don't hit the network at simulation startup.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Graph shortest path | Custom A* on adjacency list | `petgraph::algo::astar` | Handles edge cases (unreachable nodes, zero-weight edges), well-tested |
| Spatial neighbor query | Brute-force O(n^2) distance check | `rstar::RTree::nearest_neighbor_iter` | O(log n) query; bulk_load for efficient construction |
| OSM PBF parsing | Custom protobuf decoder | `osmpbf::ElementReader` | PBF format is complex (DenseNodes, delta-encoded, zlib blocks) |
| GUI immediate mode | Custom wgpu text rendering + input handling | `egui` + `egui-wgpu` | Text layout, input handling, widget state are each months of work |
| Equirectangular projection | Manual trig | `geo::algorithm::map_coords` or inline formula | Formula is simple but easy to get lat/lon order wrong |

**Key insight:** The only custom code in this phase should be the simulation models (IDM, MOBIL, signal controller, gridlock detection) and the glue between subsystems. Everything else has a mature crate.

## Common Pitfalls

### Pitfall 1: wgpu Version Mismatch with egui-wgpu
**What goes wrong:** egui-wgpu 0.33.3 depends on wgpu ^27.0.1. Project currently uses wgpu 28. Cargo will refuse to compile.
**Why it happens:** wgpu releases breaking versions every ~3 months. egui tracks 1-2 versions behind.
**How to avoid:** Downgrade workspace wgpu to "27" before starting egui integration. Or use `egui-wgpu = { git = "https://github.com/emilk/egui", branch = "main" }` if main has wgpu 28 support (check at implementation time).
**Warning signs:** `cargo check` fails with conflicting wgpu version requirements.
**Confidence:** HIGH -- verified egui-wgpu 0.33.3 Cargo.toml on GitHub requires wgpu ^27.0.1; project Cargo.toml specifies wgpu = "28".

### Pitfall 2: OSM Node Ordering in Ways
**What goes wrong:** OSM ways store node references as ordered arrays. The direction of a way determines forward/backward for oneway streets. Reversing the node order flips the road direction.
**Why it happens:** For two-way streets you must create two directed edges (forward + reverse). For oneway streets, only one edge.
**How to avoid:** Always check `oneway=yes`, `oneway=-1` (reverse), and `oneway=no` tags. Default to bidirectional.
**Warning signs:** Agents can't reach destinations, A* returns no path on what should be connected roads.

### Pitfall 3: IDM Negative Velocity Without Stopping Guard
**What goes wrong:** When dt * deceleration > current speed, naive Euler integration produces negative velocity. Agent drives backwards.
**Why it happens:** IDM deceleration can be very large when gap is small (s*/s term dominates).
**How to avoid:** Implement ballistic stopping guard: if v + a*dt < 0, compute time_to_stop = -v/a, integrate only to that time, set v=0.
**Warning signs:** Agent positions decrease along their travel direction; assertion `speed >= 0` fails.

### Pitfall 4: R-tree Coordinate System Mismatch
**What goes wrong:** R-tree queries return wrong neighbors because positions are in different coordinate systems (lat/lon vs metres).
**Why it happens:** OSM data arrives in WGS84 (lat/lon degrees). Simulation needs metres for distance-based queries.
**How to avoid:** Project ALL coordinates to local metres at OSM import time. Never store lat/lon in the simulation -- the RoadGraph and R-tree both use metres only.
**Warning signs:** Neighbor queries return agents kilometers away; IDM gaps are in degrees instead of metres.

### Pitfall 5: egui Stealing Input Events
**What goes wrong:** Camera pan/zoom stops working when egui sidebar is active because egui consumes all mouse events.
**Why it happens:** egui-winit processes events before the application does.
**How to avoid:** Check `egui_ctx.wants_pointer_input()` and `egui_ctx.wants_keyboard_input()` before forwarding events to the camera. Only update camera when egui does NOT want the event.
**Warning signs:** Can't pan/zoom the simulation view while egui sidebar is visible.

### Pitfall 6: Gridlock False Positives
**What goes wrong:** Agents at red lights are flagged as gridlocked (speed=0 for >300s at a long signal cycle).
**Why it happens:** Signal waiting looks identical to gridlock from a speed-only perspective.
**How to avoid:** Only count "waiting" time for agents NOT at a red signal. Track a `waiting_at_signal: bool` flag per agent.
**Warning signs:** Agents at traffic lights get teleported/rerouted unnecessarily.

## Code Examples

### OSM Tag Parsing for Road Properties
```rust
// Source: arch doc 04-data-pipeline-hcmc.md
fn parse_road_tags(tags: &[(String, String)]) -> Option<RoadProperties> {
    let highway = tags.iter().find(|(k, _)| k == "highway")?.1.as_str();

    let road_class = match highway {
        "primary" => RoadClass::Primary,
        "secondary" => RoadClass::Secondary,
        "tertiary" => RoadClass::Tertiary,
        "residential" => RoadClass::Residential,
        _ => return None, // skip service, footway, cycleway, etc.
    };

    let lanes = tags.iter()
        .find(|(k, _)| k == "lanes")
        .and_then(|(_, v)| v.parse::<u8>().ok())
        .unwrap_or_else(|| infer_lanes(road_class));

    let speed_limit_kmh = tags.iter()
        .find(|(k, _)| k == "maxspeed")
        .and_then(|(_, v)| v.trim_end_matches(" km/h").parse::<f64>().ok())
        .unwrap_or_else(|| default_speed(road_class));

    let oneway = tags.iter()
        .find(|(k, _)| k == "oneway")
        .map(|(_, v)| v == "yes" || v == "1")
        .unwrap_or(false);

    Some(RoadProperties { road_class, lanes, speed_limit_mps: speed_limit_kmh / 3.6, oneway })
}

fn infer_lanes(road_class: RoadClass) -> u8 {
    match road_class {
        RoadClass::Primary => 2,
        RoadClass::Secondary => 2,
        RoadClass::Tertiary => 1,
        RoadClass::Residential => 1,
    }
}
```

### Equirectangular Projection
```rust
// Source: standard cartographic formula for small areas
const DEG_TO_M_LAT: f64 = 110_540.0;  // metres per degree latitude

pub struct EquirectangularProjection {
    pub center_lat: f64,
    pub center_lon: f64,
    pub cos_center_lat: f64,
}

impl EquirectangularProjection {
    pub fn new(center_lat: f64, center_lon: f64) -> Self {
        Self {
            center_lat,
            center_lon,
            cos_center_lat: center_lat.to_radians().cos(),
        }
    }

    /// Convert WGS84 (lat, lon) to local metres (x_east, y_north).
    pub fn project(&self, lat: f64, lon: f64) -> (f64, f64) {
        let x = (lon - self.center_lon) * self.cos_center_lat * 111_320.0;
        let y = (lat - self.center_lat) * DEG_TO_M_LAT;
        (x, y)
    }
}

// District 1 centroid: approximately 10.7756, 106.7019
```

### Time-of-Day Demand Profile
```rust
// Source: arch doc 04-data-pipeline-hcmc.md
pub struct TodProfile {
    /// (hour, factor) pairs, linearly interpolated between points.
    points: Vec<(f64, f64)>,
}

impl TodProfile {
    pub fn hcmc_weekday() -> Self {
        Self {
            points: vec![
                (0.0, 0.05), (5.0, 0.10), (6.0, 0.40), (7.0, 1.00),
                (8.0, 1.00), (9.0, 0.50), (12.0, 0.70), (13.0, 0.50),
                (17.0, 1.00), (18.0, 1.00), (19.0, 0.50), (22.0, 0.10),
            ],
        }
    }

    pub fn factor_at(&self, hour: f64) -> f64 {
        // Linear interpolation between points
        for window in self.points.windows(2) {
            let (t0, f0) = window[0];
            let (t1, f1) = window[1];
            if hour >= t0 && hour < t1 {
                let t = (hour - t0) / (t1 - t0);
                return f0 + t * (f1 - f0);
            }
        }
        self.points.last().map(|&(_, f)| f).unwrap_or(0.05)
    }
}
```

### Gridlock Detection (Simple BFS)
```rust
// Recommended: simple visited-set approach over Tarjan SCC.
// Rationale: for ~1K agents on District 1, a simple waiting-graph BFS is
// O(V+E) where V = stopped agents (typically <100) and E = "blocked by" relations.
// Tarjan SCC is more complex and unnecessary at this scale.

pub struct GridlockDetector {
    pub timeout_secs: f64,  // 300s default
}

impl GridlockDetector {
    /// Build a "waiting graph": edge from A -> B means A is stopped behind B.
    /// A cycle in this graph = gridlock.
    /// Returns sets of agent IDs involved in each gridlock cycle.
    pub fn detect_cycles(&self, waiting_graph: &HashMap<AgentId, AgentId>) -> Vec<Vec<AgentId>> {
        let mut visited = HashSet::new();
        let mut cycles = Vec::new();

        for &start in waiting_graph.keys() {
            if visited.contains(&start) { continue; }
            let mut path = Vec::new();
            let mut current = start;
            let mut path_set = HashSet::new();

            loop {
                if path_set.contains(&current) {
                    // Found cycle -- extract it
                    let cycle_start = path.iter().position(|&id| id == current).unwrap();
                    cycles.push(path[cycle_start..].to_vec());
                    break;
                }
                if visited.contains(&current) { break; }
                path.push(current);
                path_set.insert(current);
                match waiting_graph.get(&current) {
                    Some(&next) => current = next,
                    None => break,
                }
            }
            visited.extend(path);
        }
        cycles
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| osmpbfreader (HashMap all data) | osmpbf (streaming, parallel) | 2020+ | 3-5x faster import, lower memory |
| Manual egui-wgpu setup | Official egui-wgpu + egui-winit crates | egui 0.28+ | Standard integration path, less boilerplate |
| wgpu 27 | wgpu 28 | Late 2025 | Minor API changes; egui-wgpu not yet updated |
| petgraph 0.5.x | petgraph 0.6.x | 2023 | Stable API, same astar() interface |

**Deprecated/outdated:**
- `egui_wgpu_backend` (hasenbanck): superseded by official `egui-wgpu` crate from emilk
- `egui_winit_platform`: superseded by official `egui-winit` crate
- wgpu `Maintain::Wait`: replaced by `PollType::wait_indefinitely()` in wgpu 28 (already handled in Phase 1)

## Open Questions

1. **wgpu 27 vs 28 downgrade impact**
   - What we know: Project uses wgpu 28 features (PollType::wait_indefinitely). egui-wgpu needs wgpu 27.
   - What's unclear: Whether PollType::wait_indefinitely exists in wgpu 27 or is a wgpu 28 addition. If wgpu 28-only, downgrading requires reverting that API call.
   - Recommendation: Check `wgpu 27` API at implementation time. If PollType exists in 27, downgrade is trivial. If not, use `Maintain::Wait` (the wgpu 27 equivalent) and update the Phase 1 code.

2. **District 1 OSM data size**
   - What we know: Architecture doc estimates 12K-15K junctions, 20K-25K edges for the full 5-district POC area. District 1 alone should be much smaller.
   - What's unclear: Exact size after filtering to primary+secondary+tertiary+residential only.
   - Recommendation: Download and test. Expect ~2K-5K junctions, ~3K-8K edges for District 1 core.

3. **OD matrix for District 1 POC**
   - What we know: No real OD data available. Architecture doc suggests gravity model as fallback.
   - What's unclear: How many zones, how many trips per zone pair.
   - Recommendation: Create a simple hardcoded 4-6 zone OD matrix for POC. Zones = major areas in District 1 (Ben Thanh, Nguyen Hue, Bitexco area, Bui Vien, waterfront). 50-100 trips/hour per zone pair. This is sufficient to demonstrate the pipeline.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Workspace Cargo.toml (test targets per crate) |
| Quick run command | `cargo test --workspace --lib` |
| Full suite command | `cargo test --workspace --no-fail-fast` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| VEH-01 | IDM deceleration to stop without negative velocity | unit | `cargo test -p velos-vehicle -- idm` | Wave 0 |
| VEH-02 | MOBIL lane-change benefit > politeness threshold | unit | `cargo test -p velos-vehicle -- mobil` | Wave 0 |
| NET-01 | OSM PBF loads into directed graph with correct properties | integration | `cargo test -p velos-net -- osm_import` | Wave 0 |
| NET-02 | R-tree nearest neighbor returns correct agents | unit | `cargo test -p velos-net -- spatial` | Wave 0 |
| NET-03 | Signal cycles green/amber/red with correct timing | unit | `cargo test -p velos-signal -- signal` | Wave 0 |
| NET-04 | Edge coordinates in metres match projection | unit | `cargo test -p velos-net -- projection` | Wave 0 |
| RTE-01 | A* finds shortest path on test graph | unit | `cargo test -p velos-net -- astar` | Wave 0 |
| DEM-01 | OD matrix loads zone-to-zone trips | unit | `cargo test -p velos-demand -- od_matrix` | Wave 0 |
| DEM-02 | ToD profile interpolates correctly at peak/off-peak | unit | `cargo test -p velos-demand -- tod_profile` | Wave 0 |
| DEM-03 | Spawner produces 80/15/5 distribution over N agents | unit | `cargo test -p velos-demand -- spawner` | Wave 0 |
| GRID-01 | Cycle detection finds circular wait, ignores linear wait | unit | `cargo test -p velos-vehicle -- gridlock` | Wave 0 |
| APP-01 | egui controls invoke SimState transitions | manual-only | Visual verification: click buttons, observe sim state changes | N/A |
| APP-02 | egui dashboard shows live metrics | manual-only | Visual verification: metrics update each frame | N/A |

### Sampling Rate
- **Per task commit:** `cargo test --workspace --lib`
- **Per wave merge:** `cargo clippy --all-targets -- -D warnings && cargo test --workspace --no-fail-fast`
- **Phase gate:** Full quality gate (clippy + test + naga shader validation)

### Wave 0 Gaps
- [ ] `crates/velos-vehicle/tests/idm_tests.rs` -- covers VEH-01
- [ ] `crates/velos-vehicle/tests/mobil_tests.rs` -- covers VEH-02
- [ ] `crates/velos-net/tests/import_tests.rs` -- covers NET-01, NET-04
- [ ] `crates/velos-net/tests/spatial_tests.rs` -- covers NET-02
- [ ] `crates/velos-net/tests/routing_tests.rs` -- covers RTE-01
- [ ] `crates/velos-signal/tests/signal_tests.rs` -- covers NET-03
- [ ] `crates/velos-demand/tests/spawner_tests.rs` -- covers DEM-01, DEM-02, DEM-03
- [ ] `crates/velos-vehicle/tests/gridlock_tests.rs` -- covers GRID-01
- [ ] Small test PBF file at `data/hcmc/test-district1.osm.pbf` or equivalent fixture

## Sources

### Primary (HIGH confidence)
- osmpbf 0.3.8 docs: https://docs.rs/osmpbf/latest/osmpbf/ -- API for ElementReader, Element enum, par_map_reduce
- petgraph docs: https://docs.rs/petgraph/latest/petgraph/ -- DiGraph, astar(), connected_components
- rstar docs: https://docs.rs/rstar/latest/rstar/ -- RTree, bulk_load, nearest_neighbor, PointDistance trait
- egui-wgpu 0.33.3 docs: https://docs.rs/egui-wgpu/latest/egui_wgpu/ -- Renderer, ScreenDescriptor, CallbackTrait
- egui main branch Cargo.toml: https://github.com/emilk/egui -- confirmed wgpu ^27.0.1 dependency
- Architecture doc 02-agent-models.md -- IDM params, MOBIL params, vehicle type definitions
- Architecture doc 04-data-pipeline-hcmc.md -- OSM import rules, signal timing defaults, ToD profiles

### Secondary (MEDIUM confidence)
- traffic-simulation.de IDM info: https://traffic-simulation.de/info/info_IDM.html -- IDM formula description, ballistic guard
- traffic-simulation.de MOBIL info: https://traffic-simulation.de/info/info_MOBIL.html -- MOBIL incentive/safety criteria
- MOBIL paper: https://www.mtreiber.de/publications/MOBIL_TRB.pdf -- Original MOBIL formulation
- hasenbanck/egui_example: https://github.com/hasenbanck/egui_example -- egui+wgpu+winit integration pattern

### Tertiary (LOW confidence)
- District 1 OSM data size estimate (untested, based on architecture doc extrapolation)
- egui wgpu 28 support timeline (no evidence of active development)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all crates verified on docs.rs/crates.io with current versions
- Architecture: HIGH - patterns follow architecture docs + established Phase 1 patterns
- IDM/MOBIL: HIGH - well-documented models with 20+ years of literature
- egui integration: MEDIUM - wgpu version mismatch requires resolution; integration pattern is standard once versions align
- Pitfalls: HIGH - based on verified version constraints and known simulation gotchas

**Research date:** 2026-03-06
**Valid until:** 2026-04-06 (stable domain; main risk is egui-wgpu version catching up to wgpu 28)
