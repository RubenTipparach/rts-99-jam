//! Game client: opens a window/canvas, runs the deterministic sim under an RTS
//! camera, and handles selection + orders.
//!
//! Controls: left-click/drag = select, right-click = move/attack,
//! middle-drag = rotate, wheel = zoom, WASD/arrows = pan.

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

#[derive(Default)]
struct Input {
    fwd: bool,
    back: bool,
    left: bool,
    right: bool,
    cursor: (f32, f32),
    left_press: Option<(f32, f32)>,
    middle_down: bool,
    last_cursor: Option<(f32, f32)>,
}

struct App {
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    game: Game,
    camera: Camera,
    input: Input,
    last_frame: Instant,
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

        let mut attrs = Window::default_attributes().with_title("rts-99-jam");
        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs = attrs.with_append(true);
        }
        attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(1024.0, 640.0));

        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        self.window = Some(window.clone());

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
                            let drag = (px - cx).hypot(py - cy);
                            if drag < 8.0 {
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
                    MouseButton::Right => {
                        if state == ElementState::Pressed {
                            if let Some((wx, wz)) = self.camera.ground_pick(cx, cy, w, h) {
                                self.game.order(wx, wz);
                            }
                        }
                    }
                    MouseButton::Middle => self.input.middle_down = state == ElementState::Pressed,
                    _ => {}
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let p = (position.x as f32, position.y as f32);
                if self.input.middle_down {
                    if let Some((lx, ly)) = self.input.last_cursor {
                        self.camera.rotate(p.0 - lx, p.1 - ly);
                    }
                }
                self.input.cursor = p;
                self.input.last_cursor = Some(p);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let units = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 50.0,
                };
                self.camera.zoom(units);
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.1);
                self.last_frame = now;

                let fwd = (self.input.fwd as i32 - self.input.back as i32) as f32;
                let right = (self.input.right as i32 - self.input.left as i32) as f32;
                if fwd != 0.0 || right != 0.0 {
                    self.camera.pan(fwd, right, dt);
                }

                self.game.update();
                if let Some(gfx) = self.gfx.as_mut() {
                    let aspect = gfx.aspect();
                    let (units, rings) = self.game.render_data();
                    let vp = self.camera.view_proj(aspect);
                    gfx.render(&units, &rings, vp, self.camera.eye(), self.game.time());
                }
                let (w, h) = self.dims();
                hud::draw(&self.camera, &self.game, w, h);
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
