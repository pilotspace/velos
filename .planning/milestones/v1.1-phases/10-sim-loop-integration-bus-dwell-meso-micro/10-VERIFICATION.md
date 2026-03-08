---
phase: 10-sim-loop-integration-bus-dwell-meso-micro
verified: 2026-03-08T15:00:00Z
status: passed
score: 7/7 must-haves verified
must_haves:
  truths:
    - "Bus agents spawned with BusState component containing route stop indices"
    - "begin_dwell() and tick_dwell() are called each frame for bus agents in tick_gpu()"
    - "FLAG_BUS_DWELLING is set on dwelling buses and cleared when dwell completes"
    - "GPU shader holds dwelling buses at zero speed (skips IDM computation)"
    - "velos-meso is a dependency of velos-gpu -- SpatialQueue and ZoneConfig are imported and used"
    - "SpatialQueue.enter()/try_exit() called on CPU for meso-designated edges each frame"
    - "Agents crossing from meso to micro zones pass through buffer zone with velocity-matched insertion"
  artifacts:
    - path: "crates/velos-gpu/src/sim_bus.rs"
      status: verified
    - path: "crates/velos-gpu/src/sim_meso.rs"
      status: verified
    - path: "crates/velos-gpu/shaders/wave_front.wgsl"
      status: verified
    - path: "crates/velos-gpu/Cargo.toml"
      status: verified
    - path: "crates/velos-gpu/tests/integration_bus_dwell.rs"
      status: verified
    - path: "crates/velos-gpu/tests/integration_meso_micro.rs"
      status: verified
---

# Phase 10: Sim Loop Integration -- Bus Dwell & Meso-Micro Hybrid Verification Report

**Phase Goal:** Bus agents stop at designated stops with realistic dwell times, and peripheral network zones run mesoscopic queue model with smooth micro-meso transitions through buffer zones
**Verified:** 2026-03-08T15:00:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Bus agents spawned with BusState containing route-matched stop indices | VERIFIED | `sim_lifecycle.rs` line 205: `BusState::new(bus_stop_indices)` attached at spawn; stop indices pre-computed from route edges matching `bus_stops` (lines 102-111) |
| 2 | begin_dwell() and tick_dwell() called each frame in tick_gpu() | VERIFIED | `sim.rs` line 397: `self.step_bus_dwell(dt)` after `step_vehicles_gpu`; `sim_bus.rs` lines 78 and 100: calls `begin_dwell()` and `tick_dwell()` |
| 3 | FLAG_BUS_DWELLING set on dwelling buses and cleared when dwell completes | VERIFIED | `sim.rs` line 648: `flags: if bus_state.map_or(false, \|bs\| bs.is_dwelling()) { 1 } else { 0 }`; integration tests confirm set/clear lifecycle |
| 4 | GPU shader holds dwelling buses at zero speed | VERIFIED | `wave_front.wgsl` lines 474-479: dwelling guard sets speed=0, acceleration=0, continues (skips IDM) |
| 5 | velos-meso is a dependency of velos-gpu with SpatialQueue and ZoneConfig used | VERIFIED | `Cargo.toml` line 35: `velos-meso = { path = "../velos-meso" }`; `sim.rs` imports SpatialQueue and ZoneConfig; `sim_meso.rs` calls `queue.try_exit()`, `sim_helpers.rs` calls `queue.enter()` |
| 6 | SpatialQueue.enter()/try_exit() called for meso edges each frame | VERIFIED | `sim_meso.rs` line 51: `queue.try_exit(self.sim_time)` in step_meso(); `sim_helpers.rs` line 350: `queue.enter(meso_vehicle)` in enter_meso_zone() |
| 7 | Agents cross meso-micro with velocity-matched insertion | VERIFIED | `sim_meso.rs` line 108: `velocity_matching_speed(meso_exit_speed, last_micro_speed)` used for insertion; `buffer_zone.rs` returns min of both speeds; gap check at line 172 prevents congested insertion |

**Score:** 7/7 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/velos-gpu/src/sim_bus.rs` | step_bus_dwell() CPU function | VERIFIED | 104 lines, substantive impl with ECS query, stochastic passengers, dwell state machine |
| `crates/velos-gpu/src/sim_meso.rs` | step_meso() + MesoAgentState + zone transitions | VERIFIED | 237 lines, MesoAgentState struct, step_meso(), spawn_from_meso(), check_gap_for_insertion(), find_last_micro_speed(), unit tests |
| `crates/velos-gpu/shaders/wave_front.wgsl` | FLAG_BUS_DWELLING guard | VERIFIED | Lines 474-479: bus dwelling guard with speed=0, accel=0, continue |
| `crates/velos-gpu/Cargo.toml` | velos-meso dependency | VERIFIED | Line 35: `velos-meso = { path = "../velos-meso" }` |
| `crates/velos-gpu/tests/integration_bus_dwell.rs` | Bus dwell integration tests | VERIFIED | 255 lines, 6 tests, all passing |
| `crates/velos-gpu/tests/integration_meso_micro.rs` | Meso-micro integration tests | VERIFIED | 196 lines, 6 tests, all passing |
| `crates/velos-gpu/src/sim_helpers.rs` | Micro-to-meso interception | VERIFIED | enter_meso_zone() at line 303 with state preservation, SpatialQueue entry, route completion for despawn |
| `crates/velos-gpu/src/sim_startup.rs` | load_zone_config() | VERIFIED | Graceful degradation: loads TOML with env override, defaults to all-Micro on missing file |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| sim.rs tick_gpu() | sim_bus.rs | `self.step_bus_dwell(dt)` | WIRED | Line 397, after step_vehicles_gpu |
| sim.rs tick_gpu() | sim_meso.rs | `self.step_meso(dt)` | WIRED | Line 390, before step_vehicles_gpu |
| sim.rs tick() CPU path | sim_bus.rs | `self.step_bus_dwell(dt)` | WIRED | Line 439 |
| sim.rs tick() CPU path | sim_meso.rs | `self.step_meso(dt)` | WIRED | Line 432 |
| sim_bus.rs | velos_vehicle::bus::BusState | ECS query | WIRED | Lines 37-53: queries `(Entity, &BusState, &RoadPosition)` |
| sim_lifecycle.rs | velos_vehicle::bus::BusState | BusState::new() at spawn | WIRED | Line 205: attached to bus entities |
| sim.rs step_vehicles_gpu | BusState.is_dwelling() | GPU flags field | WIRED | Line 648: `bus_state.map_or(false, \|bs\| bs.is_dwelling())` sets flag |
| wave_front.wgsl | FLAG_BUS_DWELLING | dwelling guard | WIRED | Lines 474-479: checks flag, holds speed=0 |
| sim_meso.rs | SpatialQueue | try_exit() | WIRED | Line 51: iterates queues |
| sim_helpers.rs | SpatialQueue | enter() | WIRED | Line 350: inserts MesoVehicle |
| sim_meso.rs | velocity_matching_speed | import | WIRED | Line 13: imports from velos_meso::buffer_zone |
| sim_helpers.rs | MesoAgentState | state preservation | WIRED | Lines 305-327: extracts and preserves full agent identity |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| AGT-01 | 10-01 | Bus agents with empirical dwell time model (5s + 0.5s/boarding + 0.67s/alighting, cap 60s) | SATISFIED | BusDwellModel used in begin_dwell(); step_bus_dwell() calls through full lifecycle; GPU holds at zero speed |
| AGT-05 | 10-02 | Meso-micro hybrid with 100m graduated buffer zone and velocity-matching insertion | SATISFIED | BufferZone with DEFAULT_BUFFER_LENGTH=100m, smoothstep interpolation, velocity_matching_speed() returns min of meso/micro speeds |
| AGT-06 | 10-02 | Mesoscopic queue model (O(1) per edge) for peripheral network zones | SATISFIED | SpatialQueue with BPR travel_time(), enter()/try_exit() per edge, meso_enabled gate, zone_config edge classification |

No orphaned requirements found.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| None | - | - | - | No anti-patterns detected |

No TODOs, FIXMEs, placeholders, or stub implementations found in any phase 10 artifacts.

### Human Verification Required

### 1. Bus Dwell Visual Behavior

**Test:** Run simulation with bus stops configured, observe bus agents stopping and resuming
**Expected:** Buses stop at designated stops, remain stationary for dwell duration, then resume driving
**Why human:** Cannot verify visual behavior and timing feel programmatically

### 2. Meso-Micro Zone Transition Smoothness

**Test:** Enable meso zones, observe agents transitioning between zones
**Expected:** No visible speed jumps or teleportation at zone boundaries; smooth deceleration/acceleration through buffer
**Why human:** Speed discontinuity detection requires visual inspection of agent trajectories

### 3. Full Pipeline Regression

**Test:** Run full simulation for 1000+ ticks with both bus dwell and meso enabled
**Expected:** No panics, no agent identity loss warnings in logs, stable frame rate
**Why human:** Long-running stability requires runtime observation

### Gaps Summary

No gaps found. All 7 observable truths verified with code evidence, all 3 requirements satisfied, all key links wired, all 12 integration tests passing, no anti-patterns detected. Phase goal fully achieved.

---

_Verified: 2026-03-08T15:00:00Z_
_Verifier: Claude (gsd-verifier)_
