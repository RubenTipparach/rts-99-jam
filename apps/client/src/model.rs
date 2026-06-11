//! Wavefront OBJ/MTL loader for the game's static model assets
//! (`assets/models/*.obj` + `.mtl`), the files artists edit in Blender.
//!
//! Engine semantics ride on the MTL:
//! - `Kd r g b` is the material color (freely editable in Blender).
//! - The material NAME carries flags Blender round-trips untouched:
//!   `_t1` marks the team-tint channel (the faction color replaces the
//!   material color), and `_d0` / `_d1` / `_d2` select the surface-detail
//!   texture (flat, organic rock grain, plate/masonry seams).
//!
//! The parser accepts what Blender exports: `v`, `vn`, `vt`, `f` with any
//! `v`, `v/vt`, `v//vn`, or `v/vt/vn` corner form (n-gons are fan
//! triangulated, negative indices resolved), `usemtl`, and ignores the
//! rest. Faces without normals get a flat normal from their winding.
//! Presentation-only: floats are fine here.

use crate::gfx::{UnitVertex, DETAIL_FLAT, DETAIL_PLATE, DETAIL_ROCK};

#[derive(Clone, Copy)]
struct Mat {
    color: [f32; 3],
    team: f32,
    detail: f32,
}

const DEFAULT_MAT: Mat = Mat {
    color: [1.0, 0.0, 1.0], // loud magenta: a face with no material is a bug
    team: 0.0,
    detail: DETAIL_PLATE,
};

/// Engine flags from a material name: `_t1` => team tint, `_d<k>` => detail.
fn flags(name: &str) -> (f32, f32) {
    let mut team = 0.0;
    let mut detail = DETAIL_PLATE;
    for tok in name.split('_') {
        match tok {
            "t1" => team = 1.0,
            "d0" => detail = DETAIL_FLAT,
            "d1" => detail = DETAIL_ROCK,
            "d2" => detail = DETAIL_PLATE,
            _ => {}
        }
    }
    (team, detail)
}

fn parse_mtl(mtl: &str) -> Vec<(String, Mat)> {
    let mut mats: Vec<(String, Mat)> = Vec::new();
    for line in mtl.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some("newmtl") => {
                if let Some(name) = it.next() {
                    let (team, detail) = flags(name);
                    mats.push((
                        name.to_string(),
                        Mat {
                            color: DEFAULT_MAT.color,
                            team,
                            detail,
                        },
                    ));
                }
            }
            Some("Kd") => {
                let mut c = [0.0f32; 3];
                for v in &mut c {
                    *v = it.next().and_then(|t| t.parse().ok()).unwrap_or(0.0);
                }
                if let Some((_, m)) = mats.last_mut() {
                    m.color = c;
                }
            }
            _ => {}
        }
    }
    mats
}

/// Resolve a 1-based (or negative, relative) OBJ index.
fn resolve(idx: i32, len: usize) -> usize {
    if idx < 0 {
        (len as i32 + idx).max(0) as usize
    } else {
        (idx as usize - 1).min(len.saturating_sub(1))
    }
}

/// Parse one model into the unit pipeline's vertex format.
pub(crate) fn load(obj: &str, mtl: &str) -> Vec<UnitVertex> {
    let mats = parse_mtl(mtl);
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut nor: Vec<[f32; 3]> = Vec::new();
    let mut cur = DEFAULT_MAT;
    let mut out: Vec<UnitVertex> = Vec::new();

    for line in obj.lines() {
        let mut it = line.split_whitespace();
        match it.next() {
            Some(t @ ("v" | "vn")) => {
                let mut p = [0.0f32; 3];
                for v in &mut p {
                    *v = it.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
                }
                if t == "v" {
                    pos.push(p);
                } else {
                    nor.push(p);
                }
            }
            Some("usemtl") => {
                if let Some(name) = it.next() {
                    cur = mats
                        .iter()
                        .find(|(n, _)| n == name)
                        .map(|(_, m)| *m)
                        .unwrap_or(DEFAULT_MAT);
                }
            }
            Some("f") => {
                // Corners as (position index, optional normal index).
                let corners: Vec<([f32; 3], Option<[f32; 3]>)> = it
                    .filter_map(|tok| {
                        let mut parts = tok.split('/');
                        let vi: i32 = parts.next()?.parse().ok()?;
                        let _vt = parts.next();
                        let vn = parts
                            .next()
                            .and_then(|t| t.parse::<i32>().ok())
                            .map(|i| nor[resolve(i, nor.len())]);
                        Some((pos[resolve(vi, pos.len())], vn))
                    })
                    .collect();
                // Fan triangulation (Blender may export quads/n-gons).
                for k in 1..corners.len().saturating_sub(1) {
                    let tri = [corners[0], corners[k], corners[k + 1]];
                    let flat = face_normal(tri[0].0, tri[1].0, tri[2].0);
                    for (p, n) in tri {
                        out.push(UnitVertex {
                            pos: p,
                            normal: n.unwrap_or(flat),
                            color: [cur.color[0], cur.color[1], cur.color[2], cur.team],
                            detail: cur.detail,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 1e-6 {
        [n[0] / l, n[1] / l, n[2] / l]
    } else {
        [0.0, 1.0, 0.0]
    }
}
