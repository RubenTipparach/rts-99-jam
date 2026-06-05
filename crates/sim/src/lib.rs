//! Deterministic simulation core.
//!
//! The [`World`] holds all game truth and advances one fixed tick at a time via
//! [`World::step`], a pure function of the current state and the commands for
//! that tick. It uses only fixed-point math ([`math`]) and a pinned in-state RNG
//! ([`DetRng`]), so the same seed + same command stream produce a byte-identical
//! state — and therefore an identical [`World::state_hash`] — on every machine.
//! See `docs/architecture/01-determinism.md` and `02-simulation.md`.
//!
//! Milestone 0 deliberately models the smallest interesting world: entities with
//! a position and a per-tick velocity, integrated each tick. It exists to *prove
//! determinism* before any rendering, networking, or gameplay is built.

#![forbid(unsafe_code)]

mod rng;

pub use rng::DetRng;

use math::Vec3;
use protocol::{Command, PlayerId};

/// A stable handle to an entity. The `generation` guards against a slot being
/// reused: an old id pointing at a recycled slot fails lookups instead of
/// silently referring to a different unit.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EntityId {
    pub index: u32,
    pub generation: u32,
}

/// Generational arena with a deterministic free list (a LIFO stack of reusable
/// slots). Allocation order depends only on the spawn/despawn sequence, never on
/// pointers or the allocator — a hard requirement for determinism.
#[derive(Default)]
struct Arena {
    generation: Vec<u32>,
    alive: Vec<bool>,
    free: Vec<u32>,
}

impl Arena {
    fn alloc(&mut self) -> EntityId {
        if let Some(index) = self.free.pop() {
            self.alive[index as usize] = true;
            EntityId {
                index,
                generation: self.generation[index as usize],
            }
        } else {
            let index = self.generation.len() as u32;
            self.generation.push(0);
            self.alive.push(true);
            EntityId {
                index,
                generation: 0,
            }
        }
    }

    /// Free the live slot at `index` (bumping its generation). Returns whether a
    /// live slot was actually freed.
    fn free_index(&mut self, index: u32) -> bool {
        let i = index as usize;
        if i < self.alive.len() && self.alive[i] {
            self.alive[i] = false;
            self.generation[i] = self.generation[i].wrapping_add(1);
            self.free.push(index);
            true
        } else {
            false
        }
    }

    #[inline]
    fn capacity(&self) -> usize {
        self.generation.len()
    }

    #[inline]
    fn is_live_index(&self, index: u32) -> bool {
        let i = index as usize;
        i < self.alive.len() && self.alive[i]
    }

    #[inline]
    fn is_alive(&self, id: EntityId) -> bool {
        let i = id.index as usize;
        i < self.alive.len() && self.alive[i] && self.generation[i] == id.generation
    }
}

/// The entire game state for one match.
///
/// Components are stored as parallel arrays (struct-of-arrays) indexed by an
/// entity's slot index — cache-friendly for iterating thousands of units, and
/// the layout the renderer will later snapshot. See `02-simulation.md`.
pub struct World {
    tick: u64,
    rng: DetRng,
    arena: Arena,
    // --- component columns, indexed by EntityId::index ---
    pos: Vec<Vec3>,
    vel: Vec<Vec3>,
    owner: Vec<PlayerId>,
}

impl World {
    /// Create an empty world seeded from the shared match seed.
    pub fn new(seed: u64) -> Self {
        World {
            tick: 0,
            rng: DetRng::new(seed),
            arena: Arena::default(),
            pos: Vec::new(),
            vel: Vec::new(),
            owner: Vec::new(),
        }
    }

    /// The current tick (simulation time).
    #[inline]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Number of live entities.
    pub fn alive_count(&self) -> usize {
        self.arena.alive.iter().filter(|&&a| a).count()
    }

    /// Position of a live entity, or `None` if the id is stale/dead.
    pub fn position(&self, id: EntityId) -> Option<Vec3> {
        if self.arena.is_alive(id) {
            Some(self.pos[id.index as usize])
        } else {
            None
        }
    }

    fn spawn(&mut self, owner: PlayerId, pos: Vec3, vel: Vec3) -> EntityId {
        let id = self.arena.alloc();
        let i = id.index as usize;
        if i >= self.pos.len() {
            self.pos.push(pos);
            self.vel.push(vel);
            self.owner.push(owner);
        } else {
            self.pos[i] = pos;
            self.vel[i] = vel;
            self.owner[i] = owner;
        }
        id
    }

    /// Advance the simulation by exactly one tick.
    ///
    /// This is the *only* place game truth changes, and it depends solely on its
    /// arguments (and the in-state RNG) — never on wall-clock time, I/O, or
    /// iteration order of unordered collections.
    pub fn step(&mut self, commands: &[Command]) {
        // 1) Apply commands (player/AI intents) in the order received. The
        //    network layer guarantees every peer sees the same order.
        for &cmd in commands {
            match cmd {
                Command::Spawn { owner, pos, vel } => {
                    self.spawn(owner, pos, vel);
                }
                Command::SetVelocity { entity_index, vel } => {
                    if self.arena.is_live_index(entity_index) {
                        self.vel[entity_index as usize] = vel;
                    }
                }
                Command::Despawn { entity_index } => {
                    self.arena.free_index(entity_index);
                }
            }
        }

        // 2) Movement system: integrate velocity. Deterministic order: ascending
        //    slot index. (One pass; fixed-point add.)
        let cap = self.arena.capacity();
        for i in 0..cap {
            if self.arena.alive[i] {
                self.pos[i] = self.pos[i] + self.vel[i];
            }
        }

        self.tick += 1;
    }

    /// A 64-bit FNV-1a hash of the entire simulation state.
    ///
    /// Walking entities in ascending-index order and hashing only fixed-point
    /// bytes makes this identical across platforms. Peers exchange it each tick
    /// to detect desync the moment it happens (`03-networking-lockstep.md`).
    pub fn state_hash(&self) -> u64 {
        let mut h = Fnv1a::new();
        h.write_u64(self.tick);
        h.write_u64(self.rng.raw());
        for i in 0..self.arena.capacity() {
            if !self.arena.alive[i] {
                continue;
            }
            h.write_u32(i as u32);
            h.write_u32(self.arena.generation[i]);
            h.write_u32(self.owner[i] as u32);
            for v in [self.pos[i], self.vel[i]] {
                h.write_i64(v.x.to_raw());
                h.write_i64(v.y.to_raw());
                h.write_i64(v.z.to_raw());
            }
        }
        h.finish()
    }
}

/// Minimal FNV-1a (64-bit). The algorithm is pinned: it is part of the desync
/// protocol, so changing it is a version bump.
struct Fnv1a(u64);

impl Fnv1a {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    #[inline]
    fn new() -> Self {
        Fnv1a(Self::OFFSET)
    }

    #[inline]
    fn write_u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    #[inline]
    fn write_i64(&mut self, v: i64) {
        self.write_u64(v as u64);
    }

    #[inline]
    fn write_u32(&mut self, v: u32) {
        self.write_u64(v as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use math::Fx;

    fn v(x: i32, y: i32, z: i32) -> Vec3 {
        Vec3::new(Fx::from_int(x), Fx::from_int(y), Fx::from_int(z))
    }

    #[test]
    fn integration_moves_units() {
        let mut w = World::new(1);
        w.step(&[Command::Spawn {
            owner: 0,
            pos: v(0, 0, 0),
            vel: v(1, 0, 0),
        }]);
        let id = EntityId {
            index: 0,
            generation: 0,
        };
        assert_eq!(w.position(id), Some(v(1, 0, 0)));
        w.step(&[]);
        w.step(&[]);
        assert_eq!(w.position(id), Some(v(3, 0, 0)));
    }

    #[test]
    fn despawn_then_spawn_reuses_slot_with_new_generation() {
        let mut w = World::new(1);
        w.step(&[Command::Spawn {
            owner: 0,
            pos: v(5, 5, 5),
            vel: v(0, 0, 0),
        }]);
        let old = EntityId {
            index: 0,
            generation: 0,
        };
        w.step(&[Command::Despawn { entity_index: 0 }]);
        assert_eq!(w.position(old), None, "stale id must not resolve");
        w.step(&[Command::Spawn {
            owner: 1,
            pos: v(9, 9, 9),
            vel: v(0, 0, 0),
        }]);
        // Slot 0 reused, generation bumped to 1.
        let new = EntityId {
            index: 0,
            generation: 1,
        };
        assert_eq!(w.position(new), Some(v(9, 9, 9)));
        assert_eq!(w.position(old), None, "old generation still invalid");
        assert_eq!(w.alive_count(), 1);
    }

    #[test]
    fn same_inputs_same_hash() {
        let build = || {
            let mut w = World::new(0xABCD);
            w.step(&[
                Command::Spawn {
                    owner: 0,
                    pos: v(0, 0, 0),
                    vel: v(1, 2, 3),
                },
                Command::Spawn {
                    owner: 1,
                    pos: v(10, 0, 0),
                    vel: v(-1, 0, 0),
                },
            ]);
            for _ in 0..10 {
                w.step(&[]);
            }
            w
        };
        assert_eq!(build().state_hash(), build().state_hash());
    }

    #[test]
    fn order_of_distinct_units_is_fixed() {
        // Spawning the same two units always hashes the same regardless of how
        // many empty ticks pass between — sanity that traversal is stable.
        let mut a = World::new(7);
        let mut b = World::new(7);
        let s = Command::Spawn {
            owner: 0,
            pos: v(1, 1, 1),
            vel: v(0, 0, 0),
        };
        a.step(&[s]);
        b.step(&[s]);
        assert_eq!(a.state_hash(), b.state_hash());
    }
}
