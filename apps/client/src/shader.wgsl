// RTS look: a procedural ground grid + instanced, vertex-lit unit cubes.
// Pure presentation (floats/GPU); the sim never sees any of this.

struct Camera {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>, // direction TO the light (xyz), normalized
};
@group(0) @binding(0) var<uniform> cam: Camera;

// ---------------- ground ----------------
struct GroundOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};

@vertex
fn vs_ground(@location(0) pos: vec3<f32>) -> GroundOut {
    var o: GroundOut;
    o.world = pos;
    o.clip = cam.view_proj * vec4<f32>(pos, 1.0);
    return o;
}

@fragment
fn fs_ground(in: GroundOut) -> @location(0) vec4<f32> {
    // Anti-aliased grid lines from world XZ coordinates.
    let cell = 4.0;
    let coord = in.world.xz / cell;
    let deriv = fwidth(coord);
    let g = abs(fract(coord - 0.5) - 0.5) / max(deriv, vec2<f32>(0.0001));
    let line = min(g.x, g.y);
    let intensity = 1.0 - min(line, 1.0);

    // Radial fade so the plane melts into the space backdrop.
    let dist = length(in.world.xz);
    let fade = clamp(1.0 - dist / 75.0, 0.0, 1.0);

    let base = vec3<f32>(0.04, 0.06, 0.11);
    let line_col = vec3<f32>(0.22, 0.40, 0.62);
    let col = mix(base, line_col, intensity) * fade;
    return vec4<f32>(col, 1.0);
}

// ---------------- units ----------------
struct UnitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_unit(
    @location(0) pos: vec3<f32>,    // cube vertex
    @location(1) normal: vec3<f32>, // cube normal
    @location(2) offset: vec3<f32>, // per-instance world position
    @location(3) color: vec4<f32>,  // per-instance team color
) -> UnitOut {
    let world = pos + offset;
    var o: UnitOut;
    o.clip = cam.view_proj * vec4<f32>(world, 1.0);
    // Vertex lighting: ambient + directional Lambert.
    let n = normalize(normal);
    let ndl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    let lit = 0.35 + 0.65 * ndl;
    o.color = color.rgb * lit;
    return o;
}

@fragment
fn fs_unit(in: UnitOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color, 1.0);
}
