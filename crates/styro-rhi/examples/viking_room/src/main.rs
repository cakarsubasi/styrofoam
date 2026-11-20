//! Descriptor heaps are used when the physical device supports them. Pass `--classic` to force
//! the classic bindless path.
use cgmath::{Matrix4, Vector3, perspective, point3, prelude::*};
use std::{collections::HashMap, io::Cursor, ptr};
use styro_rhi::{
    self as rhi, CommandRHI as _, DeviceRHI as _, QueueRHI as _, SwapchainCommandRHI as _,
    SwapchainDeviceRHI, SwapchainRHI as _, util,
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

const HEAP_DESCRIPTORS: usize = 64;

/// Per vertex data that is not the position. Stored as an array of structs.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct VertexData {
    normal: [f32; 3],
    uv: [f32; 2],
}

struct Mesh {
    positions: Vec<f32>,
    indices: Vec<u32>,
    vertex_data: Vec<VertexData>,
}

struct MeshGpu {
    positions: rhi::GpuPtr,
    positions_addr: u64,
    indices: rhi::GpuPtr,
    vertex_data: rhi::GpuPtr,
    vertex_data_addr: u64,
}

/// Parse the obj into de-duplicated vertices. obj indexes positions, normals and texture coordinates
/// separately, so every distinct `(position, texcoord, normal)` triple becomes one vertex.
fn parse_obj(bytes: &[u8]) -> Mesh {
    let model = obj::ObjData::load_buf(Cursor::new(bytes)).expect("Unable to parse obj");

    let mut mesh = Mesh {
        positions: Vec::new(),
        indices: Vec::new(),
        vertex_data: Vec::new(),
    };
    let mut seen: HashMap<(usize, Option<usize>, Option<usize>), u32> = HashMap::new();

    for polygon in model
        .objects
        .iter()
        .flat_map(|object| &object.groups)
        .flat_map(|group| &group.polys)
    {
        let corners = &polygon.0;
        // Triangulate as a fan, which is a no-op for triangles
        for i in 1..corners.len().saturating_sub(1) {
            for corner in [&corners[0], &corners[i], &corners[i + 1]] {
                let key = (corner.0, corner.1, corner.2);
                let index = *seen.entry(key).or_insert_with(|| {
                    let index = mesh.vertex_data.len() as u32;
                    mesh.positions.extend_from_slice(&model.position[corner.0]);
                    // obj texture coordinates have their origin at the bottom left, Vulkan at the top left
                    let uv = corner.1.map_or([0.0; 2], |t| {
                        [model.texture[t][0], 1.0 - model.texture[t][1]]
                    });
                    mesh.vertex_data.push(VertexData {
                        normal: corner.2.map_or([0.0; 3], |n| model.normal[n]),
                        uv,
                    });
                    index
                });
                mesh.indices.push(index);
            }
        }
    }
    mesh
}

fn upload_data(device: &mut rhi::Device, data: &[u8]) -> rhi::GpuPtr {
    let buffer = device.create_buffer(&rhi::BufferInfo {
        memory: rhi::Memory::Upload,
        size: data.len() as u64,
        usage: rhi::BufferUsage::General,
    });
    unsafe {
        ptr::copy_nonoverlapping(data.as_ptr(), device.buffer_host_addr(buffer), data.len());
    }
    buffer
}

/// Create the three mesh buffers
fn create_mesh_buffers(device: &mut rhi::Device, mesh: &Mesh) -> MeshGpu {
    let positions = upload_data(device, bytemuck::cast_slice(&mesh.positions));
    let vertex_data = upload_data(device, bytemuck::cast_slice(&mesh.vertex_data));
    let positions_addr = device.buffer_device_addr(positions);
    let vertex_data_addr = device.buffer_device_addr(vertex_data);
    MeshGpu {
        positions,
        vertex_data,
        indices: upload_data(device, bytemuck::cast_slice(&mesh.indices)),
        positions_addr,
        vertex_data_addr,
    }
}

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
    // scene
    texture: rhi::GpuImage,
    mesh: MeshGpu,
    ubo: rhi::GpuPtr,
    bindless: TextureHeap,
}

/// How the texture is made available to shaders
enum TextureHeap {
    /// Classic bindless. Textures are accessed through a single descriptor set.
    Classic,
    /// Descriptor heaps. Textures are accessed through a user written descriptor heap.
    Heaps {
        resource_heap: rhi::GpuPtr,
        sampler_heap: rhi::GpuPtr,
    },
}

fn create_buffer_with_data(
    device: &mut rhi::Device,
    usage: rhi::BufferUsage,
    data: &[u8],
) -> rhi::GpuPtr {
    let buffer = device.create_buffer(&rhi::BufferInfo {
        memory: rhi::Memory::Upload,
        size: data.len() as u64,
        usage,
    });
    unsafe {
        ptr::copy_nonoverlapping(data.as_ptr(), device.buffer_host_addr(buffer), data.len());
    }
    buffer
}

fn upload_texture_blocking(
    device: &mut rhi::Device,
    queue: &mut rhi::Queue,
    format: rhi::Format,
) -> rhi::GpuImage {
    let png = include_bytes!("viking_room.png");
    let pixels = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .expect("Unable to parse rust.png")
        .to_rgba8();
    let (width, height) = pixels.dimensions();

    let texture = device.create_image(&rhi::ImageInfo {
        dimensions: [width, height, 1],
        format,
        usage: rhi::ImageUsage::Sampled,
        ..Default::default()
    });
    let staging = create_buffer_with_data(device, rhi::BufferUsage::General, pixels.as_raw());

    // Can also transition to general instead one time, which should be optimal on most modern platforms
    let upload_semaphore = device.create_semaphore(0);
    let mut command_buffer = queue.begin_recording(0);
    command_buffer.image_barrier(
        rhi::Stage::TopOfPipe,
        rhi::Stage::Transfer,
        texture,
        rhi::ImageLayout::TransferDstOptimal,
    );
    command_buffer.copy_to_image(staging, texture.full_region());
    command_buffer.image_barrier(
        rhi::Stage::Transfer,
        rhi::Stage::FragmentShader,
        texture,
        rhi::ImageLayout::ShaderReadOnlyOptimal,
    );
    command_buffer.signal_after(rhi::Stage::BottomOfPipe, &upload_semaphore, 1);
    unsafe { queue.submit(&[command_buffer]).unwrap() };

    // Wait for the submission, after which the staging buffer is no longer in use
    upload_semaphore.wait(1);
    unsafe { device.destroy_buffer(staging) };

    texture
}

impl TextureHeap {
    fn new_classic(_device: &mut rhi::Device) -> Self {
        return Self::Classic;
    }

    fn new_descriptor_heap(device: &mut rhi::Device) -> Self {
        let props = device.get_descriptor_heap_properties();
        let descriptors_total_size = (HEAP_DESCRIPTORS * util::DESCRIPTOR_STRIDE) as u64;
        let res_alignment = props.resource_heap_alignment;
        let sampler_alignment = props.sampler_heap_alignment;
        let resource_size = (props.min_resource_heap_reserved_range + descriptors_total_size)
            .div_ceil(res_alignment)
            * res_alignment
            + res_alignment;
        let sampler_size = (props.min_sampler_heap_reserved_range + descriptors_total_size)
            .div_ceil(sampler_alignment)
            * sampler_alignment
            + sampler_alignment;

        let resource_heap = device.create_buffer(&rhi::BufferInfo {
            memory: rhi::Memory::Upload,
            size: resource_size,
            usage: rhi::BufferUsage::DescriptorHeap,
        });
        let sampler_heap = device.create_buffer(&rhi::BufferInfo {
            memory: rhi::Memory::Upload,
            size: sampler_size,
            usage: rhi::BufferUsage::DescriptorHeap,
        });

        Self::Heaps {
            resource_heap,
            sampler_heap,
        }
    }

    fn write_sampled_image_descriptor(
        &mut self,
        device: &mut rhi::Device,
        index: usize,
        texture: rhi::GpuImage,
        sampler: &rhi::SamplerInfo,
    ) {
        match self {
            TextureHeap::Classic => {
                device.write_sampled_image_descriptor(index, texture, sampler);
            }
            TextureHeap::Heaps {
                resource_heap,
                sampler_heap,
            } => {
                // Both of these update the pointers so that they are aligned and can be bound
                let mut resource_view = util::align_resource_heap(device, resource_heap);
                let mut sampler_view = util::align_sampler_heap(device, sampler_heap);

                let image_descriptor = device.get_image_descriptor(texture);
                let sampler_descriptor = device.get_sampler_descriptor(sampler);
                unsafe {
                    resource_view.as_mut_sliced(0, 1)[index] = image_descriptor;
                    sampler_view.as_mut_sliced(0, 1)[index] = sampler_descriptor;
                }
            }
        }
    }

    fn bind_heap(&mut self, command_buffer: &mut rhi::CommandBuffer) {
        // Only user managed descriptor heaps need to be bound
        if let TextureHeap::Heaps {
            resource_heap,
            sampler_heap,
        } = self
        {
            command_buffer.bind_descriptor_heap(*resource_heap, *sampler_heap);
        }
    }
}

#[repr(C)]
struct Ubo {
    model: Matrix4<f32>,
    view: Matrix4<f32>,
    proj: Matrix4<f32>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::NoUninit)]
struct Push {
    pos: u64,
    data: u64,
    ubo: u64,
    tex_id: [u32; 2],
}

impl Ubo {
    fn new(time: f32, aspect_ratio: f32) -> Self {
        let model = Matrix4::from_angle_z(cgmath::Deg::turn_div_4() * time);
        let view = Matrix4::look_at_rh(
            point3(2.0f32, 2.0f32, 2.0f32),
            cgmath::Point3::origin(),
            Vector3::unit_z(),
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

            let window_data = unsafe {
                rhi::WindowSystemData::from_raw_handles(raw_display_handle, raw_window_handle)
            };

            let surface_formats = device.enumerate_surface_formats(&window_data);
            let sdr_formats = || {
                surface_formats
                    .iter()
                    .filter(|format| matches!(format.color_space, rhi::ColorSpace::SrgbNonlinear))
            };
            // Prefer an sRGB format, so the hardware encodes what the shader outputs
            let surface_format = sdr_formats()
                .find(|format| {
                    matches!(
                        format.format,
                        rhi::Format::B8G8R8A8_Srgb | rhi::Format::R8G8B8A8_Srgb
                    )
                })
                .or_else(|| sdr_formats().next())
                .expect("No SDR capable surface.");
            let surface_is_srgb = matches!(
                surface_format.format,
                rhi::Format::B8G8R8A8_Srgb | rhi::Format::R8G8B8A8_Srgb
            );

            let texture_format = if surface_is_srgb {
                rhi::Format::R8G8B8A8_Srgb
            } else {
                rhi::Format::R8G8B8A8_Unorm
            };

            let mut queue =
                device.create_queue(rhi::QueueType::Graphics, FRAMES_IN_FLIGHT as u32, 1);

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

            let mesh = parse_obj(include_bytes!("viking_room.obj"));
            println!(
                "Parsed {} vertices and {} triangles",
                mesh.vertex_data.len(),
                mesh.indices.len() / 3
            );
            let mesh_buffers = create_mesh_buffers(&mut device, &mesh);
            let ubo = device.create_buffer(&rhi::BufferInfo {
                memory: rhi::Memory::Upload,
                size: size_of::<Ubo>() as u64 * FRAMES_IN_FLIGHT,
                usage: rhi::BufferUsage::General,
            });

            let texture = upload_texture_blocking(&mut device, &mut queue, texture_format);

            let mut bindless = if use_heaps {
                TextureHeap::new_descriptor_heap(&mut device)
            } else {
                TextureHeap::new_classic(&mut device)
            };

            bindless.write_sampled_image_descriptor(
                &mut device,
                0,
                texture,
                &rhi::SamplerInfo {
                    address_mode: [rhi::SamplerAddressMode::ClampToEdge; 3],
                    ..Default::default()
                },
            );

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
                        cull: rhi::Cull::CW,
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
                texture,
                mesh: mesh_buffers,
                ubo,
                bindless,
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

                    handles.bindless.bind_heap(&mut command_buffer);

                    command_buffer.begin_rendering(&rhi::RenderingInfo {
                        color_attachments: &[rhi::Attachment {
                            image: swapchain_image,
                            ..Default::default()
                        }],
                        ..Default::default()
                    });
                    command_buffer.set_pipeline(pipeline);
                    let ubo_addr = handles._device.buffer_device_addr(handles.ubo)
                        + (command_pool * size_of::<Ubo>() as u64);
                    let ubo_host = handles._device.buffer_host_addr(handles.ubo);
                    unsafe {
                        (ubo_host as *mut Ubo)
                            .add(command_pool as usize)
                            .write(Ubo::new(0.0, 800.0 / 600.0));
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
