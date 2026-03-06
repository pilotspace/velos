# VELOS v2 Routing & Prediction

## Resolves: W5 (CH Dynamic Weights), W6 (Arrow IPC Latency)

---

## 1. Customizable Contraction Hierarchies — CCH (W5)

### Problem

Standard Contraction Hierarchies (CH, `fast_paths` crate) require complete re-contraction (~30s for 100K edges) when edge weights change. With prediction updating costs every 60s, CH maintenance would consume 50% of compute budget.

### Solution: Customizable Contraction Hierarchies (CCH)

CCH separates the hierarchy structure (node ordering + shortcut topology) from edge weights. The ordering is computed once during startup (~30s). When weights change, only a **weight customization pass** runs — O(|shortcuts|), typically 2-5ms for 25K edges.

**Architecture:**

```
Startup (once):
  1. Build graph from OSM (25K edges, 15K nodes for HCMC POC area)
  2. Compute CCH node ordering + shortcut topology (~30s)
  3. Customize with initial free-flow weights (~3ms)

Every 60s (prediction update):
  1. Prediction ensemble produces new edge travel times
  2. CCH weight customization pass (~3ms)
  3. New queries immediately use updated weights

Per-agent query:
  1. Bidirectional Dijkstra on CCH (~0.02ms per query)
  2. Total: 500 queries/step × 0.02ms = 10ms → 0.7ms with rayon
```

**Comparison:**

| Operation | Standard CH | CCH |
|-----------|------------|-----|
| Initial build | 30s | 30s |
| Weight update | 30s (full rebuild) | 3ms (customization only) |
| Query time | 0.01ms | 0.02ms (slightly slower) |
| Dynamic weights | Incompatible | Native support |

### Implementation

```rust
pub struct CCHRouter {
    // Immutable after startup
    node_order: Vec<u32>,           // node → rank
    shortcut_graph: ShortcutGraph,  // topology with shortcut edges

    // Mutable on weight update
    forward_weights: Vec<f32>,      // weight per directed edge (current)
    backward_weights: Vec<f32>,

    // Customization workspace
    customization_buffer: Vec<f32>, // temporary during weight update
}

impl CCHRouter {
    /// Called once at startup (~30s)
    pub fn new(graph: &RoadGraph) -> Self {
        let (order, shortcuts) = compute_cch_ordering(graph);
        let mut router = Self { node_order: order, shortcut_graph: shortcuts, /* ... */ };
        router.customize(&graph.free_flow_weights());
        router
    }

    /// Called every 60s when prediction updates edge costs (~3ms)
    pub fn customize(&mut self, edge_weights: &[f32]) {
        // Bottom-up: for each shortcut edge, set weight = sum of constituent edges
        for level in 0..self.shortcut_graph.num_levels() {
            for shortcut in self.shortcut_graph.shortcuts_at_level(level) {
                self.forward_weights[shortcut.id] =
                    self.forward_weights[shortcut.lower_edge_1] +
                    self.forward_weights[shortcut.lower_edge_2];
            }
        }
    }

    /// Per-agent query (~0.02ms)
    pub fn query(&self, source: NodeId, target: NodeId) -> Option<Path> {
        bidirectional_dijkstra(&self.shortcut_graph, &self.forward_weights,
                               &self.backward_weights, source, target)
    }
}
```

### Why Not A*?

A* on the raw graph: ~0.5ms per query for HCMC-scale network.
CCH: ~0.02ms per query — 25x faster.

At 500 reroutes/step, A* costs 250ms (impossible at 10 steps/sec). CCH costs 10ms raw, 0.7ms with rayon parallelism.

---

## 2. In-Process Prediction Ensemble (W6)

### Problem

The v1 architecture bridges prediction via Arrow IPC to a Python process. This introduces:
- Cross-process latency: TLB pressure + page faults for 30MB/frame
- 300 MB/s throughput at 10 Hz for 500K agents
- Predictions always stale by 6-60 seconds
- Operational complexity of managing a Python sidecar

### Solution: Rust-Native Prediction, No Python Process

For POC, all prediction runs in-process in Rust. No Arrow IPC bridge. No Python sidecar.

**Ensemble Architecture:**

```
PredictionEnsemble (runs every 60 sim-seconds on tokio::spawn)
├── Model A: BPR Physics Extrapolation (weight: 0.40)
│   V_predicted = V_freeflow / (1 + alpha * (flow/capacity)^beta)
│
├── Model B: Exponential Smoothing (weight: 0.35)
│   Error_t = Actual_t - Predicted_t
│   Correction = gamma * Error_t + (1-gamma) * Correction_{t-1}
│   V_corrected = V_bpr + Correction
│
└── Model C: Historical Pattern Match (weight: 0.25)
    V_historical = weighted_avg(V at same (hour, day_of_week) from calibration data)

Final: V_ensemble = w_A * V_bpr + w_B * V_corrected + w_C * V_historical
Confidence = 1.0 - std_dev(V_A, V_B, V_C) / mean(V_A, V_B, V_C)
```

**Implementation:**

```rust
pub struct PredictionEnsemble {
    bpr_model: BPRPredictor,
    ets_model: ETSCorrector,
    historical_model: HistoricalMatcher,
    weights: [f32; 3],                    // auto-tuned
    overlay: Arc<ArcSwap<PredictionOverlay>>,
}

pub struct PredictionOverlay {
    pub edge_travel_times: Vec<f32>,   // predicted travel time per edge
    pub edge_confidence: Vec<f32>,     // 0.0-1.0 confidence per edge
    pub timestamp: SimTime,
}

impl PredictionEnsemble {
    /// Called every 60 sim-seconds (async, non-blocking)
    pub async fn update(&self, current_state: &SimSnapshot) {
        let v_bpr = self.bpr_model.predict(current_state);
        let v_ets = self.ets_model.predict(current_state, &v_bpr);
        let v_hist = self.historical_model.predict(current_state.time_of_day());

        let mut overlay = PredictionOverlay::new(current_state.edge_count());
        for edge_id in 0..current_state.edge_count() {
            let predictions = [v_bpr[edge_id], v_ets[edge_id], v_hist[edge_id]];
            let mean = weighted_mean(&predictions, &self.weights);
            let std = weighted_std(&predictions, &self.weights);

            overlay.edge_travel_times[edge_id] = mean;
            overlay.edge_confidence[edge_id] = 1.0 - std / mean.max(0.001);
        }

        // Atomic swap — zero-lock, zero-copy
        self.overlay.store(Arc::new(overlay));
    }
}
```

**Cost:** 25K edges × 3 models × simple arithmetic = ~0.1ms. Negligible.

**Why not Arrow IPC to Python?**

| Aspect | Arrow IPC (v1) | In-Process (v2) |
|--------|----------------|-----------------|
| Latency | 6-60s stale | 0ms (ArcSwap) |
| Throughput | 300 MB/s cross-process | In-memory, zero-copy |
| Ops complexity | Python sidecar + conda env | None |
| ML capability | Any Python ML model | Rust-native only |
| Extensibility | High (PyTorch, etc.) | Limited to Rust crates |

**Tradeoff accepted:** We lose the ability to plug in PyTorch/TensorFlow models. For POC, the built-in ensemble (BPR + ETS + historical) is sufficient. When ML models are needed (v3), we'll add a gRPC prediction service that VELOS queries — cleaner than Arrow IPC shared memory.

---

## 3. Agent Rerouting Strategy

### Staggered Reroute Evaluation

Not all agents need rerouting every step. Stagger evaluations:

```rust
pub struct RerouteScheduler {
    pub eval_interval: u32,   // 500 steps = 50 sim-seconds at Dt=0.1s
    pub batch_size: u32,      // 500 agents per step
    pub immediate_triggers: Vec<RerouteTrigger>,
}

pub enum RerouteTrigger {
    EdgeBlocked(EdgeId),          // incident, road closure
    PredictionAlert {             // prediction confidence drop
        edge_id: EdgeId,
        confidence_threshold: f32, // reroute if confidence < 0.3
    },
    TravelTimeSpike {             // actual time >> predicted
        ratio_threshold: f32,     // reroute if actual/predicted > 2.0
    },
}
```

**Per-Step Budget:**

```
500 agents × 0.02ms CCH query = 10ms sequential
With rayon (16 cores): 10ms / 16 = 0.7ms
Budget: 0.7ms — fits within frame
```

### Cost Function for Route Choice

```rust
pub fn route_cost(path: &[EdgeId], overlay: &PredictionOverlay, weights: &CostWeights) -> f32 {
    let mut total = 0.0;
    for &edge in path {
        let tt = overlay.edge_travel_times[edge as usize];
        let conf = overlay.edge_confidence[edge as usize];

        total += weights.time * tt
              + weights.safety * edge_safety_score(edge)
              + weights.comfort * edge_comfort_penalty(edge)
              + weights.fuel * edge_distance(edge) * fuel_rate(edge);

        // Low-confidence prediction → add risk penalty
        if conf < 0.5 {
            total += weights.time * tt * (1.0 - conf); // up to 100% penalty for zero confidence
        }
    }
    total
}
```

---

## 4. Time-of-Day Demand Integration (W15)

Prediction uses time-of-day patterns from calibration data:

```rust
pub struct HistoricalMatcher {
    // Pre-computed from HCMC traffic count data
    // Indexed by: [edge_id][hour_of_day][day_type]
    historical_speeds: Array3<f32>,  // ndarray
}

pub enum DayType {
    Weekday,
    Saturday,
    Sunday,
    Holiday,
}

impl HistoricalMatcher {
    pub fn predict(&self, time_of_day: f32, day_type: DayType) -> Vec<f32> {
        let hour = time_of_day as usize;
        let next_hour = (hour + 1) % 24;
        let frac = time_of_day - hour as f32;

        // Linear interpolation between hours
        self.historical_speeds.slice(s![.., hour, day_type as usize]) * (1.0 - frac)
        + self.historical_speeds.slice(s![.., next_hour, day_type as usize]) * frac
    }
}
```

HCMC demand profile (observed):

```
05:00-06:30  Morning ramp-up         (0.3 → 0.8 of peak)
06:30-08:30  Morning peak             (1.0 of peak, 280K agents)
08:30-11:00  Morning decline          (0.8 → 0.5)
11:00-13:00  Midday (lunch peak)      (0.6-0.7)
13:00-16:00  Afternoon steady         (0.5)
16:00-18:30  Evening peak             (1.0 of peak)
18:30-20:00  Evening decline          (0.7 → 0.4)
20:00-22:00  Night                    (0.3)
22:00-05:00  Late night               (0.1)
```
