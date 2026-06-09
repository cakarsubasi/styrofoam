use egui_winit::create_window;
use renderer::renderer::Renderer;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::EventLoop,
    raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle},
};

fn main() {
    let event_loop = EventLoop::new().unwrap();

    let mut app = App::default();

    event_loop.run_app(&mut app).expect("Failed to run app.");
}

struct App {
    initialized: bool,
    app_context: Option<AppContext>,
    renderer: Option<Renderer>,
}

struct AppContext {
    gui_context: egui::Context,
    window: winit::window::Window,
}

impl Default for App {
    fn default() -> Self {
        Self {
            initialized: false,
            app_context: None,
            renderer: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if !self.initialized {
            self.initialized = true;

            let gui_ctx = egui::Context::default();
            let viewport_opts = egui::ViewportBuilder::default();
            let window = create_window(&gui_ctx, &event_loop, &viewport_opts).unwrap();

            let raw_display_handle = event_loop.raw_display_handle().expect("No display handle");
            let raw_window_handle = window.raw_window_handle().expect("No window handle");

            let renderer = unsafe { Renderer::new(raw_display_handle, raw_window_handle) };
            self.renderer = Some(renderer);
            self.app_context = Some(AppContext {
                gui_context: gui_ctx,
                window,
            });
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.app_context.as_ref().unwrap().window.request_redraw();
            }
            _ => {}
        }
    }
}

fn handle_egui(ctx: &mut egui::Context) -> egui::PlatformOutput {
    let raw_input: egui::RawInput = egui::RawInput::default();

    let full_output = ctx.run_ui(raw_input, |_ctx| {
        // ui code goes here
    });

    let platform_output = full_output.platform_output;
    let _clipped_primitives = ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
    let _textures_delta = full_output.textures_delta;

    platform_output
}
