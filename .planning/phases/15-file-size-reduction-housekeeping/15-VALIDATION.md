---
phase: 15
slug: file-size-reduction-housekeeping
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-08
---

# Phase 15 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (built-in) |
| **Config file** | Cargo.toml workspace |
| **Quick run command** | `cargo test -p velos-gpu --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-gpu --lib && cargo clippy -p velos-gpu -- -D warnings`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 15-01-01 | 01 | 1 | SC-1 | regression + count | `cargo test -p velos-gpu --lib && wc -l crates/velos-gpu/src/sim.rs` | ✅ | ✅ green |
| 15-01-02 | 01 | 1 | SC-2 | regression + count | `cargo test -p velos-gpu --lib && wc -l crates/velos-gpu/src/compute.rs` | ✅ | ✅ green |
| 15-02-01 | 02 | 1 | SC-3 | manual | grep/diff REQUIREMENTS.md footer | ✅ | ✅ green |
| 15-02-02 | 02 | 1 | SC-4 | manual | grep/diff ROADMAP.md checkboxes | ✅ | ✅ green |
| 15-02-03 | 02 | 1 | SC-5 | manual | grep Phase 13 VALIDATION.md status | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No new test files needed — this phase validates via existing test suite regression and line counts.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| ROADMAP.md checkboxes correct | SC-4 | Document content check | Verify Phase 5, 9 checkboxes are `[x]` |
| REQUIREMENTS.md footer accurate | SC-3 | Document content check | Verify 45/45 complete, 0 pending |
| Phase 13 VALIDATION.md compliant | SC-5 | Document status check | Verify not draft, tasks reflect actual state |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** complete -- all tasks verified green, phase execution successful
