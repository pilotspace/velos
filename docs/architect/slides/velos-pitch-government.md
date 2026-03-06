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
  h1 { color: #0d4f3c; }
  h2 { color: #1a7a5a; }
  strong { color: #c53030; }
  table { font-size: 0.85em; }
  section.lead h1 { color: #fff; }
  section.lead { background: linear-gradient(135deg, #0d4f3c 0%, #1a7a5a 100%); color: #fff; }
  section.invert { background: #0d4f3c; color: #fff; }
  blockquote { border-left: 4px solid #1a7a5a; padding-left: 1em; font-style: italic; }
---

<!-- _class: lead -->

# VELOS

## Digital Twin Giao Thong TP.HCM
### Ho Chi Minh City Traffic Simulation Platform

**Proof-of-Concept Proposal**

---

# HCMC Traffic: The Challenge

- **10 million+ residents**, one of the fastest-growing cities in Southeast Asia
- **8 million+ motorbikes** — highest density in the world
- Peak hour average speed: **15-18 km/h** (walking speed)
- Congestion costs: estimated **$1.2 billion/year** in lost productivity
- **5,800+ traffic accidents/year** in HCMC

> Infrastructure decisions worth hundreds of millions of dollars
> are made without simulation tools designed for Vietnamese traffic.

---

# The Cost of Guessing

When a new road, flyover, or signal plan is implemented without simulation:

| Outcome | Cost |
|---------|------|
| Road project that doesn't reduce congestion | $10M - $100M wasted |
| Signal timing that creates new bottlenecks | Months of disruption |
| Metro line without feeder bus optimization | Reduced ridership |
| Event road closure without traffic plan | City-wide gridlock |

**Traffic simulation prevents these failures by testing before building.**

---

# What is a Traffic Digital Twin?

A **virtual copy** of HCMC's road network where we can:

1. **Simulate** hundreds of thousands of vehicles, motorbikes, buses, and pedestrians
2. **Test** changes before implementing them in the real world
3. **Compare** scenarios with measurable metrics (speed, delay, emissions)
4. **Predict** congestion patterns hours in advance

Think of it as a **flight simulator for traffic planners** — practice and optimize without risk.

---

# Why HCMC Needs Its Own Solution

Existing tools (SUMO from Germany, VISSIM from PTV) were designed for **European/American traffic**:

| Feature | European Tools | HCMC Reality |
|---------|---------------|--------------|
| Traffic model | Cars in lanes | **Motorbikes everywhere** |
| Lane discipline | Strict | **Minimal** |
| Intersection control | Signals everywhere | **40% unsignalized** |
| Pedestrian behavior | Crosswalks | **Jaywalking common** |
| Mode split | 80% cars | **80% motorbikes** |

**VELOS is the first traffic simulator designed from the ground up for Vietnamese mixed traffic.**

---

# What VELOS Simulates

**280,000 agents** across 5 central districts:

| Agent Type | Count | Behavior |
|-----------|-------|----------|
| Motorbikes | 200,000 | Filtering, swarming, sublane movement |
| Cars | 50,000 | Lane-based, MOBIL lane changes |
| Buses | 10,000 | Fixed routes, stop dwell times, passenger boarding |
| Pedestrians | 20,000 | Jaywalking, crosswalk usage, social forces |

Districts covered: **1, 3, 5, 10, Binh Thanh**

---

# How Motorbikes Actually Move

Traditional simulators force motorbikes into car-sized lanes:
```
  Lane 1:  [Motorbike] ------- [Motorbike]    (unrealistic)
  Lane 2:  [Car] ------------- [Car]
```

VELOS uses **continuous positioning** — motorbikes go where they actually go:
```
      [M] [M] [Car] [M]  [M] [M]
   [M] [M]  [M]    [M] [Car] [M]     (realistic)
    [M] [M] [M] [M]  [M] [M] [M]
```

- Filter through gaps between cars
- Form swarms at red lights
- Ignore lane markings (just like reality)

---

# Scenario 1: Morning Rush Hour

**Simulate:** 06:00 - 09:00, typical weekday

**Outputs:**
- Average speed per corridor (km/h)
- Total vehicle-hours of delay
- Level-of-Service grade (A through F) per road segment
- Congestion heatmap animation

**Value:** Establishes the **baseline** — how bad is traffic today? Where are the worst bottlenecks? This is the foundation for all "what-if" comparisons.

Key corridors: Vo Van Kiet, Cach Mang Thang Tam, Dien Bien Phu, Nguyen Van Linh

---

# Scenario 2: Event Road Closure

**Simulate:** Close Nguyen Hue walking street + surrounding blocks for a public event during morning rush

**Questions answered:**
- Which alternative routes absorb the displaced traffic?
- Which corridors see speed reductions > 20%?
- How far does the congestion ripple extend?
- What signal adjustments could mitigate the impact?

**Value:** Plan event traffic management **before the event**, not during it.

---

# Scenario 3: Signal Retiming

**Simulate:** Optimize signal timing on Vo Van Kiet corridor (6 intersections)
- Increase main-direction green phase by 15 seconds
- Reduce side-street green by 15 seconds

**Questions answered:**
- Does corridor travel time improve? By how much?
- Do side streets experience unacceptable delays?
- Is the net city-wide impact positive or negative?

**Value:** Test signal plans **in simulation** instead of disrupting real traffic for weeks of trial-and-error.

---

# Dashboard: What You See

```
+----------------------------------------------------------+
|  VELOS - Ho Chi Minh City Traffic Simulation              |
+------------------------+---------------------------------+
|                        |  Agents: 280,000  Avg: 23 km/h  |
|    Interactive Map     |  LOS: B           Frame: 8.2ms  |
|                        +---------------------------------+
|  - Live vehicle dots   |  Speed by Type          [chart] |
|  - Congestion heatmap  |  Motorbike: 28 km/h             |
|  - Signal states       |  Car:       32 km/h             |
|  - Flow arrows         |  Bus:       24 km/h             |
|                        +---------------------------------+
|                        |  Demand Profile         [chart] |
|                        |    Peak at 7:00 and 17:00       |
+------------------------+---------------------------------+
|  [Play] [Pause] [1x] [5x] [20x]  |  Sim Time: 07:32    |
+----------------------------------------------------------+
```

Browser-based — accessible from any computer, no software installation needed.

---

# Data We Use from HCMC

| Data Source | Purpose | Availability |
|-------------|---------|-------------|
| OpenStreetMap | Road network, lane counts, one-way streets | Public (good quality for HCMC) |
| HCMC DOT traffic counts | Calibration and validation | **Need partnership** |
| HCMC DOT signal timing | Intersection signal phases | **Need partnership** |
| HCMC Bus GTFS | Bus routes, stops, timetables | Public |
| GPS probe data (Grab/Be) | Origin-destination demand | **Need partnership** |
| SRTM elevation data | Terrain for 3D visualization | Public |

We need **HCMC DOT support** for traffic counts and signal timing data to achieve accurate calibration.

---

# Implementation Timeline

```
Month 1-3:   Build simulation engine, first vehicles on map
             [Week 8: first visible demo]

Month 4-6:   Scale to 280K agents, add prediction, pedestrians, buses
             [Week 20: full-scale demo]

Month 7-9:   Calibrate against HCMC traffic counts
             Validate accuracy (GEH < 5 for 85% of measured links)

Month 10-12: Harden, create 3 demo scenarios, prepare presentation
             [Week 48: stakeholder demonstration]
```

**5 decision checkpoints** ensure we stay on track. If any milestone fails, we have documented fallback plans.

---

# What We Need from HCMC DOT

1. **Traffic count data** — Hourly vehicle counts at 40-50 intersections across the POC area
2. **Signal timing plans** — Current phase durations for major signalized intersections
3. **Incident data** — Historical accident locations (for safety modeling)
4. **Validation access** — Ability to verify simulation results against field observations

This data is used **only for calibration**. No personal or private data is collected or stored.

---

# Expected Outcomes

| Outcome | Metric |
|---------|--------|
| Accurate traffic model | GEH < 5 for 85% of calibration links |
| Real-time simulation | 280K agents at 10 updates/second |
| Actionable scenarios | 3 what-if scenarios with quantified impact |
| Reusable platform | Extensible to full HCMC metro area in Year 2 |
| Emissions baseline | CO2/NOx estimates per road segment |

**Long-term value:** Every future road project, signal change, or event plan can be tested in simulation before spending a single dong on construction.

---

# The Ask

## Authorize the engineering team to begin development

**What we need from leadership:**
- 4 engineers allocated for 12 months
- GPU workstation hardware (one-time purchase)
- Introduction to HCMC Department of Transport for data partnership

**What we deliver:**
- Week 8: First vehicles moving on HCMC digital map
- Week 20: 280K agents running at scale
- Week 48: Calibrated model with 3 demo scenarios ready for stakeholder presentation

---

<!-- _class: lead -->

# Questions?

### VELOS — Built for Ho Chi Minh City Traffic

---
