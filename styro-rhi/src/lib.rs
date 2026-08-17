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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
///
/// The stencil test compares the fragments in the depth to a reference value provided in
/// `reference` using the operation in `compare_op`. The render pass must have a depth and stencil attachment
/// for this struct to be meaningful, else the stencil test is skipped.
///
/// The stencil test can be dynamically enabled using [CommandRHI::set_depth_stencil_state].
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
    /// Reference value used for the comparison
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
/// The depth test compares the depth value of each fragment in a shader to the value stored in the depth attachment
/// (if there is one), the write is skipped if the fragment fails the depth test. The render pass must have a depth
/// attachment for this struct to be meaningful, else the depth test is skipped.
///
/// The depth and stencil tests can be dynamically enabled using [CommandRHI::set_depth_stencil_state].
#[derive(Debug, Clone, Copy)]
pub struct DepthStencilInfo {
    /// Whether depth testing is performed
    pub depth_read: bool,
    /// Whether passing fragments will write to depth
    pub depth_write: bool,
    /// Compare operation
    pub depth_test: CompareOp,
    /// The constant value added to the depth of each fragment
    pub depth_bias: f32,
    /// The constant value multiplied with the depth of each fragment
    pub depth_bias_slope_factor: f32,
    /// The maximum or minimum depth bias of a fragment
    pub depth_bias_clamp: f32,
    /// Stencil operation state for front facing polygons
    pub stencil_front: StencilInfo,
    /// Stencil operation state for back facing polygons
    pub stencil_back: StencilInfo,
}
impl Default for DepthStencilInfo {
    fn default() -> Self {
        Self {
            depth_read: true,
            depth_write: true,
            depth_test: CompareOp::Less,
            depth_bias: 0.0,
            depth_bias_slope_factor: 0.0,
            depth_bias_clamp: 0.0,
            stencil_front: Default::default(),
            stencil_back: Default::default(),
        }
    }
}

/// Framebuffer blending operation
///
/// The blend state can be dynamically set using [CommandRHI::set_blend_state]. Alternatively, it can be
/// baked into a [Pipeline] object with [DeviceRHI::create_graphics_pipeline].
#[derive(Debug, Clone, Copy)]
pub enum BlendOp {
    /// The source and destination fragments are multiplied with their color factor and added.
    ///
    /// `src.rgb * src_color_factor + dst.rgb * dst_color_factor` `src.a * src_alpha_factor + dst.a * dst_alpha_factor`
    Add = 0,
    /// The source and destination fragments are multiplied with their color factor. The destination value is
    /// subtracted from the source value.
    ///
    /// `src.rgb * src_color_factor - dst.rgb * dst_color_factor` `src.a * src_alpha_factor - dst.a * dst_alpha_factor`
    Subtract = 1,
    /// The source and destination fragments are multiplied with their color factor. The source value is subtracted
    /// from the destination value.
    ///
    /// `dst.rgb * dst_color_factor - src.rgb * src_color_factor` `dst.a * dst_alpha_factor - src.a * src_alpha_factor`
    ReverseSubtract = 2,
    /// The minimum of each channel between the source and destination fragments are chosen. Color factors are ignored.
    ///
    /// `min(src.r, dst.r), min(src.g, dst.g), min(src.b, dst.b)` `min(src.a, dst.a)`
    Min = 3,
    /// The maximum of each channel between the source and destination fragments are chosen. Color factors are ignored.
    ///
    /// `max(src.r, dst.r), max(src.g, dst.g), max(src.b, dst.b)` `max(src.a, dst.a)`
    Max = 4,
}

/// Blend factors
///
/// Blend factors are multiplied with the attachment and the fragments before being stored. See [BlendInfo].
///
/// The blend state can be dynamically set using [CommandRHI::set_blend_state]. Alternatively, it can be
/// baked into a [Pipeline] object with [DeviceRHI::create_graphics_pipeline].
#[derive(Debug, Clone, Copy)]
pub enum BlendFactor {
    /// `(0, 0, 0)` `0`
    Zero = 0,
    /// `(1, 1, 1)` `1`
    One = 1,
    /// `src0.rgb` `src0.a`
    SrcColor = 2,
    /// `1 - src0.rgb` `1 - src0.a`
    OneMinusSrcColor = 3,
    /// `dst0.rgb` `dst0.a`
    DstColor = 4,
    /// `1 - dst0.rgb` `1 - dst0.a`
    OneMinusDstColor = 5,
    /// `src0.aaa` `src0.a`
    SrcAlpha = 6,
    /// `1 - src0.aaa` `1 - src0.a`
    OneMinusSrcAlpha = 7,
    /// `dst0.aaa` `dst0.a`
    DstAlpha = 8,
    /// `1 . dst0.aaa` `dst0.a`
    OneMinusDstAlpha = 9,
    // TODO: Need extra API entry for the constant ones. The dual-source blending ones might be a bit niche for now
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

/// Blend state
///
/// After a fragment shader, blending can be performed to determine what will be written to the color
/// attachments.
///
/// The blend state can be dynamically set using [CommandRHI::set_blend_state]. Alternatively, it can be
/// baked into a [Pipeline] object with [DeviceRHI::create_graphics_pipeline].
#[derive(Debug, Clone, Copy)]
pub struct BlendInfo {
    pub color_op: BlendOp,
    pub src_color_factor: BlendFactor,
    pub dst_color_factor: BlendFactor,
    pub alpha_op: BlendOp,
    pub src_alpha_factor: BlendFactor,
    pub dst_alpha_factor: BlendFactor,
}

/// Attachment load operation
///
/// Determines what happens to the contents of an attachment in the beginning of a subpass. See [RenderingInfo].
#[derive(Debug, Clone, Copy)]
pub enum LoadOp {
    /// Load the previous contents of the image
    Load = 0,
    /// Set the contents of the image to a uniform value
    Clear = 1,
    /// The previous contents of the image are unspecified
    ///
    /// On mobile GPUs with tile based deferred rendering, this can serve as a hint to prevent the GPU
    /// from unnecessarily clearing tile memory saving bandwidth in the process.
    DontCare = 2,
}

/// Attachment store operation
///
/// Determines what happens to the contents of an attachment at the end of a subpass. See [Attachment] and [RenderingInfo].
#[derive(Debug, Clone, Copy)]
pub enum StoreOp {
    /// Write the result of the render pass into the image
    Store = 0,
    /// The contents of the image are unspecified
    ///
    /// On mobile GPUs with tile based deferred rendering, this can serve as a hint to prevent the GPU
    /// from unnecessarily flushing tile memory saving bandwidth in the process.
    DontCare = 1,
}

/// Attachment clear value
///
/// See [Attachment] and [RenderingInfo].
#[derive(Debug, Clone, Copy)]
pub enum Clear {
    /// Specifies clear value for a color attachment
    Color(f32, f32, f32, f32),
    /// Specifies clear value for the depth-stencil attachment
    DepthStencil(f32, u32),
}

/// Attachment information
///
/// An output attachment is an image that is written to in the fragment stage of a graphics pipeline. An input attachment
/// is an image that may be loaded at the beginning of the fragment stage that is written to as an output attachment in
/// an earlier subpass during rendering.
///
/// # Valid usage
/// - `image` must refer to an image or a swapchain image resource.
/// - `image` must be a valid handle for the duration of the render pass which uses this attachment.
/// - If `load_op` is [LoadOp::DontCare], the contents of the image should not be accessed during
/// the render pass.
/// - If the attachment is a color attachment and `load_op` is [LoadOp::Load], then `clear_value` must be
/// [Clear::Color]. If the attachment is a depth or stencil attachment and `load_op` is [LoadOp::Load], then
/// `clear_value` must be [Clear::DepthStencil].
#[derive(Debug, Clone, Copy)]
pub struct Attachment {
    /// Attachment image resource
    pub image: GpuPtr,
    /// Attachment load operation
    pub load_op: LoadOp,
    /// Attachment store operation
    pub store_op: StoreOp,
    /// Attachment clear value
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

///
#[derive(Debug, Default, Clone, Copy)]
pub struct RenderingInfo<'a> {
    /// Color output attachments
    pub color_attachments: &'a [Attachment],
    /// The depth attachment for the depth test
    pub depth_attachment: Option<Attachment>,
    /// The stencil attachment for the stencil test
    pub stencil_attachment: Option<Attachment>,
}

/// Primitive topology
#[derive(Debug, Clone, Copy)]
pub enum Topology {
    /// Specifies separate points
    PointList = 0,
    /// Each pair of vertices describe a line component
    LineList = 1,
    /// Successive vertices describe a continuous line
    LineStrip = 2,
    /// Each set of three vertices describe a triangle
    TriangleList = 3,
    /// Each vertex and the following two vertices describe a triangle
    TriangleStrip = 4,
    /// Triangles are formed around a single common vertex
    TriangleFan = 5,
}

/// Front face and triangle cull mode
#[derive(Debug, Clone, Copy)]
pub enum Cull {
    /// Triangles have a counter-clockwise order, cull the back face
    CCW,
    /// Triangles have a clockwise order, cull the back face
    CW,
    /// Cull both faces
    BOTH,
    /// Cull neither face
    NONE,
}

/// Creation info for a raster based graphics pipeline
#[derive(Debug, Clone, Copy)]
pub struct RasterDescription<'a> {
    /// Primitive topology used by the pipeline
    pub topology: Topology,
    /// Cull mode used by the pipeline
    pub cull: Cull,
    /// Resolve samples used by the pipeline
    pub samples: u8,
    /// Whether alpha to coverage is enabled
    pub alpha_to_coverage: bool,
    /// Formats for the color output attachments used by the pipeline
    pub color_formats: &'a [Format],
    /// Format for the depth attachment used by the pipeline
    ///
    /// If there is no depth attachment, [Format::Undefined] should be used
    pub depth_format: Format,
    /// Format for the stencil attachment used by the pipeline
    ///
    /// If there is no stencil attachment, [Format::Undefined] should be used
    pub stencil_format: Format,
    /// Optional embedded blend state
    ///
    /// If `blend_state` is `None`, blend state can be dynamically set using [CommandRHI::set_blend_state]
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

/// Buffer memory location
///
/// See [DeviceRHI::create_buffer]
#[derive(Debug, Clone, Copy, Default)]
pub enum Memory {
    /// Buffer memory will be writable from the host
    ///
    /// This is the suggested memory type for most buffer allocations. It allows fast sequential writes from the host and fast
    /// reads from the device.
    ///
    /// With this memory type, you should be careful not to perform reads or access memory in a non-sequential manner. As
    /// the memory is typically not host-backed,
    ///
    /// On Vulkan, corresponds to `VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT`
    #[default]
    HostVisible,
    /// Buffer memory will be optimal for the device
    ///
    /// This is the suggested memory type for images that are not used for readback and buffers that will only be accessed
    /// by a device. Buffers created with this memory type may not be passed to [DeviceRHI::buffer_host_addr].
    ///
    /// On Vulkan, corresponds to `VK_MEMORY_PROPERTY_DEVICE_LOCAL_BIT`
    DeviceLocal,
    /// Buffer memory will be readable from the host
    ///
    /// This is the suggested memory type for host readback.
    ///
    /// On Vulkan, corresponds to `VK_MEMORY_PROPERTY_HOST_VISIBLE_BIT | VK_MEMORY_PROPERTY_HOST_COHERENT_BIT |
    /// VK_MEMORY_PROPERTY_HOST_CACHED_BIT`
    HostCoherent,
}

/// Buffer memory usage
///
/// See [DeviceRHI::create_buffer]
#[derive(Debug, Clone, Copy, Default)]
pub enum BufferUsage {
    /// Buffer memory is suitable for most usage
    ///
    /// These usages include:
    /// - Uniform buffers
    /// - Storage buffers
    /// - Staging buffers for upload
    /// - Readback buffers
    /// - Index buffers
    /// - Acceleration structures
    ///
    /// Among other things.
    #[default]
    General,
    /// Buffer memory is suitable for resource and sampler heaps
    ///
    /// Since GPUs typically require specialized methods of loading descriptors, this usage must be specified
    /// for any buffer that is passed to [CommandRHI::bind_descriptor_heap].
    DescriptorHeap,
}

/// Buffer creation info
///
/// Used with [DeviceRHI::create_buffer].
#[derive(Debug, Clone, Copy)]
pub struct BufferInfo {
    /// Type of memory requested
    pub memory: Memory,
    /// Requested memory size
    pub size: u64,
    /// Memory usage
    pub usage: BufferUsage,
}

/// Image usage
///
/// See [DeviceRHI::create_image]
#[derive(Debug, Clone, Copy)]
pub enum ImageUsage {
    /// Image is suitable for sampled reads in a render pass
    Sampled,
    /// Image is suitable for writing as a storage image within a render pass
    Storage,
    /// Image is suitable as a rendering attachment
    Attachment,
}

/// Image type
///
/// See [DeviceRHI::create_image]
#[derive(Debug, Clone, Copy)]
pub enum ImageType {
    /// Image is 1-dimensional
    Type1D = 0,
    /// Image is 2-dimensional
    Type2D = 1,
    /// Image is 3-dimensional
    Type3D = 2,
}

/// Image creation info
///
/// See [DeviceRHI::create_image]
#[derive(Debug, Clone, Copy)]
pub struct ImageInfo {
    /// Dimensionality of the image
    pub ty: ImageType,
    /// Dimensions of the image
    ///
    /// For 2D images, `dimensions[2]` is ignored and should be set to 1.
    ///
    /// For 1D images, `dimensions[1]` and `dimensions[2]` are ignored and should be set to 1.
    pub dimensions: UVec3,
    /// Number of mipmap levels the image has
    pub mip_count: u32,
    /// Number of layers the image has
    pub layer_count: u32,
    /// Number of samples the image has when used as a resolve attachment
    pub sample_count: u32,
    /// Image format
    pub format: Format,
    /// Image usage
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

/// Texture filtering type for an immutable sampler
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum Filter {
    Nearest = 0,
    Linear = 1,
}

/// Mipmap filtering type for an immutable sampler
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum MipmapMode {
    Nearest = 0,
    Linear = 1,
}

/// Addressing mode for an immutable sampler
#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub enum SamplerAddressMode {
    #[default]
    Repeat = 0,
    MirroredRepeat = 1,
    ClampToEdge = 2,
    ClampToBorder = 3,
    MirrorClampToEdge = 4,
}

/// Immutable sampler creation info
#[derive(Clone, Copy, PartialEq)]
pub struct SamplerInfo {
    /// Magnification filter for lookups
    pub mag_filter: Filter,
    /// Minification filter for lookups
    pub min_filter: Filter,
    /// Mipmap filter for lookups
    pub mipmap_mode: MipmapMode,
    /// Addressing mode for out of bounds lookups
    ///
    /// Each value determines the behavior for one dimension.
    pub address_mode: [SamplerAddressMode; 3],
    /// Sampler anisotropy
    ///
    /// If `anisotropy > 1`, sampler anisotropy is enabled.
    pub anisotropy: f32,
    /// Mipmap LOD bias
    pub lod_bias: f32,
    /// Mipmap LOD range specified as `[min, max]`
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

/// Implementation defined data for an image descriptor
///
/// See [DeviceRHI::get_image_descriptor]
#[repr(C, align(32))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ImageDescriptor {
    pub inner: [u64; 4],
}

/// Implementation defined data for an immutable sampler descriptor
///
/// See [DeviceRHI::get_sampler_descriptor]
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
        task_ir: Option<&ShaderIR>,
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
    fn push_descriptors(&mut self, images: &[GpuPtr], samplers: &[SamplerInfo], offset: u32);

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
