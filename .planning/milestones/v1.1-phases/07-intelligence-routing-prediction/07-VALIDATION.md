---
phase: 7
slug: intelligence-routing-prediction
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-07
audited: 2026-03-08
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) |
| **Config file** | Cargo.toml per crate |
| **Quick run command** | `cargo test -p velos-net --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p {affected_crate} --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 07-01-01 | 01 | 1 | INT-01 | unit | `cargo test -p velos-core --lib cost` | Yes (cost.rs: 15 tests) | green |
| 07-01-02 | 01 | 1 | INT-02 | unit | `cargo test -p velos-demand --lib profile` | Yes (profile.rs: 12 tests) | green |
| 07-02-01 | 02 | 1 | RTE-01 | unit+integration | `cargo test -p velos-net --test cch_tests` | Yes (cch_tests.rs: 14 topology tests) | green |
| 07-02-02 | 02 | 1 | RTE-02 | unit | `cargo test -p velos-net --test cch_tests` | Yes (cch_tests.rs: 6 customization tests) | green |
| 07-02-03 | 02 | 1 | RTE-03 | unit | `cargo test -p velos-net --test cch_tests` | Yes (cch_tests.rs: 7 query tests) | green |
| 07-03-01 | 03 | 2 | RTE-04 | unit | `cargo test -p velos-predict` | Yes (ensemble_tests.rs: BPR tests) | green |
| 07-03-02 | 03 | 2 | RTE-05 | unit | `cargo test -p velos-predict` | Yes (ensemble_tests.rs: ETS tests) | green |
| 07-03-03 | 03 | 2 | RTE-06 | unit | `cargo test -p velos-predict` | Yes (ensemble_tests.rs: ensemble blend tests) | green |
| 07-03-04 | 03 | 2 | RTE-07 | unit | `cargo test -p velos-predict` | Yes (ensemble_tests.rs: ArcSwap overlay tests) | green |
| 07-04-01 | 04 | 2 | INT-03 | unit | `cargo test -p velos-gpu --lib perception` | Yes (perception.rs: 7 tests) | green |
| 07-04-02 | 04 | 2 | INT-04 | unit | `cargo test -p velos-core --lib reroute` | Yes (reroute.rs: 12 scheduler tests) | green |
| 07-04-03 | 04 | 2 | INT-05 | unit | `cargo test -p velos-core --lib reroute` | Yes (reroute.rs: 5 evaluation tests) | green |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-predict/` -- new crate scaffolding (Cargo.toml, lib.rs, tests/)
- [x] `crates/velos-net/src/cch/` -- new module directory
- [x] `crates/velos-net/tests/cch_tests.rs` -- CCH correctness vs A* baseline
- [x] `crates/velos-core/src/cost.rs` -- CostWeights + route_cost tests
- [x] `crates/velos-core/src/reroute.rs` -- RerouteScheduler tests
- [x] `crates/velos-gpu/shaders/perception.wgsl` -- perception kernel stub
- [x] `crates/velos-predict/tests/ensemble_tests.rs` -- prediction ensemble tests
- [x] Workspace Cargo.toml: add arc-swap, rayon, tokio dependencies

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Visual route difference between Commuter/Tourist | INT-02 | Requires visual inspection of path rendering | Run sim with both profiles, same OD; verify different paths on map |
| Mid-sim road closure rerouting | RTE-03 | Requires runtime interaction | Close edge via API, verify affected agents reroute within same step |

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
| Total tests verified | 108+ |
| All green | Yes |

### Test Run Summary

| Crate | Command | Tests | Result |
|-------|---------|-------|--------|
| velos-net | `cargo test -p velos-net --test cch_tests` | 27 | PASS |
| velos-core | `cargo test -p velos-core --lib cost` | 15 | PASS |
| velos-core | `cargo test -p velos-core --lib reroute` | 17 | PASS |
| velos-demand | `cargo test -p velos-demand --lib profile` | 12 | PASS |
| velos-predict | `cargo test -p velos-predict` | 23 | PASS |
| velos-gpu | `cargo test -p velos-gpu --lib perception` | 7 | PASS |
