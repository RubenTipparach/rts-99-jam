//! Game glue: drives the deterministic sim, holds selection, turns input into
//! commands, computes fog-of-war, and produces render + HUD data. Floats here.

use crate::gfx::{InstanceRaw, RingRaw, FOW_RES};
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
fn fxi(i: i32) -> Fx {
    Fx::from_int(i)
}
#[inline]
fn fx(v: f32) -> Fx {
    Fx::from_raw((v * (1u64 << FRAC_BITS) as f32) as i64)
}

fn team_color(owner: u16, barracks: bool) -> [f32; 4] {
    match (owner, barracks) {
        (0, false) => [0.36, 0.60, 0.98, 1.0],
        (0, true) => [0.22, 0.40, 0.72, 1.0],
        (_, false) => [0.95, 0.36, 0.28, 1.0],
        (_, true) => [0.64, 0.22, 0.18, 1.0],
    }
}

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
    visible: Vec<bool>,
    explored: Vec<bool>,
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

impl Game {
    pub fn new() -> Self {
        let mut setup = Vec::new();
        // Player base (near the camera start).
        setup.push(Command::SpawnBuilding {
            owner: 0,
            kind: BuildingKind::Barracks,
            x: fxi(0),
            y: fxi(210),
        });
        for k in 0..5 {
            setup.push(Command::SpawnUnit {
                owner: 0,
                kind: UnitKind::Infantry,
                x: fxi(-8 + 4 * k),
                y: fxi(180),
            });
        }
        // Two enemy barracks far to the north, each with a guard squad — hidden
        // by fog until you scout up to them.
        for &bx in &[-150i32, 150] {
            setup.push(Command::SpawnBuilding {
                owner: 1,
                kind: BuildingKind::Barracks,
                x: fxi(bx),
                y: fxi(-190),
            });
            for k in 0..6 {
                setup.push(Command::SpawnUnit {
                    owner: 1,
                    kind: UnitKind::Infantry,
                    x: fxi(bx - 10 + 4 * k),
                    y: fxi(-165),
                });
            }
        }

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
        };
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
        let live: HashSet<u32> = self
            .curr
            .iter()
            .filter(|s| s.owner == 0 && s.kind == Kind::Infantry)
            .map(|s| s.index)
            .collect();
        self.selected.retain(|i| live.contains(i));
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
                let r = if s.kind == Kind::Barracks {
                    125.0
                } else {
                    90.0
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
    /// (Units don't read this — they're shown/hidden outright via `revealed`.)
    pub fn fow_bytes(&self) -> Vec<u8> {
        let n = FOW_RES;
        let mut field = vec![0f32; n * n];
        for (i, v) in field.iter_mut().enumerate() {
            *v = if self.visible[i] {
                1.0
            } else if self.explored[i] {
                0.45
            } else {
                0.0
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

    /// Entity is shown if it's ours, or an enemy currently in vision.
    fn revealed(&self, s: &Snap, wx: f32, wz: f32) -> bool {
        s.owner == 0 || self.cell_visible(wx, wz)
    }

    // ---- render + HUD data ----

    pub fn render_data(&self) -> (Vec<InstanceRaw>, Vec<RingRaw>) {
        let sel: HashSet<u32> = self.selected.iter().copied().collect();
        let mut units = Vec::with_capacity(self.curr.len());
        let mut rings = Vec::new();
        for s in &self.curr {
            let (wx, wz) = self.lerped(s);
            if !self.revealed(s, wx, wz) {
                continue;
            }
            let ground = terrain::height(wx, wz);
            let barracks = s.kind == Kind::Barracks;
            let (scale, mut y) = if barracks {
                ([10.0, 6.0, 10.0], ground)
            } else {
                ([1.4, 2.4, 1.4], ground)
            };
            if !barracks && s.moving {
                y += ((self.time * 9.0) + s.index as f32 * 1.3).sin() * 0.18;
            }
            units.push(InstanceRaw {
                offset: [wx, y, wz],
                scale,
                color: team_color(s.owner, barracks),
            });
            if sel.contains(&s.index) {
                rings.push(RingRaw {
                    center: [wx, ground, wz],
                    radius: if barracks { 8.0 } else { 2.2 },
                    color: [0.4, 1.0, 0.5, 0.95],
                });
            }
        }
        (units, rings)
    }

    fn info(&self, s: &Snap) -> UnitInfo {
        let (wx, wz) = self.lerped(s);
        let barracks = s.kind == Kind::Barracks;
        UnitInfo {
            owner: s.owner,
            barracks,
            wx,
            wy: terrain::height(wx, wz) + if barracks { 7.0 } else { 3.4 },
            wz,
            hp_frac: (f(s.hp) / f(s.max_hp)).clamp(0.0, 1.0),
        }
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
            .filter(|s| s.owner == 0 && s.kind == Kind::Infantry)
            .map(|s| self.info(s))
            .collect()
    }

    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub fn counts(&self) -> (u32, u32, u32, u32) {
        let mut c = (0u32, 0u32, 0u32, 0u32);
        for s in &self.curr {
            let (wx, wz) = self.lerped(s);
            if !self.revealed(s, wx, wz) {
                continue;
            }
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

    fn nearest_player(&self, wx: f32, wz: f32, r: f32) -> Option<u32> {
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
            let pad = if s.kind == Kind::Barracks { 6.0 } else { 0.0 };
            let d = (ex - wx).hypot(ez - wz) - pad;
            if d <= r && best.is_none_or(|(_, bd)| d < bd) {
                best = Some((s.index, d));
            }
        }
        best.map(|(i, _)| i)
    }

    pub fn select_single(&mut self, wx: f32, wz: f32) {
        self.selected.clear();
        if let Some(i) = self.nearest_player(wx, wz, 4.0) {
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

    pub fn order(&mut self, wx: f32, wz: f32) {
        if self.selected.is_empty() {
            return;
        }
        if let Some(target) = self.nearest_enemy(wx, wz, 4.0) {
            for &u in &self.selected {
                self.pending.push(Command::Attack { unit: u, target });
            }
        } else {
            let (x, y) = (fx(wx), fx(wz));
            for &u in &self.selected {
                self.pending.push(Command::AttackMove { unit: u, x, y });
            }
        }
    }
}
