// RTS world: textured heightmap terrain (grass/dirt/rock/sand) with fog-of-war
// darkening, animated water, lit unit boxes, and selection rings.

struct Camera {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light_dir: vec4<f32>,
    params: vec4<f32>, // x = time, y = map half-size, z = sea level, w = light count
    light_pos: array<vec4<f32>, 16>, // xyz = position, w = radius
    light_col: array<vec4<f32>, 16>, // rgb
};
@group(0) @binding(0) var<uniform> cam: Camera;

// Dynamic fx point lights (muzzle flashes, mining sparks, explosions):
// quadratic falloff to the radius, diffuse against the surface normal.
fn point_lights(world: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var sum = vec3<f32>(0.0, 0.0, 0.0);
    let count = u32(cam.params.w);
    for (var i = 0u; i < count; i = i + 1u) {
        let lp = cam.light_pos[i];
        let to = lp.xyz - world;
        let d = length(to);
        if (d < lp.w) {
            let att = 1.0 - d / lp.w;
            let ndl = max(dot(n, to / max(d, 0.001)), 0.0);
            sum = sum + cam.light_col[i].rgb * (att * att * ndl);
        }
    }
    return sum;
}

// group 1: terrain tiles + fog-of-war (also bound to the water pipeline).
@group(1) @binding(0) var t_grass: texture_2d<f32>;
@group(1) @binding(1) var t_dirt: texture_2d<f32>;
@group(1) @binding(2) var t_rock: texture_2d<f32>;
@group(1) @binding(3) var t_sand: texture_2d<f32>;
@group(1) @binding(4) var t_fow: texture_2d<f32>;
@group(1) @binding(5) var samp_tile: sampler;
@group(1) @binding(6) var samp_fow: sampler;

// group 2: per-world appearance (voxel terrain + liquid). Set when a battlefield
// is selected; defaults to a neutral tint + Earthlike water for the heightmap map.
struct World {
    tint: vec4<f32>,   // terrain tint rgb; a = lava/emissive strength
    liquid: vec4<f32>, // liquid body colour rgb; a = roughness (waviness)
};
@group(2) @binding(0) var<uniform> world: World;

// Triplanar sample of a tile: project on the three axis planes and blend by the
// surface normal, so vertical cliffs and overhangs are textured without stretch.
fn triplanar(tex: texture_2d<f32>, p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    var w = pow(abs(n), vec3<f32>(4.0));
    w = w / max(w.x + w.y + w.z, 0.001);
    let s = 0.08;
    let cx = textureSample(tex, samp_tile, p.zy * s).rgb;
    let cy = textureSample(tex, samp_tile, p.xz * s).rgb;
    let cz = textureSample(tex, samp_tile, p.xy * s).rgb;
    return cx * w.x + cy * w.y + cz * w.z;
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
    let lit = 0.45 + 0.7 * ndl;
    col = col * (vec3<f32>(lit, lit, lit) + point_lights(in.world, n)) * fow(in.world);
    return vec4<f32>(col, 1.0);
}

// ---------------- voxel terrain (marching-cubes worlds) ----------------
// A 3D mesh, so it is textured triplanar from the selected world's tile set
// (base/low/high/accent bound to the grass/dirt/rock/sand slots) and tinted per
// world. Each vertex carries soft blend weights (mid/low/high/accent + hazard)
// sampled from the 3D material field, so tiles fade into each other across a
// material boundary (e.g. surface skin -> exposed subsurface) instead of
// switching abruptly. Steep faces still lean a little toward the "high" tile.
struct VoxelOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) w: vec4<f32>,
    @location(3) haz: f32,
};
@vertex
fn vs_voxel(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) w: vec4<f32>,
    @location(3) haz: f32,
) -> VoxelOut {
    var o: VoxelOut;
    o.world = pos;
    o.normal = normal;
    o.w = w;
    o.haz = haz;
    o.clip = cam.view_proj * vec4<f32>(pos, 1.0);
    return o;
}
@fragment
fn fs_voxel(in: VoxelOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let p = in.world;
    let w = in.w; // x mid/base, y low, z high, w accent
    let haz = in.haz;

    // Triplanar-sample each tile once, then blend by the per-vertex weights.
    let cg = triplanar(t_grass, p, n);
    let cd = triplanar(t_dirt, p, n);
    let cr = triplanar(t_rock, p, n);
    let cs = triplanar(t_sand, p, n);
    let lava = mix(cs, vec3<f32>(0.95, 0.42, 0.12), 0.55);
    var col = w.x * cg + w.y * cd + w.z * cr + w.w * cs + haz * lava;

    // Steep faces lean toward exposed rock/ice, but only at near-vertical and
    // gently, so the material weights (skin vs subsurface) stay in charge.
    let slope = clamp(1.0 - n.y, 0.0, 1.0);
    col = mix(col, cr, smoothstep(0.65, 0.95, slope) * (1.0 - haz) * 0.55);
    col = col * world.tint.rgb;

    let emis = vec3<f32>(1.0, 0.45, 0.12) * world.tint.a * haz;
    let ndl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    let base = 0.4 + 0.7 * ndl;
    let lit = (col * (vec3<f32>(base, base, base) + point_lights(in.world, n)) + emis) * fow(in.world);
    return vec4<f32>(lit, 1.0);
}

// ---------------- water ----------------
struct WaterOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
};
@vertex
fn vs_water(@location(0) pos: vec3<f32>) -> WaterOut {
    var o: WaterOut;
    // Pass the vertex through: the Earthlike ocean quad is built at sea level and
    // each voxel-world liquid surface carries its own per-column height.
    o.world = pos;
    o.clip = cam.view_proj * vec4<f32>(pos, 1.0);
    return o;
}
// Animated surface normal, evaluated per fragment: large slow swells plus fine
// fast ripples from several directions (so it isn't a visible grid). `amp` scales
// the chop (calm methane vs choppy ocean). This is what makes the reflection
// shimmer per-pixel.
fn water_normal(p: vec2<f32>, t: f32, amp: f32) -> vec3<f32> {
    var d = vec2<f32>(0.0, 0.0);
    d += vec2<f32>(1.0, 0.0) * 0.30 * cos(p.x * 0.090 + t * 1.40);
    d += vec2<f32>(0.0, 1.0) * 0.30 * cos(p.y * 0.085 - t * 1.20);
    d += normalize(vec2<f32>(1.0, 0.7)) * 0.18 * cos(dot(p, vec2<f32>(0.050, 0.035)) + t * 0.90);
    d += normalize(vec2<f32>(-0.6, 1.0)) * 0.16 * cos(dot(p, vec2<f32>(-0.035, 0.050)) - t * 1.05);
    d += vec2<f32>(1.0, 0.0) * 0.08 * cos(p.x * 0.420 - t * 2.60);
    d += vec2<f32>(0.0, 1.0) * 0.08 * cos(p.y * 0.390 + t * 2.40);
    d += normalize(vec2<f32>(1.0, 1.0)) * 0.06 * cos(dot(p, vec2<f32>(0.330, 0.300)) + t * 3.10);
    d += normalize(vec2<f32>(-1.0, 0.4)) * 0.05 * cos(dot(p, vec2<f32>(0.21, -0.18)) - t * 3.7);
    return normalize(vec3<f32>(d.x * amp, 1.0, d.y * amp));
}

// Cheap procedural sky the water reflects: horizon→zenith gradient plus a sun
// disc and glow around the light direction.
fn water_sky(dir: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.y, 0.0, 1.0);
    let horizon = vec3<f32>(0.62, 0.72, 0.86);
    let zenith = vec3<f32>(0.17, 0.35, 0.62);
    var c = mix(horizon, zenith, pow(up, 0.55));
    let sun = max(dot(dir, normalize(cam.light_dir.xyz)), 0.0);
    c += vec3<f32>(1.00, 0.95, 0.80) * pow(sun, 250.0) * 1.4; // disc
    c += vec3<f32>(1.00, 0.90, 0.72) * pow(sun, 18.0) * 0.14; // glow
    return c;
}

@fragment
fn fs_water(in: WaterOut) -> @location(0) vec4<f32> {
    let t = cam.params.x;
    let amp = clamp(world.liquid.a, 0.25, 1.5);
    let n = water_normal(in.world.xz, t, amp);
    let view = normalize(cam.eye.xyz - in.world); // surface -> eye
    let refl = reflect(-view, n); // reflected view ray, into the sky
    let sky = water_sky(refl);

    // Per-world body colour (Earth water, Titan methane, ...): darker in the
    // troughs, lighter where the surface tilts toward us.
    let base = world.liquid.rgb;
    let body = mix(base * 0.45, base * 1.12, clamp(n.y, 0.0, 1.0));

    // Schlick fresnel: reflective at grazing angles, more transmissive looking
    // straight down.
    let fres = 0.02 + 0.98 * pow(1.0 - max(dot(n, view), 0.0), 5.0);
    var col = mix(body, sky, fres * 0.9);

    // Sun glint riding the ripples, plus a soft sub-surface glow.
    let spec = pow(max(dot(refl, normalize(cam.light_dir.xyz)), 0.0), 120.0);
    col += vec3<f32>(1.0, 0.96, 0.85) * spec * 0.7;
    col += base * 0.06;

    col = col * fow(in.world);
    let alpha = clamp(0.78 + fres * 0.2, 0.0, 0.98);
    return vec4<f32>(col, alpha);
}

// ---------------- units / buildings ----------------
// The instance tint's alpha selects a render mode:
//   < 1.25  normal (lit, team-tinted via the mesh's team weight)
//   1.25-2  selected building (brightened with a green lift)
//   2-3     hologram (the build-placement ghost: unlit, rolling scanlines)
//   >= 3    emissive (fx particles: pure tint color, no lighting)
struct UnitOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) albedo: vec3<f32>,
    @location(2) mode: f32,
    @location(3) world: vec3<f32>,
};
@vertex
fn vs_unit(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(5) mcol: vec4<f32>,
    @location(2) offset: vec3<f32>,
    @location(3) scale: vec3<f32>,
    @location(4) tcol: vec4<f32>,
    @location(6) rot: vec2<f32>,
    @location(7) anim: vec2<f32>,
) -> UnitOut {
    var o: UnitOut;
    // Procedural walk: geometry near the ground (the legs) swings fore-aft
    // along the mesh's facing axis (z), the left and right sides in
    // counter-phase, with a small lift on the stepping foot. anim = (phase,
    // amplitude); buildings and idle units pass amplitude 0.
    var ap = pos;
    if (anim.y > 0.0) {
        let side = select(3.14159, 0.0, pos.x >= 0.0);
        let foot = clamp(1.0 - pos.y / 1.3, 0.0, 1.0);
        let swing = sin(anim.x + side);
        ap.z = ap.z + swing * anim.y * foot;
        ap.y = ap.y + max(swing, 0.0) * anim.y * 0.45 * foot;
    }
    // Yaw the mesh (and its normal) by the instance facing: rot = (cos, sin).
    let sp = ap * scale;
    let rp = vec3<f32>(sp.x * rot.x + sp.z * rot.y, sp.y, -sp.x * rot.y + sp.z * rot.x);
    let rn = vec3<f32>(
        normal.x * rot.x + normal.z * rot.y,
        normal.y,
        -normal.x * rot.y + normal.z * rot.x,
    );
    o.normal = rn;
    let flat_tint = select(0.0, 1.0, tcol.a >= 2.0);
    let sel = select(0.0, 1.0, tcol.a >= 1.25 && tcol.a < 2.0);
    // mcol.a is the team-tint weight: blend the material toward the faction
    // color so banners/tabards/plumes read as team color, metal/skin stay neutral.
    var albedo = mix(mix(mcol.rgb, tcol.rgb, mcol.a), tcol.rgb, flat_tint);
    albedo = mix(albedo, albedo * 1.25 + vec3<f32>(0.05, 0.18, 0.07), sel);
    o.albedo = albedo;
    o.mode = tcol.a;
    let world = rp + offset;
    o.world = world;
    o.clip = cam.view_proj * vec4<f32>(world, 1.0);
    return o;
}
@fragment
fn fs_unit(in: UnitOut) -> @location(0) vec4<f32> {
    if in.mode >= 3.0 {
        // Emissive fx particle: pure color, no lighting. mode = 3 + life
        // fraction; a dying particle fades by screen-door transparency
        // (2x2 Bayer dither) at full brightness, never by darkening.
        let t = clamp(in.mode - 3.0, 0.0, 1.0);
        let px = vec2<u32>(in.clip.xy);
        let bayer = f32((px.x & 1u) + 2u * (px.y & 1u));
        if t * 4.0 < bayer + 0.5 {
            discard;
        }
        return vec4<f32>(in.albedo, 1.0);
    }
    if in.mode >= 2.0 {
        // Holographic build preview: unshaded scanlines plus screen-door
        // (dither) transparency. The cursor ghost (mode 2.0) keeps every
        // other pixel; a placed hologram (mode >= 2.25) is denser, so
        // placement reads as a solid commitment versus a tentative preview.
        let px = vec2<u32>(in.clip.xy);
        if in.mode >= 2.25 {
            if (px.x % 2u == 1u) && (px.y % 2u == 1u) {
                discard;
            }
        } else if (px.x + px.y) % 2u == 0u {
            discard;
        }
        let scan = 0.7 + 0.3 * sin(in.world.y * 5.0 - cam.params.x * 6.0);
        return vec4<f32>(in.albedo * (1.1 * scan), 1.0);
    }
    let n = normalize(in.normal);
    let ndl = max(dot(n, normalize(cam.light_dir.xyz)), 0.0);
    let lit = 0.45 + 0.7 * ndl;
    let col = in.albedo * (vec3<f32>(lit, lit, lit) + point_lights(in.world, n));
    return vec4<f32>(col, 1.0);
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
