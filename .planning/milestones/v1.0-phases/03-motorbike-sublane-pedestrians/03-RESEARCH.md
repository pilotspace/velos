# Phase 3: Motorbike Sublane & Pedestrians - Research

**Researched:** 2026-03-06
**Domain:** Continuous lateral vehicle positioning, social force pedestrian model, mixed-traffic interaction
**Confidence:** HIGH

## Summary

Phase 3 implements the core VELOS differentiator: motorbikes with continuous lateral positioning that filter between cars, swarm at red lights, and disperse on green. Pedestrians get the Helbing social force model with jaywalking. Both agent types must interact with existing IDM/MOBIL car agents at intersections.

The existing codebase (Phase 2) provides a solid foundation: hecs ECS world, per-edge leader detection, signal controllers, spatial index (rstar), and per-type instanced rendering. Phase 3 adds two new modules to `velos-vehicle` (sublane.rs, social_force.rs), one new ECS component (`LateralOffset`), and modifies `sim.rs` to split motorbike updates from car updates and replace the linear pedestrian walker with social force.

**Primary recommendation:** Implement sublane model and social force as pure CPU functions in `velos-vehicle` with comprehensive unit tests, then wire into `SimWorld::tick()` via new step functions. Use the existing rstar spatial index for cross-type neighbor queries.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- Gap-seeking trigger: motorbikes scan for lateral gaps wider than body width and drift toward them
- Minimum filtering gap: 0.6m (tight, realistic HCMC)
- Maximum lateral drift speed: 1.0 m/s
- Full road width access: motorbikes can drift into oncoming lane shoulder when gaps exist
- Lane-aware boundary for oncoming avoidance: crossing centerline triggers heightened gap-checking
- Integration: forward Euler with max-displacement clamp per step (same as existing IDM)
- Push to front + fill laterally for red-light swarming; swarming zone up to stop line only
- Dispersal on green: burst acceleration + gradual lateral merge back
- Visual cue: slight color shift for swarming motorbikes
- Full Helbing model: repulsion + attraction to destination + obstacle avoidance + anisotropic vision cone
- Jaywalking: red light (0.3 probability) AND mid-block crossing (0.1 probability)
- Vehicle interaction: vehicles react to jaywalking pedestrians
- Pedestrian paths: walk along road edges/shoulders with lateral offset (no sidewalk geometry)
- Right-of-way: signal-based, type-agnostic
- Motorbike intersection: free-form crossing using lateral freedom
- Cross-type collision avoidance: all agents avoid all types
- Conflict resolution: motorbike yields (swerves) on collision course with car
- Aggressive turns: motorbikes making left turns drift into oncoming lanes
- New `LateralOffset` component: separate ECS component, only motorbikes get it
- All agent movement code in `velos-vehicle` (no separate velos-pedestrian crate)

### Claude's Discretion
- Social force model parameter tuning (repulsion strength, interaction radius, vision cone angle)
- Exact gap-seeking algorithm implementation (how gaps are scored and selected)
- Intersection box geometry detection
- Mid-block jaywalking opportunity window definition
- Specific color values for swarming visual cue
- rstar spatial query radius and performance tuning for cross-type avoidance

### Deferred Ideas (OUT OF SCOPE)
- Step-by-step single-frame advance button in egui
- Adaptive GPU workgroups for pedestrians based on density (ADV-10)
- Bicycle agents with sublane behavior (ADV-09)
- Scenario selector dropdown for different traffic densities

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| VEH-03 | Motorbike sublane model uses continuous lateral position enabling filtering between cars, red-light clustering, and swarm behavior | Sublane model architecture, gap-seeking algorithm, red-light swarming logic, LateralOffset ECS component, dt-consistent Euler integration |
| VEH-04 | Pedestrian basic social force model (repulsion from other agents + attraction to destination), including jaywalking probability (0.3 for HCMC) | Helbing social force equations, parameter values, anisotropic vision cone, jaywalking probability model, pedestrian-vehicle interaction |

</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| hecs | workspace | ECS storage for LateralOffset component | Already used, composable component model |
| rstar | workspace | Spatial index for cross-type neighbor queries | Already used in velos-net, O(log n) range queries |
| rand | workspace | Jaywalking probability rolls, gap selection randomness | Already used in spawner |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| log | workspace | Debug logging for sublane decisions | Tracing gap-seeking and social force computation |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| rstar for neighbor queries | Custom grid hash | rstar already exists, grid hash only worth it at >100K pedestrians (deferred ADV-10) |
| f64 CPU math | GPU compute shader | CPU is correct for POC scale; GPU sublane is a v2 optimization |

No new dependencies required. All libraries are already in the workspace.

## Architecture Patterns

### Recommended Project Structure
```
crates/velos-vehicle/src/
    lib.rs           # add pub mod sublane; pub mod social_force;
    sublane.rs       # NEW: motorbike lateral model (~250 lines)
    social_force.rs  # NEW: pedestrian Helbing model (~200 lines)
    idm.rs           # existing (unchanged)
    mobil.rs         # existing (unchanged)
    types.rs         # existing (unchanged)
    gridlock.rs      # existing (unchanged)

crates/velos-core/src/
    components.rs    # ADD: LateralOffset component

crates/velos-gpu/src/
    sim.rs           # MODIFY: add step_motorbikes_sublane(), replace step_pedestrians()
    renderer.rs      # MINOR: read LateralOffset for motorbike world position, swarming color
```

### Pattern 1: LateralOffset ECS Component
**What:** A new ECS component attached only to motorbike entities, storing continuous lateral position.
**When to use:** Every motorbike agent gets this at spawn time.
**Example:**
```rust
// In velos-core/src/components.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LateralOffset {
    /// Current lateral offset from road edge centerline (metres).
    /// 0.0 = right edge, positive = toward left/center.
    pub lateral_offset: f64,
    /// Desired lateral position the motorbike is drifting toward.
    pub desired_lateral: f64,
}
```
The ECS query for motorbikes becomes:
```rust
world.query::<(&RoadPosition, &Kinematics, &IdmParams, &LateralOffset, &VehicleType)>()
```
Cars simply don't have `LateralOffset`, so they're naturally excluded from sublane logic.

### Pattern 2: Pure Function + Wire Pattern
**What:** Sublane and social force models are pure functions in `velos-vehicle`, wired into `SimWorld::tick()` as new step functions.
**When to use:** All new physics models follow this pattern (consistent with IDM/MOBIL).
**Example:**
```rust
// In velos-vehicle/src/sublane.rs — pure function, no ECS dependency
pub fn lateral_desire(
    own_lateral: f64,
    own_speed: f64,
    road_width: f64,
    neighbors: &[NeighborInfo],
    at_red_light: bool,
) -> f64 {
    // Returns desired lateral offset
}

// In sim.rs — wiring
fn step_motorbikes_sublane(&mut self, dt: f64) {
    // Query ECS for motorbikes
    // Call sublane::lateral_desire() for each
    // Apply lateral drift with max speed clamp
    // Update LateralOffset component
}
```

### Pattern 3: Spatial Index for Cross-Type Avoidance
**What:** Rebuild rstar spatial index each frame with ALL agent types, query for mixed-type neighbors.
**When to use:** For cross-type collision avoidance (motorbikes swerving around pedestrians, cars braking for jaywalkers).
**Example:**
```rust
// Build spatial index with type tags
pub struct TypedAgentPoint {
    pub id: u32,
    pub pos: [f64; 2],
    pub vehicle_type: VehicleType,
    pub speed: f64,
}

// Query: "all agents within 10m regardless of type"
let neighbors = spatial_index.nearest_within_radius(my_pos, 10.0);
let pedestrians_ahead: Vec<_> = neighbors.iter()
    .filter(|n| n.vehicle_type == VehicleType::Pedestrian)
    .collect();
```

### Anti-Patterns to Avoid
- **Separate update loops per type without interaction:** All agents must see all other agents for collision avoidance. Don't run motorbike step in isolation from pedestrian positions.
- **Storing lateral offset in RoadPosition.lane:** Lane is discrete (u8), lateral offset is continuous (f64). They serve different purposes. Cars use lane, motorbikes use LateralOffset.
- **Rebuilding spatial index per step function:** Build it ONCE per frame in `tick()`, pass to all step functions.
- **Mutating ECS during iteration:** Collect updates into a Vec, then apply. Existing pattern in `step_vehicles()`.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Spatial neighbor queries | Custom grid/hash | rstar `nearest_within_radius` | Already exists, handles non-uniform density, O(log n) |
| Random number generation | Custom PRNG | `rand::rngs::StdRng` | Already seeded, deterministic |
| Edge geometry interpolation | Custom bezier | Existing `update_agent_state()` linear interp | Already handles edge fraction -> world pos |
| Angle wrapping | Manual modulo | `f64::atan2()` | Already used everywhere, handles quadrants |

**Key insight:** Phase 3 adds physics models, not infrastructure. The infrastructure (ECS, spatial index, rendering pipeline, edge-based movement) is all in place from Phases 1-2.

## Common Pitfalls

### Pitfall 1: dt-Inconsistent Lateral Movement
**What goes wrong:** Motorbike filtering behavior changes when timestep changes (e.g., faster at dt=0.05s than dt=0.2s).
**Why it happens:** Lateral drift speed not properly clamped per-step, or gap detection uses absolute distance thresholds that interact with step size.
**How to avoid:** Clamp lateral displacement per step: `dx_lateral = desired_drift_speed * dt`, capped at `max_lateral_speed * dt`. The max displacement per step scales linearly with dt, ensuring same trajectory regardless of timestep.
**Warning signs:** Success criterion #1 explicitly tests this — run filtering scenario at dt=0.05s, 0.1s, 0.2s and compare lateral trajectories.

### Pitfall 2: Social Force Explosion
**What goes wrong:** Pedestrians accelerate to unrealistic speeds or get launched when two agents overlap.
**Why it happens:** Exponential repulsion force `A * exp((r - d) / B)` grows extremely fast when `d < r` (agents overlapping). With small B=0.08m, force doubles every 0.055m of overlap.
**How to avoid:** (1) Clamp maximum force magnitude per interaction. (2) Clamp maximum pedestrian speed to ~2.0 m/s. (3) Use `f64::min(force_magnitude, MAX_FORCE)` with MAX_FORCE = 50.0 N or similar.
**Warning signs:** Pedestrians with speed > 3 m/s, or pedestrians teleporting across the map.

### Pitfall 3: Motorbike-Car Collision at Lateral Boundaries
**What goes wrong:** Motorbike with lateral offset occupies same world-space as a car in adjacent lane.
**Why it happens:** Motorbike lateral position doesn't account for car body width when computing available gap.
**How to avoid:** Gap computation must subtract both the motorbike half-width (0.25m) and the neighbor half-width (car: 0.9m, motorbike: 0.25m) from the raw lateral distance.
**Warning signs:** Visual overlap of motorbike triangles and car rectangles in the renderer.

### Pitfall 4: Pedestrian Jaywalking Without Vehicle Awareness
**What goes wrong:** Pedestrian walks into road and vehicles don't react, or pedestrian starts crossing when a car is 2m away.
**Why it happens:** Jaywalking decision doesn't check gap acceptance (time-to-collision with approaching vehicles).
**How to avoid:** Before jaywalking, compute time-to-collision with nearest approaching vehicle. Only jaywalk if TTC > gap_acceptance_time (e.g., 2.0s). Vehicles should also run IDM-like braking when a pedestrian is detected ahead on the road.
**Warning signs:** Pedestrians getting "run over" (overlapping with vehicles at speed).

### Pitfall 5: Swarming Motorbikes Blocking Intersection Entry
**What goes wrong:** Motorbikes filling the road width at a red light cannot clear fast enough on green, creating artificial gridlock.
**Why it happens:** All motorbikes try to merge back to lane center simultaneously, creating lateral conflicts.
**How to avoid:** Dispersal is gradual — motorbikes prioritize forward acceleration on green, lateral merge happens over several seconds. Dispersal rate should be lower than swarming rate.
**Warning signs:** Intersection throughput drops significantly after adding swarming behavior.

## Code Examples

### Motorbike Sublane Gap-Seeking

```rust
// Source: Architecture doc 02-agent-models.md + CONTEXT.md decisions

/// Information about a neighboring agent relative to the ego motorbike.
pub struct NeighborInfo {
    /// Longitudinal gap (positive = ahead).
    pub longitudinal_gap: f64,
    /// Lateral offset of neighbor from road edge.
    pub lateral_offset: f64,
    /// Half-width of the neighbor's body.
    pub half_width: f64,
    /// Speed of neighbor.
    pub speed: f64,
}

/// Sublane model parameters for motorbikes.
pub struct SublaneParams {
    /// Minimum lateral gap to attempt filtering (m). HCMC: 0.6m.
    pub min_filter_gap: f64,
    /// Maximum lateral drift speed (m/s). HCMC: 1.0 m/s.
    pub max_lateral_speed: f64,
    /// Motorbike body half-width (m). Typical: 0.25m.
    pub half_width: f64,
    /// Swarm attraction strength (m/s) when at red light.
    pub swarm_lateral_speed: f64,
}

impl Default for SublaneParams {
    fn default() -> Self {
        Self {
            min_filter_gap: 0.6,
            max_lateral_speed: 1.0,
            half_width: 0.25,
            swarm_lateral_speed: 0.8,
        }
    }
}

/// Compute desired lateral offset for a motorbike.
///
/// Returns the target lateral position the motorbike should drift toward.
/// The caller applies drift speed and dt to actually move.
pub fn compute_desired_lateral(
    own_lateral: f64,
    own_speed: f64,
    road_width: f64,
    neighbors: &[NeighborInfo],
    at_red_light: bool,
    params: &SublaneParams,
) -> f64 {
    if at_red_light {
        // Swarming: find largest lateral gap near stop line
        return find_largest_gap(own_lateral, road_width, neighbors, params);
    }

    // Normal driving: scan for filtering gaps
    let mut best_lateral = own_lateral; // default: stay put
    let mut best_score = 0.0_f64;

    // Check left and right gaps
    for direction in [-1.0_f64, 1.0] {
        let probe = own_lateral + direction * 0.5; // probe 0.5m left/right
        if probe < params.half_width || probe > road_width - params.half_width {
            continue; // would go off road
        }
        let gap = lateral_gap_at(probe, neighbors, params);
        if gap >= params.min_filter_gap {
            // Score: prefer gaps that let us go faster
            let score = gap * 0.5; // simple scoring
            if score > best_score {
                best_score = score;
                best_lateral = probe;
            }
        }
    }

    best_lateral
}

/// Apply lateral drift toward desired position, clamped by max speed and dt.
pub fn apply_lateral_drift(
    current: f64,
    desired: f64,
    max_speed: f64,
    dt: f64,
) -> f64 {
    let diff = desired - current;
    let max_displacement = max_speed * dt;
    let displacement = diff.clamp(-max_displacement, max_displacement);
    current + displacement
}
```

### Helbing Social Force Model

```rust
// Source: Helbing & Molnar (1995), pedestriandynamics.org

/// Social force model parameters.
pub struct SocialForceParams {
    /// Repulsion strength (N). Standard: 2000.0.
    pub a: f64,
    /// Repulsion range (m). Standard: 0.08.
    pub b: f64,
    /// Agent body radius (m). Standard: 0.3.
    pub radius: f64,
    /// Relaxation time (s). Standard: 0.5.
    pub tau: f64,
    /// Desired walking speed (m/s). Standard: 1.2 for HCMC.
    pub desired_speed: f64,
    /// Anisotropy parameter (0..1). 0.5 = reduced force from behind.
    pub lambda: f64,
    /// Maximum force magnitude per interaction (N). Prevents explosions.
    pub max_force: f64,
    /// Maximum pedestrian speed (m/s). Clamp output.
    pub max_speed: f64,
}

impl Default for SocialForceParams {
    fn default() -> Self {
        Self {
            a: 2000.0,
            b: 0.08,
            radius: 0.3,
            tau: 0.5,
            desired_speed: 1.2,
            lambda: 0.5,     // anisotropic: 50% reduction for forces from behind
            max_force: 50.0,  // prevents explosion
            max_speed: 2.0,
        }
    }
}

/// Compute social force acceleration for one pedestrian.
///
/// Returns (ax, ay) acceleration in m/s^2.
pub fn social_force_acceleration(
    pos: [f64; 2],
    vel: [f64; 2],
    destination: [f64; 2],
    neighbors: &[([f64; 2], [f64; 2], f64)], // (pos, vel, radius)
    params: &SocialForceParams,
) -> [f64; 2] {
    // 1. Driving force: accelerate toward destination
    let dx = destination[0] - pos[0];
    let dy = destination[1] - pos[1];
    let dist = (dx * dx + dy * dy).sqrt().max(0.01);
    let desired_vx = params.desired_speed * dx / dist;
    let desired_vy = params.desired_speed * dy / dist;
    let fx_drive = (desired_vx - vel[0]) / params.tau;
    let fy_drive = (desired_vy - vel[1]) / params.tau;

    // 2. Repulsive forces from other pedestrians
    let mut fx_rep = 0.0;
    let mut fy_rep = 0.0;
    for &(other_pos, _other_vel, other_radius) in neighbors {
        let nx = pos[0] - other_pos[0];
        let ny = pos[1] - other_pos[1];
        let d = (nx * nx + ny * ny).sqrt().max(0.01);
        let r_sum = params.radius + other_radius;
        let force_mag = params.a * ((r_sum - d) / params.b).exp();
        let force_mag = force_mag.min(params.max_force); // clamp

        // Anisotropic weighting
        let speed = (vel[0] * vel[0] + vel[1] * vel[1]).sqrt().max(0.01);
        let cos_phi = -(vel[0] * nx + vel[1] * ny) / (speed * d);
        let w = params.lambda + (1.0 - params.lambda) * (1.0 + cos_phi) / 2.0;

        fx_rep += w * force_mag * nx / d;
        fy_rep += w * force_mag * ny / d;
    }

    [fx_drive + fx_rep, fy_drive + fy_rep]
}
```

### Swarming Color Shift in Renderer

```rust
// In build_instances() — detect swarming state for color shift
VehicleType::Motorbike => {
    let is_swarming = /* check if at_red_signal and near stop line */;
    if is_swarming {
        [0.4, 1.0, 0.5, 1.0] // brighter green for swarming
    } else {
        [0.2, 0.8, 0.4, 1.0] // normal motorbike green
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Discrete lane assignment for all vehicles | Continuous lateral position for motorbikes (SUMO sublane model, 2015+) | SUMO 0.25.0 (2015) | Enables motorbike filtering, fundamental for SE Asian traffic |
| Simple repulsion for pedestrians | Helbing social force with anisotropy | Helbing 1995, refined 2000 | Industry standard, calibrated parameters available |
| Separate simulation for each agent type | Mixed-traffic interaction with cross-type avoidance | Common in modern microsim | Required for HCMC where motorbikes/cars/pedestrians share road space |

**Deprecated/outdated:**
- Wiedemann 99 for motorbikes: not applicable (lane-based model, cannot represent sublane filtering)
- Discrete lane model for motorbikes: fundamentally wrong for HCMC traffic (motorbikes don't follow lanes)

## Open Questions

1. **Intersection box geometry detection**
   - What we know: Intersections are nodes in the road graph with multiple incoming/outgoing edges
   - What's unclear: How to define the "intersection box" polygon for free-form motorbike crossing
   - Recommendation: Use a simple circular zone around intersection nodes (radius = max incoming edge lane_count * 3.5m). No complex polygon needed for POC.

2. **Mid-block jaywalking opportunity window**
   - What we know: Probability is 0.1 per opportunity
   - What's unclear: How often to evaluate jaywalking opportunities (every frame? every N seconds?)
   - Recommendation: Evaluate once per second of simulation time (not per frame) to avoid dt-dependency. Store last evaluation time in pedestrian state.

3. **Social force interaction radius**
   - What we know: Standard Helbing model uses B=0.08m (very short range for contact force)
   - What's unclear: What radius to use for the rstar spatial query to find relevant neighbors
   - Recommendation: Query radius of 5.0m (social force is negligible beyond ~3m due to exponential decay, but 5m gives safety margin). Limit to nearest 10 neighbors for performance.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) |
| Config file | Cargo.toml workspace [workspace] members |
| Quick run command | `cargo test -p velos-vehicle --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| VEH-03a | Motorbike lateral gap detection finds gaps >= 0.6m | unit | `cargo test -p velos-vehicle sublane -- --exact` | Wave 0 |
| VEH-03b | Lateral drift clamped by max_lateral_speed * dt | unit | `cargo test -p velos-vehicle sublane::drift -- --exact` | Wave 0 |
| VEH-03c | dt-consistency: same lateral trajectory at dt=0.05,0.1,0.2 | unit | `cargo test -p velos-vehicle sublane::dt_consistency` | Wave 0 |
| VEH-03d | Red-light swarming: motorbikes fill road width at stop line | integration | `cargo test -p velos-gpu swarming` | Wave 0 |
| VEH-03e | Green dispersal: motorbikes accelerate and merge back | integration | `cargo test -p velos-gpu dispersal` | Wave 0 |
| VEH-04a | Social force repulsion pushes pedestrians apart | unit | `cargo test -p velos-vehicle social_force::repulsion` | Wave 0 |
| VEH-04b | Driving force attracts pedestrian toward destination | unit | `cargo test -p velos-vehicle social_force::driving` | Wave 0 |
| VEH-04c | Anisotropic weighting reduces force from behind | unit | `cargo test -p velos-vehicle social_force::anisotropy` | Wave 0 |
| VEH-04d | Jaywalking at red light with probability 0.3 | unit | `cargo test -p velos-vehicle social_force::jaywalking` | Wave 0 |
| VEH-04e | Force explosion prevention: speed clamped at max | unit | `cargo test -p velos-vehicle social_force::clamp` | Wave 0 |
| VEH-03+04 | Mixed traffic at intersection: all types interact | integration | `cargo test -p velos-gpu mixed_intersection` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-vehicle --lib`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-vehicle/tests/sublane_tests.rs` -- covers VEH-03a,b,c
- [ ] `crates/velos-vehicle/tests/social_force_tests.rs` -- covers VEH-04a,b,c,d,e
- [ ] `crates/velos-vehicle/src/sublane.rs` -- new module
- [ ] `crates/velos-vehicle/src/social_force.rs` -- new module

## Sources

### Primary (HIGH confidence)
- Existing codebase: `velos-vehicle/src/idm.rs`, `sim.rs`, `components.rs` -- established patterns for physics models
- Architecture doc: `docs/architect/02-agent-models.md` -- sublane model specification, IDM params, pedestrian params
- [Helbing Social Force Model](https://pedestriandynamics.org/models/social_force_model/) -- equations, standard parameters (A=2000N, B=0.08m, tau=0.5s)
- [SUMO Sublane Model](https://sumo.dlr.de/docs/Simulation/SublaneModel.html) -- lateral resolution, gap acceptance, maxSpeedLat

### Secondary (MEDIUM confidence)
- [Helbing & Molnar (1995) original paper](https://arxiv.org/abs/cond-mat/9805244) -- social force model foundations
- [Frontiers: non-lane-based road user model](https://www.frontiersin.org/journals/future-transportation/articles/10.3389/ffutr.2023.1183270/full) -- continuous lateral positioning for cyclists/motorcycles
- CONTEXT.md locked decisions -- validated parameter choices (0.6m gap, 1.0 m/s lateral speed, 0.3 jaywalking)

### Tertiary (LOW confidence)
- Anisotropic lambda parameter value (0.5) -- commonly cited in literature but not verified against HCMC-specific calibration data

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- no new dependencies, extends existing patterns
- Architecture: HIGH -- clear component boundaries, pure function + wire pattern proven in Phase 2
- Pitfalls: HIGH -- dt-consistency, force explosion, and collision issues are well-documented in microsimulation literature
- Social force parameters: MEDIUM -- standard Helbing values are well-established but may need HCMC-specific tuning

**Research date:** 2026-03-06
**Valid until:** 2026-04-06 (stable domain, no moving targets)
