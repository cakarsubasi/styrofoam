/// command pools, command buffers, pipeline creation, shaders
mod command;
/// Debug messenger and object naming
mod debug;
/// The device handle and constructors for handles obtained from the device
mod device;
/// The instance handle and constructors for handles obtained from the instance
mod instance;
/// Swapchain, surface, and presentation related functionality
mod swapchain;

use crate::renderer::vulkan::swapchain::SwapchainImage;

pub use self::command::{CommandBuffer, Pipeline};
pub use self::device::{Device, GpuPtr, Queue, Semaphore, ShaderIR};

#[derive(Debug)]
pub enum Error {
    DeviceLost,
    SurfaceLost,
    SwapchainOutOfDate,
    OtherError(ash::vk::Result),
}

impl From<ash::vk::Result> for Error {
    fn from(value: ash::vk::Result) -> Self {
        match value {
            ash::vk::Result::ERROR_DEVICE_LOST => Error::DeviceLost,
            ash::vk::Result::ERROR_SURFACE_LOST_KHR => Error::SurfaceLost,
            ash::vk::Result::ERROR_OUT_OF_DATE_KHR => Error::SwapchainOutOfDate,
            err => Error::OtherError(err),
        }
    }
}

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

#[derive(Default)]
pub struct RenderPassDescription<'a> {
    pub color_targets: &'a [RenderTarget],
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
    pub dimensions: UVec3,
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
pub struct SamplerDesc {
    pub mag_filter: ash::vk::Filter,
    pub min_filter: ash::vk::Filter,
    pub mipmap_mode: ash::vk::SamplerMipmapMode,
    pub address_mode: [ash::vk::SamplerAddressMode; 3],
    pub anisotropy: f32,
    pub lod_bias: f32,
    pub lod_range: [f32; 2],
    pub compare_op: Option<ash::vk::CompareOp>,
}

type UVec3 = [u32; 3];

pub enum QueueType {
    Graphics, // Graphics, Compute, and Copy
    Compute,  // Compute and Copy
    Copy,     // Copy only
}

pub trait DeviceRHI {
    type Pipeline;
    type Semaphore;
    type Queue: QueueRHI;
    type GpuPtr;

    fn create_buffer(&mut self, details: &BufferDesc) -> Self::GpuPtr;
    fn create_image(&mut self, details: &ImageDesc) -> Self::GpuPtr;

    fn buffer_host_ptr(&self, ptr: Self::GpuPtr) -> *mut u8;
    fn buffer_device_ptr(&self, ptr: Self::GpuPtr) -> u64;

    fn delete_ptr(&mut self, ptr: Self::GpuPtr);

    fn get_image_descriptor(&self, image: Self::GpuPtr) -> [u64; 4];
    fn get_sampler_descriptor(&self, desc: &SamplerDesc) -> [u64; 4];

    fn create_queue(
        &mut self,
        ty: QueueType,
        command_pools: u32,
        command_buffers_per_pool: u32,
    ) -> Self::Queue;

    fn create_semaphore(&mut self, initial_value: u64) -> Self::Semaphore;
    fn wait_semaphores(&self, semaphores: &[Self::Semaphore], values: &[u64]);

    fn create_compute_pipeline(&mut self, compute_ir: &ShaderIR) -> Self::Pipeline;
    fn create_graphics_pipeline(
        &mut self,
        vertex_ir: &ShaderIR,
        fragment_ir: &ShaderIR,
        description: &RasterDescription,
    ) -> Self::Pipeline;
    fn create_meshlet_pipeline(
        &mut self,
        meshlet_ir: &ShaderIR,
        fragment_ir: &ShaderIR,
        description: &RasterDescription,
    ) -> Self::Pipeline;
}

pub trait QueueRHI {
    type CommandBuffer: CommandRHI;

    fn begin_recording(&mut self, command_pool: u32) -> Self::CommandBuffer;
    fn submit(&mut self, command_buffers: &[Self::CommandBuffer]) -> Result<(), Error>;
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

    fn copy_to_texture(&mut self, dst: Self::GpuPtr, src: Self::GpuPtr);
    //fn copy_from_texture();

    fn bind_descriptor_heap(&mut self, resource_heap: Self::GpuPtr, sampler_heap: Self::GpuPtr);

    fn barrier(&mut self, before: Stage, after: Stage /* something goes here */);
    fn signal_after(&mut self, stage: Stage, semaphore: &Self::Semaphore, value: u64);
    fn wait_before(&mut self, stage: Stage, semaphore: &Self::Semaphore, value: u64);

    fn set_pipeline(&mut self, pipeline: &Self::Pipeline);
    fn set_depth_stencil_state(&mut self, state: DepthStencilState);
    fn set_blend_state(&mut self, state: BlendState);

    fn gpu_dispatch(&mut self, data: PushData, dimensions: UVec3);
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
    fn draw_meshlets(&mut self, data: PushData, dimension: UVec3);
    fn draw_meshlets_indirect(&mut self, data: PushData, dim_data: Self::GpuPtr);
}
