use std::slice;
use std::sync::Arc;

use ash::VkResult;
use ash::vk;
use vk_mem::Alloc;

use crate::renderer::vulkan::debug::DebugName as _;
use crate::renderer::vulkan::device::ExtDescriptorHeap;

use super::*;

pub struct Buffer {
    pub(super) device: Arc<Device>,
    pub(super) inner: vk::Buffer,
    allocation: vk_mem::Allocation,
    size: u64,
}

impl Buffer {
    pub fn new(device: Arc<Device>, size: u64, usage: vk::BufferUsageFlags) -> VkResult<Self> {
        println!("Creating buffer");
        unsafe {
            let buffer_info = vk::BufferCreateInfo::default()
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .size(size)
                .usage(usage);

            let allocation_info = vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
                ..Default::default()
            };
            let (buffer, allocation) = device
                .allocator
                .create_buffer(&buffer_info, &allocation_info)?;

            Ok(Self {
                device,
                inner: buffer,
                allocation,
                size: size,
            })
        }
    }

    pub fn copy_to_buffer(&self, data: &[u8], dst_offset: u64) {
        if data
            .len()
            .checked_add(dst_offset as usize)
            .expect("Buffer offset overflow")
            > self.len() as usize
        {
            panic!("")
        }
        unsafe {
            // This is safe with &self because VMA uses an internal mutex
            self.device
                .allocator
                .copy_memory_to_allocation(&self.allocation, data, dst_offset)
                .unwrap();
        }
    }

    fn with_mapping(&mut self, f: impl FnOnce(&mut [u8])) {
        // Safety: &mut self is required because calling any buffer function inside
        // f would create aliasing &mut
        unsafe {
            let size = self.len();
            let mapping = self
                .device
                .allocator
                .map_memory(&mut self.allocation)
                .unwrap();

            let mapping = slice::from_raw_parts_mut(mapping, size as usize);
            f(mapping);

            self.device.allocator.unmap_memory(&mut self.allocation);
        }
    }

    pub fn len(&self) -> u64 {
        self.size
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            println!("Destroying buffer");
            self.device
                .allocator
                .destroy_buffer(self.inner, &mut self.allocation);
        }
    }
}

pub struct Image {
    pub(super) device: Arc<Device>,
    pub(super) inner: vk::Image,
    allocation: vk_mem::Allocation,
}

impl Image {
    // Create a 2D Image with the given extent
    pub fn new(device: Arc<Device>, extent: vk::Extent2D) -> Self {
        unsafe {
            let image_info = vk::ImageCreateInfo::default()
                //.flags()
                .image_type(vk::ImageType::TYPE_2D)
                .format(vk::Format::R8G8B8A8_SRGB)
                .extent(vk::Extent3D {
                    width: extent.width,
                    height: extent.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::TRANSFER_DST)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
            //.initial_layout(vk::ImageLayout::UNDEFINED);
                ;
            let allocation_info = vk_mem::AllocationCreateInfo {
                usage: vk_mem::MemoryUsage::Auto,
                //flags: vk_mem::AllocationCreateFlags::HOST_ACCESS_SEQUENTIAL_WRITE,
                ..Default::default()
            };

            let (image, allocation) = device
                .allocator
                .create_image(&image_info, &allocation_info)
                .unwrap();

            Self {
                device,
                inner: image,
                allocation,
            }
        }
    }
}

impl Drop for Image {
    fn drop(&mut self) {
        unsafe {
            self.device
                .allocator
                .destroy_image(self.inner, &mut self.allocation);
        }
    }
}

trait PushData: bytemuck::Pod {}

fn push_data(device: &Device, command_buffer: vk::CommandBuffer, data: impl PushData) {
    let device = &device.ext.descriptor_heap.as_ref().unwrap().device;

    let data = bytemuck::bytes_of(&data);

    let push_data_info = vk::PushDataInfoEXT::default()
        .offset(0)
        .data(vk::HostAddressRangeConstEXT::default().address(data));

    unsafe {
        device.cmd_push_data(command_buffer, &push_data_info);
    }
}

pub struct DescriptorHeap {
    resource_heap: Buffer,
}

impl DescriptorHeap {
    pub fn new(device: Arc<Device>) -> VkResult<Self> {
        let descriptor_heap = device.ext.descriptor_heap.as_ref().expect(
            "Tried creating a descriptor heap but device does not support VK_EXT_descriptor_heap",
        );

        let resource_heap_size = descriptor_heap.props.max_resource_heap_size;

        let resource_heap = Buffer::new(
            Arc::clone(&device),
            resource_heap_size,
            vk::BufferUsageFlags::DESCRIPTOR_HEAP_EXT | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        )?
        .debug_name(c"Resource heap");

        Ok(Self { resource_heap })
    }

    pub unsafe fn write_descriptor(&self) {
        todo!()
    }

    // Hmmm, this binding can be performed one time per our command buffer
    pub unsafe fn bind(&self, command_buffer: vk::CommandBuffer) {
        unsafe {
            let device = &self.resource_heap.device;
            let ExtDescriptorHeap {
                device: descriptor_heap,
                props,
            } = &device
                .ext
                .descriptor_heap
                .as_ref()
                .expect("Unreachable unless device got modified");

            let resource_addr = device.inner.get_buffer_device_address(
                &vk::BufferDeviceAddressInfo::default().buffer(self.resource_heap.inner),
            );

            let resource_bind_info = vk::BindHeapInfoEXT::default()
                .heap_range(
                    vk::DeviceAddressRangeKHR::default()
                        .address(resource_addr)
                        .size(self.resource_heap.size),
                )
                .reserved_range_offset(
                    props.max_resource_heap_size - props.min_resource_heap_reserved_range,
                )
                .reserved_range_size(props.min_resource_heap_reserved_range);

            descriptor_heap.cmd_bind_resource_heap(command_buffer, &resource_bind_info);
        }
    }
}
