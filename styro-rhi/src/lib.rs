/// format is so bloated, it gets its own module
mod format;
/// Querying physical devices and device creation
pub mod instance;
/// The Vulkan backend
mod vulkan;
/// Window system integration
pub mod window;

pub use vulkan::command::{CommandBuffer, Pipeline};
pub use vulkan::device::{Device, GpuPtr, Queue, ShaderIR};
pub use vulkan::semaphore::Semaphore;
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
    /// Nearest filtering
    ///
    /// A single value is chosen from the nearest texel
    Nearest = 0,
    /// Linear filtering
    ///
    /// Linear interpolate 8 (3D), 4 (2D), or 2 (1D) texel values
    Linear = 1,
}

/// Mipmap filtering type for an immutable sampler
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum MipmapMode {
    /// Nearest filtering
    ///
    /// A single value is chosen from the nearest texel
    Nearest = 0,
    /// Linear filtering
    ///
    /// Linear interpolate 8 (3D), 4 (2D), or 2 (1D) texel values
    Linear = 1,
}

/// Addressing mode for an immutable sampler
///
/// Determines what wrapping operations will be used when sampling outside the image bounds
#[derive(Debug, Clone, Copy, Default, Hash, PartialEq, Eq)]
pub enum SamplerAddressMode {
    /// Image will repeat
    ///
    /// `i = i mod size`
    #[default]
    Repeat = 0,
    /// Image will mirror and repeat
    ///
    /// `i = (size - 1) - mirror((i mod (2 * size)) - size)` where `mirror(n) = if n ≥ 0 then n else - (1 + n)`
    MirroredRepeat = 1,
    /// Image will clamp to the edge value
    ///
    /// `i = clamp(i, 0, size - 1)`
    ClampToEdge = 2,
    /// Image will clamp to the border value
    ///
    /// `i = clamp(i, -1, size)`
    ClampToBorder = 3,
    /// Image will mirror clamp to the edge
    ///
    /// `i = clamp(mirror(i), 0, size - 1)`
    MirrorClampToEdge = 4,
}

/// Immutable sampler creation info
///
/// There is no direct access to sampler objects, instead they are created alongside descriptors. When using
/// descriptor heaps, an immutable sampler descriptor can be created with [DeviceRHI::get_sampler_descriptor].
/// This can then be copied to a buffer created using [BufferUsage::DescriptorHeap] and then accessed with
/// an index from within a shader.
///
/// ```slang
/// let texture = Sampler2D.Handle(uint2(texture_id, sampler_id));
/// let color = texture.Sample(texture_uv);
/// ```
///
/// # Valid usage
/// - `anisotropy` should be between 0 and 16
/// - `lod_bias` should be less than 15
/// - `lod_range[0]` should be less than or equal to `lod_range[1]`
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
/// See [DeviceRHI::get_image_descriptor].
#[repr(C, align(32))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ImageDescriptor {
    /// Raw data for an image descriptor
    ///
    /// You probably should not touch this, but it is valid to copy out and copy back in.
    pub inner: [u64; 4],
}

/// Implementation defined data for an immutable sampler descriptor
///
/// See [DeviceRHI::get_sampler_descriptor].
#[repr(C, align(32))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SamplerDescriptor {
    /// Raw data for an immutable sampler descriptor
    ///
    /// You probably should not touch this, but it is valid to copy out and copy back in.
    pub inner: [u64; 4],
}

type UVec3 = [u32; 3];

/// Queue type
///
/// TODO: Multiple queue families are not unreasonable to support, but I need to conditionally make
/// everything `VK_SHARING_MODE_CONCURRENT` and slightly change device creation so you can query for multiple queues
/// and things are only made `VK_SHARING_MODE_CONCURRENT` when this is needed. I will almost certainly never expose
/// queue ownership transfers.
pub enum QueueType {
    Graphics, // Graphics, Compute, and Copy
              // Multiple queues require handling queue ownership, so there is no point
              // until I figure that out
              //Compute,  // Compute and Copy
              //Copy,     // Copy only
}

/// Device level functionality
///
pub trait DeviceRHI {
    /// Pipeline state object
    ///
    /// See [DeviceRHI::create_graphics_pipeline], [DeviceRHI::create_compute_pipeline], [DeviceRHI::create_meshlet_pipeline],
    /// and [crate::Pipeline].
    type Pipeline;

    /// Semaphore for device-host and device-device synchronization across multiple submissions
    ///
    /// See [DeviceRHI::create_semaphore] and [crate::Semaphore].
    type Semaphore;

    /// A submission queue
    ///
    /// See [DeviceRHI::create_queue] and [crate::Queue].
    type Queue: QueueRHI;

    /// A GPU resource handle
    ///
    /// See [crate::GpuPtr].
    type GpuPtr;

    /// Create a buffer
    ///
    /// Creates a buffer with the correct alignment and returns a handle. The handle can be freed with a
    /// call to [DeviceRHI::destroy_ptr]. For buffers visible from the host ([Memory::HostVisible] or
    /// [Memory::HostCoherent]), a host pointer can be acquired using [DeviceRHI::buffer_host_addr]. The device
    /// address can be acquired using [DeviceRHI::buffer_device_addr] which can be passed to the device with
    /// push data (see draw and dispatch commands in [CommandRHI]) or by writing the address to another buffer.
    ///
    /// All created buffers are owned by the device and are destroyed when the device is dropped. However, you should
    /// still manually free buffers that you no longer need.
    ///
    /// # Valid usage
    /// - `details.size` must be greater than 0
    /// - `details.size` must not be greater than the amount of available memory (maybe I should make this function fallible)
    fn create_buffer(&mut self, details: &BufferInfo) -> Self::GpuPtr;

    /// Create an image
    ///
    /// Creates an image with the specified details of sufficient size and returns a handle. The handle can be
    /// freed with a call to [DeviceRHI::destroy_ptr]. When created, an image is in an undefined layout and can be
    /// transitioned to a different layout by calling [CommandRHI::image_barrier].
    ///
    /// Unlike buffers, images cannot be accessed directly and can only be
    /// accessed through a descriptor. A descriptor can be acquired by passing an image that is not in the undefined
    /// layout to [DeviceRHI::get_image_descriptor]. The acquired descriptor data can be written to a buffer created
    /// with [BufferUsage::DescriptorHeap], see [CommandRHI::bind_descriptor_heap] for more details. Once written,
    /// the image can be accessed within a shader by creating an opaque handle using its index within the descriptor heap.
    ///
    /// ```slang
    /// let texture = Sampler2D.Handle(uint2(texture_id, sampler_id));
    /// let color = texture.Sample(texture_uv);
    /// ```
    ///
    /// All images created are owned by the device and are destroyed when the device is dropped. However, you should still
    /// manually free images that you no longer need.
    ///
    /// # Valid usage
    /// - TODO
    ///
    /// # Notes
    /// It is probably a design mistake to have
    ///
    fn create_image(&mut self, details: &ImageInfo) -> Self::GpuPtr;

    /// Acquire a host mapped pointer to buffer
    ///
    /// The pointer can be used to copy data from the host for device use or to read data back from the device.
    ///
    /// # Valid usage
    /// - `ptr` must be a buffer
    /// - `ptr.size` and `ptr.offset` must be in bounds for the buffer
    /// - `ptr` must not be created with `Memory::DeviceLocal`
    /// - `ptr` must not be the null pointer
    ///
    /// # Safety
    /// This function is safe, however the following considerations need to be taken when using the host pointer:
    /// - Accesses must be within `[ptr..ptr.byte_add(size)]`
    /// - Pointer must not be accessed after [DeviceRHI::destroy_ptr] is called on the original allocation.
    /// - If memory type is `Memory::HostVisible`, the buffer may not have host memory backing and any read, even
    /// implicit, can have unexpected results. Only doing strict sequential writes is sufficient for correctness.
    /// - Accesses to the pointer must not race. A data race is defined as two simultaneous unsynchronized accesses
    /// where at least one is a write. Using a [Semaphore] where only one party has access to the memory region for
    /// a given value of the semaphore is sufficient. As is spatial partitioning.
    /// - Having a live [reference] is considered an implicit read.
    fn buffer_host_addr(&self, ptr: Self::GpuPtr) -> *mut u8;

    /// Acquire the device address of a buffer
    ///
    /// The pointer can be passed to the device using push data or by being written into another buffer. It can
    /// then be dereferenced from a shader to access the contained data.
    ///
    /// # Valid usage
    /// - `ptr` must be a buffer
    /// - `ptr.size` and `ptr.offset` must be in bounds for the buffer
    /// - `ptr` must not be the null pointer
    ///
    /// # Safety
    /// This function is safe, however the following considerations need to be taken when using the device pointer:
    /// - Accesses must be within `[*ptr..*(ptr+size)]`.
    /// - Pointer must not be accessed after [DeviceRHI::destroy_ptr] is called on the original allocation.
    /// - Accesses to the pointer must not race. A data race is defined as two simultaneous unsynchronized accesses
    /// where at least one is a write. Using a [Semaphore] where only one party has access to the memory region for
    /// a given value of the semaphore is sufficient. As is spatial partitioning.
    /// - Having a live [reference] is considered an implicit read.
    fn buffer_device_addr(&self, ptr: Self::GpuPtr) -> u64;

    /// Deallocate a buffer or an image
    ///
    /// # Valid usage
    /// - `ptr` must have `offset` 0 and the same `size` that it had when created using [DeviceRHI::create_buffer]
    /// or [DeviceRHI::create_image]
    /// - `ptr` must be live
    /// - `ptr` must not be in use
    ///
    /// # Safety
    /// - `ptr` must not be in use when this function is called. Using a timeline semaphore to wait on all
    /// computations on the memory to complete is sufficient to ensure this.
    unsafe fn destroy_ptr(&mut self, ptr: Self::GpuPtr);

    /// Get bytes for an image descriptor
    ///
    /// # Valid usage
    /// - `image` must be an image
    /// - `image` must be live
    /// - `image` must not be in the undefined layout
    fn get_image_descriptor(&self, image: Self::GpuPtr) -> ImageDescriptor;

    /// Get bytes for an immutable sampler descriptor
    ///
    /// # Valid usage
    /// -`desc` must be a valid [SamplerInfo] structure
    fn get_sampler_descriptor(&self, desc: &SamplerInfo) -> SamplerDescriptor;

    /// Acquire a submission queue and allocate command buffers
    ///
    /// A queue is where
    ///
    /// Multiple queues are currently unsupported.
    ///
    /// # Valid usage
    /// - `command_pools` must be greater than 0.
    /// - `command_buffers_per_pool` must be greater than 0.
    fn create_queue(
        &mut self,
        ty: QueueType,
        command_pools: u32,
        command_buffers_per_pool: u32,
    ) -> Self::Queue;

    /// Create a timeline semaphore
    ///
    /// Timeline semaphores can be used for coarse synchronization between different submissions either to the
    /// same queue or to different queues as well as synchronizing with the host using [DeviceRHI::wait_semaphores].
    /// When created, a semaphore will have `initial_value` and then can be signalled within a command buffer using
    /// [CommandRHI::signal_after] and waited from a command buffer within a different submission using
    /// [CommandRHI::wait_before]. The order of calls within a command buffer and the particular command buffer of the
    /// submission are arbitrary.
    ///
    /// Note that semaphores cannot be used for fine grained synchronization within a single command buffer, for that
    /// purpose, use [CommandRHI::barrier] and [CommandRHI::image_barrier]. They also cannot be used to synchronize
    /// between multiple command buffers in a single submission.
    ///
    /// See [crate::Semaphore].
    fn create_semaphore(&mut self, initial_value: u64) -> Self::Semaphore;

    /// Wait using semaphores on the host
    ///
    /// This function will block on the host until each timeline semaphore has a value equal or greater to the
    /// respective value in the `values` array.
    ///
    /// # Valid usage
    /// - `semaphores` and `values` must have the same length
    fn wait_semaphores(&self, semaphores: &[Self::Semaphore], values: &[u64]);

    /// Create a compute pipeline
    ///
    /// # Safety
    /// - Parsing user input is inherently unsafe. `compute_ir.bytes` must be valid SPIR-V.
    unsafe fn create_compute_pipeline(&mut self, compute_ir: &ShaderIR) -> Self::Pipeline;

    /// Create a graphics pipeline
    ///
    /// # Safety
    /// - Parsing user input is inherently unsafe. `vertex_ir.bytes`, and `fragment_ir.bytes` must be valid SPIR-V.
    unsafe fn create_graphics_pipeline(
        &mut self,
        vertex_ir: &ShaderIR,
        fragment_ir: &ShaderIR,
        description: &RasterDescription,
    ) -> Self::Pipeline;

    /// Create a meshlet pipeline
    ///
    /// # Safety
    /// - Parsing user input is inherently unsafe. `task_ir.bytes`, `meshlet_ir.bytes`, and `fragment_ir.bytes` must be
    /// valid SPIR-V.
    unsafe fn create_meshlet_pipeline(
        &mut self,
        task_ir: Option<&ShaderIR>,
        meshlet_ir: &ShaderIR,
        fragment_ir: &ShaderIR,
        description: &RasterDescription,
    ) -> Self::Pipeline;
}

/// Queue level functionality
///
/// Queues have their lifetimes associated with a device meaning that they are created with the device and are only
/// destroyed with a device, however they do own their command buffers.
///
/// # Example
/// ```
/// let mut command_buffers = vec![];
/// for i in 0..8 {
///     command_buffers.push(queue.begin_recording(command_pool as u32));
/// }
/// thread::scope(|s| {
///     for command_buffer in &mut command_buffers {
///         s.spawn(|| {
///             command_buffer.set_pipeline(&pipeline);
///             command_buffer.gpu_dispatch(&[], [64, 1, 1]);
///         });
///     }
/// });
/// queue.submit(&command_buffers).unwrap();
/// ```
///
/// See [DeviceRHI::create_queue]
pub trait QueueRHI {
    type CommandBuffer: CommandRHI;

    /// Start recording a command buffer
    ///
    /// # Valid usage
    /// - `command_pool` must be less than the `command_pools` argument passed to [DeviceRHI::create_queue]
    /// - This function must not be called more times
    fn begin_recording(&mut self, command_pool: u32) -> Self::CommandBuffer;

    /// Submit a sequence of command buffers to this queue
    ///
    /// After calling this function, the command pool associated with the given command buffers will be reset
    /// with the next call to [QueueRHI::begin_recording].
    ///
    /// # Valid usage
    /// - All command buffers must originate from this queue
    // TODO: it is likely fairly trivial to enforce this with lifetime arguments
    /// - All command buffers must originate from the same command pool
    /// - After submit is called, all command buffers from the same command pool acquired from begin_recording
    /// after the last submit are invalidated (including ones that are not submitted) and no further functions
    /// except [drop] should be called on them.
    ///
    /// # Safety
    /// - The submitted commands must not touch host memory in a way that would violate Rust's safety invariants.
    /// (I am not confident this can happen without additional unsafe code, but the basic gist is, be careful with
    /// buffer addresses you share between device and host.)
    /// - Any access(es) to resources during recording must be synchronized between the host and device. Under most
    /// cases this just means allocating a semaphore per command buffer. Making one call to [CommandRHI::signal_after]
    /// and then waiting on semaphores using [DeviceRHI::wait_semaphores].
    ///
    // TODO: Since we do not have fine grained synchronization with semaphores, it might be more sensible to move signals
    // and waits to submit like in Vulkan. Providing a simple flexible API that can do it all is likely difficult with
    // * `VkEvent`. I basically need to have a thread safe map of `VkEvent`s each matching a number passed to
    // [DeviceRHI::wait_before] and [DeviceRHI::signal_after]. `VkEvent` objects can be reused after successfully being
    // signalled but the value tracking is nontrivial and there is still the potential concern of too many `VkEvent`s being
    // created. I might visit this problem way later when I have a better general grasp into this API.
    ///
    /// # Errors
    /// Device loss may occur due to invalid command buffer usage
    unsafe fn submit(&mut self, command_buffers: &[Self::CommandBuffer]) -> Result<(), Error>;
}

/// Pipeline stage flags
///
/// Used for synchronization. See [CommandRHI::wait_before] and [CommandRHI::signal_after] for synchronization between
/// queue submissions and with the host. See [CommandRHI::barrier] and [CommandRHI::image_barrier] for synchronization
/// within a command buffer.
#[derive(Debug, Clone, Copy)]
pub enum Stage {
    /// No stage
    None,
    /// "Stage" before all pipeline stages
    ///
    /// If set to before, equivalent to [Stage::None]. If set to after, equivalent to [Stage::AllCommands].
    TopOfPipe,
    /// Stage where indirect draw and indirect dispatch commands are issued
    ///
    /// This happens before [Stage::ComputeShader], [Stage::VertexShader], [Stage::TaskShader], and
    /// [Stage::MeshShader] depending on the pipeline.
    DrawIndirect,
    /// Vertex shader stage
    ///
    /// Happens before [Stage::FragmentShader]
    VertexShader,
    /// Fragment or pixel shader stage
    ///
    /// Happens after [Stage::EarlyFragmentTests] and [Stage::VertexShader]. Happens before [Stage::LateFragmentTests]
    FragmentShader,
    /// The early fragment tests stage
    ///
    /// This stage includes depth and stencil tests before fragment shading and depth/stencil load operations.
    ///
    /// Happens before [Stage::FragmentShader].
    EarlyFragmentTests,
    /// The late fragment tests stage
    ///
    /// This stage includes depth and stencil tests after fragment shading and depth/stencil store operations.
    ///
    /// Happens after [Stage::FragmentShader] and before [Stage::ColorAttachmentOutput].
    LateFragmentTests,
    /// Final color output stage
    ///
    /// This stage includes load and store operations for color attachments, blending, and multisample resolve operations.
    ///
    /// Happens after [Stage::LateFragmentTests].
    ColorAttachmentOutput,
    /// Compute shader stage
    ComputeShader,
    /// Transfer operations stage
    ///
    /// This stage includes [Stage::Copy], [Stage::Resolve], [Stage::Blit] and [Stage::Clear].
    Transfer,
    /// "Stage" after all pipeline stages
    ///
    /// If set to before, equivalent to [Stage::AllCommands], if set to after, equivalent to [Stage::None].
    BottomOfPipe,
    /// All graphics commands
    ///
    /// [Stage::ColorAttachmentOutput] is the "last" graphics command in a pipeline.
    AllGraphics,
    /// All commands that the queue supports
    AllCommands,
    /// Host reads and writes of device memory
    ///
    /// Not a pipeline stage, but indicates to the device of potential hazards with host memory accesses.
    Host,
    /// Copy commands
    Copy,
    /// Multisample resolve commands
    // TODO: currently no API to do multisample resolve directly
    Resolve,
    /// Blit commands
    Blit,
    /// Clear commands
    // TODO: currently, there is no clear command API at all, and I am not sure if we should have it
    Clear,
    /// Task shader stage
    ///
    /// If it exists, happens before [Stage::MeshShader] and is unsequenced relative to other graphics pipeline
    /// stages.
    TaskShader,
    /// Mesh shader stage
    ///
    /// Happens before [Stage::FragmentShader] and is unsequenced relative to other graphics pipeline stages.
    MeshShader,
}

/// Layout of an image and its resources
///
///
#[derive(Debug, Clone, Copy)]
pub enum ImageLayout {
    /// The undefined image layout
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

    /// Push descriptor updates into the command buffer
    ///
    ///
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
