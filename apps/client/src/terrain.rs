//! Shared terrain height field. Used by the renderer (to build the mesh) and by
//! the game (to sit units on the surface). Presentation-only `f32`.

pub const HALF: f32 = 60.0;
pub const SEA_LEVEL: f32 = 0.0;

/// Terrain height at world (x, z): a gentle island plateau that dips below sea
/// level near the edges, so water rings the map and slopes form cliffs.
pub fn height(x: f32, z: f32) -> f32 {
    let r = (x * x + z * z).sqrt();
    let bowl = (r / 52.0).powi(3) * -16.0;
    let hills = (x * 0.07).sin() * 1.3
        + (z * 0.062).cos() * 1.1
        + ((x * 0.13).sin() * (z * 0.11).cos()) * 1.7;
    bowl + hills + 1.8
}

/// Surface normal at world (x, z), via central differences.
pub fn normal(x: f32, z: f32) -> [f32; 3] {
    let e = 0.6;
    let hl = height(x - e, z);
    let hr = height(x + e, z);
    let hd = height(x, z - e);
    let hu = height(x, z + e);
    let n = glam::Vec3::new(hl - hr, 2.0 * e, hd - hu).normalize();
    [n.x, n.y, n.z]
}
