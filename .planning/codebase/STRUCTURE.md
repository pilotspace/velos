# Codebase Structure

**Analysis Date:** 2026-03-06

## Project Status

This project is in **pre-development/architecture phase**. No Rust source code or Cargo workspace exists yet. The structure below reflects the **current state** of the repository plus the **planned crate layout** specified in architecture documents.

## Current Directory Layout

```
velos/
├── CLAUDE.md                      # Project instructions and conventions
├── docs/                          # All documentation
│   ├── architect/                 # Authoritative v2 architecture specs (8 docs)
│   │   ├── 00-architecture-overview.md    # Start here: scope, component diagram, tech stack, NFRs
│   │   ├── 01-simulation-engine.md        # Multi-GPU, wave-front dispatch, CFL stability, fixed-point
│   │   ├── 02-agent-models.md             # Motorbike sublane, IDM, pedestrian, meso-micro, bus dwell
│   │   ├── 03-routing-prediction.md       # CCH pathfinding, in-process prediction ensemble
│   │   ├── 04-data-pipeline-hcmc.md       # OSM import, signal timing, demand, calibration data
│   │   ├── 05-visualization-api.md        # deck.gl, WebSocket scaling, gRPC/REST contracts
│   │   ├── 06-infrastructure.md           # Docker Compose, checkpoint, PMTiles, monitoring
│   │   ├── 07-timeline-risks.md           # Roadmap, spikes, go/no-go gates, risk register
│   │   └── slides/                        # Marp presentation decks
│   │       ├── velos-pitch-government.md
│   │       ├── velos-pitch-investors.md
│   │       └── velos-pitch-technical.md
│   ├── digital-twin-solution-research-report.md    # Legacy research (v1 context)
│   ├── rebuild-sumo-architecture-plan.md           # Legacy v1 plan (superseded)
│   ├── velos-3d-visualization-architecture.md      # Legacy v1 viz spec
│   ├── velos-agent-intelligence-and-prediction.md  # Legacy v1 agent spec
│   ├── VELOS-API-Contract-Reference.md             # Legacy v1 API spec
│   ├── velos-architecture-review.md                # Review that produced weakness list W1-W15
│   ├── VELOS-Deployment-Infrastructure-Guide.md    # Legacy v1 infra spec
│   ├── velos-rust-parallel-frameworks-and-reusable-oss.md  # OSS library survey
│   └── velos-self-hosted-open-data-tile-map.md     # Map tile research
├── .claude/                       # Claude Code agent configuration
│   ├── agents/                    # Custom agent definitions
│   ├── commands/gsd/              # GSD workflow commands
│   ├── skills/                    # Custom skill definitions
│   │   ├── bench/                 # Benchmarking skill
│   │   ├── marp-slides/           # Slide generation skill
│   │   ├── quality-gate/          # Quality gate skill
│   │   ├── spike/                 # Technical spike skill
│   │   └── velos-dev/             # Rust development skill
│   │       └── references/
│   │           └── crate-map.md   # Crate dependency map + ECS component ownership
│   ├── get-shit-done/             # GSD framework runtime
│   ├── hooks/                     # Event hooks
│   └── settings.json              # Claude Code settings
├── .planning/                     # GSD planning documents
│   └── codebase/                  # Codebase analysis (this directory)
└── .serena/                       # Serena MCP configuration
```

## Planned Directory Layout (When Development Begins)

```
velos/
├── Cargo.toml                     # Workspace root manifest
├── crates/                        # All Rust crates
│   ├── velos-core/                # ECS world, scheduler, checkpoint, time controller
│   ├── velos-gpu/                 # wgpu device, multi-GPU, buffer pools, shader registry
│   │   └── shaders/               # WGSL compute shaders (IDM, social force, etc.)
│   ├── velos-net/                 # Road graph, OSM import, CCH pathfinding, rstar spatial index
│   ├── velos-vehicle/             # IDM car-following, MOBIL lane-change, motorbike sublane
│   ├── velos-pedestrian/          # Social force, adaptive workgroup spatial hash
│   ├── velos-signal/              # Fixed-time and actuated signal controllers
│   ├── velos-meso/                # Queue model, graduated buffer zone transitions
│   ├── velos-predict/             # BPR + ETS + historical ensemble, ArcSwap overlay
│   ├── velos-demand/              # OD matrices, time-of-day profiles, agent spawning
│   ├── velos-calibrate/           # GEH statistic, Bayesian optimization, validation
│   ├── velos-output/              # FCD, edge stats, emissions, Parquet/CSV/GeoJSON export
│   ├── velos-api/                 # tonic gRPC, axum REST/WebSocket, Redis pub/sub relay
│   └── velos-scene/               # Scenario DSL, batch runner, MOE comparison
├── dashboard/                     # TypeScript/React deck.gl dashboard (pnpm workspace)
├── proto/
│   └── velos/v2/                  # Protobuf service definitions
├── data/
│   ├── hcmc/                      # HCMC-specific data configs
│   ├── network/                   # OSM extracts + cleaned road graphs
│   ├── demand/                    # OD matrices + ToD profiles
│   ├── tiles/                     # PMTiles (vector + terrain)
│   ├── checkpoints/               # Parquet snapshots
│   └── output/                    # Simulation results
├── config/
│   ├── prometheus.yml             # Prometheus scrape config
│   ├── grafana/                   # Grafana dashboards + provisioning
│   └── tilemaker.json             # PMTiles generation config
├── docker/
│   ├── simulation/                # Dockerfile for velos-sim (GPU)
│   ├── api/                       # Dockerfile for velos-api
│   └── dashboard/                 # Dockerfile for velos-viz
├── docker-compose.yml             # Single-node deployment (6 services)
└── docs/                          # Architecture documents (existing)
```

## Directory Purposes

**`docs/architect/`:**
- Purpose: Authoritative v2 architecture specifications
- Contains: 8 numbered markdown documents covering all system aspects
- Key files: `00-architecture-overview.md` (start here), `01-simulation-engine.md` (GPU dispatch + ECS), `02-agent-models.md` (vehicle behavior)
- Status: Complete. These are the design source of truth.

**`docs/` (root level):**
- Purpose: Legacy v1 architecture docs and research reports
- Contains: Superseded specifications from pre-review phase
- Key files: `velos-architecture-review.md` (produced the W1-W15 weakness list that drove v2 redesign)
- Status: Reference only. `docs/architect/` is authoritative.

**`docs/architect/slides/`:**
- Purpose: Marp-format presentation decks for different audiences
- Contains: Government pitch, investor pitch, technical pitch
- Key files: `velos-pitch-technical.md`

**`.claude/skills/velos-dev/references/`:**
- Purpose: Quick-reference maps for development agents
- Contains: Crate dependency DAG, ECS component ownership table, gRPC error codes
- Key files: `crate-map.md`

## Key File Locations

**Architecture Reference (read before any implementation):**
- `docs/architect/00-architecture-overview.md`: Component diagram, tech stack, NFRs, weakness resolution map
- `docs/architect/01-simulation-engine.md`: Multi-GPU partitioning, wave-front dispatch, CFL stability, fixed-point arithmetic, ECS layout, frame pipeline
- `docs/architect/02-agent-models.md`: Motorbike sublane model, IDM calibration ranges, pedestrian adaptive workgroups, meso-micro buffer zone, bus dwell model, MOBIL params
- `docs/architect/03-routing-prediction.md`: CCH implementation, prediction ensemble architecture, reroute scheduling, cost function
- `docs/architect/04-data-pipeline-hcmc.md`: OSM parsing rules (HCMC-specific), signal timing inference, demand generation, calibration workflow
- `docs/architect/05-visualization-api.md`: deck.gl layers, WebSocket tile-based streaming, gRPC protobuf contract, REST endpoints, dashboard layout
- `docs/architect/06-infrastructure.md`: Docker Compose topology, checkpoint manager, PMTiles stack, Prometheus metrics, hardware requirements

**Crate Dependency Reference:**
- `.claude/skills/velos-dev/references/crate-map.md`: Crate ownership, dependency DAG (no cycles), ECS component-to-crate mapping, workspace Cargo.toml pattern

**Project Instructions:**
- `CLAUDE.md`: Build/test commands, key technical decisions (13 items), file conventions, planned crate structure

## Naming Conventions

**Crates:**
- Pattern: `velos-{domain}` (kebab-case, single domain word)
- Examples: `velos-core`, `velos-gpu`, `velos-net`, `velos-vehicle`, `velos-api`

**Architecture Documents:**
- Pattern: `NN-descriptive-name.md` (numbered, kebab-case)
- Examples: `00-architecture-overview.md`, `01-simulation-engine.md`

**Protobuf:**
- Pattern: `proto/velos/v2/*.proto` (versioned package path)
- Service: `VelosSimulation`

**WGSL Shaders:**
- Location: `crates/velos-gpu/shaders/*.wgsl`
- Validated with: `naga --validate`

**Data Files:**
- Network: `data/network/` (OSM extracts, cleaned graphs)
- Demand: `data/demand/` (OD matrices, ToD profiles)
- Tiles: `data/tiles/` (PMTiles files served by Nginx)
- HCMC configs: `data/hcmc/`

**Rust Conventions (from CLAUDE.md):**
- Max 700 lines per file
- Each crate has single responsibility
- Edition 2021, MIT license

## Where to Add New Code

**New Crate (simulation domain):**
- Create: `crates/velos-{name}/`
- Add to: workspace `Cargo.toml` members list
- Follow: dependency DAG in `.claude/skills/velos-dev/references/crate-map.md` (no cycles)
- Structure: `src/lib.rs` + domain modules

**New GPU Shader:**
- Location: `crates/velos-gpu/shaders/{name}.wgsl`
- Validate: `naga --validate crates/velos-gpu/shaders/*.wgsl`
- Register: via shader registry in `velos-gpu`

**New ECS Component:**
- Define struct in owning crate (see crate-map.md component ownership table)
- If GPU-resident: add to `AgentBufferPool` buffer layout in `velos-gpu`
- Document stride in crate-map.md

**New gRPC Endpoint:**
- Define in: `proto/velos/v2/` protobuf files
- Implement in: `crates/velos-api/`
- Add REST wrapper in: axum routes (if non-streaming)

**New Agent Model / Vehicle Type:**
- IDM params: `crates/velos-vehicle/` (add profile function like `hcmc_commuter_motorbike()`)
- Pedestrian behavior: `crates/velos-pedestrian/`
- GPU kernel: `crates/velos-gpu/shaders/`

**New Dashboard Feature:**
- Location: `dashboard/` (TypeScript/React, pnpm workspace)
- deck.gl layer: add to layer registry
- Data source: WebSocket tile subscription or REST API call

**New Data Pipeline / Import:**
- HCMC-specific: `crates/velos-net/` (network import) or `crates/velos-demand/` (demand import)
- Configuration: `data/hcmc/`

**New Scenario / Test Configuration:**
- Scenario DSL: `crates/velos-scene/`
- Calibration: `crates/velos-calibrate/`

**Tests:**
- Location: co-located in each crate (`crates/velos-{name}/src/`)
- Run: `cargo test -p velos-{name}`
- Benchmarks: `cargo bench --bench frame_time`

**Docker / Deployment:**
- Dockerfiles: `docker/{service}/`
- Compose: `docker-compose.yml` (root)
- Monitoring config: `config/prometheus.yml`, `config/grafana/`

## Special Directories

**`docs/architect/`:**
- Purpose: Authoritative architecture specifications (v2)
- Generated: No (hand-written)
- Committed: Yes

**`docs/` (root-level files):**
- Purpose: Legacy v1 specs and research (superseded by `docs/architect/`)
- Generated: No
- Committed: Yes

**`data/checkpoints/`:**
- Purpose: Parquet simulation snapshots (rolling window of 10)
- Generated: Yes (by checkpoint manager)
- Committed: No

**`data/output/`:**
- Purpose: Simulation results (FCD, edge stats, emissions)
- Generated: Yes
- Committed: No

**`data/tiles/`:**
- Purpose: PMTiles files for map visualization (generated once from OSM)
- Generated: Yes (one-time preparation via `prepare-tiles.sh`)
- Committed: No (too large, generated from public data)

**`.planning/`:**
- Purpose: GSD framework planning and codebase analysis documents
- Generated: Yes (by GSD agents)
- Committed: Varies

**`.claude/`:**
- Purpose: Claude Code agent definitions, skills, commands, hooks
- Generated: Partially (GSD framework + custom)
- Committed: Yes

---

*Structure analysis: 2026-03-06*
