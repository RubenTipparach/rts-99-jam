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

/// Command-card button `k`'s rect in CSS pixels `(x, y, w, h)`.
#[cfg(target_arch = "wasm32")]
fn card_btn_css(k: usize, h_css: f32) -> (f64, f64, f64, f64) {
    let bar = 96.0;
    let bw = 132.0;
    (
        14.0 + k as f64 * (bw + 8.0),
        (h_css - bar + 34.0) as f64,
        bw,
        44.0,
    )
}

/// Command-card button `k`'s rect in physical pixels `(x0, y0, x1, y1)`, for
/// hit-testing against raw cursor/touch coordinates.
#[cfg(target_arch = "wasm32")]
pub fn card_button_rect(k: usize, h_phys: f32) -> (f32, f32, f32, f32) {
    let d = dpr();
    let (x, y, bw, bh) = card_btn_css(k, h_phys / d);
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
    let by = h_css as f64 / 2.0 + 6.0;
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

/// Draw the in-game cursor at `(x, y)` in CSS pixels. Used while the pointer is
/// locked, when the browser hides the real cursor. A small arrowhead with a dark
/// outline so it reads over both terrain and water.
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
    let _ = ctx.fill_text(if win { "VICTORY" } else { "DEFEAT" }, wf / 2.0, py + 68.0);
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
    _draw_cursor: bool,
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
    draw_cursor: bool,
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

    // Ore: a faceted crystal (diamond + bright top facet).
    let ox = 16.0_f64;
    ctx.begin_path();
    ctx.move_to(ox + 6.0, icon_y - 8.0);
    ctx.line_to(ox + 12.0, icon_y);
    ctx.line_to(ox + 6.0, icon_y + 8.0);
    ctx.line_to(ox, icon_y);
    ctx.close_path();
    ctx.set_fill_style_str("#69c8e8");
    ctx.fill();
    ctx.begin_path();
    ctx.move_to(ox + 6.0, icon_y - 8.0);
    ctx.line_to(ox + 12.0, icon_y);
    ctx.line_to(ox + 6.0, icon_y);
    ctx.close_path();
    ctx.set_fill_style_str("#c4eefb");
    ctx.fill();
    ctx.set_fill_style_str("#e7eefa");
    let _ = ctx.fill_text(&format!("{}", game.player_ore() as i64), ox + 18.0, 24.0);

    // Carbon: a geyser puff (stacked green clouds over a dark vent).
    let cx2 = 106.0_f64;
    ctx.set_fill_style_str("#3a4540");
    ctx.fill_rect(cx2 + 3.0, icon_y + 3.0, 6.0, 5.0);
    ctx.set_fill_style_str("#5ad97c");
    ctx.begin_path();
    let _ = ctx.arc(cx2 + 6.0, icon_y, 5.0, 0.0, std::f64::consts::TAU);
    ctx.fill();
    ctx.set_fill_style_str("#a9f3bd");
    ctx.begin_path();
    let _ = ctx.arc(cx2 + 4.0, icon_y - 3.0, 3.0, 0.0, std::f64::consts::TAU);
    ctx.fill();
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

    // Command card: clickable buttons for training units (HQ -> Worker,
    // Barracks -> Infantry/Heavy) and for worker construction. Hotkeys still
    // work; the buttons mirror them.
    let actions = card_actions(game);
    if !actions.is_empty() {
        let prod = game.selected_production();
        let ore_have = game.player_ore() as i64;
        let carbon_have = game.player_carbon() as i64;
        let mut prod_shown = false;
        let dc = dpr as f64;
        for (k, &(action, label, hotkey, ore, carbon)) in actions.iter().enumerate() {
            let (bx, by, bw, bh) = card_btn_css(k, h);
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
            ctx.set_fill_style_str(if pressed {
                "#0a1220"
            } else if afford {
                "#eaf2ff"
            } else {
                "#8a93a4"
            });
            ctx.set_font("bold 13px monospace");
            let _ = ctx.fill_text(&format!("{label} [{hotkey}]"), bx + 8.0, by + 17.0);
            ctx.set_font("11px monospace");
            ctx.set_fill_style_str(if afford { "#bcd2f2" } else { "#7d8698" });
            let cost = if carbon > 0 {
                format!("{ore} ore + {carbon} c")
            } else {
                format!("{ore} ore")
            };
            let _ = ctx.fill_text(&cost, bx + 8.0, by + 33.0);
            // The first train button carries the building's queue + progress.
            if let (CardAction::Train(_), Some((queued, frac)), false) = (action, prod, prod_shown)
            {
                prod_shown = true;
                ctx.set_fill_style_str("#ffd36b");
                let _ = ctx.fill_text(&format!("{queued}/6"), bx + bw - 32.0, by + 17.0);
                if frac > 0.0 {
                    ctx.set_fill_style_str("rgba(255,211,107,0.95)");
                    ctx.fill_rect(bx, by + bh - 3.0, bw * frac.clamp(0.0, 1.0) as f64, 3.0);
                }
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

    // In-game cursor (pointer locked): the cursor arrives in physical pixels, so
    // bring it into the CSS space we draw in. Drawn under the pause overlay.
    if draw_cursor {
        draw_cursor_arrow(&ctx, (cursor.0 / dpr) as f64, (cursor.1 / dpr) as f64);
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
