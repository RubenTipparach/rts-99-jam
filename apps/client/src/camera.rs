//! An orbiting RTS camera: pan across the ground, zoom, and rotate around a
//! target point. Presentation-only `f32` math.

use glam::{Mat4, Vec2, Vec3};

pub struct Camera {
    /// Look-at target on the ground plane (world x, z).
    target: Vec2,
    /// Distance from the target (zoom).
    distance: f32,
    /// Orbit angle around the target.
    yaw: f32,
    /// Elevation angle (clamped so we never go under the ground or fully top-down).
    pitch: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: Vec2::ZERO,
            distance: 95.0,
            yaw: 0.7,
            pitch: 0.95,
        }
    }
}

impl Camera {
    pub fn view_proj(&self, aspect: f32) -> [[f32; 4]; 4] {
        let target = Vec3::new(self.target.x, 0.0, self.target.y);
        let dir = Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        );
        let eye = target + dir * self.distance;
        let view = Mat4::look_at_rh(eye, target, Vec3::Y);
        let proj = Mat4::perspective_rh(60f32.to_radians(), aspect.max(0.1), 0.5, 1000.0);
        (proj * view).to_cols_array_2d()
    }

    /// Mouse-wheel zoom. `units` is roughly notches scrolled.
    pub fn zoom(&mut self, units: f32) {
        self.distance = (self.distance * (1.0 - units * 0.12)).clamp(18.0, 240.0);
    }

    /// Drag-to-orbit (pixels of cursor movement).
    pub fn rotate(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.006;
        self.pitch = (self.pitch + dy * 0.006).clamp(0.18, 1.45);
    }

    /// Keyboard pan in screen-relative directions, scaled by zoom so it feels
    /// consistent. `dt` is the frame time in seconds.
    pub fn pan(&mut self, fwd: f32, right: f32, dt: f32) {
        let speed = self.distance * 0.9 * dt;
        let (s, c) = (self.yaw.sin(), self.yaw.cos());
        // Forward = toward the target along the ground; right = perpendicular.
        self.target.x += (c * fwd + s * right) * speed;
        self.target.y += (s * fwd - c * right) * speed;
        let limit = 80.0;
        self.target.x = self.target.x.clamp(-limit, limit);
        self.target.y = self.target.y.clamp(-limit, limit);
    }
}
