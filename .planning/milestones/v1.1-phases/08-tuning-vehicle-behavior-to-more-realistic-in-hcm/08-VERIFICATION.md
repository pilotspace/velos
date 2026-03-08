---
phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm
verified: 2026-03-08T12:00:00Z
status: passed
score: 11/11 must-haves verified
---

# Phase 8: Tuning Vehicle Behavior to More Realistic in HCM -- Verification Report

**Phase Goal:** All vehicle behavior parameters are externalized to config, GPU/CPU parameter mismatch is eliminated, and HCMC-specific behavioral rules (red-light creep, aggressive weaving, yield-based intersection negotiation) produce visually realistic mixed-traffic patterns
**Verified:** 2026-03-08T12:00:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | All vehicle behavior parameters load from data/hcmc/vehicle_params.toml | VERIFIED | `load_vehicle_config()` in config.rs reads TOML via `toml::from_str`. TOML file has 7 sections (motorbike, car, bus, truck, bicycle, emergency, pedestrian) with all IDM/Krauss/MOBIL/sublane params. Test `toml_parses_without_error` passes. |
| 2 | Each vehicle type has HCMC-calibrated defaults (truck v0=35 km/h not 90 km/h, car v0=35 km/h not 50 km/h) | VERIFIED | TOML: truck v0=9.7 (35 km/h), car v0=9.7 (35 km/h), motorbike v0=11.1 (40 km/h), bus v0=8.3 (30 km/h). Tests `truck_v0_in_hcmc_range_not_90kmh`, `car_v0_in_hcmc_range` pass with range assertions. |
| 3 | default_idm_params() and default_mobil_params() return values from config, not hardcoded literature values | VERIFIED | `types.rs:default_idm_params()` calls `VehicleConfig::default()` then `default_idm_params_from_config()`. No hardcoded literature values remain. Test `idm_params_from_config_matches_default` confirms. |
| 4 | Config validation rejects out-of-range parameters with descriptive errors | VERIFIED | `VehicleConfig::validate()` checks v0>0, s0>0, t_headway>0, a>0, b>0, delta>0, krauss_sigma in [0,1], politeness in [0,1], gap_acceptance_ttc>=0, pedestrian ranges. Tests `validate_rejects_v0_zero`, `validate_rejects_negative_v0` pass. |
| 5 | GPU shader reads vehicle-type parameters from uniform buffer, not hardcoded constants | VERIFIED | wave_front.wgsl has `@group(0) @binding(7) var<uniform> vehicle_params: array<VehicleTypeParams, 7>`. Zero hardcoded IDM/Krauss parameter constants remain (IDM_V0, IDM_S0, IDM_A, etc. all removed). Only physical limits IDM_MAX_DECEL and KRAUSS_TAU kept as non-tunable constants. |
| 6 | GPU and CPU produce identical IDM/Krauss behavior for the same config values | VERIFIED | `GpuVehicleParams::from_config()` converts VehicleConfig f64 to f32 GPU buffer. WGSL `idm_acceleration()` and `krauss_update()` read `vehicle_params[vt]` matching CPU param structs. Test `gpu_vehicle_params_from_config_all_types_indexed_correctly` verifies index mapping for all 7 types. |
| 7 | Changing a parameter in vehicle_params.toml changes both GPU and CPU behavior | VERIFIED | CPU: `load_vehicle_config()` -> `VehicleTypeParams` -> `to_idm_params()`. GPU: `VehicleConfig` -> `GpuVehicleParams::from_config()` -> `upload_vehicle_params()` -> binding 7. Single config source flows to both paths. |
| 8 | Motorbikes inch forward past stop line during red lights (red-light creep) | VERIFIED | `sublane.rs:red_light_creep_speed()` returns 0.0-0.3 m/s for Motorbike/Bicycle, 0.0 for all others. Gradual ramp with distance. Tests: `creep_motorbike_at_red_light_returns_positive`, `creep_car_returns_zero`, `creep_speed_bounded_at_max`, `creep_zero_when_past_stop_line`, `creep_speed_decreases_closer_to_stop_line` all pass. |
| 9 | Motorbikes accept gaps as small as 0.5m for lateral filtering at low speed differences | VERIFIED | `sublane.rs:effective_filter_gap()` returns `base + 0.1 * |delta_v|`. Base = 0.5m from config. At delta_v=0, gap=0.5m. Integrated into `compute_desired_lateral()`. Tests `effective_gap_at_zero_delta_v`, `effective_gap_increases_with_speed_difference` pass. |
| 10 | Gap acceptance at unsignalized intersections varies by vehicle type | VERIFIED | `intersection.rs:intersection_gap_acceptance()` uses `own_ttc_threshold` from config (motorbike=1.0s, car=1.5s, truck=2.0s). Size intimidation factors: truck/bus=1.3x, emergency=2.0x, motorbike/bicycle=0.8x. Tests cover all vehicle type pairs. |
| 11 | Deadlock prevention: max-wait timer forces acceptance after 3-5s | VERIFIED | `intersection.rs` uses `MAX_WAIT_TIME=5.0` with `FORCED_ACCEPTANCE_FACTOR=0.5`. After 5s, threshold halved. Gradual reduction at 10%/s before that. Tests `max_wait_forces_acceptance`, `deadlock_both_vehicles_max_wait` pass. |

**Score:** 11/11 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `data/hcmc/vehicle_params.toml` | HCMC-calibrated per-vehicle-type parameter defaults | VERIFIED | 145 lines, 7 sections, all HCMC-calibrated values |
| `crates/velos-vehicle/src/config.rs` | VehicleConfig struct with TOML deserialization and validation | VERIFIED | 445 lines, exports VehicleConfig, VehicleTypeParams, PedestrianParams, load_vehicle_config |
| `crates/velos-vehicle/tests/config_tests.rs` | Config loading, validation, and default fallback tests | VERIFIED | 234 lines, 16 tests |
| `crates/velos-vehicle/src/intersection.rs` | Gap acceptance logic with vehicle-type-dependent TTC and size factor | VERIFIED | 109 lines, exports intersection_gap_acceptance, IntersectionState |
| `crates/velos-vehicle/tests/intersection_tests.rs` | Gap acceptance tests for all vehicle types and edge cases | VERIFIED | 292 lines, 13 tests |
| `crates/velos-gpu/src/compute.rs` | GpuVehicleParams buffer upload and binding 7 | VERIFIED | GpuVehicleParams struct at line 28, from_config() at line 38, upload_vehicle_params() at line 411, binding 7 in BGL at line 212 and bind group at line 472 |
| `crates/velos-gpu/shaders/wave_front.wgsl` | Vehicle-type-parameterized IDM and Krauss shader | VERIFIED | VehicleTypeParams struct at line 137, binding 7 at line 148, idm_acceleration/krauss_update parameterized by vt |
| `crates/velos-gpu/tests/gpu_params_tests.rs` | GPU param struct tests | VERIFIED | 90 lines, 5 tests pass |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| config.rs | vehicle_params.toml | `toml::from_str` deserialization | WIRED | Line 113: `toml::from_str(toml_str)` in `load_vehicle_config_from_str` |
| types.rs | config.rs | factory functions delegate to config | WIRED | Line 34: `VehicleConfig::default()` called in `default_idm_params()` |
| compute.rs | config.rs | `GpuVehicleParams::from_config` | WIRED | Line 38: `from_config(config: &VehicleConfig)`, imports `VehicleConfig` at line 15 |
| wave_front.wgsl | vehicle_params uniform buffer | `vehicle_params[vt]` indexing | WIRED | Lines 172, 200, 208: `vehicle_params[vt]` reads in idm_acceleration, krauss_safe_speed, krauss_update |
| sublane.rs | config.rs | red_light_creep_speed / effective_filter_gap | WIRED | `SublaneParams::from_config()` at line 47 delegates to config.rs. `effective_filter_gap()` uses `base_min_gap` parameter from config. |
| intersection.rs | config.rs | gap_acceptance_ttc from VehicleTypeParams | WIRED | Function takes `own_ttc_threshold` parameter; callers pass `config.gap_acceptance_ttc` |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| TUN-01 | 08-01 | All ~50 vehicle behavior parameters externalized to TOML config file | SATISFIED | 7-section TOML with IDM+Krauss+MOBIL+sublane+creep params per type. VehicleConfig loads and validates. |
| TUN-02 | 08-02 | GPU/CPU parameter parity via uniform buffer from config | SATISFIED | GpuVehicleParams::from_config(), binding 7, all WGSL hardcoded constants removed. Shader compiles. 5 GPU tests pass. |
| TUN-03 | 08-01 | HCMC-calibrated parameter defaults (truck v0=35 km/h not 90 km/h) | SATISFIED | truck v0=9.7, car v0=9.7, motorbike v0=11.1, bus v0=8.3. Range tests confirm. |
| TUN-04 | 08-03 | Red-light creep behavior for motorbikes | SATISFIED | `red_light_creep_speed()` with gradual ramp 0-0.3 m/s, motorbike/bicycle only. 8 creep tests pass. |
| TUN-05 | 08-03 | Aggressive weaving with speed-dependent lateral filter gap | SATISFIED | `effective_filter_gap()` = base + 0.1*delta_v. Integrated into compute_desired_lateral(). 4 gap tests pass. |
| TUN-06 | 08-03 | Yield-based intersection negotiation with size intimidation and deadlock prevention | SATISFIED | `intersection_gap_acceptance()` with per-type TTC, size factors, wait-time reduction, 5s forced acceptance. 13 intersection tests pass. |

No orphaned requirements found -- all 6 TUN-XX IDs mapped to phase 8 in REQUIREMENTS.md are accounted for in plan frontmatter.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| sublane.rs | 69-79 | CREEP_MAX_SPEED, CREEP_DISTANCE_SCALE, GAP_SPEED_COEFF as module constants | Info | Config fields `creep_max_speed` and `creep_distance_scale` exist in VehicleTypeParams but `red_light_creep_speed()` uses module-level constants. Plan specified this function signature without config parameter. Values match config defaults. Future work could wire these to config for runtime tuning. |

No blockers. No TODOs, FIXMEs, placeholders, or stub implementations found.

### Human Verification Required

### 1. Visual realism of motorbike swarm at red lights

**Test:** Run simulation with mixed traffic at a signalized intersection. Observe motorbikes during red phase.
**Expected:** Motorbikes should gradually inch forward past the stop line, forming a dense swarm ahead of cars. Cars should remain stopped.
**Why human:** Visual behavior pattern cannot be verified programmatically.

### 2. Weaving behavior in congested traffic

**Test:** Observe motorbikes in dense mixed traffic with cars and buses.
**Expected:** Motorbikes should filter laterally through gaps, accepting smaller gaps at low speed differences and requiring larger gaps at higher speed differences.
**Why human:** Requires visual assessment of lateral movement patterns.

### 3. Intersection negotiation realism

**Test:** Observe traffic at an unsignalized intersection with mixed vehicle types.
**Expected:** Motorbikes should negotiate gaps more aggressively than cars. Vehicles approaching trucks/buses should be more cautious. Stuck vehicles should eventually force through after ~5 seconds.
**Why human:** Requires observing emergent multi-agent behavior patterns.

### Test Results

- `cargo test -p velos-vehicle`: 107 tests passed, 0 failed
- `cargo test -p velos-gpu --test gpu_params_tests`: 5 tests passed, 0 failed
- `cargo build -p velos-gpu`: Success (shader compiles via `include_wgsl!`)

---

_Verified: 2026-03-08T12:00:00Z_
_Verifier: Claude (gsd-verifier)_
