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
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

use velos_api::bridge::ApiBridge;

use crate::{
    app_input,
    camera::Camera2D,
    compute::ComputeDispatcher,
    orbit_camera::{OrbitCamera, ViewMode, ViewTransition},
    renderer::Renderer,
    renderer3d::Renderer3D,
    sim::SimWorld,
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
    renderer_3d: Renderer3D,
    camera: Camera2D,
    orbit_camera: OrbitCamera,
    view_mode: ViewMode,
    view_transition: Option<ViewTransition>,
    sim: SimWorld,
    compute_dispatcher: ComputeDispatcher,
    /// True while the middle mouse button is held (3D pan).
    middle_pressed: bool,
    /// Last cursor position for computing orbit/pan deltas.
    last_cursor_pos: Option<(f32, f32)>,
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
    /// Show camera FOV cone overlays on the map.
    show_cameras: bool,
    /// Show speed heatmap overlay on camera-covered edges.
    show_speed_overlay: bool,
    /// Dirty flag: rebuild speed overlay vertices on next frame.
    speed_overlay_dirty: bool,
    /// Frame counter for periodic speed overlay refresh.
    speed_overlay_frame_counter: u32,
    /// gRPC server listen address (for display in egui panel).
    grpc_addr: String,
    /// Cached camera count — only rebuild overlay when cameras change.
    camera_overlay_count: usize,
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
        let mut renderer_3d =
            Renderer3D::new(&device, format, size.width.max(1), size.height.max(1));

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
        let mut sim = SimWorld::new(road_graph, &device, &queue, &mut compute_dispatcher);

        // --- Start gRPC detection server on background thread ---
        let grpc_addr = std::env::var("VELOS_GRPC_ADDR")
            .unwrap_or_else(|_| "[::1]:50051".to_string());

        let (bridge, cmd_tx) = ApiBridge::new(256);
        let aggregator = Arc::clone(&sim.aggregator);
        let registry = Arc::clone(&sim.camera_registry);
        sim.api_bridge = Some(bridge);

        // Build edge R-tree and projection for camera FOV queries
        let edge_tree = Arc::new(velos_net::snap::build_edge_rtree(&sim.road_graph));
        let projection = Arc::new(
            velos_net::EquirectangularProjection::new(10.7756, 106.7019),
        );

        let detection_service = velos_api::create_detection_service(
            cmd_tx,
            aggregator,
            registry,
            edge_tree,
            projection,
        );

        let grpc_addr_clone = grpc_addr.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new()
                .expect("failed to create tokio runtime for gRPC server");
            rt.block_on(async {
                let addr = grpc_addr_clone.parse()
                    .expect("invalid gRPC listen address");
                log::info!("gRPC server listening on {}", addr);
                if let Err(e) = tonic::transport::Server::builder()
                    .add_service(detection_service)
                    .serve(addr)
                    .await
                {
                    log::error!("gRPC server error: {e:?}");
                }
            });
        });

        // Upload road geometry to 3D renderer.
        let junction_3d = crate::road_surface::convert_junction_data(&sim.junction_data);
        renderer_3d.upload_road_geometry(&device, &sim.road_graph, &junction_3d);

        let road_lines = sim.road_edge_lines();
        let (cx, cy) = sim.network_center();

        let mut camera = Camera2D::new(Vec2::new(size.width as f32, size.height as f32));
        camera.center = Vec2::new(cx, cy);
        camera.zoom = 0.5; // Start zoomed out to see the network.

        let orbit_camera = OrbitCamera::from_camera_2d(&camera);

        // Upload road network lines for rendering.
        renderer.upload_road_lines(&device, &road_lines);

        // Initialize map tile renderer (PMTiles background layer).
        let pmtiles_path = std::path::Path::new("data/hcmc/hcmc.pmtiles");
        if pmtiles_path.exists() {
            renderer.init_map_tiles(&device, Some(pmtiles_path));
            log::info!("Map tiles initialized from {:?}", pmtiles_path);
        } else {
            log::warn!("PMTiles not found at {:?}, map tiles disabled", pmtiles_path);
        }

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
            renderer_3d,
            camera,
            orbit_camera,
            view_mode: ViewMode::TopDown2D,
            view_transition: None,
            sim,
            compute_dispatcher,
            middle_pressed: false,
            last_cursor_pos: None,
            egui_ctx,
            egui_state,
            egui_renderer,
            left_pressed: false,
            last_frame_time: std::time::Instant::now(),
            show_guide_lines: false,
            show_conflict_debug: false,
            show_cameras: true,
            show_speed_overlay: false,
            speed_overlay_dirty: true,
            speed_overlay_frame_counter: 0,
            camera_overlay_count: 0,
            grpc_addr,
        }
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
            self.camera
                .resize(Vec2::new(new_size.width as f32, new_size.height as f32));
            self.renderer_3d
                .resize(&self.device, new_size.width, new_size.height);
            self.orbit_camera
                .resize(Vec2::new(new_size.width as f32, new_size.height as f32));
        }
    }

    fn update(&mut self) {
        let now = std::time::Instant::now();
        let frame_dt = now.duration_since(self.last_frame_time).as_secs_f64();
        self.last_frame_time = now;
        self.sim.metrics.frame_time_ms = frame_dt * 1000.0;

        // Update map tiles (load/decode visible tiles based on camera viewport).
        self.renderer.update_map_tiles(&self.camera, &self.device);

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

        // Advance view transition if active.
        if let Some(ref mut transition) = self.view_transition {
            let done = transition.tick(frame_dt as f32);
            if done {
                self.view_mode = transition.to;
                self.view_transition = None;
            }
        }

        // Update 3D renderer when in 3D mode (or transitioning to/from 3D).
        let needs_3d = matches!(self.view_mode, ViewMode::Perspective3D)
            || self
                .view_transition
                .as_ref()
                .is_some_and(|t| t.to == ViewMode::Perspective3D);
        if needs_3d {
            self.renderer_3d
                .update_camera(&self.queue, &self.orbit_camera);
            self.renderer_3d
                .update_lighting(&self.queue, self.sim.sim_time);
            let lod_buffers =
                self.sim.build_instances_3d(self.orbit_camera.eye_position());
            self.renderer_3d.upload_agent_instances(
                &self.device,
                &lod_buffers.mesh_instances,
                &lod_buffers.billboard_instances,
            );
        }

        // Update camera overlay geometry only when camera count changes.
        // Uses try_lock to avoid blocking the render loop if gRPC holds the lock.
        if self.show_cameras
            && let Ok(reg) = self.sim.camera_registry.try_lock()
        {
            let current_count = reg.list().len();
            if current_count != self.camera_overlay_count {
                self.camera_overlay_count = current_count;
                let proj = velos_net::EquirectangularProjection::new(10.7756, 106.7019);
                let cameras_list: Vec<_> =
                    reg.list().iter().map(|c| (*c).clone()).collect();
                drop(reg);
                let cam_refs: Vec<&velos_api::Camera> = cameras_list.iter().collect();
                let vertices = crate::sim_render::build_camera_overlay_vertices(
                    &cam_refs, &proj, true,
                );
                self.renderer
                    .update_camera_overlay(&self.device, vertices);
            }
        }

        // Periodic speed overlay refresh (~every 60 frames / ~1 second).
        self.speed_overlay_frame_counter += 1;
        if self.speed_overlay_frame_counter >= 60 {
            self.speed_overlay_frame_counter = 0;
            self.speed_overlay_dirty = true;
        }

        // Update speed overlay when dirty and visible.
        if self.show_speed_overlay
            && self.speed_overlay_dirty
            && let Ok(reg) = self.sim.camera_registry.try_lock()
            && let Ok(agg) = self.sim.aggregator.try_lock()
        {
            let cameras_list: Vec<_> =
                reg.list().iter().map(|c| (*c).clone()).collect();
            let cam_refs: Vec<&velos_api::Camera> =
                cameras_list.iter().collect();
            let proj = velos_net::EquirectangularProjection::new(10.7756, 106.7019);
            let vertices =
                crate::sim_render::build_speed_overlay_vertices(
                    &cam_refs,
                    &agg,
                    &self.sim.road_graph,
                    &proj,
                    true,
                );
            if !vertices.is_empty() {
                log::info!(
                    "Speed overlay: {} cameras, {} vertices",
                    cameras_list.len(),
                    vertices.len()
                );
            }
            self.renderer
                .update_speed_overlay(&self.device, vertices);
            self.speed_overlay_dirty = false;
        }
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&Default::default());

        // Branch rendering based on view mode.
        {
            let mut encoder = self.device.create_command_encoder(&Default::default());
            match self.view_mode {
                ViewMode::TopDown2D => {
                    self.renderer.update_camera(&self.queue, &self.camera);
                    self.renderer.render_frame(
                        &mut encoder,
                        &view,
                        self.show_guide_lines,
                        self.show_conflict_debug,
                        self.show_cameras,
                        self.show_speed_overlay,
                    );
                }
                ViewMode::Perspective3D => {
                    self.renderer_3d.render_frame(&mut encoder, &view);
                }
            }
            self.queue.submit(std::iter::once(encoder.finish()));
        }

        // egui UI via extracted module to keep app.rs under 700 lines.
        let raw_input = self.egui_state.take_egui_input(&self.window);
        let mut panel = crate::app_egui::EguiPanelState {
            sim: &mut self.sim,
            show_guide_lines: &mut self.show_guide_lines,
            show_conflict_debug: &mut self.show_conflict_debug,
            show_cameras: &mut self.show_cameras,
            show_speed_overlay: &mut self.show_speed_overlay,
            grpc_addr: &self.grpc_addr,
            view_mode: &mut self.view_mode,
            view_transition: &mut self.view_transition,
            camera_2d: &mut self.camera,
            orbit_camera: &mut self.orbit_camera,
        };
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            egui::SidePanel::left("controls")
                .exact_width(240.0)
                .show(ctx, |ui| {
                    crate::app_egui::draw_control_panel(ui, &mut panel);
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
                if !egui_wants_keyboard && event.state == ElementState::Pressed {
                    match event.physical_key {
                        PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                        PhysicalKey::Code(KeyCode::KeyV) => {
                            if state.view_transition.is_none() {
                                state.view_transition =
                                    Some(app_input::toggle_view_mode(
                                        state.view_mode,
                                        &mut state.camera,
                                        &mut state.orbit_camera,
                                    ));
                            }
                        }
                        _ => {}
                    }
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

            WindowEvent::MouseWheel { .. }
            | WindowEvent::CursorMoved { .. }
            | WindowEvent::MouseInput { .. } => {
                if !egui_wants_pointer {
                    match state.view_mode {
                        ViewMode::Perspective3D => {
                            app_input::handle_3d_input(
                                &event,
                                &mut state.orbit_camera,
                                &mut state.left_pressed,
                                &mut state.middle_pressed,
                                &mut state.last_cursor_pos,
                            );
                        }
                        ViewMode::TopDown2D => match event {
                            WindowEvent::MouseWheel { delta, .. } => match delta {
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
                            },
                            WindowEvent::CursorMoved { position, .. } => {
                                let new_pos =
                                    Vec2::new(position.x as f32, position.y as f32);
                                if state.left_pressed {
                                    if !state.camera.is_panning() {
                                        state.camera.begin_pan(new_pos);
                                    } else {
                                        state.camera.update_pan(new_pos);
                                    }
                                }
                            }
                            WindowEvent::MouseInput {
                                state: btn_state,
                                button,
                                ..
                            } => {
                                if button == MouseButton::Left {
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
                        },
                    }
                }
            }

            _ => {}
        }
    }
}
