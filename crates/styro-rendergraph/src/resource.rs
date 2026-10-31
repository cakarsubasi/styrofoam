use std::marker::PhantomData;

use styro_rhi as rhi;

use crate::GraphResources;

pub struct Persistent<T> {
    _marker: PhantomData<fn() -> T>,
}

impl<T: LogicalResource> Persistent<T> {
    pub fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

pub trait LogicalResource {
    type Descriptor: LogicalResourceDescriptor;

    fn id(&self) -> usize;

    fn from_descriptor(descriptor: <Self as LogicalResource>::Descriptor) -> Self;
}

pub trait LogicalResourceDescriptor {
    type Resource: LogicalResource;

    fn to_resource(
        self,
        device: &mut rhi::Device,
        allocator: &mut GraphResources,
    ) -> Self::Resource;
}

#[repr(transparent)]
#[derive(bytemuck::NoUninit, Clone, Copy)]
pub struct GpuPtr<T: bytemuck::NoUninit> {
    addr: u64,
    _marker: PhantomData<fn() -> T>,
}

#[repr(C)]
#[derive(bytemuck::NoUninit, Clone, Copy)]
pub struct Sampler {
    id: u32,
}

impl Sampler {
    pub(crate) unsafe fn from_idx(id: u32) -> Self {
        Self { id }
    }
}

#[repr(C)]
#[derive(bytemuck::NoUninit, Clone, Copy)]
pub struct CombinedImageSampler {
    image: u32,
    sampler: u32,
}

impl CombinedImageSampler {
    pub fn new(image: ImageHandle, sampler: Sampler) -> Self {
        todo!()
    }
}

pub struct BufferHandle {
    pub(crate) id: usize,
    pub(crate) addr: u64,
}

impl BufferHandle {
    pub fn new(id: usize, addr: u64) -> Self {
        Self { id, addr }
    }

    pub fn ptr<T: bytemuck::NoUninit>(&self) -> GpuPtr<T> {
        GpuPtr {
            addr: self.addr,
            _marker: PhantomData,
        }
    }
}

impl LogicalResource for BufferHandle {
    type Descriptor = BufferDesc;

    fn from_descriptor(descriptor: <Self as LogicalResource>::Descriptor) -> Self {
        todo!()
    }

    fn id(&self) -> usize {
        todo!()
    }
}

pub struct BufferDesc {
    pub size: u32,
    pub location: rhi::Memory,
}

impl BufferDesc {
    pub fn upload(size: usize) -> Self {
        Self {
            size: size as u32,
            location: rhi::Memory::HostVisible,
        }
    }

    pub fn gpu(size: usize) -> Self {
        Self {
            size: size as u32,
            location: rhi::Memory::DeviceLocal,
        }
    }

    pub fn readback(size: usize) -> Self {
        Self {
            size: size as u32,
            location: rhi::Memory::HostCoherent,
        }
    }
}

impl LogicalResourceDescriptor for BufferDesc {
    type Resource = BufferHandle;

    fn to_resource(
        self,
        device: &mut rhi::Device,
        resources: &mut GraphResources,
    ) -> Self::Resource {
        resources.new_buffer(device, &self)
    }
}

pub struct Res<T> {
    pub(crate) id: usize,
    _marker: PhantomData<T>,
}
impl<T> Copy for Res<T> {}
impl<T> Clone for Res<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Res<T> {
    pub(crate) unsafe fn from_idx(id: usize) -> Self {
        Self {
            id,
            _marker: PhantomData,
        }
    }
}

enum PassProvenance {
    Pass(usize),
    External,
}

enum ResourceTy {
    Buffer(usize),
    Image(usize),
}

struct BufferData {
    
}

struct ResMetaData {
    // resource type
    output_pass_id: PassProvenance,
    resource_id: ResourceTy,
    persistent_id: Option<usize>,
}

pub trait ResourceUsage {}

pub(crate) struct UsageInfo {}

pub(crate) fn get_usage(description: impl ResourceUsage) -> UsageInfo {
    todo!()
}

pub enum BufferUsageType {
    ShaderRead,
    ShaderWrite,
    Index,
}

pub struct BufferUsage {
    usage: BufferUsageType,
}

impl BufferUsage {
    pub fn read() -> Self {
        Self {
            usage: BufferUsageType::ShaderRead,
        }
    }

    pub fn write() -> Self {
        Self {
            usage: BufferUsageType::ShaderWrite,
        }
    }

    pub fn index() -> Self {
        Self {
            usage: BufferUsageType::Index,
        }
    }
}

impl ResourceUsage for BufferUsage {}

pub struct ImageHandle {
    pub(crate) id: usize,
}

impl ImageHandle {
    pub(crate) unsafe fn from_idx(id: usize) -> Self {
        Self { id }
    }
}

impl LogicalResource for ImageHandle {
    type Descriptor = ImageDesc;

    fn id(&self) -> usize {
        self.id
    }

    fn from_descriptor(descriptor: <Self as LogicalResource>::Descriptor) -> Self {
        todo!()
    }
}

pub struct ImageDesc {}

impl LogicalResourceDescriptor for ImageDesc {
    type Resource = ImageHandle;

    fn to_resource(
        self,
        device: &mut rhi::Device,
        resources: &mut GraphResources,
    ) -> Self::Resource {
        resources.new_image(device, &self)
    }
}

impl ImageDesc {
    pub fn sampled_2d(height: usize, width: usize, format: rhi::Format) -> Self {
        todo!()
    }
}

pub enum ImageUsageType {
    Transfer,
    ColorAttachmentWrite,
    SampledRead,
}

pub struct ImageUsage {
    usage: ImageUsageType,
}

impl ResourceUsage for ImageUsage {}

impl ImageUsage {
    pub fn transfer() -> Self {
        Self {
            usage: ImageUsageType::Transfer,
        }
    }

    pub fn color_write() -> Self {
        Self {
            usage: ImageUsageType::ColorAttachmentWrite,
        }
    }

    pub fn sampled() -> Self {
        Self {
            usage: ImageUsageType::SampledRead,
        }
    }

    pub fn persist(self, handle: &Persistent<ImageHandle>) -> Self {
        todo!()
    }
}
