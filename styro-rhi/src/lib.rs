/// format is so bloated, it gets its own module
mod format;
/// Querying physical devices and device creation
pub mod instance;
/// The Vulkan backend
mod vulkan;
/// Window system integration
pub mod window;

pub use vulkan::command::{CommandBuffer, Pipeline};
pub use vulkan::device::{Device, GpuPtr, Queue, Semaphore, ShaderIR};
pub use vulkan::swapchain::Swapchain;

pub use format::Format;

/// Re-export ash just in case
pub use ash;

pub use ash::util::read_spv;

#[derive(Debug, Clone, Copy)]
pub enum Error {
    DeviceLost,
    SurfaceLost,
    SwapchainOutOfDate,
    OtherError(i32),
}

/// Comparison operator for depth, stencil, and sampler operations
///
/// Comparison operators are used for:
/// - Sampler depth compare operation. See [SamplerInfo]
/// - Stencil test. See [StencilInfo]
/// - Depth test. See [DepthStencilInfo]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    /// Comparison evaluates `false`
    Never = 0,
    /// Comparison evaluates `reference < test`
    Less = 1,
    /// Comparison evaluates `reference = test`
    Equal = 2,
    /// Comparison evaluates `reference ≤ test`
    LessOrEqual = 3,
    /// Comparison evaluates `reference > test`
    Greater = 4,
    /// Comparison evaluates `reference ≠ test`
    NotEqual = 5,
    /// Comparison evaluates `reference ≥ test`
    GreaterOrEqual = 6,
    /// Comparison evaluates `true`
    Always = 7,
}

/// Stencil operator
///
/// Specifies what happens to the stencil when the conditions for `fail_op`, `pass_op`, and `depth_fail_op` are met.
/// See [StencilInfo].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StencilOp {
    /// Keeps the current value
    Keep = 0,
    /// Value is set to `0`
    Zero = 1,
    /// Value is set to `reference`
    Replace = 2,
    /// Increments the current value clamping to the largest representable unsigned value
    IncrementAndClamp = 3,
    /// Decrements the current value clamping to `0`
    DecrementAndClamp = 4,
    /// Bitwise-inverts the current value
    Invert = 5,
    /// Increments the current value wrapping around to `0` when the largest unsigned value would be exceeded
    IncrementAndWrap = 6,
    /// Decrements the current value wrapping around to the largest unsigned value when the value would go below `0`
    DecrementAndWrap = 7,
}

/// Stencil operation state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StencilInfo {
    /// Comparison operator used for the stencil test
    pub compare_op: CompareOp,
    /// Operation to perform on samples that fail the stencil test
    pub fail_op: StencilOp,
    /// Operation to perform on samples that pass the depth test and stencil test
    pub pass_op: StencilOp,
    /// Operation to perform on samples that pass the stencil tast but fail the depth test
    pub depth_fail_op: StencilOp,
    /// reference value used for the comparison
    pub reference: u32,
    /// The bits participating in the stencil test
    pub read_mask: u32,
    /// The bits that are updated as a result of the stencil test
    pub write_mask: u32,
}
impl Default for StencilInfo {
    fn default() -> Self {
        Self {
            compare_op: CompareOp::Always,
            fail_op: StencilOp::Keep,
            pass_op: StencilOp::Keep,
            depth_fail_op: StencilOp::Keep,
            reference: 0,
            read_mask: u32::MAX,
            write_mask: u32::MAX,
        }
    }
}

/// Depth and stencil operation state
///
#[derive(Debug, Clone, Copy)]
pub struct DepthStencilInfo {
    pub depth_read: bool,
    pub depth_write: bool,
    /// Compare operation
    pub depth_test: CompareOp,
    pub depth_bias: f32,
    pub depth_bias_slope_factor: f32,
    pub depth_bias_clamp: f32,
    pub stencil_front: StencilInfo,
    pub stencil_back: StencilInfo,
}

#[derive(Debug, Clone, Copy)]
pub enum BlendOp {
    Add = 0,
    Subtract = 1,
    ReverseSubtract = 2,
    Min = 3,
    Max = 4,
}

#[derive(Debug, Clone, Copy)]
pub enum BlendFactor {
    Zero = 0,
    One = 1,
    SrcColor = 2,
    OneMinusSrcColor = 3,
    DstColor = 4,
    OneMinusDstColor = 5,
    SrcAlpha = 6,
    OneMinusSrcAlpha = 7,
    DstAlpha = 8,
    OneMinusDstAlpha = 9,
    ConstantColor = 10,
    OneMinusConstantColor = 11,
    ConstantAlpha = 12,
    OneMinusConstantAlpha = 13,
    SrcAlphaSaturate = 14,
    Src1Color = 15,
    OneMinusSrc1Color = 16,
    Src1Alpha = 17,
    OneMinusSrc1Alpha = 18,
}

#[derive(Debug, Clone, Copy)]
pub struct BlendInfo {
    pub color_op: BlendOp,
    pub src_color_factor: BlendFactor,
    pub dst_color_factor: BlendFactor,
    pub alpha_op: BlendOp,
    pub src_alpha_factor: BlendFactor,
    pub dst_alpha_factor: BlendFactor,
}

#[derive(Debug, Clone, Copy)]
pub enum LoadOp {
    Load = 0,
    Clear = 1,
    DontCare = 2,
}

#[derive(Debug, Clone, Copy)]
pub enum StoreOp {
    Store = 0,
    DontCare = 1,
}

#[derive(Debug, Clone, Copy)]
pub enum Clear {
    Color(f32, f32, f32, f32),
    DepthStencil(f32, u32),
}

#[derive(Debug, Clone, Copy)]
pub struct Attachment {
    pub image: GpuPtr,
    pub load_op: LoadOp,
    pub store_op: StoreOp,
    pub clear_value: Clear,
}
impl Default for Attachment {
    fn default() -> Self {
        Self {
            image: GpuPtr::null(),
            load_op: LoadOp::Clear,
            store_op: StoreOp::Store,
            clear_value: Clear::Color(0.0, 0.0, 0.0, 0.0),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct RenderingInfo<'a> {
    pub color_attachments: &'a [Attachment],
    pub depth_attachment: Option<Attachment>,
    pub stencil_attachment: Option<Attachment>,
}

#[derive(Debug, Clone, Copy)]
pub enum Topology {
    PointList = 0,
    LineList = 1,
    LineStrip = 2,
    TriangleList = 3,
    TriangleStrip = 4,
    TriangleFan = 5,
}

#[derive(Debug, Clone, Copy)]
pub enum Cull {
    CCW,
    CW,
    BOTH,
    NONE,
}

#[derive(Debug, Clone, Copy)]
pub struct RasterDescription<'a> {
    pub topology: Topology,
    pub cull: Cull,
    pub samples: u8,
    pub alpha_to_coverage: bool,
    pub depth_format: Format,
    pub stencil_format: Format,
    pub color_formats: &'a [Format],
    pub blend_state: Option<BlendInfo>,
}

impl Default for RasterDescription<'_> {
    fn default() -> Self {
        Self {
            topology: Topology::TriangleList,
            cull: Cull::CCW,
            samples: 1,
            alpha_to_coverage: false,
            depth_format: Format::Undefined,
            stencil_format: Format::Undefined,
            color_formats: &[],
            blend_state: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum Memory {
    #[default]
    HostVisible,
    DeviceOnly,
    HostCoherent,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum BufferUsage {
    #[default]
    General,
    DescriptorHeap,
}

#[derive(Debug, Clone, Copy)]
pub struct BufferInfo {
    pub memory: Memory,
    pub size: u64,
    pub usage: BufferUsage,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageUsage {
    Sampled,
    Storage,
    Attachment,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageType {
    Type1D = 0,
    Type2D = 1,
    Type3D = 2,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageInfo {
    pub ty: ImageType,
    pub dimensions: UVec3,
    pub mip_count: u32,
    pub layer_count: u32,
    pub sample_count: u32,
    pub format: Format,
    pub usage: ImageUsage,
}
impl Default for ImageInfo {
    fn default() -> Self {
        Self {
            ty: ImageType::Type2D,
            dimensions: [0, 0, 0],
            mip_count: 1,
            layer_count: 1,
            sample_count: 1,
            format: Format::Undefined,
            usage: ImageUsage::Sampled,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Filter {
    Nearest = 0,
    Linear = 1,
}

#[derive(Debug, Clone, Copy)]
pub enum MipmapMode {
    Nearest = 0,
    Linear = 1,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum SamplerAddressMode {
    #[default]
    Repeat = 0,
    MirroredRepeat = 1,
    ClampToEdge = 2,
    ClampToBorder = 3,
    MirrorClampToEdge = 4,
}

#[derive(Clone, Copy)]
pub struct SamplerInfo {
    pub mag_filter: Filter,
    pub min_filter: Filter,
    pub mipmap_mode: MipmapMode,
    pub address_mode: [SamplerAddressMode; 3],
    pub anisotropy: f32,
    pub lod_bias: f32,
    pub lod_range: [f32; 2],
    pub compare_op: Option<CompareOp>,
}
impl Default for SamplerInfo {
    fn default() -> Self {
        Self {
            mag_filter: Filter::Linear,
            min_filter: Filter::Linear,
            mipmap_mode: MipmapMode::Linear,
            address_mode: Default::default(),
            anisotropy: Default::default(),
            lod_bias: Default::default(),
            lod_range: Default::default(),
            compare_op: Default::default(),
        }
    }
}

#[repr(C, align(32))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ImageDescriptor {
    pub inner: [u64; 4],
}
#[repr(C, align(32))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SamplerDescriptor {
    pub inner: [u64; 4],
}

type UVec3 = [u32; 3];

pub enum QueueType {
    Graphics, // Graphics, Compute, and Copy
              // Multiple queues require handling queue ownership, so there is no point
              // until I figure that out
              //Compute,  // Compute and Copy
              //Copy,     // Copy only
}

pub trait DeviceRHI {
    type Pipeline;
    type Semaphore;
    type Queue: QueueRHI;
    type GpuPtr;

    fn create_buffer(&mut self, details: &BufferInfo) -> Self::GpuPtr;
    fn create_image(&mut self, details: &ImageInfo) -> Self::GpuPtr;

    fn buffer_host_addr(&self, ptr: Self::GpuPtr) -> *mut u8;
    fn buffer_device_addr(&self, ptr: Self::GpuPtr) -> u64;

    fn destroy_ptr(&mut self, ptr: Self::GpuPtr);

    fn get_image_descriptor(&self, image: Self::GpuPtr) -> ImageDescriptor;
    fn get_sampler_descriptor(&self, desc: &SamplerInfo) -> SamplerDescriptor;

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

#[derive(Debug, Clone, Copy)]
pub enum Stage {
    None,
    TopOfPipe,
    DrawIndirect,
    VertexShader,
    FragmentShader,
    EarlyFragmentTests,
    LateFragmentTests,
    ColorAttachmentOutput,
    ComputeShader,
    Transfer,
    BottomOfPipe,
    AllGraphics,
    AllCommands,
    Host,
    Copy,
    Resolve,
    Blit,
    Clear,
    TaskShader,
    MeshShader,
}

#[derive(Debug, Clone, Copy)]
pub enum ImageLayout {
    Undefined,
    General,
    Attachment,
    ShaderReadOnlyOptimal,
    TransferSrcOptimal,
    TransferDstOptimal,
    RenderingLocalRead,
    PresentSrc,
}

pub type PushData<'a> = &'a [u8];

#[derive(Clone, Copy, Default)]
pub struct ImageCopyInfo {
    pub src_offset: UVec3,
    pub dst_offset: UVec3,
    pub extent: UVec3,
}

#[derive(Clone, Copy, Default)]
pub struct ImageBlitInfo {
    pub src_offset: UVec3,
    pub dst_offset: UVec3,
    pub src_extent: UVec3,
    pub dst_extent: UVec3,
}

pub trait CommandRHI {
    type GpuPtr;
    type Pipeline;
    type Semaphore;

    fn copy_buffer(&mut self, src_buffer: Self::GpuPtr, dst_buffer: Self::GpuPtr);

    fn copy_to_image(&mut self, src_buffer: Self::GpuPtr, dst_image: Self::GpuPtr);
    fn copy_from_image(&mut self, src_image: Self::GpuPtr, dst_buffer: Self::GpuPtr);
    fn copy_image(
        &mut self,
        src_image: Self::GpuPtr,
        dst_image: Self::GpuPtr,
        info: &ImageCopyInfo,
    );
    fn blit_image(
        &mut self,
        src_image: Self::GpuPtr,
        dst_image: Self::GpuPtr,
        info: &ImageBlitInfo,
    );

    fn bind_descriptor_heap(&mut self, resource_heap: Self::GpuPtr, sampler_heap: Self::GpuPtr);

    fn barrier(&mut self, before: Stage, after: Stage /* something goes here */);
    fn image_barrier(
        &mut self,
        before: Stage,
        after: Stage,
        image: Self::GpuPtr,
        layout: ImageLayout,
    );
    fn signal_after(&mut self, stage: Stage, semaphore: &Self::Semaphore, value: u64);
    fn wait_before(&mut self, stage: Stage, semaphore: &Self::Semaphore, value: u64);

    fn set_pipeline(&mut self, pipeline: &Self::Pipeline);
    fn set_depth_stencil_state(&mut self, state: &DepthStencilInfo);
    fn set_blend_state(&mut self, state: &BlendInfo);

    fn gpu_dispatch(&mut self, data: PushData, dimensions: UVec3);
    fn gpu_dispatch_indirect(&mut self, data: PushData, indirect_buffer: Self::GpuPtr);

    fn begin_rendering(&mut self, desc: &RenderingInfo);
    fn end_rendering(&mut self);

    fn draw(&mut self, data: PushData, indices: u32, instances: u32);
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

pub use instance::*;
pub use window::*;
