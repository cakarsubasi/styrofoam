use styro_rhi::{
    self as rhi, CommandRHI as _, DeviceRHI as _, QueueRHI as _, SwapchainCommandRHI as _,
    SwapchainDeviceRHI, SwapchainRHI as _,
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalSize, Size},
    event::WindowEvent,
    event_loop::EventLoop,
    raw_window_handle::{HasDisplayHandle as _, HasWindowHandle as _},
    window::WindowAttributes,
};

const FRAMES_IN_FLIGHT: u64 = 2;

#[derive(Default)]
struct App {
    handles: Option<Handles>,
}

struct Handles {
    // renderer handles
    window: winit::window::Window,
    _device: rhi::Device,
    queue: rhi::Queue,
    swapchain: rhi::Swapchain,
    // render state
    semaphore: rhi::Semaphore,
    pipeline: rhi::Pipeline,
    frame_index: u64,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.handles.is_none() {
            let window = event_loop
                .create_window(WindowAttributes::default().with_inner_size(Size::Physical(
                    PhysicalSize {
                        width: 800,
                        height: 600,
                    },
                )))
                .expect("Unable to create window");
            let raw_display_handle = event_loop
                .display_handle()
                .expect("No display handle")
                .as_raw();
            let raw_window_handle = window.window_handle().expect("No window handle").as_raw();

            let mut device = rhi::create_device(&rhi::DeviceCreateInfo {
                device_idx: 0,
                device_features: rhi::DeviceFeatures::default(),
                presentation: Some(raw_display_handle.into()),
            });

            let window_data = unsafe {
                rhi::WindowSystemData::from_raw_handles(raw_display_handle, raw_window_handle)
            };

            let surface_formats = device.enumerate_surface_formats(&window_data);
            let surface_format = surface_formats
                .iter()
                .find(|format| matches!(format.color_space, rhi::ColorSpace::SrgbNonlinear))
                .expect("No SDR capable surface.");

            let queue = device.create_queue(rhi::QueueType::Graphics, FRAMES_IN_FLIGHT as u32, 1);

            let swapchain = device
                .create_swapchain(
                    &queue,
                    &window_data,
                    &rhi::SwapchainInfo {
                        size: FRAMES_IN_FLIGHT as u32,
                        format: surface_format.format,
                        color_space: surface_format.color_space,
                        preferred_extent: [800, 600],
                    },
                )
                .unwrap();

            let semaphore = device.create_semaphore(0);

            let spv = include_bytes!(concat!(env!("OUT_DIR"), "/triangle.spv"));
            let text = rhi::read_spv(&mut std::io::Cursor::new(spv)).unwrap();
            let pipeline = unsafe {
                device.create_graphics_pipeline(
                    &rhi::ShaderIR {
                        bytes: &text,
                        entry: c"triangle_vert",
                    },
                    &rhi::ShaderIR {
                        bytes: &text,
                        entry: c"triangle_frag",
                    },
                    &rhi::RasterDescription {
                        color_formats: &[surface_format.format],
                        cull: rhi::Cull::NONE,
                        blend_state: None,
                        ..Default::default()
                    },
                )
            };

            self.handles = Some(Handles {
                window,
                _device: device,
                queue,
                swapchain,
                semaphore,
                pipeline,
                frame_index: 1,
            });
            if let Some(h) = &self.handles {
                h.window.request_redraw();
            }
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
                if let Some(handles) = &mut self.handles {
                    let queue = &mut handles.queue;
                    let swapchain = &mut handles.swapchain;
                    let frame_index = &mut handles.frame_index;
                    let semaphore = &mut handles.semaphore;
                    let pipeline = &mut handles.pipeline;

                    let command_pool = *frame_index % FRAMES_IN_FLIGHT;

                    if *frame_index > FRAMES_IN_FLIGHT {
                        semaphore.wait(*frame_index - FRAMES_IN_FLIGHT)
                    }
                    let swapchain_image = swapchain.acquire_next_image().unwrap();

                    let mut command_buffer = queue.begin_recording(command_pool as u32);
                    command_buffer.begin_presenting(swapchain_image);

                    command_buffer.begin_rendering(&rhi::RenderingInfo {
                        color_attachments: &[rhi::Attachment {
                            image: swapchain_image,
                            ..Default::default()
                        }],
                        ..Default::default()
                    });
                    command_buffer.set_pipeline(&pipeline);
                    command_buffer.draw(&[], 3, 1);
                    command_buffer.end_rendering();

                    command_buffer.signal_after(rhi::Stage::BottomOfPipe, &semaphore, *frame_index);

                    unsafe { queue.submit(&[command_buffer]).unwrap() };
                    handles.window.pre_present_notify();
                    swapchain.present(swapchain_image).unwrap();
                    *frame_index += 1;

                    handles.window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn main() {
    EventLoop::new()
        .unwrap()
        .run_app(&mut App::default())
        .expect("Failed to run app.");
}
