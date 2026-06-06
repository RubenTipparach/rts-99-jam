//! Drives the deterministic sim and turns it into something to draw.
//!
//! This is the dual-clock loop in miniature (see ARCHITECTURE.md §4): the sim
//! steps at a fixed rate and only via [`protocol::Command`]s, while rendering
//! interpolates between the two latest sim states every frame. The units bounce
//! inside a box — and the bounces are issued as `SetVelocity` commands computed
//! from sim state, so even this toy stays on the command-driven, deterministic
//! path. Converting fixed-point to `f32` happens here, at the wall, never inside
//! the sim.

use crate::gfx::InstanceRaw;
use math::{Fx, Vec3, FRAC_BITS};
use protocol::Command;
use sim::{DetRng, EntityId, World};
use web_time::Instant;

const TICK_HZ: u32 = 25;
const N_UNITS: u32 = 600;
const BOUND: i32 = 46; // world half-extent the units bounce within
const SEED: u64 = 0x00C0_FFEE_D00D_5EED;

const PALETTE: [[f32; 4]; 3] = [
    [0.36, 0.70, 0.95, 1.0], // faction A — cool blue
    [1.0, 0.45, 0.20, 1.0],  // faction B — ember
    [0.65, 0.85, 0.45, 1.0], // faction C — green
];

#[inline]
fn fx_to_f32(x: Fx) -> f32 {
    x.to_raw() as f32 / (1u64 << FRAC_BITS) as f32
}

pub struct Game {
    world: World,
    ids: Vec<EntityId>,
    owners: Vec<usize>,
    vel: Vec<Vec3>, // client-side mirror of each unit's velocity (for bounces)
    prev: Vec<Vec3>,
    curr: Vec<Vec3>,
    last: Instant,
    acc: f32,
    tick_dt: f32,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        let mut world = World::new(SEED);
        let mut rng = DetRng::new(SEED);

        let mut spawn = Vec::with_capacity(N_UNITS as usize);
        let mut owners = Vec::with_capacity(N_UNITS as usize);
        let mut vel = Vec::with_capacity(N_UNITS as usize);
        for i in 0..N_UNITS {
            let px = rng.range_u32((BOUND as u32) * 2) as i32 - BOUND;
            let py = rng.range_u32((BOUND as u32) * 2) as i32 - BOUND;
            // velocity components in [-0.7, 0.7] per tick, avoiding ~0
            let vx = rng.range_u32(13) as i64 - 6;
            let vy = rng.range_u32(13) as i64 - 6;
            let v = Vec3::new(
                Fx::from_ratio(if vx == 0 { 3 } else { vx }, 10),
                Fx::from_ratio(if vy == 0 { -4 } else { vy }, 10),
                Fx::ZERO,
            );
            spawn.push(Command::Spawn {
                owner: (i % 3) as u16,
                pos: Vec3::new(Fx::from_int(px), Fx::from_int(py), Fx::ZERO),
                vel: v,
            });
            owners.push((i % 3) as usize);
            vel.push(v);
        }
        world.step(&spawn);

        let ids: Vec<EntityId> = (0..N_UNITS)
            .map(|index| EntityId {
                index,
                generation: 0,
            })
            .collect();
        let curr: Vec<Vec3> = ids.iter().map(|&id| world.position(id).unwrap()).collect();
        let prev = curr.clone();

        Game {
            world,
            ids,
            owners,
            vel,
            prev,
            curr,
            last: Instant::now(),
            acc: 0.0,
            tick_dt: 1.0 / TICK_HZ as f32,
        }
    }

    /// Advance real time; step the sim a whole number of fixed ticks.
    pub fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        self.acc += dt;
        while self.acc >= self.tick_dt {
            self.step_once();
            self.acc -= self.tick_dt;
        }
    }

    fn step_once(&mut self) {
        let bound = Fx::from_int(BOUND);
        let neg_bound = Fx::from_int(-BOUND);

        // Compute bounce commands from current sim state, then step with them.
        let mut cmds = Vec::new();
        for (k, &id) in self.ids.iter().enumerate() {
            let Some(p) = self.world.position(id) else {
                continue;
            };
            let v = self.vel[k];
            let mut nv = v;
            let mut bounced = false;
            if (p.x > bound && v.x > Fx::ZERO) || (p.x < neg_bound && v.x < Fx::ZERO) {
                nv.x = -v.x;
                bounced = true;
            }
            if (p.y > bound && v.y > Fx::ZERO) || (p.y < neg_bound && v.y < Fx::ZERO) {
                nv.y = -v.y;
                bounced = true;
            }
            if bounced {
                self.vel[k] = nv;
                cmds.push(Command::SetVelocity {
                    entity_index: id.index,
                    vel: nv,
                });
            }
        }

        std::mem::swap(&mut self.prev, &mut self.curr);
        self.world.step(&cmds);
        for (k, &id) in self.ids.iter().enumerate() {
            if let Some(p) = self.world.position(id) {
                self.curr[k] = p;
            }
        }
    }

    /// Build the interpolated instance list for this frame. Sim (x, y) maps to
    /// the ground plane (world x, z); cubes stand up in +y.
    pub fn instances(&self) -> Vec<InstanceRaw> {
        let alpha = (self.acc / self.tick_dt).clamp(0.0, 1.0);
        let mut out = Vec::with_capacity(self.ids.len());
        for k in 0..self.ids.len() {
            let a = self.prev[k];
            let b = self.curr[k];
            let x = fx_to_f32(a.x) + (fx_to_f32(b.x) - fx_to_f32(a.x)) * alpha;
            let z = fx_to_f32(a.y) + (fx_to_f32(b.y) - fx_to_f32(a.y)) * alpha;
            out.push(InstanceRaw {
                offset: [x, 0.0, z],
                color: PALETTE[self.owners[k]],
            });
        }
        out
    }
}
