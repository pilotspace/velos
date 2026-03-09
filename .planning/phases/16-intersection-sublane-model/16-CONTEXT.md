# Phase 16: Intersection Sublane Model & 2D Map Tiles - Context

**Gathered:** 2026-03-09
**Status:** Ready for planning

<domain>
## Phase Boundary

Vehicles navigate intersections with continuous sublane positioning, enabling realistic motorbike filtering and conflict resolution at junctions. Self-hosted 2D vector map tiles render as background. Vehicles visually show lateral offsets through intersections with lane marking context. This phase does NOT add new vehicle behaviors on regular edges, 3D rendering, or detection ingestion.

Requirements: ISL-01, ISL-02, ISL-03, ISL-04, MAP-01, MAP-02

</domain>

<decisions>
## Implementation Decisions

### Junction Path Geometry
- Quadratic Bezier curves for vehicle turn paths through intersections
- Control points: P0 = entry edge endpoint, P1 = junction centroid, P2 = exit edge startpoint
- Precompute one Bezier per (entry_edge, exit_edge) pair at network load (~15K junctions x ~4 turns avg = ~60K curves stored)
- Lateral offset maps onto curves by shifting perpendicular to the Bezier tangent — motorbike on left side traces tighter inner arc
- Lateral offset is locked when agent enters junction — no dynamic lateral filtering inside junction areas
- Motorbikes still filter freely on approach and departure edges

### Conflict Detection & Priority
- Precompute Bezier crossing points per (turn_A, turn_B) pair at network load — store ConflictPoint(turn_A, turn_B, t_A, t_B)
- Runtime: for agents in same junction, look up ConflictPoint and check if both agents' t-parameters are near crossing t-values
- Priority: agent closer to crossing point (lower |t - t_cross|) has priority — they clear the conflict zone first
- Tie-breaking: use existing size_factor from intersection.rs (Emergency > Truck/Bus > Car > Motorbike)
- Yielding: treat conflict crossing point as virtual leader — apply IDM car-following for smooth deceleration, stop before crossing point, resume when priority agent clears
- Approach-phase check: agents approaching junction entry check for foe agents already inside, using existing intersection_gap_acceptance() TTC logic — prevents entering when a crossing agent is near the conflict point

### Map Tile Rendering
- PMTiles generated from OSM offline (tilemaker), decoded to wgpu geometry at runtime
- Full OSM detail: roads, water, buildings, POIs, labels, amenities, land use
- Decode MVT protobuf per tile, triangulate polygons (earcut), upload to wgpu vertex buffers, render as colored geometry
- Viewport-based dynamic tile loading — only decode and upload tiles visible in current camera view
- LRU tile cache for decoded tiles, decode + triangulate on background thread
- No external tile server, no HTTP — pure Rust, local file I/O
- Dashed Bezier guide lines rendered through junctions showing turn paths (toggleable via egui checkbox)

### Sublane Visualization
- Agent shapes (triangle/arrow) rotate to follow Bezier curve tangent at current t-parameter — natural turning motion
- Tangent computed as B'(t) = 2(1-t)(P1-P0) + 2t(P2-P1) — one vector operation per agent
- Color by vehicle type: motorbike = orange, car = blue, bus = green, truck = red
- Size by vehicle type: motorbike = small, car = medium, bus/truck = large
- Toggleable debug overlay via egui: conflict crossing points (red dots), active conflict pair lines, agent t-parameters on curves

### Claude's Discretion
- Exact tile cache size and eviction policy
- PMTiles zoom level selection strategy per camera zoom
- Bezier evaluation precision and sub-step count for long junction traversals
- Exact colors/opacity for map tile feature layers
- Label rendering approach for POIs/street names (text atlas vs simplified)

</decisions>

<specifics>
## Specific Ideas

- SUMO validation: architecture aligns with SUMO's foe-link matrix and internal-lane model, but VELOS adds what SUMO lacks — continuous lateral positioning through junctions (SUMO snaps to internal lane centers)
- Skipping SUMO's mid-junction waiting (internal junctions) — simpler, signal-phase handling covers left-turn yield scenarios
- Guide lines through junctions should be toggleable for clean presentation mode vs debug mode

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `velos-vehicle/src/sublane.rs`: probe-based gap scanning, `compute_desired_lateral()`, HCMC-calibrated params (0.5m min gap, 1.2m/s max lateral speed) — reuse for approach/departure edge filtering
- `velos-vehicle/src/intersection.rs`: TTC gap acceptance with size intimidation + wait-time modifier — reuse for approach-phase junction entry checks
- `velos-gpu/src/sim_render.rs`: 2D agent instances, signal indicators, road edge lines — extend with Bezier guide lines and rotated agent shapes
- `velos-gpu/src/sim_snapshot.rs`: `SimSnapshot` with `lateral_offsets` per agent — extend with junction t-parameter for rendering

### Established Patterns
- ECS components in velos-core (Position, Kinematics, LateralOffset) — add JunctionState component for t-parameter and turn movement ID
- GPU-instanced rendering via AgentInstance struct — extend with rotation field for Bezier tangent heading
- Network loaded via petgraph in velos-net — extend edge/junction data with precomputed Bezier curves and ConflictPoints

### Integration Points
- Network import (velos-net): precompute Bezier curves and conflict points during OSM import / graph construction
- Frame pipeline (velos-core): add junction traversal phase between edge transition and position update
- Renderer (velos-gpu): new map tile render pass before agent pass, junction guide line overlay
- egui UI: new checkboxes for guide lines toggle, conflict zone debug overlay

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 16-intersection-sublane-model*
*Context gathered: 2026-03-09*
