use core::slice;
use std::collections::HashMap;
use std::fs::File;
use std::ptr;
use styro_rendergraph::RenderGraph;
use styro_rhi::Attachment;
use styro_rhi::BufferInfo;
use styro_rhi::BufferUsage;
use styro_rhi::ColorSpace;
use styro_rhi::CommandBuffer;
use styro_rhi::CommandRHI;
use styro_rhi::Cull;
use styro_rhi::Device;
use styro_rhi::DeviceRHI;
use styro_rhi::Error;
use styro_rhi::Format;
use styro_rhi::GpuPtr;
use styro_rhi::ImageBlitInfo;
use styro_rhi::ImageDescriptor;
use styro_rhi::ImageInfo;
use styro_rhi::ImageLayout;
use styro_rhi::Memory;
use styro_rhi::Pipeline;
use styro_rhi::Queue;
use styro_rhi::QueueRHI;
use styro_rhi::QueueType;
use styro_rhi::RasterDescription;
use styro_rhi::RenderingInfo;
use styro_rhi::SamplerDescriptor;
use styro_rhi::SamplerInfo;
use styro_rhi::Semaphore;
use styro_rhi::ShaderIR;
use styro_rhi::Stage;
use styro_rhi::Swapchain;
use styro_rhi::SwapchainCommandRHI as _;
use styro_rhi::SwapchainDeviceRHI as _;
use styro_rhi::SwapchainInfo;
use styro_rhi::SwapchainRHI as _;
use styro_rhi::WindowSystem;
use styro_rhi::WindowSystemData;
use styro_rhi::instance::DeviceCreateInfo;
use styro_rhi::read_spv;
use winit::raw_window_handle::RawDisplayHandle;
use winit::raw_window_handle::RawWindowHandle;

use crate::renderer::ui::UiData;
use crate::renderer::ui::UiRenderer;

mod ui;

pub struct Renderer {
    device: Device,
    graphics_queue: Queue,
    swapchain: Swapchain,
    state: RenderState,
}

pub struct Renderer2 {
    render_graph: RenderGraph,

    persistent_renderers: PersistentRenderers,
}

impl Renderer2 {
    fn new(raw_display_handle: RawDisplayHandle, raw_window_handle: RawWindowHandle) -> Self {
        let mut render_graph = RenderGraph::new(raw_display_handle, raw_window_handle);

        let persistent_renderers = PersistentRenderers {
            ui: UiRenderer::new(render_graph.device_mut()),
        };

        Self {
            render_graph,
            persistent_renderers,
        }
    }

    fn run(&mut self, ui_data: UiData) {
        let mut frame = self.render_graph.new_frame();

        let swapchain_image = frame.swapchain_image();

        self.persistent_renderers
            .ui
            .run(&mut frame, swapchain_image, ui_data);
    }
}

pub struct PersistentRenderers {
    ui: UiRenderer,
}

struct RenderState {
    frame_index: u64,
    render_data: TextureRenderData,
    frame_semaphore: Semaphore,
    images: Vec<GpuPtr>,
}

impl RenderState {}

const FRAMES_IN_FLIGHT: u64 = 2;

impl Renderer {
    pub unsafe fn new(
        raw_display_handle: RawDisplayHandle,
        raw_window_handle: RawWindowHandle,
    ) -> Self {
        let mut device = Device::new(&DeviceCreateInfo {
            device_idx: 0,
            device_features: styro_rhi::instance::DeviceFeatures {
                descriptor_heap: true,
                dynamic_blend_state: true,
                ..Default::default()
            },
            presentation: Some(WindowSystem::Windows),
        });

        let graphics_queue = device.create_queue(QueueType::Graphics, FRAMES_IN_FLIGHT as u32, 1);

        let swapchain = device
            .create_swapchain(
                &graphics_queue,
                &unsafe {
                    WindowSystemData::from_raw_handles(raw_display_handle, raw_window_handle)
                },
                &SwapchainInfo {
                    size: FRAMES_IN_FLIGHT as u32,
                    format: Format::R8G8B8A8_Srgb,
                    color_space: ColorSpace::SrgbNonlinear,
                    preferred_extent: [0, 0],
                },
            )
            .unwrap();
        let images: Vec<_> = (0..2)
            .map(|_| {
                device.create_image(&ImageInfo {
                    dimensions: [800, 600, 1],
                    format: Format::R8G8B8A8_Unorm,
                    usage: styro_rhi::ImageUsage::Attachment,
                    ..Default::default()
                })
            })
            .collect();
        let frame_semaphore = device.create_semaphore(0);
        let render_data = TextureRenderData::new(&mut device);
        //UiDataGpu::new(&mut device_rhi);
        Self {
            device,
            swapchain,
            state: RenderState {
                frame_index: 1,
                render_data: render_data,
                frame_semaphore,
                images,
            },
            graphics_queue,
        }
    }

    pub fn request_redraw(&mut self) -> Result<(), Error> {
        let render_data = &self.state.render_data;
        let next_frame = &mut self.state.frame_index;
        let command_pool = *next_frame % FRAMES_IN_FLIGHT;

        if *next_frame > FRAMES_IN_FLIGHT {
            self.device.wait_semaphores(
                slice::from_ref(&self.state.frame_semaphore),
                &[*next_frame - FRAMES_IN_FLIGHT],
            );
        }
        let swapchain_image = self.swapchain.acquire_next_image()?;
        let render_target = self.state.images[(*next_frame % FRAMES_IN_FLIGHT) as usize];

        let mut command_buffer = self.graphics_queue.begin_recording(command_pool as u32);
        command_buffer.begin_presenting(swapchain_image);

        if *next_frame == 1 {
            render_data.upload(&mut self.device, &mut command_buffer);
        } else {
            render_data.draw(&self.device, &mut command_buffer, render_target);
        }

        command_buffer.image_barrier(
            Stage::ColorAttachmentOutput,
            Stage::Blit,
            render_target,
            ImageLayout::General,
        );
        command_buffer.image_barrier(
            Stage::Blit,
            Stage::BottomOfPipe,
            swapchain_image,
            ImageLayout::TransferDstOptimal,
        );

        command_buffer.blit_image(
            render_target,
            swapchain_image,
            &ImageBlitInfo {
                src_extent: [800, 600, 1],
                dst_extent: [400, 300, 1],
                ..Default::default()
            },
        );

        command_buffer.signal_after(
            Stage::BottomOfPipe,
            &self.state.frame_semaphore,
            *next_frame,
        );

        unsafe { self.graphics_queue.submit(&[command_buffer])? };
        self.swapchain.present(swapchain_image)?;
        *next_frame += 1;
        Ok(())
    }

    pub fn recreate_swapchain(&mut self) {
        //self.swapchain.recreate_swapchain(&SwapchainInfo {
        //    size: (),
        //    format: (),
        //    color_space: (),
        //    preferred_extent: (),
        //})
    }
}

pub struct TriangleRenderData {
    pipeline: Pipeline,
    indices: GpuPtr,
}

impl TriangleRenderData {
    fn new(device: &mut Device) -> Self {
        let mut spv = File::open("../../../triangle.spv").unwrap();
        let text = read_spv(&mut spv).unwrap();
        let vertex_ir = ShaderIR {
            bytes: &text,
            entry: c"triangle_vert",
        };
        let frag_ir = ShaderIR {
            bytes: &text,
            entry: c"triangle_frag",
        };
        let description = RasterDescription {
            color_formats: &[Format::R8G8B8A8_Srgb],
            cull: Cull::NONE,
            ..Default::default()
        };

        let pipeline =
            unsafe { device.create_graphics_pipeline(&vertex_ir, &frag_ir, &description) };
        let buffer_desc = BufferInfo {
            memory: Memory::HostVisible,
            size: 4 * 6,
            usage: BufferUsage::General,
        };
        let indices = device.create_buffer(&buffer_desc);
        let indices_mapped = device.buffer_host_addr(indices) as *mut u32;
        unsafe {
            let slice = slice::from_raw_parts_mut(indices_mapped, 6);
            slice[0] = 0;
            slice[1] = 1;
            slice[2] = 2;
            slice[3] = 0;
            slice[4] = 2;
            slice[5] = 3;
        }

        TriangleRenderData { pipeline, indices }
    }

    fn draw(&self, command_buffer: &mut CommandBuffer) {
        command_buffer.begin_rendering(&Default::default());
        command_buffer.set_pipeline(&self.pipeline);
        command_buffer.draw_indexed(&[], self.indices, 1);
        command_buffer.end_rendering();
    }
}

pub struct TextureRenderData {
    pipeline: Pipeline,
    indices: GpuPtr,
    staging_buffer: GpuPtr,
    texture: GpuPtr,
    resource_heap: GpuPtr,
    sampler_heap: GpuPtr,
}

impl TextureRenderData {
    fn new(device: &mut Device) -> Self {
        let mut spv = File::open("res/spirv/texture.spv").unwrap();
        let text = read_spv(&mut spv).unwrap();
        let vertex_ir = ShaderIR {
            bytes: &text,
            entry: c"vertMain",
        };
        let frag_ir = ShaderIR {
            bytes: &text,
            entry: c"fragMain",
        };
        let description = RasterDescription {
            color_formats: &[Format::R8G8B8A8_Unorm],
            cull: Cull::NONE,
            ..Default::default()
        };

        let pipeline =
            unsafe { device.create_graphics_pipeline(&vertex_ir, &frag_ir, &description) };
        pipeline.set_object_name(c"bindlessTextureExamplePipeline");
        let buffer_desc = BufferInfo {
            memory: Memory::HostVisible,
            size: 4 * 6,
            usage: BufferUsage::General,
        };
        let indices = device.create_buffer(&buffer_desc);
        device.set_object_name(indices, c"textureExampleIndexBuffer");
        let indices_mapped = device.buffer_host_addr(indices) as *mut u32;
        unsafe {
            let slice = slice::from_raw_parts_mut(indices_mapped, 6);
            const INDEX_BUFFER: [u32; 6] = [0, 1, 2, 0, 2, 3];
            slice.copy_from_slice(&INDEX_BUFFER);
        }

        let image_bytes = include_bytes!("../../../../res/images/rust.png");
        let image =
            image::load_from_memory_with_format(image_bytes, image::ImageFormat::Png).unwrap();
        let image = image.to_rgba8();
        let dimensions = image.dimensions();

        let image_ptr = device.create_image(&ImageInfo {
            dimensions: [dimensions.0, dimensions.1, 1],
            format: Format::R8G8B8A8_Unorm,
            ..Default::default()
        });
        let staging_buffer = device.create_buffer(&BufferInfo {
            memory: Memory::HostVisible,
            size: image_ptr.size as u64,
            usage: BufferUsage::General,
        });
        unsafe {
            let ptr = device.buffer_host_addr(staging_buffer);
            let image = image.iter().as_slice();
            let slice = slice::from_raw_parts_mut(ptr, staging_buffer.size as usize);
            slice[0..image.len()].copy_from_slice(image);
        }

        let heap_props = device.get_descriptor_heap_properties();

        let mut resource_heap = device.create_buffer(&BufferInfo {
            memory: Memory::HostVisible,
            size: heap_props.max_resource_heap_size,
            usage: BufferUsage::DescriptorHeap,
        });
        let mut sampler_heap = device.create_buffer(&BufferInfo {
            memory: Memory::HostVisible,
            size: heap_props.max_sampler_heap_size,
            usage: BufferUsage::DescriptorHeap,
        });

        let sampler_descriptor = device.get_sampler_descriptor(&SamplerInfo {
            ..Default::default()
        });
        unsafe {
            let resource_heap_ptr = device.buffer_host_addr(resource_heap);
            let offset = resource_heap_ptr.align_offset(32);
            resource_heap.offset = offset as u32;
            resource_heap.size -= offset as u32;

            let sampler_heap_ptr = device.buffer_host_addr(sampler_heap);
            let offset = sampler_heap_ptr.align_offset(32);
            sampler_heap.offset = offset as u32;
            sampler_heap.size -= offset as u32;
            let sampler_slice = slice::from_raw_parts_mut(
                sampler_heap_ptr.byte_add(offset),
                (sampler_heap.size as usize) / 32 * 32,
            );
            let sampler_slice: &mut [SamplerDescriptor] = bytemuck::cast_slice_mut(sampler_slice);
            sampler_slice[0] = sampler_descriptor;
        }

        Self {
            pipeline,
            indices,
            staging_buffer,
            texture: image_ptr,
            resource_heap,
            sampler_heap,
        }
    }

    fn upload(&self, device: &mut Device, command_buffer: &mut CommandBuffer) {
        command_buffer.image_barrier(
            Stage::Host,
            Stage::Transfer,
            self.texture,
            ImageLayout::TransferDstOptimal,
        );
        command_buffer.copy_to_image(self.staging_buffer, self.texture);
    }

    fn draw(&self, device: &Device, command_buffer: &mut CommandBuffer, framebuffer: GpuPtr) {
        command_buffer.image_barrier(Stage::Host, Stage::None, self.texture, ImageLayout::General);
        let mut resource_heap = self.resource_heap;
        let image_descriptor = device.get_image_descriptor(self.texture);
        unsafe {
            let resource_heap_ptr = device.buffer_host_addr(resource_heap);
            let offset = resource_heap_ptr.align_offset(32);
            resource_heap.offset = offset as u32;
            resource_heap.size -= offset as u32;
            let resource_slice = slice::from_raw_parts_mut(
                resource_heap_ptr.byte_add(offset),
                (resource_heap.size as usize) / 32 * 32,
            );
            let resource_slice: &mut [ImageDescriptor] = bytemuck::cast_slice_mut(resource_slice);
            resource_slice[0] = image_descriptor;
        }
        command_buffer.bind_descriptor_heap(self.resource_heap, self.sampler_heap);

        command_buffer.begin_rendering(&RenderingInfo {
            color_attachments: &[Attachment {
                image: framebuffer,
                ..Default::default()
            }],
            ..Default::default()
        });
        command_buffer.set_pipeline(&self.pipeline);
        let push_data = [0u32, 0u32];
        command_buffer.draw_indexed(&bytemuck::cast_slice(push_data.as_slice()), self.indices, 1);
        command_buffer.end_rendering();
    }

    fn free(&self, device: &mut Device) {
        unsafe {
            device.destroy_ptr(self.resource_heap);
            device.destroy_ptr(self.sampler_heap);
            device.destroy_ptr(self.texture);
            device.destroy_ptr(self.staging_buffer);
            device.destroy_ptr(self.indices);
        }
    }
}
