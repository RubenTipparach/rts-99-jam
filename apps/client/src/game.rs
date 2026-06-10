//! Game glue: drives the deterministic sim, holds selection, turns input into
//! commands, computes fog-of-war, and produces render + HUD data. Floats here.

use crate::camera::Camera;
use crate::fx::Fx as FxSystem;
use crate::gfx::{FxLight, InstanceRaw, RingRaw, ANIM_NONE, FOW_RES, RING, ROT_NONE};
use crate::terrain;
use math::{Fx, FRAC_BITS};
use protocol::{BuildingKind, Command, UnitKind};
use sim::{Kind, Snap, World};
use std::collections::HashSet;
use web_time::Instant;

const TICK_HZ: u32 = 20;
const SEED: u64 = 0x5011_D011_0099_0001;

#[inline]
fn f(x: Fx) -> f32 {
    x.to_raw() as f32 / (1u64 << FRAC_BITS) as f32
}
#[inline]
fn fx(v: f32) -> Fx {
    Fx::from_raw((v * (1u64 << FRAC_BITS) as f32) as i64)
}

/// Faction tint applied to the team-colored parts of a mesh (tabards, pauldrons,
/// helmet plumes, banners, flags). Neutral materials ignore it.
fn team_color(owner: u16) -> [f32; 4] {
    if owner == 0 {
        [0.25, 0.55, 1.0, 1.0]
    } else {
        [0.95, 0.30, 0.24, 1.0]
    }
}

/// Instance yaw `(cos, sin)` that points a mesh's authored face (+z: visors,
/// eyes, tool arms) along the entity's sim facing. Falls back to no rotation
/// for a degenerate facing.
fn rot_of(s: &Snap) -> [f32; 2] {
    let (dx, dz) = (f(s.facing.x), f(s.facing.y));
    let len = dx.hypot(dz);
    if len < 1e-4 {
        ROT_NONE
    } else {
        [dz / len, dx / len]
    }
}

/// As [`rot_of`], for meshes authored facing -z (the turret's barrels).
fn rot_of_neg_z(s: &Snap) -> [f32; 2] {
    let [c, sn] = rot_of(s);
    [-c, -sn]
}

/// Squared ground distance between two snapshots (floats; presentation only).
fn dist_f(a: &Snap, b: &Snap) -> f32 {
    let dx = f(a.pos.x) - f(b.pos.x);
    let dz = f(a.pos.y) - f(b.pos.y);
    dx * dx + dz * dz
}

/// Which faction a player fields. Drives which placeholder building/unit meshes
/// are drawn for that player; set from the skirmish lobby. Only the two launch
/// factions exist so far.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum Faction {
    Astromancer,
    Hollowmen,
}

/// Order feedback: what a short-lived ground ping is telling the player.
#[derive(Clone, Copy)]
enum Ping {
    /// Move order: a green blip at the destination.
    Move,
    /// Attack order: a red highlight on the target (or the attack-move point).
    Attack,
    /// Harvest order: a cyan highlight on the resource node.
    Harvest,
}

/// A short-lived order-feedback decal. If `target` is set the ping rides that
/// entity while it lives; otherwise it stays at the ordered point.
struct Effect {
    ping: Ping,
    wx: f32,
    wz: f32,
    target: Option<u32>,
    born: f32,
}

/// Seconds an order ping stays on screen.
const PING_LIFE: f32 = 0.9;

/// Most a building pad may cut or fill: sites where the natural ground
/// deviates more than this from the centre height are too steep to build on.
const MAX_PAD_CUT: f32 = 3.0;

/// Steepest ground slope (height units per world unit) ground units can walk;
/// anything steeper is fed to the sim as impassable, along with water.
const MAX_WALK_SLOPE: f32 = 1.3;

/// Building placement locks to this grid pitch (world units): structures
/// line up into predictable lanes, and the placement preview can show a
/// per-cell validity dot-grid.
const BUILD_GRID: f32 = 4.0;

/// Snap a world coordinate onto the build grid.
fn snap(v: f32) -> f32 {
    (v / BUILD_GRID).round() * BUILD_GRID
}

/// A build order awaiting its worker: a placed hologram holds the site until
/// the structure spawns (or the order quietly dies and the ghost times out).
struct PendingBuild {
    kind: BuildingKind,
    wx: f32,
    wz: f32,
    born: f32,
}

/// Seconds a placed hologram survives without its building appearing.
const PENDING_BUILD_TTL: f32 = 45.0;

/// Rough mesh height per building kind, for the screen-space click box.
fn building_height(kind: Kind) -> f32 {
    match kind {
        Kind::Hq => 17.5,
        Kind::Barracks => 12.0,
        Kind::Turret => 4.5,
        Kind::Supply => 3.5,
        _ => 0.0,
    }
}

/// Levelled-pad radius for a building footprint (matches the selection ring).
fn pad_radius(kind: BuildingKind) -> f32 {
    match kind {
        BuildingKind::Hq => 9.0,
        BuildingKind::Barracks => 8.0,
        BuildingKind::Turret => 4.0,
        BuildingKind::Supply => 4.5,
    }
}

/// Per-mesh instance lists + selection rings, matching `Gfx::render`'s
/// argument order. The crystal/pool groups draw alpha-blended after the
/// opaque world; barrels are the sludge loads hauling workers carry.
#[derive(Default)]
pub struct RenderData {
    pub infantry: Vec<InstanceRaw>,
    pub barracks_astro: Vec<InstanceRaw>,
    pub barracks_hollow: Vec<InstanceRaw>,
    pub hq_astro: Vec<InstanceRaw>,
    pub hq_hollow: Vec<InstanceRaw>,
    pub acolytes: Vec<InstanceRaw>,
    pub engineers: Vec<InstanceRaw>,
    pub ore_nodes: Vec<InstanceRaw>,
    pub carbon_nodes: Vec<InstanceRaw>,
    pub heavies: Vec<InstanceRaw>,
    pub turrets: Vec<InstanceRaw>,
    pub supplies: Vec<InstanceRaw>,
    pub barrels: Vec<InstanceRaw>,
    pub ore_crystals: Vec<InstanceRaw>,
    pub carbon_pools: Vec<InstanceRaw>,
    pub rings: Vec<RingRaw>,
}

#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct UnitInfo {
    pub owner: u16,
    pub barracks: bool,
    /// Entity kind (drives the HUD's portrait icon).
    pub kind: Kind,
    pub wx: f32,
    pub wy: f32,
    pub wz: f32,
    pub hp_frac: f32,
    /// Buildings: world footprint radius (drives the HUD corner brackets).
    pub radius: f32,
    /// Display name (faction-flavored for buildings).
    pub name: &'static str,
    /// Current and maximum hit points, for the numeric readout.
    pub hp: i32,
    pub hp_max: i32,
}

pub struct Game {
    world: World,
    tick_dt: f32,
    acc: f32,
    last: Instant,
    time: f32,
    prev: Vec<Snap>,
    curr: Vec<Snap>,
    selected: Vec<u32>,
    pending: Vec<Command>,
    visible: Vec<bool>,
    explored: Vec<bool>,
    /// Debug toggles: darken unexplored / explored areas (both on by default).
    fog_unexplored: bool,
    fog_explored: bool,
    /// Faction per side: index 0 = the player (owner 0), index 1 = everyone
    /// else. Defaults to Hollowmen vs Astromancers; the lobby overrides it.
    factions: [Faction; 2],
    /// Live order-feedback pings (move blips, attack/harvest highlights).
    effects: Vec<Effect>,
    /// The building pads changed this tick; the ground mesh must rebuild.
    terrain_dirty: bool,
    /// Particle/light effects (muzzle flashes, blood, mining sparks, ...).
    fx: FxSystem,
    /// Build orders awaiting their worker (drawn as placed holograms).
    pending_builds: Vec<PendingBuild>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        // The whole starting layout (bases, garrisons, and the resource
        // clusters) is baked into the active battlefield's map file: each
        // voxel world carries the skirmish template fitted onto its own
        // viable ground, so construct the Game AFTER the lobby picks a map.
        let map = crate::map::parse(crate::map::active_scenario()).expect("baked map is invalid");
        log::info!("loading map: {}", map.name);
        let setup = map.commands;

        let mut g = Game {
            world: World::new(SEED),
            tick_dt: 1.0 / TICK_HZ as f32,
            acc: 0.0,
            last: Instant::now(),
            time: 0.0,
            prev: Vec::new(),
            curr: Vec::new(),
            selected: Vec::new(),
            pending: setup,
            visible: vec![false; FOW_RES * FOW_RES],
            explored: vec![false; FOW_RES * FOW_RES],
            fog_unexplored: true,
            fog_explored: true,
            factions: [Faction::Hollowmen, Faction::Astromancer],
            effects: Vec::new(),
            terrain_dirty: false,
            fx: FxSystem::default(),
            pending_builds: Vec::new(),
        };
        // The enemy is driven by the in-sim bot commander (mines, builds, trains).
        g.world.set_bot(1, true);
        g.apply_terrain();
        g.step_now();
        g.prev = g.curr.clone();
        g.recompute_fow();
        g
    }

    fn step_now(&mut self) {
        let cmds = std::mem::take(&mut self.pending);
        self.prev = self.curr.clone();
        self.world.step(&cmds);
        self.curr = self.world.snapshot();
        // Keep any of the player's still-living entities selected (units AND
        // buildings) - dropping buildings here deselected them every tick.
        let live: HashSet<u32> = self
            .curr
            .iter()
            .filter(|s| s.owner == 0)
            .map(|s| s.index)
            .collect();
        self.selected.retain(|i| live.contains(i));
        self.sync_pads();
        self.emit_fx();
        // A placed hologram retires the moment its building exists on site
        // (or after a timeout, for orders that quietly died en route).
        let now = self.time;
        self.pending_builds.retain(|pb| {
            let built = self.curr.iter().any(|s| {
                s.owner == 0
                    && matches!(
                        s.kind,
                        Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
                    )
                    && (f(s.pos.x) - pb.wx).hypot(f(s.pos.y) - pb.wz) < 5.0
            });
            !built && now - pb.born < PENDING_BUILD_TTL
        });
    }

    /// Feed the sim the active battlefield's passability: water and steep
    /// cliffs are impassable to ground units. Call at match start and whenever
    /// the lobby swaps the world. (Derived client-side from the baked map; for
    /// networked play this mask should be baked, but every command here is
    /// local today.)
    pub fn apply_terrain(&mut self) {
        const N: usize = 128;
        let cell = 2.0 * terrain::HALF / N as f32;
        let mut cells = vec![true; N * N];
        for k in 0..N {
            let wz = -terrain::HALF + (k as f32 + 0.5) * cell;
            for i in 0..N {
                let wx = -terrain::HALF + (i as f32 + 0.5) * cell;
                cells[k * N + i] = !Self::terrain_blocked(wx, wz);
            }
        }
        self.world
            .set_passability(N, Fx::from_int(terrain::HALF as i32), cells);
    }

    /// Water or too-steep ground (shared by the sim grid and site checks).
    fn terrain_blocked(wx: f32, wz: f32) -> bool {
        if terrain::submerged(wx, wz) {
            return true;
        }
        let s = 4.0;
        let gx = (terrain::natural_height(wx + s, wz) - terrain::natural_height(wx - s, wz)).abs();
        let gz = (terrain::natural_height(wx, wz + s) - terrain::natural_height(wx, wz - s)).abs();
        gx.max(gz) / (2.0 * s) > MAX_WALK_SLOPE
    }

    /// The match outcome once decided: `Some(true)` = victory (every rival
    /// base razed), `Some(false)` = defeat (the player's base is gone).
    pub fn outcome(&self) -> Option<bool> {
        if self.world.eliminated(0) {
            return Some(false);
        }
        self.world.last_standing(0).then_some(true)
    }

    /// Turn this tick's sim events into particles and lights: shots become
    /// muzzle flashes + tracers, HP drops bleed or spark, deaths explode, and
    /// mining workers chip glowing flecks off the node. Effects in unseen fog
    /// are skipped.
    fn emit_fx(&mut self) {
        let at = |s: &Snap| {
            let (wx, wz) = (f(s.pos.x), f(s.pos.y));
            [wx, terrain::height(wx, wz), wz]
        };
        let find = |list: &[Snap], i: u32| list.iter().find(|s| s.index == i).copied();
        // Shots: need both ends; the target may have just died, so fall back
        // to its last known (prev) position.
        let shots: Vec<([f32; 3], [f32; 3], bool)> = self
            .world
            .shots()
            .iter()
            .filter_map(|&(a, t)| {
                let from = find(&self.curr, a).or_else(|| find(&self.prev, a))?;
                let to = find(&self.curr, t).or_else(|| find(&self.prev, t))?;
                let seen = from.owner == 0
                    || to.owner == 0
                    || self.cell_visible(f(from.pos.x), f(from.pos.y));
                Some((at(&from), at(&to), seen))
            })
            .collect();
        for (from, to, seen) in shots {
            if seen {
                self.fx.shot(from, to);
            }
        }
        // HP drops and deaths, diffed against the previous snapshot.
        let events: Vec<([f32; 3], bool, bool, bool)> = self
            .prev
            .iter()
            .filter(|p| !matches!(p.kind, Kind::OreNode | Kind::CarbonNode))
            .filter_map(|p| {
                let organic = matches!(p.kind, Kind::Infantry | Kind::Worker);
                let big = matches!(
                    p.kind,
                    Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
                );
                let seen = p.owner == 0 || self.cell_visible(f(p.pos.x), f(p.pos.y));
                if !seen {
                    return None;
                }
                match find(&self.curr, p.index) {
                    Some(c) if c.generation == p.generation => {
                        (c.hp < p.hp).then(|| (at(p), organic, big, false))
                    }
                    _ => Some((at(p), organic, big, true)),
                }
            })
            .collect();
        for (pos, organic, big, died) in events {
            if died {
                self.fx.explosion(pos, big);
            } else {
                self.fx.hit(pos, organic);
            }
        }
        // Structures being raised shower welding sparks.
        let welds: Vec<([f32; 3], f32)> = self
            .curr
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
                ) && s.construct_frac < Fx::ONE
            })
            .filter(|s| s.owner == 0 || self.cell_visible(f(s.pos.x), f(s.pos.y)))
            .map(|s| {
                let r = match s.kind {
                    Kind::Hq => 6.0,
                    Kind::Barracks => 5.0,
                    _ => 2.5,
                };
                (at(s), r)
            })
            .collect();
        for (pos, spread) in welds {
            self.fx.weld(pos, spread);
        }

        // Damaged buildings burn: smoke from light damage, flames and a
        // flickering glow once they're badly hurt.
        let fires: Vec<([f32; 3], f32, f32)> = self
            .curr
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
                )
            })
            .filter(|s| s.owner == 0 || self.cell_visible(f(s.pos.x), f(s.pos.y)))
            .filter_map(|s| {
                let frac = (f(s.hp) / f(s.max_hp)).clamp(0.0, 1.0);
                let r = match s.kind {
                    Kind::Hq => 6.0,
                    Kind::Barracks => 5.0,
                    _ => 2.5,
                };
                (frac < 0.65).then(|| (at(s), 1.0 - frac, r))
            })
            .collect();
        for (pos, severity, spread) in fires {
            self.fx.fire(pos, severity, spread);
        }

        // Mining workers fire a cutting laser at their node: beam from the
        // tool to the contact point on the crystal pile's core sphere
        // (radius ~2.2 around the node center), sparks where it bites.
        let beams: Vec<([f32; 3], [f32; 3], bool)> = self
            .curr
            .iter()
            .filter(|s| s.mining)
            .filter(|s| s.owner == 0 || self.cell_visible(f(s.pos.x), f(s.pos.y)))
            .filter_map(|s| {
                let node = self
                    .curr
                    .iter()
                    .filter(|n| matches!(n.kind, Kind::OreNode | Kind::CarbonNode))
                    .min_by(|a, b| {
                        let da = dist_f(s, a);
                        let db = dist_f(s, b);
                        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                    })?;
                let (wx, wz) = (f(s.pos.x), f(s.pos.y));
                let (nx, nz) = (f(node.pos.x), f(node.pos.y));
                let (dx, dz) = (wx - nx, wz - nz);
                let l = dx.hypot(dz).max(0.001);
                let contact = [
                    nx + dx / l * 2.2,
                    terrain::height(nx, nz) + 2.0,
                    nz + dz / l * 2.2,
                ];
                let from = [wx, terrain::height(wx, wz) + 1.4, wz];
                Some((from, contact, node.kind == Kind::CarbonNode))
            })
            .collect();
        for (from, to, carbon) in beams {
            self.fx.mining_beam(from, to, carbon);
        }

        // Carbon geysers vent: every visible gas node breathes a wisp of
        // green smoke from its crater (jittered cadence, fog-gated).
        let vents: Vec<[f32; 3]> = self
            .curr
            .iter()
            .filter(|s| s.kind == Kind::CarbonNode)
            .filter(|s| self.cell_visible(f(s.pos.x), f(s.pos.y)))
            .map(|s| {
                let (wx, wz) = (f(s.pos.x), f(s.pos.y));
                [wx, terrain::height(wx, wz) + 3.2, wz]
            })
            .collect();
        for v in vents {
            self.fx.geyser_smoke(v);
        }
    }

    /// Emissive particle instances for the renderer.
    pub fn fx_instances(&self) -> Vec<InstanceRaw> {
        self.fx.instances()
    }

    /// Active fx point lights for the renderer.
    pub fn fx_lights(&self) -> Vec<FxLight> {
        self.fx.lights()
    }

    /// Steady world lights: finished buildings switch on warm floodlights
    /// that pool on the pad beneath them (Earth 2150 style), ore crystals
    /// glow cool blue, and gas nodes glow green. Sorted nearest-first around
    /// `(fx_, fz_)` (the camera focus) so slot overflow drops far lights.
    pub fn world_lights(&self, fx_: f32, fz_: f32) -> Vec<FxLight> {
        let mut out = Vec::new();
        for s in &self.curr {
            let (wx, wz) = self.lerped(s);
            if !self.revealed(s, wx, wz) {
                continue;
            }
            let ground = terrain::height(wx, wz);
            match s.kind {
                // Floodlights only come on once construction is finished:
                // wall lamps just outside the footprint corners, hugging the
                // ground so the pool lands on the terrain vertices around
                // the pad (Earth 2150 style), never on the building's roof.
                Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
                    if f(s.construct_frac) >= 0.999 =>
                {
                    let r = match s.kind {
                        Kind::Hq => 7.0,
                        Kind::Barracks => 6.0,
                        Kind::Supply => 3.0,
                        _ => 2.5,
                    };
                    // Big structures light all four corners; small ones two.
                    let corners: &[(f32, f32)] = if r > 4.0 {
                        &[(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)]
                    } else {
                        &[(1.0, 1.0), (-1.0, -1.0)]
                    };
                    for (sx, sz) in corners {
                        let (lx, lz) = (wx + sx * (r + 1.0), wz + sz * (r + 1.0));
                        out.push(FxLight {
                            pos: [lx, terrain::height(lx, lz) + 1.6, lz],
                            radius: r * 2.4 + 4.0,
                            color: [1.3, 1.0, 0.55],
                        });
                    }
                }
                // Node glow pulses in step with the crystal shader (same
                // world-position phase), dimming as the node depletes.
                Kind::OreNode | Kind::CarbonNode => {
                    let pulse = 0.75 + 0.25 * (self.time * 2.4 + wx * 0.13 + wz * 0.17).sin();
                    let glow = (0.5 + 0.7 * f(s.resource_frac)) * pulse;
                    let color = if s.kind == Kind::OreNode {
                        [0.30 * glow, 0.62 * glow, 1.05 * glow]
                    } else {
                        [0.28 * glow, 1.0 * glow, 0.45 * glow]
                    };
                    out.push(FxLight {
                        pos: [wx, ground + 2.5, wz],
                        radius: 11.0,
                        color,
                    });
                }
                _ => {}
            }
        }
        out.sort_by(|a, b| {
            let d = |l: &FxLight| {
                let (dx, dz) = (l.pos[0] - fx_, l.pos[2] - fz_);
                dx * dx + dz * dz
            };
            d(a).total_cmp(&d(b))
        });
        out
    }

    /// Level a terrain pad under every building so structures sit on flat
    /// ground (presentation only; the sim has no heights). When the set
    /// changes, flag the ground mesh for a rebuild.
    fn sync_pads(&mut self) {
        let mut pads = Vec::new();
        for s in &self.curr {
            let r = match s.kind {
                Kind::Hq => 9.0,
                Kind::Barracks => 8.0,
                Kind::Turret => 4.0,
                Kind::Supply => 4.5,
                _ => continue,
            };
            let (wx, wz) = (f(s.pos.x), f(s.pos.y));
            pads.push(terrain::Pad {
                x: wx,
                z: wz,
                r,
                h: terrain::natural_height(wx, wz),
            });
        }
        if terrain::set_pads(pads) {
            self.terrain_dirty = true;
        }
    }

    /// True once after the building pads changed (the caller rebuilds the
    /// ground mesh).
    pub fn take_terrain_dirty(&mut self) -> bool {
        std::mem::take(&mut self.terrain_dirty)
    }

    pub fn update(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1);
        self.last = now;
        self.time += dt;
        self.acc += dt;
        while self.acc >= self.tick_dt {
            self.step_now();
            self.acc -= self.tick_dt;
        }
        let now = self.time;
        self.effects.retain(|e| now - e.born < PING_LIFE);
        self.fx.update(dt);
    }

    /// Keep wall-clock bookkeeping current without stepping the sim, used while
    /// the game is paused so resuming does not fast-forward a backlog of ticks.
    pub fn skip_tick(&mut self) {
        self.last = Instant::now();
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    fn alpha(&self) -> f32 {
        (self.acc / self.tick_dt).clamp(0.0, 1.0)
    }

    fn prev_xz(&self, index: u32) -> Option<(f32, f32)> {
        self.prev
            .iter()
            .find(|s| s.index == index)
            .map(|s| (f(s.pos.x), f(s.pos.y)))
    }

    fn lerped(&self, s: &Snap) -> (f32, f32) {
        let (cx, cz) = (f(s.pos.x), f(s.pos.y));
        match self.prev_xz(s.index) {
            Some((px, pz)) => {
                let a = self.alpha();
                (px + (cx - px) * a, pz + (cz - pz) * a)
            }
            None => (cx, cz),
        }
    }

    // ---- fog of war ----

    fn reveal(&mut self, wx: f32, wz: f32, rad: f32) {
        let res = FOW_RES as i32;
        let half = terrain::HALF;
        let to_cell = |w: f32| (w + half) / (2.0 * half) * res as f32;
        let (cx, cz) = (to_cell(wx), to_cell(wz));
        let cr = rad / (2.0 * half) * res as f32;
        let x0 = (cx - cr).floor().max(0.0) as i32;
        let x1 = (cx + cr).ceil().min(res as f32 - 1.0) as i32;
        let z0 = (cz - cr).floor().max(0.0) as i32;
        let z1 = (cz + cr).ceil().min(res as f32 - 1.0) as i32;
        for zc in z0..=z1 {
            for xc in x0..=x1 {
                let dx = xc as f32 + 0.5 - cx;
                let dz = zc as f32 + 0.5 - cz;
                if dx * dx + dz * dz <= cr * cr {
                    let i = (zc * res + xc) as usize;
                    self.visible[i] = true;
                    self.explored[i] = true;
                }
            }
        }
    }

    pub fn toggle_fog_unexplored(&mut self) {
        self.fog_unexplored = !self.fog_unexplored;
    }
    pub fn toggle_fog_explored(&mut self) {
        self.fog_explored = !self.fog_explored;
    }
    pub fn recompute_fow(&mut self) {
        for v in self.visible.iter_mut() {
            *v = false;
        }
        let entities: Vec<(f32, f32, f32)> = self
            .curr
            .iter()
            .filter(|s| s.owner == 0)
            .map(|s| {
                let (wx, wz) = self.lerped(s);
                let r = if matches!(s.kind, Kind::Hq | Kind::Barracks) {
                    62.5
                } else {
                    45.0
                };
                (wx, wz, r)
            })
            .collect();
        for (wx, wz, r) in entities {
            self.reveal(wx, wz, r);
        }
    }

    /// Fog-of-war as an R8 field for the terrain shader: visible = bright,
    /// explored = dim, unexplored = dark. A light separable blur feathers the
    /// borders so the fog fades in smoothly rather than stepping per cell.
    /// (Units don't read this - they're shown/hidden outright via `revealed`.)
    pub fn fow_bytes(&self) -> Vec<u8> {
        let n = FOW_RES;
        let explored_v = if self.fog_explored { 0.45 } else { 1.0 };
        let unexplored_v = if self.fog_unexplored { 0.0 } else { 1.0 };
        let mut field = vec![0f32; n * n];
        for (i, v) in field.iter_mut().enumerate() {
            *v = if self.visible[i] {
                1.0
            } else if self.explored[i] {
                explored_v
            } else {
                unexplored_v
            };
        }
        let r = 2i32;
        let blur = |src: &[f32], horizontal: bool| {
            let mut out = vec![0f32; n * n];
            for y in 0..n as i32 {
                for x in 0..n as i32 {
                    let (mut sum, mut cnt) = (0.0f32, 0.0f32);
                    for k in -r..=r {
                        let (sx, sy) = if horizontal { (x + k, y) } else { (x, y + k) };
                        if sx >= 0 && sy >= 0 && sx < n as i32 && sy < n as i32 {
                            sum += src[(sy * n as i32 + sx) as usize];
                            cnt += 1.0;
                        }
                    }
                    out[(y * n as i32 + x) as usize] = sum / cnt;
                }
            }
            out
        };
        let blurred = blur(&blur(&field, true), false);
        blurred.iter().map(|v| (v * 255.0) as u8).collect()
    }

    /// Fog brightness at normalized map coords (0..1) for the minimap: visible
    /// bright, explored dim, unexplored dark (respecting the debug toggles).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn fog_brightness(&self, nx: f32, nz: f32) -> f32 {
        let res = FOW_RES as i32;
        let xc = (nx * res as f32) as i32;
        let zc = (nz * res as f32) as i32;
        if xc < 0 || zc < 0 || xc >= res || zc >= res {
            return if self.fog_unexplored { 0.12 } else { 1.0 };
        }
        let i = (zc * res + xc) as usize;
        if self.visible[i] {
            1.0
        } else if self.explored[i] {
            if self.fog_explored {
                0.5
            } else {
                1.0
            }
        } else if self.fog_unexplored {
            0.12
        } else {
            1.0
        }
    }

    fn cell_visible(&self, wx: f32, wz: f32) -> bool {
        let res = FOW_RES as i32;
        let half = terrain::HALF;
        let xc = ((wx + half) / (2.0 * half) * res as f32) as i32;
        let zc = ((wz + half) / (2.0 * half) * res as f32) as i32;
        if xc < 0 || zc < 0 || xc >= res || zc >= res {
            return false;
        }
        self.visible[(zc * res + xc) as usize]
    }

    fn cell_explored(&self, wx: f32, wz: f32) -> bool {
        let res = FOW_RES as i32;
        let half = terrain::HALF;
        let xc = ((wx + half) / (2.0 * half) * res as f32) as i32;
        let zc = ((wz + half) / (2.0 * half) * res as f32) as i32;
        if xc < 0 || zc < 0 || xc >= res || zc >= res {
            return false;
        }
        self.explored[(zc * res + xc) as usize]
    }

    /// Entity is shown if it's ours, or an enemy currently in vision. Resource
    /// nodes are terrain: once explored they stay on screen (StarCraft rule -
    /// once you know it's there, it isn't going anywhere).
    fn revealed(&self, s: &Snap, wx: f32, wz: f32) -> bool {
        if matches!(s.kind, Kind::OreNode | Kind::CarbonNode) {
            return self.cell_explored(wx, wz);
        }
        s.owner == 0 || self.cell_visible(wx, wz)
    }

    // ---- render + HUD data ----

    /// Which faction the given owner fields (owner 0 = the player).
    pub fn faction_of(&self, owner: u16) -> Faction {
        self.factions[(owner != 0) as usize]
    }

    /// Set the player's faction (owner 0); the enemy takes the other launch
    /// faction. Called from the skirmish lobby before the match starts.
    #[allow(dead_code)] // wired up by the skirmish lobby (next)
    pub fn set_player_faction(&mut self, faction: Faction) {
        self.factions[0] = faction;
        self.factions[1] = match faction {
            Faction::Astromancer => Faction::Hollowmen,
            Faction::Hollowmen => Faction::Astromancer,
        };
    }

    /// Instances for each mesh - infantry, the two faction barracks (Astromancer,
    /// Hollowmen), the two faction HQs (Spire, Command HQ), the two faction
    /// workers (Acolyte, Engineer), the two resource nodes (ore, carbon) - plus
    /// selection rings and order pings. Meshes are authored at world scale, so
    /// instance scale is ~1 (nodes shrink with depletion).
    ///
    /// `ghost` is the build-placement preview: the pending building kind and
    /// the cursor's ground point. It draws as a holographic mesh (green when
    /// the site is clear, red when blocked) plus a footprint ring.
    #[allow(clippy::type_complexity)]
    pub fn render_data(&self, ghost: Option<(BuildingKind, f32, f32)>) -> RenderData {
        let sel: HashSet<u32> = self.selected.iter().copied().collect();
        let mut infantry = Vec::new();
        let mut barracks_astro = Vec::new();
        let mut barracks_hollow = Vec::new();
        let mut hq_astro = Vec::new();
        let mut hq_hollow = Vec::new();
        let mut acolytes = Vec::new();
        let mut engineers = Vec::new();
        let mut ore_nodes = Vec::new();
        let mut carbon_nodes = Vec::new();
        let mut heavies = Vec::new();
        let mut turrets = Vec::new();
        let mut supplies = Vec::new();
        let mut barrels = Vec::new();
        let mut ore_crystals = Vec::new();
        let mut carbon_pools = Vec::new();
        let mut rings = Vec::new();
        for s in &self.curr {
            let (wx, wz) = self.lerped(s);
            if !self.revealed(s, wx, wz) {
                continue;
            }
            let ground = terrain::height(wx, wz);
            let tint = team_color(s.owner);
            if matches!(s.kind, Kind::OreNode | Kind::CarbonNode) {
                // Nodes shrink as they deplete (resource_frac runs 1 -> 0).
                // The rock pedestal draws opaque; the crystal shards / gas
                // pool ride the same transform through the blended pipeline.
                let scl = 0.55 + 0.45 * f(s.resource_frac);
                let inst = InstanceRaw {
                    offset: [wx, ground, wz],
                    scale: [scl, scl, scl],
                    color: [1.0, 1.0, 1.0, 0.0],
                    rot: ROT_NONE,
                    anim: ANIM_NONE,
                };
                if s.kind == Kind::OreNode {
                    ore_nodes.push(inst);
                    ore_crystals.push(inst);
                } else {
                    carbon_nodes.push(inst);
                    carbon_pools.push(inst);
                }
            } else if matches!(
                s.kind,
                Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
            ) {
                // Buildings rise out of the ground as they are constructed
                // (construct_frac 0 -> 1); a finished one is at full height.
                let cf = f(s.construct_frac).clamp(0.08, 1.0);
                let selected = sel.contains(&s.index);
                // Selected buildings brighten (tint alpha 1.5 is the shader's
                // highlight flag) on top of their ground ring.
                let mut color = tint;
                if selected {
                    color[3] = 1.5;
                }
                let inst = InstanceRaw {
                    offset: [wx, ground, wz],
                    scale: [1.0, cf, 1.0],
                    color,
                    // Turrets swivel toward their target; other buildings sit.
                    rot: if s.kind == Kind::Turret {
                        rot_of_neg_z(s)
                    } else {
                        ROT_NONE
                    },
                    anim: ANIM_NONE,
                };
                // The Earth 2150-style floodlight pooling under the building
                // is a real point light now: see `world_lights`.
                match s.kind {
                    Kind::Turret => turrets.push(inst),
                    Kind::Supply => supplies.push(inst),
                    Kind::Hq => match self.faction_of(s.owner) {
                        Faction::Astromancer => hq_astro.push(inst),
                        Faction::Hollowmen => hq_hollow.push(inst),
                    },
                    _ => match self.faction_of(s.owner) {
                        Faction::Astromancer => barracks_astro.push(inst),
                        Faction::Hollowmen => barracks_hollow.push(inst),
                    },
                }
                if selected {
                    // Selection reads via the shader brighten + the HUD's
                    // corner brackets (a ground ring this large clips slopes).
                    // A selected production building shows its rally point as
                    // an amber marker (right-click moves it).
                    if s.owner == 0 && matches!(s.kind, Kind::Hq | Kind::Barracks) {
                        let (rx, rz) = (f(s.rally.x), f(s.rally.y));
                        rings.push(RingRaw {
                            center: [rx, terrain::height(rx, rz), rz],
                            radius: 1.7,
                            color: [1.0, 0.78, 0.3, 0.9],
                            inner: RING,
                        });
                    }
                }
            } else if s.kind == Kind::Worker {
                // Workers animate per faction. The Acolyte hovers with a slow
                // bob; the Engineer plants and bobs while walking. While mining,
                // both get a faster work bob to read as "gathering".
                let phase = s.index as f32 * 1.3;
                let work = if s.mining {
                    (self.time * 14.0 + phase).sin().abs()
                } else {
                    0.0
                };
                let inst = match self.faction_of(s.owner) {
                    Faction::Astromancer => {
                        let hover = if s.mining { 0.5 } else { 1.1 }; // dips to gather
                        let y = ground + hover + ((self.time * 2.2) + phase).sin() * 0.18;
                        InstanceRaw {
                            offset: [wx, y, wz],
                            scale: [1.0, 1.0, 1.0],
                            color: tint,
                            rot: rot_of(s),
                            // Hovers: no legs to swing.
                            anim: ANIM_NONE,
                        }
                    }
                    Faction::Hollowmen => {
                        let mut y = ground;
                        if s.moving && !s.mining {
                            y += ((self.time * 9.0) + phase).sin().abs() * 0.12;
                        }
                        y += work * 0.10; // drilling bob
                        InstanceRaw {
                            offset: [wx, y, wz],
                            scale: [1.0, 1.0, 1.0],
                            color: tint,
                            rot: rot_of(s),
                            anim: if s.moving && !s.mining {
                                [self.time * 9.0 + phase, 0.30]
                            } else {
                                ANIM_NONE
                            },
                        }
                    }
                };
                match self.faction_of(s.owner) {
                    Faction::Astromancer => acolytes.push(inst),
                    Faction::Hollowmen => engineers.push(inst),
                }
                // The hauled load rides visibly on the worker: a little
                // crystal (ore) or a barrel of green sludge (carbon), held
                // out front along the facing direction.
                if s.carry > 0 {
                    let fl = (f(s.facing.x).powi(2) + f(s.facing.y).powi(2))
                        .sqrt()
                        .max(0.001);
                    let (dx, dz) = (f(s.facing.x) / fl, f(s.facing.y) / fl);
                    let cargo = [
                        inst.offset[0] + dx * 0.9,
                        inst.offset[1] + 1.15,
                        inst.offset[2] + dz * 0.9,
                    ];
                    if s.carry == 1 {
                        ore_crystals.push(InstanceRaw {
                            offset: cargo,
                            scale: [0.16, 0.16, 0.16],
                            color: [1.0, 1.0, 1.0, 0.0],
                            rot: inst.rot,
                            anim: ANIM_NONE,
                        });
                    } else {
                        barrels.push(InstanceRaw {
                            offset: cargo,
                            scale: [1.0, 1.0, 1.0],
                            color: [1.0, 1.0, 1.0, 0.0],
                            rot: inst.rot,
                            anim: ANIM_NONE,
                        });
                    }
                }
                if sel.contains(&s.index) {
                    rings.push(RingRaw {
                        center: [wx, ground, wz],
                        radius: 2.2,
                        color: [0.4, 1.0, 0.5, 0.95],
                        inner: RING,
                    });
                }
            } else {
                // Infantry or Heavy: a combat unit with a march bob. Heavy reads
                // larger and gets a bigger selection ring.
                let heavy = s.kind == Kind::Heavy;
                let v = (s.index.wrapping_mul(2_654_435_761) % 1000) as f32 / 1000.0;
                let scl = if heavy { 1.55 } else { 0.92 + v * 0.16 };
                let mut y = ground;
                if s.moving {
                    let amp = if heavy { 0.08 } else { 0.12 };
                    let rate = if heavy { 6.0 } else { 9.0 };
                    y += ((self.time * rate) + s.index as f32 * 1.3).sin() * amp;
                }
                let inst = InstanceRaw {
                    offset: [wx, y, wz],
                    scale: [scl, scl, scl],
                    color: tint,
                    rot: rot_of(s),
                    anim: if s.moving {
                        if heavy {
                            [self.time * 5.5 + s.index as f32 * 1.3, 0.5]
                        } else {
                            [self.time * 9.0 + s.index as f32 * 1.3, 0.32]
                        }
                    } else {
                        ANIM_NONE
                    },
                };
                if heavy {
                    heavies.push(inst);
                } else {
                    infantry.push(inst);
                }
                if sel.contains(&s.index) {
                    rings.push(RingRaw {
                        center: [wx, ground, wz],
                        radius: if heavy { 3.0 } else { 2.2 },
                        color: [0.4, 1.0, 0.5, 0.95],
                        inner: RING,
                    });
                }
            }
        }
        // Build-placement hologram: the pending building at the cursor
        // (snapped to the build grid), tinted by whether the site is clear,
        // with a footprint ring (StarCraft-style) and a validity dot-grid
        // showing at a glance where placement is legal nearby.
        if let Some((kind, cx, cz)) = ghost {
            let (gx, gz) = (snap(cx), snap(cz));
            let ok = self.site_ok(kind, gx, gz);
            // Alpha >= 2.0 flags the hologram path in the unit shader.
            let holo = if ok {
                [0.30, 1.0, 0.55, 2.0]
            } else {
                [1.0, 0.30, 0.25, 2.0]
            };
            let ground = terrain::height(gx, gz);
            let inst = InstanceRaw {
                offset: [gx, ground, gz],
                scale: [1.0, 1.0, 1.0],
                color: holo,
                rot: ROT_NONE,
                anim: ANIM_NONE,
            };
            let radius = match kind {
                BuildingKind::Hq => 9.0,
                BuildingKind::Barracks => 8.0,
                BuildingKind::Turret => 4.0,
                BuildingKind::Supply => 4.5,
            };
            match kind {
                BuildingKind::Hq => match self.faction_of(0) {
                    Faction::Astromancer => hq_astro.push(inst),
                    Faction::Hollowmen => hq_hollow.push(inst),
                },
                BuildingKind::Barracks => match self.faction_of(0) {
                    Faction::Astromancer => barracks_astro.push(inst),
                    Faction::Hollowmen => barracks_hollow.push(inst),
                },
                BuildingKind::Turret => turrets.push(inst),
                BuildingKind::Supply => supplies.push(inst),
            }
            rings.push(RingRaw {
                center: [gx, ground, gz],
                radius,
                color: [holo[0], holo[1], holo[2], 0.85],
                inner: RING,
            });
            // Validity dots: one per build-grid cell around the cursor,
            // green where this building could go, red where it can't.
            const DOTS: i32 = 5; // cells each side of the cursor
            for dk in -DOTS..=DOTS {
                for di in -DOTS..=DOTS {
                    let (px, pz) = (gx + di as f32 * BUILD_GRID, gz + dk as f32 * BUILD_GRID);
                    let col = if self.site_ok(kind, px, pz) {
                        [0.30, 1.0, 0.50, 0.55]
                    } else {
                        [1.0, 0.28, 0.22, 0.45]
                    };
                    rings.push(RingRaw {
                        center: [px, terrain::height(px, pz), pz],
                        radius: 0.45,
                        color: col,
                        inner: 0.0,
                    });
                }
            }
        }

        // Placed holograms: every pending build order keeps a denser ghost on
        // its site (tint alpha 2.4 = the shader's "placed" dither) until the
        // worker raises the real structure.
        for pb in &self.pending_builds {
            let ground = terrain::height(pb.wx, pb.wz);
            let inst = InstanceRaw {
                offset: [pb.wx, ground, pb.wz],
                scale: [1.0, 1.0, 1.0],
                color: [0.30, 1.0, 0.55, 2.4],
                rot: ROT_NONE,
                anim: ANIM_NONE,
            };
            match pb.kind {
                BuildingKind::Hq => match self.faction_of(0) {
                    Faction::Astromancer => hq_astro.push(inst),
                    Faction::Hollowmen => hq_hollow.push(inst),
                },
                BuildingKind::Barracks => match self.faction_of(0) {
                    Faction::Astromancer => barracks_astro.push(inst),
                    Faction::Hollowmen => barracks_hollow.push(inst),
                },
                BuildingKind::Turret => turrets.push(inst),
                BuildingKind::Supply => supplies.push(inst),
            }
        }

        // Order pings: short-lived feedback decals. A ping with a target rides
        // the (living) target entity; the rest fade in place.
        for e in &self.effects {
            let age = ((self.time - e.born) / PING_LIFE).clamp(0.0, 1.0);
            let (mut wx, mut wz) = (e.wx, e.wz);
            if let Some(t) = e.target {
                if let Some(s) = self.curr.iter().find(|s| s.index == t) {
                    (wx, wz) = self.lerped(s);
                }
            }
            let fade = 1.0 - age;
            let (radius, color) = match e.ping {
                Ping::Move => (0.5 + 2.4 * fade, [0.35, 1.0, 0.45, 0.9 * fade]),
                Ping::Attack => (4.6 - 1.8 * age, [1.0, 0.25, 0.2, 0.95 * fade]),
                Ping::Harvest => (4.6 - 1.8 * age, [0.4, 0.95, 1.0, 0.9 * fade]),
            };
            rings.push(RingRaw {
                center: [wx, terrain::height(wx, wz), wz],
                radius,
                color,
                inner: RING,
            });
        }

        RenderData {
            infantry,
            barracks_astro,
            barracks_hollow,
            hq_astro,
            hq_hollow,
            acolytes,
            engineers,
            ore_nodes,
            carbon_nodes,
            heavies,
            turrets,
            supplies,
            barrels,
            ore_crystals,
            carbon_pools,
            rings,
        }
    }

    /// True if a building of `kind` can be placed at `(wx, wz)`: far enough
    /// from every existing building and resource node (mirrors the sim's
    /// `BUILD_CLEAR2`), and on ground gentle enough to level - the pad can
    /// only cut or fill [`MAX_PAD_CUT`] across the footprint.
    fn site_ok(&self, kind: BuildingKind, wx: f32, wz: f32) -> bool {
        for s in &self.curr {
            let solid = matches!(
                s.kind,
                Kind::Hq
                    | Kind::Barracks
                    | Kind::Turret
                    | Kind::Supply
                    | Kind::OreNode
                    | Kind::CarbonNode
            );
            if !solid {
                continue;
            }
            let (ex, ez) = (f(s.pos.x), f(s.pos.y));
            if (ex - wx).hypot(ez - wz) < 14.0 {
                return false;
            }
        }
        // Never on water, never on top of mobile units, and clear of any
        // already-placed hologram.
        let r = pad_radius(kind);
        if terrain::submerged(wx, wz) {
            return false;
        }
        for s in &self.curr {
            if matches!(s.kind, Kind::Infantry | Kind::Worker | Kind::Heavy) {
                let (ex, ez) = (f(s.pos.x), f(s.pos.y));
                if (ex - wx).hypot(ez - wz) < r + 1.5 {
                    return false;
                }
            }
        }
        for pb in &self.pending_builds {
            if (pb.wx - wx).hypot(pb.wz - wz) < 14.0 {
                return false;
            }
        }
        // Slope limit: sample the natural ground around the footprint rim
        // (also rejects a waterline footprint edge).
        let h0 = terrain::natural_height(wx, wz);
        for k in 0..8 {
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            let (sx, sz) = (wx + a.cos() * r, wz + a.sin() * r);
            if terrain::submerged(sx, sz) {
                return false;
            }
            let h = terrain::natural_height(sx, sz);
            if (h - h0).abs() > MAX_PAD_CUT {
                return false;
            }
        }
        true
    }

    fn info(&self, s: &Snap) -> UnitInfo {
        let (wx, wz) = self.lerped(s);
        let barracks = matches!(
            s.kind,
            Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
        );
        let radius = match s.kind {
            Kind::Hq => 9.0,
            Kind::Barracks => 8.0,
            Kind::Turret => 4.0,
            Kind::Supply => 4.5,
            _ => 0.0,
        };
        let name = match (s.kind, self.faction_of(s.owner)) {
            (Kind::Hq, Faction::Astromancer) => "SPIRE",
            (Kind::Hq, Faction::Hollowmen) => "COMMAND HQ",
            (Kind::Barracks, Faction::Astromancer) => "SANCTUM",
            (Kind::Barracks, Faction::Hollowmen) => "BARRACKS",
            (Kind::Turret, Faction::Astromancer) => "WARD",
            (Kind::Turret, Faction::Hollowmen) => "TURRET",
            (Kind::Supply, _) => "DEPOT",
            (Kind::Worker, Faction::Astromancer) => "ACOLYTE",
            (Kind::Worker, Faction::Hollowmen) => "ENGINEER",
            (Kind::Heavy, _) => "HEAVY",
            _ => "INFANTRY",
        };
        UnitInfo {
            owner: s.owner,
            barracks,
            kind: s.kind,
            wx,
            wy: terrain::height(wx, wz) + if barracks { 7.0 } else { 3.4 },
            wz,
            hp_frac: (f(s.hp) / f(s.max_hp)).clamp(0.0, 1.0),
            radius,
            name,
            hp: f(s.hp).ceil().max(0.0) as i32,
            hp_max: f(s.max_hp).ceil() as i32,
        }
    }

    /// Every visible building that has taken damage (for always-on health
    /// bars; pairs with the burning fx so damage reads at a glance).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn damaged_buildings(&self) -> Vec<UnitInfo> {
        self.curr
            .iter()
            .filter(|s| {
                matches!(
                    s.kind,
                    Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
                ) && s.hp < s.max_hp
            })
            .filter(|s| {
                let (wx, wz) = self.lerped(s);
                self.revealed(s, wx, wz)
            })
            .map(|s| self.info(s))
            .collect()
    }

    /// All currently-revealed entities (for the minimap).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn unit_infos(&self) -> Vec<UnitInfo> {
        self.curr
            .iter()
            .filter(|s| {
                let (wx, wz) = self.lerped(s);
                self.revealed(s, wx, wz)
            })
            .map(|s| self.info(s))
            .collect()
    }

    /// Selected entities only (for health bars).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn selected_infos(&self) -> Vec<UnitInfo> {
        let sel: HashSet<u32> = self.selected.iter().copied().collect();
        self.curr
            .iter()
            .filter(|s| sel.contains(&s.index))
            .map(|s| self.info(s))
            .collect()
    }

    /// Your infantry world positions (for drag-box highlight).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn player_units(&self) -> Vec<UnitInfo> {
        self.curr
            .iter()
            .filter(|s| {
                s.owner == 0 && matches!(s.kind, Kind::Infantry | Kind::Worker | Kind::Heavy)
            })
            .map(|s| self.info(s))
            .collect()
    }

    // ---- input → selection / orders ----

    fn nearest_enemy(&self, wx: f32, wz: f32, r: f32) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for s in &self.curr {
            if s.owner == 0 {
                continue;
            }
            let (ex, ez) = (f(s.pos.x), f(s.pos.y));
            if !self.cell_visible(ex, ez) {
                continue;
            }
            let pad = match s.kind {
                Kind::Hq => 7.0,
                Kind::Barracks => 6.0,
                _ => 0.0,
            };
            let d = (ex - wx).hypot(ez - wz) - pad;
            if d <= r && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((s.index, d));
            }
        }
        best.map(|(i, _)| i)
    }

    /// Select the player entity nearest the click, in screen space (units
    /// first, then buildings). Screen-space picking works on slopes, where a
    /// ground-plane pick would land past an elevated unit and miss it.
    ///
    /// `additive` (shift held): keep the current selection and toggle the
    /// clicked entity in or out of it instead of replacing it.
    pub fn select_single(
        &mut self,
        cam: &Camera,
        w: f32,
        h: f32,
        sx: f32,
        sy: f32,
        additive: bool,
    ) {
        if !additive {
            self.selected.clear();
        }
        let mut best: Option<(u32, f32)> = None;

        // Nearest selectable unit within a click radius.
        let unit_r = (h * 0.03).max(18.0);
        for s in &self.curr {
            if s.owner != 0 || !matches!(s.kind, Kind::Infantry | Kind::Worker | Kind::Heavy) {
                continue;
            }
            let (wx, wz) = self.lerped(s);
            let wy = terrain::height(wx, wz) + 1.4;
            if let Some((px, py)) = cam.project(glam::Vec3::new(wx, wy, wz), w, h) {
                let d = (px - sx).hypot(py - sy);
                if d <= unit_r && best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((s.index, d));
                }
            }
        }

        // Otherwise a building whose projected bounds contain the click: a
        // rough screen-space box collider over the whole mesh, so clicking
        // anywhere on the geometry selects it. Units are tested first above,
        // so a unit standing in front of (or behind) a building still wins.
        if best.is_none() {
            for s in &self.curr {
                if s.owner != 0
                    || !matches!(
                        s.kind,
                        Kind::Hq | Kind::Barracks | Kind::Turret | Kind::Supply
                    )
                {
                    continue;
                }
                let (wx, wz) = self.lerped(s);
                let ground = terrain::height(wx, wz);
                let r = match s.kind {
                    Kind::Hq => 9.0,
                    Kind::Barracks => 8.0,
                    Kind::Turret => 4.0,
                    _ => 4.5,
                };
                let top = ground + building_height(s.kind);
                let (mut x0, mut x1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
                let mut all = true;
                for (cx, cz) in [(-r, -r), (r, -r), (-r, r), (r, r)] {
                    for wy in [ground, top] {
                        match cam.project(glam::Vec3::new(wx + cx, wy, wz + cz), w, h) {
                            Some((px, py)) => {
                                x0 = x0.min(px);
                                x1 = x1.max(px);
                                y0 = y0.min(py);
                                y1 = y1.max(py);
                            }
                            None => all = false,
                        }
                    }
                }
                if !all || sx < x0 || sx > x1 || sy < y0 || sy > y1 {
                    continue;
                }
                // Several boxes can overlap; take the closest center.
                let d = (sx - (x0 + x1) * 0.5).hypot(sy - (y0 + y1) * 0.5);
                if best.is_none_or(|(_, bd)| d < bd) {
                    best = Some((s.index, d));
                }
            }
        }

        if let Some((i, _)) = best {
            if additive && self.selected.contains(&i) {
                self.selected.retain(|&u| u != i);
            } else {
                self.selected.push(i);
            }
        }
    }

    /// The selected entity if it is exactly one of the player's production
    /// buildings (HQ or Barracks), with its kind.
    fn selected_producer(&self) -> Option<(u32, Kind)> {
        if self.selected.len() != 1 {
            return None;
        }
        let i = self.selected[0];
        self.curr
            .iter()
            .find(|s| s.index == i && s.owner == 0 && matches!(s.kind, Kind::Hq | Kind::Barracks))
            .map(|s| (s.index, s.kind))
    }

    /// The selected entity if it is exactly one of the player's barracks.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn selected_barracks(&self) -> Option<u32> {
        self.selected_producer()
            .filter(|&(_, k)| k == Kind::Barracks)
            .map(|(i, _)| i)
    }

    /// The selected entity if it is exactly one of the player's HQs.
    pub fn selected_hq(&self) -> Option<u32> {
        self.selected_producer()
            .filter(|&(_, k)| k == Kind::Hq)
            .map(|(i, _)| i)
    }

    /// Queue a unit of `kind` at the selected production building (the sim
    /// validates the building/unit pairing: HQ -> workers, Barracks -> fighters).
    pub fn train_selected(&mut self, kind: UnitKind) {
        if let Some((b, _)) = self.selected_producer() {
            self.pending.push(Command::Train { building: b, kind });
        }
    }

    /// True if at least one of the selected units is a worker (can build).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn has_worker_selected(&self) -> bool {
        let sel: HashSet<u32> = self.selected.iter().copied().collect();
        self.curr
            .iter()
            .any(|s| s.kind == Kind::Worker && sel.contains(&s.index))
    }

    /// Order every selected worker to construct `kind` at `(wx, wz)`. Refused
    /// when the site is blocked or too steep - the same rule the placement
    /// ghost shows in red (the sim separately enforces clearance on arrival).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn build_selected(&mut self, kind: BuildingKind, wx: f32, wz: f32) {
        // Placement locks to the build grid (matching the preview ghost), so
        // structures align into predictable lanes units can path through.
        let (wx, wz) = (snap(wx), snap(wz));
        if !self.site_ok(kind, wx, wz) {
            return;
        }
        let sel: Vec<u32> = self.selected.clone();
        let mut issued = false;
        for u in sel {
            if self.is_worker(u) {
                self.pending.push(Command::Build {
                    unit: u,
                    kind,
                    x: fx(wx),
                    y: fx(wz),
                });
                issued = true;
            }
        }
        if issued {
            // The placed hologram holds the site until the worker raises it.
            self.pending_builds.push(PendingBuild {
                kind,
                wx,
                wz,
                born: self.time,
            });
        }
    }

    /// Whether the local player drives the Astromancer faction (the HUD
    /// picks faction-flavored icon art with this).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn player_is_astromancer(&self) -> bool {
        self.factions[0] == Faction::Astromancer
    }

    /// World position of the local player's (first) HQ, for the camera's
    /// opening shot on whatever spawn this world fitted the base onto.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn player_hq(&self) -> Option<(f32, f32)> {
        self.curr
            .iter()
            .find(|s| s.owner == 0 && s.kind == Kind::Hq)
            .map(|s| (f(s.pos.x), f(s.pos.y)))
    }

    /// The local player's ore stockpile (for the HUD).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn player_ore(&self) -> f32 {
        f(self.world.ore(0))
    }

    /// The local player's carbon stockpile (for the HUD).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn player_carbon(&self) -> f32 {
        f(self.world.carbon(0))
    }

    /// The local player's supply `(used, cap)` (for the HUD).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn player_supply(&self) -> (u32, u32) {
        (self.world.supply_used(0), self.world.supply_cap(0))
    }

    /// (queued, build-progress 0..1) for the selected production building
    /// (HQ or Barracks), for the HUD.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn selected_production(&self) -> Option<(u32, f32)> {
        let (b, _) = self.selected_producer()?;
        self.curr
            .iter()
            .find(|s| s.index == b)
            .map(|s| (s.queued, f(s.build_frac)))
    }

    /// Box-select using the on-screen rectangle (pixel coordinates), matching
    /// the drag preview exactly: each unit is projected to the screen and tested
    /// against the rect. A world-space box would disagree with the preview under
    /// the tilted camera (a screen rect maps to a ground trapezoid, not a box).
    ///
    /// `additive` (shift held): the boxed units join the current selection.
    pub fn select_box_screen(
        &mut self,
        cam: &Camera,
        w: f32,
        h: f32,
        rect: (f32, f32, f32, f32),
        additive: bool,
    ) {
        let (x0, y0, x1, y1) = rect;
        if !additive {
            self.selected.clear();
        }
        for s in &self.curr {
            if s.owner != 0 || !matches!(s.kind, Kind::Infantry | Kind::Worker | Kind::Heavy) {
                continue;
            }
            let (wx, wz) = self.lerped(s);
            // Same body point the preview projects (info().wy - 2.0).
            let wy = terrain::height(wx, wz) + 1.4;
            if let Some((sx, sy)) = cam.project(glam::Vec3::new(wx, wy, wz), w, h) {
                if sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1 && !self.selected.contains(&s.index)
                {
                    self.selected.push(s.index);
                }
            }
        }
    }

    /// Nearest resource node to a world point within `radius`, if any.
    fn nearest_node(&self, wx: f32, wz: f32, radius: f32) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for s in &self.curr {
            if !matches!(s.kind, Kind::OreNode | Kind::CarbonNode) {
                continue;
            }
            let (nx, nz) = self.lerped(s);
            let d = (nx - wx).hypot(nz - wz);
            if d <= radius && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((s.index, d));
            }
        }
        best.map(|(i, _)| i)
    }

    fn is_worker(&self, unit: u32) -> bool {
        self.curr
            .iter()
            .any(|s| s.index == unit && s.kind == Kind::Worker)
    }

    /// What the current selection can do, for the context cursor:
    /// `(any mobile units, any workers, a production building)`.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn selection_profile(&self) -> (bool, bool, bool) {
        let sel: HashSet<u32> = self.selected.iter().copied().collect();
        let mut units = false;
        let mut workers = false;
        for s in &self.curr {
            if !sel.contains(&s.index) {
                continue;
            }
            match s.kind {
                Kind::Worker => {
                    units = true;
                    workers = true;
                }
                Kind::Infantry | Kind::Heavy => units = true,
                _ => {}
            }
        }
        (units, workers, self.selected_producer().is_some())
    }

    /// A visible enemy under the point (context cursor: attack).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn hover_enemy(&self, wx: f32, wz: f32) -> bool {
        self.nearest_enemy(wx, wz, 5.0).is_some()
    }

    /// A resource node under the point (context cursor: harvest).
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn hover_node(&self, wx: f32, wz: f32) -> bool {
        self.nearest_node(wx, wz, 10.0).is_some()
    }

    fn ping(&mut self, ping: Ping, wx: f32, wz: f32, target: Option<u32>) {
        self.effects.push(Effect {
            ping,
            wx,
            wz,
            target,
            born: self.time,
        });
    }

    /// Right-click while exactly one production building is selected: move its
    /// rally point there instead of issuing a unit order. Returns whether the
    /// click was consumed.
    pub fn set_rally_selected(&mut self, wx: f32, wz: f32) -> bool {
        let Some((b, _)) = self.selected_producer() else {
            return false;
        };
        // Rally onto a resource node snaps to its center: freshly trained
        // workers then auto-harvest the cluster (the sim spreads them).
        let node = self.nearest_node(wx, wz, 10.0);
        let (rx, rz) = node
            .and_then(|n| self.curr.iter().find(|s| s.index == n))
            .map(|s| (f(s.pos.x), f(s.pos.y)))
            .unwrap_or((wx, wz));
        self.pending.push(Command::SetRally {
            building: b,
            x: fx(rx),
            y: fx(rz),
        });
        if let Some(n) = node {
            self.ping(Ping::Harvest, rx, rz, Some(n));
        } else {
            self.ping(Ping::Move, rx, rz, None);
        }
        true
    }

    /// If `(wx, wz)` lands on a placed hologram and workers are selected,
    /// re-issue the build to them and refresh the ghost's clock. Returns
    /// whether the click was consumed.
    fn resume_pending_build(&mut self, wx: f32, wz: f32) -> bool {
        let Some(idx) = self
            .pending_builds
            .iter()
            .position(|pb| (wx - pb.wx).hypot(wz - pb.wz) <= pad_radius(pb.kind) + 1.5)
        else {
            return false;
        };
        let (kind, bx, bz) = {
            let pb = &self.pending_builds[idx];
            (pb.kind, pb.wx, pb.wz)
        };
        let mut issued = false;
        for u in self.selected.clone() {
            if self.is_worker(u) {
                self.pending.push(Command::Build {
                    unit: u,
                    kind,
                    x: fx(bx),
                    y: fx(bz),
                });
                issued = true;
            }
        }
        if issued {
            self.pending_builds[idx].born = self.time;
            self.ping(Ping::Move, bx, bz, None);
        }
        issued
    }

    /// Right-click order at a ground point. `attack` (Ctrl held) forces an
    /// attack-move: the group advances and engages anything on the way.
    pub fn order(&mut self, wx: f32, wz: f32, attack: bool) {
        if self.selected.is_empty() {
            return;
        }
        // Right-clicking a placed hologram with workers selected resumes
        // the pending build (re-tasking them onto a site whose original
        // builder died or was pulled away).
        if !attack && self.resume_pending_build(wx, wz) {
            return;
        }
        // Right-clicking a resource node sends selected workers to harvest it;
        // any non-worker in the selection just moves to the spot.
        if let Some(node) = self.nearest_node(wx, wz, 10.0) {
            let sel = self.selected.clone();
            let mut harvesting = false;
            for u in sel {
                if self.is_worker(u) {
                    self.pending.push(Command::Harvest { unit: u, node });
                    harvesting = true;
                } else {
                    self.pending.push(Command::Move {
                        unit: u,
                        x: fx(wx),
                        y: fx(wz),
                    });
                }
            }
            if harvesting {
                self.ping(Ping::Harvest, wx, wz, Some(node));
            } else {
                self.ping(Ping::Move, wx, wz, None);
            }
            return;
        }
        if let Some(target) = self.nearest_enemy(wx, wz, 4.0) {
            for &u in &self.selected {
                self.pending.push(Command::Attack { unit: u, target });
            }
            self.ping(Ping::Attack, wx, wz, Some(target));
        } else {
            // Spread the group across a centered grid so they march to distinct
            // cells instead of one shared point; the sim's separation then keeps
            // them from stacking as they arrive.
            let sel: Vec<u32> = self.selected.clone();
            let cols = (sel.len() as f32).sqrt().ceil().max(1.0);
            let rows = (sel.len() as f32 / cols).ceil();
            let spacing = 2.6_f32;
            for (k, &u) in sel.iter().enumerate() {
                let c = (k % cols as usize) as f32;
                let r = (k / cols as usize) as f32;
                let ox = (c - (cols - 1.0) * 0.5) * spacing;
                let oz = (r - (rows - 1.0) * 0.5) * spacing;
                let (x, y) = (fx(wx + ox), fx(wz + oz));
                self.pending.push(if attack {
                    Command::AttackMove { unit: u, x, y }
                } else {
                    Command::Move { unit: u, x, y }
                });
            }
            let ping = if attack { Ping::Attack } else { Ping::Move };
            self.ping(ping, wx, wz, None);
        }
    }
}
