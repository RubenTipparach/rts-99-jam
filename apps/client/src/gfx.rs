//! wgpu rendering: textured heightmap terrain, animated water, instanced
//! low-poly units & buildings (vertex-colored with team tint), selection rings,
//! and a fog-of-war texture. Presentation-only.

use crate::terrain;
use std::sync::Arc;
use winit::window::Window;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub offset: [f32; 3],
    pub scale: [f32; 3],
    pub color: [f32; 4],
    /// Yaw about +Y as `(cos, sin)`; `ROT_NONE` leaves the mesh unrotated.
    pub rot: [f32; 2],
    /// Procedural walk cycle as `(phase, amplitude)`: the vertex shader
    /// swings geometry near the ground (legs) along the facing axis, the two
    /// sides in counter-phase. `ANIM_NONE` for buildings and idle units.
    pub anim: [f32; 2],
}

/// Identity rotation for [`InstanceRaw::rot`].
pub const ROT_NONE: [f32; 2] = [1.0, 0.0];
/// No walk cycle for [`InstanceRaw::anim`].
pub const ANIM_NONE: [f32; 2] = [0.0, 0.0];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RingRaw {
    pub center: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
    /// Inner radius as a fraction of `radius`: [`RING`] for the standard
    /// selection annulus, 0.0 for a filled disc (blob contact shadows).
    pub inner: f32,
}

/// Standard annulus inner fraction for [`RingRaw::inner`].
pub const RING: f32 = 0.82;

/// One vertex of a selection-ring ground decal (tessellated each frame so it
/// follows the terrain height).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct RingVertex {
    pos: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex3 {
    pos: [f32; 3],
    normal: [f32; 3],
}

/// A unit/building mesh vertex. `color.rgb` is the material color; `color.a` is
/// the team-tint weight (0 = keep material, 1 = full faction color).
/// `detail` selects the surface-detail texture: [`DETAIL_FLAT`] for things
/// that must stay flat shaded (crystals, gas, glow), [`DETAIL_ROCK`] for
/// organic rock/dirt grain, [`DETAIL_PLATE`] for plate/masonry seams.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UnitVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    detail: f32,
}

/// No surface detail: flat shaded (crystals, gas pools, fx particles).
const DETAIL_FLAT: f32 = 0.0;
/// Organic grain (green channel of the detail map): rock, dirt, scree.
const DETAIL_ROCK: f32 = 1.0;
/// Plate/masonry seams (red channel): metal hulls and carved stone.
const DETAIL_PLATE: f32 = 2.0;

/// Tag every vertex with a detail-map selector (meshes default to plate).
fn set_detail(m: &mut [UnitVertex], d: f32) {
    for v in m {
        v.detail = d;
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    light_dir: [f32; 4],
    params: [f32; 4], // time, map_half, sea_level, point-light count
    /// Point lights for fx (muzzle flashes, mining sparks, explosions):
    /// xyz = world position, w = radius.
    light_pos: [[f32; 4]; MAX_LIGHTS],
    /// Point light colors (rgb; w unused).
    light_col: [[f32; 4]; MAX_LIGHTS],
}

/// A dynamic fx point light, consumed by [`Gfx::render`].
#[derive(Clone, Copy)]
pub struct FxLight {
    pub pos: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
}

/// Point-light slots in the camera uniform: transient fx lights plus the
/// steady world lights (per-corner building floodlights, node glow).
pub const MAX_LIGHTS: usize = 64;

/// One voxel-terrain vertex: position, normal, and soft texture blend weights
/// (four tile slots + a hazard channel) the shader triplanar-blends from.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VoxelVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    weights: [f32; 4],
    haz: f32,
}

/// Per-world appearance uniform (group 2): terrain tint + liquid body colour.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WorldUniform {
    tint: [f32; 4],   // rgb tint; a = lava emissive strength
    liquid: [f32; 4], // rgb liquid colour; a = waviness
}

/// Earthlike ocean colour + chop, used when no voxel map is selected.
const EARTH_WATER: [f32; 4] = [0.06, 0.22, 0.34, 1.0];

pub const MAX_INSTANCES: usize = 8192;
pub const MAX_RINGS: usize = 256;
/// Segments per selection-ring decal, and the resulting vertex-buffer capacity.
const RING_SEGMENTS: usize = 40;
const MAX_RING_VERTS: usize = MAX_RINGS * RING_SEGMENTS * 6;
pub const FOW_RES: usize = 256;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

// --- low-poly mesh building (vertex-colored, flat-shaded) ---------------------
//
// Meshes are composed from boxes and a gable roof. Face normals are computed and
// oriented outward from the primitive's center, so winding never matters (the
// unit pipeline doesn't cull) and lighting is always correct.

fn v_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn v_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn v_normalize(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    if l > 1e-6 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// One triangle, with an outward normal (flipped to point away from `center`).
fn push_tri(
    out: &mut Vec<UnitVertex>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    center: [f32; 3],
    color: [f32; 4],
) {
    let mut n = v_normalize(v_cross(v_sub(b, a), v_sub(c, a)));
    let mid = [
        (a[0] + b[0] + c[0]) / 3.0,
        (a[1] + b[1] + c[1]) / 3.0,
        (a[2] + b[2] + c[2]) / 3.0,
    ];
    let outward = v_sub(mid, center);
    if n[0] * outward[0] + n[1] * outward[1] + n[2] * outward[2] < 0.0 {
        n = [-n[0], -n[1], -n[2]];
    }
    for p in [a, b, c] {
        out.push(UnitVertex {
            pos: p,
            normal: n,
            color,
            detail: DETAIL_PLATE,
        });
    }
}

fn push_quad(
    out: &mut Vec<UnitVertex>,
    a: [f32; 3],
    b: [f32; 3],
    c: [f32; 3],
    d: [f32; 3],
    center: [f32; 3],
    color: [f32; 4],
) {
    push_tri(out, a, b, c, center, color);
    push_tri(out, a, c, d, center, color);
}

/// An axis-aligned box from `min` to `max`, with `team` as the tint weight.
fn push_box(out: &mut Vec<UnitVertex>, min: [f32; 3], max: [f32; 3], rgb: [f32; 3], team: f32) {
    let c = [
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    ];
    let col = [rgb[0], rgb[1], rgb[2], team];
    let (x0, y0, z0) = (min[0], min[1], min[2]);
    let (x1, y1, z1) = (max[0], max[1], max[2]);
    let p = |x: f32, y: f32, z: f32| [x, y, z];
    push_quad(
        out,
        p(x0, y0, z1),
        p(x1, y0, z1),
        p(x1, y1, z1),
        p(x0, y1, z1),
        c,
        col,
    ); // +z
    push_quad(
        out,
        p(x1, y0, z0),
        p(x0, y0, z0),
        p(x0, y1, z0),
        p(x1, y1, z0),
        c,
        col,
    ); // -z
    push_quad(
        out,
        p(x1, y0, z1),
        p(x1, y0, z0),
        p(x1, y1, z0),
        p(x1, y1, z1),
        c,
        col,
    ); // +x
    push_quad(
        out,
        p(x0, y0, z0),
        p(x0, y0, z1),
        p(x0, y1, z1),
        p(x0, y1, z0),
        c,
        col,
    ); // -x
    push_quad(
        out,
        p(x0, y1, z1),
        p(x1, y1, z1),
        p(x1, y1, z0),
        p(x0, y1, z0),
        c,
        col,
    ); // +y
    push_quad(
        out,
        p(x0, y0, z0),
        p(x1, y0, z0),
        p(x1, y0, z1),
        p(x0, y0, z1),
        c,
        col,
    ); // -y
}

/// A gable roof: ridge along x, eaves at `base_y`, peak at `base_y + peak_h`.
#[allow(clippy::too_many_arguments)]
fn push_roof(
    out: &mut Vec<UnitVertex>,
    cx: f32,
    cz: f32,
    hx: f32,
    hz: f32,
    base_y: f32,
    peak_h: f32,
    rgb: [f32; 3],
) {
    let peak = base_y + peak_h;
    let c = [cx, base_y + peak_h * 0.5, cz];
    let col = [rgb[0], rgb[1], rgb[2], 0.0];
    let ra = [cx - hx, peak, cz];
    let rb = [cx + hx, peak, cz];
    let fl = [cx - hx, base_y, cz + hz];
    let fr = [cx + hx, base_y, cz + hz];
    let bl = [cx - hx, base_y, cz - hz];
    let br = [cx + hx, base_y, cz - hz];
    push_quad(out, ra, rb, fr, fl, c, col); // front slope
    push_quad(out, ra, rb, br, bl, c, col); // back slope
    push_tri(out, ra, fl, bl, c, col); // gable -x
    push_tri(out, rb, fr, br, c, col); // gable +x
}

/// A horizontal ring of `n` points (a regular polygon) at height `y`.
fn poly_ring(n: usize, cx: f32, cz: f32, r: f32, y: f32, rot: f32) -> Vec<[f32; 3]> {
    (0..n)
        .map(|k| {
            let a = rot + std::f32::consts::TAU * k as f32 / n as f32;
            [cx + r * a.cos(), y, cz + r * a.sin()]
        })
        .collect()
}

/// A vertical n-gon prism (cylinder-ish) from `y0` to `y1`, optional top cap.
#[allow(clippy::too_many_arguments)]
fn push_prism(
    out: &mut Vec<UnitVertex>,
    cx: f32,
    cz: f32,
    r: f32,
    y0: f32,
    y1: f32,
    rgb: [f32; 3],
    team: f32,
    n: usize,
    rot: f32,
    top: bool,
) {
    let center = [cx, (y0 + y1) * 0.5, cz];
    let col = [rgb[0], rgb[1], rgb[2], team];
    let lo = poly_ring(n, cx, cz, r, y0, rot);
    let hi = poly_ring(n, cx, cz, r, y1, rot);
    for k in 0..n {
        let j = (k + 1) % n;
        push_quad(out, lo[k], lo[j], hi[j], hi[k], center, col);
    }
    if top {
        let cap = [cx, y1, cz];
        for k in 0..n {
            let j = (k + 1) % n;
            push_tri(out, hi[k], hi[j], cap, center, col);
        }
    }
}

/// A tapered n-gon frustum from radius `r0`@`y0` to `r1`@`y1`, optional top cap.
#[allow(clippy::too_many_arguments)]
fn push_frustum(
    out: &mut Vec<UnitVertex>,
    cx: f32,
    cz: f32,
    r0: f32,
    r1: f32,
    y0: f32,
    y1: f32,
    rgb: [f32; 3],
    team: f32,
    n: usize,
    rot: f32,
    top: bool,
) {
    let center = [cx, (y0 + y1) * 0.5, cz];
    let col = [rgb[0], rgb[1], rgb[2], team];
    let lo = poly_ring(n, cx, cz, r0, y0, rot);
    let hi = poly_ring(n, cx, cz, r1, y1, rot);
    for k in 0..n {
        let j = (k + 1) % n;
        push_quad(out, lo[k], lo[j], hi[j], hi[k], center, col);
    }
    if top {
        let cap = [cx, y1, cz];
        for k in 0..n {
            let j = (k + 1) % n;
            push_tri(out, hi[k], hi[j], cap, center, col);
        }
    }
}

/// An n-gon pyramid: base ring at `y0`, apex at `y1`.
#[allow(clippy::too_many_arguments)]
fn push_pyramid(
    out: &mut Vec<UnitVertex>,
    cx: f32,
    cz: f32,
    r: f32,
    y0: f32,
    y1: f32,
    rgb: [f32; 3],
    team: f32,
    n: usize,
    rot: f32,
) {
    let center = [cx, (y0 + y1) * 0.5, cz];
    let col = [rgb[0], rgb[1], rgb[2], team];
    let base = poly_ring(n, cx, cz, r, y0, rot);
    let apex = [cx, y1, cz];
    for k in 0..n {
        let j = (k + 1) % n;
        push_tri(out, base[k], base[j], apex, center, col);
    }
}

/// A horizontal n-gon tube along z (gun barrels, pipes, tool shafts), from
/// `z0` to `z1` around center `(cx, cy)`, with optional end caps.
#[allow(clippy::too_many_arguments)]
fn push_tube_z(
    out: &mut Vec<UnitVertex>,
    cx: f32,
    cy: f32,
    r: f32,
    z0: f32,
    z1: f32,
    rgb: [f32; 3],
    team: f32,
    n: usize,
    caps: bool,
) {
    let center = [cx, cy, (z0 + z1) * 0.5];
    let col = [rgb[0], rgb[1], rgb[2], team];
    let ring = |z: f32| -> Vec<[f32; 3]> {
        (0..n)
            .map(|k| {
                let a = std::f32::consts::TAU * k as f32 / n as f32;
                [cx + r * a.cos(), cy + r * a.sin(), z]
            })
            .collect()
    };
    let lo = ring(z0);
    let hi = ring(z1);
    for k in 0..n {
        let j = (k + 1) % n;
        push_quad(out, lo[k], lo[j], hi[j], hi[k], center, col);
    }
    if caps {
        for (cz, rim) in [(z0, &lo), (z1, &hi)] {
            let cap = [cx, cy, cz];
            for k in 0..n {
                let j = (k + 1) % n;
                push_tri(out, rim[k], rim[j], cap, center, col);
            }
        }
    }
}

/// A faceted dome (hemisphere cap) sitting on `y`, radius `r`.
#[allow(clippy::too_many_arguments)]
fn push_dome(
    out: &mut Vec<UnitVertex>,
    cx: f32,
    cz: f32,
    r: f32,
    y: f32,
    rgb: [f32; 3],
    team: f32,
    n: usize,
    stacks: usize,
) {
    let center = [cx, y, cz];
    let col = [rgb[0], rgb[1], rgb[2], team];
    let ring_at = |s: usize| -> (Vec<[f32; 3]>, f32) {
        let t = s as f32 / stacks as f32 * std::f32::consts::FRAC_PI_2;
        (poly_ring(n, cx, cz, r * t.cos(), y + r * t.sin(), 0.0), t)
    };
    for s in 0..stacks {
        let (lo, _) = ring_at(s);
        let (hi, ht) = ring_at(s + 1);
        if s + 1 == stacks && ht >= std::f32::consts::FRAC_PI_2 - 1e-4 {
            let apex = [cx, y + r, cz];
            for k in 0..n {
                let j = (k + 1) % n;
                push_tri(out, lo[k], lo[j], apex, center, col);
            }
        } else {
            for k in 0..n {
                let j = (k + 1) % n;
                push_quad(out, lo[k], lo[j], hi[j], hi[k], center, col);
            }
        }
    }
}

/// A faceted gem (n-gon bipyramid): waist ring at `ym`, tips at `y0` and `y1`.
#[allow(clippy::too_many_arguments)]
fn push_gem(
    out: &mut Vec<UnitVertex>,
    cx: f32,
    cz: f32,
    r: f32,
    y0: f32,
    ym: f32,
    y1: f32,
    rgb: [f32; 3],
    team: f32,
    n: usize,
) {
    push_pyramid(out, cx, cz, r, ym, y1, rgb, team, n, 0.0);
    push_pyramid(out, cx, cz, r, ym, y0, rgb, team, n, 0.0);
}

/// A low-poly infantry soldier, ~2.4 units tall, facing -z.
fn infantry_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let trousers = [0.20, 0.22, 0.27];
    let trousers_dk = [0.15, 0.17, 0.21];
    let leather = [0.45, 0.35, 0.27];
    let leather_dk = [0.33, 0.25, 0.19];
    let skin = [0.80, 0.62, 0.48];
    let metal = [0.54, 0.57, 0.64];
    let metal_dk = [0.40, 0.43, 0.50];
    let wood = [0.40, 0.27, 0.16];
    let team = [0.5, 0.5, 0.5];
    // Legs: thigh, shin, knee plate and boot per side; each stays on its own
    // side of x = 0 so the walk cycle swings them in counter-phase.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 0.19;
        push_box(
            &mut m,
            [x - 0.14, 0.40, -0.17],
            [x + 0.14, 0.96, 0.17],
            trousers,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.11, 0.10, -0.13],
            [x + 0.11, 0.46, 0.13],
            trousers_dk,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.10, 0.42, -0.21],
            [x + 0.10, 0.56, -0.13],
            metal_dk,
            0.0,
        );
        // Boot, toe toward -z, with a small heel block.
        push_box(
            &mut m,
            [x - 0.12, 0.0, -0.32],
            [x + 0.12, 0.13, 0.12],
            leather_dk,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.12, 0.13, -0.06],
            [x + 0.12, 0.24, 0.12],
            leather_dk,
            0.0,
        );
    }
    // Hip skirt + belt with a buckle.
    push_box(
        &mut m,
        [-0.36, 0.86, -0.26],
        [0.36, 1.02, 0.26],
        leather,
        0.0,
    );
    push_box(
        &mut m,
        [-0.38, 1.00, -0.28],
        [0.38, 1.10, 0.28],
        leather_dk,
        0.0,
    );
    push_box(
        &mut m,
        [-0.07, 0.99, -0.31],
        [0.07, 1.11, -0.27],
        metal,
        0.0,
    );
    // Torso: team tabard over a chest plate, collar, and a back plate.
    push_box(&mut m, [-0.40, 1.06, -0.28], [0.40, 1.66, 0.28], team, 1.0);
    push_box(
        &mut m,
        [-0.34, 1.26, -0.32],
        [0.34, 1.62, -0.26],
        metal,
        0.0,
    );
    push_box(
        &mut m,
        [-0.30, 1.62, -0.24],
        [0.30, 1.74, 0.24],
        metal_dk,
        0.0,
    );
    push_box(
        &mut m,
        [-0.34, 1.16, 0.26],
        [0.34, 1.60, 0.33],
        metal_dk,
        0.0,
    );
    // Tabard tail hanging below the belt on the front.
    push_box(&mut m, [-0.14, 0.62, -0.30], [0.14, 1.06, -0.24], team, 1.0);
    // Layered shoulder pauldrons (team rim over a steel cap) + arms with
    // bracers and bare hands.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 0.47;
        push_box(
            &mut m,
            [x - 0.15, 1.52, -0.22],
            [x + 0.15, 1.74, 0.22],
            team,
            1.0,
        );
        push_box(
            &mut m,
            [x - 0.12, 1.44, -0.18],
            [x + 0.12, 1.56, 0.18],
            metal,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.09, 1.12, -0.14],
            [x + 0.09, 1.48, 0.14],
            leather,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.10, 0.88, -0.15],
            [x + 0.10, 1.16, 0.15],
            leather_dk,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.08, 0.74, -0.12],
            [x + 0.08, 0.90, 0.12],
            skin,
            0.0,
        );
    }
    // Head, jaw, nose hint, and a crested helmet: dome + brim + cheek guards
    // + a team plume fin running front to back.
    push_box(&mut m, [-0.19, 1.74, -0.18], [0.19, 2.10, 0.18], skin, 0.0);
    push_box(&mut m, [-0.05, 1.88, -0.22], [0.05, 1.96, -0.17], skin, 0.0);
    push_box(&mut m, [-0.24, 2.02, -0.25], [0.24, 2.12, 0.25], metal, 0.0);
    push_dome(&mut m, 0.0, 0.0, 0.24, 2.12, metal, 0.0, 10, 3);
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 0.23 - 0.03, 1.80, -0.20],
            [sx * 0.23 + 0.03, 2.04, 0.10],
            metal,
            0.0,
        );
    }
    push_box(&mut m, [-0.04, 2.30, -0.26], [0.04, 2.52, 0.18], team, 1.0);
    push_box(&mut m, [-0.03, 2.20, -0.30], [0.03, 2.38, -0.18], team, 1.0);
    // Spear in the right hand: round shaft, socket, leaf-blade tip, butt cap,
    // and a small team pennant under the blade.
    push_prism(
        &mut m, 0.55, 0.0, 0.045, 0.10, 2.55, wood, 0.0, 6, 0.0, false,
    );
    push_prism(
        &mut m, 0.55, 0.0, 0.030, 0.02, 0.10, metal_dk, 0.0, 6, 0.0, false,
    );
    push_prism(
        &mut m, 0.55, 0.0, 0.060, 2.55, 2.66, metal_dk, 0.0, 6, 0.0, false,
    );
    push_gem(&mut m, 0.55, 0.0, 0.085, 2.66, 2.80, 3.06, metal, 0.0, 4);
    push_box(&mut m, [0.58, 2.30, -0.02], [0.60, 2.56, 0.30], team, 1.0);
    m
}

// Placeholder faction buildings (see assets/concepts/buildings.png and
// docs/building-design-language.md). Authored at world scale, ~11 wide, facing
// -z. The team-tint channel (a = 1.0) rides the faction's signature element: the
// Astromancer core/spire and the Hollowmen banner/window.

/// Astromancer production building: a grown, hovering faceted tower with a
/// team-tinted energy core and a gold-tipped spire. A short root hangs in the
/// air gap below so it reads as floating.
fn barracks_mesh_astro() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let shell = [0.86, 0.84, 0.76];
    let shell2 = [0.74, 0.72, 0.64];
    let shell3 = [0.64, 0.62, 0.55];
    let gold = [0.86, 0.75, 0.45];
    let gold_dk = [0.70, 0.58, 0.32];
    let aether = [0.55, 0.92, 0.86];
    let team = [0.5, 0.5, 0.5];
    let rot = std::f32::consts::FRAC_PI_8; // flat face toward -z
                                           // Hanging grown root, in two carved stages with a gold drip ring (does
                                           // not touch the ground -> visible hover gap).
    push_frustum(
        &mut m, 0.0, 0.0, 4.0, 2.2, 1.6, 0.9, shell2, 0.0, 8, rot, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 2.2, 0.4, 0.9, 0.45, shell3, 0.0, 8, rot, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 2.35, 0.86, 1.02, gold_dk, 0.0, 8, rot, false,
    );
    // Octagonal body with a skirt flare, a corbelled lip, and a gold belt
    // carved in two steps.
    push_frustum(
        &mut m, 0.0, 0.0, 4.7, 4.4, 1.6, 2.4, shell2, 0.0, 8, rot, false,
    );
    push_prism(&mut m, 0.0, 0.0, 4.4, 2.4, 5.0, shell, 0.0, 8, rot, false);
    push_prism(&mut m, 0.0, 0.0, 4.55, 3.0, 3.5, gold, 0.0, 8, rot, false);
    push_prism(
        &mut m, 0.0, 0.0, 4.48, 3.5, 3.66, gold_dk, 0.0, 8, rot, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 4.6, 4.84, 5.14, shell3, 0.0, 8, rot, false,
    );
    // Tall arched gate on the +z face, framed in gold, glowing aether door.
    push_box(&mut m, [-1.5, 1.6, 3.9], [1.5, 4.6, 4.5], gold_dk, 0.0);
    push_box(&mut m, [-1.15, 1.6, 4.3], [1.15, 4.25, 4.56], aether, 0.0);
    push_gem(&mut m, 0.0, 4.42, 0.5, 4.4, 4.75, 5.2, gold, 0.0, 4);
    // Tapering shoulder with carved vertical ribs and aether window slits.
    push_frustum(
        &mut m, 0.0, 0.0, 4.4, 2.8, 5.14, 7.6, shell2, 0.0, 8, rot, false,
    );
    for k in 0..8 {
        let a = rot + std::f32::consts::TAU * (k as f32 + 0.5) / 8.0;
        let (rx, rz) = (a.cos(), a.sin());
        push_box(
            &mut m,
            [rx * 4.0 - 0.22, 5.3, rz * 4.0 - 0.22],
            [rx * 4.0 + 0.22, 6.9, rz * 4.0 + 0.22],
            shell3,
            0.0,
        );
        if k % 2 == 0 {
            push_box(
                &mut m,
                [rx * 3.62 - 0.16, 5.6, rz * 3.62 - 0.16],
                [rx * 3.62 + 0.16, 6.5, rz * 3.62 + 0.16],
                aether,
                0.0,
            );
        }
    }
    // Gold crown ring, crowning spire, and the team-tinted energy core
    // running up the middle, capped by a floating gem.
    push_prism(&mut m, 0.0, 0.0, 3.0, 7.6, 8.1, gold_dk, 0.0, 8, rot, false);
    push_pyramid(&mut m, 0.0, 0.0, 2.8, 8.1, 11.6, gold, 0.0, 8, rot);
    push_prism(&mut m, 0.0, 0.0, 1.2, 2.2, 8.2, team, 1.0, 8, rot, true);
    push_gem(&mut m, 0.0, 0.0, 0.55, 11.5, 11.95, 12.7, team, 1.0, 6);
    // A ring of three hovering shards orbiting the door side, each a carved
    // stone with a team-tinted tip.
    for (sx, sz, y0) in [(-3.6_f32, 3.4, 1.4), (3.6, 3.4, 1.8), (0.0, -5.0, 2.2)] {
        push_gem(&mut m, sx, sz, 0.5, y0 - 0.7, y0, y0 + 1.0, shell, 0.0, 6);
        push_pyramid(
            &mut m,
            sx,
            sz,
            0.32,
            y0 + 1.05,
            y0 + 1.75,
            team,
            1.0,
            6,
            0.0,
        );
    }
    m
}

/// Hollowmen production building: a grounded, armored hangar with a sawtooth
/// roof, a blast door, a smokestack, a roof turret (the built-in gun), a
/// team-tinted window band, and a hazard skirt.
fn barracks_mesh_hollow() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let steel = [0.46, 0.49, 0.53];
    let steel2 = [0.58, 0.61, 0.65];
    let steel3 = [0.38, 0.41, 0.45];
    let dark = [0.24, 0.26, 0.30];
    let haz = [0.80, 0.58, 0.20];
    let gun = [0.17, 0.19, 0.22];
    let team = [0.5, 0.5, 0.5];
    let amber = [0.95, 0.75, 0.35];
    // Foundation slab with a chamfered curb + plated body.
    push_box(&mut m, [-5.6, 0.0, -4.6], [5.6, 0.5, 4.6], dark, 0.0);
    push_box(&mut m, [-5.4, 0.5, -4.4], [5.4, 0.62, 4.4], steel3, 0.0);
    push_box(&mut m, [-5.2, 0.5, -4.2], [5.2, 4.0, 4.2], steel, 0.0);
    // Hazard skirt striped into chevron segments.
    for k in 0..8 {
        let x0 = -5.24 + k as f32 * 1.31;
        let c = if k % 2 == 0 { haz } else { dark };
        push_box(&mut m, [x0, 0.62, -4.24], [x0 + 1.31, 1.05, 4.24], c, 0.0);
    }
    // Riveted wall ribs along both long faces + corner armor plates.
    for sx in [-1.0_f32, 1.0] {
        for cz in [-3.0_f32, -1.0, 1.0, 3.0] {
            push_box(
                &mut m,
                [sx * 5.2 - 0.15, 1.05, cz - 0.25],
                [sx * 5.2 + 0.15, 3.9, cz + 0.25],
                steel3,
                0.0,
            );
        }
        push_box(
            &mut m,
            [sx * 4.9 - 0.45, 0.62, -4.45],
            [sx * 4.9 + 0.45, 4.0, -3.7],
            steel2,
            0.0,
        );
        push_box(
            &mut m,
            [sx * 4.9 - 0.45, 0.62, 3.7],
            [sx * 4.9 + 0.45, 4.0, 4.45],
            steel2,
            0.0,
        );
    }
    // Sawtooth roof (three gables along x) with skylight strips and a
    // raised ridge vent on each gable.
    for cx in [-3.4_f32, 0.0, 3.4] {
        push_roof(&mut m, cx, 0.0, 1.7, 4.4, 4.0, 1.4, steel2);
        push_box(
            &mut m,
            [cx - 1.2, 5.28, -2.6],
            [cx + 1.2, 5.5, 2.6],
            dark,
            0.0,
        );
        push_box(
            &mut m,
            [cx - 0.9, 4.62, -3.4],
            [cx - 0.4, 5.0, 3.4],
            amber,
            0.0,
        );
    }
    // Blast door on the +z face: framed, segmented into armored slats, with
    // hazard jambs and warning lights above.
    push_box(&mut m, [-1.9, 0.0, 4.1], [1.9, 3.1, 4.3], steel3, 0.0);
    for k in 0..4 {
        let y0 = 0.18 + k as f32 * 0.68;
        push_box(&mut m, [-1.55, y0, 4.22], [1.55, y0 + 0.56, 4.42], gun, 0.0);
    }
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 2.1 - 0.18, 0.0, 4.2],
            [sx * 2.1 + 0.18, 3.1, 4.4],
            haz,
            0.0,
        );
    }
    push_box(&mut m, [-0.5, 3.2, 4.2], [0.5, 3.5, 4.4], amber, 0.0);
    // Team-tinted window bands, mullioned into panes.
    for sx in [-1.0_f32, 1.0] {
        for k in 0..3 {
            let x0 = sx * 2.2 + sx.signum() * k as f32 * 0.84;
            let (a, b) = if sx < 0.0 {
                (x0 - 0.6, x0)
            } else {
                (x0, x0 + 0.6)
            };
            push_box(&mut m, [a, 2.4, 4.2], [b, 3.2, 4.34], team, 1.0);
        }
    }
    // Smokestack: banded, with a hazard collar, a dark cap, and bracing pipe
    // down to the roof.
    push_prism(
        &mut m, -4.0, -2.8, 0.7, 4.0, 7.2, steel2, 0.0, 10, 0.0, true,
    );
    push_prism(&mut m, -4.0, -2.8, 0.76, 5.2, 5.6, haz, 0.0, 10, 0.0, false);
    push_prism(
        &mut m, -4.0, -2.8, 0.78, 6.7, 7.25, dark, 0.0, 10, 0.0, true,
    );
    push_tube_z(&mut m, -4.0, 4.6, 0.16, -2.7, -0.5, steel3, 0.0, 6, true);
    // Roof turret: ring base, armored housing, twin recoil-sleeved barrels
    // (the built-in gun) and a sensor stub.
    push_prism(
        &mut m, 3.2, -0.2, 1.05, 5.3, 5.62, steel3, 0.0, 8, 0.0, false,
    );
    push_box(&mut m, [2.4, 5.5, -1.0], [4.0, 6.5, 0.6], gun, 0.0);
    for bx in [2.75_f32, 3.65] {
        push_tube_z(&mut m, bx, 6.1, 0.14, 0.5, 3.2, gun, 0.0, 6, true);
        push_tube_z(&mut m, bx, 6.1, 0.20, 1.0, 1.7, steel3, 0.0, 6, false);
    }
    push_box(&mut m, [3.0, 6.5, -0.5], [3.4, 6.8, -0.1], amber, 0.0);
    m
}

/// Astromancer Spire (HQ): a tapered faceted tower on a hanging grown root,
/// crowned by a gold spire over a team-tinted core, with small shards orbiting
/// the base. The tallest structure in the colony (see
/// docs/building-design-language.md).
fn hq_mesh_astro() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let shell = [0.86, 0.84, 0.76];
    let shell2 = [0.74, 0.72, 0.64];
    let shell3 = [0.64, 0.62, 0.55];
    let gold = [0.86, 0.75, 0.45];
    let gold_dk = [0.70, 0.58, 0.32];
    let aether = [0.55, 0.92, 0.86];
    let team = [0.5, 0.5, 0.5];
    let rot = std::f32::consts::FRAC_PI_8; // flat face toward -z
                                           // Hanging grown root in two carved stages (visible hover gap), with a
                                           // gold drip ring where it meets the base.
    push_frustum(
        &mut m, 0.0, 0.0, 5.0, 2.6, 2.0, 1.0, shell2, 0.0, 8, rot, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 2.6, 0.6, 1.0, 0.5, shell3, 0.0, 8, rot, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 2.75, 0.95, 1.12, gold_dk, 0.0, 8, rot, false,
    );
    // Broad grown base with a corbelled rim and a stepped gold ceremonial
    // belt.
    push_frustum(
        &mut m, 0.0, 0.0, 6.2, 4.6, 0.6, 3.6, shell, 0.0, 8, rot, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 6.35, 1.3, 1.75, shell3, 0.0, 8, rot, false,
    );
    push_prism(&mut m, 0.0, 0.0, 4.7, 3.6, 4.4, gold, 0.0, 8, rot, false);
    push_prism(
        &mut m, 0.0, 0.0, 4.62, 4.4, 4.58, gold_dk, 0.0, 8, rot, false,
    );
    // Tall arched sanctum gate on the +z face of the base, framed in gold,
    // glowing aether door.
    push_box(&mut m, [-1.4, 0.6, 4.3], [1.4, 3.0, 5.3], gold_dk, 0.0);
    push_box(&mut m, [-1.05, 0.6, 4.4], [1.05, 2.7, 5.36], aether, 0.0);
    push_gem(&mut m, 0.0, 4.8, 0.45, 2.9, 3.2, 3.6, gold, 0.0, 4);
    // Tapering faceted tower, two stages, with carved vertical ribs and
    // aether window slits on the lower stage.
    push_frustum(
        &mut m, 0.0, 0.0, 4.3, 2.9, 4.58, 9.6, shell2, 0.0, 8, rot, false,
    );
    for k in 0..8 {
        let a = rot + std::f32::consts::TAU * (k as f32 + 0.5) / 8.0;
        let (rx, rz) = (a.cos(), a.sin());
        push_box(
            &mut m,
            [rx * 3.85 - 0.22, 4.9, rz * 3.85 - 0.22],
            [rx * 3.85 + 0.22, 7.6, rz * 3.85 + 0.22],
            shell3,
            0.0,
        );
        if k % 2 == 0 {
            push_box(
                &mut m,
                [rx * 3.5 - 0.16, 5.4, rz * 3.5 - 0.16],
                [rx * 3.5 + 0.16, 7.0, rz * 3.5 + 0.16],
                aether,
                0.0,
            );
        }
    }
    push_prism(
        &mut m, 0.0, 0.0, 3.05, 9.6, 9.95, gold_dk, 0.0, 8, rot, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 2.9, 1.9, 9.95, 13.2, shell, 0.0, 8, rot, false,
    );
    // Gold crown ring + crowning spire, with a floating team gem above the
    // tip and four small gold finials around the crown.
    push_prism(&mut m, 0.0, 0.0, 2.2, 13.2, 14.0, gold, 0.0, 8, rot, false);
    push_pyramid(&mut m, 0.0, 0.0, 1.6, 14.0, 17.4, gold, 0.0, 8, rot);
    push_gem(&mut m, 0.0, 0.0, 0.55, 17.6, 18.05, 18.8, team, 1.0, 6);
    for k in 0..4 {
        let a = rot + std::f32::consts::TAU * k as f32 / 4.0;
        let (fx, fz) = (a.cos() * 2.05, a.sin() * 2.05);
        push_pyramid(&mut m, fx, fz, 0.28, 14.0, 15.1, gold_dk, 0.0, 4, a);
    }
    // Team-tinted energy core running up the middle of the tower.
    push_prism(&mut m, 0.0, 0.0, 1.0, 1.6, 14.8, team, 1.0, 8, rot, true);
    // Orbiting shards around the base at uneven heights: carved gems with
    // team-tinted tips and a small gold collar.
    for (sx, sz, y0) in [(-4.8_f32, 2.6, 2.6), (5.0, 1.6, 3.4), (0.8, -5.4, 2.1)] {
        push_gem(&mut m, sx, sz, 0.6, y0 - 0.9, y0, y0 + 1.3, shell, 0.0, 6);
        push_prism(
            &mut m,
            sx,
            sz,
            0.42,
            y0 + 0.42,
            y0 + 0.62,
            gold_dk,
            0.0,
            6,
            0.0,
            false,
        );
        push_pyramid(&mut m, sx, sz, 0.4, y0 + 1.45, y0 + 2.3, team, 1.0, 6, 0.0);
    }
    m
}

/// Hollowmen Command HQ: a broad armored block with angled corner armor, a
/// raised control tower with a team-tinted window band, a roof turret (the
/// built-in gun), an antenna mast, a hazard skirt, and a blast door (see
/// docs/building-design-language.md).
fn hq_mesh_hollow() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let steel = [0.46, 0.49, 0.53];
    let steel2 = [0.58, 0.61, 0.65];
    let steel3 = [0.38, 0.41, 0.45];
    let dark = [0.24, 0.26, 0.30];
    let haz = [0.80, 0.58, 0.20];
    let gun = [0.17, 0.19, 0.22];
    let team = [0.5, 0.5, 0.5];
    let amber = [0.95, 0.75, 0.35];
    // Foundation slab with a chamfered curb + broad armored body.
    push_box(&mut m, [-6.4, 0.0, -5.4], [6.4, 0.5, 5.4], dark, 0.0);
    push_box(&mut m, [-6.2, 0.5, -5.2], [6.2, 0.64, 5.2], steel3, 0.0);
    push_box(&mut m, [-6.0, 0.5, -5.0], [6.0, 4.6, 5.0], steel, 0.0);
    // Hazard skirt striped into chevron segments on both long faces.
    for k in 0..8 {
        let x0 = -6.04 + k as f32 * 1.51;
        let c = if k % 2 == 0 { haz } else { dark };
        push_box(&mut m, [x0, 0.64, -5.04], [x0 + 1.51, 1.1, 5.04], c, 0.0);
    }
    // Wall plating: riveted ribs along the long faces + louvered intake
    // vents near the -z face.
    for sx in [-1.0_f32, 1.0] {
        for cz in [-3.2_f32, -1.1, 1.1, 3.2] {
            push_box(
                &mut m,
                [sx * 6.0 - 0.15, 1.1, cz - 0.28],
                [sx * 6.0 + 0.15, 4.45, cz + 0.28],
                steel3,
                0.0,
            );
        }
    }
    for k in 0..3 {
        let y0 = 2.2 + k as f32 * 0.62;
        push_box(
            &mut m,
            [-3.4, y0, -5.12],
            [-0.8, y0 + 0.34, -4.94],
            dark,
            0.0,
        );
    }
    // Angled corner armor (frustum wedges) with hazard caps.
    for (sx, sz) in [(-4.9_f32, -3.9_f32), (4.9, -3.9), (-4.9, 3.9), (4.9, 3.9)] {
        push_frustum(
            &mut m,
            sx,
            sz,
            1.7,
            1.1,
            0.5,
            5.0,
            steel2,
            0.0,
            4,
            std::f32::consts::FRAC_PI_4,
            true,
        );
        push_frustum(
            &mut m,
            sx,
            sz,
            1.45,
            1.25,
            2.4,
            3.0,
            haz,
            0.0,
            4,
            std::f32::consts::FRAC_PI_4,
            false,
        );
    }
    // Deck rim + raised control tower: a chamfered plinth, the cab with a
    // mullioned team-tinted window band all around, and a sensor roof.
    push_box(&mut m, [-3.0, 4.6, -2.6], [3.0, 5.0, 2.6], steel3, 0.0);
    push_box(&mut m, [-2.6, 5.0, -2.2], [2.6, 7.6, 2.2], steel2, 0.0);
    push_box(&mut m, [-2.7, 6.2, -2.3], [2.7, 7.0, 2.3], team, 1.0);
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 2.72 - 0.06, 6.2, -0.14],
            [sx * 2.72 + 0.06, 7.0, 0.14],
            dark,
            0.0,
        );
        push_box(
            &mut m,
            [-0.14, 6.2, sx * 2.32 - 0.06],
            [0.14, 7.0, sx * 2.32 + 0.06],
            dark,
            0.0,
        );
    }
    push_box(&mut m, [-2.8, 7.6, -2.4], [2.8, 8.0, 2.4], dark, 0.0);
    push_box(&mut m, [-1.2, 8.0, -1.0], [1.2, 8.5, 1.0], steel3, 0.0);
    push_box(&mut m, [-0.3, 8.5, -0.3], [0.3, 8.8, 0.3], amber, 0.0);
    // Roof turret on the deck: ring base, armored housing, twin
    // recoil-sleeved barrels (the built-in gun).
    push_prism(
        &mut m, 4.1, -0.5, 1.1, 4.6, 4.95, steel3, 0.0, 8, 0.0, false,
    );
    push_box(&mut m, [3.2, 4.9, -1.4], [5.0, 6.0, 0.4], gun, 0.0);
    for bx in [3.6_f32, 4.6] {
        push_tube_z(&mut m, bx, 5.55, 0.15, 0.3, 3.6, gun, 0.0, 6, true);
        push_tube_z(&mut m, bx, 5.55, 0.22, 0.9, 1.7, steel3, 0.0, 6, false);
    }
    // Antenna mast: latticed in three stages with a dish and a beacon.
    push_prism(&mut m, -4.4, -3.2, 0.30, 4.6, 6.6, dark, 0.0, 6, 0.0, false);
    push_prism(&mut m, -4.4, -3.2, 0.20, 6.6, 8.6, dark, 0.0, 6, 0.0, false);
    push_prism(&mut m, -4.4, -3.2, 0.11, 8.6, 10.4, dark, 0.0, 6, 0.0, true);
    push_prism(
        &mut m, -4.4, -3.2, 0.45, 6.5, 6.72, steel3, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, -4.4, -3.2, 0.95, 0.35, 9.0, 9.55, steel2, 0.0, 8, 0.0, false,
    );
    push_box(
        &mut m,
        [-4.55, 10.4, -3.35],
        [-4.25, 10.7, -3.05],
        amber,
        0.0,
    );
    // Blast door on the +z face: framed, segmented into armored slats, with
    // hazard jambs and a warning light.
    push_box(&mut m, [-2.3, 0.0, 4.9], [2.3, 3.4, 5.1], steel3, 0.0);
    for k in 0..4 {
        let y0 = 0.2 + k as f32 * 0.74;
        push_box(&mut m, [-1.9, y0, 5.02], [1.9, y0 + 0.6, 5.22], gun, 0.0);
    }
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 2.5 - 0.2, 0.0, 5.0],
            [sx * 2.5 + 0.2, 3.4, 5.2],
            haz,
            0.0,
        );
    }
    push_box(&mut m, [-0.5, 3.5, 5.0], [0.5, 3.8, 5.18], amber, 0.0);
    m
}

// Placeholder faction workers (see assets/concepts/units_resources.png). ~2.7
// tall, facing -z. Team tint rides the Astromancer focus-core and the Hollowmen
// visor/shoulder. The Acolyte is authored to sit just above y=0 and is lifted
// into a hover by the instance offset.

/// Astromancer Acolyte: a grown automaton held together by magic instead of
/// joints - the head, torso, pelvis pod, and bare forearms all hover with
/// visible gaps between them (marionette-style), around a team-tinted core.
/// Faces +z (eyes), like the Engineer's visor.
fn acolyte_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let shell = [0.86, 0.84, 0.76];
    let shell2 = [0.78, 0.76, 0.68];
    let shell3 = [0.66, 0.64, 0.57];
    let gold = [0.86, 0.75, 0.45];
    let gold_dk = [0.70, 0.58, 0.32];
    let eyes = [0.55, 0.92, 0.86];
    let team = [0.5, 0.5, 0.5];
    // Pelvis pod: a carved keel in two stages with a gold tail ring, lowest
    // floating segment.
    push_frustum(
        &mut m, 0.0, 0.0, 0.30, 0.20, 0.95, 0.62, shell2, 0.0, 8, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 0.20, 0.10, 0.62, 0.40, shell3, 0.0, 8, 0.0, true,
    );
    push_prism(
        &mut m, 0.0, 0.0, 0.22, 0.66, 0.74, gold_dk, 0.0, 8, 0.0, false,
    );
    // Team-tinted core gem, exposed in the gap between pelvis and chest.
    push_gem(&mut m, 0.0, 0.0, 0.15, 1.00, 1.16, 1.32, team, 1.0, 6);
    // Chest shell: a faceted barrel with a carved waist seam, a gold collar
    // plate, and a small breastplate boss; clear gap below.
    push_frustum(
        &mut m, 0.0, 0.0, 0.26, 0.37, 1.38, 1.78, shell, 0.0, 8, 0.0, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 0.375, 1.74, 1.82, shell3, 0.0, 8, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 0.37, 0.26, 1.82, 2.08, shell, 0.0, 8, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 0.27, 2.08, 2.17, gold, 0.0, 8, 0.0, true);
    push_gem(&mut m, 0.0, 0.36, 0.07, 1.62, 1.72, 1.82, gold, 0.0, 4);
    // Back fin: a carved dorsal blade floating just behind the chest.
    push_box(
        &mut m,
        [-0.04, 1.55, -0.52],
        [0.04, 2.05, -0.40],
        shell2,
        0.0,
    );
    push_box(
        &mut m,
        [-0.03, 1.86, -0.62],
        [0.03, 2.28, -0.50],
        gold_dk,
        0.0,
    );
    // Head: a separate capsule floating above the collar (visible neck gap),
    // glowing eye band on the +z face under a gold brow ridge.
    push_frustum(
        &mut m, 0.0, 0.0, 0.16, 0.20, 2.34, 2.56, shell, 0.0, 8, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 0.20, 0.11, 2.56, 2.76, shell2, 0.0, 8, 0.0, true,
    );
    push_box(&mut m, [-0.13, 2.41, 0.16], [0.13, 2.51, 0.24], eyes, 0.0);
    push_box(
        &mut m,
        [-0.15, 2.51, 0.14],
        [0.15, 2.57, 0.23],
        gold_dk,
        0.0,
    );
    // A thin gold halo ring floating above the crown.
    push_prism(
        &mut m, 0.0, 0.0, 0.24, 2.86, 2.92, gold, 0.0, 10, 0.0, false,
    );
    // Floating shoulder pods and bare forearms: no upper arms at all, the
    // "joints" are just gaps held by magic. Each pod is a carved cone with a
    // gold cap; each forearm is faceted with a gold cuff and a tiny hand tip.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 0.56;
        push_frustum(
            &mut m, x, 0.0, 0.13, 0.07, 2.12, 1.90, shell2, 0.0, 8, 0.0, true,
        );
        push_prism(&mut m, x, 0.0, 0.10, 2.12, 2.18, gold, 0.0, 8, 0.0, true);
        push_box(
            &mut m,
            [x - 0.10, 1.30, -0.10],
            [x + 0.10, 1.42, 0.10],
            gold,
            0.0,
        );
        push_frustum(
            &mut m,
            sx * 0.58,
            0.0,
            0.09,
            0.07,
            1.28,
            0.96,
            shell,
            0.0,
            8,
            0.0,
            false,
        );
        push_gem(
            &mut m,
            sx * 0.58,
            0.0,
            0.06,
            0.78,
            0.88,
            0.96,
            shell3,
            0.0,
            6,
        );
    }
    // Two tiny rune shards orbiting the waist gap.
    push_gem(&mut m, 0.42, 0.30, 0.05, 1.12, 1.20, 1.28, gold, 0.0, 4);
    push_gem(&mut m, -0.44, -0.24, 0.05, 1.22, 1.30, 1.38, gold, 0.0, 4);
    m
}

/// Hollowmen Engineer: a stocky powered-armor worker with a team-tinted visor
/// and shoulder, a hazard chest band, and a carried tool; walks, welds, drills.
fn engineer_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let steel = [0.48, 0.51, 0.55];
    let steel2 = [0.60, 0.63, 0.67];
    let steel3 = [0.38, 0.41, 0.45];
    let dark = [0.24, 0.26, 0.30];
    let haz = [0.80, 0.58, 0.20];
    let amber = [0.95, 0.75, 0.35];
    let gun = [0.17, 0.19, 0.22];
    let team = [0.5, 0.5, 0.5];
    // Legs: armored thigh, shin guard, knee cap, and a heavy toe-capped boot
    // per side (offset stance, one foot a little forward).
    for (sx, fz) in [(-1.0_f32, -0.11_f32), (1.0, 0.11)] {
        let x = sx * 0.20;
        push_box(
            &mut m,
            [x - 0.14, 0.30, fz - 0.16],
            [x + 0.14, 0.70, fz + 0.16],
            dark,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.12, 0.08, fz - 0.13],
            [x + 0.12, 0.34, fz + 0.13],
            steel3,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.10, 0.30, fz + 0.12],
            [x + 0.10, 0.44, fz + 0.22],
            steel2,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.13, 0.0, fz - 0.16],
            [x + 0.13, 0.12, fz + 0.30],
            gun,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.11, 0.0, fz + 0.28],
            [x + 0.11, 0.09, fz + 0.36],
            haz,
            0.0,
        );
    }
    // Hip girdle + utility belt with side pouches.
    push_box(
        &mut m,
        [-0.36, 0.62, -0.26],
        [0.36, 0.80, 0.26],
        steel3,
        0.0,
    );
    push_box(&mut m, [-0.38, 0.76, -0.28], [0.38, 0.88, 0.28], dark, 0.0);
    push_box(&mut m, [-0.46, 0.66, -0.10], [-0.36, 0.86, 0.10], gun, 0.0);
    push_box(&mut m, [0.36, 0.66, -0.10], [0.46, 0.86, 0.10], gun, 0.0);
    // Torso: chest plate over a darker under-suit, hazard chest band, and a
    // small status lamp on the left pectoral.
    push_box(
        &mut m,
        [-0.40, 0.84, -0.30],
        [0.40, 1.18, 0.30],
        steel3,
        0.0,
    );
    push_box(&mut m, [-0.44, 1.12, -0.34], [0.44, 1.50, 0.34], steel, 0.0);
    push_box(&mut m, [-0.44, 1.00, -0.34], [0.44, 1.18, 0.36], haz, 0.0);
    push_box(&mut m, [-0.30, 1.30, 0.32], [-0.16, 1.42, 0.38], amber, 0.0);
    // Backpack: twin gas tanks, a frame, and a glowing vent between them.
    push_box(&mut m, [-0.34, 0.80, -0.50], [0.34, 1.46, -0.34], dark, 0.0);
    for sx in [-1.0_f32, 1.0] {
        push_prism(
            &mut m,
            sx * 0.18,
            -0.56,
            0.11,
            0.84,
            1.42,
            steel2,
            0.0,
            8,
            0.0,
            false,
        );
        push_dome(&mut m, sx * 0.18, -0.56, 0.11, 1.42, steel3, 0.0, 8, 2);
    }
    push_box(
        &mut m,
        [-0.06, 1.00, -0.62],
        [0.06, 1.36, -0.52],
        amber,
        0.0,
    );
    // Head: helmet with a team-tinted visor, a chin guard, a crown plate,
    // and a stub antenna with a tip light.
    push_box(
        &mut m,
        [-0.26, 1.50, -0.24],
        [0.26, 1.98, 0.24],
        steel2,
        0.0,
    );
    push_box(&mut m, [-0.26, 1.66, 0.22], [0.26, 1.84, 0.30], team, 1.0);
    push_box(&mut m, [-0.20, 1.52, 0.22], [0.20, 1.62, 0.28], dark, 0.0);
    push_box(&mut m, [-0.30, 1.96, -0.26], [0.30, 2.06, 0.26], dark, 0.0);
    push_prism(
        &mut m, 0.24, -0.18, 0.025, 2.06, 2.34, gun, 0.0, 6, 0.0, false,
    );
    push_box(&mut m, [0.21, 2.34, -0.21], [0.27, 2.40, -0.15], amber, 0.0);
    // Arms: shoulder actuators, segmented forearms, a team pad on the left
    // shoulder, and the powered drill tool in the right hand.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 0.53;
        push_box(
            &mut m,
            [x - 0.10, 1.36, -0.14],
            [x + 0.10, 1.54, 0.14],
            steel3,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.09, 1.04, -0.14],
            [x + 0.09, 1.40, 0.14],
            steel,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.10, 0.78, -0.15],
            [x + 0.10, 1.08, 0.15],
            steel3,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.08, 0.66, -0.12],
            [x + 0.08, 0.80, 0.12],
            gun,
            0.0,
        );
    }
    push_box(&mut m, [-0.66, 1.40, -0.18], [-0.42, 1.58, 0.18], team, 1.0);
    // The drill, pointing +z like the visor: body, hazard grip collar,
    // tapered chuck, and the bit.
    push_tube_z(&mut m, 0.53, 0.74, 0.10, -0.06, 0.46, gun, 0.0, 8, true);
    push_tube_z(&mut m, 0.53, 0.74, 0.12, -0.02, 0.14, haz, 0.0, 8, false);
    push_tube_z(&mut m, 0.53, 0.74, 0.06, 0.44, 0.66, steel2, 0.0, 8, true);
    push_box(&mut m, [0.50, 0.71, 0.62], [0.56, 0.77, 0.82], steel3, 0.0);
    m
}

// Neutral resource nodes (see assets/concepts/units_resources.png). Never team
// tinted. Authored at world scale; ~6 units across so they read as map features.

/// Ore: a cluster of bright, faceted crystals erupting from a dark rock base.
/// "Shininess" is faked with near-white tips and bright inner cores (the
/// flat-shaded pipeline has no real translucency).
/// Ore node, opaque part: just the rock pedestal. The crystal shards render
/// separately through the blended crystal pipeline (`ore_crystal_mesh`).
fn ore_node_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let rock = [0.28, 0.32, 0.38];
    let rock_dk = [0.17, 0.20, 0.25];
    let rock_lt = [0.36, 0.41, 0.48];
    // Two-stage pedestal with an uneven scree of boulders around the rim.
    push_frustum(
        &mut m, 0.0, 0.0, 3.4, 2.6, 0.0, 1.0, rock_dk, 0.0, 9, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 2.6, 2.1, 1.0, 1.35, rock, 0.0, 9, 0.2, true,
    );
    push_prism(&mut m, 0.0, 0.0, 2.9, 0.0, 0.45, rock, 0.0, 9, 0.0, true);
    for (k, &(bx, bz)) in [
        (2.9_f32, 0.9_f32),
        (1.1, -2.8),
        (-2.5, -1.7),
        (-2.9, 1.3),
        (-0.4, 3.0),
        (2.0, 2.3),
    ]
    .iter()
    .enumerate()
    {
        let r = 0.5 + 0.13 * (k % 3) as f32;
        let c = if k % 2 == 0 { rock_lt } else { rock };
        push_gem(
            &mut m,
            bx,
            bz,
            r,
            0.0,
            0.35 + 0.1 * (k % 2) as f32,
            0.8 + 0.15 * (k % 3) as f32,
            c,
            0.0,
            5,
        );
    }
    set_detail(&mut m, DETAIL_ROCK);
    m
}

/// The ore crystal shards: drawn alpha-blended with env reflections
/// (Warcraft 3 style), riding the same instance transform as the rock base.
/// Also reused tiny (scaled down) as the load a hauling worker carries.
fn ore_crystal_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let body = [0.47, 0.84, 0.92];
    let body2 = [0.36, 0.74, 0.86];
    let core = [0.82, 0.97, 1.0];
    // (cx, cz, r, height, twist, body color): a denser cluster with varied
    // facet twists so adjacent shards catch the light differently.
    let shards = [
        (0.0, 0.0, 1.2, 5.4, 0.0, body),
        (1.7, 0.7, 0.8, 3.4, 0.5, body2),
        (-1.4, 1.1, 0.7, 3.0, 0.9, body),
        (0.8, -1.6, 0.6, 2.6, 0.3, body2),
        (-1.1, -1.1, 0.5, 2.0, 0.7, body),
        (0.9, 1.6, 0.42, 1.7, 1.1, body2),
        (-0.3, 1.9, 0.34, 1.3, 0.2, body),
        (-1.9, -0.1, 0.36, 1.5, 0.8, body2),
        (0.1, -2.1, 0.30, 1.1, 0.4, body),
    ];
    for (cx, cz, r, hgt, tw, col) in shards {
        let y0 = 0.4;
        let ymid = y0 + hgt * 0.45;
        let yneck = y0 + hgt * 0.72;
        let ytip = y0 + hgt;
        // Waisted shard: widens to a girdle, narrows, then a bright tip.
        push_frustum(&mut m, cx, cz, r * 0.8, r, y0, ymid, col, 0.0, 6, tw, false);
        push_frustum(
            &mut m,
            cx,
            cz,
            r,
            r * 0.62,
            ymid,
            yneck,
            col,
            0.0,
            6,
            tw,
            false,
        );
        push_pyramid(&mut m, cx, cz, r * 0.62, yneck, ytip, core, 0.0, 6, tw);
    }
    // Crystal stays flat shaded: the glassy shader supplies all its life.
    set_detail(&mut m, DETAIL_FLAT);
    m
}

/// Carbon node, opaque part: the vented rock mound. The glowing gas pool in
/// the crater renders through the blended crystal pipeline
/// (`carbon_pool_mesh`), and the rising green smoke is an fx emitter.
fn carbon_node_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let vent = [0.21, 0.25, 0.23];
    let vent_dk = [0.13, 0.16, 0.15];
    let vent_lt = [0.28, 0.33, 0.30];
    let stain = [0.33, 0.45, 0.32];
    // Three-stage mound with a sulfurous stain band below the crater lip.
    push_frustum(
        &mut m, 0.0, 0.0, 4.0, 3.4, 0.0, 0.9, vent_dk, 0.0, 10, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 3.4, 3.0, 0.9, 1.6, vent, 0.0, 10, 0.3, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 3.0, 2.2, 1.6, 3.0, vent, 0.0, 10, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 2.62, 2.42, 2.30, 2.62, stain, 0.0, 10, 0.0, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 2.3, 2.95, 3.12, vent_dk, 0.0, 10, 0.0, false,
    );
    // Crooked vent rocks around the rim, each a faceted spur with a lighter
    // cap, plus low scree boulders at the foot.
    for (k, &(cx, cz)) in [(2.2_f32, 0.8_f32), (-1.4, 2.0), (-2.0, -1.4), (1.2, -2.0)]
        .iter()
        .enumerate()
    {
        let h = 2.2 + 0.18 * cx;
        let tw = k as f32 * 0.6;
        push_frustum(&mut m, cx, cz, 0.62, 0.40, 0.8, h, vent, 0.0, 5, tw, false);
        push_pyramid(&mut m, cx, cz, 0.40, h, h + 0.55, vent_lt, 0.0, 5, tw);
    }
    for (k, &(bx, bz)) in [(3.3_f32, -0.6_f32), (-3.1, 0.9), (0.2, 3.5), (-1.6, -3.0)]
        .iter()
        .enumerate()
    {
        push_gem(
            &mut m,
            bx,
            bz,
            0.5 + 0.1 * (k % 2) as f32,
            0.0,
            0.3,
            0.75,
            if k % 2 == 0 { vent_lt } else { vent },
            0.0,
            5,
        );
    }
    set_detail(&mut m, DETAIL_ROCK);
    m
}

/// The glowing gas pool capping a carbon geyser: a shallow translucent dome,
/// blended like the crystals so it reads as liquid light.
fn carbon_pool_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let glow = [0.50, 0.95, 0.60];
    let glow_core = [0.80, 1.0, 0.84];
    // Pool surface with a brighter boiling heart and three rising bubbles.
    push_prism(&mut m, 0.0, 0.0, 1.9, 3.0, 3.2, glow, 0.0, 12, 0.0, true);
    push_prism(
        &mut m, 0.0, 0.0, 1.3, 3.1, 3.35, glow_core, 0.0, 12, 0.0, true,
    );
    push_dome(&mut m, 0.5, 0.3, 0.30, 3.30, glow_core, 0.0, 8, 2);
    push_dome(&mut m, -0.7, -0.4, 0.22, 3.26, glow, 0.0, 8, 2);
    push_dome(&mut m, -0.1, 0.9, 0.16, 3.30, glow_core, 0.0, 8, 2);
    // The gas reads as liquid light: keep it flat shaded.
    set_detail(&mut m, DETAIL_FLAT);
    m
}

/// The barrel of green sludge a worker hauls home from a carbon geyser.
/// Authored tiny at the origin; the instance places it on the carrier.
fn barrel_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let drum = [0.30, 0.42, 0.32];
    let band = [0.18, 0.22, 0.20];
    let sludge = [0.55, 0.95, 0.50];
    let haz = [0.80, 0.58, 0.20];
    // Ribbed drum with twin rolling bands, a hazard stencil patch, a welded
    // rim, and the sludge slopping over the top.
    push_prism(&mut m, 0.0, 0.0, 0.45, 0.0, 1.0, drum, 0.0, 10, 0.0, false);
    push_prism(&mut m, 0.0, 0.0, 0.49, 0.0, 0.07, band, 0.0, 10, 0.0, false);
    push_prism(
        &mut m, 0.0, 0.0, 0.49, 0.28, 0.42, band, 0.0, 10, 0.0, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 0.49, 0.62, 0.76, band, 0.0, 10, 0.0, false,
    );
    push_box(&mut m, [-0.14, 0.44, -0.52], [0.14, 0.62, -0.40], haz, 0.0);
    push_prism(
        &mut m, 0.0, 0.0, 0.47, 0.96, 1.02, band, 0.0, 10, 0.0, false,
    );
    push_prism(
        &mut m, 0.0, 0.0, 0.38, 1.0, 1.10, sludge, 0.0, 10, 0.0, true,
    );
    push_dome(&mut m, 0.12, -0.08, 0.12, 1.08, sludge, 0.0, 6, 2);
    m
}

/// Defensive turret: an octagonal armoured base, a team-tinted housing, and a
/// raised twin-barrel cannon. Authored at world scale, ~5 wide, facing -z.
fn turret_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let steel = [0.40, 0.43, 0.48];
    let steel2 = [0.52, 0.55, 0.60];
    let dark = [0.22, 0.24, 0.28];
    let haz = [0.80, 0.58, 0.20];
    let gun = [0.16, 0.18, 0.21];
    let amber = [0.95, 0.75, 0.35];
    let team = [0.5, 0.5, 0.5];
    // Footing with anchor bolts + plated base with a hazard collar and
    // armored skirt plates.
    push_frustum(
        &mut m, 0.0, 0.0, 2.6, 2.1, 0.0, 0.6, dark, 0.0, 10, 0.0, true,
    );
    for k in 0..5 {
        let a = std::f32::consts::TAU * k as f32 / 5.0 + 0.3;
        push_prism(
            &mut m,
            a.cos() * 2.25,
            a.sin() * 2.25,
            0.13,
            0.0,
            0.78,
            steel2,
            0.0,
            6,
            0.0,
            true,
        );
    }
    push_prism(&mut m, 0.0, 0.0, 2.0, 0.6, 1.9, steel, 0.0, 10, 0.0, true);
    push_prism(&mut m, 0.0, 0.0, 2.06, 0.72, 1.0, haz, 0.0, 10, 0.0, false);
    for k in 0..5 {
        let a = std::f32::consts::TAU * k as f32 / 5.0;
        push_box(
            &mut m,
            [a.cos() * 1.9 - 0.3, 1.1, a.sin() * 1.9 - 0.3],
            [a.cos() * 1.9 + 0.3, 1.75, a.sin() * 1.9 + 0.3],
            steel2,
            0.0,
        );
    }
    // Team-tinted rotating housing on a turntable ring, with a sensor lamp.
    push_prism(&mut m, 0.0, 0.0, 1.7, 1.9, 2.08, dark, 0.0, 10, 0.0, false);
    push_prism(&mut m, 0.0, 0.0, 1.5, 2.08, 3.0, team, 1.0, 8, 0.0, true);
    push_box(&mut m, [-0.2, 3.0, -0.7], [0.2, 3.22, -0.3], amber, 0.0);
    // Twin round barrels pointing -z, with recoil sleeves and muzzle brakes,
    // and an armored breech with a rear ammo hopper.
    for cx in [-0.4_f32, 0.4] {
        push_tube_z(&mut m, cx, 2.4, 0.15, -3.4, 0.4, gun, 0.0, 8, true);
        push_tube_z(&mut m, cx, 2.4, 0.22, -1.6, -0.4, steel, 0.0, 8, false);
        push_tube_z(&mut m, cx, 2.4, 0.20, -3.4, -3.0, dark, 0.0, 8, true);
    }
    push_box(&mut m, [-0.9, 2.0, 0.2], [0.9, 2.9, 1.0], gun, 0.0); // breech
    push_box(&mut m, [-0.6, 2.1, 1.0], [0.6, 2.7, 1.5], dark, 0.0);
    push_box(&mut m, [-0.7, 2.85, 0.3], [0.7, 3.0, 0.9], steel2, 0.0);
    m
}

/// Supply depot: a squat habitat bunker (small 5x5 footprint tier) with a
/// hazard skirt, a team-tinted habitat band, a domed roof, and a vent stack.
/// Shared by both factions, like the turret.
fn supply_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let steel = [0.50, 0.53, 0.57];
    let steel2 = [0.62, 0.65, 0.69];
    let steel3 = [0.40, 0.43, 0.47];
    let dark = [0.26, 0.28, 0.32];
    let haz = [0.80, 0.58, 0.20];
    let amber = [0.95, 0.75, 0.35];
    let team = [0.5, 0.5, 0.5];
    // Pad with a chamfered curb + plated bunker body.
    push_box(&mut m, [-2.6, 0.0, -2.6], [2.6, 0.4, 2.6], dark, 0.0);
    push_box(&mut m, [-2.45, 0.4, -2.45], [2.45, 0.5, 2.45], steel3, 0.0);
    push_box(&mut m, [-2.2, 0.4, -2.2], [2.2, 2.2, 2.2], steel, 0.0);
    // Hazard skirt striped into chevron segments.
    for k in 0..4 {
        let x0 = -2.22 + k as f32 * 1.11;
        let c = if k % 2 == 0 { haz } else { dark };
        push_box(&mut m, [x0, 0.5, -2.22], [x0 + 1.11, 0.85, 2.22], c, 0.0);
    }
    // Corner pilasters + a small airlock door on the +z face.
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        push_box(
            &mut m,
            [sx * 2.05 - 0.22, 0.4, sz * 2.05 - 0.22],
            [sx * 2.05 + 0.22, 2.35, sz * 2.05 + 0.22],
            steel2,
            0.0,
        );
    }
    push_box(&mut m, [-0.55, 0.4, 2.12], [0.55, 1.7, 2.34], dark, 0.0);
    push_box(&mut m, [-0.18, 1.25, 2.3], [0.18, 1.5, 2.4], amber, 0.0);
    // Team-tinted habitat band, mullioned into panes.
    push_box(&mut m, [-2.3, 1.6, -2.3], [2.3, 2.0, 2.3], team, 1.0);
    for sx in [-1.0_f32, 1.0] {
        push_box(
            &mut m,
            [sx * 2.32 - 0.05, 1.6, -0.12],
            [sx * 2.32 + 0.05, 2.0, 0.12],
            dark,
            0.0,
        );
        push_box(
            &mut m,
            [-0.12, 1.6, sx * 2.32 - 0.05],
            [0.12, 2.0, sx * 2.32 + 0.05],
            dark,
            0.0,
        );
    }
    // Domed roof in two stages with a beacon, supply crates on the deck,
    // and a banded vent stack on one corner.
    push_frustum(
        &mut m, 0.0, 0.0, 2.0, 1.4, 2.2, 2.9, steel, 0.0, 10, 0.0, false,
    );
    push_dome(&mut m, 0.0, 0.0, 1.4, 2.9, steel2, 0.0, 10, 3);
    push_box(&mut m, [-0.14, 4.28, -0.14], [0.14, 4.5, 0.14], amber, 0.0);
    push_box(&mut m, [-1.9, 2.2, 0.6], [-1.1, 2.85, 1.4], steel3, 0.0);
    push_box(&mut m, [-1.75, 2.85, 0.75], [-1.25, 3.3, 1.25], dark, 0.0);
    push_prism(&mut m, 1.5, 1.5, 0.3, 2.2, 3.1, dark, 0.0, 8, 0.0, true);
    push_prism(&mut m, 1.5, 1.5, 0.34, 2.75, 2.9, haz, 0.0, 8, 0.0, false);
    m
}

/// Fx particle: a unit cube around the origin. Drawn emissive (instance tint
/// alpha >= 3.0), scaled per particle; the chunky cube reads PS1-appropriate.
fn particle_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_box(
        &mut m,
        [-0.5, -0.5, -0.5],
        [0.5, 0.5, 0.5],
        [1.0, 1.0, 1.0],
        0.0,
    );
    set_detail(&mut m, DETAIL_FLAT);
    m
}

/// Heavy assault unit (placeholder War-Mech / Golem): a stocky two-legged walker,
/// team-tinted core, with shoulder guns. ~3.4 tall, facing -z.
fn heavy_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let armor = [0.46, 0.49, 0.54];
    let armor2 = [0.34, 0.37, 0.42];
    let armor3 = [0.56, 0.59, 0.64];
    let dark = [0.18, 0.20, 0.24];
    let haz = [0.80, 0.58, 0.20];
    let gun = [0.15, 0.17, 0.20];
    let visor = [0.9, 0.5, 0.3];
    let team = [0.5, 0.5, 0.5];
    // Legs: armored thigh, piston shin, knee plate, and a clawed three-toe
    // foot per side.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 0.44;
        push_box(
            &mut m,
            [x - 0.20, 0.62, -0.30],
            [x + 0.20, 1.16, 0.30],
            armor2,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.14, 0.18, -0.20],
            [x + 0.14, 0.68, 0.20],
            dark,
            0.0,
        );
        push_prism(
            &mut m, x, -0.26, 0.07, 0.2, 0.72, armor3, 0.0, 6, 0.0, false,
        );
        push_box(
            &mut m,
            [x - 0.17, 0.62, 0.26],
            [x + 0.17, 0.86, 0.38],
            armor3,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.19, 0.0, -0.42],
            [x + 0.19, 0.2, 0.34],
            dark,
            0.0,
        );
        for t in 0..3 {
            let tx = x - 0.16 + t as f32 * 0.12;
            push_box(&mut m, [tx, 0.0, 0.32], [tx + 0.08, 0.16, 0.56], gun, 0.0);
        }
    }
    // Hip gimbal + broad layered torso with a chamfered top plate.
    push_box(&mut m, [-0.7, 1.1, -0.5], [0.7, 1.5, 0.5], armor2, 0.0);
    push_box(&mut m, [-0.35, 1.18, -0.56], [0.35, 1.44, 0.56], dark, 0.0);
    push_box(&mut m, [-0.85, 1.5, -0.6], [0.85, 2.7, 0.6], armor, 0.0);
    push_box(
        &mut m,
        [-0.88, 1.62, -0.63],
        [0.88, 1.86, 0.63],
        armor3,
        0.0,
    );
    push_frustum(
        &mut m,
        0.0,
        0.0,
        1.05,
        0.8,
        2.7,
        2.95,
        armor2,
        0.0,
        4,
        std::f32::consts::FRAC_PI_4,
        false,
    );
    // Hazard stripe across the lower torso glacis.
    push_box(&mut m, [-0.6, 1.52, 0.58], [0.6, 1.64, 0.64], haz, 0.0);
    // Team-tinted core lens in an armored bezel + intake vents on the back.
    push_box(&mut m, [-0.36, 1.74, 0.54], [0.36, 2.36, 0.68], dark, 0.0);
    push_box(&mut m, [-0.3, 1.8, 0.58], [0.3, 2.3, 0.74], team, 1.0);
    for k in 0..3 {
        let y0 = 1.7 + k as f32 * 0.32;
        push_box(
            &mut m,
            [-0.5, y0, -0.68],
            [0.5, y0 + 0.18, -0.56],
            dark,
            0.0,
        );
    }
    // Head turret: armored cowl, glowing visor slit, twin sensor horns.
    push_box(&mut m, [-0.4, 2.95, -0.4], [0.4, 3.45, 0.4], armor2, 0.0);
    push_box(&mut m, [-0.44, 3.35, -0.44], [0.44, 3.5, 0.44], armor3, 0.0);
    push_box(&mut m, [-0.28, 3.1, 0.38], [0.28, 3.3, 0.5], visor, 0.0);
    for sx in [-1.0_f32, 1.0] {
        push_prism(
            &mut m,
            sx * 0.3,
            -0.2,
            0.05,
            3.5,
            3.85,
            dark,
            0.0,
            6,
            0.0,
            true,
        );
    }
    // Shoulder gun pods: armored housing, round recoil-sleeved barrels out
    // the face (+z) side, and a top hatch.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 1.0;
        push_box(
            &mut m,
            [x - 0.22, 1.95, -0.42],
            [x + 0.22, 2.75, 0.42],
            dark,
            0.0,
        );
        push_box(
            &mut m,
            [x - 0.26, 2.55, -0.46],
            [x + 0.26, 2.75, 0.46],
            armor3,
            0.0,
        );
        push_tube_z(&mut m, x, 2.32, 0.10, 0.4, 1.15, gun, 0.0, 6, true);
        push_tube_z(&mut m, x, 2.32, 0.15, 0.42, 0.78, armor2, 0.0, 6, false);
        push_box(
            &mut m,
            [x - 0.06, 2.06, 0.40],
            [x + 0.06, 2.16, 0.62],
            gun,
            0.0,
        );
    }
    m
}

/// Opaque dark walls around the map rim, from above the water down past the
/// seabed, so you don't see under the (translucent) water at the edges. Drawn
/// Astromancer Ward: a levitating concrete monolith (the faction's turret).
/// A small anchor pad stays grounded; the carved stone hovers a clear gap
/// above it, gold-banded, with a team-tinted aether crystal at the crown and
/// the firing prong on the -z face (the swivel convention).
fn ward_mesh_astro() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let rock = [0.52, 0.51, 0.48];
    let rock_dk = [0.38, 0.37, 0.35];
    let rock_lt = [0.62, 0.61, 0.57];
    let gold = [0.78, 0.65, 0.35];
    let gold_dk = [0.64, 0.52, 0.27];
    let aether = [0.85, 0.95, 1.0];
    let team = [0.5, 0.5, 0.5];
    // Grounded anchor pad with a carved curb and four gold anchor studs.
    push_frustum(
        &mut m, 0.0, 0.0, 1.9, 1.3, 0.0, 0.4, rock_dk, 0.0, 8, 0.0, true,
    );
    for k in 0..4 {
        let a = std::f32::consts::TAU * k as f32 / 4.0 + 0.4;
        push_gem(
            &mut m,
            a.cos() * 1.55,
            a.sin() * 1.55,
            0.14,
            0.30,
            0.46,
            0.66,
            gold_dk,
            0.0,
            4,
        );
    }
    // The hovering monolith (clear gap from 0.4 to 1.4): carved in three
    // stages with chiselled corner facets and a lighter cap stone.
    push_frustum(
        &mut m, 0.0, 0.0, 1.9, 2.4, 1.4, 1.9, rock_dk, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 2.4, 1.9, 1.9, 4.4, rock, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 1.9, 1.1, 4.4, 5.9, rock_dk, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 1.1, 0.85, 5.9, 6.2, rock_lt, 0.0, 6, 0.0, true,
    );
    // Carved rune slits glowing aether on the upper faces.
    for k in 0..3 {
        let a = std::f32::consts::TAU * k as f32 / 3.0 + 0.5;
        let (rx, rz) = (a.cos() * 1.62, a.sin() * 1.62);
        push_box(
            &mut m,
            [rx - 0.1, 4.5, rz - 0.1],
            [rx + 0.1, 5.3, rz + 0.1],
            aether,
            0.0,
        );
    }
    // Stepped gold waistband + a floating gold halo ring under the crown.
    push_prism(&mut m, 0.0, 0.0, 2.0, 2.9, 3.3, gold, 0.0, 6, 0.0, false);
    push_prism(
        &mut m, 0.0, 0.0, 1.94, 3.3, 3.44, gold_dk, 0.0, 6, 0.0, false,
    );
    push_prism(&mut m, 0.0, 0.0, 1.35, 6.5, 6.62, gold, 0.0, 10, 0.0, false);
    // Crowning team crystal: a floating cut gem instead of a plain pyramid.
    push_gem(&mut m, 0.0, 0.0, 0.85, 6.4, 6.9, 8.0, team, 1.0, 6);
    // The aether prong fires toward -z: a gold socket, the glowing rail,
    // and a bright emitter tip.
    push_box(&mut m, [-0.35, 3.4, -2.0], [0.35, 4.1, -1.3], gold_dk, 0.0);
    push_box(&mut m, [-0.25, 3.5, -3.2], [0.25, 4.0, -1.7], aether, 0.0);
    push_gem(&mut m, 0.0, -3.3, 0.3, 3.45, 3.75, 4.05, aether, 0.0, 4);
    m
}

/// Astromancer Depot: levitating concrete storage slabs - two carved blocks
/// hovering stacked above a grounded pad, ringed by a gold band, with a
/// small team beacon on top.
fn supply_mesh_astro() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let rock = [0.55, 0.54, 0.51];
    let rock_dk = [0.40, 0.39, 0.37];
    let rock_lt = [0.64, 0.63, 0.59];
    let gold = [0.78, 0.65, 0.35];
    let gold_dk = [0.64, 0.52, 0.27];
    let aether = [0.85, 0.95, 1.0];
    let team = [0.5, 0.5, 0.5];
    // Grounded anchor pad with a carved curb.
    push_frustum(
        &mut m, 0.0, 0.0, 2.4, 1.8, 0.0, 0.4, rock_dk, 0.0, 8, 0.0, true,
    );
    push_prism(
        &mut m, 0.0, 0.0, 1.95, 0.4, 0.52, gold_dk, 0.0, 8, 0.0, true,
    );
    // Main slab hovers over a clear gap: chamfered (a frustum skirt under a
    // carved block), with corner studs and an aether seam glowing in the
    // hover gap.
    push_frustum(
        &mut m,
        0.0,
        0.0,
        2.6,
        3.3,
        1.2,
        1.65,
        rock_dk,
        0.0,
        4,
        std::f32::consts::FRAC_PI_4,
        false,
    );
    push_box(&mut m, [-2.6, 1.65, -2.2], [2.6, 2.8, 2.2], rock, 0.0);
    push_box(
        &mut m,
        [-2.68, 2.72, -2.28],
        [2.68, 2.92, 2.28],
        rock_lt,
        0.0,
    );
    push_prism(&mut m, 0.0, 0.0, 0.7, 0.9, 1.3, aether, 0.0, 8, 0.0, false);
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        push_gem(
            &mut m,
            sx * 2.3,
            sz * 1.9,
            0.18,
            2.78,
            2.98,
            3.2,
            gold_dk,
            0.0,
            4,
        );
    }
    // Gold band around the main slab's waist, stepped in two reliefs.
    push_box(&mut m, [-2.7, 1.9, -2.3], [2.7, 2.15, 2.3], gold, 0.0);
    push_box(
        &mut m,
        [-2.74, 2.15, -2.34],
        [2.74, 2.25, 2.34],
        gold_dk,
        0.0,
    );
    // A smaller carved slab floats above, rotated 45 degrees, with its own
    // gold seam; the team beacon gem floats at the top.
    push_frustum(
        &mut m,
        0.0,
        0.0,
        2.3,
        2.0,
        3.4,
        4.4,
        rock_dk,
        0.0,
        4,
        std::f32::consts::FRAC_PI_4,
        true,
    );
    push_prism(
        &mut m,
        0.0,
        0.0,
        2.16,
        3.78,
        3.95,
        gold,
        0.0,
        4,
        std::f32::consts::FRAC_PI_4,
        false,
    );
    push_gem(&mut m, 0.0, 0.0, 0.45, 4.55, 4.9, 5.5, team, 1.0, 6);
    m
}

/// with the unit pipeline via an identity instance.
fn water_walls() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let h = terrain::HALF;
    let top = terrain::SEA_LEVEL + 3.0;
    let bot = terrain::SEABED - 4.0;
    let c = [0.09, 0.12, 0.16];
    let t = 3.0;
    push_box(&mut m, [h - t, bot, -h], [h, top, h], c, 0.0); // east
    push_box(&mut m, [-h, bot, -h], [-h + t, top, h], c, 0.0); // west
    push_box(&mut m, [-h, bot, h - t], [h, top, h], c, 0.0); // south
    push_box(&mut m, [-h, bot, -h], [h, top, -h + t], c, 0.0); // north
    set_detail(&mut m, DETAIL_FLAT);
    m
}

fn terrain_mesh() -> (Vec<Vertex3>, Vec<u32>) {
    let n: usize = 220;
    let half = terrain::HALF;
    let step = (2.0 * half) / n as f32;
    let mut verts = Vec::with_capacity((n + 1) * (n + 1));
    for j in 0..=n {
        for i in 0..=n {
            let x = -half + i as f32 * step;
            let z = -half + j as f32 * step;
            verts.push(Vertex3 {
                pos: [x, terrain::height(x, z), z],
                normal: terrain::normal(x, z),
            });
        }
    }
    let mut idx = Vec::with_capacity(n * n * 6);
    let w = (n + 1) as u32;
    for j in 0..n as u32 {
        for i in 0..n as u32 {
            let a = j * w + i;
            idx.extend_from_slice(&[a, a + 1, a + w, a + 1, a + w + 1, a + w]);
        }
    }
    (verts, idx)
}

/// The Earthlike ocean sheet: a tessellated full-map grid at sea level (one
/// big quad gives the water vertex waves nothing to bend).
fn water_quad() -> Vec<[f32; 3]> {
    const N: usize = 96;
    let h = terrain::HALF;
    let s = 2.0 * h / N as f32;
    let mut out = Vec::with_capacity(N * N * 6);
    for k in 0..N {
        for i in 0..N {
            let (x0, z0) = (-h + i as f32 * s, -h + k as f32 * s);
            let (x1, z1) = (x0 + s, z0 + s);
            let a = [x0, 0.0, z0];
            let b = [x1, 0.0, z0];
            let c = [x1, 0.0, z1];
            let d = [x0, 0.0, z1];
            for v in [a, b, c, a, c, d] {
                out.push(v);
            }
        }
    }
    out
}

/// Tessellate selection rings into ground-decal triangles whose vertices sit a
/// hair above the terrain, so the ring follows hills instead of clipping.
fn ring_decals(rings: &[RingRaw]) -> Vec<RingVertex> {
    let mut out = Vec::new();
    for r in rings.iter().take(MAX_RINGS) {
        let (cx, cz) = (r.center[0], r.center[2]);
        let (r_in, r_out) = (r.radius * r.inner.clamp(0.0, 0.98), r.radius);
        let color = r.color;
        let pt = |radius: f32, c: f32, s: f32| {
            let (x, z) = (cx + c * radius, cz + s * radius);
            [x, terrain::height(x, z) + 0.4, z]
        };
        let mut prev: Option<([f32; 3], [f32; 3])> = None;
        for k in 0..=RING_SEGMENTS {
            let a = k as f32 / RING_SEGMENTS as f32 * std::f32::consts::TAU;
            let (s, c) = a.sin_cos();
            let inner = pt(r_in, c, s);
            let outer = pt(r_out, c, s);
            if let Some((pi, po)) = prev {
                for v in [pi, po, outer, pi, outer, inner] {
                    out.push(RingVertex { pos: v, color });
                }
            }
            prev = Some((inner, outer));
        }
    }
    out
}

fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: w.max(1),
                height: h.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn load_tile(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    bytes: &[u8],
    label: &str,
) -> wgpu::TextureView {
    let img = image::load_from_memory(bytes)
        .expect("decode png")
        .to_rgba8();
    let (w, h) = img.dimensions();
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &img,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * w),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

pub struct Gfx {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    terrain_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    unit_pipeline: wgpu::RenderPipeline,
    crystal_pipeline: wgpu::RenderPipeline,
    ring_pipeline: wgpu::RenderPipeline,
    terrain_vbuf: wgpu::Buffer,
    terrain_ibuf: wgpu::Buffer,
    terrain_indices: u32,
    water_buf: wgpu::Buffer,
    water_len: u32,
    infantry_buf: wgpu::Buffer,
    infantry_len: u32,
    barracks_astro_buf: wgpu::Buffer,
    barracks_astro_len: u32,
    barracks_hollow_buf: wgpu::Buffer,
    barracks_hollow_len: u32,
    hq_astro_buf: wgpu::Buffer,
    hq_astro_len: u32,
    hq_hollow_buf: wgpu::Buffer,
    hq_hollow_len: u32,
    acolyte_buf: wgpu::Buffer,
    acolyte_len: u32,
    engineer_buf: wgpu::Buffer,
    engineer_len: u32,
    ore_node_buf: wgpu::Buffer,
    ore_node_len: u32,
    carbon_node_buf: wgpu::Buffer,
    carbon_node_len: u32,
    ore_crystal_buf: wgpu::Buffer,
    ore_crystal_len: u32,
    carbon_pool_buf: wgpu::Buffer,
    carbon_pool_len: u32,
    barrel_buf: wgpu::Buffer,
    barrel_len: u32,
    turret_buf: wgpu::Buffer,
    turret_len: u32,
    supply_buf: wgpu::Buffer,
    supply_len: u32,
    ward_astro_buf: wgpu::Buffer,
    ward_astro_len: u32,
    supply_astro_buf: wgpu::Buffer,
    supply_astro_len: u32,
    particle_buf: wgpu::Buffer,
    particle_len: u32,
    heavy_buf: wgpu::Buffer,
    heavy_len: u32,
    walls_buf: wgpu::Buffer,
    walls_len: u32,
    wall_inst_buf: wgpu::Buffer,
    voxel_pipeline: wgpu::RenderPipeline,
    /// Triplanar-textured marching-cubes terrain for the active voxel map
    /// (vertex buffer + count + its tile bind group), or `None` for the heightmap.
    voxel_terrain: Option<(wgpu::Buffer, u32, wgpu::BindGroup)>,
    /// Per-column liquid surface mesh for the active voxel map (Titan/Earth).
    voxel_water: Option<(wgpu::Buffer, u32)>,
    instance_buf: wgpu::Buffer,
    ring_buf: wgpu::Buffer,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    /// The model surface-detail map (group 3 of the unit pipeline).
    detail_bind: wgpu::BindGroup,
    terrain_bind: wgpu::BindGroup,
    terrain_layout: wgpu::BindGroupLayout,
    tile_sampler: wgpu::Sampler,
    fow_sampler: wgpu::Sampler,
    world_buf: wgpu::Buffer,
    world_bind: wgpu::BindGroup,
    fow_tex: wgpu::Texture,
    pub width: u32,
    pub height: u32,
}

/// Make a GPU vertex buffer from raw vertex bytes.
fn vbuf(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, data: &[u8]) -> wgpu::Buffer {
    let b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: data.len() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&b, 0, data);
    b
}

impl Gfx {
    pub async fn new(window: Arc<Window>) -> Gfx {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance.create_surface(window).expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("request adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("device"),
                required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                    .using_resolution(adapter.limits()),
                ..Default::default()
            })
            .await
            .expect("request device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);
        let depth = make_depth(&device, width, height);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        // --- camera (group 0) ---
        let camera_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bind"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buf.as_entire_binding(),
            }],
        });

        // --- terrain textures + fog of war (group 1) ---
        let grass = load_tile(
            &device,
            &queue,
            include_bytes!("../../../assets/textures/grass.png"),
            "grass",
        );
        let dirt = load_tile(
            &device,
            &queue,
            include_bytes!("../../../assets/textures/dirt.png"),
            "dirt",
        );
        let rock = load_tile(
            &device,
            &queue,
            include_bytes!("../../../assets/textures/rock.png"),
            "rock",
        );
        let sand = load_tile(
            &device,
            &queue,
            include_bytes!("../../../assets/textures/sand.png"),
            "sand",
        );

        let fow_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("fow"),
            size: wgpu::Extent3d {
                width: FOW_RES as u32,
                height: FOW_RES as u32,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let fow_view = fow_tex.create_view(&wgpu::TextureViewDescriptor::default());

        // Linear filtering gives the ground tiles a soft painterly blur up
        // close instead of hard texel blocks.
        let tile_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tile-samp"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        // Linear filtering on the fog field gives soft, feathered borders
        // instead of blocky per-cell steps.
        let fow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("fow-samp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let tex_entry = |binding: u32, filterable: bool| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let samp_entry = |binding: u32, ty: wgpu::SamplerBindingType| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(ty),
            count: None,
        };
        let terrain_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain-layout"),
            entries: &[
                tex_entry(0, true), // grass - linear (soft painterly blur)
                tex_entry(1, true), // dirt
                tex_entry(2, true), // rock
                tex_entry(3, true), // sand
                tex_entry(4, true), // fog of war - linear-filtered soft borders
                samp_entry(5, wgpu::SamplerBindingType::Filtering), // tiles
                samp_entry(6, wgpu::SamplerBindingType::Filtering), // fow
            ],
        });
        let terrain_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-bind"),
            layout: &terrain_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&grass),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&dirt),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&rock),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&sand),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&fow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&tile_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&fow_sampler),
                },
            ],
        });

        // group 3 (unit pipeline): the model surface-detail map, triplanar
        // sampled in object space so units/buildings are textured without UVs.
        let detail = load_tile(
            &device,
            &queue,
            include_bytes!("../../../assets/textures/detail.png"),
            "detail",
        );
        let detail_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("detail-layout"),
            entries: &[
                tex_entry(0, true),
                samp_entry(1, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let detail_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("detail-bind"),
            layout: &detail_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&detail),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&tile_sampler),
                },
            ],
        });

        // group 2: per-world appearance (voxel terrain tint + liquid colour).
        let world_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let world_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world"),
            size: std::mem::size_of::<WorldUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &world_buf,
            0,
            bytemuck::bytes_of(&WorldUniform {
                tint: [1.0, 1.0, 1.0, 0.0],
                liquid: EARTH_WATER,
            }),
        );
        let world_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-bind"),
            layout: &world_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: world_buf.as_entire_binding(),
            }],
        });

        let pl_tex = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl-tex"),
            bind_group_layouts: &[Some(&camera_layout), Some(&terrain_layout)],
            immediate_size: 0,
        });
        let pl_plain = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl-plain"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        // Units/buildings: camera + the surface-detail map at group 3
        // (groups 1 and 2 are holes; fs_unit touches neither).
        let pl_unit = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl-unit"),
            bind_group_layouts: &[Some(&camera_layout), None, None, Some(&detail_layout)],
            immediate_size: 0,
        });
        // Voxel terrain + water sample the world uniform at group 2.
        let pl_world = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl-world"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&terrain_layout),
                Some(&world_layout),
            ],
            immediate_size: 0,
        });

        let depth_opaque = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let depth_blend = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        // Ground decals (selection rings, order pings): a strong negative depth
        // bias pulls them toward the camera so they never z-fight or clip into
        // slope geometry between their tessellation samples.
        let depth_decal = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: -8,
                slope_scale: -8.0,
                clamp: 0.0,
            },
        };
        let opaque_t = wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let blend_t = wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };

        let v3 = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex3>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
        };
        let v3u = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<UnitVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 5 => Float32x4, 8 => Float32],
        };
        let pos3 = wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        };
        let voxel_vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VoxelVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32],
        };
        let inst = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32x3, 4 => Float32x4, 6 => Float32x2, 7 => Float32x2],
        };
        let ring_v = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RingVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
        };

        let mk = |label: &str,
                  layout: &wgpu::PipelineLayout,
                  vs: &str,
                  fs: &str,
                  buffers: &[wgpu::VertexBufferLayout],
                  target: &wgpu::ColorTargetState,
                  depth: &wgpu::DepthStencilState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    targets: &[Some(target.clone())],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth.clone()),
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };

        let terrain_pipeline = mk(
            "terrain",
            &pl_tex,
            "vs_terrain",
            "fs_terrain",
            std::slice::from_ref(&v3),
            &opaque_t,
            &depth_opaque,
        );
        let unit_pipeline = mk(
            "unit",
            &pl_unit,
            "vs_unit",
            "fs_unit",
            &[v3u.clone(), inst.clone()],
            &opaque_t,
            &depth_opaque,
        );
        // Crystals/gas pools: same instancing as units, but alpha-blended
        // with env-mapped shine (drawn after the opaque world and water).
        // Depth writes stay ON, and fs_crystal discards back-facing
        // fragments by the authored outward normal (mesh winding is not
        // consistent, so hardware face culling would cut the wrong faces);
        // together only the nearest front-facing facet blends, instead of
        // interior and rear facets stacking in arbitrary triangle order.
        let depth_crystal = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let crystal_pipeline = mk(
            "crystal",
            &pl_plain,
            "vs_unit",
            "fs_crystal",
            &[v3u, inst],
            &blend_t,
            &depth_crystal,
        );
        let voxel_pipeline = mk(
            "voxel",
            &pl_world,
            "vs_voxel",
            "fs_voxel",
            std::slice::from_ref(&voxel_vbl),
            &opaque_t,
            &depth_opaque,
        );
        let water_pipeline = mk(
            "water",
            &pl_world,
            "vs_water",
            "fs_water",
            std::slice::from_ref(&pos3),
            &blend_t,
            &depth_blend,
        );
        let ring_pipeline = mk(
            "ring",
            &pl_plain,
            "vs_ring",
            "fs_ring",
            std::slice::from_ref(&ring_v),
            &blend_t,
            &depth_decal,
        );

        let mkbuf = |label: &str, data: &[u8], usage: wgpu::BufferUsages| {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: data.len() as u64,
                usage,
                mapped_at_creation: false,
            });
            queue.write_buffer(&b, 0, data);
            b
        };

        let (tv, ti) = terrain_mesh();
        let terrain_vbuf = mkbuf(
            "tv",
            bytemuck::cast_slice(&tv),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let terrain_ibuf = mkbuf(
            "ti",
            bytemuck::cast_slice(&ti),
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        let water = water_quad();
        let water_buf = mkbuf(
            "water",
            bytemuck::cast_slice(&water),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let infantry = infantry_mesh();
        let infantry_buf = mkbuf(
            "infantry",
            bytemuck::cast_slice(&infantry),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let barracks_astro = barracks_mesh_astro();
        let barracks_astro_buf = mkbuf(
            "barracks-astro",
            bytemuck::cast_slice(&barracks_astro),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let barracks_hollow = barracks_mesh_hollow();
        let barracks_hollow_buf = mkbuf(
            "barracks-hollow",
            bytemuck::cast_slice(&barracks_hollow),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let hq_astro = hq_mesh_astro();
        let hq_astro_buf = mkbuf(
            "hq-astro",
            bytemuck::cast_slice(&hq_astro),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let hq_hollow = hq_mesh_hollow();
        let hq_hollow_buf = mkbuf(
            "hq-hollow",
            bytemuck::cast_slice(&hq_hollow),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let acolyte = acolyte_mesh();
        let acolyte_buf = mkbuf(
            "acolyte",
            bytemuck::cast_slice(&acolyte),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let engineer = engineer_mesh();
        let engineer_buf = mkbuf(
            "engineer",
            bytemuck::cast_slice(&engineer),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let ore_node = ore_node_mesh();
        let ore_node_buf = mkbuf(
            "ore-node",
            bytemuck::cast_slice(&ore_node),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let carbon_node = carbon_node_mesh();
        let carbon_node_buf = mkbuf(
            "carbon-node",
            bytemuck::cast_slice(&carbon_node),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let ore_crystal = ore_crystal_mesh();
        let ore_crystal_buf = mkbuf(
            "ore-crystal",
            bytemuck::cast_slice(&ore_crystal),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let carbon_pool = carbon_pool_mesh();
        let carbon_pool_buf = mkbuf(
            "carbon-pool",
            bytemuck::cast_slice(&carbon_pool),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let barrel = barrel_mesh();
        let barrel_buf = mkbuf(
            "barrel",
            bytemuck::cast_slice(&barrel),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let turret = turret_mesh();
        let turret_buf = mkbuf(
            "turret",
            bytemuck::cast_slice(&turret),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let supply = supply_mesh();
        let supply_buf = mkbuf(
            "supply",
            bytemuck::cast_slice(&supply),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let ward_astro = ward_mesh_astro();
        let ward_astro_buf = mkbuf(
            "ward-astro",
            bytemuck::cast_slice(&ward_astro),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let supply_astro = supply_mesh_astro();
        let supply_astro_buf = mkbuf(
            "supply-astro",
            bytemuck::cast_slice(&supply_astro),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let particle = particle_mesh();
        let particle_buf = mkbuf(
            "particle",
            bytemuck::cast_slice(&particle),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let heavy = heavy_mesh();
        let heavy_buf = mkbuf(
            "heavy",
            bytemuck::cast_slice(&heavy),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let walls = water_walls();
        let walls_buf = mkbuf(
            "walls",
            bytemuck::cast_slice(&walls),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let wall_inst_buf = mkbuf(
            "wall-inst",
            bytemuck::bytes_of(&InstanceRaw {
                offset: [0.0, 0.0, 0.0],
                scale: [1.0, 1.0, 1.0],
                color: [0.0, 0.0, 0.0, 1.0],
                rot: ROT_NONE,
                anim: ANIM_NONE,
            }),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        // The voxel battlefield (terrain mesh + tiles + liquid) is built lazily
        // by `set_world` when the lobby picks a map; the default is the heightmap.
        let voxel_terrain: Option<(wgpu::Buffer, u32, wgpu::BindGroup)> = None;
        let voxel_water: Option<(wgpu::Buffer, u32)> = None;
        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ring_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rings"),
            size: (MAX_RING_VERTS * std::mem::size_of::<RingVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Gfx {
            surface,
            device,
            queue,
            config,
            depth,
            terrain_pipeline,
            water_pipeline,
            unit_pipeline,
            crystal_pipeline,
            ring_pipeline,
            terrain_vbuf,
            terrain_ibuf,
            terrain_indices: ti.len() as u32,
            water_buf,
            water_len: water.len() as u32,
            infantry_buf,
            infantry_len: infantry.len() as u32,
            barracks_astro_buf,
            barracks_astro_len: barracks_astro.len() as u32,
            barracks_hollow_buf,
            barracks_hollow_len: barracks_hollow.len() as u32,
            hq_astro_buf,
            hq_astro_len: hq_astro.len() as u32,
            hq_hollow_buf,
            hq_hollow_len: hq_hollow.len() as u32,
            acolyte_buf,
            acolyte_len: acolyte.len() as u32,
            engineer_buf,
            engineer_len: engineer.len() as u32,
            ore_node_buf,
            ore_node_len: ore_node.len() as u32,
            carbon_node_buf,
            carbon_node_len: carbon_node.len() as u32,
            ore_crystal_buf,
            ore_crystal_len: ore_crystal.len() as u32,
            carbon_pool_buf,
            carbon_pool_len: carbon_pool.len() as u32,
            barrel_buf,
            barrel_len: barrel.len() as u32,
            turret_buf,
            turret_len: turret.len() as u32,
            supply_buf,
            supply_len: supply.len() as u32,
            ward_astro_buf,
            ward_astro_len: ward_astro.len() as u32,
            supply_astro_buf,
            supply_astro_len: supply_astro.len() as u32,
            particle_buf,
            particle_len: particle.len() as u32,
            heavy_buf,
            heavy_len: heavy.len() as u32,
            walls_buf,
            walls_len: walls.len() as u32,
            wall_inst_buf,
            voxel_pipeline,
            voxel_terrain,
            voxel_water,
            instance_buf,
            ring_buf,
            camera_buf,
            camera_bind,
            detail_bind,
            terrain_bind,
            terrain_layout,
            tile_sampler,
            fow_sampler,
            world_buf,
            world_bind,
            fow_tex,
            width,
            height,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.width = width;
        self.height = height;
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.depth = make_depth(&self.device, width, height);
    }

    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height as f32
    }

    /// Rebuild the terrain for the currently selected voxel map (call after
    /// `voxel::set_active`, e.g. when starting a match from the lobby): triplanar
    /// terrain mesh + the world's tile set + its liquid surface + tint/liquid
    /// uniform. With no map selected this clears back to the Earthlike heightmap.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // driven by the web lobby
    pub fn set_world(&mut self) {
        let Some(grid) = crate::voxel::active() else {
            self.voxel_terrain = None;
            self.voxel_water = None;
            self.write_world(WorldUniform {
                tint: [1.0, 1.0, 1.0, 0.0],
                liquid: EARTH_WATER,
            });
            return;
        };

        // Terrain mesh (pos/normal/material) for the triplanar voxel pipeline.
        let verts: Vec<VoxelVertex> = grid
            .build_mesh()
            .iter()
            .map(|v| VoxelVertex {
                pos: v.pos,
                normal: v.normal,
                weights: v.weights,
                haz: v.haz,
            })
            .collect();
        let buf = vbuf(
            &self.device,
            &self.queue,
            "voxel-terrain",
            bytemuck::cast_slice(&verts),
        );

        // The world's tile set in the base/low/high/accent (grass/dirt/rock/sand)
        // slots, plus the shared fog-of-war texture.
        let tiles = crate::voxel::active_tiles().expect("active map has a tile set");
        let tex: Vec<wgpu::TextureView> = tiles
            .iter()
            .enumerate()
            .map(|(k, b)| load_tile(&self.device, &self.queue, b, &format!("voxel-tile-{k}")))
            .collect();
        let fow_view = self
            .fow_tex
            .create_view(&wgpu::TextureViewDescriptor::default());
        let voxel_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("voxel-bind"),
            layout: &self.terrain_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tex[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tex[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&tex[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&tex[3]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&fow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.tile_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.fow_sampler),
                },
            ],
        });
        self.voxel_terrain = Some((buf, verts.len() as u32, voxel_bind));

        // Liquid surface mesh (oceans / methane), if any.
        let liq = grid.liquid_mesh();
        self.voxel_water = if liq.is_empty() {
            None
        } else {
            Some((
                vbuf(
                    &self.device,
                    &self.queue,
                    "voxel-water",
                    bytemuck::cast_slice(&liq),
                ),
                liq.len() as u32,
            ))
        };

        // Tint + liquid colour for this world.
        let lava = if crate::voxel::active_lava() {
            1.0
        } else {
            0.0
        };
        let liquid = crate::voxel::active_liquid().unwrap_or(EARTH_WATER);
        self.write_world(WorldUniform {
            tint: [1.0, 1.0, 1.0, lava],
            liquid,
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn write_world(&self, w: WorldUniform) {
        self.queue
            .write_buffer(&self.world_buf, 0, bytemuck::bytes_of(&w));
    }

    /// Rebuild the heightmap ground mesh after the building pads changed
    /// (`terrain::set_pads`). The grid topology is fixed, so only the vertex
    /// buffer is rewritten. Voxel battlefields are unaffected (their surface
    /// comes from the baked map, not the height function).
    pub fn rebuild_terrain(&mut self) {
        let (tv, _) = terrain_mesh();
        self.queue
            .write_buffer(&self.terrain_vbuf, 0, bytemuck::cast_slice(&tv));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        infantry: &[InstanceRaw],
        barracks_astro: &[InstanceRaw],
        barracks_hollow: &[InstanceRaw],
        hq_astro: &[InstanceRaw],
        hq_hollow: &[InstanceRaw],
        acolytes: &[InstanceRaw],
        engineers: &[InstanceRaw],
        ore_nodes: &[InstanceRaw],
        carbon_nodes: &[InstanceRaw],
        heavies: &[InstanceRaw],
        turrets: &[InstanceRaw],
        supplies: &[InstanceRaw],
        wards_astro: &[InstanceRaw],
        supplies_astro: &[InstanceRaw],
        barrels: &[InstanceRaw],
        particles: &[InstanceRaw],
        ore_crystals: &[InstanceRaw],
        carbon_pools: &[InstanceRaw],
        fx_lights: &[FxLight],
        rings: &[RingRaw],
        fow: &[u8],
        view_proj: [[f32; 4]; 4],
        eye: [f32; 3],
        time: f32,
    ) {
        // All meshes share one instance buffer, packed in order: infantry,
        // the faction barracks, the faction HQs, Acolytes, Engineers, ore
        // nodes, carbon nodes, heavies, turrets, supply depots, sludge
        // barrels, particles, then the alpha-blended crystal groups last.
        // Each mesh draws its own range.
        let groups = [
            infantry.len(),
            barracks_astro.len(),
            barracks_hollow.len(),
            hq_astro.len(),
            hq_hollow.len(),
            acolytes.len(),
            engineers.len(),
            ore_nodes.len(),
            carbon_nodes.len(),
            heavies.len(),
            turrets.len(),
            supplies.len(),
            wards_astro.len(),
            supplies_astro.len(),
            barrels.len(),
            particles.len(),
            ore_crystals.len(),
            carbon_pools.len(),
        ];
        // Clamp each group's count so the running total never exceeds the buffer.
        let mut counts = [0usize; 18];
        let mut used = 0usize;
        for (c, &g) in counts.iter_mut().zip(groups.iter()) {
            *c = g.min(MAX_INSTANCES - used);
            used += *c;
        }
        let [ni, na, nh, nqa, nqh, nac, nen, nor, ncar, nhv, ntr, nsp, nwa, nsa, nbr, npt, noc, ncp] =
            counts;
        let ring_verts = ring_decals(rings);
        let nrv = ring_verts.len().min(MAX_RING_VERTS);
        let mut light_pos = [[0.0f32; 4]; MAX_LIGHTS];
        let mut light_col = [[0.0f32; 4]; MAX_LIGHTS];
        let nlights = fx_lights.len().min(MAX_LIGHTS);
        for (k, l) in fx_lights.iter().take(nlights).enumerate() {
            light_pos[k] = [l.pos[0], l.pos[1], l.pos[2], l.radius];
            light_col[k] = [l.color[0], l.color[1], l.color[2], 0.0];
        }
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj,
                eye: [eye[0], eye[1], eye[2], 1.0],
                light_dir: [0.5, 1.0, 0.35, 0.0],
                params: [time, terrain::HALF, terrain::SEA_LEVEL, nlights as f32],
                light_pos,
                light_col,
            }),
        );
        if fow.len() == FOW_RES * FOW_RES {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.fow_tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                fow,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(FOW_RES as u32),
                    rows_per_image: Some(FOW_RES as u32),
                },
                wgpu::Extent3d {
                    width: FOW_RES as u32,
                    height: FOW_RES as u32,
                    depth_or_array_layers: 1,
                },
            );
        }
        let stride = std::mem::size_of::<InstanceRaw>() as u64;
        let slices = [
            &infantry[..ni],
            &barracks_astro[..na],
            &barracks_hollow[..nh],
            &hq_astro[..nqa],
            &hq_hollow[..nqh],
            &acolytes[..nac],
            &engineers[..nen],
            &ore_nodes[..nor],
            &carbon_nodes[..ncar],
            &heavies[..nhv],
            &turrets[..ntr],
            &supplies[..nsp],
            &wards_astro[..nwa],
            &supplies_astro[..nsa],
            &barrels[..nbr],
            &particles[..npt],
            &ore_crystals[..noc],
            &carbon_pools[..ncp],
        ];
        let mut off = 0u64;
        for s in slices {
            if !s.is_empty() {
                self.queue
                    .write_buffer(&self.instance_buf, off * stride, bytemuck::cast_slice(s));
            }
            off += s.len() as u64;
        }
        if nrv > 0 {
            self.queue
                .write_buffer(&self.ring_buf, 0, bytemuck::cast_slice(&ring_verts[..nrv]));
        }

        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("enc") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.02,
                            g: 0.03,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.camera_bind, &[]);
            // The unit pipeline's surface-detail map rides at group 3 for
            // the whole pass; other pipelines simply ignore it.
            pass.set_bind_group(3, &self.detail_bind, &[]);

            let voxel = self.voxel_terrain.is_some();
            if let Some((buf, len, bind)) = self.voxel_terrain.as_ref() {
                // Marching-cubes battlefield: triplanar-textured, tinted per world.
                pass.set_pipeline(&self.voxel_pipeline);
                pass.set_bind_group(1, bind, &[]);
                pass.set_bind_group(2, &self.world_bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..*len, 0..1);
            } else {
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(1, &self.terrain_bind, &[]);
                pass.set_vertex_buffer(0, self.terrain_vbuf.slice(..));
                pass.set_index_buffer(self.terrain_ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.terrain_indices, 0, 0..1);
            }

            pass.set_pipeline(&self.unit_pipeline);
            // Map-rim walls hide under-ocean at the edges; only the Earthlike map
            // has an ocean plane, so skip them on the airless voxel worlds.
            if !voxel {
                pass.set_vertex_buffer(0, self.walls_buf.slice(..));
                pass.set_vertex_buffer(1, self.wall_inst_buf.slice(..));
                pass.draw(0..self.walls_len, 0..1);
            }
            if used > 0 {
                pass.set_vertex_buffer(1, self.instance_buf.slice(..));
                // (mesh vertex buffer, mesh vertex count, instance count) per group,
                // drawn over consecutive instance ranges matching the packing above.
                let meshes = [
                    (&self.infantry_buf, self.infantry_len, ni),
                    (&self.barracks_astro_buf, self.barracks_astro_len, na),
                    (&self.barracks_hollow_buf, self.barracks_hollow_len, nh),
                    (&self.hq_astro_buf, self.hq_astro_len, nqa),
                    (&self.hq_hollow_buf, self.hq_hollow_len, nqh),
                    (&self.acolyte_buf, self.acolyte_len, nac),
                    (&self.engineer_buf, self.engineer_len, nen),
                    (&self.ore_node_buf, self.ore_node_len, nor),
                    (&self.carbon_node_buf, self.carbon_node_len, ncar),
                    (&self.heavy_buf, self.heavy_len, nhv),
                    (&self.turret_buf, self.turret_len, ntr),
                    (&self.supply_buf, self.supply_len, nsp),
                    (&self.ward_astro_buf, self.ward_astro_len, nwa),
                    (&self.supply_astro_buf, self.supply_astro_len, nsa),
                    (&self.barrel_buf, self.barrel_len, nbr),
                    (&self.particle_buf, self.particle_len, npt),
                ];
                let mut base = 0u32;
                for (buf, vlen, count) in meshes {
                    let count = count as u32;
                    if count > 0 {
                        pass.set_vertex_buffer(0, buf.slice(..));
                        pass.draw(0..vlen, base..base + count);
                    }
                    base += count;
                }
            }

            // Liquid: the Earthlike ocean plane, or a voxel world's per-column
            // liquid surface (Earth seas / Titan methane). Revamped water shader.
            pass.set_pipeline(&self.water_pipeline);
            pass.set_bind_group(2, &self.world_bind, &[]);
            if let Some((wbuf, wlen)) = self.voxel_water.as_ref() {
                let bind = self
                    .voxel_terrain
                    .as_ref()
                    .map(|t| &t.2)
                    .unwrap_or(&self.terrain_bind);
                pass.set_bind_group(1, bind, &[]);
                pass.set_vertex_buffer(0, wbuf.slice(..));
                pass.draw(0..*wlen, 0..1);
            } else if !voxel {
                pass.set_bind_group(1, &self.terrain_bind, &[]);
                pass.set_vertex_buffer(0, self.water_buf.slice(..));
                pass.draw(0..self.water_len, 0..1);
            }

            if nrv > 0 {
                pass.set_pipeline(&self.ring_pipeline);
                pass.set_vertex_buffer(0, self.ring_buf.slice(..));
                pass.draw(0..nrv as u32, 0..1);
            }

            // Crystals and gas pools last: alpha-blended over the opaque
            // world, water and ground decals. Their instances sit at the
            // tail of the shared buffer (slot 1 is still bound).
            if noc + ncp > 0 {
                pass.set_pipeline(&self.crystal_pipeline);
                let opaque: u32 = (ni
                    + na
                    + nh
                    + nqa
                    + nqh
                    + nac
                    + nen
                    + nor
                    + ncar
                    + nhv
                    + ntr
                    + nsp
                    + nbr
                    + npt) as u32;
                if noc > 0 {
                    pass.set_vertex_buffer(0, self.ore_crystal_buf.slice(..));
                    pass.draw(0..self.ore_crystal_len, opaque..opaque + noc as u32);
                }
                if ncp > 0 {
                    pass.set_vertex_buffer(0, self.carbon_pool_buf.slice(..));
                    let b = opaque + noc as u32;
                    pass.draw(0..self.carbon_pool_len, b..b + ncp as u32);
                }
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_mesh(verts: &[UnitVertex], name: &str) {
        assert!(!verts.is_empty(), "{name} mesh is empty");
        assert_eq!(verts.len() % 3, 0, "{name} mesh isn't whole triangles");
        for (k, v) in verts.iter().enumerate() {
            assert!(
                v.pos.iter().all(|c| c.is_finite()),
                "{name}[{k}] non-finite position"
            );
            let n = v.normal;
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            assert!(
                (len - 1.0).abs() < 1e-3,
                "{name}[{k}] normal not unit length ({len})"
            );
            assert!(
                v.color[3] == 0.0 || v.color[3] == 1.0,
                "{name}[{k}] team weight must be 0 or 1"
            );
            assert!(
                v.detail == DETAIL_FLAT || v.detail == DETAIL_ROCK || v.detail == DETAIL_PLATE,
                "{name}[{k}] bad detail selector"
            );
        }
    }

    #[test]
    fn unit_meshes_are_well_formed() {
        check_mesh(&infantry_mesh(), "infantry");
        check_mesh(&barracks_mesh_astro(), "barracks-astro");
        check_mesh(&barracks_mesh_hollow(), "barracks-hollow");
        check_mesh(&hq_mesh_astro(), "hq-astro");
        check_mesh(&hq_mesh_hollow(), "hq-hollow");
        check_mesh(&acolyte_mesh(), "acolyte");
        check_mesh(&engineer_mesh(), "engineer");
        check_mesh(&ore_node_mesh(), "ore-node");
        check_mesh(&carbon_node_mesh(), "carbon-node");
        check_mesh(&ore_crystal_mesh(), "ore-crystal");
        check_mesh(&carbon_pool_mesh(), "carbon-pool");
        check_mesh(&barrel_mesh(), "barrel");
        check_mesh(&turret_mesh(), "turret");
        check_mesh(&supply_mesh(), "supply");
        check_mesh(&ward_mesh_astro(), "ward-astro");
        check_mesh(&supply_mesh_astro(), "supply-astro");
        check_mesh(&particle_mesh(), "particle");
        check_mesh(&heavy_mesh(), "heavy");
    }

    /// Offline mesh preview: rasterizes a mesh with the game's iso camera
    /// (yaw 45, pitch 0.95) and the unit shader's lighting, so the PNG shows
    /// exactly what the renderer would draw. CPU-only (no GPU needed).
    fn rasterize(mesh: &[UnitVertex], team: [f32; 3], w: u32, h: u32) -> image::RgbaImage {
        let (yaw, pitch) = (std::f32::consts::FRAC_PI_4, 0.95_f32);
        // The camera's eye direction; screen axes are perpendicular to it.
        let eye = [
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos(),
        ];
        let right = [-yaw.sin(), 0.0, yaw.cos()];
        let up = [
            right[1] * eye[2] - right[2] * eye[1],
            right[2] * eye[0] - right[0] * eye[2],
            right[0] * eye[1] - right[1] * eye[0],
        ];
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let project = |p: [f32; 3]| (dot(p, right), dot(p, up), dot(p, eye));

        // Fit the mesh into the image with a margin.
        let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for v in mesh {
            let (u, vv, _) = project(v.pos);
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(vv);
            v1 = v1.max(vv);
        }
        let scale = ((w as f32 - 60.0) / (u1 - u0)).min((h as f32 - 60.0) / (v1 - v0));
        let to_px = |u: f32, v: f32| {
            (
                (u - u0) * scale + (w as f32 - (u1 - u0) * scale) * 0.5,
                // v points up; image y points down.
                (v1 - v) * scale + (h as f32 - (v1 - v0) * scale) * 0.5,
            )
        };

        // Same lighting as fs_unit: albedo * (0.45 + 0.7 * n.l).
        let ll = (0.5_f32 * 0.5 + 1.0 + 0.35 * 0.35).sqrt();
        let light = [0.5 / ll, 1.0 / ll, 0.35 / ll];

        // The shader's surface-detail map, decoded sRGB -> linear and
        // triplanar-sampled exactly like fs_unit's `detail_at`, so the
        // preview shows the textured models the renderer draws.
        let det_img =
            image::load_from_memory(include_bytes!("../../../assets/textures/detail.png"))
                .expect("decode detail")
                .to_rgba8();
        let (dw, dh) = (det_img.width() as usize, det_img.height() as usize);
        let det_lin: Vec<[f32; 2]> = det_img
            .pixels()
            .map(|p| {
                [
                    (p[0] as f32 / 255.0).powf(2.2),
                    (p[1] as f32 / 255.0).powf(2.2),
                ]
            })
            .collect();
        let det_sample = |u: f32, v: f32, ch: usize| -> f32 {
            let fx = u.rem_euclid(1.0) * dw as f32;
            let fy = v.rem_euclid(1.0) * dh as f32;
            let (x0, y0) = (fx as usize % dw, fy as usize % dh);
            let (x1, y1) = ((x0 + 1) % dw, (y0 + 1) % dh);
            let (tx, ty) = (fx.fract(), fy.fract());
            let at = |x: usize, y: usize| det_lin[y * dw + x][ch];
            let a = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
            let b = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
            a + (b - a) * ty
        };
        let detail_at = |p: [f32; 3], n: [f32; 3], ch: usize| -> f32 {
            let mut wgt = [n[0].abs().powi(4), n[1].abs().powi(4), n[2].abs().powi(4)];
            let sum = (wgt[0] + wgt[1] + wgt[2]).max(0.001);
            for v in &mut wgt {
                *v /= sum;
            }
            let s = 0.45;
            wgt[0] * det_sample(p[2] * s, p[1] * s, ch)
                + wgt[1] * det_sample(p[0] * s, p[2] * s, ch)
                + wgt[2] * det_sample(p[0] * s, p[1] * s, ch)
        };

        let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 14, 24, 255]));
        let mut depth = vec![f32::MIN; (w * h) as usize];
        for tri in mesh.chunks_exact(3) {
            let albedo = |v: &UnitVertex| {
                [
                    v.color[0] + (team[0] - v.color[0]) * v.color[3],
                    v.color[1] + (team[1] - v.color[1]) * v.color[3],
                    v.color[2] + (team[2] - v.color[2]) * v.color[3],
                ]
            };
            let shade = 0.45 + 0.7 * dot(tri[0].normal, light).max(0.0);
            let base = albedo(&tri[0]);
            let p: Vec<(f32, f32, f32)> = tri
                .iter()
                .map(|v| {
                    let (u, vv, d) = project(v.pos);
                    let (x, y) = to_px(u, vv);
                    (x, y, d)
                })
                .collect();
            let area =
                (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
            if area.abs() < 1e-6 {
                continue;
            }
            let xmin = p.iter().map(|q| q.0).fold(f32::MAX, f32::min).max(0.0) as u32;
            let xmax = (p
                .iter()
                .map(|q| q.0)
                .fold(f32::MIN, f32::max)
                .min(w as f32 - 1.0)) as u32;
            let ymin = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).max(0.0) as u32;
            let ymax = (p
                .iter()
                .map(|q| q.1)
                .fold(f32::MIN, f32::max)
                .min(h as f32 - 1.0)) as u32;
            for py in ymin..=ymax {
                for px in xmin..=xmax {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = ((p[1].0 - fx) * (p[2].1 - fy) - (p[2].0 - fx) * (p[1].1 - fy)) / area;
                    let w1 = ((p[2].0 - fx) * (p[0].1 - fy) - (p[0].0 - fx) * (p[2].1 - fy)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let d = w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2;
                    let i = (py * w + px) as usize;
                    if d > depth[i] {
                        depth[i] = d;
                        // Interpolate the model-space position and apply the
                        // selected detail channel per pixel, like the shader
                        // (selector 0 stays flat shaded).
                        let det = if tri[0].detail < 0.5 {
                            1.0
                        } else {
                            let pos = [
                                w0 * tri[0].pos[0] + w1 * tri[1].pos[0] + w2 * tri[2].pos[0],
                                w0 * tri[0].pos[1] + w1 * tri[1].pos[1] + w2 * tri[2].pos[1],
                                w0 * tri[0].pos[2] + w1 * tri[1].pos[2] + w2 * tri[2].pos[2],
                            ];
                            let ch = if tri[0].detail > 1.5 { 0 } else { 1 };
                            0.70 + 1.40 * detail_at(pos, tri[0].normal, ch)
                        };
                        let rgb = [
                            ((base[0] * shade * det).clamp(0.0, 1.0) * 255.0) as u8,
                            ((base[1] * shade * det).clamp(0.0, 1.0) * 255.0) as u8,
                            ((base[2] * shade * det).clamp(0.0, 1.0) * 255.0) as u8,
                        ];
                        img.put_pixel(px, py, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
                    }
                }
            }
        }
        img
    }

    /// Renders a preview PNG of every building mesh to `target/previews/`.
    /// A dev tool, not a check: run on demand with
    /// `cargo test -p client render_building_previews -- --ignored`.
    #[test]
    #[ignore = "writes preview PNGs to target/previews; run on demand"]
    fn render_building_previews() {
        let team = [0.25, 0.55, 1.0]; // the player's blue
        let mut ore = ore_node_mesh();
        ore.extend(ore_crystal_mesh());
        let mut carbon = carbon_node_mesh();
        carbon.extend(carbon_pool_mesh());
        let jobs: [(&str, Vec<UnitVertex>); 10] = [
            ("hq-spire-astromancer", hq_mesh_astro()),
            ("hq-command-hollowmen", hq_mesh_hollow()),
            ("barracks-astromancer", barracks_mesh_astro()),
            ("barracks-hollowmen", barracks_mesh_hollow()),
            ("turret", turret_mesh()),
            ("supply-depot", supply_mesh()),
            ("worker-acolyte", acolyte_mesh()),
            ("worker-engineer", engineer_mesh()),
            ("ore-node", ore),
            ("carbon-node", carbon),
        ];
        std::fs::create_dir_all("target/previews").unwrap();
        for (name, mesh) in jobs {
            let img = rasterize(&mesh, team, 560, 640);
            img.save(format!("target/previews/{name}.png")).unwrap();
        }
    }

    /// Renders the Astromancer concept lineup (Spire, Sanctum, Ward,
    /// Acolyte side by side over the faction palette) to
    /// `target/previews/astromancers-concept.png`. A dev tool for
    /// `docs/factions/astromancers.md`; run on demand with
    /// `cargo test -p client render_astromancer_concept -- --ignored`.
    #[test]
    #[ignore = "writes the concept sheet to target/previews; run on demand"]
    fn render_astromancer_concept() {
        let team = [0.25, 0.55, 1.0];
        let jobs: [(Vec<UnitVertex>, u32); 4] = [
            (hq_mesh_astro(), 360),
            (barracks_mesh_astro(), 330),
            (turret_mesh(), 250),
            (acolyte_mesh(), 210),
        ];
        let (w, h) = (1280u32, 580u32);
        let mut sheet = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 14, 24, 255]));
        let mut x = 24u32;
        for (mesh, size) in jobs {
            let img = rasterize(&mesh, team, size, 440);
            for (px, py, p) in img.enumerate_pixels() {
                if p[3] > 0 && x + px < w {
                    sheet.put_pixel(x + px, 30 + py, *p);
                }
            }
            x += size + 24;
        }
        // The faction palette: porcelain shell, indigo shadow, aether cyan,
        // auric gold, team accent.
        let swatches = [
            [233u8, 229, 222],
            [26, 32, 54],
            [140, 230, 255],
            [196, 160, 84],
            [64, 140, 255],
        ];
        for (i, c) in swatches.iter().enumerate() {
            for yy in 0..48u32 {
                for xx in 0..110u32 {
                    let sx = 24 + i as u32 * 122 + xx;
                    if sx < w {
                        sheet.put_pixel(sx, h - 72 + yy, image::Rgba([c[0], c[1], c[2], 255]));
                    }
                }
            }
        }
        std::fs::create_dir_all("target/previews").unwrap();
        sheet
            .save("target/previews/astromancers-concept.png")
            .unwrap();
    }

    /// Renders a small icon PNG of every unit and building mesh to
    /// `target/previews/icons/`. A dev tool, not a check: run on demand with
    /// `cargo test -p client render_unit_icons -- --ignored` and copy the
    /// output to `assets/icons/` (embedded by the HUD command card).
    #[test]
    #[ignore = "writes icon PNGs to target/previews/icons; run on demand"]
    fn render_unit_icons() {
        let team = [0.25, 0.55, 1.0]; // the player's blue
        let jobs: [(&str, Vec<UnitVertex>); 10] = [
            ("hq-astromancer", hq_mesh_astro()),
            ("hq-hollowmen", hq_mesh_hollow()),
            ("barracks-astromancer", barracks_mesh_astro()),
            ("barracks-hollowmen", barracks_mesh_hollow()),
            ("turret", turret_mesh()),
            ("supply", supply_mesh()),
            ("worker-acolyte", acolyte_mesh()),
            ("worker-engineer", engineer_mesh()),
            ("infantry", infantry_mesh()),
            ("heavy", heavy_mesh()),
        ];
        std::fs::create_dir_all("target/previews/icons").unwrap();
        for (name, mesh) in jobs {
            let img = rasterize(&mesh, team, 96, 96);
            img.save(format!("target/previews/icons/{name}.png"))
                .unwrap();
        }
    }

    /// Renders the full model lineup (Astromancer row, Hollowmen row, shared
    /// units + neutral nodes row) to `target/previews/fidelity-sheet.png`.
    /// A dev tool for design review; run on demand with
    /// `cargo test -p client render_fidelity_sheet -- --ignored`.
    #[test]
    #[ignore = "writes the fidelity sheet to target/previews; run on demand"]
    fn render_fidelity_sheet() {
        let team = [0.25, 0.55, 1.0]; // the player's blue
        let mut ore = ore_node_mesh();
        ore.extend(ore_crystal_mesh());
        let mut carbon = carbon_node_mesh();
        carbon.extend(carbon_pool_mesh());
        let rows: [Vec<(Vec<UnitVertex>, u32)>; 3] = [
            vec![
                (hq_mesh_astro(), 330),
                (barracks_mesh_astro(), 300),
                (ward_mesh_astro(), 240),
                (supply_mesh_astro(), 240),
                (acolyte_mesh(), 170),
            ],
            vec![
                (hq_mesh_hollow(), 330),
                (barracks_mesh_hollow(), 300),
                (turret_mesh(), 240),
                (supply_mesh(), 240),
                (engineer_mesh(), 170),
            ],
            vec![
                (infantry_mesh(), 190),
                (heavy_mesh(), 230),
                (ore, 300),
                (carbon, 300),
                (barrel_mesh(), 130),
            ],
        ];
        let row_h = 430u32;
        let (w, h) = (1500u32, 24 + 3 * (row_h + 24));
        let mut sheet = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 14, 24, 255]));
        for (r, row) in rows.iter().enumerate() {
            let y0 = 24 + r as u32 * (row_h + 24);
            let mut x = 24u32;
            for (mesh, size) in row {
                let img = rasterize(mesh, team, *size, row_h);
                for (px, py, p) in img.enumerate_pixels() {
                    if p[3] > 0 && x + px < w && y0 + py < h {
                        sheet.put_pixel(x + px, y0 + py, *p);
                    }
                }
                x += size + 24;
            }
        }
        std::fs::create_dir_all("target/previews").unwrap();
        sheet.save("target/previews/fidelity-sheet.png").unwrap();
    }
}
