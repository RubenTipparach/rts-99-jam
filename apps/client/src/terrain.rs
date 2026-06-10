//! Shared terrain height field for the (large) map. Used by the renderer to
//! build the mesh and by the game to sit units on the surface. Presentation `f32`.
//!
//! Buildings level the ground: every structure registers a [`Pad`] that
//! flattens the surface to the pad height inside its footprint and feathers
//! back to the natural terrain over [`PAD_SKIRT`]. Pads are presentation only
//! (the sim is flat 2D); the game replaces the set via [`set_pads`] whenever
//! buildings appear or fall, and the renderer rebuilds the ground mesh.

use std::sync::RwLock;

pub const HALF: f32 = 512.0;
pub const SEA_LEVEL: f32 = 0.0;
/// The seabed never drops below this - shallow water (~10 m deep) instead of a
/// bottomless bowl, so the floor reads cleanly under the surface.
pub const SEABED: f32 = SEA_LEVEL - 10.0;

/// A levelled building pad: ground within `r` of `(x, z)` sits at exactly `h`.
#[derive(Clone, Copy, PartialEq)]
pub struct Pad {
    pub x: f32,
    pub z: f32,
    pub r: f32,
    pub h: f32,
}

/// Width of the feathered ramp from a pad's edge back to natural terrain.
pub const PAD_SKIRT: f32 = 8.0;

/// The active building pads. A static because `height` is sampled from free
/// functions all over the client (mesh build, unit placement, ray picking).
static PADS: RwLock<Vec<Pad>> = RwLock::new(Vec::new());

/// Replace the pad set. Returns true if it actually changed, so the caller
/// knows to rebuild the terrain mesh.
pub fn set_pads(pads: Vec<Pad>) -> bool {
    let mut cur = PADS.write().unwrap();
    if *cur == pads {
        return false;
    }
    *cur = pads;
    true
}

fn hash(x: i32, y: i32) -> f32 {
    let mut n = (x.wrapping_mul(1619) ^ y.wrapping_mul(31337)) as u32;
    n = (n ^ (n >> 13)).wrapping_mul(0x5bd1_e995);
    ((n ^ (n >> 15)) & 0xffff) as f32 / 65535.0
}

fn vnoise(x: f32, z: f32) -> f32 {
    let (ix, iz) = (x.floor() as i32, z.floor() as i32);
    let (fx, fz) = (x - ix as f32, z - iz as f32);
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sz = fz * fz * (3.0 - 2.0 * fz);
    let a = hash(ix, iz);
    let b = hash(ix + 1, iz);
    let c = hash(ix, iz + 1);
    let d = hash(ix + 1, iz + 1);
    let top = a + (b - a) * sx;
    let bot = c + (d - c) * sx;
    top + (bot - top) * sz
}

fn fbm(x: f32, z: f32) -> f32 {
    let mut v = 0.0;
    let mut amp = 0.5;
    let mut f = 1.0;
    for _ in 0..4 {
        v += (vnoise(x * f, z * f) - 0.5) * amp;
        f *= 2.0;
        amp *= 0.5;
    }
    v
}

/// Natural terrain height at world (x, z), ignoring building pads. The
/// default is rolling hills over a wide landmass, with the outer ring dipping
/// below sea level so water borders the map. If a Solar System world is
/// selected (`worlds::ACTIVE`), its procedural surface is used instead.
pub fn natural_height(x: f32, z: f32) -> f32 {
    if let Some(map) = crate::voxel::active() {
        return map.surface_height(x, z);
    }
    if let Some(def) = crate::worlds::active() {
        return crate::worlds::height(def, x, z);
    }
    let hills = fbm(x * 0.012, z * 0.012) * 34.0;
    let rolling = (x * 0.004).sin() * 3.0 + (z * 0.0045).cos() * 3.0;
    let r = (x * x + z * z).sqrt();
    let edge = ((r - 520.0) / 80.0).max(0.0);
    let bowl = -edge * edge * 20.0;
    (2.5 + hills + rolling + bowl).max(SEABED)
}

/// Terrain height at world (x, z): the natural surface, levelled by any
/// building pad covering the point (full flat inside the pad radius, then a
/// smooth ramp back to natural ground across [`PAD_SKIRT`]).
pub fn height(x: f32, z: f32) -> f32 {
    let mut h = natural_height(x, z);
    let pads = PADS.read().unwrap();
    for p in pads.iter() {
        let reach = p.r + PAD_SKIRT;
        let dx = x - p.x;
        let dz = z - p.z;
        let d2 = dx * dx + dz * dz;
        if d2 >= reach * reach {
            continue;
        }
        let d = d2.sqrt();
        // 1 inside the pad, smoothstep down to 0 at the skirt's outer edge.
        let t = ((d - p.r) / PAD_SKIRT).clamp(0.0, 1.0);
        let w = 1.0 - t * t * (3.0 - 2.0 * t);
        h += (p.h - h) * w;
    }
    h
}

pub fn normal(x: f32, z: f32) -> [f32; 3] {
    let e = 1.0;
    let hl = height(x - e, z);
    let hr = height(x + e, z);
    let hd = height(x, z - e);
    let hu = height(x, z + e);
    let n = glam::Vec3::new(hl - hr, 2.0 * e, hd - hu).normalize();
    [n.x, n.y, n.z]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_flatten_and_feather() {
        // A pad in the far map corner (away from anything other tests sample).
        let (px, pz, r) = (-470.0, -470.0, 9.0);
        let h = natural_height(px, pz);
        assert!(set_pads(vec![Pad { x: px, z: pz, r, h }]));
        // Dead flat across the whole pad radius.
        for (dx, dz) in [
            (0.0, 0.0),
            (r * 0.7, 0.0),
            (0.0, -r * 0.9),
            (-r * 0.5, r * 0.5),
        ] {
            assert!((height(px + dx, pz + dz) - h).abs() < 1e-4);
        }
        // Past the skirt the natural ground is untouched.
        let far = r + PAD_SKIRT + 1.0;
        assert!((height(px + far, pz) - natural_height(px + far, pz)).abs() < 1e-4);
        // Inside the skirt it blends between the two.
        let mid = px + r + PAD_SKIRT * 0.5;
        let blended = height(mid, pz);
        let (lo, hi) = if h < natural_height(mid, pz) {
            (h, natural_height(mid, pz))
        } else {
            (natural_height(mid, pz), h)
        };
        assert!(blended >= lo - 1e-4 && blended <= hi + 1e-4);
        set_pads(Vec::new());
    }
}
