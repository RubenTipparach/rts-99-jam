//! Loader + marching-cubes mesher for the baked voxel maps in `assets/maps`.
//!
//! A map is a 3D density grid (the `.vxl` / VXL1 format written by
//! `assets/worldgen/voxel.py`): density >= [`ISO`] is solid ground. This module
//! parses it, exposes the surface height and the buildable mask the gameplay
//! layer needs, and polygonizes the iso-surface with marching cubes (the same
//! robust tetrahedral scheme as the Python previewer) for the renderer.
//!
//! Presentation-side `f32`; nothing here touches the deterministic sim. The
//! active map is chosen at runtime via [`set_active`] (from the lobby map
//! picker); with none selected the heightmap terrain in `terrain.rs` is used.

use std::cell::{Cell, RefCell};

pub const ISO: u8 = 128;

/// One terrain mesh vertex for the voxel pipeline. `mat` is the material id
/// (0 low, 1 mid, 2 high, 3 accent, 4 hazard); the shader textures from it.
#[derive(Clone, Copy)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub mat: f32,
}

pub struct VoxelGrid {
    nx: usize,
    ny: usize,
    nz: usize,
    bounds: [f32; 6], // x0,x1,y0,y1,z0,z1
    density: Vec<u8>,
    material: Vec<u8>,
    // Per-column flat/buildable mask, for future base-placement logic (and tests).
    #[allow(dead_code)]
    buildable: Vec<u8>,
    liquid: Vec<u8>, // nx*nz: liquid surface layer (j+1), 0 = dry
}

#[inline]
fn be_u16(b: &[u8], o: usize) -> u16 {
    ((b[o] as u16) << 8) | b[o + 1] as u16
}
#[inline]
fn be_i16(b: &[u8], o: usize) -> i16 {
    be_u16(b, o) as i16
}

impl VoxelGrid {
    /// Parse a VXL1 buffer (header + zlib payload).
    pub fn parse(bytes: &[u8]) -> VoxelGrid {
        assert!(&bytes[0..4] == b"VXL1", "not a VXL1 voxel map");
        let nx = be_u16(bytes, 4) as usize;
        let ny = be_u16(bytes, 6) as usize;
        let nz = be_u16(bytes, 8) as usize;
        let bounds = [
            be_i16(bytes, 10) as f32,
            be_i16(bytes, 12) as f32,
            be_i16(bytes, 14) as f32,
            be_i16(bytes, 16) as f32,
            be_i16(bytes, 18) as f32,
            be_i16(bytes, 20) as f32,
        ];
        let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&bytes[26..])
            .expect("inflate vxl payload");
        let n = nx * ny * nz;
        let cols = nx * nz;
        assert!(raw.len() >= 2 * n + cols, "vxl payload too short");
        // The liquid column layer is optional (older maps omit it).
        let liquid = if raw.len() >= 2 * n + 2 * cols {
            raw[2 * n + cols..2 * n + 2 * cols].to_vec()
        } else {
            vec![0u8; cols]
        };
        VoxelGrid {
            nx,
            ny,
            nz,
            bounds,
            density: raw[0..n].to_vec(),
            material: raw[n..2 * n].to_vec(),
            buildable: raw[2 * n..2 * n + cols].to_vec(),
            liquid,
        }
    }

    #[inline]
    fn lin(&self, i: usize, j: usize, k: usize) -> usize {
        (j * self.nz + k) * self.nx + i
    }
    #[inline]
    fn dx(&self) -> f32 {
        (self.bounds[1] - self.bounds[0]) / (self.nx as f32 - 1.0)
    }
    #[inline]
    fn dy(&self) -> f32 {
        (self.bounds[3] - self.bounds[2]) / (self.ny as f32 - 1.0)
    }
    #[inline]
    fn dz(&self) -> f32 {
        (self.bounds[5] - self.bounds[4]) / (self.nz as f32 - 1.0)
    }
    #[inline]
    fn world(&self, i: usize, j: usize, k: usize) -> [f32; 3] {
        [
            self.bounds[0] + i as f32 * self.dx(),
            self.bounds[2] + j as f32 * self.dy(),
            self.bounds[4] + k as f32 * self.dz(),
        ]
    }

    fn col_index(&self, x: f32, z: f32) -> (usize, usize) {
        let fi = ((x - self.bounds[0]) / self.dx()).round();
        let fk = ((z - self.bounds[4]) / self.dz()).round();
        let i = (fi.max(0.0) as usize).min(self.nx - 1);
        let k = (fk.max(0.0) as usize).min(self.nz - 1);
        (i, k)
    }

    /// World-space height of the topmost solid surface under `(x, z)`.
    pub fn surface_height(&self, x: f32, z: f32) -> f32 {
        let (i, k) = self.col_index(x, z);
        for j in (0..self.ny).rev() {
            if self.density[self.lin(i, j, k)] >= ISO {
                // interpolate against the air voxel above for a smooth surface
                let here = self.density[self.lin(i, j, k)] as f32;
                if j + 1 < self.ny {
                    let above = self.density[self.lin(i, j + 1, k)] as f32;
                    let t = ((ISO as f32 - here) / (above - here)).clamp(0.0, 1.0);
                    return self.world(i, j, k)[1] + t * self.dy();
                }
                return self.world(i, j, k)[1];
            }
        }
        self.bounds[2]
    }

    fn at(&self, i: i32, j: i32, k: i32) -> f32 {
        let i = i.clamp(0, self.nx as i32 - 1) as usize;
        let j = j.clamp(0, self.ny as i32 - 1) as usize;
        let k = k.clamp(0, self.nz as i32 - 1) as usize;
        self.density[self.lin(i, j, k)] as f32
    }

    /// Outward (toward air) unit normal from the density gradient at a world point.
    fn normal(&self, p: [f32; 3]) -> [f32; 3] {
        let fi = (p[0] - self.bounds[0]) / self.dx();
        let fj = (p[1] - self.bounds[2]) / self.dy();
        let fk = (p[2] - self.bounds[4]) / self.dz();
        let (i, j, k) = (fi.round() as i32, fj.round() as i32, fk.round() as i32);
        let gx = self.at(i + 1, j, k) - self.at(i - 1, j, k);
        let gy = self.at(i, j + 1, k) - self.at(i, j - 1, k);
        let gz = self.at(i, j, k + 1) - self.at(i, j, k - 1);
        let l = (gx * gx + gy * gy + gz * gz).sqrt();
        if l > 1e-5 {
            [-gx / l, -gy / l, -gz / l]
        } else {
            [0.0, 1.0, 0.0]
        }
    }

    /// Marching cubes (tetrahedral) -> a triangle soup for the voxel pipeline.
    /// Each vertex carries the cell's material id; the shader does the texturing.
    pub fn build_mesh(&self) -> Vec<MeshVertex> {
        // Cube corners and the six tetrahedra around the 0-6 diagonal.
        const C: [[usize; 3]; 8] = [
            [0, 0, 0],
            [1, 0, 0],
            [1, 1, 0],
            [0, 1, 0],
            [0, 0, 1],
            [1, 0, 1],
            [1, 1, 1],
            [0, 1, 1],
        ];
        const TETS: [[usize; 4]; 6] = [
            [0, 1, 2, 6],
            [0, 2, 3, 6],
            [0, 3, 7, 6],
            [0, 7, 4, 6],
            [0, 4, 5, 6],
            [0, 5, 1, 6],
        ];
        let iso = ISO as f32;
        let mut out: Vec<MeshVertex> = Vec::new();

        let interp = |pa: [f32; 3], va: f32, pb: [f32; 3], vb: f32| -> [f32; 3] {
            let t = if (vb - va).abs() < 1e-6 {
                0.5
            } else {
                ((iso - va) / (vb - va)).clamp(0.0, 1.0)
            };
            [
                pa[0] + (pb[0] - pa[0]) * t,
                pa[1] + (pb[1] - pa[1]) * t,
                pa[2] + (pb[2] - pa[2]) * t,
            ]
        };

        let emit = |p: [f32; 3], mat: f32, out: &mut Vec<MeshVertex>| {
            out.push(MeshVertex {
                pos: p,
                normal: self.normal(p),
                mat,
            });
        };

        for k in 0..self.nz - 1 {
            for j in 0..self.ny - 1 {
                for i in 0..self.nx - 1 {
                    let mut cv = [0.0f32; 8];
                    let mut cp = [[0.0f32; 3]; 8];
                    let (mut mn, mut mx) = (255.0f32, 0.0f32);
                    for (c, off) in C.iter().enumerate() {
                        let v = self.density[self.lin(i + off[0], j + off[1], k + off[2])] as f32;
                        cv[c] = v;
                        cp[c] = self.world(i + off[0], j + off[1], k + off[2]);
                        mn = mn.min(v);
                        mx = mx.max(v);
                    }
                    if mn >= iso || mx < iso {
                        continue;
                    }
                    let matf = self.material[self.lin(i, j, k)].min(4) as f32;
                    for tet in TETS.iter() {
                        let mut solids = [0usize; 4];
                        let mut ns = 0;
                        let mut airs = [0usize; 4];
                        let mut na = 0;
                        for &c in tet.iter() {
                            if cv[c] >= iso {
                                solids[ns] = c;
                                ns += 1;
                            } else {
                                airs[na] = c;
                                na += 1;
                            }
                        }
                        if ns == 0 || ns == 4 {
                            continue;
                        }
                        if ns == 1 || ns == 3 {
                            let odd = if ns == 1 { solids[0] } else { airs[0] };
                            let rest: Vec<usize> =
                                tet.iter().copied().filter(|&c| c != odd).collect();
                            let a = interp(cp[odd], cv[odd], cp[rest[0]], cv[rest[0]]);
                            let b = interp(cp[odd], cv[odd], cp[rest[1]], cv[rest[1]]);
                            let c = interp(cp[odd], cv[odd], cp[rest[2]], cv[rest[2]]);
                            emit(a, matf, &mut out);
                            emit(b, matf, &mut out);
                            emit(c, matf, &mut out);
                        } else {
                            let (s0, s1) = (solids[0], solids[1]);
                            let (a0, a1) = (airs[0], airs[1]);
                            let pa = interp(cp[s0], cv[s0], cp[a0], cv[a0]);
                            let pb = interp(cp[s0], cv[s0], cp[a1], cv[a1]);
                            let pc = interp(cp[s1], cv[s1], cp[a1], cv[a1]);
                            let pd = interp(cp[s1], cv[s1], cp[a0], cv[a0]);
                            emit(pa, matf, &mut out);
                            emit(pb, matf, &mut out);
                            emit(pc, matf, &mut out);
                            emit(pa, matf, &mut out);
                            emit(pc, matf, &mut out);
                            emit(pd, matf, &mut out);
                        }
                    }
                }
            }
        }
        out
    }

    /// A flat quad per liquid column at its surface height: the water/methane
    /// mesh, rendered by the water pipeline. Empty if the world has no liquid.
    pub fn liquid_mesh(&self) -> Vec<[f32; 3]> {
        let mut out = Vec::new();
        let dx = self.dx();
        let dz = self.dz();
        for k in 0..self.nz {
            for i in 0..self.nx {
                let lj = self.liquid[k * self.nx + i];
                if lj == 0 {
                    continue;
                }
                let p = self.world(i, (lj - 1) as usize, k);
                let (x0, y, z0) = (p[0], p[1], p[2]);
                let (x1, z1) = (x0 + dx, z0 + dz);
                let a = [x0, y, z0];
                let b = [x1, y, z0];
                let c = [x1, y, z1];
                let d = [x0, y, z1];
                for v in [a, b, c, a, c, d] {
                    out.push(v);
                }
            }
        }
        out
    }
}

// --- baked maps, embedded for both native and wasm (no fs at runtime) ---------
// Order matches docs/worldgen and `worlds::WORLDS`.
const MAPS: [&[u8]; 22] = [
    include_bytes!("../../../assets/maps/moon.vxl"),
    include_bytes!("../../../assets/maps/ceres.vxl"),
    include_bytes!("../../../assets/maps/vesta.vxl"),
    include_bytes!("../../../assets/maps/mars.vxl"),
    include_bytes!("../../../assets/maps/callisto.vxl"),
    include_bytes!("../../../assets/maps/ganymede.vxl"),
    include_bytes!("../../../assets/maps/europa.vxl"),
    include_bytes!("../../../assets/maps/io.vxl"),
    include_bytes!("../../../assets/maps/titan.vxl"),
    include_bytes!("../../../assets/maps/enceladus.vxl"),
    include_bytes!("../../../assets/maps/triton.vxl"),
    include_bytes!("../../../assets/maps/rhea.vxl"),
    include_bytes!("../../../assets/maps/iapetus.vxl"),
    include_bytes!("../../../assets/maps/dione.vxl"),
    include_bytes!("../../../assets/maps/titania.vxl"),
    include_bytes!("../../../assets/maps/oberon.vxl"),
    include_bytes!("../../../assets/maps/umbriel.vxl"),
    include_bytes!("../../../assets/maps/ariel.vxl"),
    include_bytes!("../../../assets/maps/miranda.vxl"),
    include_bytes!("../../../assets/maps/pluto.vxl"),
    include_bytes!("../../../assets/maps/chiron.vxl"),
    include_bytes!("../../../assets/maps/earth.vxl"),
];

/// Number of selectable battlefields, and their display names + a representative
/// thumbnail colour, all in [`MAPS`] order (used by the lobby map picker).
pub const MAP_COUNT: usize = MAPS.len();

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // used by the web lobby
pub const MAP_NAMES: [&str; MAP_COUNT] = [
    "LUNA",
    "CERES",
    "VESTA",
    "MARS",
    "CALLISTO",
    "GANYMEDE",
    "EUROPA",
    "IO",
    "TITAN",
    "ENCELADUS",
    "TRITON",
    "RHEA",
    "IAPETUS",
    "DIONE",
    "TITANIA",
    "OBERON",
    "UMBRIEL",
    "ARIEL",
    "MIRANDA",
    "PLUTO",
    "CHIRON",
    "EARTH",
];

/// A representative surface colour per world, for the lobby thumbnail.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // used by the web lobby
pub const MAP_SWATCH: [[u8; 3]; MAP_COUNT] = [
    [150, 148, 142],
    [78, 76, 76],
    [150, 140, 120],
    [170, 96, 60],
    [120, 112, 104],
    [150, 156, 168],
    [220, 210, 196],
    [210, 190, 90],
    [180, 120, 60],
    [225, 236, 244],
    [210, 180, 170],
    [180, 188, 196],
    [120, 112, 100],
    [170, 178, 188],
    [150, 148, 150],
    [120, 112, 108],
    [78, 76, 80],
    [180, 196, 212],
    [150, 156, 168],
    [180, 140, 100],
    [80, 80, 86],
    [80, 150, 90],
];

thread_local! {
    /// Index into [`MAPS`] of the active battlefield, or `None` for the Earthlike
    /// heightmap in `terrain.rs`. Chosen at runtime from the lobby.
    static SELECTED: Cell<Option<usize>> = const { Cell::new(None) };
    /// Parsed maps, leaked to `'static` and cached by index (only a handful are
    /// ever loaded in a session).
    static CACHE: RefCell<Vec<Option<&'static VoxelGrid>>> = const { RefCell::new(Vec::new()) };
}

/// Select the active battlefield (`None` = Earthlike default). Call before
/// `Gfx::set_world`.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // driven by the web lobby
pub fn set_active(idx: Option<usize>) {
    SELECTED.with(|s| s.set(idx.filter(|&i| i < MAP_COUNT)));
}

/// The active voxel map, if one is selected (parsed + cached on first use).
pub fn active() -> Option<&'static VoxelGrid> {
    let i = SELECTED.with(|s| s.get())?;
    CACHE.with(|c| {
        let mut v = c.borrow_mut();
        if v.is_empty() {
            v.resize(MAP_COUNT, None);
        }
        if v[i].is_none() {
            v[i] = Some(Box::leak(Box::new(VoxelGrid::parse(MAPS[i]))));
        }
        v[i]
    })
}

// --- per-world texture sets + liquid, for the renderer -----------------------
// The 12 archetype tile sets [base, low, high, accent], embedded.
macro_rules! tiles {
    ($a:literal) => {
        [
            include_bytes!(concat!("../../../assets/textures/worlds/", $a, "/base.png")),
            include_bytes!(concat!("../../../assets/textures/worlds/", $a, "/low.png")),
            include_bytes!(concat!("../../../assets/textures/worlds/", $a, "/high.png")),
            include_bytes!(concat!(
                "../../../assets/textures/worlds/",
                $a,
                "/accent.png"
            )),
        ]
    };
}
const ARCH_TILES: [[&[u8]; 4]; 12] = [
    tiles!("regolith_grey"),
    tiles!("regolith_dark"),
    tiles!("dirty_ice"),
    tiles!("grooved_ice"),
    tiles!("bright_ice"),
    tiles!("europa_ice"),
    tiles!("mars_rust"),
    tiles!("io_sulfur"),
    tiles!("titan_haze"),
    tiles!("triton_ice"),
    tiles!("pluto_tholin"),
    tiles!("earth"),
];

/// Archetype index per world (into `ARCH_TILES`), in [`MAPS`] order.
#[rustfmt::skip]
const MAP_ARCHETYPE: [usize; MAP_COUNT] = [
    0, 1, 0, 6, 2, 3, 5, 7, 8, 4, 9, 2, 2, 2, 0, 0, 1, 3, 3, 10, 1, 11,
];

/// The active world's tile set (base, low, high, accent), if one is selected.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // driven by the web lobby
pub fn active_tiles() -> Option<[&'static [u8]; 4]> {
    SELECTED
        .with(|s| s.get())
        .map(|i| ARCH_TILES[MAP_ARCHETYPE[i]])
}

/// Liquid body colour (rgb) + waviness (a) for the active world, or `None` for a
/// dry world. Earth has blue water; Titan has dark, calm methane.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // driven by the web lobby
pub fn active_liquid() -> Option<[f32; 4]> {
    match SELECTED.with(|s| s.get()) {
        Some(8) => Some([0.06, 0.05, 0.07, 0.5]), // Titan: dark methane, calm
        Some(21) => Some([0.06, 0.22, 0.34, 1.0]), // Earth: blue water
        _ => None,
    }
}

/// Whether the active world's hazard material glows (Io's lava).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // driven by the web lobby
pub fn active_lava() -> bool {
    SELECTED.with(|s| s.get()) == Some(7)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_baked_map_parses_meshes_and_is_buildable() {
        for (i, bytes) in MAPS.iter().enumerate() {
            let g = VoxelGrid::parse(bytes);
            assert!(g.nx > 0 && g.ny > 0 && g.nz > 0, "map {i} empty dims");
            // surface is finite across the map
            let h = g.surface_height(0.0, 0.0);
            assert!(h.is_finite(), "map {i} non-finite surface");
            // some, but not all, of the map is buildable
            let cols = g.nx * g.nz;
            let flat = g.buildable.iter().filter(|&&b| b != 0).count();
            assert!(
                flat > cols / 10 && flat < cols,
                "map {i} buildable fraction off"
            );
            // marching cubes produces a non-trivial, well-formed mesh
            let mesh = g.build_mesh();
            assert!(
                mesh.len() >= 3 && mesh.len().is_multiple_of(3),
                "map {i} bad mesh"
            );
            for v in mesh.iter().take(2000) {
                let n = v.normal;
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                assert!((len - 1.0).abs() < 1e-3, "map {i} non-unit normal");
                assert!(v.mat >= 0.0 && v.mat <= 4.0, "map {i} bad material id");
            }
        }
    }

    #[test]
    fn active_defaults_to_none() {
        assert!(active().is_none());
    }
}
