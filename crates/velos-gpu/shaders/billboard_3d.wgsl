// billboard_3d.wgsl -- Camera-facing billboard shader with instancing.
//
// Renders billboards (camera-facing quads) for mid-range agent LOD.
// Each instance provides world position, size, and color.
// Billboards always face the camera using the camera_right and camera_up vectors.
//
// Bind groups:
//   @group(0) @binding(0): Camera uniform (view_proj, eye_position, camera_right, camera_up)
//   @group(0) @binding(1): Lighting uniform (for ambient tint only)

struct CameraUniform {
    view_proj: mat4x4<f32>,
    eye_position: vec3<f32>,
    _pad0: f32,
    camera_right: vec3<f32>,
    _pad1: f32,
    camera_up: vec3<f32>,
    _pad2: f32,
}

struct LightingUniform {
    sun_direction: vec3<f32>,
    _pad0: f32,
    sun_color: vec3<f32>,
    _pad1: f32,
    ambient_color: vec3<f32>,
    ambient_intensity: f32,
}

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<uniform> lighting: LightingUniform;

// Instance attributes (from instance buffer)
struct InstanceInput {
    @location(0) world_pos: vec3<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) _pad: f32,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) frag_color: vec4<f32>,
}

@vertex
fn vs_main(
    @builtin(vertex_index) vertex_index: u32,
    instance: InstanceInput,
) -> VertexOutput {
    // Expand 4 billboard corners from vertex_index (0-5 for two triangles)
    // Triangle 1: 0,1,2  Triangle 2: 2,1,3  (using 6 vertices for a quad)
    var uv: vec2<f32>;
    switch(vertex_index % 6u) {
        case 0u: { uv = vec2<f32>(-0.5, -0.5); }
        case 1u: { uv = vec2<f32>( 0.5, -0.5); }
        case 2u: { uv = vec2<f32>(-0.5,  0.5); }
        case 3u: { uv = vec2<f32>(-0.5,  0.5); }
        case 4u: { uv = vec2<f32>( 0.5, -0.5); }
        case 5u: { uv = vec2<f32>( 0.5,  0.5); }
        default: { uv = vec2<f32>(0.0, 0.0); }
    }

    // Expand billboard in camera space
    let right_offset = camera.camera_right * (uv.x * instance.size.x);
    let up_offset = camera.camera_up * (uv.y * instance.size.y);
    let world_pos = instance.world_pos + right_offset + up_offset;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.frag_color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Billboard receives ambient tinting only (no diffuse, flat surface)
    let ambient = lighting.ambient_color * lighting.ambient_intensity;
    // Blend: 50% base color + 50% ambient-tinted to keep billboard readable
    let tinted = mix(in.frag_color.rgb, in.frag_color.rgb * ambient, vec3<f32>(0.5));
    return vec4<f32>(tinted, in.frag_color.a);
}
