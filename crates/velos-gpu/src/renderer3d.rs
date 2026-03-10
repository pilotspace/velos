//! Renderer3D: 3D rendering with depth buffer, ground plane, and road surfaces.
//!
//! This renderer is independent of the existing 2D `Renderer`. It manages its
//! own depth texture, camera uniform buffer, and render pipelines for 3D content
//! (ground plane, road surfaces, lane markings, junction fills).
//!
//! Plans 03-04 extend this with mesh instancing, billboard rendering, lighting,
//! and view mode toggle wiring.

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use crate::orbit_camera::{create_depth_texture, OrbitCamera};
use crate::road_surface::{
    generate_junction_surfaces, generate_lane_markings, generate_road_mesh,
    JunctionData, RoadSurfaceVertex,
};
use velos_net::RoadGraph;

/// Camera uniform buffer layout for 3D rendering.
/// Must match WGSL CameraUniform struct in ground_plane.wgsl and road_surface.wgsl.
#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
struct CameraUniform3D {
    view_proj: [f32; 16],
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

/// Ground plane Y offset (slightly below zero to avoid z-fighting with road surfaces).
const GROUND_Y: f32 = -0.01;

/// Generate ground plane vertices: two triangles forming a 20000x20000 quad.
fn ground_plane_vertices() -> [GroundPlaneVertex; 6] {
    let s = GROUND_HALF_SIZE;
    let y = GROUND_Y;
    let c = GROUND_COLOR;
    [
        // Triangle 1
        GroundPlaneVertex { position: [-s, y, -s], color: c },
        GroundPlaneVertex { position: [ s, y, -s], color: c },
        GroundPlaneVertex { position: [ s, y,  s], color: c },
        // Triangle 2
        GroundPlaneVertex { position: [-s, y, -s], color: c },
        GroundPlaneVertex { position: [ s, y,  s], color: c },
        GroundPlaneVertex { position: [-s, y,  s], color: c },
    ]
}

/// 3D renderer with depth buffer, ground plane, and road surface rendering.
///
/// Owns the depth texture, camera uniform buffer, ground plane pipeline, and
/// road surface pipeline with vertex buffers for roads, markings, and junctions.
/// Extended by Plans 03-04 with mesh instancing, billboard rendering, and lighting.
pub struct Renderer3D {
    depth_texture_view: wgpu::TextureView,
    camera_uniform_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    camera_bind_group_layout: wgpu::BindGroupLayout,
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
}

impl Renderer3D {
    /// Create a new 3D renderer with depth buffer and ground plane pipeline.
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        // Depth texture
        let depth_texture_view = create_depth_texture(device, width, height);

        // Camera uniform buffer (64 bytes for Mat4)
        let camera_uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera_uniform_3d"),
            size: std::mem::size_of::<CameraUniform3D>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Camera bind group layout (shared by all 3D pipelines)
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

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera_bg_3d"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_uniform_buffer.as_entire_binding(),
            }],
        });

        // Ground plane shader and pipeline
        let shader = device.create_shader_module(wgpu::include_wgsl!("../shaders/ground_plane.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ground_plane_pipeline_layout"),
            bind_group_layouts: &[&camera_bind_group_layout],
            push_constant_ranges: &[],
        });

        let ground_plane_vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GroundPlaneVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    // position: vec3<f32>
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    // color: vec4<f32>
                    offset: 12,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        };

        let ground_plane_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("ground_plane_pipeline"),
                layout: Some(&pipeline_layout),
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
                    cull_mode: None, // Render both sides of ground plane
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        // Ground plane vertex buffer
        let vertices = ground_plane_vertices();
        let ground_plane_vertex_buffer =
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("ground_plane_vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });

        // Road surface pipeline (same vertex layout and camera bind group as ground plane)
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
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState::default(),
                multiview: None,
                cache: None,
            });

        Self {
            depth_texture_view,
            camera_uniform_buffer,
            camera_bind_group,
            camera_bind_group_layout,
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
        }
    }

    /// Recreate the depth texture on window resize.
    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        self.depth_texture_view = create_depth_texture(device, width, height);
    }

    /// Upload the orbit camera's view-projection matrix to the uniform buffer.
    pub fn update_camera(&self, queue: &wgpu::Queue, camera: &OrbitCamera) {
        let m = camera.view_proj_matrix();
        let uniform = CameraUniform3D {
            view_proj: m.to_cols_array(),
        };
        queue.write_buffer(
            &self.camera_uniform_buffer,
            0,
            bytemuck::bytes_of(&uniform),
        );
    }

    /// Render the ground plane with depth testing.
    ///
    /// Clears both color (dark background) and depth buffers, then draws the
    /// ground plane quad.
    pub fn render_ground(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
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
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });

        pass.set_pipeline(&self.ground_plane_pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);
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
    ///
    /// Draw order (back to front via Y offsets):
    /// 1. Road surfaces (y=0.0)
    /// 2. Junction surfaces (y=0.005)
    /// 3. Lane markings (y=0.01)
    fn render_roads(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.road_pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);

        // Road surfaces
        if let Some(ref buf) = self.road_vertex_buffer {
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.road_vertex_count, 0..1);
        }

        // Junction surfaces
        if let Some(ref buf) = self.junction_vertex_buffer {
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.junction_vertex_count, 0..1);
        }

        // Lane markings (on top)
        if let Some(ref buf) = self.marking_vertex_buffer {
            pass.set_vertex_buffer(0, buf.slice(..));
            pass.draw(0..self.marking_vertex_count, 0..1);
        }
    }

    /// Render a full 3D frame: ground plane, road surfaces, markings, junctions.
    ///
    /// Extended by Plans 03 and 04 with mesh instances and billboards.
    pub fn render_frame(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
    ) {
        // Ground plane clears color + depth
        self.render_ground(encoder, view);

        // Road geometry in a second pass (loads existing color/depth)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("road_surface_pass"),
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
                // Also run the validator to catch semantic errors
                let mut validator = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::empty(),
                );
                validator.validate(&module).expect("WGSL validation failed");
            }
            Err(e) => panic!("WGSL parse failed: {e}"),
        }
    }

    #[test]
    fn test_ground_plane_vertices_layout() {
        let verts = ground_plane_vertices();
        assert_eq!(verts.len(), 6, "Ground plane should have 6 vertices (2 triangles)");
        // All vertices at y=-0.01
        for v in &verts {
            assert!((v.position[1] - GROUND_Y).abs() < 1e-6, "Y should be {GROUND_Y}");
        }
    }

    #[test]
    fn test_camera_uniform_3d_size() {
        assert_eq!(
            std::mem::size_of::<CameraUniform3D>(),
            64,
            "CameraUniform3D should be 64 bytes (4x4 f32 matrix)"
        );
    }

    #[test]
    fn test_ground_plane_vertex_size() {
        // 3 floats (position) + 4 floats (color) = 7 * 4 = 28 bytes
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
