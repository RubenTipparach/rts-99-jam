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

/// One terrain mesh vertex for the voxel pipeline. `weights` are soft texture
/// blend weights across the four tile slots (`x` mid/base, `y` low, `z` high,
/// `w` accent) and `haz` is the hazard (lava) channel; together they partition
/// unity. Computed by trilinearly sampling the material field, so the shader can
/// blend tiles smoothly across material boundaries instead of switching hard.
#[derive(Clone, Copy)]
pub struct MeshVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub weights: [f32; 4],
    pub haz: f32,
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

    /// World-space height of the topmost solid surface under `(x, z)`,
    /// bilinearly blended across the four surrounding columns so units glide
    /// along the surface instead of snapping per voxel column.
    pub fn surface_height(&self, x: f32, z: f32) -> f32 {
        let fi = ((x - self.bounds[0]) / self.dx()).clamp(0.0, self.nx as f32 - 1.0);
        let fk = ((z - self.bounds[4]) / self.dz()).clamp(0.0, self.nz as f32 - 1.0);
        let (i0, k0) = (fi.floor() as usize, fk.floor() as usize);
        let (i1, k1) = ((i0 + 1).min(self.nx - 1), (k0 + 1).min(self.nz - 1));
        let (tx, tz) = (fi - i0 as f32, fk - k0 as f32);
        let a = self.column_height(i0, k0) * (1.0 - tx) + self.column_height(i1, k0) * tx;
        let b = self.column_height(i0, k1) * (1.0 - tx) + self.column_height(i1, k1) * tx;
        a * (1.0 - tz) + b * tz
    }

    /// Surface height of one column: the top solid voxel, eased against the
    /// air voxel above it.
    fn column_height(&self, i: usize, k: usize) -> f32 {
        for j in (0..self.ny).rev() {
            if self.density[self.lin(i, j, k)] >= ISO {
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

    /// Soft texture blend weights at a world point: trilinearly interpolate the
    /// (one-hot) material of the eight surrounding voxels, so a vertex sitting
    /// between a skin cell and a subsurface cell gets a mix of both. Returns the
    /// four tile weights (mid, low, high, accent) and the hazard weight; the five
    /// sum to 1.
    fn weights_at(&self, p: [f32; 3]) -> ([f32; 4], f32) {
        let fi = ((p[0] - self.bounds[0]) / self.dx()).clamp(0.0, self.nx as f32 - 1.001);
        let fj = ((p[1] - self.bounds[2]) / self.dy()).clamp(0.0, self.ny as f32 - 1.001);
        let fk = ((p[2] - self.bounds[4]) / self.dz()).clamp(0.0, self.nz as f32 - 1.001);
        let (i0, j0, k0) = (
            fi.floor() as usize,
            fj.floor() as usize,
            fk.floor() as usize,
        );
        let (tx, ty, tz) = (fi - i0 as f32, fj - j0 as f32, fk - k0 as f32);
        let mut acc = [0.0f32; 5];
        for dj in 0..2 {
            for dk in 0..2 {
                for di in 0..2 {
                    let cx = if di == 1 { tx } else { 1.0 - tx };
                    let cy = if dj == 1 { ty } else { 1.0 - ty };
                    let cz = if dk == 1 { tz } else { 1.0 - tz };
                    let m = self.material[self.lin(i0 + di, j0 + dj, k0 + dk)].min(4);
                    // material id -> tile channel: 0 low->1, 1 mid->0, 2 high->2,
                    // 3 accent->3, 4 hazard->4.
                    let ch = [1usize, 0, 2, 3, 4][m as usize];
                    acc[ch] += cx * cy * cz;
                }
            }
        }
        ([acc[0], acc[1], acc[2], acc[3]], acc[4])
    }

    /// Marching cubes (tetrahedral) -> a triangle soup for the voxel pipeline.
    /// Each vertex carries soft material blend weights; the shader does the
    /// texturing.
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

        let emit = |p: [f32; 3], out: &mut Vec<MeshVertex>| {
            let (weights, haz) = self.weights_at(p);
            out.push(MeshVertex {
                pos: p,
                normal: self.normal(p),
                weights,
                haz,
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
                            emit(a, &mut out);
                            emit(b, &mut out);
                            emit(c, &mut out);
                        } else {
                            let (s0, s1) = (solids[0], solids[1]);
                            let (a0, a1) = (airs[0], airs[1]);
                            let pa = interp(cp[s0], cv[s0], cp[a0], cv[a0]);
                            let pb = interp(cp[s0], cv[s0], cp[a1], cv[a1]);
                            let pc = interp(cp[s1], cv[s1], cp[a1], cv[a1]);
                            let pd = interp(cp[s1], cv[s1], cp[a0], cv[a0]);
                            emit(pa, &mut out);
                            emit(pb, &mut out);
                            emit(pc, &mut out);
                            emit(pa, &mut out);
                            emit(pc, &mut out);
                            emit(pd, &mut out);
                        }
                    }
                }
            }
        }
        out
    }

    /// World-space liquid surface height over `(x, z)`, if that column is wet.
    pub fn liquid_surface(&self, x: f32, z: f32) -> Option<f32> {
        let (i, k) = self.col_index(x, z);
        let lj = self.liquid[k * self.nx + i];
        (lj > 0).then(|| self.world(i, (lj - 1) as usize, k)[1])
    }

    /// The water/methane surface mesh, rendered by the water pipeline.
    /// Empty if the world has no liquid.
    ///
    /// The open sea draws as one tessellated sheet across the whole map at
    /// the dominant liquid level: the depth test sinks it under the land, so
    /// the shoreline is the exact terrain intersection instead of the old
    /// stair-stepped column quads hovering over the banks, and the dense
    /// grid gives the vertex waves something to bend. Lakes and rivers above
    /// the sea keep per-column quads, tucked down to hug their basins.
    pub fn liquid_mesh(&self) -> Vec<[f32; 3]> {
        let mut out = Vec::new();
        let dx = self.dx();
        let dz = self.dz();
        // The sea level is the modal liquid layer (the format stores liquid
        // per column; a real sea floods hundreds of columns at one level).
        let mut hist = [0u32; 256];
        for &l in self.liquid.iter() {
            hist[l as usize] += 1;
        }
        let sea_j = (1..256).max_by_key(|&j| hist[j]).filter(|&j| hist[j] > 200);
        if let Some(j) = sea_j {
            let sea = self.world(0, j - 1, 0)[1] - 0.2;
            const N: usize = 128;
            let sx = (self.bounds[1] - self.bounds[0]) / N as f32;
            let sz = (self.bounds[5] - self.bounds[4]) / N as f32;
            for k in 0..N {
                for i in 0..N {
                    let (x0, z0) = (
                        self.bounds[0] + i as f32 * sx,
                        self.bounds[4] + k as f32 * sz,
                    );
                    let (x1, z1) = (x0 + sx, z0 + sz);
                    let a = [x0, sea, z0];
                    let b = [x1, sea, z0];
                    let c = [x1, sea, z1];
                    let d = [x0, sea, z1];
                    for v in [a, b, c, a, c, d] {
                        out.push(v);
                    }
                }
            }
        }
        for k in 0..self.nz {
            for i in 0..self.nx {
                let lj = self.liquid[k * self.nx + i];
                if lj == 0 || Some(lj as usize) == sea_j {
                    continue;
                }
                let p = self.world(i, (lj - 1) as usize, k);
                let (x0, y, z0) = (p[0], p[1] - 0.3, p[2]);
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

/// Asset-file key per world (lowercase, in [`MAPS`] order): names the baked
/// `.vxl` and the pre-rendered lobby preview PNG. Only the preview-renderer
/// dev test consumes it, so every non-test target sees it as unused.
#[allow(dead_code)]
pub const MAP_KEYS: [&str; MAP_COUNT] = [
    "moon",
    "ceres",
    "vesta",
    "mars",
    "callisto",
    "ganymede",
    "europa",
    "io",
    "titan",
    "enceladus",
    "triton",
    "rhea",
    "iapetus",
    "dione",
    "titania",
    "oberon",
    "umbriel",
    "ariel",
    "miranda",
    "pluto",
    "chiron",
    "earth",
];

/// Pre-rendered 3D lobby previews (the actual voxel terrain rasterized at the
/// game camera angle; regenerate with
/// `cargo test -p client render_map_previews -- --ignored`).
#[cfg(target_arch = "wasm32")]
pub const MAP_PREVIEWS: [&[u8]; MAP_COUNT] = [
    include_bytes!("../../../assets/previews/maps/moon.png"),
    include_bytes!("../../../assets/previews/maps/ceres.png"),
    include_bytes!("../../../assets/previews/maps/vesta.png"),
    include_bytes!("../../../assets/previews/maps/mars.png"),
    include_bytes!("../../../assets/previews/maps/callisto.png"),
    include_bytes!("../../../assets/previews/maps/ganymede.png"),
    include_bytes!("../../../assets/previews/maps/europa.png"),
    include_bytes!("../../../assets/previews/maps/io.png"),
    include_bytes!("../../../assets/previews/maps/titan.png"),
    include_bytes!("../../../assets/previews/maps/enceladus.png"),
    include_bytes!("../../../assets/previews/maps/triton.png"),
    include_bytes!("../../../assets/previews/maps/rhea.png"),
    include_bytes!("../../../assets/previews/maps/iapetus.png"),
    include_bytes!("../../../assets/previews/maps/dione.png"),
    include_bytes!("../../../assets/previews/maps/titania.png"),
    include_bytes!("../../../assets/previews/maps/oberon.png"),
    include_bytes!("../../../assets/previews/maps/umbriel.png"),
    include_bytes!("../../../assets/previews/maps/ariel.png"),
    include_bytes!("../../../assets/previews/maps/miranda.png"),
    include_bytes!("../../../assets/previews/maps/pluto.png"),
    include_bytes!("../../../assets/previews/maps/chiron.png"),
    include_bytes!("../../../assets/previews/maps/earth.png"),
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

/// Index of the active voxel map in [`MAPS`] order, if one is selected.
pub fn active_index() -> Option<usize> {
    SELECTED.with(|s| s.get())
}

/// The parsed grid of any map (cached on first use). The lobby uses this to
/// read live surface heights for the preview's spawn markers.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // web lobby only
pub fn grid(idx: usize) -> Option<&'static VoxelGrid> {
    if idx >= MAP_COUNT {
        return None;
    }
    CACHE.with(|c| {
        let mut v = c.borrow_mut();
        if v.is_empty() {
            v.resize(MAP_COUNT, None);
        }
        if v[idx].is_none() {
            v[idx] = Some(Box::leak(Box::new(VoxelGrid::parse(MAPS[idx]))));
        }
        v[idx]
    })
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

/// The lobby preview camera (must match the offline preview renderer).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // web lobby only
pub const PREVIEW_YAW: f32 = std::f32::consts::FRAC_PI_4;
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // web lobby only
pub const PREVIEW_PITCH: f32 = 0.95;

/// The orthographic fit of world `idx`'s pre-rendered preview image:
/// `[u0, u1, v0, v1, img_w, img_h]`, from the sidecar the preview renderer
/// writes next to the PNGs. With [`PREVIEW_YAW`]/[`PREVIEW_PITCH`] this
/// projects any LIVE world position onto the image exactly, so the lobby's
/// spawn markers come from the real map data, never baked pixels.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // web lobby only
pub fn preview_fit(idx: usize) -> Option<[f32; 6]> {
    let key = MAP_KEYS.get(idx)?;
    for line in include_str!("../../../assets/previews/maps/projection.txt").lines() {
        let mut tok = line.split_whitespace();
        if tok.next() != Some(key) {
            continue;
        }
        let mut fit = [0.0f32; 6];
        for v in &mut fit {
            *v = tok.next()?.parse().ok()?;
        }
        return Some(fit);
    }
    None
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

/// Surface brightness tint (linear-light rgb multiplier) for world `idx`.
/// Most worlds draw their tile set as-is; the near-coal carbonaceous
/// regolith gets a readability lift (real Ceres is one of the darkest
/// surfaces in the system, but battlefield legibility wins over albedo
/// realism).
fn world_tint(idx: usize) -> [f32; 3] {
    match idx {
        1 => [1.70, 1.66, 1.60], // Ceres: lift, slightly warm
        _ => [1.0, 1.0, 1.0],
    }
}

/// [`world_tint`] of the active world (white for the Earthlike default).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // driven by the web lobby
pub fn active_tint() -> [f32; 3] {
    SELECTED
        .with(|s| s.get())
        .map(world_tint)
        .unwrap_or([1.0, 1.0, 1.0])
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
                let sum = v.weights[0] + v.weights[1] + v.weights[2] + v.weights[3] + v.haz;
                assert!(
                    (sum - 1.0).abs() < 1e-3,
                    "map {i} weights not partition of unity"
                );
                assert!(
                    v.weights.iter().all(|&w| (-1e-4..=1.0001).contains(&w))
                        && (-1e-4..=1.0001).contains(&v.haz),
                    "map {i} weight out of range"
                );
            }
        }
    }

    #[test]
    fn active_defaults_to_none() {
        assert!(active().is_none());
    }

    /// Rasterize one world's voxel terrain (plus its liquid) with the game's
    /// iso camera (yaw 45, pitch 0.95) and shader-style lambert lighting.
    /// CPU-only; per-vertex color comes from the world swatch blended by the
    /// material weights, so the preview matches the in-game palette.
    /// The archetype palettes the world tile textures are painted from
    /// (assets/worldgen/worlds.py ARCHETYPES): [low, mid, high, accent]
    /// per kit, in [`ARCH_TILES`] order, sRGB 0-255.
    #[rustfmt::skip]
    const ARCH_PALETTE: [[[f32; 3]; 4]; 12] = [
        [[60.0, 58.0, 56.0], [124.0, 119.0, 112.0], [180.0, 174.0, 164.0], [208.0, 203.0, 195.0]], // regolith_grey
        [[30.0, 29.0, 30.0], [62.0, 60.0, 60.0], [100.0, 97.0, 94.0], [198.0, 203.0, 209.0]],      // regolith_dark
        [[78.0, 68.0, 58.0], [130.0, 126.0, 124.0], [188.0, 196.0, 204.0], [216.0, 224.0, 234.0]], // dirty_ice
        [[78.0, 80.0, 90.0], [126.0, 132.0, 144.0], [190.0, 200.0, 214.0], [214.0, 226.0, 240.0]], // grooved_ice
        [[150.0, 170.0, 186.0], [208.0, 217.0, 225.0], [238.0, 244.0, 249.0], [120.0, 165.0, 198.0]], // bright_ice
        [[150.0, 150.0, 162.0], [212.0, 206.0, 196.0], [238.0, 233.0, 224.0], [156.0, 98.0, 70.0]],   // europa_ice
        [[92.0, 52.0, 36.0], [162.0, 98.0, 64.0], [202.0, 150.0, 110.0], [224.0, 224.0, 230.0]],   // mars_rust
        [[150.0, 122.0, 50.0], [212.0, 188.0, 86.0], [232.0, 220.0, 168.0], [214.0, 96.0, 40.0]],  // io_sulfur
        [[84.0, 54.0, 30.0], [168.0, 116.0, 64.0], [198.0, 158.0, 104.0], [96.0, 76.0, 54.0]], // titan_haze
        [[150.0, 122.0, 122.0], [206.0, 180.0, 170.0], [230.0, 214.0, 206.0], [84.0, 66.0, 70.0]], // triton_ice
        [[110.0, 78.0, 60.0], [172.0, 132.0, 100.0], [226.0, 208.0, 180.0], [134.0, 76.0, 54.0]],  // pluto_tholin
        [[96.0, 132.0, 72.0], [74.0, 112.0, 58.0], [120.0, 112.0, 96.0], [236.0, 240.0, 244.0]],   // earth
    ];

    fn preview(g: &VoxelGrid, idx: usize, w: u32) -> (image::RgbaImage, [f32; 6]) {
        let (yaw, pitch) = (std::f32::consts::FRAC_PI_4, 0.95_f32);
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

        let mesh = g.build_mesh();
        let liquid = g.liquid_mesh();

        // Fit the projection of everything into the image.
        let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for p in mesh.iter().map(|m| m.pos).chain(liquid.iter().copied()) {
            let (u, v, _) = project(p);
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(v);
            v1 = v1.max(v);
        }
        let h = ((v1 - v0) / (u1 - u0) * w as f32).ceil() as u32 + 8;
        let scale = ((w as f32 - 8.0) / (u1 - u0)).min((h as f32 - 8.0) / (v1 - v0));
        let to_px = |u: f32, v: f32| {
            (
                (u - u0) * scale + (w as f32 - (u1 - u0) * scale) * 0.5,
                (v1 - v) * scale + (h as f32 - (v1 - v0) * scale) * 0.5,
            )
        };

        // Per-world palette: the SAME archetype palette the tile textures
        // are painted from (assets/worldgen/worlds.py ARCHETYPES), so the
        // thumbnail's terrain colors match the battlefield. Each tile slot
        // approximates its texture's average tone; the world tint (a
        // linear-light multiplier in the shader) folds in via the gamma.
        let [plo, pmid, phigh, pacc] = ARCH_PALETTE[MAP_ARCHETYPE[idx]];
        let t = world_tint(idx);
        let tinted = |c: [f32; 3]| {
            [
                (c[0] / 255.0 * t[0].powf(1.0 / 2.2)).min(1.0),
                (c[1] / 255.0 * t[1].powf(1.0 / 2.2)).min(1.0),
                (c[2] / 255.0 * t[2].powf(1.0 / 2.2)).min(1.0),
            ]
        };
        let mix = |a: [f32; 3], b: [f32; 3], k: f32| {
            [
                a[0] + (b[0] - a[0]) * k,
                a[1] + (b[1] - a[1]) * k,
                a[2] + (b[2] - a[2]) * k,
            ]
        };
        let base = tinted(pmid);
        let low = tinted(mix(plo, pmid, 0.3));
        let high = tinted(mix(pmid, phigh, 0.7));
        let accent = tinted(pacc);
        let lava = [0.95, 0.42, 0.12];
        // Liquid color (matches `active_liquid`): Titan methane, Earth ocean.
        let liq_col = match idx {
            8 => [0.06, 0.05, 0.07],
            21 => [0.06, 0.22, 0.34],
            _ => [0.0, 0.0, 0.0],
        };
        let ll = (0.5_f32 * 0.5 + 1.0 + 0.35 * 0.35).sqrt();
        let light = [0.5 / ll, 1.0 / ll, 0.35 / ll];

        // The lobby backdrop color, baked in so the blit composes seamlessly.
        let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([11, 18, 36, 255]));
        let mut depth = vec![f32::MIN; (w * h) as usize];
        let mut tri = |p: [[f32; 3]; 3], rgb: [f32; 3]| {
            let q: Vec<(f32, f32, f32)> = p
                .iter()
                .map(|&v| {
                    let (u, vv, d) = project(v);
                    let (x, y) = to_px(u, vv);
                    (x, y, d)
                })
                .collect();
            let area =
                (q[1].0 - q[0].0) * (q[2].1 - q[0].1) - (q[2].0 - q[0].0) * (q[1].1 - q[0].1);
            if area.abs() < 1e-6 {
                return;
            }
            let px8 = [
                (rgb[0].clamp(0.0, 1.0) * 255.0) as u8,
                (rgb[1].clamp(0.0, 1.0) * 255.0) as u8,
                (rgb[2].clamp(0.0, 1.0) * 255.0) as u8,
            ];
            let xmin = q.iter().map(|t| t.0).fold(f32::MAX, f32::min).max(0.0) as u32;
            let xmax = (q.iter().map(|t| t.0).fold(f32::MIN, f32::max)).min(w as f32 - 1.0) as u32;
            let ymin = q.iter().map(|t| t.1).fold(f32::MAX, f32::min).max(0.0) as u32;
            let ymax = (q.iter().map(|t| t.1).fold(f32::MIN, f32::max)).min(h as f32 - 1.0) as u32;
            for py in ymin..=ymax {
                for px in xmin..=xmax {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = ((q[1].0 - fx) * (q[2].1 - fy) - (q[2].0 - fx) * (q[1].1 - fy)) / area;
                    let w1 = ((q[2].0 - fx) * (q[0].1 - fy) - (q[0].0 - fx) * (q[2].1 - fy)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let d = w0 * q[0].2 + w1 * q[1].2 + w2 * q[2].2;
                    let i = (py * w + px) as usize;
                    if d > depth[i] {
                        depth[i] = d;
                        img.put_pixel(px, py, image::Rgba([px8[0], px8[1], px8[2], 255]));
                    }
                }
            }
        };

        for t in mesh.chunks_exact(3) {
            let v = &t[0];
            let mut col = [0.0f32; 3];
            for c in 0..3 {
                col[c] = v.weights[0] * base[c]
                    + v.weights[1] * low[c]
                    + v.weights[2] * high[c]
                    + v.weights[3] * accent[c]
                    + v.haz * lava[c];
            }
            let lit = 0.45 + 0.7 * dot(v.normal, light).max(0.0);
            tri(
                [t[0].pos, t[1].pos, t[2].pos],
                [col[0] * lit, col[1] * lit, col[2] * lit],
            );
        }
        for t in liquid.chunks_exact(3) {
            tri([t[0], t[1], t[2]], liq_col);
        }
        // The orthographic fit of this image: [u0, u1, v0, v1, w, h]. The
        // lobby uses it (via `preview_fit`) to project LIVE world positions
        // (the fitted scenario's spawns) onto the image exactly: markers are
        // never baked into the PNG, so the data shown always comes from the
        // same map files the match loads.
        let fit = [u0, u1, v0, v1, w as f32, h as f32];
        (img, fit)
    }

    /// Renders every battlefield's 3D lobby preview to `target/previews/maps/`.
    /// A dev tool, not a check: run on demand and copy the keepers to
    /// `assets/previews/maps/` (which `MAP_PREVIEWS` embeds).
    #[test]
    #[ignore = "writes preview PNGs to target/previews/maps; run on demand"]
    fn render_map_previews() {
        std::fs::create_dir_all("target/previews/maps").unwrap();
        let mut proj = String::from(
            "# Orthographic fit of each pre-rendered lobby preview, written by\n\
             # `cargo test -p client render_map_previews -- --ignored` alongside\n\
             # the PNGs: key u0 u1 v0 v1 img_w img_h (see voxel::preview_fit).\n",
        );
        for (i, key) in MAP_KEYS.iter().enumerate() {
            let g = VoxelGrid::parse(MAPS[i]);
            let (img, fit) = preview(&g, i, 384);
            img.save(format!("target/previews/maps/{key}.png")).unwrap();
            proj.push_str(&format!(
                "{key} {:.6} {:.6} {:.6} {:.6} {} {}\n",
                fit[0], fit[1], fit[2], fit[3], fit[4], fit[5]
            ));
        }
        std::fs::write("target/previews/maps/projection.txt", proj).unwrap();
    }

    /// Renders every preview with the lobby's LIVE spawn-marker math drawn
    /// on top (the exact menu.rs projection), plus a printed report of each
    /// spawn's distance to the nearest liquid column. A dev check for marker
    /// accuracy: run on demand with
    /// `cargo test -p client render_spawn_overlays -- --ignored --nocapture`.
    #[test]
    #[ignore = "writes overlay PNGs to target/previews/spawncheck; run on demand"]
    fn render_spawn_overlays() {
        std::fs::create_dir_all("target/previews/spawncheck").unwrap();
        for (i, key) in MAP_KEYS.iter().enumerate() {
            let g = VoxelGrid::parse(MAPS[i]);
            let (mut img, fit) = preview(&g, i, 384);
            let [u0, u1, v0, v1, pw, ph] = fit;
            // The lobby's marker math, verbatim (menu.rs draw_map_preview).
            let (yaw, pitch) = (PREVIEW_YAW, PREVIEW_PITCH);
            let right = [-yaw.sin(), 0.0, yaw.cos()];
            let eye = [
                yaw.cos() * pitch.cos(),
                pitch.sin(),
                yaw.sin() * pitch.cos(),
            ];
            let upv = [
                right[1] * eye[2] - right[2] * eye[1],
                right[2] * eye[0] - right[0] * eye[2],
                right[0] * eye[1] - right[1] * eye[0],
            ];
            let s = ((pw - 8.0) / (u1 - u0)).min((ph - 8.0) / (v1 - v0));
            for (owner, sx, sz) in crate::map::world_spawns(i) {
                let p = [sx, g.surface_height(sx, sz), sz];
                let u = p[0] * right[0] + p[1] * right[1] + p[2] * right[2];
                let v = p[0] * upv[0] + p[1] * upv[1] + p[2] * upv[2];
                let px = (u - u0) * s + (pw - (u1 - u0) * s) * 0.5;
                let py = (v1 - v) * s + (ph - (v1 - v0) * s) * 0.5;
                // Distance from the spawn to the nearest liquid column.
                let (ci, ck) = g.col_index(sx, sz);
                let mut best = f32::MAX;
                for k in 0..g.nz {
                    for ii in 0..g.nx {
                        if g.liquid[k * g.nx + ii] != 0 {
                            let d = (ii as f32 - ci as f32).hypot(k as f32 - ck as f32);
                            best = best.min(d);
                        }
                    }
                }
                let du = best * g.dx();
                let (wgt, haz) = g.weights_at([sx, g.surface_height(sx, sz) - 0.5, sz]);
                let pix = img.get_pixel(px as u32, py as u32);
                println!(
                    "{key}: P{owner} spawn ({sx:.0},{sz:.0}) liquid {du:.0}u away  \
                     weights mid {:.2} low {:.2} high {:.2} accent {:.2} haz {haz:.2}  \
                     pixel {:?}",
                    wgt[0], wgt[1], wgt[2], wgt[3], pix
                );
                let col: [u8; 3] = if owner == 0 {
                    [74, 163, 255]
                } else {
                    [255, 90, 74]
                };
                let r = 4.0f32;
                for dy in -6..=6 {
                    for dx in -6..=6 {
                        let (qx, qy) = (px + dx as f32, py + dy as f32);
                        if qx < 0.0 || qy < 0.0 || qx >= pw || qy >= ph {
                            continue;
                        }
                        let d = (dx as f32).hypot(dy as f32);
                        if d < r {
                            img.put_pixel(
                                qx as u32,
                                qy as u32,
                                image::Rgba([col[0], col[1], col[2], 255]),
                            );
                        } else if d < r + 1.6 {
                            img.put_pixel(qx as u32, qy as u32, image::Rgba([8, 10, 16, 255]));
                        }
                    }
                }
            }
            img.save(format!("target/previews/spawncheck/{key}.png"))
                .unwrap();
        }
    }
}
