//! Front-end screens (web): the main menu and the skirmish lobby.
//!
//! Like the rest of the functional UI these are drawn on the Rust-driven HUD
//! canvas, never the DOM. Native has no HUD canvas, so the native dev build skips
//! straight into the match; only the shared `Screen`/`Lobby`/`Click` types are
//! compiled there.

use crate::game::Faction;

/// Which top-level screen the app is showing.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum Screen {
    Menu,
    Lobby,
    InGame,
}

/// Skirmish setup chosen in the lobby. `faction` and `map` (an index into the
/// voxel battlefields, `crate::voxel`) are applied to the match on Start; `bots`
/// is UI flavor for now.
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct Lobby {
    pub faction: Faction,
    pub bots: u8,
    pub map: u8,
}

impl Default for Lobby {
    fn default() -> Self {
        Lobby {
            faction: Faction::Hollowmen,
            bots: 1,
            map: 0,
        }
    }
}

/// A click the front-end recognized, returned by `hit` for the app to act on.
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum Click {
    None,
    Skirmish,
    SetFaction(Faction),
    AddBot,
    RemoveBot,
    NextMap,
    PrevMap,
    Back,
    Start,
}

#[cfg(target_arch = "wasm32")]
pub fn faction_name(f: Faction) -> &'static str {
    match f {
        Faction::Astromancer => "ASTROMANCERS",
        Faction::Hollowmen => "HOLLOWMEN",
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use wasm_bindgen::JsCast;
    use web_sys::CanvasRenderingContext2d as Ctx;

    fn dpr() -> f32 {
        web_sys::window()
            .map(|w| w.device_pixel_ratio() as f32)
            .filter(|d| *d > 0.5)
            .unwrap_or(1.0)
    }

    /// The five factions and the enum they map to (None = not yet playable).
    const FACTIONS: [(&str, Option<Faction>); 5] = [
        ("ASTROMANCERS", Some(Faction::Astromancer)),
        ("HOLLOWMEN", Some(Faction::Hollowmen)),
        ("NINEFOLD", None),
        ("WARREN", None),
        ("RIMELINGS", None),
    ];

    /// One interactive button in CSS pixels.
    struct Btn {
        click: Click,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        label: String,
        enabled: bool,
        selected: bool,
    }

    fn btn(click: Click, x: f64, y: f64, w: f64, h: f64, label: &str, enabled: bool) -> Btn {
        Btn {
            click,
            x,
            y,
            w,
            h,
            label: label.to_string(),
            enabled,
            selected: false,
        }
    }

    /// The interactive buttons for a screen, in CSS pixels. Single source of
    /// truth for both `draw` and `hit`.
    fn layout(screen: Screen, lobby: &Lobby, w: f64, h: f64) -> Vec<Btn> {
        let mut v = Vec::new();
        match screen {
            Screen::Menu => {
                let bw = 280.0;
                let bx = (w - bw) / 2.0;
                // Campaign sits on top (greyed, not yet playable); Skirmish below.
                v.push(btn(
                    Click::None,
                    bx,
                    h * 0.46,
                    bw,
                    56.0,
                    "CAMPAIGN  (SOON)",
                    false,
                ));
                v.push(btn(
                    Click::Skirmish,
                    bx,
                    h * 0.46 + 72.0,
                    bw,
                    56.0,
                    "SKIRMISH",
                    true,
                ));
            }
            Screen::Lobby => {
                // Faction picker (left column).
                for (i, (name, fac)) in FACTIONS.iter().enumerate() {
                    let mut b = btn(
                        fac.map(Click::SetFaction).unwrap_or(Click::None),
                        60.0,
                        168.0 + i as f64 * 60.0,
                        320.0,
                        48.0,
                        name,
                        fac.is_some(),
                    );
                    b.selected = *fac == Some(lobby.faction);
                    v.push(b);
                }
                // Bot + map controls (right column).
                let rx = w - 400.0;
                v.push(btn(
                    Click::RemoveBot,
                    rx,
                    250.0,
                    60.0,
                    44.0,
                    "-",
                    lobby.bots > 1,
                ));
                v.push(btn(
                    Click::AddBot,
                    rx + 320.0,
                    250.0,
                    60.0,
                    44.0,
                    "+",
                    lobby.bots < 3,
                ));
                // Map preview thumbnail is drawn at y=336 (h=190); prev/next below.
                v.push(btn(Click::PrevMap, rx, 548.0, 185.0, 44.0, "< PREV", true));
                v.push(btn(
                    Click::NextMap,
                    rx + 195.0,
                    548.0,
                    185.0,
                    44.0,
                    "NEXT >",
                    true,
                ));
                // Back / Start.
                v.push(btn(Click::Back, 60.0, h - 96.0, 200.0, 56.0, "BACK", true));
                v.push(btn(
                    Click::Start,
                    w - 260.0,
                    h - 96.0,
                    200.0,
                    56.0,
                    "START",
                    true,
                ));
            }
            Screen::InGame => {}
        }
        v
    }

    fn ctx() -> Option<(Ctx, f64, f64, f32)> {
        let win = web_sys::window()?;
        let canvas = win
            .document()?
            .get_element_by_id("hud")?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .ok()?;
        let ctx = canvas.get_context("2d").ok()??.dyn_into::<Ctx>().ok()?;
        let d = dpr();
        let _ = ctx.set_transform(d as f64, 0.0, 0.0, d as f64, 0.0, 0.0);
        let w = canvas.width() as f64 / d as f64;
        let h = canvas.height() as f64 / d as f64;
        Some((ctx, w, h, d))
    }

    fn draw_btn(ctx: &Ctx, b: &Btn) {
        let (fill, border, text) = if !b.enabled {
            ("rgba(28,34,48,0.85)", "rgba(70,84,110,0.6)", "#5e6a82")
        } else if b.selected {
            ("rgba(40,86,150,0.95)", "rgba(150,200,255,0.95)", "#eaf2ff")
        } else {
            ("rgba(18,26,44,0.92)", "rgba(120,160,210,0.9)", "#dce6f6")
        };
        ctx.set_fill_style_str(fill);
        ctx.fill_rect(b.x, b.y, b.w, b.h);
        ctx.set_stroke_style_str(border);
        ctx.set_line_width(if b.selected { 2.5 } else { 1.5 });
        ctx.stroke_rect(b.x, b.y, b.w, b.h);
        ctx.set_fill_style_str(text);
        ctx.set_font("bold 18px monospace");
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text(&b.label, b.x + b.w / 2.0, b.y + b.h / 2.0);
    }

    /// A top-down thumbnail of the chosen world: its surface colour plus the
    /// landform that defines it (craters, methane/water seas, or volcanoes), and
    /// the player (blue) vs enemy (red) start positions. Deterministic per world.
    fn draw_map_preview(ctx: &Ctx, x: f64, y: f64, w: f64, h: f64, idx: usize) {
        use std::f64::consts::TAU;
        let sw = crate::voxel::MAP_SWATCH[idx];
        let name = crate::voxel::MAP_NAMES[idx];
        let shade = |m: f64| {
            format!(
                "rgb({},{},{})",
                (sw[0] as f64 * m) as u8,
                (sw[1] as f64 * m) as u8,
                (sw[2] as f64 * m) as u8
            )
        };
        // Space backdrop, then the world's surface fills the panel.
        ctx.set_fill_style_str("#05080f");
        ctx.fill_rect(x, y, w, h);
        ctx.save();
        ctx.begin_path();
        ctx.rect(x, y, w, h);
        ctx.clip();
        ctx.set_fill_style_str(&shade(1.0));
        ctx.fill_rect(x, y, w, h);

        // Deterministic positions from the world index (small LCG).
        let mut s: u64 = idx as u64 * 0x9E37_79B9_7F4A_7C15 + 1;
        let mut rnd = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 33) & 0xFFFF) as f64 / 65535.0
        };
        let dot = |cx: f64, cy: f64, r: f64, col: &str| {
            ctx.set_fill_style_str(col);
            ctx.begin_path();
            let _ = ctx.arc(cx, cy, r, 0.0, TAU);
            ctx.fill();
        };

        if name == "EARTH" {
            for _ in 0..5 {
                dot(
                    x + rnd() * w,
                    y + rnd() * h,
                    h * (0.12 + rnd() * 0.16),
                    "#2f6dab",
                );
            }
            dot(x + w * 0.62, y + h * 0.4, h * 0.1, "#e8eef4"); // a snowy peak
        } else if name == "TITAN" {
            for _ in 0..4 {
                dot(
                    x + rnd() * w,
                    y + rnd() * h,
                    h * (0.1 + rnd() * 0.16),
                    "#23252f",
                );
            }
        } else if name == "IO" {
            for _ in 0..4 {
                let (vx, vy) = (x + 0.2 * w + rnd() * 0.6 * w, y + 0.2 * h + rnd() * 0.6 * h);
                dot(vx, vy, h * 0.12, &shade(1.15));
                dot(vx, vy, h * 0.05, "#ec7a2c"); // lava summit
            }
        } else {
            // Rocky / icy: scattered impact craters (dark floor, light rim).
            let n = if matches!(name, "ENCELADUS" | "TRITON" | "PLUTO" | "MARS") {
                5
            } else {
                11
            };
            for _ in 0..n {
                let (cx, cy) = (x + rnd() * w, y + rnd() * h);
                let r = h * (0.05 + rnd() * 0.1);
                ctx.set_stroke_style_str(&shade(1.25));
                ctx.set_line_width(2.0);
                ctx.begin_path();
                let _ = ctx.arc(cx, cy, r, 0.0, TAU);
                ctx.stroke();
                dot(cx, cy, r * 0.7, &shade(0.7));
            }
        }

        // Start positions: player (blue) NW, enemy (red) SE.
        dot(x + w * 0.26, y + h * 0.28, 6.0, "#4aa3ff");
        dot(x + w * 0.74, y + h * 0.72, 6.0, "#ff5a4a");
        ctx.restore();

        ctx.set_stroke_style_str("rgba(120,160,210,0.9)");
        ctx.set_line_width(1.5);
        ctx.stroke_rect(x, y, w, h);
    }

    pub fn draw(screen: Screen, lobby: &Lobby) {
        let Some((ctx, w, h, _)) = ctx() else { return };
        ctx.clear_rect(0.0, 0.0, w, h);
        // Dim the 3D scene behind the front-end.
        ctx.set_fill_style_str("rgba(4,7,14,0.82)");
        ctx.fill_rect(0.0, 0.0, w, h);

        ctx.set_text_baseline("alphabetic");
        match screen {
            Screen::Menu => {
                ctx.set_text_align("center");
                ctx.set_fill_style_str("#e7eefa");
                ctx.set_font("bold 64px monospace");
                let _ = ctx.fill_text("ASTROMANCERS", w / 2.0, h * 0.30);
                ctx.set_fill_style_str("#7f9ec8");
                ctx.set_font("18px monospace");
                let _ = ctx.fill_text(
                    "a deterministic lockstep RTS  -  two worlds, one wall",
                    w / 2.0,
                    h * 0.30 + 34.0,
                );
            }
            Screen::Lobby => {
                ctx.set_text_align("left");
                ctx.set_fill_style_str("#e7eefa");
                ctx.set_font("bold 34px monospace");
                let _ = ctx.fill_text("SKIRMISH  -  LOBBY", 60.0, 96.0);
                ctx.set_fill_style_str("#9ab2d8");
                ctx.set_font("bold 16px monospace");
                let _ = ctx.fill_text("CHOOSE YOUR FACTION", 60.0, 150.0);

                // Right column: player slots, bot count, map.
                let rx = w - 400.0;
                ctx.set_fill_style_str("#9ab2d8");
                let _ = ctx.fill_text("PLAYERS", rx, 150.0);
                ctx.set_font("16px monospace");
                ctx.set_fill_style_str("#dce6f6");
                let _ = ctx.fill_text(
                    &format!("P1  YOU   -  {}", faction_name(lobby.faction)),
                    rx,
                    188.0,
                );
                let enemy = match lobby.faction {
                    Faction::Astromancer => Faction::Hollowmen,
                    Faction::Hollowmen => Faction::Astromancer,
                };
                for i in 0..lobby.bots {
                    let _ = ctx.fill_text(
                        &format!("P{}  BOT   -  {}", i + 2, faction_name(enemy)),
                        rx,
                        212.0 + i as f64 * 22.0,
                    );
                }
                ctx.set_fill_style_str("#9ab2d8");
                ctx.set_font("bold 16px monospace");
                let _ = ctx.fill_text(&format!("BOTS: {}", lobby.bots), rx + 70.0, 278.0);
                let mi = lobby.map as usize % crate::voxel::MAP_COUNT;
                let map = crate::voxel::MAP_NAMES[mi];
                let _ = ctx.fill_text(
                    &format!("MAP:  {map}   ({}/{})", mi + 1, crate::voxel::MAP_COUNT),
                    rx,
                    326.0,
                );
                draw_map_preview(&ctx, rx, 336.0, 380.0, 190.0, mi);
            }
            Screen::InGame => {}
        }

        for b in layout(screen, lobby, w, h) {
            draw_btn(&ctx, &b);
        }
        // Restore defaults the in-game HUD relies on.
        ctx.set_text_align("left");
        ctx.set_text_baseline("alphabetic");
    }

    pub fn hit(screen: Screen, lobby: &Lobby, cx_phys: f32, cy_phys: f32) -> Click {
        let Some((_, w, h, d)) = ctx() else {
            return Click::None;
        };
        let (cx, cy) = ((cx_phys / d) as f64, (cy_phys / d) as f64);
        for b in layout(screen, lobby, w, h) {
            if b.enabled && cx >= b.x && cx <= b.x + b.w && cy >= b.y && cy <= b.y + b.h {
                return b.click;
            }
        }
        Click::None
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::{draw, hit};
