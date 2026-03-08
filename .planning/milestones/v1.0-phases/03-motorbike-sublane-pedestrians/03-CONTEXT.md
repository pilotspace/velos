# Phase 3: Motorbike Sublane & Pedestrians - Context

**Gathered:** 2026-03-06
**Status:** Ready for planning

<domain>
## Phase Boundary

Motorbikes move with continuous lateral positioning (the core VELOS differentiator) and pedestrians move via full Helbing social force model with jaywalking. This replaces Phase 2's placeholder behaviors: motorbikes currently use IDM like cars (no lateral movement), pedestrians walk straight to waypoints (no social force). Mixed traffic (motorbikes, cars, pedestrians) must interact correctly at intersections.

</domain>

<decisions>
## Implementation Decisions

### Motorbike lateral movement
- Gap-seeking trigger: motorbikes scan for lateral gaps wider than body width and drift toward them
- Minimum filtering gap: 0.6m (tight, realistic HCMC — motorbikes squeeze through narrow gaps)
- Maximum lateral drift speed: 1.0 m/s (quick lateral shifts, responsive HCMC-like behavior)
- Full road width access: motorbikes can drift into oncoming lane shoulder when gaps exist
- Lane-aware boundary for oncoming avoidance: motorbikes track which side of the road they're on; crossing centerline triggers heightened gap-checking with oncoming traffic
- Integration: forward Euler with max-displacement clamp per step (same as existing IDM), ensures dt-consistency across 0.05s, 0.1s, 0.2s

### Red-light swarming
- Push to front + fill laterally: motorbikes advance past stopped cars using gaps, then spread across full road width at the stop line
- Swarming zone: up to the intersection stop line only — motorbikes do NOT enter the intersection box on red
- Dispersal on green: burst acceleration (motorbikes have higher IDM accel than cars) + gradual lateral merge back toward normal lane positions
- Visual cue: slight color shift for motorbikes near red lights (e.g., brighter green) to make swarming behavior visible in POC

### Pedestrian social force
- Full Helbing model: repulsion from other agents + attraction to destination + obstacle avoidance + anisotropic vision cone
- Jaywalking: both at red lights (0.3 probability) AND mid-block crossing (0.1 probability per opportunity window)
- Vehicle interaction: vehicles react to jaywalking pedestrians (cars and motorbikes slow down when pedestrian is ahead in road)
- Pedestrian paths: walk along road edges/shoulders — no dedicated sidewalk geometry, uses existing road graph with lateral offset

### Mixed traffic intersections
- Right-of-way: signal-based, type-agnostic — green means go for everyone, no per-type priority rules
- Motorbike intersection behavior: free-form crossing — motorbikes take shortest path through intersection box using lateral freedom (not lane-guided)
- Cross-type collision avoidance: all agents avoid all other agents regardless of type (cars brake for motorbikes, motorbikes swerve around pedestrians)
- Conflict resolution: motorbike yields (swerves) when on collision course with car — motorbikes use lateral agility, cars maintain course
- Aggressive turns: motorbikes making left turns drift into oncoming lanes gradually and find gaps (HCMC-style)

### ECS and crate design
- New `LateralOffset` component: separate ECS component with `lateral_offset` (f64) + `desired_lateral` (f64) — only motorbikes get it, composable ECS pattern
- All agent movement code in `velos-vehicle`: motorbike sublane module + pedestrian social force module within existing crate (no separate velos-pedestrian crate)

### Claude's Discretion
- Social force model parameter tuning (repulsion strength, interaction radius, vision cone angle)
- Exact gap-seeking algorithm implementation (how gaps are scored and selected)
- Intersection box geometry detection (how to determine when an agent is "inside" an intersection)
- Mid-block jaywalking opportunity window definition
- Specific color values for swarming visual cue
- rstar spatial query radius and performance tuning for cross-type avoidance

</decisions>

<specifics>
## Specific Ideas

- The visual payoff of this phase is seeing motorbikes swarm at red lights in front of cars, then burst away on green — this is the "HCMC moment" that differentiates VELOS
- Full road width access for motorbikes is key to authenticity — HCMC motorbikes routinely use oncoming shoulder
- Aggressive left turns cutting across oncoming traffic is a defining HCMC behavior
- dt-consistency (success criterion #1) must be verified: same filtering behavior at dt=0.05s, 0.1s, 0.2s
- Cross-type collision avoidance is critical for the mixed-traffic intersection criterion (#4)

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `VehicleType` enum (`velos-core/src/components.rs`, `velos-vehicle/src/types.rs`): already has Motorbike/Car/Pedestrian variants — Phase 3 adds type-specific update paths
- `IdmParams` with motorbike defaults (`velos-vehicle/src/idm.rs`): v0=40km/h, s0=1m, higher accel — longitudinal model stays, sublane adds lateral
- `MobilParams` + `mobil_decision()` (`velos-vehicle/src/mobil.rs`): lane-change logic for cars — motorbikes bypass this with sublane model
- `step_pedestrians()` (`velos-gpu/src/sim.rs:565-606`): linear walk-to-waypoint — replace with social force model call
- `step_vehicles()` (`velos-gpu/src/sim.rs:335`): runs IDM/MOBIL for cars+motorbikes — split to run sublane for motorbikes
- Renderer per-type rendering (`velos-gpu/src/renderer.rs`): triangles/rectangles/dots already wired — color shift for swarming motorbikes is a small extension
- rstar spatial index (`velos-net`): already used for neighbor queries — extend for cross-type avoidance

### Established Patterns
- f64 CPU / f32 GPU: all physics runs in f64 on CPU, casts to f32 before upload
- One instanced draw call per agent type (REN-04 pattern)
- ECS component projection to GPU SoA via `upload_from_ecs` pattern
- Forward Euler integration with stopping guard (`integrate_with_stopping_guard` in idm.rs)
- `SimWorld::tick()` orchestrates all per-frame steps sequentially

### Integration Points
- `SimWorld::tick()` in `sim.rs`: add `step_motorbikes_sublane()` call, modify `step_vehicles()` to exclude motorbikes
- `step_pedestrians()` in `sim.rs`: replace body with social force model call into `velos-vehicle`
- `build_render_instances()` in `sim.rs`: read `LateralOffset` component to compute world-space position for motorbikes
- `velos-vehicle` crate: add `sublane.rs` module (motorbike lateral model) and `social_force.rs` module (pedestrian Helbing model)

</code_context>

<deferred>
## Deferred Ideas

- Step-by-step single-frame advance button in egui — useful for debugging sublane behavior (noted in Phase 2 deferred)
- Adaptive GPU workgroups for pedestrians based on density — v2 requirement (ADV-10)
- Bicycle agents with sublane behavior — v2 requirement (ADV-09)
- Scenario selector dropdown for different traffic densities — future phase

</deferred>

---

*Phase: 03-motorbike-sublane-pedestrians*
*Context gathered: 2026-03-06*
