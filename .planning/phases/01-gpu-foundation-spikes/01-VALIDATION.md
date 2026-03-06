---
phase: 1
slug: gpu-foundation-spikes
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-06
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in #[test] + nightly #[bench] |
| **Config file** | None — standard Cargo test config |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo clippy --all-targets -- -D warnings && cargo test --workspace && cargo bench` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo clippy --all-targets -- -D warnings && cargo test --workspace`
- **After every plan wave:** Run `cargo clippy --all-targets -- -D warnings && cargo test --workspace && cargo bench`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | GPU-01 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_gpu_compute_dispatch` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 1 | GPU-02 | unit + integration | `cargo test -p velos-core -- test_fix && cargo test -p velos-gpu --features gpu-tests -- test_gpu_fixed_point` | ❌ W0 | ⬜ pending |
| 01-02-01 | 02 | 2 | GPU-03 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_gpu_round_trip` | ❌ W0 | ⬜ pending |
| 01-02-02 | 02 | 2 | GPU-04 | unit | `cargo test -p velos-core -- test_cfl` | ❌ W0 | ⬜ pending |
| 01-03-01 | 03 | 3 | GPU-05 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_gpu_leader_index` | ❌ W0 | ⬜ pending |
| 01-03-02 | 03 | 3 | GPU-06 | integration | `cargo test -p velos-gpu --features gpu-tests -- test_gpu_pcg` | ❌ W0 | ⬜ pending |
| 01-03-03 | 03 | 3 | PERF-01 | bench | `cargo bench` | ❌ W0 | ⬜ pending |
| 01-03-04 | 03 | 3 | PERF-02 | bench | `cargo bench` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `Cargo.toml` — workspace manifest with workspace-level dependencies
- [ ] `rust-toolchain.toml` — pin nightly version
- [ ] `crates/velos-core/Cargo.toml` — crate scaffold
- [ ] `crates/velos-gpu/Cargo.toml` — crate scaffold with `gpu-tests` feature flag
- [ ] `scripts/generate_golden_vectors.py` — Python script for fixed-point test vectors
- [ ] `test-fixtures/golden_vectors.json` — generated golden vectors

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Metal adapter available | GPU-01 | Hardware-dependent | Run `cargo test -p velos-gpu --features gpu-tests` on Apple Silicon Mac — tests skip gracefully if no adapter |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
