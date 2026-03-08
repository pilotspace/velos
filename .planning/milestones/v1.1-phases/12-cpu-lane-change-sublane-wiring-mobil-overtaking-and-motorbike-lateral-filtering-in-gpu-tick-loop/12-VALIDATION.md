---
phase: 12
slug: cpu-lane-change-sublane-wiring-mobil-overtaking-and-motorbike-lateral-filtering-in-gpu-tick-loop
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-08
---

# Phase 12 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + `cargo test` |
| **Config file** | `crates/velos-gpu/Cargo.toml` |
| **Quick run command** | `cargo test -p velos-gpu` |
| **Full suite command** | `cargo test -p velos-gpu` |
| **Estimated runtime** | ~10 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-gpu`
- **After every plan wave:** Run `cargo test -p velos-gpu`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 12-01-01 | 01 | 1 | TUN-04 | unit | `cargo test -p velos-gpu gpu_vehicle_params` | ✅ | ✅ green |
| 12-01-01 | 01 | 1 | TUN-04 | unit | `cargo test -p velos-gpu creep_speed` | ✅ | ✅ green |
| 12-01-01 | 01 | 1 | TUN-06 | unit | `cargo test -p velos-gpu gap_acceptance` | ✅ | ✅ green |
| 12-02-01 | 02 | 2 | RTE-05, RTE-07 | unit | `cargo test -p velos-gpu step_prediction` | ✅ | ✅ green |
| 12-02-01 | 02 | 2 | TBD (lane-change) | unit | `cargo test -p velos-gpu step_lane_changes` | ✅ | ✅ green |
| 12-02-02 | 02 | 2 | RTE-05 | integration | `cargo test -p velos-gpu test_prediction_overlay_updates` | ✅ | ✅ green |
| 12-02-02 | 02 | 2 | TBD (lane-change) | integration | `cargo test -p velos-gpu test_mobil_triggers_lane_change` | ✅ | ✅ green |
| 12-02-02 | 02 | 2 | TBD (lane-change) | integration | `cargo test -p velos-gpu test_lane_change_completes_after_drift` | ✅ | ✅ green |
| 12-02-02 | 02 | 2 | TBD (lane-change) | integration | `cargo test -p velos-gpu test_single_lane_no_mobil` | ✅ | ✅ green |
| 12-02-02 | 02 | 2 | TBD (lane-change) | integration | `cargo test -p velos-gpu test_motorbike_sublane_adjusts_lateral` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Requirement Coverage Detail

### TUN-04: Red-light creep params → GPU uniform buffer

| Test | File | Behavior Verified |
|------|------|-------------------|
| `creep_speed_motorbike_normal_distance` | `compute.rs` | Motorbike creep at normal distance returns expected speed |
| `creep_speed_car_returns_zero` | `compute.rs` | Car (non-sublane) creep returns 0 |
| `creep_speed_too_close_returns_zero` | `compute.rs` | Below min distance returns 0 |
| `creep_speed_bicycle_also_creeps` | `compute.rs` | Bicycle creep behavior works |
| `creep_speed_far_distance_capped` | `compute.rs` | Creep speed capped at max |
| `motorbike_creep_max_speed_matches_config` | `gpu_vehicle_params.rs` | GPU struct maps creep_max_speed from TOML |
| `motorbike_creep_distance_scale` | `gpu_vehicle_params.rs` | GPU struct maps creep_distance_scale from TOML |

### TUN-06: Gap acceptance TTC → GPU uniform buffer

| Test | File | Behavior Verified |
|------|------|-------------------|
| `gap_acceptance_large_ttc_accepts` | `compute.rs` | Large TTC gap accepted |
| `gap_acceptance_small_ttc_rejects` | `compute.rs` | Small TTC gap rejected |
| `gap_acceptance_emergency_needs_larger_gap` | `compute.rs` | Emergency vehicles need larger gap |
| `gap_acceptance_forced_after_max_wait` | `compute.rs` | Gap acceptance forced after max wait time |
| `car_gap_acceptance_ttc_matches_config` | `gpu_vehicle_params.rs` | GPU struct maps gap_acceptance_ttc from TOML |

### RTE-05: Prediction overlay ArcSwap refresh

| Test | File | Behavior Verified |
|------|------|-------------------|
| `step_prediction_skips_when_not_time` | `sim_reroute.rs` | No update before 60s |
| `step_prediction_updates_when_time_elapsed` | `sim_reroute.rs` | Updates with actual edge data after 60s |
| `test_prediction_overlay_updates` | `lane_change_integration.rs` | Full integration: overlay timestamp updates |

### RTE-07: Prediction-informed routing

| Test | File | Behavior Verified |
|------|------|-------------------|
| `step_prediction_updates_when_time_elapsed` | `sim_reroute.rs` | Passes actual edge flows/capacities to PredictionService |

### TBD (lane-change): MOBIL + sublane in tick_gpu

| Test | File | Behavior Verified |
|------|------|-------------------|
| `step_lane_changes_triggers_mobil_for_car_behind_slow_leader` | `sim_mobil.rs` | MOBIL triggers for blocked car |
| `step_lane_changes_drift_reduces_time_remaining` | `sim_mobil.rs` | Drift timer decrements each tick |
| `test_mobil_triggers_lane_change` | `lane_change_integration.rs` | Full integration: car gets LaneChangeState |
| `test_lane_change_completes_after_drift` | `lane_change_integration.rs` | Lane change completes after 2s drift |
| `test_single_lane_no_mobil` | `lane_change_integration.rs` | Single-lane skips MOBIL |
| `test_motorbike_sublane_adjusts_lateral` | `lane_change_integration.rs` | Motorbike lateral offset changes |

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| MOBIL lane-change visual smoothness | TBD (lane-change) | Visual animation quality | Run simulation with GPU tick on multi-lane road, observe 2s drift |
| Motorbike sublane filtering feel | TBD (lane-change) | Realistic behavior judgment | Observe motorbike weaving in congested traffic |
| Prediction overlay routing impact | RTE-05, RTE-07 | System-level behavior | Run past 60s, observe rerouting decisions |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 10s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-03-08

---

## Validation Audit 2026-03-08

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

All 5 requirements (TUN-04, TUN-06, RTE-05, RTE-07, TBD lane-change) have automated test coverage across 16 unit tests and 5 integration tests. Total: 155 velos-gpu tests passing.
