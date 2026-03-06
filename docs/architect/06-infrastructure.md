# VELOS v2 Infrastructure & Operations

## Resolves: W8 (No Checkpoint/Restart), W11 (Map Stack Ops Burden)

---

## 1. Deployment Topology (POC)

For HCMC POC, minimize operational complexity. Single-node deployment with Docker Compose, not Kubernetes.

```
┌─────────────────────────────────────────────────────────┐
│  Single Server: 2× RTX 4090, 64GB RAM, 16-core CPU     │
│                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │ velos-sim    │  │ velos-api    │  │ velos-viz    │  │
│  │ (GPU×2)      │  │ (gRPC+WS)   │  │ (deck.gl)   │  │
│  │ Simulation   │  │ REST gateway │  │ Dashboard    │  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  │
│         │                 │                  │          │
│  ┌──────v─────────────────v──────────────────v───────┐  │
│  │                    Redis                          │  │
│  │  Pub/Sub (tile frames) + Job Queue (scenarios)    │  │
│  └───────────────────────┬───────────────────────────┘  │
│                          │                              │
│  ┌───────────────────────v───────────────────────────┐  │
│  │              Local Filesystem                     │  │
│  │  /data/network/    → OSM + cleaned graph          │  │
│  │  /data/demand/     → OD matrices + ToD profiles   │  │
│  │  /data/tiles/      → PMTiles (vector + terrain)   │  │
│  │  /data/checkpoints/→ Parquet snapshots            │  │
│  │  /data/output/     → Simulation results           │  │
│  └───────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────┘
```

**Why not Kubernetes for POC?**
- Single server with 2 GPUs is sufficient for 280K agents
- K8s adds operational complexity without proportional benefit at POC scale
- Docker Compose is simpler to debug, deploy, and reproduce
- K8s migration for production is straightforward when needed

### Docker Compose

```yaml
version: "3.9"

services:
  velos-sim:
    build: ./docker/simulation
    runtime: nvidia
    environment:
      NVIDIA_VISIBLE_DEVICES: all
    volumes:
      - ./data:/data
      - ./checkpoints:/checkpoints
    ports:
      - "50051:50051"  # gRPC
    depends_on:
      - redis

  velos-api:
    build: ./docker/api
    ports:
      - "8080:8080"    # REST
      - "8081:8081"    # WebSocket
    environment:
      VELOS_SIM_ADDR: velos-sim:50051
      REDIS_URL: redis://redis:6379
    depends_on:
      - velos-sim
      - redis

  velos-viz:
    build: ./docker/dashboard
    ports:
      - "3000:3000"    # deck.gl dashboard
    environment:
      API_URL: http://velos-api:8080
      WS_URL: ws://velos-api:8081

  redis:
    image: redis:7-alpine
    ports:
      - "6379:6379"
    volumes:
      - redis-data:/data

  tiles:
    image: nginx:alpine
    ports:
      - "8088:80"
    volumes:
      - ./data/tiles:/usr/share/nginx/html
    # Serves PMTiles + terrain tiles as static files

  prometheus:
    image: prom/prometheus:v2.48.0
    ports:
      - "9090:9090"
    volumes:
      - ./config/prometheus.yml:/etc/prometheus/prometheus.yml

  grafana:
    image: grafana/grafana:10.2.0
    ports:
      - "3001:3000"
    volumes:
      - ./config/grafana/dashboards:/var/lib/grafana/dashboards
      - ./config/grafana/provisioning:/etc/grafana/provisioning

volumes:
  redis-data:
```

---

## 2. ECS Checkpoint / Restart (W8)

### Problem

If a 24-hour simulation crashes at hour 23, all progress is lost. No checkpoint mechanism exists.

### Solution: ECS State Snapshot to Parquet

**What to checkpoint:**
- All ECS component arrays (Position, Kinematics, Route, AgentProfile)
- Simulation clock (sim_time, step_number)
- Signal controller states (current_phase, time_in_phase)
- Prediction overlay (edge travel times, confidence)
- RNG state (for deterministic replay)
- Demand queue (pending agent departures)

**What NOT to checkpoint:**
- GPU buffers (reconstructed from ECS on restore)
- CCH index (rebuilt from network + current weights)
- WebSocket connections (clients reconnect)

### Checkpoint Format: Parquet

```rust
pub struct CheckpointManager {
    pub interval: Duration,         // default: 5 min sim-time
    pub max_checkpoints: usize,     // default: 10 (rolling)
    pub checkpoint_dir: PathBuf,
    pub compression: Compression,   // Zstd level 3
}

impl CheckpointManager {
    pub fn save(&self, world: &World, sim_state: &SimState) -> Result<CheckpointMeta> {
        let timestamp = chrono::Utc::now();
        let name = format!("checkpoint_{:.1}s_{}", sim_state.sim_time, timestamp.format("%H%M%S"));
        let dir = self.checkpoint_dir.join(&name);
        std::fs::create_dir_all(&dir)?;

        // Save each component table as a Parquet file
        save_parquet(&dir.join("positions.parquet"), &world.query::<&Position>())?;
        save_parquet(&dir.join("kinematics.parquet"), &world.query::<&Kinematics>())?;
        save_parquet(&dir.join("routes.parquet"), &world.query::<&Route>())?;
        save_parquet(&dir.join("profiles.parquet"), &world.query::<&AgentProfile>())?;

        // Save simulation state as JSON
        let meta = CheckpointMeta {
            sim_time: sim_state.sim_time,
            step_number: sim_state.step_number,
            agent_count: world.len(),
            rng_state: sim_state.rng.clone(),
            signal_states: sim_state.signal_states.clone(),
            prediction_overlay: sim_state.prediction.snapshot(),
            demand_queue: sim_state.demand.pending(),
        };
        serde_json::to_writer(
            std::fs::File::create(dir.join("meta.json"))?,
            &meta,
        )?;

        // Enforce rolling window
        self.prune_old_checkpoints()?;

        Ok(meta)
    }

    pub fn restore(&self, path: &Path, world: &mut World) -> Result<SimState> {
        let meta: CheckpointMeta = serde_json::from_reader(
            std::fs::File::open(path.join("meta.json"))?
        )?;

        // Load component arrays
        let positions = load_parquet::<Position>(&path.join("positions.parquet"))?;
        let kinematics = load_parquet::<Kinematics>(&path.join("kinematics.parquet"))?;
        let routes = load_parquet::<Route>(&path.join("routes.parquet"))?;
        let profiles = load_parquet::<AgentProfile>(&path.join("profiles.parquet"))?;

        // Rebuild ECS world
        world.clear();
        for i in 0..positions.len() {
            world.spawn((positions[i], kinematics[i], routes[i], profiles[i]));
        }

        // Rebuild GPU buffers from ECS state
        // (handled by simulation engine on next frame)

        Ok(SimState::from_meta(meta))
    }
}
```

**Performance:**

| Operation | 280K agents | Time |
|-----------|-------------|------|
| Save (Zstd L3) | ~15MB compressed | ~200ms |
| Restore + GPU rebuild | ~15MB read + buffer upload | ~500ms |
| Disk usage (10 checkpoints) | ~150MB | N/A |

**Checkpoint triggers:**
1. Periodic (every 5 min sim-time)
2. On demand (gRPC `SaveCheckpoint` call)
3. Before risky operations (scenario start, network mutation)
4. On graceful shutdown (SIGTERM handler)

---

## 3. Simplified Map Tile Stack (W11)

### Problem

The v1 architecture requires Martin, 3DCityDB, Nominatim, terrain server, and Sentinel-2 tiles — 6+ services requiring a dedicated DevOps engineer.

### Solution: PMTiles + Nginx (Zero Additional Services)

**PMTiles** is a single-file format for map tiles. No database, no tile server, no maintenance. Nginx serves it as a static file with HTTP range requests.

```
Data Preparation (one-time, on developer machine):
  1. Download HCMC OSM extract
  2. Run tilemaker → hcmc.mbtiles
  3. Convert: pmtiles convert hcmc.mbtiles hcmc.pmtiles
  4. Copy hcmc.pmtiles to /data/tiles/

Runtime:
  Nginx serves /data/tiles/hcmc.pmtiles
  MapLibre GL JS client reads tiles via pmtiles:// protocol
  Total services: 1 (Nginx, already needed for dashboard)
```

**What we need vs. what we deploy:**

| Data | v1 Approach | v2 Approach (POC) | Services |
|------|------------|-------------------|----------|
| Vector tiles | Martin (PostGIS) | PMTiles (static file) | 0 (Nginx) |
| 3D buildings | 3DCityDB + pg2b3dm | OSM extrusions in deck.gl | 0 |
| Terrain | Terrain server | SRTM GeoTIFF → terrain tiles (static) | 0 (Nginx) |
| Geocoding | Nominatim | Not needed for POC | 0 |
| Satellite imagery | Sentinel-2 server | Not needed for POC | 0 |
| Search | Nominatim | Browser-side with OSM data | 0 |

**Total additional services for map stack: 0** (Nginx already serves the dashboard)

**Preparation Script:**

```bash
#!/bin/bash
# prepare-tiles.sh — run once on dev machine

# 1. Vector tiles
wget https://download.geofabrik.de/asia/vietnam-latest.osm.pbf
osmium extract --bbox=106.60,10.70,106.76,10.86 vietnam-latest.osm.pbf -o hcmc.osm.pbf
tilemaker --input hcmc.osm.pbf --output hcmc.mbtiles --config config/tilemaker.json
pmtiles convert hcmc.mbtiles data/tiles/hcmc.pmtiles

# 2. Terrain (optional for 3D view)
wget https://srtm.csi.cgiar.org/wp-content/uploads/files/srtm_5x5/TIFF/srtm_63_11.zip
unzip srtm_63_11.zip
gdal_translate -of GTiff -co TILED=YES srtm_63_11.tif data/tiles/hcmc_terrain.tif
rio rgbify hcmc_terrain.tif data/tiles/hcmc_terrain_rgb.mbtiles
pmtiles convert data/tiles/hcmc_terrain_rgb.mbtiles data/tiles/hcmc_terrain.pmtiles
```

**Nginx Config:**

```nginx
server {
    listen 80;

    # PMTiles (vector + terrain)
    location /tiles/ {
        alias /data/tiles/;
        add_header Access-Control-Allow-Origin *;
        add_header Cache-Control "public, max-age=86400";
    }

    # Dashboard static files
    location / {
        root /var/www/dashboard;
        try_files $uri /index.html;
    }
}
```

**Client-side PMTiles loading (MapLibre):**

```javascript
import { PMTiles, Protocol } from 'pmtiles';
import maplibregl from 'maplibre-gl';

const protocol = new Protocol();
maplibregl.addProtocol('pmtiles', protocol.tile);

const map = new maplibregl.Map({
    container: 'map',
    style: {
        version: 8,
        sources: {
            'hcmc': {
                type: 'vector',
                url: 'pmtiles:///tiles/hcmc.pmtiles'
            }
        },
        layers: [/* OpenMapTiles style layers */]
    },
    center: [106.68, 10.78],  // HCMC center
    zoom: 13
});
```

---

## 4. Monitoring & Observability

### Prometheus Metrics (velos-sim)

```rust
// Key simulation metrics
pub struct SimMetrics {
    pub frame_time_ms: Histogram,        // with buckets [1, 2, 5, 8, 10, 15, 20, 50]
    pub gpu_dispatch_ms: Histogram,
    pub agent_count: Gauge,
    pub avg_speed_kmh: Gauge,
    pub total_delay_hours: Counter,
    pub gridlock_events: Counter,
    pub reroute_count: Counter,
    pub checkpoint_save_ms: Histogram,
    pub prediction_update_ms: Histogram,
    pub boundary_transfers: Counter,     // multi-GPU boundary crossings
}
```

### Alerting Rules

```yaml
groups:
  - name: velos-sim
    rules:
      - alert: FrameTimeTooHigh
        expr: histogram_quantile(0.99, velos_frame_time_ms) > 15
        for: 1m
        annotations:
          summary: "Simulation frame time p99 > 15ms"

      - alert: GridlockDetected
        expr: increase(velos_gridlock_events_total[5m]) > 5
        annotations:
          summary: "Multiple gridlock events detected"

      - alert: GPUMemoryHigh
        expr: nvidia_gpu_memory_used_bytes / nvidia_gpu_memory_total_bytes > 0.85
        for: 2m
        annotations:
          summary: "GPU VRAM usage > 85%"
```

### Structured Logging

```rust
// Using tracing crate
use tracing::{info, warn, instrument};

#[instrument(skip(world))]
pub fn simulation_step(world: &mut World, step: u64, sim_time: f64) {
    let start = Instant::now();

    // ... simulation logic ...

    info!(
        step = step,
        sim_time = sim_time,
        agent_count = world.len(),
        frame_time_ms = start.elapsed().as_secs_f64() * 1000.0,
        gpu_time_ms = gpu_elapsed,
        reroute_count = reroutes,
        "Step completed"
    );
}
```

---

## 5. Hardware Requirements

### POC Minimum

| Component | Spec | Cost (est.) |
|-----------|------|-------------|
| GPU | 2× RTX 4090 24GB | $3,200 |
| CPU | AMD EPYC 7543 (16 core) or Ryzen 9 7950X | $700 |
| RAM | 64 GB DDR5 | $200 |
| Storage | 1 TB NVMe SSD | $100 |
| **Total** | | **~$4,200** |

### Cloud Alternative

| Provider | Instance | GPUs | Cost/hr |
|----------|----------|------|---------|
| AWS | g5.12xlarge | 4× A10G | $5.67 |
| GCP | a2-highgpu-2g | 2× A100 | $7.35 |
| Lambda Labs | gpu_2x_a100 | 2× A100 | $2.20 |

For POC development: use Lambda Labs ($2.20/hr × 8hr/day × 22 days = ~$387/month).

---

## 6. Data Retention

| Data Type | Hot (local SSD) | Cold (S3/MinIO) | Format |
|-----------|-----------------|-----------------|--------|
| Checkpoints | Latest 10 | 90 days | Parquet + JSON |
| FCD output | 7 days | 1 year | Parquet |
| Edge stats | 30 days | 1 year | Parquet |
| Calibration results | Permanent | N/A | JSON + Parquet |
| Network files | Permanent | N/A | Protobuf + GeoJSON |
| Map tiles | Permanent | N/A | PMTiles |

---

## 7. Security (POC Scope)

- **Network:** All services on private Docker network. Only ports 3000 (dashboard), 8080 (REST), 50051 (gRPC) exposed
- **Authentication:** API key header for gRPC/REST (simple, sufficient for POC)
- **Data privacy:** Simulation uses synthetic agents — no real PII. GPS probe data (if used for calibration) is aggregated into OD matrices at import time, raw probes are never stored
- **GDPR/CCPA:** Not applicable — no personal data in simulation
