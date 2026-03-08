# Phase 9: Sim Loop Integration — Startup & Frame Pipeline - Context

**Gathered:** 2026-03-08
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire all Phase 6-8 modules into the simulation loop. After this phase, tick_gpu() runs the full pipeline (perception, reroute, polymorphic signals, sign interaction, vehicle params, HCMC behaviors) — not just Phase 5 physics. SimWorld::new() initializes all subsystems at startup. The simulation runs the complete VELOS pipeline end-to-end.

Requirements: SIG-01, SIG-02, SIG-03, SIG-04, SIG-05, INT-03, INT-04, INT-05, RTE-03, RTE-07, TUN-02, TUN-04, TUN-06

</domain>

<decisions>
## Implementation Decisions

### Startup Initialization Order
- All new subsystems initialize in SimWorld::new(), not GpuState::new() — keeps simulation logic contained in sim layer
- Vehicle params loaded from TOML config with hardcoded default path `data/hcmc/vehicle_params.toml`, overridable via `VELOS_VEHICLE_CONFIG` env var
- If vehicle_params.toml is missing, use VehicleConfig::default() hardcoded HCMC-calibrated values and log warning — no crash
- init_reroute() blocks startup until CCH is built (<10s on 25K edges) — sim starts with rerouting ready
- PerceptionPipeline is mandatory — if GPU can't create it, that's a hard startup failure
- upload_vehicle_params() called at startup to populate GPU uniform buffer at binding 7
- upload_signs() called at startup to populate sign_buffer from network sign data

### Frame Pipeline Ordering
- Full pipeline order: spawn → signals → perception → reroute → GPU vehicles → CPU pedestrians → gridlock → cleanup
- Signal controllers tick BEFORE perception (keep current tick_gpu() ordering) — perception sees fresh signal states
- Perception GPU pass runs BEFORE vehicle physics — agents perceive current state, physics uses perception to inform behavior
- step_reroute() runs AFTER perception, BEFORE physics — agents reroute based on fresh perception, physics uses new routes
- HCMC behaviors (red_light_creep_speed, intersection_gap_acceptance) called from WGSL shader reading perception_results — no CPU readback needed for these behaviors

### Signal Controller Selection
- Config file `data/hcmc/signal_config.toml` maps intersection IDs to controller types (fixed/actuated/adaptive)
- Unmapped intersections default to FixedTimeController — safe, predictable default
- If signal_config.toml is missing, all intersections fall back to fixed-time with log warning
- Detector readings for actuated signals come from separate LoopDetector counting mechanism (velos-signal), not from perception pipeline — detectors are independent physical devices
- SignalController trait dispatches polymorphically — ActuatedController and AdaptiveController instantiated based on intersection config

### Graceful Degradation
- CCH router failure: Claude's discretion on whether to run without rerouting or hard fail
- Missing signal_config.toml: fall back to all FixedTimeController, log warning
- No sign data in network: skip silently, log info — sign_count=0 means shader skips sign interaction, empty sign_buffer is valid
- PerceptionPipeline: mandatory, hard fail if can't initialize
- Missing vehicle_params.toml: use hardcoded defaults, log warning

### Claude's Discretion
- CCH failure handling strategy (Option<RerouteState> vs hard fail)
- Exact SimWorld::new() initialization sequence within the constraints above
- How to pass device/queue from GpuState to SimWorld for GPU resource creation
- Loop detector update mechanism (how detector readings feed into actuated controller tick)
- Sign buffer population strategy (batch upload at startup vs incremental)
- Perception readback strategy for step_reroute() (sync vs async readback)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `PerceptionPipeline` (velos-gpu/src/perception.rs): Fully implemented with new(), create_bind_group(), dispatch(), readback_results(). Reads agent, signal, sign, congestion buffers.
- `RerouteState` (velos-gpu/src/sim_reroute.rs): Has init_reroute() and step_reroute() methods on SimWorld. Contains CCH router, edge_node_map, prediction_service, edge_attrs.
- `SignalController` trait (velos-signal/src/lib.rs): Polymorphic trait with tick(), get_phase_state(), reset(), spat_data(), request_priority(). FixedTimeController already implements it.
- `ActuatedController` (velos-signal/src/actuated.rs): Implements SignalController trait with loop detector-triggered phase transitions.
- `AdaptiveController` (velos-signal/src/adaptive.rs): Implements SignalController trait with demand-responsive timing.
- `upload_vehicle_params()` (velos-gpu/src/compute.rs): Exists on ComputeDispatcher, uploads GpuVehicleParams to uniform buffer at binding 7.
- `red_light_creep_speed()` (velos-vehicle/src/sublane.rs): CPU reference function. Limited to motorbike/bicycle, 0.3 m/s max, 5m ramp.
- `intersection_gap_acceptance()` (velos-vehicle/src/intersection.rs): CPU reference function with size_factor and forced acceptance after 5s wait.
- `LoopDetector` (velos-signal/src/detector.rs): Point sensor counting mechanism for actuated signal triggers.
- `signs` module (velos-signal/src/signs.rs): Sign types and GPU sign data structures.

### Established Patterns
- SimWorld holds all simulation state; GpuState holds GPU resources + SimWorld
- tick_gpu() orchestrates frame pipeline: spawn → signals → GPU vehicles → pedestrians → cleanup
- GPU compute dispatch via ComputeDispatcher with wave-front per-lane
- CPU reference + GPU production (tick() vs tick_gpu()) for validation
- Config loading pattern: hardcoded path with env override (matches pbf_path pattern)
- SimWorld.reroute field already exists (Option<RerouteState>)

### Integration Points
- `SimWorld::new()`: Currently creates road_graph, spawner, signal_controllers — needs vehicle params, reroute init, perception pipeline, sign loading
- `tick_gpu()`: Currently spawn → signals → GPU vehicles → pedestrians — needs perception and reroute steps inserted
- `ComputeDispatcher`: Has upload_vehicle_params() — needs to be called at startup
- `wave_front.wgsl`: Needs to read perception_results for HCMC behaviors (red-light creep, gap acceptance)
- `signal_controllers: Vec<...>` in SimWorld: Currently all FixedTimeController — needs polymorphic dispatch via Box<dyn SignalController>

</code_context>

<specifics>
## Specific Ideas

- Frame pipeline is the core deliverable: spawn → signals → perception → reroute → physics → pedestrians — this ordering ensures agents perceive fresh signal states and reroute before physics applies
- Signal controller types come from config file, not heuristics — explicit control over which intersections get which controller
- HCMC behaviors (creep, gap acceptance) run on GPU reading perception results — no CPU readback roundtrip for these hot-path behaviors
- Detectors and perception are independent systems — detectors are physical sensors for signals, perception is agent awareness. Don't couple them.
- Graceful degradation follows the "defaults with override" pattern established in Phase 8 — missing configs use safe defaults, never crash

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 09-sim-loop-integration-startup-frame-pipeline*
*Context gathered: 2026-03-08*
