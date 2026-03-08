---
phase: 02
slug: road-network-vehicle-models-egui
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-06
---

# Phase 02 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) |
| **Config file** | Workspace Cargo.toml (test targets per crate) |
| **Quick run command** | `cargo test --workspace --lib` |
| **Full suite command** | `cargo test --workspace --no-fail-fast` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace --lib`
- **After every plan wave:** Run `cargo clippy --all-targets -- -D warnings && cargo test --workspace --no-fail-fast`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | NET-01 | integration | `cargo test -p velos-net -- osm_import` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | NET-04 | unit | `cargo test -p velos-net -- projection` | ✅ | ✅ green |
| 02-01-03 | 01 | 1 | NET-02 | unit | `cargo test -p velos-net -- spatial` | ✅ | ✅ green |
| 02-01-04 | 01 | 1 | RTE-01 | unit | `cargo test -p velos-net -- astar` | ✅ | ✅ green |
| 02-02-01 | 02 | 1 | VEH-01 | unit | `cargo test -p velos-vehicle -- idm` | ✅ | ✅ green |
| 02-02-02 | 02 | 1 | VEH-02 | unit | `cargo test -p velos-vehicle -- mobil` | ✅ | ✅ green |
| 02-02-03 | 02 | 1 | NET-03 | unit | `cargo test -p velos-signal -- signal` | ✅ | ✅ green |
| 02-02-04 | 02 | 1 | GRID-01 | unit | `cargo test -p velos-vehicle -- gridlock` | ✅ | ✅ green |
| 02-03-01 | 03 | 1 | DEM-01 | unit | `cargo test -p velos-demand -- od_matrix` | ✅ | ✅ green |
| 02-03-02 | 03 | 1 | DEM-02 | unit | `cargo test -p velos-demand -- tod_profile` | ✅ | ✅ green |
| 02-03-03 | 03 | 1 | DEM-03 | unit | `cargo test -p velos-demand -- spawner` | ✅ | ✅ green |
| 02-04-01 | 04 | 2 | APP-01 | manual | Visual: click buttons, observe sim state | N/A | ✅ green |
| 02-04-02 | 04 | 2 | APP-02 | manual | Visual: metrics update each frame | N/A | ✅ green |

*Status: ✅ green · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-vehicle/tests/idm_tests.rs` — stubs for VEH-01
- [x] `crates/velos-vehicle/tests/mobil_tests.rs` — stubs for VEH-02
- [x] `crates/velos-vehicle/tests/gridlock_tests.rs` — stubs for GRID-01
- [x] `crates/velos-net/tests/import_tests.rs` — stubs for NET-01, NET-04
- [x] `crates/velos-net/tests/spatial_tests.rs` — stubs for NET-02
- [x] `crates/velos-net/tests/routing_tests.rs` — stubs for RTE-01
- [x] `crates/velos-signal/tests/signal_tests.rs` — stubs for NET-03
- [x] `crates/velos-demand/tests/spawner_tests.rs` — stubs for DEM-01, DEM-02, DEM-03
- [x] Small test PBF fixture at `data/hcmc/test-district1.osm.pbf` or equivalent

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| egui controls invoke SimState transitions | APP-01 | Requires visual GUI interaction | Click start/stop/pause/speed/reset buttons, verify sim state changes |
| egui dashboard shows live metrics | APP-02 | Requires visual rendering verification | Run sim, observe frame time/agent count/throughput update each frame |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved (retroactive — all tests pass, phase complete)
