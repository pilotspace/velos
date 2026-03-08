---
phase: 10
slug: sim-loop-integration-bus-dwell-meso-micro
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-08
---

# Phase 10 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test + cargo test |
| **Config file** | Cargo.toml per-crate |
| **Quick run command** | `cargo test -p velos-gpu --lib && cargo test -p velos-meso` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-gpu --lib && cargo test -p velos-meso`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 10-01-01 | 01 | 1 | AGT-01 | integration | `cargo test -p velos-gpu --test integration_bus_dwell` | ✅ integration_bus_dwell.rs (6) | ✅ green |
| 10-01-02 | 01 | 1 | AGT-01 | unit (WGSL) | `cargo test -p velos-gpu --test wave_front_validation` | ✅ wave_front_validation.rs | ✅ green |
| 10-02-01 | 02 | 1 | AGT-06 | integration | `cargo test -p velos-gpu --test integration_meso_micro` | ✅ integration_meso_micro.rs (6) | ✅ green |
| 10-02-02 | 02 | 1 | AGT-06 | unit | `cargo test -p velos-meso` | ✅ buffer_zone_tests.rs + queue_model_tests.rs | ✅ green |
| 10-03-01 | 03 | 2 | AGT-05 | integration | `cargo test -p velos-gpu --test integration_meso_micro` | ✅ integration_meso_micro.rs | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-gpu/tests/integration_bus_dwell.rs` — 6 integration tests for AGT-01 (bus dwell lifecycle)
- [x] `crates/velos-gpu/tests/integration_meso_micro.rs` — 6 integration tests for AGT-05, AGT-06 (meso-micro transitions)
- [x] WGSL test case for FLAG_BUS_DWELLING guard in `wave_front_validation.rs`
- [x] velos-meso dependency in velos-gpu/Cargo.toml — added

*Existing infrastructure covers ZoneConfig unit tests (velos-meso).*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| No visual speed discontinuity at meso-micro boundary | AGT-05 | Visual smoothness hard to unit-test beyond threshold checks | Run sim with meso zones enabled, observe agent speed traces across boundaries — speed should change smoothly over 100m |

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
