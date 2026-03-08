# Phase 15: File Size Reduction & Housekeeping - Research

**Researched:** 2026-03-08
**Domain:** Rust module extraction, project tracking document maintenance
**Confidence:** HIGH

## Summary

Phase 15 is pure tech debt closure with no behavioral changes. Two source files exceed the 700-line project convention (`sim.rs` at 948 lines, `compute.rs` at 1119 lines), three tracking documents have stale data (ROADMAP.md Phase 9 checkboxes, REQUIREMENTS.md footer, Phase 13 VALIDATION.md draft status), and one validation artifact needs finalization.

The extraction pattern is well-established in this codebase -- `sim.rs` has already been decomposed once into 11 `sim_*.rs` submodules (sim_bus, sim_helpers, sim_lifecycle, sim_meso, sim_mobil, sim_pedestrians, sim_perception, sim_render, sim_reroute, sim_snapshot, sim_startup). The same pattern applies to `compute.rs`.

**Primary recommendation:** Split compute.rs into `compute.rs` (core ComputeDispatcher struct + constructor + legacy dispatch) and `compute_wave_front.rs` (wave-front upload/dispatch/readback methods + sort_agents_by_lane). Move tests into `compute_tests.rs` if needed. For sim.rs, extract `step_signal_priority` and `update_loop_detectors` into a new `sim_signals.rs` and move `step_vehicles_gpu` into `sim_vehicles.rs`.

## Standard Stack

Not applicable -- this phase uses only existing Rust modules and project tooling. No new dependencies.

## Architecture Patterns

### Existing Module Extraction Pattern

The codebase has a proven pattern for splitting `sim.rs`:

```
crates/velos-gpu/src/
  sim.rs              # Core SimWorld struct, tick_gpu(), tick(), constructors
  sim_bus.rs          # step_bus_dwell (extracted)
  sim_helpers.rs      # apply_vehicle_update, check_signal_red, etc.
  sim_lifecycle.rs    # spawn_agents, spawn_single_agent, remove_finished_agents
  sim_meso.rs         # step_meso (extracted)
  sim_mobil.rs        # step_lane_changes (extracted)
  sim_pedestrians.rs  # step_pedestrians, step_pedestrians_gpu (extracted)
  sim_perception.rs   # step_perception, PerceptionBuffers (extracted)
  sim_render.rs       # build_instances (extracted)
  sim_reroute.rs      # step_reroute, init_reroute (extracted)
  sim_snapshot.rs     # AgentSnapshot (extracted)
  sim_startup.rs      # load_vehicle_config, build_signal_controllers, etc.
```

**Pattern:** Each extracted file contains `impl SimWorld { ... }` methods grouped by concern. They access `SimWorld` fields via `self` because Rust allows `impl` blocks in separate files within the same crate. The modules are declared in `lib.rs` as `mod sim_helpers;` (private) or `pub mod sim_meso;` (if types need external access).

### compute.rs Split Strategy

Current structure (1119 lines):

| Section | Lines | Content |
|---------|-------|---------|
| GpuVehicleParams | 1-87 | Type + from_config() |
| Param structs | 88-125 | DispatchParams, WaveFrontParams, GpuEmergencyVehicle |
| ComputeDispatcher | 127-681 | Struct fields + 14 methods (new, upload, dispatch, readback) |
| Free functions | 683-754 | compute_agent_flags, bgl_entry, sort_agents_by_lane |
| Tests | 755-1119 | 20+ test functions (364 lines) |

**Recommended split:**

| New File | Content | Est. Lines |
|----------|---------|------------|
| `compute.rs` | GpuVehicleParams, param structs, ComputeDispatcher struct + new() + legacy dispatch/readback + sign/emergency/vehicle-params upload + accessors | ~450 |
| `compute_wave_front.rs` | Wave-front upload_wave_front_data() + dispatch_wave_front() + readback_wave_front_agents() + sort_agents_by_lane + compute_agent_flags + bgl_entry | ~250 |
| Tests stay in `compute.rs` `#[cfg(test)]` | 20+ tests | ~364 |

Alternative approach if tests alone push compute.rs over:
- Move all tests into `compute_tests.rs` (separate test module file)

### sim.rs Split Strategy

Current structure (948 lines):

| Section | Lines | Content |
|---------|-------|---------|
| Imports + types | 1-160 | PartitionMode, SimState, SimMetrics, zone_centroids_from_graph, SimWorld struct |
| Constructors | 162-384 | new(), new_cpu_only(), enable_multi_gpu(), enable_meso(), reset() |
| tick methods | 386-529 | tick_gpu(), tick() -- the main orchestration |
| Signal methods | 531-682 | step_signals_with_detectors(), update_loop_detectors(), step_signal_priority() |
| GPU vehicle step | 684-812 | step_vehicles_gpu() |
| Tests | 814-948 | 2 test functions + helpers |

**Recommended split:**

| New File | Content | Est. Lines |
|----------|---------|------------|
| `sim.rs` | Imports, types (PartitionMode, SimState, SimMetrics), SimWorld struct, constructors (new, new_cpu_only, enable_multi_gpu, enable_meso, reset), tick_gpu(), tick() | ~530 |
| `sim_signals.rs` | step_signals_with_detectors(), update_loop_detectors(), step_signal_priority() | ~155 |
| `sim_vehicles.rs` | step_vehicles_gpu() | ~130 |
| Tests stay in `sim.rs` | 2 tests + helpers | ~134 |

Result: sim.rs drops from 948 to ~665 lines (well under 700).

### Re-export Pattern

When methods move to new files, no public API changes. The pattern used throughout:

```rust
// In lib.rs -- add new module declarations
mod sim_signals;
mod sim_vehicles;

// In sim_signals.rs -- extend SimWorld impl
use super::sim::SimWorld;
// or just:
impl SimWorld {
    pub(crate) fn step_signals_with_detectors(...) { ... }
}
```

The existing `sim_helpers.rs`, `sim_bus.rs`, etc. all follow this exact pattern -- they contain `impl SimWorld { ... }` blocks that extend the struct defined in `sim.rs`.

### Anti-Patterns to Avoid

- **Breaking public API:** All re-exports must be preserved. `pub use compute::ComputeDispatcher` in lib.rs must continue to work.
- **Moving struct definitions:** Keep `SimWorld` struct definition in `sim.rs` and `ComputeDispatcher` struct definition in `compute.rs`. Only move methods.
- **Changing visibility:** Methods that are `pub(crate)` must stay `pub(crate)` in new files. Methods that are `pub` must stay `pub`.

## Don't Hand-Roll

Not applicable -- this phase involves only file splitting and document editing.

## Common Pitfalls

### Pitfall 1: Import Cycles After Extraction
**What goes wrong:** Moving methods to a new file requires importing types that create circular module references.
**Why it happens:** The extracted methods reference types from `compute.rs` (like `ComputeDispatcher`, `GpuEmergencyVehicle`) and from `sim.rs` (like `SimWorld`).
**How to avoid:** Follow the existing pattern: extracted sim methods use `use crate::compute::...` for compute types. Extracted compute methods use `use crate::buffers::...` etc. No circular deps because modules import from siblings, not from each other.

### Pitfall 2: Forgetting to Declare New Modules
**What goes wrong:** Creating a new `.rs` file without adding `mod new_file;` in `lib.rs`.
**How to avoid:** Every new `.rs` file needs a corresponding `mod` declaration in `lib.rs`.

### Pitfall 3: Test Module Visibility
**What goes wrong:** Tests in `compute.rs` reference private functions/types that got moved out.
**How to avoid:** Keep tests in the file that contains the items they test, or use `pub(crate)` visibility for items that tests in other files need. The `compute_agent_flags` function is already `pub`, so tests can reference it from anywhere.

### Pitfall 4: Stale ROADMAP Plan References
**What goes wrong:** Phase 15 plans section currently lists 14-01 and 14-02 (Phase 14 plans) instead of Phase 15 plans.
**How to avoid:** When updating ROADMAP.md, fix this copy-paste error to reference the correct 15-XX plan files.

## Code Examples

### Extracting an impl block to a new file (existing pattern)

```rust
// File: sim_signals.rs
//! Signal-related SimWorld methods: detector updates, signal stepping, priority.

use petgraph::graph::{EdgeIndex, NodeIndex};
use velos_core::components::VehicleType;
use velos_signal::detector::{DetectorReading, LoopDetector};
use velos_signal::priority::{PriorityLevel, PriorityRequest};

use crate::sim::SimWorld;

impl SimWorld {
    /// Advance signal controllers with detector readings.
    pub(crate) fn step_signals_with_detectors(
        &mut self,
        dt: f64,
        detector_readings: &[(NodeIndex, Vec<DetectorReading>)],
    ) {
        // ... moved from sim.rs
    }
}
```

### Module declaration in lib.rs

```rust
// Add alongside existing sim_* modules:
mod sim_signals;
mod sim_vehicles;
// For compute split:
mod compute_wave_front;
```

## State of the Art

Not applicable -- Rust module system has been stable for years. No recent changes affect this work.

## Tracking Document Fixes Required

### ROADMAP.md Issues Found

1. **Phase 9 plan checkboxes (lines 126-128):** All three plans show `[ ]` but Phase 9 is complete. Must change to `[x]`.
2. **Phase 9 "Plans" count (line 124):** Shows "2/3 plans executed" but should show "3/3 plans complete".
3. **Phase 9 progress table row (line 175):** Shows "2/3 | In Progress" but should show "3/3 | Complete | 2026-03-08".
4. **Phase 10 plan checkboxes (lines 141-142):** Show `[ ]` but Phase 10 is complete. Must change to `[x]`.
5. **Phase 11 plan checkboxes (lines 157-158):** Show `[ ]` but Phase 11 is complete. Must change to `[x]`.
6. **Phase 14 plan checkbox (line 232):** 14-02 shows `[ ]` but Phase 14 is complete. Must change to `[x]`.
7. **Phase 15 plans section (lines 246-248):** Lists 14-01 and 14-02 (wrong phase). Must be updated with actual 15-XX plan references.
8. **Progress table formatting (lines 175-181):** Phase 9-14 rows have inconsistent column alignment and some have stray `- ` in Completed column.

### REQUIREMENTS.md Issues Found

The footer (lines 185-193) already shows "45/45 complete, 0 pending" which appears correct. However, the success criteria says "footer matches actual coverage (45/45 complete, 0 pending)" -- need to verify the traceability table matches (all 45 showing "Complete"). Upon inspection, all 45 requirements show "Complete" status. **No changes needed** unless a discrepancy is found during implementation.

### Phase 13 VALIDATION.md Issues Found

1. **Frontmatter status:** Shows `status: draft`, must change to `status: complete`.
2. **Frontmatter nyquist_compliant:** Shows `nyquist_compliant: false`, must change to `true`.
3. **Frontmatter wave_0_complete:** Shows `wave_0_complete: false`, must change to `true`.
4. **Task status column:** All tasks show `pending`, should be updated to `green` (Phase 13 is complete).
5. **File Exists column:** Wave 0 items marked as not existing need verification.
6. **Validation Sign-Off:** All checkboxes unchecked, need to be checked.
7. **Approval:** Shows "pending", should show completion.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Cargo.toml workspace |
| Quick run command | `cargo test -p velos-gpu --lib` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements to Test Map

This phase has no functional requirements -- it is pure refactoring and document updates. The key validation is that existing tests still pass after file extraction.

| Behavior | Test Type | Automated Command | Notes |
|----------|-----------|-------------------|-------|
| All existing tests pass after sim.rs split | regression | `cargo test -p velos-gpu --lib` | Must be green post-extraction |
| All existing tests pass after compute.rs split | regression | `cargo test -p velos-gpu --lib` | Must be green post-extraction |
| sim.rs under 700 lines | manual count | `wc -l crates/velos-gpu/src/sim.rs` | Target: < 700 |
| compute.rs under 700 lines | manual count | `wc -l crates/velos-gpu/src/compute.rs` | Target: < 700 |
| Clippy passes | lint | `cargo clippy -p velos-gpu -- -D warnings` | No new warnings |

### Sampling Rate

- **Per task commit:** `cargo test -p velos-gpu --lib && cargo clippy -p velos-gpu -- -D warnings`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green + `wc -l` confirms both files under 700

### Wave 0 Gaps

None -- no new tests needed. This phase verifies by running existing test suite and line counts.

## Open Questions

1. **compute.rs test placement**
   - What we know: Tests are 364 lines. If tests stay in compute.rs alongside the ComputeDispatcher struct + constructor + legacy path, compute.rs could still approach 700.
   - Recommendation: Extract wave-front methods first, count remaining lines. If still over 700, move tests to a separate `compute_tests.rs` test module.

2. **Phase 13 VALIDATION.md Wave 0 file existence**
   - What we know: Wave 0 lists test files that "don't exist" but Phase 13 is marked complete.
   - Recommendation: Check if the test files were created during Phase 13 execution. Update the validation document to reflect actual state.

## Sources

### Primary (HIGH confidence)
- Direct file inspection of `sim.rs` (948 lines), `compute.rs` (1119 lines)
- Direct inspection of ROADMAP.md stale checkboxes
- Direct inspection of REQUIREMENTS.md traceability table
- Direct inspection of Phase 13 VALIDATION.md draft status
- Existing module extraction pattern from 11 `sim_*.rs` files

## Metadata

**Confidence breakdown:**
- File splitting strategy: HIGH -- follows proven existing pattern with 11 precedents
- Document fixes: HIGH -- issues identified by direct inspection of current files
- Line count targets: HIGH -- measured current sizes, calculated post-split estimates

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (stable -- Rust module system does not change)
