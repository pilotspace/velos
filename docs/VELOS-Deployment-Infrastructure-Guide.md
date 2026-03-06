# VELOS — Deployment & Infrastructure Guide
## Production Operations for City-Scale Simulation

**Version:** 1.0 | **Date:** March 2026 | **Status:** Architecture Design

---

## 1. Infrastructure Architecture

### 1.1 Deployment Topology

```
┌─────────────────────────────────────────────────────────────┐
│                    KUBERNETES CLUSTER                         │
│                                                              │
│  ┌──────────────────────┐  ┌──────────────────────────────┐ │
│  │  API GATEWAY (CPU)    │  │  SIMULATION WORKERS (GPU)     │ │
│  │  ┌────────────────┐  │  │  ┌────────────────────────┐  │ │
│  │  │ gRPC Server     │  │  │  │ Worker 1 (RTX 4090)    │  │ │
│  │  │ WebSocket Relay │  │  │  │ VELOS Runtime + GPU     │  │ │
│  │  │ REST Proxy      │  │  │  │ 500K agents/worker     │  │ │
│  │  └────────────────┘  │  │  ├────────────────────────┤  │ │
│  │  Replicas: 2-4       │  │  │ Worker 2 (A100)        │  │ │
│  │  CPU: 4 cores        │  │  │ Scenario batch runner   │  │ │
│  │  RAM: 8 GB            │  │  └────────────────────────┘  │ │
│  └──────────────────────┘  │  GPU: 1 per worker            │ │
│                             │  CPU: 16 cores per worker     │ │
│  ┌──────────────────────┐  │  RAM: 32 GB per worker        │ │
│  │  DATA SERVICES        │  └──────────────────────────────┘ │
│  │  ┌────────────────┐  │                                    │
│  │  │ TimescaleDB     │  │  ┌──────────────────────────────┐ │
│  │  │ PostgreSQL      │  │  │  MAP TILE SERVICES             │ │
│  │  │ MinIO (S3)      │  │  │  ┌────────────────────────┐  │ │
│  │  │ MQTT Broker     │  │  │  │ Martin (Vector Tiles)   │  │ │
│  │  └────────────────┘  │  │  │ 3DCityDB (3D Tiles)     │  │ │
│  └──────────────────────┘  │  │ CesiumJS (Static)       │  │ │
│                             │  └────────────────────────┘  │ │
│  ┌──────────────────────┐  └──────────────────────────────┘ │
│  │  OBSERVABILITY        │                                    │
│  │  Prometheus + Grafana │                                    │
│  │  OpenTelemetry        │                                    │
│  │  Structured Logging   │                                    │
│  └──────────────────────┘                                    │
└─────────────────────────────────────────────────────────────┘
```

### 1.2 Hardware Requirements

| Component | Minimum | Recommended | Cloud Equivalent |
|-----------|---------|-------------|------------------|
| Simulation Worker GPU | RTX 3080 (10GB VRAM) | RTX 4090 (24GB VRAM) | AWS g5.xlarge / Azure NC |
| Simulation Worker CPU | 8 cores | 16 cores (AMD EPYC / Intel Xeon) | Included in GPU instance |
| Simulation Worker RAM | 16 GB | 32 GB | Included in GPU instance |
| API Gateway | 4 CPU cores, 8 GB RAM | 8 cores, 16 GB | AWS c6g.xlarge |
| TimescaleDB | 4 cores, 16 GB, 500 GB SSD | 8 cores, 32 GB, 1 TB NVMe | AWS RDS / managed |
| Map Tile Storage | 200 GB SSD | 500 GB NVMe | AWS EBS gp3 |

---

## 2. Infrastructure-as-Code

### 2.1 Terraform/Pulumi Resources

| Resource | Purpose | Configuration |
|----------|---------|---------------|
| Kubernetes Cluster | Orchestration | 3 control plane nodes + GPU node pool |
| GPU Node Pool | Simulation workers | Auto-scaling 1-4 nodes, NVIDIA GPU operator |
| CPU Node Pool | API, data, tiles | Auto-scaling 2-6 nodes |
| Persistent Volumes | Data storage | 500 GB gp3 for TimescaleDB, 200 GB for tiles |
| Load Balancer | gRPC + WebSocket | L4 TCP load balancer with health checks |
| Container Registry | Image storage | ECR/GCR/ACR for VELOS Docker images |

### 2.2 Docker Images

| Image | Base | Contents | Size |
|-------|------|----------|------|
| `velos-simulation` | `nvidia/cuda:12.2-runtime` | VELOS binary + GPU drivers | ~2 GB |
| `velos-api` | `debian:bookworm-slim` | gRPC + WebSocket server | ~100 MB |
| `velos-tiles` | `alpine:3.19` | Martin + 3DCityDB + CesiumJS | ~500 MB |
| `velos-dashboard` | `grafana/grafana:10` | Pre-configured Grafana dashboards | ~300 MB |

---

## 3. Observability Stack

### 3.1 Metrics (Prometheus + Grafana)

| Metric Category | Key Metrics | Alert Threshold |
|-----------------|-------------|-----------------|
| Performance | frame_time_ms, gpu_utilization, cpu_utilization | frame_time > 15ms for 60s |
| Simulation | agent_count, avg_speed, total_delay, gridlock_count | gridlock_count > 10 |
| Prediction | prediction_mape, ensemble_weight_distribution, staleness_ms | mape > 20% for 300s |
| API | grpc_request_latency, websocket_connections, arrow_ipc_throughput | latency p99 > 50ms |
| Infrastructure | pod_memory_usage, gpu_vram_usage, disk_usage | vram > 80%, disk > 85% |
| Calibration | geh_pass_rate, rmse_flow, rmse_speed | geh_pass < 80% |

### 3.2 Logging (Structured)

All VELOS components use Rust's `tracing` crate with structured JSON output:

```json
{
  "timestamp": "2026-03-05T10:30:00Z",
  "level": "INFO",
  "target": "velos_core::scheduler",
  "message": "Step completed",
  "sim_time": 1234.5,
  "agent_count": 502341,
  "frame_time_ms": 9.8,
  "gpu_time_ms": 4.6,
  "reroute_count": 987,
  "gridlock_detected": false
}
```

### 3.3 Distributed Tracing (OpenTelemetry)

Traces span the full request lifecycle: API request → simulation step → output recording → streaming delivery. GPU timing is captured via wgpu timestamps and reported as child spans.

---

## 4. Simulation Job Queue

### 4.1 Architecture

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│ Scenario API  │────→│ Job Queue    │────→│ Workers      │
│ (gRPC)        │     │ (Redis/NATS) │     │ (GPU pods)   │
└──────────────┘     └──────┬───────┘     └──────┬───────┘
                            │                     │
                            ▼                     ▼
                     ┌──────────────┐     ┌──────────────┐
                     │ Dead Letter  │     │ Result Store  │
                     │ Queue        │     │ (MinIO/S3)    │
                     └──────────────┘     └──────────────┘
```

### 4.2 Job Lifecycle

| State | Description | Timeout |
|-------|-------------|---------|
| PENDING | Job submitted, waiting for worker | 5 minutes |
| RUNNING | Worker executing simulation | Scenario duration × 2 |
| COMPLETED | Results written to object storage | N/A |
| FAILED | Error during execution (retried once) | N/A |
| DEAD_LETTER | Failed after retry, manual inspection | 30 days retention |

---

## 5. Data Retention & Archival

| Data Type | Hot Storage | Cold Storage | Retention |
|-----------|-------------|--------------|-----------|
| Raw FCD (all positions) | TimescaleDB (7 days) | Parquet on S3 (90 days) | 90 days |
| Edge statistics | TimescaleDB (30 days) | Parquet on S3 | 1 year |
| Calibration results | PostgreSQL | S3 archive | Permanent |
| Scenario definitions + MOEs | PostgreSQL | S3 archive | Permanent |
| Prediction accuracy logs | TimescaleDB (90 days) | N/A | 90 days |
| Raw sensor data | MQTT (transient) | Parquet on S3 | 1 year |

---

## 6. Security & Compliance

### 6.1 Data Privacy (GDPR/CCPA)

| Concern | Mitigation |
|---------|------------|
| GPS probe data contains personal location | Edge processing: aggregate to edge-level counts before ingestion. No raw GPS stored. |
| CCTV video streams | Count extraction at edge. No raw video transmitted to VELOS. |
| Simulation output may reveal individual patterns | Output is simulated agents, not real individuals. No PII in simulation data. |

### 6.2 Network Security

| Control | Implementation |
|---------|---------------|
| Service-to-service | mTLS via service mesh (Linkerd/Istio) |
| External API access | API keys + TLS. Optional OAuth2 for dashboard access. |
| Data at rest | AES-256 encryption for S3/MinIO objects |
| Data in transit | TLS 1.3 for all connections |

---

## 7. Self-Hosted Map Stack (Docker Compose)

```yaml
# Complete self-hosted map infrastructure
# Zero commercial API dependencies
services:
  martin:           # Vector tile server (OpenStreetMap)
    image: maplibre/martin
    ports: ["3000:3000"]
    volumes: ["./tiles:/data"]

  3dcitydb:         # CityGML → 3D Tiles pipeline
    image: 3dcitydb/3dcitydb
    ports: ["5432:5432"]
    environment:
      POSTGRES_DB: citydb

  pg2b3dm:          # 3D building tile generator
    image: geodan/pg2b3dm
    depends_on: [3dcitydb]

  terrain:          # DEM → quantized mesh terrain
    image: velos/terrain-server
    volumes: ["./dem:/data"]

  nominatim:        # Self-hosted geocoding
    image: mediagis/nominatim
    ports: ["8080:8080"]

  cesiumjs:         # Static CesiumJS viewer (no Ion)
    image: nginx:alpine
    volumes: ["./cesium-app:/usr/share/nginx/html"]
    ports: ["8088:80"]
```

**Total infrastructure cost:** ~$200–$500/month (VPS + storage) vs. $3,000–$10,000/month for commercial API equivalents.

---

## 8. Scaling Strategies

| Scenario | Strategy | Configuration |
|----------|----------|---------------|
| Single city, real-time | 1 GPU worker + 2 API pods | Default deployment |
| Multiple districts | 1 GPU worker per district, load-balanced | Horizontal GPU scaling |
| Batch scenarios | Ephemeral GPU pods, scale to N scenarios | Kubernetes Job + auto-scaling |
| National-scale planning | Meso-only mode (no GPU required) | CPU-only workers with rayon |
| Real-time + batch | Dedicated real-time worker + batch pool | Node affinity labels |

---

## 9. Disaster Recovery

| Component | RPO | RTO | Strategy |
|-----------|-----|-----|----------|
| Simulation state | 0 (stateless) | < 30 seconds | Pod restart, re-load from network + demand files |
| TimescaleDB | 1 hour | < 15 minutes | Continuous archival to S3, point-in-time recovery |
| Scenario results | 0 | < 5 minutes | Stored in S3 (11 9s durability) |
| Map tiles | N/A (regeneratable) | < 1 hour | Rebuild from open data sources |

---

*Document version: 1.0 | VELOS Deployment & Infrastructure Guide | March 2026*
