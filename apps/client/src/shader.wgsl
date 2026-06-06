// RTS world: textured heightmap terrain (grass/dirt/rock/sand) with fog-of-war
// darkening, animated water, lit unit boxes, and selection rings.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light_dir: vec4<f32>,
    params: vec4<f32>, // x = time, y = map half-size, z = sea level
};
@group(0) @binding(0) var<uniform> cam: Camera;

// group 1: terrain tiles + fog-of-war (also bound to the water pipeline).
@group(1) @binding(0) var t_grass: texture_2d<f32>;
@group(1) @binding(1) var t_dirt: texture_2d<f32>;
@group(1) @binding(2) var t_rock: texture_2d<f32>;
@group(1) @binding(3) var t_sand: texture_2d<f32>;
@group(1) @binding(4) var t_fow: texture_2d<f32>;
@group(1) @binding(5) var samp_tile: sampler;
@group(1) @binding(6) var samp_fow: sampler;

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

// Fog-of-war brightness at a world point: ~0.06 unexplored, ~0.45 explored, 1 visible.
fn fow(world: vec3<f32>) -> f32 {
    let uv = (world.xz + vec2<f32>(cam.params.y)) / (2.0 * cam.params.y);
    let v = textureSample(t_fow, samp_fow, uv).r;
    return max(0.06, v);
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
    let uv = in.world.xz * 0.25;

    let grass = textureSample(t_grass, samp_tile, uv).rgb;
    let dirt = textureSample(t_dirt, samp_tile, uv).rgb;
    let rock = textureSample(t_rock, samp_tile, uv).rgb;
    let sand = textureSample(t_sand, samp_tile, uv).rgb;

    var col = grass;
    col = mix(col, dirt, smoothstep(0.45, 0.75, noise2(in.world.xz * 0.03)));
    col = mix(col, sand, smoothstep(cam.params.z + 2.5, cam.params.z - 0.5, in.world.y));
    col = mix(col, rock, smoothstep(0.32, 0.55, slope));

    let ndl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    col = col * (0.45 + 0.7 * ndl) * fow(in.world);
    return vec4<f32>(col, 1.0);
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
    let p = in.world.xz;
    // Rippled surface normal: sum of animated directional waves at a few scales
    // (a procedural stand-in for a scrolling normal map).
    var dx = 0.0;
    var dz = 0.0;
    dx += cos(p.x * 0.090 + t * 1.40) * 0.30;
    dz += cos(p.y * 0.085 - t * 1.20) * 0.30;
    dx += cos((p.x * 0.05 + p.y * 0.03) + t * 0.90) * 0.18;
    dz += cos((p.y * 0.05 - p.x * 0.035) - t * 1.05) * 0.18;
    dx += cos(p.x * 0.210 - t * 2.30) * 0.07;
    dz += cos(p.y * 0.190 + t * 2.10) * 0.07;
    let n = normalize(vec3<f32>(dx, 1.0, dz));
    let view = normalize(cam.eye.xyz - in.world);
    let light = normalize(cam.light_dir.xyz);

    let deep = vec3<f32>(0.04, 0.13, 0.24);
    let shallow = vec3<f32>(0.10, 0.34, 0.46);
    let ndl = max(dot(n, light), 0.0);
    var col = mix(deep, shallow, ndl);
    // Fresnel: more sky reflection at grazing angles.
    let fres = pow(1.0 - max(dot(n, view), 0.0), 4.0);
    let sky = vec3<f32>(0.45, 0.62, 0.85);
    col = mix(col, sky, fres * 0.6);
    // Sharp sun glint.
    let spec = pow(max(dot(reflect(-light, n), view), 0.0), 80.0);
    col += vec3<f32>(spec) * 0.8;
    col = col * fow(in.world);
    // Mostly opaque (so the seabed doesn't read through), more so at grazing.
    let alpha = mix(0.85, 0.98, fres);
    return vec4<f32>(col, alpha);
}

// ---------------- units / buildings ----------------
struct UnitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) albedo: vec3<f32>,
};
@vertex
fn vs_unit(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(5) mcol: vec4<f32>,
    @location(2) offset: vec3<f32>,
    @location(3) scale: vec3<f32>,
    @location(4) tcol: vec4<f32>,
) -> UnitOut {
    var o: UnitOut;
    o.normal = normal;
    // mcol.a is the team-tint weight: blend the material toward the faction
    // color so banners/tabards/plumes read as team color, metal/skin stay neutral.
    o.albedo = mix(mcol.rgb, tcol.rgb, mcol.a);
    o.clip = cam.view_proj * vec4<f32>(pos * scale + offset, 1.0);
    return o;
}
@fragment
fn fs_unit(in: UnitOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let ndl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    return vec4<f32>(in.albedo * (0.45 + 0.7 * ndl), 1.0);
}

// ---------------- selection rings (ground decals) ----------------
// Built CPU-side as an annulus whose vertices follow the terrain height, so the
// ring hugs uneven ground instead of clipping through hills.
struct RingOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};
@vertex
fn vs_ring(@location(0) pos: vec3<f32>, @location(1) color: vec4<f32>) -> RingOut {
    var o: RingOut;
    o.color = color;
    o.clip = cam.view_proj * vec4<f32>(pos, 1.0);
    return o;
}
@fragment
fn fs_ring(in: RingOut) -> @location(0) vec4<f32> {
    return in.color;
}
