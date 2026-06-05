//! Fixed-point vectors built on [`Fx`].

use crate::scalar::Fx;
use core::ops::{Add, Sub};

/// A 2D fixed-point vector (e.g. grid-plane positions).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct Vec2 {
    pub x: Fx,
    pub y: Fx,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 {
        x: Fx::ZERO,
        y: Fx::ZERO,
    };

    #[inline]
    pub const fn new(x: Fx, y: Fx) -> Self {
        Vec2 { x, y }
    }

    #[inline]
    pub fn dot(self, o: Vec2) -> Fx {
        self.x * o.x + self.y * o.y
    }

    #[inline]
    pub fn scale(self, s: Fx) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    #[inline]
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    #[inline]
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}

/// A 3D fixed-point vector (world positions and velocities).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct Vec3 {
    pub x: Fx,
    pub y: Fx,
    pub z: Fx,
}

impl Vec3 {
    pub const ZERO: Vec3 = Vec3 {
        x: Fx::ZERO,
        y: Fx::ZERO,
        z: Fx::ZERO,
    };

    #[inline]
    pub const fn new(x: Fx, y: Fx, z: Fx) -> Self {
        Vec3 { x, y, z }
    }

    #[inline]
    pub fn dot(self, o: Vec3) -> Fx {
        self.x * o.x + self.y * o.y + self.z * o.z
    }

    #[inline]
    pub fn scale(self, s: Fx) -> Vec3 {
        Vec3::new(self.x * s, self.y * s, self.z * s)
    }
}

impl Add for Vec3 {
    type Output = Vec3;
    #[inline]
    fn add(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}

impl Sub for Vec3 {
    type Output = Vec3;
    #[inline]
    fn sub(self, o: Vec3) -> Vec3 {
        Vec3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_sub_scale() {
        let a = Vec3::new(Fx::from_int(1), Fx::from_int(2), Fx::from_int(3));
        let b = Vec3::new(Fx::from_int(4), Fx::from_int(5), Fx::from_int(6));
        assert_eq!(
            a + b,
            Vec3::new(Fx::from_int(5), Fx::from_int(7), Fx::from_int(9))
        );
        assert_eq!(
            b - a,
            Vec3::new(Fx::from_int(3), Fx::from_int(3), Fx::from_int(3))
        );
        assert_eq!(
            a.scale(Fx::from_int(2)),
            Vec3::new(Fx::from_int(2), Fx::from_int(4), Fx::from_int(6))
        );
    }

    #[test]
    fn dot_product() {
        let a = Vec3::new(Fx::from_int(1), Fx::from_int(2), Fx::from_int(3));
        let b = Vec3::new(Fx::from_int(4), Fx::from_int(5), Fx::from_int(6));
        // 1*4 + 2*5 + 3*6 = 32
        assert_eq!(a.dot(b), Fx::from_int(32));
    }
}
