---
phase: 20-real-time-calibration
verified: 2026-03-11T14:00:00Z
status: passed
score: 10/10 must-haves verified
gaps: []
human_verification:
  - test: "Observe streaming calibration responding to live detection data"
    expected: "OD spawn rates visually change within seconds of window completion (not 300s)"
    why_human: "Requires running simulation with active gRPC detection stream to observe real-time demand adjustments"
  - test: "Verify status indicator colors in egui panel"
    expected: "Green (Calibrating), Gray (Idle), Yellow (Stale), Orange (Paused) display correctly"
    why_human: "Visual color rendering cannot be verified programmatically"
---

# Phase 20: Real-Time Calibration Verification Report

**Phase Goal:** Simulation demand continuously self-corrects from streaming detection data without requiring restart
**Verified:** 2026-03-11T14:00:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Calibration triggers when a new aggregation window completes, not on a fixed 300s timer | VERIFIED | `CALIBRATION_INTERVAL_SECS` removed; `step_calibration()` compares `latest_window.start_ms` vs `last_processed_windows` (sim_calibration.rs:160-181); test `step_calibration_triggers_on_window_change` passes |
| 2 | Minimum 30 sim-second cooldown between recalibrations prevents thrashing | VERIFIED | `CALIBRATION_COOLDOWN_SECS: f64 = 30.0` (line 68); check at line 212; test `step_calibration_returns_early_when_cooldown_not_elapsed` passes |
| 3 | Cameras with observed count < 10 in latest window are skipped from calibration | VERIFIED | `MIN_OBSERVED_THRESHOLD: u32 = 10` (calibration.rs:25); guard at line 131-133; tests `min_observation_threshold_skips_low_observed` and `min_observation_threshold_allows_at_threshold` pass |
| 4 | Cameras with no new data for 3+ consecutive windows decay ratio toward 1.0 | VERIFIED | `decay_toward_baseline()` (calibration.rs:164-172); called at sim_calibration.rs:191,201; tests `decay_toward_baseline_at_3_windows`, `decay_toward_baseline_ratio_below_1_decays_up`, `decay_does_not_overshoot_1_0`, `decay_called_for_cameras_with_unchanged_windows` all pass |
| 5 | Per-step OD factor change is capped at +/-0.2 per recalibration | VERIFIED | `MAX_FACTOR_CHANGE_PER_STEP: f32 = 0.2` (calibration.rs:28); `apply_change_cap()` (calibration.rs:180-193); called before swap at sim_calibration.rs:286; test `apply_change_cap_applied_before_swap` passes |
| 6 | Calibration paused flag prevents overlay updates when set | VERIFIED | `calibration_paused: bool` on SimWorld (sim.rs:220); early return at sim_calibration.rs:123; test `step_calibration_returns_early_when_paused` passes |
| 7 | Late camera connections participate in the next calibration cycle | VERIFIED | Default state has `last_window_start_ms: -1` (calibration.rs:115); any new window with start_ms > -1 triggers; test `late_camera_default_state_participates` passes |
| 8 | No recalibration happens when no detection data arrives | VERIFIED | Window-change detection returns early when no new windows (sim_calibration.rs:186-194); test `step_calibration_returns_early_when_no_new_windows` passes |
| 9 | User can see calibration status (Calibrating/Idle/Stale/Paused) in the egui panel | VERIFIED | Status logic at app_egui.rs:159-167 with colored circles; four states implemented: Paused (orange), Idle (gray), Stale (yellow), Calibrating (green) |
| 10 | User can pause/resume calibration via checkbox toggle | VERIFIED | `ui.checkbox(&mut sim.calibration_paused, "Pause")` at app_egui.rs:174; function takes `&mut SimWorld` (line 137); `calibration_paused` checked at sim_calibration.rs:123 |

**Score:** 10/10 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/velos-api/src/calibration.rs` | Extended CameraCalibrationState with staleness, min observation filter, decay logic, change cap | VERIFIED | `consecutive_stale_windows` (line 102), `last_window_start_ms` (line 105), `MIN_OBSERVED_THRESHOLD` (line 25), `decay_toward_baseline()` (line 164), `apply_change_cap()` (line 180), `MAX_FACTOR_CHANGE_PER_STEP` (line 28) |
| `crates/velos-gpu/src/sim_calibration.rs` | Window-change detection trigger replacing fixed 300s timer | VERIFIED | `CALIBRATION_INTERVAL_SECS` removed; `CALIBRATION_COOLDOWN_SECS = 30.0` (line 68); window comparison at lines 160-181; `step_calibration` exports present |
| `crates/velos-gpu/src/sim.rs` | calibration_paused, last_processed_windows fields on SimWorld | VERIFIED | `calibration_paused: bool` (line 220), `last_processed_windows: HashMap<u32, i64>` (line 224), `last_calibration_poll_time: f64` (line 227); initialized in `new()` (line 350-353), `new_cpu_only()` (line 438-441), cleared in `reset()` (line 496-498) |
| `crates/velos-gpu/src/app_egui.rs` | Enhanced calibration panel with status indicator, per-camera rows, global summary, pause toggle | VERIFIED | Status indicator (line 159-167), pause checkbox (line 174), global summary (line 180-192), per-camera grid with Status column (line 194-235) |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| sim_calibration.rs | aggregator.rs | `latest_window(cam_id).start_ms` comparison | WIRED | Lines 160-163: `aggregator.latest_window(cam_id).map(\|w\| w.start_ms)` compared against `last_processed_windows` |
| sim_calibration.rs | calibration.rs | `compute_calibration_factors` with stability safeguards | WIRED | Line 272: `compute_calibration_factors()` called; line 286: `apply_change_cap()` applied; lines 191,201: `decay_toward_baseline()` called |
| sim.rs | sim_calibration.rs | `calibration_paused` and `last_processed_windows` fields | WIRED | sim.rs declares fields (lines 220-227); sim_calibration.rs reads/writes them (lines 123, 165-168, 293-298) |
| app_egui.rs | sim.rs | reads calibration_paused, calibration_states from SimWorld | WIRED | Line 137: takes `&mut SimWorld`; line 147-151: reads `calibration_states`; line 159: reads `calibration_paused`; line 174: mutates `calibration_paused` |
| app_egui.rs | calibration.rs | reads `consecutive_stale_windows` for staleness display | WIRED | Line 151: `s.consecutive_stale_windows`; lines 220-228: status text based on stale count (Live/Stale/Decaying) |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| CAL-02 | 20-01, 20-02 | System continuously calibrates demand during a running simulation from streaming detection data | SATISFIED | Window-change trigger (not 300s timer), stability safeguards (5 total), egui panel for user visibility, 33 tests passing (23 velos-api + 10 velos-gpu) |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| sim_calibration.rs | 63 | Comment references "old 300s" | Info | Helpful context, not a problem |

No blockers or warnings found. No TODOs, FIXMEs, placeholders, or stub implementations detected.

### Test Results

- **velos-api calibration:** 23/23 passed
- **velos-gpu sim_calibration:** 10/10 passed
- **Total:** 33 tests covering all stability safeguards and trigger logic

### Human Verification Required

### 1. Streaming Calibration Responsiveness

**Test:** Run simulation, start gRPC detection stream, observe demand adjustments
**Expected:** OD spawn rates change within seconds of window completion, not on a 300s timer
**Why human:** Requires running application with live detection data flow

### 2. Egui Panel Visual Correctness

**Test:** Observe status indicator colors and per-camera staleness in the calibration panel
**Expected:** Green (Calibrating), Gray (Idle), Yellow (Stale), Orange (Paused) with correct transitions
**Why human:** Color rendering and UI layout verification is visual

### Gaps Summary

No gaps found. All 10 observable truths verified with code evidence and passing tests. The phase goal -- replacing fixed-interval calibration with event-driven, streaming-aware calibration -- is achieved:

1. **Event-driven trigger:** Window-change detection via `latest_window.start_ms` comparison replaces the removed `CALIBRATION_INTERVAL_SECS = 300.0` constant
2. **5 stability safeguards:** min observation (10), cooldown (30s), staleness decay (3+ windows), change cap (+/-0.2), pause flag
3. **User-facing panel:** Status indicator, per-camera grid with staleness, global summary, pause toggle
4. **33 automated tests** cover all paths including edge cases (late cameras, empty data, cooldown, decay direction)

---

_Verified: 2026-03-11T14:00:00Z_
_Verifier: Claude (gsd-verifier)_
