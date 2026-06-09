//! Procedural battlefield recipes for the Solar System bodies the game can be
//! fought over (the Moon, Mars, the major moons, Pluto, ...). This is the
//! engine-side mirror of `assets/worldgen/worlds.py`: the same terrain knobs
//! (relief, cratering, grooves, rifts, dunes, calderas, cantaloupe, plains) feed
//! a height field the renderer sits the map on.
//!
//! Presentation-only `f32` (see `terrain.rs`); nothing here touches the
//! deterministic sim. The matching texture sets live in
//! `assets/textures/worlds/<archetype>/` and the previews in
//! `docs/worldgen/previews/`.
//!
//! The default battlefield is the Earthlike map in `terrain.rs`. To play on one
//! of these worlds instead, set [`ACTIVE`] to its index in [`WORLDS`].

use crate::terrain::HALF;

/// World units of elevation per unit of (normalized) relief.
const AMP: f32 = 42.0;

/// Terrain knobs for one world. Numeric only, so every field is consumed by
/// [`height`] and the table stays a plain `const`.
#[derive(Clone, Copy)]
pub struct WorldDef {
    pub seed: u32,
    pub relief: f32,
    pub roughness: f32,
    pub warp: f32,
    pub crater_density: f32,
    pub crater_min: f32,
    pub crater_max: f32,
    pub smoothness: f32,
    pub grooves: f32,
    pub groove_dir: f32,
    pub rifts: f32,
    pub dunes: f32,
    pub dune_dir: f32,
    pub calderas: f32,
    pub cantaloupe: f32,
    pub plains: f32,
    pub ridge: f32,
}

#[allow(clippy::too_many_arguments)] // a 17-column data-table row, by design
const fn world(
    seed: u32,
    relief: f32,
    roughness: f32,
    warp: f32,
    crater_density: f32,
    crater_min: f32,
    crater_max: f32,
    smoothness: f32,
    grooves: f32,
    groove_dir: f32,
    rifts: f32,
    dunes: f32,
    dune_dir: f32,
    calderas: f32,
    cantaloupe: f32,
    plains: f32,
    ridge: f32,
) -> WorldDef {
    WorldDef {
        seed,
        relief,
        roughness,
        warp,
        crater_density,
        crater_min,
        crater_max,
        smoothness,
        grooves,
        groove_dir,
        rifts,
        dunes,
        dune_dir,
        calderas,
        cantaloupe,
        plains,
        ridge,
    }
}

// Indices (also the order in docs/worldgen): 0 moon, 1 ceres, 2 vesta, 3 mars,
// 4 callisto, 5 ganymede, 6 europa, 7 io, 8 titan, 9 enceladus, 10 triton,
// 11 rhea, 12 iapetus, 13 dione, 14 titania, 15 oberon, 16 umbriel, 17 ariel,
// 18 miranda, 19 pluto, 20 chiron.
#[rustfmt::skip]
pub const WORLDS: [WorldDef; 21] = [
    //     seed relief rough warp cdens cmin  cmax  smth  groove gdir rift dune ddir cald cant plain ridge
    world(  11, 0.90, 1.0, 0.6, 1.6, 0.010, 0.090, 0.00, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // moon
    world(  23, 0.80, 1.0, 0.6, 1.4, 0.012, 0.060, 0.00, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // ceres
    world(  31, 1.20, 1.0, 0.6, 1.3, 0.012, 0.110, 0.00, 0.0, 0.2, 0.25, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // vesta
    world(  43, 1.40, 1.0, 0.6, 0.6, 0.012, 0.060, 0.15, 0.0, 0.0, 0.40, 0.7, 0.5, 0.0, 0.0, 0.0, 0.0), // mars
    world(  51, 0.80, 1.0, 0.6, 2.2, 0.008, 0.100, 0.00, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // callisto
    world(  61, 1.00, 1.0, 0.6, 1.0, 0.012, 0.060, 0.00, 0.8, 0.6, 0.3, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // ganymede
    world(  71, 0.35, 1.0, 0.6, 0.12, 0.012, 0.060, 0.85, 0.5, 1.1, 0.9, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // europa
    world(  83, 0.70, 1.0, 0.6, 0.0, 0.012, 0.060, 0.60, 0.0, 0.0, 0.0, 0.0, 0.0, 0.9, 0.0, 0.0, 0.0), // io
    world(  97, 0.50, 1.0, 0.6, 0.10, 0.012, 0.060, 0.55, 0.0, 0.0, 0.2, 0.7, 0.1, 0.0, 0.0, 0.0, 0.0), // titan
    world( 109, 0.45, 1.0, 0.6, 0.30, 0.012, 0.060, 0.70, 0.0, 0.0, 0.6, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // enceladus
    world( 127, 0.50, 1.0, 0.6, 0.15, 0.012, 0.060, 0.60, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.85, 0.30, 0.0), // triton
    world( 131, 0.80, 1.0, 0.6, 1.8, 0.012, 0.090, 0.00, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // rhea
    world( 139, 1.10, 1.0, 0.6, 1.6, 0.012, 0.060, 0.00, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.9), // iapetus
    world( 149, 0.80, 1.0, 0.6, 1.5, 0.012, 0.060, 0.00, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // dione
    world( 151, 1.00, 1.0, 0.6, 1.2, 0.012, 0.060, 0.00, 0.0, 0.9, 0.6, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // titania
    world( 157, 1.10, 1.0, 0.6, 1.5, 0.012, 0.100, 0.00, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // oberon
    world( 163, 0.80, 1.0, 0.6, 1.4, 0.012, 0.060, 0.10, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // umbriel
    world( 167, 0.90, 1.0, 0.6, 0.7, 0.012, 0.060, 0.30, 0.6, 1.3, 0.85, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // ariel
    world( 173, 1.50, 1.0, 0.6, 0.9, 0.012, 0.060, 0.00, 0.9, 0.4, 0.9, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // miranda
    world( 181, 1.00, 1.0, 0.6, 0.5, 0.012, 0.060, 0.20, 0.0, 0.0, 0.3, 0.0, 0.0, 0.0, 0.0, 0.7, 0.0), // pluto
    world( 191, 0.90, 1.0, 0.6, 1.0, 0.012, 0.120, 0.00, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0), // chiron
];

/// Index into [`WORLDS`] of the active battlefield, or out of range (the default)
/// to keep the Earthlike map in `terrain.rs`. Flip this to e.g. `7` to fight on Io.
pub const ACTIVE: usize = usize::MAX;

/// The active world, if [`ACTIVE`] selects one (otherwise the Earthlike default).
pub fn active() -> Option<&'static WorldDef> {
    WORLDS.get(ACTIVE)
}

// --- seeded value noise (self-contained; the live map keeps terrain.rs's) ----
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut n = (x.wrapping_mul(374_761_393) ^ y.wrapping_mul(668_265_263)) as u32;
    n ^= seed.wrapping_mul(362_437);
    n = (n ^ (n >> 13)).wrapping_mul(1_274_126_177);
    ((n ^ (n >> 16)) & 0xffff) as f32 / 65535.0
}

fn vnoise(x: f32, y: f32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - ix as f32, y - iy as f32);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let a = hash(ix, iy, seed);
    let b = hash(ix + 1, iy, seed);
    let c = hash(ix, iy + 1, seed);
    let d = hash(ix + 1, iy + 1, seed);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sy
}

/// fbm in roughly [0, 1].
fn fbm(x: f32, y: f32, seed: u32, octaves: u32) -> f32 {
    let mut total = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        total += vnoise(x * freq, y * freq, seed + o * 101) * amp;
        norm += amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    if norm > 0.0 {
        total / norm
    } else {
        0.0
    }
}

/// Ridged noise in [0, 1] (sharp crests) for grooves / sulci.
fn ridged(x: f32, y: f32, seed: u32) -> f32 {
    let mut total = 0.0;
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut norm = 0.0;
    for o in 0..4 {
        let n = 1.0 - (2.0 * vnoise(x * freq, y * freq, seed + o * 71) - 1.0).abs();
        total += n * n * amp;
        norm += amp;
        freq *= 2.0;
        amp *= 0.5;
    }
    if norm > 0.0 {
        total / norm
    } else {
        0.0
    }
}

fn basin(u: f32, v: f32, seed: u32) -> f32 {
    ((fbm(u * 1.6 + 5.0, v * 1.6 + 9.0, seed + 40, 3) - 0.45) / 0.35).clamp(0.0, 1.0)
}

/// Triton-style packed pits (negative: depressions).
fn cantaloupe(u: f32, v: f32, seed: u32) -> f32 {
    let k = 7.0;
    let (cu, cv) = (u * k, v * k);
    let (bi, bj) = (cu.floor() as i32, cv.floor() as i32);
    let mut best = 1e9_f32;
    for dj in -1..=1 {
        for di in -1..=1 {
            let (gx, gy) = (bi + di, bj + dj);
            let jx = gx as f32 + 0.5 + (hash(gx, gy, seed) - 0.5) * 0.7;
            let jy = gy as f32 + 0.5 + (hash(gx, gy, seed + 3) - 0.5) * 0.7;
            let d = (cu - jx).powi(2) + (cv - jy).powi(2);
            if d < best {
                best = d;
            }
        }
    }
    -(-best * 2.2).exp()
}

/// Cheap Worley-style cratering: bowls with raised rims, count gated by density.
fn craters(u: f32, v: f32, d: &WorldDef) -> f32 {
    let mut acc = 0.0;
    for o in 0..2u32 {
        let cs = 9.0 + d.crater_density * 7.0 + o as f32 * 11.0;
        let (cu, cv) = (u * cs, v * cs);
        let (bi, bj) = (cu.floor() as i32, cv.floor() as i32);
        let mut best = 9.0_f32;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (gx, gy) = (bi + di, bj + dj);
                if hash(gx, gy, d.seed + 17 + o * 97) > d.crater_density.min(1.0) {
                    continue;
                }
                let cx = gx as f32 + 0.2 + 0.6 * hash(gx, gy, d.seed + 1);
                let cy = gy as f32 + 0.2 + 0.6 * hash(gx, gy, d.seed + 2);
                let rad =
                    (d.crater_min + (d.crater_max - d.crater_min) * hash(gx, gy, d.seed + 3)) * 8.0;
                let dist = ((cu - cx).hypot(cv - cy)) / rad.max(0.05);
                if dist < best {
                    best = dist;
                }
            }
        }
        if best < 1.5 {
            let bowl = if best <= 1.0 { best * best - 1.0 } else { 0.0 };
            let rim = (-(((best - 0.92) / 0.16).powi(2))).exp();
            acc += bowl * 0.06 + rim * 0.045;
        }
    }
    acc
}

/// Surface elevation (world units) at `(x, z)` for world `d`.
pub fn height(d: &WorldDef, x: f32, z: f32) -> f32 {
    let inv = 1.0 / (2.0 * HALF);
    let u = (x + HALF) * inv;
    let v = (z + HALF) * inv;
    let fscale = 3.4 + d.roughness * 1.2;

    let wu = u + (fbm(u * 2.0, v * 2.0, d.seed + 5, 3) - 0.5) * d.warp * 0.35;
    let wv = v + (fbm(u * 2.0 + 3.0, v * 2.0 + 1.0, d.seed + 6, 3) - 0.5) * d.warp * 0.35;
    let mut h = (fbm(wu * fscale, wv * fscale, d.seed, 5) - 0.5) * 2.0;
    h *= 1.0 - d.smoothness * 0.65;

    if d.grooves > 0.0 {
        let (s, c) = d.groove_dir.sin_cos();
        let r = ridged(
            u * c * 6.0 - v * s * 6.0,
            u * s * 6.0 + v * c * 6.0,
            d.seed + 12,
        );
        h += (r - 0.45) * d.grooves * 0.7;
    }
    if d.dunes > 0.0 {
        let (s, c) = d.dune_dir.sin_cos();
        let phase = (u * c + v * s) * 46.0 + fbm(u * 3.0, v * 3.0, d.seed + 8, 2) * 5.0;
        h += phase.sin() * d.dunes * 0.16;
    }
    if d.cantaloupe > 0.0 {
        h += cantaloupe(u, v, d.seed + 14) * d.cantaloupe * 0.6;
    }
    if d.ridge > 0.0 {
        let band = (-((v - 0.5) / 0.045).powi(2)).exp();
        h += band * d.ridge * 0.9;
    }
    if d.plains > 0.0 {
        let m = basin(u, v, d.seed);
        h += (-0.15 - h) * (m * d.plains);
    }
    // `rifts` carve thin fractures; cheap to fold in as a localized depression.
    if d.rifts > 0.0 {
        let line = (1.0 - ((u * 1.7 + v * 2.3) * 6.0).sin().abs() * 3.0).max(0.0);
        h -= line * d.rifts * 0.08;
    }
    // `calderas` sink broad volcanic floors (Io's lava paterae).
    if d.calderas > 0.0 {
        let cmask = fbm(u * 5.0, v * 5.0, d.seed + 30, 3);
        if cmask > 0.62 {
            h -= (cmask - 0.62) / 0.38 * 0.12 * d.calderas;
        }
    }
    if d.crater_density > 0.0 {
        h += craters(u, v, d);
    }

    h * d.relief * AMP
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_world_height_is_finite_and_bounded() {
        for (i, d) in WORLDS.iter().enumerate() {
            let mut lo = f32::INFINITY;
            let mut hi = f32::NEG_INFINITY;
            let mut x = -HALF;
            while x <= HALF {
                let mut z = -HALF;
                while z <= HALF {
                    let h = height(d, x, z);
                    assert!(h.is_finite(), "world {i} non-finite height at ({x},{z})");
                    lo = lo.min(h);
                    hi = hi.max(h);
                    z += 40.0;
                }
                x += 40.0;
            }
            // Sane amplitude: not flat, not absurd.
            assert!(hi - lo > 1.0, "world {i} terrain is flat");
            assert!(hi - lo < 8.0 * AMP, "world {i} terrain amplitude is wild");
        }
    }

    #[test]
    fn active_defaults_to_earthlike() {
        // The default index is out of range, so the Earthlike map stays active.
        assert!(active().is_none());
    }
}
