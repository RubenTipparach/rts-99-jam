//! Fixed-angle RTS camera (no rotation): pan + zoom only, with screen<->world
//! helpers for picking (mouse → ground) and projection (world → screen).

use glam::{Mat4, Vec2, Vec3, Vec4Swizzles};

// Fixed isometric-ish viewing angle (StarCraft/WC3 style).
const YAW: f32 = 0.9;
const PITCH: f32 = 0.95;

pub struct Camera {
    target: Vec2, // look-at point on the ground (world x, z)
    distance: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: Vec2::new(0.0, 220.0), // start near the player's base (south)
            distance: 150.0,
        }
    }
}

impl Camera {
    fn eye_target(&self) -> (Vec3, Vec3) {
        let t = Vec3::new(self.target.x, 0.0, self.target.y);
        let dir = Vec3::new(
            YAW.cos() * PITCH.cos(),
            PITCH.sin(),
            YAW.sin() * PITCH.cos(),
        );
        (t + dir * self.distance, t)
    }

    pub fn eye(&self) -> [f32; 3] {
        self.eye_target().0.to_array()
    }

    fn mat(&self, aspect: f32) -> Mat4 {
        let (eye, target) = self.eye_target();
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        let proj = Mat4::perspective_rh(54f32.to_radians(), aspect.max(0.1), 1.0, 3000.0);
        proj * view
    }

    pub fn view_proj(&self, aspect: f32) -> [[f32; 4]; 4] {
        self.mat(aspect).to_cols_array_2d()
    }

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

    pub fn project(&self, world: Vec3, w: f32, h: f32) -> Option<(f32, f32)> {
        let clip = self.mat(w / h) * world.extend(1.0);
        if clip.w <= 0.0001 {
            return None;
        }
        let ndc = clip.xyz() / clip.w;
        Some(((ndc.x * 0.5 + 0.5) * w, (1.0 - (ndc.y * 0.5 + 0.5)) * h))
    }

    /// Recenter the camera on a world point (used by minimap clicks).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn look_at(&mut self, wx: f32, wz: f32) {
        let lim = 560.0;
        self.target.x = wx.clamp(-lim, lim);
        self.target.y = wz.clamp(-lim, lim);
    }

    pub fn zoom(&mut self, units: f32) {
        self.distance = (self.distance * (1.0 - units * 0.12)).clamp(40.0, 520.0);
    }

    pub fn pan(&mut self, fwd: f32, right: f32, dt: f32) {
        let speed = self.distance * 1.1 * dt;
        let (s, c) = (YAW.sin(), YAW.cos());
        // Screen-forward (W) points "into" the view — toward -dir on the
        // ground; screen-right (D) follows the camera's right axis.
        self.target.x += (-c * fwd + s * right) * speed;
        self.target.y += (-s * fwd - c * right) * speed;
        let lim = 560.0;
        self.target.x = self.target.x.clamp(-lim, lim);
        self.target.y = self.target.y.clamp(-lim, lim);
    }
}
