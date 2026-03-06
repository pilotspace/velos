# VELOS — API Contract Reference
## Complete Interface Specification for External Integrations

**Version:** 1.0 | **Date:** March 2026 | **Status:** Architecture Design

---

## 1. gRPC Service — `VelosSimulation`

### 1.1 Simulation Control

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `Start` | `StartRequest { network_path, demand_path, config }` | `StartResponse { sim_id, agent_count, edge_count }` | Load network and demand, initialize ECS world |
| `Step` | `StepRequest { num_steps: u32 }` | `StepResponse { sim_time, agent_count, step_duration_ms }` | Advance simulation by N steps |
| `Pause` | `Empty` | `Empty` | Pause simulation loop |
| `Reset` | `Empty` | `Empty` | Reset to initial state (preserves network) |

### 1.2 Agent Management

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `AddVehicle` | `AddVehicleRequest { origin_edge, dest_edge, vehicle_params, agent_profile, depart_time }` | `oneof { agent_id \| VelosError }` | Spawn vehicle with route |
| `AddPedestrian` | `AddPedestrianRequest { origin_x, origin_y, dest_x, dest_y, desired_speed }` | `oneof { agent_id \| VelosError }` | Spawn pedestrian |
| `RemoveAgent` | `AgentId { id: u32 }` | `oneof { success \| VelosError }` | Remove agent from simulation |
| `RerouteAgent` | `RerouteRequest { agent_id, new_destination (optional) }` | `oneof { success \| VelosError }` | Force reroute with optional new destination |

### 1.3 Network Mutation (What-If Scenarios)

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `BlockEdge` | `BlockEdgeRequest { edge_id, time_range (optional) }` | `oneof { success \| VelosError }` | Block edge (permanent or time-windowed) |
| `SetSignalPhase` | `SignalPhaseRequest { junction_id, phase_index, duration_override }` | `oneof { success \| VelosError }` | Override signal phase |
| `SetZoneResolution` | `ZoneResolutionRequest { center, radius, resolution: Micro\|Meso }` | `oneof { success \| VelosError }` | Switch meso↔micro for zone |

### 1.4 Scenario Management

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `CreateScenario` | `CreateScenarioRequest { name, base_network, modifications[], demand, duration, seed }` | `ScenarioId` | Define a scenario |
| `RunScenarioBatch` | `BatchRequest { scenario_ids[], parallel: u32 }` | `stream BatchProgress` | Execute scenarios in parallel |
| `CompareScenarios` | `CompareRequest { scenario_ids[], moe_types[] }` | `ComparisonResult { matrix }` | Compare MOEs across scenarios |

### 1.5 Streaming Subscriptions

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `SubscribeAgentPositions` | `SubscribeRequest { bbox, agent_types[], interval_steps }` | `stream AgentFrame` | Real-time agent positions |
| `SubscribeEdgeStats` | `SubscribeRequest { edge_ids[], interval_steps }` | `stream EdgeStatsFrame` | Per-edge flow/speed/density |
| `SubscribeDetectors` | `SubscribeRequest { detector_ids[] }` | `stream DetectorFrame` | Virtual detector measurements |

### 1.6 Query

| RPC | Request | Response | Description |
|-----|---------|----------|-------------|
| `GetAgentState` | `AgentId` | `AgentState { pos, speed, route, profile, cost_breakdown }` | Full agent state |
| `GetEdgeState` | `EdgeId` | `EdgeState { flow, speed, density, queue_length, travel_time }` | Edge aggregated state |
| `GetSimulationStats` | `Empty` | `SimStats { sim_time, agent_count, avg_speed, total_delay, gridlock_count }` | Network-wide KPIs |

---

## 2. Error Types

```protobuf
message VelosError {
    ErrorCode code = 1;
    string message = 2;
    map<string, string> metadata = 3;
}

enum ErrorCode {
    UNKNOWN = 0;
    EDGE_NOT_FOUND = 1;
    JUNCTION_NOT_FOUND = 2;
    AGENT_NOT_FOUND = 3;
    INVALID_ROUTE = 4;
    SIMULATION_NOT_RUNNING = 5;
    NETWORK_NOT_LOADED = 6;
    CAPACITY_EXCEEDED = 7;
    INVALID_SIGNAL_PHASE = 8;
    PREDICTION_MODEL_ERROR = 9;
    SCENARIO_NOT_FOUND = 10;
    GRIDLOCK_DETECTED = 11;
}
```

---

## 3. Streaming Message Formats

### AgentFrame (gRPC streaming)

```protobuf
message AgentFrame {
    double sim_time = 1;
    repeated AgentPosition agents = 2;       // For <50K agents
    bytes agents_binary = 3;                  // FlatBuffers for 500K+ agents
}

message AgentPosition {
    uint32 id = 1;
    float x = 2;             // World coordinate X (meters, projected CRS)
    float y = 3;             // World coordinate Y
    float speed = 4;         // m/s
    float heading = 5;       // radians
    AgentKind kind = 6;      // CAR, TRUCK, BUS, PEDESTRIAN, BICYCLE, etc.
}
```

### WebSocket Protocol (CesiumJS Bridge)

Binary FlatBuffers protocol with spatial tiling:

| Field | Type | Size | Description |
|-------|------|------|-------------|
| tile_x | u8 | 1B | Tile column (0-255) |
| tile_y | u8 | 1B | Tile row (0-255) |
| agent_count | u16 | 2B | Number of agents in this tile |
| agents[] | struct | 4B each | Per-agent: x_offset(u16) + y_offset(u16) relative to tile |

Optimizations applied: viewport culling → LOD → delta compression → FlatBuffers → spatial tiling. Result: ~500KB/frame for typical viewport.

---

## 4. Python Bridge API

```python
import velos
import pyarrow as pa

# Connect to running VELOS simulation
sim = velos.connect("localhost:50051")

# Get agent positions as zero-copy Arrow table
agents: pa.Table = sim.get_agent_positions_arrow()
# Columns: [id, x, y, speed, heading, kind, edge_id]

# Subscribe to streaming updates
for frame in sim.subscribe_agents(bbox=[x1, y1, x2, y2]):
    process_frame(frame)

# Feed ML prediction back
predicted_times = my_model.predict(agents)
sim.publish_prediction_overlay(predicted_times)

# Scenario management
scenario_id = sim.create_scenario("bus_lane", modifications=[
    velos.AddLane(edge_id=42, lane_type="bus"),
    velos.RetimeSignal(junction_id=7, new_plan=optimized_plan),
])
results = sim.run_scenario(scenario_id)
```

---

## 5. Export Formats

| Format | Content | Module | Use Case |
|--------|---------|--------|----------|
| Apache Parquet | FCD, edge stats, emissions | velos-output/parquet.rs | Big data analytics, ML training |
| GeoJSON | Edge statistics with geometry | velos-scenario/export.rs | QGIS/GIS tool import |
| CSV | Tabular detector/edge data | velos-output/edge_data.rs | Spreadsheet analysis |
| SUMO XML | FCD, edge data, detector data | velos-output/sumo_xml.rs | SUMO ecosystem compatibility |
| Arrow IPC | Live agent state stream | velos-api/arrow_ipc.rs | Zero-copy Python/R exchange |
| FlatBuffers | WebSocket agent frames | velos-api/websocket.rs | High-performance viz streaming |

---

## 6. Non-Functional Requirements

| Requirement | Target |
|------------|--------|
| gRPC streaming latency | < 1ms per frame for 10K subscribed agents |
| WebSocket bandwidth | ~500KB/frame at 30 FPS (typical viewport) |
| Python Arrow bridge | Zero-copy: 500K agents as pyarrow.Table in < 1ms |
| Scenario batch throughput | 4 scenarios in parallel on 4 GPU workers |
| API availability | 99.9% uptime with Kubernetes health checks |
| Authentication | mTLS between services (optional API key for external clients) |

---

*Document version: 1.0 | VELOS API Contract Reference | March 2026*
