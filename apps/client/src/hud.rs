//! HUD overlay (web): selection box + live highlight, health bars for selected
//! units, force counters, command bar, and a minimap. Native is a no-op.

use crate::camera::Camera;
use crate::game::Game;

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
    let dpr = web_sys::window()
        .map(|win| win.device_pixel_ratio() as f32)
        .filter(|d| *d > 0.5)
        .unwrap_or(1.0);
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
            "SOL DOMINION    your force: {pu} inf / {pb} barracks      visible enemy: {eu} inf / {eb} barracks      selected: {}",
            game.selected_count()
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

    // Minimap, bottom-right of the command bar.
    let mm = bar - 16.0;
    let mx = wf - mm - 12.0;
    let my = hf - bar + 8.0;
    ctx.set_fill_style_str("rgba(6,12,10,0.95)");
    ctx.fill_rect(mx, my, mm, mm);
    ctx.set_stroke_style_str("rgba(120,160,210,0.85)");
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
