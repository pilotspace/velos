# Phase 8: Tuning Vehicle Behavior to More Realistic in HCM - Context

**Gathered:** 2026-03-08
**Status:** Ready for planning

<domain>
## Phase Boundary

Tune all vehicle behavior parameters and interaction models to produce realistic HCMC mixed-traffic patterns. This includes: extracting hardcoded parameters into config files, fixing GPU/CPU parameter mismatches, calibrating per-vehicle-type defaults to HCMC field data, adding HCMC-specific behavioral rules (red-light creep, aggressive weaving, yield-based intersection chaos), and establishing a validation methodology to verify realism improvements.

This phase does NOT add new vehicle types, new simulation features, or new infrastructure. It tunes what exists.

</domain>

<decisions>
## Implementation Decisions

### HCMC-Specific Behaviors
- **Red-light creep:** Motorbikes inch forward past the stop line during red, forming a dense swarm ahead of cars. When green comes, the motorbike swarm launches first. Virtually no motorbike fully stops behind the stop line.
- **Aggressive weaving:** Motorbikes actively seek gaps between cars/trucks/buses to pass, even in the same lane. They squeeze through 0.5m gaps at low speed differences. Motorbikes treat lanes as suggestions.
- **No wrong-way riding:** All vehicles obey one-way restrictions. Simpler model, avoids head-on collision complexity. U-turn points from Phase 5 network remain available.
- **Yield-based intersection chaos:** At unsignalized intersections, all vehicles negotiate through gap acceptance with low gap thresholds (1.0-1.5s TTC). Motorbikes are more aggressive (lower threshold). No strict priority rules — first-come-first-served with size intimidation (trucks yield less).

### Parameter Externalization
- Extract all ~50 hardcoded vehicle behavior parameters into a TOML config file (`data/hcmc/vehicle_params.toml`)
- Per-vehicle-type parameter sections: `[motorbike]`, `[car]`, `[bus]`, `[truck]`, `[bicycle]`, `[emergency]`, `[pedestrian]`
- Config loaded at startup, no hot-reload needed (restart is acceptable for parameter changes)
- Fix GPU/CPU IDM parameter mismatch: GPU shader reads from uniform buffer populated from config, not hardcoded constants
- GPU shader constants replaced with uniform buffer values for all vehicle-type-specific parameters

### Parameter Calibration Targets
- **Motorbike:** v0=35-45 km/h (road-class dependent), min_filter_gap=0.4-0.6m, aggressive gap acceptance TTC=1.0s
- **Car:** v0=30-40 km/h urban (not 50 km/h — HCMC is congested), gap acceptance TTC=1.5s
- **Bus:** v0=25-35 km/h (slower than cars due to stops), dwell model parameters validated against HCMC bus data
- **Truck:** v0=30-40 km/h urban (not 90 km/h — trucks crawl in HCMC city center), larger safe gap
- **Bicycle:** v0=12-18 km/h, hugs right edge, similar filtering to motorbike but slower
- **Pedestrian:** desired_speed=1.0-1.4 m/s, jaywalking rate adjusted per road type (higher on small streets, lower on arterials)
- **Krauss:** HCMC-specific sigma values per vehicle type (motorbikes less dawdle than cars)

### Validation Approach
- Visual inspection as primary validation: motorbike swarming, intersection negotiation, and mixed-traffic flow should "look right" to someone familiar with HCMC traffic
- Speed distribution curves: compare simulated speed distributions per road class against HCMC field observation ranges from traffic engineering literature
- Flow-density fundamental diagrams: verify the simulation produces realistic flow-density relationships for mixed traffic
- GEH statistic deferred to velos-calibrate (v2) — this phase tunes to qualitative realism, not quantitative calibration against loop detector counts

### Claude's Discretion
- Exact parameter values within the ranges specified above (tune iteratively)
- TOML config file structure and field naming
- GPU uniform buffer layout for externalized parameters
- Red-light creep implementation approach (gradual forward drift vs discrete position jumps)
- Weaving aggressiveness model (speed-dependent gap threshold function)
- Intersection negotiation algorithm details (gap acceptance with size factor)
- Validation test scenarios (which intersections/corridors to check)
- Whether to use road-class-dependent desired speeds or a single default per vehicle type
- Pedestrian jaywalking rate adjustment formula per road type

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `IdmParams` struct (velos-vehicle/src/types.rs): Per-vehicle-type defaults — replace with config-loaded values
- `KraussParams` struct (velos-vehicle/src/krauss.rs): Single SUMO default — needs per-vehicle-type variants
- `SublaneParams` struct (velos-vehicle/src/sublane.rs): min_filter_gap, max_lateral_speed — tune for aggressive weaving
- `SocialForceParams` struct (velos-vehicle/src/social_force.rs): HCMC walking speed defaults
- `MobilParams` struct (velos-vehicle/src/types.rs): Lane change thresholds — tune politeness down for HCMC
- `should_jaywalk()` (velos-vehicle/src/social_force.rs): Hardcoded 30%/10% — make configurable per road type
- `wave_front.wgsl` (velos-gpu/shaders): GPU IDM/Krauss constants — replace with uniform buffer reads

### Established Patterns
- Factory functions pattern (`default_idm_params()`, `default_mobil_params()`) — replace with config loading
- Demand-config-driven assignment (Phase 5/6) — extend pattern to behavior params
- GPU uniform buffer for WaveFrontParams — extend for vehicle behavior params
- Per-vehicle-type branching in GPU shader (VehicleType enum u32 dispatches)

### Integration Points
- `wave_front.wgsl`: Hardcoded IDM_V0, IDM_S0, etc. constants → uniform buffer fields
- `types.rs` factory functions: Return config-loaded values instead of hardcoded defaults
- `sublane.rs`: Add red-light creep behavior to existing swarm logic
- Intersection gap acceptance: Currently 2.0s TTC → make vehicle-type-dependent
- `spawner` (velos-demand): Wire config-loaded params into agent spawning

</code_context>

<specifics>
## Specific Ideas

- Red-light creep is the signature HCMC behavior — motorbikes inch past stop line and launch first on green. This single behavior dramatically changes intersection visual realism.
- Aggressive weaving means motorbikes squeeze through 0.5m gaps between larger vehicles. The current sublane model supports this but min_filter_gap needs tuning down.
- Intersection chaos with vehicle-size-dependent gap acceptance: trucks with low threshold (they don't yield much), motorbikes with high aggressiveness (they dart through gaps).
- Truck v0 of 90 km/h is completely wrong for HCMC urban — should be 30-40 km/h in city center.
- GPU/CPU parameter mismatch (IDM a=1.5/b=3.0 on GPU vs a=1.0/b=2.0 on CPU) must be fixed as part of externalization.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 08-tuning-vehicle-behavior-to-more-realistic-in-hcm*
*Context gathered: 2026-03-08*
