---
phase: 9
slug: sim-loop-integration-startup-frame-pipeline
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-08
updated: 2026-03-08
---

# Phase 9 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in Rust test framework) |
| **Config file** | Cargo.toml workspace test settings |
| **Quick run command** | `cargo test --workspace -q` |
| **Full suite command** | `cargo test --workspace --no-fail-fast` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo clippy --all-targets --all-features -- -D warnings && cargo test --workspace -q`
- **After every plan wave:** Run `cargo test --workspace --no-fail-fast`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 09-01-01 | 01 | 1 | TUN-02 | unit | `cargo test -p velos-gpu -- load_vehicle_config` | YES | ✅ green |
| 09-01-02 | 01 | 1 | SIG-01,SIG-02 | unit | `cargo test -p velos-gpu -- build_controllers` | YES | ✅ green |
| 09-01-03 | 01 | 1 | SIG-05 | unit | `cargo test -p velos-signal -- sign` | YES | ✅ green |
| 09-01-04 | 01 | 1 | RTE-03,INT-04,INT-05 | unit | `cargo test -p velos-gpu -- sim_reroute` | YES | ✅ green |
| 09-01-05 | 01 | 1 | INT-03 | unit | `cargo test -p velos-gpu -- perception` | YES | ✅ green |
| 09-02-01 | 02 | 1 | SIG-03,SIG-04 | unit | `cargo test -p velos-signal -- spat glosa` | YES | ✅ green |
| 09-02-02 | 02 | 1 | TUN-04,TUN-06 | unit | `cargo test -p velos-gpu -- creep gap_acceptance` | YES | ✅ green |
| 09-02-03 | 02 | 1 | RTE-07 | unit | `cargo test -p velos-predict` | YES | ✅ green |
| 09-INT-01a | INT | 2 | SIG-01,SIG-02,TUN-02,SIG-05 | integration | `cargo test -p velos-gpu --test integration_startup` | YES | ✅ green |
| 09-INT-01b | INT | 2 | INT-03,RTE-03,SIG-03,SIG-04 | integration | `cargo test -p velos-gpu --test integration_frame_pipeline` | YES | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `crates/velos-gpu/tests/integration_startup.rs` — tests SimWorld::new_cpu_only() initializes all subsystems (9 tests)
- [x] `crates/velos-gpu/tests/integration_frame_pipeline.rs` — tests tick() CPU path runs full pipeline in correct order (8 tests)
- [x] WGSL shader validation: `compute::tests::wave_front_shader_naga_validates` (naga parse test exists)

*All Wave 0 gaps resolved. 17 integration tests added.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| GPU perception_results populated | INT-03 | Requires actual GPU device | Run `cargo test --features gpu-tests -p velos-gpu -- perception_dispatch` on Metal device |
| Full frame pipeline ordering | All | End-to-end timing requires GPU | Run simulation for 10 frames, check log output for correct dispatch order |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated

---

## Validation Audit 2026-03-08

| Metric | Count |
|--------|-------|
| Gaps found | 2 |
| Resolved | 2 |
| Escalated | 0 |
