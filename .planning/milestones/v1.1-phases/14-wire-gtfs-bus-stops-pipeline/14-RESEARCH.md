# Phase 14: Wire GTFS -> Bus Stops Pipeline - Research

**Researched:** 2026-03-08
**Domain:** Integration wiring -- GTFS data -> road graph snapping -> bus dwell lifecycle activation
**Confidence:** HIGH

## Summary

This phase is pure integration wiring with no new algorithms or models. All components exist and are tested: `load_gtfs_csv()` parses GTFS data into `(Vec<BusRoute>, Vec<BusSchedule>)`, `BusStop` holds edge-snapped stop locations, `BusState` manages stop progression and dwell lifecycle, and `step_bus_dwell()` already runs every frame scanning `self.bus_stops`. The gap is that `SimWorld.bus_stops` is initialized as `Vec::new()`, so the dwell pipeline is inert.

The core engineering challenge is **GtfsStop(lat/lon) -> BusStop(edge_id, offset_m) snapping**: projecting geographic coordinates onto the nearest road edge. This requires building an rstar R-tree index over edge segment geometry (not the existing agent SpatialIndex), then for each GTFS stop, finding the nearest edge segment and computing the perpendicular projection offset. Secondary challenges are precomputing route edge tables via CCH and time-gating bus spawns by GTFS trip departure times.

**Primary recommendation:** Implement snapping in `velos-net` (graph operations domain), GTFS-aware bus spawning as a separate `BusSpawner` in `velos-demand`, and wire both into `SimWorld::new()` / `spawn_agents()` in `velos-gpu`.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Precomputed route table: build `HashMap<route_id, Vec<usize>>` at startup mapping each GTFS route to its ordered stop indices in `SimWorld.bus_stops`
- All GTFS trips spawn bus agents, time-gated by trip departure time matching sim_time -- full fidelity
- Route-following via stop edges: buses follow their route's stop edge sequence as waypoints, not random OD pathfinding
- Inter-stop paths computed via CCH shortest path at startup -- compute shortest edge path between consecutive stop edges, store full edge sequence per route
- Nearest-edge snapping: for each GtfsStop (lat/lon), find the nearest road edge using the existing rstar spatial index on the road graph
- Project stop lat/lon onto the nearest edge to compute offset_m (perpendicular projection)
- Snap radius: 50m max -- stops beyond 50m from any edge are logged as warnings and skipped
- Duplicate detection: if multiple GTFS stops snap to the same edge within 10m of each other, merge into one BusStop
- Convention-based: `data/gtfs/` directory relative to working directory
- Graceful degradation: if `data/gtfs/` missing or empty, SimWorld starts with `bus_stops: Vec::new()` as today -- log info, never crash
- Loading happens in `SimWorld::new()` after road graph is available
- GTFS loading is a startup-only cost -- acceptable to block initialization
- This phase wires existing pieces only -- no new models or capabilities
- Stochastic passenger counts kept from Phase 10 -- GTFS-derived demand is out of scope
- Bus spawning extended: new GTFS-aware spawn path alongside existing demand spawner
- BusState, BusDwellModel, should_stop(), begin_dwell(), tick_dwell() unchanged
- FLAG_BUS_DWELLING GPU flag mechanism unchanged

### Claude's Discretion
- Exact spatial index query API for nearest-edge lookup (rstar knn vs brute force on edge segments)
- Whether snapping logic lives in velos-net (graph operations) or velos-demand (GTFS domain)
- Route edge path storage format (Vec<Vec<u32>> per route or flat indexed)
- GTFS bus spawn integration point in the existing Spawner vs a separate BusSpawner

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| AGT-01 | Bus agents with empirical dwell time model (5s + 0.5s/boarding + 0.67s/alighting, cap 60s) | BusDwellModel, BusState, step_bus_dwell() all exist and tested. This phase activates them by populating bus_stops from GTFS data so should_stop() triggers. |
| AGT-02 | GTFS import for 130 HCMC bus routes with stop locations and schedules | load_gtfs_csv() parser exists. This phase adds: stop snapping (lat/lon -> edge_id + offset_m), route table precomputation, and GTFS-aware bus spawning by trip departure time. |
</phase_requirements>

## Standard Stack

### Core (all already in workspace)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rstar | workspace | R-tree spatial index for nearest-edge queries | Already used for SpatialIndex; same crate for edge geometry index |
| petgraph | workspace | Road graph traversal, edge geometry access | Already the graph backbone |
| velos-net CCHRouter | custom | Inter-stop shortest path computation | Already built and cached |
| hecs | workspace | ECS entity spawning with BusState component | Already the entity system |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| log | workspace | Warning/info logging for skipped stops | All graceful degradation paths |
| rand | workspace | Stochastic passenger counts (already in use) | step_bus_dwell passenger generation |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| rstar R-tree for edges | Brute-force nearest edge | Brute-force O(S*E) where S=stops, E=edges; R-tree O(S*log E). With 25K edges and ~2K stops, brute force would take ~50M point-segment distance calculations. R-tree reduces to ~2K * ~15 = 30K. **Recommendation: use rstar R-tree** |
| Flat Vec<Vec<u32>> for route paths | Indexed flat buffer | Vec<Vec<u32>> is simpler, startup-only cost, 130 routes. **Recommendation: Vec<Vec<u32>> -- simplicity wins** |

**Installation:** No new dependencies required. All libraries already in workspace.

## Architecture Patterns

### Recommended Project Structure
```
crates/velos-net/src/
    snap.rs              # NEW: GtfsStop -> BusStop snapping (edge R-tree, projection)

crates/velos-demand/src/
    bus_spawner.rs       # NEW: GTFS-aware bus spawning (trip schedules, time-gating)
    gtfs.rs              # EXISTING: load_gtfs_csv() -- unchanged

crates/velos-gpu/src/
    sim_startup.rs       # MODIFIED: add load_gtfs_bus_stops() function
    sim.rs               # MODIFIED: call GTFS loading in SimWorld::new()
    sim_lifecycle.rs     # MODIFIED: call bus spawner alongside demand spawner
    sim_bus.rs           # EXISTING: step_bus_dwell() -- unchanged
```

### Pattern 1: Edge Segment R-Tree for Stop Snapping
**What:** Build an rstar R-tree over edge segments (not nodes) for nearest-edge lookup. Each edge's polyline geometry is decomposed into line segments, each stored as an AABB envelope in the R-tree. For a query point, find the nearest segment, then compute perpendicular projection for offset_m.
**When to use:** During GTFS stop snapping at startup.
**Why in velos-net:** Snapping is a graph-spatial operation (projecting points onto edges). velos-net owns the graph, the edge geometry, and already uses rstar. This keeps velos-demand focused on GTFS parsing without graph dependency.

```rust
// velos-net/src/snap.rs
use rstar::{RTree, RTreeObject, PointDistance, AABB};

/// A segment of a road edge, used for nearest-edge spatial queries.
pub struct EdgeSegment {
    pub edge_id: u32,
    pub segment_start: [f64; 2],
    pub segment_end: [f64; 2],
    pub offset_along_edge: f64, // cumulative distance from edge start to segment start
}

impl RTreeObject for EdgeSegment {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(
            [self.segment_start[0].min(self.segment_end[0]),
             self.segment_start[1].min(self.segment_end[1])],
            [self.segment_start[0].max(self.segment_end[0]),
             self.segment_start[1].max(self.segment_end[1])],
        )
    }
}

/// Project point onto line segment, return (nearest_point, t_parameter).
fn project_point_onto_segment(
    point: [f64; 2],
    seg_start: [f64; 2],
    seg_end: [f64; 2],
) -> (f64, [f64; 2]) {
    let dx = seg_end[0] - seg_start[0];
    let dy = seg_end[1] - seg_start[1];
    let len_sq = dx * dx + dy * dy;
    if len_sq < 1e-12 {
        return (0.0, seg_start);
    }
    let t = ((point[0] - seg_start[0]) * dx + (point[1] - seg_start[1]) * dy) / len_sq;
    let t = t.clamp(0.0, 1.0);
    let nearest = [seg_start[0] + t * dx, seg_start[1] + t * dy];
    (t, nearest)
}

/// Build an edge segment R-tree from the road graph.
pub fn build_edge_rtree(graph: &RoadGraph) -> RTree<EdgeSegment> {
    // Decompose each edge's geometry polyline into segments
    // Each segment carries edge_id and cumulative offset
}

/// Snap a geographic point to the nearest edge within max_radius.
/// Returns (edge_id, offset_m, distance) or None if beyond radius.
pub fn snap_to_nearest_edge(
    tree: &RTree<EdgeSegment>,
    point: [f64; 2],
    max_radius: f64,
) -> Option<(u32, f64, f64)> {
    // Use rstar nearest_neighbor query, then project point onto that segment
    // Compute offset_m = segment.offset_along_edge + t * segment_length
}
```

### Pattern 2: GTFS Bus Spawner (Separate from OD Spawner)
**What:** A `BusSpawner` struct that holds precomputed route tables and schedules, and each tick checks `sim_time` against trip departure times to emit bus spawn requests.
**When to use:** Called from `spawn_agents()` alongside the existing OD `Spawner`.
**Why separate:** Bus spawning is schedule-driven (GTFS trip times), not stochastic OD-driven. Mixing into the existing Spawner would conflate two unrelated mechanisms.

```rust
// velos-demand/src/bus_spawner.rs
pub struct BusSpawner {
    /// Per-route: ordered stop indices into the global bus_stops vec.
    pub route_stops: HashMap<String, Vec<usize>>,
    /// Per-route: precomputed edge path (CCH shortest paths between consecutive stops).
    pub route_edge_paths: HashMap<String, Vec<u32>>,
    /// All trip schedules sorted by departure time.
    pub schedules: Vec<BusSchedule>,
    /// Index of next unspawned trip (schedules sorted by first stop_time).
    next_trip_index: usize,
}

impl BusSpawner {
    /// Check sim_time and return spawn requests for trips whose departure <= sim_time.
    pub fn generate_bus_spawns(&mut self, sim_time_s: f64) -> Vec<BusSpawnRequest> {
        // Advance next_trip_index, emit requests for ready trips
    }
}

pub struct BusSpawnRequest {
    pub route_id: String,
    pub trip_id: String,
    pub stop_indices: Vec<usize>,
    pub edge_path: Vec<u32>,
}
```

### Pattern 3: Graceful Degradation (Convention-Based Loading)
**What:** Follow the established config loading pattern from sim_startup.rs.
**When to use:** All startup loading paths.

```rust
// sim_startup.rs
pub(crate) fn load_gtfs_bus_stops(road_graph: &RoadGraph) -> (Vec<BusStop>, Option<BusSpawner>) {
    let gtfs_path = std::env::var("VELOS_GTFS_PATH")
        .unwrap_or_else(|_| "data/gtfs".to_string());
    let path = std::path::Path::new(&gtfs_path);

    if !path.exists() || !path.is_dir() {
        log::info!("No GTFS data found at '{}', bus stops inactive", gtfs_path);
        return (Vec::new(), None);
    }

    match velos_demand::load_gtfs_csv(path) {
        Ok((routes, schedules)) => {
            // Snap stops, build route tables, create BusSpawner
        }
        Err(e) => {
            log::warn!("Failed to load GTFS data: {}. Bus stops inactive.", e);
            (Vec::new(), None)
        }
    }
}
```

### Anti-Patterns to Avoid
- **Mixing bus spawn logic into the OD Spawner:** Bus spawning is schedule-driven, not probabilistic. Keep them separate.
- **Snapping stops without the edge geometry polyline:** Using only node positions would snap to intersections, not mid-edge positions. Must use the `geometry: Vec<[f64; 2]>` field on RoadEdge.
- **Running CCH queries per-tick for bus routing:** Precompute all inter-stop paths at startup. CCH query_with_path returns node sequences; convert to edge sequences once.
- **Modifying BusState or BusDwellModel:** These are proven and tested. The only change is providing real stop indices instead of empty ones.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Point-to-edge nearest lookup | Linear scan over all edges | rstar R-tree over edge segments | 25K edges * polyline segments = ~50K+ segments. O(n) per stop vs O(log n) |
| Inter-stop shortest path | BFS/Dijkstra on raw graph | CCHRouter::query_with_path | CCH already built and cached; 0.02ms per query vs ~5ms for Dijkstra on 25K edges |
| Perpendicular projection | Approximate with node distances | Proper vector projection onto line segment | Node-only snapping gives wrong offset_m; stops between intersections need exact projection |
| WGS84 to local metres | Manual Haversine + flat earth | Existing coordinate system in RoadGraph | RoadNode.pos is already [x_east, y_north] in local metres. GtfsStop lat/lon needs one conversion |

**Key insight:** The coordinate conversion from WGS84 (GTFS lat/lon) to local metres (RoadGraph coordinate system) is the one piece that might not exist yet. Check if the OSM import pipeline has a projection utility. If not, a simple equirectangular approximation (cos(lat_ref) scaling) is sufficient for HCMC's small geographic extent.

## Common Pitfalls

### Pitfall 1: WGS84 to Local Coordinate Mismatch
**What goes wrong:** GTFS stops have lat/lon (WGS84) but RoadGraph uses local metres [x_east, y_north]. Snapping without coordinate conversion produces garbage distances.
**Why it happens:** The OSM import pipeline converts coordinates during graph construction, but the conversion function may not be exposed publicly.
**How to avoid:** Find the projection reference point used during OSM import (likely the graph centroid or a fixed HCMC reference). Apply the same equirectangular projection to GTFS stop coordinates before snapping.
**Warning signs:** All stops fail the 50m snap radius; snapped stops cluster at graph origin.

### Pitfall 2: Edge Geometry Direction Matters for offset_m
**What goes wrong:** offset_m computed relative to segment start, but geometry polyline direction may not match edge direction (source -> target).
**Why it happens:** OSM ways can have arbitrary direction; the import may or may not reverse geometry to match edge direction.
**How to avoid:** Verify that `geometry[0]` corresponds to the edge source node position and `geometry.last()` to the target. If reversed, the offset_m needs to be computed as `edge.length_m - raw_offset`.
**Warning signs:** Buses stop at the wrong end of edges; should_stop() never triggers despite correct edge_id.

### Pitfall 3: CCH Node IDs vs Graph Node Indices
**What goes wrong:** CCH uses original node indices (u32), but inter-stop path computation needs edge IDs. `query_with_path` returns node sequences, not edge sequences.
**Why it happens:** CCH operates on nodes; the edge-to-node mapping (EdgeNodeMap) converts between domains.
**How to avoid:** After `query_with_path` returns `Vec<u32>` node path, convert consecutive node pairs to edge IDs via `graph.find_edge(NodeIndex(a), NodeIndex(b))`. This is exactly what `spawn_single_agent` already does.
**Warning signs:** Bus route has nodes but no edges; bus can't follow route because Route component expects node path.

### Pitfall 4: CCH Not Available During GTFS Loading
**What goes wrong:** `SimWorld::new()` calls `init_reroute()` at the end (which builds CCH). If GTFS loading needs CCH for inter-stop paths, it must happen after CCH construction.
**Why it happens:** Current init order: graph available -> ... -> init_reroute() builds CCH.
**How to avoid:** Either (a) move CCH construction earlier, (b) build a separate lightweight CCH instance for GTFS path computation, or (c) restructure init_reroute to return the CCH router so GTFS loading can use it. Option (a) is simplest: call init_reroute() before GTFS loading, then use `self.reroute.cch_router` for inter-stop paths.
**Warning signs:** CCH router is None when GTFS loading tries to use it.

### Pitfall 5: BusState stop_indices vs bus_stops Vec Invalidation
**What goes wrong:** BusState holds `Vec<usize>` indices into the global `bus_stops` vec. If bus_stops is modified after BusState creation, indices become invalid.
**Why it happens:** bus_stops is populated at startup and never modified, so this is safe. But if future phases add dynamic stops, indices would break.
**How to avoid:** Populate bus_stops once at startup, freeze the vec, then create all BusStates. Document that bus_stops is immutable after init.
**Warning signs:** Index out of bounds in should_stop(); BusState references non-existent stop index.

## Code Examples

### WGS84 to Local Metres Projection
```rust
/// Convert WGS84 (lat, lon) to local metres [x_east, y_north]
/// using equirectangular approximation around a reference point.
///
/// Sufficient for HCMC's ~15km extent (error < 0.1% at this scale).
pub fn wgs84_to_local(lat: f64, lon: f64, ref_lat: f64, ref_lon: f64) -> [f64; 2] {
    const EARTH_RADIUS_M: f64 = 6_371_000.0;
    let lat_rad = lat.to_radians();
    let ref_lat_rad = ref_lat.to_radians();
    let x = (lon - ref_lon).to_radians() * EARTH_RADIUS_M * ref_lat_rad.cos();
    let y = (lat - ref_lat).to_radians() * EARTH_RADIUS_M;
    [x, y]
}
```

### Perpendicular Projection onto Line Segment
```rust
/// Project point P onto segment AB. Returns (t_param, nearest_point, distance).
/// t_param in [0, 1] indicates position along segment.
fn project_onto_segment(
    p: [f64; 2], a: [f64; 2], b: [f64; 2]
) -> (f64, [f64; 2], f64) {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let ab_sq = ab[0] * ab[0] + ab[1] * ab[1];
    if ab_sq < 1e-12 {
        let d = (ap[0] * ap[0] + ap[1] * ap[1]).sqrt();
        return (0.0, a, d);
    }
    let t = (ap[0] * ab[0] + ap[1] * ab[1]) / ab_sq;
    let t = t.clamp(0.0, 1.0);
    let nearest = [a[0] + t * ab[0], a[1] + t * ab[1]];
    let dx = p[0] - nearest[0];
    let dy = p[1] - nearest[1];
    let dist = (dx * dx + dy * dy).sqrt();
    (t, nearest, dist)
}
```

### Duplicate Stop Merging
```rust
/// Merge GTFS stops that snap to the same edge within merge_threshold_m.
fn merge_duplicate_stops(
    mut snapped: Vec<(GtfsStop, u32, f64)>, // (stop, edge_id, offset_m)
    merge_threshold_m: f64,
) -> Vec<BusStop> {
    // Sort by (edge_id, offset_m) for efficient neighbor comparison
    snapped.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.partial_cmp(&b.2).unwrap()));

    let mut bus_stops = Vec::new();
    let mut i = 0;
    while i < snapped.len() {
        let (ref stop, edge_id, offset_m) = snapped[i];
        // Absorb subsequent stops on same edge within threshold
        let mut j = i + 1;
        while j < snapped.len()
            && snapped[j].1 == edge_id
            && (snapped[j].2 - offset_m).abs() < merge_threshold_m
        {
            log::info!("Merging duplicate stop '{}' with '{}'", snapped[j].0.name, stop.name);
            j += 1;
        }
        bus_stops.push(BusStop {
            edge_id,
            offset_m,
            capacity: 40,
            name: stop.name.clone(),
        });
        i = j;
    }
    bus_stops
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| bus_stops: Vec::new() (inert) | GTFS-populated bus_stops | Phase 14 | Activates full bus dwell lifecycle |
| OD-only bus spawning (random routes) | GTFS schedule-driven bus spawning | Phase 14 | Buses follow real HCMC routes and timetables |
| No stop-to-edge mapping | rstar R-tree nearest-edge snapping | Phase 14 | Geographic GTFS data mapped to simulation network |

## Open Questions

1. **WGS84 Reference Point**
   - What we know: RoadGraph stores positions in local metres. The OSM import must have used a reference point for projection.
   - What's unclear: Whether the projection utility is exported from velos-net, or hardcoded in the OSM import pipeline.
   - Recommendation: Search for the projection in the OSM import code. If not exported, extract it as a public utility in velos-net. Failing that, compute reference point as graph centroid.

2. **Bus Route Edge Path: Node-Based or Edge-Based?**
   - What we know: CCHRouter::query_with_path returns `Vec<u32>` node IDs. The existing Route component uses node paths. BusState uses stop indices (into bus_stops, which are edge-based).
   - What's unclear: Whether GTFS buses should use the existing Route component (node path) for navigation between stops, or a custom edge-path representation.
   - Recommendation: Reuse the existing Route component with node paths from CCH. This integrates naturally with spawn_single_agent's existing navigation. The BusState stop_indices remain separate -- they track which stops to dwell at, not the navigation path.

3. **SimWorld Field Growth**
   - What we know: SimWorld already has 20+ fields. Adding a BusSpawner would be field #21+.
   - What's unclear: Whether to add BusSpawner as a SimWorld field or use a different ownership model.
   - Recommendation: Add `bus_spawner: Option<BusSpawner>` to SimWorld, following the pattern of `perception: Option<PerceptionPipeline>`. None when no GTFS data.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (Rust built-in) |
| Config file | Cargo.toml per crate |
| Quick run command | `cargo test -p velos-net --lib snap && cargo test -p velos-demand --lib bus_spawner` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| AGT-01 | Bus stops populated -> should_stop triggers -> dwell lifecycle | integration | `cargo test -p velos-gpu --lib sim_bus` | Existing (sim_bus.rs tests) |
| AGT-01 | begin_dwell and tick_dwell unchanged | unit | `cargo test -p velos-vehicle --lib bus` | Existing |
| AGT-02 | GTFS stop snapping: lat/lon -> edge_id + offset_m | unit | `cargo test -p velos-net --lib snap` | Wave 0 |
| AGT-02 | Snap radius enforcement (50m max) | unit | `cargo test -p velos-net --lib snap` | Wave 0 |
| AGT-02 | Duplicate stop merging (same edge within 10m) | unit | `cargo test -p velos-net --lib snap` | Wave 0 |
| AGT-02 | Graceful degradation: missing GTFS dir | unit | `cargo test -p velos-gpu --lib sim_startup` | Wave 0 |
| AGT-02 | BusSpawner time-gated spawn generation | unit | `cargo test -p velos-demand --lib bus_spawner` | Wave 0 |
| AGT-02 | Route table precomputation (stop indices per route) | unit | `cargo test -p velos-demand --lib bus_spawner` | Wave 0 |
| AGT-02 | E2E: GTFS load -> bus_stops.len() > 0 -> bus dwell activates | integration | `cargo test -p velos-gpu --lib sim -- gtfs` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-net --lib snap && cargo test -p velos-demand --lib bus_spawner && cargo test -p velos-gpu --lib sim_startup`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-net/src/snap.rs` + tests -- edge R-tree, snap_to_nearest_edge, projection logic
- [ ] `crates/velos-demand/src/bus_spawner.rs` + tests -- BusSpawner, time-gated generation, route tables
- [ ] `crates/velos-gpu/src/sim_startup.rs` tests -- load_gtfs_bus_stops graceful degradation
- [ ] `crates/velos-gpu/src/sim.rs` or `sim_lifecycle.rs` integration test -- E2E bus dwell with GTFS stops

## Discretion Recommendations

### Snapping Logic Location: velos-net (RECOMMENDED)
**Rationale:** Snapping is point-to-edge-geometry projection -- a graph-spatial operation. velos-net owns the graph, edge geometry, and already depends on rstar. Placing snapping in velos-demand would require velos-demand to depend on velos-net (creating a coupling that doesn't exist today).

### Spatial Query API: rstar R-tree over Edge Segments (RECOMMENDED)
**Rationale:** The existing SpatialIndex (agent positions) uses rstar point queries. For edge snapping, we need an R-tree over line segments (AABB envelopes of edge geometry segments). This is a different index built once at startup -- not the per-frame agent index. rstar's `nearest_neighbor` on the segment AABB tree gives O(log E) per stop.

### Route Edge Path Format: Vec<Vec<u32>> per Route (RECOMMENDED)
**Rationale:** Only 130 routes. Vec<Vec<u32>> is direct, debug-friendly, and trivially indexable. A flat indexed buffer saves negligible memory at the cost of added complexity. The route paths are read-only after startup.

### Bus Spawn Integration: Separate BusSpawner (RECOMMENDED)
**Rationale:** The existing Spawner is OD-demand-driven with probabilistic vehicle type assignment. Bus spawning from GTFS is schedule-driven with deterministic timing. These are fundamentally different spawn mechanisms. A separate BusSpawner struct in velos-demand keeps concerns clean. SimWorld calls both in spawn_agents().

## Sources

### Primary (HIGH confidence)
- Codebase inspection: velos-demand/src/gtfs.rs -- load_gtfs_csv API, GtfsStop/BusRoute/BusSchedule structs
- Codebase inspection: velos-vehicle/src/bus.rs -- BusStop, BusState, BusDwellModel API
- Codebase inspection: velos-gpu/src/sim.rs -- SimWorld fields, init order, bus_stops: Vec::new()
- Codebase inspection: velos-gpu/src/sim_bus.rs -- step_bus_dwell() implementation
- Codebase inspection: velos-gpu/src/sim_lifecycle.rs -- spawn_single_agent, bus_stop_indices computation
- Codebase inspection: velos-gpu/src/sim_reroute.rs -- init_reroute, CCHRouter availability
- Codebase inspection: velos-net/src/spatial.rs -- rstar R-tree patterns
- Codebase inspection: velos-net/src/graph.rs -- RoadEdge.geometry polyline field
- Codebase inspection: velos-net/src/cch/query.rs -- query_with_path API

### Secondary (MEDIUM confidence)
- rstar crate API: RTreeObject for custom envelope types, nearest_neighbor queries -- based on existing usage in codebase

### Tertiary (LOW confidence)
- WGS84 projection approach: equirectangular approximation -- needs verification against existing OSM import projection

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all libraries already in workspace, APIs verified from source
- Architecture: HIGH -- all integration points inspected, patterns follow established codebase conventions
- Pitfalls: HIGH -- identified from actual code inspection (CCH init order, coordinate systems, edge direction)
- Snapping algorithm: HIGH -- standard computational geometry, well-understood
- WGS84 conversion: MEDIUM -- approach is standard, but need to find/match existing projection reference point

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (stable domain, no external dependency changes expected)
