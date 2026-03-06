# VELOS — Agent Intelligence & Prediction Pipeline
## Cities:Skylines-Inspired Smart Agents + Step-Level Predictive Simulation

**Companion to:** `rebuild-sumo-architecture-plan.md`
**Date:** March 5, 2026
**Status:** Architecture Design (v2.0 — post-architecture-review)

> **GPU Pipeline Note:** This document focuses on agent intelligence and prediction logic.
> For the underlying GPU execution model (semi-synchronous EVEN/ODD dispatch, staging buffer,
> collision correction pass, per-lane leader indexing), see `rebuild-sumo-architecture-plan.md` §3 and §8.
> Agent cost evaluation runs as a GPU screening filter within the EVEN/ODD frame pipeline.

---

## Table of Contents

1. [Design Philosophy — What We Learn from Cities:Skylines](#1-design-philosophy)
2. [Agent Intelligence Architecture](#2-agent-intelligence)
3. [Multi-Factor Pathfinding Cost Function](#3-pathfinding-cost)
4. [Traffic Infrastructure Interaction (V2I)](#4-v2i-interaction)
5. [Hierarchical Prediction Pipeline](#5-prediction-pipeline)
6. [Built-in Prediction Engine (SUMO-Inspired + Ensemble)](#6-builtin-prediction)
7. [Pluggable ML Prediction Interface](#7-ml-interface)
8. [Async Dual-Loop Architecture](#8-async-dual-loop)
9. [ECS Components & New Crates](#9-ecs-components)
10. [GPU Compute Additions](#10-gpu-additions)
11. [API Extensions](#11-api-extensions)
12. [Updated Roadmap Impact](#12-roadmap-impact)
13. [Benchmark Targets](#13-benchmarks)

---

## 1. Design Philosophy — What We Learn from Cities:Skylines {#1-design-philosophy}

Cities:Skylines II introduced a breakthrough in game traffic AI that real simulators haven't fully adopted: **every agent is an autonomous decision-maker** with a multi-dimensional cost function, not just a physics particle following a pre-computed route.

### SUMO vs. Cities:Skylines vs. VELOS

```
SUMO (current state of the art):
┌──────────────────────────────────────────┐
│  Pre-simulation:                          │
│    OD matrix → route assignment (DUA)     │
│    → fixed route per vehicle              │
│                                           │
│  During simulation:                       │
│    Vehicle follows assigned route          │
│    Reroute only if: edge closed OR        │
│    TraCI command OR periodic reroute       │
│                                           │
│  Agent intelligence: NONE                 │
│  Route = pre-determined path              │
│  No interaction with signs/signals        │
│  beyond stop/go                           │
└──────────────────────────────────────────┘

Cities:Skylines II:
┌──────────────────────────────────────────┐
│  Every agent evaluates routes with:       │
│    cost = f(Time, Comfort, Money, Behavior)│
│                                           │
│  Route recalculation triggers:            │
│    • Congestion detected ahead            │
│    • Accident/blockage on path            │
│    • Found cheaper parking nearby         │
│    • Emergency vehicle approaching        │
│    • Signal phase unfavorable             │
│                                           │
│  Agent personality affects weights:        │
│    Teen → prioritize Money                │
│    Adult → prioritize Time                │
│    Senior → prioritize Comfort            │
│                                           │
│  Lane selection is also cost-based:        │
│    Turning cost, U-turn penalty,          │
│    crossing-lane risk, signal phase       │
└──────────────────────────────────────────┘

VELOS (our target):
┌──────────────────────────────────────────┐
│  Cities:Skylines intelligence PLUS:       │
│                                           │
│  ✚ Real-world traffic signal interaction  │
│    (SPaT, MAP, signal phase awareness)    │
│                                           │
│  ✚ V2I communication                     │
│    (signal priority request, green wave,  │
│     cooperative intersection management)  │
│                                           │
│  ✚ Hierarchical prediction at each step   │
│    (global → group → individual)          │
│    Built-in ensemble + pluggable ML       │
│                                           │
│  ✚ Global network knowledge              │
│    (real-time congestion map feeds into   │
│     pathfinding cost, like Waze/Google)   │
│                                           │
│  ✚ Prediction-informed routing            │
│    (route based on PREDICTED conditions   │
│     not just CURRENT conditions)          │
└──────────────────────────────────────────┘
```

---

## 2. Agent Intelligence Architecture {#2-agent-intelligence}

### The Agent Brain — Per-Step Decision Loop

Every agent in VELOS runs a decision loop each simulation step. This loop is split between GPU (fast arithmetic) and CPU (graph logic):

```
┌─────────────────────────────────────────────────────────────────┐
│                    AGENT DECISION LOOP (per step)                │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ PHASE 1: PERCEPTION (GPU — parallel for all agents)      │    │
│  │                                                          │    │
│  │  • Read global congestion map (edge travel times)        │    │
│  │  • Sense leader vehicle (gap, speed, brake lights)       │    │
│  │  • Sense traffic signal state (phase, countdown)         │    │
│  │  • Sense traffic signs (speed limit, stop, yield)        │    │
│  │  • Sense nearby agents (pedestrians, emergency vehicles) │    │
│  │  • Read prediction overlay (if available for this step)  │    │
│  │                                                          │    │
│  │  Output: PerceptionState component (written to GPU buffer)│   │
│  └──────────────────────┬───────────────────────────────────┘    │
│                         ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ PHASE 2: EVALUATION (GPU — cost function computation)    │    │
│  │                                                          │    │
│  │  For current edge and next 3 edges on route:             │    │
│  │                                                          │    │
│  │  cost = w_time    × estimated_travel_time                │    │
│  │       + w_comfort × comfort_penalty                      │    │
│  │       + w_safety  × safety_risk_score                    │    │
│  │       + w_fuel    × fuel_consumption_estimate            │    │
│  │       + w_signal  × signal_delay_estimate                │    │
│  │       + w_predict × prediction_penalty                   │    │
│  │                                                          │    │
│  │  Compare cost(current_route) vs cost(alternative)        │    │
│  │                                                          │    │
│  │  Output: should_reroute flag + cost_delta                │    │
│  └──────────────────────┬───────────────────────────────────┘    │
│                         ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ PHASE 3: DECISION (CPU — only for agents that need it)   │    │
│  │                                                          │    │
│  │  IF should_reroute AND cost_delta > reroute_threshold:   │    │
│  │    → Run A* pathfinding with cost function               │    │
│  │    → Update route component                              │    │
│  │                                                          │    │
│  │  IF approaching signal:                                  │    │
│  │    → Evaluate: stop, slow-approach, or request-priority  │    │
│  │    → V2I: send SPaT request if eligible                  │    │
│  │                                                          │    │
│  │  IF emergency vehicle detected:                          │    │
│  │    → Compute yield maneuver (pull-over lane)             │    │
│  │                                                          │    │
│  │  Output: updated Route, LaneTarget, SpeedTarget          │    │
│  └──────────────────────┬───────────────────────────────────┘    │
│                         ▼                                        │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │ PHASE 4: EXECUTION (GPU — car-following + lane-change)   │    │
│  │                                                          │    │
│  │  Apply IDM/Krauss car-following with speed target        │    │
│  │  Apply MOBIL lane-change toward target lane              │    │
│  │  Apply social-force for pedestrians                      │    │
│  │  Update position                                         │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
```

### Key Design: NOT All Agents Reroute Every Step

Rerouting requires A* pathfinding (CPU-expensive, O(E log V) on graph). We cannot run this for 500K agents every step. Solution: **staggered reroute evaluation**:

```
REROUTE SCHEDULING:

Step 0:  Agents 0-999 evaluate reroute      (batch 0)
Step 1:  Agents 1000-1999 evaluate reroute   (batch 1)
Step 2:  Agents 2000-2999 evaluate reroute   (batch 2)
...
Step 499: Agents 499000-499999               (batch 499)
Step 500: Back to batch 0                    (cycle restarts)

At Δt = 0.1s, each agent evaluates reroute every 50 seconds
→ Responsive enough for real-time driving decisions
→ Only 1000 A* computations per step (feasible on CPU with rayon)

EXCEPTION: Immediate reroute triggers (processed same step):
  • Edge ahead is blocked (accident, closure)
  • Emergency vehicle approaching (yield required)
  • V2I signal advisory received
  • Prediction model flags major congestion ahead
```

---

## 3. Multi-Factor Pathfinding Cost Function {#3-pathfinding-cost}

### Inspired by Cities:Skylines II, Extended for Real-World

```rust
// velos-agent/src/pathfinding/cost.rs

/// Cost function for A* pathfinding — evaluated per edge traversal
pub fn edge_cost(
    edge: &Edge,
    agent: &AgentProfile,
    network_state: &NetworkState,
    signal_state: &SignalState,
    prediction: Option<&PredictionOverlay>,
    time_of_day: f64,
) -> f32 {
    let mut cost = 0.0;

    // === TIME (w_time: typically 0.4 for adults) ===
    // Use predicted travel time if available, else current
    let travel_time = match prediction {
        Some(pred) => pred.predicted_travel_time(edge.id, time_of_day),
        None => network_state.current_travel_time(edge.id),
    };
    cost += agent.w_time * travel_time;

    // === COMFORT (w_comfort: 0.15 for adults, 0.35 for seniors) ===
    let comfort_penalty =
        edge.turn_angle_penalty()          // sharp turns are uncomfortable
        + edge.road_surface_penalty()       // rough roads penalized
        + edge.lane_change_count_penalty()  // many lane changes = uncomfortable
        + edge.elevation_penalty();         // steep hills
    cost += agent.w_comfort * comfort_penalty;

    // === SAFETY (w_safety: 0.15) ===
    let safety_score =
        edge.accident_history_score()       // historical accident rate
        + edge.current_incident_penalty()   // active incidents
        + edge.pedestrian_density_risk()    // high ped zones = slower
        + edge.construction_zone_penalty(); // active work zones
    cost += agent.w_safety * safety_score;

    // === FUEL / ENERGY (w_fuel: 0.1 for adults, 0.3 for teens) ===
    let fuel_cost =
        edge.length / agent.fuel_efficiency // distance-based
        * edge.congestion_fuel_multiplier() // stop-and-go uses more fuel
        + edge.toll_cost();                 // congestion pricing zones
    cost += agent.w_fuel * fuel_cost;

    // === SIGNAL DELAY (w_signal: 0.1) ===
    let signal_delay = match signal_state.get_phase(edge.to_junction) {
        Some(phase) => estimate_signal_wait(phase, agent.arrival_estimate(edge)),
        None => 0.0, // no signal at this junction
    };
    cost += agent.w_signal * signal_delay;

    // === PREDICTION PENALTY (w_predict: 0.1) ===
    // Future congestion predicted by ensemble/ML model
    let prediction_penalty = match prediction {
        Some(pred) => pred.congestion_risk(edge.id, time_of_day + travel_time),
        None => 0.0,
    };
    cost += agent.w_predict * prediction_penalty;

    cost
}
```

### Agent Profiles (Cities:Skylines-Style Personality)

```rust
/// Agent personality — determines how the cost function is weighted
pub struct AgentProfile {
    pub agent_class: AgentClass,

    // Cost function weights (must sum to 1.0)
    pub w_time: f32,        // how much they value speed
    pub w_comfort: f32,     // how much they value smooth rides
    pub w_safety: f32,      // how much they avoid risky areas
    pub w_fuel: f32,        // how much they care about cost
    pub w_signal: f32,      // how much signal waits bother them
    pub w_predict: f32,     // how much they trust/use predictions

    // Behavioral parameters
    pub reroute_threshold: f32,     // cost delta to trigger reroute
    pub risk_tolerance: f32,        // willingness to make U-turns, etc.
    pub speed_compliance: f32,      // 1.0 = exact limit, 1.1 = 10% over
    pub lane_discipline: f32,       // tendency to stay in lane
}

pub enum AgentClass {
    Commuter,        // w_time=0.45, w_fuel=0.15 — optimize time
    DeliveryVan,     // w_time=0.40, w_fuel=0.20 — time + cost
    Taxi,            // w_time=0.50, w_comfort=0.20 — passenger comfort
    Bus,             // w_comfort=0.30, w_signal=0.20 — signal priority
    Truck,           // w_fuel=0.35, w_comfort=0.10 — fuel efficiency
    Emergency,       // w_time=0.80, w_safety=0.05 — maximum speed
    Tourist,         // w_comfort=0.40, w_time=0.10 — scenic routes
    Teen,            // w_fuel=0.40, w_time=0.20 — minimize cost
    Senior,          // w_comfort=0.40, w_safety=0.25 — safe + comfortable
    Cyclist,         // w_safety=0.35, w_comfort=0.25 — avoid traffic
    Pedestrian,      // w_comfort=0.35, w_time=0.30 — walk comfort
    Autonomous,      // w_safety=0.30, w_time=0.30 — balanced, precise
    Custom(Box<CustomProfile>), // user-defined weights
}
```

---

## 4. Traffic Infrastructure Interaction (V2I) {#4-v2i-interaction}

### Signal Phase Awareness (SPaT — Signal Phase and Timing)

```
EVERY TRAFFIC SIGNAL IN VELOS BROADCASTS:

SPaT Message (updated each step):
┌─────────────────────────────────────┐
│  junction_id: J_42                   │
│  current_phase: 2 (NS_GREEN)        │
│  time_in_phase: 18.5s               │
│  time_to_next: 11.5s                │
│  phase_sequence: [1,2,3,4,1,2,...]  │
│  phase_durations: [30,30,25,15]     │
│  pedestrian_phase: WALK (8s remain) │
│  priority_queue: [Bus_42 at 200m]   │
└─────────────────────────────────────┘

AGENTS USE SPaT TO:

1. APPROACH SPEED ADVISORY (Green Wave):
   ┌────────────────────────────────────────┐
   │ If signal is green and I'm 200m away:   │
   │   time_to_reach = 200m / my_speed       │
   │   if time_to_reach < green_remaining:   │
   │     → maintain speed (catch the green)  │
   │   else:                                 │
   │     → slow down (avoid braking at red)  │
   │     → save fuel, smoother ride          │
   └────────────────────────────────────────┘

2. SIGNAL PRIORITY REQUEST (V2I):
   ┌────────────────────────────────────────┐
   │ Eligible agents: Bus, Emergency, Tram   │
   │                                         │
   │ Agent sends: PriorityRequest {          │
   │   agent_id, agent_type, distance,       │
   │   desired_phase, urgency_level          │
   │ }                                       │
   │                                         │
   │ Signal controller evaluates:            │
   │   if emergency → immediate green extend │
   │   if bus → extend green if within 5s    │
   │   if tram → schedule phase insert       │
   └────────────────────────────────────────┘

3. COOPERATIVE INTERSECTION MANAGEMENT:
   ┌────────────────────────────────────────┐
   │ For autonomous/connected vehicles:      │
   │                                         │
   │ Infrastructure broadcasts "slot":       │
   │   "Vehicle_123: cross at T+4.2s        │
   │    at speed 8.3 m/s, lane 2"           │
   │                                         │
   │ Agent adjusts speed to hit the slot     │
   │ → No stopping, smooth flow             │
   │ → Works like airport runway scheduling  │
   └────────────────────────────────────────┘
```

### Traffic Sign Interaction

```
SIGN TYPES AND AGENT RESPONSE:

┌──────────────┬────────────────────────────────────────┐
│ Sign Type     │ Agent Behavior                         │
├──────────────┼────────────────────────────────────────┤
│ Speed Limit   │ Adjust v_desired = limit × compliance │
│               │ (compliance varies by AgentProfile)   │
│               │ Emergency: may exceed with penalty    │
├──────────────┼────────────────────────────────────────┤
│ Stop Sign     │ Decelerate to 0, wait gap_acceptance  │
│               │ time, then proceed if safe            │
├──────────────┼────────────────────────────────────────┤
│ Yield Sign    │ Reduce speed, check cross-traffic     │
│               │ Stop only if conflict detected        │
├──────────────┼────────────────────────────────────────┤
│ No Left Turn  │ Pathfinding cost = ∞ for that turn    │
│               │ Agent cannot select routes using it   │
├──────────────┼────────────────────────────────────────┤
│ One Way       │ Edge directionality enforced in graph  │
│               │ Pathfinding respects direction         │
├──────────────┼────────────────────────────────────────┤
│ School Zone   │ Reduce v_desired, increase safety_w   │
│               │ Higher pedestrian awareness            │
├──────────────┼────────────────────────────────────────┤
│ Variable Speed│ Dynamic speed limit from V2I feed     │
│ (smart sign)  │ Updated based on weather/congestion   │
├──────────────┼────────────────────────────────────────┤
│ Congestion    │ Feeds into prediction_penalty in       │
│ Pricing Zone  │ cost function → some agents reroute   │
└──────────────┴────────────────────────────────────────┘
```

---

## 5. Hierarchical Prediction Pipeline {#5-prediction-pipeline}

### Three-Tier Prediction Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                HIERARCHICAL PREDICTION SYSTEM                    │
│                                                                  │
│  TIER 1: GLOBAL PREDICTION (entire network)                      │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Runs every: 60 simulation-seconds (configurable)          │  │
│  │  Input: current edge travel times, volumes, signal states  │  │
│  │  Output: PredictionOverlay for ALL edges                   │  │
│  │                                                            │  │
│  │  Built-in model: Ensemble (physics BPR + ARIMA)            │  │
│  │  Pluggable: any model implementing PredictionProvider trait │  │
│  │                                                            │  │
│  │  ┌─────────────────────────────────────────┐              │  │
│  │  │ PREDICTION OVERLAY (network-wide)        │              │  │
│  │  │                                          │              │  │
│  │  │ edge_id → {                              │              │  │
│  │  │   predicted_travel_time: [t+1m, t+5m,    │              │  │
│  │  │                          t+15m, t+30m],  │              │  │
│  │  │   predicted_density: [d+1m, ...],        │              │  │
│  │  │   predicted_speed: [s+1m, ...],          │              │  │
│  │  │   congestion_risk: 0.0..1.0,             │              │  │
│  │  │   confidence: 0.0..1.0                   │              │  │
│  │  │ }                                        │              │  │
│  │  └─────────────────────────────────────────┘              │  │
│  └───────────────────────────────────────────────────────────┘  │
│                         │                                        │
│                         ▼                                        │
│  TIER 2: GROUP PREDICTION (per zone / agent class)               │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Runs every: 30 simulation-seconds                         │  │
│  │  Input: Tier 1 overlay + zone-specific sensor data         │  │
│  │  Output: Refined prediction per zone or agent class        │  │
│  │                                                            │  │
│  │  Zones defined by: geographic area, corridor, OD pair      │  │
│  │  Agent classes: Commuter, Bus, Truck, Pedestrian, etc.     │  │
│  │                                                            │  │
│  │  Example: "Zone A (CBD) buses → predicted 20% slower       │  │
│  │           due to event at stadium in 15 minutes"           │  │
│  │                                                            │  │
│  │  Built-in: zone-level BPR with historical calibration      │  │
│  │  Pluggable: GNN per zone, transformer per agent class      │  │
│  └───────────────────────────────────────────────────────────┘  │
│                         │                                        │
│                         ▼                                        │
│  TIER 3: INDIVIDUAL AGENT PREDICTION (special agents only)       │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Runs: on-demand, triggered by API or agent flag           │  │
│  │  Input: Tier 1 + Tier 2 + agent-specific context           │  │
│  │  Output: Per-agent route recommendation or behavior override│  │
│  │                                                            │  │
│  │  Use cases:                                                │  │
│  │    • Emergency vehicle: optimal route to destination       │  │
│  │    • VIP convoy: predicted clear path                      │  │
│  │    • Delivery fleet: predicted optimal delivery sequence   │  │
│  │    • Bus: predicted schedule adherence                     │  │
│  │                                                            │  │
│  │  Built-in: A* with predicted edge costs from Tier 1+2      │  │
│  │  Pluggable: per-agent RL model, fleet optimization solver  │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## 6. Built-in Prediction Engine (SUMO-Inspired + Ensemble) {#6-builtin-prediction}

### The Default Engine: Physics + Statistical Ensemble

This runs out-of-the-box with **zero ML training required** — using only current simulation state and BPR delay functions from transport engineering, enhanced with a statistical layer:

```
BUILT-IN ENSEMBLE PREDICTION:

┌─────────────────────────────────────────────────────────────┐
│                                                              │
│  MODEL A: Physics-Based Extrapolation (BPR Function)         │
│  ────────────────────────────────────────────────────         │
│                                                              │
│  For each edge e:                                            │
│                                                              │
│  travel_time(e) = free_flow_time(e) × [1 + α × (V/C)^β]    │
│                                                              │
│  Where:                                                      │
│    free_flow_time = edge.length / edge.speed_limit           │
│    V = current_volume (vehicles/hour on edge)                │
│    C = capacity (max throughput, from lane count + signal)   │
│    α = 0.15 (BPR standard)                                  │
│    β = 4.0  (BPR standard)                                  │
│                                                              │
│  PREDICTION: extrapolate V forward using:                    │
│    V(t+Δ) = V(t) + inflow_rate(t) - outflow_rate(t)         │
│    inflow = count vehicles approaching edge                  │
│    outflow = saturation_flow × green_ratio                   │
│                                                              │
│  → Predicts travel time 1min, 5min, 15min, 30min ahead      │
│  → Zero training data needed, purely physics-based           │
│  → Accuracy: good for short-term (1-5 min), degrades beyond │
│                                                              │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  MODEL B: Statistical Correction (Exponential Smoothing)     │
│  ────────────────────────────────────────────────────         │
│                                                              │
│  Maintains rolling history of prediction errors:             │
│                                                              │
│  error(t) = actual_travel_time(t) - predicted_travel_time(t) │
│                                                              │
│  Smoothed error:                                             │
│  S(t) = α × error(t) + (1-α) × S(t-1)    (α = 0.3)        │
│                                                              │
│  Corrected prediction:                                       │
│  pred_corrected(t+Δ) = pred_physics(t+Δ) + S(t)             │
│                                                              │
│  → Learns from simulation's own prediction errors            │
│  → Self-calibrating: adapts to network characteristics       │
│  → No external training data needed                          │
│                                                              │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  MODEL C: Historical Pattern Matching (optional, needs data) │
│  ────────────────────────────────────────────────────         │
│                                                              │
│  If historical time-series data is loaded:                   │
│                                                              │
│  pred_historical(e, t+Δ) =                                   │
│    weighted_avg of historical travel_time(e) at              │
│    same time_of_day, same day_of_week,                       │
│    weighted by recency                                       │
│                                                              │
│  → "Last 4 Tuesdays at 8:30am, this edge took 45-52s"       │
│  → Captures recurring patterns (morning rush, school hours)  │
│  → Degrades gracefully if no historical data                 │
│                                                              │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ENSEMBLE COMBINATION:                                       │
│  ────────────────────                                        │
│                                                              │
│  final_prediction =                                          │
│    w_A × pred_physics +                                      │
│    w_B × pred_corrected +                                    │
│    w_C × pred_historical                                     │
│                                                              │
│  Default weights: w_A=0.4, w_B=0.35, w_C=0.25               │
│  (Auto-tuned: weights shift toward model with lowest error)  │
│                                                              │
│  Confidence score:                                           │
│  confidence = 1.0 - (std_dev_across_models / mean_prediction)│
│  → Low agreement between models → low confidence             │
│  → Agent uses confidence to weight w_predict in cost function│
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### SUMO-Inspired Elements Incorporated

```
FROM SUMO — WHAT WE KEEP:

1. Edge-based aggregation
   SUMO's fundamental unit is the edge. We maintain this.
   Predictions are per-edge, not per-lane (keeps it manageable).

2. Calibrator concept
   SUMO's calibrators inject/remove vehicles to match targets.
   VELOS extends this: calibrators inject/remove based on
   PREDICTED flow, not just measured flow.

3. Route sampler LP
   SUMO's routeSampler solves an LP to distribute counts onto routes.
   VELOS uses the same LP but with PREDICTED counts as targets:

   current counts (sensors) → predict future counts (ensemble)
   → routeSampler LP → pre-generate routes for predicted demand
   → agents spawn with these routes when demand materializes

4. Detector abstraction
   SUMO's virtual detectors count vehicles at points.
   VELOS detectors also FEED the prediction engine:

   detector_value → prediction_engine.update(edge, count, timestamp)
```

---

## 7. Pluggable ML Prediction Interface {#7-ml-interface}

### The PredictionProvider Trait

```rust
// velos-predict/src/provider.rs

/// Trait that any prediction model must implement.
/// Built-in ensemble implements this. External ML models implement this.
pub trait PredictionProvider: Send + Sync {
    /// Predict future state for all edges in the network
    /// Called asynchronously by the prediction scheduler
    fn predict_global(
        &mut self,
        current_state: &NetworkSnapshot,
        horizons: &[Duration],         // [1min, 5min, 15min, 30min]
    ) -> GlobalPrediction;

    /// Predict future state for a specific zone/group
    fn predict_group(
        &mut self,
        group: &AgentGroup,
        current_state: &NetworkSnapshot,
        global_prediction: &GlobalPrediction,
    ) -> GroupPrediction;

    /// Predict for a specific individual agent
    fn predict_individual(
        &mut self,
        agent_id: EntityId,
        agent_state: &AgentState,
        global_prediction: &GlobalPrediction,
        group_prediction: Option<&GroupPrediction>,
    ) -> IndividualPrediction;

    /// Model metadata
    fn name(&self) -> &str;
    fn confidence_calibration(&self) -> f32;  // 0-1: how well-calibrated
}

/// What a global prediction contains
pub struct GlobalPrediction {
    pub timestamp: f64,                        // sim time when prediction was made
    pub edge_predictions: HashMap<EdgeId, EdgePrediction>,
}

pub struct EdgePrediction {
    pub predicted_travel_times: Vec<f32>,       // one per horizon
    pub predicted_densities: Vec<f32>,
    pub predicted_speeds: Vec<f32>,
    pub congestion_risk: f32,                   // 0.0 = free flow, 1.0 = gridlock
    pub confidence: f32,                        // model's self-assessed confidence
}

/// What an individual prediction contains
pub struct IndividualPrediction {
    pub recommended_route: Option<Vec<EdgeId>>,  // override agent's current route
    pub speed_advisory: Option<f32>,             // recommended speed
    pub departure_advisory: Option<f64>,         // "leave at time X for best trip"
    pub custom_data: HashMap<String, f64>,       // extensible key-value
}
```

### Python ML Bridge (Arrow IPC)

```python
# External ML model implementing PredictionProvider via Arrow IPC

import pyarrow as pa
import pyarrow.ipc as ipc
from velos import PredictionBridge

class MyLSTMPredictor:
    """Custom LSTM model that predicts per-edge travel times."""

    def __init__(self, model_path: str):
        self.model = torch.load(model_path)
        self.bridge = PredictionBridge("localhost:50052")

    def run(self):
        """Main prediction loop — called by VELOS async scheduler."""
        for state in self.bridge.subscribe_network_state():
            # state is an Arrow RecordBatch (zero-copy from Rust)
            # Columns: [edge_id, travel_time, volume, density, speed,
            #           signal_phase, time_of_day, day_of_week]

            features = self.preprocess(state)

            # Predict for all edges simultaneously
            predictions = self.model.predict(features)
            # Shape: [num_edges, num_horizons]

            # Send back as Arrow RecordBatch
            result = pa.record_batch({
                'edge_id': state.column('edge_id'),
                'pred_travel_time_1m': predictions[:, 0],
                'pred_travel_time_5m': predictions[:, 1],
                'pred_travel_time_15m': predictions[:, 2],
                'pred_travel_time_30m': predictions[:, 3],
                'congestion_risk': self.compute_risk(predictions),
                'confidence': self.model.confidence_scores,
            })

            self.bridge.publish_global_prediction(result)


class MyGNNGroupPredictor:
    """GNN model that predicts per-zone traffic patterns."""

    def predict_group(self, zone_id, state, global_pred):
        # Use graph neural network on zone subgraph
        zone_graph = self.extract_subgraph(zone_id, state)
        zone_pred = self.gnn_model(zone_graph, global_pred)
        return zone_pred


class MyRLAgentPredictor:
    """RL model for individual agent route optimization."""

    def predict_individual(self, agent_id, agent_state, global_pred):
        # Reinforcement learning policy for this specific agent
        observation = self.build_observation(agent_state, global_pred)
        action = self.policy.act(observation)
        return IndividualPrediction(
            recommended_route=action.route,
            speed_advisory=action.speed,
        )
```

### Model Registry — Hot-Swap at Runtime

```
┌─────────────────────────────────────────────────────────┐
│                  MODEL REGISTRY                          │
│                                                          │
│  Global tier:                                            │
│    [active] ensemble_v1 (built-in) — always running      │
│    [standby] lstm_v2 (Python) — warming up               │
│    [disabled] gnn_v1 (Python) — retired                  │
│                                                          │
│  Group tier:                                             │
│    [active] zone_bpr (built-in) — for all zones          │
│    [active] bus_schedule_model (Python) — bus class only  │
│                                                          │
│  Individual tier:                                        │
│    [active] emergency_router (built-in) — emergency only │
│    [active] fleet_optimizer (Python) — delivery fleet     │
│                                                          │
│  API Commands:                                           │
│    velos.prediction.register("my_model", endpoint)       │
│    velos.prediction.activate("my_model", tier="global")  │
│    velos.prediction.deactivate("lstm_v2")                │
│    velos.prediction.compare(["ensemble_v1", "lstm_v2"])  │
│      → runs both, reports accuracy metrics               │
│                                                          │
│  A/B Testing:                                            │
│    velos.prediction.ab_test(                             │
│        model_a="ensemble_v1",                            │
│        model_b="lstm_v2",                                │
│        split=0.5,  # 50% of agents use each              │
│        metric="travel_time_error",                       │
│        duration=3600  # 1 hour                           │
│    )                                                     │
└─────────────────────────────────────────────────────────┘
```

---

## 8. Async Dual-Loop Architecture {#8-async-dual-loop}

### Fast Loop (Simulation) + Slow Loop (Prediction)

```
TIME →  0ms   10ms   20ms   30ms   40ms   50ms   ...  1000ms

FAST LOOP (simulation — every 10ms at Δt=0.1s, 100Hz):
├──┤├──┤├──┤├──┤├──┤├──┤├──┤├──┤├──┤├──┤ ... ├──┤├──┤├──┤
 S0  S1  S2  S3  S4  S5  S6  S7  S8  S9      S97 S98 S99

Each step Sn:
  1. Read latest PredictionOverlay (lock-free atomic pointer swap)
  2. GPU: car-following + pedestrian + cost evaluation
  3. CPU: reroute batch (1000 agents) using current overlay
  4. Output: positions, detector values

SLOW LOOP (prediction — every 600 steps = 60 sim-seconds):
├─────────────────────────────────────────────────────────────┤
 P0                                                          P1

Each prediction Pn (runs in separate thread / process):
  1. Snapshot current NetworkState (edge volumes, speeds, densities)
  2. Run ensemble prediction (BPR + ETS + historical)
     OR run external ML model via Arrow IPC
  3. Produce new PredictionOverlay
  4. Atomic swap: fast loop sees new overlay on next step

  Timeline: prediction P0 starts at step S0
            prediction P0 finishes at step ~S20 (200ms compute)
            steps S0-S19 use previous overlay (slightly stale)
            steps S20+ use new overlay from P0
            → maximum staleness: ~200ms × sim_speed

NO BLOCKING: simulation never waits for prediction
CONSISTENCY: prediction overlay is immutable once published
             (read by many, written by one, swap is atomic)
```

### Implementation: Lock-Free Overlay Swap

```rust
// velos-predict/src/overlay.rs

use std::sync::Arc;
use arc_swap::ArcSwap;

/// Global prediction state — shared between sim thread and prediction thread
pub struct PredictionState {
    /// Current active prediction overlay (read by simulation, written by predictor)
    overlay: ArcSwap<PredictionOverlay>,

    /// Prediction provider registry
    providers: Vec<Box<dyn PredictionProvider>>,

    /// Configuration
    global_interval: Duration,    // how often to run global prediction
    group_interval: Duration,     // how often to run group prediction
}

impl PredictionState {
    /// Called by simulation thread — never blocks
    pub fn current_overlay(&self) -> Arc<PredictionOverlay> {
        self.overlay.load()  // atomic load, ~1 nanosecond
    }

    /// Called by prediction thread — publishes new overlay
    pub fn publish_overlay(&self, new_overlay: PredictionOverlay) {
        self.overlay.store(Arc::new(new_overlay));  // atomic swap
        // Old overlay is automatically dropped when last reader releases it
    }
}

/// The overlay that agents read during pathfinding
pub struct PredictionOverlay {
    pub timestamp: f64,
    pub horizons: Vec<Duration>,  // [1m, 5m, 15m, 30m]

    /// Per-edge predictions — stored as flat arrays for GPU upload
    pub edge_ids: Vec<u32>,
    pub travel_times: Vec<[f32; 4]>,     // [1m, 5m, 15m, 30m] per edge
    pub congestion_risks: Vec<f32>,       // 0-1 per edge
    pub confidences: Vec<f32>,            // 0-1 per edge

    /// Group-level predictions
    pub group_overrides: HashMap<GroupId, GroupPrediction>,

    /// Individual-level predictions (sparse — only special agents)
    pub individual_overrides: HashMap<EntityId, IndividualPrediction>,
}
```

### How Prediction Feeds Into Agent Decision

```
STEP-BY-STEP FLOW:

Step N: Agent_42 (Commuter) is on Edge_100, approaching Junction_55

1. PERCEPTION (GPU):
   Agent reads PredictionOverlay for edges on current route:
     Edge_101: congestion_risk=0.8, pred_travel_time_5m=120s (normally 30s)
     Edge_102: congestion_risk=0.2, pred_travel_time_5m=35s

   Agent reads SPaT from Junction_55:
     Current phase: RED for Agent_42's direction
     Time to green: 45s
     Next phase duration: 30s

2. EVALUATION (GPU):
   Current route cost (with prediction):
     Edge_101 cost = 0.4×120 + 0.1×0.8 = 48.08

   Alternative via Edge_201:
     Edge_201 cost = 0.4×45 + 0.1×0.1 = 18.01

   cost_delta = 48.08 - 18.01 = 30.07
   reroute_threshold = 15.0

3. DECISION (CPU — triggered because cost_delta > threshold):
   Run A* from current_position to destination
   with edge costs = f(prediction overlay, signal states, agent profile)

   New route: [..., Edge_201, Edge_202, Edge_55, ...]
   → Agent avoids predicted congestion on Edge_101

4. EXECUTION (GPU):
   IDM car-following with new route's next edge as target
   Begin lane change if needed for new route
```

---

## 9. ECS Components & New Crates {#9-ecs-components}

### New Components

```rust
// === Agent Intelligence Components ===

#[repr(C)]  // GPU-resident
pub struct PerceptionState {
    pub leader_gap: f32,
    pub leader_speed: f32,
    pub signal_state: u32,          // encoded: phase + time_remaining
    pub signal_distance: f32,
    pub congestion_ahead: f32,      // 0-1 from prediction overlay
    pub emergency_nearby: u32,      // bool flag
    pub speed_limit: f32,           // from current edge/sign
    pub prediction_confidence: f32, // from overlay
}

#[repr(C)]  // GPU-resident
pub struct CostEvaluation {
    pub current_route_cost: f32,
    pub best_alternative_cost: f32,
    pub cost_delta: f32,
    pub should_reroute: u32,        // bool flag (1 = yes)
}

// CPU-only (variable size, complex logic)
pub struct AgentIntelligence {
    pub profile: AgentProfile,
    pub reroute_batch: u32,         // which batch this agent evaluates in
    pub last_reroute_step: u64,
    pub route_memory: Vec<RouteMemoryEntry>,  // past routes + outcomes
}

// === V2I Components ===

pub struct V2ICapable {
    pub can_receive_spat: bool,
    pub can_request_priority: bool,
    pub can_cooperative_cross: bool,  // slot-based intersection
    pub obu_range: f32,              // on-board unit comm range (meters)
}

pub struct SignalPriorityRequest {
    pub junction_id: JunctionId,
    pub desired_phase: u8,
    pub urgency: PriorityLevel,
    pub eta_seconds: f32,
}

pub enum PriorityLevel {
    Emergency,    // immediate
    Transit,      // high (bus, tram)
    Freight,      // medium (heavy vehicles)
    Standard,     // low (connected cars — future)
}

// === Prediction Components ===

pub struct PredictionAware {
    pub uses_tier: PredictionTier,        // Global, Group, or Individual
    pub prediction_trust: f32,            // how much agent weights predictions
    pub override_active: bool,            // individual prediction override active
}

pub enum PredictionTier {
    GlobalOnly,
    GroupLevel(GroupId),
    Individual,
}
```

### New Crates to Add

```
crates/
├── velos-agent/                      # NEW — Agent intelligence & decision
│   ├── src/
│   │   ├── brain.rs                  # Per-step decision loop orchestration
│   │   ├── perception.rs             # GPU perception system
│   │   ├── cost_function.rs          # Multi-factor pathfinding cost
│   │   ├── reroute_scheduler.rs      # Staggered reroute batching
│   │   ├── profile.rs                # Agent profiles (Commuter, Bus, etc.)
│   │   └── memory.rs                 # Route memory & learning (optional)
│   └── Cargo.toml
│
├── velos-predict/                    # NEW — Prediction pipeline
│   ├── src/
│   │   ├── overlay.rs                # PredictionOverlay + ArcSwap
│   │   ├── provider.rs               # PredictionProvider trait
│   │   ├── ensemble.rs               # Built-in ensemble (BPR + ETS + historical)
│   │   ├── bpr.rs                    # BPR delay function extrapolation
│   │   ├── ets.rs                    # Exponential smoothing correction
│   │   ├── historical.rs             # Historical pattern matcher
│   │   ├── scheduler.rs              # Async prediction scheduling (dual-loop)
│   │   ├── registry.rs               # Model registry + hot-swap
│   │   └── arrow_bridge.rs           # Arrow IPC for external ML models
│   ├── shaders/
│   │   ├── prediction_upload.wgsl    # Upload overlay to GPU for agent reads
│   │   └── cost_evaluation.wgsl      # GPU cost function computation
│   └── Cargo.toml
│
├── velos-v2i/                        # NEW — Vehicle-to-Infrastructure
│   ├── src/
│   │   ├── spat.rs                   # Signal Phase & Timing broadcast
│   │   ├── priority.rs               # Signal priority request/grant
│   │   ├── cooperative.rs            # Cooperative intersection (slot-based)
│   │   ├── signs.rs                  # Traffic sign interaction
│   │   ├── variable_speed.rs         # Dynamic speed limit zones
│   │   └── congestion_pricing.rs     # Pricing zone cost injection
│   └── Cargo.toml
```

---

## 10. GPU Compute Additions {#10-gpu-additions}

### New Shader: Cost Evaluation (WGSL)

```wgsl
// cost_evaluation.wgsl — SCREENING FILTER (M3 fix)
//
// IMPORTANT DESIGN DECISION (from architecture review M3):
// This GPU shader is a COARSE SCREENING FILTER only. It outputs a single
// `should_reroute: bool` flag per agent. The ACTUAL cost function that
// evaluates routes lives in ONE place: velos-agent/src/cost_function.rs (CPU).
//
// Why: Having two implementations of the same cost logic (GPU + CPU) inevitably
// drifts as developers modify one and forget the other. The GPU shader checks
// "is congestion bad enough to warrant rerouting?" — the CPU then does the
// actual A* pathfinding with the full multi-factor cost function.

struct PerceptionState {
    leader_gap: f32,
    leader_speed: f32,
    signal_state: u32,
    signal_distance: f32,
    congestion_ahead: f32,
    emergency_nearby: u32,
    speed_limit: f32,
    prediction_confidence: f32,
};

struct AgentWeights {
    w_predict: f32,           // prediction sensitivity (from agent profile)
    reroute_threshold: f32,   // congestion risk threshold to trigger reroute
    _pad0: f32,
    _pad1: f32,
};

struct PredictionEntry {
    travel_time_1m: f32,
    travel_time_5m: f32,
    congestion_risk: f32,
    confidence: f32,
};

struct RoutePredict {
    route_predicted_cost: f32,  // pre-computed on CPU: sum of predicted_travel_time
                                // for next K edges on agent's route (M7 fix)
    route_freeflow_cost: f32,   // sum of freeflow travel times for same edges
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<storage, read> perceptions: array<PerceptionState>;
@group(0) @binding(1) var<storage, read> weights: array<AgentWeights>;
@group(0) @binding(2) var<storage, read> predictions: array<PredictionEntry>;
@group(0) @binding(3) var<storage, read> current_edge: array<u32>;
@group(0) @binding(4) var<storage, read> route_predictions: array<RoutePredict>;
@group(0) @binding(5) var<storage, read_write> cost_output: array<CostEvaluation>;

struct CostEvaluation {
    should_reroute: u32,        // 0 = no, 1 = yes (screening result)
    congestion_severity: f32,   // for CPU to prioritize reroute batch
    route_delay_ratio: f32,     // predicted/freeflow ratio for current route
    _pad: f32,
};

@compute @workgroup_size(256)
fn evaluate_cost(@builtin(global_invocation_id) gid: vec3<u32>) {
    let idx = gid.x;
    if (idx >= arrayLength(&perceptions)) { return; }

    let p = perceptions[idx];
    let w = weights[idx];
    let edge = current_edge[idx];
    let pred = predictions[edge];
    let rp = route_predictions[idx];

    // SCREENING CHECK 1: Is current edge congested?
    let edge_congested = pred.congestion_risk > w.reroute_threshold
                         && pred.confidence > 0.5;

    // SCREENING CHECK 2: Is route-level prediction significantly worse than freeflow?
    // (M7 fix: route-level, not just current-edge)
    let route_delay_ratio = rp.route_predicted_cost / max(rp.route_freeflow_cost, 1.0);
    let route_delayed = route_delay_ratio > 1.5;  // route >50% slower than freeflow

    // SCREENING CHECK 3: Emergency nearby — always consider reroute
    let emergency = p.emergency_nearby == 1u;

    let should_reroute = select(0u, 1u, edge_congested || route_delayed || emergency);

    // Severity score for CPU to prioritize which agents get rerouted first
    let severity = pred.congestion_risk * pred.confidence
                 + select(0.0, 10.0, emergency);

    cost_output[idx] = CostEvaluation(
        should_reroute,
        severity,
        route_delay_ratio,
        0.0
    );
}
```

> **Architecture Note (M3):** The actual multi-factor cost function
> `cost = f(Time, Comfort, Safety, Fuel, Signal, Prediction)` lives exclusively in
> `velos-agent/src/cost_function.rs` (CPU). This GPU shader is intentionally simple —
> it only decides "should this agent be in the reroute batch?" The CPU then evaluates
> the full cost function during A* pathfinding for flagged agents.
>
> **Architecture Note (M7):** `route_predictions` is pre-computed on CPU each step:
> for each agent's remaining route edges, sum the predicted travel times from the
> PredictionOverlay. This gives route-level prediction awareness (not just current-edge).
> The single `route_predicted_cost: f32` is uploaded to GPU for screening.

### GPU Buffer Layout Update

```
UPDATED MEMORY BUDGET:

Buffer                          Per Agent    500K Agents
──────────────────────────────  ──────────   ───────────
Position + Velocity + Accel     24 bytes     12 MB
Leader reference                8 bytes      4 MB
Perception state                32 bytes     16 MB       ← NEW
Agent weights                   32 bytes     16 MB       ← NEW
Cost evaluation output          16 bytes     8 MB        ← NEW
Prediction overlay (per edge)   16 bytes     1.6 MB (100K edges) ← NEW
Pedestrian force                20 bytes     1 MB (50K peds)
──────────────────────────────────────────────────────────
Total GPU VRAM                               ~59 MB

Still well within budget. RTX 4090: 24 GB VRAM.
```

---

## 11. API Extensions {#11-api-extensions}

### New gRPC Services

```protobuf
// proto/prediction.proto

service VelosPrediction {
    // Register external prediction model
    rpc RegisterModel(ModelRegistration) returns (ModelId);
    rpc ActivateModel(ActivateRequest) returns (Empty);
    rpc DeactivateModel(ModelId) returns (Empty);

    // Subscribe to network state (for external ML models)
    rpc SubscribeNetworkState(StateSubscription) returns (stream NetworkSnapshot);

    // Publish prediction results (from external ML models)
    rpc PublishGlobalPrediction(GlobalPredictionData) returns (PredictionAck);
    rpc PublishGroupPrediction(GroupPredictionData) returns (PredictionAck);
    rpc PublishIndividualPrediction(IndividualPredictionData) returns (PredictionAck);

    // Query current prediction state
    rpc GetCurrentOverlay(Empty) returns (PredictionOverlay);
    rpc GetPredictionAccuracy(AccuracyQuery) returns (AccuracyReport);

    // A/B testing
    rpc StartABTest(ABTestConfig) returns (TestId);
    rpc GetABTestResults(TestId) returns (ABTestResults);
}

service VelosV2I {
    // Signal management
    rpc GetSPaT(JunctionId) returns (SPaTMessage);
    rpc RequestSignalPriority(PriorityRequest) returns (PriorityResponse);
    rpc SetAdaptiveSignalPolicy(AdaptivePolicy) returns (Empty);

    // Dynamic infrastructure
    rpc SetVariableSpeedLimit(SpeedLimitRequest) returns (Empty);
    rpc SetCongestionPricingZone(PricingZoneConfig) returns (Empty);
    rpc BlockEdgeWithEvent(EventConfig) returns (Empty);
}

service VelosAgent {
    // Agent profile management
    rpc SetAgentProfile(AgentProfileConfig) returns (Empty);
    rpc SetDefaultProfile(AgentClass, AgentProfile) returns (Empty);

    // Override individual agent behavior
    rpc OverrideAgentRoute(AgentId, RouteOverride) returns (Empty);
    rpc SetAgentPredictionTier(AgentId, PredictionTier) returns (Empty);

    // Query agent decisions
    rpc GetAgentDecisionLog(AgentId) returns (stream DecisionEvent);
    rpc GetRerouteStatistics(TimeRange) returns (RerouteStats);
}
```

---

## 12. Updated Roadmap Impact (Revised to 9 Months) {#12-roadmap-impact}

> **Post-architecture-review revision:** Timeline extended from 6→9 months to accommodate
> expanded scope. See `rebuild-sumo-architecture-plan.md` §11 for full 9-month Gantt chart.

### Agent Intelligence & Prediction Phasing (9-Month Plan)

```
Month 1-2: Foundation + GPU Semi-Sync
  Agent-related: AgentProfile component scaffolding, PerceptionState component,
  RouteArena flat allocator, Contraction Hierarchies (fast_paths)

Month 3: Agent Intelligence (E5 joins)
  E5: velos-agent crate: brain.rs, cost_function.rs (SINGLE source of truth, M3)
  E5: SPaT broadcast, signal priority request/grant
  E3: cost_evaluation.wgsl (SCREENING FILTER ONLY, not full cost — M3 fix)
  E2: Staggered reroute scheduler (1000 CH queries/step)
  E2: Multi-factor cost function, agent profiles

Month 4: Prediction + Meso
  E5: Meso queue model + meso→micro transition (C7 buffer zone fix)
  E5: BPR + ETS ensemble predictor (built-in, no ML dependency)
  E5: PredictionOverlay + ArcSwap lock-free swap
  E5: Async dual-loop architecture (fast sim + slow prediction)
  E5: Route-level prediction pre-computation on CPU (M7 fix)

Month 5: V2I + ML Bridge
  E5: Cooperative intersection management (slot-based)
  E5: Variable speed limit zones, congestion pricing zones
  E5: Model registry + hot-swap + historical pattern matcher
  E4: Arrow IPC bridge for external ML (PredictionProvider trait)

Month 6: Calibration + Scenarios
  E5: Individual-tier prediction (emergency, fleet agents)
  E5: A/B testing framework for comparing prediction models
  E4: Scenario management (M10): define, batch-run, compare

Month 7: Hardening
  E5: Prediction accuracy metrics (MAPE, GEH per edge)
  E5: Ensemble auto-weight-tuning
  E5: V2I edge cases (priority deadlock, competing emergency vehicles)

Month 8-9: Demo + Ship
  E5: Example Python LSTM model, prediction model development guide
  E5: Agent decision visualization, smart corridor demo scenario
```

### E5 Role (Expanded — Intelligence & Prediction Lead)

```
E5 (joins Month 3, full-time focus on intelligence systems):

  Month 3: Agent brain, SPaT, signal priority, cost function architecture
  Month 4: Meso model, prediction pipeline (BPR+ETS+ArcSwap), route-level prediction
  Month 5: Full V2I, model registry, historical pattern matcher
  Month 6: Individual prediction, A/B testing
  Month 7: Accuracy metrics, auto-tuning, edge case hardening
  Month 8: Documentation, LSTM example
  Month 9: Smart corridor demo scenario, final integration

Key architecture decisions:
  - GPU cost_evaluation.wgsl is SCREENING FILTER only (M3)
  - Actual cost function: single source in velos-agent/cost_function.rs (M3)
  - Route-level prediction pre-computed on CPU (M7)
  - 50m buffer zone for meso→micro transition (C7)
```

---

## 13. Benchmark Targets {#13-benchmarks}

### Prediction Performance

```
Metric                          Target          Notes
──────────────────────────────  ──────────────  ──────────────────
Built-in ensemble latency       < 50ms          for 100K edges
  (global prediction)
ML bridge round-trip             < 200ms         Arrow IPC + model inference
  (global prediction)
Prediction overlay swap          < 1μs           ArcSwap atomic operation
Cost evaluation (GPU)            < 0.5ms         for 500K agents
Reroute batch (CPU, 1000 A*)     < 5ms           parallel with rayon
Max prediction staleness         600ms           at 10x sim speed
Prediction accuracy (MAPE)       < 15%           5-min horizon, after warmup
```

### Agent Intelligence Performance

```
Metric                          Target          Notes
──────────────────────────────  ──────────────  ──────────────────
Perception update (GPU)          < 0.3ms         500K agents
Cost evaluation (GPU)            < 0.5ms         500K agents
SPaT broadcast                   < 0.1ms         1000 junctions
Reroute CH query (single agent)  < 0.01ms        100K-edge network (M4: fast_paths)
Reroute batch (1000 agents)      < 0.7ms         rayon parallel, 16 cores (M4)
V2I priority evaluation          < 0.1ms         per junction per step
Total per-step overhead          < 2ms           all agent intelligence
  (added to base simulation)
```

---

*Document version: 2.0 (post-architecture-review) | VELOS Agent Intelligence & Prediction Pipeline | March 2026*
*Changes from v1.0: M3 (GPU cost eval → screening filter only), M7 (route-level prediction), 9-month timeline.*
*See velos-architecture-review.md for full issue log.*
