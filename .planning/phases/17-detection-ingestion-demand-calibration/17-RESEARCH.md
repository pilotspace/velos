# Phase 17: Detection Ingestion & Demand Calibration - Research

**Researched:** 2026-03-10
**Domain:** gRPC streaming service, detection aggregation, demand calibration overlay
**Confidence:** HIGH

## Summary

Phase 17 introduces a new `velos-api` crate with a tonic gRPC server that accepts bidirectional streaming detection events from external CV services. Cameras are registered at runtime with position/FOV/edge mapping. Detection counts are aggregated per class over configurable time windows. A calibration overlay (ArcSwap, same pattern as PredictionOverlay) adjusts OD spawn rates based on observed-vs-simulated count ratios.

The core technical challenge is integrating a tokio-based gRPC server with the existing winit event loop (which is synchronous/pollster-based). The solution is to spawn a tokio runtime on a background thread and use channels (tokio::sync::mpsc or crossbeam) to bridge between the async gRPC world and the synchronous simulation frame loop.

**Primary recommendation:** Create `velos-api` crate with tonic 0.14 + prost 0.14, protobuf at `proto/velos/v2/detection.proto`, CalibrationOverlay as ArcSwap in velos-demand, and mpsc channels bridging gRPC server to simulation world.

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- Bidirectional streaming: client streams DetectionEvent batches, server streams acknowledgments with per-batch status
- Minimal DetectionEvent message: camera_id, timestamp, vehicle_class (enum: motorbike/car/bus/truck/bicycle/pedestrian), count, optional speed_kmh
- New `velos-api` crate for gRPC server (tonic) -- general-purpose API crate per architecture plan
- Protobuf definitions at proto/velos/v2/ with versioned package path
- tonic-build generates Rust code; Python client uses grpcio-tools for stubs
- gRPC server runs on tokio runtime alongside the winit app
- Cameras registered via gRPC RegisterCamera RPC at runtime (no config file)
- Camera position stored at exact lat/lon (not snapped to network nodes)
- FOV mapped to road network via geometric cone: camera position + heading + angle, rstar spatial queries determine which edges fall within the FOV cone
- Semi-transparent cone polygon rendered on map showing camera FOV coverage, toggleable via egui checkbox
- Cameras lost on restart unless re-registered by CV client
- Configurable time windows, default 5 minutes
- Per-class counts: HashMap<VehicleClass, u32> per window per camera
- Speed estimation: averaged per class per window (mean speed + sample count)
- Rolling retention: last 1 hour (12 windows at 5-min default)
- Older windows dropped automatically
- Multiplicative scaling factor: ratio = observed_count / simulated_count per camera zone
- Applied to OD pairs whose routes pass through camera-covered edges
- Ratio clamped to [0.5, 2.0] to prevent wild demand swings
- Calibration runs every aggregation window (5 minutes)
- Calibrated demand stored as overlay on OdMatrix -- HashMap<(Zone, Zone), f32> scaling factors
- Original OdMatrix unchanged; Spawner reads base * overlay
- Similar pattern to PredictionOverlay (ArcSwap lock-free reads)
- Minimal egui dashboard panel: per-camera observed count, simulated count, current ratio, last update time (toggleable)

### Claude's Discretion
- Exact protobuf message field types and naming conventions
- gRPC server port and configuration
- FOV cone rendering precision (triangle vs arc sector approximation)
- Edge coverage algorithm details for FOV-to-edge intersection
- Calibration smoothing or damping strategy within the [0.5, 2.0] clamp
- Simulated count collection method (counting agents passing camera edges)
- egui panel layout and styling

### Deferred Ideas (OUT OF SCOPE)
- CAL-02 (continuous calibration during running simulation from streaming data) -- Phase 20
- Built-in YOLO inference (CV-01, CV-02, CV-03) -- future milestone
- Detection confidence heatmap overlay (DAN-01) -- future milestone
- Cross-camera vehicle re-identification (DAN-02) -- future milestone
- Camera config persistence (save registered cameras to TOML for restart) -- could be a small follow-up

</user_constraints>

<phase_requirements>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DET-01 | gRPC service accepting vehicle/pedestrian detection events from external CV services | tonic 0.14 bidirectional streaming with DetectionService trait, proto at proto/velos/v2/detection.proto |
| DET-02 | Aggregates detections into per-class counts per camera over configurable time windows | DetectionAggregator struct with HashMap<CameraId, WindowedCounts>, 5-min default windows, rolling 1hr retention |
| DET-03 | Register cameras with position, FOV, and network edge/junction mapping via gRPC | RegisterCamera unary RPC, rstar spatial query for FOV cone-to-edge intersection using existing build_edge_rtree |
| DET-04 | Camera positions and FOV coverage areas overlaid on the map | Triangular cone polygon rendered via instance buffer, toggleable via egui checkbox in existing SidePanel |
| DET-05 | Speed estimation data from external CV services per camera | speed_kmh optional field in DetectionEvent, averaged per class per window in aggregator |
| DET-06 | Python and Rust client libraries for integration testing | Python: grpcio-tools stub generation from proto; Rust: tonic client from same proto, both in tests/ or tools/ |
| CAL-01 | Adjust simulation demand based on observed vs simulated counts | CalibrationOverlay with ArcSwap, multiplicative ratio clamped [0.5, 2.0], applied to OD pair spawn rates in Spawner |

</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tonic | 0.14 | gRPC server and client | De facto Rust gRPC, async/await, bidirectional streaming |
| prost | 0.14 | Protobuf code generation | Companion to tonic, generates Rust types from .proto |
| tonic-build | 0.14 | Build-time proto compilation | Generates server traits and client stubs from .proto |
| tokio | 1.x | Async runtime for gRPC server | Required by tonic; features: rt-multi-thread, macros, sync |
| arc-swap | 1 | Lock-free overlay swap | Already in workspace, proven pattern in PredictionStore |
| rstar | 0.12 | Spatial index for FOV-to-edge queries | Already in workspace, used by snap.rs for edge R-tree |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| grpcio-tools | latest | Python protobuf stub generation | Integration test client only |
| grpcio | latest | Python gRPC runtime | Python test client runtime |
| crossbeam-channel | 0.5 | Sync-async bridge channel | If mpsc proves insufficient for winit<->tokio bridging |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| tonic | tarpc | tarpc is simpler but no protobuf interop, no Python client generation |
| grpcio-tools (Python) | betterproto | betterproto generates cleaner Python but less mature |
| crossbeam-channel | tokio::sync::mpsc | tokio mpsc works if both sides have tokio context; crossbeam is sync-friendly |

**Installation (workspace Cargo.toml additions):**
```toml
[workspace.dependencies]
tonic = "0.14"
prost = "0.14"
tokio = { version = "1", features = ["rt-multi-thread", "macros", "sync"] }

# In velos-api/Cargo.toml [build-dependencies]
tonic-build = "0.14"
```

**Python client setup:**
```bash
uv pip install grpcio grpcio-tools
python -m grpc_tools.protoc --proto_path=proto --python_out=tools/python --pyi_out=tools/python --grpc_python_out=tools/python proto/velos/v2/detection.proto
```

## Architecture Patterns

### Recommended Project Structure
```
crates/velos-api/
  Cargo.toml
  build.rs                    # tonic_build::compile_protos
  src/
    lib.rs                    # pub mod detection, camera, calibration, error
    error.rs                  # ApiError with thiserror
    detection.rs              # DetectionService impl (gRPC trait)
    camera.rs                 # CameraRegistry (camera registration, FOV computation)
    aggregator.rs             # DetectionAggregator (windowed counts)
    calibration.rs            # CalibrationOverlay + CalibrationStore (ArcSwap)
    bridge.rs                 # Channel types bridging gRPC<->SimWorld
  tests/
    grpc_integration.rs       # Rust client integration test

proto/velos/v2/
  detection.proto             # DetectionService definition

tools/python/
  detection_client.py         # Python test client
  velos/v2/                   # Generated Python stubs (gitignored or committed)
```

### Pattern 1: Async-Sync Bridge (gRPC server <-> winit event loop)

**What:** The gRPC server runs on a tokio runtime in a background thread. Detection events flow from the gRPC handler into the simulation frame loop via bounded channels.

**When to use:** Whenever an async service (gRPC, HTTP) needs to communicate with the synchronous winit/pollster-based simulation loop.

**Example:**
```rust
// bridge.rs -- channel types for gRPC <-> SimWorld communication

use tokio::sync::mpsc;

/// Messages from gRPC server to simulation world.
pub enum ApiCommand {
    RegisterCamera(CameraRegistration),
    DetectionBatch(Vec<DetectionEvent>),
}

/// Messages from simulation world to gRPC server.
pub enum ApiResponse {
    CameraRegistered { camera_id: u32 },
    BatchAck { batch_id: u64, status: AckStatus },
}

pub struct ApiBridge {
    /// gRPC handler sends commands here (async sender).
    pub cmd_tx: mpsc::Sender<ApiCommand>,
    /// Simulation loop reads commands here (try_recv in frame loop).
    pub cmd_rx: mpsc::Receiver<ApiCommand>,
    // Response channel for acks (if needed).
}
```

**Integration in main.rs / app.rs:**
```rust
// Start tokio runtime on background thread BEFORE winit event loop
let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel(256);
let bridge = ApiBridge { cmd_tx, cmd_rx };

std::thread::spawn(move || {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let addr = "[::1]:50051".parse().unwrap();
        tonic::transport::Server::builder()
            .add_service(DetectionServiceServer::new(service))
            .serve(addr)
            .await
            .unwrap();
    });
});

// In frame loop: drain commands non-blocking
while let Ok(cmd) = bridge.cmd_rx.try_recv() {
    match cmd { ... }
}
```

### Pattern 2: CalibrationOverlay (ArcSwap, mirroring PredictionStore)

**What:** Lock-free overlay storing per-OD-pair calibration scaling factors. Writers swap atomically; the Spawner reads without blocking.

**When to use:** Calibration factors that update periodically (every aggregation window) and must be read every frame by the spawner.

**Example:**
```rust
// calibration.rs

use std::collections::HashMap;
use std::sync::Arc;
use arc_swap::{ArcSwap, Guard};
use velos_demand::od_matrix::Zone;

/// Immutable snapshot of calibration scaling factors.
#[derive(Debug, Clone)]
pub struct CalibrationOverlay {
    /// Per OD-pair multiplicative scaling factor.
    pub factors: HashMap<(Zone, Zone), f32>,
    /// Simulation time when this overlay was computed.
    pub timestamp_sim_seconds: f64,
}

/// Thread-safe store for calibration overlay (mirrors PredictionStore pattern).
#[derive(Debug)]
pub struct CalibrationStore {
    inner: Arc<ArcSwap<CalibrationOverlay>>,
}

impl CalibrationStore {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ArcSwap::from_pointee(CalibrationOverlay {
                factors: HashMap::new(),
                timestamp_sim_seconds: 0.0,
            })),
        }
    }

    pub fn current(&self) -> Guard<Arc<CalibrationOverlay>> {
        self.inner.load()
    }

    pub fn swap(&self, new: CalibrationOverlay) {
        self.inner.store(Arc::new(new));
    }

    pub fn clone_handle(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}
```

### Pattern 3: FOV Cone Spatial Query

**What:** Camera FOV is a cone (position + heading + half-angle). Edges within the cone are found via rstar bounding-box query + angular filter.

**When to use:** Camera registration to determine which road edges a camera "covers."

**Example:**
```rust
// camera.rs -- FOV-to-edge intersection

use rstar::RTree;
use velos_net::snap::EdgeSegment;

pub struct Camera {
    pub id: u32,
    pub lat: f64,
    pub lon: f64,
    pub heading_deg: f32,     // 0=north, clockwise
    pub fov_deg: f32,         // full angle (e.g., 60 degrees)
    pub range_m: f32,         // max detection distance
    pub covered_edges: Vec<u32>,
}

/// Find edges within a camera's FOV cone.
pub fn edges_in_fov(
    cam_pos: [f64; 2],       // local metres
    heading_rad: f64,
    half_angle_rad: f64,
    range_m: f64,
    tree: &RTree<EdgeSegment>,
) -> Vec<u32> {
    // 1. Bounding box query: circle of radius range_m around cam_pos
    let envelope = rstar::AABB::from_corners(
        [cam_pos[0] - range_m, cam_pos[1] - range_m],
        [cam_pos[0] + range_m, cam_pos[1] + range_m],
    );

    // 2. For each segment in bounding box, check if midpoint is within angular range
    let mut edges = Vec::new();
    for seg in tree.locate_in_envelope(&envelope) {
        let mid = [
            (seg.segment_start[0] + seg.segment_end[0]) / 2.0,
            (seg.segment_start[1] + seg.segment_end[1]) / 2.0,
        ];
        let dx = mid[0] - cam_pos[0];
        let dy = mid[1] - cam_pos[1];
        let dist = (dx * dx + dy * dy).sqrt();
        if dist > range_m { continue; }

        let angle_to_seg = dy.atan2(dx);
        let diff = (angle_to_seg - heading_rad).abs();
        let diff = if diff > std::f64::consts::PI { 2.0 * std::f64::consts::PI - diff } else { diff };
        if diff <= half_angle_rad {
            edges.push(seg.edge_id);
        }
    }
    edges.sort_unstable();
    edges.dedup();
    edges
}
```

### Pattern 4: Protobuf Definition

```protobuf
// proto/velos/v2/detection.proto
syntax = "proto3";
package velos.v2;

service DetectionService {
    // Bidirectional: client streams detection batches, server streams acks
    rpc StreamDetections(stream DetectionBatch) returns (stream DetectionAck);

    // Unary: register a camera
    rpc RegisterCamera(RegisterCameraRequest) returns (RegisterCameraResponse);

    // Unary: list registered cameras
    rpc ListCameras(ListCamerasRequest) returns (ListCamerasResponse);
}

enum VehicleClass {
    VEHICLE_CLASS_UNSPECIFIED = 0;
    MOTORBIKE = 1;
    CAR = 2;
    BUS = 3;
    TRUCK = 4;
    BICYCLE = 5;
    PEDESTRIAN = 6;
}

message DetectionEvent {
    uint32 camera_id = 1;
    int64 timestamp_ms = 2;          // Unix epoch milliseconds
    VehicleClass vehicle_class = 3;
    uint32 count = 4;
    optional float speed_kmh = 5;    // proto3 optional
}

message DetectionBatch {
    uint64 batch_id = 1;
    repeated DetectionEvent events = 2;
}

message DetectionAck {
    uint64 batch_id = 1;
    enum Status {
        OK = 0;
        UNKNOWN_CAMERA = 1;
        INVALID_DATA = 2;
    }
    Status status = 2;
}

message RegisterCameraRequest {
    double lat = 1;
    double lon = 2;
    float heading_deg = 3;
    float fov_deg = 4;
    float range_m = 5;
    string name = 6;
}

message RegisterCameraResponse {
    uint32 camera_id = 1;
    repeated uint32 covered_edge_ids = 2;
}

message ListCamerasRequest {}

message ListCamerasResponse {
    repeated CameraInfo cameras = 1;
}

message CameraInfo {
    uint32 camera_id = 1;
    string name = 2;
    double lat = 3;
    double lon = 4;
    float heading_deg = 5;
    float fov_deg = 6;
    float range_m = 7;
    repeated uint32 covered_edge_ids = 8;
}
```

### Anti-Patterns to Avoid
- **Blocking gRPC handler on simulation data:** Never call synchronous simulation methods from gRPC handlers. Use channels.
- **Shared mutable state between gRPC and simulation:** Use ArcSwap or channels, never Mutex across the async/sync boundary.
- **Recalculating FOV every frame:** FOV-to-edge mapping is computed once at camera registration time, stored in Camera struct.
- **Unbounded channel buffers:** Use bounded channels (256-1024) to apply backpressure if CV service overwhelms the simulation.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| gRPC server | Custom TCP protocol | tonic 0.14 | Bidirectional streaming, protobuf interop, Python client generation |
| Protobuf serialization | Manual binary encoding | prost 0.14 via tonic-build | Type safety, forward/backward compatibility |
| Lock-free shared state | RwLock + manual versioning | ArcSwap (arc-swap 1.x) | Proven pattern in PredictionStore, no reader blocking |
| Spatial queries for FOV | Manual edge iteration | rstar 0.12 R-tree queries | Already built via build_edge_rtree, O(log n) vs O(n) |
| Python client stubs | Hand-written Python client | grpcio-tools protoc generation | Auto-generated from same .proto, type-safe |

**Key insight:** All the infrastructure patterns already exist in the codebase (ArcSwap overlay, rstar spatial queries, egui panels). This phase is about wiring a new gRPC entry point into existing patterns rather than inventing new ones.

## Common Pitfalls

### Pitfall 1: Tokio Runtime Conflict with winit
**What goes wrong:** Calling `tokio::main` or `Runtime::new()` on the main thread conflicts with winit's event loop which must own the main thread.
**Why it happens:** winit requires main-thread ownership on macOS (Cocoa requirement). Tokio runtime must run on a separate thread.
**How to avoid:** Spawn tokio runtime on a background `std::thread` before `event_loop.run_app()`. Pass channel handles to both the gRPC server and the SimWorld.
**Warning signs:** Panic on macOS with "must be called on main thread" or deadlock at startup.

### Pitfall 2: Channel Drainage Starvation
**What goes wrong:** If the simulation frame loop doesn't drain the command channel fast enough, the gRPC handler blocks on a full channel, causing client timeouts.
**Why it happens:** Bounded channels apply backpressure. If the simulation is paused or running slowly, commands accumulate.
**How to avoid:** Use `try_recv()` in a loop with a per-frame budget (e.g., max 64 commands per frame). Log warnings if channel is consistently > 75% full.
**Warning signs:** gRPC clients getting deadline exceeded errors during heavy detection traffic.

### Pitfall 3: Division by Zero in Calibration Ratio
**What goes wrong:** `observed / simulated` when simulated count is 0 produces infinity or NaN.
**Why it happens:** Camera covers edges that have no simulated traffic (e.g., early in simulation, or camera on a low-traffic road).
**How to avoid:** If simulated count is 0, ratio defaults to 1.0 (no adjustment). Only compute ratio when simulated > threshold (e.g., > 5 vehicles).
**Warning signs:** NaN or Inf in calibration overlay factors.

### Pitfall 4: Protobuf Enum Zero Value
**What goes wrong:** Proto3 requires the first enum value to be 0. If you use 0 for a real vehicle class, you can't distinguish "unset" from "motorbike."
**Why it happens:** Proto3 default behavior.
**How to avoid:** Always use `UNSPECIFIED = 0` as the first enum value. Real classes start at 1.
**Warning signs:** Detection events with vehicle_class = 0 being counted as motorbikes.

### Pitfall 5: FOV Cone Edge Cases at Heading Wraparound
**What goes wrong:** Camera heading near 0/360 degrees causes angular comparison to miss edges that cross the wraparound boundary.
**Why it happens:** Naive `abs(angle - heading) < half_fov` fails when heading is near 0 and edge is at 350 degrees.
**How to avoid:** Normalize angle difference to [-pi, pi] range before comparison, or use the two-atan2 approach.
**Warning signs:** Cameras facing north missing edges slightly to the west.

## Code Examples

### Spawner Integration with CalibrationOverlay

```rust
// Modified generate_spawns in spawner.rs
pub fn generate_spawns_calibrated(
    &mut self,
    sim_hour: f64,
    dt: f64,
    calibration: &CalibrationOverlay,
) -> Vec<SpawnRequest> {
    let factor = self.tod.factor_at(sim_hour);
    let time_fraction = dt / 3600.0;
    let pairs: Vec<(Zone, Zone, u32)> = self.od.zone_pairs().collect();
    let mut spawns = Vec::new();

    for (from, to, trips) in pairs {
        // Apply calibration scaling factor (default 1.0 if no calibration data)
        let cal_factor = calibration.factors
            .get(&(from, to))
            .copied()
            .unwrap_or(1.0);
        let expected = trips as f64 * factor * cal_factor as f64 * time_fraction;
        // ... rest of spawn logic unchanged
    }
    spawns
}
```

### build.rs for velos-api

```rust
// crates/velos-api/build.rs
fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("../../proto/velos/v2/detection.proto")?;
    Ok(())
}
```

### Windowed Aggregation

```rust
// aggregator.rs
use std::collections::HashMap;

pub struct TimeWindow {
    pub start_ms: i64,
    pub end_ms: i64,
    pub counts: HashMap<VehicleClass, u32>,
    pub speed_samples: HashMap<VehicleClass, (f32, u32)>, // (sum, count)
}

pub struct DetectionAggregator {
    window_duration_ms: i64,   // default: 300_000 (5 min)
    retention_ms: i64,         // default: 3_600_000 (1 hour)
    cameras: HashMap<u32, Vec<TimeWindow>>,
}

impl DetectionAggregator {
    /// Ingest a detection event, creating windows as needed.
    pub fn ingest(&mut self, camera_id: u32, event: &DetectionEvent) {
        let windows = self.cameras.entry(camera_id).or_default();
        let window_start = (event.timestamp_ms / self.window_duration_ms) * self.window_duration_ms;

        // Find or create window
        let window = match windows.iter_mut().find(|w| w.start_ms == window_start) {
            Some(w) => w,
            None => {
                windows.push(TimeWindow {
                    start_ms: window_start,
                    end_ms: window_start + self.window_duration_ms,
                    counts: HashMap::new(),
                    speed_samples: HashMap::new(),
                });
                windows.last_mut().unwrap()
            }
        };

        *window.counts.entry(event.vehicle_class).or_insert(0) += event.count;

        if let Some(speed) = event.speed_kmh {
            let entry = window.speed_samples.entry(event.vehicle_class).or_insert((0.0, 0));
            entry.0 += speed * event.count as f32;
            entry.1 += event.count;
        }
    }

    /// Remove windows older than retention period.
    pub fn gc(&mut self, now_ms: i64) {
        let cutoff = now_ms - self.retention_ms;
        for windows in self.cameras.values_mut() {
            windows.retain(|w| w.end_ms > cutoff);
        }
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| tonic-build with prost 0.12 | tonic 0.14 + prost 0.14 | 2025 H2 | tonic-build now uses tonic-build-protobuf or direct compile_protos |
| grpcio (C-based) for Python | grpcio-tools (pure protoc plugin) | Stable | Still the standard for Python gRPC clients |
| Manual lock-free patterns | arc-swap crate | Stable | Already adopted in project via PredictionStore |

**Current (not deprecated):**
- tonic 0.14.5 is latest stable (Feb 2026)
- prost 0.14.3 is latest stable (Jan 2026)
- proto3 syntax is current (proto2 still works but proto3 is standard for new projects)

## Open Questions

1. **Simulated count collection method**
   - What we know: Need to count agents passing through camera-covered edges to compute observed/simulated ratio
   - What's unclear: Whether to count via ECS query (scan all agents per frame for edge membership) or via edge-level counters (increment on edge traversal)
   - Recommendation: Edge-level counters are more efficient. Add a `passage_count` field to edge metadata, increment when agent advances to next edge. Reset per aggregation window. This avoids O(agents * cameras) scan.

2. **gRPC server port configuration**
   - What we know: Server needs a configurable listen address
   - What's unclear: Whether to use TOML config, env var, or CLI arg
   - Recommendation: Default to `[::1]:50051` (gRPC convention), configurable via `VELOS_GRPC_ADDR` env var. Simple and doesn't require new config infrastructure.

3. **Calibration damping strategy**
   - What we know: Ratio clamped to [0.5, 2.0] per user decision
   - What's unclear: Whether to apply exponential moving average (EMA) to smooth ratio changes across windows
   - Recommendation: Use EMA with alpha=0.3: `new_ratio = 0.3 * raw_ratio + 0.7 * previous_ratio`. Prevents oscillation while still responding to demand changes. The clamp applies after EMA.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) + Python unittest |
| Config file | Cargo workspace test configuration (existing) |
| Quick run command | `cargo test -p velos-api` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DET-01 | gRPC server accepts streaming detection events | integration | `cargo test -p velos-api --test grpc_integration -- stream_detections` | Wave 0 |
| DET-02 | Aggregates detections per class per camera over time windows | unit | `cargo test -p velos-api -- aggregator` | Wave 0 |
| DET-03 | Register cameras with position/FOV/edge mapping | integration | `cargo test -p velos-api --test grpc_integration -- register_camera` | Wave 0 |
| DET-04 | Camera FOV overlay on map | manual-only | Visual verification: camera cone visible on map | N/A (visual) |
| DET-05 | Speed estimation data per camera | unit | `cargo test -p velos-api -- aggregator::speed` | Wave 0 |
| DET-06 | Python and Rust client libraries connect | integration | `cargo test -p velos-api --test grpc_integration && python tools/python/test_detection_client.py` | Wave 0 |
| CAL-01 | OD spawn rate adjustment from observed/simulated ratio | unit | `cargo test -p velos-api -- calibration` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-api`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/velos-api/tests/grpc_integration.rs` -- covers DET-01, DET-03, DET-06
- [ ] `crates/velos-api/src/aggregator.rs` unit tests -- covers DET-02, DET-05
- [ ] `crates/velos-api/src/calibration.rs` unit tests -- covers CAL-01
- [ ] `tools/python/test_detection_client.py` -- covers DET-06 (Python side)
- [ ] `proto/velos/v2/detection.proto` -- proto definition must exist before any code gen
- [ ] `crates/velos-api/build.rs` -- tonic-build configuration
- [ ] Workspace Cargo.toml additions: tonic, prost, tokio

## Sources

### Primary (HIGH confidence)
- [tonic on lib.rs](https://lib.rs/crates/tonic) - version 0.14.5, features verified
- [prost on lib.rs](https://lib.rs/crates/prost) - version 0.14.3, latest stable
- Existing codebase: `velos-predict/src/overlay.rs` -- ArcSwap PredictionStore pattern (direct code inspection)
- Existing codebase: `velos-net/src/snap.rs` -- rstar R-tree spatial query pattern (direct code inspection)
- Existing codebase: `velos-demand/src/spawner.rs` -- Spawner generate_spawns pattern (direct code inspection)
- Existing codebase: `velos-gpu/src/app.rs` -- winit event loop + egui panel pattern (direct code inspection)

### Secondary (MEDIUM confidence)
- [tonic bidirectional streaming guide](https://oneuptime.com/blog/post/2026-01-25-bidirectional-grpc-streaming-tonic-rust/view) - verified streaming patterns
- [Python gRPC quickstart](https://grpc.io/docs/languages/python/quickstart/) - grpcio-tools stub generation
- [tonic helloworld tutorial](https://github.com/hyperium/tonic/blob/master/examples/helloworld-tutorial.md) - build.rs setup pattern

### Tertiary (LOW confidence)
- gRPC server port convention (50051) -- community convention, not enforced

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - tonic 0.14 + prost 0.14 versions verified on lib.rs, arc-swap already in workspace
- Architecture: HIGH - all patterns mirror existing codebase (PredictionStore, snap.rs R-tree, egui panels)
- Pitfalls: HIGH - tokio/winit conflict is well-documented, protobuf enum zero-value is fundamental proto3 behavior
- Calibration logic: MEDIUM - ratio computation is straightforward but EMA damping factor (0.3) is a recommendation, not verified against traffic calibration literature

**Research date:** 2026-03-10
**Valid until:** 2026-04-10 (stable domain, tonic/prost minor versions unlikely to break)
