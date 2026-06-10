//! HUD overlay (web): selection box + live highlight, health bars for selected
//! units, force counters, command bar, and a minimap. Native is a no-op.

use crate::camera::Camera;
use crate::game::Game;
use protocol::BuildingKind;
#[cfg(target_arch = "wasm32")]
use protocol::UnitKind;

/// Device pixel ratio (web); the HUD draws in CSS pixels scaled by this.
#[cfg(target_arch = "wasm32")]
fn dpr() -> f32 {
    web_sys::window()
        .map(|w| w.device_pixel_ratio() as f32)
        .filter(|d| *d > 0.5)
        .unwrap_or(1.0)
}

/// Context cursor shown in-game; the HUD draws it and the OS cursor hides.
#[derive(Clone, Copy, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub enum CursorKind {
    Select,
    AddSelect,
    Pan,
    Move,
    Harvest,
    Attack,
    Build,
    Rally,
}

/// True when a physical-pixel point sits over in-game UI (the command bar,
/// the minimap, or the build grid), so edge-panning and world cursors stand
/// down there.
#[cfg(target_arch = "wasm32")]
pub fn over_ui(game: &Game, cx: f32, cy: f32, w_phys: f32, h_phys: f32) -> bool {
    let d = dpr();
    if cy >= h_phys - 96.0 * d {
        return true; // the command bar strip
    }
    let (x0, y0, x1, y1) = minimap_rect(w_phys, h_phys);
    if cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1 {
        return true;
    }
    for (k, _) in card_actions(game).iter().enumerate() {
        let (x0, y0, x1, y1) = card_button_rect(game, k, h_phys);
        if cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1 {
            return true;
        }
    }
    false
}

// --- icon art -------------------------------------------------------------
// Pre-rendered mesh portraits (assets/icons; regenerate with
// `cargo test -p client render_unit_icons -- --ignored`). Indexed by
// [`Icon`]; decoded once into offscreen canvases and drawn with drawImage.
#[cfg(target_arch = "wasm32")]
const ICON_PNGS: [&[u8]; 10] = [
    include_bytes!("../../../assets/icons/hq-astromancer.png"),
    include_bytes!("../../../assets/icons/hq-hollowmen.png"),
    include_bytes!("../../../assets/icons/barracks-astromancer.png"),
    include_bytes!("../../../assets/icons/barracks-hollowmen.png"),
    include_bytes!("../../../assets/icons/turret.png"),
    include_bytes!("../../../assets/icons/supply.png"),
    include_bytes!("../../../assets/icons/worker-acolyte.png"),
    include_bytes!("../../../assets/icons/worker-engineer.png"),
    include_bytes!("../../../assets/icons/infantry.png"),
    include_bytes!("../../../assets/icons/heavy.png"),
];

#[cfg(target_arch = "wasm32")]
fn icon_canvas(idx: usize) -> Option<web_sys::HtmlCanvasElement> {
    use wasm_bindgen::JsCast;
    thread_local! {
        static CACHE: std::cell::RefCell<std::collections::HashMap<usize, web_sys::HtmlCanvasElement>> =
            std::cell::RefCell::new(std::collections::HashMap::new());
    }
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(c) = cache.get(&idx) {
            return Some(c.clone());
        }
        let img = image::load_from_memory(ICON_PNGS[idx]).ok()?;
        let rgba = img.to_rgba8();
        let (iw, ih) = (rgba.width(), rgba.height());
        let doc = web_sys::window()?.document()?;
        let canvas = doc
            .create_element("canvas")
            .ok()?
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .ok()?;
        canvas.set_width(iw);
        canvas.set_height(ih);
        let cctx = canvas
            .get_context("2d")
            .ok()??
            .dyn_into::<web_sys::CanvasRenderingContext2d>()
            .ok()?;
        let data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(&rgba.into_raw()),
            iw,
            ih,
        )
        .ok()?;
        cctx.put_image_data(&data, 0.0, 0.0).ok()?;
        cache.insert(idx, canvas.clone());
        Some(canvas)
    })
}

/// Icon index for a command-card action (faction-aware).
#[cfg(target_arch = "wasm32")]
fn action_icon(game: &Game, action: CardAction) -> usize {
    let astro = game.player_is_astromancer();
    match action {
        CardAction::Build(BuildingKind::Hq) => {
            if astro {
                0
            } else {
                1
            }
        }
        CardAction::Build(BuildingKind::Barracks) => {
            if astro {
                2
            } else {
                3
            }
        }
        CardAction::Build(BuildingKind::Turret) => 4,
        CardAction::Build(BuildingKind::Supply) => 5,
        CardAction::Train(UnitKind::Worker) => {
            if astro {
                6
            } else {
                7
            }
        }
        CardAction::Train(UnitKind::Infantry) => 8,
        CardAction::Train(UnitKind::Heavy) => 9,
    }
}

/// Icon index for a selected entity's portrait.
#[cfg(target_arch = "wasm32")]
fn info_icon(kind: sim::Kind, astro: bool) -> usize {
    match kind {
        sim::Kind::Hq => {
            if astro {
                0
            } else {
                1
            }
        }
        sim::Kind::Barracks => {
            if astro {
                2
            } else {
                3
            }
        }
        sim::Kind::Turret => 4,
        sim::Kind::Supply => 5,
        sim::Kind::Worker => {
            if astro {
                6
            } else {
                7
            }
        }
        sim::Kind::Heavy => 9,
        _ => 8,
    }
}

/// Draw icon `idx` fitted into the given CSS-pixel box.
#[cfg(target_arch = "wasm32")]
fn draw_icon(ctx: &web_sys::CanvasRenderingContext2d, idx: usize, x: f64, y: f64, s: f64) {
    if let Some(src) = icon_canvas(idx) {
        let _ = ctx.draw_image_with_html_canvas_element_and_dw_and_dh(&src, x, y, s, s);
    }
}

// --- resource glyphs -------------------------------------------------------
/// An ore crystal (faceted diamond), ~16 px wide, centred vertically on `y`.
#[cfg(target_arch = "wasm32")]
fn draw_ore_glyph(ctx: &web_sys::CanvasRenderingContext2d, x: f64, y: f64) {
    ctx.begin_path();
    ctx.move_to(x + 6.0, y - 8.0);
    ctx.line_to(x + 12.0, y);
    ctx.line_to(x + 6.0, y + 8.0);
    ctx.line_to(x, y);
    ctx.close_path();
    ctx.set_fill_style_str("#69c8e8");
    ctx.fill();
    ctx.begin_path();
    ctx.move_to(x + 6.0, y - 8.0);
    ctx.line_to(x + 12.0, y);
    ctx.line_to(x + 6.0, y);
    ctx.close_path();
    ctx.set_fill_style_str("#c4eefb");
    ctx.fill();
}

/// A carbon geyser puff (green clouds over a dark vent), matching the
/// top-bar readout.
#[cfg(target_arch = "wasm32")]
fn draw_carbon_glyph(ctx: &web_sys::CanvasRenderingContext2d, x: f64, y: f64) {
    ctx.set_fill_style_str("#3a4540");
    ctx.fill_rect(x + 3.0, y + 3.0, 6.0, 5.0);
    ctx.set_fill_style_str("#5ad97c");
    ctx.begin_path();
    let _ = ctx.arc(x + 6.0, y, 5.0, 0.0, std::f64::consts::TAU);
    ctx.fill();
    ctx.set_fill_style_str("#a9f3bd");
    ctx.begin_path();
    let _ = ctx.arc(x + 4.0, y - 3.0, 3.0, 0.0, std::f64::consts::TAU);
    ctx.fill();
}

/// A command-card button's effect when clicked.
#[cfg(target_arch = "wasm32")]
#[derive(Clone, Copy, PartialEq)]
pub enum CardAction {
    Train(UnitKind),
    Build(BuildingKind),
}

/// The command card for the current selection: one entry per button, as
/// `(action, label, hotkey, ore cost, carbon cost)`. A production building
/// offers its trainable units; selected workers offer the construction kit.
/// Shared by the renderer and the click hit-test so they can never disagree.
#[cfg(target_arch = "wasm32")]
pub fn card_actions(game: &Game) -> Vec<(CardAction, &'static str, &'static str, i64, i64)> {
    let mut out = Vec::new();
    if game.selected_hq().is_some() {
        out.push((CardAction::Train(UnitKind::Worker), "Worker", "T", 40, 0));
    } else if game.selected_barracks().is_some() {
        out.push((
            CardAction::Train(UnitKind::Infantry),
            "Infantry",
            "T",
            50,
            0,
        ));
        out.push((CardAction::Train(UnitKind::Heavy), "Heavy", "H", 120, 60));
    }
    if game.has_worker_selected() {
        out.push((
            CardAction::Build(BuildingKind::Barracks),
            "Barracks",
            "B",
            150,
            0,
        ));
        out.push((
            CardAction::Build(BuildingKind::Turret),
            "Turret",
            "V",
            90,
            50,
        ));
        out.push((
            CardAction::Build(BuildingKind::Supply),
            "Depot",
            "G",
            100,
            0,
        ));
        out.push((CardAction::Build(BuildingKind::Hq), "HQ", "N", 400, 0));
    }
    out
}

/// Square icon buttons: training options sit in the bottom command bar
/// (left side); construction options stack in a 2-column grid on the left
/// edge of the screen.
#[cfg(target_arch = "wasm32")]
const BTN: f64 = 56.0;
#[cfg(target_arch = "wasm32")]
const BTN_GAP: f64 = 8.0;

#[cfg(target_arch = "wasm32")]
fn train_btn_css(idx: usize, h_css: f32) -> (f64, f64, f64, f64) {
    (
        14.0 + idx as f64 * (BTN + BTN_GAP),
        h_css as f64 - 96.0 + 20.0,
        BTN,
        BTN,
    )
}

#[cfg(target_arch = "wasm32")]
fn build_btn_css(idx: usize, h_css: f32) -> (f64, f64, f64, f64) {
    let col = (idx % 2) as f64;
    let row = (idx / 2) as f64;
    (
        14.0 + col * (BTN + BTN_GAP),
        h_css as f64 * 0.5 - 70.0 + row * (BTN + BTN_GAP),
        BTN,
        BTN,
    )
}

/// Command-card button `k`'s rect in CSS pixels `(x, y, w, h)`: trains count
/// along the bar, builds count down the left grid.
#[cfg(target_arch = "wasm32")]
fn card_btn_css(game: &Game, k: usize, h_css: f32) -> (f64, f64, f64, f64) {
    let actions = card_actions(game);
    let builds_before = actions[..k]
        .iter()
        .filter(|a| matches!(a.0, CardAction::Build(_)))
        .count();
    match actions[k].0 {
        CardAction::Train(_) => train_btn_css(k - builds_before, h_css),
        CardAction::Build(_) => build_btn_css(builds_before, h_css),
    }
}

/// Command-card button `k`'s rect in physical pixels `(x0, y0, x1, y1)`, for
/// hit-testing against raw cursor/touch coordinates.
#[cfg(target_arch = "wasm32")]
pub fn card_button_rect(game: &Game, k: usize, h_phys: f32) -> (f32, f32, f32, f32) {
    let d = dpr();
    let (x, y, bw, bh) = card_btn_css(game, k, h_phys / d);
    (
        x as f32 * d,
        y as f32 * d,
        (x + bw) as f32 * d,
        (y + bh) as f32 * d,
    )
}

/// Resume button rect in CSS pixels `(x, y, w, h)`, centred under the PAUSED
/// title. Single source of truth for the draw and the hit-test.
#[cfg(target_arch = "wasm32")]
fn resume_btn_css(w_css: f32, h_css: f32) -> (f64, f64, f64, f64) {
    let bw = 200.0_f64;
    let bh = 48.0_f64;
    let bx = (w_css as f64 - bw) / 2.0;
    // Sits so the title + two buttons + hint stack reads centered in the
    // pause panel (panel top is h/2 - 155).
    let by = h_css as f64 / 2.0 - 40.0;
    (bx, by, bw, bh)
}

/// Fullscreen button rect in CSS pixels, directly under the Resume button.
#[cfg(target_arch = "wasm32")]
fn fullscreen_btn_css(w_css: f32, h_css: f32) -> (f64, f64, f64, f64) {
    let (bx, by, bw, bh) = resume_btn_css(w_css, h_css);
    (bx, by + bh + 14.0, bw, bh)
}

#[cfg(target_arch = "wasm32")]
fn css_to_phys(r: (f64, f64, f64, f64), d: f32) -> (f32, f32, f32, f32) {
    (
        r.0 as f32 * d,
        r.1 as f32 * d,
        (r.0 + r.2) as f32 * d,
        (r.1 + r.3) as f32 * d,
    )
}

/// Resume button rect in physical pixels `(x0, y0, x1, y1)`, for hit-testing the
/// pause menu against raw cursor coordinates.
#[cfg(target_arch = "wasm32")]
pub fn resume_button_rect(w_phys: f32, h_phys: f32) -> (f32, f32, f32, f32) {
    let d = dpr();
    css_to_phys(resume_btn_css(w_phys / d, h_phys / d), d)
}

/// Fullscreen button rect in physical pixels, for the pause-menu hit-test.
#[cfg(target_arch = "wasm32")]
pub fn fullscreen_button_rect(w_phys: f32, h_phys: f32) -> (f32, f32, f32, f32) {
    let d = dpr();
    css_to_phys(fullscreen_btn_css(w_phys / d, h_phys / d), d)
}

/// Draw the context cursor at `(x, y)` in CSS pixels: an arrowhead for
/// selection, plus order-specific glyphs (move/harvest/attack/build/rally/
/// pan), each with a dark outline so it reads over terrain and water.
#[cfg(target_arch = "wasm32")]
fn draw_cursor_glyph(ctx: &web_sys::CanvasRenderingContext2d, x: f64, y: f64, kind: CursorKind) {
    use std::f64::consts::TAU;
    ctx.save();
    ctx.set_line_width(2.0);
    let outline = |ctx: &web_sys::CanvasRenderingContext2d| {
        ctx.set_stroke_style_str("rgba(0,0,0,0.85)");
        ctx.set_line_width(3.5);
        ctx.stroke();
        ctx.set_line_width(2.0);
    };
    match kind {
        CursorKind::Select | CursorKind::AddSelect => {
            draw_cursor_arrow(ctx, x, y);
            if kind == CursorKind::AddSelect {
                // a small green plus beside the arrow (shift: add to selection)
                ctx.begin_path();
                ctx.move_to(x + 14.0, y + 4.0);
                ctx.line_to(x + 22.0, y + 4.0);
                ctx.move_to(x + 18.0, y);
                ctx.line_to(x + 18.0, y + 8.0);
                outline(ctx);
                ctx.set_stroke_style_str("#7dff9a");
                ctx.stroke();
            }
        }
        CursorKind::Pan => {
            // four-way pan arrows
            ctx.begin_path();
            for (dx, dy) in [(0.0, -10.0), (0.0, 10.0), (-10.0, 0.0), (10.0, 0.0)] {
                ctx.move_to(x, y);
                ctx.line_to(x + dx, y + dy);
                let (px, py) = (x + dx, y + dy);
                let (ax, ay) = (dx.signum() * 4.0, dy.signum() * 4.0);
                ctx.move_to(px - ay - ax, py - ax - ay);
                ctx.line_to(px, py);
                ctx.line_to(px + ay - ax, py + ax - ay);
            }
            outline(ctx);
            ctx.set_stroke_style_str("#eaf2ff");
            ctx.stroke();
        }
        CursorKind::Move => {
            ctx.begin_path();
            ctx.move_to(x, y - 8.0);
            ctx.line_to(x + 8.0, y);
            ctx.line_to(x, y + 8.0);
            ctx.line_to(x - 8.0, y);
            ctx.close_path();
            outline(ctx);
            ctx.set_stroke_style_str("#7dff9a");
            ctx.stroke();
            ctx.set_fill_style_str("#7dff9a");
            ctx.begin_path();
            let _ = ctx.arc(x, y, 2.0, 0.0, TAU);
            ctx.fill();
        }
        CursorKind::Harvest => {
            // a cyan crystal
            ctx.begin_path();
            ctx.move_to(x, y - 9.0);
            ctx.line_to(x + 6.0, y - 2.0);
            ctx.line_to(x + 3.0, y + 7.0);
            ctx.line_to(x - 3.0, y + 7.0);
            ctx.line_to(x - 6.0, y - 2.0);
            ctx.close_path();
            outline(ctx);
            ctx.set_fill_style_str("rgba(102,217,232,0.9)");
            ctx.fill();
            ctx.set_stroke_style_str("#bdeefc");
            ctx.stroke();
        }
        CursorKind::Attack => {
            // a red crosshair
            ctx.begin_path();
            let _ = ctx.arc(x, y, 7.0, 0.0, TAU);
            for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                ctx.move_to(x + dx * 4.0, y + dy * 4.0);
                ctx.line_to(x + dx * 11.0, y + dy * 11.0);
            }
            outline(ctx);
            ctx.set_stroke_style_str("#ff5a4a");
            ctx.stroke();
        }
        CursorKind::Build => {
            // a green footprint square with a plus
            ctx.begin_path();
            ctx.rect(x - 7.0, y - 7.0, 14.0, 14.0);
            ctx.move_to(x - 4.0, y);
            ctx.line_to(x + 4.0, y);
            ctx.move_to(x, y - 4.0);
            ctx.line_to(x, y + 4.0);
            outline(ctx);
            ctx.set_stroke_style_str("#7dff9a");
            ctx.stroke();
        }
        CursorKind::Rally => {
            // an amber flag
            ctx.begin_path();
            ctx.move_to(x, y + 8.0);
            ctx.line_to(x, y - 9.0);
            outline(ctx);
            ctx.set_stroke_style_str("#ffd36b");
            ctx.stroke();
            ctx.begin_path();
            ctx.move_to(x, y - 9.0);
            ctx.line_to(x + 10.0, y - 5.5);
            ctx.line_to(x, y - 2.0);
            ctx.close_path();
            ctx.set_fill_style_str("#ffd36b");
            ctx.fill();
        }
    }
    ctx.restore();
}

/// The selection arrowhead (also the base of the Select cursor).
#[cfg(target_arch = "wasm32")]
fn draw_cursor_arrow(ctx: &web_sys::CanvasRenderingContext2d, x: f64, y: f64) {
    ctx.save();
    ctx.begin_path();
    ctx.move_to(x, y);
    ctx.line_to(x, y + 16.0);
    ctx.line_to(x + 4.5, y + 12.0);
    ctx.line_to(x + 8.0, y + 19.0);
    ctx.line_to(x + 11.0, y + 17.5);
    ctx.line_to(x + 7.5, y + 10.5);
    ctx.line_to(x + 13.0, y + 10.5);
    ctx.close_path();
    ctx.set_fill_style_str("#f4f7ff");
    ctx.fill();
    ctx.set_stroke_style_str("rgba(0,0,0,0.85)");
    ctx.set_line_width(1.2);
    ctx.stroke();
    ctx.restore();
}

/// The pause overlay: a dimmed screen, a centred panel, and a Resume button.
#[cfg(target_arch = "wasm32")]
fn draw_pause(ctx: &web_sys::CanvasRenderingContext2d, w: f32, h: f32, cursor: (f64, f64)) {
    let (wf, hf) = (w as f64, h as f64);
    ctx.save();
    // Dim the whole scene.
    ctx.set_fill_style_str("rgba(4,8,16,0.72)");
    ctx.fill_rect(0.0, 0.0, wf, hf);
    // Panel (tall enough for both buttons + hint).
    let pw = 360.0_f64;
    let ph = 270.0_f64;
    let px = (wf - pw) / 2.0;
    let py = hf / 2.0 - ph / 2.0 - 10.0;
    ctx.set_fill_style_str("rgba(10,16,30,0.96)");
    ctx.fill_rect(px, py, pw, ph);
    ctx.set_stroke_style_str("rgba(120,160,210,0.95)");
    ctx.set_line_width(2.0);
    ctx.stroke_rect(px, py, pw, ph);
    // Title.
    ctx.set_text_align("center");
    ctx.set_fill_style_str("#e7eefa");
    ctx.set_font("bold 30px monospace");
    let _ = ctx.fill_text("PAUSED", wf / 2.0, py + 60.0);
    // Resume + Fullscreen buttons (hover-reactive).
    let button = |rect: (f64, f64, f64, f64), label: &str| {
        let (bx, by, bw, bh) = rect;
        let hover = cursor.0 >= bx && cursor.0 <= bx + bw && cursor.1 >= by && cursor.1 <= by + bh;
        ctx.set_fill_style_str(if hover {
            "rgba(58,110,185,0.97)"
        } else {
            "rgba(40,80,140,0.95)"
        });
        ctx.fill_rect(bx, by, bw, bh);
        ctx.set_stroke_style_str(if hover {
            "rgba(210,235,255,1.0)"
        } else {
            "rgba(150,190,240,0.95)"
        });
        ctx.set_line_width(if hover { 2.5 } else { 1.5 });
        ctx.stroke_rect(bx, by, bw, bh);
        ctx.set_fill_style_str("#eaf2ff");
        ctx.set_font("bold 18px monospace");
        let _ = ctx.fill_text(label, bx + bw / 2.0, by + 31.0);
    };
    button(resume_btn_css(w, h), "Resume");
    button(fullscreen_btn_css(w, h), "Fullscreen");
    // Hint.
    let (_, fy, _, fh) = fullscreen_btn_css(w, h);
    ctx.set_fill_style_str("#8aa3cc");
    ctx.set_font("13px monospace");
    let _ = ctx.fill_text("Press Esc to resume", wf / 2.0, fy + fh + 28.0);
    ctx.restore();
}

/// The match verdict overlay: VICTORY or DEFEAT over a dimmed scene, with the
/// pause panel's button slot leading back to the menu.
#[cfg(target_arch = "wasm32")]
fn draw_match_end(
    ctx: &web_sys::CanvasRenderingContext2d,
    w: f32,
    h: f32,
    win: bool,
    cursor: (f64, f64),
) {
    let (wf, hf) = (w as f64, h as f64);
    ctx.save();
    ctx.set_fill_style_str("rgba(4,8,16,0.78)");
    ctx.fill_rect(0.0, 0.0, wf, hf);
    let pw = 380.0_f64;
    let ph = 230.0_f64;
    let px = (wf - pw) / 2.0;
    let py = hf / 2.0 - ph / 2.0 - 10.0;
    ctx.set_fill_style_str("rgba(10,16,30,0.96)");
    ctx.fill_rect(px, py, pw, ph);
    ctx.set_stroke_style_str(if win {
        "rgba(140,255,170,0.95)"
    } else {
        "rgba(255,120,100,0.95)"
    });
    ctx.set_line_width(2.0);
    ctx.stroke_rect(px, py, pw, ph);
    ctx.set_text_align("center");
    ctx.set_fill_style_str(if win { "#9dffb4" } else { "#ff8a76" });
    ctx.set_font("bold 34px monospace");
    let _ = ctx.fill_text(if win { "VICTORY" } else { "DEFEAT" }, wf / 2.0, py + 56.0);
    // One button, in the pause panel's Resume slot (shared hit-test).
    let (bx, by, bw, bh) = resume_btn_css(w, h);
    let hover = cursor.0 >= bx && cursor.0 <= bx + bw && cursor.1 >= by && cursor.1 <= by + bh;
    ctx.set_fill_style_str(if hover {
        "rgba(58,110,185,0.97)"
    } else {
        "rgba(40,80,140,0.95)"
    });
    ctx.fill_rect(bx, by, bw, bh);
    ctx.set_stroke_style_str(if hover {
        "rgba(210,235,255,1.0)"
    } else {
        "rgba(150,190,240,0.95)"
    });
    ctx.set_line_width(if hover { 2.5 } else { 1.5 });
    ctx.stroke_rect(bx, by, bw, bh);
    ctx.set_fill_style_str("#eaf2ff");
    ctx.set_font("bold 18px monospace");
    let _ = ctx.fill_text("Return to Menu", bx + bw / 2.0, by + 31.0);
    ctx.set_text_align("left");
    ctx.restore();
}

/// Minimap panel geometry in CSS pixels `(mx, my, mm)`: its own square box tucked
/// into the very bottom-right corner of the screen (over the command bar). Single
/// source of truth for both the draw and the hit-test.
#[cfg(target_arch = "wasm32")]
fn minimap_css(w: f32, h: f32) -> (f64, f64, f64) {
    let mm = 160.0_f64; // 2x the old 80px panel
    let margin = 12.0_f64;
    let mx = w as f64 - mm - margin;
    let my = h as f64 - mm - margin;
    (mx, my, mm)
}

/// Minimap rect in physical pixels `(x0, y0, x1, y1)`, for click/drag-to-look.
#[cfg(target_arch = "wasm32")]
pub fn minimap_rect(w_phys: f32, h_phys: f32) -> (f32, f32, f32, f32) {
    let d = dpr();
    let (mx, my, mm) = minimap_css(w_phys / d, h_phys / d);
    (
        mx as f32 * d,
        my as f32 * d,
        (mx + mm) as f32 * d,
        (my + mm) as f32 * d,
    )
}

// Cached terrain thumbnail for the minimap (built once; terrain is static).
#[cfg(target_arch = "wasm32")]
const MINI_N: usize = 96;
#[cfg(target_arch = "wasm32")]
thread_local! {
    static MINI_TERRAIN: std::cell::OnceCell<Vec<[u8; 3]>> = const { std::cell::OnceCell::new() };
}
#[cfg(target_arch = "wasm32")]
fn mini_color(h: f32) -> [u8; 3] {
    let sea = crate::terrain::SEA_LEVEL;
    if h < sea {
        let d = ((sea - h) / 10.0).clamp(0.0, 1.0);
        let s = 1.0 - d * 0.45;
        [(30.0 * s) as u8, (74.0 * s) as u8, (116.0 * s) as u8]
    } else if h < 2.5 {
        [176, 166, 120]
    } else if h < 18.0 {
        let g = (h / 18.0).clamp(0.0, 1.0);
        [
            (74.0 - 22.0 * g) as u8,
            (112.0 - 34.0 * g) as u8,
            (58.0 - 16.0 * g) as u8,
        ]
    } else {
        [120, 116, 110]
    }
}
#[cfg(target_arch = "wasm32")]
fn build_mini_terrain() -> Vec<[u8; 3]> {
    let half = crate::terrain::HALF;
    let mut out = vec![[0u8; 3]; MINI_N * MINI_N];
    for iz in 0..MINI_N {
        for ix in 0..MINI_N {
            let x = -half + (ix as f32 + 0.5) / MINI_N as f32 * 2.0 * half;
            let z = -half + (iz as f32 + 0.5) / MINI_N as f32 * 2.0 * half;
            out[iz * MINI_N + ix] = mini_color(crate::terrain::height(x, z));
        }
    }
    out
}
/// Composite the terrain thumbnail with fog into the minimap rect (device px).
///
/// The thumbnail is rotated by the camera yaw and masked to a diamond: each
/// output pixel is mapped back through the inverse rotation to a world sample,
/// and pixels outside the diamond get the dark bezel colour instead. Because the
/// world is a square and the yaw is 45 degrees, the rotated map fills a diamond
/// whose corners are the map corners. Rotating here (rather than via a canvas
/// transform) is required because `put_image_data` ignores the context transform
/// - it writes raw pixels.
#[cfg(target_arch = "wasm32")]
fn draw_minimap(
    ctx: &web_sys::CanvasRenderingContext2d,
    game: &Game,
    dpr: f32,
    mx: f64,
    my: f64,
    mm: f64,
) {
    let dev = (mm * dpr as f64).round().max(1.0) as usize;
    let r_dev = dev as f32 / 2.0;
    let (s, c) = crate::camera::YAW.sin_cos();
    const SQRT2: f32 = std::f32::consts::SQRT_2;
    let mut img = vec![0u8; dev * dev * 4];
    MINI_TERRAIN.with(|cell| {
        let terr = cell.get_or_init(build_mini_terrain);
        for py in 0..dev {
            for px in 0..dev {
                let o = (py * dev + px) * 4;
                // Minimap-local coords in [-1, 1]: u right, v down from centre.
                let u = (px as f32 + 0.5 - r_dev) / r_dev;
                let v = (py as f32 + 0.5 - r_dev) / r_dev;
                if u.abs() + v.abs() > 1.0 {
                    // Bezel outside the diamond (matches the panel background).
                    img[o] = 8;
                    img[o + 1] = 14;
                    img[o + 2] = 26;
                    img[o + 3] = 255;
                    continue;
                }
                // Inverse-rotate the minimap pixel back to a world sample. The
                // diamond's edge midpoints (|u|+|v|=1) are the map edge centres,
                // so scale by sqrt(2) before un-rotating.
                let du = u * SQRT2;
                let dv = v * SQRT2;
                let cxw = du * s + dv * c;
                let czw = -du * c + dv * s;
                let nx = ((cxw + 1.0) * 0.5).clamp(0.0, 1.0);
                let nz = ((czw + 1.0) * 0.5).clamp(0.0, 1.0);
                let ix = ((nx * MINI_N as f32) as usize).min(MINI_N - 1);
                let iz = ((nz * MINI_N as f32) as usize).min(MINI_N - 1);
                let [r, g, b] = terr[iz * MINI_N + ix];
                let bri = game.fog_brightness(nx, nz);
                img[o] = (r as f32 * bri) as u8;
                img[o + 1] = (g as f32 * bri) as u8;
                img[o + 2] = (b as f32 * bri) as u8;
                img[o + 3] = 255;
            }
        }
    });
    if let Ok(data) = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
        wasm_bindgen::Clamped(&img),
        dev as u32,
        dev as u32,
    ) {
        let _ = ctx.put_image_data(&data, (mx * dpr as f64).round(), (my * dpr as f64).round());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(clippy::too_many_arguments)]
pub fn draw(
    _c: &Camera,
    _g: &Game,
    _w: f32,
    _h: f32,
    _drag: Option<(f32, f32, f32, f32)>,
    _paused: bool,
    _cursor: (f32, f32),
    _cursor_kind: Option<CursorKind>,
    _build_mode: Option<BuildingKind>,
    _card_pressed: Option<usize>,
    _outcome: Option<bool>,
) {
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
pub fn draw(
    camera: &Camera,
    game: &Game,
    w: f32,
    h: f32,
    drag: Option<(f32, f32, f32, f32)>,
    paused: bool,
    cursor: (f32, f32),
    cursor_kind: Option<CursorKind>,
    build_mode: Option<BuildingKind>,
    card_pressed: Option<usize>,
    outcome: Option<bool>,
) {
    use crate::terrain;
    use wasm_bindgen::JsCast;

    if w < 2.0 || h < 2.0 {
        return;
    }
    let Some(canvas) = web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| doc.get_element_by_id("hud"))
        .and_then(|el| el.dyn_into::<web_sys::HtmlCanvasElement>().ok())
    else {
        return;
    };
    if canvas.width() != w as u32 {
        canvas.set_width(w as u32);
    }
    if canvas.height() != h as u32 {
        canvas.set_height(h as u32);
    }
    let Ok(Some(obj)) = canvas.get_context("2d") else {
        return;
    };
    let Ok(ctx) = obj.dyn_into::<web_sys::CanvasRenderingContext2d>() else {
        return;
    };
    // The buffer is physical pixels but is shown CSS-downscaled by the device
    // pixel ratio. Draw in CSS pixels (absolute transform, so it doesn't
    // compound across frames) so the HUD stays readable on high-DPI phones.
    let dpr = dpr();
    let _ = ctx.set_transform(dpr as f64, 0.0, 0.0, dpr as f64, 0.0, 0.0);
    let (w, h) = (w / dpr, h / dpr);
    let (wf, hf) = (w as f64, h as f64);
    ctx.clear_rect(0.0, 0.0, wf, hf);

    // Health bars: selected entities, plus every visible damaged building
    // (so a base under fire reads at a glance). Building bars scale with the
    // footprint so a depot's bar is visibly a building's, not a unit's.
    // (Names and numeric HP for the selection live in the command bar's
    // selection panel; the world only shows the bars themselves.)
    let health_bar = |u: &crate::game::UnitInfo| {
        let Some((sx, sy)) = camera.project(glam::Vec3::new(u.wx, u.wy, u.wz), w, h) else {
            return;
        };
        let bw = if u.barracks {
            (u.radius as f64 * 6.5).clamp(30.0, 64.0)
        } else {
            22.0
        };
        let bh = if u.barracks { 5.0 } else { 4.0 };
        let x = sx as f64 - bw / 2.0;
        let y = sy as f64;
        ctx.set_fill_style_str("rgba(0,0,0,0.65)");
        ctx.fill_rect(x - 1.0, y - 1.0, bw + 2.0, bh + 2.0);
        ctx.set_fill_style_str(if u.owner == 0 { "#39d35a" } else { "#e0473a" });
        ctx.fill_rect(x, y, bw * u.hp_frac.clamp(0.0, 1.0) as f64, bh);
    };
    for u in game.selected_infos() {
        health_bar(&u);
    }
    for u in game.damaged_buildings() {
        health_bar(&u);
    }

    // Selected buildings: screen-space corner brackets sized to the projected
    // footprint. Pure HUD drawing, so unlike a ground decal they can never
    // clip into slopes.
    for u in game.selected_infos() {
        if !u.barracks {
            continue;
        }
        let ground = u.wy - 7.0;
        let r = u.radius;
        // Project the footprint's four ground corners and take the screen
        // bounding box (the camera is fixed-yaw, so this stays snug).
        let mut x0 = f32::MAX;
        let mut x1 = f32::MIN;
        let mut y0 = f32::MAX;
        let mut y1 = f32::MIN;
        let mut ok = true;
        for (cx, cz) in [(-r, -r), (r, -r), (-r, r), (r, r)] {
            let (gx, gz) = (u.wx + cx, u.wz + cz);
            let gy = crate::terrain::height(gx, gz).max(ground);
            match camera.project(glam::Vec3::new(gx, gy, gz), w, h) {
                Some((px, py)) => {
                    x0 = x0.min(px);
                    x1 = x1.max(px);
                    y0 = y0.min(py);
                    y1 = y1.max(py);
                }
                None => ok = false,
            }
        }
        if !ok {
            continue;
        }
        let (x0, y0, x1, y1) = (x0 as f64, y0 as f64, x1 as f64, y1 as f64);
        let arm = ((x1 - x0) * 0.18).clamp(8.0, 22.0);
        ctx.set_stroke_style_str("#7dff9a");
        ctx.set_line_width(2.5);
        for (cx, cy, dx, dy) in [
            (x0, y0, 1.0, 1.0),
            (x1, y0, -1.0, 1.0),
            (x0, y1, 1.0, -1.0),
            (x1, y1, -1.0, -1.0),
        ] {
            ctx.begin_path();
            ctx.move_to(cx + dx * arm, cy);
            ctx.line_to(cx, cy);
            ctx.line_to(cx, cy + dy * arm);
            ctx.stroke();
        }
    }

    // Drag-selection box + live highlight of units inside it. The drag rect
    // arrives in physical pixels; bring it into the CSS space we draw in.
    if let Some((x0, y0, x1, y1)) = drag {
        let (x0, y0, x1, y1) = (x0 / dpr, y0 / dpr, x1 / dpr, y1 / dpr);
        for u in game.player_units() {
            if let Some((sx, sy)) = camera.project(glam::Vec3::new(u.wx, u.wy - 2.0, u.wz), w, h) {
                if sx >= x0 && sx <= x1 && sy >= y0 && sy <= y1 {
                    ctx.set_stroke_style_str("#7dff9a");
                    ctx.set_line_width(2.0);
                    ctx.begin_path();
                    let _ = ctx.arc(sx as f64, sy as f64, 11.0, 0.0, std::f64::consts::TAU);
                    ctx.stroke();
                }
            }
        }
        ctx.set_fill_style_str("rgba(90,220,120,0.12)");
        ctx.fill_rect(x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64);
        ctx.set_stroke_style_str("rgba(120,255,150,0.9)");
        ctx.set_line_width(1.5);
        ctx.stroke_rect(x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64);
    }

    // Command bar.
    let bar = 96.0_f64;
    ctx.set_fill_style_str("rgba(8,14,26,0.85)");
    ctx.fill_rect(0.0, hf - bar, wf, bar);
    ctx.set_stroke_style_str("rgba(120,160,210,0.85)");
    ctx.set_line_width(2.0);
    ctx.stroke_rect(1.0, hf - bar + 1.0, wf - 2.0, bar - 2.0);

    // Resource readout with proper icons: an ore crystal, a carbon geyser
    // puff, and a supply depot. Icons are tiny canvas paths so they ship with
    // the WASM HUD (no image assets). Per the HUD policy in CLAUDE.md this is
    // the only top-bar content: no faction labels, counters, or help text.
    ctx.set_font("bold 16px monospace");
    let icon_y = 17.0_f64;

    let ox = 16.0_f64;
    draw_ore_glyph(&ctx, ox, icon_y);
    ctx.set_fill_style_str("#e7eefa");
    let _ = ctx.fill_text(&format!("{}", game.player_ore() as i64), ox + 18.0, 24.0);

    let cx2 = 106.0_f64;
    draw_carbon_glyph(&ctx, cx2, icon_y);
    ctx.set_fill_style_str("#e7eefa");
    let _ = ctx.fill_text(
        &format!("{}", game.player_carbon() as i64),
        cx2 + 18.0,
        24.0,
    );

    // Supply: a depot glyph (box + roof); the count turns red when capped.
    let (sup_used, sup_cap) = game.player_supply();
    let sx = 196.0_f64;
    ctx.set_fill_style_str("#9fb6da");
    ctx.fill_rect(sx + 1.0, icon_y - 1.0, 10.0, 8.0);
    ctx.begin_path();
    ctx.move_to(sx - 1.0, icon_y - 1.0);
    ctx.line_to(sx + 6.0, icon_y - 7.0);
    ctx.line_to(sx + 13.0, icon_y - 1.0);
    ctx.close_path();
    ctx.fill();
    ctx.set_fill_style_str(if sup_used >= sup_cap {
        "#ff6a5e"
    } else {
        "#e7eefa"
    });
    let _ = ctx.fill_text(&format!("{sup_used}/{sup_cap}"), sx + 18.0, 24.0);

    // Selection display, centred in the command bar: a single selected
    // building shows its portrait, name and HP; selected units show a
    // portrait grid with per-unit health bars.
    {
        let infos = game.selected_infos();
        let astro = game.player_is_astromancer();
        let units: Vec<_> = infos.iter().filter(|u| !u.barracks).collect();
        if !units.is_empty() {
            let n = units.len().min(16);
            let cols = 8.min(n);
            let cell = 38.0_f64;
            let px = wf / 2.0 - cols as f64 * cell / 2.0;
            let py = hf - bar + 8.0;
            for (i, u) in units.iter().take(n).enumerate() {
                let x = px + (i % 8) as f64 * cell;
                let y = py + (i / 8) as f64 * (cell + 4.0);
                ctx.set_fill_style_str("rgba(20,30,48,0.9)");
                ctx.fill_rect(x, y, 32.0, 32.0);
                draw_icon(&ctx, info_icon(u.kind, astro), x, y, 32.0);
                ctx.set_stroke_style_str("rgba(140,180,230,0.8)");
                ctx.set_line_width(1.0);
                ctx.stroke_rect(x, y, 32.0, 32.0);
                ctx.set_fill_style_str("rgba(0,0,0,0.65)");
                ctx.fill_rect(x, y + 33.0, 32.0, 4.0);
                ctx.set_fill_style_str("#39d35a");
                ctx.fill_rect(x, y + 33.0, 32.0 * u.hp_frac.clamp(0.0, 1.0) as f64, 4.0);
            }
        } else if let Some(u) = infos.first() {
            // A building selects alone: portrait art + name + HP readout.
            let px = wf / 2.0 - 105.0;
            let py = hf - bar + 14.0;
            ctx.set_fill_style_str("rgba(20,30,48,0.9)");
            ctx.fill_rect(px, py, 66.0, 66.0);
            draw_icon(&ctx, info_icon(u.kind, astro), px + 1.0, py + 1.0, 64.0);
            ctx.set_stroke_style_str("rgba(140,180,230,0.9)");
            ctx.set_line_width(1.5);
            ctx.stroke_rect(px, py, 66.0, 66.0);
            ctx.set_fill_style_str("#e7eefa");
            ctx.set_font("bold 15px monospace");
            let _ = ctx.fill_text(u.name, px + 80.0, py + 24.0);
            ctx.set_font("13px monospace");
            ctx.set_fill_style_str(if u.hp_frac > 0.5 {
                "#9dffb4"
            } else if u.hp_frac > 0.25 {
                "#ffd36b"
            } else {
                "#ff8a76"
            });
            let _ = ctx.fill_text(&format!("HP: {}/{}", u.hp, u.hp_max), px + 80.0, py + 46.0);
        }
    }

    // Command card: icon buttons. Training options (HQ -> Worker, Barracks ->
    // Infantry/Heavy) sit in the bar; worker construction options stack in
    // the build grid on the left edge. Faces are icon + hotkey only; the
    // name and cost live in the hover tooltip to keep the card clean.
    let actions = card_actions(game);
    let mut tooltip: Option<(f64, f64, f64, &str, i64, i64)> = None;
    if !actions.is_empty() {
        let prod = game.selected_production();
        let ore_have = game.player_ore() as i64;
        let carbon_have = game.player_carbon() as i64;
        let mut prod_shown = false;
        let dc = dpr as f64;
        for (k, &(action, label, hotkey, ore, carbon)) in actions.iter().enumerate() {
            let (bx, by, bw, bh) = card_btn_css(game, k, h);
            let afford = ore_have >= ore && carbon_have >= carbon;
            // The button whose building is being placed right now glows green.
            let active = matches!((action, build_mode), (CardAction::Build(b), Some(m)) if b == m);
            // Hover lifts the button; a just-clicked button flashes bright.
            let (ccx, ccy) = (cursor.0 as f64 / dc, cursor.1 as f64 / dc);
            let hover = ccx >= bx && ccx <= bx + bw && ccy >= by && ccy <= by + bh;
            let pressed = card_pressed == Some(k);
            ctx.set_fill_style_str(if pressed {
                "rgba(180,220,255,0.95)"
            } else if active {
                if hover {
                    "rgba(65,140,78,0.95)"
                } else {
                    "rgba(50,110,60,0.95)"
                }
            } else if afford {
                if hover {
                    "rgba(58,110,185,0.97)"
                } else {
                    "rgba(40,80,140,0.95)"
                }
            } else if hover {
                "rgba(62,70,86,0.95)"
            } else {
                "rgba(48,54,66,0.92)"
            });
            ctx.fill_rect(bx, by, bw, bh);
            draw_icon(
                &ctx,
                action_icon(game, action),
                bx + 4.0,
                by + 2.0,
                bw - 8.0,
            );
            if !afford {
                // Unaffordable: dim the art.
                ctx.set_fill_style_str("rgba(20,24,32,0.55)");
                ctx.fill_rect(bx, by, bw, bh);
            }
            ctx.set_stroke_style_str(if pressed {
                "rgba(255,255,255,1.0)"
            } else if active {
                "rgba(150,255,170,0.95)"
            } else if hover {
                "rgba(210,235,255,1.0)"
            } else {
                "rgba(150,190,240,0.95)"
            });
            ctx.set_line_width(if hover || pressed { 2.5 } else { 1.5 });
            ctx.stroke_rect(bx, by, bw, bh);
            // Hotkey tag in the corner (the only on-button text).
            ctx.set_font("bold 11px monospace");
            ctx.set_fill_style_str("rgba(0,0,0,0.6)");
            ctx.fill_rect(bx + bw - 15.0, by + bh - 15.0, 13.0, 13.0);
            ctx.set_fill_style_str("#cfe2ff");
            let _ = ctx.fill_text(hotkey, bx + bw - 12.0, by + bh - 4.0);
            // The first train button carries the building's queue + progress.
            if let (CardAction::Train(_), Some((queued, frac)), false) = (action, prod, prod_shown)
            {
                prod_shown = true;
                ctx.set_font("bold 11px monospace");
                ctx.set_fill_style_str("#ffd36b");
                let _ = ctx.fill_text(&format!("{queued}"), bx + 3.0, by + 13.0);
                if frac > 0.0 {
                    ctx.set_fill_style_str("rgba(255,211,107,0.95)");
                    ctx.fill_rect(bx, by + bh - 3.0, bw * frac.clamp(0.0, 1.0) as f64, 3.0);
                }
            }
            if hover {
                tooltip = Some((bx, by, bw, label, ore, carbon));
            }
        }
    }

    // Minimap: a diamond radar in a framed panel, tucked into the bottom-right
    // corner. The whole map is rotated by the camera yaw (45 degrees) so the view
    // box reads upright (the direction you are looking points up) and the square
    // world reads as a diamond.
    let (mx, my, mm) = minimap_css(w, h);
    let pad = 6.0_f64;
    let mcx = mx + mm / 2.0;
    let mcy = my + mm / 2.0;
    let rad = mm / 2.0;
    // Diamond vertices (top, right, bottom, left) at the panel edge midpoints.
    let diamond = [
        (mcx, mcy - rad),
        (mcx + rad, mcy),
        (mcx, mcy + rad),
        (mcx - rad, mcy),
    ];
    let path_diamond = |ctx: &web_sys::CanvasRenderingContext2d| {
        ctx.begin_path();
        ctx.move_to(diamond[0].0, diamond[0].1);
        ctx.line_to(diamond[1].0, diamond[1].1);
        ctx.line_to(diamond[2].0, diamond[2].1);
        ctx.line_to(diamond[3].0, diamond[3].1);
        ctx.close_path();
    };
    ctx.set_fill_style_str("rgba(8,14,26,0.92)");
    ctx.fill_rect(mx - pad, my - pad, mm + pad * 2.0, mm + pad * 2.0);
    draw_minimap(&ctx, game, dpr, mx, my, mm);
    // Square housing frame, then the diamond rim.
    ctx.set_stroke_style_str("rgba(120,160,210,0.95)");
    ctx.set_line_width(2.0);
    ctx.stroke_rect(mx - pad, my - pad, mm + pad * 2.0, mm + pad * 2.0);
    path_diamond(&ctx);
    ctx.stroke();

    // World -> minimap (rotated by the camera yaw), then centre + scale into the
    // diamond. The same rotation is applied to the terrain, units, and view box,
    // so they stay registered. `(s, c)` is sin/cos of the yaw; the sqrt(2) scale
    // puts the map corners on the diamond's points.
    let (sy, cy) = crate::camera::YAW.sin_cos();
    let scale = rad / std::f64::consts::SQRT_2;
    let half = terrain::HALF as f64;
    let to_diamond = |wx: f32, wz: f32| -> (f64, f64) {
        let nx = wx as f64 / half;
        let nz = wz as f64 / half;
        let du = nx * sy as f64 - nz * cy as f64;
        let dv = nx * cy as f64 + nz * sy as f64;
        (mcx + du * scale, mcy + dv * scale)
    };

    // Clip everything that follows to the diamond, so nothing spills onto the
    // bezel and the view box is cleanly clipped instead of distorted.
    ctx.save();
    path_diamond(&ctx);
    ctx.clip();

    for u in game.unit_infos() {
        let (px, py) = to_diamond(u.wx, u.wz);
        ctx.set_fill_style_str(match (u.owner, u.barracks) {
            (0, true) => "#9fdcff",
            (0, false) => "#48b6ff",
            (_, true) => "#ff9a78",
            (_, false) => "#ff5a4a",
        });
        let s = if u.barracks { 6.0 } else { 3.0 };
        ctx.fill_rect(px - s / 2.0, py - s / 2.0, s, s);
    }

    // Camera view box: the four screen corners projected onto the ground and
    // mapped into the diamond. `ground_pick_or_far` always yields a corner (even
    // at the horizon), so the quad is never dropped or clamped - the diamond clip
    // does the real clipping, with no distortion when the camera looks off-map.
    let corners = [(0.0_f32, 0.0_f32), (w, 0.0), (w, h), (0.0, h)];
    let view: Vec<(f64, f64)> = corners
        .iter()
        .map(|&(sx, sv)| {
            let (wx, wz) = camera.ground_pick_or_far(sx, sv, w, h);
            to_diamond(wx, wz)
        })
        .collect();
    ctx.begin_path();
    ctx.move_to(view[0].0, view[0].1);
    ctx.line_to(view[1].0, view[1].1);
    ctx.line_to(view[2].0, view[2].1);
    ctx.line_to(view[3].0, view[3].1);
    ctx.close_path();
    ctx.set_fill_style_str("rgba(255,255,255,0.08)");
    ctx.fill();
    ctx.set_stroke_style_str("rgba(255,255,255,0.9)");
    ctx.set_line_width(1.5);
    ctx.stroke();

    ctx.restore();

    // Hover tooltip for a command-card button: the option's name with its
    // cost as resource glyph + amount pairs (the only place costs appear,
    // keeping the buttons themselves clean).
    if let Some((bx, by, bw, label, ore, carbon)) = tooltip {
        let tw = 150.0_f64;
        let th = if ore > 0 || carbon > 0 { 52.0 } else { 30.0 };
        let tx = (bx + bw / 2.0 - tw / 2.0).clamp(8.0, wf - tw - 8.0);
        let ty = (by - th - 10.0).max(8.0);
        ctx.set_fill_style_str("rgba(10,16,30,0.95)");
        ctx.fill_rect(tx, ty, tw, th);
        ctx.set_stroke_style_str("rgba(150,190,240,0.95)");
        ctx.set_line_width(1.5);
        ctx.stroke_rect(tx, ty, tw, th);
        ctx.set_fill_style_str("#e7eefa");
        ctx.set_font("bold 13px monospace");
        let _ = ctx.fill_text(label, tx + 10.0, ty + 19.0);
        let mut cx = tx + 10.0;
        ctx.set_font("bold 13px monospace");
        if ore > 0 {
            draw_ore_glyph(&ctx, cx, ty + 37.0);
            ctx.set_fill_style_str("#cfe2ff");
            let _ = ctx.fill_text(&format!("{ore}"), cx + 15.0, ty + 42.0);
            cx += 15.0 + 12.0 * (ore.max(1).ilog10() as f64 + 1.0) + 12.0;
        }
        if carbon > 0 {
            draw_carbon_glyph(&ctx, cx, ty + 37.0);
            ctx.set_fill_style_str("#cfe2ff");
            let _ = ctx.fill_text(&format!("{carbon}"), cx + 15.0, ty + 42.0);
        }
    }

    // In-game context cursor: the OS cursor is hidden during play, so the HUD
    // draws an order-aware glyph at the tracked position (physical pixels,
    // brought into the CSS space we draw in). Drawn under the pause overlay.
    if let Some(kind) = cursor_kind {
        draw_cursor_glyph(&ctx, (cursor.0 / dpr) as f64, (cursor.1 / dpr) as f64, kind);
    }

    // Pause overlay sits on top of everything when the game is paused.
    if paused {
        let dp = dpr as f64;
        let css_cursor = (cursor.0 as f64 / dp, cursor.1 as f64 / dp);
        match outcome {
            Some(win) => draw_match_end(&ctx, w, h, win, css_cursor),
            None => draw_pause(&ctx, w, h, css_cursor),
        }
    }
}
