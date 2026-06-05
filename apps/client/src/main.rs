//! Game client entry point. Opens a window (or a web canvas), initializes wgpu,
//! and runs the dual-clock loop: step the deterministic sim at a fixed rate,
//! render interpolated frames as fast as the display allows.

mod game;
mod gfx;

use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use game::Game;
use gfx::Gfx;

/// Delivered when async GPU init finishes (needed because wgpu init is async and
/// `resumed` is synchronous — unavoidable on the web). Only constructed on the
/// web path; native blocks on init directly.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
enum UserEvent {
    GfxReady(Gfx),
}

struct App {
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    game: Game,
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    proxy: EventLoopProxy<UserEvent>,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>) -> Self {
        App {
            window: None,
            gfx: None,
            game: Game::new(),
            proxy,
        }
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
        attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(960.0, 600.0));

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
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => gfx.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                self.game.update();
                let (instances, view_proj) = self.game.render_data(gfx.aspect());
                gfx.render(&instances, view_proj);
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
