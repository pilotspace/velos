---
phase: 5
slug: foundation-gpu-engine
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-07
validated: 2026-03-08
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) + criterion 0.5 (benchmarks) |
| **Config file** | Cargo.toml workspace test configuration |
| **Quick run command** | `cargo test --workspace -q` |
| **Full suite command** | `cargo test --workspace --no-fail-fast && cargo bench --workspace -- --baseline main` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace -q`
- **After every plan wave:** Run `cargo test --workspace --no-fail-fast`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 1 | GPU-01 | integration | `cargo test -p velos-gpu --test gpu_physics` | ✅ | ✅ green |
| 05-01-02 | 01 | 1 | GPU-03 | integration | `cargo test -p velos-gpu --test wave_front_validation` | ✅ | ✅ green |
| 05-01-03 | 01 | 1 | GPU-04 | unit | `cargo test -p velos-core --test fixed_point_tests` | ✅ | ✅ green |
| 05-02-01 | 02 | 1 | GPU-02 | unit | `cargo test -p velos-gpu --test boundary_protocol_tests` | ✅ | ✅ green |
| 05-02-02 | 02 | 1 | GPU-05 | integration | `cargo test -p velos-gpu --test boundary_protocol_tests` | ✅ | ✅ green |
| 05-02-03 | 02 | 1 | GPU-06 | benchmark | `cargo bench -p velos-gpu --bench dispatch` | ✅ | ✅ green |
| 05-03-01 | 03 | 1 | NET-01 | integration | `cargo test -p velos-net --test import_tests` | ✅ | ✅ green |
| 05-03-02 | 03 | 1 | NET-02 | unit | `cargo test -p velos-net --test cleaning_tests` | ✅ | ✅ green |
| 05-03-03 | 03 | 1 | NET-03 | unit | `cargo test -p velos-net --test hcmc_rules_tests` | ✅ | ✅ green |
| 05-03-04 | 03 | 1 | NET-04 | unit | `cargo test -p velos-demand --test tod_5district` | ✅ | ✅ green |
| 05-04-01 | 04 | 1 | NET-05 | integration | `cargo test -p velos-net --test sumo_net_import` | ✅ | ✅ green |
| 05-04-02 | 04 | 1 | NET-06 | integration | `cargo test -p velos-net --test sumo_rou_import` | ✅ | ✅ green |
| 05-05-01 | 05 | 2 | CFM-01 | unit | `cargo test -p velos-vehicle --test krauss_tests` | ✅ | ✅ green |
| 05-05-02 | 05 | 2 | CFM-02 | integration | `cargo test -p velos-gpu --test cf_model_switch` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-core/src/fixed_point.rs` — Q16.16, Q12.20, Q8.8 types with tests
- [x] `crates/velos-vehicle/src/krauss.rs` — Krauss model CPU implementation with tests
- [x] `crates/velos-vehicle/tests/krauss_tests.rs` — Krauss vs SUMO reference values
- [x] `crates/velos-net/src/sumo_import.rs` — SUMO .net.xml + .rou.xml parser
- [x] `crates/velos-net/tests/sumo_net_import.rs` — test with sample .net.xml fixture
- [x] `crates/velos-net/src/cleaning.rs` — graph cleaning pipeline
- [x] `crates/velos-net/tests/cleaning_tests.rs` — cleaning unit tests
- [x] `crates/velos-gpu/shaders/wave_front.wgsl` — wave-front dispatch shader
- [x] `tests/fixtures/simple.net.xml` — small SUMO .net.xml (3 junctions, 4+2 edges)
- [x] `tests/fixtures/simple.rou.xml` — small SUMO .rou.xml matching the .net.xml

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Agents color-coded by CF model in egui | CFM-02 | Visual inspection | Run sim, toggle agent between IDM/Krauss, verify color changes |
| 280K agents sustain 10 steps/sec | GPU-06 | Hardware-dependent | Run benchmark on target hardware, verify frame times |

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
| Compile fix | 1 (cleaning.rs unqualified RoadEdge in test helper) |

**Notes:**
- All 14 task requirements have automated test coverage
- Fixed 1 compile error: `cleaning.rs:322` used unqualified `RoadEdge` in `#[cfg(test)]` helper — qualified to `crate::graph::RoadEdge`
- VALIDATION.md commands corrected: test files live in `velos-gpu`/`velos-core`/`velos-net`/`velos-vehicle`/`velos-demand` (not `velos-sim`)
- All workspace tests pass: `cargo test --workspace` green
