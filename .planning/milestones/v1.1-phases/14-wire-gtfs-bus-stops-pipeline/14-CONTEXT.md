# Phase 14: Wire GTFS → Bus Stops Pipeline - Context

**Gathered:** 2026-03-08
**Status:** Ready for planning

<domain>
## Phase Boundary

Connect the existing GTFS parser output (`GtfsStop` with lat/lon) to `SimWorld.bus_stops` (edge-based `BusStop` with edge_id + offset_m) so the bus dwell lifecycle is active at real GTFS stop locations. After this phase, buses actually stop at designated GTFS locations rather than the dwell infrastructure being inert.

Requirements: AGT-01, AGT-02
Gap Closure: GTFS → bus_stops integration gap, bus dwell lifecycle E2E flow gap

</domain>

<decisions>
## Implementation Decisions

### Bus Route Assignment
- Precomputed route table: build `HashMap<route_id, Vec<usize>>` at startup mapping each GTFS route to its ordered stop indices in `SimWorld.bus_stops`
- All GTFS trips spawn bus agents, time-gated by trip departure time matching sim_time — full fidelity
- Route-following via stop edges: buses follow their route's stop edge sequence as waypoints, not random OD pathfinding
- Inter-stop paths computed via CCH shortest path at startup — compute shortest edge path between consecutive stop edges, store full edge sequence per route

### Stop Snapping Strategy
- Nearest-edge snapping: for each GtfsStop (lat/lon), find the nearest road edge using the existing rstar spatial index on the road graph
- Project stop lat/lon onto the nearest edge to compute offset_m (perpendicular projection)
- Snap radius: 50m max — stops beyond 50m from any edge are logged as warnings and skipped (HCMC urban density means nearly all stops should be within 50m)
- Duplicate detection: if multiple GTFS stops snap to the same edge within 10m of each other, merge into one BusStop (HCMC GTFS data may have duplicates for opposite-direction stops)

### GTFS Data Path & Loading
- Convention-based: `data/gtfs/` directory relative to working directory (follows existing `data/hcmc/` pattern)
- Graceful degradation: if `data/gtfs/` missing or empty, SimWorld starts with `bus_stops: Vec::new()` as today — log info "No GTFS data found, bus stops inactive", never crash
- Loading happens in `SimWorld::new()` after road graph is available (snapping needs the graph)
- GTFS loading is a startup-only cost — acceptable to block initialization for the snapping computation

### E2E Scope
- This phase wires existing pieces only — no new models or capabilities
- Stochastic passenger counts kept from Phase 10 — GTFS-derived demand is out of scope
- Bus spawning extended: new GTFS-aware spawn path alongside existing demand spawner (buses from GTFS trips, other vehicles from OD demand)
- BusState, BusDwellModel, should_stop(), begin_dwell(), tick_dwell() unchanged — proven in Phase 10
- FLAG_BUS_DWELLING GPU flag mechanism unchanged

### Claude's Discretion
- Exact spatial index query API for nearest-edge lookup (rstar knn vs brute force on edge segments)
- Whether snapping logic lives in velos-net (graph operations) or velos-demand (GTFS domain)
- Route edge path storage format (Vec<Vec<u32>> per route or flat indexed)
- GTFS bus spawn integration point in the existing Spawner vs a separate BusSpawner

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `load_gtfs_csv()` (velos-demand/src/gtfs.rs): Returns `(Vec<BusRoute>, Vec<BusSchedule>)` — GTFS parsing done, needs consumer
- `GtfsStop` (velos-demand/src/gtfs.rs): Has `lat`, `lon`, `stop_id`, `name` — input for snapping
- `BusStop` (velos-vehicle/src/bus.rs): Has `edge_id`, `offset_m`, `capacity`, `name` — target for snapping
- `BusState::new(stop_indices)` (velos-vehicle/src/bus.rs): Takes indices into bus_stops vec — ready to receive GTFS-derived indices
- `step_bus_dwell()` (velos-gpu/src/sim_bus.rs): Full dwell pipeline using `self.bus_stops` — activates when bus_stops is non-empty
- `SimWorld.bus_stops: Vec<BusStop>` (velos-gpu/src/sim.rs:140): Currently initialized empty — target field to populate
- `RoadGraph` (velos-net): Has edge geometry for spatial queries — needed for stop snapping
- CCH router (velos-net/src/cch/): Provides shortest path queries — needed for inter-stop path computation

### Established Patterns
- Config loading: convention-based paths with graceful degradation (vehicle_params.toml, signal_config.toml, zone_config.toml)
- Startup initialization: heavy computation in SimWorld::new() is accepted (CCH build, perception pipeline, etc.)
- CPU state machine + GPU flag: bus dwell already follows this pattern
- GTFS lives in velos-demand: parser exists, just needs a consumer that bridges to velos-vehicle BusStop format

### Integration Points
- `SimWorld::new()` (velos-gpu/src/sim.rs:247): `bus_stops: Vec::new()` needs GTFS loading + snapping
- `BusRoute.stops: Vec<GtfsStop>` → needs conversion to `Vec<BusStop>` via snapping
- Spawner or new BusSpawner needs to create bus entities with GTFS-derived BusState at trip departure times
- `SimWorld::new()` needs CCH available for inter-stop path computation (currently CCH built in init_reroute)

</code_context>

<specifics>
## Specific Ideas

- This is pure integration/wiring — all models (BusState, BusDwellModel, GTFS parser, dwell pipeline) are proven and tested
- The core engineering challenge is the GtfsStop(lat/lon) → BusStop(edge_id, offset_m) snapping
- Bus spawning becomes time-gated: check sim_time against GTFS trip departures each tick, spawn buses when departure time is reached
- Route table precomputation at startup amortizes the CCH path queries — no per-spawn pathfinding needed

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 14-wire-gtfs-bus-stops-pipeline*
*Context gathered: 2026-03-08*
