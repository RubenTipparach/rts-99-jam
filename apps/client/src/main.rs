//! Game client. Fixed-angle RTS camera; left-click/drag to select, right-click
//! to move/attack, wheel to zoom, WASD/arrows to pan.

mod camera;
mod game;
mod gfx;
mod hud;
mod terrain;

use std::sync::Arc;
use web_time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use camera::Camera;
use game::Game;
use gfx::Gfx;

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum UserEvent {
    GfxReady(Gfx),
}

/// Mobile **test** controls (web only).
///
/// DOM is allowed only for these — they let a developer drive desktop
/// interactions (pan / zoom / right-click) from a touch device. The buttons are
/// bare elements in `index.html`; all behavior is wired here. State lives in a
/// thread-local the app reads each frame; everything else stays in WASM.
#[cfg(target_arch = "wasm32")]
mod mobile {
    use std::cell::Cell;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::JsCast;

    #[derive(Clone, Copy)]
    pub struct Controls {
        pub up: bool,
        pub down: bool,
        pub left: bool,
        pub right: bool,
        pub zoom: i32,     // +1 = zoom in, -1 = out, 0 = idle
        pub rc_mode: bool, // when set, a tap issues a move/attack order
    }

    thread_local! {
        static STATE: Cell<Controls> = const {
            Cell::new(Controls {
                up: false, down: false, left: false, right: false, zoom: 0, rc_mode: false,
            })
        };
    }

    pub fn snapshot() -> Controls {
        STATE.with(|s| s.get())
    }
    fn update(f: impl FnOnce(&mut Controls)) {
        STATE.with(|s| {
            let mut c = s.get();
            f(&mut c);
            s.set(c);
        });
    }

    fn element(id: &str) -> Option<web_sys::Element> {
        web_sys::window()?.document()?.get_element_by_id(id)
    }

    /// A button that holds a flag while pressed.
    fn hold(id: &str, set: fn(&mut Controls, bool)) {
        let Some(el) = element(id) else { return };
        let down = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            e.prevent_default();
            update(|c| set(c, true));
        });
        let up = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            e.prevent_default();
            update(|c| set(c, false));
        });
        let _ = el.add_event_listener_with_callback("pointerdown", down.as_ref().unchecked_ref());
        for ev in ["pointerup", "pointerleave", "pointercancel"] {
            let _ = el.add_event_listener_with_callback(ev, up.as_ref().unchecked_ref());
        }
        down.forget();
        up.forget();
    }

    /// A button that drives the zoom direction while pressed.
    fn hold_zoom(id: &str, dir: i32) {
        let Some(el) = element(id) else { return };
        let down = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            e.prevent_default();
            update(|c| c.zoom = dir);
        });
        let up = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            e.prevent_default();
            update(|c| c.zoom = 0);
        });
        let _ = el.add_event_listener_with_callback("pointerdown", down.as_ref().unchecked_ref());
        for ev in ["pointerup", "pointerleave", "pointercancel"] {
            let _ = el.add_event_listener_with_callback(ev, up.as_ref().unchecked_ref());
        }
        down.forget();
        up.forget();
    }

    /// The right-click mode toggle (taps become orders); reflects state via a class.
    fn toggle_rc(id: &str) {
        let Some(el) = element(id) else { return };
        let btn = el.clone();
        let cb = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            e.prevent_default();
            let mut on = false;
            update(|c| {
                c.rc_mode = !c.rc_mode;
                on = c.rc_mode;
            });
            let _ = btn.class_list().toggle_with_force("on", on);
        });
        let _ = el.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref());
        cb.forget();
    }

    /// Wire up the (already-present) DOM test buttons. Safe to call once.
    pub fn install() {
        hold("pan-up", |c, v| c.up = v);
        hold("pan-down", |c, v| c.down = v);
        hold("pan-left", |c, v| c.left = v);
        hold("pan-right", |c, v| c.right = v);
        hold_zoom("zoom-in", 1);
        hold_zoom("zoom-out", -1);
        toggle_rc("rc-toggle");
    }
}

/// Browser window inner size in CSS pixels (web only).
#[cfg(target_arch = "wasm32")]
fn browser_size() -> Option<(u32, u32)> {
    let win = web_sys::window()?;
    let w = win.inner_width().ok()?.as_f64()?;
    let h = win.inner_height().ok()?.as_f64()?;
    Some((w.max(1.0) as u32, h.max(1.0) as u32))
}

/// Fade out and remove the HTML loading overlay once the game is drawing.
#[cfg(target_arch = "wasm32")]
fn hide_loading() {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("loading"))
    {
        let _ = el.set_attribute("style", "display:none");
    }
}

#[derive(Default)]
struct Input {
    fwd: bool,
    back: bool,
    left: bool,
    right: bool,
    cursor: (f32, f32),
    cursor_in: bool,
    left_press: Option<(f32, f32)>,
}

struct App {
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    game: Game,
    camera: Camera,
    input: Input,
    last_frame: Instant,
    /// Set once a touch is seen, so edge-panning (a mouse affordance) is
    /// disabled on touch devices — the d-pad pans there instead.
    pointer_is_touch: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    last_css: (u32, u32),
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    first_frame_done: bool,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    proxy: EventLoopProxy<UserEvent>,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        App {
            window: None,
            gfx: None,
            game: Game::new(),
            camera: Camera::default(),
            input: Input::default(),
            last_frame: Instant::now(),
            pointer_is_touch: false,
            last_css: (0, 0),
            first_frame_done: false,
            proxy,
        }
    }

    fn dims(&self) -> (f32, f32) {
        self.gfx
            .as_ref()
            .map(|g| (g.width as f32, g.height as f32))
            .unwrap_or((1.0, 1.0))
    }

    /// If a building is selected and the point is on its Train button, queue a
    /// unit and report that the click was consumed (web HUD only).
    #[cfg(target_arch = "wasm32")]
    fn train_button_hit(&mut self, cx: f32, cy: f32, w: f32, h: f32) -> bool {
        if self.game.selected_barracks().is_none() {
            return false;
        }
        let (x0, y0, x1, y1) = hud::train_button_rect(w, h);
        if cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1 {
            self.game.train_selected();
            true
        } else {
            false
        }
    }

    /// If the point is on the minimap, recenter the camera there and report the
    /// click consumed (web HUD only).
    #[cfg(target_arch = "wasm32")]
    fn minimap_jump(&mut self, cx: f32, cy: f32, w: f32, h: f32) -> bool {
        let (x0, y0, x1, y1) = hud::minimap_rect(w, h);
        if x1 <= x0 || y1 <= y0 || cx < x0 || cx > x1 || cy < y0 || cy > y1 {
            return false;
        }
        let nx = ((cx - x0) / (x1 - x0)).clamp(0.0, 1.0);
        let nz = ((cy - y0) / (y1 - y0)).clamp(0.0, 1.0);
        let wx = nx * 2.0 * terrain::HALF - terrain::HALF;
        let wz = nz * 2.0 * terrain::HALF - terrain::HALF;
        self.camera.look_at(wx, wz);
        true
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut attrs = Window::default_attributes().with_title("Sol Dominion");
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs = attrs.with_append(true);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(1280.0, 800.0));
        }

        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        self.window = Some(window.clone());
        #[cfg(target_arch = "wasm32")]
        if let Some((bw, bh)) = browser_size() {
            let _ = window.request_inner_size(winit::dpi::LogicalSize::new(bw as f64, bh as f64));
            self.last_css = (bw, bh);
        }
        #[cfg(target_arch = "wasm32")]
        mobile::install();

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.gfx = Some(pollster::block_on(Gfx::new(window)));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let proxy = self.proxy.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let gfx = Gfx::new(window).await;
                let _ = proxy.send_event(UserEvent::GfxReady(gfx));
            });
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::GfxReady(mut gfx) => {
                // A Resized event can fire while the GPU is still initializing
                // (gfx is None then, so it's dropped). Sync the surface to the
                // window's real size now, or the first frames render at the
                // stale tiny size — a single pixel stretched to full screen.
                if let Some(w) = &self.window {
                    let s = w.inner_size();
                    gfx.resize(s.width, s.height);
                    self.gfx = Some(gfx);
                    w.request_redraw();
                } else {
                    self.gfx = Some(gfx);
                }
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut() {
                    gfx.resize(size.width, size.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let down = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        KeyCode::KeyW | KeyCode::ArrowUp => self.input.fwd = down,
                        KeyCode::KeyS | KeyCode::ArrowDown => self.input.back = down,
                        KeyCode::KeyA | KeyCode::ArrowLeft => self.input.left = down,
                        KeyCode::KeyD | KeyCode::ArrowRight => self.input.right = down,
                        KeyCode::KeyT if down => self.game.train_selected(),
                        KeyCode::Digit1 if down => self.game.toggle_fog_unexplored(),
                        KeyCode::Digit2 if down => self.game.toggle_fog_explored(),
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let (w, h) = self.dims();
                let (cx, cy) = self.input.cursor;
                match button {
                    MouseButton::Left => {
                        if state == ElementState::Pressed {
                            #[cfg(target_arch = "wasm32")]
                            let consumed = self.train_button_hit(cx, cy, w, h)
                                || self.minimap_jump(cx, cy, w, h);
                            #[cfg(not(target_arch = "wasm32"))]
                            let consumed = false;
                            if !consumed {
                                self.input.left_press = Some((cx, cy));
                            }
                        } else if let Some((px, py)) = self.input.left_press.take() {
                            if (px - cx).hypot(py - cy) < 8.0 {
                                self.game.select_single(&self.camera, w, h, cx, cy);
                            } else {
                                let rect = (px.min(cx), py.min(cy), px.max(cx), py.max(cy));
                                self.game.select_box_screen(&self.camera, w, h, rect);
                            }
                        }
                    }
                    MouseButton::Right if state == ElementState::Pressed => {
                        if let Some((wx, wz)) = self.camera.ground_pick(cx, cy, w, h) {
                            self.game.order(wx, wz);
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.cursor = (position.x as f32, position.y as f32);
                self.input.cursor_in = true;
            }
            WindowEvent::CursorEntered { .. } => self.input.cursor_in = true,
            WindowEvent::CursorLeft { .. } => self.input.cursor_in = false,
            WindowEvent::MouseWheel { delta, .. } => {
                let units = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                self.camera.zoom(units);
            }
            // Touch drives the same select/order path as the mouse, so a phone
            // (with the DOM test buttons) can exercise desktop interactions.
            WindowEvent::Touch(touch) => {
                use winit::event::TouchPhase;
                let (cx, cy) = (touch.location.x as f32, touch.location.y as f32);
                self.input.cursor = (cx, cy);
                self.pointer_is_touch = true;
                match touch.phase {
                    TouchPhase::Started => self.input.left_press = Some((cx, cy)),
                    TouchPhase::Moved => {}
                    TouchPhase::Cancelled => self.input.left_press = None,
                    TouchPhase::Ended => {
                        let pressed = self.input.left_press.take();
                        let (w, h) = self.dims();
                        #[cfg(target_arch = "wasm32")]
                        if self.train_button_hit(cx, cy, w, h) || self.minimap_jump(cx, cy, w, h) {
                            return;
                        }
                        if let Some((px, py)) = pressed {
                            #[cfg(target_arch = "wasm32")]
                            let rc = mobile::snapshot().rc_mode;
                            #[cfg(not(target_arch = "wasm32"))]
                            let rc = false;
                            if rc {
                                if let Some((wx, wz)) = self.camera.ground_pick(cx, cy, w, h) {
                                    self.game.order(wx, wz);
                                }
                            } else if (px - cx).hypot(py - cy) < 8.0 {
                                self.game.select_single(&self.camera, w, h, cx, cy);
                            } else {
                                let rect = (px.min(cx), py.min(cy), px.max(cx), py.max(cy));
                                self.game.select_box_screen(&self.camera, w, h, rect);
                            }
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                // Keep the canvas matched to the browser window (web).
                #[cfg(target_arch = "wasm32")]
                if let Some((bw, bh)) = browser_size() {
                    if (bw, bh) != self.last_css && bw > 1 && bh > 1 {
                        self.last_css = (bw, bh);
                        if let Some(w) = &self.window {
                            let _ = w.request_inner_size(winit::dpi::LogicalSize::new(
                                bw as f64, bh as f64,
                            ));
                        }
                    }
                }
                // Self-heal: keep the GPU surface == winit's canvas size, in case
                // a Resized event was missed (e.g. during async GPU startup).
                #[cfg(target_arch = "wasm32")]
                if let Some(win) = self.window.as_ref() {
                    let s = win.inner_size();
                    if s.width > 1 && s.height > 1 {
                        if let Some(g) = self.gfx.as_mut() {
                            if g.width != s.width || g.height != s.height {
                                g.resize(s.width, s.height);
                            }
                        }
                    }
                }
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;

                let mut fwd = (self.input.fwd as i32 - self.input.back as i32) as f32;
                let mut right = (self.input.right as i32 - self.input.left as i32) as f32;
                // Edge panning: scroll the camera when the mouse rests near a
                // screen edge. Disabled on touch (the d-pad pans there) so a
                // resting finger position can't make the camera drift forever.
                if self.input.cursor_in && !self.pointer_is_touch {
                    let (sw, sh) = self.dims();
                    let (cx, cy) = self.input.cursor;
                    const EDGE: f32 = 28.0;
                    if sw > 1.0 && sh > 1.0 {
                        if cx <= EDGE {
                            right -= 1.0;
                        } else if cx >= sw - EDGE {
                            right += 1.0;
                        }
                        if cy <= EDGE {
                            fwd += 1.0;
                        } else if cy >= sh - EDGE {
                            fwd -= 1.0;
                        }
                    }
                }
                // Mobile test controls: d-pad pan + held zoom.
                #[cfg(target_arch = "wasm32")]
                {
                    let m = mobile::snapshot();
                    fwd += (m.up as i32 - m.down as i32) as f32;
                    right += (m.right as i32 - m.left as i32) as f32;
                    if m.zoom != 0 {
                        self.camera.zoom(m.zoom as f32 * 0.15);
                    }
                }
                if fwd != 0.0 || right != 0.0 {
                    self.camera.pan(fwd, right, dt);
                }

                self.game.update();
                self.game.recompute_fow();

                let drag_rect = self.input.left_press.and_then(|(px, py)| {
                    let (cx, cy) = self.input.cursor;
                    if (px - cx).hypot(py - cy) >= 8.0 {
                        Some((px.min(cx), py.min(cy), px.max(cx), py.max(cy)))
                    } else {
                        None
                    }
                });

                if let Some(gfx) = self.gfx.as_mut() {
                    let aspect = gfx.aspect();
                    let (infantry, barracks, rings) = self.game.render_data();
                    let fow = self.game.fow_bytes();
                    let vp = self.camera.view_proj(aspect);
                    gfx.render(
                        &infantry,
                        &barracks,
                        &rings,
                        &fow,
                        vp,
                        self.camera.eye(),
                        self.game.time(),
                    );
                }
                let (w, h) = self.dims();
                hud::draw(&self.camera, &self.game, w, h, drag_rect);

                // Remove the loading overlay once the first frame is on screen.
                #[cfg(target_arch = "wasm32")]
                if self.gfx.is_some() && !self.first_frame_done {
                    self.first_frame_done = true;
                    hide_loading();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

pub fn run() {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("event loop");
    let app = App::new(event_loop.create_proxy());

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = app;
        event_loop.run_app(&mut app).expect("run app");
    }
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
}

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        env_logger::init();
    }
    #[cfg(target_arch = "wasm32")]
    {
        console_error_panic_hook::set_once();
        let _ = console_log::init_with_level(log::Level::Info);
    }
    run();
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod shader_tests {
    // wgpu compiles WGSL at pipeline-creation time, so a malformed shader would
    // only blow up on the GPU. Validate it here (same naga wgpu uses) so CI
    // catches it.
    #[test]
    fn wgsl_compiles_and_validates() {
        let src = include_str!("shader.wgsl");
        let module = naga::front::wgsl::parse_str(src).expect("shader.wgsl should parse");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("shader.wgsl should validate");
    }
}
