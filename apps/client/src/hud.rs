//! HUD overlay. On the web it draws health bars, a command bar, unit/force
//! counters, and a minimap onto a 2D `<canvas id="hud">` above the WebGL canvas.
//! On native it's a no-op (the deployment target is the web page).

use crate::camera::Camera;
use crate::game::Game;

#[cfg(not(target_arch = "wasm32"))]
pub fn draw(_camera: &Camera, _game: &Game, _w: f32, _h: f32) {}

#[cfg(target_arch = "wasm32")]
pub fn draw(camera: &Camera, game: &Game, w: f32, h: f32) {
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

    let (wf, hf) = (w as f64, h as f64);
    ctx.clear_rect(0.0, 0.0, wf, hf);

    // Health bars floating above each entity.
    for u in game.unit_infos() {
        let Some((sx, sy)) = camera.project(glam::Vec3::new(u.wx, u.wy, u.wz), w, h) else {
            continue;
        };
        if sx < -40.0 || sy < -40.0 || sx > w + 40.0 || sy > h + 40.0 {
            continue;
        }
        let bw = if u.barracks { 46.0 } else { 18.0 };
        let bh = 3.5;
        let x = (sx as f64) - bw / 2.0;
        let y = sy as f64;
        ctx.set_fill_style_str("rgba(0,0,0,0.6)");
        ctx.fill_rect(x - 1.0, y - 1.0, bw + 2.0, bh + 2.0);
        ctx.set_fill_style_str(if u.owner == 0 { "#39d35a" } else { "#e0473a" });
        ctx.fill_rect(x, y, bw * u.hp_frac.clamp(0.0, 1.0) as f64, bh);
    }

    // Command bar.
    let bar = 92.0_f64;
    ctx.set_fill_style_str("rgba(8,14,26,0.82)");
    ctx.fill_rect(0.0, hf - bar, wf, bar);
    ctx.set_stroke_style_str("rgba(120,160,210,0.85)");
    ctx.set_line_width(2.0);
    ctx.stroke_rect(1.0, hf - bar + 1.0, wf - 2.0, bar - 2.0);

    // Counters.
    let (pu, eu, pb, eb) = game.counts();
    ctx.set_fill_style_str("#e7eefa");
    ctx.set_font("bold 16px monospace");
    let _ = ctx.fill_text(
        &format!(
            "SOL DOMINION    your force: {pu} inf / {pb} barracks      enemy: {eu} inf / {eb} barracks      selected: {}",
            game.selected_count()
        ),
        14.0,
        24.0,
    );
    ctx.set_fill_style_str("#8aa3cc");
    ctx.set_font("12px monospace");
    let _ = ctx.fill_text(
        "left: select / drag-box    right: move / attack    middle-drag: rotate    wheel: zoom    WASD: pan",
        14.0,
        hf - bar + 22.0,
    );

    // Minimap, bottom-right of the command bar.
    let mm = bar - 16.0;
    let mx = wf - mm - 12.0;
    let my = hf - bar + 8.0;
    ctx.set_fill_style_str("rgba(10,28,22,0.92)");
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
        let s = if u.barracks { 4.5 } else { 2.4 };
        ctx.fill_rect(mx + nx * mm - s / 2.0, my + nz * mm - s / 2.0, s, s);
    }
}
