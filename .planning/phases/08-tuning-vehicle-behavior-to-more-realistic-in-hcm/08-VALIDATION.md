---
phase: 8
slug: tuning-vehicle-behavior-to-more-realistic-in-hcm
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-08
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test -p velos-vehicle` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-vehicle && cargo test -p velos-gpu`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 08-01-01 | 01 | 0 | TOML config load | unit | `cargo test -p velos-vehicle config` | ✅ config_tests.rs (16) | ✅ green |
| 08-01-02 | 01 | 0 | GPU/CPU parity | unit | `cargo test -p velos-gpu --tests gpu_params` | ✅ gpu_params_tests.rs (5) | ✅ green |
| 08-01-03 | 01 | 1 | IDM with config params | unit | `cargo test -p velos-vehicle idm` | ✅ idm_tests.rs | ✅ green |
| 08-01-04 | 01 | 1 | Krauss per-type sigma | unit | `cargo test -p velos-vehicle krauss` | ✅ krauss_tests.rs | ✅ green |
| 08-02-01 | 02 | 2 | Red-light creep | unit | `cargo test -p velos-vehicle sublane` | ✅ sublane_tests.rs (20) | ✅ green |
| 08-02-02 | 02 | 2 | Aggressive weaving | unit | `cargo test -p velos-vehicle sublane` | ✅ sublane_tests.rs | ✅ green |
| 08-02-03 | 02 | 2 | Intersection gap accept | unit | `cargo test -p velos-vehicle intersection` | ✅ intersection_tests.rs (13) | ✅ green |
| 08-03-01 | 03 | 1 | Parameter ranges | unit | `cargo test -p velos-vehicle config` | ✅ config_tests.rs (validation) | ✅ green |
| 08-03-02 | 03 | 1 | Truck v0 corrected | unit | `cargo test -p velos-vehicle types` | ✅ types_tests.rs | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-vehicle/src/config.rs` — VehicleConfig struct with TOML deserialization
- [x] `crates/velos-vehicle/tests/config_tests.rs` — config loading, validation, default fallback
- [x] `data/hcmc/vehicle_params.toml` — HCMC-calibrated parameter defaults
- [x] `crates/velos-vehicle/tests/intersection_tests.rs` — gap acceptance logic tests
- [x] Update `crates/velos-vehicle/tests/types_tests.rs` — assertions for new HCMC defaults
- [x] Update `crates/velos-vehicle/tests/sublane_tests.rs` — aggressive weaving thresholds

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Red-light creep visual | Motorbikes visually inch forward | Requires visual simulation output | Run sim at signalized intersection, observe motorbike positions during red phase |
| Weaving realism | Motorbikes squeeze between cars | Emergent behavior from params | Run sim on mixed-traffic road, watch sublane filtering patterns |
| Speed distributions | v0 ranges match HCMC targets | Statistical validation on sim output | Run 1000-agent sim, histogram speeds by vehicle type |

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
