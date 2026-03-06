# VELOS v2 Visualization & API

## Resolves: W12 (No Horizontal WebSocket Scaling)

---

## 1. Visualization Strategy (POC)

### Primary: deck.gl 2D Analytics Dashboard

deck.gl is the primary visualization for POC because:
- GPU-accelerated: handles 200K+ points at 60 FPS
- MapLibre base map (fully open-source)
- Heatmaps, flow arrows, trip animations built-in
- Browser-based — no install required
- Better suited for traffic analytics than 3D eye-candy

**Layers:**

| Layer | deck.gl Type | Data Source | Update Rate |
|-------|-------------|-------------|-------------|
| Vehicle positions | ScatterplotLayer | WebSocket binary | 10 Hz |
| Motorbike swarms | HeatmapLayer | Aggregated density grid | 2 Hz |
| Traffic flow arrows | IconLayer (rotated) | Edge average speed | 1 Hz |
| Speed heatmap | HeatmapLayer | Edge speed colors | 1 Hz |
| Bus routes | PathLayer | Static route geometry | On load |
| Signal states | ScatterplotLayer | Signal phase colors | 1 Hz |
| Congestion overlay | ColumnLayer | Edge LOS grades | 1 Hz |

**Performance Target:** 280K agents rendered as 280K points on ScatterplotLayer at 60 FPS. deck.gl handles this natively with WebGL instanced rendering.

### Secondary: CesiumJS 3D (Optional)

For stakeholder demos, CesiumJS with:
- OSM-derived 3D building extrusions (from OSM `building:levels`)
- Terrain from SRTM DEM
- Vehicle models at LOD (3D mesh < 500m, billboard < 2km, dot > 2km)
- Self-hosted tiles (PMTiles, no Cesium Ion)

3D is not required for POC calibration/validation — it's presentation-only.

---

## 2. Horizontally Scalable WebSocket Architecture (W12)

### Problem

v1 has a single-process WebSocket relay. Each client subscribes to visible spatial tiles, but the server processes ALL tiles for ALL clients. At 100 concurrent viewers, the relay bottlenecks.

### Solution: Stateless WebSocket Relay Pods + Redis Pub/Sub

```
                    ┌─────────────────────────┐
                    │  Simulation Engine       │
                    │  (produces frames)       │
                    └─────────┬───────────────┘
                              │
                    ┌─────────v───────────────┐
                    │  Redis Pub/Sub           │
                    │  Channel per spatial tile │
                    │  tile:12:34 → frame data │
                    └─────┬───────┬───────┬───┘
                          │       │       │
                    ┌─────v──┐ ┌──v────┐ ┌v──────┐
                    │ WS Pod │ │WS Pod │ │WS Pod │  (2-N stateless pods)
                    │ :8081  │ │:8082  │ │:8083  │
                    └───┬────┘ └──┬────┘ └──┬────┘
                        │         │         │
                    clients    clients    clients
```

**How it works:**

1. Simulation engine publishes frame data to Redis channels, one channel per spatial tile (e.g., `tile:12:34`)
2. Each WebSocket relay pod subscribes only to tiles that its connected clients are viewing
3. When a client connects and sends viewport, the relay pod subscribes to relevant tile channels
4. When a client pans/zooms, the relay pod updates its Redis subscriptions
5. Relay pods are stateless — can be scaled horizontally via K8s HPA

**Spatial Tiling:**

```rust
const TILE_SIZE: f32 = 500.0;  // 500m × 500m tiles

pub fn agent_to_tile(lat: f64, lon: f64) -> (u16, u16) {
    // Relative to HCMC POC origin (10.75°N, 106.64°E)
    let x = ((lon - 106.64) * 111_000.0 / TILE_SIZE as f64) as u16;
    let y = ((lat - 10.75) * 111_000.0 / TILE_SIZE as f64) as u16;
    (x, y)
}
```

POC area (~8km × 8km): 16 × 16 = 256 tiles. Each tile: 500m × 500m.

**Frame Format (FlatBuffers binary):**

```
TileFrame:
  tile_x: u8
  tile_y: u8
  sim_time: f32
  agent_count: u16
  agents: [AgentPosition]  // packed array

AgentPosition:
  x_offset: i16    // meters from tile origin (±32km range)
  y_offset: i16
  speed_cm: u16    // cm/s (0-655 m/s range)
  heading: u8      // 0-255 mapped to 0-360°
  agent_type: u8   // 0=car, 1=motorbike, 2=bus, 3=bicycle, 4=pedestrian
```

**Per-agent: 8 bytes. Per tile (typical 1000 agents): 8KB + header = ~8.1KB.**

Client viewing 4 tiles: ~32KB per frame at 10Hz = 320 KB/s. Manageable for any connection.

**Redis Memory:**

256 tiles × 8KB = ~2MB per frame. At 10 Hz with 3-frame buffer: ~60MB. Trivial for Redis.

### Implementation

```rust
// Simulation side: publish to Redis
pub async fn publish_frame(redis: &RedisClient, frame: &SimFrame) {
    // Group agents by tile
    let mut tile_buffers: HashMap<(u8, u8), Vec<u8>> = HashMap::new();
    for agent in &frame.agents {
        let tile = agent_to_tile(agent.lat, agent.lon);
        tile_buffers.entry(tile)
            .or_default()
            .extend(agent.to_flatbuf_bytes());
    }

    // Publish each tile to its Redis channel (pipeline for efficiency)
    let mut pipe = redis::pipe();
    for ((tx, ty), buffer) in &tile_buffers {
        pipe.publish(format!("tile:{}:{}", tx, ty), buffer);
    }
    pipe.query_async(redis).await;
}

// WebSocket relay pod
pub async fn handle_client(ws: WebSocket, redis: RedisClient) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let subscribed_tiles: Arc<RwLock<HashSet<(u8, u8)>>> = Arc::new(RwLock::new(HashSet::new()));

    // Spawn Redis subscriber task
    let tiles = subscribed_tiles.clone();
    tokio::spawn(async move {
        let mut pubsub = redis.get_async_pubsub().await;
        loop {
            // Subscribe to tiles the client is viewing
            let current_tiles = tiles.read().await.clone();
            for (tx, ty) in &current_tiles {
                pubsub.subscribe(format!("tile:{}:{}", tx, ty)).await;
            }

            // Forward messages to WebSocket
            while let Some(msg) = pubsub.on_message().next().await {
                ws_tx.send(Message::Binary(msg.get_payload_bytes().to_vec())).await;
            }
        }
    });

    // Handle viewport updates from client
    while let Some(msg) = ws_rx.next().await {
        if let Ok(viewport) = parse_viewport(msg) {
            let new_tiles = viewport_to_tiles(viewport);
            *subscribed_tiles.write().await = new_tiles;
        }
    }
}
```

**Scaling:**
- 1 relay pod handles ~50 WebSocket connections (limited by fan-out computation)
- 100 viewers → 2 pods
- 500 viewers → 10 pods
- K8s HPA scales on connection count metric

---

## 3. gRPC API Contract

### Simulation Control

```protobuf
syntax = "proto3";
package velos.v2;

service VelosSimulation {
    // Lifecycle
    rpc LoadNetwork(LoadNetworkRequest) returns (LoadNetworkResponse);
    rpc Start(StartRequest) returns (StartResponse);
    rpc Step(StepRequest) returns (StepResponse);
    rpc Pause(PauseRequest) returns (PauseResponse);
    rpc Resume(ResumeRequest) returns (ResumeResponse);
    rpc Reset(ResetRequest) returns (ResetResponse);

    // Checkpoint
    rpc SaveCheckpoint(SaveCheckpointRequest) returns (SaveCheckpointResponse);
    rpc LoadCheckpoint(LoadCheckpointRequest) returns (LoadCheckpointResponse);

    // Agent management
    rpc AddVehicle(AddVehicleRequest) returns (AddVehicleResponse);
    rpc RemoveAgent(RemoveAgentRequest) returns (RemoveAgentResponse);
    rpc RerouteAgent(RerouteAgentRequest) returns (RerouteAgentResponse);

    // Network mutation (what-if)
    rpc BlockEdge(BlockEdgeRequest) returns (BlockEdgeResponse);
    rpc SetSignalTiming(SetSignalTimingRequest) returns (SetSignalTimingResponse);

    // Streaming
    rpc SubscribeAgentPositions(SubscribeRequest) returns (stream AgentFrame);
    rpc SubscribeEdgeStats(SubscribeEdgeRequest) returns (stream EdgeStatsFrame);

    // Query
    rpc GetSimulationStats(Empty) returns (SimulationStats);
    rpc GetEdgeState(GetEdgeRequest) returns (EdgeState);

    // Scenario
    rpc CreateScenario(CreateScenarioRequest) returns (CreateScenarioResponse);
    rpc RunScenario(RunScenarioRequest) returns (stream ScenarioProgress);
    rpc CompareScenarios(CompareRequest) returns (CompareResponse);
}
```

### Error Handling

```protobuf
message VelosError {
    ErrorCode code = 1;
    string message = 2;
    map<string, string> details = 3;
}

enum ErrorCode {
    UNKNOWN = 0;
    NETWORK_NOT_LOADED = 1;
    SIMULATION_NOT_RUNNING = 2;
    EDGE_NOT_FOUND = 3;
    JUNCTION_NOT_FOUND = 4;
    AGENT_NOT_FOUND = 5;
    INVALID_ROUTE = 6;
    CAPACITY_EXCEEDED = 7;
    INVALID_SIGNAL_PHASE = 8;
    SCENARIO_NOT_FOUND = 9;
    CHECKPOINT_NOT_FOUND = 10;
    CHECKPOINT_CORRUPTED = 11;
    GRIDLOCK_DETECTED = 12;
    CALIBRATION_NOT_CONVERGED = 13;
}
```

### Key Messages

```protobuf
message AddVehicleRequest {
    uint32 origin_edge = 1;
    uint32 destination_junction = 2;
    AgentType type = 3;
    string profile = 4;  // "hcmc_commuter_motorbike", "hcmc_taxi_car", etc.
    optional float departure_time = 5;
}

message AgentFrame {
    float sim_time = 1;
    uint32 step_number = 2;
    repeated AgentPosition agents = 3;
}

message AgentPosition {
    uint32 id = 1;
    double lat = 2;
    double lon = 3;
    float speed = 4;     // m/s
    float heading = 5;   // degrees
    AgentType type = 6;
}

message SimulationStats {
    float sim_time = 1;
    uint32 total_agents = 2;
    float avg_speed_kmh = 3;
    float total_delay_hours = 4;
    uint32 gridlock_count = 5;
    float frame_time_ms = 6;
    float gpu_utilization = 7;
    map<string, float> los_distribution = 8;  // "A": 0.15, "B": 0.25, ...
}

message SaveCheckpointRequest {
    string checkpoint_name = 1;  // optional, defaults to "auto_{sim_time}"
}

message SaveCheckpointResponse {
    string checkpoint_path = 1;
    uint64 size_bytes = 2;
    float save_duration_ms = 3;
}

message LoadCheckpointRequest {
    string checkpoint_path = 1;
}
```

---

## 4. REST API (Convenience Layer)

For non-streaming use cases (dashboards, notebooks), wrap gRPC with REST via axum:

```
GET  /api/v1/status                → SimulationStats
GET  /api/v1/agents?bbox=...       → AgentPosition[] (paginated)
GET  /api/v1/edges/{id}            → EdgeState
POST /api/v1/scenarios             → CreateScenario
GET  /api/v1/scenarios/{id}/status → ScenarioProgress
GET  /api/v1/checkpoints           → ListCheckpoints
POST /api/v1/checkpoints           → SaveCheckpoint
```

---

## 5. Dashboard Layout

```
┌──────────────────────────────────────────────────────────────┐
│  VELOS — Ho Chi Minh City Traffic Simulation          [POC] │
├──────────────────────┬───────────────────────────────────────┤
│                      │  KPIs                                 │
│                      │  ┌─────┐ ┌─────┐ ┌─────┐ ┌─────┐    │
│                      │  │280K │ │23.5 │ │ B   │ │8.2ms│    │
│                      │  │Agent│ │km/h │ │ LOS │ │Frame│    │
│                      │  └─────┘ └─────┘ └─────┘ └─────┘    │
│    deck.gl Map       │──────────────────────────────────────│
│    (full viewport)   │  Speed by Road Class        [chart]  │
│                      │  ┌──────────────────────────────┐    │
│    - Vehicle dots    │  │ ████████████ Motorbike 28km/h│    │
│    - Heatmap overlay │  │ ██████████   Car 32km/h      │    │
│    - Signal states   │  │ ████████     Bus 24km/h      │    │
│    - Flow arrows     │  └──────────────────────────────┘    │
│                      │──────────────────────────────────────│
│                      │  Demand Profile              [chart]  │
│                      │  ┌──────────────────────────────┐    │
│                      │  │    /\          /\             │    │
│                      │  │   /  \        /  \            │    │
│                      │  │  /    \------/    \--         │    │
│                      │  │ 6  8  10  12  14  16  18  20 │    │
│                      │  └──────────────────────────────┘    │
├──────────────────────┴───────────────────────────────────────┤
│  [Play] [Pause] [1x] [5x] [20x]  |  Sim: 07:32:15  Step: 27150 │
│  [Save Checkpoint] [Load] [Scenario: Base]  [Compare...]    │
└──────────────────────────────────────────────────────────────┘
```

---

## 6. Export Formats

| Format | Use Case | Implementation |
|--------|----------|----------------|
| Apache Parquet | Big data analytics, Python/R | arrow-rs |
| CSV | Spreadsheets, quick analysis | std::io |
| GeoJSON | GIS tools (QGIS, ArcGIS) | serde_json + geojson crate |
| SUMO XML (FCD) | Ecosystem compatibility | quick-xml |
| Shapefile | Legacy GIS | shapefile crate (optional) |
