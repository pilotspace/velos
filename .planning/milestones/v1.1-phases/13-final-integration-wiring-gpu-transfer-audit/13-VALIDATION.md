---
phase: 13
slug: final-integration-wiring-gpu-transfer-audit
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-08
---

# Phase 13 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test -p velos-gpu --lib && cargo test -p velos-core --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-gpu --lib && cargo test -p velos-core --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 13-01-01 | 01 | 1 | INT-01, INT-02 | unit | `cargo test -p velos-gpu -- sim_lifecycle` | ✅ | ✅ green |
| 13-01-02 | 01 | 1 | INT-01 | unit | `cargo test -p velos-core -- cost::tests` | ✅ | ✅ green |
| 13-02-01 | 02 | 1 | SIG-03 | unit | `cargo test -p velos-gpu -- sim_helpers::tests::glosa` | ✅ | ✅ green |
| 13-03-01 | 03 | 1 | AGT-04 | integration | `cargo test -p velos-gpu -- sim_pedestrians` | ✅ | ✅ green |
| 13-04-01 | 04 | 2 | N/A | unit | `cargo test -p velos-gpu -- sim::tests::cpu_tick_parity` | ✅ | ✅ green |
| 13-04-02 | 04 | 2 | N/A | unit | `cargo test -p velos-gpu -- sim_perception::tests::signal_dirty` | ✅ | ✅ green |
| 13-04-03 | 04 | 2 | N/A | unit | `cargo test -p velos-gpu -- sim_perception::tests::edge_ratio_dirty` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-gpu/src/sim_lifecycle.rs` — test for profile encoding in spawn_single_agent (tests: spawn_bus_agent_has_bus_profile, spawn_emergency_agent_has_emergency_profile, spawn_motorbike_with_tourist_profile, spawn_pedestrian_has_profile)
- [x] `crates/velos-gpu/src/sim.rs` — test for CPU tick parity (tests: cpu_tick_parity_lane_changes_called, cpu_tick_parity_pipeline_order); GLOSA tests in sim_helpers.rs (tests: glosa_*)
- [x] `crates/velos-gpu/src/sim_pedestrians.rs` — test for GPU pedestrian dispatch activation (3 tests in mod tests)
- [x] `crates/velos-gpu/src/sim_perception.rs` — tests for dirty-flag buffer optimizations (tests: signal_dirty_initialized_true, signal_dirty_stays_false_without_phase_change, signal_dirty_set_true_on_phase_transition, prediction_dirty_stays_false_without_update)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| GPU pedestrian visual correctness | AGT-04 | Visual validation of crowd movement | Run sim with deck.gl dashboard, observe pedestrian movement at intersections |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** complete -- validated during Phase 15 housekeeping (2026-03-08)
