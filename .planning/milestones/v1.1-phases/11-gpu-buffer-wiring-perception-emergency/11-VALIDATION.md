---
phase: 11
slug: gpu-buffer-wiring-perception-emergency
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-08
---

# Phase 11 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + cargo test |
| **Config file** | Cargo.toml (workspace) |
| **Quick run command** | `cargo test -p velos-gpu --lib -- --test-threads=1` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-gpu --lib -- --test-threads=1`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 11-01-01 | 01 | 1 | INT-03 | integration | `cargo test -p velos-gpu --test integration_perception_wiring` | ✅ integration_perception_wiring.rs | ✅ green |
| 11-01-02 | 01 | 1 | TUN-04 | unit | `cargo test -p velos-gpu --lib` | ✅ compute.rs lib tests (65) | ✅ green |
| 11-01-03 | 01 | 1 | TUN-06 | unit | `cargo test -p velos-gpu --lib` | ✅ compute.rs lib tests | ✅ green |
| 11-02-01 | 02 | 1 | AGT-08 | integration | `cargo test -p velos-gpu --test integration_emergency_wiring` | ✅ integration_emergency_wiring.rs (6) | ✅ green |
| 11-02-02 | 02 | 1 | AGT-08 | unit | `cargo test -p velos-gpu --lib -- flag_emergency` | ✅ compute.rs (4 flag tests) | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-gpu/tests/integration_perception_wiring.rs` — verify set_perception_result_buffer called, perception data flows to wave_front
- [x] `crates/velos-gpu/tests/integration_emergency_wiring.rs` — verify upload_emergency_vehicles called, FLAG_EMERGENCY_ACTIVE set, emergency_count > 0
- [x] Unit test in `compute.rs`: verify FLAG_EMERGENCY_ACTIVE is set for Emergency VehicleType
- [x] Unit test: verify emergency vehicle world position computation from edge geometry

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Visual yield cone activation | AGT-08 | GPU visual verification | Run sim with emergency vehicle, observe surrounding agents slowing/yielding |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved

## Validation Audit 2026-03-08

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |
