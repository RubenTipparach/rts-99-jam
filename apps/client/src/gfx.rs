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
    /// Yaw about +Y as `(cos, sin)`; `ROT_NONE` leaves the mesh unrotated.
    pub rot: [f32; 2],
}

/// Identity rotation for [`InstanceRaw::rot`].
pub const ROT_NONE: [f32; 2] = [1.0, 0.0];

/// Baked walk-cycle keyframes per walking unit: static OBJ assets
/// (`assets/models/<unit>-walk-<k>.obj`) covering one full gait cycle.
pub const WALK_FRAMES: usize = 8;
/// Instance buckets per walking kind: the idle base model plus each
/// walk keyframe. Instances snap to the nearest frame (no interpolation).
pub const WALK_BUCKETS: usize = WALK_FRAMES + 1;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RingRaw {
    pub center: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
    /// Inner radius as a fraction of `radius`: [`RING`] for the standard
    /// selection annulus, 0.0 for a filled disc (blob contact shadows).
    pub inner: f32,
}

/// Standard annulus inner fraction for [`RingRaw::inner`].
pub const RING: f32 = 0.82;

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
/// `detail` selects the surface-detail texture: [`DETAIL_FLAT`] for things
/// that must stay flat shaded (crystals, gas, glow), [`DETAIL_ROCK`] for
/// organic rock/dirt grain, [`DETAIL_PLATE`] for plate/masonry seams.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct UnitVertex {
    pub(crate) pos: [f32; 3],
    pub(crate) normal: [f32; 3],
    pub(crate) color: [f32; 4],
    pub(crate) detail: f32,
}

/// No surface detail: flat shaded (crystals, gas pools, fx particles).
pub(crate) const DETAIL_FLAT: f32 = 0.0;
/// Organic grain (green channel of the detail map): rock, dirt, scree.
pub(crate) const DETAIL_ROCK: f32 = 1.0;
/// Plate/masonry seams (red channel): metal hulls and carved stone.
pub(crate) const DETAIL_PLATE: f32 = 2.0;

/// Tag every vertex with a detail-map selector (meshes default to plate).
fn set_detail(m: &mut [UnitVertex], d: f32) {
    for v in m {
        v.detail = d;
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    light_dir: [f32; 4],
    params: [f32; 4], // time, map_half, sea_level, point-light count
    /// Point lights for fx (muzzle flashes, mining sparks, explosions):
    /// xyz = world position, w = radius.
    light_pos: [[f32; 4]; MAX_LIGHTS],
    /// Point light colors (rgb; w unused).
    light_col: [[f32; 4]; MAX_LIGHTS],
}

/// A dynamic fx point light, consumed by [`Gfx::render`].
#[derive(Clone, Copy)]
pub struct FxLight {
    pub pos: [f32; 3],
    pub radius: f32,
    pub color: [f32; 3],
}

/// Point-light slots in the camera uniform: transient fx lights plus the
/// steady world lights (per-corner building floodlights, node glow).
pub const MAX_LIGHTS: usize = 64;

/// One voxel-terrain vertex: position, normal, and soft texture blend weights
/// (four tile slots + a hazard channel) the shader triplanar-blends from.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VoxelVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    weights: [f32; 4],
    haz: f32,
}

/// Per-world appearance uniform (group 2): terrain tint + liquid body colour.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WorldUniform {
    tint: [f32; 4],   // rgb tint; a = lava emissive strength
    liquid: [f32; 4], // rgb liquid colour; a = waviness
}

/// Earthlike ocean colour + chop, used when no voxel map is selected.
const EARTH_WATER: [f32; 4] = [0.06, 0.22, 0.34, 1.0];

pub const MAX_INSTANCES: usize = 8192;
pub const MAX_RINGS: usize = 256;
/// Segments per selection-ring decal, and the resulting vertex-buffer capacity.
const RING_SEGMENTS: usize = 40;
const MAX_RING_VERTS: usize = MAX_RINGS * RING_SEGMENTS * 6;
pub const FOW_RES: usize = 256;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

// --- model assets --------------------------------------------------------
//
// All unit/building/node models are STATIC Wavefront OBJ/MTL assets under
// assets/models/, edited in Blender (conventions in assets/models/README.md)
// and parsed by `crate::model`. A brand-new model is scaffolded once with
// `cargo run -p modelgen`; after that the checked-in file is the source of
// truth. Only geometry parametric to the running map (the water rim walls)
// and the fx particle cube are still built in code, from boxes whose face
// normals point outward from the box center (the unit pipeline doesn't
// cull, so winding never matters and lighting is always correct).

/// Load a static model asset (baked into the binary for the web build).
macro_rules! model {
    ($name:expr) => {
        crate::model::load(
            include_str!(concat!("../../../assets/models/", $name, ".obj")),
            include_str!(concat!("../../../assets/models/", $name, ".mtl")),
        )
    };
}

/// Load a walking unit's animation set: the idle base model followed by its
/// baked walk keyframes (all sharing the base model's MTL).
macro_rules! walk_models {
    ($name:literal) => {
        vec![
            model!($name),
            crate::model::load(
                include_str!(concat!("../../../assets/models/", $name, "-walk-0.obj")),
                include_str!(concat!("../../../assets/models/", $name, ".mtl")),
            ),
            crate::model::load(
                include_str!(concat!("../../../assets/models/", $name, "-walk-1.obj")),
                include_str!(concat!("../../../assets/models/", $name, ".mtl")),
            ),
            crate::model::load(
                include_str!(concat!("../../../assets/models/", $name, "-walk-2.obj")),
                include_str!(concat!("../../../assets/models/", $name, ".mtl")),
            ),
            crate::model::load(
                include_str!(concat!("../../../assets/models/", $name, "-walk-3.obj")),
                include_str!(concat!("../../../assets/models/", $name, ".mtl")),
            ),
            crate::model::load(
                include_str!(concat!("../../../assets/models/", $name, "-walk-4.obj")),
                include_str!(concat!("../../../assets/models/", $name, ".mtl")),
            ),
            crate::model::load(
                include_str!(concat!("../../../assets/models/", $name, "-walk-5.obj")),
                include_str!(concat!("../../../assets/models/", $name, ".mtl")),
            ),
            crate::model::load(
                include_str!(concat!("../../../assets/models/", $name, "-walk-6.obj")),
                include_str!(concat!("../../../assets/models/", $name, ".mtl")),
            ),
            crate::model::load(
                include_str!(concat!("../../../assets/models/", $name, "-walk-7.obj")),
                include_str!(concat!("../../../assets/models/", $name, ".mtl")),
            ),
        ]
    };
}

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

/// Fx particle: a unit cube around the origin. Drawn emissive (instance tint
/// alpha >= 3.0), scaled per particle; the chunky cube reads PS1-appropriate.
fn particle_mesh() -> Vec<UnitVertex> {
    let mut m = Vec::new();
    push_box(
        &mut m,
        [-0.5, -0.5, -0.5],
        [0.5, 0.5, 0.5],
        [1.0, 1.0, 1.0],
        0.0,
    );
    set_detail(&mut m, DETAIL_FLAT);
    m
}

/// Opaque dark walls around the map rim, from above the water down past
/// the seabed, so you don't see under the (translucent) water at the
/// edges. Drawn with the unit pipeline via an identity instance.
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
    set_detail(&mut m, DETAIL_FLAT);
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

/// The Earthlike ocean sheet: a tessellated full-map grid at sea level (one
/// big quad gives the water vertex waves nothing to bend).
fn water_quad() -> Vec<[f32; 3]> {
    const N: usize = 96;
    let h = terrain::HALF;
    let s = 2.0 * h / N as f32;
    let mut out = Vec::with_capacity(N * N * 6);
    for k in 0..N {
        for i in 0..N {
            let (x0, z0) = (-h + i as f32 * s, -h + k as f32 * s);
            let (x1, z1) = (x0 + s, z0 + s);
            let a = [x0, 0.0, z0];
            let b = [x1, 0.0, z0];
            let c = [x1, 0.0, z1];
            let d = [x0, 0.0, z1];
            for v in [a, b, c, a, c, d] {
                out.push(v);
            }
        }
    }
    out
}

/// Tessellate selection rings into ground-decal triangles whose vertices sit a
/// hair above the terrain, so the ring follows hills instead of clipping.
fn ring_decals(rings: &[RingRaw]) -> Vec<RingVertex> {
    let mut out = Vec::new();
    for r in rings.iter().take(MAX_RINGS) {
        let (cx, cz) = (r.center[0], r.center[2]);
        let (r_in, r_out) = (r.radius * r.inner.clamp(0.0, 0.98), r.radius);
        let color = r.color;
        let pt = |radius: f32, c: f32, s: f32| {
            let (x, z) = (cx + c * radius, cz + s * radius);
            [x, terrain::height(x, z) + 0.4, z]
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

/// WC3-style blob shadows: each entry becomes a soft dark disc draped on the
/// terrain, built as a triangle fan whose rim alpha fades to zero (the ring
/// pipeline's per-vertex color interpolation does the radial gradient).
fn shadow_decals(shadows: &[RingRaw]) -> Vec<RingVertex> {
    const SEGS: usize = 12;
    let mut out = Vec::with_capacity(shadows.len() * SEGS * 3);
    for sh in shadows {
        let (cx, cz) = (sh.center[0], sh.center[2]);
        let center = RingVertex {
            pos: [cx, terrain::height(cx, cz) + 0.35, cz],
            color: sh.color,
        };
        let rim = |k: usize| {
            let a = k as f32 / SEGS as f32 * std::f32::consts::TAU;
            let (s, c) = a.sin_cos();
            let (x, z) = (cx + c * sh.radius, cz + s * sh.radius);
            RingVertex {
                pos: [x, terrain::height(x, z) + 0.35, z],
                color: [sh.color[0], sh.color[1], sh.color[2], 0.0],
            }
        };
        for k in 0..SEGS {
            out.push(center);
            out.push(rim(k));
            out.push(rim(k + 1));
        }
    }
    out
}

/// Expand one walking kind into its per-frame sub-draws: each bucket's
/// vertex buffer with that bucket's instance count, clamped so the running
/// total never exceeds the kind's (possibly clamped) instance total.
fn frame_draws<'b>(
    bufs: &'b [(wgpu::Buffer, u32)],
    counts: &[u32; WALK_BUCKETS],
    total: usize,
) -> Vec<(&'b wgpu::Buffer, u32, usize)> {
    let mut left = total as u32;
    let mut out = Vec::with_capacity(WALK_BUCKETS);
    for (k, (buf, vlen)) in bufs.iter().enumerate() {
        let c = counts.get(k).copied().unwrap_or(0).min(left);
        left -= c;
        out.push((buf, *vlen, c as usize));
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
    crystal_pipeline: wgpu::RenderPipeline,
    ring_pipeline: wgpu::RenderPipeline,
    terrain_vbuf: wgpu::Buffer,
    terrain_ibuf: wgpu::Buffer,
    terrain_indices: u32,
    water_buf: wgpu::Buffer,
    water_len: u32,
    /// Idle base + walk keyframe vertex buffers (and counts) per walking kind.
    infantry_bufs: Vec<(wgpu::Buffer, u32)>,
    barracks_astro_buf: wgpu::Buffer,
    barracks_astro_len: u32,
    barracks_hollow_buf: wgpu::Buffer,
    barracks_hollow_len: u32,
    hq_astro_buf: wgpu::Buffer,
    hq_astro_len: u32,
    hq_hollow_buf: wgpu::Buffer,
    hq_hollow_len: u32,
    acolyte_buf: wgpu::Buffer,
    acolyte_len: u32,
    engineer_bufs: Vec<(wgpu::Buffer, u32)>,
    ore_node_buf: wgpu::Buffer,
    ore_node_len: u32,
    carbon_node_buf: wgpu::Buffer,
    carbon_node_len: u32,
    ore_crystal_buf: wgpu::Buffer,
    ore_crystal_len: u32,
    carbon_pool_buf: wgpu::Buffer,
    carbon_pool_len: u32,
    barrel_buf: wgpu::Buffer,
    barrel_len: u32,
    turret_buf: wgpu::Buffer,
    turret_len: u32,
    supply_buf: wgpu::Buffer,
    supply_len: u32,
    ward_astro_buf: wgpu::Buffer,
    ward_astro_len: u32,
    supply_astro_buf: wgpu::Buffer,
    supply_astro_len: u32,
    particle_buf: wgpu::Buffer,
    particle_len: u32,
    heavy_bufs: Vec<(wgpu::Buffer, u32)>,
    pyromancer_buf: wgpu::Buffer,
    pyromancer_len: u32,
    stormcaller_buf: wgpu::Buffer,
    stormcaller_len: u32,
    hound_bufs: Vec<(wgpu::Buffer, u32)>,
    javelin_bufs: Vec<(wgpu::Buffer, u32)>,
    storm_ward_buf: wgpu::Buffer,
    storm_ward_len: u32,
    bunker_buf: wgpu::Buffer,
    bunker_len: u32,
    walls_buf: wgpu::Buffer,
    walls_len: u32,
    wall_inst_buf: wgpu::Buffer,
    voxel_pipeline: wgpu::RenderPipeline,
    /// Triplanar-textured marching-cubes terrain for the active voxel map
    /// (vertex buffer + count + its tile bind group), or `None` for the heightmap.
    voxel_terrain: Option<(wgpu::Buffer, u32, wgpu::BindGroup)>,
    /// Per-column liquid surface mesh for the active voxel map (Titan/Earth).
    voxel_water: Option<(wgpu::Buffer, u32)>,
    instance_buf: wgpu::Buffer,
    ring_buf: wgpu::Buffer,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    /// The model surface-detail map (group 3 of the unit pipeline).
    detail_bind: wgpu::BindGroup,
    terrain_bind: wgpu::BindGroup,
    terrain_layout: wgpu::BindGroupLayout,
    tile_sampler: wgpu::Sampler,
    fow_sampler: wgpu::Sampler,
    world_buf: wgpu::Buffer,
    world_bind: wgpu::BindGroup,
    fow_tex: wgpu::Texture,
    pub width: u32,
    pub height: u32,
}

/// Make a GPU vertex buffer from raw vertex bytes.
fn vbuf(device: &wgpu::Device, queue: &wgpu::Queue, label: &str, data: &[u8]) -> wgpu::Buffer {
    let b = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: data.len() as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&b, 0, data);
    b
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

        // Linear filtering gives the ground tiles a soft painterly blur up
        // close instead of hard texel blocks.
        let tile_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tile-samp"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
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
                tex_entry(0, true), // grass - linear (soft painterly blur)
                tex_entry(1, true), // dirt
                tex_entry(2, true), // rock
                tex_entry(3, true), // sand
                tex_entry(4, true), // fog of war - linear-filtered soft borders
                samp_entry(5, wgpu::SamplerBindingType::Filtering), // tiles
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

        // group 3 (unit pipeline): the model surface-detail map, triplanar
        // sampled in object space so units/buildings are textured without UVs.
        let detail = load_tile(
            &device,
            &queue,
            include_bytes!("../../../assets/textures/detail.png"),
            "detail",
        );
        let detail_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("detail-layout"),
            entries: &[
                tex_entry(0, true),
                samp_entry(1, wgpu::SamplerBindingType::Filtering),
            ],
        });
        let detail_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("detail-bind"),
            layout: &detail_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&detail),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&tile_sampler),
                },
            ],
        });

        // group 2: per-world appearance (voxel terrain tint + liquid colour).
        let world_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-layout"),
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
        let world_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("world"),
            size: std::mem::size_of::<WorldUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(
            &world_buf,
            0,
            bytemuck::bytes_of(&WorldUniform {
                tint: [1.0, 1.0, 1.0, 0.0],
                liquid: EARTH_WATER,
            }),
        );
        let world_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-bind"),
            layout: &world_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: world_buf.as_entire_binding(),
            }],
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
        // Units/buildings: camera + the surface-detail map at group 3
        // (groups 1 and 2 are holes; fs_unit touches neither).
        let pl_unit = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl-unit"),
            bind_group_layouts: &[Some(&camera_layout), None, None, Some(&detail_layout)],
            immediate_size: 0,
        });
        // Voxel terrain + water sample the world uniform at group 2.
        let pl_world = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl-world"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&terrain_layout),
                Some(&world_layout),
            ],
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
        // Ground decals (selection rings, order pings): a strong negative depth
        // bias pulls them toward the camera so they never z-fight or clip into
        // slope geometry between their tessellation samples.
        let depth_decal = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: -8,
                slope_scale: -8.0,
                clamp: 0.0,
            },
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
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 5 => Float32x4, 8 => Float32],
        };
        let pos3 = wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        };
        let voxel_vbl = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<VoxelVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32],
        };
        let inst = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32x3, 4 => Float32x4, 6 => Float32x2],
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
            &pl_unit,
            "vs_unit",
            "fs_unit",
            &[v3u.clone(), inst.clone()],
            &opaque_t,
            &depth_opaque,
        );
        // Crystals/gas pools: same instancing as units, but alpha-blended
        // with env-mapped shine (drawn after the opaque world and water).
        // Depth writes stay ON, and fs_crystal discards back-facing
        // fragments by the authored outward normal (mesh winding is not
        // consistent, so hardware face culling would cut the wrong faces);
        // together only the nearest front-facing facet blends, instead of
        // interior and rear facets stacking in arbitrary triangle order.
        let depth_crystal = wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let crystal_pipeline = mk(
            "crystal",
            &pl_plain,
            "vs_unit",
            "fs_crystal",
            &[v3u, inst],
            &blend_t,
            &depth_crystal,
        );
        let voxel_pipeline = mk(
            "voxel",
            &pl_world,
            "vs_voxel",
            "fs_voxel",
            std::slice::from_ref(&voxel_vbl),
            &opaque_t,
            &depth_opaque,
        );
        let water_pipeline = mk(
            "water",
            &pl_world,
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
            &depth_decal,
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
        // Walking units load idle + their baked walk keyframes; each bucket
        // gets its own vertex buffer and instances are grouped per frame.
        let mkframes = |name: &str, frames: Vec<Vec<UnitVertex>>| -> Vec<(wgpu::Buffer, u32)> {
            frames
                .iter()
                .enumerate()
                .map(|(k, m)| {
                    (
                        mkbuf(
                            &format!("{name}-{k}"),
                            bytemuck::cast_slice(m),
                            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        ),
                        m.len() as u32,
                    )
                })
                .collect()
        };
        let infantry_bufs = mkframes("infantry", walk_models!("infantry"));
        let engineer_bufs = mkframes("engineer", walk_models!("engineer"));
        let heavy_bufs = mkframes("heavy", walk_models!("heavy"));
        let hound_bufs = mkframes("hound", walk_models!("hound"));
        let javelin_bufs = mkframes("javelin", walk_models!("javelin"));
        let pyromancer = model!("pyromancer");
        let pyromancer_buf = mkbuf(
            "pyromancer",
            bytemuck::cast_slice(&pyromancer),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let stormcaller = model!("stormcaller");
        let stormcaller_buf = mkbuf(
            "stormcaller",
            bytemuck::cast_slice(&stormcaller),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let storm_ward = model!("storm-ward");
        let storm_ward_buf = mkbuf(
            "storm-ward",
            bytemuck::cast_slice(&storm_ward),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let bunker = model!("bunker");
        let bunker_buf = mkbuf(
            "bunker",
            bytemuck::cast_slice(&bunker),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let barracks_astro = model!("barracks-astro");
        let barracks_astro_buf = mkbuf(
            "barracks-astro",
            bytemuck::cast_slice(&barracks_astro),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let barracks_hollow = model!("barracks-hollow");
        let barracks_hollow_buf = mkbuf(
            "barracks-hollow",
            bytemuck::cast_slice(&barracks_hollow),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let hq_astro = model!("hq-astro");
        let hq_astro_buf = mkbuf(
            "hq-astro",
            bytemuck::cast_slice(&hq_astro),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let hq_hollow = model!("hq-hollow");
        let hq_hollow_buf = mkbuf(
            "hq-hollow",
            bytemuck::cast_slice(&hq_hollow),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let acolyte = model!("acolyte");
        let acolyte_buf = mkbuf(
            "acolyte",
            bytemuck::cast_slice(&acolyte),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let ore_node = model!("ore-node");
        let ore_node_buf = mkbuf(
            "ore-node",
            bytemuck::cast_slice(&ore_node),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let carbon_node = model!("carbon-node");
        let carbon_node_buf = mkbuf(
            "carbon-node",
            bytemuck::cast_slice(&carbon_node),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let ore_crystal = model!("ore-crystal");
        let ore_crystal_buf = mkbuf(
            "ore-crystal",
            bytemuck::cast_slice(&ore_crystal),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let carbon_pool = model!("carbon-pool");
        let carbon_pool_buf = mkbuf(
            "carbon-pool",
            bytemuck::cast_slice(&carbon_pool),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let barrel = model!("barrel");
        let barrel_buf = mkbuf(
            "barrel",
            bytemuck::cast_slice(&barrel),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let turret = model!("turret");
        let turret_buf = mkbuf(
            "turret",
            bytemuck::cast_slice(&turret),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let supply = model!("supply");
        let supply_buf = mkbuf(
            "supply",
            bytemuck::cast_slice(&supply),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let ward_astro = model!("ward-astro");
        let ward_astro_buf = mkbuf(
            "ward-astro",
            bytemuck::cast_slice(&ward_astro),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let supply_astro = model!("supply-astro");
        let supply_astro_buf = mkbuf(
            "supply-astro",
            bytemuck::cast_slice(&supply_astro),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let particle = particle_mesh();
        let particle_buf = mkbuf(
            "particle",
            bytemuck::cast_slice(&particle),
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
                rot: ROT_NONE,
            }),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        // The voxel battlefield (terrain mesh + tiles + liquid) is built lazily
        // by `set_world` when the lobby picks a map; the default is the heightmap.
        let voxel_terrain: Option<(wgpu::Buffer, u32, wgpu::BindGroup)> = None;
        let voxel_water: Option<(wgpu::Buffer, u32)> = None;
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
            crystal_pipeline,
            ring_pipeline,
            terrain_vbuf,
            terrain_ibuf,
            terrain_indices: ti.len() as u32,
            water_buf,
            water_len: water.len() as u32,
            infantry_bufs,
            barracks_astro_buf,
            barracks_astro_len: barracks_astro.len() as u32,
            barracks_hollow_buf,
            barracks_hollow_len: barracks_hollow.len() as u32,
            hq_astro_buf,
            hq_astro_len: hq_astro.len() as u32,
            hq_hollow_buf,
            hq_hollow_len: hq_hollow.len() as u32,
            acolyte_buf,
            acolyte_len: acolyte.len() as u32,
            engineer_bufs,
            ore_node_buf,
            ore_node_len: ore_node.len() as u32,
            carbon_node_buf,
            carbon_node_len: carbon_node.len() as u32,
            ore_crystal_buf,
            ore_crystal_len: ore_crystal.len() as u32,
            carbon_pool_buf,
            carbon_pool_len: carbon_pool.len() as u32,
            barrel_buf,
            barrel_len: barrel.len() as u32,
            turret_buf,
            turret_len: turret.len() as u32,
            supply_buf,
            supply_len: supply.len() as u32,
            ward_astro_buf,
            ward_astro_len: ward_astro.len() as u32,
            supply_astro_buf,
            supply_astro_len: supply_astro.len() as u32,
            particle_buf,
            particle_len: particle.len() as u32,
            heavy_bufs,
            pyromancer_buf,
            pyromancer_len: pyromancer.len() as u32,
            stormcaller_buf,
            stormcaller_len: stormcaller.len() as u32,
            hound_bufs,
            javelin_bufs,
            storm_ward_buf,
            storm_ward_len: storm_ward.len() as u32,
            bunker_buf,
            bunker_len: bunker.len() as u32,
            walls_buf,
            walls_len: walls.len() as u32,
            wall_inst_buf,
            voxel_pipeline,
            voxel_terrain,
            voxel_water,
            instance_buf,
            ring_buf,
            camera_buf,
            camera_bind,
            detail_bind,
            terrain_bind,
            terrain_layout,
            tile_sampler,
            fow_sampler,
            world_buf,
            world_bind,
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

    /// Rebuild the terrain for the currently selected voxel map (call after
    /// `voxel::set_active`, e.g. when starting a match from the lobby): triplanar
    /// terrain mesh + the world's tile set + its liquid surface + tint/liquid
    /// uniform. With no map selected this clears back to the Earthlike heightmap.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))] // driven by the web lobby
    pub fn set_world(&mut self) {
        let Some(grid) = crate::voxel::active() else {
            self.voxel_terrain = None;
            self.voxel_water = None;
            self.write_world(WorldUniform {
                tint: [1.0, 1.0, 1.0, 0.0],
                liquid: EARTH_WATER,
            });
            return;
        };

        // Terrain mesh (pos/normal/material) for the triplanar voxel pipeline.
        let verts: Vec<VoxelVertex> = grid
            .build_mesh()
            .iter()
            .map(|v| VoxelVertex {
                pos: v.pos,
                normal: v.normal,
                weights: v.weights,
                haz: v.haz,
            })
            .collect();
        let buf = vbuf(
            &self.device,
            &self.queue,
            "voxel-terrain",
            bytemuck::cast_slice(&verts),
        );

        // The world's tile set in the base/low/high/accent (grass/dirt/rock/sand)
        // slots, plus the shared fog-of-war texture.
        let tiles = crate::voxel::active_tiles().expect("active map has a tile set");
        let tex: Vec<wgpu::TextureView> = tiles
            .iter()
            .enumerate()
            .map(|(k, b)| load_tile(&self.device, &self.queue, b, &format!("voxel-tile-{k}")))
            .collect();
        let fow_view = self
            .fow_tex
            .create_view(&wgpu::TextureViewDescriptor::default());
        let voxel_bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("voxel-bind"),
            layout: &self.terrain_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tex[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&tex[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&tex[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&tex[3]),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&fow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::Sampler(&self.tile_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::Sampler(&self.fow_sampler),
                },
            ],
        });
        self.voxel_terrain = Some((buf, verts.len() as u32, voxel_bind));

        // Liquid surface mesh (oceans / methane), if any.
        let liq = grid.liquid_mesh();
        self.voxel_water = if liq.is_empty() {
            None
        } else {
            Some((
                vbuf(
                    &self.device,
                    &self.queue,
                    "voxel-water",
                    bytemuck::cast_slice(&liq),
                ),
                liq.len() as u32,
            ))
        };

        // Tint + liquid colour for this world.
        let lava = if crate::voxel::active_lava() {
            1.0
        } else {
            0.0
        };
        let liquid = crate::voxel::active_liquid().unwrap_or(EARTH_WATER);
        let t = crate::voxel::active_tint();
        self.write_world(WorldUniform {
            tint: [t[0], t[1], t[2], lava],
            liquid,
        });
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    fn write_world(&self, w: WorldUniform) {
        self.queue
            .write_buffer(&self.world_buf, 0, bytemuck::bytes_of(&w));
    }

    /// Rebuild the heightmap ground mesh after the building pads changed
    /// (`terrain::set_pads`). The grid topology is fixed, so only the vertex
    /// buffer is rewritten. Voxel battlefields are unaffected (their surface
    /// comes from the baked map, not the height function).
    pub fn rebuild_terrain(&mut self) {
        let (tv, _) = terrain_mesh();
        self.queue
            .write_buffer(&self.terrain_vbuf, 0, bytemuck::cast_slice(&tv));
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        infantry: &[InstanceRaw],
        infantry_frames: &[u32; WALK_BUCKETS],
        barracks_astro: &[InstanceRaw],
        barracks_hollow: &[InstanceRaw],
        hq_astro: &[InstanceRaw],
        hq_hollow: &[InstanceRaw],
        acolytes: &[InstanceRaw],
        engineers: &[InstanceRaw],
        engineer_frames: &[u32; WALK_BUCKETS],
        ore_nodes: &[InstanceRaw],
        carbon_nodes: &[InstanceRaw],
        heavies: &[InstanceRaw],
        heavy_frames: &[u32; WALK_BUCKETS],
        pyromancers: &[InstanceRaw],
        stormcallers: &[InstanceRaw],
        hounds: &[InstanceRaw],
        hound_frames: &[u32; WALK_BUCKETS],
        javelins: &[InstanceRaw],
        javelin_frames: &[u32; WALK_BUCKETS],
        storm_wards: &[InstanceRaw],
        bunkers: &[InstanceRaw],
        turrets: &[InstanceRaw],
        supplies: &[InstanceRaw],
        wards_astro: &[InstanceRaw],
        supplies_astro: &[InstanceRaw],
        barrels: &[InstanceRaw],
        particles: &[InstanceRaw],
        ore_crystals: &[InstanceRaw],
        carbon_pools: &[InstanceRaw],
        fx_lights: &[FxLight],
        rings: &[RingRaw],
        shadows: &[RingRaw],
        fow: &[u8],
        view_proj: [[f32; 4]; 4],
        eye: [f32; 3],
        time: f32,
    ) {
        // All meshes share one instance buffer, packed in order: infantry,
        // the faction barracks, the faction HQs, Acolytes, Engineers, ore
        // nodes, carbon nodes, heavies, turrets, supply depots, sludge
        // barrels, particles, then the alpha-blended crystal groups last.
        // Each mesh draws its own range.
        let groups = [
            infantry.len(),
            barracks_astro.len(),
            barracks_hollow.len(),
            hq_astro.len(),
            hq_hollow.len(),
            acolytes.len(),
            engineers.len(),
            ore_nodes.len(),
            carbon_nodes.len(),
            heavies.len(),
            pyromancers.len(),
            stormcallers.len(),
            hounds.len(),
            javelins.len(),
            storm_wards.len(),
            bunkers.len(),
            turrets.len(),
            supplies.len(),
            wards_astro.len(),
            supplies_astro.len(),
            barrels.len(),
            particles.len(),
            ore_crystals.len(),
            carbon_pools.len(),
        ];
        // Clamp each group's count so the running total never exceeds the buffer.
        let mut counts = [0usize; 24];
        let mut used = 0usize;
        for (c, &g) in counts.iter_mut().zip(groups.iter()) {
            *c = g.min(MAX_INSTANCES - used);
            used += *c;
        }
        let [ni, na, nh, nqa, nqh, nac, nen, nor, ncar, nhv, npy, nsc, nhd, njv, nsw, nbk, ntr, nsp, nwa, nsa, nbr, npt, noc, ncp] =
            counts;
        // Blob shadows draw first (under the rings) through the ring decal
        // pipeline: soft radial fans on the terrain.
        let mut ring_verts = shadow_decals(shadows);
        ring_verts.extend(ring_decals(rings));
        let nrv = ring_verts.len().min(MAX_RING_VERTS);
        let mut light_pos = [[0.0f32; 4]; MAX_LIGHTS];
        let mut light_col = [[0.0f32; 4]; MAX_LIGHTS];
        let nlights = fx_lights.len().min(MAX_LIGHTS);
        for (k, l) in fx_lights.iter().take(nlights).enumerate() {
            light_pos[k] = [l.pos[0], l.pos[1], l.pos[2], l.radius];
            light_col[k] = [l.color[0], l.color[1], l.color[2], 0.0];
        }
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj,
                eye: [eye[0], eye[1], eye[2], 1.0],
                light_dir: [0.5, 1.0, 0.35, 0.0],
                params: [time, terrain::HALF, terrain::SEA_LEVEL, nlights as f32],
                light_pos,
                light_col,
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
        let slices = [
            &infantry[..ni],
            &barracks_astro[..na],
            &barracks_hollow[..nh],
            &hq_astro[..nqa],
            &hq_hollow[..nqh],
            &acolytes[..nac],
            &engineers[..nen],
            &ore_nodes[..nor],
            &carbon_nodes[..ncar],
            &heavies[..nhv],
            &pyromancers[..npy],
            &stormcallers[..nsc],
            &hounds[..nhd],
            &javelins[..njv],
            &storm_wards[..nsw],
            &bunkers[..nbk],
            &turrets[..ntr],
            &supplies[..nsp],
            &wards_astro[..nwa],
            &supplies_astro[..nsa],
            &barrels[..nbr],
            &particles[..npt],
            &ore_crystals[..noc],
            &carbon_pools[..ncp],
        ];
        let mut off = 0u64;
        for s in slices {
            if !s.is_empty() {
                self.queue
                    .write_buffer(&self.instance_buf, off * stride, bytemuck::cast_slice(s));
            }
            off += s.len() as u64;
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
            // The unit pipeline's surface-detail map rides at group 3 for
            // the whole pass; other pipelines simply ignore it.
            pass.set_bind_group(3, &self.detail_bind, &[]);

            let voxel = self.voxel_terrain.is_some();
            if let Some((buf, len, bind)) = self.voxel_terrain.as_ref() {
                // Marching-cubes battlefield: triplanar-textured, tinted per world.
                pass.set_pipeline(&self.voxel_pipeline);
                pass.set_bind_group(1, bind, &[]);
                pass.set_bind_group(2, &self.world_bind, &[]);
                pass.set_vertex_buffer(0, buf.slice(..));
                pass.draw(0..*len, 0..1);
            } else {
                pass.set_pipeline(&self.terrain_pipeline);
                pass.set_bind_group(1, &self.terrain_bind, &[]);
                pass.set_vertex_buffer(0, self.terrain_vbuf.slice(..));
                pass.set_index_buffer(self.terrain_ibuf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.terrain_indices, 0, 0..1);
            }

            pass.set_pipeline(&self.unit_pipeline);
            // Map-rim walls hide under-ocean at the edges; only the Earthlike map
            // has an ocean plane, so skip them on the airless voxel worlds.
            if !voxel {
                pass.set_vertex_buffer(0, self.walls_buf.slice(..));
                pass.set_vertex_buffer(1, self.wall_inst_buf.slice(..));
                pass.draw(0..self.walls_len, 0..1);
            }
            if used > 0 {
                pass.set_vertex_buffer(1, self.instance_buf.slice(..));
                // (mesh vertex buffer, mesh vertex count, instance count) per group,
                // drawn over consecutive instance ranges matching the packing above.
                // Walking kinds expand into one sub-draw per animation frame
                // bucket (their instances are packed idle first, then frame
                // 0..N, with `*_frames` carrying the per-bucket counts).
                let mut meshes: Vec<(&wgpu::Buffer, u32, usize)> = Vec::new();
                meshes.extend(frame_draws(
                    self.infantry_bufs.as_slice(),
                    infantry_frames,
                    ni,
                ));
                meshes.push((&self.barracks_astro_buf, self.barracks_astro_len, na));
                meshes.push((&self.barracks_hollow_buf, self.barracks_hollow_len, nh));
                meshes.push((&self.hq_astro_buf, self.hq_astro_len, nqa));
                meshes.push((&self.hq_hollow_buf, self.hq_hollow_len, nqh));
                meshes.push((&self.acolyte_buf, self.acolyte_len, nac));
                meshes.extend(frame_draws(
                    self.engineer_bufs.as_slice(),
                    engineer_frames,
                    nen,
                ));
                meshes.push((&self.ore_node_buf, self.ore_node_len, nor));
                meshes.push((&self.carbon_node_buf, self.carbon_node_len, ncar));
                meshes.extend(frame_draws(self.heavy_bufs.as_slice(), heavy_frames, nhv));
                meshes.push((&self.pyromancer_buf, self.pyromancer_len, npy));
                meshes.push((&self.stormcaller_buf, self.stormcaller_len, nsc));
                meshes.extend(frame_draws(self.hound_bufs.as_slice(), hound_frames, nhd));
                meshes.extend(frame_draws(
                    self.javelin_bufs.as_slice(),
                    javelin_frames,
                    njv,
                ));
                meshes.push((&self.storm_ward_buf, self.storm_ward_len, nsw));
                meshes.push((&self.bunker_buf, self.bunker_len, nbk));
                meshes.push((&self.turret_buf, self.turret_len, ntr));
                meshes.push((&self.supply_buf, self.supply_len, nsp));
                meshes.push((&self.ward_astro_buf, self.ward_astro_len, nwa));
                meshes.push((&self.supply_astro_buf, self.supply_astro_len, nsa));
                meshes.push((&self.barrel_buf, self.barrel_len, nbr));
                meshes.push((&self.particle_buf, self.particle_len, npt));
                let mut base = 0u32;
                for (buf, vlen, count) in meshes {
                    let count = count as u32;
                    if count > 0 {
                        pass.set_vertex_buffer(0, buf.slice(..));
                        pass.draw(0..vlen, base..base + count);
                    }
                    base += count;
                }
            }

            // Liquid: the Earthlike ocean plane, or a voxel world's per-column
            // liquid surface (Earth seas / Titan methane). Revamped water shader.
            pass.set_pipeline(&self.water_pipeline);
            pass.set_bind_group(2, &self.world_bind, &[]);
            if let Some((wbuf, wlen)) = self.voxel_water.as_ref() {
                let bind = self
                    .voxel_terrain
                    .as_ref()
                    .map(|t| &t.2)
                    .unwrap_or(&self.terrain_bind);
                pass.set_bind_group(1, bind, &[]);
                pass.set_vertex_buffer(0, wbuf.slice(..));
                pass.draw(0..*wlen, 0..1);
            } else if !voxel {
                pass.set_bind_group(1, &self.terrain_bind, &[]);
                pass.set_vertex_buffer(0, self.water_buf.slice(..));
                pass.draw(0..self.water_len, 0..1);
            }

            if nrv > 0 {
                pass.set_pipeline(&self.ring_pipeline);
                pass.set_vertex_buffer(0, self.ring_buf.slice(..));
                pass.draw(0..nrv as u32, 0..1);
            }

            // Crystals and gas pools last: alpha-blended over the opaque
            // world, water and ground decals. Their instances sit at the
            // tail of the shared buffer (slot 1 is still bound).
            if noc + ncp > 0 {
                pass.set_pipeline(&self.crystal_pipeline);
                // Everything before the two blended groups, derived from the
                // packed total so adding an opaque group can't desync it.
                let opaque: u32 = (used - noc - ncp) as u32;
                if noc > 0 {
                    pass.set_vertex_buffer(0, self.ore_crystal_buf.slice(..));
                    pass.draw(0..self.ore_crystal_len, opaque..opaque + noc as u32);
                }
                if ncp > 0 {
                    pass.set_vertex_buffer(0, self.carbon_pool_buf.slice(..));
                    let b = opaque + noc as u32;
                    pass.draw(0..self.carbon_pool_len, b..b + ncp as u32);
                }
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
            assert!(
                v.detail == DETAIL_FLAT || v.detail == DETAIL_ROCK || v.detail == DETAIL_PLATE,
                "{name}[{k}] bad detail selector"
            );
        }
    }

    #[test]
    fn unit_meshes_are_well_formed() {
        check_mesh(&model!("infantry"), "infantry");
        check_mesh(&model!("barracks-astro"), "barracks-astro");
        check_mesh(&model!("barracks-hollow"), "barracks-hollow");
        check_mesh(&model!("hq-astro"), "hq-astro");
        check_mesh(&model!("hq-hollow"), "hq-hollow");
        check_mesh(&model!("acolyte"), "acolyte");
        check_mesh(&model!("engineer"), "engineer");
        check_mesh(&model!("ore-node"), "ore-node");
        check_mesh(&model!("carbon-node"), "carbon-node");
        check_mesh(&model!("ore-crystal"), "ore-crystal");
        check_mesh(&model!("carbon-pool"), "carbon-pool");
        check_mesh(&model!("barrel"), "barrel");
        check_mesh(&model!("turret"), "turret");
        check_mesh(&model!("supply"), "supply");
        check_mesh(&model!("ward-astro"), "ward-astro");
        check_mesh(&model!("supply-astro"), "supply-astro");
        check_mesh(&particle_mesh(), "particle");
        check_mesh(&model!("heavy"), "heavy");
        // The walk-cycle keyframe assets: idle + every baked frame.
        for (name, frames) in [
            ("infantry", walk_models!("infantry")),
            ("engineer", walk_models!("engineer")),
            ("heavy", walk_models!("heavy")),
        ] {
            assert_eq!(frames.len(), WALK_BUCKETS, "{name} walk set size");
            for (k, m) in frames.iter().enumerate() {
                check_mesh(m, &format!("{name}-frame-{k}"));
            }
        }
    }

    /// Renders the infantry walk-cycle keyframes side by side to
    /// `target/previews/infantry-walk.png`. A dev tool: run on demand with
    /// `cargo test -p client render_walk_strip -- --ignored`.
    #[test]
    #[ignore = "writes the walk strip to target/previews; run on demand"]
    fn render_walk_strip() {
        let team = [0.25, 0.55, 1.0];
        let frames = walk_models!("infantry");
        let (fw, fh) = (170u32, 320u32);
        let w = 12 + frames.len() as u32 * (fw + 12);
        let mut sheet = image::RgbaImage::from_pixel(w, fh + 24, image::Rgba([10, 14, 24, 255]));
        for (k, mesh) in frames.iter().enumerate() {
            let img = rasterize(mesh, team, fw, fh);
            let x0 = 12 + k as u32 * (fw + 12);
            for (px, py, p) in img.enumerate_pixels() {
                if x0 + px < w {
                    sheet.put_pixel(x0 + px, 12 + py, *p);
                }
            }
        }
        std::fs::create_dir_all("target/previews").unwrap();
        sheet.save("target/previews/infantry-walk.png").unwrap();
    }

    /// Offline mesh preview: rasterizes a mesh with the game's iso camera
    /// (yaw 45, pitch 0.95) and the unit shader's lighting, so the PNG shows
    /// exactly what the renderer would draw. CPU-only (no GPU needed).
    fn rasterize(mesh: &[UnitVertex], team: [f32; 3], w: u32, h: u32) -> image::RgbaImage {
        let (yaw, pitch) = (std::f32::consts::FRAC_PI_4, 0.95_f32);
        // The camera's eye direction; screen axes are perpendicular to it.
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

        // Fit the mesh into the image with a margin.
        let (mut u0, mut u1, mut v0, mut v1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for v in mesh {
            let (u, vv, _) = project(v.pos);
            u0 = u0.min(u);
            u1 = u1.max(u);
            v0 = v0.min(vv);
            v1 = v1.max(vv);
        }
        let scale = ((w as f32 - 60.0) / (u1 - u0)).min((h as f32 - 60.0) / (v1 - v0));
        let to_px = |u: f32, v: f32| {
            (
                (u - u0) * scale + (w as f32 - (u1 - u0) * scale) * 0.5,
                // v points up; image y points down.
                (v1 - v) * scale + (h as f32 - (v1 - v0) * scale) * 0.5,
            )
        };

        // Same lighting as fs_unit: albedo * (0.45 + 0.7 * n.l).
        let ll = (0.5_f32 * 0.5 + 1.0 + 0.35 * 0.35).sqrt();
        let light = [0.5 / ll, 1.0 / ll, 0.35 / ll];

        // The shader's surface-detail map, decoded sRGB -> linear and
        // triplanar-sampled exactly like fs_unit's `detail_at`, so the
        // preview shows the textured models the renderer draws.
        let det_img =
            image::load_from_memory(include_bytes!("../../../assets/textures/detail.png"))
                .expect("decode detail")
                .to_rgba8();
        let (dw, dh) = (det_img.width() as usize, det_img.height() as usize);
        let det_lin: Vec<[f32; 2]> = det_img
            .pixels()
            .map(|p| {
                [
                    (p[0] as f32 / 255.0).powf(2.2),
                    (p[1] as f32 / 255.0).powf(2.2),
                ]
            })
            .collect();
        let det_sample = |u: f32, v: f32, ch: usize| -> f32 {
            let fx = u.rem_euclid(1.0) * dw as f32;
            let fy = v.rem_euclid(1.0) * dh as f32;
            let (x0, y0) = (fx as usize % dw, fy as usize % dh);
            let (x1, y1) = ((x0 + 1) % dw, (y0 + 1) % dh);
            let (tx, ty) = (fx.fract(), fy.fract());
            let at = |x: usize, y: usize| det_lin[y * dw + x][ch];
            let a = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * tx;
            let b = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * tx;
            a + (b - a) * ty
        };
        let detail_at = |p: [f32; 3], n: [f32; 3], ch: usize| -> f32 {
            let mut wgt = [n[0].abs().powi(4), n[1].abs().powi(4), n[2].abs().powi(4)];
            let sum = (wgt[0] + wgt[1] + wgt[2]).max(0.001);
            for v in &mut wgt {
                *v /= sum;
            }
            let s = 0.45;
            wgt[0] * det_sample(p[2] * s, p[1] * s, ch)
                + wgt[1] * det_sample(p[0] * s, p[2] * s, ch)
                + wgt[2] * det_sample(p[0] * s, p[1] * s, ch)
        };

        let mut img = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 14, 24, 255]));
        let mut depth = vec![f32::MIN; (w * h) as usize];
        for tri in mesh.chunks_exact(3) {
            let albedo = |v: &UnitVertex| {
                [
                    v.color[0] + (team[0] - v.color[0]) * v.color[3],
                    v.color[1] + (team[1] - v.color[1]) * v.color[3],
                    v.color[2] + (team[2] - v.color[2]) * v.color[3],
                ]
            };
            let shade = 0.45 + 0.7 * dot(tri[0].normal, light).max(0.0);
            let base = albedo(&tri[0]);
            let p: Vec<(f32, f32, f32)> = tri
                .iter()
                .map(|v| {
                    let (u, vv, d) = project(v.pos);
                    let (x, y) = to_px(u, vv);
                    (x, y, d)
                })
                .collect();
            let area =
                (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
            if area.abs() < 1e-6 {
                continue;
            }
            let xmin = p.iter().map(|q| q.0).fold(f32::MAX, f32::min).max(0.0) as u32;
            let xmax = (p
                .iter()
                .map(|q| q.0)
                .fold(f32::MIN, f32::max)
                .min(w as f32 - 1.0)) as u32;
            let ymin = p.iter().map(|q| q.1).fold(f32::MAX, f32::min).max(0.0) as u32;
            let ymax = (p
                .iter()
                .map(|q| q.1)
                .fold(f32::MIN, f32::max)
                .min(h as f32 - 1.0)) as u32;
            for py in ymin..=ymax {
                for px in xmin..=xmax {
                    let (fx, fy) = (px as f32 + 0.5, py as f32 + 0.5);
                    let w0 = ((p[1].0 - fx) * (p[2].1 - fy) - (p[2].0 - fx) * (p[1].1 - fy)) / area;
                    let w1 = ((p[2].0 - fx) * (p[0].1 - fy) - (p[0].0 - fx) * (p[2].1 - fy)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    let d = w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2;
                    let i = (py * w + px) as usize;
                    if d > depth[i] {
                        depth[i] = d;
                        // Interpolate the model-space position and apply the
                        // selected detail channel per pixel, like the shader
                        // (selector 0 stays flat shaded).
                        let det = if tri[0].detail < 0.5 {
                            1.0
                        } else {
                            let pos = [
                                w0 * tri[0].pos[0] + w1 * tri[1].pos[0] + w2 * tri[2].pos[0],
                                w0 * tri[0].pos[1] + w1 * tri[1].pos[1] + w2 * tri[2].pos[1],
                                w0 * tri[0].pos[2] + w1 * tri[1].pos[2] + w2 * tri[2].pos[2],
                            ];
                            let ch = if tri[0].detail > 1.5 { 0 } else { 1 };
                            0.70 + 1.40 * detail_at(pos, tri[0].normal, ch)
                        };
                        let rgb = [
                            ((base[0] * shade * det).clamp(0.0, 1.0) * 255.0) as u8,
                            ((base[1] * shade * det).clamp(0.0, 1.0) * 255.0) as u8,
                            ((base[2] * shade * det).clamp(0.0, 1.0) * 255.0) as u8,
                        ];
                        img.put_pixel(px, py, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
                    }
                }
            }
        }
        img
    }

    /// Renders a preview PNG of every building mesh to `target/previews/`.
    /// A dev tool, not a check: run on demand with
    /// `cargo test -p client render_building_previews -- --ignored`.
    #[test]
    #[ignore = "writes preview PNGs to target/previews; run on demand"]
    fn render_building_previews() {
        let team = [0.25, 0.55, 1.0]; // the player's blue
        let mut ore = model!("ore-node");
        ore.extend(model!("ore-crystal"));
        let mut carbon = model!("carbon-node");
        carbon.extend(model!("carbon-pool"));
        let jobs: [(&str, Vec<UnitVertex>); 10] = [
            ("hq-spire-astromancer", model!("hq-astro")),
            ("hq-command-hollowmen", model!("hq-hollow")),
            ("barracks-astromancer", model!("barracks-astro")),
            ("barracks-hollowmen", model!("barracks-hollow")),
            ("turret", model!("turret")),
            ("supply-depot", model!("supply")),
            ("worker-acolyte", model!("acolyte")),
            ("worker-engineer", model!("engineer")),
            ("ore-node", ore),
            ("carbon-node", carbon),
        ];
        std::fs::create_dir_all("target/previews").unwrap();
        for (name, mesh) in jobs {
            let img = rasterize(&mesh, team, 560, 640);
            img.save(format!("target/previews/{name}.png")).unwrap();
        }
    }

    /// Renders the Astromancer concept lineup (Spire, Sanctum, Ward,
    /// Acolyte side by side over the faction palette) to
    /// `target/previews/astromancers-concept.png`. A dev tool for
    /// `docs/factions/astromancers.md`; run on demand with
    /// `cargo test -p client render_astromancer_concept -- --ignored`.
    #[test]
    #[ignore = "writes the concept sheet to target/previews; run on demand"]
    fn render_astromancer_concept() {
        let team = [0.25, 0.55, 1.0];
        let jobs: [(Vec<UnitVertex>, u32); 4] = [
            (model!("hq-astro"), 360),
            (model!("barracks-astro"), 330),
            (model!("turret"), 250),
            (model!("acolyte"), 210),
        ];
        let (w, h) = (1280u32, 580u32);
        let mut sheet = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 14, 24, 255]));
        let mut x = 24u32;
        for (mesh, size) in jobs {
            let img = rasterize(&mesh, team, size, 440);
            for (px, py, p) in img.enumerate_pixels() {
                if p[3] > 0 && x + px < w {
                    sheet.put_pixel(x + px, 30 + py, *p);
                }
            }
            x += size + 24;
        }
        // The faction palette: porcelain shell, indigo shadow, aether cyan,
        // auric gold, team accent.
        let swatches = [
            [233u8, 229, 222],
            [26, 32, 54],
            [140, 230, 255],
            [196, 160, 84],
            [64, 140, 255],
        ];
        for (i, c) in swatches.iter().enumerate() {
            for yy in 0..48u32 {
                for xx in 0..110u32 {
                    let sx = 24 + i as u32 * 122 + xx;
                    if sx < w {
                        sheet.put_pixel(sx, h - 72 + yy, image::Rgba([c[0], c[1], c[2], 255]));
                    }
                }
            }
        }
        std::fs::create_dir_all("target/previews").unwrap();
        sheet
            .save("target/previews/astromancers-concept.png")
            .unwrap();
    }

    /// Renders a small icon PNG of every unit and building mesh to
    /// `target/previews/icons/`. A dev tool, not a check: run on demand with
    /// `cargo test -p client render_unit_icons -- --ignored` and copy the
    /// output to `assets/icons/` (embedded by the HUD command card).
    #[test]
    #[ignore = "writes icon PNGs to target/previews/icons; run on demand"]
    fn render_unit_icons() {
        let team = [0.25, 0.55, 1.0]; // the player's blue
        let jobs: [(&str, Vec<UnitVertex>); 10] = [
            ("hq-astromancer", model!("hq-astro")),
            ("hq-hollowmen", model!("hq-hollow")),
            ("barracks-astromancer", model!("barracks-astro")),
            ("barracks-hollowmen", model!("barracks-hollow")),
            ("turret", model!("turret")),
            ("supply", model!("supply")),
            ("worker-acolyte", model!("acolyte")),
            ("worker-engineer", model!("engineer")),
            ("infantry", model!("infantry")),
            ("heavy", model!("heavy")),
        ];
        std::fs::create_dir_all("target/previews/icons").unwrap();
        for (name, mesh) in jobs {
            let img = rasterize(&mesh, team, 96, 96);
            img.save(format!("target/previews/icons/{name}.png"))
                .unwrap();
        }
    }

    /// Renders the full model lineup (Astromancer row, Hollowmen row, shared
    /// units + neutral nodes row) to `target/previews/fidelity-sheet.png`.
    /// A dev tool for design review; run on demand with
    /// `cargo test -p client render_fidelity_sheet -- --ignored`.
    #[test]
    #[ignore = "writes the fidelity sheet to target/previews; run on demand"]
    fn render_fidelity_sheet() {
        let team = [0.25, 0.55, 1.0]; // the player's blue
        let mut ore = model!("ore-node");
        ore.extend(model!("ore-crystal"));
        let mut carbon = model!("carbon-node");
        carbon.extend(model!("carbon-pool"));
        let rows: [Vec<(Vec<UnitVertex>, u32)>; 3] = [
            vec![
                (model!("hq-astro"), 330),
                (model!("barracks-astro"), 300),
                (model!("ward-astro"), 240),
                (model!("supply-astro"), 240),
                (model!("acolyte"), 170),
            ],
            vec![
                (model!("hq-hollow"), 330),
                (model!("barracks-hollow"), 300),
                (model!("turret"), 240),
                (model!("supply"), 240),
                (model!("engineer"), 170),
            ],
            vec![
                (model!("infantry"), 190),
                (model!("heavy"), 230),
                (ore, 300),
                (carbon, 300),
                (model!("barrel"), 130),
            ],
        ];
        let row_h = 430u32;
        let (w, h) = (1500u32, 24 + 3 * (row_h + 24));
        let mut sheet = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 14, 24, 255]));
        for (r, row) in rows.iter().enumerate() {
            let y0 = 24 + r as u32 * (row_h + 24);
            let mut x = 24u32;
            for (mesh, size) in row {
                let img = rasterize(mesh, team, *size, row_h);
                for (px, py, p) in img.enumerate_pixels() {
                    if p[3] > 0 && x + px < w && y0 + py < h {
                        sheet.put_pixel(x + px, y0 + py, *p);
                    }
                }
                x += size + 24;
            }
        }
        std::fs::create_dir_all("target/previews").unwrap();
        sheet.save("target/previews/fidelity-sheet.png").unwrap();
    }
}
