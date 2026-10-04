use std::marker::PhantomData;

use styro_rhi as rhi;

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

    fn to_resource(self) -> Self::Resource;
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
    _private: (),
}

impl BufferHandle {
    pub fn ptr<T: bytemuck::NoUninit>(&self) -> GpuPtr<T> {
        todo!()
    }
}

impl LogicalResource for BufferHandle {
    type Descriptor = BufferDesc;

    fn from_descriptor(descriptor: <Self as LogicalResource>::Descriptor) -> Self {
        todo!()
    }

    fn id(&self) -> usize {
        self.id
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

    fn to_resource(self) -> Self::Resource {
        todo!()
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
    _private: (),
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

    fn to_resource(self) -> Self::Resource {
        todo!()
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
