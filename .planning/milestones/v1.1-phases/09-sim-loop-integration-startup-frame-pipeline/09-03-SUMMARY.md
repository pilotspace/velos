---
phase: 09-sim-loop-integration-startup-frame-pipeline
plan: 03
subsystem: gpu-engine
tags: [perception, detectors, signals, reroute, frame-pipeline, priority]

requires:
  - phase: 09-01
    provides: "SimWorld::new() with perception/signals/detectors, sim_startup.rs"
  - phase: 09-02
    provides: "PerceptionResult binding(8) in wave_front.wgsl, HCMC behavior functions"
provides:
  - "Full 10-step frame pipeline in tick_gpu(): spawn -> detectors -> signals -> priority -> perception -> reroute -> vehicles -> pedestrians -> gridlock -> cleanup"
  - "sim_perception.rs with step_perception() and PerceptionBuffers (signal, congestion, travel ratio)"
  - "Loop detector update feeding actuated signal controllers per frame"
  - "Signal priority request processing for bus and emergency vehicles within 100m"
  - "Perception results available GPU-side and fed into step_reroute()"
  - "sim_pedestrians.rs extracted for 700-line compliance"
affects: [velos-gpu]

tech-stack:
  added: []
  patterns: [full-frame-pipeline, detector-actuated-feedback, signal-priority-requests, perception-dispatch-readback]

key-files:
  created:
    - crates/velos-gpu/src/sim_perception.rs
    - crates/velos-gpu/src/sim_pedestrians.rs
  modified:
    - crates/velos-gpu/src/sim.rs
    - crates/velos-gpu/src/lib.rs

key-decisions:
  - "Loop detector uses speed * 0.1s backward approximation for prev_pos (conservative, avoids extra HashMap storage)"
  - "Signal priority proximity threshold: 100m from intersection on incoming edge"
  - "PerceptionBuffers pre-allocated at startup with zeroed congestion grid (20x20, 500m cells)"
  - "step_pedestrians extracted to sim_pedestrians.rs to keep sim.rs at 634 lines"
  - "CPU tick() path gets detector readings and priority but not perception/reroute (no GPU)"

patterns-established:
  - "10-step frame pipeline order: spawn -> detectors -> signals -> priority -> perception -> reroute -> vehicles -> pedestrians -> gridlock -> cleanup"
  - "Detector-actuated signal feedback: per-frame LoopDetector checks feed DetectorReading into SignalController::tick()"

requirements-completed: [SIG-03, SIG-04, INT-03, INT-04, INT-05, RTE-03, RTE-07]

duration: 7min
completed: 2026-03-08
---

# Phase 09 Plan 03: Full Frame Pipeline Wiring Summary

**Complete 10-step frame pipeline wired into tick_gpu(): loop detectors feed actuated signals, GPU perception dispatches per frame, reroute evaluates from perception results, bus/emergency priority requests processed at signalized intersections**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-08T05:19:47Z
- **Completed:** 2026-03-08T05:27:20Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- Wired complete 10-step frame pipeline in tick_gpu() running all Phase 6-8 subsystems in correct order
- Created sim_perception.rs with PerceptionBuffers (signal state, congestion grid, edge travel ratios) and step_perception() GPU dispatch/readback
- Implemented update_loop_detectors() scanning agents crossing virtual detector points per frame
- Updated step_signals_with_detectors() passing detector readings to actuated/adaptive controllers
- Added step_signal_priority() processing bus/emergency priority requests within 100m of intersections
- Extracted step_pedestrians to sim_pedestrians.rs for 700-line compliance (sim.rs: 634 lines)
- CPU tick() path also receives detector readings and priority requests (no perception/reroute without GPU)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create sim_perception.rs with perception dispatch and auxiliary GPU buffers** - `5dab20f` (feat)
2. **Task 2: Wire full 10-step frame pipeline in tick_gpu** - `230dcc5` (feat)

## Files Created/Modified
- `crates/velos-gpu/src/sim_perception.rs` - PerceptionBuffers struct, step_perception(), update_signal_buffer(), update_edge_travel_ratio_buffer()
- `crates/velos-gpu/src/sim_pedestrians.rs` - step_pedestrians() extracted from sim.rs
- `crates/velos-gpu/src/sim.rs` - Full 10-step pipeline in tick_gpu(), update_loop_detectors(), step_signals_with_detectors(), step_signal_priority()
- `crates/velos-gpu/src/lib.rs` - Added sim_pedestrians and sim_perception modules

## Decisions Made
- Loop detector prev_pos approximation uses speed * 0.1s backward estimate rather than storing a HashMap of previous positions (simpler, sufficient for forward-only crossing detection)
- Signal priority proximity threshold set to 100m from intersection node on incoming edge (matches reasonable signal awareness range for transit vehicles)
- PerceptionBuffers zeroed congestion grid is a placeholder; real congestion data will come from spatial hashing in future work
- CPU tick() receives detector readings and priority but skips perception/reroute (requires GPU device)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] sim.rs exceeded 700-line limit (770 lines)**
- **Found during:** Task 2
- **Issue:** Adding loop detector, signal priority, and updated pipeline pushed sim.rs to 770 lines
- **Fix:** Extracted step_pedestrians (134 lines) to sim_pedestrians.rs, reducing sim.rs to 634 lines
- **Files modified:** sim.rs, sim_pedestrians.rs, lib.rs
- **Verification:** `wc -l sim.rs` = 634

**2. [Rule 1 - Bug] hecs query API mismatch**
- **Found during:** Task 2
- **Issue:** `self.world.query::<T>().iter(&self.world)` incorrect for hecs 0.11 (iter() takes no args)
- **Fix:** Changed to `self.world.query::<T>().iter()` matching existing codebase patterns
- **Files modified:** crates/velos-gpu/src/sim.rs
- **Verification:** clippy passes clean

---

**Total deviations:** 2 auto-fixed (1 blocking, 1 bug)
**Impact on plan:** Both fixes necessary for compilation and line compliance. No scope creep.

## Issues Encountered
- Pre-existing flaky test `car_agents_krauss_ratio_approximately_30_percent` (RNG sampling variance) -- not caused by this plan, passes on re-run
- Pre-existing clippy warning in velos-predict test (manual_range_contains) -- out of scope

## Next Phase Readiness
- Phase 09 complete: all 3 plans executed, full frame pipeline wired
- Every VELOS frame now runs: movement, detection, signals, perception, reroute
- Ready for Phase 10 (gap closure / validation)

---
*Phase: 09-sim-loop-integration-startup-frame-pipeline*
*Completed: 2026-03-08*
