//! Descriptor heaps are used when the physical device supports them. Pass `--classic` to force
//! the classic bindless path.
use cgmath::{Matrix4, Vector3, perspective, point3, prelude::*};
use std::time::Instant;
use styro_rhi::{
    self as rhi, CommandRHI as _, DeviceRHI as _, QueueRHI as _, SwapchainCommandRHI as _,
    SwapchainDeviceRHI, SwapchainRHI as _,
};
use viking_room::{
    Mesh, TextureHeap, choose_surface_format, create_buffer_with_data, create_texture_blocking,
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

const PREFERRED_EXTENT: [u32; 2] = [1600, 900];

struct MeshGpu {
    positions: rhi::GpuPtr,
    positions_addr: u64,
    indices: rhi::GpuPtr,
    vertex_data: rhi::GpuPtr,
    vertex_data_addr: u64,
}

impl MeshGpu {
    fn new(device: &mut rhi::Device, mesh: &Mesh) -> MeshGpu {
        let positions = create_buffer_with_data(
            device,
            rhi::BufferUsage::General,
            bytemuck::cast_slice(&mesh.positions),
        );
        let vertex_data = create_buffer_with_data(
            device,
            rhi::BufferUsage::General,
            bytemuck::cast_slice(&mesh.vertex_data),
        );
        let positions_addr = device.buffer_device_addr(positions);
        let vertex_data_addr = device.buffer_device_addr(vertex_data);
        MeshGpu {
            positions,
            vertex_data,
            indices: create_buffer_with_data(
                device,
                rhi::BufferUsage::General,
                bytemuck::cast_slice(&mesh.indices),
            ),
            positions_addr,
            vertex_data_addr,
        }
    }
}

#[derive(Default)]
struct App {
    handles: Option<Handles>,
}

struct Handles {
    // renderer handles
    window: winit::window::Window,
    device: rhi::Device,
    queue: rhi::Queue,
    swapchain: rhi::Swapchain,
    // render state
    semaphore: rhi::Semaphore,
    pipeline: rhi::Pipeline,
    start: Instant,
    frame_index: u64,
    color_images: Vec<rhi::GpuImage>,
    depth_images: Vec<rhi::GpuImage>,
    // scene
    texture: rhi::GpuImage,
    mesh: MeshGpu,
    ubo: rhi::GpuPtr,
    bindless: TextureHeap,
}

impl Drop for Handles {
    fn drop(&mut self) {
        let device = &mut self.device;

        self.semaphore.wait(self.frame_index - 1);

        // Safety: uhhh
        unsafe {
            device.destroy_buffer(self.ubo);
            device.destroy_buffer(self.mesh.indices);
            device.destroy_buffer(self.mesh.positions);
            device.destroy_buffer(self.mesh.vertex_data);
            self.bindless.destroy(device);
            device.destroy_image(self.texture);
            for image in &self.depth_images {
                device.destroy_image(*image);
            }
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::NoUninit)]
struct Push {
    pos: u64,
    data: u64,
    ubo: u64,
    tex_id: [u32; 2],
}

#[repr(C)]
struct Ubo {
    model: Matrix4<f32>,
    view: Matrix4<f32>,
    proj: Matrix4<f32>,
}

impl Ubo {
    fn new(time: f32, aspect_ratio: f32) -> Self {
        let model = Matrix4::from_angle_z(cgmath::Deg::turn_div_4() * time);
        let view = Matrix4::look_at_rh(
            point3(2.0f32, 2.0f32, 2.0f32),
            cgmath::Point3::origin(),
            -Vector3::unit_z(),
        );
        let proj = perspective(cgmath::Deg(45.0f32), aspect_ratio, 0.1f32, 10.0f32);
        Ubo { model, view, proj }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if self.handles.is_none() {
            let window = event_loop
                .create_window(WindowAttributes::default().with_inner_size(Size::Physical(
                    PhysicalSize {
                        width: PREFERRED_EXTENT[0],
                        height: PREFERRED_EXTENT[1],
                    },
                )))
                .expect("Unable to create window");
            let raw_display_handle = event_loop
                .display_handle()
                .expect("No display handle")
                .as_raw();
            let raw_window_handle = window.window_handle().expect("No window handle").as_raw();

            let force_classic = std::env::args().any(|arg| arg == "--classic");
            let use_heaps = !force_classic
                && rhi::instance::enumerate_physical_devices()
                    .first()
                    .is_some_and(|device| device.device_features.descriptor_heap);
            println!(
                "Using {}",
                if use_heaps {
                    "descriptor heaps"
                } else {
                    "classic bindless"
                }
            );

            let mut device = rhi::create_device(&rhi::DeviceCreateInfo {
                device_idx: 0,
                device_features: rhi::DeviceFeatures {
                    descriptor_heap: use_heaps,
                    ..rhi::DeviceFeatures::default()
                },
                presentation: Some(raw_display_handle.into()),
            });

            let mut queue =
                device.create_queue(rhi::QueueType::Graphics, FRAMES_IN_FLIGHT as u32, 1);

            let window_data = unsafe {
                rhi::WindowSystemData::from_raw_handles(raw_display_handle, raw_window_handle)
            };

            let surface_format = choose_surface_format(&device, &window_data);
            let swapchain = device
                .create_swapchain(
                    &queue,
                    &window_data,
                    &rhi::SwapchainInfo {
                        size: FRAMES_IN_FLIGHT as u32,
                        format: surface_format.format,
                        color_space: surface_format.color_space,
                        preferred_extent: PREFERRED_EXTENT,
                    },
                )
                .unwrap();

            let color_format = surface_format.format;
            let color_images = (0..FRAMES_IN_FLIGHT)
                .map(|_| {
                    device.create_image(&rhi::ImageInfo {
                        ty: rhi::ImageType::Type2D,
                        dimensions: [PREFERRED_EXTENT[0], PREFERRED_EXTENT[1], 1],
                        sample_count: rhi::SampleCount::X4,
                        format: color_format,
                        usage: rhi::ImageUsage::Attachment,
                        ..Default::default()
                    })
                })
                .collect();

            let depth_format = rhi::Format::D32_Sfloat;
            let depth_images = (0..FRAMES_IN_FLIGHT)
                .map(|_| {
                    device.create_image(&rhi::ImageInfo {
                        ty: rhi::ImageType::Type2D,
                        dimensions: [PREFERRED_EXTENT[0], PREFERRED_EXTENT[1], 1],
                        sample_count: rhi::SampleCount::X4,
                        format: depth_format,
                        usage: rhi::ImageUsage::Attachment,
                        ..Default::default()
                    })
                })
                .collect();

            let semaphore = device.create_semaphore(0);

            let mesh = Mesh::from_obj_bytes(include_bytes!("viking_room.obj"));
            println!(
                "Parsed {} vertices and {} triangles",
                mesh.vertex_data.len(),
                mesh.indices.len() / 3
            );
            let mesh_buffers = MeshGpu::new(&mut device, &mesh);
            let ubo = device.create_buffer(&rhi::BufferInfo {
                memory: rhi::Memory::Upload,
                size: size_of::<Ubo>() as u64 * FRAMES_IN_FLIGHT,
                usage: rhi::BufferUsage::General,
            });

            let png = include_bytes!("viking_room.png");
            let texture =
                create_texture_blocking(&mut device, &mut queue, png, rhi::Format::R8G8B8A8_Srgb);

            let mut bindless = if use_heaps {
                TextureHeap::new_descriptor_heap(&mut device)
            } else {
                TextureHeap::new_classic(&mut device)
            };

            bindless.write_sampled_image_descriptor(&mut device, 0, texture);
            bindless.write_sampler_descriptor(&mut device, 0, &rhi::SamplerInfo::default());

            let spirv = if use_heaps {
                include_bytes!(concat!(env!("OUT_DIR"), "/mesh_3d_heap.spv")).as_slice()
            } else {
                include_bytes!(concat!(env!("OUT_DIR"), "/mesh_3d_classic.spv")).as_slice()
            };
            let text = rhi::read_spv(&mut std::io::Cursor::new(spirv)).unwrap();
            let pipeline = unsafe {
                device.create_graphics_pipeline(
                    &rhi::ShaderIR {
                        bytes: &text,
                        entry: c"vertMain",
                    },
                    &rhi::ShaderIR {
                        bytes: &text,
                        entry: c"fragMain",
                    },
                    &rhi::RasterDescription {
                        color_formats: &[surface_format.format],
                        depth_format: depth_format,
                        samples: rhi::SampleCount::X4,
                        cull: rhi::Cull::CW,
                        blend_state: None,
                        ..Default::default()
                    },
                )
            };

            self.handles = Some(Handles {
                window,
                device,
                queue,
                swapchain,
                semaphore,
                pipeline,
                start: Instant::now(),
                frame_index: 1,
                texture,
                mesh: mesh_buffers,
                ubo,
                bindless,
                depth_images,
                color_images,
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

                    let size = handles.window.inner_size();
                    let aspect_ratio = size.width as f32 / size.height as f32;

                    let command_pool = *frame_index % FRAMES_IN_FLIGHT;
                    let color_attachment = handles.color_images[command_pool as usize];
                    let depth_buffer = handles.depth_images[command_pool as usize];

                    if *frame_index > FRAMES_IN_FLIGHT {
                        semaphore.wait(*frame_index - FRAMES_IN_FLIGHT)
                    }
                    let swapchain_image = swapchain.acquire_next_image().unwrap();

                    let mut command_buffer = queue.begin_recording(command_pool as u32);
                    command_buffer.begin_presenting(swapchain_image);

                    handles.bindless.bind_heap(&mut command_buffer);

                    command_buffer.begin_rendering(&rhi::RenderingInfo {
                        color_attachments: &[rhi::Attachment {
                            image: color_attachment,
                            store_op: rhi::StoreOp::DontCare,
                            resolve: swapchain_image,
                            ..Default::default()
                        }],
                        depth_attachment: Some(rhi::Attachment {
                            image: depth_buffer,
                            load_op: rhi::LoadOp::Clear,
                            store_op: rhi::StoreOp::DontCare,
                            clear_value: rhi::Clear::DepthStencil(1.0, 0),
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                    command_buffer.set_depth_stencil_state(Some(&rhi::DepthStencilInfo {
                        depth_read: true,
                        depth_write: true,
                        depth_test: rhi::CompareOp::Less,
                        ..Default::default()
                    }));
                    command_buffer.set_pipeline(pipeline);
                    let ubo_addr = handles.device.buffer_device_addr(handles.ubo)
                        + (command_pool * size_of::<Ubo>() as u64);
                    let ubo_host = handles.device.buffer_host_addr(handles.ubo);
                    let elapsed = handles.start.elapsed().as_millis() as f64 / 1000.0;
                    unsafe {
                        (ubo_host as *mut Ubo)
                            .add(command_pool as usize)
                            .write(Ubo::new(elapsed as f32, aspect_ratio));
                    }
                    let push_data = Push {
                        pos: handles.mesh.positions_addr,
                        data: handles.mesh.vertex_data_addr,
                        ubo: ubo_addr,
                        tex_id: [0, 0],
                    };
                    command_buffer.draw_indexed(
                        bytemuck::bytes_of(&push_data),
                        handles.mesh.indices,
                        1,
                    );
                    command_buffer.end_rendering();

                    command_buffer.signal_after(rhi::Stage::BottomOfPipe, semaphore, *frame_index);

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
