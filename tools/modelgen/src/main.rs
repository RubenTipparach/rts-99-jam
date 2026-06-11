//! Model scaffolding tool: builds the game's unit/building/node meshes
//! procedurally (boxes, prisms, frustums, domes, gems) and writes them out
//! ONCE as static Wavefront OBJ/MTL assets under `assets/models/`.
//!
//! The checked-in OBJ/MTL files are the source of truth and are edited in
//! Blender (see `assets/models/README.md`); re-running this tool OVERWRITES
//! them, so only run it to scaffold a brand-new model, never to "refresh"
//! one that may have hand edits.
//!
//! Run: `cargo run -p modelgen` scaffolds every model; pass names
//! (`cargo run -p modelgen -- hound javelin`) to scaffold only those and
//! leave every other checked-in (possibly hand-edited) asset untouched.
//!
//! This is a dev tool, not part of the deterministic sim: floats are fine.

mod roster;

use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

/// A mesh vertex, mirroring the client's `UnitVertex`: `color.a` is the
/// team-tint weight; `detail` selects the surface-detail channel.
#[derive(Clone, Copy)]
struct UnitVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    detail: f32,
}

/// No surface detail: flat shaded (crystals, gas pools).
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
    // Both stages share the same n-gon orientation so the join is sealed.
    push_frustum(
        &mut m, 0.0, 0.0, 3.4, 2.6, 0.0, 1.0, rock_dk, 0.0, 9, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 2.6, 2.1, 1.0, 1.35, rock, 0.0, 9, 0.0, true,
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
    // All stages share the same n-gon orientation and exact join radii so
    // the stack is watertight; the stain band sits proud of the slope.
    push_frustum(
        &mut m, 0.0, 0.0, 4.0, 3.4, 0.0, 0.9, vent_dk, 0.0, 10, 0.0, false,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 3.4, 3.0, 0.9, 1.6, vent, 0.0, 10, 0.0, false,
    );
    // Capped: the translucent gas pool sits right over this throat, so it
    // must read as solid rock through it, not a hollow shell.
    push_frustum(
        &mut m, 0.0, 0.0, 3.0, 2.2, 1.6, 3.0, vent, 0.0, 10, 0.0, true,
    );
    push_frustum(
        &mut m, 0.0, 0.0, 2.74, 2.55, 2.30, 2.62, stain, 0.0, 10, 0.0, true,
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
/// Walk-cycle frames baked per walking unit (one full gait cycle).
const WALK_FRAMES: usize = 8;

/// Bake one walk frame by applying the gait pose to the idle mesh: geometry
/// near the ground (below y of about 1.3) swings fore-aft along z, the two
/// sides (sign of x) in counter-phase, with a small lift on the stepping
/// foot. Normals stay as authored, like the rest of the flat-shaded look.
fn bake_walk_frame(mesh: &[UnitVertex], frame: usize, amp: f32) -> Vec<UnitVertex> {
    let phase = std::f32::consts::TAU * frame as f32 / WALK_FRAMES as f32;
    mesh.iter()
        .map(|v| {
            let side = if v.pos[0] >= 0.0 {
                0.0
            } else {
                std::f32::consts::PI
            };
            let foot = (1.0 - v.pos[1] / 1.3).clamp(0.0, 1.0);
            let swing = (phase + side).sin();
            let mut p = v.pos;
            p[2] += swing * amp * foot;
            p[1] += swing.max(0.0) * amp * 0.45 * foot;
            UnitVertex { pos: p, ..*v }
        })
        .collect()
}

/// Stable material name carrying the engine flags: `_t1` marks the team
/// tint channel, `_d0/_d1/_d2` the surface-detail channel. The loader reads
/// flags from the NAME (so they survive Blender) and the color from `Kd`.
fn mat_name(c: [f32; 4], detail: f32) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let (r, g, bl) = (b(c[0]), b(c[1]), b(c[2]));
    let t = u8::from(c[3] > 0.5);
    let d = detail as u8;
    format!("m{r:02x}{g:02x}{bl:02x}_t{t}_d{d}")
}

/// Write one mesh as `<name>.obj` (+ `<name>.mtl` unless `mtllib` points at
/// a shared material file, as walk frames do), deduplicating positions and
/// normals and preserving triangle order (faces switch `usemtl` inline).
fn write_model(dir: &Path, name: &str, mtllib: &str, mesh: &[UnitVertex]) {
    let mut obj = String::new();
    let mut mtl = String::new();
    let _ = writeln!(
        obj,
        "# {name}.obj - scaffolded once by `cargo run -p modelgen`."
    );
    let _ = writeln!(
        obj,
        "# The checked-in file is the source of truth: edit it in Blender."
    );
    let _ = writeln!(
        obj,
        "# Material names carry engine flags: _t1 team tint, _d0/_d1/_d2 detail channel."
    );
    let _ = writeln!(obj, "mtllib {mtllib}.mtl");
    let _ = writeln!(obj, "o {name}");
    let bits = |p: [f32; 3]| [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
    let mut vmap: HashMap<[u32; 3], usize> = HashMap::new();
    let mut nmap: HashMap<[u32; 3], usize> = HashMap::new();
    let mut vlist: Vec<[f32; 3]> = Vec::new();
    let mut nlist: Vec<[f32; 3]> = Vec::new();
    let mut mats: Vec<(String, [f32; 3])> = Vec::new();
    let mut faces: Vec<(String, [usize; 3], [usize; 3])> = Vec::new();
    for tri in mesh.chunks_exact(3) {
        let m = mat_name(tri[0].color, tri[0].detail);
        if !mats.iter().any(|(n, _)| *n == m) {
            mats.push((
                m.clone(),
                [tri[0].color[0], tri[0].color[1], tri[0].color[2]],
            ));
        }
        let (mut vi, mut ni) = ([0usize; 3], [0usize; 3]);
        for (k, v) in tri.iter().enumerate() {
            vi[k] = *vmap.entry(bits(v.pos)).or_insert_with(|| {
                vlist.push(v.pos);
                vlist.len() - 1
            });
            ni[k] = *nmap.entry(bits(v.normal)).or_insert_with(|| {
                nlist.push(v.normal);
                nlist.len() - 1
            });
        }
        faces.push((m, vi, ni));
    }
    for p in &vlist {
        let _ = writeln!(obj, "v {:.6} {:.6} {:.6}", p[0], p[1], p[2]);
    }
    for n in &nlist {
        let _ = writeln!(obj, "vn {:.6} {:.6} {:.6}", n[0], n[1], n[2]);
    }
    let mut cur = String::new();
    for (m, vi, ni) in &faces {
        if *m != cur {
            let _ = writeln!(obj, "usemtl {m}");
            cur = m.clone();
        }
        let _ = writeln!(
            obj,
            "f {}//{} {}//{} {}//{}",
            vi[0] + 1,
            ni[0] + 1,
            vi[1] + 1,
            ni[1] + 1,
            vi[2] + 1,
            ni[2] + 1
        );
    }
    fs::write(dir.join(format!("{name}.obj")), obj).expect("write obj");
    if name == mtllib {
        let _ = writeln!(
            mtl,
            "# {name}.mtl - colors are editable; KEEP the material names,"
        );
        let _ = writeln!(mtl, "# the engine reads the _t/_d flags from them.");
        for (n, c) in &mats {
            let _ = writeln!(mtl, "newmtl {n}");
            let _ = writeln!(mtl, "Kd {:.4} {:.4} {:.4}", c[0], c[1], c[2]);
        }
        fs::write(dir.join(format!("{name}.mtl")), mtl).expect("write mtl");
    }
}

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/models");
    fs::create_dir_all(&dir).expect("create assets/models");
    let jobs: Vec<(&str, Vec<UnitVertex>)> = vec![
        ("infantry", infantry_mesh()),
        ("heavy", heavy_mesh()),
        ("acolyte", acolyte_mesh()),
        ("engineer", engineer_mesh()),
        ("hq-astro", hq_mesh_astro()),
        ("hq-hollow", hq_mesh_hollow()),
        ("barracks-astro", barracks_mesh_astro()),
        ("barracks-hollow", barracks_mesh_hollow()),
        ("turret", turret_mesh()),
        ("supply", supply_mesh()),
        ("ward-astro", ward_mesh_astro()),
        ("supply-astro", supply_mesh_astro()),
        ("ore-node", ore_node_mesh()),
        ("ore-crystal", ore_crystal_mesh()),
        ("carbon-node", carbon_node_mesh()),
        ("carbon-pool", carbon_pool_mesh()),
        ("barrel", barrel_mesh()),
        // Roster expansion (assets/concepts/roster_*.png).
        ("pyromancer", roster::pyromancer_mesh()),
        ("stormcaller", roster::stormcaller_mesh()),
        ("hex-witch", roster::hex_witch_mesh()),
        ("druid", roster::druid_mesh()),
        ("evoker", roster::evoker_mesh()),
        ("chronomancer", roster::chronomancer_mesh()),
        ("seer", roster::seer_mesh()),
        ("wisp", roster::wisp_mesh()),
        ("tempest-dais", roster::tempest_dais_mesh()),
        ("hound", roster::hound_mesh()),
        ("javelin", roster::javelin_mesh()),
        ("wrecker", roster::wrecker_mesh()),
        ("bulwark", roster::bulwark_mesh()),
        ("earthshaker", roster::earthshaker_mesh()),
        ("hailstorm", roster::hailstorm_mesh()),
        ("interceptor", roster::interceptor_mesh()),
        ("vulture", roster::vulture_mesh()),
        ("athenaeum", roster::athenaeum_mesh()),
        ("storm-ward", roster::storm_ward_mesh()),
        ("crucible", roster::crucible_mesh()),
        ("conservatory", roster::conservatory_mesh()),
        ("aerie", roster::aerie_mesh()),
        ("ley-nexus", roster::ley_nexus_mesh()),
        ("arsenal", roster::arsenal_mesh()),
        ("bunker", roster::bunker_mesh()),
        ("flak-tower", roster::flak_tower_mesh()),
        ("machine-shop", roster::machine_shop_mesh()),
        ("radar-array", roster::radar_array_mesh()),
        ("starport", roster::starport_mesh()),
        ("fusion-reactor", roster::fusion_reactor_mesh()),
        ("drydock", roster::drydock_mesh()),
        ("missile-silo", roster::missile_silo_mesh()),
    ];
    // Optional name filter: scaffold only the requested models so existing
    // (possibly hand-edited) assets are never overwritten by accident.
    let filter: Vec<String> = std::env::args().skip(1).collect();
    for f in &filter {
        assert!(jobs.iter().any(|(n, _)| n == f), "unknown model name: {f}");
    }
    let want = |name: &str| filter.is_empty() || filter.iter().any(|f| f == name);
    let mut n = 0;
    for (name, mesh) in &jobs {
        if want(name) {
            write_model(&dir, name, name, mesh);
            n += 1;
        }
    }
    // Walk-cycle keyframes for the walking units, sharing the base MTL.
    // (name, gait amplitude) - the heavy stomps wider and slower. The
    // Bulwark walks too but its barrier spans x = 0, which the naive bake
    // would shear, so its frames are authored by hand instead.
    for (name, amp) in [
        ("infantry", 0.32_f32),
        ("engineer", 0.30),
        ("heavy", 0.5),
        ("hound", 0.30),
        ("javelin", 0.36),
        ("wrecker", 0.42),
    ] {
        if !want(name) {
            continue;
        }
        let base = &jobs.iter().find(|(j, _)| *j == name).expect("walk base").1;
        for f in 0..WALK_FRAMES {
            write_model(
                &dir,
                &format!("{name}-walk-{f}"),
                name,
                &bake_walk_frame(base, f, amp),
            );
            n += 1;
        }
    }
    println!("wrote {n} models to {}", dir.display());
}
