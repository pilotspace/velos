---
phase: 16
slug: intersection-sublane-model
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-09
audited: 2026-03-10
---

# Phase 16 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[cfg(test)]` + `cargo test` |
| **Config file** | Workspace Cargo.toml (already configured) |
| **Quick run command** | `cargo test -p velos-net --lib junction && cargo test -p velos-vehicle --lib junction_traversal && cargo test -p velos-gpu --lib -- sim_render sim_junction map_tiles` |
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

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | Test Count | Status |
|---------|------|------|-------------|-----------|-------------------|------------|--------|
| 16-01-01 | 01 | 1 | ISL-01 | unit | `cargo test -p velos-net --lib junction::tests` | 27 | ✅ green |
| 16-01-02 | 01 | 1 | ISL-03 | unit | `cargo test -p velos-net --lib junction::tests::offset_position` | 1 | ✅ green |
| 16-01-03 | 01 | 1 | ISL-04 | unit | `cargo test -p velos-vehicle --lib junction_traversal::tests::conflict` | 7 | ✅ green |
| 16-02-01 | 02 | 1 | ISL-02 | unit | `cargo test -p velos-vehicle --lib junction_traversal::tests` | 18 | ✅ green |
| 16-02-02 | 02 | 1 | ISL-01,ISL-04 | integration | `cargo test -p velos-gpu --lib -- sim_junction` | 10 | ✅ green |
| 16-03-01 | 03 | 2 | MAP-01 | unit | `cargo test -p velos-gpu --lib -- map_tiles` | 9 | ✅ green |
| 16-04-01 | 04 | 2 | MAP-02 | unit | `cargo test -p velos-gpu --lib -- sim_render` | 10 | ✅ green |
| 16-01-ECS | 01 | 1 | ISL-01 | unit | `cargo test -p velos-core --lib -- junction` | 3 | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Total: 77 automated tests across 5 modules — all green**

---

## Requirement Coverage Matrix

| Requirement | Description | Test Modules | Status |
|-------------|-------------|--------------|--------|
| ISL-01 | Bezier turn paths with lateral offset | velos-net::junction (27), velos-core::components (3), velos-gpu::sim_junction (10) | COVERED |
| ISL-02 | Sublane traversal / vehicle-type behavior | velos-vehicle::junction_traversal (18), velos-gpu::sim_render (10) | COVERED |
| ISL-03 | Bezier lateral offset shift | velos-net::junction::offset_position (1) | COVERED |
| ISL-04 | Conflict priority resolution | velos-vehicle::junction_traversal::conflict_* (7), velos-gpu::sim_junction (10) | COVERED |
| MAP-01 | Map tile decode + render pipeline | velos-gpu::map_tiles (9) | COVERED |
| MAP-02 | Heading follows Bezier tangent | velos-gpu::sim_render::heading_from_tangent (3) | COVERED |

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Dashed Bezier guide lines render correctly | MAP-02 | Visual output requires human inspection | Toggle guide line overlay via egui checkbox, verify dashed lines follow junction turn paths |
| Map tile colors look correct against simulation | MAP-01 | Subjective visual quality | Pan across HCMC Districts 1, 3, 5 — buildings grey, water blue, parks green |
| Vehicle rotation smooth through junction | MAP-02 | Animation smoothness is perceptual | Watch motorbike/car traverse junction — heading should follow curve without jerks |

---

## Validation Sign-Off

- [x] All tasks have automated verification
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] All requirements have automated coverage (6/6 COVERED)
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** PASSED

---

## Validation Audit 2026-03-10

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |
| Total automated tests | 77 |
| Requirements covered | 6/6 |
