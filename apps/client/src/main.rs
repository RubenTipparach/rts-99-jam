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
    left_press: Option<(f32, f32)>,
}

struct App {
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    game: Game,
    camera: Camera,
    input: Input,
    last_frame: Instant,
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
            UserEvent::GfxReady(gfx) => {
                self.gfx = Some(gfx);
                if let Some(w) = &self.window {
                    w.request_redraw();
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
                            self.input.left_press = Some((cx, cy));
                        } else if let Some((px, py)) = self.input.left_press.take() {
                            if (px - cx).hypot(py - cy) < 8.0 {
                                if let Some((wx, wz)) = self.camera.ground_pick(cx, cy, w, h) {
                                    self.game.select_single(wx, wz);
                                }
                            } else if let (Some(a), Some(b)) = (
                                self.camera.ground_pick(px, py, w, h),
                                self.camera.ground_pick(cx, cy, w, h),
                            ) {
                                self.game.select_box(a.0, a.1, b.0, b.1);
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
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let units = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                self.camera.zoom(units);
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
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;

                let fwd = (self.input.fwd as i32 - self.input.back as i32) as f32;
                let right = (self.input.right as i32 - self.input.left as i32) as f32;
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
                    let (units, rings) = self.game.render_data();
                    let fow = self.game.fow_bytes();
                    let vp = self.camera.view_proj(aspect);
                    gfx.render(
                        &units,
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
