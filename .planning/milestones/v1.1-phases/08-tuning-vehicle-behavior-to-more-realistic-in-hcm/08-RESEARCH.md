# Phase 8: Tuning Vehicle Behavior to More Realistic in HCM - Research

**Researched:** 2026-03-08
**Domain:** Vehicle behavior parameter calibration, HCMC mixed-traffic realism, GPU/CPU parameter unification
**Confidence:** HIGH

## Summary

This phase involves three distinct workstreams: (1) extracting ~50 hardcoded vehicle behavior parameters into a TOML config file with per-vehicle-type sections, (2) fixing the GPU/CPU parameter mismatch by replacing WGSL shader constants with uniform buffer reads, and (3) adding HCMC-specific behavioral rules (red-light creep, aggressive weaving, yield-based intersection negotiation) while calibrating parameter defaults to HCMC field data ranges.

The codebase is well-structured for this work. All parameter structs (`IdmParams`, `KraussParams`, `SublaneParams`, `MobilParams`, `SocialForceParams`) already exist as standalone structs with factory/default functions. The GPU shader (`wave_front.wgsl`) has 11 hardcoded constants that must be replaced with uniform buffer reads. The existing `toml` and `serde` workspace dependencies, plus established TOML config patterns in `velos-net` and `velos-meso`, provide a clear template for the config loading infrastructure.

**Primary recommendation:** Start with parameter externalization (TOML config + uniform buffer), then tune defaults, then add behavioral rules. This ordering ensures all parameters are configurable before behavioral work begins, avoiding re-hardcoding.

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **Red-light creep:** Motorbikes inch forward past the stop line during red, forming a dense swarm ahead of cars. When green comes, the motorbike swarm launches first. Virtually no motorbike fully stops behind the stop line.
- **Aggressive weaving:** Motorbikes actively seek gaps between cars/trucks/buses to pass, even in the same lane. They squeeze through 0.5m gaps at low speed differences. Motorbikes treat lanes as suggestions.
- **No wrong-way riding:** All vehicles obey one-way restrictions. Simpler model, avoids head-on collision complexity.
- **Yield-based intersection chaos:** At unsignalized intersections, all vehicles negotiate through gap acceptance with low gap thresholds (1.0-1.5s TTC). Motorbikes are more aggressive (lower threshold). No strict priority rules -- first-come-first-served with size intimidation.
- **Parameter externalization:** ~50 params into `data/hcmc/vehicle_params.toml`, per-vehicle-type sections, loaded at startup, no hot-reload.
- **GPU/CPU mismatch fix:** GPU shader reads from uniform buffer populated from config, not hardcoded constants.
- **Calibration targets:** Specific v0 ranges per vehicle type (motorbike 35-45 km/h, car 30-40, bus 25-35, truck 30-40, bicycle 12-18, pedestrian 1.0-1.4 m/s).
- **Validation:** Visual inspection primary, speed distributions, flow-density diagrams. GEH deferred to v2.

### Claude's Discretion
- Exact parameter values within specified ranges
- TOML config file structure and field naming
- GPU uniform buffer layout for externalized parameters
- Red-light creep implementation approach (gradual drift vs discrete jumps)
- Weaving aggressiveness model (speed-dependent gap threshold function)
- Intersection negotiation algorithm details
- Validation test scenarios
- Road-class-dependent desired speeds vs single default per vehicle type
- Pedestrian jaywalking rate formula per road type

### Deferred Ideas (OUT OF SCOPE)
None -- discussion stayed within phase scope
</user_constraints>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| toml | 0.8 | TOML config file parsing | Already in workspace, used by velos-net and velos-meso |
| serde | 1.x (derive) | Struct deserialization from TOML | Already in workspace, standard Rust pattern |
| bytemuck | (workspace) | GPU buffer byte casting | Already used for WaveFrontParams, GpuAgentState |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| log | (workspace) | Parameter loading diagnostics | Config validation warnings |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| TOML file | JSON/YAML | TOML is human-readable for numeric params, already used in project |
| Uniform buffer array | Storage buffer | Uniform buffer is faster for small read-only data, 7 vehicle types * ~10 params = ~280 floats fits easily |

**Installation:**
No new dependencies needed. `toml` and `serde` already in workspace `Cargo.toml`. Add `toml.workspace = true` and `serde.workspace = true` to `velos-vehicle/Cargo.toml`.

## Architecture Patterns

### Recommended Config Structure

```toml
# data/hcmc/vehicle_params.toml

[motorbike]
v0 = 11.1          # 40 km/h desired speed (range: 35-45 km/h = 9.7-12.5 m/s)
s0 = 1.0            # minimum gap (m)
t_headway = 0.8     # aggressive following (s)
a = 2.0             # max accel (m/s^2)
b = 3.0             # comfortable decel (m/s^2)
delta = 4.0
krauss_sigma = 0.3  # less dawdle than cars
min_filter_gap = 0.5 # aggressive filtering (m)
max_lateral_speed = 1.2
politeness = 0.1    # very selfish lane change
gap_acceptance_ttc = 1.0  # aggressive at intersections (s)

[car]
v0 = 9.7            # 35 km/h (range: 30-40 km/h = 8.3-11.1 m/s)
s0 = 2.0
t_headway = 1.5
a = 1.0
b = 2.0
delta = 4.0
krauss_sigma = 0.5
politeness = 0.3
gap_acceptance_ttc = 1.5

# ... similar for bus, truck, bicycle, emergency, pedestrian
```

### Recommended Rust Config Struct

```rust
// velos-vehicle/src/config.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct VehicleConfig {
    pub motorbike: VehicleTypeParams,
    pub car: VehicleTypeParams,
    pub bus: VehicleTypeParams,
    pub truck: VehicleTypeParams,
    pub bicycle: VehicleTypeParams,
    pub emergency: VehicleTypeParams,
    pub pedestrian: PedestrianParams,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VehicleTypeParams {
    // IDM
    pub v0: f64,
    pub s0: f64,
    pub t_headway: f64,
    pub a: f64,
    pub b: f64,
    #[serde(default = "default_delta")]
    pub delta: f64,
    // Krauss
    pub krauss_sigma: f64,
    // MOBIL
    pub politeness: f64,
    #[serde(default = "default_threshold")]
    pub threshold: f64,
    // Intersection
    pub gap_acceptance_ttc: f64,
    // Sublane (optional, only motorbike/bicycle)
    #[serde(default)]
    pub min_filter_gap: Option<f64>,
    #[serde(default)]
    pub max_lateral_speed: Option<f64>,
}
```

### GPU Uniform Buffer Layout for Vehicle Params

The current `WaveFrontParams` struct is 32 bytes with 2 padding fields. The vehicle-type parameters need a separate uniform buffer (binding 7) to avoid breaking existing layout.

```rust
// 7 vehicle types * 8 floats = 56 floats = 224 bytes
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVehicleParams {
    // Per vehicle type (indexed by vehicle_type u32):
    // [v0, s0, t_headway, a, b, krauss_accel, krauss_decel, krauss_sigma]
    params: [[f32; 8]; 7],
}
```

WGSL side:
```wgsl
struct VehicleTypeParams {
    v0: f32,
    s0: f32,
    t_headway: f32,
    a: f32,
    b: f32,
    krauss_accel: f32,
    krauss_decel: f32,
    krauss_sigma: f32,
}

@group(0) @binding(7) var<uniform> vehicle_params: array<VehicleTypeParams, 7>;
```

Then replace hardcoded constants in shader:
```wgsl
// BEFORE:
let v_ratio = v / IDM_V0;
// AFTER:
let vp = vehicle_params[agent.vehicle_type];
let v_ratio = v / vp.v0;
```

### Pattern: Red-Light Creep

The existing `sublane.rs` already has red-light swarming mode in `compute_desired_lateral()` that activates when `at_red_light == true`. Red-light creep extends this with longitudinal forward drift:

```rust
// In red-light creep logic (CPU side, integrated into tick):
pub fn red_light_creep_speed(
    distance_to_stop_line: f64,
    vehicle_type: VehicleType,
    params: &VehicleTypeParams,
) -> f64 {
    if vehicle_type != VehicleType::Motorbike && vehicle_type != VehicleType::Bicycle {
        return 0.0; // Only motorbikes/bicycles creep
    }
    if distance_to_stop_line < 0.5 {
        return 0.0; // Already past stop line
    }
    // Gradual drift: 0.3 m/s when > 2m from stop line, slowing near stop line
    let creep = 0.3 * (distance_to_stop_line / 5.0).min(1.0);
    creep
}
```

### Pattern: Intersection Gap Acceptance with Size Factor

```rust
pub fn intersection_gap_acceptance(
    own_type: VehicleType,
    other_type: VehicleType,
    ttc: f64,
    own_ttc_threshold: f64,
) -> bool {
    // Size intimidation factor: larger vehicles have lower effective threshold
    let size_factor = match other_type {
        VehicleType::Truck | VehicleType::Bus => 1.3, // more cautious around big vehicles
        VehicleType::Emergency => 2.0,                  // always yield to emergency
        VehicleType::Motorbike | VehicleType::Bicycle => 0.8, // less cautious around small
        _ => 1.0,
    };
    ttc > own_ttc_threshold * size_factor
}
```

### Anti-Patterns to Avoid
- **Hardcoding new parameters in behavioral rules:** Every new parameter (creep speed, size factor, gap threshold) MUST go through the TOML config, not as const in source.
- **Separate GPU parameter buffers per vehicle type:** Use a single array-indexed buffer, not 7 separate bindings.
- **Changing GpuAgentState layout:** The 40-byte struct is established. Add new flags via the existing `flags` bitfield, not new fields.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| TOML parsing | Custom parser | `toml::from_str` + serde derive | Existing pattern in velos-net, velos-meso |
| GPU buffer alignment | Manual padding | `bytemuck::Pod` + `#[repr(C)]` | Already used everywhere, handles alignment |
| Per-vehicle-type dispatch | Match chains in shader | Array indexing by vehicle_type u32 | `vehicle_params[agent.vehicle_type]` is O(1) and cleaner |
| Parameter validation | Ad-hoc checks | `VehicleConfig::validate()` method | Centralized range checking with descriptive errors |

## Common Pitfalls

### Pitfall 1: GPU/CPU Parameter Drift After Externalization
**What goes wrong:** After extracting to TOML, the GPU and CPU code paths still use different parameter sources if not wired correctly.
**Why it happens:** GPU reads from uniform buffer, CPU reads from `VehicleConfig` struct -- if the uniform buffer upload is missed, they diverge again.
**How to avoid:** Single source of truth: `VehicleConfig` loads from TOML, factory functions read from it, and `GpuVehicleParams` is derived from the same `VehicleConfig` at upload time.
**Warning signs:** CPU reference test results differ from GPU readback results.

### Pitfall 2: WGSL Uniform Buffer Alignment
**What goes wrong:** Struct members get wrong values due to WGSL alignment rules.
**Why it happens:** WGSL requires 16-byte alignment for struct members in arrays. An `array<VehicleTypeParams, 7>` where `VehicleTypeParams` has 8 f32 fields (32 bytes) is fine (32 is divisible by 16), but if you change to 7 or 9 fields, alignment breaks silently.
**How to avoid:** Keep `VehicleTypeParams` at 8 f32 fields (32 bytes, 16-byte aligned). If more fields needed, pad to 12 (48 bytes) or 16 (64 bytes). Always use `bytemuck::Pod` on Rust side to catch size mismatches at compile time.
**Warning signs:** Some vehicle types get correct params, others get garbage values.

### Pitfall 3: Red-Light Creep Causing Collisions
**What goes wrong:** Motorbikes creep past stop line into the intersection and get hit by cross-traffic.
**Why it happens:** Creep doesn't check for conflicting green phase or cross-traffic.
**How to avoid:** Creep only moves forward within a bounded zone (e.g., up to 3m past stop line). Cross-traffic interaction handled by existing gap acceptance logic. Creep speed reduces to 0 when at swarm front.
**Warning signs:** Motorbikes appear in the middle of intersections during red.

### Pitfall 4: Truck v0 Change Breaking Existing Tests
**What goes wrong:** Changing truck v0 from 90 km/h to 35 km/h breaks `types_tests.rs` assertions.
**Why it happens:** Tests assert exact parameter values from `default_idm_params()`.
**How to avoid:** Update tests to assert config-loaded values. The `default_idm_params()` function should return HCMC-calibrated defaults (from hardcoded fallback matching the TOML), not literature values.
**Warning signs:** `cargo test -p velos-vehicle` fails after parameter changes.

### Pitfall 5: Intersection Negotiation Deadlock
**What goes wrong:** All vehicles at an unsignalized intersection wait for each other, none proceeds.
**Why it happens:** Symmetric gap acceptance with no tie-breaking mechanism.
**How to avoid:** Add a first-come priority: vehicle that arrived first at the intersection gets lower TTC threshold. If simultaneous, larger vehicle proceeds (size intimidation). Add a max-wait timer (3-5s) that forces acceptance.
**Warning signs:** Agents accumulate at unsignalized intersections and never clear.

## Code Examples

### Existing Parameter Mismatch (MUST FIX)

GPU `wave_front.wgsl` hardcoded values vs CPU `types.rs` defaults:

| Parameter | GPU (wave_front.wgsl) | CPU (types.rs Car) | Mismatch |
|-----------|----------------------|-------------------|----------|
| IDM a | 1.5 | 1.0 | YES |
| IDM b | 3.0 | 2.0 | YES |
| IDM v0 | 13.89 (50 km/h) | 13.9 (50 km/h) | Minor |
| Krauss accel | 2.6 | 2.6 | OK |
| Krauss sigma | 0.5 | 0.5 | OK |

The GPU uses `IDM_A = 1.5` and `IDM_B = 3.0` while CPU Car defaults are `a = 1.0` and `b = 2.0`. This is the documented mismatch that must be resolved by making both read from the same config.

### TOML Loading Pattern (from velos-meso/zone_config.rs)

```rust
// Established project pattern:
pub fn load_from_toml_str(toml_str: &str) -> Result<Self, Error> {
    toml::from_str(toml_str).map_err(|e| Error::ConfigParse(e.to_string()))
}
```

### Existing Flag Bits in GpuAgentState

```
flags bitfield:
  bit 0: FLAG_BUS_DWELLING (0x01)
  bit 1: FLAG_EMERGENCY_ACTIVE (0x02)
  bit 2: FLAG_YIELDING (0x04)
  bits 3-31: available for new flags
```

New flags needed:
- bit 3: `FLAG_AT_RED_LIGHT` (0x08) -- enables red-light creep behavior
- bit 4: `FLAG_AT_UNSIGNALIZED_INTERSECTION` (0x10) -- enables gap acceptance

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single IDM params for all types | Per-type IDM defaults in `default_idm_params()` | Phase 6 (06-01) | Each type has different v0, s0, etc. |
| Hardcoded WGSL constants | To be replaced with uniform buffer | This phase | Eliminates GPU/CPU mismatch |
| Fixed 2.0s gap acceptance (jaywalking) | To be vehicle-type-dependent | This phase | Motorbikes 1.0s, cars 1.5s, trucks 2.0s |
| Truck v0 = 25.0 m/s (90 km/h) | Should be 8.3-11.1 m/s (30-40 km/h) | This phase | Trucks crawl in HCMC urban, not highway speed |
| Car v0 = 13.9 m/s (50 km/h) | Should be 8.3-11.1 m/s (30-40 km/h) | This phase | HCMC congestion means lower desired speeds |

**HCMC speed context (from traffic studies):**
- Average motorcycle speed during rush hour on main HCMC roads: 12-21 km/h
- Urban speed limits for motorcycles: 30-40 km/h
- Desired speed (v0) represents free-flow aspiration, not actual congested speed
- IDM naturally reduces speed in congestion; v0 should be the free-flow target

## Open Questions

1. **Road-class-dependent v0 vs single per-type v0**
   - What we know: CONTEXT.md says motorbike v0 = 35-45 km/h "road-class dependent"
   - What's unclear: Whether to implement road-class override as config multiplier or per-road-class v0 table
   - Recommendation: Use per-vehicle-type base v0 in config, with optional `[speed_limits_by_road_class]` section that overrides. The existing `RoadClass` enum (Residential, Tertiary, Secondary, Primary, Trunk, Motorway, Service) can key the overrides. Start simple: single v0 per type, add road-class modifiers if visual validation shows wrong speeds.

2. **GPU-side red-light creep vs CPU-only**
   - What we know: Red-light creep is longitudinal forward movement. The GPU shader handles longitudinal physics.
   - What's unclear: Whether to implement creep in GPU shader or as CPU pre-processing that sets a creep speed before GPU dispatch.
   - Recommendation: CPU-side. Set a small positive speed on GPU agent state before dispatch when `FLAG_AT_RED_LIGHT` is set and vehicle is motorbike. The GPU IDM will then use this as the initial speed. Simpler than adding creep logic to WGSL.

3. **Weaving gap threshold function shape**
   - What we know: Motorbikes squeeze through 0.5m gaps at low speed differences. Current `min_filter_gap` is 0.6m.
   - What's unclear: Whether gap threshold should scale with speed difference (larger gaps needed at higher delta-v) or remain constant.
   - Recommendation: Speed-dependent: `effective_gap = min_filter_gap + 0.1 * delta_v_abs`. At 0 speed difference, 0.5m gap accepted. At 5 m/s difference, need 1.0m gap. This prevents unrealistic high-speed squeezes.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | cargo test (built-in) |
| Config file | Cargo.toml workspace |
| Quick run command | `cargo test -p velos-vehicle` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements -> Test Map

This phase has no formal requirement IDs (TBD in REQUIREMENTS.md). Mapping behavioral goals to tests:

| Goal | Behavior | Test Type | Automated Command | File Exists? |
|------|----------|-----------|-------------------|-------------|
| TOML config load | Config parses and populates all 7 vehicle types | unit | `cargo test -p velos-vehicle config` | No -- Wave 0 |
| GPU/CPU parity | Uniform buffer params match config-loaded CPU params | unit | `cargo test -p velos-vehicle gpu_params` | No -- Wave 0 |
| IDM with config params | IDM acceleration uses config v0, not hardcoded | unit | `cargo test -p velos-vehicle idm` | Yes (update) |
| Krauss per-type sigma | Krauss dawdle varies by vehicle type | unit | `cargo test -p velos-vehicle krauss` | Yes (update) |
| Red-light creep | Motorbikes advance past stop line at red | unit | `cargo test -p velos-vehicle creep` | No -- Wave 0 |
| Aggressive weaving | Sublane gap < 0.6m accepted for motorbikes | unit | `cargo test -p velos-vehicle sublane` | Yes (update) |
| Intersection gap accept | Vehicle-type-dependent TTC thresholds | unit | `cargo test -p velos-vehicle intersection` | No -- Wave 0 |
| Parameter ranges | All params within HCMC-specified ranges | unit | `cargo test -p velos-vehicle validate` | No -- Wave 0 |
| Truck v0 corrected | Truck v0 = 30-40 km/h not 90 km/h | unit | `cargo test -p velos-vehicle types` | Yes (update) |

### Sampling Rate
- **Per task commit:** `cargo test -p velos-vehicle && cargo test -p velos-gpu`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full workspace green before verify

### Wave 0 Gaps
- [ ] `crates/velos-vehicle/src/config.rs` -- VehicleConfig struct with TOML deserialization
- [ ] `crates/velos-vehicle/tests/config_tests.rs` -- config loading, validation, default fallback
- [ ] `data/hcmc/vehicle_params.toml` -- HCMC-calibrated parameter defaults
- [ ] `crates/velos-vehicle/tests/intersection_tests.rs` -- gap acceptance logic
- [ ] Update `crates/velos-vehicle/tests/types_tests.rs` -- assertions for new HCMC defaults
- [ ] Update `crates/velos-vehicle/tests/sublane_tests.rs` -- aggressive weaving thresholds

## Sources

### Primary (HIGH confidence)
- Codebase analysis: `crates/velos-vehicle/src/types.rs` -- current parameter defaults and VehicleType enum
- Codebase analysis: `crates/velos-gpu/shaders/wave_front.wgsl` -- GPU hardcoded constants (lines 61-78)
- Codebase analysis: `crates/velos-gpu/src/compute.rs` -- WaveFrontParams struct and buffer layout
- Codebase analysis: `crates/velos-vehicle/src/sublane.rs` -- existing swarming logic
- Codebase analysis: `crates/velos-vehicle/src/social_force.rs` -- jaywalking gap acceptance
- Codebase analysis: `crates/velos-meso/src/zone_config.rs` -- TOML loading pattern
- CONTEXT.md -- all locked decisions and parameter ranges

### Secondary (MEDIUM confidence)
- [SUMO Sublane Model docs](https://sumo.dlr.de/docs/Simulation/SublaneModel.html) -- sublane filtering behavior reference
- [IDM Wikipedia](https://en.wikipedia.org/wiki/Intelligent_driver_model) -- standard IDM parameter ranges
- [HCMC traffic speed study](https://www.mdpi.com/2071-1050/13/10/5577) -- average speeds 12-21 km/h rush hour
- [Vietnam speed limits](https://www.hanoijourney.com/speed-limits-in-vietnam/) -- urban motorcycle limits 30-40 km/h

### Tertiary (LOW confidence)
- General traffic engineering literature for gap acceptance TTC thresholds (1.0-2.0s range is standard, HCMC-specific calibration not available in published studies)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- all dependencies already in workspace, patterns established
- Architecture: HIGH -- clear struct migration path, uniform buffer extension straightforward
- Pitfalls: HIGH -- GPU/CPU mismatch well-documented in code, alignment rules well-known
- HCMC parameter values: MEDIUM -- ranges from CONTEXT.md user decisions, supported by traffic literature but not from controlled calibration studies
- Behavioral rules (creep, weaving, intersection): MEDIUM -- implementation approaches are sound engineering but HCMC-specific tuning is qualitative

**Research date:** 2026-03-08
**Valid until:** 2026-04-08 (stable domain, no fast-moving dependencies)
