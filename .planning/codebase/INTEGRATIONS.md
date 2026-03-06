# External Integrations

**Analysis Date:** 2026-03-06

**Project Status:** Pre-development. All integrations below are planned per architecture documents in `docs/architect/`. No code or live connections exist yet.

## APIs & External Services

**Simulation Control (self-hosted, outgoing):**
- gRPC API (`velos.v2.VelosSimulation`) - Full simulation lifecycle, agent management, streaming
  - Server: tonic (Rust)
  - Port: 50051
  - Contract: `proto/velos/v2/` (protobuf definitions)
  - Auth: API key header (simple, POC-scope)
  - Reference: `docs/architect/05-visualization-api.md` Section 3

- REST API (convenience wrapper over gRPC) - Dashboard/notebook access
  - Server: axum (Rust)
  - Port: 8080
  - Endpoints: `/api/v1/status`, `/api/v1/agents`, `/api/v1/edges/{id}`, `/api/v1/scenarios`, `/api/v1/checkpoints`
  - Reference: `docs/architect/05-visualization-api.md` Section 4

- WebSocket (real-time agent position streaming)
  - Server: axum (Rust), stateless relay pods
  - Port: 8081
  - Protocol: FlatBuffers binary frames (8 bytes/agent, ~32KB/frame per viewport)
  - Spatial tiling: 500m x 500m grid, 256 tiles for POC area
  - Reference: `docs/architect/05-visualization-api.md` Section 2

**External Data Sources (import-time only, not live):**
- OpenStreetMap (Geofabrik) - Road network
  - URL: `https://download.geofabrik.de/asia/vietnam-latest.osm.pbf`
  - Format: PBF
  - License: ODbL
  - Tool: `osmium extract --bbox=106.64,10.75,106.72,10.82`
  - Reference: `docs/architect/04-data-pipeline-hcmc.md` Section 2

- SRTM Terrain Data - Elevation for 3D view (optional)
  - URL: `https://srtm.csi.cgiar.org/wp-content/uploads/files/srtm_5x5/TIFF/srtm_63_11.zip`
  - Format: GeoTIFF (30m resolution)
  - License: Public domain
  - Reference: `docs/architect/06-infrastructure.md` Section 3

- HCMC Department of Transport - Traffic counts
  - Format: CSV/Excel
  - Coverage: ~50 locations, hourly counts, major arterials
  - License: Government open data
  - Reference: `docs/architect/04-data-pipeline-hcmc.md` Section 5

- HCMC Bus Management Center - Bus routes
  - Format: GTFS
  - Coverage: ~130 bus routes
  - License: Public
  - Reference: `docs/architect/04-data-pipeline-hcmc.md` Section 6

**Potential Partnerships (not confirmed):**
- Grab/Be taxi fleets - GPS probe data for OD matrix calibration
  - Format: CSV/Parquet
  - Use: Trip OD aggregation (raw probes never stored, privacy by design)
  - Fallback if unavailable: Gravity model from HCMC statistical yearbook
  - Reference: `docs/architect/04-data-pipeline-hcmc.md` Section 4

- Waze - Incident reports + speed data (if partnership available)
  - Reference: `docs/architect/04-data-pipeline-hcmc.md` Section 5

## Data Storage

**Databases:**
- None. VELOS uses no traditional database for POC.

**In-Memory State:**
- hecs ECS world - All agent state (Position, Kinematics, Route, AgentProfile)
  - VRAM per agent: ~52 bytes (280K agents = ~14.6 MB)
  - Components: Position (12B), Kinematics (8B), LeaderIndex (4B), IDMParams (20B), LaneChangeState (8B)
  - Reference: `docs/architect/01-simulation-engine.md` Section 5

**File Storage (local filesystem):**
- `/data/network/` - Cleaned OSM road graph (protobuf + GeoJSON)
- `/data/demand/` - OD matrices + time-of-day profiles
- `/data/tiles/` - PMTiles vector + terrain tiles (served by Nginx)
- `/data/checkpoints/` - Parquet snapshots (rolling 10, ~15MB each compressed with Zstd L3)
- `/data/output/` - Simulation results in Parquet, CSV, GeoJSON, SUMO FCD XML

**Checkpoint Format:**
- Parquet files per component table: `positions.parquet`, `kinematics.parquet`, `routes.parquet`, `profiles.parquet`
- JSON metadata: `meta.json` (sim_time, step_number, rng_state, signal_states, prediction_overlay)
- Save: ~200ms for 280K agents
- Restore: ~500ms including GPU buffer rebuild
- Reference: `docs/architect/06-infrastructure.md` Section 2

**Caching:**
- Redis 7 (Alpine) - Pub/sub fan-out for WebSocket spatial tile frames
  - Memory: ~60MB (256 tiles x 8KB x 10Hz x 3-frame buffer)
  - Also used for scenario job queue
  - Port: 6379
  - Reference: `docs/architect/05-visualization-api.md` Section 2

## Authentication & Identity

**Auth Provider:**
- Custom (minimal for POC)
  - Implementation: API key in HTTP header for gRPC and REST endpoints
  - No user management, no OAuth, no OIDC
  - All services on private Docker network; only ports 3000, 8080, 50051 exposed
  - Reference: `docs/architect/06-infrastructure.md` Section 7

**Data Privacy:**
- No real PII in simulation (synthetic agents)
- GPS probe data (if used) is aggregated into OD matrices at import time; raw probes never stored
- GDPR/CCPA not applicable

## Monitoring & Observability

**Metrics:**
- Prometheus v2.48.0
  - Port: 9090
  - Scrape targets: velos-sim, velos-api
  - Key metrics: `velos_frame_time_ms` (histogram), `velos_agent_count` (gauge), `velos_gridlock_events` (counter), `velos_avg_speed_kmh` (gauge), `velos_reroute_count` (counter), `velos_checkpoint_save_ms` (histogram), `velos_boundary_transfers` (counter)
  - Config: `config/prometheus.yml`
  - Reference: `docs/architect/06-infrastructure.md` Section 4

**Dashboards:**
- Grafana 10.2.0
  - Port: 3001
  - Provisioning: `config/grafana/dashboards/`, `config/grafana/provisioning/`
  - Weekly performance tracking: bench_frame_10k, bench_frame_100k, bench_frame_280k

**Alerting Rules (Prometheus):**
- `FrameTimeTooHigh`: p99 frame time > 15ms for 1 min
- `GridlockDetected`: >5 gridlock events in 5 min
- `GPUMemoryHigh`: VRAM usage > 85% for 2 min
- Reference: `docs/architect/06-infrastructure.md` Section 4

**Structured Logging:**
- `tracing` crate with `#[instrument]` span annotations
- Fields: step, sim_time, agent_count, frame_time_ms, gpu_time_ms, reroute_count
- Reference: `docs/architect/06-infrastructure.md` Section 4

**GPU Monitoring:**
- `nvidia-smi` / nvidia GPU exporter for Prometheus
- Track VRAM usage (target: < 16GB for 280K agents on RTX 4090)

## CI/CD & Deployment

**Hosting:**
- Single-server Docker Compose deployment (POC)
- Hardware: 2x RTX 4090, 64GB RAM, 16-core CPU, 1TB NVMe
- Cloud alternative: Lambda Labs gpu_2x_a100 ($2.20/hr)

**Docker Services (7 containers):**

| Service | Image | Ports | Purpose |
|---------|-------|-------|---------|
| `velos-sim` | `./docker/simulation` | 50051 (gRPC) | Simulation engine (GPU) |
| `velos-api` | `./docker/api` | 8080 (REST), 8081 (WS) | API gateway |
| `velos-viz` | `./docker/dashboard` | 3000 | deck.gl dashboard |
| `redis` | `redis:7-alpine` | 6379 | Pub/sub + job queue |
| `tiles` | `nginx:alpine` | 8088 | PMTiles static file server |
| `prometheus` | `prom/prometheus:v2.48.0` | 9090 | Metrics collection |
| `grafana` | `grafana/grafana:10.2.0` | 3001 | Metrics dashboards |

Reference: `docs/architect/06-infrastructure.md` Section 1

**CI Pipeline (planned):**
- GPU-enabled CI runner (Lambda Labs cloud, $400/mo)
- Per-PR gates:
  ```bash
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --all --no-fail-fast
  cargo bench --bench frame_time -- --baseline main  # no >10% regression
  naga --validate crates/velos-gpu/shaders/*.wgsl
  ```
- Weekly: full benchmark suite published to Grafana
- Reference: `docs/architect/07-timeline-risks.md` Section 9

## Environment Configuration

**Required env vars (Docker Compose):**
- `NVIDIA_VISIBLE_DEVICES=all` - GPU access for simulation container
- `VELOS_SIM_ADDR=velos-sim:50051` - gRPC endpoint for API service
- `REDIS_URL=redis://redis:6379` - Redis connection for pub/sub
- `API_URL=http://velos-api:8080` - REST endpoint for dashboard
- `WS_URL=ws://velos-api:8081` - WebSocket endpoint for dashboard

**Secrets:**
- API key for gRPC/REST authentication (mechanism TBD, simple header-based for POC)
- No cloud service credentials required for POC (all self-hosted)

## Webhooks & Callbacks

**Incoming:**
- None for POC
- v3 planned: MQTT ingestion from loop detectors and cameras

**Outgoing:**
- None for POC
- WebSocket is push-based (server publishes via Redis pub/sub) but not webhook-based

## Export Integrations

**Output Formats (written to local filesystem):**

| Format | Library | Use Case |
|--------|---------|----------|
| Apache Parquet | `arrow-rs` | Big data analytics, Python/R consumption |
| CSV | `std::io` | Spreadsheet analysis |
| GeoJSON | `serde_json` + `geojson` crate | GIS tools (QGIS, ArcGIS) |
| SUMO FCD XML | `quick-xml` | Ecosystem compatibility with SUMO tools |
| Shapefile | `shapefile` crate (optional) | Legacy GIS workflows |

Reference: `docs/architect/05-visualization-api.md` Section 6

## Data Retention Policy

| Data Type | Hot (local SSD) | Cold (S3/MinIO) | Format |
|-----------|-----------------|-----------------|--------|
| Checkpoints | Latest 10 | 90 days | Parquet + JSON |
| FCD output | 7 days | 1 year | Parquet |
| Edge stats | 30 days | 1 year | Parquet |
| Calibration results | Permanent | N/A | JSON + Parquet |
| Network files | Permanent | N/A | Protobuf + GeoJSON |
| Map tiles | Permanent | N/A | PMTiles |

Reference: `docs/architect/06-infrastructure.md` Section 6

---

*Integration audit: 2026-03-06*
