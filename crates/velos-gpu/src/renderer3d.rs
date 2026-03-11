//! Renderer3D: 3D rendering with depth buffer, ground plane, road surfaces,
//! lit 3D mesh instancing, and billboard rendering.
//!
//! This renderer is independent of the existing 2D `Renderer`. It manages its
//! own depth texture, camera uniform buffer, lighting uniform, and render
//! pipelines for all 3D content (ground, roads, meshes, billboards).

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

use crate::lighting::{compute_lighting, LightingUniform};
use crate::mesh_loader::{MeshSet, Vertex3D};
use crate::orbit_camera::{
    create_depth_texture, BillboardInstance3D, MeshInstance3D, OrbitCamera,
};
use crate::road_surface::{
    generate_junction_surfaces, generate_lane_markings, generate_road_mesh, JunctionData,
    RoadSurfaceVertex,
};
use velos_core::components::VehicleType;
use velos_net::RoadGraph;

/// Extended camera uniform buffer layout for 3D rendering (128 bytes).
///
/// Includes view_proj matrix, eye position, and camera orientation vectors
/// needed by both mesh and billboard shaders.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniform3D {
    view_proj: [f32; 16],
    eye_position: [f32; 3],
    _pad0: f32,
    camera_right: [f32; 3],
    _pad1: f32,
    camera_up: [f32; 3],
    _pad2: f32,
}

/// Ground plane vertex: position (vec3) + color (vec4).
/// Must match WGSL VertexInput struct in ground_plane.wgsl.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct GroundPlaneVertex {
    position: [f32; 3],
    color: [f32; 4],
}

/// Ground plane color: muted green #3a5a3a.
const GROUND_COLOR: [f32; 4] = [0.227, 0.353, 0.227, 1.0];

/// Ground plane half-size in metres (total 20km x 20km).
const GROUND_HALF_SIZE: f32 = 10_000.0;

/// Ground plane Y offset (well below road surfaces to avoid z-fighting).
const GROUND_Y: f32 = -0.5;

/// Generate ground plane vertices: two triangles forming a 20000x20000 quad.
fn ground_plane_vertices() -> [GroundPlaneVertex; 6] {
    let s = GROUND_HALF_SIZE;
    let y = GROUND_Y;
    let c = GROUND_COLOR;
    [
        // Triangle 1
        GroundPlaneVertex {
            position: [-s, y, -s],
            color: c,
        },
        GroundPlaneVertex {
            position: [s, y, -s],
            color: c,
        },
        GroundPlaneVertex {
            position: [s, y, s],
            color: c,
        },
        // Triangle 2
        GroundPlaneVertex {
            position: [-s, y, -s],
            color: c,
        },
        GroundPlaneVertex {
            position: [s, y, s],
            color: c,
        },
        GroundPlaneVertex {
            position: [-s, y, s],
            color: c,
        },
    ]
}

/// 3D renderer with depth buffer, ground plane, road surfaces, mesh instancing,
/// and billboard rendering with time-of-day lighting.
pub struct Renderer3D {
    depth_texture_view: wgpu::TextureView,
    camera_uniform_buffer: wgpu::Buffer,
    lighting_uniform_buffer: wgpu::Buffer,
    /// Bind group with only camera uniform (for ground_plane.wgsl, road_surface.wgsl).
    ground_bind_group: wgpu::BindGroup,
    /// Bind group with camera + lighting uniforms (for mesh_3d.wgsl, billboard_3d.wgsl).
    agent_bind_group: wgpu::BindGroup,
    camera_bind_group_layout: wgpu::BindGroupLayout,
    _agent_bind_group_layout: wgpu::BindGroupLayout,
    ground_plane_pipeline: wgpu::RenderPipeline,
    ground_plane_vertex_buffer: wgpu::Buffer,
    ground_plane_vertex_count: u32,
    surface_format: wgpu::TextureFormat,
    // Road surface geometry (Plan 02)
    road_pipeline: wgpu::RenderPipeline,
    road_vertex_buffer: Option<wgpu::Buffer>,
    road_vertex_count: u32,
    marking_vertex_buffer: Option<wgpu::Buffer>,
    marking_vertex_count: u32,
    junction_vertex_buffer: Option<wgpu::Buffer>,
    junction_vertex_count: u32,
    // 3D agent rendering (Plan 03)
    mesh_pipeline: wgpu::RenderPipeline,
    billboard_pipeline: wgpu::RenderPipeline,
    mesh_set: MeshSet,
    /// Per-vehicle-type instance buffers for mesh rendering.
    mesh_instance_buffers: HashMap<VehicleType, (wgpu::Buffer, u32)>,
    /// Per-vehicle-type instance buffers for billboard rendering.
    billboard_instance_buffers: HashMap<VehicleType, (wgpu::Buffer, u32)>,
}

impl Renderer3D {
    /// Create a new 3D renderer with depth buffer, ground plane, road surface,
    /// lit mesh instancing, and billboard pipelines.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        // Depth texture
        let depth_texture_view = create_depth_texture(device, width, height);

        // Camera uniform buffer (128 bytes: view_proj + eye_pos + camera_right + camera_up)
        let camera_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform_3d"),
            size: std::mem::size_of::<CameraUniform3D>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Lighting uniform buffer (48 bytes)
        let lighting_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lighting_uniform_3d"),
            size: std::mem::size_of::<LightingUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Camera-only bind group layout (for ground plane and road surface shaders)
        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("camera_bgl_3d"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let ground_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ground_bg_3d"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_uniform_buffer.as_entire_binding(),
            }],
        });

        // Camera + lighting bind group layout (for mesh and billboard shaders)
        let _agent_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("agent_bgl_3d"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        let agent_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("agent_bg_3d"),
            layout: &_agent_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: lighting_uniform_buffer.as_entire_binding(),
                },
            ],
        });

        // Reverse-Z: near maps to 1.0, far to 0.0 — use GreaterEqual compare.
        let depth_stencil_state = wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::GreaterEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };

        // --- Ground plane pipeline ---
        let shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/ground_plane.wgsl"));

        let ground_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("ground_plane_pipeline_layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let ground_plane_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GroundPlaneVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let ground_plane_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ground_plane_pipeline"),
                layout: Some(&ground_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    buffers: &[ground_plane_vertex_layout],
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
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::GreaterEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState {
                        // Negative bias in reverse-Z pushes ground behind roads
                        constant: -2,
                        slope_scale: -2.0,
                        clamp: 0.0,
                    },
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        let vertices = ground_plane_vertices();
        let ground_plane_vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ground_plane_vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        // --- Road surface pipeline ---
        let road_shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/road_surface.wgsl"));

        let road_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("road_surface_pipeline_layout"),
                bind_group_layouts: &[&camera_bind_group_layout],
                push_constant_ranges: &[],
            });

        let road_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RoadSurfaceVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let road_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("road_surface_pipeline"),
                layout: Some(&road_pipeline_layout),
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
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
                depth_stencil: Some(depth_stencil_state.clone()),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // --- Mesh 3D pipeline ---
        let mesh_shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/mesh_3d.wgsl"));

        let mesh_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mesh_3d_pipeline_layout"),
                bind_group_layouts: &[&_agent_bind_group_layout],
                push_constant_ranges: &[],
            });

        let mesh_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex3D>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        };

        let mesh_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshInstance3D>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 16,
                    shader_location: 4,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let mesh_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mesh_3d_pipeline"),
                layout: Some(&mesh_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &mesh_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[mesh_vertex_layout, mesh_instance_layout],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &mesh_shader,
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
                    cull_mode: Some(wgpu::Face::Back),
                    ..Default::default()
                },
                depth_stencil: Some(depth_stencil_state.clone()),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // --- Billboard 3D pipeline ---
        let billboard_shader =
            device.create_shader_module(wgpu::include_wgsl!("../shaders/billboard_3d.wgsl"));

        let billboard_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("billboard_3d_pipeline_layout"),
                bind_group_layouts: &[&_agent_bind_group_layout],
                push_constant_ranges: &[],
            });

        let billboard_instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<BillboardInstance3D>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 20,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
                wgpu::VertexAttribute {
                    offset: 36,
                    shader_location: 3,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        };

        let billboard_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("billboard_3d_pipeline"),
                layout: Some(&billboard_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &billboard_shader,
                    entry_point: Some("vs_main"),
                    buffers: &[billboard_instance_layout],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &billboard_shader,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: surface_format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
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
                depth_stencil: Some(depth_stencil_state),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // Load mesh assets
        let mesh_set = MeshSet::load_all(device);

        Self {
            depth_texture_view,
            camera_uniform_buffer,
            lighting_uniform_buffer,
            ground_bind_group,
            agent_bind_group,
            camera_bind_group_layout,
            _agent_bind_group_layout,
            ground_plane_pipeline,
            ground_plane_vertex_buffer,
            ground_plane_vertex_count: vertices.len() as u32,
            surface_format,
            road_pipeline,
            road_vertex_buffer: None,
            road_vertex_count: 0,
            marking_vertex_buffer: None,
            marking_vertex_count: 0,
            junction_vertex_buffer: None,
            junction_vertex_count: 0,
            mesh_pipeline,
            billboard_pipeline,
            mesh_set,
            mesh_instance_buffers: HashMap::new(),
            billboard_instance_buffers: HashMap::new(),
        }
    }

    /// Recreate the depth texture on window resize.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.depth_texture_view = create_depth_texture(device, width, height);
    }

    /// Upload the orbit camera's view-projection matrix and orientation to the
    /// uniform buffer. The extended 128-byte uniform includes eye position,
    /// camera right, and camera up vectors for billboard rendering.
    pub fn update_camera(&self, queue: &wgpu::Queue, camera: &OrbitCamera) {
        let eye = camera.eye_position();
        let view_proj = camera.view_proj_matrix();

        // Derive camera orientation vectors from view matrix
        let view = glam::Mat4::look_at_rh(eye, camera.focus, Vec3::Y);
        let right = Vec3::new(view.col(0).x, view.col(1).x, view.col(2).x);
        let up = Vec3::new(view.col(0).y, view.col(1).y, view.col(2).y);

        let uniform = CameraUniform3D {
            view_proj: view_proj.to_cols_array(),
            eye_position: eye.into(),
            _pad0: 0.0,
            camera_right: right.into(),
            _pad1: 0.0,
            camera_up: up.into(),
            _pad2: 0.0,
        };
        queue.write_buffer(
            &self.camera_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniform),
        );
    }

    /// Update the lighting uniform buffer from simulation elapsed time.
    pub fn update_lighting(&self, queue: &wgpu::Queue, sim_elapsed_seconds: f64) {
        let lighting = compute_lighting(sim_elapsed_seconds);
        queue.write_buffer(
            &self.lighting_uniform_buffer,
            0,
            bytemuck::bytes_of(&lighting),
        );
    }

    /// Upload agent instance data for mesh and billboard rendering.
    ///
    /// Creates GPU instance buffers for each vehicle type that has instances.
    pub fn upload_agent_instances(
        &mut self,
        device: &wgpu::Device,
        mesh_instances: &HashMap<VehicleType, Vec<MeshInstance3D>>,
        billboard_instances: &HashMap<VehicleType, Vec<BillboardInstance3D>>,
    ) {
        self.mesh_instance_buffers.clear();
        for (vtype, instances) in mesh_instances {
            if instances.is_empty() {
                continue;
            }
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("mesh_inst_{vtype:?}")),
                contents: bytemuck::cast_slice(instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.mesh_instance_buffers
                .insert(*vtype, (buffer, instances.len() as u32));
        }

        self.billboard_instance_buffers.clear();
        for (vtype, instances) in billboard_instances {
            if instances.is_empty() {
                continue;
            }
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("bb_inst_{vtype:?}")),
                contents: bytemuck::cast_slice(instances),
                usage: wgpu::BufferUsages::VERTEX,
            });
            self.billboard_instance_buffers
                .insert(*vtype, (buffer, instances.len() as u32));
        }
    }

    /// Render the ground plane with depth testing.
    ///
    /// Clears both color (dark background) and depth buffers, then draws the
    /// ground plane quad.
    pub fn render_ground(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ground_plane_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.05,
                        g: 0.05,
                        b: 0.1,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture_view,
                depth_ops: Some(wgpu::Operations {
                    // Reverse-Z: clear to 0.0 (= infinitely far)
                    load: wgpu::LoadOp::Clear(0.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        pass.set_pipeline(&self.ground_plane_pipeline);
        pass.set_bind_group(0, &self.ground_bind_group, &[]);
        pass.set_vertex_buffer(0, self.ground_plane_vertex_buffer.slice(..));
        pass.draw(0..self.ground_plane_vertex_count, 0..1);
    }

    /// Upload road geometry (surfaces, markings, junctions) from a RoadGraph.
    ///
    /// Generates all road mesh data and uploads to static GPU vertex buffers.
    /// This should be called once at load time; the buffers are rendered every frame.
    pub fn upload_road_geometry(
        &mut self,
        device: &wgpu::Device,
        graph: &RoadGraph,
        junction_data: &HashMap<u32, JunctionData>,
    ) {
        // Road surfaces
        let road_verts = generate_road_mesh(graph);
        if !road_verts.is_empty() {
            self.road_vertex_buffer =
                Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("road_surface_vertices"),
                    contents: bytemuck::cast_slice(&road_verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }));
            self.road_vertex_count = road_verts.len() as u32;
        }

        // Lane markings
        let marking_verts = generate_lane_markings(graph);
        if !marking_verts.is_empty() {
            self.marking_vertex_buffer =
                Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("lane_marking_vertices"),
                    contents: bytemuck::cast_slice(&marking_verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }));
            self.marking_vertex_count = marking_verts.len() as u32;
        }

        // Junction surfaces
        let junction_verts = generate_junction_surfaces(junction_data);
        if !junction_verts.is_empty() {
            self.junction_vertex_buffer =
                Some(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("junction_surface_vertices"),
                    contents: bytemuck::cast_slice(&junction_verts),
                    usage: wgpu::BufferUsages::VERTEX,
                }));
            self.junction_vertex_count = junction_verts.len() as u32;
        }

        log::info!(
            "Uploaded road geometry: {} road verts, {} marking verts, {} junction verts",
            self.road_vertex_count,
            self.marking_vertex_count,
            self.junction_vertex_count,
        );
    }

    /// Render road surfaces, junction fills, and lane markings.
    fn render_roads(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.road_pipeline);
        pass.set_bind_group(0, &self.ground_bind_group, &[]);

        if let Some(ref buf) = self.road_vertex_buffer {
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.road_vertex_count, 0..1);
        }

        if let Some(ref buf) = self.junction_vertex_buffer {
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.junction_vertex_count, 0..1);
        }

        if let Some(ref buf) = self.marking_vertex_buffer {
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.marking_vertex_count, 0..1);
        }
    }

    /// Render 3D mesh agents (nearest tier).
    fn render_meshes(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.mesh_pipeline);
        pass.set_bind_group(0, &self.agent_bind_group, &[]);

        for (vtype, (instance_buf, instance_count)) in &self.mesh_instance_buffers {
            if let Some(gpu_mesh) = self.mesh_set.meshes.get(vtype) {
                pass.set_vertex_buffer(0, gpu_mesh.vertex_buffer.slice(..));
                pass.set_vertex_buffer(1, instance_buf.slice(..));
                pass.set_index_buffer(gpu_mesh.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..gpu_mesh.index_count, 0, 0..*instance_count);
            }
        }
    }

    /// Render billboard agents (mid-range tier).
    fn render_billboards(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.billboard_pipeline);
        pass.set_bind_group(0, &self.agent_bind_group, &[]);

        for (instance_buf, instance_count) in self.billboard_instance_buffers.values() {
            pass.set_vertex_buffer(0, instance_buf.slice(..));
            // 6 vertices per billboard quad (two triangles)
            pass.draw(0..6, 0..*instance_count);
        }
    }

    /// Render a full 3D frame: ground plane, road surfaces, 3D meshes, billboards.
    pub fn render_frame(&self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        // Ground plane clears color + depth
        self.render_ground(encoder, view);

        // Road geometry + agent rendering in a second pass (loads existing color/depth)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            self.render_roads(&mut pass);
            self.render_meshes(&mut pass);
            self.render_billboards(&mut pass);
        }
    }

    /// Returns the camera bind group layout for other 3D pipelines to reuse.
    pub fn camera_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.camera_bind_group_layout
    }

    /// Returns the depth texture view for other render passes.
    pub fn depth_view(&self) -> &wgpu::TextureView {
        &self.depth_texture_view
    }

    /// Returns the surface format this renderer was created with.
    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ground_plane_wgsl_naga_validates() {
        let source = include_str!("../shaders/ground_plane.wgsl");
        let result = naga::front::wgsl::parse_str(source);
        match result {
            Ok(module) => {
                let mut validator = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::empty(),
                );
                validator
                    .validate(&module)
                    .expect("WGSL validation failed");
            }
            Err(e) => panic!("WGSL parse failed: {e}"),
        }
    }

    #[test]
    fn test_mesh_3d_wgsl_naga_validates() {
        let source = include_str!("../shaders/mesh_3d.wgsl");
        let result = naga::front::wgsl::parse_str(source);
        match result {
            Ok(module) => {
                let mut validator = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::empty(),
                );
                validator
                    .validate(&module)
                    .expect("mesh_3d.wgsl validation failed");
            }
            Err(e) => panic!("mesh_3d.wgsl parse failed: {e}"),
        }
    }

    #[test]
    fn test_billboard_3d_wgsl_naga_validates() {
        let source = include_str!("../shaders/billboard_3d.wgsl");
        let result = naga::front::wgsl::parse_str(source);
        match result {
            Ok(module) => {
                let mut validator = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::empty(),
                );
                validator
                    .validate(&module)
                    .expect("billboard_3d.wgsl validation failed");
            }
            Err(e) => panic!("billboard_3d.wgsl parse failed: {e}"),
        }
    }

    #[test]
    fn test_ground_plane_vertices_layout() {
        let verts = ground_plane_vertices();
        assert_eq!(
            verts.len(),
            6,
            "Ground plane should have 6 vertices (2 triangles)"
        );
        for v in &verts {
            assert!(
                (v.position[1] - GROUND_Y).abs() < 1e-6,
                "Y should be {GROUND_Y}, got {}", v.position[1]
            );
        }
    }

    #[test]
    fn test_camera_uniform_3d_size() {
        // 64 (view_proj) + 16 (eye_pos + pad) + 16 (right + pad) + 16 (up + pad) = 112
        assert_eq!(
            std::mem::size_of::<CameraUniform3D>(),
            112,
            "CameraUniform3D should be 112 bytes"
        );
    }

    #[test]
    fn test_ground_plane_vertex_size() {
        assert_eq!(
            std::mem::size_of::<GroundPlaneVertex>(),
            28,
            "GroundPlaneVertex should be 28 bytes"
        );
    }

    #[test]
    fn test_road_surface_wgsl_naga_validates() {
        let source = include_str!("../shaders/road_surface.wgsl");
        let result = naga::front::wgsl::parse_str(source);
        match result {
            Ok(module) => {
                let mut validator = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::empty(),
                );
                validator
                    .validate(&module)
                    .expect("road_surface.wgsl validation failed");
            }
            Err(e) => panic!("road_surface.wgsl parse failed: {e}"),
        }
    }
}
