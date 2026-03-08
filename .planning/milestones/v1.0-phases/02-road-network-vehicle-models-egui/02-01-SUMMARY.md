---
phase: 02-road-network-vehicle-models-egui
plan: 01
subsystem: network
tags: [osm, petgraph, rstar, road-graph, spatial-index, astar, projection]

requires:
  - phase: 01-gpu-foundation-spikes
    provides: "Cargo workspace, velos-core ECS components (Position, Kinematics)"
provides:
  - "RoadGraph (petgraph DiGraph wrapper) with RoadNode, RoadEdge, RoadClass"
  - "OSM PBF import pipeline for District 1 HCMC"
  - "EquirectangularProjection for lat/lon to local metres"
  - "SpatialIndex (rstar R-tree) for agent neighbor queries"
  - "A* routing via find_route on road graph"
  - "District 1 OSM PBF data file"
affects: [02-02, 02-03, 02-04, velos-vehicle, velos-demand]

tech-stack:
  added: [osmpbf 0.3, petgraph 0.6, rstar 0.12]
  patterns: ["OSM two-pass import (nodes then ways)", "bulk_load R-tree per frame", "travel-time A* with Euclidean heuristic"]

key-files:
  created:
    - crates/velos-net/Cargo.toml
    - crates/velos-net/src/lib.rs
    - crates/velos-net/src/error.rs
    - crates/velos-net/src/projection.rs
    - crates/velos-net/src/graph.rs
    - crates/velos-net/src/osm_import.rs
    - crates/velos-net/src/spatial.rs
    - crates/velos-net/src/routing.rs
    - crates/velos-net/tests/projection_tests.rs
    - crates/velos-net/tests/import_tests.rs
    - crates/velos-net/tests/spatial_tests.rs
    - crates/velos-net/tests/routing_tests.rs
    - data/hcmc/district1.osm.pbf
  modified:
    - Cargo.toml
    - .gitignore

key-decisions:
  - "Overpass API XML converted to PBF via osmium-tool (API returns XML by default)"
  - "Included primary_link, secondary_link, tertiary_link road types alongside their parent classes"
  - "Edge cost = length/speed (travel time), not raw distance, for realistic routing"

patterns-established:
  - "OSM import: two-pass streaming (collect nodes, then build edges from ways)"
  - "Projection at import time: all coordinates stored in local metres, never lat/lon in simulation"
  - "R-tree bulk_load per frame: O(n log n) rebuild, not incremental insert/remove"
  - "Admissible A* heuristic: Euclidean distance / max_speed across all edges"

requirements-completed: [NET-01, NET-02, NET-04, RTE-01]

duration: 7min
completed: 2026-03-06
---

# Phase 02 Plan 01: Road Network Foundation Summary

**OSM PBF import for District 1 HCMC into petgraph DiGraph with equirectangular projection, rstar spatial index, and A* travel-time routing**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-06T14:32:26Z
- **Completed:** 2026-03-06T14:40:14Z
- **Tasks:** 2
- **Files modified:** 15

## Accomplishments
- velos-net crate with 6 modules: error, projection, graph, osm_import, spatial, routing
- District 1 OSM PBF (693KB) imports into directed graph with lane counts, speed limits, oneway rules
- EquirectangularProjection converts lat/lon to local metres centered on District 1 centroid (10.7756, 106.7019)
- SpatialIndex wraps rstar R-tree with bulk_load for O(n log n) neighbor queries
- A* pathfinding with travel-time cost and admissible Euclidean heuristic
- 24 tests: 8 unit (projection, tag parsing), 3 integration (PBF import), 5 spatial, 4 routing, 4 projection integration

## Task Commits

Each task was committed atomically:

1. **Task 1: Scaffold velos-net + graph + projection + OSM import** - `5347536` + `7ea350a` (feat/test -- committed in prior session)
2. **Task 2: R-tree spatial index + A* routing** - `bb1bf47` (feat)

## Files Created/Modified
- `crates/velos-net/Cargo.toml` - Crate manifest with osmpbf, petgraph, rstar deps
- `crates/velos-net/src/lib.rs` - Module re-exports
- `crates/velos-net/src/error.rs` - NetError enum (Io, OsmParse, NoPathFound)
- `crates/velos-net/src/projection.rs` - EquirectangularProjection with project/unproject
- `crates/velos-net/src/graph.rs` - RoadGraph, RoadNode, RoadEdge, RoadClass types
- `crates/velos-net/src/osm_import.rs` - Two-pass OSM PBF import with tag parsing
- `crates/velos-net/src/spatial.rs` - SpatialIndex with AgentPoint and R-tree queries
- `crates/velos-net/src/routing.rs` - find_route with petgraph::algo::astar
- `crates/velos-net/tests/projection_tests.rs` - Center, offset, symmetry, roundtrip tests
- `crates/velos-net/tests/import_tests.rs` - PBF load, edge properties, bidirectional checks
- `crates/velos-net/tests/spatial_tests.rs` - Empty, bulk_load, radius, nearest tests
- `crates/velos-net/tests/routing_tests.rs` - Diamond graph, disconnected, cost, self-route
- `data/hcmc/district1.osm.pbf` - District 1 OSM PBF extract (693KB)
- `Cargo.toml` - Added velos-net to workspace, osmpbf/petgraph/rstar workspace deps
- `.gitignore` - Added exception for data/hcmc/*.osm.pbf

## Decisions Made
- Overpass API returns XML, not PBF -- installed osmium-tool to convert XML to PBF format
- Included road link types (primary_link, secondary_link, tertiary_link) alongside parent classes for better graph connectivity
- Edge cost uses travel time (length/speed) rather than raw distance for A* -- produces more realistic routes
- R-tree uses squared distance internally (locate_within_distance) for performance

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Overpass API returns XML, not PBF**
- **Found during:** Task 1 (OSM PBF download)
- **Issue:** Overpass API `/api/map` endpoint returns XML format, but osmpbf crate requires PBF
- **Fix:** Installed osmium-tool via brew, converted XML to PBF with `osmium cat`
- **Files modified:** data/hcmc/district1.osm.pbf
- **Verification:** `file` confirms PBF format, import_osm loads successfully
- **Committed in:** 5347536 (prior session)

**2. [Rule 1 - Bug] Clippy: match expression should use matches! macro**
- **Found during:** Task 1 (clippy check)
- **Issue:** `match oneway_tag { ... => true, _ => false }` flagged by clippy
- **Fix:** Changed to `matches!(oneway_tag, Some("yes") | Some("1") | Some("true"))`
- **Files modified:** crates/velos-net/src/osm_import.rs
- **Committed in:** 5347536 (prior session)

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes necessary for correct operation. No scope creep.

## Issues Encountered
- Task 1 files were partially committed in a prior session (02-03 plan execution touched velos-net). Verified all code present, committed remaining spatial.rs and routing.rs in Task 2.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Road graph ready for vehicle simulation (velos-vehicle can reference edges)
- Spatial index ready for neighbor detection in IDM/MOBIL
- A* routing ready for path assignment in demand spawner
- District 1 PBF available for all integration tests

---
*Phase: 02-road-network-vehicle-models-egui*
*Completed: 2026-03-06*
