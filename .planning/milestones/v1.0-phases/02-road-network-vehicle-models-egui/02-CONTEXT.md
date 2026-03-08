# Phase 2: Road Network & Vehicle Models + egui - Context

**Gathered:** 2026-03-06
**Status:** Ready for planning

<domain>
## Phase Boundary

Build a real HCMC road network from OSM, wire IDM car-following and MOBIL lane-change for cars,
spawn agents from OD matrices shaped by time-of-day profiles (80% motorbike / 15% car / 5% pedestrian),
route via A*, control intersections with fixed-time signals, detect gridlock, and add an egui sidebar
with simulation controls and live metrics. Motorbike sublane model (VEH-03) and pedestrian social
force (VEH-04) are Phase 3.

</domain>

<decisions>
## Implementation Decisions

### HCMC road area
- **District 1 core** — Ben Thanh area, dense mixed traffic, major roads
- Source via **Overpass API query** (bounding box around District 1 centroid — no file to maintain)
- Import **highway=primary + secondary + tertiary + residential** (no service roads, alleys, paths)
- **Project to local metres at load time**: equirectangular projection centered on District 1 centroid
  so road coordinates are in metres and match the renderer directly (no camera-level transform needed)

### Phase 2 agent types
- **Motorbikes use IDM as placeholder**: drive like cars (IDM car-following, rightmost lane, no sublane)
  — `VehicleType` enum exists from the start so Phase 3 just swaps the update function
- **Pedestrians walk straight to destination**: linear interpolation toward goal, ignore other agents
  — Phase 3 adds social force model on top
- Both spawned at correct DEM-03 ratios (80%/15%/5%) — demand requirements fully covered in Phase 2
- **Visual distinction by color + shape**: cars = blue rectangles, motorbikes = green triangles
  (reusing existing shape), pedestrians = white dots — one draw call per agent type (extends REN-04 pattern)

### egui layout
- **Left sidebar** (~240px fixed width) — simulation view fills remaining screen, no overlap with agents
- **Controls** (APP-01): Start / Pause / Reset + speed multiplier slider (0.1x – 4x)
- **Metrics** (APP-02): frame time (ms), agent count broken down by type, agents/sec throughput
  — validates PERF-01 and PERF-02 from egui

### Crate additions
- Create **one crate per subsystem**: `velos-net`, `velos-vehicle`, `velos-demand`, `velos-signal`
  — matches planned 12-crate structure, clean single-responsibility boundaries
- **Simulation tick stays in `velos-gpu/app.rs`** — extend `GpuState::update()` to call into new crates
  each frame; refactor later if it exceeds 700 lines
- **Gridlock detection algorithm**: Claude's discretion — either simple visited-set BFS or Tarjan SCC,
  whichever is cleaner for 1K agents on a small District 1 network

</decisions>

<specifics>
## Specific Ideas

- District 1 was chosen because it shows the most interesting mixed traffic (Ben Thanh intersection)
  — motorbikes swarming at red lights is the visual payoff even if sublane model is Phase 3
- The `VehicleType` enum approach is key: the agent struct carries type from spawn, behavior is selected
  per-type in the update loop — no architectural change needed when Phase 3 adds sublane
- egui left sidebar mirrors tools like Blender or simulation dashboards — familiar for engineers

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `VelosApp` / `GpuState` (`crates/velos-gpu/src/app.rs`): `GpuState::update()` is the per-frame hook
  — new subsystem calls go here; `GpuState::new()` initializes all resources
- `Camera2D` (`crates/velos-gpu/src/camera.rs`): already in metres with zoom/pan — road network
  projected to metres plugs in directly, no changes needed
- `Renderer` + `AgentInstance` (`crates/velos-gpu/src/renderer.rs`): `update_instances_from_cpu`
  accepts positions + headings arrays — extend to accept per-type instance arrays and add
  rectangle + dot shape vertex buffers for cars and pedestrians
- `BufferPool` (`crates/velos-gpu/src/buffers.rs`): currently hardcoded to 1024-agent capacity —
  needs to be driven by actual spawned agent count (or capacity enlarged)
- `ComputeDispatcher` (`crates/velos-gpu/src/compute.rs`): dispatches agent_update.wgsl — Phase 2
  may keep or extend the compute shader depending on whether IDM runs GPU-side or CPU-side
- `Position` + `Kinematics` (`crates/velos-core/src/components.rs`): f64 CPU-side components —
  new `VehicleType` component and road-edge index component will be added here

### Established Patterns
- **One draw call per agent type** (REN-04): established in Phase 1 — extend Renderer to have
  triangle/rectangle/dot pipeline or swap shape vertex buffer per draw call
- **ECS component projection to GPU SoA**: `upload_from_ecs` pattern in BufferPool — new agent
  components (vehicle type, lane position) follow the same pattern
- **Double-buffered SoA GPU buffers**: pos_front/back, kin_front/back — any new GPU-side state
  (e.g., agent road-edge index) follows this layout
- **f64 CPU / f32 GPU**: all physics (IDM, MOBIL) runs in f64 on CPU, casts to f32 before upload

### Integration Points
- `GpuState::update()` in `app.rs` → call `velos-vehicle` IDM/MOBIL step, `velos-signal` tick,
  `velos-net` neighbor queries, `velos-demand` spawner, then GPU upload + dispatch + readback
- `Renderer::render_frame()` → needs per-type instance arrays (cars, motorbikes, pedestrians)
  rather than one unified array — API change required
- `velos-net` road graph coordinates → equirectangular projection to metres → directly usable
  as `Position {x, y}` in ECS — no camera-level transform
- egui-wgpu integration: render egui on the same wgpu device/surface after the simulation render
  pass — standard egui-wgpu pattern, VelosApp::window_event handles egui input passthrough

</code_context>

<deferred>
## Deferred Ideas

- Scenario selector dropdown in egui — useful once multiple OD configs exist (Phase 3+)
- Queue length metrics per intersection in egui — add after gridlock detection is proven
- Step-by-step single-frame advance button — useful for debugging Phase 3 sublane behavior

</deferred>

---

*Phase: 02-road-network-vehicle-models-egui*
*Context gathered: 2026-03-06*
