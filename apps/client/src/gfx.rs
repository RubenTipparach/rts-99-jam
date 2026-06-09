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
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RingRaw {
    pub center: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
}

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
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct UnitVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    light_dir: [f32; 4],
    params: [f32; 4], // time, map_half, sea_level, _
}

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

/// A low-poly infantry soldier, ~2.4 units tall, facing -z.
fn infantry_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let trousers = [0.20, 0.22, 0.27];
    let leather = [0.45, 0.35, 0.27];
    let skin = [0.80, 0.62, 0.48];
    let metal = [0.54, 0.57, 0.64];
    let wood = [0.40, 0.27, 0.16];
    // Legs.
    push_box(
        &mut m,
        [-0.34, 0.0, -0.25],
        [-0.04, 0.95, 0.25],
        trousers,
        0.0,
    );
    push_box(
        &mut m,
        [0.04, 0.0, -0.25],
        [0.34, 0.95, 0.25],
        trousers,
        0.0,
    );
    // Arms (under the pauldrons).
    push_box(
        &mut m,
        [-0.5, 1.0, -0.18],
        [-0.34, 1.52, 0.18],
        leather,
        0.0,
    );
    push_box(&mut m, [0.34, 1.0, -0.18], [0.5, 1.52, 0.18], leather, 0.0);
    // Torso + shoulder pauldrons (team-colored tabard/armor).
    push_box(
        &mut m,
        [-0.42, 0.92, -0.3],
        [0.42, 1.72, 0.3],
        [0.5, 0.5, 0.5],
        1.0,
    );
    push_box(
        &mut m,
        [-0.52, 1.5, -0.28],
        [-0.32, 1.72, 0.28],
        [0.5, 0.5, 0.5],
        1.0,
    );
    push_box(
        &mut m,
        [0.32, 1.5, -0.28],
        [0.52, 1.72, 0.28],
        [0.5, 0.5, 0.5],
        1.0,
    );
    // Head + helmet + team plume.
    push_box(&mut m, [-0.22, 1.72, -0.2], [0.22, 2.12, 0.2], skin, 0.0);
    push_box(&mut m, [-0.26, 2.0, -0.24], [0.26, 2.26, 0.24], metal, 0.0);
    push_box(
        &mut m,
        [-0.05, 2.26, -0.16],
        [0.05, 2.58, 0.1],
        [0.5, 0.5, 0.5],
        1.0,
    );
    // Spear on the right side, tip above the head.
    push_box(&mut m, [0.5, 0.2, 0.0], [0.6, 2.5, 0.12], wood, 0.0);
    push_box(&mut m, [0.48, 2.46, -0.02], [0.62, 2.72, 0.14], metal, 0.0);
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
    let gold = [0.86, 0.75, 0.45];
    let rot = std::f32::consts::FRAC_PI_8; // flat face toward -z
                                           // Hanging grown root (does not touch the ground -> visible hover gap).
    push_frustum(
        &mut m, 0.0, 0.0, 4.0, 0.5, 1.6, 0.5, shell2, 0.0, 8, rot, false,
    );
    // Octagonal body, tapering, with a gold belt.
    push_prism(&mut m, 0.0, 0.0, 4.4, 1.6, 5.0, shell, 0.0, 8, rot, false);
    push_prism(&mut m, 0.0, 0.0, 4.5, 3.0, 3.5, gold, 0.0, 8, rot, false);
    push_frustum(
        &mut m, 0.0, 0.0, 4.4, 2.8, 5.0, 7.6, shell2, 0.0, 8, rot, false,
    );
    // Crowning spire + team-tinted energy core running up the middle.
    push_pyramid(&mut m, 0.0, 0.0, 2.8, 7.6, 11.6, gold, 0.0, 8, rot);
    push_prism(
        &mut m,
        0.0,
        0.0,
        1.2,
        2.2,
        8.2,
        [0.5, 0.5, 0.5],
        1.0,
        6,
        rot,
        true,
    );
    // Two hovering shards flanking the door side (+z).
    for sx in [-3.2_f32, 3.2] {
        push_prism(&mut m, sx, 3.4, 0.5, 1.4, 2.4, shell, 0.0, 6, 0.0, false);
        push_pyramid(&mut m, sx, 3.4, 0.5, 2.4, 3.4, [0.5, 0.5, 0.5], 1.0, 6, 0.0);
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
    let dark = [0.24, 0.26, 0.30];
    let haz = [0.80, 0.58, 0.20];
    let gun = [0.17, 0.19, 0.22];
    // Foundation slab + plated body.
    push_box(&mut m, [-5.6, 0.0, -4.6], [5.6, 0.5, 4.6], dark, 0.0);
    push_box(&mut m, [-5.2, 0.5, -4.2], [5.2, 4.0, 4.2], steel, 0.0);
    push_box(&mut m, [-5.2, 0.5, -4.2], [5.2, 1.0, 4.2], haz, 0.0); // hazard skirt
                                                                    // Sawtooth roof (three gables along x).
    for cx in [-3.4_f32, 0.0, 3.4] {
        push_roof(&mut m, cx, 0.0, 1.7, 4.4, 4.0, 1.4, steel2);
    }
    // Blast door + team-tinted window band on the +z face.
    push_box(&mut m, [-1.6, 0.0, 4.15], [1.6, 2.8, 4.35], gun, 0.0);
    push_box(&mut m, [-1.7, 0.2, 4.2], [1.7, 0.7, 4.4], haz, 0.0);
    push_box(
        &mut m,
        [-4.6, 2.4, 4.2],
        [-2.2, 3.2, 4.34],
        [0.5, 0.5, 0.5],
        1.0,
    );
    push_box(
        &mut m,
        [2.2, 2.4, 4.2],
        [4.6, 3.2, 4.34],
        [0.5, 0.5, 0.5],
        1.0,
    );
    // Smokestack.
    push_prism(&mut m, -4.0, -2.8, 0.7, 4.0, 7.2, steel2, 0.0, 6, 0.0, true);
    push_prism(&mut m, -4.0, -2.8, 0.72, 6.8, 7.2, dark, 0.0, 6, 0.0, true);
    // Roof turret with a barrel (the built-in gun).
    push_box(&mut m, [2.4, 4.0, -1.0], [4.0, 5.0, 0.6], gun, 0.0);
    push_box(&mut m, [2.9, 4.3, 0.5], [3.5, 4.7, 3.2], gun, 0.0);
    m
}

// Placeholder faction workers (see assets/concepts/units_resources.png). ~2.7
// tall, facing -z. Team tint rides the Astromancer focus-core and the Hollowmen
// visor/shoulder. The Acolyte is authored to sit just above y=0 and is lifted
// into a hover by the instance offset.

/// Astromancer Acolyte: a hooded caster that hovers, with a team-tinted focus
/// core and glowing eyes; grows structures and draws motes of matter.
fn acolyte_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let shell = [0.86, 0.84, 0.76];
    let shell2 = [0.78, 0.76, 0.68];
    let gold = [0.86, 0.75, 0.45];
    let eyes = [0.55, 0.92, 0.86];
    let team = [0.5, 0.5, 0.5];
    // Trailing robe point flaring up into the body.
    push_frustum(
        &mut m, 0.0, 0.0, 0.18, 0.62, 0.30, 0.95, shell2, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 0.62, 0.46, 0.95, 1.90, shell, 0.0, 6, 0.0, false,
    );
    push_box(&mut m, [-0.5, 1.3, -0.14], [0.5, 1.5, 0.14], gold, 0.0); // sash
                                                                       // Shoulder mantle + hood.
    push_frustum(
        &mut m, 0.0, 0.0, 0.46, 0.34, 1.90, 2.25, shell2, 0.0, 6, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 0.34, 0.26, 2.25, 2.55, shell, 0.0, 6, 0.0, false,
    );
    push_pyramid(&mut m, 0.0, 0.0, 0.32, 2.40, 2.95, shell2, 0.0, 6, 0.0);
    push_box(&mut m, [-0.16, 2.05, 0.22], [0.16, 2.20, 0.34], eyes, 0.0); // glowing eyes
                                                                          // Team-tinted focus core hovering at the chest.
    push_prism(&mut m, 0.0, 0.34, 0.20, 1.25, 1.70, team, 1.0, 6, 0.0, true);
    m
}

/// Hollowmen Engineer: a stocky powered-armor worker with a team-tinted visor
/// and shoulder, a hazard chest band, and a carried tool; walks, welds, drills.
fn engineer_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    let steel = [0.48, 0.51, 0.55];
    let steel2 = [0.60, 0.63, 0.67];
    let dark = [0.24, 0.26, 0.30];
    let haz = [0.80, 0.58, 0.20];
    let amber = [0.95, 0.75, 0.35];
    let gun = [0.17, 0.19, 0.22];
    let team = [0.5, 0.5, 0.5];
    // Legs + boots (neutral stance).
    push_box(&mut m, [-0.34, 0.0, -0.32], [-0.06, 0.66, 0.10], dark, 0.0);
    push_box(&mut m, [0.06, 0.0, -0.10], [0.34, 0.66, 0.32], dark, 0.0);
    push_box(&mut m, [-0.36, 0.0, -0.34], [-0.04, 0.12, 0.28], gun, 0.0);
    push_box(&mut m, [0.04, 0.0, -0.12], [0.36, 0.12, 0.50], gun, 0.0);
    // Torso + hazard chest band.
    push_box(&mut m, [-0.44, 0.66, -0.34], [0.44, 1.50, 0.34], steel, 0.0);
    push_box(&mut m, [-0.44, 1.00, -0.34], [0.44, 1.18, 0.36], haz, 0.0);
    // Backpack + glowing vent.
    push_box(&mut m, [-0.34, 0.80, -0.56], [0.34, 1.46, -0.34], dark, 0.0);
    push_box(
        &mut m,
        [-0.24, 1.20, -0.60],
        [0.24, 1.40, -0.54],
        amber,
        0.0,
    );
    // Head + team-tinted visor + crown.
    push_box(
        &mut m,
        [-0.26, 1.50, -0.24],
        [0.26, 1.98, 0.24],
        steel2,
        0.0,
    );
    push_box(&mut m, [-0.26, 1.66, 0.22], [0.26, 1.84, 0.30], team, 1.0);
    push_box(&mut m, [-0.30, 1.96, -0.26], [0.30, 2.06, 0.26], dark, 0.0);
    // Arms + carried tool + team shoulder pad.
    push_box(
        &mut m,
        [-0.62, 0.80, -0.16],
        [-0.44, 1.46, 0.16],
        steel,
        0.0,
    );
    push_box(&mut m, [0.44, 0.80, -0.16], [0.62, 1.46, 0.16], steel, 0.0);
    push_box(&mut m, [0.46, 0.74, 0.0], [0.60, 1.00, 0.50], gun, 0.0);
    push_box(&mut m, [-0.62, 1.36, -0.18], [-0.42, 1.54, 0.18], team, 1.0);
    m
}

/// Opaque dark walls around the map rim, from above the water down past the
/// seabed, so you don't see under the (translucent) water at the edges. Drawn
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

fn water_quad() -> [[f32; 3]; 6] {
    let h = terrain::HALF;
    [
        [-h, 0.0, -h],
        [h, 0.0, -h],
        [h, 0.0, h],
        [-h, 0.0, -h],
        [h, 0.0, h],
        [-h, 0.0, h],
    ]
}

/// Tessellate selection rings into ground-decal triangles whose vertices sit a
/// hair above the terrain, so the ring follows hills instead of clipping.
fn ring_decals(rings: &[RingRaw]) -> Vec<RingVertex> {
    let mut out = Vec::new();
    for r in rings.iter().take(MAX_RINGS) {
        let (cx, cz) = (r.center[0], r.center[2]);
        let (r_in, r_out) = (r.radius * 0.82, r.radius);
        let color = r.color;
        let pt = |radius: f32, c: f32, s: f32| {
            let (x, z) = (cx + c * radius, cz + s * radius);
            [x, terrain::height(x, z) + 0.25, z]
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
    ring_pipeline: wgpu::RenderPipeline,
    terrain_vbuf: wgpu::Buffer,
    terrain_ibuf: wgpu::Buffer,
    terrain_indices: u32,
    water_buf: wgpu::Buffer,
    infantry_buf: wgpu::Buffer,
    infantry_len: u32,
    barracks_astro_buf: wgpu::Buffer,
    barracks_astro_len: u32,
    barracks_hollow_buf: wgpu::Buffer,
    barracks_hollow_len: u32,
    acolyte_buf: wgpu::Buffer,
    acolyte_len: u32,
    engineer_buf: wgpu::Buffer,
    engineer_len: u32,
    walls_buf: wgpu::Buffer,
    walls_len: u32,
    wall_inst_buf: wgpu::Buffer,
    instance_buf: wgpu::Buffer,
    ring_buf: wgpu::Buffer,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    terrain_bind: wgpu::BindGroup,
    fow_tex: wgpu::Texture,
    pub width: u32,
    pub height: u32,
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

        let tile_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tile-samp"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
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
                tex_entry(0, false), // grass - nearest (PS1 look)
                tex_entry(1, false), // dirt
                tex_entry(2, false), // rock
                tex_entry(3, false), // sand
                tex_entry(4, true),  // fog of war - linear-filtered soft borders
                samp_entry(5, wgpu::SamplerBindingType::NonFiltering), // tiles
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
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 5 => Float32x4],
        };
        let pos3 = wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        };
        let inst = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32x3, 4 => Float32x4],
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
            &pl_plain,
            "vs_unit",
            "fs_unit",
            &[v3u, inst],
            &opaque_t,
            &depth_opaque,
        );
        let water_pipeline = mk(
            "water",
            &pl_tex,
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
            &depth_blend,
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
            }),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
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
            ring_pipeline,
            terrain_vbuf,
            terrain_ibuf,
            terrain_indices: ti.len() as u32,
            water_buf,
            infantry_buf,
            infantry_len: infantry.len() as u32,
            barracks_astro_buf,
            barracks_astro_len: barracks_astro.len() as u32,
            barracks_hollow_buf,
            barracks_hollow_len: barracks_hollow.len() as u32,
            acolyte_buf,
            acolyte_len: acolyte.len() as u32,
            engineer_buf,
            engineer_len: engineer.len() as u32,
            walls_buf,
            walls_len: walls.len() as u32,
            wall_inst_buf,
            instance_buf,
            ring_buf,
            camera_buf,
            camera_bind,
            terrain_bind,
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

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        infantry: &[InstanceRaw],
        barracks_astro: &[InstanceRaw],
        barracks_hollow: &[InstanceRaw],
        acolytes: &[InstanceRaw],
        engineers: &[InstanceRaw],
        rings: &[RingRaw],
        fow: &[u8],
        view_proj: [[f32; 4]; 4],
        eye: [f32; 3],
        time: f32,
    ) {
        // All meshes share one instance buffer, packed in order: infantry,
        // Astromancer buildings, Hollowmen buildings, Acolytes, Engineers. Each
        // mesh is drawn over its own contiguous range.
        let ni = infantry.len().min(MAX_INSTANCES);
        let na = barracks_astro.len().min(MAX_INSTANCES - ni);
        let nh = barracks_hollow.len().min(MAX_INSTANCES - ni - na);
        let nac = acolytes.len().min(MAX_INSTANCES - ni - na - nh);
        let nen = engineers.len().min(MAX_INSTANCES - ni - na - nh - nac);
        let ring_verts = ring_decals(rings);
        let nrv = ring_verts.len().min(MAX_RING_VERTS);
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj,
                eye: [eye[0], eye[1], eye[2], 1.0],
                light_dir: [0.5, 1.0, 0.35, 0.0],
                params: [time, terrain::HALF, terrain::SEA_LEVEL, 0.0],
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
        if ni > 0 {
            self.queue
                .write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&infantry[..ni]));
        }
        if na > 0 {
            self.queue.write_buffer(
                &self.instance_buf,
                ni as u64 * stride,
                bytemuck::cast_slice(&barracks_astro[..na]),
            );
        }
        if nh > 0 {
            self.queue.write_buffer(
                &self.instance_buf,
                (ni + na) as u64 * stride,
                bytemuck::cast_slice(&barracks_hollow[..nh]),
            );
        }
        if nac > 0 {
            self.queue.write_buffer(
                &self.instance_buf,
                (ni + na + nh) as u64 * stride,
                bytemuck::cast_slice(&acolytes[..nac]),
            );
        }
        if nen > 0 {
            self.queue.write_buffer(
                &self.instance_buf,
                (ni + na + nh + nac) as u64 * stride,
                bytemuck::cast_slice(&engineers[..nen]),
            );
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

            pass.set_pipeline(&self.terrain_pipeline);
            pass.set_bind_group(1, &self.terrain_bind, &[]);
            pass.set_vertex_buffer(0, self.terrain_vbuf.slice(..));
            pass.set_index_buffer(self.terrain_ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.terrain_indices, 0, 0..1);

            pass.set_pipeline(&self.unit_pipeline);
            // Map-rim walls (always), via an identity instance.
            pass.set_vertex_buffer(0, self.walls_buf.slice(..));
            pass.set_vertex_buffer(1, self.wall_inst_buf.slice(..));
            pass.draw(0..self.walls_len, 0..1);
            if ni > 0 || na > 0 || nh > 0 || nac > 0 || nen > 0 {
                pass.set_vertex_buffer(1, self.instance_buf.slice(..));
                if ni > 0 {
                    pass.set_vertex_buffer(0, self.infantry_buf.slice(..));
                    pass.draw(0..self.infantry_len, 0..ni as u32);
                }
                if na > 0 {
                    pass.set_vertex_buffer(0, self.barracks_astro_buf.slice(..));
                    pass.draw(0..self.barracks_astro_len, ni as u32..(ni + na) as u32);
                }
                if nh > 0 {
                    pass.set_vertex_buffer(0, self.barracks_hollow_buf.slice(..));
                    pass.draw(
                        0..self.barracks_hollow_len,
                        (ni + na) as u32..(ni + na + nh) as u32,
                    );
                }
                if nac > 0 {
                    pass.set_vertex_buffer(0, self.acolyte_buf.slice(..));
                    pass.draw(
                        0..self.acolyte_len,
                        (ni + na + nh) as u32..(ni + na + nh + nac) as u32,
                    );
                }
                if nen > 0 {
                    pass.set_vertex_buffer(0, self.engineer_buf.slice(..));
                    pass.draw(
                        0..self.engineer_len,
                        (ni + na + nh + nac) as u32..(ni + na + nh + nac + nen) as u32,
                    );
                }
            }

            pass.set_pipeline(&self.water_pipeline);
            pass.set_bind_group(1, &self.terrain_bind, &[]);
            pass.set_vertex_buffer(0, self.water_buf.slice(..));
            pass.draw(0..6, 0..1);

            if nrv > 0 {
                pass.set_pipeline(&self.ring_pipeline);
                pass.set_vertex_buffer(0, self.ring_buf.slice(..));
                pass.draw(0..nrv as u32, 0..1);
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
        }
    }

    #[test]
    fn unit_meshes_are_well_formed() {
        check_mesh(&infantry_mesh(), "infantry");
        check_mesh(&barracks_mesh_astro(), "barracks-astro");
        check_mesh(&barracks_mesh_hollow(), "barracks-hollow");
        check_mesh(&acolyte_mesh(), "acolyte");
        check_mesh(&engineer_mesh(), "engineer");
    }
}
