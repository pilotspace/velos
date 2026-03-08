---
phase: 14
slug: wire-gtfs-bus-stops-pipeline
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-08
---

# Phase 14 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) |
| **Config file** | Cargo.toml per crate |
| **Quick run command** | `cargo test -p velos-net --lib snap && cargo test -p velos-demand --lib bus_spawner` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-net --lib snap && cargo test -p velos-demand --lib bus_spawner && cargo test -p velos-gpu --lib sim_startup`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 14-01-01 | 01 | 1 | AGT-02 | unit | `cargo test -p velos-net --lib snap` | ❌ W0 | ⬜ pending |
| 14-01-02 | 01 | 1 | AGT-02 | unit | `cargo test -p velos-net --lib snap` | ❌ W0 | ⬜ pending |
| 14-01-03 | 01 | 1 | AGT-02 | unit | `cargo test -p velos-net --lib snap` | ❌ W0 | ⬜ pending |
| 14-02-01 | 02 | 1 | AGT-02 | unit | `cargo test -p velos-demand --lib bus_spawner` | ❌ W0 | ⬜ pending |
| 14-02-02 | 02 | 1 | AGT-02 | unit | `cargo test -p velos-demand --lib bus_spawner` | ❌ W0 | ⬜ pending |
| 14-03-01 | 03 | 2 | AGT-01 | unit | `cargo test -p velos-gpu --lib sim_startup` | ❌ W0 | ⬜ pending |
| 14-03-02 | 03 | 2 | AGT-01, AGT-02 | integration | `cargo test -p velos-gpu --lib sim -- gtfs` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/velos-net/src/snap.rs` + tests — edge R-tree, snap_to_nearest_edge, projection logic
- [ ] `crates/velos-demand/src/bus_spawner.rs` + tests — BusSpawner, time-gated generation, route tables
- [ ] `crates/velos-gpu/src/sim_startup.rs` tests — load_gtfs_bus_stops graceful degradation
- [ ] `crates/velos-gpu/src/sim.rs` integration test — E2E bus dwell with GTFS stops

---

## Manual-Only Verifications

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
