// mesh_3d.wgsl -- Lit 3D mesh shader with instancing.
//
// Renders 3D meshes with diffuse + ambient shading from a directional sun.
// Each instance provides world position, heading (Y-axis rotation), and color.
//
// Bind groups:
//   @group(0) @binding(0): Camera uniform (view_proj, eye_position)
//   @group(0) @binding(1): Lighting uniform (sun_direction, sun_color, ambient)

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

// Vertex attributes (from mesh vertex buffer)
struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

// Instance attributes (from instance buffer)
struct InstanceInput {
    @location(2) world_pos: vec3<f32>,
    @location(3) heading: f32,
    @location(4) color: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_normal: vec3<f32>,
    @location(1) frag_color: vec4<f32>,
}

@vertex
fn vs_main(vertex: VertexInput, instance: InstanceInput) -> VertexOutput {
    // Rotate vertex around Y axis by heading
    let cos_h = cos(instance.heading);
    let sin_h = sin(instance.heading);

    let rotated_pos = vec3<f32>(
        vertex.position.x * cos_h - vertex.position.z * sin_h,
        vertex.position.y,
        vertex.position.x * sin_h + vertex.position.z * cos_h,
    );

    let rotated_normal = vec3<f32>(
        vertex.normal.x * cos_h - vertex.normal.z * sin_h,
        vertex.normal.y,
        vertex.normal.x * sin_h + vertex.normal.z * cos_h,
    );

    // Translate by instance world position
    let world_pos = rotated_pos + instance.world_pos;

    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.world_normal = normalize(rotated_normal);
    out.frag_color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let N = normalize(in.world_normal);
    // Sun direction points toward the sun; negate for light direction toward surface
    let L = normalize(-lighting.sun_direction);

    // Diffuse shading (Lambert)
    let n_dot_l = max(dot(N, L), 0.0);
    let diffuse = lighting.sun_color * n_dot_l;

    // Ambient
    let ambient = lighting.ambient_color * lighting.ambient_intensity;

    // Final color: (ambient + diffuse) * base color
    let lit_color = (ambient + diffuse) * in.frag_color.rgb;
    return vec4<f32>(lit_color, in.frag_color.a);
}
