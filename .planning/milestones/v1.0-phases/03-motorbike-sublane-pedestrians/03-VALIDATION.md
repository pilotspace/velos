---
phase: 3
slug: motorbike-sublane-pedestrians
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-06
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml workspace [workspace] members |
| **Quick run command** | `cargo test -p velos-vehicle --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-vehicle --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | VEH-03 | unit | `cargo test -p velos-vehicle sublane -- --exact` | ✅ | ✅ green |
| 03-01-02 | 01 | 1 | VEH-03 | unit | `cargo test -p velos-vehicle sublane::drift -- --exact` | ✅ | ✅ green |
| 03-01-03 | 01 | 1 | VEH-03 | unit | `cargo test -p velos-vehicle sublane::dt_consistency` | ✅ | ✅ green |
| 03-01-04 | 01 | 1 | VEH-03 | integration | `cargo test -p velos-gpu swarming` | ✅ | ✅ green |
| 03-01-05 | 01 | 1 | VEH-03 | integration | `cargo test -p velos-gpu dispersal` | ✅ | ✅ green |
| 03-02-01 | 02 | 1 | VEH-04 | unit | `cargo test -p velos-vehicle social_force::repulsion` | ✅ | ✅ green |
| 03-02-02 | 02 | 1 | VEH-04 | unit | `cargo test -p velos-vehicle social_force::driving` | ✅ | ✅ green |
| 03-02-03 | 02 | 1 | VEH-04 | unit | `cargo test -p velos-vehicle social_force::anisotropy` | ✅ | ✅ green |
| 03-02-04 | 02 | 1 | VEH-04 | unit | `cargo test -p velos-vehicle social_force::jaywalking` | ✅ | ✅ green |
| 03-02-05 | 02 | 1 | VEH-04 | unit | `cargo test -p velos-vehicle social_force::clamp` | ✅ | ✅ green |
| 03-MIX-01 | 01+02 | 2 | VEH-03+VEH-04 | integration | `cargo test -p velos-gpu mixed_intersection` | ✅ | ✅ green |

*Status: ✅ green · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-vehicle/tests/sublane_tests.rs` — stubs for VEH-03 (gap detection, drift clamping, dt-consistency)
- [x] `crates/velos-vehicle/tests/social_force_tests.rs` — stubs for VEH-04 (repulsion, driving force, anisotropy, jaywalking, clamp)
- [x] `crates/velos-vehicle/src/sublane.rs` — new module (motorbike lateral model)
- [x] `crates/velos-vehicle/src/social_force.rs` — new module (Helbing pedestrian model)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Swarming visual cue (color shift) | VEH-03 | Visual rendering check | Run simulation, observe motorbikes at red light turn brighter green |
| Motorbike free-form intersection crossing | VEH-03 | Complex spatial behavior | Run simulation, observe motorbikes taking shortest path through intersection |
| Aggressive left turns into oncoming | VEH-03 | Complex multi-agent behavior | Run simulation, observe motorbikes drifting into oncoming lane for left turns |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 15s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved (retroactive — all tests pass, phase complete)
