# Phase 20: Real-Time Calibration - Research

**Researched:** 2026-03-11
**Domain:** Streaming calibration trigger refactor, egui panel enhancement, stability safeguards
**Confidence:** HIGH

## Summary

Phase 20 is a focused refactor of the existing batch calibration system (Phase 17) to trigger on aggregation window completion rather than a fixed 300-second timer. The core calibration math (EMA smoothing, clamping, OD factor computation) is already proven and reusable. The work involves three areas: (1) replacing the timer trigger with window-change detection in `step_calibration()`, (2) adding stability safeguards (minimum observation threshold, decay-toward-baseline, per-step change cap), and (3) enhancing the egui calibration panel with status indicators and a pause toggle.

All required code exists in the codebase. No new crates, no new dependencies, no new gRPC methods. The aggregator already exposes `latest_window(camera_id)` which returns a `TimeWindow` with `start_ms` -- the trigger mechanism reads this value and compares against a per-camera `last_window_start_ms` tracker.

**Primary recommendation:** Implement as a single plan with three sequential waves: trigger refactor, stability safeguards, egui panel enhancement.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- Event-driven recalibration on new aggregation window completion, replacing fixed 300s timer
- When DetectionAggregator rolls to a new TimeWindow (detected by comparing latest window start_ms against last-processed), trigger calibration immediately
- Minimum cooldown of 30 sim-seconds between recalibrations to prevent thrashing
- If no detection data arrives, no recalibration happens (demand stays at last computed overlay)
- Existing step_calibration() method refactored: remove CALIBRATION_INTERVAL_SECS timer, replace with window-change detection logic
- Fallback: if detection stream is active but aggregator has no new complete windows, skip
- Keep existing EMA alpha=0.3 and clamp [0.5, 2.0]
- Minimum observation threshold per camera: skip if observed count < 10 in latest window
- Decay toward baseline: if camera has no new data for 3 consecutive windows, decay ratio toward 1.0 at rate 0.1 per missed window
- Per-step demand change cap: absolute change in any OD pair factor capped at +/-0.2 per recalibration
- Track last_window_start_ms per camera in CameraCalibrationState
- Egui panel: status indicator (Calibrating/Idle/Stale), per-camera row with details, global summary
- Pause/resume toggle to freeze calibration overlay
- No ratio history chart (out of scope)
- Stream disconnect: ratios freeze at last computed values
- Late camera connection: cameras registered mid-simulation participate in next calibration cycle
- Camera removal: ratio contributions dropped on next recalibration
- Simulation restart: calibration state resets (SimWorld::new() semantics)

### Claude's Discretion
- Exact egui layout and styling for enhanced calibration panel
- Whether to add log::info on each recalibration event or keep at debug level
- Internal data structure for tracking per-camera window freshness (could be HashMap<u32, i64>)
- Whether minimum observation threshold (10) should be configurable via TOML or hardcoded constant

### Deferred Ideas (OUT OF SCOPE)
- Ratio history chart / time-series visualization
- Camera removal/unregister gRPC RPC
- Configurable EMA alpha / clamp bounds via egui sliders
- Per-vehicle-class calibration ratios
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CAL-02 | System continuously calibrates demand during a running simulation from streaming detection data | Window-change trigger replaces timer; stability safeguards prevent oscillation; pause toggle gives user control; egui panel provides visibility |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| arc_swap | (workspace) | Lock-free CalibrationOverlay swap | Already used in Phase 17 CalibrationStore |
| hecs | (workspace) | ECS world queries for simulated counts | Already used throughout SimWorld |
| egui | (workspace) | Calibration panel UI | Already used in app_egui.rs |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| log | (workspace) | Recalibration event logging | On each recalibration trigger |

No new dependencies required. All libraries already in the workspace.

## Architecture Patterns

### Recommended File Structure
```
crates/
├── velos-api/src/
│   └── calibration.rs          # Extend CameraCalibrationState, add change cap logic
├── velos-gpu/src/
│   ├── sim_calibration.rs      # Refactor step_calibration() trigger logic
│   ├── sim.rs                  # Add calibration_paused field, per-camera window tracking
│   └── app_egui.rs             # Enhanced calibration panel
```

### Pattern 1: Window-Change Detection Trigger
**What:** Replace fixed-interval timer with per-camera window freshness check
**When to use:** Every frame in step_calibration()
**Example:**
```rust
// In SimWorld (sim.rs), add fields:
pub(crate) calibration_paused: bool,
pub(crate) last_processed_windows: HashMap<u32, i64>,  // camera_id -> last start_ms
pub(crate) last_calibration_sim_time: f64,  // for cooldown check

// In step_calibration() (sim_calibration.rs):
pub(crate) fn step_calibration(&mut self) {
    if self.calibration_paused { return; }

    let aggregator = self.aggregator.lock().unwrap();
    let registry = self.camera_registry.lock().unwrap();
    let cameras = registry.list();
    if cameras.is_empty() { return; }

    // Check if ANY camera has a new completed window
    let mut has_new_window = false;
    for cam in &cameras {
        if let Some(window) = aggregator.latest_window(cam.id) {
            let last = self.last_processed_windows.get(&cam.id).copied().unwrap_or(-1);
            if window.start_ms > last {
                has_new_window = true;
                break;
            }
        }
    }
    if !has_new_window { return; }

    // Cooldown: minimum 30 sim-seconds between recalibrations
    if self.sim_time - self.last_calibration_time < 30.0 { return; }

    // ... proceed with calibration (existing logic) ...
    // After computing, update last_processed_windows for all cameras
}
```

### Pattern 2: Decay Toward Baseline
**What:** Cameras with no new data for 3+ consecutive windows decay ratio toward 1.0
**When to use:** During calibration factor computation
**Example:**
```rust
// In CameraCalibrationState, add:
pub consecutive_stale_windows: u32,

// During calibration, if camera's latest window hasn't changed:
if window.start_ms == last_processed {
    state.consecutive_stale_windows += 1;
    if state.consecutive_stale_windows >= 3 {
        // Decay toward 1.0 at rate 0.1 per missed window
        let decay = 0.1 * (state.consecutive_stale_windows - 2) as f32;
        state.previous_ratio += (1.0 - state.previous_ratio) * decay.min(1.0);
    }
}
```

### Pattern 3: Per-Step Change Cap
**What:** Limit absolute change in any OD pair factor to +/-0.2 per recalibration
**When to use:** After computing new overlay, before swapping
**Example:**
```rust
// Compare new factors against current overlay
let current = self.calibration_store.current();
for ((oz, dz), new_factor) in new_overlay.factors.iter_mut() {
    if let Some(&old_factor) = current.factors.get(&(*oz, *dz)) {
        let delta = *new_factor - old_factor;
        *new_factor = old_factor + delta.clamp(-0.2, 0.2);
    }
    // If no old factor, new_factor stands (first calibration for this OD pair)
}
```

### Anti-Patterns to Avoid
- **Polling timer alongside window detection:** Don't keep the old 300s timer as fallback. The window-change detection is the sole trigger.
- **Locking aggregator for the entire calibration:** Lock briefly to read window data, drop, then do ECS queries and compute.
- **Mutating shared state under aggregator lock:** The aggregator lock should only be held for reads. All mutations go to SimWorld-owned fields.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Lock-free overlay swap | Custom atomic/RwLock | ArcSwap (CalibrationStore) | Already proven in Phase 17 |
| EMA smoothing | New smoothing algorithm | Existing compute_camera_ratio() | Proven, tested, clamped |
| Thread-safe camera list | New concurrent data structure | Mutex<CameraRegistry> | Already works, contention is negligible |

**Key insight:** This phase extends existing infrastructure, not replacing it. The calibration math is unchanged -- only the trigger mechanism and stability safeguards are new.

## Common Pitfalls

### Pitfall 1: Lock Ordering Deadlock
**What goes wrong:** step_calibration() holds registry lock while trying to acquire aggregator lock (or vice versa), while gRPC thread holds aggregator lock trying to acquire registry lock.
**Why it happens:** Both locks are Arc<Mutex<_>> shared between SimWorld and gRPC handlers.
**How to avoid:** Always acquire registry first, then aggregator. Or better: acquire one, clone/extract needed data, drop, then acquire the other. The existing code already follows this pattern (see line 133 in sim_calibration.rs: `drop(registry)` before ECS query).
**Warning signs:** Simulation freezes when gRPC detections arrive.

### Pitfall 2: Comparing Window start_ms Across Clocks
**What goes wrong:** Aggregator windows use Unix epoch milliseconds from detection timestamps, but SimWorld tracks sim_time in seconds from a different epoch. Mixing these for staleness comparison.
**Why it happens:** Two different time domains in the system.
**How to avoid:** last_processed_windows tracks start_ms values (i64, Unix epoch ms) -- same domain as TimeWindow.start_ms. Cooldown uses sim_time (f64, sim-seconds) -- same domain as self.sim_time. Never cross the domains.
**Warning signs:** Calibration never triggers, or triggers every frame.

### Pitfall 3: Change Cap Preventing Initial Calibration
**What goes wrong:** First calibration produces factors far from 1.0, but change cap limits each step to +/-0.2, so convergence takes many cycles.
**Why it happens:** New OD pairs have no old factor to compare against.
**How to avoid:** When no old factor exists for an OD pair, apply the new factor directly (no cap). Only cap when modifying an existing factor.
**Warning signs:** Calibration appears stuck at 1.0 for many cycles after cameras connect.

### Pitfall 4: Decay Overshooting Past 1.0
**What goes wrong:** Decay logic pushes ratio past 1.0 if previous_ratio < 1.0 (moves away from baseline).
**Why it happens:** Naive implementation adds fixed decay amount instead of decaying toward target.
**How to avoid:** Use `ratio += (1.0 - ratio) * decay_rate` which always moves toward 1.0 regardless of direction.
**Warning signs:** Ratios below 1.0 get pushed further below.

### Pitfall 5: Egui Borrow Conflict with Mutex
**What goes wrong:** Holding registry/aggregator lock while rendering egui panel causes frame stutter or deadlock.
**Why it happens:** egui rendering happens on the main thread; gRPC handlers run on tokio threads.
**How to avoid:** Clone needed data out of locks before passing to egui. The existing draw_calibration_panel already does `reg.list()` and drops the lock (implicitly at end of scope). Keep this pattern.
**Warning signs:** Frame time spikes correlate with gRPC activity.

## Code Examples

### Existing step_calibration() Trigger (to be replaced)
```rust
// Source: crates/velos-gpu/src/sim_calibration.rs:58-59,110-113
const CALIBRATION_INTERVAL_SECS: f64 = 300.0;

// In step_calibration():
if self.sim_time - self.last_calibration_time < CALIBRATION_INTERVAL_SECS {
    return;
}
self.last_calibration_time = self.sim_time;
```

### Existing CameraCalibrationState (to be extended)
```rust
// Source: crates/velos-api/src/calibration.rs:87-104
pub struct CameraCalibrationState {
    pub previous_ratio: f32,
    pub last_observed: u32,
    pub last_simulated: u32,
}
```

### Aggregator Window Access (trigger source)
```rust
// Source: crates/velos-api/src/aggregator.rs:111-115
pub fn latest_window(&self, camera_id: u32) -> Option<&TimeWindow> {
    self.cameras
        .get(&camera_id)
        .and_then(|windows| windows.iter().max_by_key(|w| w.start_ms))
}
```

### Existing Egui Calibration Panel (to be enhanced)
```rust
// Source: crates/velos-gpu/src/app_egui.rs:137-170
fn draw_calibration_panel(ui: &mut egui::Ui, sim: &SimWorld, grpc_addr: &str) {
    // Current: simple grid with Camera/Obs/Sim/Ratio columns
    // Phase 20: add status indicator, staleness, pause toggle, global summary
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Fixed 300s timer trigger | Window-change detection | Phase 20 | Responsive to data arrival, not clock |
| No minimum observation filter | Skip camera if observed < 10 | Phase 20 | Prevents noisy ratios from sparse data |
| No staleness handling | Decay toward 1.0 after 3 stale windows | Phase 20 | Prevents phantom demand adjustments |
| Unlimited factor jumps | +/-0.2 per-step change cap | Phase 20 | Smooth transitions, no demand shocks |
| Basic egui grid | Status indicators + pause toggle | Phase 20 | User visibility and control |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in test + cargo test |
| Config file | Cargo.toml (workspace) |
| Quick run command | `cargo test -p velos-api --lib calibration` |
| Full suite command | `cargo test -p velos-api -p velos-gpu` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| CAL-02a | Window-change detection triggers recalibration | unit | `cargo test -p velos-api --lib calibration::tests::window_change_triggers -x` | Wave 0 |
| CAL-02b | Cooldown prevents thrashing (30s minimum) | unit | `cargo test -p velos-api --lib calibration::tests::cooldown_prevents_thrashing -x` | Wave 0 |
| CAL-02c | Minimum observation threshold skips sparse cameras | unit | `cargo test -p velos-api --lib calibration::tests::min_observation_threshold -x` | Wave 0 |
| CAL-02d | Decay toward baseline after 3 stale windows | unit | `cargo test -p velos-api --lib calibration::tests::decay_toward_baseline -x` | Wave 0 |
| CAL-02e | Per-step change cap limits factor jumps to +/-0.2 | unit | `cargo test -p velos-api --lib calibration::tests::change_cap_limits_jumps -x` | Wave 0 |
| CAL-02f | No recalibration when no detection data arrives | unit | `cargo test -p velos-api --lib calibration::tests::no_data_no_recalibration -x` | Wave 0 |
| CAL-02g | Calibration paused flag prevents overlay update | unit | `cargo test -p velos-gpu --lib -- sim_calibration::tests::paused_skips -x` | Wave 0 |
| CAL-02h | Late camera participates on next cycle | unit | `cargo test -p velos-api --lib calibration::tests::late_camera_participates -x` | Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-api --lib calibration && cargo test -p velos-gpu --lib sim_calibration`
- **Per wave merge:** `cargo test -p velos-api -p velos-gpu`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] Tests for window-change detection trigger logic (new behavior)
- [ ] Tests for cooldown enforcement
- [ ] Tests for minimum observation threshold (10 count filter)
- [ ] Tests for decay-toward-baseline (3-window staleness)
- [ ] Tests for per-step change cap (+/-0.2)
- [ ] Tests for calibration_paused flag
- [ ] Tests for late camera connection behavior

## Open Questions

1. **Cooldown interaction with multiple cameras completing windows simultaneously**
   - What we know: Cooldown is 30 sim-seconds global, not per-camera
   - What's unclear: If 5 cameras complete windows within 1 second, does the first trigger handle all 5, or do we queue?
   - Recommendation: Single trigger handles all cameras that have new windows at that moment. This is how the existing code already works (iterates all cameras per calibration call).

2. **Staleness window count vs. time-based staleness**
   - What we know: Decision says "3 consecutive aggregation windows" for decay trigger
   - What's unclear: Are these consecutive windows globally (300s each = 900s) or per-camera (could be different if cameras have different data rates)?
   - Recommendation: Per-camera tracking. Each camera's `consecutive_stale_windows` increments when a calibration cycle runs but that camera's latest window hasn't changed. Reset to 0 when a new window is detected.

## Sources

### Primary (HIGH confidence)
- `crates/velos-gpu/src/sim_calibration.rs` - Existing trigger logic, step_calibration(), step_api_commands()
- `crates/velos-api/src/calibration.rs` - CalibrationStore, CameraCalibrationState, compute_camera_ratio(), compute_calibration_factors()
- `crates/velos-api/src/aggregator.rs` - DetectionAggregator, TimeWindow, latest_window()
- `crates/velos-gpu/src/sim.rs` - SimWorld fields, tick_gpu() pipeline, tick() pipeline
- `crates/velos-gpu/src/app_egui.rs` - draw_calibration_panel(), EguiPanelState

### Secondary (MEDIUM confidence)
- Phase 17 decisions in STATE.md - EMA alpha=0.3, clamp [0.5, 2.0], threshold <=5

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all libraries already in use, no new deps
- Architecture: HIGH - extending proven patterns, refactoring trigger only
- Pitfalls: HIGH - identified from direct code reading, lock ordering and time domain issues are concrete

**Research date:** 2026-03-11
**Valid until:** 2026-04-11 (stable -- internal refactor, no external dependency changes)
