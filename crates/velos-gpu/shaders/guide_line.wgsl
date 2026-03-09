// Dashed line shader for Bezier guide lines through junctions.
//
// Renders thin quad strips with a discard-based dash pattern.
// Shares the same camera uniform as map_tile.wgsl and agent_render.wgsl.

struct CameraUniform {
    view_proj: mat4x4f,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec2f,
    @location(1) color: vec4f,
    @location(2) line_dist: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) color: vec4f,
    @location(1) line_dist: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4f(in.position, 0.0, 1.0);
    out.color = in.color;
    out.line_dist = in.line_dist;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Dash pattern: 3m dash, 2m gap (5m period).
    // Discard fragments in the gap portion (> 60% of period).
    let pattern = fract(in.line_dist / 5.0);
    if (pattern > 0.6) {
        discard;
    }
    return in.color;
}
