//! Shared terrain height field for the (large) map. Used by the renderer to
//! build the mesh and by the game to sit units on the surface. Presentation `f32`.

pub const HALF: f32 = 600.0;
pub const SEA_LEVEL: f32 = 0.0;
/// The seabed never drops below this - shallow water (~10 m deep) instead of a
/// bottomless bowl, so the floor reads cleanly under the surface.
pub const SEABED: f32 = SEA_LEVEL - 10.0;

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

/// Terrain height at world (x, z): rolling hills over a wide landmass, with the
/// outer ring dipping below sea level so water borders the map.
pub fn height(x: f32, z: f32) -> f32 {
    let hills = fbm(x * 0.012, z * 0.012) * 34.0;
    let rolling = (x * 0.004).sin() * 3.0 + (z * 0.0045).cos() * 3.0;
    let r = (x * x + z * z).sqrt();
    let edge = ((r - 520.0) / 80.0).max(0.0);
    let bowl = -edge * edge * 20.0;
    (2.5 + hills + rolling + bowl).max(SEABED)
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
