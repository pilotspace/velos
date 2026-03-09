//! VelosApp: winit ApplicationHandler with egui integration.
//!
//! State pattern: GpuState is None until resumed() fires.
//! Frame loop: sim tick -> render agents -> render egui -> present.

use std::sync::Arc;

use glam::Vec2;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    camera::Camera2D,
    compute::ComputeDispatcher,
    renderer::Renderer,
    sim::{SimState, SimWorld},
};

/// All GPU, rendering, and simulation state.
struct GpuState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    _adapter: wgpu::Adapter,
    renderer: Renderer,
    camera: Camera2D,
    sim: SimWorld,
    compute_dispatcher: ComputeDispatcher,
    // egui state
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    /// True while the left mouse button is held down.
    left_pressed: bool,
    last_frame_time: std::time::Instant,
    /// Show dashed Bezier guide lines through junctions.
    show_guide_lines: bool,
    /// Show conflict crossing points and active conflict pair debug overlay.
    show_conflict_debug: bool,
}

impl GpuState {
    fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        let (device, queue, adapter, surface) = pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let surface = instance.create_surface(window.clone()).unwrap();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: Some(&surface),
                    force_fallback_adapter: false,
                })
                .await
                .expect("No GPU adapter found");

            let (device, queue) = adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: Some("velos-gpu"),
                        required_features: wgpu::Features::empty(),
                        required_limits: wgpu::Limits::default(),
                        ..Default::default()
                    },
                )
                .await
                .expect("GPU device request failed");

            (device, queue, adapter, surface)
        });

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &surface_config);

        let mut renderer = Renderer::new(&device, format);

        // Load road graph.
        let pbf_path = std::path::Path::new("data/hcmc/district1.osm.pbf");
        let road_graph = if pbf_path.exists() {
            match velos_net::import_osm(pbf_path, 10.7756, 106.7019) {
                Ok(g) => {
                    log::info!(
                        "Loaded road graph: {} nodes, {} edges",
                        g.node_count(),
                        g.edge_count()
                    );
                    g
                }
                Err(e) => {
                    log::error!("Failed to import OSM: {e:?}");
                    velos_net::RoadGraph::new(petgraph::graph::DiGraph::new())
                }
            }
        } else {
            log::warn!("PBF not found at {:?}, using empty graph", pbf_path);
            velos_net::RoadGraph::new(petgraph::graph::DiGraph::new())
        };

        let mut compute_dispatcher = ComputeDispatcher::new(&device);
        let sim = SimWorld::new(road_graph, &device, &queue, &mut compute_dispatcher);
        let road_lines = sim.road_edge_lines();
        let (cx, cy) = sim.network_center();

        let mut camera = Camera2D::new(Vec2::new(size.width as f32, size.height as f32));
        camera.center = Vec2::new(cx, cy);
        camera.zoom = 0.5; // Start zoomed out to see the network.

        // Upload road network lines for rendering.
        renderer.upload_road_lines(&device, &road_lines);

        // Upload guide lines and debug overlay for junction visualization.
        renderer.update_guide_lines(&device, &sim.junction_data);
        renderer.update_debug_overlay(&device, &sim.junction_data);

        // Initialize egui.
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui_ctx.viewport_id(),
            &*window,
            None,
            None,
            None,
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());

        Self {
            window,
            surface,
            surface_config,
            device,
            queue,
            _adapter: adapter,
            renderer,
            camera,
            sim,
            compute_dispatcher,
            egui_ctx,
            egui_state,
            egui_renderer,
            left_pressed: false,
            last_frame_time: std::time::Instant::now(),
            show_guide_lines: false,
            show_conflict_debug: false,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
            self.camera
                .resize(Vec2::new(new_size.width as f32, new_size.height as f32));
        }
    }

    fn update(&mut self) {
        let now = std::time::Instant::now();
        let frame_dt = now.duration_since(self.last_frame_time).as_secs_f64();
        self.last_frame_time = now;
        self.sim.metrics.frame_time_ms = frame_dt * 1000.0;

        let base_dt = 0.016_f64; // ~60 FPS base timestep
        let (motorbikes, cars, mut pedestrians) = self.sim.tick_gpu(
            base_dt,
            &self.device,
            &self.queue,
            &mut self.compute_dispatcher,
        );
        // Append signal indicators as dot-shaped instances.
        pedestrians.extend(self.sim.build_signal_indicators());
        self.renderer
            .update_instances_typed(&self.queue, &motorbikes, &cars, &pedestrians);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&Default::default());

        self.renderer.update_camera(&self.queue, &self.camera);

        // Render agents and overlays into first encoder.
        {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            self.renderer.render_frame(
                &mut encoder,
                &view,
                self.show_guide_lines,
                self.show_conflict_debug,
            );
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // egui UI -- inline drawing to avoid borrow checker issues.
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let sim = &mut self.sim;
        let show_gl = &mut self.show_guide_lines;
        let show_cd = &mut self.show_conflict_debug;
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            egui::SidePanel::left("controls")
                .exact_width(240.0)
                .show(ctx, |ui| {
                    ui.heading("VELOS");
                    ui.separator();

                    ui.heading("Controls");
                    let is_running = sim.sim_state == SimState::Running;
                    let btn_text = if is_running { "Pause" } else { "Start" };
                    if ui.button(btn_text).clicked() {
                        sim.sim_state = if is_running {
                            SimState::Paused
                        } else {
                            SimState::Running
                        };
                    }
                    if ui.button("Reset").clicked() {
                        sim.reset();
                    }
                    ui.add(
                        egui::Slider::new(&mut sim.speed_mult, 0.1..=4.0)
                            .text("Speed"),
                    );
                    ui.separator();

                    ui.heading("Metrics");
                    let m = &sim.metrics;
                    ui.label(format!("Frame: {:.1}ms", m.frame_time_ms));
                    let hours = (m.sim_time / 3600.0) as u32;
                    let mins = ((m.sim_time % 3600.0) / 60.0) as u32;
                    let secs = (m.sim_time % 60.0) as u32;
                    ui.label(format!("Time: {:02}:{:02}:{:02}", hours, mins, secs));
                    ui.label(format!("Agents: {}", m.agent_count));
                    ui.separator();

                    ui.heading("Vehicles");
                    let legend: &[(&str, [u8; 3], u32)] = &[
                        ("Motorbike",  [255, 153, 0],   m.motorbike_count),   // orange
                        ("Car",        [51, 102, 255],   m.car_count),         // blue
                        ("Bus",        [51, 204, 51],    m.bus_count),          // green
                        ("Bicycle",    [230, 230, 51],   m.bicycle_count),     // yellow
                        ("Truck",      [230, 51, 51],    m.truck_count),       // red
                        ("Emergency",  [255, 255, 255],  m.emergency_count),   // white
                        ("Pedestrian", [230, 230, 230],  m.ped_count),         // grey
                    ];
                    for &(name, [r, g, b], count) in legend {
                        ui.horizontal(|ui| {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(12.0, 12.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(
                                rect,
                                2.0,
                                egui::Color32::from_rgb(r, g, b),
                            );
                            ui.label(format!("{name}: {count}"));
                        });
                    }

                    // Bus line color legend (per-route breakdown).
                    if m.bus_count > 0 {
                        ui.separator();
                        ui.heading("Bus Lines");
                        let bus_colors: &[(&str, [u8; 3])] = &[
                            ("Line 0", [255, 214, 0]),    // gold
                            ("Line 1", [0, 191, 102]),    // emerald
                            ("Line 2", [217, 51, 51]),    // crimson
                            ("Line 3", [51, 153, 255]),   // dodger blue
                            ("Line 4", [237, 128, 0]),    // tangerine
                            ("Line 5", [153, 51, 204]),   // purple
                            ("Line 6", [0, 204, 204]),    // teal
                            ("Line 7", [230, 102, 153]),  // rose
                        ];
                        for &(name, [r, g, b]) in bus_colors {
                            ui.horizontal(|ui| {
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(12.0, 12.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().rect_filled(
                                    rect,
                                    2.0,
                                    egui::Color32::from_rgb(r, g, b),
                                );
                                ui.label(name);
                            });
                        }
                    }

                    ui.separator();
                    ui.heading("Debug Overlays");
                    ui.checkbox(show_gl, "Show Guide Lines");
                    ui.checkbox(show_cd, "Show Conflict Debug");
                });
        });

        self.egui_state
            .handle_platform_output(&self.window, full_output.platform_output);

        let tris = self
            .egui_ctx
            .tessellate(full_output.shapes, full_output.pixels_per_point);

        for (id, image_delta) in &full_output.textures_delta.set {
            self.egui_renderer
                .update_texture(&self.device, &self.queue, *id, image_delta);
        }

        let screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point: self.window.scale_factor() as f32,
        };

        // Render egui into separate encoder.
        let mut egui_encoder = self.device.create_command_encoder(&Default::default());
        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut egui_encoder,
            &tris,
            &screen_desc,
        );

        let mut pass = egui_encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_render_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                ..Default::default()
            })
            .forget_lifetime();
        self.egui_renderer.render(&mut pass, &tris, &screen_desc);
        drop(pass);

        self.queue.submit(std::iter::once(egui_encoder.finish()));
        output.present();

        for id in &full_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        Ok(())
    }
}

/// Top-level winit application. Implements ApplicationHandler.
pub struct VelosApp {
    state: Option<GpuState>,
}

impl VelosApp {
    pub fn new() -> Self {
        Self { state: None }
    }
}

impl Default for VelosApp {
    fn default() -> Self {
        Self::new()
    }
}

impl ApplicationHandler for VelosApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            let window = Arc::new(
                event_loop
                    .create_window(
                        WindowAttributes::default()
                            .with_title("VELOS - Traffic Microsimulation")
                            .with_inner_size(winit::dpi::LogicalSize::new(1280_u32, 720_u32)),
                    )
                    .expect("Failed to create window"),
            );
            self.state = Some(GpuState::new(window));
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        // Pass events to egui FIRST. If egui wants input, don't forward to camera.
        let egui_response = state.egui_state.on_window_event(&state.window, &event);
        let egui_wants_pointer = state.egui_ctx.wants_pointer_input();
        let egui_wants_keyboard = state.egui_ctx.wants_keyboard_input();

        if egui_response.consumed {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput { event, .. } => {
                if !egui_wants_keyboard
                    && event.physical_key
                        == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
                {
                    event_loop.exit();
                }
            }

            WindowEvent::Resized(size) => {
                state.resize(size);
            }

            WindowEvent::RedrawRequested => {
                state.update();
                match state.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                        let size = state.window.inner_size();
                        state.resize(size);
                    }
                    Err(e) => {
                        log::error!("Render error: {e:?}");
                    }
                }
                state.window.request_redraw();
            }

            WindowEvent::MouseWheel { delta, .. } => {
                if !egui_wants_pointer {
                    match delta {
                        MouseScrollDelta::LineDelta(x, y) => {
                            if x.abs() > 0.0 {
                                state.camera.pan_by(x * 20.0, 0.0);
                            }
                            state.camera.scroll(y);
                        }
                        MouseScrollDelta::PixelDelta(pos) => {
                            let px = pos.x as f32;
                            let py = pos.y as f32;
                            if px.abs() > 1.0 {
                                state.camera.pan_by(px, 0.0);
                            }
                            state.camera.scroll(py / 20.0);
                        }
                    }
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                if !egui_wants_pointer {
                    let new_pos = Vec2::new(position.x as f32, position.y as f32);
                    if state.left_pressed {
                        if !state.camera.is_panning() {
                            state.camera.begin_pan(new_pos);
                        } else {
                            state.camera.update_pan(new_pos);
                        }
                    }
                }
            }

            WindowEvent::MouseInput {
                state: btn_state,
                button,
                ..
            } => {
                if !egui_wants_pointer && button == MouseButton::Left {
                    match btn_state {
                        ElementState::Pressed => {
                            state.left_pressed = true;
                        }
                        ElementState::Released => {
                            state.left_pressed = false;
                            state.camera.end_pan();
                        }
                    }
                }
            }

            _ => {}
        }
    }
}
