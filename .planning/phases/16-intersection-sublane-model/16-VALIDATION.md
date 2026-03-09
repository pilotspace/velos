---
phase: 16
slug: intersection-sublane-model
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-09
---

# Phase 16 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[cfg(test)]` + `cargo test` |
| **Config file** | Workspace Cargo.toml (already configured) |
| **Quick run command** | `cargo test -p velos-net --lib junction && cargo test -p velos-vehicle --lib junction_traversal` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p velos-net -p velos-vehicle -p velos-gpu --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 16-01-01 | 01 | 1 | ISL-01 | unit | `cargo test -p velos-vehicle --lib junction_traversal::tests::lateral_offset_preserved -x` | ❌ W0 | ⬜ pending |
| 16-01-02 | 01 | 1 | ISL-03 | unit | `cargo test -p velos-net --lib junction::tests::bezier_lateral_offset -x` | ❌ W0 | ⬜ pending |
| 16-01-03 | 01 | 1 | ISL-04 | unit | `cargo test -p velos-vehicle --lib junction_traversal::tests::conflict_priority -x` | ❌ W0 | ⬜ pending |
| 16-02-01 | 02 | 1 | ISL-02 | unit | `cargo test -p velos-vehicle --lib sublane::tests -x` | ✅ | ⬜ pending |
| 16-03-01 | 03 | 2 | MAP-01 | unit | `cargo test -p velos-gpu --lib map_tiles::tests::decode_tile -x` | ❌ W0 | ⬜ pending |
| 16-03-02 | 03 | 2 | MAP-02 | unit | `cargo test -p velos-vehicle --lib junction_traversal::tests::heading_follows_tangent -x` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/velos-net/src/junction.rs` — test stubs for ISL-01, ISL-03, ISL-04 (BezierTurn, ConflictPoint)
- [ ] `crates/velos-vehicle/src/junction_traversal.rs` — test stubs for ISL-01, ISL-04, MAP-02 (traverse, conflict, heading)
- [ ] `crates/velos-gpu/src/map_tiles.rs` — test stubs for MAP-01 (tile decode + cache)
- [ ] New workspace dependencies: pmtiles2, mvt-reader, earcutr, flate2, lru

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Dashed Bezier guide lines render correctly | MAP-02 | Visual output requires human inspection | Toggle guide line overlay via egui checkbox, verify dashed lines follow junction turn paths |
| Map tile colors look correct against simulation | MAP-01 | Subjective visual quality | Pan across HCMC Districts 1, 3, 5 — buildings grey, water blue, parks green |
| Vehicle rotation smooth through junction | MAP-02 | Animation smoothness is perceptual | Watch motorbike/car traverse junction — heading should follow curve without jerks |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
