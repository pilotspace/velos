---
phase: 09-sim-loop-integration-startup-frame-pipeline
verified: 2026-03-08T12:45:00Z
status: passed
score: 7/7 must-haves verified
---

# Phase 9: Sim Loop Integration -- Startup & Frame Pipeline Verification Report

**Phase Goal:** All Phase 6-8 modules are wired into sim.rs::tick_gpu() and app.rs::GpuState::new() -- the simulation runs the full pipeline (perception, reroute, polymorphic signals, sign interaction, vehicle params, HCMC behaviors) not just Phase 5 physics
**Verified:** 2026-03-08T12:45:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths (from ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | upload_vehicle_params() is called at startup -- GPU uniform buffer at binding 7 contains correct per-type parameters, not zeros | VERIFIED | sim.rs:161 calls `sim_startup::upload_vehicle_params()`. sim_startup.rs:226-233 calls `dispatcher.upload_vehicle_params(queue, &gpu_params)`. compute.rs:457-458 writes to `vehicle_params_buffer`. wave_front.wgsl:148 declares `@binding(7) var<uniform> vehicle_params` |
| 2 | init_reroute() is called at startup -- CCH router, prediction overlay, and reroute scheduler are initialized and non-None | VERIFIED | sim.rs:206 calls `sim.init_reroute()` in `SimWorld::new()`. sim_reroute.rs:53 implements `init_reroute()` building CCH and prediction service |
| 3 | PerceptionPipeline is instantiated in GpuState and dispatched every frame in tick_gpu() -- perception_results buffer is populated | VERIFIED | sim.rs:176 creates `PerceptionPipeline::new(device, 300_000)`. sim.rs:326 calls `self.step_perception(device, queue, dispatcher)` every frame. sim_perception.rs:84-143 dispatches perception GPU pass and readbacks results |
| 4 | step_reroute() is called every frame after perception -- agents with should_reroute flag receive new CCH routes | VERIFIED | sim.rs:329 calls `self.step_reroute(&perception_results)` directly after step_perception (line 326) |
| 5 | SignalController dispatch uses the trait polymorphically -- actuated/adaptive controllers instantiated based on intersection config | VERIFIED | sim.rs:107 declares `signal_controllers: Vec<(NodeIndex, Box<dyn SignalController>)>`. sim_startup.rs:103-118 instantiates ActuatedController/AdaptiveController/FixedTimeController based on config.controller string. config.rs loads from TOML |
| 6 | sign_buffer is populated with sign data at startup via upload_signs() -- handle_sign_interaction processes real sign data | VERIFIED | sim.rs:173 calls `sim_startup::upload_network_signs()`. sim_startup.rs:196-222 collects GpuSign from edges and calls `dispatcher.upload_signs()`. wave_front.wgsl:305-357 implements `handle_sign_interaction()` reading from binding 6 signs buffer |
| 7 | red_light_creep_speed() and intersection_gap_acceptance() are called from the GPU simulation path for motorbike agents | VERIFIED | wave_front.wgsl:369-380 implements `red_light_creep_speed()`. wave_front.wgsl:403-415 implements `intersection_gap_acceptance()`. wave_front.wgsl:520-565 integrates both into `wave_front_update()` main loop reading perception_results per agent |

**Score:** 7/7 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/velos-gpu/src/sim_startup.rs` | Startup initialization methods for SimWorld | VERIFIED | 346 lines (min 80). Exports load_vehicle_config, build_signal_controllers, build_loop_detectors, upload_network_signs, upload_vehicle_params. 5 unit tests |
| `crates/velos-signal/src/config.rs` | SignalConfig TOML deserialization and loading | VERIFIED | 184 lines. Exports SignalConfig, IntersectionConfig, load_signal_config. Graceful fallback on missing file. 5 unit tests |
| `data/hcmc/signal_config.toml` | Default signal controller configuration for HCMC intersections | VERIFIED | 21 lines with documented format and example |
| `crates/velos-gpu/src/sim_perception.rs` | Perception dispatch and readback wiring for SimWorld | VERIFIED | 260 lines (min 60). PerceptionBuffers struct, step_perception(), update_signal_buffer(), update_edge_travel_ratio_buffer(). 4 unit tests |
| `crates/velos-gpu/src/sim_pedestrians.rs` | Extracted pedestrian stepping (700-line compliance) | VERIFIED | 158 lines. Extracted from sim.rs |
| `crates/velos-gpu/src/cpu_reference.rs` | CPU reference vehicle physics | VERIFIED | Extracted from sim.rs. Module declared in lib.rs |
| `crates/velos-gpu/shaders/wave_front.wgsl` | WGSL shader with perception binding + HCMC behaviors | VERIFIED | 579 lines (<700). Contains PerceptionResult struct, @binding(8), red_light_creep_speed(), intersection_gap_acceptance(), integrated into wave_front_update() |
| `crates/velos-gpu/src/sim.rs` | Sim tick with full pipeline | VERIFIED | 634 lines (<700). 10-step pipeline in tick_gpu(), polymorphic signal controllers, perception/reroute integration |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| sim_startup.rs | config.rs | load_signal_config() call | WIRED | sim_startup.rs imports velos_signal::config::SignalConfig; sim.rs:164 calls load_signal_config() |
| sim_startup.rs | compute.rs | upload_vehicle_params() call | WIRED | sim_startup.rs:231 calls dispatcher.upload_vehicle_params() |
| app.rs | sim.rs | SimWorld::new(road_graph, &device, &queue, &mut compute_dispatcher) | WIRED | app.rs:115 |
| sim_perception.rs | perception.rs | PerceptionPipeline::dispatch() and readback_results() | WIRED | sim_perception.rs:140 calls perception.dispatch(), line 143 calls readback_results() |
| sim.rs | sim_perception.rs | step_perception() call in tick_gpu() | WIRED | sim.rs:326 |
| sim.rs | sim_reroute.rs | step_reroute() call in tick_gpu() | WIRED | sim.rs:329 |
| wave_front.wgsl | perception_results buffer | @group(0) @binding(8) storage read | WIRED | wave_front.wgsl:163 declares binding. compute.rs:522-523 includes binding 8 in bind group |
| compute.rs | perception.rs | perception result_buffer() in bind group entry 8 | WIRED | compute.rs:425 set_perception_result_buffer(), compute.rs:522 binding: 8 entry |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| SIG-01 | 09-01 | Actuated signal control with loop detector-triggered phase transitions | SATISFIED | ActuatedController instantiated from TOML config (sim_startup.rs:106-112). LoopDetector updates per frame (sim.rs:408-452). Detector readings fed to actuated controllers (sim.rs:388-400) |
| SIG-02 | 09-01 | Adaptive signal control with demand-responsive timing optimization | SATISFIED | AdaptiveController instantiated from TOML config (sim_startup.rs:113) |
| SIG-03 | 09-03 | SPaT broadcast to agents within range for signal-aware driving | SATISFIED | SignalController trait has spat_data() method (lib.rs:53). Signal states written to perception signal_buffer per frame (sim_perception.rs:150-181). Agents read signal_state via perception_results in WGSL |
| SIG-04 | 09-03 | Signal priority request from buses and emergency vehicles | SATISFIED | step_signal_priority() in sim.rs:459-529 scans bus/emergency vehicles within 100m and calls ctrl.request_priority() |
| SIG-05 | 09-01 | Traffic sign interaction: speed limits, stop/yield, no-turn restrictions, school zones | SATISFIED | Signs uploaded at startup (sim_startup.rs:196-222). handle_sign_interaction() in wave_front.wgsl:305-357 processes SpeedLimit, Stop, SchoolZone |
| INT-03 | 09-03 | GPU perception phase: sense leader, signal state, signs, nearby agents, congestion | SATISFIED | PerceptionPipeline dispatched every frame (sim.rs:326). PerceptionResult struct contains leader_speed, leader_gap, signal_state, signal_distance, congestion fields |
| INT-04 | 09-03 | GPU evaluation phase: cost comparison, should_reroute flag + cost_delta | SATISFIED | step_reroute() called with perception results (sim.rs:329). Reroute subsystem evaluates from perception data |
| INT-05 | 09-03 | Staggered reroute evaluation (1K agents/step) | SATISFIED | step_reroute() in sim_reroute.rs implements staggered evaluation with perception results |
| RTE-03 | 09-03 | Dynamic agent rerouting at 500 reroutes/step using CCH queries | SATISFIED | init_reroute() builds CCH at startup (sim.rs:206). step_reroute() called per frame |
| RTE-07 | 09-03 | Prediction-informed routing | SATISFIED | update_edge_travel_ratio_buffer() in sim_perception.rs:187-207 reads from prediction_service overlay and writes to GPU buffer |
| TUN-02 | 09-01 | GPU/CPU parameter parity -- GPU reads vehicle-type params from uniform buffer populated from config | SATISFIED | upload_vehicle_params() writes GpuVehicleParams to binding 7 (sim_startup.rs:226-233). WGSL reads vehicle_params[vt] for per-type IDM/Krauss constants |
| TUN-04 | 09-02 | Red-light creep behavior -- motorbikes inch past stop line during red | SATISFIED | red_light_creep_speed() in wave_front.wgsl:369-380. Called from main loop at line 524-530 when signal_state == RED |
| TUN-06 | 09-02 | Yield-based intersection negotiation -- vehicle-type-dependent TTC gap acceptance | SATISFIED | intersection_gap_acceptance() in wave_front.wgsl:403-415 with size_factor(). Called from main loop at lines 535-565 when signal_state == NONE |

No orphaned requirements found. All 13 requirement IDs from PLAN frontmatters (SIG-01, SIG-02, SIG-03, SIG-04, SIG-05, INT-03, INT-04, INT-05, RTE-03, RTE-07, TUN-02, TUN-04, TUN-06) match the phase mapping in REQUIREMENTS.md.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none found) | - | - | - | - |

No TODOs, FIXMEs, placeholders, or stub implementations found in phase 09 modified files (sim.rs, sim_startup.rs, sim_perception.rs, sim_pedestrians.rs, config.rs, wave_front.wgsl, compute.rs).

### Human Verification Required

### 1. GPU Perception Pipeline End-to-End

**Test:** Run the simulation with agents on a signalized intersection. Inspect perception_results to verify signal_state values correspond to actual controller states.
**Expected:** Agents near a red light receive signal_state=2 in their perception results. Motorbike agents should creep forward (speed ~0.1-0.3 m/s) instead of fully stopping.
**Why human:** Requires running GPU pipeline with real wgpu device. Cannot verify GPU buffer contents via static analysis.

### 2. Actuated Signal Response to Detector Triggers

**Test:** Create a scenario with an actuated intersection configured in signal_config.toml. Observe if gap-out transitions occur when no vehicles cross detectors.
**Expected:** After min_green expires, if no detector triggers for gap_threshold seconds, phase transitions to the next phase.
**Why human:** Requires temporal simulation behavior observation with specific traffic patterns.

### 3. Signal Priority for Bus/Emergency Vehicles

**Test:** Spawn a bus or emergency vehicle on an edge approaching a signalized intersection within 100m.
**Expected:** Signal controller receives a priority request (PriorityRequest with appropriate PriorityLevel). Actuated/adaptive controllers extend green or shorten conflicting red.
**Why human:** Requires observing signal timing changes in response to priority requests during live simulation.

### Gaps Summary

No gaps found. All 7 success criteria from ROADMAP.md are verified against the codebase. All 13 requirement IDs are satisfied with implementation evidence. All artifacts exist, are substantive (well above minimum line counts), and are wired through imports and runtime calls. The 10-step frame pipeline is implemented in tick_gpu() with correct ordering. sim.rs is at 634 lines (under 700 limit). No anti-patterns detected.

---

_Verified: 2026-03-08T12:45:00Z_
_Verifier: Claude (gsd-verifier)_
