use ash::vk;
use core::slice;
use styro_rhi::BufferDesc;
use styro_rhi::BufferUsage;
use styro_rhi::CommandBuffer;
use styro_rhi::CommandRHI;
use styro_rhi::Cull;
use styro_rhi::Device;
use styro_rhi::DeviceRHI;
use styro_rhi::Error;
use styro_rhi::GpuPtr;
use styro_rhi::ImageDesc;
use styro_rhi::Memory;
use styro_rhi::Pipeline;
use styro_rhi::Queue;
use styro_rhi::QueueRHI;
use styro_rhi::QueueType;
use styro_rhi::RasterDescription;
use styro_rhi::RenderPassDescription;
use styro_rhi::Semaphore;
use styro_rhi::ShaderIR;
use styro_rhi::ash;
use winit::raw_window_handle::RawDisplayHandle;
use winit::raw_window_handle::RawWindowHandle;

pub struct Renderer {
    device: Device,
    graphics_queue: Queue,
    state: RenderState,
}

struct RenderState {
    frame_index: u64,
    render_data: TriangleRenderData,
    frame_semaphore: Semaphore,
}

impl RenderState {}

const FRAMES_IN_FLIGHT: u64 = 2;

impl Renderer {
    pub unsafe fn new(
        raw_display_handle: RawDisplayHandle,
        raw_window_handle: RawWindowHandle,
    ) -> Self {
        let mut device_rhi = Device::new_with_presentation(raw_display_handle, raw_window_handle);

        let graphics_queue =
            device_rhi.create_queue(QueueType::Graphics, FRAMES_IN_FLIGHT as u32, 1);
        let frame_semaphore = device_rhi.create_semaphore(0);
        let render_data = TriangleRenderData::new(&mut device_rhi);
        UiDataGpu::new(&mut device_rhi);
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

        render_data.draw(&mut command_buffer);

        command_buffer.end_render_pass();

        command_buffer.signal_after(
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            &self.state.frame_semaphore,
            *next_frame,
        );

        self.graphics_queue.submit(&[command_buffer])?;
        *next_frame += 1;
        Ok(())
    }
}

pub struct TriangleRenderData {
    pipeline: Pipeline,
    indices: GpuPtr,
}

impl TriangleRenderData {
    fn new(device: &mut Device) -> Self {
        //let compiler = Slangc::new();
        //let shader = compiler
        //    .compile(Path::new("res/shaders/triangle.slang"))
        //    .unwrap();
        let text = include_bytes!("../../../triangle.spv");
        let vertex_ir = ShaderIR {
            bytes: text, //&shader.spirv.text,
            entry: c"triangle_vert",
        };
        let frag_ir = ShaderIR {
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
        let indices_mapped = device.buffer_host_ptr(indices) as *mut u32;
        unsafe {
            let slice = slice::from_raw_parts_mut(indices_mapped, 3);
            slice[0] = 0;
            slice[1] = 1;
            slice[2] = 2;
        }

        TriangleRenderData { pipeline, indices }
    }

    fn draw(&self, command_buffer: &mut CommandBuffer) {
        command_buffer.set_pipeline(&self.pipeline);
        command_buffer.draw_indexed_instanced(&[], self.indices, 1);
    }
}

pub struct TextureRenderData {
    pipeline: Pipeline,
    indices: GpuPtr,
    texture: GpuPtr,
}

impl TextureRenderData {
    fn new(device: &mut Device) -> Self {
        let text = include_bytes!("../../../texture.spv");
        let vertex_ir = ShaderIR {
            bytes: text, //&shader.spirv.text,
            entry: c"vertMain",
        };
        let frag_ir = ShaderIR {
            bytes: text, //&shader.spirv.text,
            entry: c"fragMain",
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
        let indices_mapped = device.buffer_host_ptr(indices) as *mut u32;
        unsafe {
            let slice = slice::from_raw_parts_mut(indices_mapped, 3);
            slice[0] = 0;
            slice[1] = 1;
            slice[2] = 2;
        }

        let image_bytes = include_bytes!("../../../res/images/rust.png");
        let image =
            image::load_from_memory_with_format(image_bytes, image::ImageFormat::Png).unwrap();
        let image = image.to_rgba8();
        let dimensions = image.dimensions();
        let image_ptr = device.create_image(&ImageDesc {
            dimensions: [dimensions.0, dimensions.1, 1],
            format: vk::Format::R8G8B8A8_SRGB,
            ..Default::default()
        });

        for pixel in image.iter() {
            todo!();
        }

        //device.with_mapping(image_ptr, |mapped| {
        //    for (dst, src) in mapped.iter_mut().zip(image.iter()) {
        //        *dst = *src;
        //    }
        //});

        //Self { pipeline, indices }
        //
        todo!()
    }

    fn draw(&self, command_buffer: &mut CommandBuffer) {
        command_buffer.set_pipeline(&self.pipeline);
        command_buffer.draw_indexed_instanced(&[], self.indices, 1);
    }
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
    vertices: u64,
    texture: [i32; 2],
}

struct UiDataGpu {
    vertex_buffer: GpuPtr,
    index_buffer: GpuPtr,
    textures: Vec<GpuPtr>,
    pipeline: Pipeline,
}

impl UiDataGpu {
    fn new(device: &mut Device) -> Self {
        let text = include_bytes!("../../../egui.spv");

        let vertex_ir = ShaderIR {
            bytes: text,
            entry: c"vertMain",
        };
        let frag_ir = ShaderIR {
            bytes: text,
            entry: c"fragMain",
        };
        let description = RasterDescription {
            color_formats: &[vk::Format::R8G8B8A8_SRGB],
            cull: Cull::NONE,
            ..Default::default()
        };

        let pipeline = device.create_graphics_pipeline(&vertex_ir, &frag_ir, &description);
        Self {
            vertex_buffer: GpuPtr::null(),
            index_buffer: GpuPtr::null(),
            textures: vec![],
            pipeline,
        }
    }

    fn update(&mut self, data: &UiData) {
        for primitive in &data.triangles {
            match &primitive.primitive {
                egui::epaint::Primitive::Mesh(mesh) => {
                    let vertex_data: &[u8] = &bytemuck::cast_slice(&mesh.vertices);
                    let index_data: &[u8] = &bytemuck::cast_slice(&mesh.indices);

                    //self.create_or_update_buffers(renderer, vertex_data, index_data);
                }
                egui::epaint::Primitive::Callback(paint_callback) => panic!("Not supported"),
            }
            break;
        }
    }
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
