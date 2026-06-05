// Minimal instanced-quad shader. Each unit is one instance: a small quad at a
// world position, tinted by team color. Lighting/PBR come in later milestones —
// this is M1 "see the deterministic sim move."

struct Camera {
    view_proj: mat4x4<f32>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(
    @location(0) quad: vec2<f32>,    // unit-quad corner (already sized)
    @location(1) offset: vec2<f32>,  // per-instance world position
    @location(2) color: vec4<f32>,   // per-instance team color
) -> VsOut {
    let world = offset + quad;
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(world, 0.0, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
