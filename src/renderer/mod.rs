#![allow(unsafe_op_in_unsafe_fn)]
pub mod shader;
#[macro_use]
pub mod vulkan;

use ash;
use ash::vk;
use core::slice;
use std::path::Path;
use std::sync::Arc;
use winit::raw_window_handle::RawDisplayHandle;
use winit::raw_window_handle::RawWindowHandle;

use crate::renderer::shader::Slangc;
use crate::renderer::vulkan::debug::DebugName as _;
use crate::renderer::vulkan::device::Device2;
use crate::renderer::vulkan::device::GpuPtr;
use crate::renderer::vulkan::device::QueueRef;
use crate::renderer::vulkan::device::ShaderIR2;
use crate::renderer::vulkan::device::TimelineSemaphore;
use crate::renderer::vulkan::*;

pub struct Renderer {
    device: Device2,
    graphics_queue: QueueRef,
    state: RenderState,
}

struct RenderState {
    frame_index: u64,
    render_data: RenderData,
    frame_semaphore: TimelineSemaphore,
}

impl RenderState {}

const FRAMES_IN_FLIGHT: u64 = 2;

impl Renderer {
    pub unsafe fn new(
        raw_display_handle: RawDisplayHandle,
        raw_window_handle: RawWindowHandle,
    ) -> Self {
        let mut device_rhi = Device2::new_with_presentation(raw_display_handle, raw_window_handle);

        let graphics_queue =
            device_rhi.create_queue(QueueType::Graphics, FRAMES_IN_FLIGHT as u32, 1);
        let frame_semaphore = device_rhi.create_semaphore(0);
        let render_data = setup_render_data(&mut device_rhi);
        Self {
            device: device_rhi,
            state: RenderState {
                frame_index: 1,
                render_data: render_data,
                frame_semaphore,
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
        let mut command_buffer = self
            .graphics_queue
            .begin_recording_presentation(command_pool as u32, *next_frame)?;

        command_buffer.begin_render_pass(&RenderPassDescription::default());

        command_buffer.set_pipeline(&render_data.pipeline);
        command_buffer.draw_indexed_instanced(&[], render_data.indices, 1);

        command_buffer.end_render_pass();

        command_buffer.signal_after(
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            &self.state.frame_semaphore,
            *next_frame,
        );

        self.graphics_queue.submit_and_present(&command_buffer)?;
        *next_frame += 1;
        Ok(())
    }
}

pub struct RenderData {
    pipeline: Pipeline,
    indices: GpuPtr,
}

fn setup_render_data(device: &mut Device2) -> RenderData {
    //let compiler = Slangc::new();
    //let shader = compiler
    //    .compile(Path::new("res/shaders/triangle.slang"))
    //    .unwrap();
    let text = include_bytes!("../../triangle.spv");
    let vertex_ir = ShaderIR2 {
        bytes: text, //&shader.spirv.text,
        entry: c"triangle_vert",
    };
    let frag_ir = ShaderIR2 {
        bytes: text, //&shader.spirv.text,
        entry: c"triangle_frag",
    };
    let description = RasterDescription {
        color_formats: &[vk::Format::R8G8B8A8_SRGB],
        cull: Cull::NONE,
        ..Default::default()
    };

    let pipeline = device.create_graphics_pipeline(&vertex_ir, &frag_ir, &description);
    let buffer_desc = BufferDesc {
        memory: Memory::Default,
        size: 4 * 3,
        usage: BufferUsage::Index,
    };
    let indices = device.create_buffer(&buffer_desc);
    device.with_mapping(indices, |bytes| {
        let indices: &mut [u32] = bytemuck::cast_slice_mut(bytes);
        indices[0] = 0;
        indices[1] = 1;
        indices[2] = 2;
    });

    RenderData { pipeline, indices }
}

pub struct UiData {
    pub triangles: Vec<egui::ClippedPrimitive>,
    pub textures: egui::TexturesDelta,
}

#[derive(bytemuck::Zeroable, bytemuck::Pod, Clone, Copy)]
#[repr(C)]
struct EguiVertex {
    pos: [f32; 2],  // 8
    uv: [f32; 2],   // 16
    color: [u8; 4], // 20
}

#[derive(bytemuck::Zeroable, bytemuck::Pod, Clone, Copy)]
#[repr(C)]
struct UiPushData {
    mesh_id: i32,
    texture_id: i32,
}

//struct UiDataGpu {
//    vertex_buffer: Buffer,
//    index_buffer: Buffer,
//    textures: Vec<Image>,
//    pipeline: Pipeline,
//    push_data: UiPushData,
//}
//
//impl UiDataGpu {}
//
//impl UiDataGpu {
//    const fn vertex_size() -> usize {
//        size_of::<egui::epaint::Vertex>()
//    }
//}
//
//impl UiRenderer {
//    pub fn new() -> Self {
//        Self { data: None }
//    }
//
//    fn create_or_update_buffers(
//        &mut self,
//        renderer: &mut Renderer,
//        vertex_data: &[u8],
//        index_data: &[u8],
//    ) {
//        let device = &renderer._device;
//        let vertex_buffer_len = vertex_data.len();
//        let index_buffer_len = index_data.len();
//        if let None = self.data {
//            let vertex_buffer = renderer.descriptor_heap.create_buffer(
//                vertex_buffer_len as u64,
//                vk::BufferUsageFlags::STORAGE_BUFFER
//                    | vk::BufferUsageFlags::TRANSFER_DST
//                    | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
//            );
//
//            let index_buffer = Buffer::new(
//                Arc::clone(&device),
//                index_buffer_len as u64,
//                vk::BufferUsageFlags::INDEX_BUFFER | vk::BufferUsageFlags::TRANSFER_DST,
//            )
//            .unwrap()
//            .debug_name(c"UI index buffer");
//
//            let image = Image::new(
//                Arc::clone(&device),
//                vk::Extent2D {
//                    width: 64,
//                    height: 64,
//                },
//            )
//            .debug_name(c"UI texture");
//
//            let shader_module = Arc::clone(&device).create_shader_module(
//                &Slangc::new()
//                    .compile(Path::new("res/shaders/2d/egui.slang"))
//                    .unwrap(),
//            );
//            let pipeline = renderer
//                .pipeline_cache
//                .create_graphics_pipeline(
//                    &TargetFormat {
//                        color: &[vk::Format::R8G8B8A8_SRGB],
//                        depth: None,
//                        stencil: None,
//                    },
//                    &shader_module,
//                )
//                .unwrap()
//                .debug_name(c"Ui pipeline");
//
//            todo!();
//            //self.data = Some(UiDataGpu {
//            //    vertex_buffer,
//            //    index_buffer,
//            //    textures: vec![],
//            //    pipeline,
//            //    push_data: UiPushData {
//            //        mesh_id,
//            //        texture_id: -1,
//            //    },
//            //});
//        }
//
//        if let Some(ref gpu_data) = self.data {
//            // Need resize
//            gpu_data.vertex_buffer.copy_to_buffer(vertex_data, 0);
//            gpu_data.index_buffer.copy_to_buffer(index_data, 0);
//        }
//    }
//
//    pub fn update(&mut self, data: UiData, renderer: &mut Renderer) {
//        for primitive in &data.triangles {
//            match &primitive.primitive {
//                egui::epaint::Primitive::Mesh(mesh) => {
//                    let vertex_data: &[u8] = &bytemuck::cast_slice(&mesh.vertices);
//                    let index_data: &[u8] = &bytemuck::cast_slice(&mesh.indices);
//
//                    self.create_or_update_buffers(renderer, vertex_data, index_data);
//                }
//                egui::epaint::Primitive::Callback(paint_callback) => panic!("Not supported"),
//            }
//            break;
//        }
//    }
//}
//
//// Probably better to actually implement this on GpuData instead
//impl Renderable for UiRenderer {
//    fn record_command_buffer(&self, command_buffer: &RecordingCommandBuffer) {
//        if let Some(ref gpu_data) = self.data {
//            command_buffer.cmd_bind_pipeline(vk::PipelineBindPoint::GRAPHICS, &gpu_data.pipeline);
//
//            let index_buffer = &gpu_data.index_buffer;
//            command_buffer.cmd_bind_index_buffer2(
//                index_buffer,
//                0,
//                index_buffer.len(),
//                vk::IndexType::UINT32,
//            );
//            let index_count = index_buffer.len() as u32 / 4;
//
//            command_buffer.cmd_push_data(bytemuck::bytes_of(&gpu_data.push_data));
//
//            command_buffer.cmd_draw_indexed(
//                index_count,
//                1,
//                0,
//                0, // ?
//                0,
//            );
//        }
//    }
//}
//
