---
phase: 07-intelligence-routing-prediction
plan: 04
subsystem: prediction
tags: [bpr, ets, exponential-smoothing, historical, ensemble, arc-swap, lock-free]

requires:
  - phase: 06-agent-models-signal-control
    provides: "BPR pattern from velos-meso queue model"
provides:
  - "PredictionEnsemble: BPR + ETS + historical blending with adaptive weights"
  - "PredictionOverlay: ArcSwap-based lock-free concurrent reads"
  - "PredictionService: 60-second interval update orchestrator"
  - "AdaptiveWeights: inverse-error softmax weight adjustment"
affects: [07-05, 07-06, velos-net routing cost function]

tech-stack:
  added: [arc-swap]
  patterns: [ArcSwap lock-free overlay, inverse-error softmax weight adaptation, BPR fast-path beta=4.0]

key-files:
  created:
    - crates/velos-predict/Cargo.toml
    - crates/velos-predict/src/lib.rs
    - crates/velos-predict/src/bpr.rs
    - crates/velos-predict/src/ets.rs
    - crates/velos-predict/src/historical.rs
    - crates/velos-predict/src/adaptive.rs
    - crates/velos-predict/src/overlay.rs
    - crates/velos-predict/tests/ensemble_tests.rs
  modified:
    - Cargo.toml

key-decisions:
  - "PredictionInput struct instead of 8 function args (clippy too-many-arguments)"
  - "Inverse-error softmax for adaptive weights (lower error = higher weight)"
  - "Minimum weight floor 0.05 prevents silencing any model"
  - "Confidence = 1 - (range/mean) for inter-model disagreement"
  - "Historical matcher: flat Vec with 96 slots/edge (24 hours * 4 day types)"

patterns-established:
  - "ArcSwap overlay pattern: immutable snapshot + atomic swap for lock-free reads"
  - "BPR beta=4.0 fast path: vc*vc*vc*vc instead of powf"
  - "ETS correction: gamma * error + (1-gamma) * prev_correction"

requirements-completed: [RTE-04, RTE-05, RTE-06, RTE-07]

duration: 6min
completed: 2026-03-07
---

# Phase 07 Plan 04: Prediction Ensemble Summary

**BPR + ETS + historical ensemble with ArcSwap overlay, adaptive weights, and 60-second update interval for prediction-informed routing**

## Performance

- **Duration:** 6 min
- **Started:** 2026-03-07T16:01:07Z
- **Completed:** 2026-03-07T16:06:45Z
- **Tasks:** 3
- **Files modified:** 9

## Accomplishments
- BPR physics extrapolation with beta=4.0 fast path and negative flow clamping
- ETS exponential smoothing corrector with gamma=0.3 convergence tracking
- Historical pattern matcher with 24-hour x 4 day-type lookup per edge
- Adaptive weights via inverse-error softmax with 0.05 minimum floor
- ArcSwap-based PredictionOverlay for lock-free concurrent reads during swap
- PredictionService with 60 sim-second update interval orchestration
- 23 comprehensive tests covering all models, ensemble blending, and ArcSwap atomicity

## Task Commits

Each task was committed atomically:

1. **Task 1: Scaffold velos-predict crate + BPR and ETS models** - `e952cd7` (feat)
2. **Task 2: Historical matcher + adaptive weights** - `8ba7637` (feat)
3. **Task 3: PredictionOverlay with ArcSwap** - `8459bb8` (feat)

## Files Created/Modified
- `crates/velos-predict/Cargo.toml` - Crate manifest with arc-swap dependency
- `crates/velos-predict/src/lib.rs` - PredictionEnsemble, PredictionService, PredictionInput
- `crates/velos-predict/src/bpr.rs` - BPR physics extrapolation (t_free * (1 + 0.15 * (V/C)^4))
- `crates/velos-predict/src/ets.rs` - Exponential smoothing correction with gamma=0.3
- `crates/velos-predict/src/historical.rs` - Time-of-day pattern matcher (96 slots per edge)
- `crates/velos-predict/src/adaptive.rs` - Inverse-error softmax adaptive weight adjustment
- `crates/velos-predict/src/overlay.rs` - ArcSwap PredictionOverlay + PredictionStore
- `crates/velos-predict/tests/ensemble_tests.rs` - 23 tests covering all components
- `Cargo.toml` - Added velos-predict to workspace, arc-swap to workspace deps

## Decisions Made
- PredictionInput struct to bundle update parameters (avoids clippy too-many-arguments on PredictionService::update)
- Inverse-error softmax for adaptive weights: `target_w = (1/error) / sum(1/errors)`, smoothed by learning_rate=0.05
- Minimum weight floor of 0.05 prevents complete silencing of any model during transient errors
- Confidence computed as `1 - (max-min)/mean` normalized disagreement across models
- Historical matcher uses flat Vec with 96 slots per edge (24 hours * 4 day types) for O(1) lookup

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Clippy too-many-arguments on PredictionService::update**
- **Found during:** Task 3 (PredictionService implementation)
- **Issue:** 8 parameters exceeded clippy default limit of 7
- **Fix:** Introduced PredictionInput struct to bundle flow/capacity/free_flow/actual/hour/day_type
- **Files modified:** crates/velos-predict/src/lib.rs, tests/ensemble_tests.rs
- **Verification:** cargo clippy -p velos-predict -- -D warnings passes clean
- **Committed in:** 8459bb8 (Task 3 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Minor API shape change. PredictionInput struct improves ergonomics.

## Issues Encountered
None

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Prediction ensemble ready to feed into routing cost functions (07-05, 07-06)
- PredictionStore::clone_handle enables sharing across threads for concurrent routing queries
- Historical matcher ready to receive recorded observations from simulation runs

---
*Phase: 07-intelligence-routing-prediction*
*Completed: 2026-03-07*
