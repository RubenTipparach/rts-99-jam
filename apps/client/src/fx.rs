//! Presentation-only particle and light effects.
//!
//! Every sim interaction gets visible feedback (see the fx policy in
//! `CLAUDE.md`): shots spawn muzzle flashes, tracers, and impact sparks;
//! falling HP spawns blood or debris; deaths explode; mining sputters
//! crystal sparks. Bursty effects also drop a short-lived point light, fed to
//! the renderer's vertex-lit point-light slots (`gfx::MAX_LIGHTS`).
//!
//! Pure floats, pure cosmetics: nothing here feeds back into the sim. The
//! internal RNG is just a frame-local jitter source.

use crate::gfx::{FxLight, InstanceRaw, MAX_LIGHTS, ROT_NONE};

const MAX_PARTICLES: usize = 4096;
const GRAVITY: f32 = -22.0;

struct Particle {
    pos: [f32; 3],
    vel: [f32; 3],
    /// Seconds remaining / total (drives fade + shrink).
    life: f32,
    max_life: f32,
    size: f32,
    color: [f32; 3],
    /// 0 = drifts, 1 = full ballistic fall.
    weight: f32,
}

struct Light {
    pos: [f32; 3],
    radius: f32,
    color: [f32; 3],
    life: f32,
    max_life: f32,
}

#[derive(Default)]
pub struct Fx {
    particles: Vec<Particle>,
    lights: Vec<Light>,
    seed: u32,
}

impl Fx {
    /// Cheap presentation-side jitter in [-1, 1] (xorshift; never the sim RNG).
    fn jitter(&mut self) -> f32 {
        let mut x = self.seed.wrapping_add(0x9e37_79b9);
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x & 0xffff) as f32 / 32768.0 - 1.0
    }

    #[allow(clippy::too_many_arguments)] // a tuning-knob row, like the gfx primitives
    fn burst(
        &mut self,
        pos: [f32; 3],
        color: [f32; 3],
        n: usize,
        speed: f32,
        life: f32,
        size: f32,
        weight: f32,
    ) {
        for _ in 0..n {
            if self.particles.len() >= MAX_PARTICLES {
                return;
            }
            let (jx, jy, jz) = (self.jitter(), self.jitter(), self.jitter());
            let js = self.jitter().abs();
            self.particles.push(Particle {
                pos,
                vel: [jx * speed, (jy * 0.5 + 0.9) * speed, jz * speed],
                life,
                max_life: life,
                size: size * (0.7 + 0.3 * js),
                color,
                weight,
            });
        }
    }

    fn light(&mut self, pos: [f32; 3], radius: f32, color: [f32; 3], life: f32) {
        self.lights.push(Light {
            pos,
            radius,
            color,
            life,
            max_life: life,
        });
    }

    /// A shot from `from` to `to`: muzzle flash (light), a bright tracer
    /// strung along the line, and impact sparks at the target.
    pub fn shot(&mut self, from: [f32; 3], to: [f32; 3]) {
        let muzzle = [from[0], from[1] + 1.6, from[2]];
        let hit = [to[0], to[1] + 1.2, to[2]];
        self.light(muzzle, 9.0, [1.0, 0.75, 0.30], 0.12);
        // Tracer: a few emissive beads racing down the firing line.
        let d = [hit[0] - muzzle[0], hit[1] - muzzle[1], hit[2] - muzzle[2]];
        for k in 0..3 {
            if self.particles.len() >= MAX_PARTICLES {
                break;
            }
            let t = k as f32 / 3.0;
            self.particles.push(Particle {
                pos: [
                    muzzle[0] + d[0] * t,
                    muzzle[1] + d[1] * t,
                    muzzle[2] + d[2] * t,
                ],
                vel: [d[0] * 6.0, d[1] * 6.0, d[2] * 6.0],
                life: 0.14,
                max_life: 0.14,
                size: 0.32,
                color: [1.0, 0.85, 0.45],
                weight: 0.0,
            });
        }
        self.burst(hit, [1.0, 0.8, 0.35], 4, 6.0, 0.28, 0.28, 0.6);
    }

    /// A unit took damage: organic targets bleed, mechanical ones shed sparks.
    pub fn hit(&mut self, pos: [f32; 3], organic: bool) {
        let p = [pos[0], pos[1] + 1.2, pos[2]];
        if organic {
            self.burst(p, [0.75, 0.10, 0.08], 6, 5.0, 0.5, 0.30, 1.0);
        } else {
            self.burst(p, [1.0, 0.85, 0.40], 5, 7.0, 0.35, 0.24, 0.8);
            self.burst(p, [0.45, 0.48, 0.52], 3, 4.0, 0.6, 0.30, 1.0);
        }
    }

    /// Something died: a fireball, smoke, and an orange light flash.
    /// `big` for buildings.
    pub fn explosion(&mut self, pos: [f32; 3], big: bool) {
        let p = [pos[0], pos[1] + 1.0, pos[2]];
        let s = if big { 2.0 } else { 1.0 };
        self.light(p, 16.0 * s, [1.0, 0.55, 0.18], 0.45);
        self.burst(
            p,
            [1.0, 0.62, 0.18],
            (10.0 * s) as usize,
            9.0 * s,
            0.5,
            0.55 * s,
            0.5,
        );
        self.burst(
            p,
            [1.0, 0.9, 0.5],
            (6.0 * s) as usize,
            12.0 * s,
            0.3,
            0.4 * s,
            0.4,
        );
        self.burst(
            p,
            [0.25, 0.24, 0.26],
            (8.0 * s) as usize,
            4.0 * s,
            1.1,
            0.7 * s,
            0.1,
        );
    }

    /// A damaged building burning, called every sim tick while it's hurt.
    /// `severity` is 0..1 (1 = nearly destroyed): light damage smolders,
    /// heavy damage adds licking flames and a flickering glow. `spread`
    /// scatters the flames across the building's footprint.
    pub fn fire(&mut self, pos: [f32; 3], severity: f32, spread: f32) {
        let (jx, jz) = (self.jitter(), self.jitter());
        let p = [pos[0] + jx * spread, pos[1] + 1.0, pos[2] + jz * spread];
        // Smoke column: slow, rising, long-lived.
        self.burst(
            p,
            [0.22, 0.21, 0.23],
            1,
            1.4,
            1.6,
            0.45 + 0.4 * severity,
            0.0,
        );
        if severity > 0.4 {
            // Licking flames.
            self.burst(p, [1.0, 0.55, 0.12], 2, 2.4, 0.35, 0.34, 0.0);
            self.burst(p, [1.0, 0.85, 0.35], 1, 2.0, 0.25, 0.22, 0.0);
        }
        if severity > 0.55 {
            // Flickering firelight (re-fed every tick, so it dances).
            let flick = 0.8 + 0.2 * self.jitter().abs();
            self.light(p, 11.0 * flick, [1.0, 0.5, 0.14], 0.14);
        }
    }

    /// A worker chipping at a node: crystal sparks + a soft teal glint.
    pub fn mining(&mut self, pos: [f32; 3], carbon: bool) {
        let p = [pos[0], pos[1] + 1.0, pos[2]];
        let color = if carbon {
            [0.45, 0.95, 0.55]
        } else {
            [0.55, 0.90, 1.0]
        };
        self.burst(p, color, 2, 3.5, 0.4, 0.18, 0.9);
        self.light(
            p,
            6.0,
            [color[0] * 0.6, color[1] * 0.6, color[2] * 0.6],
            0.15,
        );
    }

    /// Advance and expire particles/lights.
    pub fn update(&mut self, dt: f32) {
        for q in &mut self.particles {
            q.life -= dt;
            q.vel[1] += GRAVITY * q.weight * dt;
            q.pos[0] += q.vel[0] * dt;
            q.pos[1] += q.vel[1] * dt;
            q.pos[2] += q.vel[2] * dt;
        }
        self.particles.retain(|q| q.life > 0.0);
        for l in &mut self.lights {
            l.life -= dt;
        }
        self.lights.retain(|l| l.life > 0.0);
    }

    /// Emissive instances for the particle draw group.
    pub fn instances(&self) -> Vec<InstanceRaw> {
        self.particles
            .iter()
            .map(|q| {
                let t = (q.life / q.max_life).clamp(0.0, 1.0);
                let s = q.size * (0.4 + 0.6 * t);
                InstanceRaw {
                    offset: q.pos,
                    scale: [s, s, s],
                    // Fade toward black as the particle dies (emissive mode).
                    color: [q.color[0] * t, q.color[1] * t, q.color[2] * t, 3.0],
                    rot: ROT_NONE,
                }
            })
            .collect()
    }

    /// The brightest current point lights, newest first, capped to the
    /// renderer's slots. Radius eases out and color fades with life.
    pub fn lights(&self) -> Vec<FxLight> {
        let mut out: Vec<FxLight> = self
            .lights
            .iter()
            .rev()
            .take(MAX_LIGHTS)
            .map(|l| {
                let t = (l.life / l.max_life).clamp(0.0, 1.0);
                FxLight {
                    pos: l.pos,
                    radius: l.radius * (0.6 + 0.4 * t),
                    color: [
                        l.color[0] * t * 2.2,
                        l.color[1] * t * 2.2,
                        l.color[2] * t * 2.2,
                    ],
                }
            })
            .collect();
        out.truncate(MAX_LIGHTS);
        out
    }
}
