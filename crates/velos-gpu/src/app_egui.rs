//! egui panel rendering extracted from app.rs to stay under 700 lines.
//!
//! Draws the left side panel with simulation controls, metrics, vehicle legend,
//! debug overlays, view mode toggle, and calibration panel.

use crate::app_input;
use crate::camera::Camera2D;
use crate::orbit_camera::{OrbitCamera, ViewMode, ViewTransition};
use crate::sim::{SimState, SimWorld};

/// State refs needed by the egui panel draw function.
pub struct EguiPanelState<'a> {
    pub sim: &'a mut SimWorld,
    pub show_guide_lines: &'a mut bool,
    pub show_conflict_debug: &'a mut bool,
    pub show_cameras: &'a mut bool,
    pub show_speed_overlay: &'a mut bool,
    pub grpc_addr: &'a str,
    pub view_mode: &'a mut ViewMode,
    pub view_transition: &'a mut Option<ViewTransition>,
    pub camera_2d: &'a mut Camera2D,
    pub orbit_camera: &'a mut OrbitCamera,
}

/// Draw the left control panel.
pub fn draw_control_panel(ui: &mut egui::Ui, s: &mut EguiPanelState<'_>) {
    ui.heading("VELOS");
    ui.separator();

    ui.heading("Controls");
    let is_running = s.sim.sim_state == SimState::Running;
    let btn_text = if is_running { "Pause" } else { "Start" };
    if ui.button(btn_text).clicked() {
        s.sim.sim_state = if is_running {
            SimState::Paused
        } else {
            SimState::Running
        };
    }
    if ui.button("Reset").clicked() {
        s.sim.reset();
    }
    ui.add(egui::Slider::new(&mut s.sim.speed_mult, 0.1..=4.0).text("Speed"));

    // View mode toggle
    let mode_label = match *s.view_mode {
        ViewMode::TopDown2D => "View: 2D",
        ViewMode::Perspective3D => "View: 3D",
    };
    if ui.button(format!("[V] {mode_label}")).clicked() && s.view_transition.is_none() {
        *s.view_transition = Some(app_input::toggle_view_mode(
            *s.view_mode,
            s.camera_2d,
            s.orbit_camera,
        ));
    }
    ui.separator();

    draw_metrics(ui, s.sim);
    ui.separator();

    draw_vehicle_legend(ui, s.sim);

    ui.separator();
    ui.heading("Debug Overlays");
    ui.checkbox(s.show_guide_lines, "Show Guide Lines");
    ui.checkbox(s.show_conflict_debug, "Show Conflict Debug");
    ui.checkbox(s.show_cameras, "Show Cameras");
    ui.checkbox(s.show_speed_overlay, "Show Speed Overlay");

    ui.separator();
    ui.checkbox(&mut s.sim.show_calibration_panel, "Calibration Panel");

    if s.sim.show_calibration_panel {
        draw_calibration_panel(ui, s.sim, s.grpc_addr);
    }
}

fn draw_metrics(ui: &mut egui::Ui, sim: &SimWorld) {
    ui.heading("Metrics");
    let m = &sim.metrics;
    ui.label(format!("Frame: {:.1}ms", m.frame_time_ms));
    let hours = (m.sim_time / 3600.0) as u32;
    let mins = ((m.sim_time % 3600.0) / 60.0) as u32;
    let secs = (m.sim_time % 60.0) as u32;
    ui.label(format!("Time: {:02}:{:02}:{:02}", hours, mins, secs));
    ui.label(format!("Agents: {}", m.agent_count));
}

fn draw_vehicle_legend(ui: &mut egui::Ui, sim: &SimWorld) {
    ui.heading("Vehicles");
    let m = &sim.metrics;
    let legend: &[(&str, [u8; 3], u32)] = &[
        ("Motorbike", [255, 153, 0], m.motorbike_count),
        ("Car", [51, 102, 255], m.car_count),
        ("Bus", [51, 204, 51], m.bus_count),
        ("Bicycle", [230, 230, 51], m.bicycle_count),
        ("Truck", [230, 51, 51], m.truck_count),
        ("Emergency", [255, 255, 255], m.emergency_count),
        ("Pedestrian", [230, 230, 230], m.ped_count),
    ];
    for &(name, [r, g, b], count) in legend {
        ui.horizontal(|ui| {
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter()
                .rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
            ui.label(format!("{name}: {count}"));
        });
    }

    if m.bus_count > 0 {
        ui.separator();
        ui.heading("Bus Lines");
        let bus_colors: &[(&str, [u8; 3])] = &[
            ("Line 0", [255, 214, 0]),
            ("Line 1", [0, 191, 102]),
            ("Line 2", [217, 51, 51]),
            ("Line 3", [51, 153, 255]),
            ("Line 4", [237, 128, 0]),
            ("Line 5", [153, 51, 204]),
            ("Line 6", [0, 204, 204]),
            ("Line 7", [230, 102, 153]),
        ];
        for &(name, [r, g, b]) in bus_colors {
            ui.horizontal(|ui| {
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                ui.painter()
                    .rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
                ui.label(name);
            });
        }
    }
}

fn draw_calibration_panel(ui: &mut egui::Ui, sim: &mut SimWorld, grpc_addr: &str) {
    ui.separator();
    ui.heading("Calibration");

    // --- Collect data from locked registry (minimal lock scope) ---
    let camera_data: Vec<_> = {
        let reg = sim.camera_registry.lock().unwrap();
        reg.list()
            .iter()
            .map(|cam| {
                let state = sim.calibration_states.get(&cam.id);
                let obs = state.map(|s| s.last_observed).unwrap_or(0);
                let sim_count = state.map(|s| s.last_simulated).unwrap_or(0);
                let ratio = state.map(|s| s.previous_ratio).unwrap_or(1.0);
                let stale = state.map(|s| s.consecutive_stale_windows).unwrap_or(0);
                (cam.name.clone(), obs, sim_count, ratio, stale)
            })
            .collect()
    };

    // --- 1. Header: status indicator + pause toggle ---
    let (status_label, status_color) = if sim.calibration_paused {
        ("Paused", egui::Color32::from_rgb(230, 130, 0))
    } else if camera_data.is_empty() {
        ("Idle", egui::Color32::from_rgb(128, 128, 128))
    } else if camera_data.iter().all(|(_, _, _, _, stale)| *stale >= 3) {
        ("Stale", egui::Color32::from_rgb(230, 200, 0))
    } else {
        ("Calibrating", egui::Color32::from_rgb(0, 200, 0))
    };

    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 5.0, status_color);
        ui.label(status_label);
    });

    ui.checkbox(&mut sim.calibration_paused, "Pause Calibration");
    ui.label(format!("gRPC: {grpc_addr}"));

    // --- 2. Global summary ---
    let active_count = camera_data.iter().filter(|(_, _, _, _, s)| *s < 3).count();
    let mean_ratio = if camera_data.is_empty() {
        1.0
    } else {
        let sum: f32 = camera_data.iter().map(|(_, _, _, r, _)| r).sum();
        sum / camera_data.len() as f32
    };
    let secs_since = (sim.sim_time - sim.last_calibration_time).max(0.0) as u64;

    ui.separator();
    ui.horizontal(|ui| {
        ui.label(format!("Active: {active_count}"));
        ui.separator();
        ui.label(format!("Mean ratio: {mean_ratio:.2}"));
        ui.separator();
        ui.label(format!("Last cal: {secs_since}s ago"));
    });

    // --- 3. Per-camera grid ---
    if !camera_data.is_empty() {
        ui.separator();
        egui::Grid::new("calibration_grid")
            .striped(true)
            .show(ui, |ui| {
                ui.label("Camera");
                ui.label("Obs");
                ui.label("Sim");
                ui.label("Ratio");
                ui.label("Status");
                ui.end_row();

                for (name, obs, sim_count, ratio, stale) in &camera_data {
                    ui.label(name);
                    ui.label(format!("{obs}"));
                    ui.label(format!("{sim_count}"));
                    ui.label(format!("{ratio:.2}"));

                    let (cam_status, cam_color) = if *stale == 0 {
                        ("Live".to_string(), egui::Color32::from_rgb(0, 200, 0))
                    } else if *stale < 3 {
                        (format!("Stale ({stale})"), egui::Color32::from_rgb(230, 200, 0))
                    } else {
                        ("Decaying".to_string(), egui::Color32::from_rgb(230, 130, 0))
                    };
                    ui.colored_label(cam_color, cam_status);
                    ui.end_row();
                }
            });
    }
}
