---
phase: 02-road-network-vehicle-models-egui
plan: 04
status: complete
started: 2026-03-06
completed: 2026-03-06
duration: ~25min (including interactive fixes)
---

# Plan 02-04: Integration + egui UI

## What Was Built

Wired all Phase 2 subsystems (velos-net, velos-vehicle, velos-signal, velos-demand) into the simulation loop with real-time rendering and egui controls.

## Key Decisions

- **wgpu kept at 27** for egui-wgpu 0.33 compatibility (plan called for downgrade from 28)
- **Road line rendering added** via separate LineList pipeline + road_line.wgsl shader
- **Signal approach matching** uses incoming edges (not outgoing) for correct red/green assignment
- **Zone centroids derived from network bounds** (not hardcoded) and randomized within 300m radius
- **OD matrix boosted 10x** for visible demo density (~5600 trips/hr at peak)
- **Sim starts at 7:00 AM** (morning rush, ToD factor=1.0) instead of midnight

## Tasks Completed

| # | Task | Commit | Files |
|---|------|--------|-------|
| 1 | egui deps + per-type renderer shapes | 65eb583 | Cargo.toml, velos-gpu/Cargo.toml, renderer.rs |
| 2 | Wire subsystems + egui sidebar | effec29 | sim.rs (new), app.rs, components.rs, lib.rs, compute.rs |
| 3 | Visual verification (human-verify) | a6a061e | app.rs, renderer.rs, sim.rs, road_line.wgsl |

## Deviations

- Road network line rendering was NOT in original plan but required for usability (agents invisible without road context)
- Signal logic had 2 bugs fixed during verification: incoming edge matching + red signal clamping
- Zone centroids refactored from hardcoded to network-derived for correct spawn distribution

## Key Files

### Created
- `crates/velos-gpu/src/sim.rs` — simulation tick logic, agent spawning, vehicle stepping, signal checking, gridlock detection
- `crates/velos-gpu/shaders/road_line.wgsl` — line shader for road network rendering

### Modified
- `crates/velos-gpu/src/app.rs` — egui integration, road line upload, signal indicators
- `crates/velos-gpu/src/renderer.rs` — per-type shape rendering, road line pipeline, signal dots
- `crates/velos-core/src/components.rs` — VehicleType, RoadPosition, Route, WaitState ECS components

## Self-Check

- [x] Agents spawn from OD matrices across District 1 road network
- [x] IDM car-following and MOBIL lane-change operate
- [x] Traffic signals cycle and agents stop at red lights
- [x] Three agent types render with distinct shapes/colors
- [x] egui sidebar with Start/Pause/Reset + speed slider + live metrics
- [x] Road network visible as line overlay
- [x] Signal state indicators at intersections
- [x] Camera pan/zoom not stolen by egui
- [x] Human verification: APPROVED
