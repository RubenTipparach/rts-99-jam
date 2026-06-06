//! Game glue: drives the deterministic sim, holds selection, turns player input
//! into commands, and produces render + HUD data. Floats live here (the wall).

use crate::gfx::{InstanceRaw, RingRaw};
use crate::terrain;
use math::{Fx, FRAC_BITS};
use protocol::{BuildingKind, Command, UnitKind};
use sim::{Kind, Snap, World};
use std::collections::HashMap;
use web_time::Instant;

const TICK_HZ: u32 = 20;
const SEED: u64 = 0x5011_D011_0099_0001;

#[inline]
fn f(x: Fx) -> f32 {
    x.to_raw() as f32 / (1u64 << FRAC_BITS) as f32
}
#[inline]
fn fxi(i: i32) -> Fx {
    Fx::from_int(i)
}

fn team_color(owner: u16, barracks: bool) -> [f32; 4] {
    match (owner, barracks) {
        (0, false) => [0.34, 0.58, 0.96, 1.0], // player infantry — blue
        (0, true) => [0.20, 0.36, 0.66, 1.0],  // player barracks
        (_, false) => [0.93, 0.34, 0.27, 1.0], // enemy infantry — red
        (_, true) => [0.62, 0.20, 0.18, 1.0],  // enemy barracks
    }
}

/// Per-entity info for the HUD (health bars, minimap).
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct UnitInfo {
    pub owner: u16,
    pub barracks: bool,
    pub wx: f32,
    pub wy: f32,
    pub wz: f32,
    pub hp_frac: f32,
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
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        let world = World::new(SEED);
        let mut setup = Vec::new();
        // Player base (south).
        setup.push(Command::SpawnBuilding {
            owner: 0,
            kind: BuildingKind::Barracks,
            x: fxi(0),
            y: fxi(-26),
        });
        for k in 0..4 {
            setup.push(Command::SpawnUnit {
                owner: 0,
                kind: UnitKind::Infantry,
                x: fxi(-4 + 2 * k),
                y: fxi(-20),
            });
        }
        // Two enemy barracks (north), each with a guard squad.
        for &bx in &[-14i32, 14] {
            setup.push(Command::SpawnBuilding {
                owner: 1,
                kind: BuildingKind::Barracks,
                x: fxi(bx),
                y: fxi(24),
            });
            for k in 0..5 {
                setup.push(Command::SpawnUnit {
                    owner: 1,
                    kind: UnitKind::Infantry,
                    x: fxi(bx - 4 + 2 * k),
                    y: fxi(18),
                });
            }
        }

        let mut g = Game {
            world,
            tick_dt: 1.0 / TICK_HZ as f32,
            acc: 0.0,
            last: Instant::now(),
            time: 0.0,
            prev: Vec::new(),
            curr: Vec::new(),
            selected: Vec::new(),
            pending: setup,
        };
        // Run tick 0 so the scenario exists immediately.
        g.step_now();
        g.prev = g.curr.clone();
        g
    }

    fn step_now(&mut self) {
        let cmds = std::mem::take(&mut self.pending);
        self.prev = self.curr.clone();
        self.world.step(&cmds);
        self.curr = self.world.snapshot();
        // Drop selections that are no longer live player infantry.
        let alive: HashMap<u32, &Snap> = self.curr.iter().map(|s| (s.index, s)).collect();
        self.selected.retain(|i| {
            alive
                .get(i)
                .is_some_and(|s| s.owner == 0 && s.kind == Kind::Infantry)
        });
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
    }

    pub fn time(&self) -> f32 {
        self.time
    }

    fn alpha(&self) -> f32 {
        (self.acc / self.tick_dt).clamp(0.0, 1.0)
    }

    /// Interpolated world (x, z) of an entity this frame.
    fn lerped_xz(&self, prev_map: &HashMap<u32, (f32, f32)>, s: &Snap) -> (f32, f32) {
        let cx = f(s.pos.x);
        let cz = f(s.pos.y);
        if let Some(&(px, pz)) = prev_map.get(&s.index) {
            let a = self.alpha();
            (px + (cx - px) * a, pz + (cz - pz) * a)
        } else {
            (cx, cz)
        }
    }

    fn prev_map(&self) -> HashMap<u32, (f32, f32)> {
        self.prev
            .iter()
            .map(|s| (s.index, (f(s.pos.x), f(s.pos.y))))
            .collect()
    }

    /// Build render instances (units + buildings) and selection rings.
    pub fn render_data(&self) -> (Vec<InstanceRaw>, Vec<RingRaw>) {
        let pm = self.prev_map();
        let sel: std::collections::HashSet<u32> = self.selected.iter().copied().collect();
        let mut units = Vec::with_capacity(self.curr.len());
        let mut rings = Vec::new();
        for s in &self.curr {
            let (wx, wz) = self.lerped_xz(&pm, s);
            let ground = terrain::height(wx, wz);
            let barracks = s.kind == Kind::Barracks;
            let (scale, mut y) = if barracks {
                ([8.5, 5.0, 8.5], ground)
            } else {
                ([0.95, 1.7, 0.95], ground)
            };
            if !barracks && s.moving {
                y += ((self.time * 9.0) + s.index as f32 * 1.3).sin() * 0.12;
            }
            units.push(InstanceRaw {
                offset: [wx, y, wz],
                scale,
                color: team_color(s.owner, barracks),
            });
            if sel.contains(&s.index) {
                rings.push(RingRaw {
                    center: [wx, ground, wz],
                    radius: if barracks { 6.5 } else { 1.5 },
                    color: [0.4, 1.0, 0.5, 0.9],
                });
            }
        }
        (units, rings)
    }

    /// World positions + hp for the HUD.
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn unit_infos(&self) -> Vec<UnitInfo> {
        let pm = self.prev_map();
        self.curr
            .iter()
            .map(|s| {
                let (wx, wz) = self.lerped_xz(&pm, s);
                let barracks = s.kind == Kind::Barracks;
                let h = terrain::height(wx, wz) + if barracks { 5.4 } else { 2.2 };
                let hp_frac = (f(s.hp) / f(s.max_hp)).clamp(0.0, 1.0);
                UnitInfo {
                    owner: s.owner,
                    barracks,
                    wx,
                    wy: h,
                    wz,
                    hp_frac,
                }
            })
            .collect()
    }

    /// (player units, enemy units, player buildings, enemy buildings)
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn counts(&self) -> (u32, u32, u32, u32) {
        let mut c = (0, 0, 0, 0);
        for s in &self.curr {
            match (s.owner, s.kind) {
                (0, Kind::Infantry) => c.0 += 1,
                (_, Kind::Infantry) => c.1 += 1,
                (0, Kind::Barracks) => c.2 += 1,
                (_, Kind::Barracks) => c.3 += 1,
            }
        }
        c
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn selected_count(&self) -> usize {
        self.selected.len()
    }

    // ---- input → selection / orders ----

    fn find_player_unit(&self, wx: f32, wz: f32, r: f32) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for s in &self.curr {
            if s.owner != 0 || s.kind != Kind::Infantry {
                continue;
            }
            let d = (f(s.pos.x) - wx).hypot(f(s.pos.y) - wz);
            if d <= r && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((s.index, d));
            }
        }
        best.map(|(i, _)| i)
    }

    fn find_enemy(&self, wx: f32, wz: f32, r: f32) -> Option<u32> {
        let mut best: Option<(u32, f32)> = None;
        for s in &self.curr {
            if s.owner == 0 {
                continue;
            }
            let pad = if s.kind == Kind::Barracks { 5.0 } else { 0.0 };
            let d = (f(s.pos.x) - wx).hypot(f(s.pos.y) - wz) - pad;
            if d <= r && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((s.index, d));
            }
        }
        best.map(|(i, _)| i)
    }

    pub fn select_single(&mut self, wx: f32, wz: f32) {
        self.selected.clear();
        if let Some(i) = self.find_player_unit(wx, wz, 2.5) {
            self.selected.push(i);
        }
    }

    pub fn select_box(&mut self, ax: f32, az: f32, bx: f32, bz: f32) {
        let (x0, x1) = (ax.min(bx), ax.max(bx));
        let (z0, z1) = (az.min(bz), az.max(bz));
        self.selected.clear();
        for s in &self.curr {
            if s.owner != 0 || s.kind != Kind::Infantry {
                continue;
            }
            let (x, z) = (f(s.pos.x), f(s.pos.y));
            if x >= x0 && x <= x1 && z >= z0 && z <= z1 {
                self.selected.push(s.index);
            }
        }
    }

    /// Right-click: attack an enemy near the point, else attack-move there.
    pub fn order(&mut self, wx: f32, wz: f32) {
        if self.selected.is_empty() {
            return;
        }
        let (x, y) = (
            Fx::from_raw((wx * (1 << FRAC_BITS) as f32) as i64),
            Fx::from_raw((wz * (1 << FRAC_BITS) as f32) as i64),
        );
        if let Some(target) = self.find_enemy(wx, wz, 3.0) {
            for &u in &self.selected {
                self.pending.push(Command::Attack { unit: u, target });
            }
        } else {
            for &u in &self.selected {
                self.pending.push(Command::AttackMove { unit: u, x, y });
            }
        }
    }
}
