---
marp: true
theme: default
paginate: true
backgroundColor: #fff
color: #333
style: |
  section {
    font-family: 'Segoe UI', Arial, sans-serif;
  }
  h1 { color: #1a365d; }
  h2 { color: #2c5282; }
  strong { color: #e53e3e; }
  table { font-size: 0.85em; }
  section.lead h1 { color: #fff; }
  section.lead { background: linear-gradient(135deg, #1a365d 0%, #2c5282 100%); color: #fff; }
  section.invert { background: #1a365d; color: #fff; }
  blockquote { border-left: 4px solid #e53e3e; padding-left: 1em; font-style: italic; }
---

<!-- _class: lead -->

# VELOS

## GPU-Accelerated Traffic Digital Twin
### Ho Chi Minh City Proof-of-Concept

**Proposal for Engineering Authorization**

---

# The Problem: HCMC Traffic Crisis

- **10M+ residents**, 8M+ registered motorbikes
- Average commute speed: **15-18 km/h** during peak hours
- Congestion costs the city an estimated **$1.2B/year** in lost productivity
- Infrastructure investment decisions worth **$50M-$500M** are made with spreadsheets and gut instinct

> Every failed road project wastes years and millions.
> Traffic simulation prevents costly mistakes before construction begins.

---

# Why Current Tools Fail for HCMC

| Tool | Problem |
|------|---------|
| **SUMO** (open-source) | Single-threaded, caps at 80K vehicles. No motorbike model. |
| **PTV VISSIM** (commercial) | $100K+/year license. Western lane-based model. Poor motorbike support. |
| **MATSim** | Activity-based, not real-time. No GPU acceleration. |

**The core issue:** All existing tools assume Western lane discipline.
HCMC has **80% motorbikes** that filter, swarm, and ignore lane boundaries.

No tool on the market can simulate HCMC traffic realistically.

---

# VELOS: Purpose-Built for HCMC

**GPU-accelerated, motorbike-native traffic microsimulation**

- **280,000 agents** simulated in real-time (10 steps/second)
- **Motorbike sublane model** — continuous lateral positioning, filtering, swarming
- **GPU-parallel** — 100x faster than SUMO for the same agent count
- **What-if scenarios** — test road closures, signal changes, new infrastructure
- **Calibrated** — validated against real HCMC traffic counts (GEH < 5)

---

# Key Innovation: Motorbike-First Design

Traditional simulators:
```
Lane 1:  [Car] ---- [Car] ---- [Car]
Lane 2:  [Car] ---- [Car] ---- [Car]
```

VELOS for HCMC:
```
         [M] [M][C][M]  [M][M]
     [M][M]  [M]   [M][C] [M]
      [M] [M][M][M]  [M][M][M]
```

- Motorbikes use **continuous lateral position** (not fixed lanes)
- **Filtering** through gaps between cars
- **Swarm formation** at red lights
- This is how HCMC actually works

---

# Scale: What 280K Agents Means

| Metric | SUMO | VISSIM | **VELOS** |
|--------|------|--------|-----------|
| Max real-time agents | 80K | 100K | **280K** |
| Motorbike model | No | Basic | **Native sublane** |
| GPU acceleration | No | No | **Yes (2x RTX 4090)** |
| License cost | Free | $100K+/yr | **Open-source** |
| HCMC calibrated | No | No | **Yes** |

280K agents covers **5 central HCMC districts** during peak hour with realistic mode split (71% motorbike, 18% car, 4% bus, 7% bicycle).

---

# POC Scope

**Area:** Districts 1, 3, 5, 10, Binh Thanh (~50 km road network)

**Duration:** 12-month development, 4 engineers

**Deliverables:**
1. Calibrated simulation engine (GEH < 5 for 85% of links)
2. Real-time 2D dashboard with heatmaps, KPIs, flow visualization
3. Three demo scenarios with measurable outcomes
4. Complete API for scenario submission and result retrieval
5. Docker-based deployment (runs on a single server)

---

# Three Demo Scenarios

### Scenario 1: Morning Rush Hour Baseline
> Simulate 06:00-09:00. Measure average speed, delay, LOS per corridor.

### Scenario 2: Event Road Closure
> Close Nguyen Hue + surrounding blocks. How does traffic redistribute?
> Which corridors see >20% speed drop?

### Scenario 3: Signal Retiming
> Optimize Vo Van Kiet corridor signals (6 intersections).
> Does corridor travel time improve? What happens on side streets?

Each scenario produces **quantified, comparable metrics** — not just animations.

---

# Technology Approach (High-Level)

```
 Data Layer         Simulation Engine        Output Layer
 ──────────         ─────────────────        ────────────
 OSM Road Map       Rust + GPU (wgpu)        deck.gl Dashboard
 Traffic Counts  →  280K Agent ECS       →   gRPC / REST API
 Signal Timing      IDM + Motorbike Model    Parquet / GeoJSON
 Bus Routes         Prediction Ensemble      Scenario Comparison
```

- **Rust:** Memory-safe, no garbage collector, GPU-ready
- **GPU compute:** 2x RTX 4090 for parallel agent simulation
- **deck.gl:** Browser-based, no install needed for viewers
- **Open-source stack:** No vendor lock-in, no license fees

---

# Implementation Strategy: 4 Phases

```
Phase 1 (M1-3)     Phase 2 (M4-6)     Phase 3 (M7-9)     Phase 4 (M10-12)
────────────────    ────────────────    ────────────────    ────────────────
Foundation          Scale              Calibration         Hardening

50K vehicles        280K agents        GEH validation      3 demo scenarios
Single GPU          2 GPUs             Parameter tuning    Performance tuning
Basic dashboard     Full dashboard     Scenario engine     Documentation
Motorbike model     Prediction         Emissions output    Stakeholder demo
```

**5 decision gates** at Weeks 2, 8, 12, 20, 32 — each with explicit Go/No-Go criteria and fallback plans. No surprises.

---

# Risk Management

| Risk | Mitigation |
|------|-----------|
| GPU dispatch architecture doesn't perform | Technical spike in **Week 1** before any committed dev. Proven fallback approach ready. |
| HCMC traffic data insufficient | Data collection begins Month 2. Multiple backup sources (GPS probes, Google Maps, field surveys). |
| Calibration doesn't converge | Bayesian auto-tuning. Scope reduction to District 1 as fallback. External consultant budget reserved. |
| Motorbike model unstable | Standalone prototype tested before integration. Simpler discrete sublane fallback. |

Every critical risk has **early detection** and a **concrete fallback**.

---

# Competitive Landscape

| Competitor | Approach | HCMC Fit |
|-----------|----------|----------|
| SUMO | Open-source, CPU-only | Poor — no motorbike, too slow |
| PTV VISSIM | Commercial, lane-based | Poor — wrong traffic model, expensive |
| Tsinghua MOSS | GPU, research prototype | Medium — no motorbike, no public release |
| CityFlow | Reinforcement learning focus | Poor — simplified physics |
| **VELOS** | **GPU, motorbike-native, HCMC-calibrated** | **Built for this** |

VELOS is the **only** platform designed from scratch for Southeast Asian mixed-traffic conditions.

---

# Team

| Role | Expertise | Commitment |
|------|-----------|-----------|
| **E1: Engine Lead** | Rust, GPU compute, ECS, parallelism | 12 months |
| **E2: Network & Routing** | Graph algorithms, pathfinding, OSM | 12 months |
| **E3: API & Visualization** | TypeScript, deck.gl, gRPC, WebSocket | 12 months |
| **E4: Calibration & Data** | Traffic engineering, statistics, Python | 10 months |

Small, focused team. Each engineer owns a clear vertical.
No dependencies on external vendors or commercial tools.

---

# What Success Looks Like

**At Month 12, we deliver:**

- A **calibrated** traffic model of 5 HCMC districts
- **280K agents** running in real-time with motorbike behavior
- **3 scenario comparisons** with quantified traffic impact
- A **reusable platform** extensible to full HCMC metro (Year 2)

**What this enables:**
- Data-driven infrastructure investment decisions
- Pre-construction impact assessment for road projects
- Signal timing optimization without physical trial-and-error
- Foundation for a real-time HCMC traffic digital twin

---

# Beyond POC: Growth Path

```
Year 1 (POC)              Year 2 (Production)         Year 3 (Platform)
──────────────            ──────────────────          ─────────────────
5 Districts               Full HCMC Metro             Multi-City
280K agents               2M agents                   10M agents
3 scenarios               Live sensor feeds           Real-time digital twin
4 engineers               8 engineers                 SaaS product
```

The POC is not a dead end — it's the **foundation** for a city-scale digital twin platform.

---

# The Ask

## Authorize the engineering team to begin the 12-month POC

**What we need:**
- Engineering team allocation (4 engineers, 12 months)
- Hardware procurement (2x RTX 4090 workstation)
- Data partnership support (HCMC DOT introduction)

**What you get:**
- Week 2: Architecture decision (spike results)
- Week 8: First vehicles moving on HCMC map
- Week 20: 280K agents at scale
- Week 48: Calibrated, demo-ready platform

---

<!-- _class: lead -->

# Questions?

### VELOS — Traffic Simulation Built for HCMC

---
