---
phase: 1
slug: gpu-foundation-spikes
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-06
---

# Phase 1 -- Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in #[test] + nightly #[bench] |
| **Config file** | Cargo.toml [features] for gpu-tests gate |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo clippy --all-targets -- -D warnings && cargo test --workspace --features gpu-tests && cargo bench -p velos-gpu --features gpu-tests` |
| **Estimated runtime** | ~20 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo clippy --all-targets -- -D warnings && cargo test --workspace`
- **After every plan wave:** Run `cargo clippy --all-targets -- -D warnings && cargo test --workspace --features gpu-tests`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 20 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | GPU-01 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_compute_dispatch` | W0 | pending |
| 01-01-02 | 01 | 1 | GPU-02 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_f32_f64_tolerance` | W0 | pending |
| 01-01-03 | 01 | 1 | GPU-03 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_round_trip_1k` | W0 | pending |
| 01-01-04 | 01 | 1 | GPU-04 | unit | `cargo test -p velos-core -- test_cfl_check` | W0 | pending |
| 01-02-01 | 02 | 2 | REN-01 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_window_surface` | W0 | pending |
| 01-02-02 | 02 | 2 | REN-02 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_instanced_render` | W0 | pending |
| 01-02-03 | 02 | 2 | REN-03 | unit | `cargo test -p velos-gpu -- test_camera_projection` | W0 | pending |
| 01-02-04 | 02 | 2 | REN-04 | manual-only | Code review: one draw_indexed per shape type | N/A | pending |
| 01-02-05 | 02 | 2 | PERF-01 | bench | `cargo bench -p velos-gpu --features gpu-tests -- frame_time` | W0 | pending |
| 01-02-06 | 02 | 2 | PERF-02 | bench | `cargo bench -p velos-gpu --features gpu-tests -- throughput` | W0 | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

- [ ] `Cargo.toml` -- workspace manifest with workspace-level dependencies
- [ ] `rust-toolchain.toml` -- pin nightly version
- [ ] `crates/velos-core/Cargo.toml` -- crate scaffold
- [ ] `crates/velos-gpu/Cargo.toml` -- crate scaffold with `gpu-tests` feature flag
- [ ] `crates/velos-gpu/tests/gpu_round_trip.rs` -- covers GPU-01, GPU-02, GPU-03
- [ ] `crates/velos-core/src/cfl.rs` (inline tests) -- covers GPU-04
- [ ] `crates/velos-gpu/tests/render_tests.rs` -- covers REN-01, REN-02
- [ ] `crates/velos-gpu/src/camera.rs` (inline tests) -- covers REN-03
- [ ] `crates/velos-gpu/benches/dispatch.rs` -- covers PERF-01, PERF-02

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Single draw call per shape type | REN-04 | Architectural constraint, not runtime behavior | Code review: verify one `draw_indexed` call per shape type in render pass |
| Metal adapter available | GPU-01 | Hardware-dependent | Run GPU tests on Apple Silicon Mac -- tests skip gracefully if no adapter |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 20s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
