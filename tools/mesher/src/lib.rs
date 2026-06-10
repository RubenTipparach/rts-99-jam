//! Marching-tetrahedra mesher for the voxel map editor, compiled to
//! `wasm32-unknown-unknown` and loaded by `tools/mapeditor/index.html` (inside
//! its meshing Web Worker). A straight port of the editor's JS `meshFromGrid`,
//! same tetra scheme as the engine, so the output is bit-comparable in shape.
//!
//! No wasm-bindgen: plain `extern "C"` exports over linear memory. Protocol:
//! call `set_size(n)`, write the density + material grids at `density_ptr()` /
//! `material_ptr()`, call `mesh(...)` (returns the vertex count), then read
//! `count*3` floats at `pos_ptr()` / `nor_ptr()`, `count*4` at `wgt_ptr()`, and
//! `count` at `haz_ptr()`. Single-threaded by construction (one wasm instance
//! per worker), which is what makes the `static mut` state sound.
//!
//! Rebuild + install:
//! ```sh
//! cargo build -p mesher --release --target wasm32-unknown-unknown
//! cp target/wasm32-unknown-unknown/release/mesher.wasm tools/mapeditor/
//! ```
//!
//! This is a dev-tool crate: floats are fine here (it is not part of the
//! deterministic sim).

use core::ptr::addr_of_mut;

const ISO: f32 = 128.0;

/// Cube corner offsets, in the same order as the editor/engine.
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
/// The six tetrahedra that tile a cube (shared main diagonal 0-6).
const TETS: [[usize; 4]; 6] = [
    [0, 1, 2, 6],
    [0, 2, 3, 6],
    [0, 3, 7, 6],
    [0, 7, 4, 6],
    [0, 4, 5, 6],
    [0, 5, 1, 6],
];

struct State {
    density: Vec<u8>,
    material: Vec<u8>,
    pos: Vec<f32>,
    nor: Vec<f32>,
    wgt: Vec<f32>,
    haz: Vec<f32>,
}

static mut STATE: State = State {
    density: Vec::new(),
    material: Vec::new(),
    pos: Vec::new(),
    nor: Vec::new(),
    wgt: Vec::new(),
    haz: Vec::new(),
};

fn st() -> &'static mut State {
    // Sound: wasm32-unknown-unknown is single-threaded and the embedder never
    // re-enters; this is the one access path to the state.
    unsafe { &mut *addr_of_mut!(STATE) }
}

/// Resize the input grids to `n` voxels; write them at the `*_ptr()` addresses
/// after this call (resizing can move the buffers).
#[no_mangle]
pub extern "C" fn set_size(n: usize) {
    let s = st();
    s.density.resize(n, 0);
    s.material.resize(n, 0);
}

#[no_mangle]
pub extern "C" fn density_ptr() -> *mut u8 {
    st().density.as_mut_ptr()
}
#[no_mangle]
pub extern "C" fn material_ptr() -> *mut u8 {
    st().material.as_mut_ptr()
}
#[no_mangle]
pub extern "C" fn pos_ptr() -> *const f32 {
    st().pos.as_ptr()
}
#[no_mangle]
pub extern "C" fn nor_ptr() -> *const f32 {
    st().nor.as_ptr()
}
#[no_mangle]
pub extern "C" fn wgt_ptr() -> *const f32 {
    st().wgt.as_ptr()
}
#[no_mangle]
pub extern "C" fn haz_ptr() -> *const f32 {
    st().haz.as_ptr()
}

/// Density at clamped grid coords, as f32. Layout matches the editor:
/// `(j * nz + k) * nx + i`.
#[inline]
fn dens_at(d: &[u8], nx: usize, ny: usize, nz: usize, i: i64, j: i64, k: i64) -> f32 {
    let ic = i.clamp(0, nx as i64 - 1) as usize;
    let jc = j.clamp(0, ny as i64 - 1) as usize;
    let kc = k.clamp(0, nz as i64 - 1) as usize;
    d[(jc * nz + kc) * nx + ic] as f32
}

/// Gradient normal (toward air) at the grid-rounded world point.
#[allow(clippy::too_many_arguments)]
#[inline]
fn normal_at(
    d: &[u8],
    nx: usize,
    ny: usize,
    nz: usize,
    x0: f32,
    y0: f32,
    z0: f32,
    dx: f32,
    dy: f32,
    dz: f32,
    wx: f32,
    wy: f32,
    wz: f32,
) -> [f32; 3] {
    let i = ((wx - x0) / dx).round() as i64;
    let j = ((wy - y0) / dy).round() as i64;
    let k = ((wz - z0) / dz).round() as i64;
    let a = dens_at(d, nx, ny, nz, i - 1, j, k) - dens_at(d, nx, ny, nz, i + 1, j, k);
    let b = dens_at(d, nx, ny, nz, i, j - 1, k) - dens_at(d, nx, ny, nz, i, j + 1, k);
    let c = dens_at(d, nx, ny, nz, i, j, k - 1) - dens_at(d, nx, ny, nz, i, j, k + 1);
    let l = (a * a + b * b + c * c).sqrt();
    if l > 0.0 {
        [a / l, b / l, c / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}

/// Point on edge a-b where the density crosses ISO.
#[inline]
fn interp(a: [f32; 3], va: f32, b: [f32; 3], vb: f32) -> [f32; 3] {
    let t = if (vb - va).abs() < 1e-6 {
        0.5
    } else {
        ((ISO - va) / (vb - va)).clamp(0.0, 1.0)
    };
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// March the cells `i in [i0, i1)`, `k in [k0, k1)` of the grid (a chunk; pass
/// `0, nx-1, 0, nz-1` for the whole grid). Returns the vertex count (triangle
/// soup, count/3 tris). Cell ranges are in cube cells, so chunk meshes share
/// corner voxels with their neighbours and tile seamlessly.
#[allow(clippy::too_many_arguments)]
#[no_mangle]
pub extern "C" fn mesh(
    nx: u32,
    ny: u32,
    nz: u32,
    x0: f32,
    y0: f32,
    z0: f32,
    dx: f32,
    dy: f32,
    dz: f32,
    cy: f32,
    i0: u32,
    i1: u32,
    k0: u32,
    k1: u32,
) -> u32 {
    let (nx, ny, nz) = (nx as usize, ny as usize, nz as usize);
    let State {
        density,
        material,
        pos,
        nor,
        wgt,
        haz,
    } = st();
    pos.clear();
    nor.clear();
    wgt.clear();
    haz.clear();
    if nx < 2 || ny < 2 || nz < 2 || density.len() < nx * ny * nz {
        return 0;
    }
    let i0 = (i0 as usize).min(nx - 1);
    let i1 = (i1 as usize).min(nx - 1);
    let k0 = (k0 as usize).min(nz - 1);
    let k1 = (k1 as usize).min(nz - 1);

    let s_y = nz * nx;
    let s_z = nx;
    let mut offs = [0usize; 8];
    for (o, c) in offs.iter_mut().zip(C.iter()) {
        *o = c[1] * s_y + c[2] * s_z + c[0];
    }

    let mut cv = [0f32; 8];
    let mut cp = [[0f32; 3]; 8];
    for k in k0..k1 {
        for j in 0..ny - 1 {
            for i in i0..i1 {
                let base = (j * nz + k) * nx + i;
                let (mut mn, mut mx) = (255u8, 0u8);
                for q in 0..8 {
                    let v = density[base + offs[q]];
                    cv[q] = v as f32;
                    mn = mn.min(v);
                    mx = mx.max(v);
                }
                if mn >= 128 || mx < 128 {
                    continue;
                }
                for q in 0..8 {
                    cp[q] = [
                        x0 + (i + C[q][0]) as f32 * dx,
                        y0 + (j + C[q][1]) as f32 * dy,
                        z0 + (k + C[q][2]) as f32 * dz,
                    ];
                }
                let m = material[base].min(4);
                let w = [
                    if m == 1 { 1.0 } else { 0.0 },
                    if m == 0 { 1.0 } else { 0.0 },
                    if m == 2 { 1.0 } else { 0.0 },
                    if m == 3 { 1.0 } else { 0.0 },
                ];
                let hz = if m == 4 { 1.0 } else { 0.0 };
                let mut emit = |p: [f32; 3]| {
                    let n = normal_at(
                        density, nx, ny, nz, x0, y0, z0, dx, dy, dz, p[0], p[1], p[2],
                    );
                    pos.extend_from_slice(&[p[0], p[1] - cy, p[2]]);
                    nor.extend_from_slice(&n);
                    wgt.extend_from_slice(&w);
                    haz.push(hz);
                };
                for tet in TETS {
                    let mut sol = [0usize; 4];
                    let mut air = [0usize; 4];
                    let (mut ns, mut na) = (0usize, 0usize);
                    for &c in &tet {
                        if cv[c] >= ISO {
                            sol[ns] = c;
                            ns += 1;
                        } else {
                            air[na] = c;
                            na += 1;
                        }
                    }
                    if ns == 0 || ns == 4 {
                        continue;
                    }
                    if ns == 1 || ns == 3 {
                        let odd = if ns == 1 { sol[0] } else { air[0] };
                        for &c in &tet {
                            if c != odd {
                                emit(interp(cp[odd], cv[odd], cp[c], cv[c]));
                            }
                        }
                    } else {
                        let pa = interp(cp[sol[0]], cv[sol[0]], cp[air[0]], cv[air[0]]);
                        let pb = interp(cp[sol[0]], cv[sol[0]], cp[air[1]], cv[air[1]]);
                        let pc = interp(cp[sol[1]], cv[sol[1]], cp[air[1]], cv[air[1]]);
                        let pd = interp(cp[sol[1]], cv[sol[1]], cp[air[0]], cv[air[0]]);
                        emit(pa);
                        emit(pb);
                        emit(pc);
                        emit(pa);
                        emit(pc);
                        emit(pd);
                    }
                }
            }
        }
    }
    (pos.len() / 3) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A single solid voxel column in a small grid must produce a closed-ish
    /// surface: a nonzero, triangle-multiple vertex count with sane outputs.
    #[test]
    fn meshes_a_blob() {
        let (nx, ny, nz) = (8usize, 8usize, 8usize);
        set_size(nx * ny * nz);
        let s = st();
        for j in 0..4 {
            for k in 2..6 {
                for i in 2..6 {
                    s.density[(j * nz + k) * nx + i] = 255;
                    s.material[(j * nz + k) * nx + i] = 2;
                }
            }
        }
        let count = mesh(
            nx as u32, ny as u32, nz as u32, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 4.0, 0, 7, 0, 7,
        );
        assert!(count > 0, "blob should produce triangles");
        assert_eq!(count % 3, 0, "triangle soup");
        let s = st();
        assert_eq!(s.pos.len(), count as usize * 3);
        assert_eq!(s.wgt.len(), count as usize * 4);
        assert_eq!(s.haz.len(), count as usize);
        assert!(s.pos.iter().all(|v| v.is_finite()));
        assert!(s.nor.iter().all(|v| v.is_finite()));

        // Chunked meshing must tile: two half-range passes together produce the
        // same vertex count as the full range.
        let left = mesh(
            nx as u32, ny as u32, nz as u32, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 4.0, 0, 4, 0, 7,
        );
        let right = mesh(
            nx as u32, ny as u32, nz as u32, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 4.0, 4, 7, 0, 7,
        );
        assert_eq!(left + right, count, "chunks should tile without overlap");
    }
}
