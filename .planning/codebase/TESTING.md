# Testing Patterns

**Analysis Date:** 2026-03-06

## Project Status

VELOS is in pre-development (architecture/design phase). No test code exists yet. These patterns are derived from architecture documents (`docs/architect/`), `CLAUDE.md`, and `.claude/CLAUDE.md`. All patterns below are **prescriptive** -- follow them when writing tests.

## Test Framework

**Runner:**
- Rust built-in test framework (`#[cfg(test)]`, `#[test]`)
- No external test runner crate specified
- Config: standard Cargo test configuration in `Cargo.toml`

**Assertion Library:**
- Standard `assert!`, `assert_eq!`, `assert_ne!` macros
- Use `approx` crate or manual epsilon comparison for floating-point assertions
- For fixed-point values: compare integer representations exactly

**Benchmark Framework:**
- `criterion` (implied by `cargo bench --bench frame_time`)
- Benchmark binary: `benches/frame_time.rs`

**Run Commands:**
```bash
# Run all tests across workspace
cargo test --workspace

# Run a single crate's tests
cargo test -p velos-core

# Run a specific test by name
cargo test -p velos-vehicle test_idm_acceleration

# Run all tests without stopping on first failure
cargo test --all --no-fail-fast

# Run benchmarks
cargo bench --bench frame_time

# Run benchmarks with regression detection against main
cargo bench --bench frame_time -- --baseline main

# WGSL shader validation (not a Rust test, but part of quality gate)
naga --validate crates/velos-gpu/shaders/*.wgsl

# Full quality gate (run before every commit)
cargo clippy --all-targets -- -D warnings && cargo test --workspace && cargo bench --bench frame_time
```

## Test File Organization

**Location:**
- Co-located: tests live in `#[cfg(test)] mod tests` at the bottom of source files (Rust convention)
- Integration tests: `tests/` directory at crate root for cross-module testing

**Naming:**
- Test functions: `test_` prefix with descriptive name (e.g., `test_idm_acceleration`, `test_cfl_substep`)
- Benchmark functions: `bench_` prefix (e.g., `bench_frame_10k`, `bench_frame_100k`, `bench_frame_280k`)

**Structure:**
```
crates/
  velos-core/
    src/
      lib.rs
      scheduler.rs       # contains #[cfg(test)] mod tests { ... }
    tests/
      integration.rs     # cross-module integration tests
    benches/
      frame_time.rs      # criterion benchmarks
  velos-vehicle/
    src/
      idm.rs             # contains #[cfg(test)] mod tests { ... }
      mobil.rs
    tests/
      integration.rs
```

## Test Structure

**Unit Test Organization:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_idm_acceleration_free_road() {
        // Arrange: create agent with known parameters
        let params = IDMParams { v0: 50.0, s0: 2.0, T: 1.2, a: 1.5, b: 3.0 };
        let speed = 30.0;  // m/s, below desired
        let gap = 100.0;   // m, large gap (free road)
        let delta_v = 0.0;

        // Act
        let acc = idm_acceleration(speed, gap, delta_v, &params);

        // Assert: on free road, should accelerate toward v0
        assert!(acc > 0.0, "Should accelerate on free road");
        assert!(acc <= params.a, "Should not exceed max acceleration");
    }

    #[test]
    fn test_idm_acceleration_close_following() {
        let params = IDMParams { v0: 50.0, s0: 2.0, T: 1.2, a: 1.5, b: 3.0 };
        let speed = 30.0;
        let gap = 3.0;      // m, very close
        let delta_v = 5.0;  // approaching leader

        let acc = idm_acceleration(speed, gap, delta_v, &params);

        assert!(acc < 0.0, "Should decelerate when close to leader");
        assert!(acc >= -9.0, "Should not exceed physical deceleration limit");
    }
}
```

**Patterns:**
- Arrange-Act-Assert structure
- Descriptive test names that encode the scenario
- Physical bounds assertions (acceleration limits, speed limits, position bounds)
- Use HCMC-specific parameter values from `docs/architect/02-agent-models.md` for test data

## Critical Test Categories

### 1. Numerical Stability Tests

Required for all physics code. Reference: `docs/architect/01-simulation-engine.md` Section 3.

```rust
#[test]
fn test_cfl_substep_short_edge() {
    // Edge shorter than v_max * dt should trigger sub-stepping
    let edge_length = 2.0;  // 2m edge (HCMC alley)
    let speed = 33.3;       // 120 km/h
    let dt = 0.1;
    let cfl = speed * dt / edge_length;
    assert!(cfl > 1.0, "CFL should exceed 1.0 for short edge at high speed");

    let n_sub = cfl.ceil() as u32;
    let sub_dt = dt / n_sub as f32;
    let sub_cfl = speed * sub_dt / edge_length;
    assert!(sub_cfl < 1.0, "Sub-stepped CFL must be < 1.0");
}

#[test]
fn test_idm_no_nan_zero_speed() {
    // Zero speed must not produce NaN (division by zero risk)
    let acc = idm_acceleration(0.0, 10.0, 0.0, &default_params());
    assert!(!acc.is_nan(), "IDM must not produce NaN at zero speed");
    assert!(!acc.is_infinite(), "IDM must not produce infinity at zero speed");
}

#[test]
fn test_idm_no_nan_zero_gap() {
    // Zero gap must not produce NaN
    let acc = idm_acceleration(10.0, 0.0, 5.0, &default_params());
    assert!(!acc.is_nan(), "IDM must not produce NaN at zero gap");
    assert!(acc >= -9.0, "Deceleration must be physically bounded");
}
```

### 2. Cross-GPU Determinism Tests

Reference: `docs/architect/01-simulation-engine.md` Section 4.

```rust
#[test]
fn test_fixed_point_position_roundtrip() {
    // Q16.16 fixed-point must roundtrip without loss for typical positions
    let pos_meters = 1234.567;
    let fixed = (pos_meters * 65536.0) as i32;
    let recovered = fixed as f32 / 65536.0;
    assert!((recovered - pos_meters).abs() < 0.001, "Position roundtrip error too large");
}

#[test]
fn test_fixed_point_multiply_no_overflow() {
    // Verify fix_mul handles large values without overflow
    let a = 30000 * 65536;  // 30km position
    let b = 100 * 1048576;  // 100 m/s speed
    // Must not panic or produce garbage
    let result = fix_mul(a, b, 65536);
    assert!(result > 0, "Fixed-point multiply should produce positive result");
}
```

### 3. Edge Transition Tests

```rust
#[test]
fn test_edge_transition_carry_overflow() {
    // Agent overshooting edge end should carry position to next edge
    let edge_length = 50.0;
    let position = 53.3;  // 3.3m overshoot
    let overflow = position - edge_length;
    assert!((overflow - 3.3).abs() < 0.01);
}

#[test]
fn test_edge_transition_clamp_when_full() {
    // Agent should stop at edge end when next edge has no capacity
    // Position clamped to edge_length, speed set to 0
}
```

### 4. Motorbike Sublane Model Tests

Reference: `docs/architect/02-agent-models.md` Section 1.

```rust
#[test]
fn test_motorbike_no_filter_at_high_speed() {
    // Motorbikes should not attempt filtering above max_filter_speed (20 km/h)
    let filter_params = MotorbikeFilter { min_gap_lateral: 0.8, max_filter_speed: 20.0 / 3.6, .. };
    let speed = 25.0 / 3.6;  // 25 km/h -- above threshold
    let desire = motorbike_lateral_desire(speed, &filter_params);
    assert_eq!(desire, 0.0, "Should not filter above max_filter_speed");
}

#[test]
fn test_motorbike_lateral_position_bounds() {
    // Lateral position must stay within [0, edge_width]
}
```

### 5. CCH Routing Correctness Tests

Reference: `docs/architect/03-routing-prediction.md` Section 1.

```rust
#[test]
fn test_cch_query_matches_dijkstra() {
    // CCH results must exactly match Dijkstra on the same graph
    let graph = load_test_network();
    let cch = CCHRouter::new(&graph);

    for (source, target) in random_pairs(1000) {
        let cch_path = cch.query(source, target);
        let dijkstra_path = dijkstra(&graph, source, target);
        assert_eq!(cch_path.cost(), dijkstra_path.cost(),
            "CCH and Dijkstra must agree for ({}, {})", source, target);
    }
}

#[test]
fn test_cch_customization_updates_paths() {
    // After weight customization, queries must reflect new weights
}
```

### 6. Checkpoint Round-Trip Tests

Reference: `docs/architect/06-infrastructure.md` Section 2.

```rust
#[test]
fn test_checkpoint_save_restore_roundtrip() {
    // Save ECS state to Parquet, restore, verify identical
    let world = create_test_world(1000);  // 1000 agents
    let sim_state = SimState::test_default();

    let mgr = CheckpointManager::new(temp_dir());
    mgr.save(&world, &sim_state).unwrap();

    let mut restored_world = World::new();
    let restored_state = mgr.restore(&checkpoint_path, &mut restored_world).unwrap();

    assert_eq!(world.len(), restored_world.len());
    assert_eq!(sim_state.sim_time, restored_state.sim_time);
    assert_eq!(sim_state.step_number, restored_state.step_number);
    // Verify component-level equality for all agents
}
```

### 7. Gridlock Detection Tests

Reference: `docs/architect/01-simulation-engine.md` Section 7.

```rust
#[test]
fn test_gridlock_detection_finds_cycle() {
    // Create a circular dependency: A waits for B, B waits for C, C waits for A
    // Detector should identify this as a gridlock cluster
}

#[test]
fn test_no_false_gridlock_at_red_light() {
    // Agents stopped at red light should NOT trigger gridlock
    // (they have a non-stalled leader: the signal)
}
```

## Mocking

**Framework:** No external mocking framework specified. Use Rust trait objects and test doubles.

**Patterns:**
```rust
// Define trait for external dependency
pub trait GpuDevice {
    fn dispatch(&self, pipeline: &Pipeline, workgroups: u32);
    fn read_buffer(&self, buffer: &Buffer) -> Vec<u8>;
}

// Test double
#[cfg(test)]
struct MockGpuDevice {
    dispatch_count: Cell<u32>,
}

#[cfg(test)]
impl GpuDevice for MockGpuDevice {
    fn dispatch(&self, _pipeline: &Pipeline, _workgroups: u32) {
        self.dispatch_count.set(self.dispatch_count.get() + 1);
    }
    fn read_buffer(&self, _buffer: &Buffer) -> Vec<u8> { vec![0; 1024] }
}
```

**What to Mock:**
- GPU device/queue (wgpu) -- for unit testing physics logic without GPU
- Redis client -- for testing WebSocket relay without Redis
- File system -- for testing checkpoint save/restore
- Network (HTTP/gRPC clients) -- for testing API layer

**What NOT to Mock:**
- IDM/MOBIL physics calculations -- test with real math
- Fixed-point arithmetic -- test exact integer behavior
- CCH routing -- test against Dijkstra ground truth
- ECS world operations -- use real `hecs::World`

## Fixtures and Factories

**Test Data:**
```rust
// Use HCMC-specific parameter values from architecture docs
fn hcmc_motorbike_params() -> IDMParams {
    IDMParams { v0: 40.0 / 3.6, s0: 1.0, T: 0.8, a: 2.5, b: 4.0 }
}

fn hcmc_car_params() -> IDMParams {
    IDMParams { v0: 50.0 / 3.6, s0: 2.0, T: 1.2, a: 1.5, b: 3.0 }
}

fn create_test_world(agent_count: usize) -> World {
    let mut world = World::new();
    for i in 0..agent_count {
        world.spawn((
            Position { edge_id: i as u32 % 100, lane_idx: 0, offset: FixedQ16_16::from(0.0), lateral: FixedQ8_8::from(1.75) },
            Kinematics { speed: FixedQ12_20::from(10.0), acceleration: FixedQ8_24::from(0.0) },
        ));
    }
    world
}
```

**Location:**
- Test helpers and factories: in `#[cfg(test)]` modules or `tests/common/mod.rs`
- HCMC-specific test data: reference values from `docs/architect/02-agent-models.md`

## Coverage

**Requirements:** Not explicitly enforced, but the author's global instructions (`~/.claude/CLAUDE.md`) mandate: "MUST write unit tests for all new features and bug fixes."

**View Coverage:**
```bash
# Using cargo-tarpaulin or cargo-llvm-cov (install separately)
cargo tarpaulin --workspace --out html
# or
cargo llvm-cov --workspace --html
```

## Test Types

**Unit Tests:**
- Scope: Individual functions and methods within a crate
- Co-located in source files via `#[cfg(test)] mod tests`
- Focus on: physics correctness, numerical stability, edge cases, fixed-point arithmetic

**Integration Tests:**
- Scope: Cross-crate interactions (e.g., velos-core + velos-vehicle + velos-gpu)
- Location: `tests/` directory at crate root
- Focus on: full simulation step pipeline, checkpoint round-trip, CCH query correctness

**Benchmark Tests:**
- Framework: `criterion` (via `cargo bench`)
- Key benchmarks defined in `docs/architect/07-timeline-risks.md`:
  - `bench_frame_10k`: target < 2ms
  - `bench_frame_100k`: target < 10ms
  - `bench_frame_280k`: target < 15ms (after Week 20)
  - `bench_cch_1000`: target < 1ms (1000 CCH queries)
  - `bench_checkpoint`: target < 500ms (280K agent save)
- Regression gate: PR cannot merge if benchmark regresses > 10% vs. `main`

**WGSL Shader Validation:**
- Not a traditional test, but part of CI quality gate
- Command: `naga --validate crates/velos-gpu/shaders/*.wgsl`
- Validates shader correctness at compile time

**Stress Tests (Gate Criteria):**
- G1 (Week 8): 10K vehicles, no crashes or NaN after 1000 steps, frame time < 5ms
- G2 (Week 12): 50K motorbikes + 10K cars, zero lateral collision crashes in 10,000-step stress test
- G3 (Week 20): 280K agents, frame_time p99 < 15ms sustained for 10,000 steps, no memory leaks
- G5 (Week 44): 3 demo scenarios run to completion without crash

## Common Patterns

**Async Testing:**
```rust
#[tokio::test]
async fn test_prediction_ensemble_update() {
    let ensemble = PredictionEnsemble::new(test_config());
    let snapshot = SimSnapshot::test_default();
    ensemble.update(&snapshot).await;

    let overlay = ensemble.overlay.load();
    assert!(overlay.edge_travel_times.len() > 0);
    for &tt in &overlay.edge_travel_times {
        assert!(tt > 0.0, "Travel time must be positive");
        assert!(!tt.is_nan(), "Travel time must not be NaN");
    }
}
```

**Error Testing:**
```rust
#[test]
fn test_checkpoint_restore_corrupted_file() {
    let mgr = CheckpointManager::new(temp_dir());
    let result = mgr.restore(&Path::new("/nonexistent"));
    assert!(result.is_err());
}

#[test]
fn test_route_query_disconnected_nodes() {
    let cch = CCHRouter::new(&test_graph());
    let result = cch.query(0, 99999);  // nonexistent target
    assert!(result.is_none(), "Should return None for unreachable target");
}
```

**Physical Bounds Testing:**
```rust
#[test]
fn test_acceleration_within_physical_limits() {
    // IDM acceleration must always be in [-9.0, a_max]
    for speed in [0.0, 5.0, 10.0, 20.0, 33.3] {
        for gap in [0.1, 1.0, 5.0, 50.0, 1000.0] {
            for delta_v in [-10.0, 0.0, 10.0] {
                let acc = idm_acceleration(speed, gap, delta_v, &default_params());
                assert!(acc >= -9.0 && acc <= default_params().a,
                    "Acceleration {acc} out of physical bounds at v={speed}, s={gap}, dv={delta_v}");
                assert!(!acc.is_nan());
                assert!(!acc.is_infinite());
            }
        }
    }
}
```

## CI Pipeline Quality Gates

Reference: `docs/architect/07-timeline-risks.md` Section 9.

**Every PR (mandatory, blocks merge):**
```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --no-fail-fast
cargo bench --bench frame_time -- --baseline main  # no regression > 10%
naga --validate crates/velos-gpu/shaders/*.wgsl
```

**Weekly performance tracking:**
- Run standard benchmark suite every Friday
- Publish results to Grafana
- Alert if 3 consecutive weeks show regression

---

*Testing analysis: 2026-03-06*
