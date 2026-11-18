use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use ash::VkResult;
use ash::vk;
use vk_mem::Alloc;

use super::device::DeviceHandles;

use crate::*;

pub(super) struct GpuResources {
    handle_counter: AtomicU64,
    pub(super) buffers: HashMap<u64, Buffer>,
    pub(super) images: HashMap<u64, Image>,
    device: Arc<DeviceHandles>,
    sampler_cache: RwLock<HashMap<SamplerInfo, vk::Sampler>>,
}

unsafe impl Send for GpuResources {}
unsafe impl Sync for GpuResources {}

impl GpuResources {
    pub fn new(device: Arc<DeviceHandles>) -> VkResult<Self> {
        Ok(Self {
            handle_counter: AtomicU64::new(1),
            device,
            buffers: HashMap::new(),
            images: HashMap::new(),
            sampler_cache: RwLock::new(HashMap::new()),
        })
    }

    /// Take &self and use separate locking just in case. Cache samplers
    /// since they are immutable and can be reused. They should also be relatively cheap to have and we can
    /// drop them all at once. Probably don't care enough about this path to use LRU or pruning strategies.
    pub(super) fn get_or_create_sampler(&self, info: &SamplerInfo) -> vk::Sampler {
        let lock = self.sampler_cache.read().unwrap();

        if let Some(sampler) = lock.get(info) {
            *sampler
        } else {
            drop(lock);
            let mut lock = self.sampler_cache.write().unwrap();

            unsafe {
                let sampler = self
                    .device
                    .inner
                    .create_sampler(&info.to_vk(), None)
                    .unwrap();

                lock.insert(*info, sampler);

                sampler
            }
        }
    }

    pub(super) fn write_sampler_descriptor(
        &self,
        desc: &SamplerInfo,
        addr: &mut SamplerDescriptor,
    ) {
        let device = &self.device;
        let descriptor_heap = &device.descriptor_heap;

        unsafe {
            let descriptor = [vk::HostAddressRangeEXT::default()
                .address(bytemuck::cast_slice_mut(&mut addr.inner))];

            let samplers = [desc.to_vk()];

            descriptor_heap
                .as_ref()
                .expect("Descriptor heap feature is not enabled.")
                .write_sampler_descriptors(&samplers, &descriptor)
                .unwrap();
        }
    }

    pub(super) fn write_image_descriptor(&self, image: GpuImage, addr: &mut ImageDescriptor) {
        let image = self.get_image(image);

        let device = &self.device;
        let descriptor_heap = &device.descriptor_heap;
        let _props = &device.descriptor_heap_props;
        unsafe {
            let resource = [vk::ResourceDescriptorInfoEXT::default()
                .ty(vk::DescriptorType::SAMPLED_IMAGE)
                .data(vk::ResourceDescriptorDataEXT {
                    p_image: &vk::ImageDescriptorInfoEXT {
                        p_view: &vk::ImageViewCreateInfo::default()
                            .view_type(image.desc.ty.into())
                            .format(image.desc.format.into())
                            .image(image.inner)
                            .subresource_range(vk::ImageSubresourceRange {
                                aspect_mask: vk::ImageAspectFlags::COLOR,
                                base_mip_level: 0,
                                level_count: image.desc.mip_count,
                                base_array_layer: 0,
                                layer_count: image.desc.layer_count,
                            }),
                        layout: vk::ImageLayout::GENERAL,
                        ..Default::default()
                    },
                })];
            let descriptor = [vk::HostAddressRangeEXT::default()
                .address(bytemuck::cast_slice_mut(&mut addr.inner))];
            descriptor_heap
                .as_ref()
                .expect("Descriptor heap feature is not enabled.")
                .write_resource_descriptors(&resource, &descriptor)
                .unwrap();
        }
    }

    pub fn ptr_to_buffer(&self, ptr: GpuPtr) -> &Buffer {
        match self.buffers.get(&ptr.handle) {
            Some(buffer) => buffer,
            None => panic!("GpuPtr does not refer to a live buffer"),
        }
    }

    pub fn get_image(&self, image: GpuImage) -> &Image {
        match self.images.get(&image.handle) {
            Some(image) => image,
            None => panic!("GpuImage does not refer to a live image"),
        }
    }

    pub(super) fn next_handle(&self) -> u64 {
        self.handle_counter.fetch_add(1, Ordering::Relaxed)
    }

    pub(super) fn create_buffer(&mut self, desc: &BufferInfo) -> GpuPtr {
        let buffer = Buffer::new(Arc::clone(&self.device), desc).unwrap();

        let handle = self.next_handle();

        let size = buffer.size as u32;
        self.buffers.insert(handle, buffer);

        GpuPtr {
            handle,
            offset: 0,
            size,
        }
    }

    pub(super) fn create_image(&mut self, desc: &ImageInfo) -> GpuImage {
        let image = Image::new(Arc::clone(&self.device), desc);
        self.insert_image(image)
    }

    pub(super) fn insert_image(&mut self, image: Image) -> GpuImage {
        let handle = self.next_handle();
        let desc = image.desc;
        self.images.insert(handle, image);

        GpuImage { handle, desc }
    }

    pub(super) fn free_buffer(&mut self, ptr: GpuPtr) {
        match self.buffers.remove(&ptr.handle) {
            Some(buffer) => buffer.free_resources(&self.device),
            None => panic!("Double free."),
        }
    }

    pub(super) fn free_image(&mut self, image: GpuImage) {
        match self.images.remove(&image.handle) {
            Some(image) if image.is_swapchain_image() => {
                panic!("Swapchain images should not be passed to destroy_image()")
            }
            Some(image) => image.free_resources(&self.device),
            None => panic!("Double free."),
        }
    }

    pub fn insert_swapchain_image(&mut self, image: Image) -> GpuImage {
        debug_assert!(image.is_swapchain_image());
        self.insert_image(image)
    }

    /// Free a swapchain image. Null and already freed handles are ignored.
    pub fn free_swapchain_image(&mut self, image: GpuImage) {
        if let Some(image) = self.images.remove(&image.handle) {
            image.free_resources(&self.device);
        }
    }
}

impl Drop for GpuResources {
    fn drop(&mut self) {
        unsafe {
            self.device.inner.device_wait_idle().unwrap();

            for (_, buffer) in self.buffers.drain() {
                buffer.free_resources(&self.device);
            }
            for (_, image) in self.images.drain() {
                image.free_resources(&self.device);
            }

            for (_, sampler) in self.sampler_cache.write().unwrap().drain() {
                self.device.inner.destroy_sampler(sampler, None);
            }
        }
    }
}

impl BufferUsage {
    fn usage(&self) -> vk::BufferUsageFlags {
        match self {
            BufferUsage::DescriptorHeap => vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT,
            _ => {
                vk::BufferUsageFlags::INDEX_BUFFER
                    | vk::BufferUsageFlags::INDIRECT_BUFFER
                    | vk::BufferUsageFlags::STORAGE_BUFFER
                    | vk::BufferUsageFlags::UNIFORM_BUFFER
            }
        }
    }
}

impl Memory {
    fn vma_options(&self) -> vk_mem::AllocationCreateInfo {
        match self {
            Memory::Upload => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
                ..Default::default()
            },
            Memory::GpuOnly => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::AutoPreferDevice,
                ..Default::default()
            },
            Memory::Cached => vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_RANDOM,
                ..Default::default()
            },
        }
    }
}

pub(super) struct Buffer {
    pub inner: vk::Buffer,
    pub(super) allocation: vk_mem::Allocation,
    size: u64,
    pub _ty: BufferUsage,
    pub(super) mapped_ptr: Option<*mut u8>,
}

impl Buffer {
    pub fn new(device: Arc<DeviceHandles>, desc: &BufferInfo) -> VkResult<Self> {
        unsafe {
            let size = desc.size;
            let buffer_usage = desc.usage;

            let buffer_info = vk::BufferCreateInfo::default()
                // HMMMM
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .size(size)
                .usage(
                    buffer_usage.usage()
                        | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS
                        | vk::BufferUsageFlags::TRANSFER_SRC
                        | vk::BufferUsageFlags::TRANSFER_DST,
                );

            let allocation_info = desc.memory.vma_options();

            let (buffer, mut allocation) = device
                .allocator
                .create_buffer(&buffer_info, &allocation_info)?;

            let mapped_ptr = if let Memory::GpuOnly = desc.memory {
                None
            } else {
                Some(device.allocator.map_memory(&mut allocation).unwrap())
            };

            Ok(Self {
                inner: buffer,
                allocation,
                size: size,
                _ty: buffer_usage,
                mapped_ptr,
            })
        }
    }

    pub fn len(&self) -> u64 {
        self.size
    }

    pub fn free_resources(mut self, device: &DeviceHandles) {
        unsafe {
            if let Some(_) = self.mapped_ptr {
                device.allocator.unmap_memory(&mut self.allocation);
            }

            device
                .allocator
                .destroy_buffer(self.inner, &mut self.allocation);
        }
    }
}

impl ImageInfo {
    fn sample_count(&self) -> vk::SampleCountFlags {
        match self.sample_count {
            1 => vk::SampleCountFlags::TYPE_1,
            2 => vk::SampleCountFlags::TYPE_2,
            4 => vk::SampleCountFlags::TYPE_4,
            8 => vk::SampleCountFlags::TYPE_8,
            x => panic!("Invalid sample count {x}"),
        }
    }
}

impl ImageUsage {
    fn to_vk(&self) -> vk::ImageUsageFlags {
        return vk::ImageUsageFlags::INPUT_ATTACHMENT
            | vk::ImageUsageFlags::COLOR_ATTACHMENT
            | vk::ImageUsageFlags::STORAGE
            | vk::ImageUsageFlags::SAMPLED
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::TRANSFER_DST;
    }
}

pub(super) struct AllocatedImageData {
    pub allocation: vk_mem::Allocation,
}

pub(super) struct SwapchainImageData {
    pub idx: u32,
    pub submit_wait: Cell<vk::Semaphore>,
    pub submit_signal_present_wait: Cell<vk::Semaphore>,
}

pub(super) enum ImageData {
    Allocated(AllocatedImageData),
    Swapchain(SwapchainImageData),
}

pub(super) struct Image {
    pub inner: vk::Image,
    pub view: Option<vk::ImageView>,
    pub desc: ImageInfo,
    pub current_layout: Cell<vk::ImageLayout>,
    pub data: ImageData,
}

impl Image {
    // Create a 2D Image with the given extent
    pub fn new(device: Arc<DeviceHandles>, description: &ImageInfo) -> Self {
        unsafe {
            let layout = vk::ImageLayout::UNDEFINED;
            let image_info = vk::ImageCreateInfo::default()
                .image_type(description.ty.into())
                .format(description.format.into())
                .extent(vk::Extent3D {
                    width: description.dimensions[0],
                    height: description.dimensions[1],
                    depth: description.dimensions[2],
                })
                .mip_levels(description.mip_count)
                .array_layers(description.layer_count)
                .samples(description.sample_count())
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(description.usage.to_vk())
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(layout);

            let allocation_info = vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                ..Default::default()
            };

            let (image, allocation) = device
                .allocator
                .create_image(&image_info, &allocation_info)
                .unwrap();

            // Attachments always need a view. Without descriptor heaps, basically everything needs a view.
            let view = if matches!(description.usage, ImageUsage::Attachment)
                || device.descriptor_heap.is_none()
            {
                Some(
                    device
                        .inner
                        .create_image_view(
                            &vk::ImageViewCreateInfo::default()
                                .view_type(description.ty.into())
                                .format(description.format.into())
                                .image(image)
                                .subresource_range(vk::ImageSubresourceRange {
                                    aspect_mask: vk::ImageAspectFlags::COLOR, // TODO: add multiple attachment types or try to infer the attachment type
                                    base_mip_level: 0,
                                    level_count: description.mip_count,
                                    base_array_layer: 0,
                                    layer_count: description.layer_count,
                                }),
                            None,
                        )
                        .unwrap(),
                )
            } else {
                None
            };

            let data = AllocatedImageData { allocation };

            Self {
                inner: image,
                view,
                desc: description.clone(),
                current_layout: Cell::new(layout),
                data: ImageData::Allocated(data),
            }
        }
    }

    pub fn is_swapchain_image(&self) -> bool {
        matches!(self.data, ImageData::Swapchain(_))
    }

    pub fn free_resources(mut self, device: &DeviceHandles) {
        unsafe {
            if let Some(view) = self.view {
                device.inner.destroy_image_view(view, None);
            }

            if let ImageData::Allocated(data) = &mut self.data {
                device
                    .allocator
                    .destroy_image(self.inner, &mut data.allocation);
            }
        }
    }
}
