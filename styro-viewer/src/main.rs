use egui_winit::create_window;
use styro_viewer::renderer::Renderer;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::EventLoop,
    raw_window_handle::{HasDisplayHandle, HasWindowHandle},
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
    gui_context: egui_winit::State,
    //ui_renderer: UiRenderer,
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
            let viewport_opts = egui::ViewportBuilder::default(); // .with_inner_size([100.0, 100.0]);
            let window = create_window(&gui_ctx, &event_loop, &viewport_opts).unwrap();

            let raw_display_handle = event_loop
                .display_handle()
                .expect("No display handle")
                .as_raw();
            let raw_window_handle = window.window_handle().expect("No window handle").as_raw();

            let renderer = unsafe { Renderer::new(raw_display_handle, raw_window_handle) };
            let viewport_id = gui_ctx.viewport_id();
            let gui_context =
                egui_winit::State::new(gui_ctx, viewport_id, &window, None, None, None);
            self.renderer = Some(renderer);
            self.app_context = Some(AppContext {
                gui_context,
                //ui_renderer: UiRenderer::new(),
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
        let app_context = self.app_context.as_ref().unwrap();
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let app_context = self.app_context.as_mut().unwrap();
                let renderer = self.renderer.as_mut();
                let new_input = app_context.gui_context.take_egui_input(&app_context.window);

                let mut ctx = egui::Context::default();

                let full_output = ctx.run_ui(new_input, |ui| {
                    ui.label("hope this works");
                });

                let clipped_primitives =
                    ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
                let textures_delta = full_output.textures_delta;

                //app_context.ui_renderer.update(
                //    UiData {
                //        triangles: clipped_primitives,
                //        textures: textures_delta,
                //    },
                //    renderer,
                //);

                if let Some(renderer) = renderer {
                    if let Err(err) = renderer.request_redraw() {
                        // Window resized, minimized, etc.
                        eprintln!("{:?}", err);
                    } else {
                        app_context.window.request_redraw();
                    }
                } else {
                    app_context.window.request_redraw();
                }

                app_context
                    .gui_context
                    .handle_platform_output(&app_context.window, full_output.platform_output);
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

    platform_output
}
