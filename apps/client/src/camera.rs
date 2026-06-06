//! Orbiting RTS camera: pan, zoom, rotate, plus screen<->world helpers for
//! picking (mouse → ground) and projection (unit → screen, for the HUD).

use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};

pub struct Camera {
    target: Vec2, // look-at point on the ground (world x, z)
    distance: f32,
    yaw: f32,
    pitch: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: Vec2::ZERO,
            distance: 70.0,
            yaw: 0.8,
            pitch: 0.95,
        }
    }
}

impl Camera {
    fn eye_target(&self) -> (Vec3, Vec3) {
        let t = Vec3::new(self.target.x, 0.0, self.target.y);
        let dir = Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        );
        (t + dir * self.distance, t)
    }

    pub fn eye(&self) -> [f32; 3] {
        self.eye_target().0.to_array()
    }

    fn mat(&self, aspect: f32) -> Mat4 {
        let (eye, target) = self.eye_target();
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        let proj = Mat4::perspective_rh(58f32.to_radians(), aspect.max(0.1), 0.5, 1000.0);
        proj * view
    }

    pub fn view_proj(&self, aspect: f32) -> [[f32; 4]; 4] {
        self.mat(aspect).to_cols_array_2d()
    }

    /// Intersect the ray through screen pixel (sx, sy) with the ground plane
    /// y = 0; returns world (x, z).
    pub fn ground_pick(&self, sx: f32, sy: f32, w: f32, h: f32) -> Option<(f32, f32)> {
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let inv = self.mat(w / h).inverse();
        let nx = sx / w * 2.0 - 1.0;
        let ny = 1.0 - sy / h * 2.0;
        let near = inv.project_point3(Vec3::new(nx, ny, 0.0));
        let far = inv.project_point3(Vec3::new(nx, ny, 1.0));
        let dir = far - near;
        if dir.y.abs() < 1e-6 {
            return None;
        }
        let t = -near.y / dir.y;
        if t < 0.0 {
            return None;
        }
        let hit = near + dir * t;
        Some((hit.x, hit.z))
    }

    /// Project a world point to screen pixels; `None` if behind the camera.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn project(&self, world: Vec3, w: f32, h: f32) -> Option<(f32, f32)> {
        let clip = self.mat(w / h) * world.extend(1.0);
        if clip.w <= 0.0001 {
            return None;
        }
        let ndc = clip.xyz() / clip.w;
        Some(((ndc.x * 0.5 + 0.5) * w, (1.0 - (ndc.y * 0.5 + 0.5)) * h))
    }

    pub fn zoom(&mut self, units: f32) {
        self.distance = (self.distance * (1.0 - units * 0.12)).clamp(16.0, 200.0);
    }

    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.006;
        self.pitch = (self.pitch + dy * 0.006).clamp(0.25, 1.45);
    }

    pub fn pan(&mut self, fwd: f32, right: f32, dt: f32) {
        let speed = self.distance * 0.9 * dt;
        let (s, c) = (self.yaw.sin(), self.yaw.cos());
        self.target.x += (c * fwd + s * right) * speed;
        self.target.y += (s * fwd - c * right) * speed;
        let lim = 70.0;
        self.target.x = self.target.x.clamp(-lim, lim);
        self.target.y = self.target.y.clamp(-lim, lim);
    }
}
