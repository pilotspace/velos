//! Instanced 2D agent renderer with per-type shape geometry.
//!
//! One render pipeline serves all agent types. Each shape type (triangle,
//! rectangle, dot) gets its own draw call with a distinct vertex buffer.
//!
//! Frame usage:
//!   1. update_camera(queue, camera) -- upload projection matrix
//!   2. update_instances_typed(queue, motorbikes, cars, pedestrians) -- build per-type arrays
//!   3. render_frame(encoder, view) -- record per-type draw calls

use crate::camera::Camera2D;
use crate::map_tiles::MapTileRenderer;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Per-instance data uploaded to GPU for each agent.
/// Matches WGSL InstanceInput: location 1-4.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct AgentInstance {
    /// World-space position (metres).
    pub position: [f32; 2],
    /// Heading in radians (CCW from east).
    pub heading: f32,
    /// Padding to align color to 16 bytes.
    pub _pad: f32,
    /// RGBA color [0.0, 1.0].
    pub color: [f32; 4],
}

impl AgentInstance {
    /// Vertex buffer layout for the instance buffer (VertexStepMode::Instance).
    pub fn vertex_buffer_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<AgentInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    // location(1): world_pos
                    offset: 0,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    // location(2): heading
                    offset: 8,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    // location(3): _pad (consumed by WGSL)
                    offset: 12,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    // location(4): color
                    offset: 16,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Camera uniform buffer layout. Must match WGSL CameraUniform.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [f32; 16],
}

/// Vertex layout for shape mesh vertices (local space).
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct ShapeVertex {
    local_pos: [f32; 2],
}

/// Triangle vertices for a motorbike shape in local space.
/// Points forward (east, +x direction). Scale: ~2m long, 1m wide.
const TRIANGLE_VERTICES: &[ShapeVertex] = &[
    ShapeVertex { local_pos: [2.0, 0.0] },   // nose (forward)
    ShapeVertex { local_pos: [-1.0, 0.8] },  // left rear
    ShapeVertex { local_pos: [-1.0, -0.8] }, // right rear
];

/// Rectangle vertices for a car shape in local space.
/// Two triangles forming a ~3m long, 1.5m wide rectangle, centered.
const RECTANGLE_VERTICES: &[ShapeVertex] = &[
    // First triangle (top-left)
    ShapeVertex { local_pos: [-1.5, 0.75] },
    ShapeVertex { local_pos: [1.5, 0.75] },
    ShapeVertex { local_pos: [1.5, -0.75] },
    // Second triangle (bottom-right)
    ShapeVertex { local_pos: [-1.5, 0.75] },
    ShapeVertex { local_pos: [1.5, -0.75] },
    ShapeVertex { local_pos: [-1.5, -0.75] },
];

/// Dot/diamond vertices for a pedestrian shape in local space.
/// Two triangles forming a ~0.8m diamond, centered.
const DOT_VERTICES: &[ShapeVertex] = &[
    // First triangle (top)
    ShapeVertex { local_pos: [0.0, 0.4] },
    ShapeVertex { local_pos: [0.4, 0.0] },
    ShapeVertex { local_pos: [-0.4, 0.0] },
    // Second triangle (bottom)
    ShapeVertex { local_pos: [0.0, -0.4] },
    ShapeVertex { local_pos: [0.4, 0.0] },
    ShapeVertex { local_pos: [-0.4, 0.0] },
];

/// Road line vertex: position + color.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct RoadLineVertex {
    position: [f32; 2],
    color: [f32; 4],
}

/// Per-type instance counts for draw call ranges.
#[derive(Default, Clone, Copy)]
struct TypeCounts {
    motorbike_count: u32,
    car_count: u32,
    pedestrian_count: u32,
}

/// Instanced 2D renderer with per-type shape geometry and road line overlay.
pub struct Renderer {
    render_pipeline: wgpu::RenderPipeline,
    road_pipeline: wgpu::RenderPipeline,
    camera_bind_group: wgpu::BindGroup,
    camera_bind_group_layout: wgpu::BindGroupLayout,
    camera_uniform_buffer: wgpu::Buffer,
    triangle_vertex_buffer: wgpu::Buffer,
    rectangle_vertex_buffer: wgpu::Buffer,
    dot_vertex_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
    instance_capacity: u32,
    type_counts: TypeCounts,
    road_vertex_buffer: Option<wgpu::Buffer>,
    road_vertex_count: u32,
    pub surface_format: wgpu::TextureFormat,
    /// Optional map tile background renderer.
    map_tiles: Option<MapTileRenderer>,
}

impl Renderer {
    /// Create the render pipeline for the given surface texture format.
    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Self {
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/agent_render.wgsl"));

        let camera_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bgl"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_uniform_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("render_pipeline_layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        // Road line pipeline (LineList topology, same camera bind group).
        let road_shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/road_line.wgsl"));
        let road_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RoadLineVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };
        let road_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("road_line_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &road_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[road_vertex_layout],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &road_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::LineList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // Shape vertex buffer layout: location(0) local_pos vec2<f32>
        let shape_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<ShapeVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        };

        let render_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("agent_render_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[shape_vertex_layout, AgentInstance::vertex_buffer_layout()],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::REPLACE),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let triangle_vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("triangle_vertices"),
                contents: bytemuck::cast_slice(TRIANGLE_VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let rectangle_vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("rectangle_vertices"),
                contents: bytemuck::cast_slice(RECTANGLE_VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let dot_vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("dot_vertices"),
                contents: bytemuck::cast_slice(DOT_VERTICES),
                usage: wgpu::BufferUsages::VERTEX,
            });

        let instance_capacity = 8192_u32;
        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instance_buffer"),
            size: (instance_capacity as usize * std::mem::size_of::<AgentInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            render_pipeline,
            road_pipeline,
            camera_bind_group,
            camera_bind_group_layout,
            camera_uniform_buffer,
            triangle_vertex_buffer,
            rectangle_vertex_buffer,
            dot_vertex_buffer,
            instance_buffer,
            instance_capacity,
            type_counts: TypeCounts::default(),
            road_vertex_buffer: None,
            road_vertex_count: 0,
            surface_format,
            map_tiles: None,
        }
    }

    /// Upload road network edge geometry as line segments.
    /// Call once at init after loading the road graph.
    pub fn upload_road_lines(&mut self, device: &wgpu::Device, edges: &[([f32; 2], [f32; 2])]) {
        let road_color = [0.25, 0.25, 0.35, 1.0_f32]; // dark grey-blue
        let mut vertices = Vec::with_capacity(edges.len() * 2);
        for (start, end) in edges {
            vertices.push(RoadLineVertex {
                position: *start,
                color: road_color,
            });
            vertices.push(RoadLineVertex {
                position: *end,
                color: road_color,
            });
        }
        self.road_vertex_count = vertices.len() as u32;
        if !vertices.is_empty() {
            self.road_vertex_buffer = Some(device.create_buffer_init(
                &wgpu::util::BufferInitDescriptor {
                    label: Some("road_lines"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                },
            ));
        }
        log::info!("Uploaded {} road line segments", edges.len());
    }

    /// Upload the camera view-projection matrix to the uniform buffer.
    pub fn update_camera(&self, queue: &wgpu::Queue, camera: &Camera2D) {
        let m = camera.view_proj_matrix();
        let uniform = CameraUniform {
            view_proj: m.to_cols_array(),
        };
        queue.write_buffer(
            &self.camera_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniform),
        );
    }

    /// Update instance buffer with per-type agent arrays.
    ///
    /// Instances are packed contiguously: [motorbikes | cars | pedestrians].
    /// Draw calls use offset ranges to select the shape vertex buffer per type.
    pub fn update_instances_typed(
        &mut self,
        queue: &wgpu::Queue,
        motorbikes: &[AgentInstance],
        cars: &[AgentInstance],
        pedestrians: &[AgentInstance],
    ) {
        let total = motorbikes.len() + cars.len() + pedestrians.len();
        let count = total.min(self.instance_capacity as usize);

        self.type_counts = TypeCounts {
            motorbike_count: motorbikes.len().min(count) as u32,
            car_count: cars.len().min(count.saturating_sub(motorbikes.len())) as u32,
            pedestrian_count: pedestrians
                .len()
                .min(count.saturating_sub(motorbikes.len() + cars.len()))
                as u32,
        };

        // Build packed buffer
        let mut all_instances = Vec::with_capacity(count);
        all_instances.extend_from_slice(
            &motorbikes[..self.type_counts.motorbike_count as usize],
        );
        all_instances
            .extend_from_slice(&cars[..self.type_counts.car_count as usize]);
        all_instances.extend_from_slice(
            &pedestrians[..self.type_counts.pedestrian_count as usize],
        );

        if !all_instances.is_empty() {
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&all_instances),
            );
        }
    }

    /// Rebuild the instance buffer from CPU-side position and heading arrays.
    /// Compatibility fallback: all agents rendered as green triangles.
    pub fn update_instances_from_cpu(
        &mut self,
        queue: &wgpu::Queue,
        positions: &[[f32; 2]],
        headings: &[f32],
    ) {
        let count = positions.len().min(self.instance_capacity as usize);
        let instances: Vec<AgentInstance> = (0..count)
            .map(|i| AgentInstance {
                position: positions[i],
                heading: headings[i],
                _pad: 0.0,
                color: [0.2, 0.8, 0.4, 1.0],
            })
            .collect();

        self.type_counts = TypeCounts {
            motorbike_count: count as u32,
            car_count: 0,
            pedestrian_count: 0,
        };

        if !instances.is_empty() {
            queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&instances),
            );
        }
    }

    /// Record the render pass with per-type draw calls.
    ///
    /// Issues 3 draw calls, one per shape type:
    /// 1. Triangle vertex buffer for motorbike instances
    /// 2. Rectangle vertex buffer for car instances
    /// 3. Dot vertex buffer for pedestrian instances
    pub fn render_frame(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        // Render map tiles first (clears the screen).
        // If map tiles exist, agents render on top with LoadOp::Load.
        let has_map_tiles = self.map_tiles.is_some();
        if let Some(ref mt) = self.map_tiles {
            mt.render(encoder, view, &self.camera_bind_group);
        }

        let load_op = if has_map_tiles {
            wgpu::LoadOp::Load
        } else {
            wgpu::LoadOp::Clear(wgpu::Color {
                r: 0.05,
                g: 0.05,
                b: 0.1,
                a: 1.0,
            })
        };

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("agent_render_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            ..Default::default()
        });

        // Draw road lines first (background layer).
        if let Some(ref road_buf) = self.road_vertex_buffer
            && self.road_vertex_count > 0
        {
            pass.set_pipeline(&self.road_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, road_buf.slice(..));
            pass.draw(0..self.road_vertex_count, 0..1);
        }

        // Draw agents on top.
        pass.set_pipeline(&self.render_pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
        pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

        let tc = &self.type_counts;
        let mut instance_offset = 0u32;

        // Draw motorbikes as triangles
        if tc.motorbike_count > 0 {
            pass.set_vertex_buffer(0, self.triangle_vertex_buffer.slice(..));
            pass.draw(
                0..TRIANGLE_VERTICES.len() as u32,
                instance_offset..instance_offset + tc.motorbike_count,
            );
            instance_offset += tc.motorbike_count;
        }

        // Draw cars as rectangles
        if tc.car_count > 0 {
            pass.set_vertex_buffer(0, self.rectangle_vertex_buffer.slice(..));
            pass.draw(
                0..RECTANGLE_VERTICES.len() as u32,
                instance_offset..instance_offset + tc.car_count,
            );
            instance_offset += tc.car_count;
        }

        // Draw pedestrians as dots/diamonds
        if tc.pedestrian_count > 0 {
            pass.set_vertex_buffer(0, self.dot_vertex_buffer.slice(..));
            pass.draw(
                0..DOT_VERTICES.len() as u32,
                instance_offset..instance_offset + tc.pedestrian_count,
            );
        }
    }

    /// Set the map tile renderer for background tile rendering.
    pub fn set_map_tiles(&mut self, renderer: MapTileRenderer) {
        self.map_tiles = Some(renderer);
    }

    /// Initialize map tile renderer from a PMTiles file path.
    ///
    /// If path is None or file not found, map tiles are silently disabled.
    pub fn init_map_tiles(&mut self, device: &wgpu::Device, pmtiles_path: Option<&std::path::Path>) {
        let renderer = MapTileRenderer::new(
            device,
            self.surface_format,
            &self.camera_bind_group_layout,
            pmtiles_path,
        );
        self.map_tiles = Some(renderer);
    }

    /// Update map tiles based on current camera viewport.
    /// Call before `render_frame()`.
    pub fn update_map_tiles(&mut self, camera: &Camera2D, device: &wgpu::Device) {
        if let Some(ref mut mt) = self.map_tiles {
            mt.update(camera, device);
        }
    }

    /// Returns the camera bind group layout for sharing with other pipelines.
    pub fn camera_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.camera_bind_group_layout
    }

    /// Total instance count across all types.
    pub fn total_instance_count(&self) -> u32 {
        self.type_counts.motorbike_count
            + self.type_counts.car_count
            + self.type_counts.pedestrian_count
    }
}
