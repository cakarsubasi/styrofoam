/// command pools, command buffers, pipeline creation, shaders
pub mod command;
/// Debug messenger and object naming
pub mod debug;
/// The device handle and constructors for handles obtained from the device
pub mod device;
/// The instance handle and constructors for handles obtained from the instance
pub mod instance;
/// Image and buffer allocation, copying, memory mapping and related functionality
//pub mod memory;
/// Swapchain, surface, and presentation related functionality
pub mod swapchain;
/// Fences and semaphores
pub mod sync;

use std::path::Path;

use crate::renderer::vulkan::device::{GpuPtr, ShaderIR2};
use crate::renderer::vulkan::swapchain::SwapchainImage;

pub use self::command::{CommandBuffer, ImageView, Pipeline, ShaderModule, traits::*};
pub use self::device::Device;
pub use self::instance::Instance;
pub use self::swapchain::{PresentationContext, Surface, Swapchain, TargetFormat};
pub use self::sync::{Fence, Semaphore};

pub struct DepthStencilState {
    //mode: DepthFlags,
    pub depth_test: ash::vk::CompareOp,
    pub depth_bias: f32,
    pub depth_bias_slope_factor: f32,
    pub depth_bias_clamp: f32,
}
pub struct BlendState {
    color_op: ash::vk::BlendOp,
    src_color_factor: ash::vk::BlendFactor,
    dst_color_factor: ash::vk::BlendFactor,
    alpha_op: ash::vk::BlendOp,
    src_alpha_factor: ash::vk::BlendFactor,
    dst_alpha_factor: ash::vk::BlendFactor,
}

pub type LoadOp = ash::vk::AttachmentLoadOp;
pub type StoreOp = ash::vk::AttachmentStoreOp;
pub type Clear = ash::vk::ClearValue; // Should probably newtype an enum since unions are not ergonomic in rust

#[derive(Clone, Copy)]
pub enum Framebuffer {
    Image(GpuPtr),
    Swapchain(SwapchainImage),
}

pub struct RenderTarget {
    pub image: Framebuffer,
    pub load_op: LoadOp,
    pub store_op: StoreOp,
    pub clear_value: ash::vk::ClearValue,
}
impl Default for RenderTarget {
    fn default() -> Self {
        Self {
            image: Framebuffer::Image(GpuPtr::null()),
            load_op: LoadOp::CLEAR,
            store_op: StoreOp::STORE,
            clear_value: Default::default(),
        }
    }
}

pub struct RenderPassDescription {
    pub color_targets: Vec<RenderTarget>,
    pub depth_target: Option<RenderTarget>,
    pub stencil_target: Option<RenderTarget>,
}

pub enum Topology {
    TriangleList,
}
pub enum Cull {
    CCW,
    CW,
    BOTH,
    NONE,
}
pub struct RasterDescription<'a> {
    pub topology: Topology,
    pub cull: Cull,
    pub alpha_to_coverage: bool,
    pub depth_format: ash::vk::Format,
    pub stencil_format: ash::vk::Format,
    pub color_formats: &'a [ash::vk::Format],
}
impl Default for RasterDescription<'_> {
    fn default() -> Self {
        Self {
            topology: Topology::TriangleList,
            cull: Cull::CCW,
            alpha_to_coverage: false,
            depth_format: ash::vk::Format::UNDEFINED,
            stencil_format: ash::vk::Format::UNDEFINED,
            color_formats: &[],
        }
    }
}

#[derive(Clone, Copy)]
pub enum Memory {
    Default,
    DeviceOnly,
    HostCoherent,
}
#[derive(Clone, Copy)]
pub enum BufferUsage {
    Uniform,
    Storage,
    Index,
    DescriptorHeap,
}
pub struct BufferDesc {
    pub memory: Memory,
    pub size: u64,
    pub usage: BufferUsage,
}
#[derive(Clone, Copy)]
pub struct ImageDesc {
    pub ty: ash::vk::ImageType,
    pub dimensions: U32_3,
    pub mip_count: u32,
    pub layer_count: u32,
    pub sample_count: u32,
    pub format: ash::vk::Format,
    pub usage: ash::vk::ImageUsageFlags,
}
impl Default for ImageDesc {
    fn default() -> Self {
        Self {
            ty: ash::vk::ImageType::TYPE_2D,
            dimensions: [0, 0, 0],
            mip_count: 1,
            layer_count: 1,
            sample_count: 1,
            format: ash::vk::Format::UNDEFINED,
            usage: ash::vk::ImageUsageFlags::empty(),
        }
    }
}

type U32_3 = [u32; 3];

// Probably shouldn't have a public InstanceRHI and instead create a new instance with every device
pub trait InstanceRHI {
    type Device;
    type RawWindowHandle;

    fn create_device(&mut self) -> Self::Device;

    fn create_device_with_presentation(&mut self, rwh: Self::RawWindowHandle) -> Self::Device;
}

pub enum QueueType {
    Graphics, // Graphics, Compute, and Copy
    Compute,  // Compute and Copy
    Copy,     // Copy only
}

pub trait DeviceRHI {
    //type ShaderText;
    type Pipeline;
    type Semaphore: SemaphoreRHI;
    type Queue: QueueRHI;
    type GpuPtr;

    fn create_buffer(&mut self, details: &BufferDesc) -> Self::GpuPtr;
    fn create_image(&mut self, details: &ImageDesc) -> Self::GpuPtr;
    fn with_mapping(&mut self, ptr: Self::GpuPtr, f: fn(&mut [u8]));
    fn delete_ptr(&mut self, ptr: Self::GpuPtr);

    fn create_queue(
        &mut self,
        ty: QueueType,
        command_pools: u32,
        command_buffers_per_pool: u32,
    ) -> Self::Queue;

    fn create_semaphore(&mut self, initial_value: u64) -> Self::Semaphore;
    fn wait_semaphores(&self, semaphores: &[Self::Semaphore], values: &[u64]);

    fn create_compute_pipeline(&mut self, compute_ir: &ShaderIR2) -> Self::Pipeline;
    fn create_graphics_pipeline(
        &mut self,
        vertex_ir: &ShaderIR2,
        fragment_ir: &ShaderIR2,
        description: &RasterDescription,
    ) -> Self::Pipeline;
    fn create_meshlet_pipeline(
        &mut self,
        meshlet_ir: &ShaderIR2,
        fragment_ir: &ShaderIR2,
        description: &RasterDescription,
    ) -> Self::Pipeline;
}

pub trait ShaderCompilerRHI {
    type ShaderText;

    fn compile(&mut self, path: &Path) -> Vec<Self::ShaderText>;
}

pub trait SemaphoreRHI {
    fn wait(&mut self, value: u64);
}

pub trait QueueRHI {
    type CommandBuffer: CommandRHI;

    fn begin_recording(&mut self, command_pool: u32) -> Self::CommandBuffer;
    fn submit(&mut self, command_buffers: &[Self::CommandBuffer]);
}

pub type Stage = ash::vk::PipelineStageFlags2;

pub enum ShaderStage {
    Vertex,
    Fragment,
    Compute,
    Mesh,
}

pub type PushData<'a> = &'a [u8];

pub trait CommandRHI {
    type GpuPtr;
    type Pipeline;
    type Semaphore;

    fn mem_cpy(&mut self, dst: Self::GpuPtr, src: Self::GpuPtr);

    //fn copy_to_texture();
    //fn copy_from_texture();

    fn barrier(&mut self, before: Stage, after: Stage /* something goes here */);
    fn signal_after(&mut self, stage: Stage, semaphore: &Self::Semaphore, value: u64);
    fn wait_before(&mut self, stage: Stage, semaphore: &Self::Semaphore, value: u64);

    fn set_pipeline(&mut self, pipeline: &Self::Pipeline);
    // Can we just set these dynamically or do we need to create them in advance?
    fn set_depth_stencil_state(&mut self, state: DepthStencilState);
    fn set_blend_state(&mut self, state: BlendState);

    fn gpu_dispatch(&mut self, data: PushData, dimensions: U32_3);
    fn gpu_dispatch_indirect(&mut self, data: PushData, indirect_buffer: Self::GpuPtr);

    fn begin_render_pass(&mut self, desc: &RenderPassDescription);
    fn end_render_pass(&mut self);

    fn draw_indexed_instanced(&mut self, data: PushData, indices: Self::GpuPtr, instances: u32);
    fn draw_indexed_instanced_indirect(
        &mut self,
        data: PushData,
        indices: Self::GpuPtr,
        indirect: Self::GpuPtr,
    );
    // let's skip this one for now
    // fn draw_indexed_instanced_indirect_multi(&mut self, ...);
    fn draw_meshlets(&mut self, data: PushData, dimension: U32_3);
    fn draw_meshlets_indirect(&mut self, data: PushData, dim_data: Self::GpuPtr);
}
