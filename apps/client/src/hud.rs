//! HUD overlay (web): selection box + live highlight, health bars for selected
//! units, force counters, command bar, and a minimap. Native is a no-op.

use crate::camera::Camera;
use crate::game::Game;

/// Device pixel ratio (web); the HUD draws in CSS pixels scaled by this.
#[cfg(target_arch = "wasm32")]
fn dpr() -> f32 {
    web_sys::window()
        .map(|w| w.device_pixel_ratio() as f32)
        .filter(|d| *d > 0.5)
        .unwrap_or(1.0)
}

/// Train button rect in CSS pixels `(x, y, w, h)`, given the CSS canvas height.
#[cfg(target_arch = "wasm32")]
fn train_btn_css(h_css: f32) -> (f64, f64, f64, f64) {
    let bar = 96.0;
    (14.0, (h_css - bar + 44.0) as f64, 170.0, 34.0)
}

/// Train button rect in physical pixels `(x0, y0, x1, y1)`, for hit-testing
/// against raw cursor/touch coordinates.
#[cfg(target_arch = "wasm32")]
pub fn train_button_rect(w_phys: f32, h_phys: f32) -> (f32, f32, f32, f32) {
    let _ = w_phys;
    let d = dpr();
    let (x, y, bw, bh) = train_btn_css(h_phys / d);
    (
        x as f32 * d,
        y as f32 * d,
        (x + bw) as f32 * d,
        (y + bh) as f32 * d,
    )
}

/// Minimap rect in physical pixels `(x0, y0, x1, y1)`, for click-to-jump.
#[cfg(target_arch = "wasm32")]
pub fn minimap_rect(w_phys: f32, h_phys: f32) -> (f32, f32, f32, f32) {
    let d = dpr();
    let (w, h) = (w_phys / d, h_phys / d);
    let bar = 96.0_f32;
    let mm = bar - 16.0;
    let mx = w - mm - 12.0;
    let my = h - bar + 8.0;
    (mx * d, my * d, (mx + mm) * d, (my + mm) * d)
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
    let mut img = vec![0u8; dev * dev * 4];
    MINI_TERRAIN.with(|c| {
        let terr = c.get_or_init(build_mini_terrain);
        for py in 0..dev {
            for px in 0..dev {
                let nx = (px as f32 + 0.5) / dev as f32;
                let nz = (py as f32 + 0.5) / dev as f32;
                let ix = ((nx * MINI_N as f32) as usize).min(MINI_N - 1);
                let iz = ((nz * MINI_N as f32) as usize).min(MINI_N - 1);
                let [r, g, b] = terr[iz * MINI_N + ix];
                let bri = game.fog_brightness(nx, nz);
                let o = (py * dev + px) * 4;
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
pub fn draw(_c: &Camera, _g: &Game, _w: f32, _h: f32, _drag: Option<(f32, f32, f32, f32)>) {}

#[cfg(target_arch = "wasm32")]
pub fn draw(camera: &Camera, game: &Game, w: f32, h: f32, drag: Option<(f32, f32, f32, f32)>) {
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

    // Health bars: selected entities only.
    for u in game.selected_infos() {
        let Some((sx, sy)) = camera.project(glam::Vec3::new(u.wx, u.wy, u.wz), w, h) else {
            continue;
        };
        let bw = if u.barracks { 50.0 } else { 22.0 };
        let bh = 4.0;
        let x = sx as f64 - bw / 2.0;
        let y = sy as f64;
        ctx.set_fill_style_str("rgba(0,0,0,0.65)");
        ctx.fill_rect(x - 1.0, y - 1.0, bw + 2.0, bh + 2.0);
        ctx.set_fill_style_str(if u.owner == 0 { "#39d35a" } else { "#e0473a" });
        ctx.fill_rect(x, y, bw * u.hp_frac.clamp(0.0, 1.0) as f64, bh);
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

    let (pu, eu, pb, eb) = game.counts();
    ctx.set_fill_style_str("#e7eefa");
    ctx.set_font("bold 16px monospace");
    let _ = ctx.fill_text(
        &format!(
            "SOL DOMINION    ore {ore}      your force: {pu} inf / {pb} barracks      visible enemy: {eu} inf / {eb} barracks      selected: {}",
            game.selected_count(),
            ore = game.player_ore() as i64,
        ),
        14.0,
        24.0,
    );
    ctx.set_fill_style_str("#8aa3cc");
    ctx.set_font("12px monospace");
    let _ = ctx.fill_text(
        "left: select / drag-box    right: move / attack    wheel: zoom    WASD: pan    (scout north to find the enemy)",
        14.0,
        hf - bar + 22.0,
    );

    // Debug readout (toggled with number keys).
    let (fog_u, fog_e) = game.fog_flags();
    let on = |b: bool| if b { "on" } else { "OFF" };
    ctx.set_fill_style_str("#7fd0a0");
    ctx.set_font("12px monospace");
    let _ = ctx.fill_text(
        &format!(
            "DEBUG   [1] unexplored fog: {}    [2] explored fog: {}    (click minimap to jump)",
            on(fog_u),
            on(fog_e)
        ),
        14.0,
        44.0,
    );

    // Production command card when one of your buildings is selected.
    if let Some((queued, frac)) = game.selected_production() {
        let (bx, by, bw, bh) = train_btn_css(h);
        let cost = game.train_cost() as i64;
        let afford = game.player_ore() >= game.train_cost();
        ctx.set_fill_style_str("#cfe0ff");
        ctx.set_font("12px monospace");
        let _ = ctx.fill_text(&format!("BARRACKS — queue {queued}/6"), bx, by - 6.0);
        ctx.set_fill_style_str(if afford {
            "rgba(40,80,140,0.95)"
        } else {
            "rgba(48,54,66,0.92)"
        });
        ctx.fill_rect(bx, by, bw, bh);
        ctx.set_stroke_style_str("rgba(150,190,240,0.95)");
        ctx.set_line_width(1.5);
        ctx.stroke_rect(bx, by, bw, bh);
        ctx.set_fill_style_str(if afford { "#eaf2ff" } else { "#8a93a4" });
        ctx.set_font("bold 14px monospace");
        let _ = ctx.fill_text(
            &format!("Train Infantry [T] — {cost}"),
            bx + 10.0,
            by + 22.0,
        );
        if frac > 0.0 {
            ctx.set_fill_style_str("rgba(255,211,107,0.95)");
            ctx.fill_rect(bx, by + bh - 3.0, bw * frac.clamp(0.0, 1.0) as f64, 3.0);
        }
    }

    // Minimap (terrain + fog), bottom-right of the command bar.
    let mm = bar - 16.0;
    let mx = wf - mm - 12.0;
    let my = hf - bar + 8.0;
    draw_minimap(&ctx, game, dpr, mx, my, mm);
    ctx.set_stroke_style_str("rgba(120,160,210,0.95)");
    ctx.set_line_width(1.5);
    ctx.stroke_rect(mx, my, mm, mm);
    let half = terrain::HALF as f64;
    for u in game.unit_infos() {
        let nx = (u.wx as f64 + half) / (2.0 * half);
        let nz = (u.wz as f64 + half) / (2.0 * half);
        ctx.set_fill_style_str(match (u.owner, u.barracks) {
            (0, true) => "#9fdcff",
            (0, false) => "#48b6ff",
            (_, true) => "#ff9a78",
            (_, false) => "#ff5a4a",
        });
        let s = if u.barracks { 5.0 } else { 2.6 };
        ctx.fill_rect(mx + nx * mm - s / 2.0, my + nz * mm - s / 2.0, s, s);
    }
}
