# Phase 12: CPU Lane-Change, Prediction Loop & GPU Config - Context

**Gathered:** 2026-03-08
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire MOBIL lane-change overtaking (cars) and motorbike lateral filtering (sublane squeeze-through) into the GPU tick loop, wire PredictionService::update() into the frame loop so prediction overlay refreshes every 60 sim-seconds, and propagate HCMC creep/gap behavior constants from TOML config to GPU uniform buffer — eliminating all hardcoded WGSL constants for these behaviors.

Requirements: RTE-05, RTE-07, TUN-04, TUN-06, TBD (lane-change)
Gap Closure: RTE-05 (partial), RTE-07 (partial), PredictionService→frame loop, HCMC params→GPU

</domain>

<decisions>
## Implementation Decisions

### Lane-Change Execution Model
- MOBIL stays on CPU (sim_mobil.rs) — heavy branching (safety criterion, follower decel, acceleration comparison) is poor GPU fit
- CPU evaluates `evaluate_mobil()` → decides target lane → sets LaneChangeState on ECS entity
- GPU reads LaneChangeState during physics step to apply gradual lateral drift (existing 2-second drift in `process_car_lane_changes()`)
- Motorbikes use sublane filtering (`compute_desired_lateral()`) — continuous lateral positioning, NOT discrete lane changes
- Cars use MOBIL (discrete lane-to-lane), motorbikes use sublane (continuous lateral)
- Pattern follows Phase 10 precedent: CPU state machine + GPU flag/state consumption

### Lane-Change Trigger Conditions
- Motorbike sublane filtering runs every tick — lateral position is continuous, changes frame-by-frame
- MOBIL for cars: evaluated every tick but naturally throttled by LaneChangeState cooldown (can't start new lane change while 2-second drift is in progress)
- No explicit staggering needed — LaneChangeState active flag prevents re-evaluation mid-maneuver
- MOBIL evaluates adjacent lanes only (left + right), no cross-lane jumps
- Safety: existing `LaneChangeState` safeguard prevents concurrent lateral moves

### Prediction Loop Placement
- `PredictionService::should_update(sim_time)` called every tick — single f64 comparison, essentially free
- `PredictionService::update()` runs synchronously only when should_update returns true (every 60 sim-seconds, ~600 ticks at dt=0.1s)
- Placement: after `step_vehicles_gpu()` and `step_bus_dwell()`, before `step_pedestrians()` — vehicles have moved, edge flows are fresh
- ArcSwap swap is atomic — next frame's `step_reroute()` automatically sees updated overlay, no explicit wiring needed
- No async needed: BPR+ETS+historical computation is ~0.2ms, well within frame budget
- Pipeline becomes: ... → step_vehicles_gpu() → step_bus_dwell() → step_prediction() → step_pedestrians()

### GPU Uniform Buffer Extension
- Extend GpuVehicleParams from 8 to 12 floats per vehicle type (32 bytes → 48 bytes, 16-byte aligned for WGSL)
- New fields: creep_max_speed, creep_distance_scale, creep_min_distance, gap_acceptance_ttc
- WGSL VehicleTypeParams struct updated to match — 12 f32 fields
- Remove ALL hardcoded WGSL constants: CREEP_MAX_SPEED, CREEP_DISTANCE_SCALE, CREEP_MIN_DISTANCE, GAP_MAX_WAIT_TIME, GAP_FORCED_ACCEPTANCE_FACTOR, GAP_WAIT_REDUCTION_RATE
- For non-sublane vehicles (car, bus, truck, emergency): creep fields set to 0.0 in upload — shader early-exits on creep_max_speed == 0.0
- gap_acceptance_ttc populated for all vehicle types from vehicle_params.toml (already present in TOML)
- Rust VehicleTypeParams already has all fields (config.rs) — just need to extend GpuVehicleParams upload path

### Claude's Discretion
- Exact step_prediction() implementation (how to gather edge flow/capacity data for PredictionInput)
- Whether gap-related WGSL constants (GAP_MAX_WAIT_TIME etc.) go into VehicleTypeParams or a separate uniform
- Test strategy for prediction loop integration (unit vs integration tests)
- Whether MOBIL evaluation order matters (left-first vs right-first)
- Sublane filtering GPU stub implementation details (CPU reference exists, GPU needs wiring)

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `MobilParams` (velos-vehicle/src/types.rs): Per-vehicle-type MOBIL parameters, loaded from config via `default_mobil_params()`
- `mobil_decision()` (velos-vehicle/src/mobil.rs:51): Core MOBIL safety + incentive evaluation — ready to use
- `evaluate_mobil()` (velos-gpu/src/sim_mobil.rs:31): Integrates MOBIL into car physics
- `process_car_lane_changes()` (velos-gpu/src/sim_mobil.rs:154): Applies gradual 2-second lateral drift
- `PredictionService` (velos-predict/src/lib.rs:159): Complete with should_update(), update(), 60s interval
- `PredictionStore` (velos-predict/src/overlay.rs:31): ArcSwap-backed lock-free overlay swap
- `PredictionOverlay` (velos-predict/src/overlay.rs:16): edge_travel_times + edge_confidence + timestamp
- `VehicleConfig` / `VehicleTypeParams` (velos-vehicle/src/config.rs): TOML loading with creep_max_speed, creep_distance_scale, gap_acceptance_ttc fields already defined
- `GpuVehicleParams` (velos-gpu/src/compute.rs:28): GPU upload struct — needs extension from 8 to 12 floats
- `upload_vehicle_params()` (velos-gpu/src/sim_startup.rs:254): Startup upload to binding 7
- `red_light_creep_speed()` (velos-vehicle/src/sublane.rs:94): CPU reference for creep behavior
- `compute_desired_lateral()` (velos-vehicle/src/sublane.rs:148): Main sublane positioning logic
- `apply_lateral_drift()` (velos-vehicle/src/sublane.rs:325): Smooth lateral movement

### Established Patterns
- CPU state machine + GPU flag consumption (bus dwell: CPU manages, GPU reads FLAG_BUS_DWELLING)
- Frame pipeline ordering: spawn → signals → perception → reroute → meso → GPU vehicles → bus dwell → pedestrians
- Config loading: hardcoded TOML path with env override, VehicleConfig::default() fallback
- GPU uniform buffer at binding 7 for vehicle-type params
- ArcSwap for lock-free prediction overlay reads

### Integration Points
- `SimWorld::tick_gpu()` (velos-gpu/src/sim.rs:371): 10-step pipeline — needs step_prediction() added after step_bus_dwell()
- `GpuVehicleParams` (velos-gpu/src/compute.rs:28): Extend struct, update upload_vehicle_params()
- `VehicleTypeParams` WGSL struct (wave_front.wgsl:137): Extend from 8 to 12 fields
- `wave_front.wgsl` lines 365-389: 6 hardcoded constants to remove → read from uniform buffer
- `sim_mobil.rs`: Already called from step_vehicles_gpu() via CPU reference path — needs proper integration into GPU tick
- `step_reroute()` (velos-gpu/src/sim_reroute.rs): Automatically benefits from ArcSwap overlay update — no code change needed

</code_context>

<specifics>
## Specific Ideas

- This phase is pure integration/wiring — all models (MOBIL, PredictionService, sublane) are already implemented and tested
- The prediction loop is the simplest workstream: add one if-check per tick, ~0.2ms computation every 600 ticks
- GPU buffer extension is mechanical: add 4 fields to Rust struct, match in WGSL struct, delete hardcoded constants
- MOBIL + sublane are the most complex: need to ensure CPU lane-change decisions are visible to GPU physics
- Key invariant: after this phase, zero hardcoded behavior constants remain in WGSL — everything comes from vehicle_params.toml via uniform buffer

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 12-cpu-lane-change-sublane-wiring-mobil-overtaking-and-motorbike-lateral-filtering-in-gpu-tick-loop*
*Context gathered: 2026-03-08*
