# VELOS v2 Data Pipeline — Ho Chi Minh City

## Resolves: W15 (Time-of-Day Demand Profiles)

---

## 1. Data Sources for HCMC POC

| Data Type | Source | Format | License | Quality |
|-----------|--------|--------|---------|---------|
| Road network | OpenStreetMap (Geofabrik) | PBF | ODbL | Good — HCMC well-mapped |
| Traffic counts | HCMC Dept. of Transport | CSV/Excel | Gov open data | Medium — limited coverage |
| Signal timing | Field survey + HCMC DOT | Manual entry / CSV | N/A | Low — most signals undocumented |
| GPS probes | Grab/Be taxi fleets (if partnership) | CSV/Parquet | Commercial | High — millions of trips |
| Bus routes | HCMC Bus Management Center | GTFS | Public | Good — official GTFS available |
| Building footprints | OSM | PBF | ODbL | Good — major buildings mapped |
| Elevation (DEM) | SRTM 30m | GeoTIFF | Public domain | Sufficient |

---

## 2. Network Import Pipeline

### Step 1: OSM Download

```bash
# HCMC metro area extract (~150MB PBF)
wget https://download.geofabrik.de/asia/vietnam-latest.osm.pbf

# Clip to POC area (Districts 1, 3, 5, 10, Binh Thanh)
osmium extract --bbox=106.64,10.75,106.72,10.82 \
    vietnam-latest.osm.pbf -o hcmc-poc.osm.pbf
```

### Step 2: OSM → VELOS Network Graph

```rust
pub struct NetworkImporter {
    pub min_road_class: RoadClass, // default: Tertiary (skip alleys < 4m wide)
    pub default_speed: HashMap<RoadClass, f32>,
    pub lane_width: f32,           // 3.5m for cars, but HCMC reality is often 2.8-3.0m
}

pub enum RoadClass {
    Motorway,     // QL1, QL22, QL13 (national highways near HCMC)
    Trunk,        // Vo Van Kiet, Nguyen Van Linh
    Primary,      // Nguyen Thi Minh Khai, Cach Mang Thang Tam
    Secondary,    // District-level arterials
    Tertiary,     // Local streets
    Residential,  // Hem (alleys) — included but simplified
}
```

**HCMC-Specific Parsing Rules:**

1. **One-way streets:** HCMC has many one-way streets (especially District 1). OSM `oneway=yes` is well-tagged.
2. **Motorbike-only lanes:** Some streets have dedicated motorbike lanes. Parse `motorcycle=designated`.
3. **Bus lanes:** Phan Dang Luu, Vo Van Kiet. Parse `bus=designated`.
4. **U-turns:** HCMC uses many median U-turn points instead of left turns. Import as junction nodes with U-turn edges.
5. **Roundabouts:** Phu Dong, Dien Bien Phu. Parse `junction=roundabout`.
6. **Missing lane counts:** If OSM doesn't tag lane count, infer from road class:

```rust
fn infer_lanes(road_class: RoadClass) -> u8 {
    match road_class {
        Motorway => 3,    // per direction
        Trunk => 3,
        Primary => 2,
        Secondary => 2,
        Tertiary => 1,
        Residential => 1,
    }
}
```

### Step 3: Network Cleaning

```rust
pub fn clean_network(graph: &mut RoadGraph) {
    // 1. Remove disconnected components (keep largest)
    remove_small_components(graph, min_edges: 10);

    // 2. Merge very short edges (< 5m) into adjacent edges
    merge_short_edges(graph, min_length: 5.0);

    // 3. Simplify geometry (Douglas-Peucker, tolerance 2m)
    simplify_edge_geometry(graph, tolerance: 2.0);

    // 4. Validate: all edges have length > 0, all junctions reachable
    validate_connectivity(graph);
}
```

**Expected network size for POC area:**
- Junctions: ~12,000-15,000
- Edges: ~20,000-25,000
- Total road length: ~400-500 km

---

## 3. Signal Timing Data

### Reality Check

HCMC signal timing data is poorly documented. Most intersections use fixed-time plans, but:
- Cycle lengths vary: 60s-120s
- Phase splits are often hand-tuned by local traffic police
- No central SCATS/SCOOT system (unlike Singapore, Seoul)
- Many intersections are unsignalized (priority by size/aggression)

### Data Collection Strategy

**Tier 1 — Known signals (30% of junctions):**
- Field survey of major intersections (Phu Dong roundabout, Nguyen Hue/Le Loi, etc.)
- Extract from Google Street View timestamps (green/red phase durations)
- Partnership with HCMC DOT for any available timing plans

**Tier 2 — Inferred signals (30% of junctions):**
- Classify junction type from OSM tags and aerial imagery
- Apply default signal timing by junction type:

```rust
pub fn default_signal_timing(junction: &Junction) -> SignalPlan {
    let cycle = match junction.leg_count() {
        2 => 60,   // T-junction
        3 => 80,   // 3-way
        4 => 90,   // 4-way
        _ => 100,  // roundabout or complex
    };

    // Equal split with 3s all-red clearance
    let phase_count = junction.leg_count();
    let green_per_phase = (cycle - phase_count as u32 * 3) / phase_count as u32;

    SignalPlan::fixed_time(cycle, green_per_phase, all_red: 3)
}
```

**Tier 3 — Unsignalized (40% of junctions):**
- Priority rules: larger road has priority
- Gap acceptance model for minor road vehicles
- No signal plan — pure yield/priority behavior

```rust
pub enum JunctionControl {
    Signalized(SignalPlan),
    PriorityRule { major_edge_ids: Vec<EdgeId> },
    Uncontrolled,  // first-come-first-served (rare, only in alleys)
}
```

---

## 4. Demand Generation

### OD Matrix from Available Data

**Option A: GPS Probe Data (Preferred)**

If Grab/Be partnership provides trip data:

```
1. Filter trips within POC bounding box
2. Map trip origins/destinations to nearest network nodes
3. Aggregate into OD matrix by (origin_zone, destination_zone, time_period)
4. Scale by sample rate (Grab ~15% of trips → multiply by ~7)
```

**Option B: Gravity Model (Fallback)**

Without GPS data, use gravity model calibrated from traffic counts:

```
T_ij = K * P_i * A_j * f(d_ij)

Where:
  T_ij = trips from zone i to zone j
  P_i  = production (population + employment of zone i)
  A_j  = attraction (employment + commercial area of zone j)
  f(d_ij) = deterrence function: exp(-beta * d_ij)
  K    = calibration constant
```

Zone data from HCMC statistical yearbook (population, employment by ward).

### Time-of-Day Profiles (W15)

```rust
pub struct DemandProfile {
    pub time_factors: Vec<(f32, f32)>,  // (hour, scaling_factor)
}

pub fn hcmc_weekday_profile() -> DemandProfile {
    DemandProfile {
        time_factors: vec![
            (0.0, 0.05),   // midnight
            (5.0, 0.10),   // early morning
            (6.0, 0.40),   // morning ramp
            (6.5, 0.80),   // approaching peak
            (7.0, 1.00),   // AM PEAK
            (8.0, 1.00),   // AM PEAK
            (8.5, 0.80),   // declining
            (9.0, 0.50),   // mid-morning
            (11.0, 0.60),  // lunch buildup
            (12.0, 0.70),  // LUNCH PEAK
            (13.0, 0.50),  // post-lunch
            (15.0, 0.50),  // afternoon
            (16.0, 0.80),  // PM ramp
            (17.0, 1.00),  // PM PEAK
            (18.0, 1.00),  // PM PEAK
            (18.5, 0.80),  // declining
            (19.0, 0.50),  // evening
            (20.0, 0.30),  // night
            (22.0, 0.10),  // late night
        ],
    }
}

pub fn hcmc_weekend_profile() -> DemandProfile {
    DemandProfile {
        time_factors: vec![
            (0.0, 0.05),
            (7.0, 0.20),
            (9.0, 0.50),   // later start
            (10.0, 0.70),  // shopping/leisure
            (12.0, 0.80),  // lunch
            (14.0, 0.60),
            (16.0, 0.70),  // afternoon leisure
            (18.0, 0.80),  // evening dining
            (20.0, 0.50),
            (22.0, 0.20),
        ],
    }
}
```

### Seasonal / Event Demand

For POC, support event-based demand spikes:

```rust
pub struct DemandEvent {
    pub name: String,                    // "Tet Holiday", "Football match"
    pub time_range: (SimTime, SimTime),
    pub affected_zones: Vec<ZoneId>,
    pub multiplier: f32,                 // 1.5 = 50% more trips
    pub additional_od: Option<ODMatrix>, // extra trips to/from event venue
}
```

---

## 5. Traffic Count Data for Calibration

### Available Sources

1. **HCMC DOT automated counters:** ~50 locations, hourly counts, limited to major arterials
2. **Manual counts (field survey):** Can deploy for critical calibration points
3. **Google Maps traffic layer:** Qualitative (green/yellow/red) — useful for validation, not calibration
4. **Waze data:** If partnership available, incident reports + speed data

### Calibration Workflow

```
1. Import traffic counts for N locations (target: 50+)
2. Run simulation with initial demand (gravity model or GPS-derived OD)
3. Compare simulated vs. observed: GEH statistic per link
4. If GEH > 5 for >15% of links:
   a. Run Bayesian optimization (argmin crate) to tune:
      - OD matrix scaling factors per zone pair
      - IDM parameters per road class
      - Signal timing offsets
   b. Re-run simulation
   c. Repeat until convergence (GEH < 5 for 85%+ links)
5. Validate against held-out 20% of count locations
```

**GEH Statistic:**

```rust
pub fn geh(simulated: f32, observed: f32) -> f32 {
    let diff = simulated - observed;
    let sum = simulated + observed;
    if sum < 1.0 { return 0.0; }
    (2.0 * diff * diff / sum).sqrt()
}

// GEH < 5: acceptable
// GEH < 4: good
// GEH < 3: excellent
```

---

## 6. Bus Route Import (GTFS)

HCMC has official GTFS data for ~130 bus routes.

```rust
pub struct GTFSImporter;

impl GTFSImporter {
    pub fn import(gtfs_path: &Path, network: &RoadGraph) -> Vec<BusRoute> {
        // 1. Parse GTFS stops.txt → map to nearest network nodes
        // 2. Parse GTFS routes.txt + trips.txt → route sequences
        // 3. Parse GTFS stop_times.txt → timetables
        // 4. Map-match route shapes to network edges
        // 5. Generate BusRoute structs with stop positions on edges
    }
}

pub struct BusRoute {
    pub route_id: String,
    pub name: String,           // "Bus 01: Ben Thanh - Cho Lon"
    pub edges: Vec<EdgeId>,     // route path on network
    pub stops: Vec<BusStopRef>, // stops with positions
    pub headway: Duration,      // e.g., 10 min peak, 20 min off-peak
    pub operating_hours: (f32, f32), // e.g., (5.0, 22.0)
}
```

---

## 7. Data Quality Validation

Before simulation, validate all input data:

```rust
pub struct DataValidator;

impl DataValidator {
    pub fn validate_network(graph: &RoadGraph) -> Vec<DataIssue> {
        let mut issues = vec![];

        // Connectivity: all nodes reachable from largest SCC
        if let Some(unreachable) = find_unreachable_nodes(graph) {
            issues.push(DataIssue::Warning(format!("{} unreachable nodes", unreachable.len())));
        }

        // Edge lengths: flag edges < 1m or > 10km
        for edge in graph.edges() {
            if edge.length < 1.0 {
                issues.push(DataIssue::Warning(format!("Edge {} too short: {}m", edge.id, edge.length)));
            }
            if edge.length > 10000.0 {
                issues.push(DataIssue::Error(format!("Edge {} too long: {}m", edge.id, edge.length)));
            }
        }

        // Speed limits: flag missing or unreasonable
        for edge in graph.edges() {
            if edge.speed_limit == 0.0 || edge.speed_limit > 120.0 {
                issues.push(DataIssue::Warning(format!("Edge {} speed: {} km/h", edge.id, edge.speed_limit)));
            }
        }

        issues
    }

    pub fn validate_demand(od: &ODMatrix, graph: &RoadGraph) -> Vec<DataIssue> {
        let mut issues = vec![];

        // Total demand reasonable? (HCMC: ~2M trips/peak hour for metro area)
        let total = od.total_trips();
        if total < 100_000 || total > 5_000_000 {
            issues.push(DataIssue::Warning(format!("Total demand {} seems unreasonable", total)));
        }

        // All OD zones mapped to network nodes?
        for zone in od.zones() {
            if graph.nearest_node(zone.centroid()).is_none() {
                issues.push(DataIssue::Error(format!("Zone {} not on network", zone.id)));
            }
        }

        issues
    }
}
```
