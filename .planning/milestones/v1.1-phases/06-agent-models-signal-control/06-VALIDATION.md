---
phase: 6
slug: agent-models-signal-control
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-07
audited: 2026-03-08
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml [workspace] |
| **Quick run command** | `cargo test --lib -p velos-vehicle -p velos-signal -p velos-demand -p velos-meso -p velos-core` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib -p <affected-crate>`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 06-01-01 | 01 | 1 | AGT-01 | unit | `cargo test -p velos-vehicle --test bus_tests` | `crates/velos-vehicle/tests/bus_tests.rs` (10 tests) | ✅ green |
| 06-01-02 | 01 | 1 | AGT-02 | unit | `cargo test -p velos-demand --test gtfs_tests` | `crates/velos-demand/tests/gtfs_tests.rs` (6 tests) | ✅ green |
| 06-01-03 | 01 | 1 | AGT-03 | unit | `cargo test -p velos-vehicle --test types_tests -- bicycle` | `crates/velos-vehicle/tests/types_tests.rs` (bicycle IDM params) | ✅ green |
| 06-01-04 | 01 | 1 | AGT-07 | unit | `cargo test -p velos-vehicle --test types_tests -- truck` | `crates/velos-vehicle/tests/types_tests.rs` (truck IDM params) | ✅ green |
| 06-02-01 | 02 | 1 | SIG-01 | unit | `cargo test -p velos-signal --test actuated_tests` | `crates/velos-signal/tests/actuated_tests.rs` (8 tests) | ✅ green |
| 06-02-02 | 02 | 1 | SIG-02 | unit | `cargo test -p velos-signal --test adaptive_tests` | `crates/velos-signal/tests/adaptive_tests.rs` (7 tests) | ✅ green |
| 06-02-03 | 02 | 1 | SIG-03 | unit | `cargo test -p velos-signal --test spat_tests` | `crates/velos-signal/tests/spat_tests.rs` (6 tests) | ✅ green |
| 06-02-04 | 02 | 1 | SIG-04 | unit | `cargo test -p velos-signal --test priority_tests` | `crates/velos-signal/tests/priority_tests.rs` (6 tests) | ✅ green |
| 06-03-01 | 03 | 2 | AGT-04 | integration | `cargo test -p velos-gpu --test pedestrian_adaptive_tests --features gpu-tests` | `crates/velos-gpu/tests/pedestrian_adaptive_tests.rs` (4 tests, gpu-gated) | ✅ green |
| 06-03-02 | 03 | 2 | AGT-08 | integration | `cargo test -p velos-vehicle --test emergency_tests` | `crates/velos-vehicle/tests/emergency_tests.rs` (11 tests) | ✅ green |
| 06-03-03 | 03 | 2 | SIG-05 | integration | `cargo test -p velos-signal --test signs_tests` | `crates/velos-signal/tests/signs_tests.rs` (17 tests) | ✅ green |
| 06-04-01 | 04 | 2 | AGT-05 | unit | `cargo test -p velos-meso --test buffer_zone_tests` | `crates/velos-meso/tests/buffer_zone_tests.rs` (15 tests) | ✅ green |
| 06-04-02 | 04 | 2 | AGT-06 | unit | `cargo test -p velos-meso --test queue_model_tests` | `crates/velos-meso/tests/queue_model_tests.rs` (12 tests) | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-vehicle/src/bus.rs` + `crates/velos-vehicle/tests/bus_tests.rs` — bus dwell model
- [x] `crates/velos-demand/src/gtfs.rs` + `crates/velos-demand/tests/gtfs_tests.rs` — GTFS parsing
- [x] `crates/velos-meso/` — entire new crate (lib.rs, queue_model.rs, buffer_zone.rs, zone_config.rs + tests)
- [x] `crates/velos-signal/src/actuated.rs` + tests — actuated controller
- [x] `crates/velos-signal/src/adaptive.rs` + tests — adaptive controller
- [x] `crates/velos-signal/src/priority.rs` + tests — priority request handling
- [x] `crates/velos-signal/src/spat.rs` + tests — SPaT broadcast
- [x] `crates/velos-signal/src/signs.rs` + tests — traffic sign component
- [x] `crates/velos-signal/src/detector.rs` + tests — loop detector
- [x] `crates/velos-vehicle/src/emergency.rs` + tests — emergency vehicle logic
- [x] GTFS test fixture file (minimal route/stop/stop_times for 1-2 routes)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Bus visible passenger boarding delay | AGT-01 SC1 | Visual timing judgment | Run GTFS route scenario, observe bus dwell animation at stops |
| Emergency vehicle yield behavior | AGT-08 SC2 | Multi-agent spatial interaction | Spawn emergency vehicle, verify surrounding agents yield visually |
| Speed discontinuity at buffer zones | AGT-05 SC5 | Smooth interpolation visual check | Drive agent through meso-micro boundary, verify no speed jumps |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved

---

## Validation Audit 2026-03-08

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |

**Summary:** All 13 tasks have automated tests. 102 total tests across 13 test files, all green. Pedestrian adaptive GPU tests are feature-gated behind `gpu-tests` (requires GPU hardware). No validation gaps detected.
