// RTS world: heightmap terrain (grass/dirt/cliff/sand), animated water, lit unit
// boxes, and ground selection rings — with distance fog. Presentation-only.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,       // camera world position
    light_dir: vec4<f32>, // direction TO the sun (xyz)
    params: vec4<f32>,    // x = time, y = fog density, z = sea level
};
@group(0) @binding(0) var<uniform> cam: Camera;

fn apply_fog(color: vec3<f32>, world: vec3<f32>) -> vec3<f32> {
    let d = distance(world, cam.eye.xyz);
    let f = clamp(1.0 - exp(-d * cam.params.y), 0.0, 1.0);
    let fog_col = vec3<f32>(0.46, 0.56, 0.68);
    return mix(color, fog_col, f);
}

fn hash2(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.547);
}
fn noise2(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let a = hash2(i);
    let b = hash2(i + vec2<f32>(1.0, 0.0));
    let c = hash2(i + vec2<f32>(0.0, 1.0));
    let d = hash2(i + vec2<f32>(1.0, 1.0));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// ---------------- terrain ----------------
struct TerrainOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
};
@vertex
fn vs_terrain(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>) -> TerrainOut {
    var o: TerrainOut;
    o.world = pos;
    o.normal = normal;
    o.clip = cam.view_proj * vec4<f32>(pos, 1.0);
    return o;
}
@fragment
fn fs_terrain(in: TerrainOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let slope = clamp(1.0 - n.y, 0.0, 1.0);
    let nz = noise2(in.world.xz * 0.25);

    var base = mix(vec3<f32>(0.17, 0.40, 0.15), vec3<f32>(0.36, 0.30, 0.16), nz * 0.7);
    // sand near the waterline
    let near = smoothstep(cam.params.z + 1.6, cam.params.z - 0.4, in.world.y);
    base = mix(base, vec3<f32>(0.62, 0.57, 0.38), near * 0.85);
    // rocky cliffs on steep slopes
    let rock = mix(vec3<f32>(0.30, 0.28, 0.26), vec3<f32>(0.19, 0.18, 0.17), nz);
    base = mix(base, rock, smoothstep(0.34, 0.6, slope));

    let ndl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    var col = base * (0.40 + 0.75 * ndl);
    return vec4<f32>(apply_fog(col, in.world), 1.0);
}

// ---------------- water ----------------
struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};
@vertex
fn vs_water(@location(0) pos: vec3<f32>) -> WaterOut {
    var o: WaterOut;
    o.world = vec3<f32>(pos.x, cam.params.z, pos.z);
    o.clip = cam.view_proj * vec4<f32>(o.world, 1.0);
    return o;
}
@fragment
fn fs_water(in: WaterOut) -> @location(0) vec4<f32> {
    let t = cam.params.x;
    let wx = sin(in.world.x * 0.30 + t * 1.3);
    let wz = cos(in.world.z * 0.27 - t * 1.1);
    let n = normalize(vec3<f32>(wx * 0.15, 1.0, wz * 0.15));
    let ndl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    let tone = 0.5 + 0.5 * (wx + wz) * 0.5;
    var col = mix(vec3<f32>(0.03, 0.15, 0.29), vec3<f32>(0.10, 0.34, 0.47), tone);
    col += vec3<f32>(ndl * 0.18);
    return vec4<f32>(apply_fog(col, in.world), 0.74);
}

// ---------------- units / buildings ----------------
struct UnitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec3<f32>,
};
@vertex
fn vs_unit(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) offset: vec3<f32>,
    @location(3) scale: vec3<f32>,
    @location(4) color: vec4<f32>,
) -> UnitOut {
    var o: UnitOut;
    let world = pos * scale + offset;
    o.world = world;
    o.normal = normal;
    o.color = color.rgb;
    o.clip = cam.view_proj * vec4<f32>(world, 1.0);
    return o;
}
@fragment
fn fs_unit(in: UnitOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let ndl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    var col = in.color * (0.36 + 0.72 * ndl);
    return vec4<f32>(apply_fog(col, in.world), 1.0);
}

// ---------------- selection rings ----------------
struct RingOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};
@vertex
fn vs_ring(
    @location(0) quad: vec2<f32>,
    @location(1) center: vec3<f32>,
    @location(2) radius: f32,
    @location(3) color: vec4<f32>,
) -> RingOut {
    var o: RingOut;
    let world = center + vec3<f32>(quad.x * radius, 0.06, quad.y * radius);
    o.uv = quad;
    o.color = color;
    o.clip = cam.view_proj * vec4<f32>(world, 1.0);
    return o;
}
@fragment
fn fs_ring(in: RingOut) -> @location(0) vec4<f32> {
    let r = length(in.uv);
    let a = smoothstep(1.0, 0.93, r) * smoothstep(0.78, 0.88, r);
    if (a < 0.02) {
        discard;
    }
    return vec4<f32>(in.color.rgb, a * in.color.a);
}
