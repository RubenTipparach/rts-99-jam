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
/// is UI flavor for now. `map_open`/`map_scroll` drive the map-select modal.
#[derive(Clone, Copy)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub struct Lobby {
    pub faction: Faction,
    pub bots: u8,
    pub map: u8,
    pub map_open: bool,
    pub map_scroll: u8,
}

impl Default for Lobby {
    fn default() -> Self {
        Lobby {
            faction: Faction::Hollowmen,
            bots: 1,
            map: 0,
            map_open: false,
            map_scroll: 0,
        }
    }
}

/// A click the front-end recognized, returned by `hit` for the app to act on.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum Click {
    None,
    Skirmish,
    SetFaction(Faction),
    AddBot,
    RemoveBot,
    OpenMap,
    CloseMap,
    PickMap(u8),
    ScrollMap(i8),
    Back,
    Start,
    /// Toggle browser fullscreen (web only; needs this user gesture).
    Fullscreen,
}

/// Rows shown at once in the map-select modal (also bounds `Lobby::map_scroll`).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub const MAP_VIS_ROWS: usize = 9;

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

    /// Uniform UI scale: the layout is authored for a ~1180x720 desktop window
    /// and shrinks to fit small screens (phone landscape) without overlapping.
    fn ui_scale(w: f64, h: f64) -> f64 {
        (w / 1180.0).min(h / 720.0).clamp(0.42, 1.0)
    }

    /// The interactive buttons for a screen, in CSS pixels. Single source of
    /// truth for both `draw` and `hit`. Fixed offsets/sizes scale by `ui_scale`.
    fn layout(screen: Screen, lobby: &Lobby, w: f64, h: f64) -> Vec<Btn> {
        let s = ui_scale(w, h);
        let mut v = Vec::new();
        // Fullscreen toggle, top-right on every front-end screen (web builds).
        if screen != Screen::InGame {
            v.push(btn(
                Click::Fullscreen,
                w - 196.0 * s - 16.0,
                16.0,
                196.0 * s,
                44.0 * s,
                "FULLSCREEN",
                true,
            ));
        }
        match screen {
            Screen::Menu => {
                let bw = 280.0 * s;
                let bx = (w - bw) / 2.0;
                // Campaign sits on top (greyed, not yet playable); Skirmish below.
                v.push(btn(
                    Click::None,
                    bx,
                    h * 0.46,
                    bw,
                    56.0 * s,
                    "CAMPAIGN  (SOON)",
                    false,
                ));
                v.push(btn(
                    Click::Skirmish,
                    bx,
                    h * 0.46 + 72.0 * s,
                    bw,
                    56.0 * s,
                    "SKIRMISH",
                    true,
                ));
            }
            Screen::Lobby => {
                // Faction picker (left column).
                for (i, (name, fac)) in FACTIONS.iter().enumerate() {
                    let mut b = btn(
                        fac.map(Click::SetFaction).unwrap_or(Click::None),
                        60.0 * s,
                        (168.0 + i as f64 * 60.0) * s,
                        320.0 * s,
                        48.0 * s,
                        name,
                        fac.is_some(),
                    );
                    b.selected = *fac == Some(lobby.faction);
                    v.push(b);
                }
                // Bot + map controls (right column).
                let rx = w - 400.0 * s;
                v.push(btn(
                    Click::RemoveBot,
                    rx,
                    250.0 * s,
                    60.0 * s,
                    44.0 * s,
                    "-",
                    lobby.bots > 1,
                ));
                v.push(btn(
                    Click::AddBot,
                    rx + 320.0 * s,
                    250.0 * s,
                    60.0 * s,
                    44.0 * s,
                    "+",
                    lobby.bots < 3,
                ));
                // Diamond map preview draws above this; the picker opens a modal.
                v.push(btn(
                    Click::OpenMap,
                    rx,
                    548.0 * s,
                    380.0 * s,
                    44.0 * s,
                    "SELECT MAP",
                    true,
                ));
                // Back / Start.
                v.push(btn(
                    Click::Back,
                    60.0 * s,
                    h - 96.0 * s,
                    200.0 * s,
                    56.0 * s,
                    "BACK",
                    true,
                ));
                v.push(btn(
                    Click::Start,
                    w - 260.0 * s,
                    h - 96.0 * s,
                    200.0 * s,
                    56.0 * s,
                    "START",
                    true,
                ));
            }
            Screen::InGame => {}
        }
        v
    }

    /// Fetch the HUD canvas context, sizing the backing buffer to the screen
    /// (physical pixels) so the front-end fills it - the in-game HUD sizes the
    /// same canvas, but it never runs on the menu screens. Returns the context
    /// plus the CSS width/height to lay out in, and the device pixel ratio.
    fn ctx(w_phys: f64, h_phys: f64) -> Option<(Ctx, f64, f64, f32)> {
        let win = web_sys::window()?;
        let canvas = win
            .document()?
            .get_element_by_id("hud")?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .ok()?;
        if w_phys >= 2.0 && canvas.width() != w_phys as u32 {
            canvas.set_width(w_phys as u32);
        }
        if h_phys >= 2.0 && canvas.height() != h_phys as u32 {
            canvas.set_height(h_phys as u32);
        }
        let ctx = canvas.get_context("2d").ok()??.dyn_into::<Ctx>().ok()?;
        let d = dpr();
        let _ = ctx.set_transform(d as f64, 0.0, 0.0, d as f64, 0.0, 0.0);
        let w = canvas.width() as f64 / d as f64;
        let h = canvas.height() as f64 / d as f64;
        Some((ctx, w, h, d))
    }

    fn draw_btn(ctx: &Ctx, b: &Btn, s: f64) {
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
        ctx.set_font(&format!(
            "bold {}px monospace",
            (18.0 * s).round().max(10.0)
        ));
        ctx.set_text_align("center");
        ctx.set_text_baseline("middle");
        let _ = ctx.fill_text(&b.label, b.x + b.w / 2.0, b.y + b.h / 2.0);
    }

    const VIS_ROWS: usize = super::MAP_VIS_ROWS;

    /// The map-select modal's panel rect (mx, my, mw, mh) in CSS pixels.
    fn modal_rect(w: f64, h: f64) -> (f64, f64, f64, f64) {
        let s = ui_scale(w, h);
        let mw = (760.0 * s).min(w - 24.0);
        let mh = (520.0 * s).min(h - 24.0);
        ((w - mw) / 2.0, (h - mh) / 2.0, mw, mh)
    }

    /// Interactive elements of the map-select modal: the visible list rows, the
    /// scrollbar up/down buttons, and DONE. Single source of truth for draw+hit.
    fn modal_layout(lobby: &Lobby, w: f64, h: f64) -> Vec<Btn> {
        let s = ui_scale(w, h);
        let row_h = 40.0 * s;
        let (mx, my, mw, mh) = modal_rect(w, h);
        let (lx, ly, lw) = (mx + 30.0 * s, my + 92.0 * s, 300.0 * s);
        let count = crate::voxel::MAP_COUNT;
        let scroll = lobby.map_scroll as usize;
        let mut v = Vec::new();
        for r in 0..VIS_ROWS {
            let i = scroll + r;
            if i >= count {
                break;
            }
            let mut b = btn(
                Click::PickMap(i as u8),
                lx,
                ly + r as f64 * row_h,
                lw,
                row_h - 6.0 * s,
                crate::voxel::MAP_NAMES[i],
                true,
            );
            b.selected = i == lobby.map as usize;
            v.push(b);
        }
        let sbx = lx + lw + 8.0 * s;
        let track = VIS_ROWS as f64 * row_h;
        v.push(btn(
            Click::ScrollMap(-1),
            sbx,
            ly,
            26.0 * s,
            30.0 * s,
            "^",
            scroll > 0,
        ));
        v.push(btn(
            Click::ScrollMap(1),
            sbx,
            ly + track - 30.0 * s,
            26.0 * s,
            30.0 * s,
            "v",
            scroll + VIS_ROWS < count,
        ));
        v.push(btn(
            Click::CloseMap,
            mx + mw - 180.0 * s,
            my + mh - 62.0 * s,
            150.0 * s,
            44.0 * s,
            "DONE",
            true,
        ));
        v
    }

    /// A diamond (isometric) thumbnail of a world: its surface colour plus the
    /// landform that defines it (craters / seas / volcanoes) and the player (blue)
    /// vs enemy (red) starts. Oriented as a diamond to match the in-game camera.
    fn draw_diamond(ctx: &Ctx, cx: f64, cy: f64, a: f64, b: f64, idx: usize) {
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
        // Map the unit square to an iso diamond centred at (cx, cy): the four
        // square corners become top / right / bottom / left of the diamond.
        let iso = |u: f64, t: f64| (cx + (u - t) * a, cy + (u + t - 1.0) * b);
        let corners = [iso(0.0, 0.0), iso(1.0, 0.0), iso(1.0, 1.0), iso(0.0, 1.0)];
        let path = |c: &[(f64, f64); 4]| {
            ctx.begin_path();
            ctx.move_to(c[0].0, c[0].1);
            for p in &c[1..] {
                ctx.line_to(p.0, p.1);
            }
            ctx.close_path();
        };
        path(&corners);
        ctx.set_fill_style_str(&shade(1.0));
        ctx.fill();

        // Wrapping: the splitmix constant overflows u64 for any idx >= 2, which
        // panics in builds with overflow checks (it nuked the lobby in prod).
        let mut s: u64 = (idx as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(1);
        let mut rnd = || {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((s >> 33) & 0xFFFF) as f64 / 65535.0
        };
        let dot = |u: f64, t: f64, r: f64, col: &str| {
            let (px, py) = iso(u, t);
            ctx.set_fill_style_str(col);
            ctx.begin_path();
            let _ = ctx.ellipse(px, py, r, r * 0.6, 0.0, 0.0, TAU);
            ctx.fill();
        };
        let rr = a * 0.07;
        if name == "EARTH" {
            for _ in 0..6 {
                dot(
                    0.18 + rnd() * 0.64,
                    0.18 + rnd() * 0.64,
                    rr * 1.7,
                    "#2f6dab",
                );
            }
            dot(0.6, 0.42, rr, "#e8eef4");
        } else if name == "TITAN" {
            for _ in 0..4 {
                dot(
                    0.18 + rnd() * 0.64,
                    0.18 + rnd() * 0.64,
                    rr * 1.8,
                    "#23252f",
                );
            }
        } else if name == "IO" {
            for _ in 0..4 {
                let (u, t) = (0.22 + rnd() * 0.56, 0.22 + rnd() * 0.56);
                dot(u, t, rr * 1.6, &shade(1.15));
                dot(u, t, rr * 0.7, "#ec7a2c");
            }
        } else {
            let n = if matches!(name, "ENCELADUS" | "TRITON" | "PLUTO" | "MARS") {
                5
            } else {
                11
            };
            for _ in 0..n {
                let (u, t) = (0.14 + rnd() * 0.72, 0.14 + rnd() * 0.72);
                dot(u, t, rr * 1.1, &shade(1.25));
                dot(u, t, rr * 0.7, &shade(0.7));
            }
        }
        dot(0.3, 0.3, rr * 0.9, "#4aa3ff");
        dot(0.7, 0.7, rr * 0.9, "#ff5a4a");

        path(&corners);
        ctx.set_stroke_style_str("rgba(120,160,210,0.95)");
        ctx.set_line_width(1.5);
        ctx.stroke();
    }

    /// The scrollable map-select modal: a list (with scrollbar) on the left and a
    /// live diamond preview of the highlighted world on the right.
    fn draw_modal(ctx: &Ctx, lobby: &Lobby, w: f64, h: f64) {
        let s = ui_scale(w, h);
        let row_h = 40.0 * s;
        ctx.set_fill_style_str("rgba(2,4,10,0.6)");
        ctx.fill_rect(0.0, 0.0, w, h);
        let (mx, my, mw, mh) = modal_rect(w, h);
        ctx.set_fill_style_str("rgba(12,18,32,0.98)");
        ctx.fill_rect(mx, my, mw, mh);
        ctx.set_stroke_style_str("rgba(120,160,210,0.95)");
        ctx.set_line_width(2.0);
        ctx.stroke_rect(mx, my, mw, mh);

        ctx.set_text_align("left");
        ctx.set_text_baseline("alphabetic");
        ctx.set_fill_style_str("#e7eefa");
        ctx.set_font(&format!(
            "bold {}px monospace",
            (24.0 * s).round().max(12.0)
        ));
        let _ = ctx.fill_text("SELECT BATTLEFIELD", mx + 30.0 * s, my + 52.0 * s);

        for b in modal_layout(lobby, w, h) {
            draw_btn(ctx, &b, s);
            if let Click::PickMap(i) = b.click {
                let sw = crate::voxel::MAP_SWATCH[i as usize];
                ctx.set_fill_style_str(&format!("rgb({},{},{})", sw[0], sw[1], sw[2]));
                ctx.fill_rect(b.x + 8.0 * s, b.y + 7.0 * s, 18.0 * s, b.h - 14.0 * s);
            }
        }

        // Scrollbar track + thumb between the up/down buttons.
        let (lx, ly, lw) = (mx + 30.0 * s, my + 92.0 * s, 300.0 * s);
        let sbx = lx + lw + 8.0 * s;
        let track = VIS_ROWS as f64 * row_h;
        let (t0, th) = (ly + 32.0 * s, track - 64.0 * s);
        ctx.set_fill_style_str("rgba(40,52,74,0.85)");
        ctx.fill_rect(sbx, t0, 26.0 * s, th);
        let count = crate::voxel::MAP_COUNT;
        let max_scroll = count.saturating_sub(VIS_ROWS).max(1) as f64;
        let thumb_h = (th * VIS_ROWS as f64 / count as f64).max(20.0 * s);
        let thumb_y = t0 + (lobby.map_scroll as f64 / max_scroll) * (th - thumb_h);
        ctx.set_fill_style_str("rgba(130,170,220,0.95)");
        ctx.fill_rect(sbx, thumb_y, 26.0 * s, thumb_h);

        // Live preview of the highlighted world, right half of the modal.
        let mi = lobby.map as usize % count;
        let px = lx + lw + 56.0 * s;
        let pcx = (px + mx + mw - 30.0 * s) / 2.0;
        ctx.set_text_align("center");
        ctx.set_fill_style_str("#cfe0f5");
        ctx.set_font(&format!(
            "bold {}px monospace",
            (22.0 * s).round().max(11.0)
        ));
        let _ = ctx.fill_text(crate::voxel::MAP_NAMES[mi], pcx, my + 92.0 * s);
        let pa = (mx + mw - 30.0 * s - px) * 0.46;
        draw_diamond(ctx, pcx, my + mh * 0.52, pa, pa * 0.6, mi);
        ctx.set_text_align("left");
    }

    pub fn draw(screen: Screen, lobby: &Lobby, w_phys: f32, h_phys: f32) {
        let Some((ctx, w, h, _)) = ctx(w_phys as f64, h_phys as f64) else {
            return;
        };
        ctx.clear_rect(0.0, 0.0, w, h);
        // Opaque backdrop: the match hasn't started (no map chosen yet), so the
        // front-end fully covers the scene rather than dimming it. Two dark bands
        // give a touch of depth without needing the gradient API.
        ctx.set_fill_style_str("#070b16");
        ctx.fill_rect(0.0, 0.0, w, h);
        ctx.set_fill_style_str("#0b1224");
        ctx.fill_rect(0.0, 0.0, w, h * 0.5);

        ctx.set_text_baseline("alphabetic");
        let s = ui_scale(w, h);
        let font = |bold: bool, px: f64| {
            format!(
                "{}{}px monospace",
                if bold { "bold " } else { "" },
                (px * s).round().max(10.0)
            )
        };
        match screen {
            Screen::Menu => {
                ctx.set_text_align("center");
                ctx.set_fill_style_str("#e7eefa");
                ctx.set_font(&font(true, 64.0));
                let _ = ctx.fill_text("ASTROMANCERS", w / 2.0, h * 0.30);
            }
            Screen::Lobby => {
                ctx.set_text_align("left");
                ctx.set_fill_style_str("#e7eefa");
                ctx.set_font(&font(true, 34.0));
                let _ = ctx.fill_text("SKIRMISH  -  LOBBY", 60.0 * s, 96.0 * s);
                ctx.set_fill_style_str("#9ab2d8");
                ctx.set_font(&font(true, 16.0));
                let _ = ctx.fill_text("CHOOSE YOUR FACTION", 60.0 * s, 150.0 * s);

                // Right column: player slots, bot count, map.
                let rx = w - 400.0 * s;
                ctx.set_fill_style_str("#9ab2d8");
                let _ = ctx.fill_text("PLAYERS", rx, 150.0 * s);
                ctx.set_font(&font(false, 16.0));
                ctx.set_fill_style_str("#dce6f6");
                let _ = ctx.fill_text(
                    &format!("P1  YOU   -  {}", faction_name(lobby.faction)),
                    rx,
                    188.0 * s,
                );
                let enemy = match lobby.faction {
                    Faction::Astromancer => Faction::Hollowmen,
                    Faction::Hollowmen => Faction::Astromancer,
                };
                for i in 0..lobby.bots {
                    let _ = ctx.fill_text(
                        &format!("P{}  BOT   -  {}", i + 2, faction_name(enemy)),
                        rx,
                        (212.0 + i as f64 * 22.0) * s,
                    );
                }
                ctx.set_fill_style_str("#9ab2d8");
                ctx.set_font(&font(true, 16.0));
                let _ = ctx.fill_text(&format!("BOTS: {}", lobby.bots), rx + 70.0 * s, 278.0 * s);
                let mi = lobby.map as usize % crate::voxel::MAP_COUNT;
                let map = crate::voxel::MAP_NAMES[mi];
                let _ = ctx.fill_text(
                    &format!("MAP:  {map}   ({}/{})", mi + 1, crate::voxel::MAP_COUNT),
                    rx,
                    326.0 * s,
                );
                draw_diamond(&ctx, rx + 190.0 * s, 432.0 * s, 180.0 * s, 95.0 * s, mi);
            }
            Screen::InGame => {}
        }

        for b in layout(screen, lobby, w, h) {
            draw_btn(&ctx, &b, s);
        }
        // The map-select modal draws on top of the lobby.
        if screen == Screen::Lobby && lobby.map_open {
            draw_modal(&ctx, lobby, w, h);
        }
        // Restore defaults the in-game HUD relies on.
        ctx.set_text_align("left");
        ctx.set_text_baseline("alphabetic");
    }

    pub fn hit(
        screen: Screen,
        lobby: &Lobby,
        cx_phys: f32,
        cy_phys: f32,
        w_phys: f32,
        h_phys: f32,
    ) -> Click {
        let Some((_, w, h, d)) = ctx(w_phys as f64, h_phys as f64) else {
            return Click::None;
        };
        let (cx, cy) = ((cx_phys / d) as f64, (cy_phys / d) as f64);
        let inside =
            |b: &Btn| b.enabled && cx >= b.x && cx <= b.x + b.w && cy >= b.y && cy <= b.y + b.h;

        // When the map modal is open it owns all input: a click on a control acts;
        // a click outside the panel dismisses it; a click inside is swallowed.
        if screen == Screen::Lobby && lobby.map_open {
            for b in modal_layout(lobby, w, h) {
                if inside(&b) {
                    return b.click;
                }
            }
            let (mx, my, mw, mh) = modal_rect(w, h);
            if cx < mx || cx > mx + mw || cy < my || cy > my + mh {
                return Click::CloseMap;
            }
            return Click::None;
        }

        for b in layout(screen, lobby, w, h) {
            if inside(&b) {
                return b.click;
            }
        }
        Click::None
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::{draw, hit};
