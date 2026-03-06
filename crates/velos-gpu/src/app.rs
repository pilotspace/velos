//! VelosApp: winit ApplicationHandler for the GPU pipeline visual proof.
//!
//! State pattern: GpuState is None until can_create_surfaces() fires.
//! Frame loop: ECS step -> GPU upload -> compute -> readback -> render -> present.

use std::sync::Arc;

use glam::Vec2;
use hecs::World;
use velos_core::components::{Kinematics, Position};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    buffers::BufferPool,
    camera::Camera2D,
    compute::ComputeDispatcher,
    renderer::Renderer,
};

const AGENT_COUNT: usize = 1000;
const BUFFER_CAPACITY: u32 = 1024;

/// All GPU and simulation state, initialized once on first resume.
#[allow(dead_code)]
struct GpuState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter: wgpu::Adapter,
    world: World,
    buffer_pool: BufferPool,
    dispatcher: ComputeDispatcher,
    renderer: Renderer,
    camera: Camera2D,
    /// True while the left mouse button is held down.
    /// Pan starts on the first CursorMoved after the press so begin_pan
    /// always receives a real cursor position (never Vec2::ZERO).
    left_pressed: bool,
    frame_count: u64,
}

impl GpuState {
    fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();

        // Initialize GPU context and surface
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

        // Configure surface
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

        // Initialize ECS world with 1K agents
        let mut world = World::new();
        let speed = 5.0_f64; // m/s
        for i in 0..AGENT_COUNT {
            let angle = (i as f64) * std::f64::consts::TAU / (AGENT_COUNT as f64);
            // Spread agents in a 200m radius circle
            let x = 300.0 + 200.0 * angle.cos();
            let y = 200.0 + 200.0 * angle.sin();
            // Each agent moves tangent to the circle
            let vx = speed * (-angle.sin());
            let vy = speed * angle.cos();
            world.spawn((
                Position { x, y },
                Kinematics {
                    vx,
                    vy,
                    speed,
                    heading: vx.atan2(vy),
                },
            ));
        }

        let mut buffer_pool = BufferPool::new(&device, BUFFER_CAPACITY);
        let dispatcher = ComputeDispatcher::new(&device);
        let renderer = Renderer::new(&device, format);
        let camera = Camera2D::new(Vec2::new(size.width as f32, size.height as f32));

        // Initial upload
        buffer_pool.upload_from_ecs(&world, &queue);
        // Copy back -> front to prime the front buffers for first dispatch
        {
            let mut encoder = device.create_command_encoder(&Default::default());
            let pos_bytes = (buffer_pool.agent_count as usize * 8) as u64;
            let kin_bytes = (buffer_pool.agent_count as usize * 16) as u64;
            if pos_bytes > 0 {
                encoder.copy_buffer_to_buffer(
                    &buffer_pool.pos_back,
                    0,
                    &buffer_pool.pos_front,
                    0,
                    pos_bytes,
                );
                encoder.copy_buffer_to_buffer(
                    &buffer_pool.kin_back,
                    0,
                    &buffer_pool.kin_front,
                    0,
                    kin_bytes,
                );
            }
            queue.submit(std::iter::once(encoder.finish()));
            let _ = device.poll(wgpu::PollType::wait_indefinitely());
        }

        Self {
            window,
            surface,
            surface_config,
            device,
            queue,
            adapter,
            world,
            buffer_pool,
            dispatcher,
            renderer,
            camera,
            left_pressed: false,
            frame_count: 0,
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
        const DT: f32 = 0.016; // ~60 FPS timestep

        // Compute dispatch: reads front, writes back
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.dispatcher.dispatch(
            &mut encoder,
            &self.buffer_pool,
            &self.device,
            &self.queue,
            DT,
        );
        self.queue.submit(std::iter::once(encoder.finish()));
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.buffer_pool.swap();

        // Readback positions for CPU-side instance buffer update
        // Phase 1: small enough to do this every frame for 1K agents
        let positions = ComputeDispatcher::readback_positions(
            &self.buffer_pool,
            &self.device,
            &self.queue,
        );

        // Derive heading per agent from tangential angle (constant for circular orbit)
        let headings: Vec<f32> = (0..positions.len())
            .map(|i| {
                let angle = (i as f64) * std::f64::consts::TAU / (AGENT_COUNT as f64);
                // Tangential direction heading
                (std::f64::consts::FRAC_PI_2 + angle) as f32
            })
            .collect();

        self.renderer
            .update_instances_from_cpu(&self.queue, &positions, &headings);

        self.frame_count += 1;
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&Default::default());

        self.renderer.update_camera(&self.queue, &self.camera);

        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.renderer.render_frame(
            &mut encoder,
            &view,
            self.buffer_pool.agent_count,
        );

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
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
                            .with_title("VELOS - GPU Pipeline Proof")
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

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::KeyboardInput { event, .. } => {
                if event.physical_key
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
                match delta {
                    // Physical scroll wheel or trackpad line-based scroll.
                    // X delta pans horizontally; Y delta zooms.
                    MouseScrollDelta::LineDelta(x, y) => {
                        if x.abs() > 0.0 {
                            // Horizontal scroll: pan X. Scale to ~pixels at 1x zoom.
                            state.camera.pan_by(x * 20.0, 0.0);
                        }
                        state.camera.scroll(y);
                    }
                    // macOS trackpad two-finger scroll fires PixelDelta.
                    // X delta pans; Y delta zooms (natural scroll).
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

            WindowEvent::CursorMoved { position, .. } => {
                let new_pos = Vec2::new(position.x as f32, position.y as f32);
                if state.left_pressed {
                    // begin_pan on first CursorMoved while button held, so last_cursor
                    // is always set from a real position event (never Vec2::ZERO).
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
                            // Don't call begin_pan here; wait for the first CursorMoved
                            // so begin_pan receives the actual cursor position from the event.
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
