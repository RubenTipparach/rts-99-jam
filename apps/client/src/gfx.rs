//! wgpu rendering: heightmap terrain, animated water, instanced lit unit boxes,
//! and ground selection rings, with depth + distance fog. Presentation-only.

use crate::terrain;
use std::sync::Arc;
use winit::window::Window;

/// Per-instance unit/building data.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct InstanceRaw {
    pub offset: [f32; 3],
    pub scale: [f32; 3],
    pub color: [f32; 4],
}

/// Per-instance selection ring.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RingRaw {
    pub center: [f32; 3],
    pub radius: f32,
    pub color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex3 {
    pos: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    light_dir: [f32; 4],
    params: [f32; 4], // time, fog density, sea level, _
}

pub const MAX_INSTANCES: usize = 4096;
pub const MAX_RINGS: usize = 512;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

// Unit cube: footprint [-0.5,0.5] in x/z, standing from y=0..1 (scaled by the
// instance), so `offset` places its base on the ground.
fn cube_vertices() -> Vec<Vertex3> {
    let lo = -0.5f32;
    let hi = 0.5f32;
    let p = |x: f32, y: f32, z: f32| [x, y, z];
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        (
            [1.0, 0.0, 0.0],
            [
                p(hi, 0.0, lo),
                p(hi, 0.0, hi),
                p(hi, 1.0, hi),
                p(hi, 1.0, lo),
            ],
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                p(lo, 0.0, hi),
                p(lo, 0.0, lo),
                p(lo, 1.0, lo),
                p(lo, 1.0, hi),
            ],
        ),
        (
            [0.0, 1.0, 0.0],
            [
                p(lo, 1.0, lo),
                p(hi, 1.0, lo),
                p(hi, 1.0, hi),
                p(lo, 1.0, hi),
            ],
        ),
        (
            [0.0, -1.0, 0.0],
            [
                p(lo, 0.0, hi),
                p(hi, 0.0, hi),
                p(hi, 0.0, lo),
                p(lo, 0.0, lo),
            ],
        ),
        (
            [0.0, 0.0, 1.0],
            [
                p(hi, 0.0, hi),
                p(lo, 0.0, hi),
                p(lo, 1.0, hi),
                p(hi, 1.0, hi),
            ],
        ),
        (
            [0.0, 0.0, -1.0],
            [
                p(lo, 0.0, lo),
                p(hi, 0.0, lo),
                p(hi, 1.0, lo),
                p(lo, 1.0, lo),
            ],
        ),
    ];
    let mut out = Vec::with_capacity(36);
    for (normal, q) in faces {
        for i in [0usize, 1, 2, 0, 2, 3] {
            out.push(Vertex3 { pos: q[i], normal });
        }
    }
    out
}

fn terrain_mesh() -> (Vec<Vertex3>, Vec<u32>) {
    let n: usize = 100;
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
            let b = a + 1;
            let c = a + w;
            let d = c + 1;
            idx.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
    (verts, idx)
}

fn water_quad() -> [[f32; 3]; 6] {
    let h = terrain::HALF;
    [
        [-h, 0.0, -h],
        [h, 0.0, -h],
        [h, 0.0, h],
        [-h, 0.0, -h],
        [h, 0.0, h],
        [-h, 0.0, h],
    ]
}

fn ring_quad() -> [[f32; 2]; 6] {
    [
        [-1.0, -1.0],
        [1.0, -1.0],
        [1.0, 1.0],
        [-1.0, -1.0],
        [1.0, 1.0],
        [-1.0, 1.0],
    ]
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

pub struct Gfx {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth: wgpu::TextureView,
    terrain_pipeline: wgpu::RenderPipeline,
    water_pipeline: wgpu::RenderPipeline,
    unit_pipeline: wgpu::RenderPipeline,
    ring_pipeline: wgpu::RenderPipeline,
    terrain_vbuf: wgpu::Buffer,
    terrain_ibuf: wgpu::Buffer,
    terrain_indices: u32,
    water_buf: wgpu::Buffer,
    cube_buf: wgpu::Buffer,
    cube_len: u32,
    ring_quad_buf: wgpu::Buffer,
    instance_buf: wgpu::Buffer,
    ring_buf: wgpu::Buffer,
    camera_buf: wgpu::Buffer,
    camera_bind: wgpu::BindGroup,
    pub width: u32,
    pub height: u32,
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
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pl"),
            bind_group_layouts: &[Some(&camera_layout)],
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
        let opaque_target = wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        };
        let blend_target = wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        };

        let v3 = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex3>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
        };
        let pos3 = wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3],
        };
        let inst = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32x3, 4 => Float32x4],
        };
        let pos2 = wgpu::VertexBufferLayout {
            array_stride: 8,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x2],
        };
        let ring_inst = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RingRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![1 => Float32x3, 2 => Float32, 3 => Float32x4],
        };

        let mk = |label: &str,
                  vs: &str,
                  fs: &str,
                  buffers: &[wgpu::VertexBufferLayout],
                  target: &wgpu::ColorTargetState,
                  depth: &wgpu::DepthStencilState| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
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
            "vs_terrain",
            "fs_terrain",
            std::slice::from_ref(&v3),
            &opaque_target,
            &depth_opaque,
        );
        let unit_pipeline = mk(
            "unit",
            "vs_unit",
            "fs_unit",
            &[v3, inst],
            &opaque_target,
            &depth_opaque,
        );
        let water_pipeline = mk(
            "water",
            "vs_water",
            "fs_water",
            &[pos3],
            &blend_target,
            &depth_blend,
        );
        let ring_pipeline = mk(
            "ring",
            "vs_ring",
            "fs_ring",
            &[pos2, ring_inst],
            &blend_target,
            &depth_blend,
        );

        let buf = |label: &str, data: &[u8], usage: wgpu::BufferUsages| {
            let b = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: data.len() as u64,
                usage,
                mapped_at_creation: false,
            });
            queue.write_buffer(&b, 0, data);
            b
        };

        let (tverts, tidx) = terrain_mesh();
        let terrain_vbuf = buf(
            "terrain-v",
            bytemuck::cast_slice(&tverts),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let terrain_ibuf = buf(
            "terrain-i",
            bytemuck::cast_slice(&tidx),
            wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
        );
        let water = water_quad();
        let water_buf = buf(
            "water",
            bytemuck::cast_slice(&water),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let cube = cube_vertices();
        let cube_buf = buf(
            "cube",
            bytemuck::cast_slice(&cube),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );
        let rq = ring_quad();
        let ring_quad_buf = buf(
            "ringquad",
            bytemuck::cast_slice(&rq),
            wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        );

        let instance_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("instances"),
            size: (MAX_INSTANCES * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ring_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rings"),
            size: (MAX_RINGS * std::mem::size_of::<RingRaw>()) as u64,
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
            ring_pipeline,
            terrain_vbuf,
            terrain_ibuf,
            terrain_indices: tidx.len() as u32,
            water_buf,
            cube_buf,
            cube_len: cube.len() as u32,
            ring_quad_buf,
            instance_buf,
            ring_buf,
            camera_buf,
            camera_bind,
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

    pub fn render(
        &mut self,
        units: &[InstanceRaw],
        rings: &[RingRaw],
        view_proj: [[f32; 4]; 4],
        eye: [f32; 3],
        time: f32,
    ) {
        let nu = units.len().min(MAX_INSTANCES);
        let nr = rings.len().min(MAX_RINGS);
        self.queue.write_buffer(
            &self.camera_buf,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj,
                eye: [eye[0], eye[1], eye[2], 1.0],
                light_dir: [0.5, 1.0, 0.35, 0.0],
                params: [time, 0.012, terrain::SEA_LEVEL, 0.0],
            }),
        );
        if nu > 0 {
            self.queue
                .write_buffer(&self.instance_buf, 0, bytemuck::cast_slice(&units[..nu]));
        }
        if nr > 0 {
            self.queue
                .write_buffer(&self.ring_buf, 0, bytemuck::cast_slice(&rings[..nr]));
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
                            r: 0.45,
                            g: 0.55,
                            b: 0.67,
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

            // terrain
            pass.set_pipeline(&self.terrain_pipeline);
            pass.set_vertex_buffer(0, self.terrain_vbuf.slice(..));
            pass.set_index_buffer(self.terrain_ibuf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.terrain_indices, 0, 0..1);

            // units / buildings
            if nu > 0 {
                pass.set_pipeline(&self.unit_pipeline);
                pass.set_vertex_buffer(0, self.cube_buf.slice(..));
                pass.set_vertex_buffer(1, self.instance_buf.slice(..));
                pass.draw(0..self.cube_len, 0..nu as u32);
            }

            // water (transparent)
            pass.set_pipeline(&self.water_pipeline);
            pass.set_vertex_buffer(0, self.water_buf.slice(..));
            pass.draw(0..6, 0..1);

            // selection rings (transparent)
            if nr > 0 {
                pass.set_pipeline(&self.ring_pipeline);
                pass.set_vertex_buffer(0, self.ring_quad_buf.slice(..));
                pass.set_vertex_buffer(1, self.ring_buf.slice(..));
                pass.draw(0..6, 0..nr as u32);
            }
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
