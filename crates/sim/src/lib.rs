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

use math::{Fx, Vec2, Vec3};
use protocol::{BuildingKind, Command, PlayerId, ResourceKind, UnitKind};

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
    /// Builder/harvester: moves and avoids stacking like infantry. Carries a
    /// feeble tool attack used only on explicit orders (no auto-aggro).
    Worker,
    /// Heavy assault unit: slow, high HP, hits hard.
    Heavy,
    /// Headquarters (Astromancer Spire, Hollowmen Command HQ): the tech root.
    /// Trains workers and is the workers' deposit point.
    Hq,
    Barracks,
    /// Defensive emplacement: immobile, auto-fires on nearby enemies.
    Turret,
    /// Supply depot: raises the owner's unit cap. Inert otherwise.
    Supply,
    /// Ore crystals: harvested into the ore stockpile.
    OreNode,
    /// Carbon gas geyser: harvested into the carbon stockpile.
    CarbonNode,
}

/// Mobile units: they path, take move orders, and obey the no-stacking rule.
fn is_mobile(k: Kind) -> bool {
    matches!(k, Kind::Infantry | Kind::Worker | Kind::Heavy)
}

/// Combat units that auto-acquire and engage enemies (excludes workers).
fn is_fighter(k: Kind) -> bool {
    matches!(k, Kind::Infantry | Kind::Heavy)
}

/// A building (occupies a footprint, owned, can be a drop-off / target).
fn is_building(k: Kind) -> bool {
    matches!(k, Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply)
}

/// Buildings that run a production queue (HQ makes workers, Barracks fighters).
fn is_producer(k: Kind) -> bool {
    matches!(k, Kind::Hq | Kind::Barracks)
}

/// Workers deposit their load at these (the HQ, or a forward Barracks).
fn is_dropoff(k: Kind) -> bool {
    matches!(k, Kind::Hq | Kind::Barracks)
}

/// Harvestable resource nodes (neutral, static, not valid combat targets).
fn is_resource(k: Kind) -> bool {
    matches!(k, Kind::OreNode | Kind::CarbonNode)
}

/// A node's full starting amount, by kind (also used for the display fraction).
fn node_capacity(k: Kind) -> Fx {
    match k {
        Kind::OreNode => Fx::from_int(1500),
        Kind::CarbonNode => Fx::from_int(1200),
        _ => Fx::ZERO,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Order {
    Idle,
    Move {
        x: Fx,
        y: Fx,
    },
    AttackMove {
        x: Fx,
        y: Fx,
    },
    Attack {
        target: u32,
    },
    /// Worker: cycle between mining `node` and depositing at the nearest base.
    Harvest {
        node: u32,
    },
    /// Worker: walk to `(x, y)` and raise a building of `kind` (pays ore there).
    Build {
        kind: BuildingKind,
        x: Fx,
        y: Fx,
    },
}

struct Stats {
    max_hp: Fx,
    speed: Fx,
    /// Attack reach measured to the target's *edge* (its obstacle radius is
    /// added per target), so big buildings can be hit from outside their
    /// footprint.
    range: Fx,
    damage: Fx,
    attack_cd: Fx,
    aggro2: Fx,
}

fn stats(kind: Kind) -> Stats {
    match kind {
        Kind::Infantry => Stats {
            max_hp: Fx::from_int(50),
            speed: Fx::from_ratio(30, 100),
            range: Fx::from_int(7),
            damage: Fx::from_int(5),
            attack_cd: Fx::from_int(12),
            aggro2: Fx::from_int(256), // aggro 16
        },
        // Workers move like infantry; a feeble close-range tool attack, used
        // only when explicitly ordered (no auto-aggro - see `is_fighter`).
        Kind::Worker => Stats {
            max_hp: Fx::from_int(40),
            speed: Fx::from_ratio(30, 100),
            range: Fx::from_int(2),
            damage: Fx::from_int(3),
            attack_cd: Fx::from_int(16),
            aggro2: Fx::ZERO,
        },
        // Heavy: slow bruiser with lots of HP and a heavy hit.
        Kind::Heavy => Stats {
            max_hp: Fx::from_int(180),
            speed: Fx::from_ratio(18, 100),
            range: Fx::from_int(9),
            damage: Fx::from_int(14),
            attack_cd: Fx::from_int(20),
            aggro2: Fx::from_int(324), // aggro 18
        },
        // HQ: the base anchor; the toughest structure on the field.
        Kind::Hq => Stats {
            max_hp: Fx::from_int(900),
            speed: Fx::ZERO,
            range: Fx::ZERO,
            damage: Fx::ZERO,
            attack_cd: Fx::ZERO,
            aggro2: Fx::ZERO,
        },
        Kind::Barracks => Stats {
            max_hp: Fx::from_int(500),
            speed: Fx::ZERO,
            range: Fx::ZERO,
            damage: Fx::ZERO,
            attack_cd: Fx::ZERO,
            aggro2: Fx::ZERO,
        },
        // Turret: immobile auto-defense with good range.
        Kind::Turret => Stats {
            max_hp: Fx::from_int(260),
            speed: Fx::ZERO,
            range: Fx::from_int(12),
            damage: Fx::from_int(9),
            attack_cd: Fx::from_int(14),
            aggro2: Fx::from_int(144),
        },
        // Supply depot: cheap, unarmed, just a wall of habitat plating.
        Kind::Supply => Stats {
            max_hp: Fx::from_int(300),
            speed: Fx::ZERO,
            range: Fx::ZERO,
            damage: Fx::ZERO,
            attack_cd: Fx::ZERO,
            aggro2: Fx::ZERO,
        },
        // Resource nodes are inert: tons of "hp" so stray AoE can't pop them.
        Kind::OreNode | Kind::CarbonNode => Stats {
            max_hp: Fx::from_int(100000),
            speed: Fx::ZERO,
            range: Fx::ZERO,
            damage: Fx::ZERO,
            attack_cd: Fx::ZERO,
            aggro2: Fx::ZERO,
        },
    }
}

/// Worker harvesting tuning.
const NEUTRAL: PlayerId = u16::MAX;
const CARRY_CAP: Fx = Fx::from_int(8);
const MINE_RATE: Fx = Fx::from_ratio(1, 5); // resource per tick while mining
const MINE_RANGE2: Fx = Fx::from_int(36); // mine within range 6 of a node
const DEPOSIT_RANGE2: Fx = Fx::from_int(100); // deposit within range 10 of a building

const PROD_TICKS: i32 = 55;
const MAX_QUEUE: u32 = 6;

// Supply: every unit on the field occupies one supply. The HQ provides a base
// block and each Supply depot adds more, up to a hard ceiling. Production
// holds (without dropping the queue) while the owner is at their cap.
pub const SUPPLY_PER_HQ: u32 = 10;
pub const SUPPLY_PER_DEPOT: u32 = 8;
pub const MAX_SUPPLY: u32 = 60;

// Economy: ore + carbon, both mined by workers - there is no passive income.
// Players start with an ore stockpile and pay per trained unit / built
// structure. Nobody auto-produces - every unit is queued by command. Advanced
// units and the turret also cost carbon, so harvesting the gas geyser matters.
const STARTING_ORE: i32 = 200;
pub const TRAIN_COST: i32 = 50;
pub const WORKER_COST: i32 = 40;

/// `(ore, carbon)` cost to train a unit of `kind`.
fn unit_cost(kind: UnitKind) -> (Fx, Fx) {
    match kind {
        UnitKind::Infantry => (Fx::from_int(TRAIN_COST), Fx::ZERO),
        UnitKind::Heavy => (Fx::from_int(120), Fx::from_int(60)),
        UnitKind::Worker => (Fx::from_int(WORKER_COST), Fx::ZERO),
    }
}

/// `(ore, carbon)` cost to construct a building of `kind`.
fn building_cost(kind: BuildingKind) -> (Fx, Fx) {
    match kind {
        // A new HQ founds an expansion; priced like a StarCraft town hall.
        BuildingKind::Hq => (Fx::from_int(400), Fx::ZERO),
        BuildingKind::Barracks => (Fx::from_int(150), Fx::ZERO),
        BuildingKind::Turret => (Fx::from_int(90), Fx::from_int(50)),
        BuildingKind::Supply => (Fx::from_int(100), Fx::ZERO),
    }
}

// Construction: a worker walks to a site and raises a building, paid on arrival.
// The structure then ticks up over `CONSTRUCT_TICKS` before it is functional
// (income / production / turret fire). Sites keep clear of each other.
const CONSTRUCT_TICKS: i32 = 80;
const BUILD_RANGE2: Fx = Fx::from_int(64); // worker builds within range 8 of the site
const BUILD_CLEAR2: Fx = Fx::from_int(196); // sites must be >= 14 from other buildings

// Bot commander tuning.
const BOT_THINK_TICKS: u64 = 15; // re-plan cadence (every ~0.75s at 20 Hz)
const BOT_MAX_BUILDINGS: usize = 3; // production/defense structures (HQ and depots excluded)
const BOT_TARGET_WORKERS: usize = 6; // staff the mining crew up to this

// Obstacle avoidance: mobile units are pushed out of these static footprints so
// they path around structures instead of through them. Radii are world units.
const UNIT_RADIUS: Fx = Fx::from_int(1);
fn obstacle_radius(k: Kind) -> Option<Fx> {
    match k {
        // Large footprint tier (see docs/building-design-language.md).
        Kind::Hq => Some(Fx::from_int(7)),
        Kind::Barracks => Some(Fx::from_int(6)),
        Kind::Turret => Some(Fx::from_ratio(5, 2)), // 2.5
        Kind::Supply => Some(Fx::from_int(3)),      // small 5x5 tier
        Kind::OreNode | Kind::CarbonNode => Some(Fx::from_ratio(7, 2)), // 3.5
        _ => None,
    }
}

// Collision avoidance: infantry never share a spot. Each tick a unit is pushed
// away from any other infantry whose center is closer than `SEP_DIST`, so a
// crowd drifts apart and a group-move settles into distinct cells rather than
// stacking on one point. `SEP_FACTOR` (each pair resolves half the overlap) and
// the `SEP_MAX` step clamp keep it a smooth drift instead of a teleport.
// The tolerance is tight and the per-tick push caps *below* move speed, so a
// unit shouldering through a crowd keeps most of its momentum instead of
// stalling against neighbours.
const SEP_DIST: Fx = Fx::from_ratio(2, 1); // desired min spacing between centers
const SEP_FACTOR: Fx = Fx::from_ratio(1, 2);
const SEP_MAX: Fx = Fx::from_ratio(18, 100); // max push per tick (< infantry speed)

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
    /// Workers only, display-only: actively mining a node this tick.
    pub mining: bool,
    /// Resource nodes only, display-only: fraction of the node remaining (1..0).
    pub resource_frac: Fx,
    /// Buildings only: units queued for production and the current unit's
    /// build progress (0..1). Display-only; not part of the state hash.
    pub queued: u32,
    pub build_frac: Fx,
    /// Buildings only: construction progress, 1.0 once functional. Below 1.0 the
    /// structure is still being raised (display-only fraction).
    pub construct_frac: Fx,
    /// Production buildings only: where freshly trained units gather
    /// (display-only; set by `Command::SetRally`).
    pub rally: Vec3,
    /// Direction the entity faces (raw, un-normalized; renderer derives yaw).
    pub facing: Vec2,
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
    /// Per-player carbon stockpile, indexed by `PlayerId`.
    carbon: Vec<Fx>,
    /// Resource nodes only: amount of material remaining.
    amount: Vec<Fx>,
    /// Workers only: material currently carried, and which stockpile it feeds
    /// (0 = ore, 1 = carbon).
    carried: Vec<Fx>,
    carry_kind: Vec<u8>,
    /// Workers only, display-only: actively mining a node this tick.
    mining: Vec<bool>,
    /// Buildings only: ticks of construction remaining (0 = functional).
    construct: Vec<Fx>,
    /// Producers only: which unit kind the building is currently producing
    /// (0 = Infantry, 1 = Heavy, 2 = Worker).
    prod_kind: Vec<u8>,
    /// Per-player flag: this player is driven by the in-sim bot commander.
    bot: Vec<bool>,
    /// Facing per entity: the raw (un-normalized) direction it last moved,
    /// mined, or attacked toward. Part of the deterministic state (hashed);
    /// the renderer normalizes it into a yaw.
    facing: Vec<Vec2>,
    /// Shots fired this tick, as `(attacker, target)` slot indices. Display
    /// events for the client fx layer; cleared every step, never hashed.
    shots: Vec<(u32, u32)>,
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
            carbon: Vec::new(),
            amount: Vec::new(),
            carried: Vec::new(),
            carry_kind: Vec::new(),
            mining: Vec::new(),
            construct: Vec::new(),
            prod_kind: Vec::new(),
            bot: Vec::new(),
            facing: Vec::new(),
            shots: Vec::new(),
        }
    }

    /// Mark a player as bot-controlled (the in-sim commander mines, builds, and
    /// trains for them). Must be set identically on every peer.
    pub fn set_bot(&mut self, player: PlayerId, on: bool) {
        let i = player as usize;
        while self.bot.len() <= i {
            self.bot.push(false);
        }
        self.bot[i] = on;
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

    /// A player's current carbon (starts at zero).
    pub fn carbon(&self, player: PlayerId) -> Fx {
        self.carbon
            .get(player as usize)
            .copied()
            .unwrap_or(Fx::ZERO)
    }

    fn carbon_mut(&mut self, player: PlayerId) -> &mut Fx {
        let i = player as usize;
        while self.carbon.len() <= i {
            self.carbon.push(Fx::ZERO);
        }
        &mut self.carbon[i]
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
            self.amount.push(Fx::ZERO);
            self.carried.push(Fx::ZERO);
            self.carry_kind.push(0);
            self.mining.push(false);
            self.construct.push(Fx::ZERO);
            self.prod_kind.push(0);
            // Face "north" (toward -y) until the first move/attack.
            self.facing.push(Vec2::new(Fx::ZERO, Fx::from_int(-1)));
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
        self.amount[i] = node_capacity(kind);
        self.carried[i] = Fx::ZERO;
        self.carry_kind[i] = 0;
        self.mining[i] = false;
        // Scenario / production spawns are instant; the Build path overrides this
        // to ramp construction up over CONSTRUCT_TICKS.
        self.construct[i] = Fx::ZERO;
        self.prod_kind[i] = 0;
        self.facing[i] = Vec2::new(Fx::ZERO, Fx::from_int(-1));
        if owner != NEUTRAL {
            let _ = self.ore_mut(owner); // materialize the owner's stockpile
        }
        // Default rally a little "south" of a building.
        self.rally[i] = Vec3::new(x, y - Fx::from_int(7), Fx::ZERO);
        id
    }

    /// Supply in use: every living mobile unit (fighters and workers) is one.
    pub fn supply_used(&self, owner: PlayerId) -> u32 {
        let mut n = 0;
        for i in 0..self.arena.capacity() {
            if self.arena.alive[i] && is_mobile(self.kind[i]) && self.owner[i] == owner {
                n += 1;
            }
        }
        n
    }

    /// Supply ceiling: a block per completed HQ plus a block per completed
    /// Supply depot, clamped to [`MAX_SUPPLY`].
    pub fn supply_cap(&self, owner: PlayerId) -> u32 {
        let mut cap = 0;
        for i in 0..self.arena.capacity() {
            if !self.arena.alive[i] || self.owner[i] != owner || self.construct[i] > Fx::ZERO {
                continue;
            }
            cap += match self.kind[i] {
                Kind::Hq => SUPPLY_PER_HQ,
                Kind::Supply => SUPPLY_PER_DEPOT,
                _ => 0,
            };
        }
        cap.min(MAX_SUPPLY)
    }

    /// Nearest living entity of a different owner: returns `(index, dist2)`.
    fn nearest_enemy(&self, i: usize) -> Option<(u32, Fx)> {
        let me = self.pos[i];
        let my_owner = self.owner[i];
        let mut best: Option<(u32, Fx)> = None;
        for j in 0..self.arena.capacity() {
            if !self.arena.alive[j] || self.owner[j] == my_owner || is_resource(self.kind[j]) {
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

    /// Nearest living building owned by `owner` (a worker's drop-off point).
    fn nearest_dropoff(&self, owner: PlayerId, from: Vec3) -> Option<Vec3> {
        let mut best: Option<(Vec3, Fx)> = None;
        for j in 0..self.arena.capacity() {
            if !self.arena.alive[j] || !is_dropoff(self.kind[j]) || self.owner[j] != owner {
                continue;
            }
            let d2 = dist2(from, self.pos[j].x, self.pos[j].y);
            match best {
                Some((_, bd)) if bd <= d2 => {}
                _ => best = Some((self.pos[j], d2)),
            }
        }
        best.map(|(p, _)| p)
    }

    /// Nearest living resource node (any kind), for auto-retargeting a worker
    /// whose node ran out.
    fn nearest_node(&self, from: Vec3) -> Option<u32> {
        let mut best: Option<(u32, Fx)> = None;
        for j in 0..self.arena.capacity() {
            if !self.arena.alive[j] || !is_resource(self.kind[j]) {
                continue;
            }
            let d2 = dist2(from, self.pos[j].x, self.pos[j].y);
            match best {
                Some((_, bd)) if bd <= d2 => {}
                _ => best = Some((j as u32, d2)),
            }
        }
        best.map(|(i, _)| i)
    }

    /// Advance the simulation by one tick. The only place game truth changes.
    /// Nearest enemy base building to `from` (skipping neutral resource nodes).
    fn nearest_enemy_building(&self, owner: PlayerId, from: Option<Vec3>) -> Option<Vec3> {
        let from = from?;
        let mut best: Option<(Vec3, Fx)> = None;
        for j in 0..self.arena.capacity() {
            if self.arena.alive[j]
                && is_producer(self.kind[j])
                && self.owner[j] != owner
                && self.owner[j] != NEUTRAL
            {
                let d2 = dist2(from, self.pos[j].x, self.pos[j].y);
                match best {
                    Some((_, bd)) if bd <= d2 => {}
                    _ => best = Some((self.pos[j], d2)),
                }
            }
        }
        best.map(|(p, _)| p)
    }

    /// Deterministic bot commander: each bot player's intents for this tick, as
    /// ordinary `Command`s (so they flow through the same validated path as a
    /// human's). Mines with idle workers, builds up to a cap, trains a surplus
    /// into units, and pushes a massed army at the enemy. Throttled by tick.
    fn ai_commands(&self) -> Vec<Command> {
        let mut out = Vec::new();
        if self.bot.iter().all(|&b| !b) || !self.tick.is_multiple_of(BOT_THINK_TICKS) {
            return out;
        }
        let cap = self.arena.capacity();
        for p in 0..self.bot.len() {
            if !self.bot[p] {
                continue;
            }
            let owner = p as PlayerId;
            let mut idle_workers: Vec<u32> = Vec::new();
            let mut any_worker: Option<u32> = None;
            let mut worker_count = 0usize;
            let mut hqs: Vec<u32> = Vec::new();
            let mut barracks: Vec<u32> = Vec::new();
            let mut hq_base: Option<Vec3> = None;
            let mut barracks_base: Option<Vec3> = None;
            let mut building_count = 0usize;
            let mut infantry_count = 0usize;
            let mut idle_infantry: Vec<u32> = Vec::new();
            let mut worker_building = false;
            for i in 0..cap {
                if !self.arena.alive[i] || self.owner[i] != owner {
                    continue;
                }
                match self.kind[i] {
                    Kind::Worker => {
                        worker_count += 1;
                        if any_worker.is_none() {
                            any_worker = Some(i as u32);
                        }
                        match self.order[i] {
                            Order::Idle => idle_workers.push(i as u32),
                            Order::Build { .. } => worker_building = true,
                            _ => {}
                        }
                    }
                    Kind::Hq => {
                        hqs.push(i as u32);
                        if hq_base.is_none() {
                            hq_base = Some(self.pos[i]);
                        }
                    }
                    Kind::Barracks => {
                        barracks.push(i as u32);
                        building_count += 1;
                        if barracks_base.is_none() {
                            barracks_base = Some(self.pos[i]);
                        }
                    }
                    Kind::Turret => building_count += 1,
                    Kind::Infantry | Kind::Heavy => {
                        infantry_count += 1;
                        if self.order[i] == Order::Idle {
                            idle_infantry.push(i as u32);
                        }
                    }
                    _ => {}
                }
            }
            // The HQ anchors the base; fall back to a Barracks if it fell.
            let base = hq_base.or(barracks_base);
            let ore = self.ore(owner);
            let carbon = self.carbon(owner);
            // Supply first when the cap is close; otherwise production up to
            // two Barracks, then sprinkle Turrets.
            let supply_tight = self.supply_used(owner) + 3 >= self.supply_cap(owner);
            let next_build = if supply_tight {
                BuildingKind::Supply
            } else if barracks.len() < 2 {
                BuildingKind::Barracks
            } else {
                BuildingKind::Turret
            };
            let (build_ore, build_carbon) = building_cost(next_build);

            // 1) Construct: when flush and under the cap (depots are exempt
            // from the cap: the bot always builds out of a supply block).
            let mut builder: Option<u32> = None;
            if (supply_tight || building_count < BOT_MAX_BUILDINGS)
                && !worker_building
                && ore >= build_ore + Fx::from_int(60)
                && carbon >= build_carbon
            {
                builder = idle_workers.first().copied().or(any_worker);
                if let (Some(w), Some(bp)) = (builder, base) {
                    // Depots fill their own row behind the production line, a
                    // slot per depot built, so sites never collide.
                    let (off, dy) = if next_build == BuildingKind::Supply {
                        let slot = (self.supply_cap(owner) / SUPPLY_PER_DEPOT) as i32 % 5;
                        ((slot - 2) * 14, 44)
                    } else {
                        (((building_count % 3) as i32 - 1) * 24, 26)
                    };
                    out.push(Command::Build {
                        unit: w,
                        kind: next_build,
                        x: bp.x + Fx::from_int(off),
                        y: bp.y + Fx::from_int(dy),
                    });
                }
            }

            // 2) Mine: idle workers (other than the builder) take the nearest node.
            for &w in &idle_workers {
                if Some(w) == builder {
                    continue;
                }
                if let Some(node) = self.nearest_node(self.pos[w as usize]) {
                    out.push(Command::Harvest { unit: w, node });
                }
            }

            // 3) Staff the economy: train workers at the HQ until the mining
            // crew is full (they pay for themselves quickly).
            if worker_count < BOT_TARGET_WORKERS {
                let (wo, wc) = unit_cost(UnitKind::Worker);
                for &h in &hqs {
                    if self.queue[h as usize] < MAX_QUEUE && ore >= wo && carbon >= wc {
                        out.push(Command::Train {
                            building: h,
                            kind: UnitKind::Worker,
                        });
                        break;
                    }
                }
            }

            // 4) Train: spend surplus ore (keeping a build reserve) on one unit;
            // upgrade to a Heavy when the bot has banked enough carbon.
            let reserve = if building_count < BOT_MAX_BUILDINGS {
                build_ore
            } else {
                Fx::ZERO
            };
            let want = if carbon >= unit_cost(UnitKind::Heavy).1 {
                UnitKind::Heavy
            } else {
                UnitKind::Infantry
            };
            let (uo, uc) = unit_cost(want);
            for &b in &barracks {
                if self.queue[b as usize] < MAX_QUEUE && ore >= reserve + uo && carbon >= uc {
                    out.push(Command::Train {
                        building: b,
                        kind: want,
                    });
                    break;
                }
            }

            // 5) Attack: once a force has massed, push idle infantry at the foe.
            if infantry_count >= 12 && self.tick.is_multiple_of(BOT_THINK_TICKS * 12) {
                if let Some(t) = self.nearest_enemy_building(owner, base) {
                    for &u in &idle_infantry {
                        out.push(Command::AttackMove {
                            unit: u,
                            x: t.x,
                            y: t.y,
                        });
                    }
                }
            }
        }
        out
    }

    pub fn step(&mut self, commands: &[Command]) {
        self.shots.clear();
        self.apply_commands(commands);
        // Bot players issue their commands through the same path, deterministically.
        let ai = self.ai_commands();
        self.apply_commands(&ai);
        self.construction();
        self.acquire_targets();
        self.production();
        self.units_update();
        self.tick += 1;
    }

    /// Shots fired during the last step, as `(attacker, target)` slot indices.
    /// Presentation events (muzzle flashes, tracers); not part of the hash.
    pub fn shots(&self) -> &[(u32, u32)] {
        &self.shots
    }

    /// Squared reach of an attack with `range` against target `t`: range is
    /// measured to the target's *edge*, so a building's footprint (or a mobile
    /// unit's body radius) extends it. This is what lets units hit buildings
    /// whose center they can never reach, and short-range tools land on units
    /// held apart by separation.
    fn reach2(&self, range: Fx, t: usize) -> Fx {
        let pad = obstacle_radius(self.kind[t]).unwrap_or(if is_mobile(self.kind[t]) {
            UNIT_RADIUS
        } else {
            Fx::ZERO
        });
        let r = range + pad;
        r * r
    }

    /// Turn entity `i` toward the point `(tx, ty)`. Stored raw (un-normalized);
    /// zero-length turns are ignored so facing always stays meaningful.
    fn face(&mut self, i: usize, tx: Fx, ty: Fx) {
        let dx = tx - self.pos[i].x;
        let dy = ty - self.pos[i].y;
        if dx != Fx::ZERO || dy != Fx::ZERO {
            self.facing[i] = Vec2::new(dx, dy);
        }
    }

    /// Tick down construction on buildings being raised.
    fn construction(&mut self) {
        for i in 0..self.arena.capacity() {
            if self.arena.alive[i] && self.construct[i] > Fx::ZERO {
                self.construct[i] = (self.construct[i] - Fx::ONE).max(Fx::ZERO);
            }
        }
    }

    fn apply_commands(&mut self, commands: &[Command]) {
        for &cmd in commands {
            match cmd {
                Command::SpawnUnit { owner, kind, x, y } => {
                    let k = match kind {
                        UnitKind::Infantry => Kind::Infantry,
                        UnitKind::Worker => Kind::Worker,
                        UnitKind::Heavy => Kind::Heavy,
                    };
                    self.spawn(k, owner, x, y);
                }
                Command::SpawnBuilding { owner, kind, x, y } => {
                    let k = match kind {
                        BuildingKind::Hq => Kind::Hq,
                        BuildingKind::Barracks => Kind::Barracks,
                        BuildingKind::Turret => Kind::Turret,
                        BuildingKind::Supply => Kind::Supply,
                    };
                    self.spawn(k, owner, x, y);
                }
                Command::SpawnResource { kind, x, y } => {
                    let k = match kind {
                        ResourceKind::Ore => Kind::OreNode,
                        ResourceKind::Carbon => Kind::CarbonNode,
                    };
                    self.spawn(k, NEUTRAL, x, y);
                }
                Command::Harvest { unit, node } => {
                    let (u, n) = (unit as usize, node as usize);
                    if self.arena.alive_at(unit)
                        && self.kind[u] == Kind::Worker
                        && self.arena.alive_at(node)
                        && is_resource(self.kind[n])
                    {
                        self.order[u] = Order::Harvest { node };
                    }
                }
                Command::Build { unit, kind, x, y } => {
                    if self.arena.alive_at(unit) && self.kind[unit as usize] == Kind::Worker {
                        self.order[unit as usize] = Order::Build { kind, x, y };
                    }
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
                Command::Train { building, kind } => {
                    let b = building as usize;
                    // The HQ trains workers; the Barracks trains fighters.
                    let trainable = self.arena.alive_at(building)
                        && matches!(
                            (self.kind[b], kind),
                            (Kind::Barracks, UnitKind::Infantry | UnitKind::Heavy)
                                | (Kind::Hq, UnitKind::Worker)
                        );
                    let (ore, carbon) = unit_cost(kind);
                    if trainable
                        && self.construct[b] <= Fx::ZERO
                        && self.queue[b] < MAX_QUEUE
                        && self.ore(self.owner[b]) >= ore
                        && self.carbon(self.owner[b]) >= carbon
                    {
                        let owner = self.owner[b];
                        *self.ore_mut(owner) -= ore;
                        *self.carbon_mut(owner) -= carbon;
                        self.queue[b] += 1;
                        self.prod_kind[b] = match kind {
                            UnitKind::Infantry => 0,
                            UnitKind::Heavy => 1,
                            UnitKind::Worker => 2,
                        };
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
        if self.arena.alive_at(unit) && is_mobile(self.kind[unit as usize]) {
            self.order[unit as usize] = order;
        }
    }

    /// Idle units auto-engage the nearest enemy within aggro range.
    fn acquire_targets(&mut self) {
        for i in 0..self.arena.capacity() {
            if !self.arena.alive[i] || !is_fighter(self.kind[i]) {
                continue;
            }
            if self.order[i] != Order::Idle {
                continue;
            }
            let aggro2 = stats(self.kind[i]).aggro2;
            if let Some((t, d2)) = self.nearest_enemy(i) {
                if d2 <= aggro2 {
                    self.order[i] = Order::Attack { target: t };
                }
            }
        }
    }

    fn production(&mut self) {
        for i in 0..self.arena.capacity() {
            if !self.arena.alive[i] || !is_producer(self.kind[i]) || self.construct[i] > Fx::ZERO {
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
            // Built - but hold (without consuming the queue) while at the
            // supply cap; a new depot releases it.
            let owner = self.owner[i];
            if self.supply_used(owner) >= self.supply_cap(owner) {
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

    /// Spawn one queued unit just in front of building `i`, headed to its rally.
    fn produce_at(&mut self, i: usize) {
        let owner = self.owner[i];
        let p = self.pos[i];
        let rally = self.rally[i];
        let kind = match self.prod_kind[i] {
            1 => Kind::Heavy,
            2 => Kind::Worker,
            _ => Kind::Infantry,
        };
        // tiny deterministic spread so they don't stack perfectly
        let jitter = Fx::from_ratio((self.rng.range_u32(7) as i64) - 3, 2);
        let id = self.spawn(kind, owner, p.x + jitter, p.y - Fx::from_int(3));
        self.order[id.index as usize] = Order::Move {
            x: rally.x,
            y: rally.y,
        };
    }

    /// Step entity `i` toward `(tx, ty)` at `speed`, snapping on arrival.
    fn step_toward(&mut self, i: usize, tx: Fx, ty: Fx, speed: Fx) {
        self.face(i, tx, ty);
        let me = self.pos[i];
        let dx = tx - me.x;
        let dy = ty - me.y;
        let d = (dx * dx + dy * dy).sqrt();
        if d > speed && d > Fx::ZERO {
            let s = speed / d;
            self.pos[i].x = me.x + dx * s;
            self.pos[i].y = me.y + dy * s;
        } else {
            self.pos[i].x = tx;
            self.pos[i].y = ty;
        }
    }

    fn units_update(&mut self) {
        let cap = self.arena.capacity();
        let mut damage = vec![Fx::ZERO; cap];

        for i in 0..cap {
            if !self.arena.alive[i] || !is_mobile(self.kind[i]) {
                continue;
            }
            // Per-kind stats: workers have zero range/damage/aggro, so even an
            // attack order just walks them to the target and deals nothing.
            let inf = stats(self.kind[i]);
            let me = self.pos[i];
            self.mining[i] = false;

            // Harvest is a self-contained cycle (mine the node, then return to
            // the nearest base and deposit), handled before the combat orders.
            if let Order::Harvest { node } = self.order[i] {
                let n = node as usize;
                let node_ok = self.arena.alive_at(node) && is_resource(self.kind[n]);
                let returning =
                    self.carried[i] >= CARRY_CAP || (!node_ok && self.carried[i] > Fx::ZERO);
                if returning {
                    // Carry the load home to the nearest owned building.
                    if let Some(bp) = self.nearest_dropoff(self.owner[i], me) {
                        if dist2(me, bp.x, bp.y) <= DEPOSIT_RANGE2 {
                            let load = self.carried[i];
                            let owner = self.owner[i];
                            if self.carry_kind[i] == 1 {
                                *self.carbon_mut(owner) += load;
                            } else {
                                *self.ore_mut(owner) += load;
                            }
                            self.carried[i] = Fx::ZERO;
                            if !node_ok {
                                self.order[i] = match self.nearest_node(me) {
                                    Some(nn) => Order::Harvest { node: nn },
                                    None => Order::Idle,
                                };
                            }
                        } else {
                            self.step_toward(i, bp.x, bp.y, inf.speed);
                        }
                    }
                } else if node_ok {
                    // Walk to the node, then mine it.
                    let np = self.pos[n];
                    if dist2(me, np.x, np.y) <= MINE_RANGE2 {
                        self.face(i, np.x, np.y);
                        let space = CARRY_CAP - self.carried[i];
                        let take = MINE_RATE.min(self.amount[n]).min(space);
                        self.amount[n] -= take;
                        self.carried[i] += take;
                        self.carry_kind[i] = if self.kind[n] == Kind::CarbonNode {
                            1
                        } else {
                            0
                        };
                        self.mining[i] = true;
                        if self.amount[n] <= Fx::ZERO {
                            self.arena.free_index(node);
                        }
                    } else {
                        self.step_toward(i, np.x, np.y, inf.speed);
                    }
                } else {
                    // Empty-handed and the node is gone: find another or stop.
                    self.order[i] = match self.nearest_node(me) {
                        Some(nn) => Order::Harvest { node: nn },
                        None => Order::Idle,
                    };
                }
                self.cooldown[i] = (self.cooldown[i] - Fx::ONE).max(Fx::ZERO);
                continue;
            }

            // Build is also self-contained: walk to the site, then raise the
            // building if the owner can afford it (ore + carbon) and the spot is
            // clear. The new structure then ticks up over CONSTRUCT_TICKS.
            if let Order::Build { kind, x, y } = self.order[i] {
                let owner = self.owner[i];
                if dist2(me, x, y) <= BUILD_RANGE2 {
                    let (ore, carbon) = building_cost(kind);
                    if self.ore(owner) >= ore
                        && self.carbon(owner) >= carbon
                        && self.site_clear(x, y)
                    {
                        *self.ore_mut(owner) -= ore;
                        *self.carbon_mut(owner) -= carbon;
                        let k = match kind {
                            BuildingKind::Hq => Kind::Hq,
                            BuildingKind::Barracks => Kind::Barracks,
                            BuildingKind::Turret => Kind::Turret,
                            BuildingKind::Supply => Kind::Supply,
                        };
                        let id = self.spawn(k, owner, x, y);
                        self.construct[id.index as usize] = Fx::from_int(CONSTRUCT_TICKS);
                    }
                    self.order[i] = Order::Idle;
                } else {
                    self.step_toward(i, x, y, inf.speed);
                }
                self.cooldown[i] = (self.cooldown[i] - Fx::ONE).max(Fx::ZERO);
                continue;
            }

            // Resolve a move-target and/or an attack-target from the order.
            let mut move_to: Option<(Fx, Fx)> = None;
            let mut attack: Option<usize> = None;

            match self.order[i] {
                Order::Idle => {}
                Order::Harvest { .. } => {} // handled above
                Order::Build { .. } => {}   // handled above
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
                        if dist2(me, tp.x, tp.y) <= self.reach2(inf.range, t) {
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
                        if d2 <= self.reach2(inf.range, t as usize) {
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
                // Square up to the target even between shots.
                self.face(i, self.pos[t].x, self.pos[t].y);
                if self.cooldown[i] <= Fx::ZERO {
                    damage[t] += inf.damage;
                    self.cooldown[i] = inf.attack_cd;
                    self.shots.push((i as u32, t as u32));
                }
            } else if let Some((tx, ty)) = move_to {
                let dx = tx - me.x;
                let dy = ty - me.y;
                let d = (dx * dx + dy * dy).sqrt();
                if d > inf.speed && d > Fx::ZERO {
                    let s = inf.speed / d;
                    self.pos[i].x = me.x + dx * s;
                    self.pos[i].y = me.y + dy * s;
                    self.face(i, tx, ty);
                } else {
                    self.pos[i].x = tx;
                    self.pos[i].y = ty;
                }
            }

            self.cooldown[i] = (self.cooldown[i] - Fx::ONE).max(Fx::ZERO);
        }

        // Turrets: immobile auto-defense. Acquire the nearest enemy in range and
        // fire on cooldown, into the same damage buffer the mobile units use.
        for i in 0..cap {
            if !self.arena.alive[i] || self.kind[i] != Kind::Turret || self.construct[i] > Fx::ZERO
            {
                continue;
            }
            let st = stats(Kind::Turret);
            if self.cooldown[i] <= Fx::ZERO {
                if let Some((t, d2)) = self.nearest_enemy(i) {
                    if d2 <= self.reach2(st.range, t as usize) {
                        let t = t as usize;
                        self.face(i, self.pos[t].x, self.pos[t].y);
                        damage[t] += st.damage;
                        self.cooldown[i] = st.attack_cd;
                        self.shots.push((i as u32, t as u32));
                    }
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
        self.avoid_obstacles();
    }

    /// True if `(x, y)` is far enough from every existing building and resource
    /// node to place one.
    fn site_clear(&self, x: Fx, y: Fx) -> bool {
        for j in 0..self.arena.capacity() {
            if !self.arena.alive[j] {
                continue;
            }
            if (is_building(self.kind[j]) || is_resource(self.kind[j]))
                && dist2(self.pos[j], x, y) < BUILD_CLEAR2
            {
                return false;
            }
        }
        true
    }

    /// Push mobile units out of static footprints (buildings, resource nodes) so
    /// they path around structures instead of standing inside them. Obstacles are
    /// snapshotted first, so the result is order-independent and deterministic.
    fn avoid_obstacles(&mut self) {
        let cap = self.arena.capacity();
        let mut obstacles: Vec<(Vec3, Fx)> = Vec::new();
        for j in 0..cap {
            if !self.arena.alive[j] {
                continue;
            }
            if let Some(r) = obstacle_radius(self.kind[j]) {
                obstacles.push((self.pos[j], r));
            }
        }
        for i in 0..cap {
            if !self.arena.alive[i] || !is_mobile(self.kind[i]) {
                continue;
            }
            for &(op, orad) in &obstacles {
                let r = orad + UNIT_RADIUS;
                let dx = self.pos[i].x - op.x;
                let dy = self.pos[i].y - op.y;
                let d2 = dx * dx + dy * dy;
                if d2 >= r * r {
                    continue;
                }
                if d2 <= Fx::from_ratio(1, 64) {
                    // Concentric: shove along +x by the full radius (deterministic).
                    self.pos[i].x = op.x + r;
                    continue;
                }
                let d = d2.sqrt();
                let push = (r - d) / d;
                self.pos[i].x += dx * push;
                self.pos[i].y += dy * push;
            }
        }
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
            if !self.arena.alive[i] || !is_mobile(self.kind[i]) {
                continue;
            }
            let me = self.pos[i];
            let (mut px, mut py) = (Fx::ZERO, Fx::ZERO);
            for j in 0..cap {
                if i == j || !self.arena.alive[j] || !is_mobile(self.kind[j]) {
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
            if !self.arena.alive[i] || !is_mobile(self.kind[i]) {
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
            let resource_frac = if is_resource(self.kind[i]) {
                let cap = node_capacity(self.kind[i]);
                if cap > Fx::ZERO {
                    (self.amount[i] / cap).clamp(Fx::ZERO, Fx::ONE)
                } else {
                    Fx::ZERO
                }
            } else {
                Fx::ZERO
            };
            let construct_frac = if is_building(self.kind[i]) && self.construct[i] > Fx::ZERO {
                (Fx::from_int(CONSTRUCT_TICKS) - self.construct[i]) / Fx::from_int(CONSTRUCT_TICKS)
            } else {
                Fx::ONE
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
                mining: self.mining[i],
                resource_frac,
                queued: self.queue[i],
                build_frac,
                construct_frac,
                rally: self.rally[i],
                facing: self.facing[i],
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
                Kind::Worker => 2,
                Kind::OreNode => 3,
                Kind::CarbonNode => 4,
                Kind::Heavy => 5,
                Kind::Turret => 6,
                Kind::Hq => 7,
                Kind::Supply => 8,
            });
            h.write_u32(self.owner[i] as u32);
            h.write_i64(self.pos[i].x.to_raw());
            h.write_i64(self.pos[i].y.to_raw());
            h.write_i64(self.hp[i].to_raw());
            h.write_i64(self.cooldown[i].to_raw());
            h.write_i64(self.prod[i].to_raw());
            h.write_u32(self.queue[i]);
            h.write_i64(self.amount[i].to_raw());
            h.write_i64(self.carried[i].to_raw());
            h.write_u64(self.carry_kind[i] as u64);
            h.write_i64(self.construct[i].to_raw());
            h.write_u64(self.prod_kind[i] as u64);
            h.write_i64(self.facing[i].x.to_raw());
            h.write_i64(self.facing[i].y.to_raw());
            let (tag, a, b) = match self.order[i] {
                Order::Idle => (0u64, 0i64, 0i64),
                Order::Move { x, y } => (1, x.to_raw(), y.to_raw()),
                Order::AttackMove { x, y } => (2, x.to_raw(), y.to_raw()),
                Order::Attack { target } => (3, target as i64, 0),
                Order::Harvest { node } => (4, node as i64, 0),
                Order::Build { x, y, .. } => (5, x.to_raw(), y.to_raw()),
            };
            h.write_u64(tag);
            h.write_i64(a);
            h.write_i64(b);
        }
        for &ore in &self.ore {
            h.write_i64(ore.to_raw());
        }
        for &carbon in &self.carbon {
            h.write_i64(carbon.to_raw());
        }
        for &b in &self.bot {
            h.write_u64(b as u64);
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

    fn barracks_count(w: &World, owner: PlayerId) -> usize {
        w.snapshot()
            .iter()
            .filter(|s| s.owner == owner && s.kind == Kind::Barracks)
            .count()
    }

    #[test]
    fn bot_mines_and_constructs() {
        // The classic opening: an HQ and a worker by an ore line, nothing
        // else. With no passive income, everything the bot builds must be
        // funded by mining.
        let mut w = World::new(11);
        w.set_bot(1, true);
        w.step(&[
            Command::SpawnBuilding {
                owner: 1,
                kind: BuildingKind::Hq,
                x: fx(0),
                y: fx(0),
            },
            Command::SpawnUnit {
                owner: 1,
                kind: UnitKind::Worker,
                x: fx(6),
                y: fx(6),
            },
            Command::SpawnResource {
                kind: ResourceKind::Ore,
                x: fx(24),
                y: fx(0),
            },
        ]);
        for _ in 0..9000 {
            w.step(&[]);
        }
        // The ore node was mined down (workers harvested it)...
        let node = w.snapshot().into_iter().find(|s| s.kind == Kind::OreNode);
        assert!(
            node.map(|n| n.resource_frac < Fx::ONE).unwrap_or(true),
            "bot should have mined the ore node"
        );
        // ...the mining crew was staffed from the HQ...
        assert!(w.supply_used(1) > 1, "bot should have trained more workers");
        // ...and the surplus built a production structure from nothing.
        assert!(
            barracks_count(&w, 1) >= 1,
            "bot should have constructed a barracks"
        );
    }

    #[test]
    fn fighters_break_buildings() {
        // Attack range is measured to the target's edge, so a unit can kill a
        // building whose center it can never reach (the footprint pushes it
        // out). This was the "units can't damage buildings" bug.
        let mut w = World::new(21);
        w.step(&[
            Command::SpawnBuilding {
                owner: 1,
                kind: BuildingKind::Barracks,
                x: fx(0),
                y: fx(0),
            },
            Command::SpawnUnit {
                owner: 0,
                kind: UnitKind::Infantry,
                x: fx(20),
                y: fx(0),
            },
        ]);
        w.step(&[Command::Attack { unit: 1, target: 0 }]);
        // 500 hp at 5 damage per 12 ticks = 1200 ticks; leave headroom.
        for _ in 0..2000 {
            w.step(&[]);
        }
        assert!(
            !w.snapshot().iter().any(|s| s.kind == Kind::Barracks),
            "infantry should have destroyed the barracks"
        );
    }

    #[test]
    fn workers_attack_only_on_command() {
        let mut w = World::new(23);
        w.step(&[
            Command::SpawnUnit {
                owner: 0,
                kind: UnitKind::Worker,
                x: fx(0),
                y: fx(0),
            },
            Command::SpawnUnit {
                owner: 1,
                kind: UnitKind::Worker,
                x: fx(8),
                y: fx(0),
            },
        ]);
        // No auto-aggro: idle workers near each other never fight.
        for _ in 0..200 {
            w.step(&[]);
        }
        assert_eq!(w.alive_count(), 2);
        // Ordered to attack, the tool arm does real damage.
        w.step(&[Command::Attack { unit: 0, target: 1 }]);
        for _ in 0..600 {
            w.step(&[]);
        }
        assert_eq!(w.alive_count(), 1, "the ordered worker should win");
    }

    #[test]
    fn mobile_units_are_pushed_out_of_buildings() {
        let mut w = World::new(3);
        w.step(&[
            Command::SpawnBuilding {
                owner: 0,
                kind: BuildingKind::Barracks,
                x: fx(0),
                y: fx(0),
            },
            // Spawned right on top of the building's footprint.
            Command::SpawnUnit {
                owner: 0,
                kind: UnitKind::Infantry,
                x: fx(0),
                y: fx(0),
            },
        ]);
        for _ in 0..30 {
            w.step(&[]);
        }
        let inf = w
            .snapshot()
            .into_iter()
            .find(|s| s.kind == Kind::Infantry)
            .expect("infantry exists");
        // Cleared the radius-6 barracks footprint instead of standing inside it.
        assert!(dist2(inf.pos, Fx::ZERO, Fx::ZERO) >= Fx::from_int(36));
    }

    #[test]
    fn player_barracks_trains_only_on_command() {
        let mut w = World::new(7);
        // An HQ for the supply block, plus the barracks under test.
        w.step(&[
            Command::SpawnBuilding {
                owner: 0,
                kind: BuildingKind::Hq,
                x: fx(40),
                y: fx(0),
            },
            Command::SpawnBuilding {
                owner: 0,
                kind: BuildingKind::Barracks,
                x: fx(0),
                y: fx(0),
            },
        ]);
        // The player's barracks must not auto-produce.
        for _ in 0..120 {
            w.step(&[]);
        }
        assert_eq!(w.alive_count(), 2);
        // Queue training; units build over time.
        w.step(&[
            Command::Train {
                building: 1,
                kind: UnitKind::Infantry,
            },
            Command::Train {
                building: 1,
                kind: UnitKind::Infantry,
            },
        ]);
        for _ in 0..200 {
            w.step(&[]);
        }
        assert!(w.alive_count() > 2);
    }

    #[test]
    fn hq_trains_workers_and_only_workers() {
        let mut w = World::new(5);
        w.step(&[Command::SpawnBuilding {
            owner: 0,
            kind: BuildingKind::Hq,
            x: fx(0),
            y: fx(0),
        }]);
        // Fighters are not trainable at the HQ; the command is dropped.
        w.step(&[Command::Train {
            building: 0,
            kind: UnitKind::Infantry,
        }]);
        for _ in 0..200 {
            w.step(&[]);
        }
        assert_eq!(w.alive_count(), 1);
        // Workers are, and they deposit at the HQ once mining.
        w.step(&[Command::Train {
            building: 0,
            kind: UnitKind::Worker,
        }]);
        for _ in 0..200 {
            w.step(&[]);
        }
        let snap = w.snapshot();
        assert!(
            snap.iter().any(|s| s.kind == Kind::Worker),
            "HQ should have produced a worker"
        );
    }

    #[test]
    fn supply_caps_production_until_a_depot_rises() {
        let mut w = World::new(13);
        let mut setup = vec![
            Command::SpawnBuilding {
                owner: 0,
                kind: BuildingKind::Hq,
                x: fx(0),
                y: fx(0),
            },
            Command::SpawnBuilding {
                owner: 0,
                kind: BuildingKind::Barracks,
                x: fx(30),
                y: fx(0),
            },
        ];
        // Fill the HQ's whole supply block (10) with spawned infantry.
        for k in 0..10 {
            setup.push(Command::SpawnUnit {
                owner: 0,
                kind: UnitKind::Infantry,
                x: fx(-30 - 3 * k),
                y: fx(0),
            });
        }
        w.step(&setup);
        assert_eq!(w.supply_used(0), 10);
        assert_eq!(w.supply_cap(0), SUPPLY_PER_HQ);
        // Queue two more: production must hold at the cap.
        w.step(&[
            Command::Train {
                building: 1,
                kind: UnitKind::Infantry,
            },
            Command::Train {
                building: 1,
                kind: UnitKind::Infantry,
            },
        ]);
        for _ in 0..200 {
            w.step(&[]);
        }
        assert_eq!(w.supply_used(0), 10, "production must hold at the cap");
        // A depot raises the cap and releases the held queue.
        w.step(&[Command::SpawnBuilding {
            owner: 0,
            kind: BuildingKind::Supply,
            x: fx(0),
            y: fx(30),
        }]);
        assert_eq!(w.supply_cap(0), SUPPLY_PER_HQ + SUPPLY_PER_DEPOT);
        for _ in 0..200 {
            w.step(&[]);
        }
        assert_eq!(w.supply_used(0), 12, "queued units flow once supply frees");
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
            cmds.push(Command::Train {
                building: 0,
                kind: UnitKind::Infantry,
            });
        }
        w.step(&cmds);
        // Only what we could afford got queued, and ore was spent.
        let snap = w.snapshot();
        let b = snap.iter().find(|s| s.index == 0).unwrap();
        assert_eq!(b.queued, 4); // 200 / 50
        assert!(w.ore(0) < start);
    }
}
