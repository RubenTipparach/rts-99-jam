//! Deterministic RTS simulation core.
//!
//! Holds all game truth and advances one fixed tick at a time via
//! [`World::step`], a pure function of the current state and the commands for
//! that tick. Fixed-point only ([`math`]) + a pinned in-state RNG, so the same
//! seed + command stream produce a byte-identical state on every machine.
//! See `docs/architecture/01-determinism.md` and `02-simulation.md`.
//!
//! This models a small but real RTS slice: infantry and barracks for two
//! factions, production, movement, and combat - enough to fight over a base.

#![forbid(unsafe_code)]

mod rng;

pub use rng::DetRng;

use math::{Fx, Vec3};
use protocol::{BuildingKind, Command, PlayerId, UnitKind};

/// A stable handle to an entity (generation guards against slot reuse).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct EntityId {
    pub index: u32,
    pub generation: u32,
}

/// What an entity is.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Infantry,
    Barracks,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Order {
    Idle,
    Move { x: Fx, y: Fx },
    AttackMove { x: Fx, y: Fx },
    Attack { target: u32 },
}

struct Stats {
    max_hp: Fx,
    speed: Fx,
    range2: Fx,
    damage: Fx,
    attack_cd: Fx,
    aggro2: Fx,
}

fn stats(kind: Kind) -> Stats {
    match kind {
        Kind::Infantry => Stats {
            max_hp: Fx::from_int(50),
            speed: Fx::from_ratio(30, 100),
            range2: Fx::from_int(25), // range 5
            damage: Fx::from_int(5),
            attack_cd: Fx::from_int(12),
            aggro2: Fx::from_int(256), // aggro 16
        },
        Kind::Barracks => Stats {
            max_hp: Fx::from_int(500),
            speed: Fx::ZERO,
            range2: Fx::ZERO,
            damage: Fx::ZERO,
            attack_cd: Fx::ZERO,
            aggro2: Fx::ZERO,
        },
    }
}

const PROD_TICKS: i32 = 55;
const TEAM_UNIT_CAP: usize = 30;
const MAX_QUEUE: u32 = 6;

// Economy: a single resource ("ore"). Players start with a stockpile, gain a
// trickle of income per owned building, and pay per trained unit. Nobody
// auto-produces - every unit is queued by command.
const STARTING_ORE: i32 = 200;
pub const TRAIN_COST: i32 = 50;
const INCOME_PER_BUILDING: Fx = Fx::from_ratio(1, 2); // per building, per tick

// Collision avoidance: infantry never share a spot. Each tick a unit is pushed
// away from any other infantry whose center is closer than `SEP_DIST`, so a
// crowd drifts apart and a group-move settles into distinct cells rather than
// stacking on one point. `SEP_FACTOR` (each pair resolves half the overlap) and
// the `SEP_MAX` step clamp keep it a smooth drift instead of a teleport.
const SEP_DIST: Fx = Fx::from_ratio(5, 2); // desired min spacing between centers
const SEP_FACTOR: Fx = Fx::from_ratio(1, 2);
const SEP_MAX: Fx = Fx::from_ratio(30, 100); // max push per tick (= infantry speed)

/// One entity as seen by the renderer/HUD (read-only; client converts to floats).
#[derive(Clone, Copy)]
pub struct Snap {
    pub index: u32,
    pub generation: u32,
    pub kind: Kind,
    pub owner: PlayerId,
    pub pos: Vec3,
    pub hp: Fx,
    pub max_hp: Fx,
    pub moving: bool,
    /// Buildings only: units queued for production and the current unit's
    /// build progress (0..1). Display-only; not part of the state hash.
    pub queued: u32,
    pub build_frac: Fx,
}

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

    fn free_index(&mut self, index: u32) {
        let i = index as usize;
        if i < self.alive.len() && self.alive[i] {
            self.alive[i] = false;
            self.generation[i] = self.generation[i].wrapping_add(1);
            self.free.push(index);
        }
    }

    fn capacity(&self) -> usize {
        self.generation.len()
    }

    fn alive_at(&self, index: u32) -> bool {
        let i = index as usize;
        i < self.alive.len() && self.alive[i]
    }
}

/// The entire game state for one match.
pub struct World {
    tick: u64,
    rng: DetRng,
    arena: Arena,
    kind: Vec<Kind>,
    owner: Vec<PlayerId>,
    pos: Vec<Vec3>,
    hp: Vec<Fx>,
    order: Vec<Order>,
    cooldown: Vec<Fx>,
    prod: Vec<Fx>,
    queue: Vec<u32>,
    rally: Vec<Vec3>,
    /// Per-player ore stockpile, indexed by `PlayerId`.
    ore: Vec<Fx>,
}

#[inline]
fn dist2(a: Vec3, bx: Fx, by: Fx) -> Fx {
    let dx = a.x - bx;
    let dy = a.y - by;
    dx * dx + dy * dy
}

impl World {
    pub fn new(seed: u64) -> Self {
        World {
            tick: 0,
            rng: DetRng::new(seed),
            arena: Arena::default(),
            kind: Vec::new(),
            owner: Vec::new(),
            pos: Vec::new(),
            hp: Vec::new(),
            order: Vec::new(),
            cooldown: Vec::new(),
            prod: Vec::new(),
            queue: Vec::new(),
            rally: Vec::new(),
            ore: Vec::new(),
        }
    }

    /// A player's current ore (defaults to the starting stockpile).
    pub fn ore(&self, player: PlayerId) -> Fx {
        self.ore
            .get(player as usize)
            .copied()
            .unwrap_or(Fx::from_int(STARTING_ORE))
    }

    fn ore_mut(&mut self, player: PlayerId) -> &mut Fx {
        let i = player as usize;
        while self.ore.len() <= i {
            self.ore.push(Fx::from_int(STARTING_ORE));
        }
        &mut self.ore[i]
    }

    #[inline]
    pub fn tick(&self) -> u64 {
        self.tick
    }

    pub fn alive_count(&self) -> usize {
        self.arena.alive.iter().filter(|&&a| a).count()
    }

    fn ensure(&mut self, index: usize) {
        while self.kind.len() <= index {
            self.kind.push(Kind::Infantry);
            self.owner.push(0);
            self.pos.push(Vec3::ZERO);
            self.hp.push(Fx::ZERO);
            self.order.push(Order::Idle);
            self.cooldown.push(Fx::ZERO);
            self.prod.push(Fx::ZERO);
            self.queue.push(0);
            self.rally.push(Vec3::ZERO);
        }
    }

    fn spawn(&mut self, kind: Kind, owner: PlayerId, x: Fx, y: Fx) -> EntityId {
        let id = self.arena.alloc();
        let i = id.index as usize;
        self.ensure(i);
        self.kind[i] = kind;
        self.owner[i] = owner;
        self.pos[i] = Vec3::new(x, y, Fx::ZERO);
        self.hp[i] = stats(kind).max_hp;
        self.order[i] = Order::Idle;
        self.cooldown[i] = Fx::ZERO;
        // Nobody auto-produces: a building stays idle until a unit is queued.
        self.prod[i] = Fx::ZERO;
        self.queue[i] = 0;
        let _ = self.ore_mut(owner); // materialize the owner's stockpile
                                     // Default rally a little "south" of a building.
        self.rally[i] = Vec3::new(x, y - Fx::from_int(7), Fx::ZERO);
        id
    }

    fn team_unit_count(&self, owner: PlayerId) -> usize {
        let mut n = 0;
        for i in 0..self.arena.capacity() {
            if self.arena.alive[i] && self.kind[i] == Kind::Infantry && self.owner[i] == owner {
                n += 1;
            }
        }
        n
    }

    /// Nearest living entity of a different owner: returns `(index, dist2)`.
    fn nearest_enemy(&self, i: usize) -> Option<(u32, Fx)> {
        let me = self.pos[i];
        let my_owner = self.owner[i];
        let mut best: Option<(u32, Fx)> = None;
        for j in 0..self.arena.capacity() {
            if !self.arena.alive[j] || self.owner[j] == my_owner {
                continue;
            }
            let d2 = dist2(me, self.pos[j].x, self.pos[j].y);
            match best {
                Some((_, bd)) if bd <= d2 => {}
                _ => best = Some((j as u32, d2)),
            }
        }
        best
    }

    /// Advance the simulation by one tick. The only place game truth changes.
    pub fn step(&mut self, commands: &[Command]) {
        self.apply_commands(commands);
        self.acquire_targets();
        self.economy();
        self.production();
        self.units_update();
        self.tick += 1;
    }

    /// Trickle ore income to each player for every building they own.
    fn economy(&mut self) {
        for i in 0..self.arena.capacity() {
            if self.arena.alive[i] && self.kind[i] == Kind::Barracks {
                let owner = self.owner[i];
                *self.ore_mut(owner) += INCOME_PER_BUILDING;
            }
        }
    }

    fn apply_commands(&mut self, commands: &[Command]) {
        for &cmd in commands {
            match cmd {
                Command::SpawnUnit { owner, kind, x, y } => {
                    let k = match kind {
                        UnitKind::Infantry => Kind::Infantry,
                    };
                    self.spawn(k, owner, x, y);
                }
                Command::SpawnBuilding { owner, kind, x, y } => {
                    let k = match kind {
                        BuildingKind::Barracks => Kind::Barracks,
                    };
                    self.spawn(k, owner, x, y);
                }
                Command::Move { unit, x, y } => self.set_order(unit, Order::Move { x, y }),
                Command::AttackMove { unit, x, y } => {
                    self.set_order(unit, Order::AttackMove { x, y })
                }
                Command::Attack { unit, target } => {
                    if self.arena.alive_at(target) {
                        self.set_order(unit, Order::Attack { target });
                    }
                }
                Command::Train { building } => {
                    let b = building as usize;
                    let cost = Fx::from_int(TRAIN_COST);
                    if self.arena.alive_at(building)
                        && self.kind[b] == Kind::Barracks
                        && self.queue[b] < MAX_QUEUE
                        && self.ore(self.owner[b]) >= cost
                    {
                        let owner = self.owner[b];
                        *self.ore_mut(owner) -= cost;
                        self.queue[b] += 1;
                        if self.prod[b] <= Fx::ZERO {
                            self.prod[b] = Fx::from_int(PROD_TICKS);
                        }
                    }
                }
                Command::Stop { unit } => self.set_order(unit, Order::Idle),
                Command::SetRally { building, x, y } => {
                    if self.arena.alive_at(building) {
                        self.rally[building as usize] = Vec3::new(x, y, Fx::ZERO);
                    }
                }
            }
        }
    }

    fn set_order(&mut self, unit: u32, order: Order) {
        if self.arena.alive_at(unit) && self.kind[unit as usize] == Kind::Infantry {
            self.order[unit as usize] = order;
        }
    }

    /// Idle units auto-engage the nearest enemy within aggro range.
    fn acquire_targets(&mut self) {
        for i in 0..self.arena.capacity() {
            if !self.arena.alive[i] || self.kind[i] != Kind::Infantry {
                continue;
            }
            if self.order[i] != Order::Idle {
                continue;
            }
            let aggro2 = stats(Kind::Infantry).aggro2;
            if let Some((t, d2)) = self.nearest_enemy(i) {
                if d2 <= aggro2 {
                    self.order[i] = Order::Attack { target: t };
                }
            }
        }
    }

    fn production(&mut self) {
        for i in 0..self.arena.capacity() {
            if !self.arena.alive[i] || self.kind[i] != Kind::Barracks {
                continue;
            }
            // Every building is manual: it builds only what has been queued.
            if self.queue[i] == 0 {
                continue;
            }
            self.prod[i] = (self.prod[i] - Fx::ONE).max(Fx::ZERO);
            if self.prod[i] > Fx::ZERO {
                continue;
            }
            // Built - but hold (without consuming the queue) if at the unit cap.
            if self.team_unit_count(self.owner[i]) >= TEAM_UNIT_CAP {
                continue;
            }
            self.queue[i] -= 1;
            self.prod[i] = if self.queue[i] > 0 {
                Fx::from_int(PROD_TICKS)
            } else {
                Fx::ZERO
            };
            self.produce_at(i);
        }
    }

    /// Spawn one infantry just in front of building `i`, headed to its rally.
    fn produce_at(&mut self, i: usize) {
        let owner = self.owner[i];
        let p = self.pos[i];
        let rally = self.rally[i];
        // tiny deterministic spread so they don't stack perfectly
        let jitter = Fx::from_ratio((self.rng.range_u32(7) as i64) - 3, 2);
        let id = self.spawn(Kind::Infantry, owner, p.x + jitter, p.y - Fx::from_int(3));
        self.order[id.index as usize] = Order::Move {
            x: rally.x,
            y: rally.y,
        };
    }

    fn units_update(&mut self) {
        let cap = self.arena.capacity();
        let inf = stats(Kind::Infantry);
        let mut damage = vec![Fx::ZERO; cap];

        for i in 0..cap {
            if !self.arena.alive[i] || self.kind[i] != Kind::Infantry {
                continue;
            }
            let me = self.pos[i];

            // Resolve a move-target and/or an attack-target from the order.
            let mut move_to: Option<(Fx, Fx)> = None;
            let mut attack: Option<usize> = None;

            match self.order[i] {
                Order::Idle => {}
                Order::Move { x, y } => {
                    if dist2(me, x, y) <= Fx::from_ratio(4, 10) {
                        self.order[i] = Order::Idle;
                    } else {
                        move_to = Some((x, y));
                    }
                }
                Order::Attack { target } => {
                    let t = target as usize;
                    if self.arena.alive[t] && self.owner[t] != self.owner[i] {
                        let tp = self.pos[t];
                        if dist2(me, tp.x, tp.y) <= inf.range2 {
                            attack = Some(t);
                        } else {
                            move_to = Some((tp.x, tp.y));
                        }
                    } else {
                        self.order[i] = Order::Idle;
                    }
                }
                Order::AttackMove { x, y } => {
                    let near = self.nearest_enemy(i).filter(|&(_, d2)| d2 <= inf.aggro2);
                    if let Some((t, d2)) = near {
                        if d2 <= inf.range2 {
                            attack = Some(t as usize);
                        } else {
                            let tp = self.pos[t as usize];
                            move_to = Some((tp.x, tp.y));
                        }
                    } else if dist2(me, x, y) <= Fx::from_ratio(4, 10) {
                        self.order[i] = Order::Idle;
                    } else {
                        move_to = Some((x, y));
                    }
                }
            }

            // Attack if ready; otherwise advance toward the move-target.
            if let Some(t) = attack {
                if self.cooldown[i] <= Fx::ZERO {
                    damage[t] += inf.damage;
                    self.cooldown[i] = inf.attack_cd;
                }
            } else if let Some((tx, ty)) = move_to {
                let dx = tx - me.x;
                let dy = ty - me.y;
                let d = (dx * dx + dy * dy).sqrt();
                if d > inf.speed && d > Fx::ZERO {
                    let s = inf.speed / d;
                    self.pos[i].x = me.x + dx * s;
                    self.pos[i].y = me.y + dy * s;
                } else {
                    self.pos[i].x = tx;
                    self.pos[i].y = ty;
                }
            }

            self.cooldown[i] = (self.cooldown[i] - Fx::ONE).max(Fx::ZERO);
        }

        // Apply damage, then resolve deaths (deterministic: ascending index).
        for (i, &d) in damage.iter().enumerate() {
            if d > Fx::ZERO && self.arena.alive[i] {
                self.hp[i] -= d;
            }
        }
        for i in 0..cap {
            if self.arena.alive[i] && self.hp[i] <= Fx::ZERO {
                self.arena.free_index(i as u32);
            }
        }

        self.separate();
    }

    /// Push overlapping infantry apart so no two units share a spot.
    ///
    /// Pushes are computed from a single consistent snapshot of positions
    /// (read all, then apply), so the result is independent of iteration order
    /// and stays deterministic. Magnitude is proportional to the overlap and
    /// clamped to `SEP_MAX`, so units glide apart and settle exactly at
    /// `SEP_DIST` with no oscillation.
    fn separate(&mut self) {
        let cap = self.arena.capacity();
        let sep2 = SEP_DIST * SEP_DIST;
        let coincident = Fx::from_ratio(1, 64);
        let mut push = vec![(Fx::ZERO, Fx::ZERO); cap];

        for (i, slot) in push.iter_mut().enumerate() {
            if !self.arena.alive[i] || self.kind[i] != Kind::Infantry {
                continue;
            }
            let me = self.pos[i];
            let (mut px, mut py) = (Fx::ZERO, Fx::ZERO);
            for j in 0..cap {
                if i == j || !self.arena.alive[j] || self.kind[j] != Kind::Infantry {
                    continue;
                }
                let dx = me.x - self.pos[j].x;
                let dy = me.y - self.pos[j].y;
                let d2 = dx * dx + dy * dy;
                if d2 >= sep2 {
                    continue;
                }
                if d2 <= coincident {
                    // (Near-)coincident: nudge apart by index so a pair always
                    // splits along x; the parity term keeps clusters from
                    // collapsing onto a single line.
                    px += if i < j { SEP_DIST } else { -SEP_DIST };
                    py += if (i ^ j) & 1 == 0 {
                        SEP_DIST
                    } else {
                        -SEP_DIST
                    };
                    continue;
                }
                let d = d2.sqrt();
                let inv = (SEP_DIST - d) / d; // overlap spread over the offset's length
                px += dx * inv;
                py += dy * inv;
            }
            *slot = (px, py);
        }

        for (i, &(rawx, rawy)) in push.iter().enumerate() {
            if !self.arena.alive[i] || self.kind[i] != Kind::Infantry {
                continue;
            }
            let mut sx = rawx * SEP_FACTOR;
            let mut sy = rawy * SEP_FACTOR;
            let len2 = sx * sx + sy * sy;
            if len2 > SEP_MAX * SEP_MAX {
                let k = SEP_MAX / len2.sqrt();
                sx = sx * k;
                sy = sy * k;
            }
            self.pos[i].x += sx;
            self.pos[i].y += sy;
        }
    }

    /// Read-only snapshot for rendering and the HUD.
    pub fn snapshot(&self) -> Vec<Snap> {
        let mut out = Vec::with_capacity(self.alive_count());
        for i in 0..self.arena.capacity() {
            if !self.arena.alive[i] {
                continue;
            }
            // Build progress of the current unit (display-only).
            let build_frac = if self.kind[i] == Kind::Barracks && self.prod[i] > Fx::ZERO {
                (Fx::from_int(PROD_TICKS) - self.prod[i]) / Fx::from_int(PROD_TICKS)
            } else {
                Fx::ZERO
            };
            out.push(Snap {
                index: i as u32,
                generation: self.arena.generation[i],
                kind: self.kind[i],
                owner: self.owner[i],
                pos: self.pos[i],
                hp: self.hp[i],
                max_hp: stats(self.kind[i]).max_hp,
                moving: !matches!(self.order[i], Order::Idle),
                queued: self.queue[i],
                build_frac,
            });
        }
        out
    }

    /// A 64-bit FNV-1a hash of the entire simulation state (desync detection).
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
            h.write_u32(match self.kind[i] {
                Kind::Infantry => 0,
                Kind::Barracks => 1,
            });
            h.write_u32(self.owner[i] as u32);
            h.write_i64(self.pos[i].x.to_raw());
            h.write_i64(self.pos[i].y.to_raw());
            h.write_i64(self.hp[i].to_raw());
            h.write_i64(self.cooldown[i].to_raw());
            h.write_i64(self.prod[i].to_raw());
            h.write_u32(self.queue[i]);
            let (tag, a, b) = match self.order[i] {
                Order::Idle => (0u64, 0i64, 0i64),
                Order::Move { x, y } => (1, x.to_raw(), y.to_raw()),
                Order::AttackMove { x, y } => (2, x.to_raw(), y.to_raw()),
                Order::Attack { target } => (3, target as i64, 0),
            };
            h.write_u64(tag);
            h.write_i64(a);
            h.write_i64(b);
        }
        for &ore in &self.ore {
            h.write_i64(ore.to_raw());
        }
        h.finish()
    }
}

/// Minimal FNV-1a (64-bit), pinned: it is part of the desync protocol.
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

    fn fx(i: i32) -> Fx {
        Fx::from_int(i)
    }

    #[test]
    fn combat_resolves_and_is_deterministic() {
        let build = || {
            let mut w = World::new(42);
            w.step(&[
                Command::SpawnUnit {
                    owner: 0,
                    kind: UnitKind::Infantry,
                    x: fx(0),
                    y: fx(0),
                },
                Command::SpawnUnit {
                    owner: 1,
                    kind: UnitKind::Infantry,
                    x: fx(3),
                    y: fx(0),
                },
            ]);
            for _ in 0..400 {
                w.step(&[]);
            }
            w
        };
        let a = build();
        let b = build();
        assert_eq!(a.state_hash(), b.state_hash());
        // Two adjacent enemies should fight until one dies.
        assert!(a.alive_count() <= 1);
    }

    #[test]
    fn player_barracks_trains_only_on_command() {
        let mut w = World::new(7);
        w.step(&[Command::SpawnBuilding {
            owner: 0,
            kind: BuildingKind::Barracks,
            x: fx(0),
            y: fx(0),
        }]);
        // The player's barracks must not auto-produce.
        for _ in 0..120 {
            w.step(&[]);
        }
        assert_eq!(w.alive_count(), 1);
        // Queue training; units build over time.
        w.step(&[
            Command::Train { building: 0 },
            Command::Train { building: 0 },
        ]);
        for _ in 0..200 {
            w.step(&[]);
        }
        assert!(w.alive_count() > 1);
    }

    #[test]
    fn no_barracks_auto_produces() {
        // Nobody auto-produces - an idle barracks (any owner) stays alone.
        for owner in [0u16, 1] {
            let mut w = World::new(7);
            w.step(&[Command::SpawnBuilding {
                owner,
                kind: BuildingKind::Barracks,
                x: fx(0),
                y: fx(0),
            }]);
            for _ in 0..200 {
                w.step(&[]);
            }
            assert_eq!(w.alive_count(), 1);
        }
    }

    #[test]
    fn training_costs_ore_and_is_capped() {
        let mut w = World::new(7);
        w.step(&[Command::SpawnBuilding {
            owner: 0,
            kind: BuildingKind::Barracks,
            x: fx(0),
            y: fx(0),
        }]);
        let start = w.ore(0);
        // More Train commands than the 200 stockpile can pay for (50 each).
        let mut cmds = Vec::new();
        for _ in 0..8 {
            cmds.push(Command::Train { building: 0 });
        }
        w.step(&cmds);
        // Only what we could afford got queued, and ore was spent.
        let snap = w.snapshot();
        let b = snap.iter().find(|s| s.index == 0).unwrap();
        assert_eq!(b.queued, 4); // 200 / 50
        assert!(w.ore(0) < start);
    }
}
